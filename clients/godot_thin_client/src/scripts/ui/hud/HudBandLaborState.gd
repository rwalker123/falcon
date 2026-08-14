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

func pending_assigns_for(entity: int) -> Dictionary:
	var e: Variant = _pending_labor.get(entity, {})
	if not (e is Dictionary):
		return {}
	var a: Variant = (e as Dictionary).get("assign", {})
	return a if a is Dictionary else {}

## `improvement` is what the source will be building once the edit lands — the composed improvement on
## a sheet that just ticked one on, else whatever it was already building. It is recorded because
## `assign_labor` deliberately does NOT touch that axis (issue #442), so an optimistic overlay that
## dropped it would flash a running build off the board for one turn.
func record_pending_assign(entity: int, kind: String, workers: int, x: int, y: int, herd_id: String,
		floor: float, improvement: String = SourceForecast.IMPROVEMENT_NONE) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	var assigns: Dictionary = entry.get("assign", {})
	assigns[pending_key(kind, x, y, herd_id)] = {
		"kind": kind, "workers": max(0, workers), "x": x, "y": y, "herd_id": herd_id,
		"floor": SourceForecast.clamp_floor(floor), "improvement": improvement,
	}
	entry["assign"] = assigns
	_pending_labor[entity] = entry
	changed.emit(&"pending")

func record_pending_move(entity: int, x: int, y: int) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	entry["move"] = {"x": x, "y": y}
	_pending_labor[entity] = entry
	changed.emit(&"pending")

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

## The build crew's key on a wire assignment AND on a merged row — one spelling, because
## `effective_worker_map` copies it through under the wire's own name so `staffed_total` can read
## either shape.
const BUILD_WORKERS_KEY := "improvement_workers"

## **THE HANDS ONE MERGED ROW SPENDS — the take crew AND the builders**, the client's transcription of
## `LaborAssignment::staffed_total`. A source carries two allocations (`docs/plan_standing_upkeep.md`
## §2.2) and the sim charges the band for BOTH, so any reader asking *"how much of this band is
## committed here"* has to sum the pair.
##
## **SUMMING `workers` ALONE IS THE DEFECT THIS EXISTS TO MAKE UNSPELLABLE.** `effective_idle` did
## exactly that, so a band with three builders on a Cultivate reported three idle who did not exist —
## and every ceiling built on top of it (the role cards' steppers, the pen-extend crew, the bench's
## `benchable_workers`) offered crews the sim then refused, naming an idle count the player could see
## on the panel. Reported from play: `3 idle of 18` on a band whose every hand was spent.
static func staffed_total(entry: Dictionary) -> int:
	return maxi(int(entry.get("workers", 0)), 0) \
		+ maxi(int(entry.get(BUILD_WORKERS_KEY, 0)), 0)

## The build crew's key on a wire assignment AND on a merged row — one spelling, because
## `effective_worker_map` copies it through under the wire's own name so `staffed_total` can read
## Confirmed labor assignments overlaid with this band's pending assigns, keyed by source/role.
## Each value: {kind, workers, improvement_workers, x, y, herd_id, policy, pending: bool, + per-source
## yield fields}.
func effective_worker_map(band: Dictionary) -> Dictionary:
	var merged: Dictionary = {}
	for a in labor_assignments_of(band):
		if not (a is Dictionary):
			continue
		var kind := String((a as Dictionary).get("kind", "")).strip_edges().to_lower()
		var key := pending_key(kind, int(a.get("target_x", -1)), int(a.get("target_y", -1)), String(a.get("fauna_id", "")))
		merged[key] = {
			"kind": kind, "workers": int(a.get("workers", 0)),
			# **THE SECOND ALLOCATION TRAVELS WITH THE FIRST** (`docs/plan_standing_upkeep.md` §2.2).
			# The take crew and the build crew are two fields of ONE assignment and the band pays for
			# both, so a map that carried only the take let every consumer of it describe a band with
			# more hands than it has.
			BUILD_WORKERS_KEY: maxi(int(a.get(BUILD_WORKERS_KEY, 0)), 0),
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
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	for key in pend:
		var pd: Dictionary = pend[key]
		# The builders the edit LEAVES IN PLACE, for the reason the `improvement` below is kept:
		# `assign_labor` states neither, so a pending TAKE edit that blanked the build crew would make
		# three staffed builders vanish from the workforce bar and reappear as idle hands for the turn
		# between the command and its confirmation — the very miscount this map's `staffed_total` fixes.
		var standing_builders := maxi(
			int((merged.get(key, {}) as Dictionary).get(BUILD_WORKERS_KEY, 0)), 0)
		merged[key] = {
			"kind": String(pd.get("kind", "")), "workers": int(pd.get("workers", 0)),
			BUILD_WORKERS_KEY: standing_builders,
			"x": int(pd.get("x", -1)), "y": int(pd.get("y", -1)),
			"herd_id": String(pd.get("herd_id", "")),
			"floor": SourceForecast.clamp_floor(
				float(pd.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))),
			# The improvement the edit LEAVES IN PLACE. `assign_labor` no longer re-asserts (or
			# clears) an improvement — that is the whole point of the split — so a pending crew edit
			# on a cultivating patch must keep showing the build, not blank it for one turn.
			"improvement": String(pd.get("improvement", "")), "pending": true,
			# A pending (optimistic) assign has no confirmed yield yet — render no yield number.
			# Likewise no confirmed workers_needed, so 0 ⇒ "unknown" ⇒ no overstaffing note until
			# the next snapshot resolves what the source actually used.
			"actual_yield": 0.0, "sustainable_yield": 0.0, "has_yield": false,
			"workers_needed": 0,
			# Nor any projected arrivals — the schedule comes from the sim's forward run, so an
			# un-acknowledged edit shows no strip until the next snapshot projects it.
			"arrival_schedule": PackedFloat32Array(),
		}
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

