extends RefCounted

## **IS THE PLAYER TYPING?** — the ONE definition, and one of the two inputs to the arbiter.
##
## `KeyboardArbiter.owner_for` asks this to decide whether `OWNER_TEXT_ENTRY` holds the keyboard; see
## that file for the policy this feeds. **Focus is not a gate on its own**, which is the mistake this
## file used to encode: polled reads (`MapView._process`, `Main._process`) never enter the event
## system at all, and a focused `LineEdit` consumes only the keys it USES — everything else still
## falls through to `_unhandled_input`. So the answer here is an INPUT to the decision, never the
## decision.
##
## **IT IS ITS OWN FILE BECAUSE THE OTHER CALLER IS NOT ARBITRATING.** `MenuShell.release_text_focus`
## asks `is_text_entry` about a single node when it HANDS THE KEYBOARD BACK — the mirror-image
## failure, and a different question from "who may act". Two spellings of "this is a text field" is
## how a later `SpinBox` gets added to one and not the other, so there is exactly one.
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


## True when this viewport's keyboard focus sits in a text field — i.e. when a gameplay key would be
## stealing a keystroke the player meant for that field.
##
## A null viewport answers `false`: a node outside the tree cannot be competing with a field for the
## keyboard, and refusing to poll there would silently disable the map in a harness.
static func held_in(viewport: Viewport) -> bool:
	if viewport == null:
		return false
	return is_text_entry(viewport.gui_get_focus_owner())
