---
paths:
  - "core_sim/src/network.rs"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/GameLaunch.gd"
  - "clients/godot_thin_client/src/scripts/Hud.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/TopBarReadouts.gd"
  - "clients/godot_thin_client/src/scripts/ui/{TellingPanel,AnnotationRenderer,BandOverlayRenderer}.gd"
---

# World handoff — which world is this frame from?

The server can hold a succession of worlds in one process (`new_game`, `map_size`/ResetMap), and a
client can connect at any point in that succession. This file is the contract that decides **which
world a given snapshot frame belongs to, and what the client is allowed to do before it knows**. It
spans both halves because either half can break it alone.

## The rule

**A newly connected client is sent NOTHING until the next broadcast.** The server does not offer a
connecting client the last frame it happens to have, because that frame belongs to whatever world
existed *at accept time*, which is not necessarily the world the connecting client is about to ask
for — and the client cannot tell the two apart.

The client, correspondingly:

1. holds the **loading overlay** until a FULL snapshot arrives whose `world_epoch` exceeds the
   baseline it captured at `_ready` (`GameLaunch.last_world_epoch`, persisted across `Main.tscn`
   reloads);
2. **keeps re-sending `new_game` until a world reveals** — the request is retried until *answered*,
   not until *sent*;
3. **clears its per-world caches whenever the applied `world_epoch` changes**, because a full
   snapshot restates only what the new world *has*, never what it lacks.

## Why the epoch gate cannot do this alone

`world_epoch` is a monotonic **server-side** counter (`rebuild_world_from_config` bumps it, the
capture stamps it into every header). A client can compare it against a world it has already
revealed — that is what makes the in-session `Abandon Run → New Game` path correct — but on the
**first launch of a client process** the baseline is `0`, and every epoch a live server can produce
beats `0`. The gate is structurally unable to recognise a stale frame there.

That is not hypothetical. With the connect-time replay still in place, a fresh client process
against a server holding a previous world reveals **the previous world**:

```
Main._ready baseline_epoch=0
reveal_candidate epoch=2 baseline=0 turn=6  -> REVEALING     <- the OLD world
apply epoch=2 turn=6 ...                                     <- old map, old HUD state
apply epoch=3 turn=1 ...                                     <- the world we asked for, ~1s later
```

The precondition is simply **the server outliving the client process** — a separately started
server, repeated `--client-only` runs, or launching from the Godot editor.

## Removing the replay is the fix; the alternatives all still race

Every cheaper-looking option was worked through and fails on the same window — **a frame that
arrives after our request but was produced before it**:

| Rejected | Why |
|---|---|
| "Tell the server to reset" | It already does. `new_game` throws the whole Bevy app away and builds a new one; the second world comes back at turn 1 with an empty knowledge ledger. **No server state leaks** — the stale data is a frame the server *hands out*, not state it *kept*. |
| Clear the cached frame when a rebuild begins | Only covers accept-during-rebuild. The observed failure is accept-**before** the command is dequeued, where the cache is still legitimately current. |
| Connect the stream only after sending `new_game` | The replay fires at accept time, which is after the send and may still precede the rebuild. |
| Treat the first frame on a connection as the baseline | Hangs forever on the normal case (idle server, first frame IS the world we asked for). |
| A client-chosen token echoed in the snapshot header | Race-free, and the only thing that would work *with* the replay — but it needs a command field, a schema field, decoder work and a golden re-record to buy what deleting 14 lines buys. Reach for it only if a future client genuinely needs to attach to a world it did not create. |

**Nothing else consumed the replay.** The Godot client always sends `new_game` on connect, so it
never wants the previous world; no test or tool connects to those sockets. Attaching a client to a
running game is not a flow that exists today — if it ever does, it needs the token above, not the
replay back.

## The dropped-first-frame race, and why the retry is the answer

Deleting the replay exposes a pre-existing hole on the accept path: a frame published while the
connection is still sitting in the TCP backlog is broadcast **before** the socket joins the client
list, and that client never gets it. The replay used to paper over this by accident.

The topology has since changed — accept and broadcast are separate threads handing sockets over a
channel (`snapshot-socket.md`) — and the race survives it unchanged, now with the handoff as one more
place a frame can overtake a new client.

It cannot be closed on the server: the backlog means a connection exists for the OS before the
server knows about it, so *some* frame can always be broadcast into the gap. So the client closes it
instead — `NEW_GAME_ANSWER_TIMEOUT` re-sends `new_game` if no world has revealed, and the retry is
**unbounded on purpose**: a stuck loading screen is unrecoverable for the player, while a redundant
`new_game` merely builds another fresh world.

