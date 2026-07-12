# HUD Navigation & Turn Orb — implementation plan

Status: implementation + ui_preview verification complete (A–D landed in the client:
zoom collapsed to `MapView._apply_zoom`, interface-scale subsystem removed, minimap +
zoom rail moved bottom-left, turn orb + idle_workers attention producer). Pending commit.
Zoom-rail in/out use a hand-drawn `MagnifierButton` icon (font magnifier glyphs tofu).
Follow-up (committed after the first landing): the standalone left-dock **Alerts panel was
removed** and its alerts folded into the orb as two more producers — `starving` (critical)
and `losing_population` (warn) alongside `idle_workers` (warn), so the orb is now the single
"needs you" surface. Icons: 🍖 / 📉 / 🛠.

Reshapes the bottom-bar navigation chrome. Three coupled changes plus the
plumbing for a generic "player attention" model, prototyped and signed off via
two interactive artifacts.

## Motivation

The upper-right widget was an **interface-scale** control (`content_scale_factor`
— it enlarges the whole HUD/text) mislabeled as a zoom widget. Because it scaled
the entire canvas uniformly, map icons never re-sized *relative to the hexes* and
never crossed the icon LOD threshold — unlike the trackpad/wheel/`Q`·`E` path,
which drives `MapView.zoom_factor` → `last_hex_radius`. Two unrelated subsystems,
one masquerading as the other. Interface scale is a legitimate feature but belongs
in a future Options/Settings menu, not a permanent top-bar widget.

## Scope (this PR)

**A. One zoom path.** Map zoom is only ever `MapView._apply_zoom`. Remove the
interface-scale subsystem entirely (widget + `content_scale_factor` + its keybinds)
and the dead `camera.zoom` path. Give the map-zoom a real on-screen home.

**B. Minimap → bottom-left, with a docked zoom rail.** All map-navigation
affordances co-located: minimap (click-to-pan), zoom `＋`/`－`, and a fit-to-view
`⊡`. The rail wires to the *same* `_apply_zoom` path the trackpad uses.

**C. Turn orb replaces the "Advance Turn" button.** A 4X-style circular orb that
**calm-pulses only when nothing needs the player**, and otherwise becomes the hub
for typed **attention reasons** — each a row that jumps the camera to the thing on
the map. Seeded with one producer (idle workers); built generic so wars /
decisions / awaiting-expeditions slot in later with no orb changes.

---

## A · Zoom: collapse to one path

### MapView.gd — new public API (thin wrappers over existing internals)
- `const ZOOM_BUTTON_STEP := 0.5` — one rail click. (Bigger than `MOUSE_ZOOM_STEP`
  0.2 so a click feels deliberate; classify as a config lever if tuned later.)
- `func zoom_step(direction: int) -> void` — `_apply_zoom(direction * ZOOM_BUTTON_STEP, _viewport_center_pivot())`.
  `direction` is `+1`/`-1`. Pivot on the map center so button-zoom doesn't drift.
