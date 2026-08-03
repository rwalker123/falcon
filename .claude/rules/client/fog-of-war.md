---
paths:
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/src/scripts/ClientSettings.gd"
---

# Fog of war — the client is not an authority

The fog flow crosses four scripts, and **every one of them can break it on its own**, which is why
they share this file rather than one owning it: the preference store, the menu row that writes it,
the coordinator that turns it into a command, and the renderer that draws the answer. Splitting the
rationale across four homes is how the invariants below got lost the first time.

## Key scripts

| Script | Its part in the fog flow |
|--------|--------------------------|
| `ClientSettings.gd` | Holds `fog_of_war_enabled` (default `true`, `[map]` section of `user://client_settings.cfg`) — the **preference**, i.e. what the player last asked for. Never the render state, and **never written from a snapshot** |
| `ui/MenuShell.gd` | The Options pane's "Fog of war" row (`_make_toggle_row`, the boolean twin of `_make_speed_slider_row`; registered in `_option_toggles` with its default in meta so "Restore defaults" resets it). Writes `ClientSettings` **and stops** |
| `Main.gd` | The **only** sender. Owns the `F` key, the `ClientSettings.changed` subscription, the `_fog_server_state` resend guard and `_sync_fog_of_war` (snapshot → render + reconcile) |
| `MapView.gd` | Owns `_fow_enabled` — the render cache **and** the fail-closed initial state — plus `set_fow_enabled` and every downstream fog gate |

## Fog of war is SERVER-authoritative — the client renders what it is told

The sim owns fog of war. `SimulationConfig.fog_enabled` gates **both** the visibility raster **and
the herd display list**, and that second half is the whole reason the setting moved server-side:
with fog off, the fauna the herd filter used to drop are now genuinely *sent*, so the Fauna tab
shows them with **no client-side special case at all** (`FaunaPanel` renders whatever
`data["herds"]` holds and was deliberately left untouched). A client-only flag could never have
fixed that — it can hide things the server sent, never conjure things it withheld.

**A client that renders a revealed map while the panels redact is a SERVER-side symptom, not one of
the four scripts below.** The two halves reach the client by different routes — `fogEnabled` rides
every delta undiffed, the visibility raster is diffed — so the flag can arrive without the raster
that gives it meaning, and every fog gate here then passes an all-`Active` raster perfectly
correctly. See `../core_sim/turn-profiling.md` → "A HELD section must be restated when it comes
back"; do not go looking for a stale texture in `MapView` first.

**Three things share the word "enabled"; keep them apart.**

| | What it is | Written by | Read by |
|---|---|---|---|
| `ClientSettings.fog_of_war_enabled` | the player's persisted **preference** (`user://client_settings.cfg`, `[map]`, default `true`) | the Options toggle, the `F` key | `Main`, to decide whether to send a command |
| snapshot `fog_enabled` | the server's **current state** (top-level bool from `VisionSection.fogEnabled`) | the sim | `Main._sync_fog_of_war` |
| `MapView._fow_enabled` | a **render cache** — plus the **fail-closed initial state** | `Main`, off each snapshot | every fog gate and renderer |

**One direction only:** preference → `set_fog` command → server → snapshot → render. **Never write
`ClientSettings` from a snapshot** — that closes the loop into an echo, where a rejected or
server-overridden command silently rewrites the preference it came from.

**`F` no longer touches MapView.** `Main._toggle_fow_overlay` flips *only* the preference, which is
what makes the hotkey and the Options checkbox one state rather than two that drift. `Main` is the
sole sender: it listens on `ClientSettings.changed` and emits `set_fog on|off` through
`_send_runtime_command`. **`MenuShell` deliberately has no handle to Main / Inspector /
CommandClient and must not grow one** — the Options row writes `ClientSettings` and stops there,
which is also why the row works in the LANDING menu with no server up: the preference just persists.

