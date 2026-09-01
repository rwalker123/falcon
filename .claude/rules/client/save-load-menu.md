---
paths:
  - "clients/godot_thin_client/src/scripts/SaveSlots.gd"
  # `Main` owns the load handoff, the drift notice and the pause menu's focus release, so the rule
  # describing them has to load when Main is touched.
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/ui/ConfigDriftNotice.gd"
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/src/scripts/ui/LandingScreen.gd"
  - "clients/godot_thin_client/src/scripts/GameLaunch.gd"
  - "clients/godot_thin_client/src/scripts/CommandClient.gd"
  - "clients/godot_thin_client/native/src/bridge/query.rs"
---

# Save & load: the menu's half of the save channel

The server half — the blob, the slot files, what a load owes, the autosave cadence — is
`.claude/rules/core_sim/save-game.md`. This file is the client's: the seam that asks, the panes that
render the answers, and the two things a load does that a save does not.

## The four asks ride the QUERY worker, not the command line

`list_saves` is a genuine `QueryPayload`. `save_game` / `load_game` / `delete_save` are
`CommandPayload`s that **answer on `QueryReplyEnvelope`**, because a client that cannot tell whether
its save landed has not saved. All four therefore go through `bridge/query.rs`'s worker rather than
`send_line`, for the one reason a forecast does: the answer is written back on the connection that
asked, so the connection has to stay open to read it. `QueryRequest` carries a whole `CommandPayload`
and its own timeout for exactly this.

**The save channel's timeout is its own** (`SAVE_REPLY_TIMEOUT`, 60s against the forecast's 5s). A
forecast is arithmetic over a world already in memory; a save verb touches the disk and a load stands
a whole Bevy app back up — 118 ms to encode and 63 ms to decode at 160x104, queued behind whatever
turn the sim is resolving. Firing early here does not merely lose an answer, it tells the player their
save failed while it is being written.

## Two seams, one drain, and why the id spaces are disjoint

`ForecastQuery` and `SaveSlots` are both fed from **one** `CommandBridge.poll_query_replies` call.
That drain is destructive, so `Main._pump_forecast_queries` drains once and hands the same array to
both; each ignores ids it does not hold. **`SaveSlots.REQUEST_ID_BASE` (`1 << 40`) is what makes that
safe** — two counters both starting at 1 would each answer the other's replies, landing a forecast in
the save pane or a `save_op` on a compose sheet. Ids are `u64` on the wire and `ForecastQuery` counts
up from 1, so a collision needs four billion forecasts in one session and there is no coordination to
forget.

## `MenuShell` is a view over an INJECTED seam

`MenuShell` has no handle to `Main`, the `Inspector` or a `CommandClient` and must not grow one — the
same boundary the fog-of-war row keeps. The owner builds `SaveSlots` over its command client and
hands it in with `set_save_slots`; the panes read its state, call its verbs, and emit.

**The landing screen owns a command client of its own**, and that is the point of `list_saves` being
answered before the server's `world_active` gate: the load menu is opened when there is no world, and
a `no_active_world` refusal there would make the feature unreachable from the one screen that needs
it. `CommandClient` therefore owns the endpoint precedence (`resolve_host` / `resolve_port` /
`resolve_proto_port`, env → ports file → constant) rather than `Main`, which is no longer its only
caller.

## The saves panes are rebuilt whole, and opening one is an ASK

`_build_saves_pane` serves both Load and Save; only the blurb, the name field and the action row
differ. Every seam answer rebuilds the pane from scratch, which keeps *what is on screen* a pure
function of the seam's state rather than widgets patched from three directions — and is why
`_save_name_text` is a member, and why `_on_save_name_changed` re-grabs focus and the caret: the field
being typed into is a NEW node on every keystroke.

**Only an explicit open re-asks.** `_activate_item` routes the two saves panes through
`_open_saves_pane`, never `_show_pane` — `refresh()` emits `slots_changed` synchronously, so a rebuild
that re-asked would recurse. A FAILED list is likewise not re-asked automatically; it offers a **Try
again** button, because a rebuild-driven retry would spin the socket for as long as the pane is open.

**`LIST_IDLE`, `LIST_PENDING`, `LIST_READY`-with-no-rows and `LIST_FAILED` are four different lines.**
"No saves yet" and "we have not asked yet" and "the server did not answer" are three different facts,
and only one of them is a dead end. No disabled button in these panes is unexplained: the status line,
the empty-list note or the name-field caption is always above it.

## Every rule the server enforces is enforced by the affordance first

The whitelist (letters, digits, space, `-`, `_`, ≤ 64) is mirrored in `SaveSlots.slot_name_error`,
which returns the player's sentence and drives a caption under the field — a bad name is never
discovered by pressing a button and getting `invalid_slot` back from a round trip that had no chance.
**`autosave` is refused locally** for save and delete (a load of it is exactly what a rolling backup
is for), which is what makes the pane's promise true rather than aspirational.

**There are no modals.** The shell's confirmation pattern is *the consequence in the button's own
label plus the `armed` variant*, as "Abandon and return to menu" and "Apply now — ends this run"
already are:

