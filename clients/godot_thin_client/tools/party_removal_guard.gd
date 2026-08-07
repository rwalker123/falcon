extends Node

## Headless gate for **`removedPopulations` reaching the Band panel** — the client half of the
## ghost-party bug.
##
## ## The gap this closes
##
## Reported from play: the parties row's red `✕` did nothing, repeatedly, and the feed answered
## `Expedition 2 does not exist in the simulation`. The recall was never broken — the sim was
## correctly refusing a party the CLIENT was still drawing. A party that a `send_hunt_expedition`
## spawned and an in-camp `recall_expedition` despawned inside ONE tick was published on a **held**
## frame, which does not store into the baseline, so `diff_removed` had nothing to sweep and every
## later frame carried `populations: []` / `removedPopulations: []`. The row never healed.
##
## That is fixed sim-side. What was never exercised is the other end: **nothing proved the client
## acts on a `removedPopulations` id for a party removed outside a turn boundary.** The GDScript
## never sees that field at all — the native decoder drops the id out of its cached `populations`
## array and republishes the array whole (`SectionCache::patch`), and every surface is supposed to
## rebuild off it. "Supposed to" was the entire state of the evidence: `decode_guard`'s delta
## assertions explicitly pin that a merged frame keeps the baseline's row COUNT ("a delta patches
## the world, it never shrinks it"), so before this guard the removal branch had **no fixture
## anywhere** and a `patch` that simply ignored `removed` would have passed every gate in the repo.
##
## ## What it asserts, and why it is a whole harness rather than a preview state
##
## The claim is *wire → panel*, so the run starts at real FlatBuffers bytes and ends at live nodes:
##
##   baseline envelope → `SnapshotLoader.poll_stream` → **arrival delta** (appends a player band and
##   its detached hunting party) → **removal delta** (names the party in `removedPopulations`)
##
## with the real `SnapshotDecoder` in the middle and a real `HudLayer` + `BandCityPanel` + `MapView`
## at the end, fanned out the way `Main._apply_snapshot` fans out (see `_apply_frame`). Nothing here
## edits `HudBandLaborState`: the only thing that ever removes the party is the wire.
##
## Four properties, each of which a ghost row would fail on its own terms:
##
##   1. the party leaves `HudBandLaborState`'s own party grouping — `band_parties` / the expedition
##      roster, not merely one rendered list;
##   2. the parties zone's header count follows it (`Parties` · `n out · m workers`);
##   3. everything else keyed on that party clears with it — the **map marker** and the parties
##      **inspector strip**, which is opened here through the real row press before the removal (a
##      strip left standing on a despawned party is the same ghost one level down);
##   4. a party that was the SELECTED subject does not strand the drawer on itself.
##
## Every one of them is guarded by a **precondition** asserted on the arrival frame, because each
## reads as "absent" both when the removal worked and when the party never arrived at all.
##
## Fixtures: `cargo xtask decode-fixture` (`snapshot_party_delta_envelope.bin` /
## `snapshot_party_removal_delta_envelope.bin`; the ids and shape are stated in
## `xtask/src/decode_fixture.rs` under "The PARTY-REMOVAL pair").
##
## Run as a scene (NOT --script: the HUD reaches project autoloads). No GPU needed — nothing here is
## judged on pixels:
##   godot --headless --path . res://tools/party_removal_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const SnapshotLoaderScript := preload("res://src/scripts/SnapshotLoader.gd")
const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")

const SNAPSHOT_FIXTURE := "res://tests/fixtures/snapshot_envelope.bin"
const PARTY_ARRIVAL_FIXTURE := "res://tests/fixtures/snapshot_party_delta_envelope.bin"
const PARTY_REMOVAL_FIXTURE := "res://tests/fixtures/snapshot_party_removal_delta_envelope.bin"

## Scratch prefs, never the player's — the `band_panel_preview` / `command_guard` rule. The dock file
## is the load-bearing one: this guard SELECTS the parties tab, and without the isolation it would
## both read whichever tab a previous session left behind and write its own choice back over it.
const GUARD_PREFS_PATH := "user://party_removal_guard_prefs.cfg"
const GUARD_DOCK_PREFS_PATH := "user://party_removal_guard_dock.cfg"

