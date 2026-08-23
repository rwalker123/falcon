---
paths:
  - "clients/godot_thin_client/src/scripts/Inspector.gd"
  - "clients/godot_thin_client/src/scripts/ui/inspector/**"
---

<!-- Extracted verbatim from lines 152-168;3913-4026 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Inspector panels

> ## ⚠ The Inspector is legacy scaffolding and is expendable
>
> **Standing decision (Ray, 2026-07-26).** It dates from before the client had a real UX and was
> the stand-in for seeing simulation state at all. Many of its tabs need redoing, and it is slated
> for **rework after the performance arc lands** (#386 delta streaming, #392 multi-core, and their
> sub-issues).
>
> **Breaking it in the meantime is acceptable.** Do not let this panel constrain, delay, or
> complicate performance work, and do not invest in optimising code here — it is more likely to be
> replaced than tuned (#390 is parked for exactly this reason). The contracts below describe how it
> works *today* and are worth honouring for correctness while it lives; they are **not** a reason to
> hold back a change elsewhere.

## Key scripts

| Script | Purpose |
|--------|---------|
| `Inspector.gd` | Inspector coordinator: streaming fan-out, capability gating, typography; hosts per-tab panels |
| `ui/inspector/PowerPanel.gd` | Power tab panel — reference for the tab-panel extraction contract (`apply_update`/`reset`) |
| `ui/inspector/CrisisPanel.gd` | Crisis tab panel — adds command hooks (`set_command_hooks`) and `apply_typography` to the contract |
| `ui/inspector/KnowledgePanel.gd` | Knowledge tab panel — adds `set_command_connected` (connection-gating) and `ingest_log_entry` (log-path telemetry), which since issue #381 also parses the `trade.telemetry ` line itself. **`append_events` and the Trade→Knowledge signal seam are GONE** — the Trade tab was retired and this panel was that batch's only consumer, so the parse moved to the one place already being fed every log entry |
| `ui/inspector/SentimentPanel.gd` | Sentiment tab panel — display; axis bias is coordinator-owned and pushed in via `set_axis_bias` |
| `ui/inspector/VictoryPanel.gd` | Victory tab panel — display + one-shot "victory achieved" log via `set_log_hook` |
| `ui/inspector/FaunaPanel.gd` | Fauna tab panel — **display-only** herd list/detail + estimated hunt yields. The follow-herd command it used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a; hunting is now HUD labor allocation), so it issues no command; `set_command_connected` is a contract no-op |
| `ui/inspector/GreatDiscoveriesPanel.gd` | GreatDiscoveries tab panel — large, self-contained (ledger + progress + definition catalog + details); capability-gated (`CAP_MEGAPROJECTS`), no command/log/MapView coupling |
| `ui/inspector/LogsPanel.gd` | Logs tab panel — owns the LogStreamClient + polling + filters + tick sparkline; emits `log_entry_received` (coordinator dispatches to Knowledge); fed synthetic lines via `append_entry`. **Ingest is per entry, render is once per poll.** `_render()` rebuilds the whole `LOG_ENTRY_LIMIT` buffer into one BBCode string and re-shapes it (`set_text` + `get_line_count()`), so calling it from `_record` re-shaped the buffer once per ingested line — ~145 times per turn at `RUST_LOG=info`, measured at **1.9–2.2 s of blocked main thread per turn** (client-wide: `_apply_snapshot` was only 11–31 ms of it). It now renders once in `_poll_stream`'s `if updated:` block beside `_update_sparkline()`; `append_entry` renders its own single line. The INGEST stays per entry — a dropped log line is an accumulator loss, not a re-renderable one |
| `ui/inspector/InfluencerPanel.gd` | Influencers tab panel — owns the influencer roster; capability-gated (`CAP_INDUSTRY_T1`/`T2`) via `set_available`; exposes `aggregate_resonance()` (coordinator feeds it into the Culture tab) and `get_influencers()` (coordinator's still-inline influencer command controls read the roster back). The influencer *command* controls stay coordinator-owned |
| `ui/inspector/CorruptionPanel.gd` | Corruption tab panel — display-only ledger (reputation modifier, audit capacity, incidents); not capability-gated |
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the highlight-rivers toggle (now a shader uniform — see Edge Blending → Rivers). Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` via `set_map_view`. **The `%LogisticsOverlayToggle` is gone from this panel and from `InspectorLayer.tscn`** — it pushed `set_trade_overlay_enabled` for the trade-link overlay, which was removed with the link network behind it (`overlay-channels.md` → "RETIRED", `docs/plan_contact_and_logistics.md`); issue #232's route-network overlay decides its own control. **The nested Map-Overlays section is gone too** — the overlay-channel picker lives on the minimap's top border now (`overlay-channels.md`; `docs/plan_knowledge_screen.md` §6), so the `OverlaySection`/`OverlayTabs` subtree and its script went with it |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `ui/inspector/TerrainPanel.gd` | Terrain tab panel — the largest: biome list + drill-down, tile list/detail, the runtime terrain-highlight dropdown, and the **Export Map** button (the tile Scout button was retired with the single-task `scout` command). Snapshot-driven (in `_tab_panels`): `apply_update` ingests `tiles`/`tile_updates`/`tile_removed`/`food_modules` and renders. Owns the inbound MapView hex-selection (`focus_tile_from_map`, coordinator forwards) and drives `set_terrain_highlight` / `relative_height_at` via `set_map_view`. The biome palette + tag labels arrive on the `overlays` key (coordinator routes them in via `set_terrain_palette`/`set_terrain_tag_labels`; `get_terrain_tag_labels()` went with OverlayPanel, its one caller — the tag channel's availability is `MapView.has_terrain_tag_data` now). Export sends via `set_command_hooks`, gated by `set_command_connected` |
## A hidden Inspector does not render — the contract on `_apply_update`

**The Inspector is hidden by default** (`Main.gd` seats `set_panel_visible(false)` at startup; the
player reopens it with `I`), and `Main` calls `update_snapshot` on **every** snapshot. Before issue
#384 that fan-out ran unconditionally: `_apply_update` walked every `_tab_panels` entry and then
`_render_dynamic_sections()`, costing **~113 ms per turn — 61% of the client's entire
snapshot-apply cost — to render a panel nobody could see.**

The gate lives **inside `Inspector`**, not at the `Main.gd` call site: `Inspector` owns
`_panel_visible` and `set_panel_visible`, so the skip and the show-hook that undoes it sit
together and cannot drift, `update_snapshot` has more than one caller, and part of `_apply_update`
must run while hidden anyway.

Three clauses, and the third is the one that bites:

1. **EVERY frame is skippable — and that is a reversal, recorded here so the old reasoning is not
   re-derived from first principles.** The rule used to be *only a full snapshot is skippable*,
   because a delta described a change against state the panels already held and nothing later could
   reconstruct a dropped one. **Delta streaming (#386) inverted the premise**: the native decoder
   maintains a cached world and republishes it **whole** on every frame — base keys patched from
   each delta's `*_updates` — so a merged delta frame is byte-equivalent to a full snapshot of the
   same state and the NEXT frame reconstructs anything dropped. Self-containment, not payload kind,
   is what the skip ever depended on; now both kinds have it.

   **The skip and the decoder's base-key patching are ONE contract.** If merged frames ever stop
   being complete, this gate silently serves a stale panel. The completeness is pinned by
   `decode_guard`'s section-cache assertions (`.claude/rules/client/native-extension.md`), and it is
   already a client-wide invariant — `MapView.display_snapshot` reads `tiles` / `populations` /
   `culture_layers` straight off the same merged frame every turn. `Main._apply_snapshot` is the
   only live caller and hands the identical dict to `update_delta` and `update_snapshot`, so there
   is no second, thinner producer to worry about. **A partial frame is still unsafe to cache and
   replay — it is simply no longer producible**; if some path starts handing `update_delta` one
   again, revert this skip with it rather than patching around it.

   Measured: the hidden fan-out was 16–30 ms/turn once deltas became the steady-state carrier, ~60 %
   of the client's `apply`. The old gate skipped only full snapshots, which by then arrived once per
   world — so it had quietly stopped working.

   **…but applying a frame does not discharge the catch-up.** `_hidden_snapshot_pending` means
   *a frame has arrived that the panels have not ingested*, so it is set whenever one is skipped and
   cleared only when one is actually fanned out. The first version of this gate cleared it on every
   non-skipped update, hidden deltas included, which broke the sequence **full-while-hidden →
   delta-while-hidden → `I`**: the replay was declared complete and the panels opened holding only
   what the delta carried — precisely the stale-when-opened failure clause 2 exists to prevent. It
   self-heals on the next turn, which is why neither review nor the first guard harness caught it —
   that harness fed no deltas. `inspector_hidden_guard` case 5 pins it, now with THREE distinct
   roster sizes so "catch-up never ran" (3), "catch-up replayed the older frame" (5) and "correct"
   (6) are all distinguishable.

   **The residual loss this used to carry is gone.** While the replay rebuilt panels from the cached
   *full* snapshot, a delta arriving after it was overwritten and its sub-turn changes lost until
   the next full snapshot; the recorded eventual fix was to queue deltas while hidden and replay
   `cached full + queued deltas in order`. Neither is needed: `_cached_snapshot` holds the newest
   frame of either kind, and replaying one merged frame *is* the whole state.
2. **Catch up on show.** `set_panel_visible(false→true)` replays the cached latest snapshot. The
   cache holds it **by reference** — deep-copying would cost exactly the work the gate saves, and
   the decoder builds a fresh tree per frame that no consumer mutates.
3. **Anything ACCUMULATING must stay above the gate.** The test is not "is it cheap", it is **"is
   it reconstructible from the next full snapshot?"** Panel state is a rebuild-from-keys and so
   survives being skipped.

   **The Inspector currently holds NO accumulator, and that is a change worth knowing about.**
   `command_events` was the one — per-turn *events*, a dropped one gone from the running log
   forever, so `_ingest_command_events` ran unconditionally and `_seen_command_events` made the
   replay safe. Issue #272 retired that stream: the events belong to the **event dock**
   (`Main._apply_snapshot` feeds `EventDockPanel.ingest_events` directly and never routes them
   through here), and the Commands tab that displayed them has since been deleted. What survives above the
   gate is the cheap per-frame prefix — `_cached_snapshot` and `_last_turn` — which is what makes
   clauses 1 and 2 possible at all, so the gate still may not creep upward over it.
   `inspector_hidden_guard` witnesses the prefix on `_last_turn` (written above the gate, read
   nowhere else on this path) now that `_seen_command_events` is gone.

   **An accumulator added back here goes above the gate and gets its own assertion in that guard.**

   Adding anything to `_apply_update` means answering that question for it. Near-misses that are
   safe, each checked rather than assumed: `VictoryPanel._log_victory` is edge-triggered but on
   *persistent* state that rides every snapshot; `CulturePanel._log_new_culture_tensions` runs only
   on the delta branch, which is never gated; `KnowledgePanel.append_events` accumulates but hangs
   off `ingest_log_entry`, the log-stream path, not this one.

`tools/inspector_hidden_guard.gd` pins all of it (see `harness-headless-guards.md`) — the property is
invisible in normal play, since a stale-when-opened Inspector looks like a panel that just hasn't
updated yet.

## The console chatter has left the building

`_append_command_log` is the funnel for every client-side line the Inspector writes — connection
state, a command sent, a command refused, a rollback. It has two outlets, and **neither is a console
widget any more**: the Logs tab's buffer (`_append_log_entry`, tagged `COMMAND` /
`inspector.command`) and **`system_event(label, detail, alert, kind)`**, which `Main` relays to the
event dock's System channel. A dropped command socket is something the player must be told, and the
Commands tab that used to mirror every line shipped hidden behind a debug tab — which is why the
dock is now the surface that carries it and the tab was deleted rather than kept in parallel.

**`kind` is what keeps the funnel's completeness from becoming the bar's noise.** `_send_command`'s
ACCEPTED-send line — the only place in the client where a command is known to have gone — logs
`HudEventVocab.KIND_COMMAND_ECHO`, which the dock ignores outright; every other site here takes the
`KIND_SYSTEM` default and reaches the player. That is the boundary: a command accepted for sending is
an echo, everything else (including both failure exits of `_send_command`) is a fault. `_send_command`
and `send_runtime_command` take an `ack_kind` defaulting to the echo, for the one caller whose success
message reports a fault rather than a receipt (`Main`'s `resync`). **The Logs buffer is unaffected by
the kind** — it records every line, because it is the debug console now. The rest of the rule is in
`event-dock.md` → "A kind the dock IGNORES".

Two details are load-bearing:

- **`alert` is stated by the emitting site, never derived from the text.** `_append_command_log`
  takes it as a defaulted parameter (`false`), so the five panel-injected `Callable`s (Map / Terrain
  / Crisis / Knowledge / Victory) are unchanged and a panel's own command receipt is a
  Routine note; the six FAILURE sites in this file pass `true`. This file knows which of its own
  lines is bad news — a string match on its own log strings would only pretend to.
- **The LINE is the dock row's label, not a detail beside a fixed one.** A dock row draws its label
  at full size on the leading edge and its detail as small faint text on the TRAILING one, so
  emitting `("Command", entry)` strands the only words that matter at the far end of a screen-wide
  bar. The channel chip already says where the line came from.

## Inspector Panels

See `docs/godot_inspector_plan.md` for full roadmap.

| Tab | Purpose |
|-----|---------|
| Map | Map size dropdown, Generate Map button, scenario controls, highlight-rivers toggle (the overlay selector moved to the minimap — `overlay-channels.md`) |
| Terrain | Full biome histogram, tag histograms, tile drill-down, terrain-type highlight dropdown, **Export Map** button |
| Fauna | Herd registry + density telemetry (display-only; follow-herd command retired) |
| Culture | Layer trait vectors, divergence meters, resonance pushes |
| Military | Readiness heatmaps, cohort summaries |
| Power | Grid metrics, node list, incident feed |
| Crisis | Dashboard gauges, modifier tray, event log |
| Knowledge | Ledger overview, timeline graph, espionage mission queue, trade-diffusion events |
| Logs | Streaming tracing feed, level/target/text filters, duration sparkline |

**There is no Commands tab.** Turn stepping, rollback and autoplay are the `CommandToolbar` buttons
above the `TabContainer`, not a tab. Six of the designer/debug verbs it carried — `bias`, `support`,
`suppress`, `support_channel`, `spawn_influencer`, `corruption` and `heat` — **no longer exist in
the command surface at all**: the tab was their only caller, so they were removed end to end
(parser, payload, proto field numbers now `reserved`). The systems they poked are untouched — axis
bias, the influencer roster, the corruption ledger and tile temperature are still simulated and
still stream to the client, which is why `SentimentPanel` still renders `axis_bias`. `reload_config`
(and its `reload_sim_config` alias) **survives**, reachable via `cargo xtask command` with no client
control. The command
*hub* (`_send_command` / `send_runtime_command` / `_ensure_command_connection` / `command_client` /
`autoplay_timer`) is Inspector-level and carries every command the game sends, so it long outlives
the tab.

**A FAILED SEND IS ONE OF *TWO* THINGS, AND CALLING BOTH A DISCONNECT SENT A PLAYER HUNTING A DEAD
NETWORK THAT WAS FINE.** This paragraph used to read *"a failed send here is a transport failure and
can be nothing else"*, on the reasoning that `CommandClient.send_line` answers
`ERR_CANT_ACQUIRE_RESOURCE` with no bridge and `ERR_CANT_CONNECT` when the bridge could not deliver,
and that **"those are its only two values"** — a command the SIM refuses having already gone down the
socket, its refusal arriving later on the server's own event stream.

> **⛔ THAT WAS TRUE UNTIL PARSING MOVED INTO THE BRIDGE, AND ITS OWN EXAMPLE IS WHAT BROKE.**
> `bridge/command.rs` → `send_line` calls `parse_command_line` **before** it sends anything and returns
> `{ok: false, error: …}` on a line it cannot read. That is a THIRD answer the invariant did not admit,
> and the one that fired in play: `assign_labor 0 1 builders 1` — the line this section used as its
> worked example — was **rejected locally**, because the role was missing from the text grammar
> (`harness-band-panel.md` → "A ROLE PASSES TWO GATES"), and the player was told the server was
> unreachable while it was answering perfectly.

So the site forks, and **it forks on the error CODE rather than on the reason's prose**:

- **`ERR_CANT_CONNECT` → `HudEventVocab.COMMAND_REFUSED_FORMAT`** — `Refused before it left the client
  — "assign_labor 0 1 builders 1": unexpected token "builders"`. The bridge's reason is passed
  **verbatim**, because that reason names the token that failed and is the only actionable thing in the
  line. *"before it left the client"* is honest for every failure this code covers — an unparseable
  line, a dispatch that never reached the worker, a write the worker could not make, a wait that timed
  out — none of which is the server refusing anything.
- **Anything else → `COMMAND_NOT_SENT_FORMAT`**, which **narrowed rather than widened** and keeps its
  original meaning. Widening its wording to cover both would have traded a wrong message for a vague
  one, which is the tempting repair and the worse one.

**`CommandClient` retains the reason now** (`last_send_error`) instead of dropping it after a
`push_warning` no player ever sees. **`command_connected` is NOT touched on the local-rejection path,
and never was**: `_update_command_status()` re-reads `CommandClient.status()`, which answers
`STATUS_CONNECTED` whenever a bridge exists — so the status indicator was correct throughout and **the
log line alone was the lie**. That is written down here rather than left as an assumption, because the
obvious "fix" is to stop flipping a flag that was never flipping.

**KNOWN GAP — no harness drives this line.** Neither preview harness stands up `Inspector`, and
`command_guard` exercises the builder rather than the send path's failure branch, so the fork is
verified by reading. The cheapest seam if it needs pinning is a pure static mapping
`(Error, reason) → message`, which is what the rest of this client does with a decision worth asserting.

`_ensure_command_connection`'s two messages are untouched — *"Command pending: command socket still
connecting."* and *"Command unavailable (…)"* already name the socket rather than a refusal.

**Capability gating** (`Inspector._apply_capability_gating`): most tabs enable only when the matching `CapabilityFlags` bit is set. **Terrain is exempt** — it is an always-available inspection tab with no capability-gated actions (the former Found Camp action + its CAP_CONSTRUCTION gate were removed with the retired `found_camp` command). **Migrated tab panels don't grey out** — instead of disabling the tab (confusing: a dead tab with no explanation), the coordinator calls `panel.set_available(has_flag)` and the panel stays clickable, rendering a "🔒 Locked — unlocks via …" message while gated (see `PowerPanel`). `_set_tab_enabled` is still used for tabs not yet migrated to the panel contract. Its **terrain-type highlight** dropdown lists every defined terrain (via `TerrainDefinitions`), and selecting one calls `MapView.set_terrain_highlight(id)`, which outlines/tints all matching hexes map-wide (ignoring Fog of War) — handy for spotting a biome or confirming one is absent. Selecting "none" (`-1`) clears it.

The overview text draws a **full biome histogram** (`_render_terrain` → `_histogram_bar`): every present biome, sorted by count, with a monospace `[code]` bar scaled to the most common biome plus its tile count and percentage — all computed client-side from the streamed `_terrain_counts`. The **Export Map** button (`_on_export_map_button_pressed`) sends the fire-and-forget `export_map` runtime command; the server writes the current map (terrain snapshot + resolved seed) to its `exports/` scratch dir as JSON (see `sim_schema` `MapExport`). Tile coordinates shown here as `@x,y` (`_format_tile_coords`) index straight into the export's row-major samples, so the same coordinate names a hex in the client, in the export file, and in tests.

### Tab-panel extraction pattern

`Inspector.gd` is being decomposed from a single god-object into per-tab panels;
`Inspector` stays the **coordinator** (streaming, capability gating, typography,
reserved-width/resize) and forwards each update to the tab panels. A tab panel:

- Is a script attached to the tab's own scene node (its `class_name` typed by the
  node's base type — the Power tab is a `ScrollContainer`, so `PowerInspectorPanel
  extends ScrollContainer`). References its widgets by `%UniqueName` (mark those
  nodes `unique_name_in_owner` in `InspectorLayer.tscn`) and wires its own signals
  in `_ready()`. Same model as the pre-existing `scripting/ScriptManagerPanel`.
- Implements the coordinator contract: `apply_update(data: Dictionary,
  full_snapshot: bool)` — the panel reads only the snapshot/delta keys it owns and
  re-renders itself — and `reset()` — drop all panel state so the coordinator can
  re-seed it from a clean slate. `Inspector._apply_update` forwards to
  `panel.apply_update(...)`; `_render_static_sections` calls `panel.reset()` (today
  only on init; it is the hook a future disconnect/full-reinit flow would call). The panel owns its schema keys,
  state, and rendering; the coordinator knows none of them. Panels needing extra
  collaborators add setters (as `ScriptManagerPanel` does with `set_manager()`).
- Capability-gated panels also implement `set_available(available: bool)` — the
  coordinator maps the `CapabilityFlags` bit to it in `_apply_capability_gating`,
  and the panel renders a locked explanation while unavailable (the tab is *not*
  disabled). Always-on tabs (e.g. Terrain) skip this.

Optional contract hooks a panel adds only if it needs them:
- `apply_typography()` — the coordinator's `apply_typography()` calls it so the
  panel styles its own widgets (`CrisisPanel`). `Typography.gd` is currently a
  no-op stub, so this has no visual effect yet — it preserves intent for when
  typography is implemented.
- Collaborator setters for cross-cutting dependencies, kept narrow: `set_map_view`
  (overlay sync), `set_command_hooks(send: Callable, append_log: Callable)` for
  tabs that issue runtime commands (`CrisisPanel` spawn/auto-seed, `KnowledgePanel`
  policy/budget/mission). The panel never reaches back into the coordinator — it
  holds only the Callables/handles it is given.
- `set_command_connected(connected: bool)` — for tabs whose command controls
  enable/disable on the command socket state (`KnowledgePanel`). The coordinator's
  `_update_command_controls_enabled` delegates the panel's own controls to this.
- `ingest_log_entry(entry: Dictionary)` — for tabs fed by parsed *log messages*
  rather than snapshot keys (`KnowledgePanel` knowledge/espionage/counter-intel
  telemetry). The coordinator's log loop calls it per entry.
- Public feeder methods for cross-panel data flow. **The one instance of this is retired**:
  `KnowledgePanel.append_events` was fed by Trade's diffusion records via a
  `knowledge_events_produced` signal the coordinator forwarded. Issue #381 removed the Trade tab,
  and since Knowledge was the batch's only consumer AND already received every log entry through
  `ingest_log_entry`, the parse moved there — deleting the signal, the forward and the feeder.
  Prefer that shape: if the would-be producer and consumer are both already fed the same raw
  stream, parse at the consumer instead of adding a seam.
- Coordinator-owned state pushed into a display panel: `SentimentPanel.set_axis_bias`
  — the coordinator reads the snapshot's `axis_bias` key into `_axis_bias` and pushes it
  to the Sentiment view, instead of the panel owning the key. It held that state because
  the Commands tab's axis controls also mutated it optimistically; with those gone the
  snapshot is the only writer, and the push is the one remaining reason the key lives here.
- Command-issuing via a signal when the command needs coordinator-only context (pattern
  reference; the Fauna/Terrain examples were retired with the single-task commands — FaunaPanel
  is now display-only and TerrainPanel's Scout button is gone). `set_log_hook(append_log)` is the
  log-only variant of `set_command_hooks` (`VictoryPanel`'s one-shot victory announcement).

The coordinator collects extracted panels in `_tab_panels` and fans `apply_update`
out to them at the **end** of `_apply_update`, after its own key routing (e.g.
`_ingest_overlays`), so a panel's own keys win over coordinator-side feeders on
conflict (see the `crisis_overlay` vs `overlays.crisis_annotations` precedence note).

**Reference implementations:** `ui/inspector/PowerPanel.gd` (Power — pure
snapshot/render), `ui/inspector/CrisisPanel.gd` (Crisis — command hooks +
typography), `ui/inspector/KnowledgePanel.gd` (Knowledge — the fullest: connection
gating and log-path ingestion). **The decomposition is complete** — every inspector tab is
now its own panel (see the key-scripts table). `Inspector.gd` (≈870 lines, down from
~6,500) is purely the coordinator: streaming fan-out, the command hub + autoplay timer,
capability gating, typography, MapView attach, and the cross-panel seams (faction
resolution for Fauna/Terrain, influencer resonance → Culture, the `overlays` fan-out
junction routing palette→Terrain and annotations→Crisis — the CHANNELS on that same key are
no longer routed anywhere: the minimap's picker reads them off `MapView`).

**The command hub is Inspector-level, not tab-level — and that is the reason deleting the Commands
tab was safe.** `_send_command` → `command_client`, `send_runtime_command` (what `Main` calls for
every order the player gives: new-game, turn advance, labor, move band, resync), the
`autoplay_timer`, `_ensure_command_connection` and `_update_command_status` all live in the
coordinator, shared with the turn controls in `RootPanel/CommandToolbar` (outside the
`TabContainer`), the Terrain tab's Export Map button, and the four panels holding
`set_command_hooks` Callables. What went with the tab was only the tab-facing half: the log/status
mirroring, the axis-bias apply signal (`_axis_bias` itself survives — Sentiment renders it), the
`set_influencer_roster` push, and the autoplay toggle's mirror.

**Autoplay's only control is now the toolbar Play/Pause button**, so `_on_autoplay_toggled` is the
single entry point and `AUTOPLAY_INTERVAL_SECONDS` (0.5 s, the interval the retired spin box
shipped with) is the single rate — the interval is no longer settable from the UI.
`_disable_autoplay` un-presses the button as well as stopping the timer: it used to only clear the
tab's mirror toggle, which left the toolbar button lit over a stopped timer whenever a failed
advance or a lost snapshot stream killed autoplay. With the tab gone that button is the only state
the player can see, so the mirror had to move onto it rather than be deleted.

---