| Act | What the button says |
|---|---|
| Save to a new name | `Save to “<name>”`, primary |
| Save over one that exists | `Overwrite “<name>”`, **armed** |
| Load from the landing screen | `Load selected`, primary |
| Load from inside a run | `Load — discards this run`, **armed** |
| Delete | `Delete` arms a second row: `Delete “<name>” permanently` + `Cancel` |

## The name field borrows the keyboard, and hands it back

The slot-name `LineEdit` is the first free-text input in the game, and it exposed a pre-existing
defect: `MapView._process` polls WASD and Q·E with `Input.get_action_strength`, which never touches
the event system, so typing a save's name also drove the map. The guard lives where the polling does
(`.claude/rules/client/map-renderers.md` → "Typing must not drive the map"); what belongs to this
file is the other half. **`MenuShell.release_text_focus()` is called on every pane change, after a
save is submitted, and by `Main._hide_pause_menu`** — because focus left stuck kills WASD for the rest
of the session with nothing on screen to explain it, which is strictly worse than the bug. Neither
`queue_free` nor hiding the pause `CanvasLayer` releases it; the release is explicit.

It releases only a text control **inside this shell**. A focused field elsewhere in the tree is
somebody else's to hand back.

## A load is the `new_game` handoff, wearing the same clothes

A load rebuilds the world server-side and bumps `WorldEpoch`, so it is subject to the whole world
handoff (`.claude/rules/core_sim/world-handoff.md`) and reuses it rather than inventing a second path.
`MenuShell` therefore **emits `load_requested` and sends nothing**:

- `LandingScreen` stashes the slot in `GameLaunch.pending_load_slot` and swaps to `Main.tscn`.
- `Main._on_pause_load` stashes it and **reloads its own scene**. Sending `load_game` in place would
  leave a live HUD rendering the old world while the server built another one.

`Main._build_world_request` then consumes whichever handoff is armed — **a pending load wins**, the
two are never armed together — and `_try_send_world_request` sends `new_game` as a text command or
`load_game` through the seam, under the identical retry-until-answered latch. `GameLaunch` gains
`active_load_slot` beside `active_new_game` for the same reason that one exists: `apply_theme_now`
reloads the scene, and without it a theme applied mid-run would silently GENERATE a world in place of
the save being played.

**Only a transport failure re-asks a refused load**, the split `ForecastQuery` makes. `no_such_slot`
and `unreadable` are statements about that slot and re-asking cannot change them, so the reason stands
on the loading overlay (`_set_loading_overlay_text`) and the latch stays set.

## The config-drift notice names files, over the world it is about

The sim deliberately does not save config, so a loaded world runs under whatever tuning is live now.
`SaveOpReply.config_drift` is the only place that fact exists, and empty is the good case.

It is **not** shown in the menu pane: a successful load is already tearing that shell down.
`Main` holds the rows from the reply until the loaded world REVEALS, then raises `ConfigDriftNotice`
over it — the numbers it warns about are the ones now on screen. Consumed on raise, so a later world
reveal cannot re-show the previous load's warning.

Each row is one file plus one sentence, and **the five (saved → live) pairs get five different
sentences**: edited, now loaded from a file, the file is gone, not recorded in the save, recorded but
no longer loaded. `Builtin` and `File` are a real difference — a shipped default appearing or a file
being deleted changes what the sim runs on just as an edit does — so collapsing them would report the
change as an edit that never happened.

**The card is an `AutoSizingPanel`** (`.claude/rules/client/panel-framework.md`), and that is not
decoration. Measuring it by hand was the first shape and it produced a card the height of the screen:
an autowrap Label's minimum height is only right once the container has been sorted at the card's real
width, so a same-pass measurement wraps the prose at zero width. The width goes on now and the height
is taken a frame later.

## Key scripts

| Script | Purpose |
|--------|---------|
| `SaveSlots.gd` (`class_name SaveSlots`) | The save-channel seam, modelled on `ForecastQuery`: `set_sender`/`deliver`, `refresh`/`request_save`/`request_load`/`request_delete`, the `slots_changed` + `op_finished` signals, the four list states, the error-token → prose table, the slot-name whitelist, and the `format_size` / `format_when` renderers. **Owns no socket**; its ids live at `REQUEST_ID_BASE` so it can share `ForecastQuery`'s drain |
| `ui/ConfigDriftNotice.gd` (`class_name ConfigDriftNotice`) | The post-load warning: an `AutoSizingPanel` card over a scrim naming each config file whose tuning moved, one sentence per (saved → live) pair. Raised by `Main` on the reveal that follows a load, never by the menu |

## Verify

`menu_preview` renders the whole feature from canned replies delivered through the seam's real
`deliver` — `menu_load_list`, `menu_load_selected`, `menu_load_delete_confirm`, `menu_load_in_run`,
`menu_save`, `menu_save_overwrite`, `menu_save_reserved_name`, `menu_load_empty`,
`menu_load_no_server` and `config_drift`. The harness IS the transport, which is what makes the
failure states renderable at all: none of them is reachable from a healthy stack. It also carries
`_assert_text_focus_is_handed_back`, which takes no PNG. Details in
`.claude/rules/client/harness-menu-workbench.md`.
