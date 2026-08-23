extends RefCounted

## The Telling book and its page turns.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 21

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# How far into the oral page-turn the live-arrival state drives its REAL tween before capturing, as a
# fraction of the panel's own duration — a chosen mid-motion phase in place of "however many frames
# the clock happened to give us". Must stay strictly inside (0, 1) to keep the tween RUNNING.
const TELLING_LIVE_TURN_FRACTION := 0.4

# The Telling panel's medium rungs. Named here only so the states read; the client keys its styling
# off a table with an `oral` fallback, never off these three being exhaustive.
const TELLING_MEDIUM_PAINTED := "painted"

const TELLING_MEDIUM_WRITTEN := "written"

## TWO beats sharing ONE tick — a single speaking turn that said two things, so they form ONE page.
## Reproduces the playtest bug (the fixed-height page scrolled the second beat off instead of growing).
func _telling_two_beat_oral_fixture() -> Array:
	return [
		{"tick": 6, "kind": "narrative_beat",
			"label": "We have stopped catching rabbits and started keeping them. A fence, a little grass, and they breed under our own eyes.",
			"detail": "husbandry.penning = 0.34"},
		{"tick": 6, "kind": "narrative_beat",
			"label": "We are more now than we were when we left the bone ground. The children born on this road have never slept anywhere else.",
			"detail": "band.count = 31"},
	]

## TWO tall pages (ticks 0 and 1, seven distinct long beats each) that BOTH overflow `PAGE_MAX_HEIGHT`, so
## the inner ScrollContainer actually scrolls — the fixture the yields-to-reader scroll test needs (a page
## that fits the cap can hold no non-zero scroll offset to preserve).
func _telling_tall_pages_fixture() -> Array:
	var out: Array = []
	var long := "The chase is longer every season and ends in less; the aurochs were the road we walked, and the road is going quiet under our own feet."
	for tick in [0, 1]:
		for i in range(7):
			out.append({"tick": tick, "kind": "narrative_beat",
				"label": "%d. %s" % [i, long], "detail": "beat %d of tick %d" % [i, tick]})
	return out

## Ordinary command receipts for the split frame — the transactional acknowledgements that used to
## be pushed off the feed by two beats. Deliberately MORE than one, so "the feed is legible again"
## is something the frame can actually show rather than imply.
func _telling_command_receipts() -> Array:
	return [
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "6 foragers → (27, 26)"},
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "3 hunters → Aurochs Herd"},
		{"tick": 23, "kind": "command", "label": "Move band", "detail": "Band 1 → (28, 25)"},
		{"tick": 23, "kind": "site_discovered", "label": "Salt Pillar Reach", "detail": "Wondrous site at (31, 22)"},
	]

## Advance every live tween by a FIXED slice — a deliberately chosen mid-motion phase, for the state
## that captures a page turn in flight. Deterministic because the clock contributes nothing.
func _step_tweens(seconds: float) -> void:
	for tween in h.get_tree().get_processed_tweens():
		if tween.is_valid() and tween.is_running():
			tween.custom_step(seconds)