## Optimistic idle = working-age minus **every** hand each effective assignment spends — the take crew
## AND the builders (`staffed_total`) — minus the bench crew.
##
## **THE BUILDERS WERE MISSING AND EVERY CEILING BUILT ON THIS INHERITED IT.** This summed the wire's
## per-assignment `workers` alone, which is the TAKE crew, so a band with three hands on a Cultivate
## reported three idle who were already spent — `3 idle of 18` beside `Forage 9 · Hunt 6 · Idle 3`,
## reported from play. The sim's `LaborAllocation::assigned_total` sums `LaborAssignment::staffed_total`,
## i.e. `workers + improvement_workers`; this is that same sum, and the disagreement was entirely the
## client's.
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
## **THE SUM IS OVER THE SOURCES THE SIM ITSELF FUNDS.** A row with nobody on the take is skipped
## exactly as `systems::labor::maintenance_shares` skips it — its supply is never stamped, so counting
## its demand would show a shortfall the sim is not charging anybody for. A source whose at-risk meter
## is still being BUILT contributes its demand too, deliberately: the sim leaves it out of the pool
## (its builders answer for it), but its published `upkeepShortfall` is still what that meter bleeds,
## and a band summary that hid it would go quiet on a walked-away build.
##
## `kind` is `LABOR_KIND_FORAGE` for the agriculture pool and `LABOR_KIND_HUNT` for the husbandry one
## — the two webs' own labor kinds, so the caller never invents a third vocabulary for the split.
func upkeep_pool_state(band: Dictionary, kind: String) -> Dictionary:
	var demand := 0.0
	var supplied := 0.0
	var shortfall := 0.0
	for entry in labor_assignments_of(band):
		if not (entry is Dictionary):
			continue
		var assignment: Dictionary = entry
		if String(assignment.get("kind", "")).to_lower() != kind:
			continue
		if int(assignment.get("workers", 0)) <= 0:
			continue
		var source := _upkeep_source_for(assignment, kind)
		if source.is_empty():
			continue
		var state := SourceForecast.upkeep_state(source, HudComposeVocab.BARE_FORECAST_PREFIX)
		demand += float(state.get("demand", SourceForecast.NO_UPKEEP_DEMAND))
		supplied += float(state.get("supplied", SourceForecast.NO_UPKEEP_DEMAND))
		shortfall += float(state.get("shortfall", SourceForecast.NO_UPKEEP_DEMAND))
	return {"demand": demand, "supplied": supplied, "shortfall": shortfall}

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

## **THE OTHER CREW THIS BAND HAS ON A SOURCE** (`docs/plan_standing_upkeep.md` §2.2) — the hands on
## the BUILD, beside `workers_for_*`'s take crew. Both ship on the assignment now; until they did, the
## client could SEND a build crew and never read one back, which is what forced the compose sheet to
## seed its stepper at nobody.
##
## **THERE IS NO `maintain_workers_for_*` TWIN** (§2.5). Maintenance left the tile: the wire's
## `maintainWorkers` slot is deprecated and the keeping is a band-level role, so a per-source keeper
## count is a question with no answer. What a source gets is a SHARE of the pool, read through
## `SourceForecast.upkeep_state`.
##
## **`0` IS A REAL READING AND IS THE COMMON ONE** — no verb in flight genuinely means no builders. It
## must never be treated as "unknown" and replaced by a seed: doing so would put phantom hands on
## every unbuilt source in the game.
func build_workers_for_forage(band: Dictionary, x: int, y: int) -> int:
	return maxi(int(forage_assignment_of(band, x, y).get("improvement_workers", 0)), 0)

func build_workers_for_hunt(band: Dictionary, herd_id: String) -> int:
	return maxi(int(hunt_assignment_of(band, herd_id).get("improvement_workers", 0)), 0)

## **THE CROP THIS BAND ASKED FOR ON A PATCH** — the player's stated SELECTION, which is not the same
## question as `ForagePatchState.committedSpecies`.
##
## The patch's field is what the GROUND is committed to and is only set once a crew has worked it; this
## one exists from the moment the player chose, so it is the only answer available on a sheet reopened
## over ground nobody has worked yet. `""` = *"pick the tile's dominant legal plant for me"*, which is
## a real instruction rather than an absent one.
func species_for_forage(band: Dictionary, x: int, y: int) -> String:
	return String(forage_assignment_of(band, x, y).get("species", "")).strip_edges()