- `func _viewport_center_pivot() -> Vector2` — `_get_adjusted_viewport_size() * 0.5`
  (respects the inspector left-inset; local coords, matching `_apply_zoom`'s pivot space).
- `func fit_to_view() -> void` — public alias calling `_fit_map_to_view()`.
- `signal zoom_changed(zoom_factor: float)` — emit at the **end** of both
  `_apply_zoom` (only when the factor actually changed — it already early-returns on
  no-op) and `_fit_map_to_view`. Lets the HUD render the live zoom readout.

### Remove the interface-scale subsystem
- **Main.gd**: delete `ui_zoom` var; `UI_ZOOM_STEP/MIN/MAX` consts; `_ensure_ui_zoom_actions`,
  `_resolve_ui_zoom`, `_apply_ui_zoom`, `_adjust_ui_zoom`, `set_ui_zoom`,
  `_on_hud_zoom_delta`, `_on_hud_zoom_reset`; the two `hud.connect("ui_zoom_*")` lines;
  the `_hud_invoke("set_ui_zoom", …)` call; and the `ui_zoom_in/out/reset` branches in
  `_unhandled_input` (and their action registration). Interface scale + its `=`/`-`/`0`
  keys are **fully removed** this PR — they return via Options later.
- **Main.gd**: delete dead `_adjust_camera_zoom` + `CAMERA_ZOOM_STEP/MIN/MAX` (zero callers).
- **Hud.gd**: delete `ui_zoom_delta`/`ui_zoom_reset` signals; `zoom_controls`/`zoom_out_button`/
  `zoom_reset_button`/`zoom_in_button` onready refs; `set_ui_zoom`; `_connect_zoom_controls`
  (and its call in `_ready`); `_on_zoom_out_pressed`/`_on_zoom_reset_pressed`/`_on_zoom_in_pressed`.
- **HudLayer.tscn**: delete the `TopBar/ZoomControls` node and its three button children.

---

## B · Minimap bottom-left + zoom rail

### HudLayer.tscn — BottomBar reorder + new nav cluster
Target left→right order in `BottomBar`:
`[NavCluster (minimap + zoom rail)] [BottomSpacer→expands] [ResourceSummary] [TurnCluster]`.
(The `ResourceSummary` "Resources: --" placeholder was later removed — see
`docs/plan_band_city_dock.md` / the turn-orb Advance fix; the bar is now
`[NavCluster] [BottomSpacer] [TurnCluster]`.)

- Wrap the minimap and rail in a new `NavCluster` (`HBoxContainer`, `size_flags_horizontal = 0`,
  bottom-aligned) placed as the **first** BottomBar child (before `BottomSpacer`).
  - Keep the existing `MinimapContainer` (MarginContainer) as the first NavCluster child —
    do NOT rename it; `MapView._setup_2d_minimap` finds it via `Hud.get_minimap_container()`.
  - Add `ZoomRail` (`VBoxContainer`, `size_flags_vertical = 4` center) as the second child,
    a thin column docked to the minimap's right edge with:
    `ZoomInButton` (`＋`), `ZoomLevelLabel` (`"1.0×"`, mono, cyan, tabular), `ZoomOutButton` (`－`),
    `ZoomFitButton` (`⊡`, tooltip "Fit map to view (C)"). Small square buttons (~34px).
- Style all three rail buttons via `HudStyle.apply_button(btn, "ghost")` in Hud `_ready`
  (see C for why the orb uses styled buttons too). No raw default-theme buttons.

### Hud.gd
- New onready refs: `zoom_rail`, `zoom_in_button2`/`zoom_out_button2`/`zoom_fit_button`,
  `zoom_level_label` (use fresh names; the old `zoom_*_button` refs are deleted in A).
- New signals: `map_zoom_step(direction: int)`, `map_zoom_fit`.
- Connect the buttons in a `_connect_zoom_rail()` (called from `_ready`): `＋` → `emit map_zoom_step(1)`,
  `－` → `emit map_zoom_step(-1)`, `⊡` → `emit map_zoom_fit`.
- `func set_zoom_readout(zoom_factor: float) -> void` — `zoom_level_label.text = "%.1f×" % zoom_factor`.

### Main.gd — wiring
- `hud.map_zoom_step → map_view.zoom_step`
- `hud.map_zoom_fit → map_view.fit_to_view`
- `map_view.zoom_changed → hud.set_zoom_readout`
- Seed the readout once after first layout (call `hud.set_zoom_readout(map_view.zoom_factor)`).

---

## C · Turn orb (replaces NextTurnButton)

New encapsulated scene `src/ui/TurnOrb.tscn` + `src/ui/TurnOrb.gd` (extends `Control`),
placed as the last `BottomBar` child (a `TurnCluster`). The old `NextTurnButton` node and
its Hud refs/handlers are removed; the orb re-emits the existing signals so **Main wiring
for advance/jump is unchanged**.

### Visuals (match the signed-off artifact; palette from HudStyle)
- **Caption** (left of the orb): `Turn N` (mono) + a status line: cyan/HEALTHY `▸ all clear`
  when ready, else severity-tinted `N items need you`.
- **Orb face**: a round `Button` (StyleBoxFlat, full corner radius, `SIGNAL_DEEP` border,
  subtle cyan inner glow) whose text is the glyph `‣‣` in `SIGNAL`. Face border/glyph tint =
  `SIGNAL` when ready, else the highest-severity color.
- **Ring**: drawn in the orb's `_draw` behind the face — a static `LINE_SOFT` circle plus a
  dashed `SIGNAL` pulse arc. The pulse animates (alpha 0.3↔0.85, ~2.6s) in `_process` **only
  when the registry is empty**; hidden otherwise.
- **Badge**: when reasons exist, a small filled circle top-right with the count, tinted by the
  highest severity (`WARN`/`DANGER`/`SIGNAL`), drawn in `_draw`.
- Reuse `HudStyle` constants for every color. No hardcoded hexes (config-lever / named-const rule).

### Interaction (locked)
- **Click the orb face → toggle the reasons popover** (NOT advance — advancing-by-default is a
  deliberate future option; for now the orb is a review surface).
- **Popover** (a `PanelContainer` via `HudStyle.card_stylebox()`, positioned above the orb,
  built at runtime):
  - Header: `NEEDS YOUR ATTENTION` + `N items` (mono, dim). When empty: an `✓ All clear` block
    ("Every band is working and no decision awaits. Advance the turn.").
  - One row per attention entry (`HudStyle.apply_button(row, "ghost")`): a severity stripe,
    a kind icon, `label` (ink) + `detail` (mono dim), and a right-aligned `Jump →`
    (or `Open ▸` when non-locating). Sorted highest-severity first.
  - Footer: an **`Advance ▸`** button (styled `primary`).
  - Row press → if the entry locates (`x >= 0`), emit `focus_requested(x, y)` and close;
    else (`x < 0`) a no-op stub for now (only `idle_workers` exists, always locating).
  - Footer press → emit `advance_requested`.
  - Dismiss on outside-click (a full-rect transparent catcher while open) and on re-click.

### TurnOrb.gd public API + signals
- `signal focus_requested(x: int, y: int)`
- `signal advance_requested`
- `func set_attention(entries: Array) -> void` — store, recompute ready/badge/tint, rebuild the
  (open) popover, restart/stop the pulse.
- `func set_turn(turn: int) -> void` — caption `Turn N`.
- Ready state derives purely from `entries.is_empty()`.

### Hud.gd relays (keep Main wiring stable)
- New onready `turn_orb`; connect in `_ready`:
  - `turn_orb.focus_requested → (x,y): emit alert_focus_requested(x, y)` (reuses the existing
    `alert_focus_requested → MapView.focus_on_tile` wiring — same jump the Alerts panel uses).
  - `turn_orb.advance_requested → : emit next_turn_requested(1)` (existing `_on_hud_next_turn`).
- In `update_overlay`, after setting `_current_turn`, call `turn_orb.set_turn(turn)`.
- Delete `next_turn_button` ref, `_connect_control_buttons` next-turn wiring, `_on_next_turn_pressed`.
  (`_connect_control_buttons` may be emptied/removed if nothing else uses it.)

---

## D · Attention model (generic; one producer this PR)

### The contract (one entry per thing that wants the player's eye)
```
{
  kind:     String    # "idle_workers" (this PR) | "war" | "decision" | "expedition_awaiting" | …
  severity: String    # "info" | "warn" | "critical"  → drives color + badge tint
  label:    String    # "3 idle workers"      one-line summary
  detail:   String    # "Band 2"              secondary context
  x: int, y: int      # map focus for the jump; (-1, -1) = non-locating (renders "Open ▸")
}
```
- Severity → color via a small map in TurnOrb: `info→SIGNAL`, `warn→WARN`, `critical→DANGER`.
- Kind → icon via a small map in TurnOrb (`idle_workers → "🛠"`; unknowns → "●").
- Orb readiness = registry empty.

### Producer: idle_workers (in `Hud.update_band_alerts`)
It already iterates the player faction each snapshot. Alongside the `alerts` array, build an
`attention` array: for each player band with `int(entry.get("idle_workers", 0)) > 0`, append
`{kind:"idle_workers", severity:"warn", label:"%d idle workers" % n, detail:band_name,
x:int(entry.get("current_x",-1)), y:int(entry.get("current_y",-1))}` (band_name via the existing
`_band_display_name(entry, band_index)`). After the loop, `turn_orb.set_attention(attention)`.
Sort is done in the orb.

Future producers (stubs, not built now): `war` (critical, focus → frontier), `decision`
(info, non-locating → opens a panel), `expedition_awaiting` (warn, focus → expedition tile).

---

## Verification (client-dev, no server needed)
- `cargo xtask godot-build`.
- **ui_preview harness**: add/extend a state that seeds the orb both ways — an **all-clear**
  frame (empty attention → calm pulse, `▸ all clear`) and a **with-reasons** frame (2 idle-worker
  entries → badge `2`, amber tint, popover open showing two `Jump →` rows). Confirm the minimap
  sits bottom-left with the zoom rail, and the top-right interface-scale widget is gone. Read the
  PNGs and confirm before reporting.
- Sanity: `grep` confirms no remaining references to `content_scale_factor`, `ui_zoom`,
  `zoom_controls`, `_adjust_camera_zoom`, `NextTurnButton`.

## Docs to update on completion
- `clients/godot_thin_client/CLAUDE.md`: Hotkeys table (`Q/E` + wheel remain map zoom; drop any
  interface-scale mention), the Minimap System section (now bottom-left, with the zoom rail), and a
  new short "Turn orb & attention model" subsection under Command Targeting / HUD.
- Keep this plan doc's Status line current.
