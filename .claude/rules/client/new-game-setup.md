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

# New Game setup: how many peoples of each kind, and who decides

The New Game pane's world settings were a pure function of what the client already knew — a preset,
a size from `MapSizes`, a seed. **The peoples counts are the first that are not**, and everything
below follows from that.

## The player counts OTHERS, one row per start archetype, and the sim owns the ceilings

Each control is **a count of peoples of one start archetype**: pick 2 of the first and 1 of the second
and the world holds four peoples, theirs and three others. Not a faction count, not a roster size — the
player's own people (always the profile's first archetype) are never a number they choose.

**THE ROWS ARE THE SERVER'S, NOT THE UI's.** The `faction_capacity` reply carries
`archetypes: [{id, display_label, site_count, default_count}]` in catalog order, and `MenuShell`
renders one slider per row under the row's `display_label` — it names no archetype of its own (the
shipped catalog is `major` "Rivals" and `minor` "Minor peoples", but a third archetype in
`start_profiles.json` appears here with no client change). The only copy it keeps is `_labels`,
remembered across a re-ask so the summary does not flip to bare ids while a new answer is in flight.

How many peoples a map can seat is a property of the **grid**, not of the running world: it is how
many starts fit at each archetype's `min_separation` (`core_sim`'s `archetype_capacities`, one
implementation, `.claude/rules/core_sim/` owns the rule). The client therefore **asks** — the
`faction_capacity` query, answered while the server is idle, for the width, height and start profile the
player has actually picked — and never restates the seating arithmetic in GDScript. A size click is a
NEW question, because it is a question about the map being made, and so is a change of profile
(`FactionCapacity.request` carries `profile_id`; a repeat of an answered grid AND profile is dropped).

**THE ONE FORMULA THE CLIENT DOES HOLD** is the ceiling contract on `ArchetypeCapacity` in
`command.proto`: `ceiling_k = max(0, site_count_k − 1 − Σ_{j<k} count_j)` — the 1 is the human seat and
`count_j` the CURRENT counts of the EARLIER rows (`FactionCapacity.ceiling_at`, with
`permits`/`clamp_count` per row and `clamp_all` walking the set in order). It is a contract and not a
restatement of the rule, because the server applies the same formula to the earlier GRANTED counts: a
client that stays inside it is granted exactly what it asked. **Moving an earlier row re-clamps every
later one** — the later sliders' bound, value, readout and the count that would be SENT all follow, in
place (`_on_rival_slider_changed` → `_sync_rival_widgets`).

## Omitting the argument is a REQUEST, not a fallback

`new_game <preset> <w> <h> <seed> <profile> [<archetype_id>=<count> ...]` takes the counts as its optional tail (one token per archetype),
and the three states are genuinely three:

| The pane | On the wire | What the server does |
|---|---|---|
| The player picked *n* of `major` and *m* of `minor` | `… <profile> major=n minor=m` | seats that many of each, clamped in catalog order to what the grid holds |
| The player picked none | `… <profile> major=0 minor=0` | the player is alone — an explicit choice |
| No answer to offer a choice from | `… <profile>` | each archetype's **unattended count**: its own `default_count` pin in `start_profiles.json`, else 0 |

**An empty `archetype_counts` array is that third state, and it travels the whole way:** the shell emits
`[]` on `new_game_requested` (the signal carries an Array of `{id, count}` in catalog order),
`GameLaunch.pending_new_game["archetype_counts"]` carries it, and `Main.new_game_line` appends no token
for it. `Main._sanitized_archetype_counts` also drops an entry whose id is empty or holds a space or `=`
(each would corrupt its neighbour's token) and one whose count is negative (`FactionCapacity.NO_COUNT`,
"not offered a choice"). **A guessed number would be worse than no number** — it would either refuse a
count the map could seat or ask for one it could not — and a capacity ask that failed must never stop
a player starting a game.

> ⛔ **AN ABSENT COUNT MEANS THE UNATTENDED COUNT — usually zero — NOT THE NUMBER THE SCREEN OFFERS.**
> It resolves to `worldgen::unattended_archetype_counts`, the roster a `cargo run` server, a test
> harness or an unattended `new_game` comes up with, deliberately split from the map-scaled
> `default_count` the New Game screen pre-selects. The two requests are different (an explicit `0` names
> the count whatever the profile pins), though the world they land on is the same by default. The pane
> says so without a sentence: with no answer to offer, the one readout reads `None`, which is the count
> the absent tokens land on.

