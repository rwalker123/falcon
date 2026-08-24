class_name ReadyForImprovement

## THE AGGREGATE `⌃` — the map-wide answer to *"which of the sources my people are WORKING could take
## their next step right now"*, as an ordinary overlay raster plus the count lines its legend states.
##
## ⛔ **IT IS NOT "which of my sources could climb a rung", AND THE DIFFERENCE IS THE WHOLE POINT.**
## Read that way the channel lit every land tile on the map the moment Cultivation was learned — every
## wild patch answers *"a rung is available"*, so "can be improved" stopped being a scarce property and
## the map washed out into a sheet that named nothing. A hex lights only if a source on it satisfies
## ALL FOUR of:
##
## 1. **A player band is WORKING it** — a `labor_assignments` row, or a hunting party's quarry.
## 2. **No OTHER faction owns it** — a refusal, not a requirement; see `_not_another_faction_s`.
## 3. **A rung above it is genuinely available** — `RungGates.next_rung_ready`.
## 4. **Nothing is being built there** — `RungGates.rung_in_progress` answers empty.
##
## ⛔ **CONDITION 1 WAS ONCE "it has already been improved", AND THAT WAS WRONG IN A WAY WORTH
## RECORDING.** It was chosen to make the set scarce, and it did — but the FIRST rung on a source is an
## improvement onto ground carrying none, so a test demanding an existing improvement can never show a
## first improvement. Reported from play: a faction that had just learned Herding, holding two hunted
## herds it could have started taming that turn, saw an empty map — the one knowledge it had spent
## nothing of was the one the channel structurally could not talk about. Working the source is the
## scarcity that was actually wanted: a band has hands on a handful of sources, never on a continent.
##
## **NOTHING HERE READS A PER-WEB BOOLEAN, and that is the property to keep.** Conditions 1 and 2 are
## answered off the band's own assignment rows and one owner pair; 3 and 4 are `RungGates`. The day a
## route ladder (trail → road) ships, a route worked by a band flows through this file unchanged.
##
## `docs/plan_knowledge_screen.md` §7. The map has marked the per-source case since issue #412: a
## worked patch or herd that can climb wears a `⌃` on its own badge. What that cannot show is the
## SHAPE of the opportunity — how many there are, which web they are on, and whether any of them is
## ground nobody is standing on. This channel is that view, and it is a CHANNEL rather than an event
## precisely because nothing here is ever lit for the player: a discovery does not highlight the map,
## it changes what this channel paints the next time the player asks for it.
##
## **THE PLAYER NEVER READS THE WORD "RUNG" OR "CLIMB", AND THIS FILE DOES.** The ladder's own
## vocabulary is what `RungGates`, `SourceForecast` and the whole intensification arc are written in,
## so the code keeps it; what a channel row and a legend say is *improve*, because a player asked to
## "climb a rung" on a hex is being handed a metaphor the game never taught them. The split is
## deliberate — the three `CHANNEL_*` constants below are the entire player-facing surface.
##
## **EVERYTHING IS `static` AND STATELESS**, the `RungGates` / `SourceForecast` shape. The model
## `derive` answers is held by `MapView` (which caches it beside the raster), never here.
##
## **IT ASKS `RungGates`, IT DOES NOT RE-DERIVE THE LADDER.** Conditions 3 and 4 are the badge's own
## two questions in the badge's own order — *is a rung already under way?*, then *is one on offer?* —
## so where this channel and the per-source mark are asking the same thing they cannot disagree. Every
## ladder term (which rungs a species admits, which knowledge gates them, whether the ground will take
## seed) stays in `RungGates`, where the compose sheet and the WORK board read it too. Conditions 1
## and 2 are NOT ladder terms and are deliberately not pushed down there: `RungGates` answers what a
## source *could* climb, which is the right answer for a compose sheet opened on wild ground. This
## channel is the one surface asking the narrower question, so the narrowing lives here — which is
## also why a lit hex here is a STRICT SUBSET of the hexes wearing a `⌃`.
##
## **THE VIEW IS DUCK-TYPED (`Object`), not typed `MapView`.** `MapView` calls this, so a typed
## parameter would close a `class_name` cycle — the same reason `OverlayChannels` reads the view
## loosely. It is only ever handed a live `MapView`.
##
## **THE HERD WEB HAS NO OWNER ON THE WIRE, so condition 2 is a PATCH-ONLY test.** A `HerdTelemetryState`
## carries no owning faction at all — a real, pre-existing gap, the one `RungGates.hunt_gates`'
## docstring already records — so a herd another faction has tamed reads as ours here, exactly as it
## does on the compose sheet. Condition 1 is therefore the whole faction test on that web. Nothing
## here invents an ownership signal: closing the gap means putting an owner on the herd row and then
## reading it in the shared gate, for all four surfaces at once.

