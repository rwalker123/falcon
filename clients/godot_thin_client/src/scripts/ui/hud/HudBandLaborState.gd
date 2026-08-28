class_name HudBandLaborState
extends RefCounted

## "The digested per-snapshot player world + the optimistic overlay" — the player-faction bands and
## expeditions captured each snapshot, the herds / forage-patch / food-module lookups the labor UI
## reads, the grid scalars for hex math, the losing-population diff, and the optimistic pending-labor
## overlay. Pure DATA: it never holds a scene node or a `%Name` lookup — the derived READS over those
## tables (`find_world_herd`, `food_module_icon`, `band_parties`/`band_party_workers`,
## `effective_role_workers`) live here too, because a pure filter of the model's own tables IS the
## model's remit; keeping them on the HUD is what made the band zone reach into the parties zone.
## `changed(reason)` is emitted on snapshot ingest and on a pending mutation; nothing consumes it yet
## (Phase 0 emits, Phase 2+ subscribes).
##
## Dictionaries/Arrays are returned BY REFERENCE from the read accessors, matching the HUD's existing
## in-place-read behaviour — callers must NOT assume a copy.

signal changed(reason: StringName)

# The pending-key labor vocabulary. Mirrors `SourceForecast.LABOR_KIND_FORAGE` / `LABOR_KIND_HUNT` (the
# command-side names); a forage source keys by tile, a hunt source by herd, every other role (scout /
# warrior) is one band-wide slot keyed by its own kind.
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"

# **THE HARVEST AXIS IS A NUMBER NOW, so there is no option list to be per-kind.** The per-kind lists
# went with issue #442 (the build verbs left `policy` for their own axis); the four-stance list they
# collapsed into went with the harvest-floor arc, which replaced the stance with an escapement floor
# in `0.0..=1.0`. `DEFAULT_HARVEST_FLOOR` aliases SourceForecast's for the reason its predecessor did:
# the value lives in exactly one place.
const DEFAULT_HARVEST_FLOOR := SourceForecast.DEFAULT_HARVEST_FLOOR

# `home_band_entity` on a cohort no band detached — the decoder's own "0 for a normal band".
const NO_HOME_BAND_ENTITY := 0

# The tile a cohort stands on when the snapshot did not state one. **The party's and the home band's
# sentinels DIFFER on purpose**: `party_cancels_in_camp` compares the two positions, and one shared
# sentinel would make two silent absences read as "standing in the same camp".
const PARTY_POSITION_UNKNOWN := -1
const HOME_POSITION_UNKNOWN := -2

# The food-module `kind` that marks a HUNTING site rather than a gathering one — the split
# `FoodIcons.for_site` needs to pick a quarry glyph over a forage sprig. Lives here with
# `food_module_icon`, its only reader.
const FOOD_SITE_KIND_GAME_TRAIL := "game_trail"

# Every player-faction resident band from the latest snapshot (roster order; first == `_player_band`).
var _player_bands: Array = []
# The single player band (first player-faction cohort) — assign/move/clear-all target it.
var _player_band: Dictionary = {}
# The band currently shown in the dockable Band/City panel; persists across selection changes and
# re-resolves by entity each snapshot.
var _panel_band: Dictionary = {}
# The player-faction expedition cohorts (detached scout/hunt parties) captured each snapshot.
var _player_expeditions: Array = []
# Every herd in the snapshot — the live position + label source for hunted-herd rows (herds migrate).
var _world_herds: Array = []
# The viewer's CONTACT TIES (arc #527), whole-section replaced each snapshot that carries them. One
# DIRECTED row per edge — `observer_band_id` knows `subject_band_id` — already filtered sim-side to
# this faction's observers, so nothing here re-filters and nothing here asks about faction: faction
# is a property of the endpoint and never a column on the row.
var _connections: Array = []
# Optimistic pending labor per band entity: {turn, assign:{key->{...}}, move:{x,y}} (see the HUD).
var _pending_labor: Dictionary = {}
# The authoritative snapshot turn (header tick) — reconciles pending against the server's processing.
var _current_turn: int = -1
# Map grid geometry: the dimensions AND the horizontal-wrap flag, all three arriving together on the
# snapshot `grid` key. Wrap lives here beside the width it is meaningless without — every wrap-aware
# hex distance needs the pair, so splitting them left the HUD threading one from a member and the
# other from this model.
var _grid_width: int = 0
var _grid_height: int = 0
var _wrap_horizontal: bool = false
# Previous per-band size (entity -> size) so a shrink is detectable across snapshots.
var _prev_band_sizes: Dictionary = {}
# Snapshot forage patches keyed by tile (the Current-actions Forage row's max-useful forecast source).
var _forage_patch_lookup: Dictionary = {}
# Snapshot food modules keyed by tile (a Forage row's resource glyph, matching the map marker).
var _food_module_by_tile: Dictionary = {}
# ---- THE KIT ROSTER (`docs/plan_denial_raid.md`) --------------------------------------------------
# The world's kit roster in `equipment.json` order, and the kit each verb uses when the player names
# none. WORLD-level data rather than band-level: one roster serves every band and every sheet, so it
# sits beside the grid scalars rather than being re-read per compose. The ORDER is the wire's and is
# preserved — `equipment.json` authors the null choice (`none`) last, which is what puts it at the
# bottom of every picker without this model knowing which entry is null.
var _kits: Array = []
var _default_hunt_kit_id: String = KitRoster.NO_KIT_ID
var _default_forage_kit_id: String = KitRoster.NO_KIT_ID
# The two BAND-WIDE roles' defaults. They had no kit axis — and so no default to name — until the
# roster gained wayfinding gear and clubs for them.
var _default_scout_kit_id: String = KitRoster.NO_KIT_ID
var _default_warrior_kit_id: String = KitRoster.NO_KIT_ID

# ---- Read accessors (backing value returned by reference — no deep copy) --------------------------

func player_bands() -> Array:
	return _player_bands

func player_band() -> Dictionary:
	return _player_band

func panel_band() -> Dictionary:
	return _panel_band

func player_expeditions() -> Array:
	return _player_expeditions

func world_herds() -> Array:
	return _world_herds

func pending_labor() -> Dictionary:
	return _pending_labor

func current_turn() -> int:
	return _current_turn

func grid_width() -> int:
	return _grid_width

func grid_height() -> int:
	return _grid_height

## Does the map wrap east-west? Fed to `SourceForecast.hex_distance_wrapped` beside `grid_width()`.
func wrap_horizontal() -> bool:
	return _wrap_horizontal

func prev_band_sizes() -> Dictionary:
	return _prev_band_sizes

func forage_patch_lookup() -> Dictionary:
	return _forage_patch_lookup

func food_module_by_tile() -> Dictionary:
	return _food_module_by_tile

## The world's kit roster, in wire order.
func kits() -> Array:
	return _kits

## The kit a verb uses when the player names none — the token the command builders OMIT for, so a
## composition that never touched the picker emits the line it emitted before the picker existed.
## `""` for a job the wire has not named a default for.
func default_kit_id(job: String) -> String:
	match job:
		KitRoster.JOB_FORAGE:
			return _default_forage_kit_id
		KitRoster.JOB_SCOUT:
			return _default_scout_kit_id
		KitRoster.JOB_WARRIOR:
			return _default_warrior_kit_id
		KitRoster.JOB_BUILDERS:
			# **THE WIRE NAMES NO BUILDERS DEFAULT**, so this answers `""` — the "a job the wire has
			# not named a default for" case above, stated rather than reached by fall-through. Falling
			# through to the HUNT default would be worse than saying nothing twice over: the role
			# card's picker would mark the wrong entry `(default)`, and `Main._kit_token` would then
			# OMIT the token for a builders selection that happened to equal the hunt kit, so the sim
			# would resolve `default_kits.builders` (`none`) for a choice the player made and the
			# panel showed.
			return KitRoster.NO_KIT_ID
		_:
			return _default_hunt_kit_id

# ---- Snapshot lookups (derived reads over the ingested tables) -----------------------------------

## The snapshot herd with this id, wherever it is on the map; {} when unknown. Herds MIGRATE every
## turn, so `_world_herds` — not a hunt assignment's launch-time `target_x/target_y` — is the
## authority on where a hunted herd IS. Mirrors `MapView._herd_by_id` (the hunted-herd ring's resolver).
func find_world_herd(herd_id: String) -> Dictionary:
	if herd_id == "":
		return {}
	for herd in _world_herds:
		if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
			return herd
	return {}

## The herd a hunting party is bound to, resolved from the live telemetry; {} for a scout party, a
## party whose target was lost/replaced, or an unknown id. The stateless detail layer takes this as a
## PARAMETER (`DetailFormat.expedition_row_tooltip` / `expedition_next_delivery_line`), so the id →
## herd step lives here rather than being spelled at each of its three call sites.
func expedition_target_herd(exp: Dictionary) -> Dictionary:
	return find_world_herd(String(exp.get("expedition_target_herd", "")).strip_edges())

## **The species name a PARTY declares for the herd `herd_id`** — "" when no live party is bound to it.
##
## The sim resolves this at launch and carries it on the party for the party's whole life
## (`expeditionTargetSpecies`), so it answers for a target that has left `_world_herds` entirely: herd
## telemetry is fog-filtered to hexes the player can see right now and pruned at local extinction, and
## a detached party is not a vision source — so a hunting party's own quarry routinely goes dark while
## the party is still bound to it, and the id was all the HUD had left to render (issue #378).
##
## **A pure filter of `_player_expeditions`, NOT a cache of herd names.** It holds nothing and remembers
## nothing: the parties array is replaced wholesale each snapshot, so this can only ever answer for a
## herd some live party is hunting *now*, and it goes quiet the moment that party folds back. A
## last-seen-name cache would answer for herds nobody is bound to any more and would go stale unnoticed.
func expedition_target_label(herd_id: String) -> String:
	if herd_id == "":
		return ""
	for party in _player_expeditions:
		if not (party is Dictionary):
			continue
		var exp := party as Dictionary
		if String(exp.get("expedition_target_herd", "")).strip_edges() != herd_id:
			continue
		var species := String(exp.get("expedition_target_species", "")).strip_edges()
		if species != "":
			return species
	return ""

