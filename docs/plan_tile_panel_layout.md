# Tile Panel Layout — one card, one list, one drawer

**Status:** design settled, implementation pending.
**Scope:** client-only (`clients/godot_thin_client`). No schema, no sim, no command changes.
**Supersedes:** the split Tile card + Occupants card described in
`clients/godot_thin_client/CLAUDE.md` → "Command Targeting" → *Selection split*.

---

## 1. The problem

A populated hex asks the left dock to render roughly **1,450 px** of content into a
column that is typically **700–900 px** tall, so the selection panel scrolls on any hex
worth looking at — and the action buttons are what falls below the fold.

Measured, from row counts + `HudStyle` margins:

| Block | Height |
|---|---|
| Tile card terrain rows (up to 15) | ~380 |
| `%ForageAssignControls` compose block | ~270 |
| Roster, 3 bands + 2 herds | ~255 |
| Herd detail drawer | ~310 |
| `%HerdAssignControls` compose block | ~270 |

The rows are cheap. **The two compose blocks are the cost** — band picker, six-rung
policy grid, hint, stepper, forecast, button — and they are rendered *inline, expanded,
permanently*, once for the patch and once for the herd.

Two facts that shape the fix:

- **Stockpile already ships hidden** (`Hud.gd`, `stockpile_panel.visible = false`), so
  today's left dock is really Tile + Occupants + Command Feed.
- **The player band's detail already left this panel** for the dockable Band/City panel
  (`docs/plan_band_city_dock.md` §3). The Occupants drawer only fills for foreign bands,
  expeditions and herds — so "units on the tile" is lighter than it looks.

## 2. The shape

**The hex becomes one card holding one selectable list and one capped drawer.**

The land is not a separate card sitting above the occupants; it is **the first row of the
list**, because it is the same kind of thing they are — a subject on this hex you can put
workers on. Selecting any row fills the single drawer beneath. Only one drawer is ever
open, and it is height-capped so the card cannot outgrow the dock.

Above the list, a **pinned chip strip** carries the terrain's standing condition, so the
facts you reason with while composing an action never scroll away.

```
┌ TILE  (66, 10) ─────────────────────────┐
│ [In sight] [Hospitable] [Temperate]     │   ← chips, pinned, wrap
│ [Fertile] [Verdant Basin]               │
│                                          │
│ ◈ Prairie Steppe        Savanna · 3 🌾  │   ← the land, always first
│ BANDS (2)                                │
│ ⛺ Band 1                       30 · 🌾  │
│ ⛺ Band 2                     18 · idle  │
│ WILDLIFE (1)                             │
│ 🐗 Wild Boar                  Big game  │
│ ─────────────────────────────────────── │
│ ▏ drawer for the selected row           │   ← capped, scrolls internally
│ ▏ …detail rows + its compose block      │
└──────────────────────────────────────────┘
```

**Why this over tabs or an accordion.** Both fix *three fixed sections*; a hex has an
unbounded number of occupants, so four bands still stack four drawers inside a tab. Rows
are 30 px and drawers are 300+; making the drawer the scarce, shared resource is what
actually bounds the panel. It is also the smallest structural change — the Occupants
roster is *already* a select-a-row-fill-a-drawer list. This gives it the land as a first
row and a height cap; it does not invent a new interaction.

**Deferred to a follow-on PR:** moving the compose block itself into a flyout over the
panel (the turn orb's popover and `NarrativeForkPanel` already establish the scrim-and-
dismiss pattern). That buys back another ~270 px and is orthogonal to this change — the
drawer cap makes the panel *correct* without it.

## 3. Scene changes (`src/ui/HudLayer.tscn`)

`OccupantsPanel` is **removed from the scene**; its four content nodes are reparented
into `TilePanel`'s `CardContent`. **Every `unique_name_in_owner` name is preserved**, so
`_build_forage_assign_controls`, `_build_herd_assign_controls`, `_build_expedition_panel`
and `_build_allocation_panel` keep working untouched — they simply live under a scroll
container now.

