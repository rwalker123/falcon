extends RefCounted

## THE OPENING CARD — the Telling's tick-0 lines said on a modal card, once per world, in place of the
## outfitting card's first auto-open, and handing off to it.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. See `.claude/rules/client/test-harnesses.md`. It runs LAST, because every
## block starts on a WORLD BOUNDARY (`reset_world_state`) — that is what re-arms a once-per-world card —
## and a reset clears state every chapter before it relies on. It ends on one too.
##
## ## THE TWO INPUTS ARE PUSHED IN `Main`'s OWN ORDER
##
## Within one snapshot `Main` dispatches `populations` (the outfitting window) BEFORE `command_events`
## (the beats), so the opening frame holds the window before the story has been read. The first block
## pushes them in exactly that order, with no frame between — the case a decision taken where either
## lands gets wrong. The other orders (beats first; beats a whole snapshot late) each have a block.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 32

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# ---- the two real opening lines ----------------------------------------------------------------
## `opening.cold_open` / `guidance.food_and_the_split` from `core_sim/src/data/beat_definitions.json`,
## rendered in the mythic register with `{count}` at the fixture band's size — spelled out rather than
## composed, so the card is judged on the prose the sim really sends.
const COLD_OPEN_LINE := "We are 30. The ground behind us is bone, and we will not go back to it. Ahead lies a country with no names — not the hills, not the waters, not the years to come. Naming it is your work now. Walk well, and be remembered."
const GUIDANCE_LINE := "Food is the first thing. A band eats only from the ground around it, and when that ground thins, the band goes hungry. If we grow too many for one place, split the band and let the new one walk to fresh ground. But first, before we walk: choose what we carry."
const COLD_OPEN_GLOSS := "turn.index = 0 · band.count = 30"
const GUIDANCE_GLOSS := "provisions.total = 12 · band.count = 30"
## A beat on a LATER tick, and a non-beat on tick 0 — both must stay off the card.
const LATER_LINE := "The first frost came early that year."
const LATER_TICK := 3
const OPENING_TICK := 0
const KIND_BEAT := "narrative_beat"
## A tick-0 row of another kind — what the filter must not take for an opening line.
const KIND_OTHER := "command_echo"
const OTHER_LINE := "Outfit order sent."

# ---- the opening band ---------------------------------------------------------------------------
const GRANT_ENTITY := 51
const SPLINTER_ENTITY := 52
const BAND_SIZE := 30
const BAND_X := 44
const BAND_Y := 9
const KIT_BUDGET := 17
const MATERIAL_BUDGET := 30
const KIT_STALKING := "big_game"
const KIT_GATHERING := "gathering"
const DEFAULT_STALKING := 4
const DEFAULT_GATHERING := 4
const MATERIAL_HIDE := "hide"
const DEFAULT_HIDE := 6
## How far below the card the catcher click lands — inside the scrim, outside the card.
const CATCHER_CLICK_BELOW_CARD := 40.0

func run(harness) -> void:
	h = harness
	await _beats_after_window_in_one_frame()
	await _catcher_click_hands_off()
	await _escape_hands_off()
	await _no_story_opens_outfitting()
	await _late_story_raises_the_card()
	await _late_story_after_the_opening_shows_nothing()
	# Leave the world as the chapter found the boundary: nothing open, nothing held.
	_new_world()
	await h._settle()

# ---- the states ---------------------------------------------------------------------------------

## The reported case: the window and the two beats in ONE snapshot, in `Main`'s dispatch order.
func _beats_after_window_in_one_frame() -> void:
	_new_world()
	h._hud.update_band_alerts([_grant_band(true)])
	h._hud.ingest_command_events(_opening_events())
	await h._settle()
	var opening := _opening()
	h._assert_hud("opening — the card shows when the window lands BEFORE the beats in one frame",
		opening.is_open())
	h._assert_hud("opening — the outfitting card is held at its pill, not expanded, under the card",
		_loadout().is_open() and not _loadout().is_expanded())
	var lines := opening.panel().lines()
	h._assert_hud("opening — the card carries exactly the tick-0 beats, cold open then guidance (%s)"
		% [lines], lines == [COLD_OPEN_LINE, GUIDANCE_LINE])
	h._assert_hud("opening — a later tick's beat and a tick-0 non-beat stay off the card",
		not lines.has(LATER_LINE) and not lines.has(OTHER_LINE))
	var button := opening.panel().handoff_button()
	h._assert_hud("opening — one button, reading the vocab's hand-off label (got %s)"
		% [button.text if button != null else "<none>"],
		button != null and button.text == HudLoadoutVocab.OPENING_HANDOFF_LABEL)
	h._assert_hud("opening — no gloss reaches the card",
		not _subtree_text(opening.panel()).contains(COLD_OPEN_GLOSS))
	h._assert_hud("opening — the beats still reach the Telling panel",
		_telling_holds(COLD_OPEN_LINE) and _telling_holds(GUIDANCE_LINE))
	h._assert_hud("opening — ESC belongs to the card while it is up",
		_esc_claimant() == h.MAIN_SCRIPT.ESC_OPENING_CARD)
	await h._save("opening_card")

	await _press(button)
	h._assert_hud("opening — the button closes the card", not opening.is_open())
	h._assert_hud("opening — …and opens the opening band's outfitting card",
		_loadout().is_expanded() and _loadout().subject_band_id() == _band_id(GRANT_ENTITY))
	await h._save("opening_card_handoff")

	# ONCE PER WORLD: a full snapshot re-sends the ring and the roster; nothing comes back up.
	h._hud.update_band_alerts([_grant_band(true)])
	h._hud.ingest_command_events(_opening_events())
	await h._settle()
	h._assert_hud("opening — a re-sent opening in the same world does not raise the card again",
		not opening.is_open() and _loadout().is_expanded())
	h._assert_hud("opening — with the card gone, ESC falls through to the pause menu",
		_esc_claimant() == h.MAIN_SCRIPT.ESC_PAUSE)

	# A SPLINTER LATER IN THE GAME is the controller's ordinary auto-open — the hold was the first only.
	h._hud.update_band_alerts([_grant_band(true), _splinter_band()])
	await h._settle()
	h._assert_hud("opening — a later splinter's window opens its own card, untouched by the hold",
		not opening.is_open() and _loadout().is_expanded()
		and _loadout().subject_band_id() == _band_id(SPLINTER_ENTITY))