func run(harness) -> void:
	h = harness

	# G1 — ORAL: the current utterance only. No page furniture, no leaf controls, no page number — oral
	# memory does not keep the previous telling, so the visible page is pinned to the NEWEST beat (the
	# fork at tick 22). Ingest the real authored copy (incl. the catalog's longest line, so a page's
	# wrap is genuinely exercised).
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud.ingest_command_events(ForageFx.telling_fixture_events())
	await h._settle()
	await h._save("telling_panel_oral")

	# G2 — PAINTED: the accumulating wall. The SAME entries, now retained as pages you can walk FORWARD
	# through (a marks + position cue, no back control). Parked mid-way (page 3/6) so the retained
	# earlier pages and the forward-only affordance read at once. `debug_jump_to` is the NON-animating
	# park — these SETTLED end-state frames must not catch a page-turn tween mid-flight (that's what the
	# `telling_turn_*_mid` states capture on purpose).
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	h._hud._telling.debug_jump_to(2)
	await h._settle()
	await h._save("telling_panel_painted")

	# G3 — WRITTEN: the full book. Page number + ‹ › leaf controls, parked on a NON-LAST page (3/6) so
	# backward leafing is visibly available (both ‹ and › active). Nothing about the copy changes
	# between the rungs (per-medium copy is a deliberate non-goal) — only the title, accent and
	# CAPABILITIES age, which is the whole point.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	h._hud._telling.debug_jump_to(2)
	await h._settle()
	await h._save("telling_panel_written")

	# G3b — UNREAD: the yields-to-reader rule. The reader is held on an OLD page (1/6) while newer pages
	# exist; the page never turns on its own, so a subtle "a new telling waits" cue appears instead of
	# yanking them forward. (Advancing the turn — reveal_newest() — is what catches them up.)
	h._hud._telling.debug_jump_to(0)
	await h._settle()
	await h._save("telling_panel_unread")

	# G4 — THE FRAME THAT PROVES THE SPLIT WORKED. The Telling panel holds its fixed page while a batch
	# of ordinary command receipts arrives: before the split, two beats filled the narrative card
	# outright and pushed every receipt off screen. The Telling must claim exactly its own kinds here
	# and nothing else — the receipts belong to the event dock. (Oral restored.)
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud.ingest_command_events(_telling_command_receipts())
	await h._settle()
	await h._save("telling_and_feed")

	# G5 — THE DEFAULT DOCK LAYOUT. The right dock holds the Telling panel ALONE: Victory and
	# Terrain Types both ship suppressed, so the narrative surface gets the full right-dock height
	# instead of the squeezed share it had while it lived under the left dock's selection cards.
	# The left dock is the selection card's alone now — the command feed that used to sit under it is
	# retired — which is the layout this frame exists to show.
	h._hud.update_victory_state(WorldFx.victory_state_fixture())
	await h._settle()
	await h._save("dock_default_layout")
	# The Telling panel is registered with `right_dock.add(..., 10)`, and `PanelDock._reorder`
	# reparents. Screenshotting the dock only shows it LOOKS right; assert WHERE it lives, so a
	# dropped/reordered registration (or a scene edit that re-authors it under the left dock)
	# fails here instead of silently reverting the narrative surface to the left column.
	h._assert_hud("default layout: Telling panel lives in the right dock stack",
		h._hud.telling_panel.get_parent() == h._hud.right_stack)

	# G6 — the same frame with the reference card toggled back on (the `V` path), so the right dock's
	# stacking order — Telling, then Victory — is visible and the Telling panel is seen to yield height
	# rather than overlap. It goes through the REAL `toggle_victory` (prefs write included — the
	# harness cleared the section at startup, and this toggles back below). The Terrain Types legend
	# that used to stack third here is retired with the `L` card.
	h._hud.toggle_victory()
	await h._settle()
	await h._save("dock_panels_revealed")
	h._assert_hud("toggled on: Victory panel is visible", h._hud.victory_panel.visible)
	# Restore the shipped default so any later state renders the real layout.
	h._hud.toggle_victory()

	# TWO-BEAT ORAL — a single speaking turn firing TWO beats (both sharing one tick, so they are ONE
	# page). The page must GROW to fit both beats + gloss with NO scrollbar — the playtest fix (the
	# strictly-fixed height scrolled the second beat out of view). Assert the inner scroll is not engaged.
	h._hud._telling.reset()
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud.ingest_command_events(_telling_two_beat_oral_fixture())
	await h._settle()
	h._assert_hud("two-beat oral page grows to fit both beats with no scrollbar",
		not h._hud._telling.debug_page_scrolls())
	await h._save("telling_panel_oral_two_beats")

	# SCROLL YIELDS-TO-READER — a beyond-cap (scrolling) page must NOT yank a mid-page reader to the top on
	# an IDEMPOTENT static repaint (a retaining-medium beat arrival that leaves the visible page unmoved),
	# but MUST start at the top on a real page turn. Two tall written pages that both overflow the cap.
	h._hud._telling.reset()
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	h._hud.ingest_command_events(_telling_tall_pages_fixture())
	h._hud._telling.debug_jump_to(0)
	await h._settle()
	var telling_scroll: ScrollContainer = h._hud._telling._scroll
	telling_scroll.scroll_vertical = 40   # the reader has scrolled down the tall page
	await h._settle()
	h._assert_hud("tall page overflows so the reader's scroll offset holds", telling_scroll.scroll_vertical == 40)
	# Idempotent repaint: a new beat arrives on a NEW tick, but written stays on page 0 (index clamped, the
	# visible page's text is unchanged) — the yields case. Must PRESERVE the reader's scroll position.
	h._hud.ingest_command_events([{"tick": 2, "kind": "narrative_beat", "label": "A far-off new telling waits.", "detail": "later"}])
	h._assert_hud("idempotent repaint of the same page preserves the reader's scroll position",
		telling_scroll.scroll_vertical == 40)
	# A real page turn resets the inner scroll to the top of the new page.
	h._hud._telling.leaf(1)
	h._assert_hud("a real page turn resets the inner scroll to the top", telling_scroll.scroll_vertical == 0)
	h._hud._telling.debug_end_turn()

	# LIVE-PATH ORAL ARRIVAL — the REAL trigger, no debug hook. Drive the actual per-snapshot Hud entry
	# points (`update_voice_medium` THEN `ingest_command_events`, plus the `_refit_right_dock` a real
	# snapshot fires) with a genuinely new beat, and PROVE a running tween is created AND survives to paint
	# frames (an idempotent re-render / refit in the same cycle must not `_kill_tween` it). This is the gap
	# the mid-transition freeze states could not cover: they show the tween CAN render, not that the live
	# beat-arrival path TRIGGERS one.
	h._hud._telling.reset()
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud.ingest_command_events([{"tick": 0, "kind": "narrative_beat",
		"label": "The scouts came back thinner and louder than they left, all of them saying one word: Salt Pillar Reach.",
		"detail": "sites.discovered_this_turn = 1"}])
	await h._settle()   # initial population — no animation by design
	# A new snapshot: medium re-pushed unchanged (must NOT clobber), then a genuinely new beat arrives.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud.ingest_command_events([{"tick": 5, "kind": "narrative_beat",
		"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
		"detail": "provisions.total falling for 3 turns"}])
	h._hud._refit_right_dock()   # a refit in the same cycle must not kill the in-flight turn tween
	h._assert_hud("live oral beat-arrival creates a running page-turn tween",
		h._hud._telling.debug_turn_active())
	# Advance the REAL tween to a CHOSEN mid-motion phase. Animation time is frozen (see `_ready`), so
	# awaiting frames here would advance it by exactly nothing and the state would capture the page
	# BEFORE the turn — the one frame in the run whose subject the freeze could have erased. One
	# `custom_step` of 40% of the oral dissolve keeps it genuinely in flight AND makes the phase a
	# decision instead of whatever the clock handed us (which is what made this frame drift).
	_step_tweens(TellingPanel.PAGE_TURN_DURATION_ORAL * TELLING_LIVE_TURN_FRACTION)
	h._assert_hud("live oral tween survives an in-cycle refit and is still running mid-motion",
		h._hud._telling.debug_turn_active())
	# The one `_settle` that must NOT flush tweens: this frame IS the mid-turn render.
	await h._settle(false)
	await h._save("telling_live_oral_arrival")
	h._hud._telling.debug_end_turn()   # settle deterministically before the next state

	# ---- Page-turn animation: motion matures with the medium (mid-transition capture) --------------
	# The harness dumps single frames, so each state DRIVES a page turn, then FREEZES the tween at its
	# midpoint (`debug_freeze_turn_at`) so the outgoing and incoming pages COEXIST in the captured PNG —
	# proof the motion is real. Setup jumps (`debug_jump_to`) are non-animating so the measured turn
	# starts from a clean resting page. The block ends with a clean static render, so the frozen overlay
	# never leaks into a later frame.
	h._hud._telling.reset()
	h._hud.ingest_command_events(ForageFx.telling_fixture_events())

	# WRITTEN — a horizontal SLIDE, forward: the outgoing page exits left as the incoming enters from the
	# right. Frozen mid-slide, both pages are onscreen offset horizontally, with the ‹ › book furniture.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	h._hud._telling.debug_jump_to(1)
	await h._settle()
	h._hud._telling.leaf(1)
	h._hud._telling.debug_freeze_turn_at(0.5)
	await h._settle()
	await h._save("telling_turn_written_mid")

	# PAINTED — the incoming page RISES from just below with a fade (new marks drifting onto the wall).
	# Frozen partway up, the incoming page sits low + faint over the fading outgoing one.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	h._hud._telling.debug_jump_to(1)
	await h._settle()
	h._hud._telling.leaf(1)
	h._hud._telling.debug_freeze_turn_at(0.45)
	await h._settle()
	await h._save("telling_turn_painted_mid")

	# ORAL — a CROSSFADE in place: a new recitation replacing the last (oral keeps no prior page). Frozen
	# at the crossover, both pages read at partial alpha in the same spot, with NO furniture.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud._telling.debug_jump_to(3)
	await h._settle()
	h._hud._telling.reveal_newest()
	h._hud._telling.debug_freeze_turn_at(0.5)
	await h._settle()
	await h._save("telling_turn_oral_mid")

	# INTERRUPTION — a rapid second turn must KILL the running tween and settle to the CORRECT final page,
	# with no leftover overlay/offset. Turn 0→1, immediately 1→2, then force the settle a completed tween
	# would reach, and assert the visible page is 2 with the overlay gone.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	h._hud._telling.debug_jump_to(0)
	await h._settle()
	h._hud._telling.leaf(1)          # 0 → 1 (tween begins)
	h._hud._telling.leaf(1)          # 1 → 2 immediately (must kill + restart)
	h._hud._telling.debug_end_turn() # force the settle
	await h._settle()
	h._assert_hud("interrupted page-turn settles to the final page with no leftover overlay",
		h._hud._telling.debug_visible_index() == 2 and not h._hud._telling.debug_overlay_visible())
	await h._save("telling_turn_interrupted")

	# Clean static state (newest oral page, no frozen overlay) before the downstream frames.
	h._hud.update_voice_medium([{"faction": 0, "medium_id": WorldFx.TELLING_MEDIUM_ORAL, "medium_index": 0}])
	h._hud._telling.reveal_newest()
	await h._settle()