## What `poll_stream` reads off the transport — `STATUS_CONNECTED` keeps it on the quiet path, the
## same choice `stream_frame_guard` makes and for the same reason (a guard that prints warnings
## trains people to ignore them).
const FAKE_STATUS := StreamPeerTCP.STATUS_CONNECTED

## The fixture's own handles, mirrored from `xtask/src/decode_fixture.rs`'s `PARTY_FIXTURE_*`
## constants. They are restated rather than discovered so a fixture that silently stopped carrying
## the party fails HERE, naming the entity it could not find, instead of quietly asserting about
## whatever cohort happened to be first.
const BAND_ENTITY := 9001
const PARTY_ENTITY := 9002
const PARTY_SIZE := 4
const PARTY_TILE := Vector2i(2, 1)
## The party's quarry id — its row and its inspector strip both render it (a hunt party's summary is
## `🏹 <quarry>…`), so it is the NEEDLE for "some control under the panel still names this party".
## Nothing else in the fixture world carries this string: the baseline's herds are saturated rows
## whose ids are path hashes.
const PARTY_QUARRY_NEEDLE := "game_boar_04"

## The frames each mutation is given to rebuild. Nothing renders, so this is layout settling only —
## the controls have to exist before one can be read back or pressed.
const SETTLE_FRAMES := 3

## A transport stub. `poll_stream` calls exactly these two methods on `loader.stream`.
class FakeStream:
	extends RefCounted
	var batches: Array = []

	func poll(_delta: float) -> Array:
		if batches.is_empty():
			return []
		return batches.pop_front()

	func status() -> int:
		return FAKE_STATUS

var _hud: Node = null
var _panel: Node = null
var _map: Node2D = null
var _loader: SnapshotLoader = null
var _failures: Array[String] = []

## The live nodes captured on the ARRIVAL frame, so the removal can be judged against the very
## controls the party had rather than against a fresh search that could pass on an empty panel.
var _party_row: Button = null
var _party_strip: Control = null

func _ready() -> void:
	NarrativeForkPanel.config_path_override = GUARD_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(GUARD_PREFS_PATH))
	BandCityPanel.config_path_override = GUARD_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(GUARD_DOCK_PREFS_PATH))

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	_hud.set_band_city_panel(_panel)
	_map = Node2D.new()
	_map.set_script(MAP_VIEW_SCRIPT)
	add_child(_map)
	# FoW OFF, once. `Main._sync_fog_of_war` owns this in the running client; the fixture world's
	# `fogEnabled` saturates to `true`, and a fogged own-faction party would make the marker
	# precondition fail for a reason that has nothing to do with removals.
	_map.set_fow_enabled(false)
	# The map's occupant selection re-enters the HUD through Main's two relays.
	_map.unit_selected.connect(_hud.show_unit_selection)
	_map.tile_selected.connect(_hud.show_tile_selection)
	await _settle()

	var frames := _decode_or_fail()
	if not _failures.is_empty():
		_finish()
		return

	# ---- frame 1: the baseline world -----------------------------------------------------------
	_apply_frame(frames[0])
	await _settle()

	# ---- frame 2: the party arrives ------------------------------------------------------------
	_apply_frame(frames[1])
	await _settle()
	await _assert_the_party_arrived(frames[1])
	if not _failures.is_empty():
		# Every assertion below reads "absent", which is what a party that never arrived also reads
		# like. Stop rather than report a green removal against a panel that never had the row.
		_finish()
		return

	# ---- frame 3: the wire retracts it ---------------------------------------------------------
	_apply_frame(frames[2])
	await _settle()
	_assert_the_wire_carried_the_removal(frames[2])
	_assert_the_party_left_the_labor_model()
	_assert_the_header_count_followed()
	_assert_the_row_and_the_strip_are_gone()
	_assert_the_map_marker_is_gone()
	_assert_no_drawer_is_stranded()
	_finish()