## Beats FIRST, window second, and the player clicks the scrim rather than the button.
func _catcher_click_hands_off() -> void:
	_new_world()
	h._hud.ingest_command_events(_opening_events())
	h._hud.update_band_alerts([_grant_band(true)])
	await h._settle()
	var opening := _opening()
	h._assert_hud("opening/catcher — the card shows when the beats land BEFORE the window",
		opening.is_open() and not _loadout().is_expanded())
	var card_rect := opening.panel()._card.get_global_rect()
	var viewport: Viewport = h.get_viewport()
	var canvas_point := Vector2(card_rect.get_center().x,
		minf(card_rect.end.y + CATCHER_CLICK_BELOW_CARD, viewport.get_visible_rect().size.y - 1.0))
	h._assert_hud("opening/catcher — the click point is outside the card",
		not card_rect.has_point(canvas_point))
	var point := InputProbe.canvas_to_window(viewport, h.get_window(), canvas_point)
	InputProbe.hover(viewport, point)
	InputProbe.press_left(viewport, point)
	InputProbe.release_left(viewport, point)
	await h._settle()
	h._assert_hud("opening/catcher — a click on the scrim closes the card", not opening.is_open())
	h._assert_hud("opening/catcher — …and still opens the outfitting card, never dropping it",
		_loadout().is_expanded() and _loadout().subject_band_id() == _band_id(GRANT_ENTITY))

## ESC, through `Main`'s own chain: the claimant first, then the action it routes to.
func _escape_hands_off() -> void:
	_new_world()
	h._hud.update_band_alerts([_grant_band(true)])
	h._hud.ingest_command_events(_opening_events())
	await h._settle()
	h._assert_hud("opening/esc — the card is up", _opening().is_open())
	h._assert_hud("opening/esc — ESC is the card's, even over an open compose sheet's claim",
		h.MAIN_SCRIPT.escape_claimant(false, true, false, false, false, false, true)
			== h.MAIN_SCRIPT.ESC_OPENING_CARD)
	h._assert_hud("opening/esc — Main's probe names exist on the HUD",
		h._hud.has_method("is_opening_card_open") and h._hud.has_method("dismiss_opening_card"))
	h._hud.dismiss_opening_card()
	await h._settle()
	h._assert_hud("opening/esc — ESC closes the card", not _opening().is_open())
	h._assert_hud("opening/esc — …and opens the outfitting card",
		_loadout().is_expanded() and _loadout().subject_band_id() == _band_id(GRANT_ENTITY))

## A world whose opening says nothing — a save loaded mid-opening, beats absent — opens as before.
func _no_story_opens_outfitting() -> void:
	_new_world()
	h._hud.update_band_alerts([_grant_band(true)])
	await h._settle()
	h._assert_hud("opening/none — with no tick-0 beat the card does not show", not _opening().is_open())
	h._assert_hud("opening/none — …and the outfitting card opens itself as it always did",
		_loadout().is_expanded())

## Continues the world above: the story arrives a snapshot LATE, with the window still open.
func _late_story_raises_the_card() -> void:
	h._hud.ingest_command_events(_opening_events())
	await h._settle()
	var opening := _opening()
	h._assert_hud("opening/late — a story arriving a snapshot late still raises the card",
		opening.is_open())
	h._assert_hud("opening/late — …over the outfitting card, put away to its pill",
		_loadout().is_open() and not _loadout().is_expanded())
	await h._save("opening_card_late")
	await _press(opening.panel().handoff_button())
	h._assert_hud("opening/late — the button hands back to the outfitting card",
		not opening.is_open() and _loadout().is_expanded())