**So "the counts shown are the counts sent" is load-bearing, not incidental — per archetype.** A player
who never touches a slider still sends the numbers the rows opened on — `_on_capacity_changed` →
`_seed_counts_from_answer` seeds `_counts` from the answer (an unpicked row follows the server's
`default_count`, a picked one keeps its pick; the set is then re-clamped in catalog order) and nothing
downstream re-derives it — and if that chain broke, the screen would promise two of each and hand over an
empty world, looking entirely normal doing it. `menu_preview` pins both ends, row by row: the shell's
`new_game_requested` payload, and `Main.new_game_line`, which is `static` precisely so the rule that
decides between trailing tokens and none is reachable without standing a client up.

## The control shows only what it has been told

Sliders exist **only** once an answer has landed, one per archetype row. Every other state shows
**ONE row — a generic "Other peoples" heading and the readout alone, reading `None`** — the count the
world will actually be built with — in the same column a slider's readout occupies, so the pane always
states a number and never invents a range or a kind it cannot honour.

- **A 0 ceiling is not a failure, and its slider stays.** A row the grid cannot seat anyone for keeps
  its slider, disabled at `0..0` (`editable = false`), rather than disappearing — a later row's ceiling
  moves as an earlier row is dragged, and rebuilding that control under the player's hand would drop the
  drag. When NO row can seat anyone the caption reads *"you will be alone in the world"*, and the pane
  sends an explicit `0` per row — the count it just told the player they are getting.
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
smaller map's ceiling through `clamp_all`, and a row the player has NOT touched follows the server's
default on every answer (`_picked`, keyed by archetype id, is what tells those two apart).

## A RE-ASK IS NOT A STATE CHANGE

`_on_capacity_changed` touches only the peoples rows. The setup pane also holds the seed `LineEdit`, and
rebuilding a text field under a player mid-word is the caret defect the Save pane already paid for
(`.claude/rules/client/save-load-menu.md`); nothing else in the pane depends on the answer.

**That is not enough on its own, and the first implementation proved it.** Clicking through the map
sizes re-asks on every click, and the row freed and recreated its children on every emit: control
gone → pending caption in → control back, twice per click. Because the caption is shorter than the
slider row, the seed field, the summary and the actions row all moved 22px each way. It read as a
flash, and it was reported from a playtest. So the row obeys three rules, and they are about the
transitions rather than the states:

- **A re-ask over already-answered rows changes nothing.** `_refresh_rivals_row` returns
  immediately while the seam is `PENDING` and the rows have rendered an answer before
  (`_rivals_answered_once`). Briefly-stale bounds beat a control that vanishes and returns.
- **An answer that names the same archetypes updates the EXISTING nodes** — `max_value`, the value
  (through `set_value_no_signal`, since this is not the player moving the control), the readout, the
  caption. `_rebuild_rivals_rows` is the only path that frees anything, and only a genuine shape change
  (`_rivals_shape`, the joined archetype ids: unanswered → answered, or a different roster) reaches it.
- **Every shape holds its height.** The unanswered row puts its readout in a row of
  `RIVALS_CONTROL_ROW_HEIGHT`, and the caption reserves one line of its own font's height even when a
  failed ask leaves it EMPTY, since a zero-height Label would raise everything below it the moment an
  ask failed.

**The pick survives the flight, too.** `_resolved_archetype_counts` returns the player's counts while a
re-ask is in flight over answered rows, rather than collapsing to `[]` — otherwise the summary reads
"none asked for" for the few milliseconds after every click, and a Begin pressed in that window would
silently discard the picks. The server clamps a count the new grid cannot seat; it
cannot recover one this screen threw away.

## THE SEED IS A STRING, FROM THE KEYSTROKE TO THE SOCKET

A seed is a **u64**. The `new_game` line is text and the server parses that field as a u64, so there
is no number to round-trip through — and every hop that made one lost seeds:

| Hop | Carries | Why not a number |
|---|---|---|
| `MenuShell._seed_edit` | up to `SEED_MAX_LENGTH` = **20** characters | 20 is a u64's full decimal width |
| `new_game_requested`'s `seed` | `String` | a GDScript `int` is **signed** 64-bit |
| `GameLaunch.pending_new_game` / `active_new_game` `"seed"` | `String` | same, and it survives a scene reload |
| `Main.new_game_line`'s `seed_text` | `String`, substituted with `%s` | `%d` would cap it |

**Both halves of that were reported from one playtest.** The field capped at 12 characters, so the
19-digit seed the client itself had minted came back as `618699468282` and generated a different
world. **Raising the cap alone would have been worse than the bug**: a GDScript `int` stops at
`9223372036854775807` and a u64 runs to `18446744073709551615`, so roughly half of all clock-derived
seeds — `16811688588392450970` was the same session's second seed — would have been accepted as
typed, silently changed, and never reported as changed. The string carries the whole range and the
summary row therefore states the digits that go on the wire verbatim.

