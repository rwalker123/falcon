---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{SelectionCardController,SubjectDrawerController}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{HudSelectionState,hud_selection_vocab}.gd"
---

<!-- Extracted verbatim from lines 178-178;180-180;1612-1775 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The selection card — ONE card, ONE list, ONE drawer

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/SelectionCardController.gd` | `RefCounted` controller (HUD decomposition Phase 2b, `docs/plan_hud_decomposition.md`) owning the selection card's **IDENTITY/LIST half** — the tile-card header, the pinned condition-**chip** strip, and the whole **roster / subject list** (LAND row + Bands/Wildlife sub-groups), plus the roster-row clicks and the fresh-hex auto-select. The state-isolated half (zero drawer coupling, zero shared compose/band-tint state), so it split off ahead of the drawer (Phase 2c-2b then took the drawer's COMPOSE half into `DrawerComposeController`; HudLayer keeps the drawer's RENDER DISPATCH). Hud holds it as `_selectioncard`, constructed in `_ready` after `_turnorb`, handed the three card nodes (`tile_panel`/`tile_chips`/`subject_list`) + the SAME `_selection`/`_band_labor` model instances (BY REFERENCE) and NOTHING ELSE — the one injected Callable it used to take (`_alloc_hint_label`) went away when that factory moved to the all-`static` `HudWidgets`, which it now calls directly; it reads the forage/hunt worker counts straight off `_band_labor.workers_for_forage`/`workers_for_hunt` (those readers moved onto the labor model), and `_is_player_unit` is a trivial private copy. Owns the moved Phase-2a diff caches `_tile_chip_slots` / `_subject_row_keys`, so the in-place chip/row updaters travel with it and a same-tile restate still patches nodes rather than tearing them down (`tile_panel_no_flash`). **The render seam:** `HudLayer._render_selection_panel` stays the ORCHESTRATOR — it resets the band-tint scalars, calls `_selectioncard.render(tile_info)` (roster + tile-card + chips + auto-select + list), then `_drawer.render_subject_drawer()` (`SubjectDrawerController`, Phase 2c-3). **The row-click seam:** a roster/land click mutates `_selection` and emits the controller's OWN `subject_changed` → `HudLayer._on_selection_subject_changed` (close compose sheet → re-render), plus `roster_occupant_selected(kind, id)` **RELAYED** onto `HudLayer.roster_occupant_selected` → Main (the TurnOrbController pattern); the auto-pick emits only the relayed `roster_occupant_selected` (it runs mid-render). Publics HudLayer's band/labor navigation calls back into: `render`, `select_roster_occupant`, **`select_land_subject`** (the `note_choice_tile` + `select_land` pair BOTH ways of choosing the land go through — the land row here, and the map's select-then-cycle land stop via `Hud.show_land_selection`; it emits nothing, each caller announcing its own), `find_roster_unit`/`find_roster_herd`, `tile_contents_unseen`. Behaviour identical to the old inlined selection-card code |
| `ui/hud/SubjectDrawerController.gd` | `RefCounted` controller (HUD decomposition Phase 2c-3, `docs/plan_hud_decomposition.md`) owning the selection card's **DRAWER RENDER DISPATCH** — the last piece of the selection card to leave `Hud.gd`, after `SelectionCardController` (identity/list) and `DrawerComposeController` (compose). It holds the **one-drawer dispatch** (`render_subject_drawer` → `_render_land_drawer` vs `_render_occupant_drawer` → `_render_unknown_contents_note`), the **land-drawer content producer** (`_tile_terrain_lines` + its `_graze_stock_lines` / `_stock_value` leaves), the **`%AllocationPanel` occupant branches** (`_build_allocation_panel` — the no-dock fallback stacking `BandPanelController`'s three zone builders — plus `_build_expedition_panel` / `_build_band_move_actions` / `_make_band_move_actions`), and the **height-capping fit path** (`fit_subject_drawer`). It owns the drawer's RENDER diff state (`_tile_detail_lines_cache` + the fit-flight/last-height guards). Hud holds it as `_drawer`, constructed in `_ready` after `_bandpanel` (it dispatches into `_bandpanel` and `_drawercompose`). **THE MOVE VERB IS A TYPED COLLABORATOR, not a Callable** — the drawer's Move button `.connect()`s straight to `TargetingController.begin_move_band` (which owns `_pending_move_band` + the banner; the targeting machinery has three other modes). `_is_player_unit` is a trivial private COPY (the `SelectionCardController` / `BandPanelController` precedent). Collaborators: the SAME `_selection` / `_band_labor` model instances BY REFERENCE, `_selectioncard` (`tile_contents_unseen` + `selected_terrain_label`), `_drawercompose` (`refresh_compose_sheet` / `build_forage_drawer_actions` / `build_herd_drawer_actions`), `_bandpanel` (`has_panel` / `render_band` / the three zone builders / `confirm_recall_expedition`), `_banddetail` (`unit_summary_lines`), and the HUD CanvasLayer as the **host** its fit awaits a frame through (a `RefCounted` has no `get_tree()`). **The drawer scene nodes it writes stay `@onready` on HudLayer and are passed in** (`tile_detail` / `occupant_detail` / `allocation_panel` / `herd_assign_controls` / `forage_assign_controls` / `subject_body` / `subject_scroll` + read-only `left_dock_scroll`) — a `%Name` node loses `unique_name_in_owner` if reparented. **THE FIT PATH IS THE HIGH-RISK PIECE:** `fit_subject_drawer` `await`s `_host.get_tree().process_frame` and is wired to `subject_body.minimum_size_changed` + `viewport.size_changed.bind(true)` (the force-past-the-gate flag) in HudLayer's `_ready`, plus `_refit_left_dock`'s forced refit — all repointed here. **The two calling-in seams STAY on HudLayer:** `_render_selection_panel` (reflectively-reached coordinator core) and `_refresh_disclosure_hosts` (the two-host fan-out — it also renders the band panel) both call `_drawer.render_subject_drawer()`. Word tables/formats/thresholds stay on `HudLayer` and are read back as `HudLayer.X`. Behaviour identical to the old inlined drawer-dispatch code |
- **The selection card — ONE card, ONE list, ONE drawer** (`Hud.gd`,
  `docs/plan_tile_panel_layout.md`; this SUPERSEDES the earlier split Tile + Occupants
  cards). A populated hex used to ask the left dock for ~1450px of content — two inline,
  permanently-expanded compose blocks — so the action buttons fell below the fold. The hex is
  now **one left-dock `PanelCard`** (`TilePanel`, priority 10, title = the coordinates):
  - **`%TileChips`** — a pinned `HFlowContainer` of the tile's STANDING CONDITION, so the facts
    you reason with while composing never scroll away: Sight (`_sight_value_color`, SIGNAL when
    live) · Habitability (`TileHabitability.rating_for`/`color_for`) · Climate (neutral INK_DIM —
    informational, never the warning palette) · Tags (skipped when empty/`none`) · Site. **Each
    chip is skipped when its field is absent**, exactly as the equivalent row is, so a rehydrated
    tile never shows an invented rating; on an Unexplored hex ONLY the Sight chip renders. Chrome
    comes from `HudStyle.chip_stylebox(border)` — the palette owns it, never an open-coded box.
    **A chip FACE is a word, not a sentence** — `Remembered — not in sight now` was the widest
    element in the strip, so the Sight chip reads `In sight` / `Remembered` / `Unexplored`
    (`_tile_sight_chip_value`) and the full sentence moves to `_make_chip`'s optional `tooltip`
    (the only chip that takes one; the rest stay mouse-transparent). One value behind both forms.
    **THE CHIPS REPLACE THOSE ROWS, THEY DO NOT ACCOMPANY THEM** — see `_tile_terrain_lines`.
  - **`%SubjectList`** — the selectable list, with **the LAND as its first row**
    (`_build_land_row`, no group header) above the `Bands (N)` / `Wildlife (N)` sub-groups. The
    land is the same KIND of thing they are — a subject on this hex you can put workers on. Its
    label is the BIOME name, its glyph the tile's food-module icon (`FoodIcons.for_site`, the
    same one the map marker draws) or the neutral `◈`, its dot the patch's ecology tier, and its
    meta the shortest true form: `N 🌾` staffed · else the module label · else `No forage` (gated
    on the module KEY, never its `"None"` label). Selecting it emits
    **`roster_occupant_selected("land", -1)`** — an ADDITIVE third kind on the existing `(kind, id)`
    contract (Main forwards it blindly to `MapView.select_occupant`, so Main needed no change). It
    moves no ring — there is no occupant, and the hex outline already marks the tile — but MapView's
    `"land"` branch **clears `selected_unit_id` / `selected_herd_id`** (leaving `selected_tile` alone —
    the land IS that hex — while `cycle_index` follows the pick, the land being the map cycle's last
    stop, so the next map re-click continues from it to the top of the ring). **That clear is what
    makes the land selectable at all on an occupied hex:** `refresh_selection_payload`
    answers `kind: "unit"` for as long as `selected_unit_id >= 0`, so without it the per-snapshot
    refresh restored the band and the tile branch was never reached.
    **The map click also REACHES the land, on any hex that has an occupant.** The select-then-cycle
    ring is everything this list shows — bands, then herds, then the land last — so re-clicking a hex
    walks to the land row exactly as clicking it does, and MapView announces it with its own
    `land_selected` signal relayed through `Main` to **`Hud.show_land_selection`**, which records the
    choice tile through `SelectionCardController.select_land_subject` (the same pair the land ROW
    click uses). Without that recording the auto-select rule below fires on the two empty occupant
    dicts the land state consists of and puts the first band straight back. See
    "Select-then-cycle" for the ring's order and the empty-hex case.

    **The map click deselects the same way.** Clicking a hex with no band or herd while an occupant
    is selected runs `MapView._handle_entity_selection`'s clear branch, which drops
    `selected_unit_id` / `selected_herd_id` and emits `selection_cleared` — and leaves `selected_tile`
    on the hex just clicked, because `handle_hex_click` ran `_emit_tile_selection` one call earlier
    and that hex IS the new selection. So deselecting an occupant on the map keeps the tile selected
    (its white outline stays drawn) and falls back to the LAND card: `refresh_selection_payload`
    reaches its `{"kind": "tile"}` branch, and `Hud.clear_selection` re-renders from the `tile_info`
    that `show_tile_selection` populated on the same click. Clearing the tile there instead left the
    hex with only its faint hover outline and no card (issue #405). Guarded by ui_preview
    `tile_panel_deselect_keeps_tile`, which drives the real path — a MapView with fog off, Main's
    three signals wired, a click on the herd then a click on empty land — and asserts the tile, the
    cleared occupant and the `"tile"` payload. Verified to FAIL with the tile clear restored.
  - **`%SubjectScroll` / `%SubjectBody`** — the ONE drawer, filled by whichever row is lit and
    **height-capped** via `DockScrollFit.fit_height` against the room left in the dock
    (`SUBJECT_DRAWER_MIN_HEIGHT` floor), so a crowded hex scrolls INSIDE the drawer instead of
    dragging the dock. Only one drawer is ever open — rows are ~30px, a compose block is 300+, so
    making the drawer the scarce shared resource is what bounds the card. The fit **waits a whole
    frame** (not just `call_deferred`): the drawer's content height is a function of its width, so
    a measurement taken before the new subject lays out reports the previous one's wrapping. A 1px
    `HudStyle.hairline_stylebox()` rule (`%SubjectDivider`, the same LINE_SOFT weight
    `header_stylebox` draws under a title) marks where the list ends and the drawer begins —
    without it the drawer's first row runs straight on from the last wildlife row.
  - **The LAND drawer renders only what a CHIP CANNOT CARRY** (`_tile_terrain_lines`, whose ONE
    caller is `_render_land_drawer` — the map hover tooltip builds its own text): Height · the
    river lines · **`Foraging` and its indented basket, then `Grazing`** (the two food webs, named
    for who eats them and rendered adjacent — `land-readouts.md` → "The tile card's TWO FOOD-WEB
    ROWS"; the `Forage:` module row is deleted, and each web's ecology phase now rides its own stock
    row) · Crop / Cultivation / Field. **It emits ROWS and no FoW sentence at all** — each unseen
    state's one sentence is the roster's own unknown-contents note, rendered directly beneath this
    label. The drawer used to add a second one in BOTH states (`Last seen — information incomplete.
    Scout to update.` / `Not yet scouted — send a band to reveal this area.`), each immediately above
    a note saying the same thing, with the Sight chip saying it a third time; see `land-readouts.md` →
    "An unseen hex says so ONCE, and promises nothing it cannot do". **An unexplored hex therefore produces
    NO rows and the label hides** (`_render_land_drawer` gates on `lines.is_empty()`) — a visible
    empty `RichTextLabel` still claims its line height and would read as a blank gap. **That same
    emptiness is passed on as `_render_unknown_contents_note(force)`**, which without it skips itself
    on a non-empty roster and left the whole LAND drawer blank on an Unexplored hex holding your own
    party (every child hidden at once) — see `land-readouts.md` → "An unseen hex says so ONCE".
    Sight / Habitability / Climate / Tags / Site are the CHIPS'
    and `Biome` is the land ROW's own label, so printing any of them here restated the strip
    verbatim (§8's "no restated identity"). The `TILE_SIGHT_KEY` / `Habitability` cases in
    `DetailFormat.detail_bbcode` stay — it is a shared key→tint registry every detail surface consults.
  - **THE DRAWER IS THE READ STATE; THE COMPOSE SHEET IS THE WRITE STATE** (Part 2 of
    `docs/plan_tile_panel_layout.md`, §10-§17). Capping the drawer bounded the card but did not make
    it SMALL — the two compose blocks were still ~270px of always-expanded picker sitting permanently
    in a column that also has to show the land, the roster and the detail rows. Composing is **modal
    by nature** (open, decide, commit, done), so `%ForageAssignControls` / `%HerdAssignControls` now
    end at a one-line **standing-assignment summary** + an **`Assign foragers ▸` / `Assign hunters ▸`
    / `Assign herders ▸`** button (`_build_forage_drawer_actions` / `_build_herd_drawer_actions`),
    and the block itself renders into the floating `ui/hud/ComposeSheet.gd`. `%AllocationPanel` stays
    INLINE (for an expedition it is two buttons and a callout).
    - **The builders were NOT reparented — they were PARAMETERISED.** `_build_forage_assign_controls(
      tile_info, target)` / `_build_herd_assign_controls(herd, target)` take an explicit target
      container, because reparenting a `%Name` node silently clears `unique_name_in_owner` and breaks
      every lookup in the owner script (`PanelCard`'s contract note). Every rebuild path (stepper
      tick, policy click, band-picker change) re-runs the same builder against the same target, so the
      compose state members (`_forage_assign_*` / `_hunt_assign_*` and the autofill one-shots) are
      untouched — the sheet is a different HOST, not a different state model. **Gate-reason lines
      travel WITH the picker**: they explain the greyed buttons, so they belong beside them.
    - **Extend-pen stays in the DRAWER.** It is a one-click standing action on a built pen, not a
      compose flow; hiding it behind a sheet you must open first would be worse. So
      `_build_herd_drawer_actions` renders it (or the "Fencing N%" badge) directly.
    - **The standing summary reuses `SourceForecast.source_yield_readout` — it never recomputes a rate.**
      `♻ 3 foragers · +2.74 /turn`, policy glyph from `FoodIcons.for_policy`, and the SAME two
      INDEPENDENT flags a Band-panel Current-actions row wears: the ⚠ overdraw (ecological, the
      sim-answered `overdraws`) and the `· only N of M working` overstaff note (labor). `has_yield` is
      the one key the readout needs that is not on the wire assignment, so it is set locally; every
      number comes off the assignment the sim sent. Unstaffed → no summary row, just the button.
    - **LIFECYCLE.** Opens on the drawer button; one sheet at a time. Closes on commit, the `✕`, a
      catcher click, `Esc`, a **selection change** (`show_*_selection` / `_select_roster_occupant` /
      `_on_land_row_selected` / `clear_selection`) or a **targeting flow starting** (`_on_move_band_
      pressed` / `_on_send_expedition_pressed` / `_on_pick_quarry_pressed` — a sheet floating
      over the map while the player is asked to click a hex is a trap). **A SNAPSHOT MUST NOT CLOSE
      IT** — `reapply_selection` runs every turn and closing would make the sheet unusable under
      autoplay; `_refresh_compose_sheet` (called from `_render_subject_drawer`, the snapshot
      chokepoint, and again from `update_band_alerts` so a staffing change lands too) re-renders it
      IN PLACE and closes only when the composed subject is actually gone. `close_compose_sheet` is
      idempotent, and `ComposeSheet.closed` is what clears `_compose_kind`/`_compose_subject`, so the
      two can never disagree about whether a sheet is open.
    - **ESC PRECEDENCE.** `Hud.is_compose_sheet_open()` is checked BEFORE `is_targeting_active()` —
      the sheet is the innermost surface. The chain is `Main.escape_claimant(pause_open, compose_open,
      targeting)`, a pure static extracted so the ORDER is assertable without standing up the app
      scene; `Main._unhandled_input` matches on its answer.
    - **Nothing is re-derived.** Every yield, forecast, ceiling and gate reason comes from the same
      call it came from when the block lived in the drawer, and the forage range gate / herd
      local-vs-expedition branch still read the **selected band's** position, explicitly threaded.
  - `_selected_subject` (`SUBJECT_LAND|UNIT|HERD`) says which KIND of row is lit;
    `_selected_unit`/`_selected_herd` stay authoritative for WHICH. **The auto-select rule is
    unchanged plus a land fallback**: first roster unit → else first herd → else the land — but it
    fires **only where the player has not already chosen on THIS hex**. `_subject_choice_tile`
    (set by `_select_roster_occupant` and the land-row click, `(-1,-1)` = never) is what tells a
    fresh hex from a decided one: choosing the LAND row clears both occupant dicts, so without it
    the per-snapshot `reapply_selection("tile", …)` re-ran the default and **stole the selection
    back to the first band**, making the land unselectable on any occupied hex. A new hex has
    different coords, so the default is preserved exactly. **This guard is only half the fix** — it
    covers the `reapply_selection("tile", …)` path, which on an occupied hex is only ever reached
    because the land row also clears MapView's own occupant selection (see `%SubjectList`). Guarded
    by ui_preview `tile_panel_land_sticky`, which drives the REAL path — it instances MapView, wires
    the two signals Main wires, clicks the hex, clicks the land row and feeds back whatever
    `refresh_selection_payload` answers (never a hardcoded `"tile"`, which would assert a path the
    bug cannot reach). Verified to FAIL with MapView's `"land"` branch removed.
  - Each occupant row is a `Button` hosting a mouse-transparent
  HBox — a selection accent, a **vitality dot**, name, size, and (bands) an
  activity glyph; a **wildlife** row reads **species + its STAFFING** — the hunters on the herd in
  the same `<count> <glyph>` form the land row uses (`🦌 Red Deer   1 🏹`, twin of `◈ Savanna   2 🌾`),
  with the unworked-but-huntable form `0 🏹` and *no* meta at all on a non-huntable herd. The
  **size class** moved into the herd drawer's first row (`Size: Big game`) because the row's one
  meta slot now belongs to the count — but a **predator reads `Size: Big predator`, not `Big game`**
  (a carnivore is a hunter, not quarry; `DetailFormat` branches the `"%s game"`/`"%s predator"` format
  AND the wild-ceiling hint `Wild game`/`Wild predator — hunt only` on `is_predator`, the same
  `prey_sense_radius > 0` signal the prey-sense ring keys on — Predators Phase 1a). **A detail row never restates what its
  roster row already shows** (the same rule the Band/City panel header follows). The roster
  row IS the identity line — name + size/staffing — so every drawer dropped
  the rows that echoed it: band → `Unit` + `Size`; herd → `Herd` / `Species`
  (the name appeared three times); expedition → `Unit` + `Party` (`Party`
  printed the same `size` field the row's meta shows). **THE FAUNA ID IS A DATABASE KEY AND IS
  NEVER RENDERED** (`game_fowl_27` means nothing to a player and crowded out the two things that
  do). It briefly rode the row as a dim meta on the theory that the command feed named herds by
  it — the right fix was to stop the FEED leaking it (`Main._on_hud_send_hunt_expedition` now
  notes `fauna_label`, the species, while the command line keeps `fauna_id`), not to teach the
  player the key. It stays **data**: the row's `pressed` bind and every `assign_labor` / `tame` /
  `send_hunt_expedition` address the herd by it. Renders of it elsewhere are **fallbacks only**
  (`SourceForecast.herd_display_name` / `_herd_label_for_id` reach for `id` only when species AND label are
  both missing) — never the normal path. What's left in a drawer is only what the row can't show — herd: Size / Herd (the stock pair, with
  its ecology phase riding it — see `herd-readouts.md`) / Husbandry / Corral; expedition: Mission / Target / Leaves standing / Phase / Carried /
  Position. **A herd states NO `Position`**: `herd_summary_lines` renders in this drawer and
  nowhere else, and the card's own `TILE (x, y)` header sits two rows above it, so the row was the
  same coordinate pair twice on one card. Its `Next waypoint` is a different fact — where the herd
  is HEADING — and stays. The expedition's `Position` is NOT the same row: a party is somewhere
  other than the tile whose card you are reading. **The expedition's `Phase` keeps its WORDS** — the compact
  Active-expeditions row is where the glyph vocabulary belongs; the drawer IS the disclosure. **Its
  `Policy` row is gone**: a raid names a FLOOR, so `BandDetailLines` renders `Leaves standing: NN%`
  off `expedition_floor`. There is no policy word left to keep. In the drawer,
  `%OccupantDetail` is the selected occupant's
  **detail** for **herds/expeditions** (`_herd_summary_lines` +
  `%HerdAssignControls`; expedition → `_build_expedition_panel` into
  `%AllocationPanel`). **Player-band detail relocated into
  the dockable `BandCityPanel`** (see **Band/City dockable panel** below): the list
  still lists the band, but its summary + labor allocation render in the panel — the drawer
  renders the one-line `BAND_PANEL_POINTER_TEXT` pointer instead, since an empty drawer is now
  VISIBLE furniture and would read as a rendering fault. **One order stays here beside that pointer:
  a ghost `Move`** (`_build_band_move_actions`, `docs/plan_tile_panel_layout.md` §18) — repositioning
  is a MAP action and the player is already on the map with this hex open, so crossing to another
  panel to give it is the wrong shape. It is wired straight to `_on_move_band_pressed`, which
  resolves through `_resolve_assign_band()` and therefore targets **the band selected in THIS list**
  — the whole point on a hex carrying several. It shares the `%AllocationPanel` host with
  `_build_expedition_panel` / `_build_allocation_panel`, which are mutually exclusive branches, so
  the no-panel fallback path's own Orders `Move` is never doubled (asserted in ui_preview). Player
  resident bands only, and `Clear all` is deliberately NOT here — returning every worker to idle is
  a heavier action that belongs beside the labor allocation it clears. `BandCityPanel` and
  `_build_allocation_sections` are untouched. Selecting a row (`_on_roster_row_selected`) re-homes the
  selection and emits `roster_occupant_selected(kind, id)`; **Main forwards it to
  `MapView.select_occupant`, which moves the map selection ring** (sets
  `selected_unit_id`/`selected_herd_id`) with no hex click. A fresh tile click
  auto-selects the first occupant through the same path. The **vitality dot is
  unified** across map/roster/drawer: a band's dot uses `BandFoodStatus.color_for_turns`
  (`turns_of_food` → green/amber/red), a herd's uses `_ecology_tier_color`
  (`ecology_phase` → thriving green / stressed amber / collapsing red), sharing the
  exact `HudStyle` HEALTHY/WARN/DANGER constants. Non-player bands list with a neutral
  dot and no allocation panel (their larder/orders aren't ours to see). (The Tile card
  has no camp action — the `found_camp` command was removed end-to-end.)