## The resource glyph for the food module on (x, y) — the same icon `MapView._draw_food_site` draws
## there. "" when the tile has no known module (undiscovered), so the row renders bare rather than
## with a misleading fallback sprig.
func food_module_icon(x: int, y: int) -> String:
	var site: Variant = _food_module_by_tile.get(Vector2i(x, y), null)
	if not (site is Dictionary):
		return ""
	var module_key := String((site as Dictionary).get("module", ""))
	var is_hunt := String((site as Dictionary).get("kind", "")) == FOOD_SITE_KIND_GAME_TRAIL
	return FoodIcons.for_site(module_key, is_hunt, int((site as Dictionary).get("terrain_id", -1)))

## The bundled SPRITE for the food module on (x, y) — the art twin of `food_module_icon`, and the
## same art `MapView`'s marker draws there. `null` when the tile has no known module or the site's art
## key has no PNG, which is the caller's cue to fall back to the emoji (`HudWidgets.build_marker_icon`
## makes that choice).
##
## It lives here, beside its emoji twin, so the module / is-hunt / terrain triple is resolved ONCE:
## split across two files the sprite and the glyph could come to disagree about which site this is,
## which is the exact failure `FoodIcons.site_key_for` was factored out to prevent one layer down.
func food_module_sprite(x: int, y: int) -> Texture2D:
	var site: Variant = _food_module_by_tile.get(Vector2i(x, y), null)
	if not (site is Dictionary):
		return null
	var module_key := String((site as Dictionary).get("module", ""))
	var is_hunt := String((site as Dictionary).get("kind", "")) == FOOD_SITE_KIND_GAME_TRAIL
	return SiteSprites.for_site(module_key, is_hunt, int((site as Dictionary).get("terrain_id", -1)))

## The player expeditions this band detached (grouped by `home_band_entity`) — the parties zone's row
## set and the Workforce bar's Parties segment both read it, so the two can never disagree about which
## parties belong to the band.
func band_parties(band: Dictionary) -> Array:
	var band_entity := int(band.get("entity", -1))
	var rows: Array = []
	for exp_variant in _player_expeditions:
		if exp_variant is Dictionary and int((exp_variant as Dictionary).get("home_band_entity", 0)) == band_entity:
			rows.append(exp_variant)
	return rows

## **WOULD RECALLING THIS PARTY CANCEL IT ON THE SPOT?** The client-side reading of the sim's
## `cancel_party_standing_in_camp`: a party standing in its home band's camp with no map report owed is
## folded back by `handle_recall_expedition` the instant the command lands, so the order is a CANCEL of
## something that never took effect rather than an errand home. `false` = the ordinary recall, which
## walks the party back over turns.
##
## **THE FOUR TERMS ARE THE SIM'S, MATCHED EXACTLY.** A looser client test — say "the phase is hunting
## and it carries nothing" — would print *Cancel* over a party that really does walk home, which is the
## same lie the wrong way round. In particular: co-location is EXACT, never comm range (the sim folds a
## `Returning` party back within 2 tiles, but doing that on a recall would teleport workers home rather
## than cancel an order); and the pack is NOT a term (a full larder does not force a round trip).
##
## Every single-party recall surface reads this one function — the parties row ✕, the parties inspector
## link, the Occupants drawer's button — so the verb they show and the confirm they raise cannot
## disagree about what the sim will do. Lives on this model because it is a question about the snapshot
## (the party, and the home band it is grouped under), not about any one panel.
func party_cancels_in_camp(exp: Dictionary) -> bool:
	if not bool(exp.get("is_expedition", false)):
		return false
	# "Nothing to deliver" is about the MAP, not the pack: flushing `pending_reveal` to the faction map
	# is the one thing an out-of-band fold-back cannot do, so it is what gates the sim. The decoder
	# projects the wire's coordinate pair to this count for exactly this question.
	if int(exp.get("pending_reveal_count", 0)) > 0:
		return false
	var home := player_band_by_entity(int(exp.get("home_band_entity", NO_HOME_BAND_ENTITY)))
	if home.is_empty():
		return false
	# Absent coordinates on either side must not read as a match, so the two defaults DIFFER — a party
	# whose position the snapshot never stated is in no camp.
	var party_x := int(exp.get("current_x", PARTY_POSITION_UNKNOWN))
	var party_y := int(exp.get("current_y", PARTY_POSITION_UNKNOWN))
	var home_x := int(home.get("current_x", HOME_POSITION_UNKNOWN))
	var home_y := int(home.get("current_y", HOME_POSITION_UNKNOWN))
	return party_x == home_x and party_y == home_y

## Workers currently out with this band's parties — the Workforce bar's Parties segment.
func band_party_workers(band: Dictionary) -> int:
	var total := 0
	for exp in band_parties(band):
		total += int((exp as Dictionary).get("size", 0))
	return total

# ---- Snapshot ingest / mutators (emit `changed`) -------------------------------------------------

func set_turn(turn: int) -> void:
	_current_turn = turn
	changed.emit(&"turn")

func set_grid(width: int, height: int, wrap_horizontal_flag: bool) -> void:
	_grid_width = width
	_grid_height = height
	_wrap_horizontal = wrap_horizontal_flag
	changed.emit(&"grid")

func set_world_herds(herds: Array) -> void:
	_world_herds = herds
	changed.emit(&"world_herds")

## Ingest the viewer's contact ties. A whole-section replace, like the herd list above: the sim
## re-sends the vector whenever any edge moves and omits it otherwise, so a non-Array is ignored and
## the last value stands (`set_kit_roster`'s rule) — a delta that carried no ties means "unchanged",
## never "you have forgotten everyone".
func set_connections(connections_variant: Variant) -> void:
	if not (connections_variant is Array):
		return
	_connections = connections_variant
	changed.emit(&"connections")

func connections() -> Array:
	return _connections

## **WHAT THIS CLIENT CALLS THE BAND WITH THIS DURABLE `band_id`** — `""` when the roster holds no
## such band, which is the answer a caller must be able to act on rather than a name it can print.
##
## **THERE IS EXACTLY ONE BAND-NAMING RULE IN THIS CLIENT AND THIS IS THE JOIN ONTO IT.** A band's
## name is its ROSTER POSITION (`HudFormat.band_display_name`) — the cycler, the band picker, the
## faction page and the event dock's `band=` substitution all say `Band 2` for the same band because
## they all resolve it that way. Anything that holds a band by its id and needs a label — a shipment's
## destination, a connection's subject — comes here, so a band cannot be called two things on two
## surfaces. (The dock keeps its own `{band_id: name}` dictionary rather than calling this, because it
## must relabel rows it is already holding when the roster changes; it is built from the same
## `band_display_name` in the same pass, so the two cannot disagree.)
func band_label_for_id(band_id: int) -> String:
	if band_id == HudConst.NO_BAND_ID:
		return ""
	for i in range(_player_bands.size()):
		var candidate: Dictionary = _player_bands[i]
		if int(candidate.get("band_id", HudConst.NO_BAND_ID)) == band_id:
			return HudFormat.band_display_name(candidate, i + 1)
	return ""

## **THE TIES ONE BAND HOLDS**, in the ledger's own order (the sim publishes a stable `BTreeMap`
## walk, so the picker's rows do not reshuffle frame to frame).
##
## Keyed on the DURABLE `band_id`, never the entity: a tie outlives a rollback and the command that
## spends it addresses the band the same way.
##
## **A PARKED EDGE (strength 0) IS RETURNED, not filtered.** It means "we know such a people exist
## and have no current tie", which the destination picker shows disabled with that as its reason —
## dropping it here would hide from the player the very fact that the tie is what gates trade.
func connections_for_band(band_id: int) -> Array:
	var rows: Array = []
	if band_id == HudConst.NO_BAND_ID:
		return rows
	for row_variant in _connections:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if int(row.get("observer_band_id", HudConst.NO_BAND_ID)) == band_id:
			rows.append(row)
	return rows

## Ingest the world's kit roster and the FOUR job defaults. **They ride ONE call**, because they are
## one fact: a roster whose defaults name kits it does not contain would let every picker open on an
## entry it cannot show. A non-Array roster is ignored (the last value stands), matching the
## `set_food_modules` / `set_forage_patches` ingest — a delta carries a section only when it changed,
## so absence means unchanged and never "the world has no kits".
func set_kit_roster(kits_variant: Variant, default_hunt: String, default_forage: String,
		default_scout: String, default_warrior: String) -> void:
	if not (kits_variant is Array):
		return
	_kits = kits_variant
	_default_hunt_kit_id = default_hunt
	_default_forage_kit_id = default_forage
	_default_scout_kit_id = default_scout
	_default_warrior_kit_id = default_warrior
	changed.emit(&"kits")

func set_panel_band(band: Dictionary) -> void:
	_panel_band = band
	changed.emit(&"panel_band")

## Ingest the per-snapshot player-faction split (the four fields `update_band_alerts` sets together).
func ingest_snapshot_bands(prev_sizes: Dictionary, band: Dictionary, bands: Array, expeditions: Array) -> void:
	_prev_band_sizes = prev_sizes
	_player_band = band
	_player_bands = bands
	_player_expeditions = expeditions
	changed.emit(&"snapshot")

## Ingest the snapshot food modules (x/y/module/kind + terrain_id) into the per-tile lookup. A
## non-Array input is ignored (the lookup keeps its last value), matching the old ingest.
func set_food_modules(modules_variant: Variant) -> void:
	if not (modules_variant is Array):
		return
	_food_module_by_tile.clear()
	for entry in modules_variant:
		if not (entry is Dictionary):
			continue
		var site: Dictionary = entry
		var sx := int(site.get("x", -1))
		var sy := int(site.get("y", -1))
		if sx >= 0 and sy >= 0:
			_food_module_by_tile[Vector2i(sx, sy)] = site
	changed.emit(&"food_modules")

