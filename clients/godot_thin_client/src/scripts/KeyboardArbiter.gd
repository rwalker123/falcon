extends RefCounted

## **WHO OWNS THE KEYBOARD RIGHT NOW** — the one arbiter, and the one roster of gameplay keys.
##
## Before this file the client had FIVE independent keyboard consumers and no arbiter: each decided
## for itself whether it should act, by a different mechanism, and three of them were wrong in
## different ways. Typing a save's name panned the map (`MapView._process` polls WASD), then toggled
## the event dock (`Main._process` polls `r`), then `Ctrl+C` re-centred the map
## (`MapView._unhandled_input` compares a bare `keycode`). Each fix chased the key the player
## happened to press next, because **nothing could enumerate the hotkeys** and so nothing could
## assert anything about all of them at once.
##
## Two things live here, and they are the same idea seen twice:
##
## 1. **THE REGISTRY** — every gameplay key the client reads, with the CLASS it belongs to and the
##    site that reads it. It is the roster a test can walk. `tools/hotkey_guard.gd` walks it in both
##    directions: every registry row is asserted against every owner, and every keyboard read in
##    `src/` must be accounted for by a row (or by a named exception). Adding a hotkey therefore
##    means adding a row, and adding a row automatically extends the test.
## 2. **THE ARBITER** — `keyboard_owner()`, a PURE function of the live state, returning who the
##    keyboard belongs to this frame; `allows()` then answers whether a class of key may act under
##    that owner. Purity is deliberate and copied from `Main.escape_claimant`, which solved exactly
##    this problem for exactly one key: booleans in, a name out, assertable without standing up a
##    scene.
##
## **WHY NOT A CONTEXT STACK.** The obvious shape — push a context when a surface opens, pop it when
## it closes — trades this bug class for a worse one: an unbalanced pop leaves the keyboard owned by
## a surface that is gone, which is the stuck-focus failure (`MenuShell.release_text_focus`) with no
## focus owner to inspect. The surfaces here (a `CanvasLayer` toggled by `visible`, a `LineEdit` that
## is rebuilt on every keystroke) have no reliable open/close hooks to hang a stack off. A DERIVED
## predicate cannot go out of balance because it is recomputed from the world every frame.
##
## **THE POLICY IS DELIBERATELY COARSE.** Three owners, and only `GAMEPLAY` may act. In particular a
## MODAL MENU now owns the keyboard even with nothing focused — before this, `r` toggled the event
## dock *behind* an open pause menu, which is the same complaint one step removed. Text entry
## outranks the menu because a field inside the menu is the case that started all of this.
##
## **WHAT IS NOT ARBITRATED, ON PURPOSE.** Targeting, the compose sheet and the work inspector all
## permit map motion — you pan while choosing a target — so they are not owners. And `ESCAPE` is
## allowed under EVERY owner, because it is how the player gets out; which surface it reaches is
## `Main.escape_claimant`'s four-way decision and is not re-litigated here.
##
## A static-func helper (no `class_name`, no autoload) on the `ClientBuild` / `ServerPortsFile` /
## `TextEntryFocus` pattern: it holds no state and its callers `preload` it as a collaborator.

## **IS THE PLAYER TYPING?** — still the one definition, still its own file, because `MenuShell` asks
## it about a single node when it HANDS THE KEYBOARD BACK, which is not an arbitration question.
## This file is its only other caller.
const TextEntryFocus := preload("res://src/scripts/TextEntryFocus.gd")

# ---- OWNERS ---------------------------------------------------------------------------------------
## A `LineEdit`/`TextEdit` holds this viewport's focus. Nothing but `ESCAPE` may act.
const OWNER_TEXT_ENTRY := "text_entry"
## A modal menu (the pause overlay) is open. Nothing but `ESCAPE` may act.
const OWNER_MODAL_MENU := "modal_menu"
## Nobody is claiming the keyboard: every class may act.
const OWNER_GAMEPLAY := "gameplay"
## Every owner, in precedence order — the roster `tools/hotkey_guard.gd` enumerates the registry
## against. A new owner that is not in this list is a policy nothing tests.
const OWNERS := [OWNER_TEXT_ENTRY, OWNER_MODAL_MENU, OWNER_GAMEPLAY]

# ---- KEY CLASSES ----------------------------------------------------------------------------------
## Pan and zoom (W/A/S/D, Q·E) — polled in `MapView._process`.
const CLASS_MAP_MOTION := "map_motion"
## View toggles that change what the map draws (C fit, H grid, T textures) — raw keys in
## `MapView._unhandled_input`.
const CLASS_MAP_VIEW := "map_view"
## The HUD surfaces (I V R F and the backquote workbench) — polled in `Main._process`.
const CLASS_PANEL_TOGGLE := "panel_toggle"
## The way out. **Allowed under every owner**, and routed by `Main.escape_claimant`.
const CLASS_ESCAPE := "escape"