# ---- The fan-out ---------------------------------------------------------------------------------

## One decoded frame, dispatched the way `Main._apply_snapshot` dispatches it.
##
## **A SUBSET, and a deliberately literal one.** Only the sections this claim travels through are
## here (the map render, the grid, the herd roster, the population roster, and the selection
## re-resolve that closes Main's fan-out), but each keeps Main's own `has()` + `SnapshotSections`
## pair. That pair is not decoration: a merged delta republishes every key, so `has()` alone stopped
## being a change signal when delta streaming landed, and a fan-out written here without the manifest
## check would exercise a dispatch rule the client does not use.
func _apply_frame(frame: Dictionary) -> void:
	_map.display_snapshot(frame)
	if frame.has("grid"):
		_hud.set_grid_dimensions(frame["grid"])
	if frame.has("herds") and SnapshotSections.changed(frame, "herds"):
		_hud.update_herds(frame["herds"])
	if frame.has("populations") and SnapshotSections.changed(frame, "populations"):
		_hud.update_band_alerts(frame["populations"])
	# Main's tail: re-resolve the standing selection against the freshly rebuilt markers, so a
	# selected subject that left the world is dropped rather than streamed from a stale dict.
	var payload: Dictionary = _map.refresh_selection_payload()
	_hud.reapply_selection(String(payload.get("kind", "none")), payload.get("data", {}))

## Decode the three fixtures through the REAL `SnapshotLoader`, in one chain on one decoder.
##
## Three separate polls rather than one batch: the arrival frame has to be RENDERED and interacted
## with (the strip opened, the party selected) before the removal is applied, which is the whole
## shape of the case.
func _decode_or_fail() -> Array:
	var loader: SnapshotLoader = SnapshotLoaderScript.new()
	var fake := FakeStream.new()
	fake.batches = [
		[_read_fixture(SNAPSHOT_FIXTURE)],
		[_read_fixture(PARTY_ARRIVAL_FIXTURE)],
		[_read_fixture(PARTY_REMOVAL_FIXTURE)],
	]
	loader.stream = fake
	loader.stream_enabled = true
	_loader = loader
	if not _failures.is_empty():
		return []
	var frames: Array = []
	for label in ["the baseline snapshot", "the party ARRIVAL delta", "the party REMOVAL delta"]:
		var polled: Array = loader.poll_stream(0.0)
		if polled.size() != 1:
			_fail("%s was not applied (the poll answered %d frame(s), expected 1) — a delta whose `baseFrameSeq` names a frame this client does not hold is DROPPED, so regenerate the pair with `cargo xtask decode-fixture`" % [label, polled.size()])
			return []
		frames.append(polled[0])
	return frames

# ---- The preconditions (the arrival frame) -------------------------------------------------------