## Ingest the snapshot forage patches into the per-tile lookup. A non-Array input is ignored (the
## lookup keeps its last value), matching the old ingest.
func set_forage_patches(patches_variant: Variant) -> void:
	if not (patches_variant is Array):
		return
	_forage_patch_lookup.clear()
	for entry in patches_variant:
		if not (entry is Dictionary):
			continue
		var patch: Dictionary = entry
		var px := int(patch.get("x", -1))
		var py := int(patch.get("y", -1))
		if px >= 0 and py >= 0:
			_forage_patch_lookup[Vector2i(px, py)] = patch
	changed.emit(&"forage_patches")

# ---- Optimistic pending labor overlay ------------------------------------------------------------

## Stable key identifying a source/role within a band's assignment set.
func pending_key(kind: String, x: int, y: int, herd_id: String) -> String:
	match kind:
		LABOR_KIND_FORAGE:
			return "forage:%d,%d" % [x, y]
		LABOR_KIND_HUNT:
			return "hunt:%s" % herd_id
		_:
			return kind  # scout / warrior — one band-wide role each

## **THIS BAND'S OWN BUILD QUEUE, AS THE KEYS THE QUEUE MODELS ARE KEYED BY** — the wire's
## `PopulationCohortState.buildQueue` in the band's own order, each row mapped through `pending_key`,
## `[]` for a band with nothing queued (`docs/plan_standing_upkeep.md` §4.9 item 9a).
##
## **THE RANK IS THE INDEX.** Entry 0 is the head and there is deliberately no second integer to
## disagree with it, so this list IS the order and nothing re-sorts it.
##
## ⛔ **`SourceForecast.build_queue_position` IS NOT THIS.** That field is published per SOURCE and
## rides the WINNING band (the soonest estimate), so on a source two bands hold it states another
## band's place in another band's line — which drew band B's `[X, Y, Z]` as `[Y, X, Z]` and then had
## the drag arithmetic compute an insert index from the wrong list. It is a READOUT; this is the rank.
##
## **ONE DERIVATION, SHARED BY THE BLOCK AND THE DROP.** The queue block orders on it and
## `_queue_drop` indexes into it, so the position a drag sends is an index into the list the player
## was looking at — the property a second walk of the wire would only be able to get wrong.
##
## The keys are `pending_key`'s own shape and the wire's `kind` is the `LaborAssignment` vocabulary
## (`"forage"` / `"hunt"`), so an entry joins its work row on ONE spelling rather than on two that
## merely happen to match.
func build_queue_keys(band: Dictionary) -> Array:
	var keys: Array = []
	var entries: Variant = band.get("build_queue", [])
	if not (entries is Array):
		return keys
	for entry_variant in (entries as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		keys.append(pending_key(String(entry.get("kind", "")).strip_edges().to_lower(),
			int(entry.get("target_x", -1)), int(entry.get("target_y", -1)),
			String(entry.get("fauna_id", ""))))
	return keys

func pending_assigns_for(entity: int) -> Dictionary:
	var e: Variant = _pending_labor.get(entity, {})
	if not (e is Dictionary):
		return {}
	var a: Variant = (e as Dictionary).get("assign", {})
	return a if a is Dictionary else {}

## **THE DECLARATION THE OVERLAY IS CARRYING FOR ONE SOURCE** — `IMPROVEMENT_NONE` when this band has
## no un-acknowledged edit on it (`docs/plan_standing_upkeep.md` §4.7a ①).
##
## It exists so an OPEN compose sheet can see a `⌃` pressed on the Work board a moment ago: the sheet
## derives its rung through `SourceForecast.build_verb`, whose `declared` argument is honoured at a
## zero meter, and without this the sheet would go on OFFERING a rung the band has just queued.
##
## **IT READS THE OVERLAY ALONE, NEVER THE CONFIRMED ASSIGNMENT.** `improvement_for_forage` / `_hunt`
## answer that other question and the compose sheet already seeds from them; folding the two here
## would make the composition un-clearable — a sheet that deliberately composes *no build* over a
## source the wire says is building one is a state the harnesses stage directly.
##
## The key shape is `pending_key`'s, so this cannot drift from what `record_pending_assign` wrote.
func pending_improvement_for(band: Dictionary, kind: String, x: int, y: int,
		herd_id: String) -> String:
	var entry: Variant = pending_assigns_for(int(band.get("entity", -1))).get(
		pending_key(kind, x, y, herd_id), null)
	if not (entry is Dictionary):
		return SourceForecast.IMPROVEMENT_NONE
	return String((entry as Dictionary).get("improvement", SourceForecast.IMPROVEMENT_NONE))

## `improvement` is what the source will be building once the edit lands — the composed improvement on
## a sheet that just ticked one on, else whatever it was already building. It is recorded because
## `assign_labor` deliberately does NOT touch that axis (issue #442), so an optimistic overlay that
## dropped it would flash a running build off the board for one turn.
##
## ⛔ **`kit_id` RIDES THE RECORD FOR THE OPPOSITE REASON, AND ITS ABSENCE WAS A DEAD CONTROL.**
## `assign_labor` DOES state the take kit, so the optimistic row must state the kit the command just
## carried — never the CONFIRMED row's, which on a brand-new assignment does not exist. It was not
## recorded at all, and `effective_worker_map`'s pending branch REPLACES the merged row rather than
## patching it, so every pending row rebuilt with `kit_id == ""`. Two things followed. The work
## inspector's `Harvesters` picker opened on NOTHING — no selected entry, an EMPTY FACE, since `""`
## resolves through `KitRoster.display_name_for_id` to `""` — and picking a kit from it re-sent
## `assign_labor`, which re-recorded the row with no kit again, so the face never filled and the
## control read as dead. Worse, `BandPanelController._emit_work_assign` RESTATES `model.kit_id` on
## every `+`/`−` and `Main._kit_token` OMITS an empty one, so a crew edit on a pending row silently
## re-kitted the crew back to the job default — the exact failure that restate exists to prevent.
##
## **THE HUNT WEB ONLY LOOKED IMMUNE.** A herd publishes `default_kit_id`, so the picker's fallback
## resolved a real face there and the blank face never showed; the SILENT RE-KIT was live on both webs.
func record_pending_assign(entity: int, kind: String, workers: int, x: int, y: int, herd_id: String,
		floor: float, improvement: String = SourceForecast.IMPROVEMENT_NONE,
		kit_id: String = KitRoster.NO_KIT_ID) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	var assigns: Dictionary = entry.get("assign", {})
	assigns[pending_key(kind, x, y, herd_id)] = {
		"kind": kind, "workers": max(0, workers), "x": x, "y": y, "herd_id": herd_id,
		"floor": SourceForecast.clamp_floor(floor), "improvement": improvement,
		"kit_id": kit_id,
	}
	entry["assign"] = assigns
	_pending_labor[entity] = entry
	changed.emit(&"pending")

# ---- THE WITHDRAWAL — the queue's one remaining optimistic fact -----------------------------------
#
# **IT RIDES THE EXISTING PER-BAND RECORD, beside `assign` and `move`** (`docs/plan_standing_upkeep.md`
# §4.7b ④), so `reconcile_pending` and `_prune_pending_entity` cover it with no second lifecycle to
# keep in step.
#
# ⛔ **IT IS KEYED ON THE TURN, NOT ON THE NEXT SNAPSHOT — AND THE REASON IS NO LONGER THE ONE THAT
# WAS WRITTEN HERE.** The justification used to be the stale turn-written `buildQueuePosition`: the
# recapture every command triggers still carried the withdrawn entry at its old position, so a "hide
# it until the next snapshot" rule flickered the row straight back. **That reason is dead.**
# `PopulationCohortState.buildQueue` is captured LIVE off the allocation (§4.9 item 9a), so the
# recapture an `unqueue` triggers has already dropped the entry, and so has the `improvement` token
# on the source's own labor row.
#
# What the overlay still covers is the ROUND TRIP: the frame is drawn the instant the `✕` is pressed
# and the recapture is a network hop away, so without it the row sits there until the reply lands.
# Keying on the TURN is what carries it across that window and no further — `reconcile_pending`
# already drops additions on *a snapshot with a NEWER turn*, and this takes the identical rule for
# free by living in the same record.
#
# ⛔ **THE ORDERING OVERLAY THAT USED TO SHARE THIS RECORD IS GONE**, for exactly the fact that
# changed: a client-side ordering beside a live wire one is a SECOND ordering, which is the drift
# `buildQueue`'s own doc comment forbids ("a client therefore needs no optimistic ordering overlay,
# and must not keep one"). A withdrawal has nothing to drift against — it removes a row rather than
# ranking one — which is why it survives where the ordering did not.

## The keys this band has WITHDRAWN this turn — `pending_key`'s own shape, so it cannot drift from
## what the queue models are keyed by.
func pending_unqueues_for(entity: int) -> Dictionary:
	var e: Variant = _pending_labor.get(entity, {})
	if not (e is Dictionary):
		return {}
	var u: Variant = (e as Dictionary).get("unqueue", {})
	return u if u is Dictionary else {}


## **WITHDRAW A DECLARATION OPTIMISTICALLY.** The row leaves the BUILD QUEUE block on the frame the
## `✕` is pressed rather than a turn later, which is the asymmetry the declaration's own optimistic
## row created.
##
## ⛔ **IT CLEARS THE IMPROVEMENT; IT DOES NOT DROP THE PENDING RECORD.** `unqueue` withdraws a
## DECLARATION and leaves the take crew standing — and the same record may be holding a pending CREW
## edit on that very source, which dropping it would discard. `effective_worker_map` blanks the
## effective improvement for a withdrawn key instead, so the work row's `⌃` returns to its offer face
## on the same frame without anything else about the row moving.
func record_pending_unqueue(entity: int, kind: String, x: int, y: int, herd_id: String) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	var withdrawn: Dictionary = entry.get("unqueue", {})
	withdrawn[pending_key(kind, x, y, herd_id)] = true
	entry["unqueue"] = withdrawn
	_pending_labor[entity] = entry
	changed.emit(&"pending")

## The withdrawal's rollback, for a `unqueue` that never left the client — `drop_pending_assign`'s
## rule, one key at a time, so a band's other un-acknowledged edits survive a failed send.
func drop_pending_unqueue(entity: int, key: String) -> bool:
	var entry: Variant = _pending_labor.get(entity, null)
	if not (entry is Dictionary):
		return false
	var withdrawn: Variant = (entry as Dictionary).get("unqueue", {})
	if not (withdrawn is Dictionary) or not (withdrawn as Dictionary).has(key):
		return false
	(withdrawn as Dictionary).erase(key)
	_prune_pending_entity(entity, entry as Dictionary)
	changed.emit(&"pending")
	return true

func record_pending_move(entity: int, x: int, y: int) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	entry["move"] = {"x": x, "y": y}
	_pending_labor[entity] = entry
	changed.emit(&"pending")

## **DROP ONE OPTIMISTIC ENTRY THAT NEVER LEFT THE CLIENT.** The write above is made BEFORE the
## command is handed to the transport, which is right — the card must answer the stepper on the frame
## it was pressed — but nothing rolled it back when the send FAILED. Reported from play: an
## `assign_labor … builders 3` that never reached the server left the Builders card showing three
## builders the sim had never heard of, until the next turn's `reconcile_pending` quietly took them
## away, while the build queue beside it correctly read `⚠ No builders`. One screen, two answers, and
## the wrong one was the one with the number on it.
##
## **IT DROPS THAT ENTRY AND NOTHING ELSE.** A failure clears the edit that failed; a rollback that
## emptied the whole overlay would discard the player's OTHER un-acknowledged edits on the same band —
## a worse bug than the one it fixes, and one that passes any "the bad entry is gone" check. The
## entity's record itself is pruned only once it is genuinely empty, so a band with a pending MOVE
## keeps it.
##
## Returns whether anything was dropped, so the caller re-renders exactly when there is something to
## re-render.
func drop_pending_assign(entity: int, key: String) -> bool:
	var entry: Variant = _pending_labor.get(entity, null)
	if not (entry is Dictionary):
		return false
	var assigns: Variant = (entry as Dictionary).get("assign", {})
	if not (assigns is Dictionary) or not (assigns as Dictionary).has(key):
		return false
	(assigns as Dictionary).erase(key)
	_prune_pending_entity(entity, entry as Dictionary)
	changed.emit(&"pending")
	return true

## The move twin — same failure, same rule. The move overlay is ONE slot per band rather than a keyed
## set, so the identity is the band alone and there is nothing narrower to name.
func drop_pending_move(entity: int) -> bool:
	var entry: Variant = _pending_labor.get(entity, null)
	if not (entry is Dictionary) or not (entry as Dictionary).has("move"):
		return false
	(entry as Dictionary).erase("move")
	_prune_pending_entity(entity, entry as Dictionary)
	changed.emit(&"pending")
	return true

## Forget a band's pending RECORD once it holds nothing — but only then. `reconcile_pending` walks
## these by entity and an empty husk would be a turn's worth of no-op work; a record still holding the
## band's other un-acknowledged edits must survive untouched.
func _prune_pending_entity(entity: int, entry: Dictionary) -> void:
	var assigns: Variant = entry.get("assign", {})
	if (assigns is Dictionary) and (assigns as Dictionary).is_empty():
		entry.erase("assign")
	# The withdrawal set is the assign map's twin and prunes on the same rule: an empty one is a husk,
	# and a record still holding ANY of the band's three un-acknowledged edits must survive untouched.
	var withdrawn: Variant = entry.get("unqueue", {})
	if (withdrawn is Dictionary) and (withdrawn as Dictionary).is_empty():
		entry.erase("unqueue")
	if entry.has("assign") or entry.has("move") or entry.has("unqueue"):
		_pending_labor[entity] = entry
		return
	_pending_labor.erase(entity)

## Drop pending entries the server has already processed: a snapshot whose turn is NEWER than the
## entry's issue turn is authoritative confirmation (and reflects any clamping). Returns true when it
## dropped anything, so the caller can push the pruned overlay onward.
func reconcile_pending(turn: int) -> bool:
	if _pending_labor.is_empty():
		return false
	var dropped := false
	for entity in _pending_labor.keys():
		var entry: Dictionary = _pending_labor[entity]
		if int(entry.get("turn", -1)) < turn:
			_pending_labor.erase(entity)
			dropped = true
	if dropped:
		changed.emit(&"pending")
	return dropped

# Per-source rate keys whose ABSENCE is meaningful, so they are copied through only when the wire
# assignment carried them (see the loop in `effective_worker_map`). `realized_yield` is the steady
# food average. (Its issue-#337 twins in the trade account went with that axis, arc #527.)
## THE FORECAST'S BAND (`docs/plan_hunt_through_combat.md` §6.4) travels the same presence-sensitive
## way, and for a sharper reason than the rates above: `source_yield_readout` renders a range ONLY
## when the two bounds differ, so a `get(..., 0.0)` default would hand it `0.0–0.0` — equal, and
## therefore silent — on an assignment that never carried them. Copying only what the wire sent keeps
## "no band published" and "the band is a point" the same rendered answer by construction rather than
## by luck.
const OPTIONAL_YIELD_KEYS: Array[String] = [
	# `fodder_yield` is the THIRD account (issue #449) and it rides this list for the plain reason the
	# others do — a key not copied here does not exist as far as the work board is concerned, and a
	# sown hay Field's whole product is this one number. It is the one entry whose absence carries no
	# second meaning: there is no `realized_fodder_yield` to fall back from, so an absent key and a
	# published zero are the same reading (`SourceForecast.fodder_rate_of` says why at length).
	"realized_yield", "fodder_yield",
	SourceForecast.YIELD_RANGE_LOW_KEY, SourceForecast.YIELD_RANGE_HIGH_KEY,
]

## **THE HANDS ONE MERGED ROW SPENDS**, the client's transcription of
## `LaborAssignment::staffed_total`.
##
## **IT IS THE TAKE CREW ALONE AGAIN** (`docs/plan_standing_upkeep.md` §2.5). A source carried a
## second, per-source BUILD allocation for one slice and this had to sum the pair; the builders are a
## band-level POOL now — an ordinary `builders` role row of `labor_assignments`, like `scout` — so
## `improvementWorkers` is off the wire and a row states one number again. The pool's own hands are
## still counted, because its role row is a row in this same list.
##
## **THE DEFECT IT WAS WRITTEN FOR IS STILL LIVE, one shape over.** `effective_idle` sums this, and a
## reader that skipped a whole ROW would report hands that do not exist — which is what a band's
## `builders` row would become if anything here started filtering rows by kind.
static func staffed_total(entry: Dictionary) -> int:
	return maxi(int(entry.get("workers", 0)), 0)

## Confirmed labor assignments overlaid with this band's pending assigns, keyed by source/role.
## Each value: {kind, workers, x, y, herd_id, policy, pending: bool, + per-source yield fields}.
func effective_worker_map(band: Dictionary) -> Dictionary:
	var merged: Dictionary = {}
	for a in labor_assignments_of(band):
		if not (a is Dictionary):
			continue
		var kind := String((a as Dictionary).get("kind", "")).strip_edges().to_lower()
		var key := pending_key(kind, int(a.get("target_x", -1)), int(a.get("target_y", -1)), String(a.get("fauna_id", "")))
		merged[key] = {
			# **ONE CREW PER ROW AGAIN** (`docs/plan_standing_upkeep.md` §2.5). A row carried a second
			# `improvement_workers` allocation for one slice; the builders are a band-level POOL now, so
			# their hands ride their OWN row of this same map, keyed `builders` like `scout`.
			"kind": kind, "workers": int(a.get("workers", 0)),
			"x": int(a.get("target_x", -1)), "y": int(a.get("target_y", -1)),
			"herd_id": String(a.get("fauna_id", "")),
			# WHERE THIS CREW STOPS, as a fraction of the source's capacity — the whole of the harvest
			# axis. The decoder always inserts it, so an absent one means "no such assignment"; the
			# default is the sim's own.
			"floor": SourceForecast.clamp_floor(
				float(a.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))),
			# THE SECOND AXIS (issue #442) — what this crew is BUILDING, "" when nothing. Carried
			# beside the floor everywhere the map is read, so no consumer has to go back to the raw
			# assignment for it.
			"improvement": String(a.get("improvement", "")), "pending": false,
			# **WHAT THIS CREW CARRIES WHEN IT WORKS THE SOURCE** (`docs/plan_standing_upkeep.md`
			# §4.9 item 12c) — the TAKE kit, the left half of the work inspector's kit pair. It is
			# the assignment's own `kit_id`, already resolved by the sim, and it rides this map for
			# the reason every other key here does: **this map is a hand-listed allowlist**, so a key
			# not copied here does not exist as far as the work board is concerned. (That is exactly
			# how the good-shortfall pair below came out empty on a row whose wire carried it.)
			"kit_id": String(a.get("kit_id", "")),
			# **WHERE THE PLAYER PUT THIS ROW WHEN THE BAND RUNS SHORT** (`docs/plan_standing_upkeep.md`
			# §4.9 item 9b) — one of the three WORDS the decoder writes, normalized here so no reader
			# downstream has to decide what an unrecognised token means. It rides beside the floor for
			# the floor's reason: it is a standing property of the row, and every surface that reads
			# this map reads it.
			"priority": HudWorkVocab.work_priority_of(a.get("priority", "")),
			# Per-source yields (food/turn) for the row headline/tooltip/overhunt flag. `has_yield`
			# gates the readout — a confirmed assignment carries them; a pending one (below) does not.
			"actual_yield": float(a.get("actual_yield", 0.0)),
			"sustainable_yield": float(a.get("sustainable_yield", 0.0)),
			"has_yield": a.has("actual_yield"),
			# Min workers that produced this turn's take — drives the overstaffing note.
			"workers_needed": int(a.get("workers_needed", 0)),
			# Provisions offered but not collected (under-crewed) — drives the muted "· N wasted" note.
			"wasted_yield": float(a.get("wasted_yield", 0.0)),
			# WHEN this source's food lands (index i = i+1 turns from now) — drives the row's arrival
			# tick strip. Empty = "not projected", which renders no strip (never a famine).
			"arrival_schedule": as_schedule(a.get("arrival_schedule", null)),
		}
		# The PRESENCE-SENSITIVE rate keys, copied through only when the wire carried them:
		# `source_yield_readout` distinguishes "absent" (fall back to the actual/sustainable split)
		# from "present and 0", so a `get(..., 0.0)` default here would silently assert a zero.
		for rate_key in OPTIONAL_YIELD_KEYS:
			if (a as Dictionary).has(rate_key):
				(merged[key] as Dictionary)[rate_key] = float((a as Dictionary)[rate_key])
		# **THE MATERIAL ACCOUNT TRAVELS TOO, AND IT IS NOT A SCALAR** (arc #527 follow-up). It rides
		# beside the list above on the same reasoning one account further out — an inedible quarry's
		# WHOLE product is this vector, and a key not copied here does not exist as far as the work
		# board is concerned — but it cannot ride IN it: every entry there is coerced through
		# `float()`, and an Array through that constructor is a hard script error, not a zero. An
		# absent key and an empty array are one reading (no row), so there is nothing to fall back
		# from here either, and it is copied verbatim rather than normalized: `material_rows_of` is
		# the one normalizer and it lives beside the readouts that spend it.
		if (a as Dictionary).has(SourceForecast.ASSIGNMENT_MATERIAL_YIELD_KEY):
			(merged[key] as Dictionary)[SourceForecast.ASSIGNMENT_MATERIAL_YIELD_KEY] = \
				(a as Dictionary)[SourceForecast.ASSIGNMENT_MATERIAL_YIELD_KEY]
		# **AND SO DOES THE GOOD-SIDE SHORTFALL PAIR** (`docs/plan_standing_upkeep.md` §2.7) — what
		# this row's SOURCE was billed in materials and what the band's store paid toward it. Copied
		# verbatim beside the account above, for the same two reasons: they are ARRAYS, so they cannot
		# ride the `float()`-coerced list; and **this map is a hand-listed allowlist**, so a key not
		# copied here does not exist as far as the work board is concerned — which is exactly how the
		# board's good-shortfall note came out empty on a row whose wire carried both terms.
		#
		# **THE PENDING OVERLAY DELIBERATELY DOES NOT PRESERVE THEM.** A `+`/`−` on a row is a TAKE
		# edit; what the source was billed in goods is settled by the turn, so a pending row states no
		# shortfall until the recapture answers — the same shape `actual_yield` has, and the opposite
		# of `improvement`, which the player is mid-edit on.
		for material_key in [SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_DEMAND_KEY,
				SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_SUPPLIED_KEY]:
			if (a as Dictionary).has(material_key):
				(merged[key] as Dictionary)[material_key] = (a as Dictionary)[material_key]
		# **THE SIM'S OWN CREW CEILING FOR THIS ROW**, and it rides presence-sensitively for a
		# sharper reason than the rates above: its `0` is *no crew is useful here* on a HUNT row and
		# *does not apply* on every other one, so a `get(..., 0)` default would assert the first
		# reading about a forage patch and kill its `+`. The board's hunt branch is the only reader
		# (`SourceForecast.with_published_useful_crew`), and an absent key leaves the closed forms
		# answering exactly as they did.
		if (a as Dictionary).has(SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY):
			(merged[key] as Dictionary)[SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY] = \
				int((a as Dictionary)[SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY])
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	for key in pend:
		var pd: Dictionary = pend[key]
		# **THE PUBLISHED CREW CEILING SURVIVES A PENDING EDIT**, for the reason `improvement` does:
		# `assign_labor` does not restate it, and blanking it would drop this row back onto the
		# fightless closed form — a HIGHER ceiling — for the one frame between the click and its
		# confirmation, which is precisely the frame the `+` is being clicked in. Its domain (the
		# hands on the row plus the band's idle ones) is unmoved by a `+`/`-` on this same row.
		var settled: Variant = (merged.get(key, {}) as Dictionary).get(
			SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY, null)
		# **AND THE RANK SURVIVES A PENDING CREW EDIT, for the reason the ceiling above does.**
		# `assign_labor` states no priority — the mark is `work_priority`'s alone — so rebuilding the
		# row from the pending dict would blank a High mark for exactly the frame the `+` is being
		# clicked in, and the player would watch their own prefix flicker off their own press. Read
		# off the CONFIRMED row; a source with no confirmed row at all is a brand-new assignment,
		# which the sim creates at `Normal`, and that is what the normalizer answers for `""`.
		var settled_priority := HudWorkVocab.work_priority_of(
			(merged.get(key, {}) as Dictionary).get("priority", ""))
		# **THERE IS NO BUILD CREW LEFT TO PRESERVE HERE** (`docs/plan_standing_upkeep.md` §2.5). This
		# overlay used to carry the confirmed `improvement_workers` through a pending TAKE edit,
		# because `assign_labor` states no build crew and blanking it made staffed builders read as
		# idle for the turn between the command and its confirmation. The builders are a band-level
		# ROLE now, so their hands are a row of their own that a per-source edit never touches — and
		# the `improvement` below is still preserved for exactly the reason it always was.
		merged[key] = {
			"kind": String(pd.get("kind", "")), "workers": int(pd.get("workers", 0)),
			"x": int(pd.get("x", -1)), "y": int(pd.get("y", -1)),
			"herd_id": String(pd.get("herd_id", "")),
			"floor": SourceForecast.clamp_floor(
				float(pd.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))),
			# The improvement the edit LEAVES IN PLACE. `assign_labor` no longer re-asserts (or
			# clears) an improvement — that is the whole point of the split — so a pending crew edit
			# on a cultivating patch must keep showing the build, not blank it for one turn.
			"improvement": String(pd.get("improvement", "")), "pending": true,
			"priority": settled_priority,
			# **THE KIT THE COMMAND JUST CARRIED, not the one the confirmed row states.** This is the
			# opposite treatment to `priority` above and to the ceiling above that, and the difference
			# is whether `assign_labor` states the field: it does not state a rank, so a rank is read
			# off the settled row; it DOES state the take kit, so the optimistic row states what was
			# sent. On a brand-new assignment there is no settled row to fall back to anyway, which
			# is the case the work inspector's blank, unselectable kit picker was reported on.
			"kit_id": String(pd.get("kit_id", KitRoster.NO_KIT_ID)),
			# A pending (optimistic) assign has no confirmed yield yet — render no yield number.
			# Likewise no confirmed workers_needed, so 0 ⇒ "unknown" ⇒ no overstaffing note until
			# the next snapshot resolves what the source actually used.
			"actual_yield": 0.0, "sustainable_yield": 0.0, "has_yield": false,
			"workers_needed": 0,
			# Nor any projected arrivals — the schedule comes from the sim's forward run, so an
			# un-acknowledged edit shows no strip until the next snapshot projects it.
			"arrival_schedule": PackedFloat32Array(),
		}
		if settled != null:
			(merged[key] as Dictionary)[SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY] = \
				int(settled)
	# **A WITHDRAWAL BLANKS THE IMPROVEMENT AND TOUCHES NOTHING ELSE** (`docs/plan_standing_upkeep.md`
	# §4.7b ④). `unqueue` withdraws a DECLARATION: the crew stays, the floor stays, the banked meter
	# stays — so the one thing that must stop being true on the frame the `✕` is pressed is that this
	# source is building something. Blanking it here rather than at the queue block is what puts the
	# work row's `⌃` back to its offer face on the same frame, off the one map every readout shares.
	for withdrawn_key in pending_unqueues_for(int(band.get("entity", -1))):
		if merged.has(withdrawn_key):
			(merged[withdrawn_key] as Dictionary)["improvement"] = SourceForecast.IMPROVEMENT_NONE
	return merged