## The channel key, and the label/description the picker states for it. **Both halves of the wiring
## read these constants** — `MapView._install_ready_for_improvement_overlay` stamps them onto the synthesized
## channel and `OverlayChannels.CHANNELS` names them in its row — so the picker's list and the map's
## own channel table cannot drift into two names for one thing.
const CHANNEL_KEY := "ready_for_improvement"
const CHANNEL_LABEL := "Ready for Improvement"
const CHANNEL_DESCRIPTION := "Land and herds your people could improve right now."

## A tile carrying at least one ready source wears a DIM WASH of the channel colour, every other tile
## none. **A full fill was the first cut and it was unreadable** — the lit hexes blew out against the
## dark grid, and a map of them was a glare rather than a reading; a wash states the same thing and
## lets the map underneath stay visible. The value is a FIXED wash level, NOT a per-hex strength: the
## raster is binary on purpose, because "there is an opportunity on this hex" is the whole claim, and
## shading it by how many offers a hex holds would say a hex with three is a better place to stand
## than one with a Sow. The generic `GRID_COLOR.lerp(overlay_color, value)` path then paints it, the
## `hunt_danger` shape.
const TILE_READY := 0.55

## `raw` counts the ready sources on the hex — a hex can hold a patch and several herds at once, and
## the raw plane is what a tile readout would quote. It is NOT what the ramp reads; see above.
const TILE_NONE := 0.0

## The WIRE keys conditions 1 and 2 are read out of — named here rather than spelled at the read site,
## the way this file already names its model keys. All three sit on the source dict the decoder
## publishes (`native/src/dict/subsistence.rs`), unprefixed: this channel walks `forage_patch_lookup`
## and `herds` directly, never the `patch_`-prefixed `tile_info` cross-ref.
##
## `SOURCE_CURRENT_RUNG_KEY` is on BOTH webs; the owner pair is on the plant web only.
const SOURCE_CURRENT_RUNG_KEY := "current_rung"
const SOURCE_HAS_OWNER_KEY := "has_owner"
const SOURCE_OWNER_KEY := "owner"

## The model's keys. `ready` is an `Array[Vector2i]` of the tiles the raster lit — the only part of
## the model that is a LIST rather than a count, because the legend names the nearest of them.
##
## **IT REPLACED an `unworked` list, which this rule made meaningless**: every lit source is one a band
## is working, so "how many of them is nobody on" is now always zero.
const MODEL_NORMALIZED := "normalized"
const MODEL_RAW := "raw"
const MODEL_PATCHES := "patches"
const MODEL_HERDS := "herds"
const MODEL_READY := "ready"

const FACTS_NONE := "Nothing your people work can be improved yet."
const FACTS_TOTAL_FORMAT := "%s · %s, %s"
const FACTS_NEAREST_FORMAT := "Nearest (%d, %d)"

## The count vocabulary, singular then plural. Spelled out rather than `+ "s"` so the legend's three
## nouns are all one edit away, and because "1 sources" in a card this small reads as a bug.
const NOUN_SOURCE := ["source", "sources"]
const NOUN_PATCH := ["patch", "patches"]
const NOUN_HERD := ["herd", "herds"]