## The story arrives only after the turn advanced and the window shut: there is no opening to say.
func _late_story_after_the_opening_shows_nothing() -> void:
	_new_world()
	h._hud.update_band_alerts([_grant_band(true)])
	await h._settle()
	h._hud.update_band_alerts([_grant_band(false)])
	await h._settle()
	h._assert_hud("opening/shut — precondition: the window has shut", not _loadout().is_open())
	h._hud.ingest_command_events(_opening_events())
	await h._settle()
	h._assert_hud("opening/shut — a story after the window shut raises nothing",
		not _opening().is_open())

# ---- helpers ------------------------------------------------------------------------------------

## A WORLD BOUNDARY, then the two catalogues the outfitting card joins onto — the reset clears them.
func _new_world() -> void:
	h._hud.reset_world_state()
	h._hud.update_equipment_config(JSON.stringify(_equipment_config()))
	h._hud.update_opening_loadout({
		HudLoadoutVocab.PICKABLE_MATERIALS_KEY: [MATERIAL_HIDE],
		HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY: [],
	})

func _opening() -> OpeningCardController:
	return h._hud.opening_card()

func _loadout() -> StartingLoadoutController:
	return h._hud.starting_loadout_panel()

func _esc_claimant() -> String:
	return h.MAIN_SCRIPT.escape_claimant(false, h._hud.is_compose_sheet_open(),
		h._hud.is_targeting_active(), false, false, false, h._hud.is_opening_card_open())

## Real pointer input on the control's own rect centre; the press and the release with no frame
## between them (`test-harnesses.md` → "A SIMULATED GESTURE IS NOT HERMETIC").
func _press(control: Control) -> void:
	if control == null:
		h._assert_hud("opening — the control to press was rendered", false)
		return
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		control.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	InputProbe.release_left(viewport, point)
	await h._settle()

func _telling_holds(line: String) -> bool:
	for entry_variant in h._hud._telling._entries:
		if String((entry_variant as Dictionary).get("bbcode", "")).contains(line):
			return true
	return false

func _subtree_text(node: Node) -> String:
	var out := ""
	if node is Label:
		out += (node as Label).text + "\n"
	elif node is Button:
		out += (node as Button).text + "\n"
	for child in node.get_children():
		out += _subtree_text(child)
	return out

## The opening as the sim sends it on turn 0, plus the two rows the card must NOT take.
func _opening_events() -> Array:
	return [
		{"tick": OPENING_TICK, "kind": KIND_BEAT, "label": COLD_OPEN_LINE, "detail": COLD_OPEN_GLOSS},
		{"tick": OPENING_TICK, "kind": KIND_OTHER, "label": OTHER_LINE, "detail": ""},
		{"tick": OPENING_TICK, "kind": KIND_BEAT, "label": GUIDANCE_LINE, "detail": GUIDANCE_GLOSS},
		{"tick": LATER_TICK, "kind": KIND_BEAT, "label": LATER_LINE, "detail": ""},
	]

func _grant_band(open: bool) -> Dictionary:
	return _band(GRANT_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: open,
		HudLoadoutVocab.KIT_BUDGET_KEY: KIT_BUDGET,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: MATERIAL_BUDGET,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		HudLoadoutVocab.WINDOW_KITS_KEY: [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_STALKING},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_GATHERING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_GATHERING},
		],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: MATERIAL_HIDE,
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_HIDE},
		],
	})

## A splinter's TAKE on the opening band, the window a later split opens.
func _splinter_band() -> Dictionary:
	return _band(SPLINTER_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.KIT_BUDGET_KEY: 0,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: 0,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: _band_id(GRANT_ENTITY),
		HudLoadoutVocab.WINDOW_KITS_KEY: [],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [],
	})

func _band(entity: int, window: Dictionary) -> Dictionary:
	return BandFx.with_band_id({
		"entity": entity,
		"faction": HudConst.PLAYER_FACTION_ID,
		"size": BAND_SIZE,
		"working_age": KIT_BUDGET,
		"current_x": BAND_X,
		"current_y": BAND_Y,
		"idle_workers": 0,
		HudLoadoutVocab.WINDOW_KEY: window,
	})

func _band_id(entity: int) -> int:
	return entity + BandFx.FIXTURE_BAND_ID_OFFSET

func _equipment_config() -> Dictionary:
	return {HudLoadoutVocab.CONFIG_KITS_KEY: [
		_kit(KIT_STALKING, "Stalking kit", ["hunt"], ["spears", "sled"]),
		_kit(KIT_GATHERING, "Harvesting kit", ["forage"], ["baskets"]),
	]}

func _kit(id: String, display_name: String, jobs: Array, uses: Array) -> Dictionary:
	return {
		HudLoadoutVocab.KIT_ID_KEY: id,
		HudLoadoutVocab.KIT_DISPLAY_NAME_KEY: display_name,
		HudLoadoutVocab.KIT_JOBS_KEY: jobs,
		HudLoadoutVocab.KIT_USES_KEY: uses,
	}