**Size that timeout off the measurement, not off patience.** Its two failure modes are not
symmetric. Firing early interrupts a *healthy* generation, costing the player a different world than
the one already being built (a `seed 0` re-roll) plus a second worldgen — on every large-map start on
a slow machine. Firing late only means a rare dropped frame self-heals slowly. So it is set at ~7x
the measured worst case: `new_game.begin`→`new_game.completed` for the largest offered map (Huge,
128x80) is **4.4s in a debug build**, which is what the client runs. A snug fit here is a bug.

## The client-side half: empty is not "unchanged" across a world boundary

Even with a perfect gate, the reset in (3) above is still required, and it fixes a second bug the
gate never touched. "A field absent from a delta means *unchanged*" is correct **within** one world
and wrong **across** worlds, and several surfaces merge rather than replace:

- **`TopBarReadouts._ingest_intensification`** iterates the payload. A fresh world sends
  `intensification_knowledge: []`, the loop body never runs, and `Herding ✓` from the previous game
  renders forever.
- **`TellingPanel`** is deliberately never reset on a full snapshot (its de-dup makes re-ingesting
  the `commandEvents` ring harmless, and resetting would drop scrolled-off history). Right within a
  world; wrong across one — a world change is not a new snapshot of the same story.
- **`MapView.herd_trails`** is keyed by herd id and only erases ids absent from the current
  snapshot, so a repeated id in the new world appends to the old world's path.

**`map_size`/ResetMap rebuilds the world with no scene reload**, so this half is not merely
belt-and-braces for the launch case — it is the only thing making an in-session world change
correct. Key the reset on the **applied `world_epoch` changing**, never on scene construction.

### As built: `_reset_per_world_state`, and how to decide what belongs in it

`Main._apply_snapshot` compares a **full** snapshot's `world_epoch` against `_world_epoch_applied` and,
on a change, calls `_reset_per_world_state()` **before any dispatch**, then stores the new value. That
order is the contract: the same snapshot carries the new world's backfill (the `commandEvents` ring,
the knowledge rows, the herd list), so resetting *after* the dispatch would wipe what just landed.

`Main` decides only WHEN. Each surface owns its own reset, reached by the usual silent `has_method`
probe — `hud.reset_world_state()` (a **coordinator delegator**; `HudLayer` grows no feature logic, it
calls `TopBarReadouts.reset_world_state` / `TellingPanel.reset` / `cancel_active_targeting`) and
`map_view.reset_world_state()` (which fans out to `AnnotationRenderer` / `BandOverlayRenderer`). `Main`
also clears `_campaign_label_signature` / `_victory_analytics_signature`, which print once per
*distinct* value and would otherwise stay silent in a new world that happened to match the old one.

**The test for a cache is the shape, not a list**: it needs clearing iff it does *not* rebuild wholesale
from each snapshot. Three shapes qualify — it **merges** (the knowledge strip above); it is **keyed by
an id and erases only on ABSENCE** (`herd_trails`, `culture_layer_map`); or it was **pushed IN from
another surface** keyed by an id the new world reuses (`BandOverlayRenderer._labor_pending` from the
HUD, `AnnotationRenderer._selected_trade_entity` from the Trade tab, MapView's culture highlight from
the Culture tab, and the selection triplet `selected_unit_id` / `selected_herd_id` / `selected_tile` +
`cycle_index`). Everything `display_snapshot` clears-and-refills — the `tile_*` lookups, `food_sites`,
`discovered_sites`, `forage_patch_lookup`, `harvest_sites`/`scout_sites`, `units`/`herds`, the overlay
channels, every `TerrainRenderer` raster, `SecondaryMarkerRenderer`'s per-frame slots — heals itself and
is deliberately **absent** from the reset, as are the genuine view PREFERENCES (`active_overlay_key`,
the trade-overlay toggle, the terrain-highlight id, the texture/grid toggles).

Two asymmetries worth keeping:

- **The selection is cleared silently, not through `selection_cleared`.** `_apply_snapshot` ends in
  `_refresh_hud_selection`, which reads the cleared MapView state and drops the HUD card the same
  frame — one path, no double-clear.
- **Targeting is cancelled from the HUD side only.** `HudLayer.reset_world_state` calls
  `cancel_active_targeting()`, whose normal path pushes `{}` down to `AnnotationRenderer._targeting`;
  clearing the renderer's mirror as well would let the banner and the reticle desync.

**Verify with `ui_preview`'s `world_reset` state**, which is a behavioural guard rather than a picture:
it seeds a knowledge strip and a book of beats, asserts both are present, calls
`Hud.reset_world_state()`, then asserts the strip is hidden and the Telling's `_entries` is empty. A PNG
alone could not carry that claim — a hidden strip and a strip that was never seeded look identical.
`band.png` from the same run shows the strip populated, which is what makes `world_reset.png`'s empty
top bar mean something.