# ---- HOW A KEY IS READ ----------------------------------------------------------------------------
## An `InputMap` action read by POLLING the `Input` singleton (`is_action_just_pressed`,
## `get_action_strength`). Polling never enters the event system, so a focused `LineEdit` does not
## starve it — this kind is why the arbiter exists at all.
const KIND_POLLED_ACTION := "polled_action"
## An `InputMap` action matched on an `InputEvent` in an `_unhandled_input` handler.
const KIND_EVENT_ACTION := "event_action"
## A raw `InputEventKey` matched by keycode in an `_unhandled_input` handler.
const KIND_KEYCODE := "keycode"

# ---- THE READING SITES ----------------------------------------------------------------------------
# Named so a registry row says WHERE the key is consumed, and so the source scan can hold each site
# to the guard its rows require.
const SITE_MAP_PROCESS := "MapView._process"
const SITE_MAP_UNHANDLED := "MapView._unhandled_input"
const SITE_MAIN_PROCESS := "Main._process"
const SITE_MAIN_UNHANDLED := "Main._unhandled_input"

## **EVERY GAMEPLAY KEY THE CLIENT READS.** One row per key; `id` is the action name for the two
## action kinds and a descriptive name for a raw key. `keycode` is authoritative for
## `KIND_POLLED_ACTION` too: `ensure_action_bindings` registers the binding FROM this table, so the
## key and the class that governs it cannot drift apart the way the two `_ensure_*_binding` copies in
## `MapView` and `Main` could.
const REGISTRY := [
	{"id": "map_pan_left", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_A, "site": SITE_MAP_PROCESS},
	{"id": "map_pan_right", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_D, "site": SITE_MAP_PROCESS},
	{"id": "map_pan_up", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_W, "site": SITE_MAP_PROCESS},
	{"id": "map_pan_down", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_S, "site": SITE_MAP_PROCESS},
	{"id": "map_zoom_in", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_E, "site": SITE_MAP_PROCESS},
	{"id": "map_zoom_out", "class": CLASS_MAP_MOTION, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_Q, "site": SITE_MAP_PROCESS},
	{"id": "map_fit_to_view", "class": CLASS_MAP_VIEW, "kind": KIND_KEYCODE,
		"keycode": KEY_C, "site": SITE_MAP_UNHANDLED},
	{"id": "map_grid_lines", "class": CLASS_MAP_VIEW, "kind": KIND_KEYCODE,
		"keycode": KEY_H, "site": SITE_MAP_UNHANDLED},
	{"id": "map_terrain_textures", "class": CLASS_MAP_VIEW, "kind": KIND_KEYCODE,
		"keycode": KEY_T, "site": SITE_MAP_UNHANDLED},
	{"id": "toggle_inspector", "class": CLASS_PANEL_TOGGLE, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_I, "site": SITE_MAIN_PROCESS},
	{"id": "toggle_victory", "class": CLASS_PANEL_TOGGLE, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_V, "site": SITE_MAIN_PROCESS},
	{"id": "toggle_event_dock", "class": CLASS_PANEL_TOGGLE, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_R, "site": SITE_MAIN_PROCESS},
	{"id": "toggle_fow", "class": CLASS_PANEL_TOGGLE, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_F, "site": SITE_MAIN_PROCESS},
	# BACKQUOTE for the designer surface: nothing else in the client binds it (the hotkey table in
	# `clients/godot_thin_client/CLAUDE.md` is the player-facing roster), and it costs the game no
	# letter it may still want.
	{"id": "toggle_workbench", "class": CLASS_PANEL_TOGGLE, "kind": KIND_POLLED_ACTION,
		"keycode": KEY_QUOTELEFT, "site": SITE_MAIN_PROCESS},
	{"id": "ui_cancel", "class": CLASS_ESCAPE, "kind": KIND_EVENT_ACTION,
		"keycode": KEY_ESCAPE, "site": SITE_MAIN_UNHANDLED},
	{"id": "targeting_cancel", "class": CLASS_ESCAPE, "kind": KIND_KEYCODE,
		"keycode": KEY_ESCAPE, "site": SITE_MAP_UNHANDLED},
]

# ---- THE ARBITER ----------------------------------------------------------------------------------

## **WHO OWNS THE KEYBOARD** — pure, on `Main.escape_claimant`'s pattern, so the whole policy can be
## enumerated by a test with no scene standing.
##
## Text entry outranks the modal menu because the field that started this lives INSIDE the menu:
## asking "is a menu open?" first would hand a typed `r` back to the event dock.
static func keyboard_owner(text_entry_focused: bool, modal_menu_open: bool) -> String:
	if text_entry_focused:
		return OWNER_TEXT_ENTRY
	if modal_menu_open:
		return OWNER_MODAL_MENU
	return OWNER_GAMEPLAY