```
TilePanel (PanelCard, card_title "Tile", min 320w, visible=false)
└ CardContent (VBox, separation 8)
  ├ %TileChips     HFlowContainer                  ← new
  ├ %SubjectList   VBox, separation 4              ← was %RosterList, renamed
  └ %SubjectScroll ScrollContainer                 ← new; h-scroll off, v-scroll auto
    └ %SubjectBody VBox, separation 6              ← new
      ├ %TileDetail            RichTextLabel  (bbcode, fit_content, autowrap word)
      ├ %ForageAssignControls  VBox, separation 6, hidden
      ├ %OccupantDetail        RichTextLabel  (bbcode, fit_content, autowrap word)
      ├ %AllocationPanel       VBox, separation 6, hidden
      └ %HerdAssignControls    VBox, separation 6, hidden
```

`%RosterList` → `%SubjectList` is a rename because the list now holds the land too, and
"roster" is an occupants word. Update its references in `Hud.gd`.

`PanelDock` registration in `Hud._ready` becomes:

```gdscript
left_dock.add(tile_panel, 10)
left_dock.add(stockpile_panel, 20)
left_dock.add(command_feed_panel, 30)
left_dock.set_relevant(command_feed_panel, false)   # see §7
```

## 4. Selection model

Today the panel has two selection slots (`_selected_unit`, `_selected_herd`) and "the
tile is selected" is the *absence* of both. The land is now an explicit, selectable
subject, so add one member:

```gdscript
var _selected_subject: String = SUBJECT_LAND   # SUBJECT_LAND | SUBJECT_UNIT | SUBJECT_HERD
```

Set it wherever `_selected_unit` / `_selected_herd` are set today — `show_tile_selection`,
`show_unit_selection`, `show_herd_selection`, `reapply_selection`, `_select_roster_occupant`.
The two dicts stay authoritative for *which* unit/herd; `_selected_subject` only says which
kind of row is lit.

**The auto-select rule is deliberately unchanged, plus a fallback.** On a fresh tile click
with nothing selected: first roster unit → else first herd → **else the land**. Today a hex
with no occupants hides the Occupants card and leaves the Tile card showing terrain, which
*is* "the land is selected" — so this preserves current behaviour exactly rather than
introducing a new default. Clicking a band or herd marker on the map still routes through
`show_unit_selection` / `show_herd_selection` as it does now.

Selecting the land row emits **no** `roster_occupant_selected` — there is no occupant to
move the map ring to, and the hex selection outline already marks the tile.

## 5. The chip strip (`%TileChips`)

Chips carry the tile's **standing condition** — the one-word states you reason with while
composing. Numbers stay as rows in the land drawer, where their subject is.

| Chip | Source | Tint |
|---|---|---|
| Sight | `_tile_sight_line` value | `_sight_value_hex` (SIGNAL live, INK_DIM otherwise) |
| Habitability | `TileHabitability.rating_for` | `TileHabitability.hex_for_rating` |
| Climate | `TileClimate.band_for` | neutral INK_DIM — informational, never warning palette |
| Tags | `tags_text` | neutral; **skipped** when empty or `none` |
| Site | `site_name` | neutral; skipped when absent |

Each chip is skipped when its field is absent, exactly as the equivalent row is today — a
rehydrated tile must never show an invented rating. On an Unexplored hex only the Sight
chip renders.

Add **`HudStyle.chip_stylebox(border: Color) -> StyleBoxFlat`** — the palette is the
authority for chrome, and open-coding a rounded box in `Hud.gd` is the trap
`HudStyle` exists to prevent. Suggested: bg `Color(0, 0, 0, 0.25)`, 1 px border in the
passed colour at ~40% alpha, corner radius 999 (pill), content margins 7/2.

`HFlowContainer` so chips wrap at the 360 px dock width.

## 6. The land row and the drawer

**Land row** (`_build_land_row`, mirroring `_build_band_row` / `_build_herd_row`):

- **Label** = the biome name (`terrain_label`). Naming the row by its biome is more
  informative than a generic "The land", and it keeps the card title as the coordinates.