## **WHAT THE PLAYER IS DOING ON ONE SOURCE, FOLDED ACROSS EVERY BAND** — `{workers, improvement}`.
## The per-BAND readers above answer "does THIS band work it"; a tile card and an attention producer
## are asking about the source, which several bands may share and which the *panel* band may not be
## one of. Reading `player_band()` there would have called an improved patch unworked whenever the
## crew on it belonged to another band.
##
## Pending-aware, because it reads `effective_worker_map`: a just-issued Cultivate stops the tile card
## reading "Reverting" on the same frame rather than one snapshot later.
##
## `improvement` is the FIRST non-empty one found. At most one improvement is ever in flight on a
## source (the ladder's own rule — an improvement is always the source's next rung), so a second one
## would be a wire contradiction, not a case to merge.
##
## `bands` names the ROSTER to fold over; empty means the INGESTED one, which is what every reader
## running after `ingest_snapshot_bands` wants. The turn-orb attention producers pass the INCOMING
## roster instead — they run BEFORE the ingest (Producer 2's decline diff requires that ordering), so
## the ingested list is still LAST turn's, and a band whose crews the client has not ingested yet
## would read as working nothing at all.
func forage_effort_at(x: int, y: int, bands: Array = []) -> Dictionary:
	return _effort_on(pending_key(LABOR_KIND_FORAGE, x, y, ""), bands)

