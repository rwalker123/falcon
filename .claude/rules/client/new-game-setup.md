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
| No answer to offer a choice from | `… <profile>` | its **unattended roster**: no rivals, unless `simulation_config.json` pins `default_ai_faction_count` |

`FactionCapacity.NO_COUNT` (`-1`) is that third state, and it travels the whole way: the shell emits
it on `new_game_requested`, `GameLaunch.pending_new_game` carries it, and `Main.new_game_line`
appends nothing for it. **A guessed number would be worse than no number** — it would either refuse a
count the map could seat or ask for one it could not — and a capacity ask that failed must never stop
a player starting a game.

> ⛔ **AN ABSENT COUNT MEANS ZERO RIVALS, and it did not always.** It used to resolve to the server's
> configured `default_ai_faction_count`; it now resolves to `worldgen::unattended_ai_faction_count`,
> which is **none** — the roster a `cargo run` server, a test harness or an unattended `new_game`
> comes up with, deliberately split from the map-scaled number the New Game screen pre-selects. The
> two requests are still different (an explicit `0` names the count whatever the config says), but
> the world they land on is the same by default. Every sentence the client writes about the absent
> case has to say *no rivals*, and the failure caption does.

**So "the count shown is the count sent" is load-bearing, not incidental.** A player who never
touches the slider still sends the number the row opened on — `_on_capacity_changed` seeds
`_rival_count` from the answer and nothing downstream re-derives it — and if that chain broke, the
screen would promise two rivals and hand over an empty world, looking entirely normal doing it.
`menu_preview` pins both ends: the shell's `new_game_requested` payload, and `Main.new_game_line`,
which is `static` precisely so the rule that decides between a trailing count and none is reachable
without standing a client up.

## The control shows only what it has been told

A slider exists **only** when the answer landed and its ceiling is at least one. Pending, failed and
a genuine 0 ceiling each get their own caption instead, in the `MenuShell` idiom the Theme row set:
a caption is always on screen, never a tooltip, because the thing the control cannot show is why it
is offering what it is.

- **A 0 ceiling is not a failure.** A grid with no room for a second start reads as *"you will be
  alone in the world"*, and the pane sends an explicit `0` — the count it just told the player they
  are getting.
- **A failed ask is not a dead end, but it has a consequence and the caption names it**: no count is
  sent, so the world is built with **no rivals in it**. `Begin the trail` stays live — the player is
  never blocked — and the summary reads `none asked for`, which stays distinct from the explicit
  `none` because the request is.
- **Re-entering the pane is the retry**, exactly as the saves panes' "Try again" button is: a
  rebuild-driven retry would spin the socket for as long as the screen is open.

**The pick survives a size change; the ceiling clamps it.** A count chosen on a roomy map meets the
smaller map's ceiling through `clamp_count`, and a player who has NOT touched the control follows the
server's default on every answer (`_rival_picked` is what tells those two apart).

## A RE-ASK IS NOT A STATE CHANGE

`_on_capacity_changed` touches only the rival row. The setup pane also holds the seed `LineEdit`, and
rebuilding a text field under a player mid-word is the caret defect the Save pane already paid for
(`.claude/rules/client/save-load-menu.md`); nothing else in the pane depends on the answer.

**That is not enough on its own, and the first implementation proved it.** Clicking through the map
sizes re-asks on every click, and the row freed and recreated its children on every emit: control
gone → pending caption in → control back, twice per click. Because the caption is shorter than the
slider row, the seed field, the summary and the actions row all moved 22px each way. It read as a
flash, and it was reported from a playtest. So the row obeys three rules, and they are about the
transitions rather than the states:

- **A re-ask over an already-answered row changes nothing.** `_refresh_rivals_row` returns
  immediately while the seam is `PENDING` and the row has rendered an answer before
  (`_rivals_answered_once`). Briefly-stale bounds beat a control that vanishes and returns.
- **An answer that keeps the shape updates the EXISTING nodes** — `max_value`, the value (through
  `set_value_no_signal`, since this is not the player moving the control), the readout, the caption.
  `_rebuild_rivals_row` is the only path that frees anything, and only a genuine shape change
  (no-slider → slider, or the reverse) reaches it.
- **Both shapes are the same height.** The caption-only states put a `RIVALS_CONTROL_ROW_HEIGHT`
  spacer where the control would be, so even a real shape change moves nothing below the row.
  Measured across all seven rendered states: the caption sits at the same y, and so do the seed field
  and the summary.

**The pick survives the flight, too.** `_resolved_rival_count` returns the player's count while a
re-ask is in flight over an answered row, rather than collapsing to `NO_COUNT` — otherwise the
summary reads "server default" for the few milliseconds after every click, and a Begin pressed in
that window would silently discard the pick. The server clamps a count the new grid cannot seat; it
cannot recover one this screen threw away.

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
`menu_new_game_rivals_alone`, `menu_new_game_rivals_unavailable`, and the map-size click itself in
`menu_new_game_rivals_reask` / `_reasked` — and asserts what a frame cannot show: that no slider is
offered without a ceiling, that a failed ask still offers `Begin the trail`, that the resolved count
on the wire matches the pick, that the control across a re-ask is the SAME NODE at the SAME RECT with
the row's height and the summary unchanged, that the row is the same height with and without a
slider, and that the capacity seam's ids are disjoint from the save seam's. Details in `.claude/rules/client/harness-menu-workbench.md`.