- **Glyph** = `FoodIcons.for_site(food_module, false, terrain_id)` when the tile carries a
  module, else `◈`. Same convention as the Band panel's source rows — a source reads
  identically in the panel and on the map. `◈` is text-presentation, per the line-art
  policy in `FoodIcons`.
- **Dot** = `_ecology_tier_color(patch_ecology_phase)` when a patch exists, else `INK_FAINT`
  — the same vitality vocabulary as band and herd rows.
- **Meta**, shortest true form: `N 🌾` when workers are assigned · the module label when a
  patch exists unworked · `No forage` when there is none.
- No group header above it; `BANDS (n)` / `WILDLIFE (n)` headers stay for their groups.

**Drawer.** `%SubjectScroll` is height-capped so the card fits the dock and scrolls
*internally* rather than dragging the whole dock. Reuse `DockScrollFit` rather than writing
new height math — but it currently measures a `RichTextLabel`, and our body is a VBox. Split
its existing math:

```gdscript
static func fit(scroll, label: RichTextLabel, dock_scroll, min_height, bottom_margin) -> void:
    fit_height(scroll, label.get_content_height(), dock_scroll, min_height, bottom_margin)

static func fit_height(scroll, content_height: float, dock_scroll, min_height, bottom_margin) -> void:
    # the existing body of fit(), with `cap` seeded from content_height
```

Call `fit_height(subject_scroll, subject_body.get_combined_minimum_size().y, left_scroll,
SUBJECT_DRAWER_MIN_HEIGHT, …)` after every drawer render and on viewport resize, mirroring
`_refit_right_dock`. `SUBJECT_DRAWER_MIN_HEIGHT` is a named const (~180).

`_height_reserved_below` counts only *visible* siblings, so the hidden command feed
contributes 0 and the drawer reclaims that room — no change needed there.

**Player band selected.** The drawer stays redirected to the Band/City panel exactly as
today, but the new layout makes the resulting empty drawer visible. Render a single dim
pointer line instead of a blank gap: *"Labor allocation is in the Band / City panel."*

**Unseen hex.** The fog behaviour is load-bearing and must survive verbatim: terrain rows
and chips render (geography is remembered knowledge), occupant rows do not, and the
`OCCUPANTS_UNKNOWN_REMEMBERED` / `OCCUPANTS_UNKNOWN_UNEXPLORED` message renders in the
drawer. An empty list is a claim of emptiness the client cannot back up — it must never be
shown on a hex the player cannot see. `_render_occupants_unknown` moves into the single
card; it is reworked, never deleted.

## 7. Command feed

Kept in code, **hidden by default**, toggled by a hotkey — exactly the pattern Victory
(`V`) and Terrain Types (`L`) already use. It holds six read-only receipts and **no verbs**,
so nothing has to be absorbed elsewhere.

- `left_dock.set_relevant(command_feed_panel, false)` at startup, so the dock reflows with
  no gap (a bare `visible = false` does not).
- Toggle persists to `user://narrative.cfg` `[hud_panels]` — **the same file** the legend,
  Victory and voice-register prefs use. Do not add a second prefs file.
- Hotkey: **`R`**. Confirm it is unbound in `Main`, `MapView` and `Hud` `_unhandled_input`
  before wiring; pick another free key and update this doc if it collides.
- Set `hotkey_hint = "R"` on the card so the header advertises it.

## 8. What must not regress

- **Fog gating.** `_tile_contents_unseen` stays the one test; occupants stay redacted on an
  unseen hex; **your own units stay listed even on an Unexplored hex** (a scouting party is
  excluded from fog reveal server-side and routinely stands on a tile it cannot see).
- **The compose builders are untouched.** `_build_forage_assign_controls`,
  `_build_herd_assign_controls`, `_build_expedition_panel`, `_build_allocation_panel` and
  every stepper/picker/forecast path keep their current behaviour and unique names. This
  change moves nodes and adds a list; it must not re-derive a single yield number.
- **No restated identity.** A drawer never repeats what its row already shows — the rule
  that removed `Unit` / `Size` / `Herd` / `Species` rows. The land drawer must therefore not
  re-print the biome, which is now the row label.