## THE ONE PASS OVER EVERY SOURCE THE FACTION CAN SEE — the raster, the per-web counts, and the
## lit tiles, from one walk of the patches and one of the herds.
##
## **IT IS EXPENSIVE, AND THAT IS MEASURED RATHER THAN FEARED.** A live world seeds a `ForagePatch` on
## EVERY food-module tile that carries any human-edible capacity (`core_sim/src/forage.rs` →
## `spawn_initial_forage`) and the capture caps none of them, so this scales with the number of
## SOURCES rather than with anything the map draws. `map_preview`'s scale probe walks the ceiling — a
## full-size 256×192 world with a patch on every tile — at **~6.8 µs a source, ~331 ms for 49,152**.
##
## So `MapView` does NOT call this during the snapshot ingest, unlike its `province` twin: the channel
## is BUILT ON DEMAND (`MapView.DEFERRED_OVERLAY_BUILDERS`), which means a player who has not selected
## it pays nothing at all, and one who has pays once per turn — through the picker's re-assert, not
## through a new mechanism. Nothing here needs to know that; it is recorded so the next reader knows
## the cost is real and where it is handled.
static func derive(view: Object) -> Dictionary:
	var width := int(view.grid_width)
	var height := int(view.grid_height)
	var total: int = maxi(width * height, 0)
	var normalized := PackedFloat32Array()
	normalized.resize(total)
	normalized.fill(TILE_NONE)
	var raw := PackedFloat32Array()
	raw.resize(total)
	raw.fill(TILE_NONE)
	var ready: Array[Vector2i] = []
	var model := {
		MODEL_NORMALIZED: normalized,
		MODEL_RAW: raw,
		MODEL_PATCHES: 0,
		MODEL_HERDS: 0,
		MODEL_READY: ready,
	}
	if total <= 0:
		return model
	var knowledge: Dictionary = view.faction_knowledge
	var worked := worked_sources(view)
	var patches := 0
	for tile_variant in view.forage_patch_lookup:
		var tile: Vector2i = tile_variant
		if not _on_map(tile, width, height):
			continue
		var key: String = view.secondary_food_key(tile.x, tile.y)
		# **CONDITION 1, AND IT IS THE FIRST TEST ON PURPOSE.** It is a dictionary lookup, where the two
		# `RungGates` questions behind it are the expensive half — and on a live world it refuses the
		# overwhelming majority of patches, the sim seeding one on every food-module tile.
		if not worked.has(key):
			continue
		if not _offers_a_rung(SourceForecast.LABOR_KIND_FORAGE,
				view.forage_patch_lookup[tile], String(worked[key]), knowledge):
			continue
		patches += 1
		_stamp(normalized, raw, width, tile)
		ready.append(tile)
	var herds := 0
	for herd_variant in view.herds:
		if not (herd_variant is Dictionary):
			continue
		var herd: Dictionary = herd_variant
		var herd_tile := Vector2i(int(herd.get("x", -1)), int(herd.get("y", -1)))
		if not _on_map(herd_tile, width, height):
			continue
		var herd_key: String = view.secondary_herd_key(String(herd.get("id", "")))
		if not worked.has(herd_key):
			continue
		if not _offers_a_rung(SourceForecast.LABOR_KIND_HUNT, herd,
				String(worked[herd_key]), knowledge):
			continue
		herds += 1
		_stamp(normalized, raw, width, herd_tile)
		# A herd standing on a ready patch adds a SECOND entry for the same hex, deliberately: the
		# nearest answer is about a hex to walk to, and de-duplicating would only change which of two
		# identical coordinates the legend prints.
		ready.append(herd_tile)
	model[MODEL_PATCHES] = patches
	model[MODEL_HERDS] = herds
	return model