## The herd twin of `forage_effort_at`, keyed by fauna id — herds MIGRATE, so a herd is never located
## by tile in this model.
func hunt_effort_on(herd_id: String, bands: Array = []) -> Dictionary:
	return _effort_on(pending_key(LABOR_KIND_HUNT, -1, -1, herd_id), bands)

## **`workers` HERE IS THE TAKE CREW ALONE, AND THAT IS NOT THE `staffed_total` OMISSION.** Every
## reader of this answer asks *"is anybody HARVESTING this source"* — the attention producers' unworked
## rung and under-crewed herd, the tile card's worked mark — and a builder takes nothing, so folding
## the build crew in would report a patch nobody is gathering as gathered. What a build is doing here
## is the `improvement` beside it, which those readers have their own use for.
func _effort_on(key: String, bands: Array = []) -> Dictionary:
	var workers := 0
	var improvement := SourceForecast.IMPROVEMENT_NONE
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if not (band_variant is Dictionary):
			continue
		var merged := effective_worker_map(band_variant)
		if not merged.has(key):
			continue
		var m: Dictionary = merged[key]
		workers += int(m.get("workers", 0))
		if improvement == SourceForecast.IMPROVEMENT_NONE:
			improvement = String(m.get("improvement", "")).strip_edges().to_lower()
	return {"workers": workers, "improvement": improvement}