**The affordance refuses what the server could not parse, because the alternative is a dead client.**
A `new_game` the server cannot parse is never answered, and the client sits on the loading overlay
with nothing to press. So `MenuShell.seed_error` is static and pure — the `SaveSlots.slot_name_error`
shape, one rule asked by the field, by `Main`'s wire boundary and by the harness — and it drives the
caption under the field and the `Begin the trail` gate together. Two refusals, and both are short
declaratives: **digits only** (a u64 has no sign, no separators and no `0x`, so a whitelist of the
ten digits is the whole rule) and **the largest seed**, named. An EMPTY field is not a refusal; it is
`SEED_UNSEEDED` (`"0"`), which keeps its old meaning of *derive from the run clock*.

`Begin the trail` now has **two** reasons to lock — nothing listening, and a seed that cannot be
sent — and they are the same kind of reason: each would produce a run that never starts. Every other
unanswered state still starts a game.

> ⛔ **THE COMPARISON AGAINST u64 MAX IS MADE ON TEXT.** `SEED_MAX_TEXT` is itself above a GDScript
> `int`'s ceiling, so `int(typed) > int(SEED_MAX_TEXT)` compares two values that both saturated at
> `9223372036854775807` and answers **false for every seed in the top half of the range** — the exact
> seeds the string carriage exists for. `_exceeds_seed_max` drops leading zeros and compares lengths,
> then the equal-length strings lexicographically, which for digits is numeric order.

**The caption is re-worded, never rebuilt.** It runs on every keystroke and the field beside it holds
the caret, which is the Save pane's caret defect one pane over (`save-load-menu.md`). `_add_note`
returns its Label so this one caller can hold it; the rest ignore the return.

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
| `FactionCapacity.gd` (`class_name FactionCapacity`) | The `faction_capacity` seam, modelled on `SaveSlots`: `set_sender`/`deliver`, `request(width, height, profile_id)` (a repeat of an answered grid AND profile is dropped), `retry`, the four states, the parsed `archetypes` rows, the contract formula (`ceiling_at`) with per-row `permits`/`clamp_count`/`clamp_all`/`default_at`/`default_counts`, `NO_COUNT` — "never offered a choice", which is not a number — and **`server_is_unreachable()`**, the one place the `transport` token is told apart from a refusal a server sent, since this seam owns the token vocabulary. **Owns no socket** |
| `QueryRequestIds.gd` (`class_name QueryRequestIds`) | The one request-id allocator for every seam on the native query worker: `REQUEST_ID_BASE` (clear of `ForecastQuery`), `IDS_PER_SESSION`, `reserve_block()` |

## Verify

`menu_preview` renders the rows (two archetypes, `major` and `minor`) through the seam's real `deliver`, from canned replies —
`menu_new_game_rivals_pending`, `menu_new_game_rivals`, `menu_new_game_rivals_picked`,
`menu_new_game_rivals_alone`, `menu_new_game_rivals_unavailable` (a server answered, no count),
`menu_new_game_rivals_no_server` (nothing answered), `menu_new_game_rivals_recovered` (it came back),
`menu_landing_seat_refused` (the rail notice), and the map-size click itself in
`menu_new_game_rivals_reask` / `_reasked`, plus `menu_new_game_archetypes_reclamped` (an earlier row
raised so a later row's ceiling falls under its pick) — and asserts what a frame cannot show: that no
slider is offered before an answer names the rows, that each row is headed by the server's label, that a
0-ceiling row is a disabled slider, that an unanswered ask reads `None` and adds no sentence, that
`Begin the trail` is offered in every state but the unreachable one and withheld in that one, that the
rail notice is up in that state and GONE once a server answers — including the copy a bounced session
handed in — that the retry does not change what is on screen, that the resolved counts on the wire
match the picks, per archetype, that moving an earlier row re-clamps a later one in place (the same
slider node, and the wire agreeing with the screen), that
the control across a re-ask is the SAME NODE at the SAME RECT with the row's height and the summary
labels unchanged, that the rows are the same height with a ceiling and without one, and that the
capacity seam's ids are disjoint from the save seam's.

**The seed has its own block** (`_run_seed_states`, with one still, `menu_new_game_seed_refused`): both
playtest seeds are typed into the real field and asserted to reach the composed `new_game` line digit
for digit, an empty field composes `0`, and a seed above u64 max and one holding a non-digit are each
refused with a caption and a locked `Begin the trail` and emit nothing. Details in
`.claude/rules/client/harness-menu-workbench.md`.
