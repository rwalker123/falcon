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

# The pending-key labor vocabulary. Mirrors `HudLayer.LABOR_KIND_FORAGE` / `LABOR_KIND_HUNT` (the
# command-side names); a forage source keys by tile, a hunt source by herd, every other role (scout /
# warrior) is one band-wide slot keyed by its own kind.
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"

# The policy rungs each source kind offers — the four extractive rungs plus the two kind-specific
# INVESTMENT rungs (hunt: tame/corral, forage: cultivate/sow). Canonical here (the labor readers below
# re-seed a compose picker against them); `HudLayer` re-exports both via `const X = HudBandLaborState.X`.
# `DEFAULT_HUNT_POLICY` aliases SourceForecast's — one source of truth, shared by both files.
const HUNT_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "tame", "corral"]
const FORAGE_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "cultivate", "sow"]
const DEFAULT_HUNT_POLICY := SourceForecast.DEFAULT_HUNT_POLICY

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

func record_pending_assign(entity: int, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String) -> void:
	if entity < 0:
		return
	var entry: Dictionary = _pending_labor.get(entity, {})
	entry["turn"] = _current_turn
	var assigns: Dictionary = entry.get("assign", {})
	assigns[pending_key(kind, x, y, herd_id)] = {
		"kind": kind, "workers": max(0, workers), "x": x, "y": y, "herd_id": herd_id, "policy": policy,
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

## Confirmed labor assignments overlaid with this band's pending assigns, keyed by source/role.
## Each value: {kind, workers, x, y, herd_id, policy, pending: bool, + per-source yield fields}.
func effective_worker_map(band: Dictionary) -> Dictionary:
	var merged: Dictionary = {}
	for a in _labor_assignments_of(band):
		if not (a is Dictionary):
			continue
		var kind := String((a as Dictionary).get("kind", "")).strip_edges().to_lower()
		var key := pending_key(kind, int(a.get("target_x", -1)), int(a.get("target_y", -1)), String(a.get("fauna_id", "")))
		merged[key] = {
			"kind": kind, "workers": int(a.get("workers", 0)),
			"x": int(a.get("target_x", -1)), "y": int(a.get("target_y", -1)),
			"herd_id": String(a.get("fauna_id", "")), "policy": String(a.get("policy", "")), "pending": false,
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
	var pend := pending_assigns_for(int(band.get("entity", -1)))
	for key in pend:
		var pd: Dictionary = pend[key]
		merged[key] = {
			"kind": String(pd.get("kind", "")), "workers": int(pd.get("workers", 0)),
			"x": int(pd.get("x", -1)), "y": int(pd.get("y", -1)),
			"herd_id": String(pd.get("herd_id", "")), "policy": String(pd.get("policy", "")), "pending": true,
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

## Optimistic idle = working-age minus the sum of effective worker counts.
func effective_idle(band: Dictionary) -> int:
	var assigned := 0
	var merged := effective_worker_map(band)
	for key in merged:
		assigned += int((merged[key] as Dictionary).get("workers", 0))
	return max(0, int(band.get("working_age", 0)) - assigned)

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
	for entry in _labor_assignments_of(band):
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

## A band's `labor_assignments` array (pure read of the band dict).
func _labor_assignments_of(band: Dictionary) -> Array:
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
	for entry in _labor_assignments_of(band):
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
	for entry in _labor_assignments_of(band):
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

## The take policy of the band's existing hunt on `herd_id`, else the default.
func policy_for_hunt(band: Dictionary, herd_id: String) -> String:
	var policy := String(hunt_assignment_of(band, herd_id).get("policy", "")).strip_edges().to_lower()
	# HUNT_POLICY_OPTIONS, not the extractive four: a herd already being Corralled must
	# re-seed the compose picker as Corral, or re-staffing it would silently drop the pen.
	return policy if policy in HUNT_POLICY_OPTIONS else DEFAULT_HUNT_POLICY

## The take policy of the band's existing forage on (x,y), else the default.
func policy_for_forage(band: Dictionary, x: int, y: int) -> String:
	var policy := String(forage_assignment_of(band, x, y).get("policy", "")).strip_edges().to_lower()
	# FORAGE_POLICY_OPTIONS, not the extractive four: a patch already being Cultivated must
	# re-seed the compose picker as Cultivate, or re-staffing it would silently drop the
	# investment back to Sustain (and the patch would go feral).
	return policy if policy in FORAGE_POLICY_OPTIONS else DEFAULT_HUNT_POLICY

## Max workers a band can commit to ONE source: its idle workers plus any it already has on that
## source (the assign REPLACES that count, so re-editing an existing assignment isn't capped below its
## current staffing). Reduces to `idle_workers` for a fresh source.
func assignable_hunt_workers(band: Dictionary, herd_id: String) -> int:
	return int(band.get("idle_workers", 0)) + workers_for_hunt(band, herd_id)

func assignable_forage_workers(band: Dictionary, x: int, y: int) -> int:
	return int(band.get("idle_workers", 0)) + workers_for_forage(band, x, y)