- **`roster_occupant_selected`** keeps its current contract (`kind`, `id`) so
  `MapView.select_occupant` and the map ring are unaffected.

## 9. Verification (`tools/ui_preview.tscn`)

New states:

| State | Asserts |
|---|---|
| `tile_panel_land` | land row selected, chips pinned, forage compose in the drawer |
| `tile_panel_herd` | herd selected, hunt compose in the drawer, land row still visible |
| `tile_panel_band` | player band selected → the Band/City pointer line, not a blank gap |
| `tile_panel_crowded` | 3 bands + 2 herds — rows all visible, drawer capped, card fits the dock |
| `tile_panel_no_forage` | land row reads `No forage`, no compose block |
| `tile_panel_unseen` | remembered hex — chips + land row + unknown-contents message, no occupant rows |
| `tile_panel_feed_shown` | command feed toggled on, dock reflows, nothing clipped |

Existing `food_tile` / `forage_*` / `herd_*` / `tile_sight_*` states exercise the same
builders and must keep passing; **their frames will change** (one card instead of two) —
that is the intended diff, not a regression. Re-read them after the change and confirm the
content is intact.

---

# Part 2 — the compose sheet (option E)

**Status:** IMPLEMENTED. Follows the Part 1 rebuild (`13880e8`), same branch. See
`clients/godot_thin_client/CLAUDE.md` → the selection-card section and the `ui/hud/ComposeSheet.gd`
row for the shipped shape.

## 10. Why

Part 1 bounded the panel by capping the drawer. It did not make the drawer *small* — the
two compose blocks are still ~270 px of always-expanded controls living permanently in a
column that also has to show the land, the roster and the detail rows. On a hex where you
are composing a hunt, the drawer is mostly picker.

Composing is **modal by nature**: you open it, make a decision, commit, and it is done.
So the read state keeps the column and the write state borrows space only while in use.

## 11. Shape

The drawer keeps the detail rows, gains a one-line **standing-assignment summary**, and
ends in a single `Assign … ▸` button. Pressing it opens a **compose sheet** floating over
the HUD; committing or dismissing returns to the read state.

```
  drawer (read state)                 sheet (write state)
  ┌──────────────────────┐            ┌────────────────────────┐
  │ Biomass   820 / 1100 │            │ ASSIGN HUNTERS  Boar ✕ │
  │ Range     1 tile     │            │ Band  [Band 1      ⌄] │
  │ Ecology   Thriving   │    ▸       │ [♻][⬆][⇄][💀][◎][🐄]  │
  │ ──────────────────── │            │ hint …                 │
  │ ♻ 2 hunters +10.67   │            │ Party  [−] 1 [+]       │
  │ [ Assign hunters ▸ ] │            │ forecast …             │
  └──────────────────────┘            │ [ Send Hunting Exp. ]  │
                                      └────────────────────────┘
```

**Which blocks move:** `%ForageAssignControls` (land) and `%HerdAssignControls` (herd)
only. `%AllocationPanel` stays inline — for an expedition it is two buttons and a callout,
and its band branch only renders in the no-Band/City-panel fallback.

## 12. Two traps, both already documented in this repo

**Use `AutoSizingPanel`, not `DockScrollFit`.** The root `CLAUDE.md` rule: a *free-floating*
panel measured against the **viewport** is `AutoSizingPanel.fit_to_content`; `DockScrollFit`
is for a card inside a dock's `VBoxContainer`. The sheet is free-floating, so it is
`AutoSizingPanel` — the opposite of what Part 1 needed. Picking the wrong one misbehaves
silently rather than failing.

**Nest the catcher, don't sibling it.** Reuse `NarrativeForkPanel` exactly: the sheet node
*is* a full-screen dismiss catcher (`MOUSE_FILTER_STOP`) with the card as a **child**, not a
sibling — siblings make the ordering ambiguous and the catcher swallows the card's own
clicks. Pin the catcher to the viewport **explicitly**; the full-rect anchor preset does not
settle in time and leaves a zero-size rect (`NarrativeForkPanel._pin_to_viewport`).