## **MAY THIS CLASS OF KEY ACT UNDER THIS OWNER?**
##
## `ESCAPE` is allowed under every owner — a player who cannot leave a surface is stuck, and the
## surface it reaches is `Main.escape_claimant`'s decision, not this one's. Everything else needs the
## keyboard to be unclaimed.
static func allows(owner: String, key_class: String) -> bool:
	if key_class == CLASS_ESCAPE:
		return true
	return owner == OWNER_GAMEPLAY

## The arbiter fed from the LIVE world — the only impure function here, and the one every call site
## uses. A null viewport answers `GAMEPLAY`: a node outside the tree cannot be competing with a field
## for the keyboard, and refusing to act there would silently disable the map inside a harness.
##
## `modal_menu_open` is passed IN rather than discovered, because the pause overlay is `Main`'s node:
## `Main` reads its own `pause_layer.visible` and pushes the flag to `MapView`
## (`set_modal_menu_open`), the same coordinator mediation every other cross-node fact here uses.
static func owner_for(viewport: Viewport, modal_menu_open: bool) -> String:
	return keyboard_owner(TextEntryFocus.held_in(viewport), modal_menu_open)

# ---- EXACT MATCHING -------------------------------------------------------------------------------

## **A BARE KEY, WITH NO MODIFIER HELD** — the raw-event half of exact matching.
##
## `event.keycode == KEY_C` is ALSO true for `Ctrl+C`, `Cmd+C` and `Shift+C`, which is how copying a
## save's name re-centred the map. Every raw keycode comparison in the client goes through here
## instead; the polled sites get the same property from `exact_match = true`.
##
## Echo repeats are dropped too: these are one-shot toggles, and a held key re-firing them every
## frame was never wanted.
static func is_bare_key(event: InputEvent, keycode: Key) -> bool:
	var key := event as InputEventKey
	if key == null:
		return false
	if not key.pressed or key.echo:
		return false
	if key.keycode != keycode:
		return false
	return not (key.ctrl_pressed or key.alt_pressed or key.meta_pressed or key.shift_pressed)

## **ESCAPE, WITH OR WITHOUT MODIFIERS** — deliberately NOT `is_bare_key`.
##
## Escape is the way out of a surface, so a stray held modifier must not strand the player in it.
## This is the one key in the registry that is matched loosely, and the only one where a loose match
## costs nothing: no surface binds a modified Escape to anything else.
static func is_escape_key(event: InputEvent) -> bool:
	var key := event as InputEventKey
	if key == null:
		return false
	return key.pressed and not key.echo and key.keycode == KEY_ESCAPE

# ---- BINDINGS -------------------------------------------------------------------------------------

## Register the `InputMap` bindings for one class, from the registry. Idempotent, and additive: an
## action the project already defines (`project.godot` ships the pan/zoom and inspector ones) keeps
## its events, so a player's rebinding survives.
##
## **THE EVENT IS CREATED WITH NO MODIFIER FLAGS SET**, which is what makes `exact_match = true`
## reads at the polled sites match a bare keypress and refuse a modified one.
static func ensure_action_bindings(key_class: String) -> void:
	for entry in REGISTRY:
		if entry["class"] != key_class or entry["kind"] != KIND_POLLED_ACTION:
			continue
		var action: String = entry["id"]
		if not InputMap.has_action(action):
			InputMap.add_action(action)
		var keycode: Key = entry["keycode"]
		var already_bound := false
		for existing in InputMap.action_get_events(action):
			var existing_key := existing as InputEventKey
			if existing_key != null and (existing_key.keycode == keycode
					or existing_key.physical_keycode == keycode):
				already_bound = true
				break
		if already_bound:
			continue
		var event := InputEventKey.new()
		event.keycode = keycode
		event.physical_keycode = keycode
		InputMap.action_add_event(action, event)

# ---- REGISTRY QUERIES (used by the guard and by the sites) ----------------------------------------

## Every row whose `id` matches, or `{}` when the key is not registered — which is what the source
## scan reports as an unaccounted-for read.
static func entry_for(id: String) -> Dictionary:
	for entry in REGISTRY:
		if entry["id"] == id:
			return entry
	return {}

## The ids of every row read by polling `Input`. The source scan holds each polled read in `src/` to
## this list.
static func polled_action_ids() -> Array:
	var ids: Array = []
	for entry in REGISTRY:
		if entry["kind"] == KIND_POLLED_ACTION:
			ids.append(entry["id"])
	return ids
