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

## Optimistic idle = working-age minus the sum of effective worker counts **minus the bench crew**.
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
		assigned += int((merged[key] as Dictionary).get("workers", 0))
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

## Total herders actually assigned to a herd, summed across every player band and overlaying pending
## (staged) edits so a just-staffed herder counts IMMEDIATELY — before the turn resolves. A managed
## herd's local crew ride `Hunt` assignments (the policy is Corral/Sustain), so this sums the hunt
## workers targeting `herd_id`. This is the ACTUAL staffing the herd drawer + the work panel read
## against `herders_needed`; it deliberately does NOT reconstruct from last turn's resolved
## `herded_fraction`, which lags a turn and produced the self-contradictory "5 needed · only 2 of 5
## working" the instant after the player assigned a herder (fauna neglect-escape arc).
func assigned_herders_for(herd_id: String) -> int:
	if herd_id == "":
		return 0
	var total := 0
	for band in current_player_bands():
		if band is Dictionary:
			total += effective_hunt_workers(band, herd_id)
	return total

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

## Max workers a band can commit to ONE source: its idle workers plus any it already has on that
## source (the assign REPLACES that count, so re-editing an existing assignment isn't capped below its
## current staffing). Reduces to `idle_workers` for a fresh source.
func assignable_hunt_workers(band: Dictionary, herd_id: String) -> int:
	return int(band.get("idle_workers", 0)) + workers_for_hunt(band, herd_id)

func assignable_forage_workers(band: Dictionary, x: int, y: int) -> int:
	return int(band.get("idle_workers", 0)) + workers_for_forage(band, x, y)
