extends RefCounted

## Dock reservations, the terrain legend and the narrative fork.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# Slice 1 reserved-dock probe: left-edge reservation width used to verify the HUD insets.
const RESERVED_PROBE_WIDTH := 300.0

# The Telling fixture's two authored voice registers. Named here ONLY so the harness can pin the
# preference deterministically — nothing in the client hardcodes a register (VoiceLine.register is
# free-form by design; the panel builds its toggle from what the fork actually carries).
const FORK_REGISTER_MYTHIC := "mythic"

const FORK_REGISTER_WARM := "warm"

## The Telling: a pending fork on the wire, in the per-faction shape the native decoder produces
## (`[{faction, forks: [...]}]`). Copy is verbatim from beat_definitions.json —
## `sedentarization.soft_drift` / `soft_drift.long_chase` — with `{beast.plural}` resolved the way
## the sim resolves nouns at post time, so the frame judges REAL prose at REAL length.
func _pending_forks_fixture() -> Array:
	return [{
		"faction": 0,
		"forks": [{
			"beat_id": "sedentarization.soft_drift",
			"wardrobe_id": "soft_drift.long_chase",
			"posted_tick": 41,
			"narration": [
				{"register": FORK_REGISTER_MYTHIC, "text": "Three seasons, and each one we chased the mammoths and left the seed-ground unturned. The children do not remember a walled night. At the fires, they have begun to call us the People of the Long Chase. Is that who we are?"},
				{"register": FORK_REGISTER_WARM, "text": "Three seasons now, all of them spent following the mammoths, and nobody's turned the seed-ground once. The children have never slept behind a wall. People have started calling us the People of the Long Chase. Is that us?"},
			],
			"choices": [
				{"choice_id": "yes_trail", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We are the trail"},
					{"register": FORK_REGISTER_WARM, "text": "Yes — we're trail people"},
				]},
				{"choice_id": "no_root", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We were meant to root"},
					{"register": FORK_REGISTER_WARM, "text": "No — we were meant to settle"},
				]},
				# Exactly one choice carries is_defer, and the SERVER computes it — the client reads
				# the flag and never re-derives which choice writes nothing.
				{"choice_id": "defer", "is_defer": true, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "Say nothing"},
					{"register": FORK_REGISTER_WARM, "text": "Let it lie for now"},
				]},
			],
			"gloss": [
				{"signal": "sedentarization.score", "value": 41.0},
				{"signal": "stance.roam_settle", "value": -0.18},
			],
		}],
	}]

func _stance_axes_fixture() -> Array:
	return [{"faction": 0, "axes": [{"axis": "roam_settle", "value": -0.18}]}]