## **THE HANDS ONE COMPOSE SHEET MAY SPEND — idle, plus EVERY crew this band has committed on THIS
## source.** The sheet edits a source's take and its build together and commits them as one
## transaction, so it is clamped as one: each stepper's ceiling is this pool minus what the OTHER
## stepper currently proposes (`DrawerComposeController`), which is what makes two hands dropped off
## the take available to the builders in the same gesture rather than a commit-and-reopen later.
##
## **IT REPLACED A PAIR OF PER-ACTIVITY CEILINGS, and the pair is what the bug was.** `idle + this
## source's take` and `idle + this source's builders` are each the ceiling the sim judges ONE command
## against (`LaborAllocation::idle_for` gives back only the activity being restated), and read on
## their own they are correct; read side by side on a sheet that edits both, they describe a band with
## more hands than it has in one direction and fewer in the other. A fully-allocated band's BUILDERS
## stepper sat dead at `0` no matter what the player did to the take beside it.
##
## It reads the published `idleWorkers` — `BandWorkforce::idle()`, every committed hand netted out,
## the bench included — rather than `effective_idle`, which sums the wire's per-assignment `workers`
## and so counts only the TAKE crews.
func source_crew_pool_hunt(band: Dictionary, herd_id: String) -> int:
	return maxi(int(band.get("idle_workers", 0)) + workers_for_hunt(band, herd_id)
		+ build_workers_for_hunt(band, herd_id), 0)

func source_crew_pool_forage(band: Dictionary, x: int, y: int) -> int:
	return maxi(int(band.get("idle_workers", 0)) + workers_for_forage(band, x, y)
		+ build_workers_for_forage(band, x, y), 0)

## **A RUNG THIS FACTION HAS DECLARED AND PUT NOBODY ON** — the declared verb when every band working
## the source has zero builders on it, `IMPROVEMENT_NONE` otherwise. The client half of the
## declared-but-unstaffed readout (`SourceForecast.unstaffed_build_state` decides which of the two
## unstaffed states it is; this only answers *is anybody building it*).
##
## **IT IS READ OFF THE CONFIRMED WIRE ROW ALONE, and that is not an oversight.** The declaration and
## the crew are two fields of ONE `LaborAssignment`, so reading them from one row is the only way the
## pair can describe one moment. The optimistic overlay carries a declaration but no build crew
## (`assign_labor` never states one), so a pending-aware read would report *declared, nobody on it* for
## the turn after a player committed a build WITH builders — a warning fired at exactly the person who
## did the right thing. Silence for one snapshot is the honest degrade: this is a standing fact, and it
## renders from the frame the sim confirms it.
##
## **FOLDED ACROSS EVERY BAND, because a source several bands can reach may be built by any of them.**
## The verb is the first non-empty declaration (at most one rung is ever in flight on one source); the
## crew is the SUM, so one band's builders cover another band's bare declaration.
func unstaffed_build_forage(x: int, y: int, bands: Array = []) -> String:
	return _unstaffed_build(bands,
		func(band: Dictionary) -> String: return improvement_for_forage(band, x, y),
		func(band: Dictionary) -> int: return build_workers_for_forage(band, x, y))

func unstaffed_build_hunt(herd_id: String, bands: Array = []) -> String:
	return _unstaffed_build(bands,
		func(band: Dictionary) -> String: return improvement_for_hunt(band, herd_id),
		func(band: Dictionary) -> int: return build_workers_for_hunt(band, herd_id))

## **THE BUILD CREW ON A SOURCE, FOLDED ACROSS EVERY BAND THAT CAN REACH IT** — the same fold
## `_unstaffed_build` makes, published on its own because the herd drawer needs the COUNT rather than
## the derived state (`docs/plan_standing_upkeep.md` §4.6a: `BUILD_METER_HOLDS` is a crew treading
## water with a crew on it and a build parked on purpose without one).
func build_crew_forage(x: int, y: int, bands: Array = []) -> int:
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if band_variant is Dictionary:
			builders += build_workers_for_forage(band_variant, x, y)
	return builders

func build_crew_hunt(herd_id: String, bands: Array = []) -> int:
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if band_variant is Dictionary:
			builders += build_workers_for_hunt(band_variant, herd_id)
	return builders

func _unstaffed_build(bands: Array, declared_of: Callable, builders_of: Callable) -> String:
	var declared := SourceForecast.IMPROVEMENT_NONE
	var builders := 0
	for band_variant in (bands if not bands.is_empty() else current_player_bands()):
		if not (band_variant is Dictionary):
			continue
		var band: Dictionary = band_variant
		if declared == SourceForecast.IMPROVEMENT_NONE:
			declared = String(declared_of.call(band))
		builders += int(builders_of.call(band))
	if builders > SourceForecast.BUILD_CREW_NONE:
		return SourceForecast.IMPROVEMENT_NONE
	return declared
