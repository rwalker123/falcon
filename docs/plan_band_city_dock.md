# Band / City Dockable Panel — design & implementation plan

Status: complete (branch `worktree-feat+band-city-dock`, pending push/PR). Design
signed off via an interactive prototype. Slices 1–4 landed + verified live
(commands fixed via the worktree proto-port fix). Slice 4 shipped the two-column
wide-dock layout; true multi-column section-flow deferred. Built in slices on one
branch.

## Motivation

Band detail currently lives in the left-dock **Occupants card drawer**
(`_unit_summary_lines` + `_build_allocation_panel`). It is already too big for that
area, and bands grow into cities — settlement stages ⛺ nomadic → 🛖 camp → 🏘️
village — accreting districts, production, buildings, garrison over time. A cramped
inline drawer won't hold that. Band/city detail needs its own home: a **dedicated,
user-dockable panel that reserves screen space (never overlays the map)** and grows
with the settlement.

## Locked design decisions (from prototype review)

1. **Band detail relocates** entirely to the new panel. The left-dock **Occupants
   roster stays** (it lists who's on a hex); **non-band** occupants (herds today,
   game/others later) keep detailing in the selection area's drawer. Only the
   **resident band** branch of the drawer moves out. (Expeditions — transient
   detached parties — stay inline for now; revisit later.)
2. **Panel cycling recenters the map** on the cycled settlement (trial; may revisit).
3. **All four dock edges** (left default). Content should, over time, **adapt to a
   tall (L/R) vs wide (T/B)** dock — vertical stack vs columnar reflow.
4. The **settlement stage** (glyph + name + Nomadic/Camp/Village) is **promoted into
   the panel header** — it is *not* in the drawer text today (only the map token /
   roster tooltip).
5. The **dock choice persists** across sessions.

## Architecture

### 1. Reserved-space docking, generalized (4 edges, multi-reserver)

Today the Inspector reserves the **left** edge only, via a single scalar:
`Inspector.reserved_width_changed(width)` → `Main._apply_inspector_inset` →
`Hud.set_left_inset(px)` + `MapView.set_view_inset_left(px)`. `MapView` shrinks the
viewport on x, translates the node right, and clips (`_get_adjusted_viewport_size`,
`position.x = inset`, `_apply_view_clip`). All left-specific.

Generalize to a **reservation registry** keyed by reserver id, each contributing
`(edge, size)`, summed per edge. Two reservers exist now (Inspector, the new Band
panel) and they can occupy different edges.

- **`MapView.set_reserved_inset(id: StringName, edge: int, size: float)`** — `edge`
  is a Godot `Side` (`SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM`). `size <= 0`
  removes the reserver. Recompute four edge totals (`left/right/top/bottom` = Σ of
  sizes whose edge matches), then re-apply layout. Replaces `_view_inset_left`.
  - `_get_adjusted_viewport_size()`: `x = max(w - left - right, 1)`,
    `y = max(h - top - bottom, 1)`.
  - node translate: `position = Vector2(left, top)` — only **leading** insets shift
    the content; trailing (right/bottom) insets just shrink the viewport.
  - `_apply_view_clip`: clip rect `Rect2(Vector2.ZERO, adjusted_size)`; enable when
    **any** inset > 0 (was: only left).
- **`Hud.set_reserved_inset(id, edge, size)`** — same registry/summing; apply to
  `LayoutRoot`: `offset_left = left`, `offset_top = top`, `offset_right = -right`,
  `offset_bottom = -bottom`. Replaces `set_left_inset`.
- **`Main._apply_reservation(id, edge, size)`** — fans a reserver's `(edge, size)`
  out to both `map_view` and `hud`. Inspector: `_apply_reservation(&"inspector",
  SIDE_LEFT, width)`. Band panel: `_apply_reservation(&"band_panel", edge, size)`.

The old left-only API (`set_view_inset_left`, `set_left_inset`,
`_apply_inspector_inset`) is removed and its one caller migrated — no shims.

### 2. The Band/City panel (`src/ui/BandCityPanel.tscn` + `src/scripts/ui/BandCityPanel.gd`)

