extends RefCounted

## The Options pane's INTERFACE SCALE, at both ends of its range.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## It is LAST, after `button_faces` and before the harness's icon-probe epilogue, and that placement
## is deliberate: `content_scale_factor` is WINDOW state, not scene state, so a leaked scale would
## silently re-project every frame captured after it with no error and no failed assertion. Running
## last bounds the blast radius, and the chapter restores `ClientSettings.UI_SCALE_DEFAULT` before it
## returns so even the epilogue's `food_icons` frame is untouched.
##
## The claim has two halves and needs both. The PNGs carry the one only an eye can make — nothing
## clips, overflows or overlaps at either extreme, on a canvas the squeeze is real on (the harness
## pins 1500x900, so `ui_scale` 1.5 leaves a 1000x600 logical viewport). The assertions carry the one
## no picture can: that the MAP is genuinely immune rather than merely plausible-looking, which is the
## half the deleted `content_scale_factor` widget got wrong (`.claude/rules/client/interface-scale.md`).

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The map behind these frames. A SMALL grid on purpose: MapView is cover-fit, so few hexes means big
## ones, and "the map held still" is then a judgement the eye can actually make between two PNGs.
const SCALE_GRID_W := 24
const SCALE_GRID_H := 16

## Prairie — one flat, evenly-lit biome across the whole grid, so nothing in the frame competes with
## the hex geometry the comparison is about.
const SCALE_TERRAIN_ID := 11

## How far a length may drift and still count as unchanged. A pixel, because the quantities compared
## are viewport extents and hex radii in a float pipeline, not integers.
const SCALE_EPSILON_PX := 1.0

## The map under test, alive for the length of this chapter only.
var _map: Node2D = null

## The map's own measurements at `ui_scale` 1.0 — the reference every scaled state is judged against.
var _base_screen_local := Vector2.ZERO
var _base_hex_radius := 0.0
var _base_origin := Vector2.ZERO
var _base_viewport := Vector2.ZERO


func run(harness) -> void:
	h = harness

	# A REAL MapView, visible, behind a dense HUD state — the two things the scale has to do opposite
	# things to. Every other MapView this harness instances is `visible = false` (data only); this one
	# must draw, because the whole question is whether the world under the chrome moved.
	_map = h.MAP_VIEW_SCRIPT.new()
	h.add_child(_map)
	# FoW OFF, stated rather than inherited (the rule in `fog-of-war.md`): `_fow_enabled` fails closed
	# at `true`, and a fully fogged frame would compare identically at every scale — deterministic
	# because its subject disappeared, which is worse than a frame that varies.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_scale_map_snapshot())
	h._hud.show_unit_selection(BandFx.band_fixture())

	await _apply_scale(ClientSettings.UI_SCALE_DEFAULT)
	_base_screen_local = _map.screen_size_local()
	_base_hex_radius = _map.last_hex_radius
	_base_origin = _map.last_origin
	_base_viewport = h.get_viewport().get_visible_rect().size
	# The precondition for everything below: a map that never laid out has a zero radius, and every
	# "unchanged" assertion would then hold vacuously.
	h._assert_hud("interface scale: the reference map laid out (hex radius %.2f, screen %s local px)"
		% [_base_hex_radius, str(_base_screen_local)],
		_base_hex_radius > 0.0 and _base_screen_local.x > 0.0)
	await h._save("interface_scale_default")

	await _apply_scale(ClientSettings.UI_SCALE_MIN)
	_assert_scale_holds(ClientSettings.UI_SCALE_MIN)
	await h._save("interface_scale_small")

	await _apply_scale(ClientSettings.UI_SCALE_MAX)
	_assert_scale_holds(ClientSettings.UI_SCALE_MAX)
	await h._save("interface_scale_large")

	# RESTORE, and assert the restore landed — a leaked scale corrupts every later frame silently, so
	# "we set it back" is exactly the kind of claim that must not be taken on trust.
	await _apply_scale(ClientSettings.UI_SCALE_DEFAULT)
	h._assert_hud("interface scale: the run is left at 1.0, so no later frame inherits a scale",
		is_equal_approx(h.get_window().content_scale_factor, float(ClientSettings.UI_SCALE_DEFAULT)))
	_map.queue_free()
	_map = null


## Push a scale the way the Options slider does — through `ClientSettings.changed`, so `UiScaler` and
## `MapView` each react through their own real subscription and nothing here reaches the window
## directly. **The MEMBER is assigned, never `set_ui_scale`**: the setter `_save()`s, and this harness
## has no `ClientSettings` config-path override, so it would write the developer's own
## `user://client_settings.cfg` (the determinism bug `map_preview` records for the speed sliders).
func _apply_scale(value: float) -> void:
	ClientSettings.ui_scale = value
	ClientSettings.changed.emit()
	await h._settle()


## The two claims a PNG cannot make, asked of the live nodes at `scale`.
##
## `content_scale_factor` proves `UiScaler` is actually wired — without it every frame here renders
## identically and the whole chapter passes while testing nothing. The viewport ratio proves the
## engine did what the design rests on: the logical rect shrinks by exactly `s`. The three map
## quantities are the immunity claim — the screen measured in MAP-LOCAL units, and the radius and
## origin that between them decide where every hex lands and how big it is drawn.
func _assert_scale_holds(scale: float) -> void:
	h._assert_hud("interface scale %.2f: UiScaler pushed it onto the window" % scale,
		is_equal_approx(h.get_window().content_scale_factor, scale))
	var want_viewport: Vector2 = _base_viewport / scale
	var got_viewport: Vector2 = h.get_viewport().get_visible_rect().size
	h._assert_hud("interface scale %.2f: the logical viewport is %s, not %s"
		% [scale, str(want_viewport.round()), str(_base_viewport.round())],
		got_viewport.distance_to(want_viewport) <= SCALE_EPSILON_PX)
	h._assert_hud("interface scale %.2f: the map's own screen extent is unchanged (%s local px)"
		% [scale, str(_map.screen_size_local())],
		_map.screen_size_local().distance_to(_base_screen_local) <= SCALE_EPSILON_PX)
	h._assert_hud("interface scale %.2f: the hexes are the same size (radius %.2f)"
		% [scale, _map.last_hex_radius],
		absf(_map.last_hex_radius - _base_hex_radius) <= SCALE_EPSILON_PX)
	h._assert_hud("interface scale %.2f: the map sits in the same place (origin %s)"
		% [scale, str(_map.last_origin)],
		_map.last_origin.distance_to(_base_origin) <= SCALE_EPSILON_PX)


## Terrain and nothing else: no bands, no herds, no sites. The frame is judged on hex geometry, and
## every marker in it would be one more thing to explain away when two frames differ.
func _scale_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(SCALE_GRID_W * SCALE_GRID_H)
	terrain.fill(SCALE_TERRAIN_ID)
	return {
		"grid": {"width": SCALE_GRID_W, "height": SCALE_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [],
		"herds": [],
	}