## Optimistic idle = working-age minus **every** hand each effective assignment spends
## (`staffed_total`), minus the bench crew.
##
## **THE BUILDERS ARE IN IT, AND THE MECHANISM THAT PUTS THEM THERE HAS CHANGED ONCE.** This summed
## the wire's per-assignment `workers` alone while a build had its own per-source crew, so a band with
## three hands on a Cultivate reported three idle who were already spent — `3 idle of 18` beside
## `Forage 9 · Hunt 6 · Idle 3`, reported from play. The fix then was to sum `staffed_total`, i.e.
## `workers + improvement_workers`. **`docs/plan_standing_upkeep.md` §2.5 retired that second crew**:
## the builders are a band-level `builders` ROW of this same list now and `staffed_total` is `workers`
## alone again, so they are counted for a different reason and the invariant survives its own
## mechanism. **A reader that started filtering this list by KIND would put the phantom hands back.**
## The sim's `LaborAllocation::assigned_total` sums the same `LaborAssignment::staffed_total`; any
## disagreement here is the client's.
##
## **A WORKER AT THE BENCH IS ASSIGNED LABOR, AND THE BENCH IS NOT A `LaborTarget`.** A band's people
## are spent on the `labor_assignments` this overlays AND on the crafting bench, and a bench crew is
## nowhere in that map — so netting only the assignments counted those hands as free, and every
## "n idle" the player sees (the WORKFORCE zone's three sites, `FactionRollup`'s faction total)
## over-reported by the crew already standing at the bench, in the reassuring direction. This is the
## same subtraction the sim makes in `BandWorkforce::idle()`, which is what `PopulationCohortState`
## publishes as `idle_workers`.
##
## **IT IS STILL COMPUTED AND NOT READ OFF THE WIRE.** `idle_workers` is last snapshot's answer; the
## `+` steppers gate on an OPTIMISTIC idle so a just-issued assignment counts before the turn
## resolves, which is exactly what `effective_worker_map`'s pending overlay supplies. A bench crew
## carries no such overlay (a `bench_crew` edit shows on the next snapshot), so the published crew is
## the right term to subtract.
func effective_idle(band: Dictionary) -> int:
	var assigned := 0
	var merged := effective_worker_map(band)
	for key in merged:
		assigned += staffed_total(merged[key] as Dictionary)
	return max(0, int(band.get("working_age", 0)) - assigned - bench_workers(band))

## The crew standing at the band's crafting bench (`PopulationCohortState.bench.workers`) — spent
## labor that carries no `LaborAssignment`, hence its own reader. Beside `effective_idle` because it
## is that function's third term, and public because the crew stepper's ceiling is a DIFFERENT
## question: see `benchable_workers`.
func bench_workers(band: Dictionary) -> int:
	var bench_variant: Variant = band.get(HudCraftingVocab.BAND_BENCH_KEY, {})
	if not (bench_variant is Dictionary):
		return 0
	var bench: Dictionary = bench_variant
	return max(0, int(bench.get(HudCraftingVocab.BENCH_WORKERS_KEY, 0)))

## **THE CEILING A BENCH CREW STEPPER CLAMPS AGAINST — "how many COULD be at the bench", which is not
## "how many are idle".** The crew already at the bench stays put while its job is swapped, so it is
## not spent from the player's point of view when re-crewing; the sim draws the same distinction
## between `BandWorkforce::idle()` and `benchable()`, and capping the stepper at `effective_idle`
## would pin it to the crew already on it.
func benchable_workers(band: Dictionary) -> int:
	return effective_idle(band) + bench_workers(band)

## Effective worker count on ONE forage tile, overlaying any pending value (the single-source scalar
## twin of `effective_worker_map` — beside it because it reads the same pending overlay + confirmed base).
func effective_forage_workers(band: Dictionary, x: int, y: int) -> int:
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	var key := pending_key(LABOR_KIND_FORAGE, x, y, "")
	if pend.has(key):
		return int((pend[key] as Dictionary).get("workers", 0))
	return workers_for_forage(band, x, y)

## Effective worker count hunting ONE herd, overlaying any pending value.
func effective_hunt_workers(band: Dictionary, herd_id: String) -> int:
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	var key := pending_key(LABOR_KIND_HUNT, -1, -1, herd_id)
	if pend.has(key):
		return int((pend[key] as Dictionary).get("workers", 0))
	return workers_for_hunt(band, herd_id)

## **WHAT ONE BAND'S KEEPING POOL IS BEING ASKED FOR, AND WHAT IT COVERED** — the band-level sum, per
## WEB, of the per-source upkeep the wire already publishes (`docs/plan_standing_upkeep.md` §2.5).
## `{demand, supplied, shortfall}` in work units, all three summed and none of them derived from the
## other two.
##
## **IT REPLACED `assigned_keepers_for`, and the replacement is not cosmetic.** That reader summed the
## per-source `maintain` crews, which no longer exist: maintenance is a band-level role and each
## source is paid a SHARE of the pool. A headcount is therefore no longer available per source, and
## the pool's own state is the only thing that answers *"is this band keeping what it holds"*.
##
## **THE SUM IS OVER EVERY SOURCE THIS BAND HOLDS ON THIS WEB, TAKE CREW OR NOT.** It skipped a row
## with nobody on the take, on the reasoning that `systems::labor::maintenance_shares` skips it —
## which is exactly backwards. **That function deliberately EXCLUDES the take crew from eligibility**
## (`core_sim/tests/forage_cultivation.rs`: *"a patch with no gatherers is still kept by the band's
## pool"*): the row's licence to exist is the ground's own at-risk meter, never who happens to be
## standing on it. So a band that finished a Cultivate and moved its foragers to a richer stand was
## billed by the sim and contributed nothing here — the card understating both its demand and its
## shortfall, silently, on the one state the sim has a regression test for.
##
## **WHAT THE FILTER WAS ALSO DOING IS DONE BY TWO TESTS THAT REMAIN**, and neither is a headcount:
## the KIND test above it excludes every band-wide role (`agriculture` / `husbandry` / `builders` /
## `scout` / `warrior` carry their own kinds, none of which is a web's), and `_upkeep_source_for`
## answers `{}` for a row whose patch or herd the snapshot does not carry — so *"is this row a real
## source rather than a band-wide role"* survives structurally.
##
## A source whose at-risk meter is still being BUILT contributes its demand too, deliberately: the sim
## leaves it out of the pool (its builders answer for it), but its published `upkeepShortfall` is
## still what that meter bleeds, and a band summary that hid it would go quiet on a walked-away build.
##
## **IT ALSO CARRIES THE BARE PER-WORKER WORK RATE the sources it summed publish**, which is what lets
## the pool card project what its OWN hands supply against that demand — read off the same sources the
## demand came from, so a pool with something to pay for always has a rate to price its hands at.
## `maxf` over them rather than the first: every source publishes the same constant, and taking the
## largest means a single malformed row cannot silently zero the projection.
##
## `kind` is `LABOR_KIND_FORAGE` for the agriculture pool and `LABOR_KIND_HUNT` for the husbandry one
## — the two webs' own labor kinds, so the caller never invents a third vocabulary for the split.
func upkeep_pool_state(band: Dictionary, kind: String) -> Dictionary:
	var demand := 0.0
	var supplied := 0.0
	var shortfall := 0.0
	var per_worker := SourceForecast.BUILD_WORK_NONE
	for entry in labor_assignments_of(band):
		if not (entry is Dictionary):
			continue
		var assignment: Dictionary = entry
		if String(assignment.get("kind", "")).to_lower() != kind:
			continue
		var source := _upkeep_source_for(assignment, kind)
		if source.is_empty():
			continue
		var state := SourceForecast.upkeep_state(source, HudComposeVocab.BARE_FORECAST_PREFIX)
		demand += float(state.get("demand", SourceForecast.NO_UPKEEP_DEMAND))
		supplied += float(state.get("supplied", SourceForecast.NO_UPKEEP_DEMAND))
		shortfall += float(state.get("shortfall", SourceForecast.NO_UPKEEP_DEMAND))
		per_worker = maxf(per_worker, SourceForecast.build_work_per_worker_turn(
			source, HudComposeVocab.BARE_FORECAST_PREFIX))
	return {"demand": demand, "supplied": supplied, "shortfall": shortfall,
		POOL_PER_WORKER_TURN_KEY: per_worker}

## The key the bare per-worker work rate rides out on. **Named rather than spelled at each reader**,
## unlike the three figures beside it, because it is read from another script: a typo in a `get` there
## is a silent zero, which would read as *this pool supplies nothing* and mark a fully staffed card.
const POOL_PER_WORKER_TURN_KEY := "per_worker_turn"

## **WHICH WAY THIS BAND SPLITS A POOL IT CANNOT STRETCH** — `PopulationCohortState.upkeepFundMode`,
## normalized to one of the two tokens `upkeep_mode` takes.
##
## **AN EMPTY VALUE READS AS `spread`, and that is the sim's own rule**: empty is only ever a frame
## the sim did not write, and an unstated policy singles nobody out. Anything unrecognised reads the
## same way rather than being echoed back — the control that renders this offers exactly two choices,
## so a third token would light neither and leave the band looking unset.
func upkeep_fund_mode(band: Dictionary) -> String:
	var mode := String(band.get("upkeep_fund_mode", "")).strip_edges().to_lower()
	return mode if mode == HudConst.UPKEEP_FUND_MODE_PRIORITY else HudConst.UPKEEP_FUND_MODE_SPREAD

## The LIVE source dict one assignment row points at — the patch under a forage row, the world herd
## under a hunt row. `{}` where the snapshot does not carry it (a patch outside the ingested lookup,
## a herd that has gone), which the caller skips: an absent source states no upkeep, and inventing a
## zero for it would read as *"this band is keeping everything"*.
##
## **A HERD IS RESOLVED BY ID AND NEVER BY THE ROW'S TARGET TILE**, herds migrating; the patch is
## resolved by tile, a patch being fixed. Same split every other reader of these two makes.
func _upkeep_source_for(assignment: Dictionary, kind: String) -> Dictionary:
	if kind == LABOR_KIND_HUNT:
		return find_world_herd(String(assignment.get("fauna_id", "")))
	var tile := Vector2i(int(assignment.get("target_x", -1)), int(assignment.get("target_y", -1)))
	var patch: Variant = forage_patch_lookup().get(tile, {})
	return patch if patch is Dictionary else {}

