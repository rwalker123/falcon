---
paths:
  - "clients/godot_thin_client/src/scripts/FactionCapacity.gd"
  - "clients/godot_thin_client/src/scripts/QueryRequestIds.gd"
  # The setup pane owns the control; the landing screen owns the seam behind it; `Main` turns the
  # pick into the one optional argument on the `new_game` line, and `GameLaunch` carries it between
  # the two scenes. All four can break the contract below, so all four load this file.
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/src/scripts/ui/LandingScreen.gd"
  - "clients/godot_thin_client/src/scripts/GameLaunch.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
---

# New Game setup: how many rivals, and who decides

The New Game pane's world settings were a pure function of what the client already knew — a preset,
a size from `MapSizes`, a seed. **The rival count is the first one that is not**, and everything
below follows from that.

## The player counts OTHERS, and the sim owns the ceiling

The control is **a count of rival peoples**: pick 2 and the world holds three peoples, theirs and two
others. Not a faction count, not a roster size — the player's own people are never a number they
choose.

How many peoples a map can seat is a property of the **grid**, not of the running world: it is how
many starts fit at `faction_start_min_separation` (`core_sim`'s `faction_start_capacity`, one
implementation, `.claude/rules/core_sim/` owns the rule). The client therefore **asks** — the
`faction_capacity` query, answered while the server is idle, for the width and height the player has
actually picked — and never restates the arithmetic in GDScript. A size click is a NEW question,
because it is a question about the map being made.

## Omitting the argument is a REQUEST, not a fallback

`new_game <preset> <w> <h> <seed> <profile> [rivals]` takes the count as its one optional argument,
and the three states are genuinely three:

| The pane | On the wire | What the server does |
|---|---|---|
| The player picked *n* | `… <profile> n` | seats *n* rivals, clamped to what the grid holds |
| The player picked none | `… <profile> 0` | the player is alone — an explicit choice |
| No answer to offer a choice from | `… <profile>` | uses its configured `default_ai_faction_count` |

`FactionCapacity.NO_COUNT` (`-1`) is that third state, and it travels the whole way: the shell emits
it on `new_game_requested`, `GameLaunch.pending_new_game` carries it, and `Main._build_world_request`
appends nothing for it. **A guessed number would be worse than no number** — it would either refuse a
count the map could seat or ask for one it could not — and a capacity ask that failed must never stop
a player starting a game.

## The control shows only what it has been told

A slider exists **only** when the answer landed and its ceiling is at least one. Pending, failed and
a genuine 0 ceiling each get their own caption instead, in the `MenuShell` idiom the Theme row set:
a caption is always on screen, never a tooltip, because the thing the control cannot show is why it
is offering what it is.

- **A 0 ceiling is not a failure.** A grid with no room for a second start reads as *"you will be
  alone in the world"*, and the pane sends an explicit `0` — the count it just told the player they
  are getting.
- **A failed ask is not a dead end.** The caption says the world will be built with the server's own
  rival count, and `Begin the trail` stays live.
- **Re-entering the pane is the retry**, exactly as the saves panes' "Try again" button is: a
  rebuild-driven retry would spin the socket for as long as the screen is open.

**The pick survives a size change; the ceiling clamps it.** A count chosen on a roomy map meets the
smaller map's ceiling through `clamp_count`, and a player who has NOT touched the control follows the
server's default on every answer (`_rival_picked` is what tells those two apart).

## The answer re-derives the ROW, never the pane

`_on_capacity_changed` rebuilds only `_rivals_box`. The setup pane also holds the seed `LineEdit`,
and rebuilding a text field under a player mid-word is the caret defect the Save pane already paid
for (`.claude/rules/client/save-load-menu.md`). Nothing else in the pane depends on the answer.

## One allocator for every query seam

`QueryRequestIds` holds the request-id floor and the per-instance block that `SaveSlots` used to hold
alone, because **two seams keeping their own counters can hand out the same block**. `LandingScreen`
builds a `SaveSlots` and a `FactionCapacity` in one `_ready`, and `Time.get_ticks_usec()` can return
the same microsecond for both; the allocator's tie-break count is what covers that case, and it only
does so while both seams draw from the SAME counter. The consequence of a collision is not a lost
answer but a *wrong* one: both seams are fed from the same destructive drain and route by id, so a
`faction_capacity` reply carrying a `save_op`'s id finishes that op. The failure mode in full, and
the load once reported as a success, are in `save-load-menu.md`.

## Key scripts

| Script | Purpose |
|--------|---------|
| `FactionCapacity.gd` (`class_name FactionCapacity`) | The `faction_capacity` seam, modelled on `SaveSlots`: `set_sender`/`deliver`, `request(width, height)` (a repeat of an answered grid is dropped), `retry`, the four states, `permits`/`clamp_count`, and `NO_COUNT` — "never offered a choice", which is not a number. **Owns no socket** |
| `QueryRequestIds.gd` (`class_name QueryRequestIds`) | The one request-id allocator for every seam on the native query worker: `REQUEST_ID_BASE` (clear of `ForecastQuery`), `IDS_PER_SESSION`, `reserve_block()` |

## Verify

`menu_preview` renders the row through the seam's real `deliver`, from canned replies —
`menu_new_game_rivals_pending`, `menu_new_game_rivals`, `menu_new_game_rivals_picked`,
`menu_new_game_rivals_alone`, `menu_new_game_rivals_unavailable` — and asserts what a frame cannot
show: that no slider is offered without a ceiling, that a failed ask still offers `Begin the trail`,
that the resolved count on the wire matches the pick, and that the capacity seam's ids are disjoint
from the save seam's. Details in `.claude/rules/client/harness-menu-workbench.md`.