## THE SOURCES A PLAYER BAND IS ON, as `{secondary key: declared improvement}`.
##
## Two jobs, one walk: it supplies the `improvement` axis every rung answer needs, and its KEY SET is
## condition 1 itself — a source no band is on never reaches the ladder questions at all.
##
## **THE DECLARATION IS NOT REDUNDANT with the source's own meters.** `RungGates.rung_in_progress`
## resolves the verb off the meter and demotes `improvement` to a pending declaration — which is
## exactly the case the meters cannot answer: a Sow ordered this turn stands at 0% on every rung, so
## without the declaration the source would read as an untouched opportunity on the very turn the
## player committed it, while its own badge showed the build. The walk that supplies it is over BANDS
## (tens) and their assignment rows, not over sources (thousands), so it costs nothing next to the
## pass it feeds.
##
## **A SECOND READING OF `BandOverlayRenderer`'s SAME SET, and it has to be.** That renderer resolves
## the identical claim, but fused into a draw — per-band effective columns, crew and builder
## accumulation, the ring and the badge all fall out of the same loop — so there is no seam to share
## short of restructuring a shipped render path. What the two DO share is the identity: both key on
## `MapView.secondary_food_key` / `secondary_herd_key`, so "the same source" means the same thing in
## both, and both ask `RungGates` rather than deciding anything about the ladder for themselves.
static func worked_sources(view: Object) -> Dictionary:
	var out: Dictionary = {}
	for unit_variant in view.units:
		if not (unit_variant is Dictionary):
			continue
		var band: Dictionary = unit_variant
		if not view._is_player_unit(band):
			continue
		# A DETACHED PARTY'S QUARRY IS A WORKED SOURCE and rides the cohort rather than an assignment
		# row (`expedition_target_herd`) — the same branch the worked-mark pass makes, and for the same
		# reason: a party three turns out has claimed that herd as surely as one standing on it. A
		# party builds nothing, so its improvement axis is structurally empty.
		if bool(band.get("is_expedition", false)):
			var quarry := String(band.get("expedition_target_herd", "")).strip_edges()
			if quarry != "":
				_claim(out, view.secondary_herd_key(quarry), SourceForecast.IMPROVEMENT_NONE)
			continue
		var assignments: Variant = band.get("labor_assignments", [])
		if not (assignments is Array):
			continue
		for entry_variant in (assignments as Array):
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant
			if int(entry.get("workers", 0)) <= 0:
				continue
			var improvement := String(entry.get("improvement", SourceForecast.IMPROVEMENT_NONE))
			match String(entry.get("kind", "")).strip_edges().to_lower():
				SourceForecast.LABOR_KIND_FORAGE:
					var tx := int(entry.get("target_x", -1))
					var ty := int(entry.get("target_y", -1))
					if tx >= 0 and ty >= 0:
						_claim(out, view.secondary_food_key(tx, ty), improvement)
				SourceForecast.LABOR_KIND_HUNT:
					var herd_id := String(entry.get("fauna_id", ""))
					if herd_id != "":
						_claim(out, view.secondary_herd_key(herd_id), improvement)
	return out


## THE LEGEND'S LINES — `"5 sources · 3 patches, 2 herds"` and the nearest one's coordinate.
##
## Derived from the cached `model` on demand rather than stamped into it at ingest, because the
## NEAREST answer moves with the selection and a selection change is not a snapshot. The scan is over
## the lit list (tens), never over the sources again.
static func facts(view: Object, model: Dictionary) -> PackedStringArray:
	var lines := PackedStringArray()
	var patches := int(model.get(MODEL_PATCHES, 0))
	var herds := int(model.get(MODEL_HERDS, 0))
	var total := patches + herds
	if total <= 0:
		lines.append(FACTS_NONE)
		return lines
	lines.append(FACTS_TOTAL_FORMAT % [
		_counted(total, NOUN_SOURCE), _counted(patches, NOUN_PATCH), _counted(herds, NOUN_HERD)])
	var nearest := _nearest(view, model.get(MODEL_READY, []))
	if nearest.x >= 0:
		lines.append(FACTS_NEAREST_FORMAT % [nearest.x, nearest.y])
	return lines


## DOES THIS SOURCE OFFER A RUNG — the four conditions of the class docstring, in order: **ours and
## already improved** first, then the badge's own two questions in the badge's own order.
##
## **THE TWO NEW CONDITIONS GO IN FRONT, and they are about the SOURCE rather than about the ladder.**
## `RungGates` answers what a source *could* climb; it has no opinion on whether the source has ever
## been touched or whose it is, and it should not — the compose sheet asks it about a wild patch on
## purpose. This channel is the one surface asking a narrower question, so the narrowing lives here.
##
## **A RUNG UNDER WAY IS NOT AN OFFER**, which is the whole reason `rung_in_progress` is asked at all:
## `next_rung_ready` excludes the verb a crew DECLARED, but a patch whose Cultivate meter is at 42%
## still admits its next rung and would count as an opportunity on a map of them. `rung_in_progress`
## is what says "this one is already being climbed" — and it keys on the METER, so it also catches the
## half-built source nobody is working, which is a standing rung rather than an invitation.
static func _offers_a_rung(kind: String, source: Dictionary, improvement: String,
		knowledge: Dictionary) -> bool:
	if source.is_empty():
		return false
	if not _not_another_faction_s(kind, source):
		return false
	if not RungGates.rung_in_progress(kind, source, improvement).is_empty():
		return false
	return not RungGates.next_rung_ready(kind, source, improvement, knowledge).is_empty()


