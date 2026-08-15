---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{SelectionCardController,SubjectDrawerController}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{HudSelectionState,hud_selection_vocab}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/DetailFormat.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/SourceForecast.gd"
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
| `ui/hud/SubjectDrawerController.gd` | `RefCounted` controller (HUD decomposition Phase 2c-3, `docs/plan_hud_decomposition.md`) owning the selection card's **DRAWER RENDER DISPATCH** — the last piece of the selection card to leave `Hud.gd`, after `SelectionCardController` (identity/list) and `DrawerComposeController` (compose). It holds the **one-drawer dispatch** (`render_subject_drawer` → `_render_land_drawer` vs `_render_occupant_drawer` → `_render_unknown_contents_note`), the **land-drawer content producer** (`_tile_terrain_lines` + its `_graze_stock_lines` / `_stock_value` leaves), the **`%AllocationPanel` occupant branches** (`_build_allocation_panel` — the no-dock fallback stacking `BandPanelController`'s three BAND zone builders (a band declares three zones; the faction page's fourth is `FactionRollup`'s and never reaches this host) — plus `_build_expedition_panel` / `_build_band_move_actions` / `_make_band_move_actions`), and the **height-capping fit path** (`fit_subject_drawer`). It owns the drawer's RENDER diff state (`_tile_detail_lines_cache` + the fit-flight/last-height guards). Hud holds it as `_drawer`, constructed in `_ready` after `_bandpanel` (it dispatches into `_bandpanel` and `_drawercompose`). **THE MOVE VERB IS A TYPED COLLABORATOR, not a Callable** — the drawer's Move button `.connect()`s straight to `TargetingController.begin_move_band` (which owns `_pending_move_band` + the banner; the targeting machinery has three other modes). `_is_player_unit` is a trivial private COPY (the `SelectionCardController` / `BandPanelController` precedent). Collaborators: the SAME `_selection` / `_band_labor` model instances BY REFERENCE, `_selectioncard` (`tile_contents_unseen` + `selected_terrain_label`), `_drawercompose` (`refresh_compose_sheet` / `build_forage_drawer_actions` / `build_herd_drawer_actions`), `_bandpanel` (`has_panel` / `render_band` / the three zone builders / `confirm_recall_expedition`), `_banddetail` (`unit_summary_lines`), and the HUD CanvasLayer as the **host** its fit awaits a frame through (a `RefCounted` has no `get_tree()`). **The drawer scene nodes it writes stay `@onready` on HudLayer and are passed in** (`tile_detail` / `occupant_detail` / `allocation_panel` / `herd_assign_controls` / `forage_assign_controls` / `subject_body` / `subject_scroll` + read-only `left_dock_scroll`) — a `%Name` node loses `unique_name_in_owner` if reparented. **THE FIT PATH IS THE HIGH-RISK PIECE:** `fit_subject_drawer` `await`s `_host.get_tree().process_frame` and is wired to `subject_body.minimum_size_changed` + `viewport.size_changed.bind(true)` (the force-past-the-gate flag) in HudLayer's `_ready`, plus `_refit_left_dock`'s forced refit — all repointed here. **The two calling-in seams STAY on HudLayer:** `_render_selection_panel` (reflectively-reached coordinator core) and `_refresh_disclosure_hosts` (the two-host fan-out — it also renders the band panel) both call `_drawer.render_subject_drawer()`. Word tables/formats/thresholds stay on `HudLayer` and are read back as `HudLayer.X`. Behaviour identical to the old inlined drawer-dispatch code |
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
  both missing) — never the normal path.
  **AND THERE IS A FOURTH CASE, which is the one that actually leaked** (issue #378): not a herd
  whose strings are empty, but **no herd dict at all**. `_herd_label_for_id`'s three tiers — the tile
  roster, the selected herd, the snapshot herd list — every one of them needs the herd to be in a live
  array, and a **hunting party's own quarry is guaranteed to leave all three**: herd telemetry is
  fog-gated to hexes lit *right now*, a detached party is deliberately not a vision source
  (`visibility_systems.rs`, `Without<Expedition>`), and local extinction prunes the herd outright. So a
  party outlives every array that could name its target, and the Parties zone rendered
  `game_boar_88` — a row beside two healthy ones reading `Wild Boar`.
  The fix is on the wire, not a fourth client tier over the same dead arrays: the sim resolves the
  species at launch, where the herd is in the registry by construction, and carries it on the party as
  **`expeditionTargetSpecies`**. `_herd_label_for_id` consults the party's own declared name
  (`HudBandLaborState.expedition_target_label`) **last** — while the herd is visible the live telemetry
  is the better answer, since the party's copy is a launch-time snapshot. That accessor is a pure
  filter over `_player_expeditions`, **not a name cache**: the parties array is replaced wholesale each
  snapshot, so it can only answer for a herd some live party is hunting now.
  The target's live `(x, y)` still comes from the herd list and is still absent here — that is a
  separate statement, and the one the "target herd lost" delivery line already makes. What's left in a drawer is only what the row can't show — herd: Size / Herd (the stock pair, with
  its ecology phase riding it — see `herd-readouts.md`) / Husbandry / Corral; expedition: Mission / Target / Orders / Phase / Carried /
  Position. **A herd states NO `Position`**: `herd_summary_lines` renders in this drawer and
  nowhere else, and the card's own `TILE (x, y)` header sits two rows above it, so the row was the
  same coordinate pair twice on one card. Its `Next waypoint` is a different fact — where the herd
  is HEADING — and stays. The expedition's `Position` is NOT the same row: a party is somewhere
  other than the tile whose card you are reading. **The expedition's `Phase` keeps its WORDS** — the compact
  Active-expeditions row is where the glyph vocabulary belongs; the drawer IS the disclosure. **Its
  `Policy` row is gone**: a raid names a FLOOR, so the `Orders:` row renders `NN% left standing` off
  `expedition_floor` — beside the fill target, the two being one sentence
  (`DetailFormat.expedition_orders_line`). There is no policy word left to keep. In the drawer,
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


## The roster row's leading MARK is a node, and the patch path must SWAP it (issue #439)

A land / herd row used to fuse its glyph into the name (`_roster_name_label("%s %s" % [glyph, name])`).
It cannot, now that the mark is bundled art: a texture does not live inside a `Label.text`. The mark is
therefore its own child ahead of the name, built by **`HudWidgets.build_marker_icon`** (the one builder
every HUD text surface shares — see `hud-modules.md` for what it is and why it is a `TextureRect`), and
the name label carries the name ALONE so the meta beside it goes on absorbing the row's slack.

**THE TRAP IS THE IN-PLACE PATCH PATH.** These rows are patched rather than rebuilt (`_set_row_name` /
`_store_row_refs` / `button.get_meta`), and the mark is a **`TextureRect` when the subject has bundled
art and a glyph `Label` when it does not** — a distinction a row can cross between restates: a tile
gains or loses a food module, a herd's label resolves to a species the client has no art for. Writing
`.text` to a `TextureRect` is a **silent no-op**, so a patch that only wrote to the existing node would
leave a stale mark beside a freshly-patched name, which is the exact staleness the patch path exists to
avoid. `_set_row_icon` therefore patches the one property when the kind is unchanged and **swaps the
node at its own child index** when it flips, re-stashing it on the button — so the common case stays
rebuild-free and the flip is still correct.

**THE GLYPH FALLBACK'S INK IS APPLIED, NOT INHERITED, AND BOTH PATHS OWE IT.** Fusing the glyph into
the name label used to give it that label's `font_color` for free; as its own bare `Label` it inherits
nothing (this client applies no `Theme`), so a `◈` nobody colours renders at Godot's stock near-white —
brighter than the `INK_DIM` name beside it and no longer dimming or brightening with the row. The pair
is decided in ONE place, `_roster_row_ink(selected)`, which `_roster_name_label` / `_set_row_name` and
`_row_icon` / `_set_row_icon` all read, so the mark and the name cannot disagree about how lit the row
is. **`_set_row_icon` re-applies it on the patch path**, not only at build time: a row's lit state
changes without the row being rebuilt — that is what the patch path is FOR — so a mark coloured only at
birth keeps its original ink while the name moves. Art takes no colour: a marker sprite is drawn
untinted (`hud-modules.md` → `build_marker_icon`). `ui_preview`'s `tile_panel` chapter holds both
halves as claims about the colours the two labels actually RESOLVE (`get_theme_color`, which answers
the stock default when no override is set — an "an override is set" assertion would pass on the bug,
which IS a missing override): the LIT half on `tile_panel_no_forage`, the UNLIT half on
`tile_panel_land_glyph_unlit`, where lighting the band beside the land row dims it through the patch
path. Sabotage-verified, and they fail DISJOINTLY — dropping the build-time colour fails only the lit
one, dropping the patch-path re-apply only the unlit one.

`row_icon` is deliberately **not** the `glyph_label` meta slot: that is the band row's TRAILING activity
glyph, a different question in a different place, and folding them would make one meta key mean two
things.

**No frame can hold this claim** — both renderings are a perfectly ordinary row, and the stale one is
stale only against a tile that is not in the same picture. `ui_preview`'s `tile_panel` chapter asserts it
instead, on the same-tile restate block: one land row loses its `food_module` between two
`reapply_selection`s, with a precondition that the roster really PATCHED (identical child instance ids —
otherwise a rebuild would launder the bug) and the before/after classes read off `row_icon`.
Sabotage-verified: dropping the swap fails exactly that one assertion.


## ONE ROW PER LIVE METER, AND THE TURNS LEAD IT (issue #545)

`docs/plan_unit_costed_work.md` §11 put a rung's SIZE on the card — a rung declares a fixed size in
WORK UNITS, a crew produces work units per turn, and TURNS ARE THE OUTPUT — and it did it by adding
lines. A herd being tamed cost four:

```
Husbandry    Domesticating 0.3 / 100 work (0%)
             ≈308 turns at this crew
Keepers      1 — drawn from the band's Husbandry
Keeping      still being built — its own crew pays 0.7 work a turn, worth 1 builder
```

Reported from play: the last two could not be read. **Both existed to say *there is nothing to do
here*, and both said the same number twice** — one as a demand in hands, one as a rate in work — while
the reading a glance actually wants was buried on the second line. So the block is ONE row:

```
Husbandry   ≈308 turns (0%)      building — the turns LEAD, the meter is context
Husbandry   🐄 Domesticated 100% built
Husbandry   🐄 Domesticated 92% ⚠ built, and its keeping is short
```

**Both webs, identically, through ONE composer** — `DetailFormat.rung_row_value`, which the tile
card's `Cultivation` / `Field` rows and the herd drawer's `Husbandry` / `Corral` rows all render
through, so a rung's state cannot be worded one way on a patch and another on a herd. The plant side
reads `Cultivation ≈50 turns (0%)` / `🌾 Tended 100%` / `🌾 Tended 92% ⚠`.

- **THE WORK ABSOLUTES CAME OFF THE CARD.** `0.3 / 100 work` is what you read while COMPOSING a build,
  beside the stepper that moves it, so `DetailFormat.build_meter_value` stays and is now the compose
  sheet's alone. The card states the outcome; the sheet states the transaction.
- **THE PERCENTAGE STAYS ON A BUILT RUNG, and it is not decoration.** A completed meter sits exactly
  at its own cost, so `92%` is a rung that has ALREADY begun eroding — the one number on the card that
  shows it, and exactly what a glance should catch.
- **`built` IS THE ACHIEVEMENT FLAG, NEVER `progress >= 1`.** Fullness and achievement stay orthogonal
  (`SourceForecast.build_verb`'s own note): a patch at 92% is still tended AND is being repaired, so
  passing the meter as the fork would make a rung's LOSS and a rung's REPAIR one edge.
- **BOTH ROWS RENDER WHEN BOTH METERS ARE LIVE**, each labelled by its own rung and carrying its own
  state and its own hazard — `Cultivation 🌾 Tended 100%` over `Field ≈30 turns (12%)`, and
  `Husbandry 🐄 Domesticated 100%` over `Corral ≈6 turns (40%)`. A single merged row would silently
  drop either the rung you hold or the build in flight.
- **The rung row is gated on `built OR meter > 0 OR declared`**, which is what makes the declared-and-
  unmanned state renderable at a meter of ZERO (see the section below).
- **`your gear: −17 work off this job` SURVIVED as the one indented sub-row** (`build_gear_lines`,
  which is what `build_estimate_lines` became once the turn estimate moved into the row). It renders
  only above zero, is the only way a player can tell a tool is worth carrying to a garden and not to a
  farm, and was no part of the four-line block that made a rung unreadable. It is PER SOURCE, so it
  hangs off whichever meter a crew is actually filling.
- **THE `at this crew` TAIL WENT WITH THE SUB-ROW, and nothing is lost.** It existed because an
  indented estimate under a meter had to say whose answer it was; the row IS the crew's answer now, and
  the state and the count can no longer disagree because they are one string.

### THE ABSENCE OF A HAZARD IS THE ONLY SIGNAL THAT THINGS ARE FINE

That is what deleting the `Keeping:` row costs, and it is the rule everything above rests on: **a
failure state that renders bare reads as success.** It is the same trap as the unstaffed build one
section down, where a calm `0%` was taken for work in progress. So `rung_row_value` forks, and every
FAILURE state leads with `HudSelectionVocab.RUNG_HAZARD_GLYPH`:

| state | reads | why it is not the one above it |
|---|---|---|
| declared, nobody assigned, nothing banked | `⚠ Not started — no builders assigned` | there is no meter to state — `0 / 50 (0%)` is that zero written three ways |
| **work banked, nobody on it, keeping COVERS it** | **`Held at 42%` — no mark, neutral ink** | **it is a decision, not a failure** (see below) |
| banked work on a rung that is NOT the one in flight | `Held at 42%`, or `⚠ Reverting 42%` when its keeping is short | the source's ONE countdown is about the OTHER rung |
| a crew on it banking exactly the ROT | `⚠ ∞ turns (42%)` | somebody IS on it and their turn is being wasted; the remedy is MORE of them |
| under the rot, staffed or not | `⚠ ∞ turns, losing ground (42%)` | the work already bought is going BACK — so it is RED, not amber |
| built, and the keeping pool is short | `🌾 Tended 92% ⚠` | the rung is HELD and slipping, which no build crew fixes |
| **the band's builders are ON it and its own gate refuses** | **`⚠ Blocked 96% — your builders are held here`**, over an indented remedy | **the hands are staffed and STUCK** — see below |

> #### ⛔ THE BLOCKED ROW NAMES THE REMEDY, AND THE REMEDY IS THE KEEPING ROLE (§4.6b)
>
> `BUILD_QUEUE_BLOCKED` (`-4`) is the band's `builders` pool standing on the HEAD of its queue with the
> gate on that entry refusing — so the pool is spending its turns on an entry that cannot advance and
> **every entry behind it is waiting on the block**, which is the cost no other hazard on this table
> carries. It is not the `Stalled` row: that one is a crew of nobody meeting a `-1`, where the wasted
> resource is the rung, and this one wastes the WHOLE POOL.
>
> **The measured remedy is `assign_labor <f> <b> husbandry <n>` — the keeping role — ALONE.** The gate
> that produces this is the keeping shortfall's, so staffing the web's own keeping role clears it; the
> copy names that role and nothing else, and deliberately does not hedge with *"and stop hunting"* or
> any second lever the player would then have to rank. `DetailFormat.build_blocked_lines` renders the
> indented second line and is the ONE producer of it, on both webs — `HudWorkVocab.ROLE_NAME_HUSBANDRY`
> on a herd, `ROLE_NAME_AGRICULTURE` on a patch, so the sentence names the card the player will press.
>
> **It renders NOTHING when the keeping is paid**, which is the pairing that keeps the remedy honest: a
> `-4` whose shortfall has cleared is a block with some other cause, and pointing at a fully-staffed
> role would be the warning-outlives-its-mechanism failure this arc keeps producing. The row's own
> hazard mark still stands; only the remedy is withheld.

> #### ⛔ THE HELD ROW IS THE ONE STATE HERE THAT MUST **NOT** BE MARKED (§4.6a)
>
> **Parking a half-built improvement is a legitimate thing to do.** Take the builders off a Cultivate
> at 50%, leave the band's keeping staffed, and the meter holds there indefinitely — the keeping pool
> owes an at-risk meter at every fullness (`docs/plan_standing_upkeep.md` §2.4), so nothing is being
> lost. **Marking it is how a player is taught to ignore the mark**, which costs every other row in
> this table its meaning; that is this whole section's rule, pointed the other way.
>
> **It says `Held` in WORDS rather than `∞ turns`, and so does the COMPOSE FACE** (`build_turns_clause`
> takes the crew for it). The two `∞` rows are statements about a CREW and there is none here — and the
> glyph is the larder runway's, shared on the strength of a player learning a mark once and reading it
> everywhere, so spending it where nothing is wrong teaches that it sometimes means nothing is wrong.
> The two producers saying the SAME WORD about the same state is what makes the pair trustworthy.
>
> **It replaced `⚠ Reverting 42%`, which was this surface's OWN producer of the state.** That row
> fired on *work banked and nobody on it* — an inference that a parked meter must be bleeding, true
> only while an unbuilt rung was billed to its build crew. **The wire answers it now**: `-2` where the
> keeping covers it, `-3` where it does not. The crew is still read, and is not a second opinion — it
> is the one fact `BUILD_METER_HOLDS` cannot carry, choosing between two wordings of one sentinel.

**ONE MORE STATE EXISTS AND IT IS MARKED TOO** — `⚠ Stalled 42%`, the sim's `-1` on a source with
builders on it: a rung whose knowledge, site or species gate does not hold, or whose crew stands over
an empty escapement room. It gets its own word, and must not render as a bare percentage, which is the
silence this family exists to remove. **It reads alike to the HELD row in INK and could not be more
different in meaning** — *no answer* against *no problem* — which is exactly why the held row spends a
word on saying so.

**THE BUILT ROW'S MARK IS ROUTED TO THE AT-RISK RUNG** (`SourceForecast.rung_is_under_kept`, §4.6a).
`is_under_kept` answers for the SOURCE — one pool, one shortfall — and **only one meter on a source is
ever at risk**: the newest one carrying work, which is what the published shortfall is resolved
through (`at_risk_rung`, the client's copy of the sim's newest-first walk). A patch mid-Sow is billed
for the FIELD, so a `⚠` on the tended row beneath would point the player at ground that is fine — and
a false mark costs every true one its meaning exactly as a missing one does.

**THE ROUTING USED TO BE ACCIDENTAL.** The test carried a `build_is_in_flight` gate, there to keep the
mark off a source whose bill the BUILDERS owed; the pooled keeping deleted that gate's reason and
merged the test with `is_unbuilt_and_unpaid`, and with the gate went the routing it had been doing by
side effect. **It decides which ROW shows a number, never the number** — the same job `build_verb`
already does for the build verb, off the same table. **And it withholds a mark from a ROW, never the
fact**: the source-level `At risk:` row still states what the shortfall costs and how long is left.

> #### ⛔ THE COUNTDOWN IS PER SOURCE TOO, AND IT NEEDED THE SAME ROUTING
>
> **`buildTurnsRemaining` describes ONE rung — whichever `build_verb` names — and the card has two
> rows.** Found by review: a Cultivate abandoned at 60% (never completed, so `built` is false) with a
> `Sow` declared over it publishes the FIELD's countdown, and the Cultivation row printed
> `≈30 turns (60%)` for a meter nobody is touching.
>
> **THE TWO PER-SOURCE NUMBERS NEED NOT NAME THE SAME RUNG**, which is why each is routed on its own
> question: on that patch `at_risk_rung` answers CULTIVATE (the newest meter carrying work) and
> `build_verb` answers SOW. A row asking either question of the other gets a confident wrong answer.
>
> A row that is not the rung in flight therefore states **what it is** rather than a number that is not
> its own: `RUNG_HELD_FORMAT` where the keeping covers it, `RUNG_REVERTING_FORMAT` where it does not.
> **That format came back for exactly this.** Retiring it was right for the at-risk meter — the sim's
> `-2`/`-3` replaced it there — and wrong for the other row, which the sim's answer cannot reach.
>
> **`rung_row_value`'s last argument is a STRING (`declared_rung`), not the bool it replaced**, so both
> facts a row needs come from one place: *is this the rung the player declared* (the unstarted row) and
> *is this the rung in flight* (`build_verb`'s answer, which honours a declaration only at a zero
> meter). Two separately-passed bools could disagree; one string cannot. **`SubjectDrawerController`'s
> `building_rung` retired with it** — comparing against the raw declaration is what put the source's
> countdown on whichever row happened to name the declared verb, and the GEAR line hangs off
> `build_verb`'s answer now for the same reason.

**THE TINT IS DECIDED ONCE FOR FOUR ROWS, AND IT KEYS ON THE MARK.** `DetailFormat.rung_value_hex` is
what `cultivation_value_hex` / `field_value_hex` / `husbandry_value_hex` / `corral_value_hex` all
delegate to — **red on the ROTTING row's own phrase**, amber on the hazard glyph, signal green on the
rung's own BUILT badge, neutral ink otherwise. Each of those used to guess by substring (`no
builders`, then `Reverting`, then the badge word), so every new hazard state needed its own guess and
could ship without its colour. **The HELD row is in no branch at all** and takes the neutral fall-
through, which is the render: a state that is fine by having no entry cannot acquire a colour by
someone adding a row. The one case above the rule is the STARVING pen, which is DANGER red
because the herd is shrinking right now.

**THE ROTTING TEST RUNS FIRST, and that ordering is the whole of it.** That row wears BOTH needles —
it leads with the hazard mark like every other failure state, and must, or the mark stops meaning
*something is wrong here* — so an amber branch tested first swallows it and the schema's promised
red/yellow split exists on the wire and nowhere on screen. `HudSelectionVocab.RUNG_ROTTING_PHRASE` is
passed INTO `RUNG_ROTTING_FORMAT` rather than spelled inside it, so the phrase the row prints and the
phrase the tint tests are one string — the BUILT badges' own shape.

**AND A FULL-WIDTH SENTENCE THAT LEADS WITH THE MARK IS A WARNING**, which is now a rule in
`detail_bbcode` rather than a list of known sentences. It tested `line == OVERGRAZING_WARNING` by
equality, so every other hazard sentence rendered in the muted `INK_DIM` a descriptive line gets —
including `HERDERS_SHED_FORMAT`, the one line in the client that says animals are drifting off.

### RETIRED — `Keepers:` and `Keeping:`

`DetailFormat.upkeep_lines` is `at_risk_lines` now, and it emits the `At risk:` row alone:

- **`Keeping:` stated a standing bill on every source that owed anything.** A rung's keeping only
  becomes a decision when it is SHORT — which is exactly when `At risk:` renders — so the bill went and
  its failure stayed. `UPKEEP_ROW`, `UPKEEP_VALUE_FORMAT`, the keeper/builder noun pairs,
  `UPKEEP_MID_BUILD_FORMAT` and `UPKEEP_UNBUILT_VALUE` went with it.
- **The mid-build sentence's job is the rung row's `∞` now.** *"Its builders are not covering that —
  this rung is sliding back"* was the row saying a build's crew is under the rate, on a surface that is
  not the row the player would act on. `BUILD_TURNS_NEVER` says it where the meter is.
- **`Keepers:` stated a DEMAND in hands, every turn, on a herd where nothing was wrong.** What a head
  count is for is *am I short* — which `HERDERS_SHED_FORMAT` carries, with the count, and only when the
  pool has failed to cover this herd. `KEEPERS_ROW`, `HERDERS_STAFFED_FORMAT`, `HERDERS_UNDER_FORMAT`,
  `herders_label` and `herders_value_hex` are gone.
- **`At risk:` is WARN-inked now** (`_value_hex`'s own case). It fell through to neutral INK for as
  long as the calm `Keeping:` row above it carried the context; as the whole detail behind a marked
  rung, a shortfall stated in the same ink as a stock reading is the reassuring direction again.
- **It takes no `kind`.** The row above states WHICH rung is in trouble and the four-hazard fork
  decides the mark; this states what the trouble costs, off the published shortfall alone, so it covers
  both sides of the meter without re-deciding which side it is on.

**The four per-rung BUILD VERBS went too** — `Preparing` / `Sowing` / `Domesticating` / `Building`
each headlined a row that now leads with a number. The compose sheet keeps its own participles
(`HudComposeVocab.IMPROVEMENT_RUNNING_LABELS`), because a sheet is COMPOSING that verb rather than
reporting it. **`Reverting` went too, in §4.6a** — `HudSelectionVocab.RUNG_HELD_FORMAT` is what a
parked meter reads now, and the losing half is the rotting row's.

**The FIVE answers `buildTurnsRemaining` publishes all render** — a count is a finish date, `-2` is an
amber `∞` (`BUILD_METER_HOLDS`, the meter standing still), `-3` a red one saying *losing ground*
(`BUILD_METER_ROTS`, the meter going backwards), `-4` is `Blocked` (`BUILD_QUEUE_BLOCKED`, §4.6b — the
pool staffed and refused, above), `-1` is *no answer*. **`-3` was split out of `-2` and
this client did not follow for a release**, flattening it back to *no answer* and so rendering a build
actively bleeding banked work as the STALLED hazard — *a gate refuses this, no crew size fixes it* —
when the remedy is precisely more hands; the long form is in `labor-ui.md` → "THE SECOND `∞` IS RED".

**And `-1` is no longer allowed to render as nothing on a row a crew IS on**: it is the `Stalled`
hazard there, and it stays
silent only where the row itself does not render. **THE CLIENT DERIVES NONE OF IT**: an unstaffed
source and a refused gate both reach this reader as `-1`, and re-deriving either would call every idle
improvement on the map a never-finisher.

**Frames + assertions.** `tile_meter_building` / `tile_meter_held` / `tile_meter_never` /
**`tile_meter_rotting`** / **`tile_meter_blocked`** / `tile_build_unstaffed`
(`chapters/improvements.gd`) carry the plant hazards as WORD-AND-TINT markup — ONE patch at ONE meter
value, with only the band's assignment and the published sentinel moving. `tile_meter_held` and
`tile_meter_never` are judged as a PAIR, since they are one step apart and the whole claim is that
they read differently; `tile_meter_blocked` carries its own PNG-less negative — the same `-4` with the
keeping PAID, which must render the hazard row and no remedy line at all.
`tile_two_meters_live` is the both-rows frame, and its third claim is the SILENCE: a patch whose
keeping is paid must carry no mark on either row, or the mark means nothing on the states that do.
`improvement_unstarted_standing_price` is the compose-sheet pre-commit quote (see `labor-ui.md`). `herd_corral`
carries the animal both-rows case with the gear line under the building rung; `herd_under_herded`
carries hazard 4, the built row's `⚠` beside the shed sentence and the `At risk:` countdown.

**All five hazard states are ASKED OF THE PRODUCER as one conjunction** (`_hazard_states_all_marked`),
because two of them render in states no frame in the chapter stages and the claim is about the SET: any
state escaping the mark is the bug, and a per-state frame samples rather than closes it.

**The fixtures DERIVE `work_done` from the fraction they already state** (`BaseFx.price_plant_build` /
`HerdFx.price_animal_build`), so a fixture that re-dials a meter cannot end up with a percentage and
an absolute that disagree — which is the one thing this readout exists to make visible.

### A build DECLARED with nobody on it is a fourth state, and it said nothing at all

The `-1` rule above is right and it left a hole. A source **nobody has staffed** answers
`BUILD_TURNS_NO_ESTIMATE`, correctly — nobody has promised anything there — so every meter surface
rendered no line; and a rung row is gated on `progress > 0`, so at a meter of zero the tile card and
the herd drawer rendered **no row either**. Reported from play: commit a Cultivate with BUILDERS at
0 and everything reads as though work is under way — the compose sheet quoting
`Cultivating 0 / 50 work (0%)`, a `0%` rung plate on the map — with nothing anywhere saying nothing
is happening. **A declared-but-unstaffed build is an actionable standing fact, not an absence of
information**, exactly as the `∞` one state over is.

**`SourceForecast.unstaffed_build_state` is the ONE fork, and it keeps three states apart:**

| crew | meter | answer | what it means |
|---|---|---|---|
| 0 | 0 | `BUILD_UNSTAFFED_UNSTARTED` | **not started — nobody assigned** |
| 0 | >0 | `BUILD_STAFFED` — **the WIRE answers it**, `-2` held or `-3` losing | held on purpose, or losing ground |
| >0 | any | `BUILD_STAFFED`; the estimate speaks for the pace | somebody is on it |

**IT ANSWERS ONE STATE NOW** (§4.6a). `BUILD_UNSTAFFED_SLIDING` read *work banked + nobody on it ⇒
bleeding*, an INFERENCE that the pooled keeping made wrong half the time — a parked build whose keeping
is met simply holds. The sim publishes the real fork for zero builders and `build_pace` classifies it,
so there is one producer of that state and a client-side rot test would be a second opinion about a
number the sim owns. The one state left cannot collide with the `∞` face, so the fork is still one
function rather than four surfaces each deciding for themselves.
`unstaffed_build_of(progress, crew)` is the same fork asked of an ALREADY-RESOLVED rung, for the map
badge, which has just resolved both through `RungGates.rung_in_progress`.

**Nothing new is asked of the wire.** The declaration (`LaborAssignment.improvement`, which the sim
**derives** from the band's build queue), the hands (the band's `builders` row of
`laborAssignments`, since the per-source `improvementWorkers` retired with the crew it counted —
`docs/plan_standing_upkeep.md` §2.5) and the meter are all published; the client derives the state.

Four surfaces, and each says it in its own register:

- **The tile card** renders the rung row it used to suppress, valued
  `DetailFormat.BUILD_UNSTARTED_VALUE` (`⚠ Not started — no builders assigned`) in WARN. Above zero
  the row is the wire's own verdict, and the **real build crew** chooses only the WORDING of
  `BUILD_METER_HOLDS`: a crew treading water, or a build parked on purpose. **It was a bool meaning
  *this rung is the one in flight*, which is a different question** — found by review, and it parts
  from the crew on a state this arc built a whole readout for: a completed Cultivate that later erodes
  below its retention bar carries no declared improvement (completion clears it), so the bool could
  answer *staffed* on a meter nobody is touching and the row wore the amber `⚠ ∞ turns` where the
  deliberate-hold `Held at 92%` belongs. `HudBandLaborState.build_crew_forage` / `build_crew_hunt` are
  the folded counts both webs pass.
- **The herd drawer** takes the rung as a parameter — `herd_summary_lines`' trailing
  `unstaffed_build`, and since §4.6a the `build_crew` beside it — because a pure producer over one herd
  dict cannot see the player's labor row. **On this web `HOLDS` always means parked**: no animal rung
  declares a `meter_decay`, so an animal meter never goes backwards and no crew can be treading water
  on one.
- **The compose sheet** puts it in the improvement control's WARN note slot
  (`HudComposeVocab.BUILD_UNSTARTED_NOTE`) and inks the face through the same pace the `∞` uses.
  **`BUILD_SLIDING_NOTE` is retired**: it filled a silence — at zero builders the sheet's own estimate
  used to drop out — and `build_turns_at` answers at zero now, so the note would restate the line
  directly above it and would claim a loss on a meter the player parked.
- **The map badge** drops the percentage for `🌱⚠` in `HudStyle.WARN` — see `overlay-channels.md`.

**The tint is decided ONCE for four rows.** `DetailFormat.BUILD_UNSTAFFED_NEEDLE` is what
`cultivation_value_hex` / `field_value_hex` / `husbandry_value_hex` / `corral_value_hex` all match,
and `BUILD_UNSTARTED_VALUE` is BUILT from it, so the words and the test cannot drift.

**THE DECLARATION AND THE CREW ARE TWO DIFFERENT ROWS NOW, AND ONLY ONE OF THEM CAN BE READ
OPTIMISTICALLY** (§2.5). The declaration is the SOURCE's `LaborAssignment.improvement`; the hands are
the band's own `builders` role row, which names no source at all. So
`HudBandLaborState.unstaffed_build_forage` / `_hunt` walk `labor_assignments` for the CONFIRMED
declaration and read the pool through **`effective_role_workers(band, "builders")`**, which is
pending-aware.

**THE CREW HALF WAS CONFIRMED FOR A RELEASE, AND THE REASON WRITTEN HERE FOR IT WAS WRONG.** It said
the pending overlay "carries a declaration and not a role edit" — true of `effective_worker_map`, the
per-SOURCE map, and FALSE of `effective_role_workers`, the ROLE reader, which has always existed and
answers exactly this question off `pending_assigns_for`. What the confirmed read actually bought was
the defect: a player who had just staffed the role read a Builders card saying `2` beside a compose
sheet saying *"⚠ Not started — nobody is on this band's Builders role"* until the turn resolved — two
surfaces on one screen, and the stale one phrased as an accusation. **A pending role edit cannot be
refused** (`assign_labor` clamps the count rather than rejecting it), so the optimistic read can only
ever silence a warning that was about to stop being true.

**The DECLARATION stays confirmed**, and that is not the same compromise: there is no pending overlay
for it to read, and it composes safely with the pending-aware `building_rung` beside it on the tile
card — a fresh commit has no confirmed declaration yet, so this reader answers nothing.

**`build_crew_forage` / `build_crew_hunt` beside it stay CONFIRMED, deliberately.** They do not decide
whether to warn; they choose the WORDING of a verdict the wire already computed (`BUILD_METER_HOLDS`
is a crew treading water or a build parked on purpose), and that verdict was resolved against the
confirmed crew. Reading an optimistic crew there would classify last turn's number with next turn's
hands.

**THE POOL IS FOLDED OVER THE BANDS THAT WORK THE SOURCE, NEVER OVER EVERY BAND.**
`HudBandLaborState.build_crew_forage` / `build_crew_hunt` sum `workers_for_role(band, "builders")`
across the bands holding an assignment on THIS source, because the question these surfaces ask is *is
anybody building this* — a fact about the source. Folding the whole faction's builders would put a
crew on every source on the map the moment one band staffed the role. The COMPOSE SHEET asks a
different question (*what would MY band's pool make of it*) and answers it with one band's, which is
why the two readings differ and must (`labor-ui.md` → "The build's closed form").

**Declaring with no builders stays LEGAL, and withdrawing is `unqueue`.** Ticking the box appends a
queue entry, which costs nothing and loses nothing while it waits; unticking sends
`unqueue <faction> <source>` and drops it. The bug this state was written for was that a declaration
was invisible, not that it was possible, so nothing here blocks a commit.

**Frames + assertions.** `tile_build_unstaffed` (`chapters/improvements.gd`) carries the tile card's
row as word-AND-tint markup, the sheet's note, and the NEGATIVE that names the defect — the build's
own word must appear nowhere on the card, in any ink. The map's three answers and the herd drawer's
pair are DRIVEN beside it: a plate is drawn to a canvas and no assertion can read a glyph back off
one, and the herd's producer is pure. Each group is a pair or a triple, because "always warn" passes
any lone positive. Sabotage-verified on two DISJOINT mutations: making `unstaffed_build_of` always
answer `BUILD_STAFFED` fails exactly the sheet's note and the map's two unstaffed answers, while the
card claims stay green (a different seam); making `HudBandLaborState._unstaffed_build` always answer
`IMPROVEMENT_NONE` fails exactly the card's row.