A **new CanvasLayer** (parallel to the Inspector), containing a Control anchored to
the active edge, sized on its cross-axis (width for L/R, height for T/B). Emits
**`reservation_changed(edge, size)`** on dock change / show / hide / collapse; Main
fans it out via `_apply_reservation(&"band_panel", …)`.

Chrome (owned by the panel):
- **Header**: settlement stage glyph + name + stage label (decision 4); a **cycler**
  `◀ n/N ▶` (decision 2); a **4-cell dock chooser** (decision 3); a **collapse**
  toggle (rails to a thin strip, reservation shrinks accordingly).
- **Body**: a `ScrollContainer` → `VBox` (the **host** for the band-detail content),
  laid out vertically for L/R and reflowed to columns for T/B (decision 3 — the
  columnar T/B reflow can land as a follow-up if needed; L/R vertical is the v1
  must-have).
- **Dock chooser** → sets the edge, re-anchors, re-reserves, **persists** (decision 5).

**Persistence (decision 5):** store the chosen edge (+ collapsed?) in a small user
settings file under `user://` (e.g. `user://band_city_dock.cfg` via `ConfigFile`),
loaded on `_ready`, saved on change. (Client has no central user-prefs store yet;
this is the first — keep it self-contained and reusable.)

### 3. Content relocation (band detail → panel)

The band-detail **logic stays in `Hud.gd`** (it owns all the state:
`_selected_unit`, `_player_band(s)`, `_pending_labor`, band-picker, expedition
controls, targeting flows — moving it wholesale is high-risk). Instead **retarget
where it renders**:

- The BandCityPanel exposes body target nodes — a `%BandDetail` (RichTextLabel) and
  a `%BandAllocationPanel` (VBox) — mirroring the Occupants card's `%OccupantDetail`
  / `%AllocationPanel`. Hud receives them (Main wiring or a setter).
- `Hud._render_occupant_drawer`'s **band branch** renders `_unit_summary_lines` +
  `_build_allocation_panel` into the **panel's** targets (and shows/relevant-marks
  the panel); the **herd/expedition** branches render into the Occupants drawer as
  today. Selecting a band shows/reserves the panel; deselecting or selecting a herd
  hides it (reservation → 0).
- **Header content**: Hud pushes the selected band's `settlement_stage_icon` /
  `settlement_stage_label` (already on the unit marker) + a display name to the
  panel header.
- **Selection routing** (already exists): map click → roster auto-select →
  `roster_occupant_selected(kind, id)`. The panel follows this same signal (via Hud
  or Main relay). **Cycler** → walks `_player_bands`, calls the existing
  `MapView.select_occupant` + recenters (decision 2, reuse `focus_on_tile` /
  `focus_and_select_tile`).

## Slices (one branch, reviewed between)

- **Slice 1 — reserved-dock infra.** Generalize MapView + Hud + Main to the 4-edge
  multi-reserver registry; migrate the Inspector onto it. No new panel, no
  user-visible change (Inspector still docks left). Foundation, independently
  verifiable.
- **Slice 2 — BandCityPanel scaffold + reservation.** New CanvasLayer + header
  chrome (stage/cycler/dock chooser/collapse) + dock persistence, reporting its
  reservation through slice 1. Empty/placeholder body. Dockable to all 4 edges,
  map + HUD reflow.
- **Slice 3 — content relocation.** Move the band branch of the drawer into the
  panel body; wire selection routing + cycler + map recenter + stage header; hide
  the inline band drawer (herds stay). 
- **Slice 4 (optional/polish).** T/B columnar reflow; content adapts tall vs wide.

## Verification
- ui_preview / map_preview harnesses (client-dev reads the PNGs). The full
  map-reflow + reservation is only fully exercisable in the running client
  (`scripts/run_stack.sh`), so slice 1's map inset and slice 2's dock reflow get a
  reasoning-plus-preview check here and a definitive in-app check by the human.
- No Rust/schema changes expected (pure client). `cargo xtask godot-build` only to
  confirm the native still builds.

## Docs on completion
- `clients/godot_thin_client/CLAUDE.md`: new "Band/City dockable panel" section; the
  reserved-space docking section (now 4-edge multi-reserver); the Key Scripts table
  (`BandCityPanel`); note the band drawer relocated out of the Occupants card.