## Effective worker count on a band-wide ROLE (scout/warrior), overlaying any pending value — the
## role twin of `effective_forage_workers` / `effective_hunt_workers`. Roles key by kind alone (one
## band-wide slot each), so there is no tile/herd to pass. Returns `{workers, pending}` because the
## role CARDS tint their title amber while an optimistic edit is unconfirmed.
func effective_role_workers(band: Dictionary, kind: String) -> Dictionary:
	var key := pending_key(kind, -1, -1, "")
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	if pend.has(key):
		return {"workers": int((pend[key] as Dictionary).get("workers", 0)), "pending": true}
	return {"workers": workers_for_role(band, kind), "pending": false}

## Workers currently on a band-wide role (scout/warrior); 0 when unstaffed. The role sibling of
## `workers_for_forage` / `workers_for_hunt`.
func workers_for_role(band: Dictionary, kind: String) -> int:
	for entry in labor_assignments_of(band):
		if entry is Dictionary and String((entry as Dictionary).get("kind", "")).to_lower() == kind:
			return int((entry as Dictionary).get("workers", 0))
	return 0

## **THE KIT THE SIM HAS THIS ROLE RESOLVED TO** — the band's own row's `LaborAssignment.kitId`,
## `KitRoster.NO_KIT_ID` when the role is unstaffed (no row at all).
##
## **ON THE `builders` ROW IT IS THE DERIVED ANSWER, NOT A STORED ONE** (`equipment.md` → "THE WIRE
## STATES THE DERIVED KIT"). The sim resolves that row per queue entry at capture — a kit named on the
## row wins, else the roster answers for the HEAD entry's web — so a card reading this field states
## what the pool is holding this turn. Every other role publishes the row's own kit, resolved to its
## job default when the player named none.
##
## `static` because it reads the band dict and nothing else: the role CARD and the two compose sheets
## both need it, and a second private copy is how one of them comes to read `kit_id` off the wrong
## row.
static func role_kit_id(band: Dictionary, kind: String) -> String:
	for entry in labor_assignments_of(band):
		if entry is Dictionary and String((entry as Dictionary).get("kind", "")).to_lower() == kind:
			return String((entry as Dictionary).get("kit_id", KitRoster.NO_KIT_ID))
	return KitRoster.NO_KIT_ID

# ---- THE HEAD OF THE QUEUE — one derivation, two consumers -----------------------------------------
#
# **THE WHOLE `builders` POOL STANDS ON THE HEAD ENTRY** until its meter fills, then on the next
# (`docs/plan_standing_upkeep.md` §4.6b). Two surfaces spend that fact and they must not derive it
# twice: the Builders card picks and greys its kit for the web the head is on, and the compose sheet
# tells a build IN FLIGHT from one merely `◷ Queued` by whether the pool is standing on THIS source.
#
# ⛔ **BOTH USED TO ASK `SourceForecast.build_is_queue_head`, AND THAT WAS THE WRONG BAND'S HEAD.**
# `buildQueuePosition` is published per SOURCE and rides the winning band — the soonest estimate among
# the bands working it — so `position == 0` means *some* band has it at the head, routinely not this
# one. Band B with a plant Cultivate at its own head read the ANIMAL branch whenever band C's head was
# a herd B also worked; and band B's sheet on a source standing THIRD in B's line rendered a running
# meter because C had it first — putting the one-way `Cultivating 0 / 50 work (0%)` face back through
# a door the play report never came in by. Same defect as the queue block's, one consumer over
# (§4.9 item 9a).

## **THE ENTRY THIS BAND'S BUILDERS ARE STANDING ON** — the first row of its own `buildQueue`, `{}`
## when it has nothing queued. THE one derivation of "the head"; the two answers below are its only
## readers, so the card and the sheet cannot come to disagree about which entry is being funded.
func build_queue_head(band: Dictionary) -> Dictionary:
	var entries: Variant = band.get("build_queue", [])
	if not (entries is Array) or (entries as Array).is_empty():
		return {}
	var head: Variant = (entries as Array)[0]
	return head if head is Dictionary else {}

## **WHICH WEB THIS BAND'S BUILDERS ARE ACTUALLY OUT ON** — the branch of the entry at the head of its
## own queue, `KitRoster.BUILD_BRANCH_NONE` when it has nothing queued.
##
## The head is the one entry whose gear is being spent, which is what the Builders card's picker is
## choosing for and therefore what its greying is asked about (`KitRoster.kit_offer`'s third rule).
##
## The branch falls out of the entry's own `kind` — the `LaborAssignment` vocabulary the wire spells
## it in, a patch plant and a herd animal, the same fact `systems::labor` stamps an entry with. No
## source lookup and no walk of the band's rows: the queue names its own entries now.
func head_build_branch(band: Dictionary) -> String:
	var head := build_queue_head(band)
	if head.is_empty():
		return KitRoster.BUILD_BRANCH_NONE
	return KitRoster.build_branch_for_kind(String(head.get("kind", "")).strip_edges().to_lower())

## **IS THIS SOURCE THE ENTRY THIS BAND'S BUILDERS ARE STANDING ON?** — *are they on THIS one*, which
## is what separates a build in flight from one waiting its turn.
##
## It takes the SOURCE dict because its two callers hold one and not an identity: a hunt sheet's
## subject is the herd (named by `id`) and a forage sheet's is the tile_info (named by its `x`/`y`,
## which are the patch's). Resolving that shape here rather than at the call site keeps the whole
## question one answer, which is the property this pair exists to have.
func is_band_build_head(band: Dictionary, kind: String, source: Dictionary) -> bool:
	var head := build_queue_head(band)
	if head.is_empty():
		return false
	var head_kind := String(head.get("kind", "")).strip_edges().to_lower()
	if head_kind != kind:
		return false
	if kind == LABOR_KIND_HUNT:
		return String(head.get("fauna_id", "")) == String(source.get("id", ""))
	return int(head.get("target_x", -1)) == int(source.get("x", -1)) \
		and int(head.get("target_y", -1)) == int(source.get("y", -1))

## Coerce a wire `arrival_schedule` to a PackedFloat32Array. The native decoder already hands over a
## packed array; a fixture (or an absent field) may hand over a plain Array or null.
static func as_schedule(value: Variant) -> PackedFloat32Array:
	if value is PackedFloat32Array:
		return value
	var packed := PackedFloat32Array()
	if value is Array:
		for amount in (value as Array):
			packed.push_back(float(amount))
	return packed

## A band's `labor_assignments` array, or [] when the snapshot carried none (pure read of the band
## dict). `static` + PUBLIC so `DetailFormat` / `AttentionController` reach it as a class-name static
## (`HudBandLaborState.labor_assignments_of`) instead of a fourth private copy or a Callable injection;
## a static is callable unqualified from this class's own methods, which is how the four readers below
## reach it.
static func labor_assignments_of(band: Dictionary) -> Array:
	var v: Variant = band.get("labor_assignments", [])
	return v if v is Array else []

# ---- Player band roster + per-source labor readers -----------------------------------------------

## The player bands the band-picker lists. Normally `_player_bands` (captured each snapshot); falls back
## to `[_player_band]` when only the single band was seeded (e.g. the ui_preview harness, or before the
## first alerts pass) so the dropdown is always populated.
func current_player_bands() -> Array:
	if not _player_bands.is_empty():
		return _player_bands
	return [_player_band] if not _player_band.is_empty() else []

## Resolve a listed player band by its entity id; {} if it is no longer present.
func player_band_by_entity(entity: int) -> Dictionary:
	for b in current_player_bands():
		if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
			return b
	return {}

## **THE SAME BAND, REACHED FROM THE OTHER HANDLE** — its durable `BandId`, which is what the SIM
## names a band by. `player_band_by_entity` above answers the client-local handle every overlay reader
## keys on; this one exists for the single direction that cannot use it, an event's `band=` detail
## token arriving from the wire (`EventDockPanel.band_work_tab_requested`).
##
## **THE ROSTER IS THE ONLY PLACE THE TWO HANDLES MEET**, which is why the join is here and not at
## either end of that hop: the dock holds no entities and `BandPanelController` takes no `band_id`.
## `{}` when the roster does not know the id — a band that starved out, or a row still held from
## before a resync.
func player_band_by_band_id(band_id: int) -> Dictionary:
	for b in current_player_bands():
		if b is Dictionary and int((b as Dictionary).get("band_id", -1)) == band_id:
			return b
	return {}

## The band's standing FORAGE assignment on (x,y) — `{}` when it works no such tile. The one lookup
## behind the worker count, the seeded policy and the drawer's standing summary, so the three can
## never read different assignments.
func forage_assignment_of(band: Dictionary, x: int, y: int) -> Dictionary:
	for entry in labor_assignments_of(band):
		if not (entry is Dictionary):
			continue
		var a: Dictionary = entry
		if String(a.get("kind", "")).to_lower() == LABOR_KIND_FORAGE \
				and int(a.get("target_x", -1)) == x and int(a.get("target_y", -1)) == y:
			return a
	return {}

## The band's standing HUNT assignment on `herd_id` — `{}` when it hunts no such herd. The herd twin
## of `forage_assignment_of`.
func hunt_assignment_of(band: Dictionary, herd_id: String) -> Dictionary:
	for entry in labor_assignments_of(band):
		if not (entry is Dictionary):
			continue
		var a: Dictionary = entry
		if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
			return a
	return {}

## Workers currently foraging a specific in-range tile; 0 when unstaffed.
func workers_for_forage(band: Dictionary, x: int, y: int) -> int:
	return int(forage_assignment_of(band, x, y).get("workers", 0))

## Workers currently hunting a specific herd; 0 when unstaffed.
func workers_for_hunt(band: Dictionary, herd_id: String) -> int:
	return int(hunt_assignment_of(band, herd_id).get("workers", 0))