**No scrim.** `NarrativeForkPanel` dims because a fork is a story beat demanding attention.
An assignment is a working action composed *against* the map — the band's work-range ring,
the herd's position, the hunt reach are all live context. Use the catcher for click-outside
dismissal with **no dimming**. This is the one place the sheet deliberately departs from the
fork panel.

## 13. Hosting the controls

**Do not reparent the existing `%ForageAssignControls` / `%HerdAssignControls` nodes into
the sheet.** `PanelCard`'s contract note is explicit: reparenting silently clears
`unique_name_in_owner`, breaking every `%Name` lookup in the owner script.

Instead give the builders an explicit target:

```gdscript
func _build_forage_assign_controls(tile_info: Dictionary, target: VBoxContainer) -> void
func _build_herd_assign_controls(herd: Dictionary, target: VBoxContainer) -> void
```

The sheet passes its own content container. Every existing rebuild path (stepper tick,
policy click, band-picker change) keeps working because it re-runs the same builder against
the same target. The compose state members (`_forage_assign_count`, `_forage_assign_policy`,
`_hunt_assign_count`, `_hunt_assign_policy`, the autofill one-shots) are unchanged — the
sheet is a different *host*, not a different state model.

Gate-reason lines move **with** the picker: they explain the greyed buttons, so they belong
beside them.

## 14. The drawer's standing summary

Only when the source is actually staffed by the player faction. Reuse
`_source_yield_readout` — do **not** recompute a rate:

`♻ 3 foragers · +2.74 /turn` · `♻ 2 hunters · +10.67 /turn`

Policy glyph from `FoodIcons.for_policy`, warn/overdraw flags exactly as the Band panel's
Current-actions rows render them. Unstaffed → no summary row, just the button.

Button label: `Assign foragers ▸` / `Assign hunters ▸` (`Assign herders ▸` on a managed
herd — `_is_managed_hunt_source` already decides that noun).

## 15. Lifecycle

- **Open** on the drawer button. One sheet at a time; opening replaces any open one.
- **Close** on: commit (Assign / Send), the `✕`, a catcher click, or `Esc`.
- **Esc precedence.** `Main._unhandled_input` currently runs ESC → targeting-cancel → pause
  menu (Main.gd:661-667). The sheet becomes the **first** claimant: add
  `Hud.is_compose_sheet_open()` and check it before `is_targeting_active()`.
- **Targeting closes the sheet.** Move-band and send-expedition enter tile targeting; a
  floating sheet over the map while the player is asked to click a hex is a trap. Any
  `_pending_*` targeting flow starting must close the sheet first.
- **Selection changes close the sheet.** Picking another row, another hex, or another band
  invalidates the subject being composed.
- **A snapshot must NOT close it.** `reapply_selection` runs every turn; closing on it would
  make the sheet unusable during autoplay. The sheet re-renders in place against the fresh
  subject, and closes only if that subject is gone.

## 16. What must not regress

Everything in §8 still holds. Additionally:

- **No yield, forecast, ecology or gate logic may be re-derived.** This moves the host of
  existing controls. Every number and every gate reason must come from the same call it
  comes from today.
- The forage **range gate** and the herd **local-vs-expedition branch** are decided from the
  *selected band's* position; that threading is explicit today and must stay explicit.
- `Esc` must still cancel targeting and still open the pause menu when no sheet is open.

## 17. Verification

| State | Asserts |
|---|---|
| `tile_panel_compose_forage` | sheet open over the land, full policy grid + stepper + button, map still visible (no scrim) |
| `tile_panel_compose_herd` | herd sheet, expedition branch, raid forecast intact |
| `tile_panel_compose_gated` | locked rungs greyed **and** their gate reasons rendered in the sheet |
| `tile_panel_land` (updated) | closed state — summary line + `Assign foragers ▸`, drawer visibly shorter than Part 1 |
| `tile_panel_crowded` (updated) | closed state on a crowded hex; card is now well clear of the dock ceiling |