func run(harness) -> void:
	h = harness

	# State 8 — reserved-space docking (Slice 1 refactor): a left-edge reservation of
	# RESERVED_PROBE_WIDTH px insets the whole HUD (LayoutRoot.offset_left), so the top/bottom
	# bars start that much further right — mirroring how the docked Inspector shrinks the play
	# space. Save the inset frame, then release it (size 0) and save the restored frame.
	h._hud.clear_selection()
	h._hud.set_reserved_inset(&"inspector", SIDE_LEFT, RESERVED_PROBE_WIDTH)
	await h._settle()
	await h._save("reserved_dock")
	h._hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	await h._settle()
	await h._save("reserved_dock_cleared")

	# Terrain-legend sort control (base terrain legend, key == "terrain"). Several
	# biomes of varying tile counts so the default count-desc order + the Name/Count
	# sort toggles + sort persistence across a regen push are all visible. Rendered
	# before the full-screen icon probe below so the right-dock legend isn't covered.
	# Opened here and closed at the end of THIS block (not hundreds of lines later).
	h._open_legend()
	h._hud.update_overlay_legend(TileFx.terrain_legend_fixture())
	await h._settle()
	await h._save("terrain_legend_count_desc")  # default: Count, high→low

	# Click "Name" → alphabetical A→Z.
	h._hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await h._settle()
	await h._save("terrain_legend_name_asc")

	# Click "Name" again → Z→A.
	h._hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await h._settle()
	await h._save("terrain_legend_name_desc")

	# Click "Count" → back to count, and again → low→high.
	h._hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	h._hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	await h._settle()
	await h._save("terrain_legend_count_asc")

	# Simulate a map regen (fresh terrain-legend push): the chosen sort (count asc)
	# must persist, not snap back to the default.
	h._hud.update_overlay_legend(TileFx.terrain_legend_fixture())
	await h._settle()
	await h._save("terrain_legend_persist")
	h._close_legend()

	# ---- The Telling (docs/plan_the_telling.md) -----------------------------------------------
	# The narrative fork decision surface + the client-side end-turn gate. The fixture is the REAL
	# authored copy from core_sim/src/data/beat_definitions.json (`sedentarization.soft_drift`, the
	# `soft_drift.long_chase` wardrobe entry, nouns resolved as the sim resolves them at post time),
	# so the frame shows prose at real length rather than lorem that flatters the layout.
	h._hud.clear_selection()
	h._hud.update_overlay(41, {})
	# Pin the register so the run is deterministic (the preference persists in user://).
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)

	# State F1 — the panel, auto-opened the first time the fork appears: the narration as the hero
	# element, three choices in catalog order (the defer choice styled `ghost`, and ALWAYS enabled —
	# it is the out the gate depends on), the gloss collapsed, the voice toggle in the footer.
	h._hud.update_pending_forks(_pending_forks_fixture())
	h._hud.update_stance_axes(_stance_axes_fixture())
	await h._settle()
	await h._save("narrative_fork_panel")

	# State F2 — the SAME fork in the other register. Verifies the toggle and that the noticeably
	# shorter/looser `warm` copy lays out as well as the long `mythic` one. The registers come from
	# the fork itself, never a hardcoded list.
	h._hud._turnorb._fork_panel._on_register_picked(FORK_REGISTER_WARM)
	await h._settle()
	await h._save("narrative_fork_panel_warm")

	# State F3 — THE GATE, and the single most important assertion in this file. With a blocking
	# fork seeded, an orb-face click must NOT advance the turn (it opens the reasons popover
	# instead), and the popover's Advance button must be DISABLED and wear the reason. This is the
	# exact inverse of `turn_orb_clear_click_advances`.
	h._hud._turnorb._fork_panel.close()
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)
	var fork_advance_hits := [0]
	var fork_advance_cb := func() -> void: fork_advance_hits[0] += 1
	h._hud.turn_orb.advance_requested.connect(fork_advance_cb)
	h._hud.turn_orb._on_face_pressed()
	await h._settle()
	var fork_footer: Button = h._turn_orb_advance_button()
	h._assert_turn_orb("blocking fork: face click does not advance",
		fork_advance_hits[0] == 0 and h._hud.turn_orb._popover_open)
	h._assert_turn_orb("blocking fork: Advance is disabled",
		fork_footer != null and fork_footer.disabled)
	await h._save("turn_orb_fork_blocks")
	h._hud.turn_orb.advance_requested.disconnect(fork_advance_cb)
	h._hud.turn_orb.toggle_popover()

	# (The old State F4 `narrative_feed` — narrative prose styled INSIDE the command feed — was
	# retired with PR-C. The feed no longer renders narrative kinds at all, so the state could only
	# ever have shown their absence; `telling_and_feed` below is its replacement and tests the
	# thing that now matters: that the receipts survive alongside real narrative volume.)

	# ---- The Telling panel (PR-C) ------------------------------------------------------------
	# States G1–G3. The dock is cleared first so the two narrative cards are judged on their own
	# chrome rather than on whatever the previous state left selected.
	# `clear_selection()` deliberately KEEPS the tile card (deselecting an occupant should not
	# forget the hex), so the tile info has to go first or the Tile card fills the dock and both
	# narrative cards get squeezed out of the frame entirely.
	h._hud._selection._selected_tile_info.clear()
	h._hud.clear_selection()
	h._hud._telling.reset()