## The ESCAPEMENT FLOOR of the band's existing hunt on `herd_id`, else the default. The wire always
## carries the field (the decoder inserts it unconditionally), so an ABSENT one means no such
## assignment — which is exactly when the default is the right answer for a picker being seeded.
func floor_for_hunt(band: Dictionary, herd_id: String) -> float:
	var assignment := hunt_assignment_of(band, herd_id)
	if not assignment.has("floor"):
		return DEFAULT_HARVEST_FLOOR
	return SourceForecast.clamp_floor(float(assignment["floor"]))

## The plant twin: the floor of the band's existing forage on (x,y), else the default.
func floor_for_forage(band: Dictionary, x: int, y: int) -> float:
	var assignment := forage_assignment_of(band, x, y)
	if not assignment.has("floor"):
		return DEFAULT_HARVEST_FLOOR
	return SourceForecast.clamp_floor(float(assignment["floor"]))

## **THE SECOND AXIS** (issue #442) — what the band's existing hunt on `herd_id` is BUILDING, or
## `IMPROVEMENT_NONE` when it builds nothing. Validated against the animal ladder so a mis-spelled or
## cross-web value reads as "building nothing" rather than driving a control the source cannot offer.
func improvement_for_hunt(band: Dictionary, herd_id: String) -> String:
	return _validated_improvement(hunt_assignment_of(band, herd_id), SourceForecast.HUNT_IMPROVEMENTS)

## The plant twin: what the band's existing forage on (x,y) is building.
func improvement_for_forage(band: Dictionary, x: int, y: int) -> String:
	return _validated_improvement(
		forage_assignment_of(band, x, y), SourceForecast.FORAGE_IMPROVEMENTS)

func _validated_improvement(assignment: Dictionary, web: Array) -> String:
	var improvement := String(assignment.get("improvement", "")).strip_edges().to_lower()
	return improvement if improvement in web else SourceForecast.IMPROVEMENT_NONE

## **RETIRED — `build_workers_for_forage` / `_hunt`, the per-source BUILD crew**
## (`docs/plan_standing_upkeep.md` §2.5). `LaborAssignment.improvementWorkers` is off the wire: a verb
## DECLARES and names no hands, and the hands stand on the band's `builders` role. There is no
## per-source build number left to read, and nothing here may re-derive one — what answers *"who could
## be raising this source"* is `build_crew_forage` / `build_crew_hunt` below, off that role row.
##
## **THERE IS NO `maintain_workers_for_*` TWIN EITHER** (§2.5), for the same reason one slice earlier:
## a source gets a SHARE of the keeping pool, read through `SourceForecast.upkeep_state`.

## **THE CROP THIS BAND ASKED FOR ON A PATCH** — the player's stated SELECTION, which is not the same
## question as `ForagePatchState.committedSpecies`.
##
## The patch's field is what the GROUND is committed to and is only set once a crew has worked it; this
## one exists from the moment the player chose, so it is the only answer available on a sheet reopened
## over ground nobody has worked yet. `""` = *"pick the tile's dominant legal plant for me"*, which is
## a real instruction rather than an absent one.
func species_for_forage(band: Dictionary, x: int, y: int) -> String:
	return String(forage_assignment_of(band, x, y).get("species", "")).strip_edges()

## **WHICH PLANTS THIS BAND'S CREW ALREADY CARRIES HOME FROM (x, y)** — the standing take selection,
## EMPTY for the whole basket. A different question from `species_for_forage` one row up: that is the
## COMMIT crop a Cultivate/Sow names, this is what the gatherers pick up at rung 1, and a crew can be
## doing both at once on different plants.
##
## **THE WIRE'S ORDER IS PRESERVED** — the sim sorts and deduplicates the set at the source, so this is
## already the ascending key order the composed selection is normalised into, and the two are therefore
## comparable by value. Never re-sort it into a display order: what would go back on the wire is a
## different instruction from the one that came off it.
func take_species_for_forage(band: Dictionary, x: int, y: int) -> PackedStringArray:
	var keys := PackedStringArray()
	for key in forage_assignment_of(band, x, y).get("take_species", []):
		var trimmed := String(key).strip_edges()
		if trimmed != "":
			keys.append(trimmed)
	return keys

## **THE HANDS ONE COMPOSE SHEET MAY SPEND — idle, plus the crew this band already has on THIS
## source.** It is the ceiling the sim judges `assign_labor` against (`set_assignment` gives back the
## standing crew of the row being restated), so a sheet re-editing a fully-staffed source can still
## reach the count it is already at rather than being clamped down to the band's bare idle.
##
## **THE BUILD TERM IS GONE, AND SO IS THE PAIRED-CEILING PROBLEM IT SOLVED**
## (`docs/plan_standing_upkeep.md` §2.5). This pool carried `+ this source's builders` while a sheet
## edited two crews and committed them as one transaction; a verb states no hands now, so the sheet
## has ONE stepper and this is once again a plain per-activity ceiling — the shape it started as,
## reached from the other side.
##
## It reads the published `idleWorkers` — `BandWorkforce::idle()`, every committed hand netted out,
## the bench included — rather than `effective_idle`, which is the OPTIMISTIC answer carrying the
## pending overlay. A ceiling composed from that would offer a crew on the strength of a command the
## server has not acknowledged.
func source_crew_pool_hunt(band: Dictionary, herd_id: String) -> int:
	return maxi(int(band.get("idle_workers", 0)) + workers_for_hunt(band, herd_id), 0)

func source_crew_pool_forage(band: Dictionary, x: int, y: int) -> int:
	return maxi(int(band.get("idle_workers", 0)) + workers_for_forage(band, x, y), 0)

## **A RUNG THIS FACTION HAS DECLARED AND PUT NOBODY ON** — the declared verb when every band working
## the source has zero builders on it, `IMPROVEMENT_NONE` otherwise. The client half of the
## declared-but-unstaffed readout (`SourceForecast.unstaffed_build_state` decides which of the two
## unstaffed states it is; this only answers *is anybody building it*).
##
## **IT IS READ OFF THE CONFIRMED WIRE ROWS ALONE, and that is not an oversight.** The declaration
## rides a `LaborAssignment` the sim DERIVES from the band's queue entry, and the hands ride that same
## band's `builders` row, so reading both off the confirmed frame is the only way the pair describes
## one moment. The optimistic overlay carries a declaration but never a role edit, so a pending-aware
## read would report *declared, nobody on it* for the turn after a player staffed the builders — a
## warning fired at exactly the person who did the right thing. Silence for one snapshot is the honest
## degrade: this is a standing fact, and it renders from the frame the sim confirms it.
##
## **FOLDED ACROSS EVERY BAND WORKING THE SOURCE, because a source several bands can reach may be
## built by any of them.** The verb is the first non-empty declaration (at most one rung is ever in
## flight on one source); the crew is the SUM of those bands' pools, so one band's builders cover
## another band's bare declaration.
func unstaffed_build_forage(x: int, y: int, bands: Array = []) -> String:
	return _unstaffed_build(bands,
		func(band: Dictionary) -> String: return improvement_for_forage(band, x, y),
		func(band: Dictionary) -> bool: return not forage_assignment_of(band, x, y).is_empty())

func unstaffed_build_hunt(herd_id: String, bands: Array = []) -> String:
	return _unstaffed_build(bands,
		func(band: Dictionary) -> String: return improvement_for_hunt(band, herd_id),
		func(band: Dictionary) -> bool: return not hunt_assignment_of(band, herd_id).is_empty())

## **THE HANDS THAT COULD BE RAISING THIS SOURCE — the `builders` POOL of every band that works it**
## (`docs/plan_standing_upkeep.md` §2.5). It was a sum of per-source `improvementWorkers`; a verb
## states no hands now, so what answers *"is anybody building this"* is the band-level role.
##
## **IT IS RESTRICTED TO THE BANDS THAT WORK THE SOURCE, and that restriction is the whole of its
## honesty.** An improvement verb only ever reaches a band already working the source, so those are
## exactly the bands that can hold it in a queue; summing every player band's pool instead would put a
## crew on every source on the map the moment one band staffed a single builder.
##
## **IT IS A CREW, NOT A FUNDING CLAIM.** The whole pool goes on the HEAD of its band's queue, so a
## waiting entry is not being worked by these hands this turn — which is what
## `SourceForecast.build_queue_position` rides beside the countdown to say. What this answers is the
## one fact `BUILD_METER_HOLDS` cannot carry: whether there is a crew to be treading water at all, or
## whether the meter is parked (`SourceForecast.build_pace`).
func build_crew_forage(x: int, y: int, bands: Array = []) -> int:
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if band_variant is Dictionary \
				and not forage_assignment_of(band_variant, x, y).is_empty():
			builders += workers_for_role(band_variant, HudConst.LABOR_KIND_BUILDERS)
	return builders

func build_crew_hunt(herd_id: String, bands: Array = []) -> int:
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if band_variant is Dictionary \
				and not hunt_assignment_of(band_variant, herd_id).is_empty():
			builders += workers_for_role(band_variant, HudConst.LABOR_KIND_BUILDERS)
	return builders

## `works_source` says whether this band holds the source at all — the same restriction
## `build_crew_*` applies, asked as a predicate so one band's pool is visited once.
##
## **THE POOL IS READ PENDING-AWARE, and the DECLARATION is not.** They are two different rows and
## only one of them can be edited optimistically here: the crew is the band's own `builders` role,
## which the role card stages through `pending_assigns_for`, while the declaration is the SOURCE's
## `LaborAssignment.improvement` and has no pending overlay to read. Reading the confirmed crew is
## what made this warning fire at a player for the turn AFTER they staffed the role — the accusation
## beside a Builders card already reading `2`. **A pending role edit cannot be refused**
## (`assign_labor` clamps the count rather than rejecting it), so the optimistic read can only ever
## silence a warning that was about to stop being true anyway.
func _unstaffed_build(bands: Array, declared_of: Callable, works_source: Callable) -> String:
	var declared := SourceForecast.IMPROVEMENT_NONE
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if not (band_variant is Dictionary):
			continue
		var band: Dictionary = band_variant
		if not bool(works_source.call(band)):
			continue
		if declared == SourceForecast.IMPROVEMENT_NONE:
			declared = String(declared_of.call(band))
		builders += int(effective_role_workers(
			band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))
	if builders > SourceForecast.BUILD_CREW_NONE:
		return SourceForecast.IMPROVEMENT_NONE
	return declared