## Everything the removal is judged against, asserted while the party is still here — and the strip
## and the selection are OPENED here too, because a removal cannot be shown to close them otherwise.
func _assert_the_party_arrived(frame: Dictionary) -> void:
	if not _populations_of(frame).has(PARTY_ENTITY):
		_fail("the ARRIVAL delta's merged `populations` does not carry entity %d — the fixture no longer stages a party, so every assertion after it would pass on an empty panel (`cargo xtask decode-fixture`)" % PARTY_ENTITY)
		return
	if _band_fixture().is_empty():
		_fail("the ARRIVAL delta staged no player-faction BAND (entity %d) — a party groups under its home band, so with no band there is no parties zone to judge" % BAND_ENTITY)
		return

	_panel.set_active_tab(&"parties")
	await _settle()

	var parties: Array = _hud._band_labor.band_parties(_band_fixture())
	if parties.size() != 1 or int((parties[0] as Dictionary).get("entity", -1)) != PARTY_ENTITY:
		_fail("`band_parties` answered %d row(s) for the home band on the arrival frame, expected exactly the one party (entity %d)" % [parties.size(), PARTY_ENTITY])
		return

	_party_row = _find_button_containing(_panel, PARTY_QUARRY_NEEDLE)
	if _party_row == null:
		_fail("the parties zone renders no row naming `%s` on the arrival frame — the panel never showed the party, so its disappearance later proves nothing" % PARTY_QUARRY_NEEDLE)
		return
	if not _panel_shows_header_count(1, PARTY_SIZE):
		_fail("the parties header does not read `%s` on the arrival frame — the count assertion after the removal would be comparing against a number the panel never printed" % (HudComposeVocab.PARTIES_HEADER_FORMAT % [1, PARTY_SIZE]))
		return

	# THE STRIP, opened through the REAL row press — the same `pressed` a player's click fires.
	_party_row.pressed.emit()
	await _settle()
	if String(_hud._bandpanel._party_open_key) != str(PARTY_ENTITY):
		_fail("pressing the party row did not open the parties inspector strip (`_party_open_key` is `%s`) — the strip's clearance cannot be asserted if it was never opened" % String(_hud._bandpanel._party_open_key))
		return
	# THE SELECTION, taken through the REAL map click on the party's own hex.
	_map.handle_hex_click(PARTY_TILE.x, PARTY_TILE.y, MOUSE_BUTTON_LEFT)
	await _settle()
	if _map.selected_unit_id != PARTY_ENTITY:
		_fail("clicking the party's hex (%d, %d) selected unit %d, not the party (%d) — the stranded-drawer assertion needs the party to BE the selected subject" % [PARTY_TILE.x, PARTY_TILE.y, _map.selected_unit_id, PARTY_ENTITY])
		return
	if int(_hud._selection.unit().get("entity", -1)) != PARTY_ENTITY:
		_fail("the HUD's selected subject is entity %d after clicking the party's hex, not the party (%d)" % [int(_hud._selection.unit().get("entity", -1)), PARTY_ENTITY])
		return
	if not _map_marker_entities().has(PARTY_ENTITY):
		_fail("`MapView` built no unit marker for the party on the arrival frame, so its removal from the map cannot be observed")
		return

	# **CAPTURE THE TWO CONTROLS LAST, once nothing else will rebuild them.** Every interaction above
	# re-renders the zone in place (`_toggle_parties_inspector` calls `rerender`), which FREES the
	# controls it was made on — so a reference taken earlier is already dangling by the time the
	# removal lands, and asking whether it left the tree would answer "yes" whatever the removal did.
	# Captured here and asserted live, the same question is a real one.
	_party_row = _find_button_containing(_panel, PARTY_QUARRY_NEEDLE)
	_party_strip = _find_control_containing(_panel, PARTY_QUARRY_NEEDLE, _party_row)
	if not _still_in_tree(_party_row) or not _still_in_tree(_party_strip):
		_fail("the party's row and its inspector strip are not both live in the panel at the moment the removal is applied — the clearance assertions would be vacuous")

# ---- The assertions (the removal frame) ----------------------------------------------------------

## The wire half, and the only assertion here that can see the DECODER. The merged array must have
## shrunk by exactly the party while still carrying the home band — a decoder that dropped the whole
## section, or the whole faction, would satisfy every panel assertion below for the wrong reason.
func _assert_the_wire_carried_the_removal(frame: Dictionary) -> void:
	var entities := _populations_of(frame)
	if entities.has(PARTY_ENTITY):
		_fail("the REMOVAL delta's merged `populations` STILL carries entity %d — the wire named it in `removedPopulations` and the decoder republished the array with it standing (`SectionCache::patch`'s removal branch)" % PARTY_ENTITY)
	if not entities.has(BAND_ENTITY):
		_fail("the REMOVAL delta's merged `populations` lost the home band (entity %d) too — a removal must drop the named row and nothing else" % BAND_ENTITY)
	var removed: Variant = frame.get("population_removed", null)
	if removed == null or not Array(removed).has(PARTY_ENTITY):
		_fail("the merged frame does not publish `population_removed` naming %d — the sparse removal list rides the frame beside the patched array, and a consumer reading it would see nothing" % PARTY_ENTITY)

