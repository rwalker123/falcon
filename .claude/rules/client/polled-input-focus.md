---
paths:
  - "clients/godot_thin_client/src/scripts/TextEntryFocus.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
---

# Polled input and the typing player

**Its own file, deliberately narrow.** The two polled-input sites live in `MapView.gd` and `Main.gd`
and the release lives in `MenuShell.gd`; folding this into `map-renderers.md` would drag the whole
MapView renderer-decomposition doc onto every `Main.gd` edit to carry one section, and leaving it
there without `Main.gd` in the `paths:` would hide it from half the code it governs.

## Typing must not drive the game — the TWO polled input sites

`MapView._process` reads pan and zoom with `Input.get_action_strength("map_pan_*")` /
`("map_zoom_*")`. **That samples raw device state and never enters the event system**, so a focused
`LineEdit` consuming the keystroke is irrelevant to it: the map panned and zoomed while the player
typed a save's name, with W/A/S/D and Q·E doing both jobs at once. It is not a focus bug in the event
paths — `_unhandled_input` is correctly never reached for a key a Control consumed — and fixing it
there would have changed nothing. `C`/`H`/`T` and the targeting Escape live in `_unhandled_input` and
need no guard for exactly that reason.

**THE SECOND SITE IS `Main._process`, and missing it cost a second bug report.** Its five toggle
hotkeys — `toggle_inspector`, `toggle_victory`, `toggle_event_dock`, `toggle_fow`,
`WORKBENCH_TOGGLE_ACTION` — are read with `Input.is_action_just_pressed`, which polls exactly as
`get_action_strength` does. The first fix guarded only `MapView`, because the sweep that scoped it
searched `is_action_pressed` / `get_action_strength` / `is_key_pressed` and **omitted
`is_action_just_pressed`**, which is how every HUD hotkey is read — so typing `r` in the save field
still toggled the event dock behind the menu. A sweep for polled input has to be a sweep of the
`Input` SINGLETON, not of the four spellings that come to mind.

**The full set is those two functions and nothing else**, confirmed by `Input\.` across every client
`.gd`. `Input.is_mouse_button_pressed` in `MapView._process` is polled too and is deliberately
unguarded: it reads a MOUSE button to release a drag latch, and no text field competes for one.
`Input.warp_mouse` / `parse_input_event` are writes, not reads.

**One predicate, one file** — `src/scripts/TextEntryFocus.gd`, an all-static helper on the
`ClientBuild` / `ServerPortsFile` pattern, asked by `MapView._text_entry_has_focus`, by `Main`'s
hotkey block and by `MenuShell`'s focus release. It started as a private method here and the second
site was fixed a commit later; two copies of *"is the player typing?"* is precisely how a later
`SpinBox` gets added to one and not the other and the bug returns in half the client.

**Scoped to TEXT ENTRY, never to "anything focused".** A focused Button does not consume letters, so
suppressing input whenever a button held focus would kill the map and every panel toggle after each
click on a HUD control — a worse bug than the one being fixed.

**The guard covers the keystroke reads and NOTHING around them.** In `MapView._process` the mouse-pan
release latch and the targeting / expedition pulses stay outside it; in `Main._process` the query
pump, the connection poll, the world-request retry and the snapshot drain stay outside it — that pump
is what carries the answer to the save being named, so a guard around the whole function would stall
the socket the save depends on.

**The failure in the other direction is worse than the bug, and it is the caller's to prevent.** A
field that keeps focus after its surface is dismissed leaves the map unresponsive to WASD and every
panel toggle dead, with nothing on screen to explain why. So the surface that owns a text field owns
handing the keyboard back — `MenuShell.release_text_focus()`, called on every pane change, after a
save is submitted, and by `Main._hide_pause_menu`. **Hiding a `CanvasLayer` does not do it for you**:
`CanvasLayer` is not a `CanvasItem`, so its `visible` never reaches the Controls under it as a
visibility change. Neither does `queue_free` within the frame it is called — the node holds focus
until it actually leaves the tree.

Verified by sabotage, three ways, in `menu_preview._assert_text_focus_is_handed_back`: widening the
predicate to `is Control` fails the narrowness leg; making it always false fails all three
focus-is-taken legs; stubbing `release_text_focus` fails all three release legs — which is also the
proof that neither `queue_free` nor hiding the layer releases focus on its own.

## Key scripts

| Script | Purpose |
|--------|---------|
| `TextEntryFocus.gd` | The ONE *"is the player typing?"* predicate — `is_text_entry(node)` and `held_in(viewport)`. Asked by `MapView._text_entry_has_focus`, by `Main._process`'s hotkey block and by `MenuShell.release_text_focus`. All-static, no `class_name`, `preload`ed by its callers (the `ClientBuild` / `ServerPortsFile` pattern) |
