extends RefCounted

## Fog of war: what a remembered or unexplored hex may state.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 14

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The three fog-of-war states MapView tags onto tile_info (mirrors Hud.VISIBILITY_*).
const VIS_UNEXPLORED := "unexplored"

## YOUR OWN scouting expedition standing on an UNEXPLORED hex — the case the fog rule must NOT break.
## The tile carries the party AND a herd; the herd is redacted (nobody can see it), but the party stays.
func _own_expedition_unexplored_tile() -> Dictionary:
	var tile := TileFx.sight_tile_fixture(VIS_UNEXPLORED)
	tile["units"] = [BandFx.expedition_fixture()]
	tile["unit_count"] = 1
	return tile

## A FOREIGN band (faction 1) on a hex in the given sight state. On an unseen hex it must vanish from
## the roster (it is not ours); on a visible hex it lists normally with a neutral dot.
func _foreign_band_tile(visibility_state: String) -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["units"] = [{
		"id": "Rival Band",
		"entity": 6001,
		"faction": 1,
		"size": 63,
		"pos": [66, 10],
		"activity": "forage",
	}]
	tile["unit_count"] = 1
	return tile

func run(harness) -> void:
	h = harness

	# States 2-fog-a/b/c — the three SIGHT states. The player must always be able to tell "there is
	# nothing here" apart from "I can't see what's here", so the Tile card leads with a `Sight:` row and
	# an unseen hex REPLACES its Occupants roster with a statement instead of rendering an empty one.
	#   2-fog-a  Active      — `Sight: In sight` (cyan), full live card (the `food_tile` state,
	#                          `chapters/land_readouts.gd`).
	#   2-fog-b  Discovered  — a remembered hex that DOES carry a herd: the herd must NOT be listed and
	#                          the Occupants card must read "out of sight · …bands and herds move".
	#                          (MapView fog-gates herds out of tile_info at source; the HUD re-reads the
	#                          same visibility_state flag, so it's honest even fed a leaky dict — which
	#                          is exactly what this fixture is.)
	#   2-fog-c  Unexplored  — never seen: `Sight: Unexplored` + "Nobody has been here."
	h._show_tile(TileFx.sight_tile_fixture(TileFx.VIS_ACTIVE))
	await h._settle()
	await h._save("tile_sight_active")

	h._hud.clear_selection()
	h._show_tile(TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED))
	await h._settle()
	await h._save("tile_sight_remembered")

	h._hud.clear_selection()
	h._show_tile(TileFx.sight_tile_fixture(VIS_UNEXPLORED))
	await h._settle()
	await h._save("tile_sight_unexplored")
	h._hud.clear_selection()

	# States 2-fog-d/e/f — the UNIT half of the fog rule:
	#     hidden == tile not visible AND unit is not ours.
	#   2-fog-d  YOUR OWN expedition on an UNEXPLORED hex → STILL listed and selectable. This is the
	#            regression guard for the load-bearing exception: the sim excludes expeditions from fog
	#            reveal (discovery is comm-range gated), so your own party ROUTINELY stands on an
	#            Unexplored tile — a plain visibility gate would delete it from the map/roster exactly
	#            while you're using it. The roster also warns that you still can't see anything ELSE there.
	#   2-fog-e  A FOREIGN band on a fogged (Remembered) hex → NOT listed; Occupants reads out-of-sight.
	#   2-fog-f  The same foreign band on a VISIBLE hex → listed normally (neutral dot, no allocation).
	h._show_tile(_own_expedition_unexplored_tile())
	await h._settle()
	await h._save("tile_sight_own_expedition")

	# tile_panel_unexplored_own_band — THE SAME HEX with the LAND row lit instead of the auto-picked
	# party, which is where the two FoW rules collide. UNEXPLORED ground yields NO terrain rows at all,
	# so `_render_land_drawer` hides `%TileDetail` and the forage/herd/allocation blocks with it — and
	# the unknown-contents note suppresses itself whenever the roster is non-empty, which on THIS hex
	# it is, because the sim excludes expeditions from fog reveal and your own party stands here. Every
	# child of the drawer hidden at once is a blank capped area under the divider where the land's
	# whole content belongs, and a PNG cannot tell that apart from a drawer that rendered fine — so the
	# claim is asserted on the CONTROL, driven through the real land-row handler.
	h._hud._selectioncard._on_land_row_selected()
	await h._settle()
	# THE PRECONDITIONS, without which the two assertions below pass on a hex that never reached the
	# state: no terrain rows to fall back on, AND a roster that would suppress the note on its own.
	h._assert_hud("precondition: unexplored ground gives the LAND drawer no terrain rows to show",
		not h._hud.tile_detail.visible)
	h._assert_hud("precondition: your own party still lists here, so the note's roster skip is armed",
		not h._hud._selection.roster_units().is_empty())
	h._assert_hud("the LAND drawer on an UNEXPLORED hex holding your own party is not blank",
		h._hud.occupant_detail.visible and h._hud.occupant_detail.text.strip_edges() != "")
	# A needle no other copy on this card can satisfy: the roster's own out-of-sight hint and the
	# remembered note are DIFFERENT sentences, so matching this one cannot be borrowing either.
	h._assert_hud("…stating the UNEXPLORED unknown-contents sentence, and not the remembered one",
		h._hud.occupant_detail.text.contains(HudConst.OCCUPANTS_UNKNOWN_UNEXPLORED)
			and not h._hud.occupant_detail.text.contains(HudConst.OCCUPANTS_UNKNOWN_REMEMBERED))
	await h._save("tile_panel_unexplored_own_band")

	h._hud.clear_selection()
	h._show_tile(_foreign_band_tile(TileFx.VIS_DISCOVERED))
	await h._settle()
	await h._save("tile_sight_foreign_hidden")

	h._hud.clear_selection()
	h._show_tile(_foreign_band_tile(TileFx.VIS_ACTIVE))
	await h._settle()
	await h._save("tile_sight_foreign_visible")
	h._hud.clear_selection()

	# State 2b — the same food tile, single FAR band (~21 tiles away, beyond work_range 2): foraging is
	# stationary gathering with NO expedition fallback, so the Forage button is DISABLED and an
	# out-of-range hint shows ("(66,10) is 21 tiles away — beyond this band's forage range (2)").
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[1]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("food_forage_out_of_range")

	# State 2c — TWO bands at DIFFERENT distances from ONE food tile, NEAR band selected (821, 1 tile
	# away ≤ range 2): enabled **Forage**. The band-picker selection — not the tile — drives it.
	h._hud._band_labor._player_bands = BandFx.forage_range_bands()
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("food_forage_band_near")

	# State 2d — same two bands, FAR band selected via the picker (822, ~21 tiles away): the SAME tile
	# now DISABLES Forage + shows the out-of-range hint, proving WHICH band is selected drives the
	# enabled-vs-disabled state (the case single-band playtest can't cover).
	h._hud._compose.set_forage_band(int(BandFx.forage_range_bands()[1]["entity"]))
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("food_forage_band_far")
	# Reset so later states resolve their usual band.
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)

	# `band_alerts` (`chapters/band_expedition.gd`) overwrote _player_band with alert-fixture bands
	# (which carry no hunt_reach);
	# re-seed the reference band so the herd assign controls resolve a proper band with a hunt reach.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