## **IS THIS SOMEBODY ELSE'S?** — the only ownership question worth asking, and it is spelled as a
## REFUSAL rather than a requirement.
##
## ⛔ **A REQUIREMENT WOULD HAVE HIDDEN EXACTLY THE TILES THIS CHANNEL EXISTS FOR.** A patch's `owner`
## is `Some` only once an improvement meter is above zero (`forage::ForagePatch::owner`), so an
## UNIMPROVED patch your band is working states no owner at all — and demanding `has_owner` would have
## refused every first-rung opportunity on the plant web, which is the whole of the early game.
## Reported from play: a faction that had just learned Herding, with two hunted herds it could tame,
## saw an empty map.
##
## So the test is "not another faction's": no owner recorded is FINE, an owner that is ours is fine,
## and only a stated foreign owner refuses. A herd carries no owner on the wire at all (the
## pre-existing `RungGates.hunt_gates` gap), so there is nothing to ask there and nothing is invented.
static func _not_another_faction_s(kind: String, source: Dictionary) -> bool:
	if kind != SourceForecast.LABOR_KIND_FORAGE:
		return true
	if not bool(source.get(SOURCE_HAS_OWNER_KEY, false)):
		return true
	return int(source.get(SOURCE_OWNER_KEY, -1)) == MapView.PLAYER_FACTION_ID


## Record a band's claim on a source. **A DECLARED VERB OUTRANKS AN EMPTY ONE**: two bands can work
## one patch and only one of them may have ordered the build, so a later empty declaration must not
## erase an earlier real one.
static func _claim(out: Dictionary, key: String, improvement: String) -> void:
	if improvement.strip_edges() == SourceForecast.IMPROVEMENT_NONE and out.has(key):
		return
	out[key] = improvement


static func _on_map(tile: Vector2i, width: int, height: int) -> bool:
	return tile.x >= 0 and tile.y >= 0 and tile.x < width and tile.y < height


static func _stamp(normalized: PackedFloat32Array, raw: PackedFloat32Array, width: int,
		tile: Vector2i) -> void:
	var idx := tile.y * width + tile.x
	normalized[idx] = TILE_READY
	raw[idx] = raw[idx] + 1.0


## The tile the nearest answer is measured FROM: the selected player band, else the first one
## the frame carries. `(-1, -1)` when the player has no band on the map at all, which is what makes
## the legend drop the coordinate rather than print an arbitrary one — "nearest" with nothing to be
## near is not a fact.
static func _anchor_tile(view: Object) -> Vector2i:
	var fallback := Vector2i(-1, -1)
	for unit_variant in view.units:
		if not (unit_variant is Dictionary):
			continue
		var band: Dictionary = unit_variant
		if not view._is_player_unit(band):
			continue
		var pos: Array = Array(band.get("pos", []))
		if pos.size() != 2:
			continue
		var tile := Vector2i(int(pos[0]), int(pos[1]))
		if int(band.get("entity", -1)) == int(view.selected_unit_id):
			return tile
		if fallback.x < 0:
			fallback = tile
	return fallback


## The lit tile closest to the anchor, **through `MapView`'s own hex metric and its own wrap
## rule** — `_hex_distance` is what the work-range disks are drawn with, so "nearest" here is the same
## nearest the map draws, and `_wrapped_col_delta` is what keeps a source just across the seam from
## reading as a world away.
static func _nearest(view: Object, ready: Array) -> Vector2i:
	var anchor := _anchor_tile(view)
	if anchor.x < 0:
		return anchor
	var best := Vector2i(-1, -1)
	var best_distance := -1
	for tile_variant in ready:
		var tile: Vector2i = tile_variant
		var effective_col: int = anchor.x + view._wrapped_col_delta(anchor.x, tile.x)
		var distance: int = view._hex_distance(anchor.x, anchor.y, effective_col, tile.y)
		if best_distance < 0 or distance < best_distance:
			best_distance = distance
			best = tile
	return best


static func _counted(value: int, noun: Array) -> String:
	return "%d %s" % [value, noun[0] if value == 1 else noun[1]]
