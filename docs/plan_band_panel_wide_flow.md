# Band/City panel — wide-dock multi-column content flow

Status: implemented + preview-verified (branch `worktree-feat+band-panel-wide-flow`),
pending live check + push/PR. Follow-up to the Band/City dock PR (#103), which shipped a
bounded two-column wide layout as a stopgap. Tracked in `TASKS.md` → Client/UI.

## Problem

When the Band/City panel is docked **top/bottom** (wide, short), the body today is
a fixed two-column layout (`SUMMARY_COLUMN_WIDTH` summary | `ALLOC_COLUMN_WIDTH`
allocation) packed left. The whole labor allocation is **one column**, so on a wide
/ ultrawide strip most of the width is empty on the right and it reads poorly.

The allocation is built flat into `get_band_alloc_container()` by
`Hud._build_allocation_panel` (a single VBox: Workers header → Current actions →
Band roles → Orders → Send expedition), so its internal sections can't spread
across columns.

## Goal

When docked wide, the band content **flows into multiple columns to fill the
strip**. Tall docks (left/right) keep the single vertical stack, unchanged.

## Design — section blocks + a mode-arranging host

Break the band content into **discrete section blocks** (self-contained `Control`s)
and hand them to the panel, which arranges them by dock aspect. This replaces the
current "Hud fills one alloc container" contract with a "Hud hands the panel an
ordered list of section blocks" contract.

### The section blocks (Hud builds, in `_render_band_into_panel`)
Ordered list of blocks (each a self-contained VBox/Control, fixed/max column width):
1. **Summary** — the `BandDetail` RichTextLabel (unchanged content).
2. **Active expeditions** — the existing `_build_panel_expeditions` output (its own
   block; omitted when the band has none).
3. **Workers** — the `Population N · Workers W (Idle n)` header.
4. **Current actions** — the staffed forage/hunt worker-stepper rows (or the
   empty-state hint).
5. **Band roles** — Scout + Warrior stepper rows + their hints.
6. **Orders** — Move / Clear all.
7. **Send expedition** — the party stepper + policy picker + send buttons (omitted
   when idle == 0, as today).

**Refactor `_build_allocation_panel` to produce blocks 3–7 as discrete section
VBoxes instead of flat children of one container.** CRITICAL: the per-row wiring —
worker steppers, band-picker, optimistic **pending** styling, expedition controls,
`_emit_assign_labor` closures, Move/Clear handlers — must be **byte-for-byte
preserved**; only the *parent node* of each row changes (row → its section VBox
instead of the flat container). This is the delicate part (we recently fixed
aliasing + foreign-selection bugs here); do not alter the labor logic.

### The contract change (Hud ↔ BandCityPanel)
- Replace `get_band_alloc_container()` / `get_band_detail_label()` /
  `get_band_expeditions_container()` fill-a-container hosting with:
  **`BandCityPanel.set_band_sections(blocks: Array)`** — Hud passes the ordered
  block array; the panel takes ownership, frees the previous blocks, and arranges
  the new ones in the active layout.
- Hud's `_render_band_into_panel` builds the block array and calls
  `set_band_sections`. The summary RichTextLabel is now Hud-built per render (or a
  stable node handed in the array) — either is fine as long as ownership is clear.
- `set_band_present(false)` / empty-state handling stays (empty array → the panel
  shows its empty-state / hides).

### The panel's mode arranger (`_relayout_body` / a new `_arrange_sections`)
- **Tall (LEFT/RIGHT):** a vertical `ScrollContainer` → `VBoxContainer` holding the
  blocks in order — identical to today's stack.
- **Wide (TOP/BOTTOM):** a `VFlowContainer` (bounded to the strip height) holding
  the blocks — it flows them top→bottom and **wraps to a new column** when a column
  fills, so the sections spread across the width. Each block keeps a fixed/max
  column width (named const) so columns are tidy and the stepper `−/+` controls
  stay beside their labels. Wrap the flow in a horizontal `ScrollContainer` only if
  the columns can still exceed the width (defensive).
  - NOTE: `VFlowContainer` is correct **only** in the wide/bounded-height case. In
    the tall unbounded-height scroll it collapses to min height and mis-columns
    (confirmed in the #103 work) — so tall stays a plain VBox. The two modes use
    **different container types**, which is why blocks are reparented rather than a
    single container being reparented.
- On a **dock change** that flips tall↔wide, the panel re-arranges the **same block
  nodes** into the new container (reparent) — no Hud re-render needed (the panel
  holds the block refs). On a **Hud re-render** (stepper edit / snapshot),
  `set_band_sections` swaps in fresh blocks and re-arranges.
- Collapsed state unchanged (thin rail, blocks hidden).

## Risks / guardrails
- The allocation wiring is delicate — preserve it exactly (see the bugs fixed in
  #103: dict-aliasing blank, foreign-selection blank, pending styling). Verify the
  Bug-1/Bug-2 preview frames still pass.
- Tall docks must look identical to today.
- Ownership: exactly one owner per block at a time; freeing on re-render must not
  double-free or leak (the block-handoff makes ownership explicit — the panel owns
  what it's given, frees on the next `set_band_sections`).

## Verification
- `cargo xtask godot-build` + `--import` + `marker_field_guard` PASS.
- `band_panel_preview`: left/right unchanged (vertical stack); **top/bottom now show
  the sections flowed into multiple columns filling the width** (not one column with
  dead space); collapsed intact; the Bug-1 (stacked-with-inspector) and Bug-2
  (foreign-tile stepper edit stays populated) frames still pass. Render at a wide
  window so the multi-column wrap is exercised. Read the PNGs.
- Live (human): dock top/bottom on a wide screen and confirm the fill reads well.

## Docs on completion
- `clients/godot_thin_client/CLAUDE.md` → "Band/City dockable panel" → "Responsive
  body": update to the section-block model (tall VBox stack vs wide VFlowContainer
  column-flow; `set_band_sections` contract).
- `TASKS.md`: check off the multi-column-fill item.
