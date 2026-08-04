---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{BandCityPanel,BandFoodStatus,PenStatus}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandPanelController.gd"
  - "clients/godot_thin_client/tools/band_panel_preview.gd"
---

<!-- Extracted verbatim from lines 182-182;192-192;194-194;3475-3912 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The Band/City dockable panel

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/BandPanelController.gd` | `RefCounted` controller (HUD decomposition Phase 2d, `docs/plan_hud_decomposition.md`) owning the **BAND/CITY PANEL's whole render path** — the last big mass to leave `Hud.gd`. It holds the panel HANDLE (`_panel`), the three public **zone builders** `build_band_zone` / `build_work_zone` / `build_parties_zone` and everything under them (the band zone's vitals/PEOPLE/food-outlook/WORKFORCE + role cards; the work zone's paged board, filter chips, pager, inspector strip and source models; the parties zone's rows, inspector strip, footer and the mission compose sheet), the panel's **cycler + snapshot refresh** (`render_band` / `refresh_snapshot` / `rerender` / `cycle_band` / `focus_band` / `select_expedition` / `focus_labor_source` / `confirm_recall_expedition` / `_push_zone_badges`), and the **zone state that survives a snapshot** — `_work_filter` / `_work_sort` / `_work_page` / `_work_open_key` / `_work_policy_open` / `_work_zone_host` / `_work_zone_band` / `_band_zone_tier` / `_party_open_key` / `_party_compose_open` / `_party_compose_mission` / `_send_expedition_count` / `_send_hunt_policy` — ~1,580 lines, 72 moved functions. **`_band_zone_tier` is why the band and work halves are ONE controller**: it is a bare `int` written by `build_band_zone` and read by `_on_zones_resized`, so splitting them would have straddled it. Hud holds it as `_bandpanel`, constructed in `_ready` after `_disclosures` (the vitals row wires its carets through it). **THE PANEL HANDLE IS PRIVATE** — the two non-moving `HudLayer` readers (`_refresh_disclosure_hosts`, `_render_occupant_drawer`) only ever asked "is a panel injected?", so they ask **`has_panel()`** instead of holding the node. **The injection surface is TWO Callables** (it was nine, then six; the three detail-line ones went with `BandDetailLines`, and the four send-expedition/quarry targeting ones went with `TargetingController`), each retained on HudLayer by the "an injection you still have to hold is relocated, not eliminated" test: `_emit_assign_labor` (owns the `assign_labor_requested` emit + optimistic pending write, so `assign_labor` stays INDIRECT) · `_herd_label_for_id`. Each is reached through a **typed adapter**. The parties zone's send-expedition + quarry verbs (`begin_send_expedition` / `begin_pick_quarry` / `cancel_pick_quarry` / `is_expedition_quarry`) are a typed **`TargetingController`** collaborator now, not four Callables. `_is_player_unit` is a trivial private COPY (the `SelectionCardController` precedent). Collaborators: the SAME `_band_labor` / `_compose` model instances BY REFERENCE, `_selectioncard` (roster lookup + map pinning, for the cycler / labor-source / party jump routing, **plus `selected_terrain_label()`** — the one selection read the vitals rows need), `_disclosures` for `wire_label` ONLY, **`_banddetail` (a typed `BandDetailLines` ref — the vitals label and the parties inspector strip render through it; the three `*_fn` members `_unit_summary_lines_fn` / `_expedition_summary_lines_fn` / `_expedition_row_tooltip_fn` and their adapter wrappers are DELETED, the tooltip being a static `DetailFormat.expedition_row_tooltip` call now)**, and the HUD CanvasLayer as the **host** it `add_child`s its `ConfirmationDialog` into (a `RefCounted` cannot parent — the `TurnOrbController` pattern). **It emits FIVE signals, all RELAYED by HudLayer** (the controller never emits a HudLayer signal): `cancel_order_requested` · `send_hunt_expedition_requested` · `recall_expedition_requested` · `alert_focus_requested` · `roster_occupant_selected`. **`set_band_city_panel` / `cycle_panel_band` / `focus_panel_band` MUST stay callable on the HUD node** — `Main._wire_band_city_panel` probes all three with `has_method` and binds the latter two to `BandCityPanel`'s `cycle_requested` / `subject_activated`, and a failed probe fails SILENTLY — so HudLayer keeps them as thin delegators. **`_build_allocation_panel` does NOT live on this controller**: it writes the drawer's `%AllocationPanel` node, so it stays with the drawer render dispatch (it moved to `SubjectDrawerController` with that dispatch in Phase 2c-3, still a thin function stacking this controller's three public zone builders; its two siblings on that host, `_build_band_move_actions` / `_build_expedition_panel`, are branches of `_render_occupant_drawer` and travelled with it for the same reason). Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`SelectionCardController`/`DrawerComposeController` convention. Behaviour identical to the old inlined band-panel code |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail. Header (stage glyph/name/label + the band's hex coordinates + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse), body hosts **THREE NAMED ZONES AT A FIXED CROSS-AXIS SIZE** via **`set_zones(band, work, parties)`** (keys `&"band"`/`&"work"`/`&"parties"`; the panel OWNS and frees them). Two shells, chosen by the panel's own **WIDTH** (`WIDE_SHELL_MIN_WIDTH` — never a dock-edge test, so a resizable dock needs no special case). **That threshold is DERIVED, never hand-picked**: `ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + ZONE_WORK_MIN_WIDTH + WIDE_SEPARATOR_SPAN + PANEL_CHROME_H` = 380 + 354 + 380 + 50 + 26 = **1190**. `ZONE_WORK_MIN_WIDTH` (380) MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` — one readable board column — exactly as `ZONE_WORK_MAX_WIDTH` (1520) mirrors `WORK_COLUMN_MIN_WIDTH × WORK_MAX_COLUMNS`; the two are a PAIR with Hud's column consts and move with them. The chrome term is load-bearing because the threshold is tested against the panel's OUTER `_panel_extent().x` while the zones live in `_interior_size()`. It shipped hand-picked at **900**, which broke the whole 900–1055 band (the derived threshold was 1056 then, before the flanks widened): the work zone came out 224px, Hud clamped to one column, its labels clipped — and the NARROW shell would have given the board the full 874px, so flipping wide early made it ~4× narrower, degrading the thing the wide shell exists to improve. `WIDE_SEPARATOR_SPAN` / `PANEL_CHROME_H` are `const`s (a `const` cannot call `_wide_separator_span()`), shared by the threshold, `_wide_content_cap()`, `_wide_separator_span()` and `_interior_size()` so they cannot drift. **wide** (in practice T/B) = the three zones side by side, band/parties fixed `ZONE_BAND_WIDTH` (380) / `ZONE_PARTY_WIDTH` (`PANEL_WIDTH − PANEL_CHROME_H` = 354 — see "The wide shell's flanks are never narrower than the narrow shell's zone"), work EXPAND_FILL, `LINE_SOFT` hairlines between, no tab bar; **narrow** (in practice L/R) = a Band·Work·Parties tab bar under the header + exactly one zone beneath it (active tab = SIGNAL ink + a 2px SIGNAL underline, badges via `set_tab_badge(zone, text, hot)`, selection persisted as `CONFIG_KEY_TAB`). **The cross-axis size is FIXED** — `PANEL_WIDTH` 380 (L/R) / `PANEL_HEIGHT_WIDE` 360 clamped to `MAX_WIDE_HEIGHT_FRACTION` of the window (T/B) — so `current_reservation_size()` changes ONLY on dock/collapse/hide/viewport-resize and a content edit can no longer re-emit `reservation_changed` → `MapView.set_reserved_inset` → cache invalidation (the map flicker on every `+` press). **There is deliberately no `ScrollContainer` anywhere in the panel** (no-scroll by design; the work zone pages itself against **`work_zone_size()`**, the zone's interior after chrome — e.g. 354×1107 in a 380 L dock, 789×300 in a 1920 bottom dock with the chrome rail sharing that row — and re-pages on the **`zones_resized`** signal). **Zone hosts are plain `Control`s, not containers**, so an over-wide zone content cannot push the card past its fixed cross-axis size; `clip_contents` keeps overflow inside its own zone. Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`. On a HORIZONTAL dock the card's row also carries **a trailing CHROME RAIL** the HUD parks its stacked bottom-bar chrome into (`rail_slot_host` / `set_rail_width`, issue #324 — see "Band/City dockable panel"). See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/PenStatus.gd` | Single source of truth for **"is this pen's herd starving?"** — `FULLY_FED` / `FED_EPSILON` + `fed_fraction(herd)` / `is_starving(fed)`, reading `HerdTelemetryState.penFedFraction` (`< 1` ⇒ the keeper underpaid the pen's feed, so the herd is SHRINKING every turn). Plus `herd_is_starving(herd)` for a caller holding only the herd dict. The ONE test all three surfaces ask — the herd drawer (`DetailFormat.corral_label` + the Pen feed row), the map's distress badge (`MapView._draw_herd`) and the turn orb's `starving_pen` producer — so they can never disagree about which pen is dying |
## Band/City dockable panel

`ui/BandCityPanel.gd`/`.tscn` — a CanvasLayer that is the **persistent band/city
command center**: shown whenever ≥1 player band exists, always displaying a
"current band" (`_panel_band`). Design/roadmap: `docs/plan_band_city_dock.md`.

- **Dockable + persisted.** The user docks it to any of the 4 edges (default
  `SIDE_LEFT`) or collapses it to a thin rail; the choice (+ collapsed bool)
  persists to `user://band_city_dock.cfg` via `ConfigFile` (loaded in `_ready`,
  saved on change — the client's first user-pref file). It reserves its edge
  through the registry above: `reservation_changed(edge, size)` →
  `Main._apply_reservation(&"band_panel", edge, size)` (size = the cross-axis
  width/height, `COLLAPSED_SIZE` when railed, or 0 when hidden), so the map + HUD
  reflow off the reserved edge. All geometry/typography are named constants +
  `HudStyle`; the map-facing edge gets a `SIGNAL_DEEP` accent seam.
- **Header chrome.** Settlement **stage glyph + name + stage label + the band's hex coordinates**
  (`set_header` — glyph/label from the band marker's `settlement_stage_icon` /
  `settlement_stage_label`, neutral glyph fallback), a `◀ n/N ▶` **cycler**
  (`set_cycler`) over `_player_bands`, a 2×2 **dock chooser** (active edge
  highlighted), and a **collapse** toggle. `cycle_requested(delta)` → Main relays
  to `Hud.cycle_panel_band`.
  **The coordinates are IDENTITY, so they sit beside the stage word on the header's second line**
  (`Camp  (71, 18)`, same quiet ink and size, coordinates second — a band is "Camp" first and "at
  (71, 18)" second) rather than as a `Position:` row in the band zone's vitals, where they cost a
  height-capped column a row and answered a question the chrome above it was already answering.
  `set_header` takes them as a **preformatted `String`** (`HudFormat.BAND_HEADER_POSITION_FORMAT`,
  `""` ⇒ the label hides and costs no gap) — the panel never reads a band dict.
  **The resolver is `BandPanelController._panel_position_label`, and it exists because THE TWO PATHS
  INTO THIS PANEL SPELL POSITION DIFFERENTLY.** The per-snapshot refresh hands over the cohort dict
  the native decoder builds (`current_x` / `current_y`, **no `pos` key at all**); a click on the
  band's map marker hands over MapView's marker copy (a two-element `pos` array). While the
  coordinates were a vitals row keyed on `pos` alone, the row therefore **appeared when you clicked
  the band on the map and vanished on the next turn tick** — one surface rendering two different row
  sets depending on how you got there. The resolver prefers the cohort keys, falls back to `pos`, and
  answers `""` when neither resolves, so both paths produce the identical header. That split is
  exactly what the harness pair `band_panel_people` (snapshot path) / `band_panel_people_map_path`
  (map path) exists to keep separable — a fixture reached only one way cannot see it.
- **Header rows — no restated identity.** The panel's own chrome already states the band's **name +
  settlement stage**, so its summary grid does NOT repeat them: `_unit_summary_lines(unit, in_panel =
  true)` **drops the `Unit: <name>` row** (it was a third copy of the name) and **replaces `Size: <n>`**
  — population under another name — with a **`Population  29 · Workers 14 (Idle 12)`** row
  (`WORKERS_VALUE_FORMAT`, idle from the SAME `_effective_idle` the `+` steppers gate on). That labor
  line used to render as the allocation stack's first block, which meant it appeared wherever CURRENT
  ACTIONS did — **stranded between Active expeditions and Current actions**; the panel now passes
  `with_population_header = false` to `_build_allocation_sections`, so it exists once, in the identity
  grid. The header reads: name / stage / Population / Food / Morale / Position.
  `Unit` and `Size` are gone from **both** hosts — the Occupants drawer's roster row names the band
  and shows its size, so they restated it there too. `in_panel` survives as the gate on the
  **Population** row alone: the dock is the only host with a labor readout, and a foreign band has no
  `working_age`/`idle_workers`, so rendering it in the drawer would print a fabricated
  `Workers 0 (Idle 0)`. `_unit_summary_lines` is still shared with the Occupants-card drawer (foreign
  bands + the no-panel `ui_preview` fallback), and the legacy in-card allocation host keeps the
  population header block. (The grid it describes has since lost two more rows: the `Population …
  Workers …` line went to the two bars below it, and **Position went UP into the header chrome** —
  see "Header chrome" above. The drawer host still prints a Position row, which is why
  `unit_summary_lines` gates it on a `with_position` parameter rather than on `compact`.)
- **Content relocation (from the Occupants card).** The **player-band** branch of
  `Hud._render_occupant_drawer` now renders into the panel via `BandPanelController.render_band`,
  which assembles an ordered array of **section blocks** — a summary block
  (`_unit_summary_lines`), the Active-expeditions block, then the allocation sections
  (`_build_allocation_sections`) — and hands them to `BandCityPanel.set_band_sections`
  (see "Responsive body"). `_build_allocation_sections` returns the discrete Workers /
  Current actions / Band roles / Orders / Send-expedition VBoxes; the legacy
  `_build_allocation_panel(band, target)` wrapper still exists and fills the flat
  `%AllocationPanel` (the no-panel `ui_preview` fallback) by appending those same blocks.
  Herd/expedition detail stays in the Occupants card (`%OccupantDetail` / `%AllocationPanel`
  — still the expedition host **and** the no-panel fallback).
- **Arrival schedule — WHEN the steady food actually lands** (the discrete twin of the steady
  `realized_yield` headline). The sim streams `LaborAssignment.arrivalSchedule:[float]` (index i = food
  delivered i+1 turns from now, length = `arrivals_horizon_turns` = 20, `0.0` = nothing that turn, EMPTY =
  "not projected" — never a famine), decoded in `native/src/lib.rs` beside `realized_yield` as
  `arrival_schedule` (a `PackedFloat32Array`; `Hud._as_schedule` coerces a fixture Array/absent to
  packed). Two client surfaces, both presentation over sim numbers — no yield/ecology is re-derived:
  - **Per-source TICK STRIP** (`ui/hud/ArrivalStrip.gd`, a `_draw` Control): under each Current-actions
    Forage/Hunt row's secondary line (`HudWidgets.build_two_line_stepper` appends it as an indented line 3, inside
    the row container so the section-block/wide-tall packing is untouched), a compact 20-cell strip — one
    cell per upcoming turn, `HEALTHY` = a delivering turn, `LINE_SOFT` = an empty one, ~2px apart. **It
    renders ONLY when the schedule has a GAP** (`ArrivalStrip.has_gap` — at least one `0.0` in the
    horizon): a continuous forage source has no lumpiness to explain, so it gets no strip; the gap test
    is the whole rule (deliberately NOT a hunt/forage kind check). Per-cell tooltip `"Turn N — +X.XX
    food"` / `"Turn N — nothing lands"` (N = `_current_turn + i + 1`; relative "in N turns" before the
    first overlay).
  - **Merged FOOD OUTLOOK chart** (`ui/hud/FoodOutlookChart.gd`, a `_draw` Control) — its own **section
    block** (`_build_food_outlook_block`, appended right after the summary block, headed `FOOD OUTLOOK`;
    BBCode can't host a drawn chart, so it is NOT a summary line). Composed CLIENT-SIDE: start from the
    band's larder (`stores.provisions`), walk `food += Σ arrival_schedule[i] over the band's assignments
    − (food_consumption + pen_feed_upkeep)`, clamped at 0, over the 20-turn horizon (drain held flat).
    Draws a `SIGNAL` filled area + line, a `HEALTHY` dot on each haul turn, a faint `LINE_SOFT` baseline,
    and a dashed `DANGER` vertical labelled `empty ~turn N` where the walk first hits 0. Same player +
    real-food-flow gate as the Food breakdown, plus at least one non-empty schedule; a
    `custom_minimum_size` (≤ `SECTION_COLUMN_WIDTH`) lets the wide-column packer measure it.
- **Live + persistent.** `BandPanelController.refresh_snapshot()` (called each snapshot from
  `update_band_alerts`) hides the panel when there are zero player bands, else
  re-resolves `_panel_band` against the fresh snapshot (by entity, falling back to
  the first band) and re-renders so steppers/idle stay current. Selecting a
  herd/empty tile leaves `_panel_band` intact — the panel persists across selection
  changes. `cycle_panel_band(delta)` walks `_player_bands`, **recenters the map**
  on the band (`alert_focus_requested` → `MapView.focus_and_select_tile`), then
  pins the exact band so ring/Tile card/roster/panel all agree.
- **Bands vs expeditions.** `update_band_alerts` splits the player faction into
  `_player_bands` (resident bands — NOT `is_expedition`) and `_player_expeditions`
  (detached scout/hunt parties). The cycler + band-picker read `_player_bands`
  only, so a band + 2 expeditions reads **1/1**, not 1/3. Expeditions surface
  instead as an **Active expeditions** section on their home band (see below).
- **Active expeditions section.** `BandPanelController.render_band` → `_build_panel_expeditions_block`
  builds a self-contained expeditions **section block** (handed to the panel in the section
  array, so it's its own flow item / stack row) with one ghost-button
  row per `_player_expeditions` entry whose `home_band_entity == _panel_band.entity`
  (correct for N bands; omitted when none). Row summary — mission glyph + subject + the sim
  `ExpeditionPhase` as a **glyph** (`FoodIcons.for_status`), the phase WORD having moved into the row
  tooltip: hunt `🏹 <herd> · <Policy>  ●`, scout `⚑ → (x,y)  ➤`. The tooltip spells out the mission,
  the hunt policy's behaviour hint, the phase + what it means, and the click affordance.
  **`awaiting` is the one exception — it keeps its words, WARN-amber** (`▮▮ Awaiting orders`): it is
  not a status but a demand on the player (the party is parked at its objective burning provisions
  until you act), and a call to action must never require a hover to find. (A follow-up will make
  `awaiting` a turn-orb attention producer; the orb model already fits it.)
  A row click reuses the cycler's routing —
  `alert_focus_requested`→`focus_and_select_tile` + `roster_occupant_selected`→
  `MapView.select_occupant` — so the map ring moves to the expedition and the
  **Occupants card** (not the band panel) renders its `_build_expedition_panel`
  drawer; `_panel_band` stays put. `home_band_entity` is decoded in
  `native/src/lib.rs population_to_dict` from the snapshot's `homeBandEntity`,
  flowed onto the MapView unit marker, and covered by `marker_field_guard`.
- **Responsive body — THREE NAMED ZONES, two shells (`set_zones`).** The block-packing body
  (`set_band_sections` + `_pack_wide_columns`) is **gone**: column membership used to be a function
  of *measured block heights*, so a section hopped columns when the player pressed a `+`, and the
  panel fitted its cross-axis size to content, so every content change re-emitted
  `reservation_changed` and flickered the map. The body is now three named zones — `band` / `work` /
  `parties` — at a **fixed** cross-axis size, hosted by the wide (3 columns) or narrow (tabbed) shell
  per the panel's own WIDTH. Nothing is balanced, so nothing migrates; nothing is content-fitted, so
  the reservation is constant per dock edge. See the `ui/BandCityPanel.gd` roster row for the full
  contract (`work_zone_size()`, `zones_resized`, `set_tab_badge`, the no-ScrollContainer rule, and
  the plain-`Control` zone hosts).
- **A HORIZONTAL DOCK IS TWO FLOATING ISLANDS OVER LIVE MAP** (issue #377) — the card, sized to its
  content and centred, and the HUD's chrome cluster pinned to the strip's trailing edge. The reference
  is the TILE BAR at the top of the screen: a card as wide as what it has to say, over map, with the
  readouts as their own cluster beside it. `_position_card_and_rail` writes both rects; `_card_width()`
  is the card's own width and **`_interior_size()` reads the CARD, not the strip**, so every zone
  measurement follows with no edit of its own. A VERTICAL dock is untouched — the card still fills its
  380px strip edge to edge and there is no rail.
  - **This REVERSES the `PRESET_FULL_RECT` decision below, and the reversal is the point.** The card
    used to span the strip deliberately so a bottom dock read as ONE continuous bar. On a 3440-wide
    monitor that bar is two feet of dark chrome with an empty work zone stretched across the middle of
    it — the reported defect. The card no longer spans anything; only `_root` (the layout region) does.
  - **The MAP therefore stops insetting for a horizontal band dock.** `Main._reserver_overlays_map`
    zeroes the map's share of the reservation for `band_panel` on `SIDE_TOP`/`SIDE_BOTTOM`, so live map
    renders under and either side of the card. Without it the strip either side of the card is
    reserved-but-blank — *worse* than the bar, since it is dead space you can neither see nor click.
    **The HUD inset STAYS**, so the docks and the bottom bar keep clear of the strip and nothing can be
    drawn over; and the panel keeps its `_reservations` entry, which is what still displaces the event
    dock past it.
  - **The seam is a VERTICAL-dock thing now.** It accents the map-facing edge of the *strip*, which is
    right while the card fills that strip and wrong once it does not — on a horizontal dock it would
    rule a line across the whole monitor with a small card floating under part of it, re-drawing the
    very bar the islands replaced. `_position_seam` hides it; a floating card states its edge with its
    border.
- **THE TRAILING CHROME RAIL — the HUD's bottom-bar chrome SHARES a horizontal dock's row** (issue #324).
  `_build` wraps `_panel_column` in a `_card_row` HBox; **`_rail` is a SIBLING of the card under `_root`**,
  anchored to the trailing edge (it was the row's last cell until #377, i.e. chrome sitting ON the card,
  which is exactly what welded the two into one bar). `ChromeRailSeparator` went with that join — a
  hairline down the gap between two islands would re-assert it — so `RAIL_SEPARATOR_SPAN` is now a BARE
  gutter, the room the card must leave, with nothing drawn in it.
  - **ONE column, not a gutter at each end.** A leading + trailing pair was built first and rejected on
    sight with a real minimap in it: `NavBacking` is ~300px wide, so two opposite gutters cost ~562px of
    row, pushed the band zone inward AND stranded dead space around the orb. One column costs
    `max(nav, turn)` — **296–308 depending on map aspect** (296 Standard, 308 Large; the rail width
    tracks the minimap's `grid_width / grid_height`, and 140px of embedded height × the 0.087 aspect
    gap between them is the 12px), i.e. the NAV cluster, since the turn cluster shrank
    to 116 when its `Turn N` caption moved into the orb face — plus one `RAIL_SEPARATOR_SPAN` gutter, so
    **321 of row** on Standard. That hands the zones ~240px back, and the wide→narrow flip is
    `WIDE_SHELL_MIN_WIDTH` + the span: **~1511px** of window on a Standard minimap, ~1523 on Large
    (the widest rail in `MapSizes.OPTIONS`) — where two gutters put it at ~1618. At 1920×1080 in a
    bottom dock the work zone measures **789px** (`band_panel_dockrow_bottom` prints it), against the
    ~550 the two-gutter geometry would have left it. **The
    minimap tops out near 228px and the 520 `max_width` clamp is UNREACHABLE**, so there is no thin
    margin to worry about and no lever to touch: `MinimapPanel.get_aspect_ratio()` is
    `grid_width / grid_height`, `embedded_height` is 140, so the clamp needs an aspect ≥ 3.71 while the
    widest shipped map is Large at 104/64 = 1.625.
  - **The rail is separated from the content column like any other region** — `_card_row` carries
    `ZONE_SEPARATION` separation and a `_make_zone_separator()` hairline sits between the column and the
    rail, so the gutter is `ZONE_SEPARATION + ZONE_SEPARATOR_THICKNESS + ZONE_SEPARATION` =
    **`RAIL_SEPARATOR_SPAN`** (25) — *exactly* one inter-zone gutter, and `WIDE_SEPARATOR_SPAN` is now
    written as **two** of it so the two can never disagree. Without it the rail butted straight up
    against the parties content and read as part of it. The separator is **shown and hidden WITH the
    rail**: a `BoxContainer` skips separation around a hidden child, so retiring the rail retires its
    whole 25px AND its hairline — which is what keeps a vertical dock from growing a stray rule down the
    middle and losing 25px of zone. **`_rail_span()` (width + gutter), NOT `_rail_width()`, is what the
    width maths subtracts** — using the bare width would silently over-report the usable row by 25px.
  - **Two SLOTS, stacked top-to-bottom** (`RAIL_SLOT_TOP` = nav cluster, `RAIL_SLOT_BOTTOM` = turn
    cluster), minimap-on-top for BOTH `SIDE_TOP` and `SIDE_BOTTOM` so the stack reads the same either
    way. `RAIL_SLOT_SEPARATION` is its own const because `DockRowController._required_height` reads it
    as part of the stack's measured height.
  - **The panel owns the rail, its stack and the slot HOSTS — and NOTHING inside those hosts.**
    `rail_slot_host(slot)` (always non-null; the hosts exist from `_build`) hands the HUD a slot;
    `set_rail_width(width)` is how the HUD declares what the column takes. **This is the OPPOSITE of
    `set_zones`**, which takes ownership of and frees what it is handed — keep the contrast; do not
    "unify" them. The width is **DECLARED, never measured here**: the HUD owns the chrome, so the HUD
    measures it (the `max` over both clusters), which is what keeps `work_zone_size()`
    content-independent.
  - **THE THREE STRUCTURAL CHOICES INSIDE THE RAIL, each load-bearing** (see `_build_rail`): (1) `_rail`
    is a **plain `Control`, not a container** — the same reason `_make_zone_host` is one: a container
    reports its children's combined minimum, so the chrome could push the card past its FIXED cross-axis
    size. (2) **Because that wrapper blocks propagation, everything inside it can be an ordinary
    container** — which is why the slots are `MarginContainer`s rather than plain Controls: a slot must
    report the height of the cluster parked in it or the stack collapses to nothing, and a container
    reading its own child's minimum is exactly that mechanism, with nobody measuring the chrome twice.
    (3) The stack centres via **`BoxContainer.ALIGNMENT_CENTER`, never anchor arithmetic** — and that is
    a MEASURED conclusion, not a preference. `set_anchors_and_offsets_preset` derives its offsets from
    `get_minimum_size()`, the *virtual* one, which **ignores `custom_minimum_size`**: on the bottom dock
    `PRESET_HCENTER_WIDE` gave `NavBacking` (a `PanelContainer`, so it implements that virtual) offsets
    `[0, -76, 0, +76]` = ±152/2, correctly centred, and gave `TurnCluster` (a plain `Control` whose 128px
    height is *only* `custom_minimum_size`) offsets `[0, 0, 0, 0]` — its TOP edge pinned to the mid-line,
    then grown DOWNWARD by `_size_changed`'s minimum clamp, rendering **64px low** (rect y 900–1028 in a
    host spanning 730–1070). A container-driven stack has no such asymmetry.
  - **A rail exists only on a HORIZONTAL dock**, and `_rail_width` forces 0 by EDGE rather than trusting
    the declared value, so the panel is correct whatever order a dock change and the HUD's push arrive
    in. A vertical strip is `PANEL_WIDTH` (380) with no room beside its zones for a ~300px chrome column,
    so `SIDE_LEFT`/`SIDE_RIGHT` are **bit-identical to before**.
  - **`_rail_span()` is folded in at exactly TWO places** — `_shell_is_wide()` (the threshold is
    zones + separators + `PANEL_CHROME_H` tested against the OUTER width, and the rail spends that same
    outer width before the zones see any of it) and `_interior_size()`. **`work_zone_size()` and
    `_apply_wide_content_cap()` both read `_interior_size()`, so they follow with NO edit**, and the
    ultrawide `SHRINK_CENTER` path then centres the content column in the room the rail leaves.
  - **THE CARD'S WIDTH IS BUILT UP FROM A DECLARED COLUMN COUNT, NOT CLAMPED DOWN FROM A CAP** (issue
    #377). `_card_width()` (wide shell) = `ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + columns ×
    ZONE_WORK_MIN_WIDTH + WIDE_SEPARATOR_SPAN + PANEL_CHROME_H`, and the column count arrives through
    **`set_work_columns`** from `BandPanelController`, which is the only thing that knows how many
    sources there are. That is the `set_rail_width` contract again — DECLARED, never measured here.
    - **It is acyclic because the count comes from the zone's HEIGHT.** `_work_board_capacity` derives
      `rows` from the box's height (which a horizontal dock fixes), then `cols = ceil(count / rows)`.
      Width follows count; count never follows width. The SHELL is still chosen from the room the
      *strip* has (`_shell_is_wide`), never from the card, so nothing can feed back into the choice
      that produced it.
    - **`set_work_columns` RETURNS the count it granted, and the board must build to that.** A want is
      not a grant: `_affordable_work_columns()` caps it at what the strip can actually pay for, and a
      board built to the want overflows its CLIPPING zone host silently — measured at ~190px in a 380px
      side dock and ~725px at the wide shell's own minimum width, where a 34-source band asks for four
      columns and the strip affords one.
    - **It must NOT emit `zones_resized`.** The caller is the controller in the middle of building the
      board, and that signal is what makes it re-page — emitting would re-enter `_fill_work_zone` from
      inside itself. The cached size is refreshed silently instead, which also stops the next genuine
      resize firing a spurious re-page against a stale value.
    - **`set_rail_width` runs the FULL `_apply_dock_layout`**, not `_apply_rail` alone: the rail's rect
      is anchored by `_position_card_and_rail` rather than laid out by a container, and the card's width
      and centring are both computed against `_rail_span()`. Calling `_apply_rail` alone left the cluster
      at whatever width the dock was last applied at — measured as a 296px rail hanging 180px off the
      end of a 1920px strip.
    - **What this fixes, in the reporter's words: "the work area seems too wide."** It was. With the
      width read off the monitor, a band with NO worked sources still got four board columns and a
      2330px card. It now gets one column and a **1190px** card with 1929px of open map around it.
    - **The COLUMN's own `SHRINK_CENTER` cap is gone with it.** `_apply_wide_content_cap` capped the
      column inside a full-width card — the only thing a full-bleed bar leaves to narrow — and
      `SHRINK_CENTER` clears the expand flag, so with `_rail` on `SIZE_FILL` the row had no expanding
      child and `BoxContainer` packed both from the LEADING edge: column flush left, parked chrome at
      the **72% mark**, ~790px of dead card trailing it. The column simply fills its card now, and the
      centring happens one level up on the whole card.
    `_panel_extent()` is deliberately **untouched** — it is documented as the card's OUTER size and the
    card is still full-width — and `_position_seam()` needs no change either: the seam spans `_root` and
    correctly accents the whole reserved strip.
  - **`set_rail_width` can NEVER re-emit `reservation_changed`**, and that is what stops a feedback loop
    (HUD pushes a width → panel relayouts → no emit → no `Main` fan-out → no HUD reflow). The rail spends
    only the LONG axis; the reservation is the CROSS one (`_cross_axis_size`, which reads only the
    collapse flag, the dock edge and the viewport).
- **Zone `band` — vitals · PEOPLE · food outlook · WORKFORCE + role cards** (`BandPanelController.build_band_zone`).
  The Food/Trade/Morale/Growth rows are the disclosures — and their breakdowns open in a
  POPOVER, never inline (see Band food status: inline growth is what clipped this very zone).
  **There is no `Output:` row and no `Position:` row here.** Productivity reads on the WORK zone's
  head, where the rates it scales are (see Zone `work` below); the coordinates read in the panel
  HEADER, where the rest of the band's identity is (see "Header chrome"). Both left because this
  column is the one with no room, and both landed where their answer is already being used —
  `unit_summary_lines` gates the second on `with_position`, and the Occupants drawer, which has no
  header, keeps it.
  **The SHORT tier spends its two remaining optional rows differently: the Trade row is DROPPED, the
  Fodder row is MERGED** (`_build_vitals_label` passes `compact` to
  `BandDetailLines.unit_summary_lines`). Both are the row-level twin of this zone's
  food-outlook-chart gate below and taken for the same measured reason — the row measures 26px
  against a ~300px T/B zone that CLIPS rather than scrolls — but `compact` says **HEIGHT is scarce,
  not width**, which is the horizontal dock exactly, and that buys the second treatment: the hay
  stock rides the Food line as `· 128.4 hay` instead of vanishing, because a hay larder has no other
  surface to be legible on while Trade still reads on the WORK zone header's `⇄` total. See
  `band-readouts.md` for the clause and the width it was measured against. The Trade row carries the band's own stock
  AND its per-turn rate — both band-scoped, since the sim keeps trade goods in the cohort's `stores`;
  it is specified in `band-readouts.md`; the `Population … Workers … (Idle …)` LINE is
  **gone** — the two bars below state the same facts as charts, and a text restatement above them was
  the third telling of one fact. **PEOPLE** is the new one: a stacked children/working/elders bar
  (`age_children`/`age_working`/`age_elders`, falling back to `working_age` for the middle) plus its
  key and the **dependent count** — `14 dependents`, WARN-tinted once the ratio
  `(children+elders)/working × 100` passes `PEOPLE_DEPENDENCY_HEAVY`. **THE RATIO IS NOT SHOWN
  ANYWHERE** — it only decides that tint. `dep 88/100` read as a score out of 100, the game's own
  designer could not tell what it meant, and a tooltip quoting it did not make it any more useful; the
  bar beside it already shows the split, so the chip states the COUNT, which is the fact the player
  acts on. `HudFormat.dependency_tooltip` is deliberately SHORT: what a dependent is (children and elders, who
  eat but cannot be put to work), how many adults carry them, and — only when heavy — "More mouths
  than hands."
  **The top-bar strip no longer carries a dependency figure at all** (`Pop 30 👶9 🛠16 🧓5`): it is
  the FACTION total across every band, and dependents are fed per BAND — a band in trouble is in
  trouble whatever the faction average says, and a healthy average hides it. `_dependency_color` went
  with it.
  **`Label` DEFAULTS TO `MOUSE_FILTER_IGNORE`**, so `tooltip_text` on one is a SILENT no-op — six
  labels across this HUD (the dependency chip, the discoveries strip, both detail-row builders, the
  zone-head readout, the work total) shipped tooltips that had never once been seen. Every Label
  tooltip now goes through **`HudWidgets.set_label_tooltip`**, which sets the filter with the text; use it.
  **The brackets arrive FRACTIONAL** (`Scalar` — see the decoder note) and are apportioned to whole
  people by LARGEST REMAINDER (`HudFormat.apportion_people`), never rounded one at a time: 9.29 + 16.54 + 4.64
  rounds independently to 9 + 17 + 5 = **31** for a band of 30, and a panel that disagrees with the
  top bar about how many people are in the band reads as a bug in both.
  **Absent age data OMITS the whole block** — never a fabricated split.
  Its palette is deliberately MUTED (`VOICE_PIGMENT` / `INK_DIM` / `VOICE_INK`) against
  **WORKFORCE**'s saturated one (`HEALTHY` / `SIGNAL` / `VOICE_INK` / `WARN` / `INK_FAINT`): two bars,
  same shape, different question — *who they are* vs *what they do* — and they must not read as the
  same chart twice. Scout + Warrior are **CARDS** now (bordered, name · hint · the same `−/+` stepper
  and `assign_labor` emit), not rows in a list — the fix for a standing role being indistinguishable
  from a worked source. **The Warrior card carries a LIVE THREAT ALERT** (Predators Phase 3): when
  `_band_predator_threat_present(band)` is true its static hint is replaced by the crimson
  (`HudStyle.THREAT_ACCENT`) `HudWorkVocab.WARRIOR_THREAT_ALERT_FORMAT` — `⚠ Predator nearby — N on
  guard`, N being the on-guard warrior count — so the guarding role is legible exactly when the threat
  it answers is. **The danger is DERIVED CLIENT-SIDE, not a wire flag**: a herd counts as a menacing
  predator when it is VISIBLE (`_band_labor.world_herds()` is already fog-filtered), has
  `prey_sense_radius > 0`, has `attack × aggression > 0` (the same THREAT product the map overlay
  draws), and sits within `raid_radius` (the sim's echoed `predators.raid_radius`, per cohort) odd-r
  hex distance of the band tile — measured with the shared wrap-aware `SourceForecast.hex_distance_wrapped`,
  never a hand-rolled distance. **The zone yields by HEIGHT TIER** (`_band_zone_tier`, measured against the
  zone box — never the dock edge): full chart + hinted cards at/above `BAND_ZONE_TALL_MIN_HEIGHT`, a
  compact chart above `BAND_ZONE_CHART_MIN_HEIGHT`, and below it **no chart and hint-less cards** (a
  360px T/B dock). A tier change re-renders the zones; anything else just re-pages the board — that
  is what `_on_zones_resized` distinguishes, and skipping it lands a tall-shell band zone in a short
  box where its host silently clips it.
- **Zone `work` — THE PAGED BOARD** (`BandPanelController.build_work_zone` / `_fill_work_zone`). Header (`WORK` ·
  n sources · total /turn · the trade total when non-zero · **`Output 62%` when the band is below
  full productivity** · a `⋯` `MenuButton`) · filter CHIPS · the board · pager · inspector strip.
  **The Output item QUALIFIES the two totals beside it rather than adding to them**, which is why it
  trails them and why it lives here at all: `output_multiplier` is the discontent modifier every rate
  on this board is already scaled by, so the head is where its consequence is visible — a vitals row
  in the height-capped band zone stated it away from everything it acts on. Same gate that row
  carried (**only below `SourceForecast.OUTPUT_FULL`** — a permanent `Output 100%` is noise on a row
  that is otherwise live summary, the rule the trade total already follows) and the same buckets,
  through `BandFoodStatus.color_for_output`, a `Color` accessor rather than a hex one because this
  head is built out of `Label`s rather than BBCode — and, since the head is now the multiplier's ONE
  surface, the only accessor there is (`hex_for_output` went with the vitals row that called it). Vocabulary (`WORK_OUTPUT_FORMAT` /
  `WORK_OUTPUT_TOOLTIP`) is `HudWorkVocab`'s, like every other head item's.
  **The chips ARE the summary and the filter** (All / 🌿 Foraging n · rate / 🦌 Hunting n · rate / ⚠ k,
  the last hidden at k = 0), replacing collapsible group headers. Both the header total and the chip
  rates state BOTH products, each only when non-zero — see "Work rows and the two hunt products". Rows are ONE line at a fixed
  `WORK_ROW_HEIGHT`: severity stripe (WARN overdrawing/overstaffed, SIGNAL pending) · glyph · clipped
  label · rate · the SOURCE-RUNG mark · policy/⚠ marks · the existing `−/+`. The rung mark and the
  policy marks are TWO AXES — what the source IS against what is being done to it — and the row keeps
  both; the rung slot is reserved on every row, so the label's share of a
  `WORK_COLUMN_MIN_WIDTH` column is ~20px narrower than the marks column alone would suggest (spec in
  `labor-ui.md` → "The work row carries TWO axes"). **Capacity is derived ENTIRELY from
  `work_zone_size()`** (`_work_board_capacity`): `cols = clamp(w / WORK_COLUMN_MIN_WIDTH, 1,
  WORK_MAX_COLUMNS)`, `rows = (h − head − chips − inspector − pager) / WORK_ROW_HEIGHT`, filled
  **column-major** with a hairline between columns; the pager is resolved in **two passes** because it
  only exists when one page cannot hold everything yet costs a row. **EVERY reserved height must be
  what the element actually draws at** — the default `HudStyle` button chrome pads 9px top and bottom,
  which alone makes a stepper ~40px and pushes the page off the bottom of the zone, so the board's
  buttons take `HudWidgets.compact`'s squeeze. Clicking a row opens the **inspector strip**: the row's
  old second/third lines in one place (yield/policy/status in words, warning lines, the `ArrivalStrip`)
  plus three inline links — `Jump to source` · `Change policy` (an inline picker, the four EXTRACTIVE
  rungs only — the investment rungs are ladder commitments made at the source's own compose control,
  where their gates and payoff forecasts live) · **`Unassign`**. That is the per-source removal: a
  hover `✕` beside the `−` stepper would be a mis-click hazard, this is the labelled version. One row
  open at a time, and it COSTS board rows, which is why the capacity maths subtracts it.
  **THAT WHOLE SPECIAL CASE IS GONE** (issue #442). `model["policy"]` could legitimately be
  `corral`/`cultivate`/`tame`/`sow`, none of which the picker offered — so the radio highlighted
  nothing and a press silently discarded a ~25-turn ladder build. The fix then was a WARN standing
  line plus a `_confirm_destructive` on the pick; the fix now is that the state cannot arise. A
  `policy` is always one of the four stances, and what a row is BUILDING rides its own
  `improvement` field, which `assign_labor` does not touch — so a stance re-pick lights a rung like
  any other row, emits immediately, and leaves the build alone. `WORK_INSPECT_STANDING_INVESTMENT_FORMAT`,
  the two confirm strings and `WORK_INSPECTOR_STANDING_LINE_HEIGHT` (the taller reserved height that
  line needed) are all deleted; `_work_inspector_height` has one open height again. The strip still
  does NOT show a build's progress — the work model carries no meter, and the source's own compose
  control has it.
- **Zone `parties`** (`BandPanelController.build_parties_zone`): head + a `⋯` menu (`Recall all parties (n)`,
  behind the same confirm), one row per party (mission glyph · subject · phase · a **DANGER-red**
  recall `✕` — steady, full-opacity, reading as a destructive control like the Work inspector's
  Unassign), an **inspector strip** the row body opens, and the footer.
  **The row `✕` CONFIRMS before recalling** — `BandPanelController.confirm_recall_expedition(exp)` names the party
  (`_herd_label_for_id` for a hunt, "scouting" for a scout) through the shared `_confirm_destructive`,
  and every SINGLE-recall entry point (the row `✕`, the strip's Recall link, the Occupants drawer's
  Recall button) routes through it; `_on_recall_expedition_pressed` stays the RAW emit, so "Recall all"
  loops it under its OWN one confirm and never pops N prompts.
  **The row BODY opens an inspector strip** (`_toggle_parties_inspector(str(entity))` → `_party_open_key`
  → `BandPanelController.rerender`, the exact `_work_open_key`/`_build_work_inspector` pattern): a bottom
  `PanelContainer` (reusing `HudStyle.work_inspector_stylebox`) with a titled header + close `✕`, the full
  `_expedition_summary_lines` detail as dim status parts (Mission / Target / Policy / Phase / Carried /
  **Next delivery** / Position — so the strip IS the detail panel), and `Jump to party` (INK) / `Recall`
  (DANGER) inline links. The **"Next delivery" line** (`_expedition_next_delivery_line`, shared by the
  strip, the Occupants drawer, and the row tooltip) is ALWAYS shown for a hunt party once the field is on
  the wire (`has("expedition_projected_delivery")`): `Next delivery: ~N food in M turns` when projecting
  (`↻` appended for a recurring/Deplete party), `~N food (raid underway)` when the ETA is unknown, and —
  when the projection is `0` — a line that **disambiguates on the party's own TARGET, not the tile's
  herd**. A hunt party is bound to ONE specific herd (`expedition_target_herd`) chosen at launch, and a
  projected `0` over a **healthy** herd is structurally impossible (the sim proves it — the in-flight
  forecast byte-equals the pre-launch estimate), so a `0` means the target is *elsewhere*: `none — its
  target herd has no surplus to raid` when `_band_labor.find_world_herd(expedition_target_herd)` **is** in telemetry
  (a different, at-floor herd — NOT the boar the player is inspecting), or `target herd lost — the party
  is returning home` when it is **absent** (lost/replaced). This is the fix for the live "reads no-surplus
  next to a thriving boar" report — the target was a different herd. To make that visible, the drawer's
  **`Target:` row appends the target herd's live `(x, y)`** (read from `_world_herds`, keyed `x`/`y` — a
  migrating target is usually NOT the herd on the current tile). Never a silently blank line.
  `BandPanelController.build_parties_zone` orders
  `head → rows → inspector(if open) → EXPAND_FILL spacer → footer`, so the Scout/Hunt footer stays
  bottom-pinned with the strip under the clicked row; the strip's detail-line separation is tightened to
  `PARTIES_INSPECTOR_LINE_SEPARATION` to keep row + strip + pinned footer inside the height-capped T/B
  zone. The footer offers the two missions **DIRECTLY** — `⚑ Scout` and `🏹 Hunt`, side by side —
  and **both stay VISIBLE and DISABLED with their reason when idle == 0** (the section vanishing is
  what made expeditions look removed from the game). Pressing one swaps in the **compose sheet already
  on that mission**, titled `Setup a scouting/hunting party…`, with the `✕` as the only way back. The
  mission is therefore still chosen FIRST and the policy picker is still unreachable except under Hunt
  (it used to sit above the scouting button and read as if it modified it) — what is gone is the
  intermediate `Send a party…` page that only existed to ask which mission.
  **The HUNT form asks QUARRY → POLICY → PARTY**, in the order the decision is actually made: the herd
  sets the per-policy take, the useful party size and the trip length, so every field under it is
  unanswerable without it. The `Quarry` row mirrors the `Party` row's shape with a button instead of a
  stepper (`Choose…` primary when empty, `🐗 Wild Boar` ghost once picked, either way opening the map
  quarry picker); with no quarry the sheet renders the hint plus a **visible, disabled** Send and nothing
  else. **A quarry must lie strictly BEYOND the band's `hunt_reach`** — a hunting party exists for game
  the band cannot work from home, so a nearer herd is a local hunt. `_is_expedition_quarry` is the ONE
  definition (`SourceForecast.band_tile` + `_hex_distance_wrapped`, the herd drawer's own split) and all three sites
  route through it: MapView's glow rings only eligible herds (via `min_distance` — see Command
  Targeting), `_try_pick_quarry` REFUSES an in-reach herd and stays in targeting with a
  `QUARRY_WITHIN_REACH_FORMAT` nudge naming the herd, the distance, the reach and the local alternative
  (the split is invisible on the map, so the refusal is where it gets taught), and the sheet
  re-validates every render, so a herd that MIGRATES into reach falls back to `Choose…` rather than
  forecasting a raid the player should not make. With one, the policy rungs finally carry their ascending metric, the party stepper caps at the
  raid's max-useful plateau (a policy click auto-fills to it via the sheet's own `_send_party_autofill` —
  **not** the herd drawer's `_hunt_assign_autofill`), the trip forecast renders, and the Send button takes
  its viable/slow/denial/no-surplus treatment and emits `send_hunt_expedition_requested` directly.
  `_send_party_quarry_id` is re-resolved through `_band_labor.find_world_herd` every render (a vanished herd clears
  it rather than forecasting a stale id) and cleared on open, cancel, send, and a panel-band change.
  **SCOUT is unchanged** — its only input is party size and nothing about it depends on the destination,
  so it has no ordering problem to fix and still picks its target tile on the map after the send.
- **Destructive bulk actions ASK, and name what is SPARED** (`_confirm_destructive`, a
  `ConfirmationDialog` — a Window, like the `⋯` `MenuButton`'s popup, so opening either cannot move a
  zone's height). `Unassign all work` sends **`cancel_order <faction> <band> work`** — the signal
  `cancel_order_requested(band, scope)` gained the scope this pass; `work` clears Forage + Hunt only
  and leaves standing roles, parties and an in-progress move alone. `Recall all` is one
  `recall_expedition` per party (no bulk verb, and parties are few).
- **Move and Clear all are GONE from the panel.** Move belongs to the Tile panel in a later change;
  `_on_move_band_pressed` / `_pending_move_band` / the whole targeting machinery are intact and still
  reachable (the expedition drawer's Move), just not surfaced here.
- **A zone must FIT its zone.** The hosts clip, so overflow is invisible in a frame — and a zone
  content whose *minimum* size exceeds the zone (four policy rungs abreast in a 380px dock) does worse:
  it drags the whole zone column out past its host, taking the section menu beside it off the edge.
  Hence `ZONE_POLICY_PICKER_COLUMNS` and `band_panel_preview`'s **recursive zone-bounds assertion**,
  which is the only thing that catches either.
  **`ZONE_POLICY_PICKER_COLUMNS` (2) is deliberately BELOW the shared `POLICY_PICKER_COLUMNS` (3), so the
  Band panel's launch picker reads 2 + 2 where every free-floating picker reads 3 + 1.** That
  inconsistency is bought, not overlooked: at 3 the four two-line rungs need ~444px against the ~354px
  the L/R dock's zone gives, and the measured frame comes back with `⇊ Deplete` cut in half, the Quarry
  button clipped and the hint text sliced — plus two extra `_assert_zone_content_fits` failures. Closing
  the gap needs a WIDER parties zone (or a narrower metric line), never a bigger number here.
  **CONTAINMENT IS NOT COMPLETENESS, and that distinction is a second assertion.** Content the box
  cannot hold gets CLIPPED, and clipped content still reports a rect *inside* its host — so the
  bounds assertion passes on a frame that is visibly sliced (the Food/Morale inline breakdown cut the
  WORKFORCE row mid-glyph and erased both role cards, with every assertion green). `band_panel_preview`
  therefore also runs **`_assert_zone_content_fits`**: for every visible descendant of a zone host,
  `top + get_combined_minimum_size().y` must fit the zone box. It recurses past the zero-minimum
  plain-`Control` wrappers `HudWidgets.wrap_zone` produces (they report no minimum, so measuring them alone
  proves nothing) and stops at the first control that DOES report one — its minimum already accounts
  for its children. Run it beside the other two at every state.
  **AN ASSERTION IS ONLY AS GOOD AS THE FIXTURE UNDER IT, and that is what `band_panel_vitals_worst_case`
  fixes.** Every optional vitals row had its own frame, and every one of those fixtures was otherwise
  ordinary — so the band zone was never once asked to hold the whole set at the same time, and a band
  carrying all of them overflowed a `clip_contents` box with every assertion green (issue #374). That
  state stages ONE band with a hay larder *and* a pen feed bill, productivity below full, a fertility
  reading, a trade stock *and* rate, and the per-source `arrival_schedule`s the FOOD OUTLOOK chart
  needs — every gate in `build_band_zone` / `unit_summary_lines` live at once — in the height-capped
  TOP dock, and runs the bounds assertion, `_assert_zone_content_fits` and
  `_assert_merged_food_row_fits` over it. It reads **299px of a 300px box**. The margin is the point,
  so the state also PRINTS its per-zone extent (`_report_zone_content_extent`): a near-miss and a
  comfortable fit are the same green line otherwise. Sabotage-verified — putting the `Output:` row
  back takes the run from 0 errors to 25, `short by 25`.
- **The no-dock fallback renders the SAME three builders**, stacked into `%AllocationPanel`
  (`_build_allocation_panel`) — there is no second layout to maintain. It passes `with_vitals = false`,
  since the Occupants card's own drawer already prints those rows above it.
- Verify chrome + reflow via `tools/band_panel_preview.gd`
  (`godot --path . res://tools/band_panel_preview.tscn` → `ui_preview_out/
  band_panel_{left,right,top,bottom,collapsed}.png`). **The ZONE states are the Part-2 frames:**
  `band_panel_people` (both bars, the dependency ratio, the two role cards) ·
  **`band_panel_people_map_path`** (the SAME block reached the OTHER way — by clicking the band ON THE
  MAP, through the real `MapView._rebuild_unit_markers` → `refresh_selection_payload` →
  `show_unit_selection` → `BandPanelController.render_band`. `band_panel_people` drives the SNAPSHOT path,
  which re-resolves the brackets from the raw `populations` floats and therefore SELF-HEALS a
  truncating marker copy — so it structurally could not catch the `int()`-narrowed age brackets. This
  state ASSERTS the three PEOPLE brackets sum to the band's own `size`, and was verified to FAIL —
  `sum to 29 but the band holds 30 (raw [9.0, 16.0, 4.0])` — with the narrowing put back) ·
  `band_panel_work_page` (34 sources, narrow shell) · `band_panel_work_wide` (the same 34 in the
  bottom dock — 4 columns, column-major, `Page 1 / 2`, `1–28 of 34`) · `band_panel_inspector` (a row
  open, the board shrunk to 31 rows and a pager appearing to pay for it) · `band_panel_compose_hunt`
  (quarry → policy → party → forecast, with the real per-policy metrics and max-useful cap) ·
  **`band_panel_compose_hunt_eradicate`** (the ONE surface that renders `SEND_HUNT_POLICY_HINTS`
  verbatim, so it is the frame the EXPEDITION Eradicate hint is judged on: the rung's face reads the
  ladder's top `💀 +6.50 ⇄ +0.81`, the hint describes the one-trip haul, the currency the SPECIES pays
  (meat, ⇄ trade goods, or both — the raid banks the trade half too since #337) + the permanent end state, and
  the raid line below it delivers `~52 food · ⇄ ~7 trade goods` under an ordinary primary Send — no
  denial anywhere, #337) ·
  `band_panel_compose_hunt_no_quarry` (the empty state: `Choose…`, the hint, a disabled Send, nothing
  below) · `band_panel_compose_scout` (the same sheet under Scout — no quarry row, no policy picker). A
  BEHAVIOURAL assertion rides beside them: `_assert_quarry_eligibility` drives the real
  `_try_pick_quarry` with a herd INSIDE the fixture band's `hunt_reach` (must leave
  `_send_party_quarry_id` empty and stay armed) and one beyond it (must set it) — verified to FAIL
  with the `_is_expedition_quarry` test removed. The GLOW is MapView's, so its frame is
  `map_preview`'s `map_quarry_targeting` (two huntable herds straddling the reach; only the far one
  may wear the ring) · `band_panel_no_idle` (both mission buttons disabled and their shared reason) ·
  `band_panel_clear_confirm` · the **work-inspector policy-picker** PAIR, which is the only coverage
  that control has ever had (`_work_policy_open` was never set true in either harness):
  **`band_panel_work_policy_investment`** (a Hunt row that is BUILDING a pen —
  `improvement: "corral"` beside a `sustain` stance) and **`band_panel_work_policy_extractive`** (the
  same picker on the row beside it, building nothing). Since issue #442 the two frames' assertions are
  IDENTICAL, and that is now the claim: the picker cannot tell a building row from a non-building one,
  because a stance pick no longer touches the build. Both rows light exactly one rung and both emit
  immediately with no dialog. (The pair was written for the opposite claim — the investment frame
  asserted a rendered WARN line and a `ConfirmationDialog` instead of an emit — and the rows are
  opened BY HERD now rather than by rung, a rung no longer being an identity.) A pick emits
  immediately with no dialog, and **exactly one** rung
  wears the `primary` variant, read back off the button's `normal` stylebox against
  `HudStyle.BUTTON_PRIMARY_BG` (there is no other marker of "this rung is lit", which is why that
  colour is now a named const rather than a literal inside `apply_button`). The harness also ASSERTS, per state, that **no
  `ScrollContainer` exists anywhere in the panel** and that **nothing a zone renders falls outside its
  zone rect** — checked RECURSIVELY, since the top-level content is anchored full-rect and so always
  "fits" while the thing that actually overflows is a board row off the bottom of a column. Both
  assertions have already caught real regressions (a stepper's default button chrome busting
  `WORK_ROW_HEIGHT`; the band zone standing 5px past a 360px T/B dock); **keep them green.** A THIRD
  per-state assertion guards the shell threshold: **whenever the wide shell is active,
  `work_zone_size().x` must be at least `ZONE_WORK_MIN_WIDTH`** — the invariant the old hand-picked
  `WIDE_SHELL_MIN_WIDTH` violated, and one the zone-bounds assertion structurally cannot catch (a
  CLIPPED label still sits inside its rect, so "everything is within bounds" is true and useless).
  States `band_panel_shell_below_threshold` / `band_panel_shell_at_threshold` bracket the flip — one
  pixel below (must be the NARROW tabbed shell) and exactly at it (the narrowest legitimate wide
  shell, work zone exactly 380px, rows still legible) — each additionally asserting WHICH shell it
  got. They pin the **canvas** via `_pin_canvas`, not just the window: `project.godot` stretches
  `canvas_items` with an `expand` aspect, so the canvas never goes below the 1920 base width and a
  plain `_pin_window` renders a 1920-wide panel that proves nothing about a sub-1920 threshold. State `band_panel_status_glyphs` is the
  **row-vocabulary** frame: a confirmed working forage row (`●` + `♻` + the overstaffing note) and a
  working hunt row (`●` + `⚠`) beside a pending row (`○`, amber), plus one Active-expeditions row per
  phase (`➤` outbound / `●` hunting / `◄` delivering / `◄` returning / `▮▮ Awaiting orders` in amber)
  — read it at true size whenever a glyph changes. States `band_panel_arrivals_left` /
  `band_panel_arrivals_top` / **`band_panel_arrivals_bottom`** are the **arrival-schedule** frame (a
  lumpy hunt row with a gappy tick strip beside a continuous forage row that draws NONE, + the rising
  `FOOD OUTLOOK` sawtooth chart, tall and wide), and `band_panel_arrivals_empty` is the emptying-larder
  case (the descending chart's dashed `empty ~turn N` marker). **The T/B (`_top`/`_bottom`) frames are
  the band-zone HEIGHT guard** — they render the chart-bearing `_arrivals_band_fixture` (NOT the
  chartless `_band_fixture`) through `_assert_zone_content_fits`, so the SHORT-tier chart drop
  (`BandPanelController.build_band_zone` gates `_build_food_outlook_block` behind `_band_zone_tier !=
  BAND_ZONE_TIER_SHORT`) is actually asserted: forcing the chart ungated needs 415px in the ~300px T/B
  box (115px over), and the tier gate is what makes it fit at 0 overflow while the tall L shell keeps the
  full chart. Without a chart-bearing fixture in a T/B dock, `content-fits` was vacuously green (the
  chartless fixtures never overflow). **`band_panel_vitals_worst_case` is that same lesson one rung
  further** — a fixture carrying every optional vitals row at once, since a chart-bearing band is
  still not the tallest band; see "A zone must FIT its zone".
  **The five `band_panel_dockrow_*` states are the DOCK-ROW REFLOW's frames** (issue #324), rendered at
  a `_pin_canvas`ed **1920×1080** — 1080p with a bottom dock is the case the issue is about — and driven
  through the REAL `reservation_changed → Hud.reflow_dock_row` path, which the harness wires exactly as
  `Main._connect_band_city_panel` does (a second listener + a one-shot seed) rather than poking the
  controller. **They SEED A REAL EMBEDDED MINIMAP first** (`_seed_embedded_minimap`, driven exactly as
  `MinimapController._setup` drives it: `setup_embedded` into `Hud.get_minimap_container()` then
  `set_grid_size`, with the grid from `MapSizes.option_for(MapSizes.DEFAULT_KEY)` and the raster a
  documented flat 1px-per-hex stand-in for `_rebuild_image`'s per-hex paint — **never a literal width or
  aspect**). That is not optional polish: against an empty `MinimapContainer` the chrome column collapses
  to the zoom rail's ~80px, so the rail width, the frames and the containment assertion are all honest
  about nothing — which is precisely how the two-gutter geometry reached a live playtest instead of being
  caught here. `_bottom` / `_top`: the chrome shares the row as ONE trailing stacked column (minimap above
  the orb), **nothing in the row's leading gutter and the band zone flush to its left edge**, `BottomBar`
  gone, the wide shell held, work zone ≥ `ZONE_WORK_MIN_WIDTH`. `_left`: **the control** — chrome home in
  `BottomBar`, `_rail_width()` zero, and it captures the never-reflowed `work_zone_size()` baseline.
  `_collapsed_bottom`: the fit gate DECLINES a 46px strip, which is the frame that proves collapse cannot
  slice the minimap. **The `_ultrawide` PAIR is the frame set issue #377 is judged on** — a bottom dock at
  3440×1080, rendered LAST because it re-pins the canvas and `_reflow_round_trip` compares against a
  baseline captured at `DOCKROW_CANVAS`. They are DOCK-ROW states rather than a wider
  `band_panel_wide_ultrawide` because the parked chrome is half the subject and they need this block's
  REAL seeded minimap: against an empty `MinimapContainer` the rail is the zoom rail's ~80px and a
  mis-placed cluster is nearly invisible. **`_ultrawide` is the busy half** (34 sources, four columns) and
  **`_ultrawide_empty` is the one that carries the width claim** — a band with nothing worked, which the
  busy frame structurally cannot test, since a 34-row board wants every column it can get and a
  content-sized card is then indistinguishable from a monitor-sized one. Four assertions —
  **`_assert_card_is_narrower_than_strip`** (the PRECONDITION: without it the rest pass for free on any
  window the card fills anyway), **`_assert_rail_is_right_justified`** (measured against the STRIP now,
  not the card — the cluster is the card's SIBLING, so the claim is the stronger "at the edge of the
  screen"), **`_assert_card_is_centred`** (the CARD, not its content column) and
  **`_assert_card_follows_its_content`** (the empty card is narrower than the busy one, asks for fewer
  columns, and the difference is EXACTLY the columns dropped — a width-only test would pass on a card
  that shrank for an unrelated reason while the board stayed at four). Measured: **2330px / 4 columns
  busy → 1190px / 1 quiet**, in a 3440px strip. Sabotage-verified: making `_card_width` return the whole
  available strip trips the precondition on BOTH frames (loudly refusing to prove anything, rather than
  passing) plus both halves of the content claim, naming `3119px with nothing to show and 3119px with 34
  sources`. `_reflow_round_trip`: bottom → left → bottom → left, asserting the clusters came home
  to their EXACT parent, child index, anchors and size flags, `BottomBar`'s authored minimum height, and a
  work zone identical to that baseline — reparenting round-trips are where this class of change rots. Four
  assertions back them: `_assert_chrome_parked` (both halves of the swap — the bar's visibility AND each
  cluster's parent, since either can be right while the other is wrong), **`_assert_parked_chrome_fits`**
  (each cluster inside the rail, the rail inside the card, AND the stack CENTRED in the column — fitting
  does not imply centring, since a stack pinned to the mid-line and grown downward still sits inside a
  340px column while rendering ~64px low, which is exactly the `set_anchors_and_offsets_preset` trap),
  **`_assert_no_rail_width`** (which asserts BOTH halves of a retired rail: `_rail_span()` zero — the 25px gutter as well as the column — and the separator hairline hidden, since a `BoxContainer` only skips separation around a HIDDEN child, so the visibility is what makes the span's zero honest), and `_assert_chrome_home_exact`. **The shell-threshold probes derive their widths
  as `WIDE_SHELL_MIN_WIDTH + the live rail width`**: the reflow fires at those canvases too, so bracketing
  the raw window width against the bare threshold would bracket a test the panel no longer applies.



---

## The wide shell's flanks are never narrower than the narrow shell's zone (issue #374)

`ZONE_BAND_WIDTH` is **380** and `ZONE_PARTY_WIDTH` is **`PANEL_WIDTH − PANEL_CHROME_H` = 354**. Both
were 300, and the defect that fixed is the one that reads as absurd once stated: the NARROW shell
hands its single zone the panel's whole strip less chrome — 354 — so **the layout with a whole screen
to spend was giving the same rows LESS width than the layout squeezed into a side dock**. The band
zone CLIPS rather than scrolls, so the missing width came straight off its vitals rows as wraps.

- **The parties zone takes exactly the narrow shell's zone width, and that is the floor the rule
  states**: no wide-shell zone may be narrower than the side dock's. Its four-rung compose picker is
  already 2×2 at that width (`ZONE_POLICY_PICKER_COLUMNS`), so it is the width that control was
  tuned against.
- **The band zone takes the full `PANEL_WIDTH`** because it is the zone whose rows are widest — the
  merged Food line measures 353px — and it is the number this file already uses for "one readable
  column".
- **"Make the parties zone 380 too" is the wrong move and it LOOKS right.** The flanks come out of
  the work zone, and the work board's column count is a `floor`, so the failure is a cliff rather
  than a squeeze. At 1920 in a bottom dock a 380px parties zone leaves the board **763px on Standard
  — two columns by 3px — and 751px on Large, which is ONE**, because the chrome rail tracks the
  minimap's grid aspect and is 308px wide on Large against Standard's 296. Dropping to one column at
  1920 is precisely what the wide shell exists to prevent. The algebraic ceiling on Large at 1920 is
  **371**, so nothing between 354 and 380 is worth taking, and **Large is the worst case in
  `MapSizes.OPTIONS`** — its 104/64 = 1.625 aspect beats even Huge's 1.600. Re-run that measurement
  before raising it, on Large, not on the default map.

`WIDE_SHELL_MIN_WIDTH` follows by derivation to **1190**, and the flip point with the rail's span
added lands at ~1511px of window (Standard) / ~1523 (Large) — see the chrome-rail bullet above.
`PANEL_CHROME_H` moved UP the const block to sit beside the margins it sums, because
`ZONE_PARTY_WIDTH` now references it and a GDScript `const` may not read one declared below it.

## Work rows and the two hunt products (issue #337)

A board row's rate column is a single fixed width, so it shows the product the source actually PAYS:
food when there is food (unchanged for every forage patch and edible quarry), else the trade rate with
`FoodIcons.TRADE_GOODS_GLYPH` — `⇄+0.22` on a hunted wolf pack, never the `+0.00` that said the hunt
was worth nothing. `_work_row_rate_text` is the one definition. The **inspector strip** has room for
the pair and states both (`SourceForecast.yield_components`), which is where an edible quarry's trade
shows.

**The AGGREGATES carry a SIBLING trade total, never a folded-in one.** The header's food figure and
each chip's food figure stay `actual_yield`-denominated — that is the sim's larder identity, and
folding trade in would break the one invariant this arc preserved — but omitting trade entirely made
the header *visibly* not add up: `3 sources +0.35 /turn` with a `⇄+0.22` wolf row directly beneath it,
so the one source paying only trade read as contributing nothing. So the per-row rule is applied one
level up: a second total beside the first, shown only when non-zero. The header reads `3 sources +0.35
/turn ⇄ +0.22` (`WORK_TRADE_TOTAL_TOOLTIP` spells out that it is counted beside the food total, not in
it) and a per-kind chip reads `🦌 2 · 0.20 ⇄ 0.22` — via `SourceForecast.magnitude_components`, the
bare-magnitude twin of `yield_components` (a chip states levels, not deltas, so no `+`). A kind whose
whole set pays trade alone drops the food term: `🦌 1 · ⇄ 0.22`, not a `0.00` denying that its sources
produce anything. **A band with no trade-paying source renders exactly as it did before.**
`_work_component_sum(models, key)` is the zone's ONE summing primitive, so head and chips add the same
rows the same way.

**"Sort by yield" is TWO TIERS, not a raw magnitude sort** (`_work_sorts_before`): food-paying sources
first by their food figure descending, then trade-only sources by their trade figure descending.
Sorting on food *alone* was the bug — it interleaved every trade-only source among the zero-food rows
at the bottom of the board, off page one on a busy band, the same "an inedible quarry is worth
nothing" reading the per-row work removed. But ranking them by raw displayed magnitude is a DIFFERENT
error and must not be "fixed" back to it: a wolf's `0.22` trade above a patch's `0.15` food compares
two quantities the sim publishes **no exchange rate** between, and under a control labelled *sort by
yield* that asserts the wolf is the more productive source — a claim the game does not make and the
player cannot check. Tiering asserts nothing about an exchange rate; it only orders attention. **Food
leads not because it is worth more per unit** but because the larder is the live survival constraint
the player decides against every turn, while trade is still economically thin (the design doc's own
Deferred section). Revisit when trade acquires a sink, not before.

Frames `band_panel_work_trade_rows` (mixed board — food row, food+trade row, trade-only row) /
`band_panel_work_trade_inspector` / **`band_panel_work_trade_totals`** (the same band with the deer
unassigned, so the sole hunt pays trade: `2 sources +0.15 /turn ⇄ +0.22`, chip `🦌 1 · ⇄ 0.22` — the
aggregate suppression path the mixed board cannot reach). The rule and the axis contract live in
`labor-ui.md`.

**Yield is the OPT-IN sort, not the default** — see the next section for why.


## The board's default order is one the player's own edit cannot change (issue #460)

`_work_sort` defaults to **`WORK_SORT_NAME`**. Sorting by yield by default made the board re-order
*mid-edit*: yield scales with workers, so every `+`/`−` press changed the very key the rows were
ranked on, `_repage_work_zone` re-sorted immediately, and the row jumped out from under the pointer —
so the next press landed on a **different source**. A default order must be a function of things the
player is not currently changing. `Sort by yield` remains, one pick away in the `⋯` menu; a player who
asks for a live ranking gets one, and live re-ranking under an edit is arguably what that pick means.

**It sorts KIND first, then label, then `key` — and the kind term is load-bearing, not tidiness.**
`Sort by name` is still one sort with one name; what it orders on is kind-major. The tempting
simplification is that the label prefixes already group by kind, so alphabetical order would do:
`"Forage (%d, %d)"` sorts above `"Hunt %s"`. **That is false, and it is the trap this term exists to
close.** A plant row's label is resolved through `WORK_ROW_PLANT_FORMATS`, keyed on the crew noun
`HudFormat.plant_crew_label` returns — so a source whose Cultivate improvement is done renders
**`WORK_ROW_TEND_FORMAT`, `"Tend (%d, %d)"`**, while its `kind` stays `forage` (the format is DISPLAY
ONLY). Alphabetically `Forage < Hunt < Tend`, so a label-only sort renders a band working a wild
patch, a herd and a Tended Patch as **Forage → Hunt → Tend**: the forage kind split in two with the
hunt block wedged between. `WORK_FILTER_FORAGE` selects on `kind == "forage"` — *both* labels — so
that board contradicts the very chips above it. `band_panel_rung_ready` already stages this mix.

Sorting on `kind` means no third label prefix can break it. The kind test is the same **boolean-tier
idiom** `_work_sorts_before` uses, which is exact for the two kinds that exist; a third would need an
explicit rank, since a boolean cannot express one.

**BOTH comparators tiebreak on the model's `key`, and that is a correctness fix, not tidiness.**
`sort_custom` is **not stable** in Godot, and a tie is reachable in each mode: two herds can carry the
same label (two "Wild Boar" herds produce identical `"Hunt %s"` strings), and two sources can carry
the same rate — two patches at one food figure in the food tier, and every source paying **neither**
component sitting at `0.0` together in the trade tier. (Not "every source paying no trade": the tier
test is `has_component(rate)`, the FOOD figure, so a patch paying food and no trade is in the food
tier and never reaches the trade comparison.) Without the tiebreak neither sort is a total order, so
tied rows could swap on any unrelated re-render — a snapshot tick, a zone resize — which is the same
jump this section exists to remove, just triggered by something other than the pointer. `key` is the
source identity
`_work_source_models` already assigns, so it is the one available key that no game state moves. The
default sort is its own named function for it, **`_work_name_sorts_before`** (it was an inline lambda);
`_work_sorts_before` takes the tiebreak *below* its tier + rate comparisons and changes neither.

**The rate tie is tested with exact `!=`, deliberately NOT `is_equal_approx`.** An epsilon tie test is
not transitive — `a ≈ b` and `b ≈ c` without `a ≈ c` — which breaks the strict weak ordering
`sort_custom` requires, i.e. it would destroy the very property the tiebreak exists to establish.

**The choice PERSISTS, and the vocabulary stays out of the panel.** `BandCityPanel` keeps
`work_sort` in `user://band_city_dock.cfg` beside the dock edge, collapse flag and active tab
(`CONFIG_KEY_WORK_SORT`), exposed as `work_sort_pref()` / `set_work_sort_pref()` — an **opaque
string** it never validates. `BandPanelController` adopts it in `set_panel` only when it appears in
`HudWorkVocab.WORK_SORTS`, and `_set_work_sort` writes back through the panel. The panel owns the
FILE; the controller owns the WORD — the same split that keeps `BandCityPanel` ignorant of every
other zone vocabulary. **What that validation prevents is not an unsorted board — it is the YIELD
board.** `_sort_work_models` is a two-way branch (`WORK_SORT_NAME`, else yield), so an unknown or
retired persisted value — a hand-edited file, a sort name dropped since it was written — would fall
into the else and silently reinstate the very default this section exists to remove. Deleting the
guard as decorative is the one way the old behaviour comes back. The harnesses need no new isolation:
`config_path_override` already points the whole file at a scratch path.

**The `⋯` menu marks the active sort.** `HudWidgets.build_section_menu` grew an optional per-entry
key, **`HudWidgets.MENU_ENTRY_CHECKED`**: an entry carrying it is built with `add_radio_check_item` +
`set_item_checked`, everything else stays a plain `add_item`, so call sites that pass none are
unchanged. **Its ABSENCE is not `false`** — the key is tested with `has`, not read with a `false`
default, because a plain action like `Unassign all` is not a member of any mutually exclusive set and
marking it would claim it belongs to one. The work head passes it on the two sort entries only.
`_repage_work_zone` rebuilds the head, so a pick refreshes the mark with no extra wiring. Without it
the default change would be invisible — the menu offered two sorts and stated neither.

`band_panel_preview` holds both halves at `band_panel_work_page`. **`_assert_work_sort_stable`**
drives the comparators directly (neither claim is visible in a PNG — a re-sorted board is a perfectly
plausible board): under the default it mutates one model's `rate` (the worker step, in miniature) and
asserts the key order is **identical**, then flips to `WORK_SORT_YIELD` and asserts the same mutation
**does** reorder — the counter-check is half the assertion, since a comparator that ignored `rate`
entirely would pass the first test alone. It also sorts one array from two different starting
permutations and requires the identical key sequence, which is the only thing that can see a missing
tiebreak. **It deliberately does not pin `_work_sort` for the first claim**: nothing in the harness
picks a sort, so the live member is exactly what a fresh session boots with, and pinning it to
`WORK_SORT_NAME` would assert that the name sort is stable while saying nothing about which sort the
board actually uses — which is the whole of the issue. **`_assert_work_menu_marks_active_sort`** reads
the popup rather than a frame (a popup is a Window and never lands in the capture) and requires
*exactly one* checked item matching the live sort; it finds the work zone's menu **by its `Sort by
name` entry**, since the parties zone builds a `⋯` `MenuButton` too and the node type cannot tell them
apart.

Both were verified to FAIL: with `WORK_SORT_YIELD` put back as the default the stability assertion
trips (and the menu assertion follows the reverted default, so it is not hard-wired to NAME), and with
the two `key` tiebreaks removed exactly the two total-order assertions fail while the other three stay
green.


## The WORK board's rung-ready mark

Issue #412, `docs/plan_worked_source_marks.md` §5 — the panel twin of the map badge.

A work row carries **three orthogonal glyph axes, each in its own reserved slot**, and collapsing any
two would erase the distinction the feature exists to draw:

| slot | model key | question |
|---|---|---|
| rung | `rung_glyph` | what the source **IS** (its standing rung) |
| ready | `ready_glyph` / `building_glyph` | what it **COULD BE** (`⌃` + the offered rung's glyph, SIGNAL) or what it **IS BECOMING** (`<glyph><percent>%`, SIGNAL_DEEP) — one slot, two mutually exclusive states |
| marks | `marks` | what the band is **DOING** (the verb in flight, plus `⚠`) |

The ready answer is `RungGates.next_rung_ready`, the same call the map badge and the compose sheet's
gates make, so the three surfaces cannot disagree about what is climbable. The `⌃` chevron is
load-bearing, not decoration: the verb and standing-rung glyphs COLLIDE (`▦` is both "Sow" and "this
is a Field"), so the glyph alone would read as *done*.

**THE SEVERITY STRIPE IS UNTOUCHED**, and the `⌃N ready` chip is its own count beside the `⚠` chip
rather than folded into it. The stripe and the attention chip mean TROUBLE (overdrawing, overstaffed,
an unacknowledged edit); a rung on offer is an OPPORTUNITY, and one control meaning both finds
neither. The chip is what makes the knowledge-completion moment legible — a track finishes and a dozen
rows light at once.

`BandPanelController` holds **`_topbar` as a typed collaborator for `faction_tracks` ONLY**, the same
narrow reason `DrawerComposeController` holds it, and a typed ref rather than a Callable per the
extraction rules. Do not grow other reads through it.

**`focus_labor_source` names the LAND for a forage row.** It always named the herd for a hunt row
(`roster_occupant_selected("herd", herd_id)`), but the forage branch only focused the tile and let the
hex's AUTO-PICK choose the subject — so on a shared hex it opened whichever band or herd stood there
rather than the patch, jumping to a place but not to a thing. The land IS the patch's subject (its rung
rows and its Sow control live on the land card), and `SUBJECT_LAND` is the established third kind on
the `(kind, id)` contract that the panel's own land row and the map's select-then-cycle already use.

Frames: `band_panel_rung_ready` (a tended patch offers Sow, a tamed pen-ceiling Aurochs offers Corral,
a wild-ceiling Roe Deer offers nothing — the CONTRAST is the point) and `band_panel_rung_ready_filter`.