**The resend guard is `Main._fog_server_state`** — a tri-state (`UNKNOWN` before the first snapshot
carries the key), not a bool, so "haven't heard yet" stays distinct from "heard, and it is off". A
command goes out *only* when the preference disagrees with it. That one guard does double duty: it
stops the `changed` handler and the per-snapshot reconcile from ping-ponging, **and** it is what
applies a persisted "fog off" to a freshly generated world — every snapshot re-checks, so after
`new_game` the disagreement fires one `set_fog` and the next snapshot agrees.

**`_fow_enabled` defaults to `true`, and that default is load-bearing.** It is not merely a cache:
between startup and the first snapshot carrying `fog_enabled` there is nothing to render *from*, so
the flag has to **fail closed** or the client draws a fully-revealed map in that window. `Main._ready`
used to seat it (`set_fow_enabled(true)` before the first world rendered); that seat is gone now the
sim owns the flag, so the declaration at `MapView.gd`'s `_fow_enabled` carries it, matching the
server's `SimulationConfig.fog_enabled` default.

**The offline harnesses must STATE their fog condition, never inherit the default** — this is the
guardrail, and it exists because the absence of it cost a silent regression. When the default was
`false`, `map_preview`'s first five states (`map_band_work` / `_label_overlap` / `_yield_farzoom` /
`_scout` / `_pending`, all saved *before* its first `set_fow_enabled` call) came out unfogged **by
accident**; the day it flipped, all five silently rendered as blank fog with their subject gone.
Worse, `ui_preview`'s `tile_panel_land_sticky` — a *behavioural* guard that clicks a crowded hex and
asserts the land selection survives — kept printing **PASS while asserting nothing**, because fog
gated every band and herd out of `tile_info` and left no occupant to fail to stick to. Both now call
`set_fow_enabled(false)` at their setup site. `blend_probe` was already immune (its first call
precedes its first save) and `band_panel_preview` reads unit *markers*, which are built unfiltered.
**Any new harness state that instances a MapView declares its fog state explicitly**; a frame that is
green because its subject disappeared is worse than one that varies.

**`MapView.set_fow_enabled` stays a public, locally-callable setter** — it is now the *only*
MapView-side fog entry point and `Main` is its only live caller, but `tools/map_preview.gd` (30
call sites) and `tools/blend_probe.gd` (11) drive fog states **offline with no server to ask**, so
it cannot become private or snapshot-only. Its early-out on an unchanged value is what makes the
per-snapshot push free; its side effect of clearing `active_overlay_key` when *enabling* is
load-bearing and must be preserved. Nothing else changed: `_is_tile_visible`,
`_visibility_state_at`, `_apply_visibility_to_info`, `_unit_hidden_by_fog`, the pan clamp and every
renderer gate are untouched, because with fog off the server now sends an all-Active visibility
raster and those gates pass naturally.

**Nothing in the boot path needs the setter body to have run.** With the default `true` and a first
snapshot of `fog_enabled: true` the body early-outs and never executes — which is fine, because
`display_snapshot` independently does five of its seven side effects (`rebuild_shader_maps`,
`_invalidate_map_cache`, `queue_redraw`, `_emit_overlay_legend`, `_minimap.update`, plus
`_clamp_pan_offset`), and the other one — clearing `active_overlay_key` — is a no-op at boot, since
that field already initialises to `""` and the overlay picker is only populated *from* a snapshot.

**Deltas.** `Main._sync_fog_of_war` takes an `is_delta` flag and returns early if a delta omits the
key. Currently unexercised — the native decoder resolves `fog_enabled` from its own cached `Option`
and always emits it — but the failure mode if that stops holding is silent and ugly: on a delta an
absent key means *unchanged*, not *fog on*, so taking the `true` default would strobe the fog back
on every turn.

**Verify the Options row** with `godot --path clients/godot_thin_client res://tools/menu_preview.tscn`
→ `ui_preview_out/menu_options.png`.