## Claim 1. Asked of the MODEL, not of a rendered list: the panel could be re-rendered from a stale
## grouping and look right on this turn while every other reader of `player_expeditions` (the
## Workforce bar's Parties segment, the attention producers) still counts the ghost.
func _assert_the_party_left_the_labor_model() -> void:
	var band := _band_fixture()
	if band.is_empty():
		_fail("the home band (entity %d) is gone from the labor model after the removal — the removal delta names only the party" % BAND_ENTITY)
		return
	var parties: Array = _hud._band_labor.band_parties(band)
	if not parties.is_empty():
		_fail("`band_parties` still groups %d row(s) under the home band after the removal — the ghost row the recall could never clear" % parties.size())
	for exp_variant in _hud._band_labor.player_expeditions():
		if exp_variant is Dictionary and int((exp_variant as Dictionary).get("entity", -1)) == PARTY_ENTITY:
			_fail("`player_expeditions` still holds entity %d after the removal" % PARTY_ENTITY)
	if int(_hud._band_labor.band_party_workers(band)) != 0:
		_fail("the band still reports %d worker(s) out with parties after the removal — the Workforce bar's Parties segment reads this" % int(_hud._band_labor.band_party_workers(band)))

## Claim 2. The header is a rendered string, so it is read off the panel's own Labels rather than
## recomputed — that is the difference between "the model agrees" and "the player is told".
func _assert_the_header_count_followed() -> void:
	if _panel_shows_header_count(1, PARTY_SIZE):
		_fail("the parties header still reads `%s` after the removal" % (HudComposeVocab.PARTIES_HEADER_FORMAT % [1, PARTY_SIZE]))
	if not _panel_shows_header_count(0, 0):
		_fail("the parties header does not read `%s` after the removal — the count and the worker total must both fall to nothing" % (HudComposeVocab.PARTIES_HEADER_FORMAT % [0, 0]))

## Claim 3a. Both the row and the inspector strip, and in BOTH directions: the controls the party
## had must be gone from the tree, AND no freshly-built control may name it either — a rerender that
## rebuilt the same row would free the captured nodes and satisfy the first half alone.
func _assert_the_row_and_the_strip_are_gone() -> void:
	if _still_in_tree(_party_row):
		_fail("the parties zone's row for the party is still in the tree after the removal")
	if _still_in_tree(_party_strip):
		_fail("the parties INSPECTOR STRIP is still in the tree after the removal — a strip pinned to a despawned party is the same ghost one level down")
	var survivor := _find_control_containing(_panel, PARTY_QUARRY_NEEDLE, null)
	if survivor != null:
		_fail("a `%s` under the panel still names `%s` after the removal" % [survivor.get_class(), PARTY_QUARRY_NEEDLE])
	if String(_hud._bandpanel._party_open_key) != "":
		_fail("`_party_open_key` is still `%s` after the removal — the stale key re-opens the strip on the next thing that happens to share the id" % String(_hud._bandpanel._party_open_key))

## Claim 3b. The map marker.
func _assert_the_map_marker_is_gone() -> void:
	if _map_marker_entities().has(PARTY_ENTITY):
		_fail("`MapView` still holds a unit marker for entity %d after the removal" % PARTY_ENTITY)

## Claim 4. The selected subject. `refresh_selection_payload` is supposed to notice the selected unit
## has left the world, clear its id and fall through to the hex — so the drawer shows the LAND the
## party stood on, never the party.
func _assert_no_drawer_is_stranded() -> void:
	if _map.selected_unit_id == PARTY_ENTITY:
		_fail("`MapView.selected_unit_id` is still the removed party (%d) — every later snapshot re-resolves the selection against it" % PARTY_ENTITY)
	if int(_hud._selection.unit().get("entity", -1)) == PARTY_ENTITY:
		_fail("the HUD's subject drawer is still standing on the removed party (%d)" % PARTY_ENTITY)
	for row_variant in _hud._selection.roster_units():
		if row_variant is Dictionary and int((row_variant as Dictionary).get("entity", -1)) == PARTY_ENTITY:
			_fail("the Occupants roster still lists the removed party (%d)" % PARTY_ENTITY)

