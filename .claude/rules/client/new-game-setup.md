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
> the world they land on is the same by default. The row says so without a sentence: with no answer to
> offer, the readout reads `None`, which is the count the absent argument lands on.

**So "the count shown is the count sent" is load-bearing, not incidental.** A player who never
touches the slider still sends the number the row opened on — `_on_capacity_changed` seeds
`_rival_count` from the answer and nothing downstream re-derives it — and if that chain broke, the
screen would promise two rivals and hand over an empty world, looking entirely normal doing it.
`menu_preview` pins both ends: the shell's `new_game_requested` payload, and `Main.new_game_line`,
which is `static` precisely so the rule that decides between a trailing count and none is reachable
without standing a client up.

## The control shows only what it has been told

A slider exists **only** when the answer landed and its ceiling is at least one. Every other state
shows **the readout alone, reading `None`** — the count the world will actually be built with — in the
same column the slider's readout occupies, so the row always states a number and never invents a range
it cannot honour.

- **A 0 ceiling is not a failure.** A grid with no room for a second start reads as *"you will be
  alone in the world"* under the `None`, and the pane sends an explicit `0` — the count it just told
  the player they are getting.
- **A FAILED ASK GETS NO CAPTION AT ALL.** There were two sentences under this row — one for a server
  that answered without a count, one for a server that never answered — and both are gone. The row's
  question is *how many others?*, its answer is *none*, and the readout says that in one word; a
  paragraph about capacity queries under a slider explains the client's plumbing to someone who has
  no model of it.
- **THE REASON MOVED TO THE RAIL; IT WAS NOT DELETED.** Removing the caption and stopping there left
  a greyed-out "Begin the trail" with nothing anywhere saying why, which is worse than the caption
  that was wrong. So an unreachable server raises the shell's one-line notice above the nav —
  `MenuShell.NOTICE_NO_SERVER`, *"Unable to connect to the server. Please try restarting the game."*
  One explanation, in one place, in the player's terms; the row stays silent and the box says the
  thing the button cannot.
- **`Begin the trail` is DISABLED for one state only: nothing is listening.** With no server there is
  nothing to send `new_game` to — the press used to swap to a `Main` that sat on a black loading
  screen forever. `FactionCapacity.server_is_unreachable` is the test (the `transport` token, and only
  it), so a server that answered without a count still starts a game with the argument omitted.
  `Preview map` stays live throughout: that pane is the client's own preset list and asks nothing.
- **A merely PENDING ask disables nothing and says nothing.** That is the normal case for a moment at
  every startup, and a primary action — or a notice — that blinked on every open would be worse than
  the bug this fixed. `MenuShell._server_unreachable` is the latch that makes the two
  distinguishable: a transport failure sets it, any answer FROM a server clears it, and a `PENDING`
  seam leaves it alone — which is also what stops the retry below flickering the state it is retrying.
  **The button, the notice and the retry clock all hang off that one latch**, so they appear and
  disappear together and a stale "cannot connect" cannot sit over a working screen.
- **ONE BOX, NEVER TWO.** A session bounced back from a failed seat claim arrives with the SAME
  sentence already handed in (`set_notice`, from `Main`), and its capacity ask then fails too.
  `_notice_line` prefers the handed-in text and falls back to the latch, so the two paths render one
  box; and because both carry `NOTICE_NO_SERVER`, an answer retracts the handed-in copy as well
  (`_note_server_reachability`). A refusal whose sentence is a DIFFERENT fact — a seat held by another
  player — is left standing, since a reachable server does not make it untrue.
- **An unreachable server heals itself, on a clock.** `RIVALS_RETRY_SECONDS` (3 s) re-asks while the
  latch holds AND the setup pane is up, and nothing else runs it. A blocking state has to clear
  without the player finding the one control that re-asks; a refused TCP connect on localhost returns
  immediately, so the interval is the whole cost. Re-entering the pane still retries, as the saves
  panes' "Try again" does, and a rebuild still never does — that would spin the socket once per redraw.
- **The summary keeps `none asked for`**, distinct from the explicit `none`, because the REQUEST is
  different even though both worlds end up with no rivals.

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
- **Both shapes are the same height.** The no-slider state puts its readout in a row of
  `RIVALS_CONTROL_ROW_HEIGHT`, so even a real shape change moves nothing below the row — and the
  caption reserves one line of its own font's height even when a failed ask leaves it EMPTY, since a
  zero-height Label would raise everything below it the moment an ask failed.
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
| `FactionCapacity.gd` (`class_name FactionCapacity`) | The `faction_capacity` seam, modelled on `SaveSlots`: `set_sender`/`deliver`, `request(width, height)` (a repeat of an answered grid is dropped), `retry`, the four states, `permits`/`clamp_count`, `NO_COUNT` — "never offered a choice", which is not a number — and **`server_is_unreachable()`**, the one place the `transport` token is told apart from a refusal a server sent, since this seam owns the token vocabulary. **Owns no socket** |
| `QueryRequestIds.gd` (`class_name QueryRequestIds`) | The one request-id allocator for every seam on the native query worker: `REQUEST_ID_BASE` (clear of `ForecastQuery`), `IDS_PER_SESSION`, `reserve_block()` |

## Verify

`menu_preview` renders the row through the seam's real `deliver`, from canned replies —
`menu_new_game_rivals_pending`, `menu_new_game_rivals`, `menu_new_game_rivals_picked`,
`menu_new_game_rivals_alone`, `menu_new_game_rivals_unavailable` (a server answered, no count),
`menu_new_game_rivals_no_server` (nothing answered), `menu_new_game_rivals_recovered` (it came back),
`menu_landing_seat_refused` (the rail notice), and the map-size click itself in
`menu_new_game_rivals_reask` / `_reasked` — and asserts what a frame cannot show: that no slider is
offered without a ceiling, that an unanswered ask reads `None` and adds no sentence, that
`Begin the trail` is offered in every state but the unreachable one and withheld in that one, that the
rail notice is up in that state and GONE once a server answers — including the copy a bounced session
handed in — that the retry does not change what is on screen, that the resolved count on the wire
matches the pick, that
the control across a re-ask is the SAME NODE at the SAME RECT with the row's height and the summary
unchanged, that the row is the same height with and without a slider, and that the capacity seam's ids
are disjoint from the save seam's. Details in `.claude/rules/client/harness-menu-workbench.md`.
