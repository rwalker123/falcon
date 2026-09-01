extends RefCounted

## **IS THE PLAYER TYPING?** — the ONE definition, shared by every site that has to know.
##
## Two things in this client read the keyboard by POLLING the `Input` singleton rather than by
## handling an event: `MapView._process` (pan/zoom, `get_action_strength`) and `Main._process` (the
## five toggle hotkeys, `is_action_just_pressed`). **Polling samples raw device state and never
## enters the event system**, so a focused `LineEdit` consuming the keystroke is irrelevant to it —
## typing a save's name panned the map with W/A/S/D and toggled the event dock with `r`. Every other
## keyboard path in the client is event-driven and is correctly starved by a focused Control.
##
## **IT IS ONE FILE BECAUSE TWO COPIES DRIFT.** The predicate started as a private method on
## `MapView` and `Main`'s hotkeys were fixed a commit later; the moment someone adds a `SpinBox` or a
## rich-text field to one copy and not the other, the bug returns in half the client and looks like a
## different bug. `MenuShell`'s focus RELEASE asks the same question about a single node, so it comes
## through here too — there is no second spelling of "this is a text field" anywhere.
##
## **TEXT ENTRY ONLY, NEVER "ANYTHING FOCUSED".** A focused Button does not consume letters, so
## suppressing hotkeys whenever a button held focus would kill WASD and the toggles after every click
## on a HUD control — a worse bug than the one this exists to fix.
##
## A static-func helper (no `class_name`, no autoload) for the same reasons `ClientBuild` and
## `ServerPortsFile` are: it holds no state, and its callers `preload` it like a collaborator rather
## than depending on the global class cache.

## The two Godot controls that eat printable keys. `CodeEdit` subclasses `TextEdit`, so the `is`
## check covers it; nothing else in the client's widget vocabulary takes free text.
static func is_text_entry(node: Object) -> bool:
	return node is LineEdit or node is TextEdit


## **THE GUARD.** True when this viewport's keyboard focus sits in a text field, i.e. when a polled
## read would be stealing a keystroke the player meant for that field.
##
## A null viewport answers `false`: a node outside the tree cannot be competing with a field for the
## keyboard, and refusing to poll there would silently disable the map in a harness.
static func held_in(viewport: Viewport) -> bool:
	if viewport == null:
		return false
	return is_text_entry(viewport.gui_get_focus_owner())