# ---- Readers ------------------------------------------------------------------------------------

## Every `entity` in a frame's merged `populations` array.
func _populations_of(frame: Dictionary) -> Array:
	var ids: Array = []
	var rows_variant: Variant = frame.get("populations", [])
	if not (rows_variant is Array):
		return ids
	for row_variant in rows_variant:
		if row_variant is Dictionary:
			ids.append(int((row_variant as Dictionary).get("entity", -1)))
	return ids

## The home band as the labor model currently holds it — `{}` once it is gone.
func _band_fixture() -> Dictionary:
	for band_variant in _hud._band_labor.player_bands():
		if band_variant is Dictionary and int((band_variant as Dictionary).get("entity", -1)) == BAND_ENTITY:
			return band_variant
	return {}

func _map_marker_entities() -> Array:
	var ids: Array = []
	for unit_variant in _map.units:
		if unit_variant is Dictionary:
			ids.append(int((unit_variant as Dictionary).get("entity", -1)))
	return ids

## Does any Label under the panel print the parties header's `n out · m workers` clause for this
## pair? Read off the rendered text, never recomputed.
func _panel_shows_header_count(parties: int, workers: int) -> bool:
	return _find_control_containing(_panel, HudComposeVocab.PARTIES_HEADER_FORMAT % [parties, workers], null) != null

## The first `Button` under `root` whose face contains `needle`.
func _find_button_containing(root: Node, needle: String) -> Button:
	if root is Button and needle in (root as Button).text:
		return root
	for child in root.get_children():
		var found := _find_button_containing(child, needle)
		if found != null:
			return found
	return null

## The first `Control` under `root` (skipping `except` and its subtree) whose own `text` contains
## `needle`. Covers `Button` and `Label` — the two the parties zone renders its row and its strip
## title with — and deliberately does NOT read `RichTextLabel`, which prints no plain `text`.
##
## `except` is untyped for the same reason `_still_in_tree`'s argument is: a caller may legitimately
## hand over a node the panel has since freed, and a `Node`-typed parameter turns that into a script
## ERROR rather than an answer.
func _find_control_containing(root: Node, needle: String, except: Variant) -> Control:
	if except != null and is_instance_valid(except) and root == except:
		return null
	if (root is Label and needle in (root as Label).text) \
			or (root is Button and needle in (root as Button).text):
		return root
	for child in root.get_children():
		var found := _find_control_containing(child, needle, except)
		if found != null:
			return found
	return null

## Is a node captured earlier STILL a live part of the panel? A freed node answers false through
## `is_instance_valid`, and a node merely detached (a zone rebuilt into a new column) answers false
## through `is_inside_tree` — both are "gone" for this claim, and neither alone covers the other.
##
## **Untyped, deliberately.** Answering this question about a node the panel has already freed is the
## whole point, and a `Node`-typed parameter refuses a freed reference with a script error instead.
func _still_in_tree(node: Variant) -> bool:
	return node != null and is_instance_valid(node) and (node as Node).is_inside_tree()

# ---- Plumbing -----------------------------------------------------------------------------------

func _read_fixture(path: String) -> PackedByteArray:
	if not FileAccess.file_exists(path):
		_fail("no fixture at %s — generate it with: cargo xtask decode-fixture" % path)
		return PackedByteArray()
	var bytes := FileAccess.get_file_as_bytes(path)
	if bytes.is_empty():
		_fail("fixture at %s is empty" % path)
	return bytes

func _settle() -> void:
	for _i in SETTLE_FRAMES:
		await get_tree().process_frame

func _fail(message: String) -> void:
	_failures.append(message)
	printerr("party_removal_guard: FAIL — %s" % message)

func _finish() -> void:
	if _failures.is_empty():
		print("party_removal_guard: PASS — the wire's removal reached the roster, the header, the strip, the marker and the selection")
		get_tree().quit(0)
		return
	printerr("party_removal_guard: FAILED with %d problem(s)" % _failures.size())
	get_tree().quit(1)