Behavioural assertions, in the style of the Part 1 sticky test — and each must be shown to
**fail** before it is trusted:

1. `Esc` with a sheet open closes the sheet and does **not** open the pause menu.
2. `reapply_selection` with a sheet open leaves it open.
3. Starting a move-band targeting flow closes the sheet.

---

# Part 3 — Move on the tile panel, and two corrections

**Status:** design settled, implementation pending. Follows Part 2, same branch.

## 18. Move a band from the tile panel

Selecting a player band in the subject list redirects its detail to the Band/City panel, so
the drawer holds only a pointer line. But **repositioning a band is a map action**, and the
player is already on the map with the hex open — making them cross to another panel to give
that order is the wrong shape.

Add a **Move** button to that drawer branch. `BandCityPanel` and `_build_allocation_sections`
are **not touched** — its own Orders section keeps its Move.

- Wire it directly to the existing `_on_move_band_pressed`. It resolves its target through
  `_resolve_assign_band()`, which returns `_selected_unit` whenever that is a player band —
  so it already moves **the band selected in the list**, which is the whole point on a hex
  carrying several. No new targeting logic, no change to `move_band_requested`.
- `ghost` treatment, and the Orders section's tooltip verbatim. (An earlier draft justified
  `ghost` as "matching the Orders section" — that was factually wrong: Orders' Move is
  `primary`. `ghost` is still right, for a different reason. `primary` means *the* thing to do
  on this surface, and on a band that is labor allocation, which lives elsewhere; repositioning
  is a secondary convenience. It also matches the **expedition** Move sitting in this same
  drawer, which is the consistency axis that matters here.)
- **Player resident bands only.** That drawer branch is already player-band-gated; a foreign
  band's orders are not ours to give.
- **Not** in the no-Band/City-panel fallback path — that renders `%AllocationPanel`, which
  already has its own Move, and a second one would double it.
- Expeditions already have Move here via `_build_expedition_panel`; leave that alone.

`Clear all` is deliberately **not** added. It returns every worker on the band to idle, which
is a heavier action than repositioning, and it belongs beside the labor allocation it clears.

## 19. The open-state Assign button must not wear `armed`

`HudStyle.apply_button(btn, "armed")` is the **destructive/warned** treatment — `DANGER`
border, `#f0c3bd` text — used for things like a slow raid the player is choosing anyway. "Its
compose sheet is open" is not a warning; it is a *live* state, and this HUD spells live in
SIGNAL cyan (the Sight chip, the selection accent, the turn orb's calm pulse).

Use **`primary`** while the button's own sheet is open and `ghost` at rest.

## 20. The land row's meta duplicates and truncates

The row currently renders the module name, which ellipsises to `Savanna Gras…` at dock width
while the drawer prints `Forage: Savanna Grassland — Savanna Track` and the sheet header reads
`ASSIGN FORAGERS  Savanna Grassland` — the same string three times, and the only truncated one
is the row.

The row's meta earns its space in the **closed** state, when another subject is selected and
the row is the only place the land reports anything. So make it the shortest true thing:

| Condition | Meta |
|---|---|
| staffed by the player | `3 🌾` — the worker count, the actionable fact |
| a patch, unstaffed | `0 🌾` |
| no patch | `No forage` |

The worker count comes from the same `_forage_assignment_of` lookup Part 2 introduced for the
standing summary — do not count assignments a second way.

**Correction, recorded because the first draft of this section was wrong.** §20 originally
specified the module label for the unstaffed case and blamed the ellipsis on the code. The code
was already correct at `13880e8`; the defect was in the *fixtures* — `tile_panel_crowded` left
`_player_bands` empty, so a hex a band was actively foraging fell through to the label branch.
That is fixed in the harness.

But the unstaffed case then still truncated, because a module label does not fit the row at dock
width. Hence `0 🌾`: the row already **leads with that module's own glyph**, so the label was
restating it, and it was the only one of the three renderings that truncated (the drawer's
`Forage:` row and the sheet header both print it whole). The zero form is parallel to the
staffed one, so an unworked patch reads at a glance rather than by comparison.
