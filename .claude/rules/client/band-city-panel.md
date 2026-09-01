---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{BandCityPanel,BandFoodStatus,PenStatus}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandPanelController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{BandComposeFloat,WorkInspectorDialog}.gd"
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
| `ui/hud/BandPanelController.gd` | `RefCounted` controller (HUD decomposition Phase 2d, `docs/plan_hud_decomposition.md`) owning the **BAND/CITY PANEL's whole render path** — the last big mass to leave `Hud.gd`. It holds the panel HANDLE (`_panel`), the three public **zone builders** `build_band_zone` / `build_work_zone` / `build_parties_zone` and everything under them (the band zone's vitals/PEOPLE/food-outlook/WORKFORCE + role cards; the work zone's paged board, filter chips, pager, inspector strip and source models; the parties zone's rows, inspector strip, footer and the mission compose sheet), the panel's **cycler + snapshot refresh** (`render_band` / `refresh_snapshot` / `rerender` / `cycle_band` / `focus_band` / `select_expedition` / `focus_labor_source` / `confirm_recall_expedition` / `_push_zone_badges`), and the **zone state that survives a snapshot** — `_work_filter` / `_work_sort` / `_work_page` / `_work_open_key` / `_work_policy_open` / `_work_zone_host` / `_work_zone_band` / `_band_zone_tier` / `_party_open_key` / `_party_compose_open` / `_party_compose_mission` / `_send_expedition_count` / `_send_hunt_policy` — ~1,580 lines, 72 moved functions. **`_band_zone_tier` is why the band and work halves are ONE controller**: it is a bare `int` written by `build_band_zone` and read by `_on_zones_resized`, so splitting them would have straddled it. Hud holds it as `_bandpanel`, constructed in `_ready` after `_disclosures` (the vitals row wires its carets through it). **THE PANEL HANDLE IS PRIVATE** — the two non-moving `HudLayer` readers (`_refresh_disclosure_hosts`, `_render_occupant_drawer`) only ever asked "is a panel injected?", so they ask **`has_panel()`** instead of holding the node. **The injection surface is TWO Callables** (it was nine, then six; the three detail-line ones went with `BandDetailLines`, and the four send-expedition/quarry targeting ones went with `TargetingController`), each retained on HudLayer by the "an injection you still have to hold is relocated, not eliminated" test: `_emit_assign_labor` (owns the `assign_labor_requested` emit + optimistic pending write, so `assign_labor` stays INDIRECT) · `_herd_label_for_id`. Each is reached through a **typed adapter**. The parties zone's send-expedition + quarry verbs (`begin_send_expedition` / `begin_pick_quarry` / `cancel_pick_quarry` / `is_expedition_quarry`) are a typed **`TargetingController`** collaborator now, not four Callables. `_is_player_unit` is a trivial private COPY (the `SelectionCardController` precedent). Collaborators: the SAME `_band_labor` / `_compose` model instances BY REFERENCE, `_selectioncard` (roster lookup + map pinning, for the cycler / labor-source / party jump routing, **plus `selected_terrain_label()`** — the one selection read the vitals rows need), `_disclosures` for `wire_label` ONLY, **`_banddetail` (a typed `BandDetailLines` ref — the vitals label and the parties inspector strip render through it; the three `*_fn` members `_unit_summary_lines_fn` / `_expedition_summary_lines_fn` / `_expedition_row_tooltip_fn` and their adapter wrappers are DELETED, the tooltip being a static `DetailFormat.expedition_row_tooltip` call now)**, and the HUD CanvasLayer as the **host** it `add_child`s its `ConfirmationDialog` into (a `RefCounted` cannot parent — the `TurnOrbController` pattern). **It emits SIX signals, all RELAYED by HudLayer** (the controller never emits a HudLayer signal): `cancel_order_requested` · `send_hunt_expedition_requested` · `recall_expedition_requested` · **`split_band_requested`** · `alert_focus_requested` · `roster_occupant_selected`. **`set_band_city_panel` / `cycle_panel_band` / `focus_panel_band` MUST stay callable on the HUD node** — `Main._wire_band_city_panel` probes all three with `has_method` and binds the latter two to `BandCityPanel`'s `cycle_requested` / `subject_activated`, and a failed probe fails SILENTLY — so HudLayer keeps them as thin delegators. **`_build_allocation_panel` does NOT live on this controller**: it writes the drawer's `%AllocationPanel` node, so it stays with the drawer render dispatch (it moved to `SubjectDrawerController` with that dispatch in Phase 2c-3, still a thin function stacking this controller's three public zone builders; its two siblings on that host, `_build_band_move_actions` / `_build_expedition_panel`, are branches of `_render_occupant_drawer` and travelled with it for the same reason). Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`SelectionCardController`/`DrawerComposeController` convention. Behaviour identical to the old inlined band-panel code |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail (the rail runs along the dock's PLENTIFUL axis — stacked on L/R, one line with the restore toggle right-justified on T/B — and `COLLAPSED_SIZE` is a FLOOR on the strip it reserves, not an answer; see "The collapsed rail runs along the dock's plentiful axis"). Header (stage glyph/name/label + the band's hex coordinates + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse) plus an **ACTION REGISTRY** — a registration seam (`register_action` / `action_invoked`) holding every verb the panel offers, the `⚒` included, rendered on its own BAR row under the header on a vertical dock, on the SUBJECT ROW itself on a horizontal one and on the COLLAPSED RAIL in either, taking zero height wherever it is not the live mount; see "The action registry is ONE list with THREE mount points" — body hosts **AN ORDERED LIST OF NAMED ZONES AT A FIXED CROSS-AXIS SIZE**, declared by the SUBJECT via **`set_zone_layout(specs)`** and filled by **`set_zones(contents)`** (keys `&"band"`/`&"work"`/`&"knowledge"`/`&"parties"`; the panel OWNS and frees them, and frees a content handed in for a zone the layout does not declare). A band declares three, the faction page four — see "THE BODY IS AN ORDERED LIST OF ZONES". Two shells, chosen by the panel's own **WIDTH** (`wide_shell_min_width()` — never a dock-edge test, so a resizable dock needs no special case). **That threshold is DERIVED FROM THE LIVE ZONE LIST, never hand-picked and never a fixed set of terms**: it sums each declared zone's flank (an EXPANDING zone contributing `ZONE_WORK_MIN_WIDTH`, the one readable board column the test exists to protect) plus **one `RAIL_SEPARATOR_SPAN` per GAP** plus `PANEL_CHROME_H` — so a band's three come to 380 + 380 + 354 + 2×25 + 26 = **1190** and the faction page's four to 380 + 380 + 354 + 354 + 3×25 + 26 = **1569**. **It is therefore PER-SUBJECT**: on a window between the two the faction page correctly tabs while a band's page stays abreast, which is also why `set_zone_layout` is called BEFORE the zone contents are built. `ZONE_WORK_MIN_WIDTH` (380) MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` — one readable board column — exactly as `ZONE_WORK_MAX_WIDTH` (1520) mirrors `WORK_COLUMN_MIN_WIDTH × WORK_MAX_COLUMNS`; the two are a PAIR with Hud's column consts and move with them. The chrome term is load-bearing because the threshold is tested against the panel's OUTER `_panel_extent().x` while the zones live in `_interior_size()`. It shipped hand-picked at **900**, which broke the whole 900–1055 band (the derived threshold was 1056 then, before the flanks widened): the work zone came out 224px, Hud clamped to one column, its labels clipped — and the NARROW shell would have given the board the full 874px, so flipping wide early made it ~4× narrower, degrading the thing the wide shell exists to improve. `PANEL_CHROME_H` is a `const`; `_wide_separator_span()` and `_fixed_zone_span()` are FUNCTIONS over `_zone_layout`, shared by `wide_shell_min_width()`, `_card_width()`, `_affordable_work_columns()` and `zone_size()` so none of them can disagree about how much width the chrome eats. (`WIDE_SEPARATOR_SPAN`, the `const` that hard-wired TWO gaps, is deleted — it was the one term a fourth column could not have been added around.) **wide** (in practice T/B) = every declared zone side by side, the flanks fixed at `ZONE_BAND_WIDTH` (380) / `ZONE_PARTY_WIDTH` (`PANEL_WIDTH − PANEL_CHROME_H` = 354 — see "The wide shell's flanks are never narrower than the narrow shell's zone") / `ZONE_KNOWLEDGE_WIDTH` (the same 354, taking the same floor for the same rule), work EXPAND_FILL, `LINE_SOFT` hairlines in every gap, no tab bar; **narrow** (in practice L/R) = the subject's own tab bar under the header + exactly one zone beneath it (active tab = SIGNAL ink + a 2px SIGNAL underline, badges via `set_tab_badge(zone, text, hot)`, selection persisted as `CONFIG_KEY_TAB`). **The cross-axis size is FIXED** — `PANEL_WIDTH` 380 (L/R) / `_horizontal_panel_height()` = the body budget (`PANEL_HEIGHT_WIDE` 418 at one band column, `PANEL_HEIGHT_WIDE_TWO_COLUMN` 335 at two, the `maxf` making 418 the live answer at both) **plus the active shell's own chrome** (`_shell_chrome_height()`: 0 wide, the tab bar narrow), clamped to `MAX_WIDE_HEIGHT_FRACTION` of the window (T/B) — see "The strip's height is 418 at ONE band column and 335 at two" — so `current_reservation_size()` changes ONLY on dock/collapse/hide/viewport-resize and a content edit can no longer re-emit `reservation_changed` → `MapView.set_reserved_inset` → cache invalidation (the map flicker on every `+` press). **TWO sanctioned `ScrollContainer`s exist in the panel — the PARTIES list and the BAND zone** — and the harness asserts both halves for each: that it exists, and that no OTHER zone has grown one (`_assert_scroll_only_where_sanctioned`, a table of `(node name, owning zone)` pairs, so a scroll under the wrong zone still fails). Everything else is no-scroll by design; the work zone pages itself against **`work_zone_size()`** — a named reader of the KEYED **`zone_size(zone)`**, which is one answer with one parameter rather than a named accessor per zone that a fourth zone would have to add a fifth of — the zone's interior after chrome — e.g. 354×1107 in a 380 L dock, 789×300 in a 1920 bottom dock with the chrome rail sharing that row — and re-pages on the **`zones_resized`** signal). **Zone hosts are plain `Control`s, not containers**, so an over-wide zone content cannot push the card past its fixed cross-axis size; `clip_contents` keeps overflow inside its own zone. Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`, which since issue #377 fans a HORIZONTAL dock's reservation to the map at 0 (the card floats over live map) and a TOP dock's to the HUD at 0 as well (its readouts belong beside the card, not below the strip). On a **BOTTOM** dock the strip also carries **a trailing CHROME RAIL** the HUD parks its stacked bottom-bar chrome into (`rail_slot_host` / `set_rail_width`, issue #324) — a SIBLING of the card, not a cell of its row, and bottom-only since #377 (a top dock never displaces `BottomBar`, so its chrome stays home). See "Band/City dockable panel". See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/hud/BandComposeFloat.gd` | **The parties compose sheet, floated off the panel when its zone cannot hold it** — see "A COMPOSE SHEET THE ZONE CANNOT HOLD LEAVES THE ZONE" for the trigger. An **`AutoSizingPanel`**, not `PanelCard` + `DockScrollFit`: this card is measured against the VIEWPORT rather than against a dock's remaining height, which is the free-floating half of that pair (`panel-framework.md`). Both axes are fitted explicitly, because the node is a plain `Control` and no child minimum ever reaches it. **It is the card and NOTHING more — there is deliberately no full-screen catcher.** `ComposeSheet`, the herd drawer's floating sheet, is a catcher with a card inside it so a click anywhere outside dismisses; that is exactly wrong here, because the DOCK's sheet stays open through a map pick — the targeting banner and the herd glow ride on the sheet still being open while the player clicks a herd — and a catcher would eat that click. `PanelRoot`'s autopsy applies in reverse: a `STOP` control the pointer finds makes the Viewport mark the press handled before `MapView._unhandled_input` sees it, so every pixel this node claims is a pixel of dead map, and it claims only its own rect (`band_panel_preview._assert_float_leaves_the_map_clickable` drives that through `Viewport.push_input`, never off a `mouse_filter` value). **It never overlaps the card it came from, structurally rather than by a clamp**: `_room()` is the viewport inside `VIEWPORT_MARGIN` cut back to the MAP-FACING side of the panel card (`MAP_FACING_SIDE`, the opposite of the docked edge) with `ANCHOR_GAP` of clearance, and the width fit, the height fit and the placement all read that ONE rect — a card too tall for it scrolls, it does not creep back across the seam. **`target_width` is the ZONE width plus this card's own chrome**, never the zone width itself: `AutoSizingPanel`'s width is the OUTER one, and a sheet handed the zone width minus a border, two content margins and a scroll gutter re-wraps, which would falsify the very measurement that floated it. `mount` applies that width BEFORE the frame `refit` waits, or the height fit reads the previous width's wrapping and leaves the card ~100px taller than its content (measured). Its ONE `ScrollContainer` is not a breach of the panel's no-scroll rule — that rule is about content whose height feeds back into a FIXED reservation, and this ceiling is real viewport room — and it stays DISABLED unless `fit_to_content` finds the content taller than the room. It draws in `BandCityPanel.panel_card_stylebox()`, the panel's own, so it reads as the panel's surface rather than a second kind of card |
| `ui/hud/WorkInspectorDialog.gd` | **The work board's inspector, rehosted OUT of the work zone** (`docs/plan_standing_upkeep.md` §4.9 item 12d) — see "THE WORK INSPECTOR IS A DIALOG" below. An **`AutoSizingPanel`** on its OWN `CanvasLayer` (`HudLayer.work_inspector_host()`, `WORK_INSPECTOR_LAYER_INDEX` = 105), holding the `PanelContainer` `BandPanelController._build_work_inspector` still builds — the head line, the conditional notes, the arrivals strip, and (since item 12d's SECOND pass) the POLICY / PRIORITY / KITS **sections** with their controls drawn, over a two-button actions row. **A `Control` on a layer and never a `Popup`**: `Popup` auto-hides on an outside click and on parent focus loss, which is precisely the dismissal this surface forbids (it RE-TARGETS when another board row is selected, so a stepper press elsewhere is ordinary use). **NON-MODAL — no catcher, no scrim**, `BandComposeFloat`'s rule for the same reason one layer down: every pixel it claims is a pixel of dead map, so it claims only the card. **Centred in the ROOM the dock leaves — one placement for all four dock edges**, no `room_bounds` (it is a surface you WRITE INTO, so it takes a layer above the docked ones rather than dodging them — `panel-framework.md`'s table). `_room()` is the viewport inside `VIEWPORT_MARGIN` cut back to the panel card's MAP-FACING side, `BandComposeFloat`'s own rect through `BandComposeFloat.map_facing_side`. It was centred in the raw viewport for one slice, which held only while the card was ~104–156px tall; the sections took it to 340 and a viewport centre then ran straight through a bottom dock's panel. `mount(strip, reserved, card_rect, map_facing)` is the whole API: `reserved` is `BandPanelController._work_inspector_height`'s answer for the same model and becomes the card's `min_height`, which is how *reserved ≥ drawn* survived the move. Rebuilt per render, never patched (the rung track's rule — every figure on the strip moves per snapshot), and the re-mount IS the re-target |
| `ui/hud/FactionRollup.gd` | **All-`static`, stateless** builder of the FACTION PAGE's FOUR zones (issue #450) — the all-band rollup the cycler pins first. `build_band_zone` (the summed PEOPLE bar + the band page's own vitals rows — Food / **Fodder** / **Upkeep** / Morale / Growth; a sixth, Trade, went with arc #527's retired account, and the `Kit` row it sat beside went with `docs/plan_standing_upkeep.md` §4.9 item 12 — durabilities never aggregated, so that row was an alert and a drill-down, and the CRAFTING panel's kit ledger already states the items in full. The **`Upkeep`** row is the standing MATERIAL bill, folded PER BAND out of `DetailFormat.band_material_bill` and rendering only where some band on the roster owes a good — see `band-readouts.md` → "THE STANDING MATERIAL BILL". The `Fodder` row is the Food row beat for beat, sums the same way, and has the band row's DORMANT form on the same gate folded across the roster — see `band-readouts.md` → "THE FACTION PAGE'S `Fodder:` ROW". `build_band_zone` takes the faction's `{track: progress}` row as a sixth parameter for that row's hover alone, and `_build_vitals_label` CLEARS the previous render's carets before building, which this page did not do until a dormant row inherited one), `build_work_zone` (the whole workforce as one bar and the per-band roster), **`build_knowledge_zone`** (SETTLING, the craft tracks, DISCOVERIES — the fourth column the panel's ordered-list body exists to hold, with a `full` HEIGHT TIER that drops the last of the three in a height-capped horizontal dock) and `build_parties_zone` (every party and the band it left, its NAME jumping to that band — see "THE PARTIES ROW NAMES THE HOME BAND" for why `_summary_row` binds a separate `jump_owner`), plus the `_stat_row` leaf they are built from. Its two new inputs are threaded in as PARAMETERS like every other: the player faction's sedentarization entry and its discovered-site array, read off `FactionReadouts` (`faction_sedentarization` / `faction_discovered_sites`), which is where the PLAYER-FACTION FILTER over those two per-faction wire arrays already lives — a second walk looking for `PLAYER_FACTION_ID` is a second chance to disagree about whose faction is being reported. **It is a shared LAYER rather than a controller because the page is a READOUT** — no steppers, no compose sheet, no open row, nothing that survives a snapshot — so it has no per-cluster state to own, which is the whole of what makes a controller one (`hud-modules.md`). The one thing it needs is threaded in as a PARAMETER: the `HudBandLaborState` instance, plus the faction's `{track: progress}` row and the caller's `herd_label_for_id` Callable (the treatment `HudFormat.panel_expedition_summary` already takes — a stateless layer must not reach for the roster/selection/herd-list state that resolver reads). **IT RE-DERIVES NOTHING**: every total is a SUM over answers the per-band surfaces already give (`DetailFormat.band_net_food` / `band_provisions` / `band_fodder_store` / `band_net_fodder` / `band_material_bill`, `HudBandLaborState.effective_idle` / `effective_worker_map` / `effective_role_workers` / `band_party_workers`, `FactionReadouts.faction_tracks`), so a band's own page and this one cannot disagree about a number — a rollup with its own food ledger would be a second source of truth for the identity `larder_delta == income − consumption − pen_feed − raid_forfeit` the food arc keeps closed. Dependency direction: it reads `HudWidgets` / `HudFormat` / `DetailFormat` / `SourceForecast` / `HudStyle` / the vocab leaves and `FactionReadouts`' track table, and none of them may read it back |
| `ui/PenStatus.gd` | Single source of truth for **"is this pen's herd starving?"** — `FULLY_FED` / `FED_EPSILON` + `fed_fraction(herd)` / `is_starving(fed)`, reading `HerdTelemetryState.penFedFraction` (`< 1` ⇒ the pen's own pasture plus the fodder carried in did not cover its demand, so the herd is SHRINKING every turn — it is never a bill the keeper failed to pay, human food not being animal feed). Plus `herd_is_starving(herd)` for a caller holding only the herd dict. The ONE test all three surfaces ask — the herd drawer's **`Fed:`** row (`DetailFormat.pen_feed_value`, which carries the mark, the fed share, the pasture/fodder split and the shortfall; the CORRAL row states the rung alone, see `herd-readouts.md`), the map's distress badge (`MapView._draw_herd`) and the turn orb's `starving_pen` producer — so they can never disagree about which pen is dying |
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
  (`set_cycler`) over `_player_bands`, a 2×2 **dock chooser** (active edge highlighted), and a
  **collapse** toggle. `cycle_requested(delta)` → Main relays to `Hud.cycle_panel_band`.
  **WHERE THE VERBS RENDER DEPENDS ON THE DOCK'S ORIENTATION** — on this row between the cycler and
  the window controls when docked T/B, on the ACTION BAR one row below when docked L/R (see "The
  action registry is ONE list with THREE mount points"). What is on this row in BOTH orientations, in
  this order, is the subject, the cycler over it, and the two WINDOW controls, which act on the panel
  rather than on the band; the horizontal mount sits between the cycler and those controls, so nothing
  else moves.
  **The `⚒` Materials & Crafting launcher CARRIES NO SUBJECT, and that is why it is panel chrome at
  all.** The registry is subject-independent, so ONE button serves a band page and the faction page and the
  band zone's 300px budget is untouched; `crafting_requested` says only that it was pressed, and
  `BandPanelController` answers WHICH band with `_band_labor.panel_band()` — the panel band on a band
  page, the last band loaded on the faction page, `render_faction` deliberately never touching it.
  Its glyph and tooltip are read back from `HudCraftingVocab`, so the button and the panel it opens
  cannot drift apart. See `crafting-panel.md`.
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
  **THE MARKER IS A STRUCTURAL COPY OF THE COHORT — `entry.duplicate()` plus declared stamps.** It was
  a hand-listed literal naming 56 of the cohort's keys, and it leaked THREE times: `hunt_mode`, then
  `working_age`/`idle_workers`, then the Minimal TOE's six, which made a band's **`Kit` row disappear**
  when you clicked its map icon (`DetailFormat.band_states_kit` is a bare `has()` on the spears key)
  and took the ⚠ zero-effective-attack warning silently with it. Every leak had one shape — the decoder
  grew a field, the panel read it, nobody remembered the list — and **enumerating what to KEEP cannot
  be made safe by care, while enumerating what to ADD can**, the addition being the thing being
  written. Measured at the changeover, the list was already missing 13 more keys off a live cohort,
  four of them read by the panel at the time (`fodder_store`, `raid_forfeit`,
  `expedition_fill_target` — since retired with its lever, issue #491 — and `expedition_trip_bound`)
  — leaks four through seven, unreported and waiting.
  - **The copy is SHALLOW, and that is the correct depth.** `duplicate(true)` would re-allocate
    `labor_assignments` / `stores` / `harvest` / `scout` per band per frame, the per-turn cost
    `turn-profiling.md` spent a pass removing. Those four sub-trees are re-stamped with their own deep
    copies exactly as before, so nested aliasing is unchanged and `snapshot_alias_guard`'s "MapView
    must not write into the decoder's cached world" is untouched — every stamp lands in the copy's own
    top level.
  - **It PRESERVES ABSENCE, which is why `band_states_kit` and `hunt_gate_model` keep their `has()`
    tests unchanged.** `duplicate()` reproduces the cohort's key set exactly: present stays present,
    absent stays absent. It was the hand list that destroyed absence semantics, by dropping keys the
    cohort had. Neither test may become a `> 0` — `0` durability means DRY and must render in DANGER
    ink, and a defaulted `attack 0` would refuse every hunt in the game.
  - **No coercions ride the copy.** The literal wrapped every field in `int()`/`float()`/`String()`,
    which is where the age-bracket narrowing bug lived; a duplicate carries the decoder's own types.
    The surviving coercions are on the STAMPS, and they defend against a hand-built FIXTURE rather than
    the decoder: `pos` from the resolved `current_x`/`current_y` ints, and `dest_x`/`dest_y`/
    `travel_task_kind` out of the `harvest`/`scout` sub-dicts.
  - **A new map-only stamp goes in `marker_field_guard.MARKER_STAMPED_KEYS`**, which asserts the
    partition `marker.keys() == entry.keys() ∪ stamps − omissions` in both directions. Nothing has to
    be remembered for a new *decoder* field: it is covered the day it exists.
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
    − food_consumption`, clamped at 0, over the 20-turn horizon (drain held flat). **The pens' feed is
    not a term** — a pen eats its fenced pasture and its keeper's hay, never the larder — and raids stay
    out for the reason they always did: an episodic past loss is not a steady drain.
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
- **`_panel_band` is the ACTING band, not just the displayed one.** Because it survives a
  selection change and the faction page and is re-resolved every snapshot, it is the middle rung
  of `Hud._resolve_assign_band` — so a compose sheet opened on a herd or a tile staffs the band
  the panel is showing rather than the first one in the roster. Callers must **re-resolve it by
  entity** (`HudBandLaborState.player_band_by_entity`) rather than read `panel_band()` directly:
  `set_panel_band` stores `unit.duplicate(true)`, a render-time copy, and the idle-worker count on
  it is what the compose steppers cap against. The rationale and its guards are in `labor-ui.md`
  → "The `Band:` picker opens on the band the player is LOOKING AT".
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
- **Responsive body — AN ORDERED LIST OF NAMED ZONES, two shells (`set_zone_layout` + `set_zones`).** The block-packing body
  (`set_band_sections` + `_pack_wide_columns`) is **gone**: column membership used to be a function
  of *measured block heights*, so a section hopped columns when the player pressed a `+`, and the
  panel fitted its cross-axis size to content, so every content change re-emitted
  `reservation_changed` and flickered the map. The body is now three named zones — `band` / `work` /
  `parties` — at a **fixed** cross-axis size, hosted by the wide (3 columns) or narrow (tabbed) shell
  per the panel's own WIDTH. Nothing is balanced, so nothing migrates; nothing is content-fitted, so
  the reservation is constant per dock edge. See the `ui/BandCityPanel.gd` roster row for the full
  contract (`work_zone_size()`, `zones_resized`, `set_tab_badge`, the sanctioned-scrolls rule, and
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
    reserved-but-blank — *worse* than the bar, since it is dead space you can neither see nor click. The
    panel keeps its `_reservations` entry, which is what still displaces the event dock past it.
  - **THE HUD YIELDS ON THE BOTTOM EDGE AND NOT ON THE TOP, and the asymmetry is the whole rule**
    (`Main._reserver_overlays_hud`). Insetting the HUD is right exactly when the HUD has something IN
    that strip the card would be drawn over. **BOTTOM**: the bottom bar lives there, so the HUD yields
    and `DockRowController` relocates the minimap and orb into the card's row. **TOP**: the HUD's
    top-right column (turn, faction totals, the Telling card) lives there, and the card is a centred
    island with open strip either side — so yielding pushed that whole column DOWN below the strip,
    stranding it mid-map while the space it belongs in sat empty beside the card.
  - **The exemption is only HALF the fix, and the missing half is why it looks complete.**
    `Main._update_band_panel_lateral_bounds` → **`set_lateral_bounds`** tells the card what to keep clear
    of, and `_available_card_span()` is the one definition of the room that leaves. Without it a band
    with NO worked sources makes a narrow card with room to spare — which is what a first look shows —
    while a 34-source band makes a 1570px card in a 1920px strip and lands straight through the readouts.
    Two consequences follow and both are arithmetic, not regressions: `_affordable_work_columns` counts
    against the bounded span (counting against the raw row builds a board the clamped card cannot hold,
    measured at 135px of overflow into a clipping host), and **`_shell_is_wide` tests the bounded span
    too. **That flipped when the readouts were retired** (issue #450): a 1920 top dock used to pick the
    NARROW tabbed shell — 1920 − 360 − 419 = 1141 against the 1190 three zones need, where 419 was the
    readout block's LIVE width — and with the block gone the trailing bound is the right dock's own
    ~344, leaving 1216. The card gets the wide shell there now, with the board's readable column
    intact; the alternative to tabbing was never "draw over the readouts", it was "have less room".
  - **The bound is the LIVE column width, not the authored minimum** (`HudLayer.lateral_column_widths`),
    and that is the opposite of `left_column_width` / `right_column_width`, which the event dock uses.
    Those bound an EDGE that must not move every turn, so authored is right and a column drawing wider
    merely overlaps a little. This bound decides whether a CARD is drawn over the readouts: measured at
    1920 they rendered **419px against a 344px authored minimum** (the metrics line was simply longer
    than the minimum allowed), so an authored bound put the card through them. **That block is retired
    (issue #450) and the rule outlived it**: the surviving regions are the two DOCKS, whose stacks have
    a zeroed horizontal minimum, so the live read rarely exceeds the authored one today — but a dock
    card that outgrows its column is the same failure one surface along, and the band card re-lays-out
    per snapshot anyway. The band card re-lays-out per
    snapshot anyway, so tracking the live width costs it nothing.
    **A live bound has to be RE-SAMPLED, and that is the other half**: `Main` pushes it from
    `_apply_reservation` *and* from the end of `_apply_snapshot`'s fan-out, because the panel's
    reservation changes only on dock/collapse/hide/resize while those columns move in ordinary play (the
    metrics line grows as its numbers gain digits; `L`/`V`/`R` toggle right-dock cards). A bound sampled
    only on reservation goes stale and the card is drawn over the readouts anyway — the exact failure it
    exists to prevent. `set_lateral_bounds` early-outs on an unchanged pair, so the per-turn push is two
    `maxf`s and a compare, and it re-lays-out only when the columns really moved.
  - **THE STRIP MUST NOT EAT THE MAP'S CLICKS, and that is a `mouse_filter` decision.** `_root` spans
    the whole reserved strip and was left at the `Control` default (`STOP`), which was invisible while
    the card covered every pixel of it. Once the card became an island, `PanelRoot` was the topmost
    `STOP` control over ~1929px of *visible map* on a 3440 bottom dock, so the Viewport marked every
    press handled and `MapView._unhandled_input` never ran — no hex selection, no right/middle-drag pan,
    no wheel zoom, and nothing wrong in the frame. `_root` is **`MOUSE_FILTER_IGNORE`**; the card and the
    chrome cluster are explicitly `STOP`, so each island still eats its own clicks (an `IGNORE` parent
    does not stop its children being picked). This is `EventDockPanel`'s rule read the other way round —
    that root IS its bar, so it sets `STOP` for the same reason this one sets `IGNORE`. Guarded
    behaviourally by `band_panel_preview`'s **`_assert_open_strip_reaches_the_map`** (`push_input`
    through the real dispatch: bare canvas must reach the probe, both gaps beside the card must reach
    it, the card must not) — a PNG is pixel-identical either way, so the claim cannot be a picture.

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
    `wide_shell_min_width()` + the span: **~1511px** of window on a Standard minimap, ~1523 on Large
    (a BAND's three zones; the faction page's four put its own flip 379px higher)
    (the widest rail in `MapSizes.OPTIONS`) — where two gutters put it at ~1618. At 1920×1080 in a
    bottom dock the work zone measures **789px** (`band_panel_dockrow_bottom` prints it), against the
    ~550 the two-gutter geometry would have left it. **The
    minimap tops out near 228px and the 520 `max_width` clamp is UNREACHABLE**, so there is no thin
    margin to worry about and no lever to touch: `MinimapPanel.get_aspect_ratio()` is
    `grid_width / grid_height`, `embedded_height` is 140, so the clamp needs an aspect ≥ 3.71 while the
    widest shipped map is Large at 104/64 = 1.625.
  - **The chrome is separated from the card by a BARE gutter** — `RAIL_SEPARATOR_SPAN` (25) is still
    `ZONE_SEPARATION + ZONE_SEPARATOR_THICKNESS + ZONE_SEPARATION`, *exactly* one inter-zone gutter, and
    `_wide_separator_span()` is still written as one of it PER GAP so the two cannot disagree. **What is gone is
    the drawn hairline.** While the rail was the last cell of `_card_row` it needed one, or it butted
    straight up against the parties content and read as part of it; since issue #377 the two are separate
    islands, and a rule down the gap between them would re-assert the very join the islands removed. The
    `ChromeRailSeparator` `ColorRect` went with it, so the gutter is now pure spacing computed by
    `_position_card_and_rail` rather than `BoxContainer` separation around a hidden child. **`_rail_span()`
    (width + gutter), NOT `_rail_width()`, is still what the width maths subtracts** — using the bare
    width would silently over-report the usable row by 25px.
  - **Two SLOTS, stacked top-to-bottom** (`RAIL_SLOT_TOP` = nav cluster, `RAIL_SLOT_BOTTOM` = turn
    cluster), minimap on top — which on a bottom dock also leaves the orb where it already lives,
    bottom-right. **Only the BOTTOM dock has a rail at all since issue #377**; a TOP dock leaves the
    chrome home (see `DockRowController.REFLOW_EDGES`). `RAIL_SLOT_SEPARATION` is its own const because `DockRowController._required_height` reads it
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
  - **A rail exists only on a BOTTOM dock**, and `_rail_width` forces 0 by EDGE rather than trusting
    the declared value, so the panel is correct whatever order a dock change and the HUD's push arrive
    in. A vertical strip is `PANEL_WIDTH` (380) with no room beside its zones for a ~300px chrome column,
    so `SIDE_LEFT`/`SIDE_RIGHT` are **bit-identical to before**. **`SIDE_TOP` joined them in issue #377**:
    it never displaced `BottomBar` (the inset and the bar are on opposite edges there), so it had nothing
    to recover, and relocating anyway put the minimap and turn orb at the TOP of the screen — chrome with
    a fixed home, moved for a symmetry that was never measured. A top-docked card still floats and
    centres; it simply has the whole strip to do it in.
  - **`_rail_span()` is folded into the ONE width primitive** — `_available_card_span()`, which
    `_shell_is_wide()`, `_card_width()` and `_interior_size()` all read, so the chrome's column and its
    gutter come off the row before any zone sees them and none of the three can disagree about how much
    is left. (It reached this shape by stages: the span was originally folded into `_shell_is_wide()` and
    `_interior_size()` separately, and the ultrawide centring was a `SHRINK_CENTER` on the content
    COLUMN. That branch is gone — the card itself is what narrows now, and the column just fills it.)
  - **THE CARD'S WIDTH IS BUILT UP FROM A DECLARED COLUMN COUNT, NOT CLAMPED DOWN FROM A CAP** (issue
    #377). `_card_width()` (wide shell) = `ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + columns ×
    ZONE_WORK_MIN_WIDTH + _wide_separator_span() + PANEL_CHROME_H` (the two flank terms being
    `_fixed_zone_span()`, a sum over the LIVE zone list), and the column count arrives through
    **`set_work_columns`** from `BandPanelController`, which is the only thing that knows how many
    sources there are. That is the `set_rail_width` contract again — DECLARED, never measured here.
    - **It is acyclic because the count comes from the zone's HEIGHT and from a CONSTANT — never from
      a width.** `_work_board_capacity` derives `rows` from the box's height (which a horizontal dock
      fixes) and `_declare_work_layout` turns that into `cols = ceil(count / rows_per_col)`. Width
      follows count; count never follows width. The SHELL is still chosen from the room the *strip*
      has (`_shell_is_wide`), never from the card, so nothing can feed back into the choice that
      produced it. (This bullet read `cols = ceil(count / rows)` flat until the rows-per-column
      preference below split "how tall a column CAN be" from "how tall it SHOULD be".)
    - **THE COLUMN BREAKS AT THREE ROWS BY PREFERENCE, AND THE PREFERENCE YIELDS TO THE AFFORDANCE.**
      `HudWorkVocab.WORK_PREFERRED_ROWS_PER_COLUMN` (3) is what the board ASKS at, not what it is
      capped to. Filling a column to whatever the height afforded is what made six sources on a bottom
      dock read **5 + 1**: `ceil(6/5)` asks for two columns and the column-major fill drops one lonely
      row into the second. Asking at three asks for the same two columns and fills them **3 + 3**.
      - **The test for taking the shorter column is SOURCES SHOWN, not page size** —
        `min(page, count)` against the height-derived layout's, both costed against the SAME
        `work_columns_affordable()`. That distinction is the whole feature rather than a detail of it:
        compared on raw page size, a 2 × 5 board (10 slots, four of them unfillable by a six-source
        band) beats a 2 × 3 one, so the preference would lose in exactly the configuration it was
        written for and the board would go on drawing 5 + 1. Where the page actually BINDS — a band
        past what its columns can hold, or a strip that affords one column — the two readings agree
        and the taller column is kept.
      - **What that buys is the guarantee the design exists for: no source ever falls off the page for
        this, and `pages` never rises.** A 380px side dock affords one column, so a 3-row fill there
        would drop a 1 × 5 page to 1 × 3 and push two sources onto a second page. The affordance
        refuses the ask, the fallback keeps the full height, and nothing moves — **which is also why
        there is no dock-edge test**: a vertical dock disables the preference by affording one column,
        not by being asked what edge it is on.
      - **The affordance is READ, not discovered by declaring.** `BandCityPanel.work_columns_affordable()`
        publishes the cap `set_work_columns` applies, so the controller costs both candidates and makes
        exactly ONE declaration per pass. Declaring twice to find out would resize the card mid-build
        to a width it then hands back. It does not invert the declare direction — the affordance is
        geometry (strip, edge, shell, lateral bounds), the same kind of thing `work_zone_size()` already
        reports, and none of it depends on the source count.
      - **Measured across the whole dock/viewport matrix** (`band_panel_preview`'s
        `_probe_work_board_layout`, 11 configurations × 5 source counts × 2 queue depths): the only
        boards that moved are the ones that were lopsided. A bottom dock affording two columns goes
        6 sources 2 × 5 → 2 × 3 and 4 sources 1 × 5 → 2 × 3; 8 and 12 sources keep their tall columns
        because there the page binds; every vertical dock and every one-column bottom dock is
        unchanged; the 34-source board is unchanged to the pixel.
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
    - **The COLUMN's own `SHRINK_CENTER` cap is gone with it.** `_apply_wide_content_cap` (since deleted, along with the `_wide_content_cap()` it clamped to) capped the
      column inside a full-width card — the only thing a full-bleed bar leaves to narrow — and
      `SHRINK_CENTER` clears the expand flag, so with `_rail` on `SIZE_FILL` the row had no expanding
      child and `BoxContainer` packed both from the LEADING edge: column flush left, parked chrome at
      the **72% mark**, ~790px of dead card trailing it. The column simply fills its card now, and the
      centring happens one level up on the whole card.
    **`_panel_extent()` is the STRIP, not the card** — it was the card's outer size while the two were
    the same rect, and issue #377 separated them without renaming it. Every width now derives from it
    through `_available_card_span()` (strip less the chrome span less the HUD bounds); reading
    `_panel_extent().x` as "how wide the card is" gets you the whole monitor. `_position_seam()` likewise
    changed: the seam accents the map-facing edge of the STRIP, which is right only while the card fills
    it, so a horizontal dock hides it rather than ruling a line across the monitor above a small floating
    card.
  - **`set_rail_width` COULD once never re-emit `reservation_changed`, and that is no longer true.**
    The old guarantee rested on the rail spending only the LONG axis while the reservation is the CROSS
    one, and on that cross axis reading nothing but the collapse flag, the dock edge and the viewport.
    Since it carries the active shell's chrome (`_shell_chrome_height`) and the shell is chosen from the
    span the rail comes out of, **a rail width can flip the shell and move the strip by the tab bar's
    height** — so `set_rail_width` and `set_lateral_bounds` both call `_republish_reservation_if_changed`.
    The loop the old rule protected still terminates: the HUD's second push lands on the same number and
    is dropped by `set_rail_width`'s own early-out, and the republish is silent on an unchanged size.

### A size the panel DRAWS but never PUBLISHES is a bar drawn through the card

`Main` does not poll this panel. It stores what `reservation_changed` carried, and
`_update_event_dock_edge_offset` sums that to decide where a co-edge event-dock bar starts. So the
published number and the drawn number must not diverge.

They could not, while the cross axis was a pure function of the collapse flag, the dock edge and the
viewport — the three paths that move those all emit. **They can now.** The axis carries the shell's own
chrome, the shell is chosen from `_available_card_span()`, and that span's other two terms — the lateral
bounds and the rail's span — arrive **DECLARED**, on setters that relayout without emitting.
`set_lateral_bounds` is re-pushed on every snapshot on a TOP dock. Measured at `ui_scale` 1.35, band
panel and event dock both docked TOP: the panel drew **395** while `Main` still held **360**, and the bar
sat 35px inside the card, cutting through the role cards.

**This was a regression introduced by the `_shell_chrome_height` term itself.** Before it, a shell flip
could not change the reservation, so the missing emission on those two setters was latent and harmless;
making the reservation shell-dependent turned it into a live 35px error.

`_republish_reservation_if_changed` compares against `_published_reservation` (seeded to `-1.0`, a value
no reservation can take, so the first republish is never suppressed by a coincidence with an unset
member). **Every pre-existing `_emit_reservation()` call site stays UNCONDITIONAL, deliberately**:
`Main._apply_reservation` is not only how the size travels, it is the hook that re-pushes the lateral
bounds and recomputes the event dock's perpendicular insets, both of which read live HUD geometry a
viewport resize can move without moving this panel at all. Deduplicating them would silently stop that
recomputation; the republish is strictly additive.

**The guard is asserted by CONSUMING `reservation_changed`, never by polling
`current_reservation_size()`** — polled, it passes with the defect in, because the poll re-derives the
very number that was never published.

### The horizontal card already takes its whole span

Measured on a TOP dock at 1.35: `_bound_leading` 360, `_bound_trailing` 344, `_available_card_span()`
718.2, `_card_width()` **718.2** — equal, at 1.0 and 1.35 alike. A gap between the card and the screen
edge on a horizontal dock is the HUD's authored lateral column (`Hud.lateral_column_widths()`, a
`max(authored, live)`), which the card is holding clear on purpose. It is not the card failing to
stretch, and widening it into that gap would put it over a live HUD column.
- **Zone `band` — vitals · PEOPLE · KEEPING · food outlook · WORKFORCE + role cards** (`BandPanelController.build_band_zone`; the KEEPING block is `docs/plan_standing_upkeep.md` §2.5's and is specified in "THE KEEPING BLOCK" below).
  The Food/Fodder/Upkeep/Morale/Growth rows are the disclosures — and their breakdowns open in a
  POPOVER, never inline (see Band food status: inline growth is what clipped this very zone).
  **There is no `Output:` row and no `Position:` row here.** Productivity reads on the WORK zone's
  head, where the rates it scales are (see Zone `work` below); the coordinates read in the panel
  HEADER, where the rest of the band's identity is (see "Header chrome"). Both left because this
  column is the one with no room, and both landed where their answer is already being used —
  `unit_summary_lines` gates the second on `with_position`, and the Occupants drawer, which has no
  header, keeps it.
  **The SHORT tier MERGES its optional rows rather than dropping them** (`_build_vitals_label` passes
  `compact` to `BandDetailLines.unit_summary_lines`). It is the row-level twin of this zone's
  food-outlook-chart gate below and taken for the same measured reason — a vitals row measures 26px
  against a ~300px T/B zone that CLIPS rather than scrolls — but `compact` says **HEIGHT is scarce,
  not width**, which is the horizontal dock exactly, and that buys the merge treatment: the fodder
  stock rides the Food line as `· 128.4 fodder` instead of vanishing, because that larder has no other
  surface to be legible on. See `band-readouts.md` for the clause and the width it was measured
  against. **A `Trade` row was the one row this tier DROPPED**, on the reasoning that its rate still
  read on the WORK zone header — the whole reason a drop was affordable for it and for nothing else;
  arc #527 retired that account, the row and the header total together, so the tier drops nothing
  today. The `Population … Workers … (Idle …)` LINE is
  **gone** — the two bars below state the same facts as charts, and a text restatement above them was
  the third telling of one fact. **PEOPLE** is the new one: a stacked children/working/elders bar
  (`children`/`working_age`/`elders`) plus its
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
  **THE WIRE CARRIES WHOLE PEOPLE, AND THIS PANEL RENDERS THEM.** `children` + `working_age` +
  `elders == size`, guaranteed by the sim, and `working_age` IS the working bracket — there is no
  second worker number on the dict for a reader to disagree with. The panel rounds nothing. It used
  to: the brackets shipped as raw fixed-point `Scalar`s (the fraction is a GROWTH ACCUMULATOR
  internal to the sim, not a fact about people) and the client apportioned them by largest remainder
  for itself, which could round a 16.6-worker cohort UP to **17** in this bar while the WORKFORCE
  header directly below counted the sim's floored **16** — one band, one frame, two answers. The
  decision moved to the sim, which is the only place it can be made once.
  **Absent age data OMITS the whole block** — never a fabricated split; the header total is the
  band's own `size`, so the two bars cannot disagree about how many people are in it.
  Its palette is deliberately MUTED (`VOICE_PIGMENT` / `INK_DIM` / `VOICE_INK`) against
  **WORKFORCE**'s saturated one (`HEALTHY` / `SIGNAL` / `VOICE_INK` / `WARN` / `VOICE_PIGMENT` /
  `INK_FAINT`): two bars,
  same shape, different question — *who they are* vs *what they do* — and they must not read as the
  same chart twice.
  **THE WORKFORCE SEGMENTS PARTITION `working_age`, WHICH IS WHY THE BENCH AND THE BUILDERS EACH HAVE
  ONE.** Forage · Hunt · **Build** · Roles · **Bench** · Idle, and the head states `n idle of m` off
  the same `HudBandLaborState.effective_idle` — which nets BOTH of those crews out: a worker at the
  bench is assigned labor (`crafting-panel.md` → "The stepper's ceiling"), and a builder is the band's
  own `builders` ROLE since §2.5 (`labor-ui.md` → "`effective_idle` SUMS `staffed_total`"; the segment
  is `effective_role_workers(band, "builders")` now, the per-source build crew it used to sum having
  left the tile). Without a
  segment of its own each would leave Idle and appear nowhere, so the bar would quietly stop adding up
  to the head beside it — which is exactly what shipped: `Forage 9 · Hunt 6 · Idle 3` accounting for
  18 with three builders both miscounted and invisible.
  **The builders are their own slice rather than folded into Forage or Hunt**: those two name what a
  crew TAKES, and a builder takes nothing, so folding them in would show a band gathering from a patch
  nobody is gathering. It draws in `SIGNAL_DEEP` — the live cyan a rung under construction already
  wears, one step down, so it reads as work-in-flight without competing with the Hunt slice beside it.
  `FactionRollup._build_workforce_block` carries both segments for the identical reason — the two bars
  are one chart at two scales, and a faction total missing a segment the band bar has is the same hole
  one level up.
  **PARTIES ARE A HEADER CLAUSE, NOT A SEGMENT** — `3 idle of 16 · 10 away`
  (`HudWorkVocab.WORKFORCE_AWAY_FORMAT`, shown only when the count is non-zero, tooltipped with what
  "away" means). The sim removes a party's members from the parent band's working-age cohort the turn
  it launches (`band_cohort.working -= party_scalar`), so those hands are not inside `working_age` at
  all; a Parties SEGMENT therefore made the segments sum PAST their own denominator — a bar totalling
  22 above a head reading "4 idle of 16". The fact still has to be reachable, so it moved to the head
  rather than being deleted. `FactionRollup`'s bar takes the same clause off the same
  `HudBandLaborState.band_party_workers` sum. **FIVE standing roles are CARDS** — Scout + Warrior
  here, Agriculture + Husbandry + **Builders** in the KEEPING block below, in rows of two — (bordered, name · the `−/+` stepper and its
  `assign_labor` emit · **the kit row, where the role has one** · the role's description LAST), not rows
  in a list — the fix for a standing role being indistinguishable from a worked source. That middle
  slot differs by role and the difference is a rule rather than a layout: Scout and Warrior mount a
  PICKER over its gear line, Builders the gear line ALONE (a per-band pick over a per-entry
  derivation), and the keeping pair neither. See "The role cards carry the band's OTHER two kits"
  below for the picker half and for why the prose trails, and "THE KEEPING BLOCK" for the other two.
  **The Warrior card carries a LIVE THREAT ALERT** (Predators Phase 3): when
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
  n sources · total /turn · the fodder total when non-zero · **`Output 62%` when the band is below
  full productivity** · a `⋯` `MenuButton`) · filter CHIPS · the board · pager. **The inspector strip
  is NOT in that stack any more** — §4.9 item 12d hosts it as a viewport-centred `WorkInspectorDialog`
  off the panel entirely; see "THE WORK INSPECTOR IS A DIALOG".
  **The Output item QUALIFIES the two totals beside it rather than adding to them**, which is why it
  trails them and why it lives here at all: `output_multiplier` is the discontent modifier every rate
  on this board is already scaled by, so the head is where its consequence is visible — a vitals row
  in the height-capped band zone stated it away from everything it acts on. Same gate that row
  carried (**only below `SourceForecast.OUTPUT_FULL`** — a permanent `Output 100%` is noise on a row
  that is otherwise live summary, the rule the sibling account total already follows) and the same buckets,
  through `BandFoodStatus.color_for_output`, a `Color` accessor rather than a hex one because this
  head is built out of `Label`s rather than BBCode — and, since the head is now the multiplier's ONE
  surface, the only accessor there is (`hex_for_output` went with the vitals row that called it). Vocabulary (`WORK_OUTPUT_FORMAT` /
  `WORK_OUTPUT_TOOLTIP`) is `HudWorkVocab`'s, like every other head item's.
  **A ROW THE BAND HAS ONLY BUILDERS ON IS STILL A ROW.** The admission test read the TAKE crew alone,
  so a source with three hands raising its meter and nobody gathering was dropped from the board, from
  its chip counts and from the WORK tab's badge — the one place a player would look to see what those
  hands are doing, and the one place the `+` that staffs them lives. `_work_source_models` admits on
  either crew and carries `build_workers`, which the inspector strip states as a `N building` clause
  beside the take's `N assigned` (`labor-ui.md` → "`effective_idle` SUMS `staffed_total`").
  **The chips ARE the summary and the filter** (All / 🌿 Foraging n · rate / 🦌 Hunting n · rate / ⚠ k,
  the last hidden at k = 0), replacing collapsible group headers. Both the header total and the chip
  rates state BOTH products, each only when non-zero — see "Work rows and the two hunt products".
  **Rows are TWO lines at a fixed `WORK_ROW_TWO_LINE_HEIGHT`** (44 = `WORK_ROW_HEIGHT` + the two-line
  stepper's own gap + one `WORK_INSPECTOR_NOTE_LINE_HEIGHT`): **line one is identity and controls** —
  severity stripe (WARN overdrawing/overstaffed, SIGNAL pending) · glyph · name · the SOURCE-RUNG mark
  · the rung-on-offer slot · policy/⚠ marks · the `−/+` — and **line two is the ACCOUNTS then the
  FLOOR**, full width, indented onto the name's own column. See "THE ROW IS TWO LINES" below. The rung mark and the
  policy marks are TWO AXES — what the source IS against what is being done to it — and the row keeps
  both; the rung slot is reserved on every row, so the label's share of a
  `WORK_COLUMN_MIN_WIDTH` column is ~20px narrower than the marks column alone would suggest (spec in
  `labor-ui.md` → "The work row carries TWO axes"). **Capacity is derived ENTIRELY from
  `work_zone_size()`** (`_work_board_capacity`): `cols = clamp(w / WORK_COLUMN_MIN_WIDTH, 1,
  WORK_MAX_COLUMNS)`, `rows = (h − head − chips − pager) / WORK_ROW_TWO_LINE_HEIGHT` (the `− inspector`
  term retired with §4.9 item 12d), filled
  **column-major** with a hairline between columns; the pager is resolved in **two passes** because it
  only exists when one page cannot hold everything yet costs a row. **EVERY reserved height must be
  what the element actually draws at** — the default `HudStyle` button chrome pads 9px top and bottom,
  which alone makes a stepper ~40px and pushes the page off the bottom of the zone, so the board's
  buttons take `HudWidgets.compact`'s squeeze. Clicking a row opens the **inspector strip**: the row's
  old second/third lines in one place (yield/policy/status in words, warning lines, the `ArrivalStrip`)
  plus, since §4.9 item 12d's second pass, the POLICY / PRIORITY / KITS **sections** with their
  controls drawn (the policy grid is the four EXTRACTIVE rungs only — the investment rungs are ladder
  commitments made at the source's own compose control, where their gates and payoff forecasts live)
  and a two-button actions row: `Jump to source` · **`Unassign`**. That is the per-source removal: a
  hover `✕` beside the `−` stepper would be a mis-click hazard, this is the labelled version. One row
  open at a time, and since §4.9 item 12d it COSTS THE BOARD NOTHING — the strip is the body of a
  viewport-centred dialog, so the capacity maths has no term for it to subtract. The retired reading
  was *"it COSTS board rows, which is why the capacity maths subtracts it."*
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
  **A REAL RECALL CONFIRMS; A CANCEL DOES NOT** — see "THE RECALL VERB FOLLOWS THE SIM" below.
  `BandPanelController.confirm_recall_expedition(exp)` names the party (`_herd_label_for_id` for a hunt,
  "scouting" for a scout) through the shared `_confirm_destructive` on the recall branch and acts
  straight off the press on the cancel one, and every SINGLE-recall entry point (the row `✕`, the
  strip's link, the Occupants drawer's button) routes through it; `_on_recall_expedition_pressed` stays
  the RAW emit, so "Recall all" loops it under its OWN one confirm and never pops N prompts.
  **The row BODY opens an inspector strip** (`_toggle_parties_inspector(str(entity))` → `_party_open_key`
  → `BandPanelController.rerender`, the exact `_work_open_key`/`_build_work_inspector` pattern): a bottom
  `PanelContainer` (reusing `HudStyle.work_inspector_stylebox`) with a titled header + close `✕`, the full
  `_expedition_summary_lines` detail as dim status parts (Mission / Target / **Orders** / Phase /
  Carried / **Next delivery** / the trip-bound clause — so the strip IS the detail panel), and
  `Jump to party` (INK) / `Recall` (DANGER) inline links. **`Position` is in the producer and never
  reaches THIS host**: it renders off `pos`, which is the map marker's stamp, and the parties zone reads
  the raw cohort dicts, which carry `current_x`/`current_y` and no `pos` at all. It is live in the
  Occupants drawer, which is reached through the marker. The **"Next delivery" line** (`_expedition_next_delivery_line`, shared by the
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
  zone. **That box is ~300px and it CLIPS, so the strip's height is a budget and both halves of it have
  now been spent** — see "The parties strip's SEVEN lines" below.
  The footer offers the two missions **DIRECTLY** — `⚑ Scout` and `🏹 Hunt`, side by side —
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
  the band cannot work from home, so a nearer herd is a local hunt (**this rule is the HUNT form's
  alone** — the denial form relaxes it, see "DENIAL is a third MISSION" below).
  `TargetingController.is_expedition_quarry` is the ONE definition (`SourceForecast.band_tile` + `_hex_distance_wrapped`, the herd drawer's own split) and all three sites
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
  **The HUNT form states the trip's BOUND** (`docs/plan_hunt_through_combat.md` §5.2) as its own quiet
  line beneath the one-line forecast, rather than folded into it: THAT form is the one-liner already
  carrying five facts, where the drawer's boxed readout folds the identical clause into its verdict —
  both through `SourceForecast.trip_bound_clause`, so the two surfaces cannot phrase one stop
  differently. **This zone is the SECOND launch site of `send_hunt_expedition`, and the arc's standing
  rule is that the two entry points cannot offer different orders** — a lever present on the herd
  drawer's sheet and absent here would be the same defect as a lever that does nothing. Since issue
  #491 the FLOOR is the only order either sheet composes; the fill target that used to sit under the
  party stepper here is retired, and why is in `labor-ui.md` → "RETIRED — the FILL TARGET".
- **Destructive bulk actions ASK, and name what is SPARED** (`_confirm_destructive`, a
  `ConfirmationDialog` — a Window, like the `⋯` `MenuButton`'s popup, so opening either cannot move a
  zone's height). `Unassign all work` sends **`cancel_order <faction> <band> work`** — the signal
  `cancel_order_requested(band, scope)` gained the scope this pass; `work` clears Forage + Hunt only
  and leaves standing roles, parties and an in-progress move alone. `Recall all` is one
  `recall_expedition` per party (no bulk verb, and parties are few).
  **IT IS THE PANEL'S ONE CONFIRM PATH — three prompts, one surface.** The recall prompt,
  `Unassign all work` and `Recall all parties` all go through it, so the console's dialog
  treatment (`HudStyle.apply_dialog`, `sprites-widgets.md` → "The modal dialog") is applied HERE and
  reaches all three from one call. Every one of them wore Godot's stock light-grey chrome and a
  default `Confirm` title bar until it was, on a dark cyan-accented HUD — reported from playtest as
  looking like another application's dialog. **A new prompt in this panel must not build its own
  `ConfirmationDialog`**: there is exactly one construction site in the client, and that is what
  makes the surface a fact rather than a convention. `HudWorkVocab.CONFIRM_DIALOG_TITLE` is still
  set on the Window and is no longer DRAWN (the treatment is borderless) — it is the window's NAME,
  which only an unembedded dialog would show. **The `settle` prompt was the fourth** — it went with
  the retired arrival verb, and `split_band` deliberately has no prompt at all: the split sheet's own
  verdict block already states what it costs, so a modal would ask the player to confirm a number
  they are looking at.
- **Move and Clear all are GONE from the panel.** Move belongs to the Tile panel in a later change;
  `_on_move_band_pressed` / `_pending_move_band` / the whole targeting machinery are intact and still
  reachable (the expedition drawer's Move), just not surfaced here.
- **A zone must FIT its zone.** The hosts clip, so overflow is invisible in a frame — and a zone
  content whose *minimum* size exceeds the zone (four policy rungs abreast in a 380px dock) does worse:
  it drags the whole zone column out past its host, taking the section menu beside it off the edge.
  Hence `band_panel_preview`'s **recursive zone-bounds assertion**, which is the only thing that
  catches either.
  **THE ZONE'S OWN 2-COLUMN CLAMP IS RETIRED, and the constant with it.** `ZONE_POLICY_PICKER_COLUMNS`
  (2) existed because the picker's long faces could not fit three abreast here — at 3 the rungs
  overran the ~354px zone and the measured frame came back with a face cut in half, the Quarry button
  clipped and two extra `_assert_zone_content_fits` failures. **The faces were the problem, not the
  column count**: they are one word each now (`Everything` / `Best` / `Learning`,
  `HudComposeVocab.FLOOR_PRESET_LABELS`, with the phrase each stands for leading its tooltip), and the
  grid measures **234px of a 356px column** at the shared `POLICY_PICKER_COLUMNS` (3). So both this
  zone's pickers — the parties launch sheet and the work inspector's — take the shared default and the
  Band panel's picker is no longer a different creature from the free-floating one. **The horizontal
  padding was NOT cut**: the shortened faces alone left 122px spare, so trimming `POLICY_PICKER_PADDING_H`
  would have been chrome spent on nothing.
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
  state stages ONE band with a hay larder, productivity below full, a fertility
  reading, a kit ledger, and the per-source `arrival_schedule`s the FOOD OUTLOOK chart
  needs — every gate in `build_band_zone` / `unit_summary_lines` live at once — in the height-capped
  TOP dock, and runs the bounds assertion, `_assert_zone_content_fits` and
  `_assert_merged_food_row_fits` over it. It reads **299px of a 300px box**. The margin is the point,
  so the state also PRINTS its per-zone extent (`_report_zone_content_extent`): a near-miss and a
  comfortable fit are the same green line otherwise. Sabotage-verified — putting the `Output:` row
  back takes the run from 0 errors to 25, `short by 25`.
  **The PARTIES zone needed the same state and for the same reason** — `band_panel_worst_case_party`,
  the party carrying every optional detail line at once. See "The parties strip's SEVEN lines" below;
  the lesson generalises, so a zone that clips and a producer with conditional lines want one of these
  before the count is trusted.
- **The no-dock fallback renders the SAME three builders**, stacked into `%AllocationPanel`
  (`_build_allocation_panel`) — there is no second layout to maintain. It passes `with_vitals = false`,
  since the Occupants card's own drawer already prints those rows above it.
- Verify chrome + reflow via `tools/band_panel_preview.gd`
  (`scripts/preview.sh res://tools/band_panel_preview.tscn` → `ui_preview_out/
  band_panel_{left,right,top,bottom,collapsed}.png`). **The ZONE states are the Part-2 frames:**
  `band_panel_people` (both bars, the dependency ratio, the two role cards) ·
  **`band_panel_workforce_away`** (the reference band with its TWO parties in the field — the only
  configuration in which the Parties defect is visible: the header reads `3 idle of 16 · 10 away`
  over a bar of Forage 5 · Hunt 4 · Roles 4 · Idle 3, summing to 16 rather than to the 26 a Parties
  segment made of it) ·
  **`band_panel_people_map_path`** (the SAME block reached the OTHER way — by clicking the band ON THE
  MAP, through the real `MapView._rebuild_unit_markers` → `refresh_selection_payload` →
  `show_unit_selection` → `BandPanelController.render_band`. `band_panel_people` drives the SNAPSHOT path,
  which re-resolves the brackets from the `populations` entry and therefore SELF-HEALS a lossy marker
  copy — so it structurally could not catch the dropped age brackets. This
  state ASSERTS the three PEOPLE brackets sum to the band's own `size`. **It also
  carries the Minimal TOE's kit claims** (`_assert_map_path_states_kit`): the PAYLOAD holds every kit
  key — named from `DetailFormat`'s and `SourceForecast`'s OWN constants, since the structural copy
  leaves no key list on MapView to borrow and borrowing one would assert that the copy copies what the
  copy copies — the payload is the WHOLE cohort, and spears arrives un-narrowed as the FLOAT the wire
  carries. **The RENDER half of it is retired with the `Gear` row** (`docs/plan_standing_upkeep.md`
  §4.9 item 12): no vitals row states an item condition any more, so there is nothing on this path
  left to draw `Spears 87`, and the payload claim is the one that mattered — the leak this was written
  for is the marker copy dropping wire fields, and the crafting panel's kit ledger reads exactly those
  fields off exactly that payload. Its band still comes from **`_kit_band_fixture`, a SEPARATE
  fixture, and that separation was itself a finding**: while the `Gear` row existed it cost 26px
  against a band zone already reading 299 of its 300px box in a height-capped T/B dock, so putting the
  kit keys on the shared `_band_fixture` overflowed `Zone_band` by exactly 25px in **13 states**. The
  fixtures stay apart because the payload claim wants a cohort carrying every kit key and no other
  state does) ·
  `band_panel_work_page` (34 sources, narrow shell) · `band_panel_work_wide` (the same 34 in the
  bottom dock — 4 columns, column-major, `Page 1 / 2`, `1–28 of 34`) · `band_panel_inspector` (a row
  open, the board shrunk to 31 rows and a pager appearing to pay for it) · `band_panel_compose_hunt`
  (quarry → policy → party → forecast, with the real per-policy metrics and max-useful cap) ·
  **`band_panel_compose_hunt_eradicate`** (the ONE surface that renders `SEND_HUNT_POLICY_HINTS`
  verbatim, so it is the frame the EXPEDITION Eradicate hint is judged on: the rung's face reads the
  ladder's top `💀 +6.50`, the hint describes the one-trip haul, the currency the SPECIES pays
  + the permanent end state, and
  the raid line below it delivers `~52 food` under an ordinary primary Send — no
  denial anywhere, #337) ·
  `band_panel_compose_hunt_no_quarry` (the empty state: `Choose…`, the hint, a disabled Send, nothing
  below — reached by CLEARING a composed quarry, so it inherits the full form's mark) ·
  **`band_panel_compose_hunt_empty`** (the same form reached the way a PLAYER reaches it — a band with
  no parties, the composing act closed and reopened through the REAL `🏹 Hunt` footer button, in the
  tall LEFT dock. It is the state that was missing when the floating-sheet defect was reported the
  second time: every other compose fixture writes `_party_compose_open` and picks a quarry first, so
  the harness never rendered the smallest the sheet ever is) ·
  `band_panel_compose_scout` (the same sheet under Scout — no quarry row, no policy picker). A
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
  colour is now a named const rather than a literal inside `apply_button`). The harness also ASSERTS, per state, that **every
  `ScrollContainer` in the panel is a SANCTIONED one under the zone that sanctions it** (the parties
  list and the band zone; `_assert_scroll_only_where_sanctioned`) and that **nothing a zone renders
  falls outside its zone rect** — checked RECURSIVELY, since the top-level content is anchored
  full-rect and so always "fits" while the thing that actually overflows is a board row off the
  bottom of a column. **The containment walk does NOT descend into a sanctioned scroll**: that guard
  exists because zone hosts CLIP, and inside a scroll the premise is false — every scrolled band zone
  would otherwise report as an overflow. The scroll's own rect is still checked. Both
  assertions have already caught real regressions (a stepper's default button chrome busting
  `WORK_ROW_HEIGHT`; the band zone standing 5px past a 360px T/B dock); **keep them green.** A THIRD
  per-state assertion guards the shell threshold: **whenever the wide shell is active,
  `work_zone_size().x` must be at least `ZONE_WORK_MIN_WIDTH`** — the invariant the old hand-picked
  `wide_shell_min_width()` violated, and one the zone-bounds assertion structurally cannot catch (a
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
  `_collapsed_bottom`: the fit gate DECLINES the collapsed strip, which is the frame that proves collapse cannot
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
  **`_assert_no_rail_width`** (`_rail_span()` zero — the 25px gutter as well as the column. It used to assert a second half, the separator hairline being hidden, because a `BoxContainer` only skips separation around a HIDDEN child and the visibility was what made the span's zero honest; issue #377 deleted the hairline and moved the rail out of `_card_row`, so the span is now computed geometry and there is nothing left to hide), and `_assert_chrome_home_exact`. **The shell-threshold probes derive their widths
  as `wide_shell_min_width() + the live rail width`** — the panel's own LIVE answer, so the pair
  brackets whichever subject is up (a band, in that block): the reflow fires at those canvases too, so bracketing
  the raw window width against the bare threshold would bracket a test the panel no longer applies.



---

## The action registry is ONE list with THREE mount points, chosen by the panel's own state

Every verb the panel offers is a registration, and **where it renders is a LAYOUT decision the panel
takes, never part of the registration**:

- **A VERTICAL dock (L/R) mounts them on the ACTION BAR** — its own row between the subject row and
  the tab strip. The card's content column reads **title → actions → tabs → content**.
- **A HORIZONTAL dock (T/B) mounts them on the SUBJECT ROW**, between the `◀ n/N ▶` cycler and the
  WINDOW controls, and the bar takes zero height there.
- **A COLLAPSED panel mounts them on the RAIL, in EITHER orientation.** Both of the mounts above go
  with the chrome that carries them when the panel rails, and a rail offering only a glyph and a
  restore toggle would make collapsing an all-or-nothing choice between the map and the band's verbs.
  On the rail they run along whichever axis the rail runs along, so they cost the axis
  `COLLAPSED_SIZE` binds nothing at all — see "The collapsed rail runs along the dock's plentiful
  axis".

Either way row one carries the subject (stage glyph + name + stage label + coordinates), the cycler,
the dock chooser and the collapse caret, in that order and in both orientations — **the cycler does
not move**, and the horizontal mount sits between it and the window controls.

**THE TWO DOCKS HAVE OPPOSITE SCARCE AXES, and that is the whole reason there are two mounts.** A
vertical dock's width is a fixed `PANEL_WIDTH` while its height is the window's; a horizontal dock's
width is the whole monitor while its HEIGHT is what it reserves off the map. So the same row of glyphs
costs the two docks completely different things:

| | vertical (L/R) | horizontal (T/B) |
|---|---|---|
| a verb on the SUBJECT ROW | the card's floor tracks the feature count | free — the row has hundreds of px spare |
| a verb on the BAR | ~44px off a zone whose box is the window's height | 44px of live MAP, on every screen |

**On a vertical dock a row-one action makes the panel's width a function of its CHROME.** That row's
minimum is `subject + every control on it` and the card's minimum is that plus `PANEL_CHROME_H`, so
while the `⚒` sat there the card's floor was **364px against the 380px the dock reserves**, with a
one-button margin left; a second action would have taken it to **402** and forced the card wider than
its own reservation. On the bar the row measures **302** and the card **326**, and a second action
moves neither by a pixel.

**On a horizontal dock the bar is the expensive one, and the subject row has the width to spare.**
Measured on a TOP dock: the row wants **381px of a 770px card interior** with the `⚒` back on it —
**389px spare, ~9 more glyphs** at 40px each before it binds, and more than that on a wide-shell card
(the narrowest wide card is 1190, i.e. a 1164px interior). The strip meanwhile reads its pre-registry
**360 at one band column and 335 at two**: `_horizontal_panel_height()` carries no bar term at all.

**The actions RE-HOME when the dock edge OR the collapse state changes at runtime.** `set_dock` and
`set_collapsed` both call `_refresh_action_mount()` before laying out, and the rebuild is wholesale —
all three rows are cleared and the live one filled — so nothing can be left behind on the mount the
panel just moved off. It is a no-op when the answer is unchanged, which is what keeps an ordinary
layout pass from rebuilding (and so re-`disabled`ing) every button. **The predicate is re-asked by
that rebuild**, so a gated verb renders gated on whichever mount is carrying it; a mount that skipped
it would offer a live button for an act the caller has closed.

**A mount that is not carrying the actions is HIDDEN, not merely empty.** A `BoxContainer` skips its
separation only around a HIDDEN child, so an empty-but-visible row costs its parent a gap it draws
nothing in — the bar would take a slice of the strip on every horizontal dock, the header row a slice
of the very width the seam protects, and the rail a gap between its glyph and its restore toggle.
**The collapse test lives in the MOUNT, not in the visibility pass**: a collapsed panel's live mount
is the rail, so the other two answer false without a second reading of the flag.

**All of it is `band_panel_preview`'s `_assert_action_registry`, PNG-less**, and every mount claim is
asserted as a PAIRING — the glyph on the mount the orientation calls for AND absent from the other,
the bar measuring a row only where it carries them. A one-sided claim passes on a panel that lost the
button entirely.

**The seam is a registration, so the next action is one line and nobody has to think about the panel's
width, its height or which dock it is on** — the reserved-edge registry's shape, and its reason:

```gdscript
register_action(id: StringName, glyph: String, tooltip: String, enabled: Callable = Callable(),
		sprite: Texture2D = null)
unregister_action(id: StringName)
refresh_actions()                      # re-ask every predicate
has_action(id: StringName) -> bool
signal action_invoked(id: StringName)  # THE outbound edge
```

- **`id`** is the stable key the press comes back on and the handle `unregister_action` takes.
  Re-registering a live id REPLACES its descriptor and keeps its place in the row.
- **`enabled`** is a zero-argument `Callable` answering `bool`; an EMPTY one means always enabled. It
  is asked at registration and by `refresh_actions()` — **never during a layout pass**, which is what
  keeps the mount's geometry out of reach of band state.
- **`sprite`** is bundled ART for the face, or `null`. Where given it REPLACES the glyph — a `Button`
  carries art on its own `icon` property, so it is art OR glyph, never both — and the `glyph` becomes
  what renders when the art is absent. It is resolved by the REGISTRANT (the knowledge launcher's
  cairn comes in as `HudSprites.for_mark(…)`), which keeps it a DECLARED input like `glyph` rather
  than a lookup the mount rebuild redoes per button, i.e. inside the descriptor contract and not
  beside it like the pip. An art face is re-padded to `ICON_BUTTON_SPRITE_PADDING` and capped at
  `ICON_BUTTON_ICON_MAX_WIDTH`, derived from each other so its minimum stays `ICON_BUTTON_SIZE` — see
  `knowledge-panel.md` → "The face is bundled art" for why the ghost chrome's label padding and
  `expand_icon` are both wrong on a 24px face.
- **The row is rebuilt wholesale from `_actions`, never patched**, so registration order is the only
  thing that decides the order on screen.
- **No orientation argument, ever.** A caller must not know or care which mount is live; adding one
  would make every call site restate a layout rule the panel already holds in `_action_mount_for_state`.
  The predicates, tooltips and `action_invoked` behave identically at either mount.

**The `⚒` IS REGISTERED THROUGH IT, not special-cased.** `BandCityPanel` makes the same
`register_action(ACTION_CRAFTING, …)` call a caller would, and `crafting_requested` is a RELAY of
`action_invoked(ACTION_CRAFTING)` — kept as its own named edge so `BandPanelController` connects to a
signal that names the act rather than filtering ids it did not register. Registering it in the panel
(rather than from the crafting controller) is what keeps the button's presence a property of the
panel: it is subject-independent chrome and must exist on a band page and the faction page alike.

**AND THE KNOWLEDGE SCREEN'S `▲` IS THE SECOND ONE, which is the claim above made good.**
`ACTION_KNOWLEDGE` + `knowledge_requested`, on the same footing and for the same reason — what your
people know is a FACTION fact, so it must be reachable from a band page and the faction page alike
(`knowledge-panel.md`). Adding it took a descriptor and a relay and **no geometry at all**: the bar
absorbed it on every one of the three mounts, the vertical dock's card floor did not move, and the
horizontal dock's strip did not move by a pixel. Its relay resolves NOTHING, unlike the `⚒`'s — a
discovery unlocks a verb across the whole map and no band owns it, so there is no subject to look up.

> **THE HARNESS'S REGISTRY ASSERTIONS MUST NOT HARD-CODE THE SHIPPED COUNT.** Four of them did — "the
> ⚒ is registered", "a second action puts a second glyph on the bar", "retiring every action", and the
> bar-height message — and all four broke the day the second launcher landed, one of them by
> unregistering the `⚒` alone and then measuring a bar that was still carrying the `▲`.
> `_assert_action_registry` reads `_panel._actions.size()` once and states every claim against it, so a
> third launcher costs that block no edit.

### A PIP IS NOT PART OF THE DESCRIPTOR, and the separation is load-bearing

`set_action_pip(id, count)` / `action_pip(id)`. A registered action can wear a small count badge over
its glyph — today the knowledge screen's unspent count, which is the one number on this header that is
a NUDGE rather than a reading.

**It comes in through its own seam because it moves on a different clock.** `register_action`'s whole
contract is that a descriptor is DECLARED at wiring time and never a function of snapshot state — that
is what keeps the bar's geometry off the render's hot path — while a pip is restated every turn. Three
consequences, each of which was a defect first:

- **The count is retained on `_action_pips`, not on the button.** `_rebuild_action_mount` throws every
  button away whenever the panel re-homes its actions (a dock change, a collapse), so a count living
  only on the node vanishes on a dock flip and comes back on the next turn tick — invisible in any
  frame, which is why `band_panel_preview` asserts it across a `set_dock`.
- **The pill is an ANCHORED, mouse-transparent CHILD of the button, inside its own rect.** A Button is
  not a Container, so such a child contributes nothing to the parent's minimum size — exactly the
  property wanted: a badge that took layout width would make the bar's minimum a function of a
  snapshot count, i.e. the coupling the descriptor rule exists to prevent. `MOUSE_FILTER_IGNORE`, or
  the one thing a player does on seeing a pip would stop working.
- **`0` is "no pip", never a pip reading zero**, and `set_action_pip` is silent on an unregistered id:
  a caller pushing a count before it has registered its action is a wiring order, not an error.

It wears `WARN` on `GROUND` — the tab badge's own `hot` pair, so a pip and a hot tab read as one
family.

**An EMPTY bar costs nothing.** With no registrations the outer `MarginContainer` is hidden, and a
hidden child contributes neither its own height nor the column's separation — measured, the body sits
**flush** under the subject row (gap 0.0px, against 44.0 with the bar up). The bar is a seam, not a
permanent tax on a panel with no actions, and on a horizontal dock — where nothing is ever mounted on
it — that is the state it is always in.

### What the bar costs, and who pays it

One row of `_make_icon_button` glyphs measures **44px** — `ACTION_BAR_MARGIN_V` (4, half the body
gutter) either side of the ghost button's own 36.

**A VERTICAL dock pays it out of the ZONE**, its height being the window's rather than a reserved
strip. That is free at any real window height and is only visible to a probe pinned to a short canvas:
`band_panel_preview`'s `COMPACT_TIER_PROBE_HEIGHT` went **480 → 524** with the bar, and it has to move
again for any row added to or removed from the card's content column, or the probe silently slides
into the tier below and asserts against the wrong one.

**A HORIZONTAL dock pays NOTHING, because there is nothing on the bar there.**
`_horizontal_panel_height()` is the body budget plus the active shell's chrome and no third term, so
the strip reads **360 at one band column and 335 at two** — `_assert_band_columns` compares the RAW
strip against those two consts, which is itself a check on the registry: a bar leaking a row here
fails those two before any mount assertion runs. Against the band zone's ~300px box a 44px row would
have been 15% of a budget whose SHORT tier already reads 299 of 300, so the alternative to a strip
that grows was a clipped flank — and a strip that grows is 44px of live map given up on every screen,
which is what moved the actions to the subject row.

Registration still republishes the reservation, the `set_rail_width` contract, because the mount it
lands on can move the card's chrome and it is a DECLARED input made at wiring time rather than per
snapshot.

## The collapsed rail runs along the dock's plentiful axis

The rail a collapsed panel shows carries three things — the stage glyph, the registered verbs and the
restore toggle — and **the axis it lays them out on is the dock's, not a constant**: STACKED on a
left/right dock, on ONE LINE with the restore toggle justified to the trailing end on a top/bottom
one. `_apply_header_rail_orientation` flips `_header_rail.vertical` (and the rail's own action row
with it) from the same `_is_vertical_edge` the action mounts are chosen by.

**It is the mount question one level down, and it has the same answer for the same reason: THE TWO
ORIENTATIONS HAVE OPPOSITE SCARCE AXES.** A left/right rail is `COLLAPSED_SIZE` wide and a whole
window tall, so stacking spends the plentiful axis; a top/bottom rail is that same 46px TALL and a
whole monitor wide, so the identical stack spends the scarce one. Docked horizontally the strip holds
about one icon square after the card's chrome, which one control fills on its own — so a stacked rail
put the restore toggle past the bottom of the card and, on a bottom dock, off the screen edge: the
one control that gets a collapsed panel back was unreachable. Right-justifying it is also the
expanded header's own arrangement, where the window controls sit at the trailing end of the subject
row.

**`COLLAPSED_SIZE` is a FLOOR on the strip, not the strip.** The card is a `PanelContainer` and a
`Control` is clamped UP to its minimum, so a rail needing more than the strip does not clip — it
grows the card past the reservation, off the screen edge on a bottom dock, taking whatever sits at
the rail's far end with it. `_collapsed_cross_axis_size()` therefore reserves
`max(COLLAPSED_SIZE, rail minimum + the card's content margins)`: 54px on a vertical dock (a 30px
icon square in a 46px strip never did fit) and 56px on a horizontal one. That is CHROME, not content
— the rail holds only the glyph, the registered verbs and the toggle, all of which move on a dock
change, a collapse or a registration, each of which republishes already — so it does not put the
reservation on the render's hot path. **The chrome term is the content margins ALONE, no border**:
the stylebox's explicit content margins ARE its minimum size, so that is exactly what the
`PanelContainer` adds to the rail's own minimum, and `PANEL_CHROME_H` (a declared WIDTH budget that
carries the border) would over-reserve by 2px.

**The orientation is applied BEFORE the anchors, and that ordering is load-bearing**: the collapsed
strip is sized from the rail's minimum, so anchoring first reserves the OTHER orientation's rail —
measured, a bottom dock laid out at 128px, the tall rail's stacked minimum, while reporting the 56 it
had re-measured by the time anything asked. That is the "a size the panel DRAWS but never PUBLISHES"
failure reached from a new direction, and `band_panel_preview` asserts the two agree.

**Verbs grow along the rail's LONG axis, so the rail absorbs more of them without ever touching the
axis `COLLAPSED_SIZE` binds.** Measured with the `⚒` mounted: a collapsed LEFT rail spends 108px of
1128 and a collapsed BOTTOM one 103 of 1550 — room for roughly 31 and 45 more glyphs at
`ICON_BUTTON_SIZE + ACTION_BAR_SEPARATION` each. Past that the rail would clip rather than scroll,
and the answer then is the same one the expanded mounts would need; nothing about the rail is a
scrolling surface.

## THE FACTION PAGE IS A SUBJECT, AND A SUBJECT DECLARES ITS OWN ZONES (issue #450)

> #### ⛔ THE PAGE DECLARES **THREE** ZONES NOW — every "four" below is history
>
> `docs/plan_knowledge_screen.md` §4 deleted the KNOWLEDGE zone: the craft tracks went to the
> knowledge screen (`knowledge-panel.md`), and SETTLING and DISCOVERIES were rehomed into the `band`
> zone — neither is knowledge, neither is earned by practice, neither unlocks a verb. `ZONE_KNOWLEDGE`,
> `ZONE_KNOWLEDGE_WIDTH`, `ZONE_TAB_KNOWLEDGE` and `FactionRollup.build_knowledge_zone` are all gone.
>
> **What that changes in the passages below, and nothing else:** the page declares three zones rather
> than four, so `wide_shell_min_width()` sums two gaps instead of three and its threshold is the
> **1190** a band's three cost rather than 1569; the tab strip reads `Faction · Work · Parties`; the
> KNOWLEDGE zone's own section and its 300px budget row describe a zone that no longer exists; and the
> height tier that dropped DISCOVERIES moved to the `band` zone as
> `HudWorkVocab.FACTION_BAND_FULL_MIN_HEIGHT`, **re-measured at 480** against a 461px block (it is NOT
> the retired 480 arrived at from the old zone's 452 — the coincidence is worth knowing about).
>
> **Everything else in this section stands**, which is why it is banner-corrected rather than rewritten:
> the ordered-list body, the declare-before-build ordering, the per-subject shell threshold, the three
> exceptions a band's subject does not need, and the arithmetic rules are all unchanged by losing a
> column. The roster rows at the top of this file still name the retired zone inside their own single
> table cells; each cell is ONE atomic merge unit, so they are left for whichever change next has
> reason to rewrite one.

The all-band rollup — population, food stores and rates, herds and pens, knowledge, and a
SUMMARY of workers and parties — is a **pinned first entry in the panel's existing cycler**, rendered
through the same shell a band uses. `BandPanelController.render_faction` is its
`render_band`, `FactionRollup` is its zone builders, and `_panel_is_faction` is the one bit of state
that says which of the two every re-entry resolves to.

**It is a subject rather than a panel or a tab strip because the shell already fits it.** The body's
zones ask *who is this*, *what is it doing* and *who is out*, and those are the same
questions one scale up — so the page is a different SUBJECT rendered through the same body, which is
exactly what the cycler selects. A second tab strip would collide with the narrow shell's
zone bar (that bar picks a ZONE; a second one picking a subject makes one control mean
two things), and its own panel would mean a second edge reservation, a second dock preference and a
duplicate copy of the header, dock chooser and collapse toggle — a great deal of machinery for a
read-only summary. Unpinned, its position would drift as bands are founded and lost, so it would have
to be hunted for, which is the opposite of what a standing overview is for.

### THE BODY IS AN ORDERED LIST OF ZONES, AND THE SUBJECT NAMES IT

The panel hosted exactly three zones for as long as a band was the only subject. It hosts a **declared
list** now: `BandCityPanel.set_zone_layout(specs)` takes an ordered array of `{key, label, width}`
descriptors — one wide-shell column and one narrow-shell tab each — and `set_zones(contents)` fills
them by key. A band declares **three** (`BandPanelController.BAND_ZONE_LAYOUT`) and the faction page
declares **three** (`FACTION_ZONE_LAYOUT`) — it declared a fourth, `knowledge`, until that zone was
retired (see the banner above). **The seam is what matters, not the count**: the two lists are free to
differ again the moment a subject has a column the other does not.

- **THE LAYOUT IS DECLARED BEFORE THE ZONES ARE BUILT, and that ordering is load-bearing.** The shell
  threshold is a sum over the declared zones, so arriving from the four-zone page can flip the shell —
  and every builder pages its content against `zone_size()`, which the flip moves. A subject that
  built its zones first and declared its layout second would page a board against the previous
  subject's shell. `set_zone_layout` therefore refreshes the cached box **silently, emitting no
  `zones_resized`** — `set_work_columns`' contract, for the same reason: the caller is the controller
  at the start of a render and is about to build everything against the new box, and emitting would
  re-enter that render from inside itself.
- **The SUBJECT owns the words, the PANEL owns the keys.** `HudWorkVocab.ZONE_TAB_*` holds `Band` ·
  `Work` · `Know` · `Parties` · `Faction`; the panel keeps `ZONE_BAND` / `ZONE_WORK` /
  `ZONE_KNOWLEDGE` / `ZONE_PARTIES`, because a key indexes a persisted tab preference and a badge
  table. The panel's own `DEFAULT_ZONE_LAYOUT` is a BOOTSTRAP carrying keys, widths and **no labels at
  all** — it exists so the geometry is sane before any controller has rendered, and the tab bar is
  never visible until `set_zones` turns `_band_present` on.
- **`set_tab_label` IS DELETED with the mechanism that needed it.** It was a per-zone label OVERRIDE
  whose one use was renaming `Band` to `Faction` on this page; a subject that names its own labels
  makes it redundant, and one fewer way to say the same thing is one fewer way for two surfaces to
  disagree.
- **A content handed in for an UNDECLARED zone is FREED, not dropped.** Ownership passes on the call,
  so ignoring it silently would leak a whole zone's control tree once per render.
- **The persisted tab is validated against `ZONE_KEYS`, not against the live layout.** Prefs load
  before any subject has spoken — what is standing at that moment is the bootstrap
  `DEFAULT_ZONE_LAYOUT` — so a persisted tab must survive a check against a layout no SUBJECT has
  authored yet; `_effective_tab` falls back to the first zone that has content whenever the live
  subject does not declare the selected one.
  **The example used to be `knowledge`, and this diff is what made it wrong**: that key was removed
  from `ZONE_KEYS` with the tab, so a persisted `knowledge` pref is now REJECTED by the very list the
  paragraph is about — the opposite of what it was illustrating.
  ⛔ **AND THERE IS NO REPLACEMENT EXAMPLE, WHICH IS A FACT ABOUT THE GUARD RATHER THAN A GAP IN THE
  PROSE.** `ZONE_KEYS` is down to `band` / `work` / `parties` and `DEFAULT_ZONE_LAYOUT` declares all
  three, so **no currently existing key can demonstrate the rule by absence** — the first draft of this
  correction said the bootstrap "declares no such zone" of `parties`, which is simply false. The rule
  is unchanged and still load-bearing; what it has lost is a case that exercises it, and it gets one
  back the moment a subject declares a zone the bootstrap does not.

**THE PAGE IS READ-ONLY, DELIBERATELY.** The issue's scope is "counts and where they are, not
per-worker controls": role steppers, labor assignment and both compose sheets stay on the per-band
pages, and the cycler is how a player reaches the band a row makes them want to act on. Nothing on the
page emits a signal, which is also what lets its builders be `static` at all.

**THE CYCLER READS ONE HIGHER ON EVERY BAND NOW**, and that is the change's one visible effect on
every surface it did not add: a lone band went from `1 / 1` to `2 / 2`, because the page is entry one.
The `◀`/`▶` are consequently LIVE on a single-band faction, where they used to be dead —
`cycle_band` returns early on `_cycler_count() <= 1`, which one band no longer satisfies. Every
pre-existing `band_panel_*` frame moved in that readout and in nothing else.

### The three exceptions a band's subject does not need

- **CYCLING ONTO IT MOVES NO CAMERA**, and that is a documented exception to decision 2 of
  `docs/plan_band_city_dock.md` ("panel cycling recenters the map on the cycled settlement"). The
  faction has no tile; recentring on whichever band the player happened to leave would move the map
  for a page that says nothing about where it is. **Asserted as a PAIR with the walk back onto a
  band**, which must still recentre — a cycler that had stopped recentring entirely satisfies the
  first claim alone.
- **THE HEADER'S JUMP AFFORDANCE IS OFF** (`BandCityPanel.set_subject_jumpable`). The subject cluster
  stops taking the mouse, drops the pointing-hand cursor and drops the tooltip promising a jump.
  Leaving it live and no-oping `focus_band` would have left a header that offers a jump, lights under
  the pointer and then does nothing — the worst of the three states. `focus_band` guards anyway,
  because `focus_panel_band` is reached BY NAME through `Main`'s silent `has_method` probe and must be
  safe to call in every state.
- **THE NARROW SHELL'S FIRST TAB READS `Faction`**, because `FACTION_ZONE_LAYOUT` says so. The
  tab bar picks a zone, and a zone's name states the scope its content is at — the band's on every
  other page and the faction's on this one. It was a per-zone override (`set_tab_label`) until the
  layout became a subject's to declare; the word is now a field of the same literal that names the
  zone, which is the only place it can drift from.

### What the zones hold, and the one placement worth defending

| zone | holds |
|---|---|
| `band` | the summed PEOPLE bar · the band page's own vitals rows through its own renderer — Food · Kit · Morale · Growth (a fifth, Trade, went with arc #527's retired account) |
| `work` | the whole WORKFORCE bar · one row per band, its work summarised as counts (`2 sources · 1 pen`) |
| `knowledge` | SETTLING (the stage's WORD, keyed, with its meter) · the craft tracks · DISCOVERIES (kinds, headed by the instance count) — **the last of the three only where the box can hold it** |
| `parties` | one row per party — its mission summary and the band it LEFT |

**EACH ACCOUNT IS A STOCK AND A RATE, NEVER AN INLINE LEDGER.** The Food block grew `Income` / `Eaten`
/ `Pen feed` rows for a while and they are gone (the last of the three twice over — the pen's food bill
is itself retired): a band states `Food: 74 (93 turns) · -0.81 /turn` on
ONE line and puts that breakdown behind a disclosure popover, so the rollup had invented a four-row
ledger the per-band surface deliberately does not have. Both figures the page owes are still there —
the stock on the row, the rate on the head — and the breakdown is one cycle away on the band that owns
it. It is also what keeps the zone in its box: the ledger form measured **328px of a 300px box** at the
vitals type size, and the stock-and-rate form measures **241**.

**THERE IS NO FACTION-WIDE FOOD RUNWAY**, the `(93 turns)` a band's own row carries. Turns-of-food is
one larder against one band's drain; averaged, it hides the band that is starving behind the ones that
are not — the reason the dependency figure came off the top bar.

### THE TYPE SCALE IS THE PAGE'S OWN VITALS ROWS

**Every row on this page renders at the size the `band` zone's vitals rows do** — Food, Kit,
Morale, Growth — with heads at `ZONE_HEAD_FONT_SIZE` (10) and key chips at
`COMPOSITION_KEY_FONT_SIZE` (11) above them. One size for all four zones' rows, deliberately: a page
whose zones disagreed about how big a row is reads as two designs sharing a card, and the vitals are
the reference because they are the first thing the page shows.

**IT SHIPPED WRONG THREE TIMES, and the third correction reversed the second.** First the rows were
pinned at 12 and read as a different kind of thing beside the vitals. Correcting that *against the
vitals label* was then itself called an error — that label is a bare `RichTextLabel` at the stock
default (~16) and sits under no head, so it put a 10pt `FOOD` over a 16pt `Larder` — and the rows
were re-pinned to the WORK BOARD's 13 under the rule *"what a surface shares with its model is its
RELATIONSHIP to the thing above it, not its absolute size; match the head→row step, not the row."*

**That rule is retired.** Against the vitals rows two tabs away, 13 simply reads SMALL, and it was
reported as such on sight. The page's reference is not the board — it is the other three zones of the
same page, which the player reaches by clicking a tab. **A small-caps head over larger rows is this
panel's ordinary idiom** (`PEOPLE`, `WORKFORCE` both do it), so a wide head→row step is not the
hierarchy fault the second correction took it for.

**The size is `HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE`, and the vitals label has no override at all**
— it draws at Godot's stock default, so that const is the default written down.
`band_panel_preview._assert_faction_row_size_matches_vitals` reads the LIVE rendered size off the
vitals `RichTextLabel` and requires the two equal, which is the only thing that can see the pair drift
if the engine default ever moves; `_assert_faction_type_scale` beside it pins every head and every row
by EQUALITY against the two named sizes.

**Its first cut was decorative — caught by sabotage, not by review.** That version asserted *"no head
renders LARGER than its rows"*, which the reported bug satisfies: 10 over 13 is a correct
relationship and 10 over 16 was then thought the defect, so the test passed on the very thing it was
written for (it printed `largest head 10, smallest row 16` and went green). Only the constants can
express the rule. Re-sabotaged after the rewrite: it fails naming the stray sizes it found.

`band_panel_preview._assert_faction_type_scale` holds it — a mis-sized Label sits inside its zone rect
and fits its box, so the bounds and content-fits assertions pass on either error, and at the harness's
canvas scale the difference is a couple of pixels, so no frame carries it either. It reads the RENDERED
size (`get_theme_font_size`) rather than the override, so "set no override and inherit the default" is
measured as what it actually draws at.

**IT ASSERTS EQUALITY AGAINST THE TWO NAMED SIZES, and its first cut was decorative — caught by
sabotage, not by review.** That version asserted *"no head renders LARGER than its rows"*, which the
reported bug satisfies: 10 over 13 is the correct relationship and 10 over 16 is the defect, so the
test passed on the very thing it was written for (it printed `largest head 10, smallest row 16` and
went green). The direction was never wrong — the MAGNITUDE was — so an inequality between the two
cannot express the rule and only the constants can. Re-sabotaged after the rewrite: it fails naming
`6 stray: [16, 16, 16, 16, 16, 16]`.

### RETIRED — KNOWLEDGE AS ITS OWN ZONE, AND THE FOUR-ZONE BUDGET

> **The zone this section is about is GONE** (`docs/plan_knowledge_screen.md` §4, and the banner at
> the head of this section). The craft tracks are a free-floating screen now and SETTLING and
> DISCOVERIES are `band`-zone blocks. What is kept below is the REASONING, which outlived the zone:
> why a height tier is a real "can this box hold it" test rather than a round number between two
> docks, why DISCOVERIES is the block that yields, why an unknown box must answer FULL, and the
> `meter_bar` scale trap that bit both meters in opposite directions. The `band` zone's own tier
> (`FACTION_BAND_FULL_MIN_HEIGHT`) was measured the same way and is recorded in `knowledge-panel.md`.


The craft tracks began as the WORK zone's last block, and the placement argument for them there still
stands as far as it went: a track is not a stock and not a population — it is what the faction's hands
may ATTEMPT, and every rung it gates is a row on a work board, which is why they never belonged beside
the stores. What broke it is the other two blocks. **Settling and Discoveries are the same KIND of
fact** — what the faction *is* and what it *knows*, neither of which any band owns — and a work zone
carrying all three would have been the roster of hands with a second page stapled under it, in a box
that clips. So the panel's body became an ordered list and this subject declares a fourth column.

**THE 300px BOX IS THE BINDING CONSTRAINT, AND IT IS WHY THIS ZONE HAS A HEIGHT TIER.** Measured on
`band_panel_faction_wide` (`_report_zone_content_extent`, which prints all four):

| zone | of a 300px box |
|---|---|
| `band` | 200 |
| `work` | 155 |
| `knowledge` | **226** (tiered — see below) |
| `parties` | 54 |

All three blocks at the page's row size measured **336px** with the sites list capped at two, 36 over,
so a horizontal dock's zone **drops DISCOVERIES** and keeps Settling + the craft tracks — the `_build_food_outlook_block`
idiom, gated on `HudWorkVocab.FACTION_KNOWLEDGE_FULL_MIN_HEIGHT` against the box the panel is offering
THIS zone (never the dock edge: a short window and a collapsed box hit the same wall, and an edge test
misses both). Discoveries is the block that yields on two counts — it is the only list here with no
ceiling of its own (Settling is one row, the craft ladder five), and it is permanent geographic
knowledge rather than a call to act, with the top bar's own `◈ Discoveries N` still carrying the
count. What survives is what a player might DO something about.

**An unknown box answers FULL, not compact.** The no-dock host and every frame before the first layout
pass report nothing, and that is not evidence of a small box; silently dropping a block is the drastic
branch and must be positively justified — the asymmetry `_party_compose_floats` takes for the same
reason.

**The tier is what retired the discovery list's own cap.** `FACTION_DISCOVERY_ROWS_MAX` (2) existed to
squeeze three blocks into 300px; with the block gated on height instead, the list takes the page's
shared `FACTION_LIST_ROWS_MAX` (6) and the side dock — where a player actually reads this page — shows
six kinds and a `+N more`. One cap for all three of this page's lists.

**The threshold is a real "can this box hold it?" test rather than a round number between two docks.**
At that cap the full block measures **452px of the side dock's 1057** (`band_panel_faction_knowledge`
prints it), so `FACTION_KNOWLEDGE_FULL_MIN_HEIGHT` at 480 leaves a box that only just clears the gate
28px of room — and that margin, not the gap between ~300 and ~1055, is what a fifth block or a sixth
craft track eats into.

**Re-measure before adding a row anywhere on this page.** The band zone has been over its box once
already in this arc's own history, this zone has now been over it twice, and every row here is
unconditional — so those figures are the zones' heights rather than their best cases. The harness
fixture stages all three blocks at their worst case (five tracks, one finished; MORE discovered kinds
than the list shows, so the `+N more` row is in the measurement), because each block OMITS ITSELF when
its data is absent and an unseeded fixture measures a zone with two of its three blocks missing.

**The tier is asserted as a PAIR and neither half is a claim alone.** `band_panel_faction_wide`
requires DISCOVERIES gone AND the other two present (without the second, a zone that rendered nothing
passes); `band_panel_faction_knowledge`'s 1057px side dock requires it there. `_assert_zone_content_fits`
can see neither direction — a dropped block leaves a box that fits trivially, and a clipped one still
reports a rect inside its host.

**`HudFormat.meter_bar` GRADES A 0–100 SCORE, NOT A FRACTION, and both meters on this page got it
wrong in opposite directions.** A track's progress is `0..1`, so the knowledge block's bare `progress`
filled zero cells at every value under 0.5 — every meter shipped EMPTY, indistinguishable from an
unstarted track beside a live `62%` — while sedentarization arrives already on the 0–100 scale and must
NOT be divided by it. `FactionReadouts._knowledge_meter_text` scales the same way for the same reason.
The assertion that can see it tests for a FILLED cell: `ends_with("%")` passes on an empty bar.

**THE PARTIES ROW NAMES THE HOME BAND, not the party's tile.** A party's own coordinates change every
turn and mean nothing without the map; the band it left is what the player cycles to in order to act
on it, and it is the "where they are" half of the ask a one-line summary row can honestly carry.

**AND THE NAME JUMPS THERE, which is why `_summary_row` takes TWO owners.** It bound ONE entity to both
of the row's acts for a while, so a link reading `Band 2` selected the expedition — the row named one
subject and delivered another, against its own docstring and against this section. The **toggle** keys
on the PARTY (that is what the row's detail is about, and what `_faction_open_row` matches) and the
**jump** on the HOME BAND (`jump_owner`, wired to `jump_to_band_entity`); the WORK tab passes the same
entity twice, a row about a band jumping to that band. `_jump_to_party_entity` went with the bug — the
page offers no way to reach a party directly, deliberately, there being nothing to DO to one from here.
**The claim is a PAIR** (`band_panel_preview._assert_faction_party_row_jumps_home`): binding the party
routes the press through `jump_to_band_entity`, which cannot resolve a party in the band roster and
**NO-OPS**, so the panel keeps the subject it already had and a subject-only assertion reads the right
entity for entirely the wrong reason. It asserts the page is LEFT and then that the subject is the home
band.

### The three arithmetic rules

- **THE PEOPLE BRACKETS ARE WHOLE PEOPLE, SO THE ROLLUP IS A PLAIN SUM OF INTEGERS** and the faction
  total is the sum of the bands' own `size`. It used to sum the raw fixed-point brackets and
  apportion once at the end, because rounding each band first and adding the results lost a person
  per `.5` remainder; that whole problem left with the fractions. The harness's roster is
  deliberately two bands of DIFFERENT size (30 and 12), so a page that had stopped summing renders a
  number distinguishable from either band's own.
- **THE NET IS SUMMED FROM EACH BAND'S OWN `band_net_food`**, never recomposed from the three totals
  above it. `raid_forfeit` is a fourth, EPISODIC term of that identity and belongs in the net without
  earning a standing row, so a recomposed net would quietly disagree with the band pages.
- **BOTH LISTS ARE CAPPED, AND THE CAP IS STATED.** The zones clip and neither list pages (the work
  board's pager belongs to a band's own sources), so a bounded list ends in `+N more`. A truncated
  list with nothing under it reads as the whole roster — the one way a rollup can lie about a total
  it is printing directly above.

### Frames

`band_panel_faction` (the tall LEFT dock, band tab) · `band_panel_faction_work` (the work tab, which
the narrow shell's one-zone body cannot show in the frame above) · **`band_panel_faction_knowledge`**
(the KNOW tab — the fourth zone, which exists on this subject and on no other, so the tab is reachable
at all only because the faction subject DECLARED it) · `band_panel_faction_wide` (the
bottom dock, all four zones abreast — the only layout the page reads as a whole in). The fixture is
**two resident bands and one party** (`_faction_roster`), and two bands is the whole point: on a
one-band faction every total is that band's own, so a page that had stopped summing would render
identically and every assertion would pass.

**THE SECOND BAND KEEPS A CORRALLED HERD AND PAYS ITS FEED**, and the FEED is the whole reason now.
The pen's upkeep renders no row of its own, but it is a real term of `band_net_food` — so without it
the headline net on this page is the one figure a live pen-keeping faction never sees, and the work
zone's per-band row never says `· 1 pen`. It is also what caught the zone at 328px of its 300px box
while the Food block was still a ledger. (Its ORIGINAL justification was a `band`-zone **Herds** block
reading `0 / 0` on an all-wild roster; that block was replaced by the five vitals rows later on this
same branch, so the fixture outlived the reason it was added for.)

**EVERY STAT ROW IS ASSERTED TO RENDER ITS KEY**, and that is not belt-and-braces: `clip_text` drops a
Label's minimum width to nothing usable, and the spacer beside it is the row's only expanding child, so
a clipped key is squeezed away and the block renders as a column of right-aligned numbers with no
names. It shipped that way once. Both geometric assertions pass on it comfortably — a one-pixel Label
is inside its zone and fits its box — so `_faction_keyless_rows` measures the laid-out WIDTH rather
than the `text`, which is set correctly in the broken build too.

**IT IS MEASURED AGAINST THE TEXT'S OWN FONT WIDTH, NOT AGAINST ZERO — and the helper was written the
other way and was DEAD, so nothing caught that.** `clip_text` does not zero the minimum; **Godot floors
it at ONE PIXEL**, so the original `size.x <= 0.0` scan reports a fully clipped column as perfectly
healthy. That was demonstrated rather than reasoned: `_faction_keyless_rows` was referenced only by its
own recursion (no assertion ever called it), and when it was first wired up the `clip_text` sabotage
printed `0 keyless` and went green. A key renders iff the row granted it at least the width its own
font measures for its own string, less `KEYLESS_KEY_WIDTH_TOLERANCE` for the sub-pixel disagreement
between a container's rounded layout and the text server's float; re-sabotaged after the rewrite, it
fails naming `12 keyless`.

**THE SCAN RUNS ON `_assert_faction_knowledge_zone`, NOT ON `_assert_faction_page`, and the shell is
why.** The narrow shell parents ONLY the active tab's zone (`_reparent_zones` DETACHES the rest), so a
zone read from another tab has never been laid out and every row in it measures zero — asked from the
`band` tab, the scan would report every knowledge row keyless. The KNOW tab's state is also where all
three of that zone's blocks render, so it lays out the widest set of stat rows the page ever has at
once; the `band` zone has no `_stat_row`s at all (a vitals `RichTextLabel` and the PEOPLE bar), so
there is nothing for the scan to find there.

`_assert_faction_page` carries the rendered claims (the PEOPLE total against a figure composed from
the fixtures' own floats, the five vitals rows' keys, the header's three slots, the cycler's
`1 / N+1`, the dead jump affordance driven through the REAL press handler, the WHOLE declared tab strip
by equality, the party row's home band);
`_assert_faction_cycler` carries the routing ones, which no frame can hold — the pinned entry is
REACHED through `Hud.cycle_panel_band` rather than by calling `render_faction`, the camera pair above,
and that a snapshot leaves the page up rather than handing the panel back to a band under the player.
Its last act returns the panel to a band, without which every state below it would re-render as the
rollup on its next `_push_bands`.

**THE FOUR-ZONE BODY'S OWN THREE ASSERTIONS**, none of which any frame here can hold:

- **THE SETTLING ROW IS KEYED BY A PLAYER WORD, NEVER BY THE SIM'S WIRE TOKEN.**
  `SedentarizationStage::as_str()` spells the three stages `none` / `soft` / `hard` and
  `FactionReadouts.SEDENTARIZATION_STAGE_NONE` says outright that they are "not a word anyone sees" —
  but the row passed the token straight through as its key, so the page rendered
  `soft  ▰▰▰▱▱  62/100`, a lowercase enum beside the capitalised `Nomadic` the below-threshold case
  already had. `HudWorkVocab.FACTION_SETTLING_STAGE_LABELS` is the one home for those words (`none` →
  Nomadic, `soft` → Seasonal base, `hard` → Ready to settle, taken from the sim's own prompt wording,
  **provisional pending playtest** like `ZONE_TAB_KNOWLEDGE`'s abbreviation), an absent or unrecognised
  stage falls through it to `Nomadic` rather than leaking the token, and `FACTION_SETTLING_NOMADIC`
  reads the table's own `none` row so that word has one home. The harness asserts the LABEL by
  equality; asserting the raw token, which it did, is what locked the bug in.
- **`_assert_faction_knowledge_zone`** — each of the three blocks through the thing only IT can say
  (the SETTLING head's readout naming both the stage and the score, a FINISHED track reading `known`
  beside a still-climbing one reading a FILLED meter and a percent, the DISCOVERIES head counting
  INSTANCES where its rows count KINDS, the twice-found kind reading `2`, and the `+N more` row the
  cap owes). **Each block omits itself when its data is absent**, so a zone that had lost two of the
  three passes both geometric assertions comfortably — an empty box fits anything.
- **`_assert_faction_zone_layout`** — asked of the wide shell's own HOSTS, never of `_zone_layout`,
  because the layout is the INPUT: a `set_zone_layout` that accepted four specs and built three
  columns satisfies every assertion made against the array and none made against the tree.
- **`_assert_faction_shell_threshold`** — the FOUR-zone derivation by EQUALITY against the panel's own
  named widths with an explicit gap count of three, then the flip bracketed one pixel apart on the
  page itself. **A threshold left at the three-zone value is invisible in every rendered frame here**:
  the faction states sit on windows that clear 1569 comfortably, so a page flipping wide 379px too
  early draws a perfectly plausible board. Only a window BETWEEN the two thresholds can tell them
  apart, and the equality claim is what catches the gap-count term the bracket cannot — under a
  two-gap sabotage the bracket derives its own probe widths from the same wrong number and stays
  green.

## The parties strip's SEVEN lines, and the two things that paid for them

The parties inspector strip IS the detail panel for a launched party, and on a horizontal dock it lives
in a `clip_contents` zone of ~300px that also owes a head, at least one party row and a bottom-pinned
footer. Its whole budget is therefore what `BandDetailLines.expedition_summary_lines` can light up at
once, and for a long time nobody had counted: the strip overran that box by **10px** on the ONE fixture
that opened it (`band_panel_parties_inspector_wide`, reported twice per run — once by the recursive
bounds assertion, once by `_assert_zone_content_fits`) and was the harness's last standing error.

**THE FIXTURE WAS NOT THE WORST CASE, and that is the part that mattered.** That party carries no fill
target, no carry cap and no trip bound. A hunt party carrying every optional line at once needs
**SEVEN**:

| line | its gate |
|---|---|
| `Mission` | unconditional |
| `Target` + the target's live `(x, y)` | `is_raid`, a non-empty `expedition_target_herd`, the herd still in telemetry |
| `Orders` | `is_hunt` |
| `Phase` | a non-empty `expedition_phase` |
| `Carried` (`N / cap`, the PACK's material clause, then the `· FULL` badge) | `is_raid`, and a carry cap that is > 0 and met |
| `Next delivery` (`↻` for a recurring party) | `is_hunt` + `has("expedition_projected_delivery")` |
| the trip-bound clause | a non-empty `expedition_trip_bound` |

That party measured **328px of the 300px box**. A DENIAL party is strictly shorter (five lines), and the
quoted-party note a between-rungs party earns rides the `Collapse:` ROW as a clause rather than as a
line — which is this budget's rule already being followed.

**THE PACK'S MATERIALS FOLLOWED THAT RULE RATHER THAN BECOMING AN EIGHTH LINE** (arc #527 follow-up).
`PopulationCohortState.materialBatches` is resolved with no resident-band gate, so a party in flight
has carried them the whole trip and nothing rendered them — a scout hauled a wolf home and the UI
never mentioned the hide. What the party is carrying home IS the `Carried:` sentence, so the clause
rides that row: `Carried: 18 / 18 (5 turns) · 4.5 hide · 1.2 hide · FULL`. **A scout takes the same
clause on its `Provisions:` row** — a scouting party that walks over a kill banks materials exactly
as a raid does, and one spelling is what stops the two hosts wording the pack differently.

**ONE TERM PER BATCH, NEVER MERGED BY MATERIAL.** A batch is one pile of one material AT ONE RATING,
so two piles of `hide` at different readings are two terms; summing them rebuilds the retired trade
scalar out of the very vector that replaced it. The per-axis readings stay the Crafting panel's
register — this row answers *what is coming home and how much*, in a box that cannot afford a
characteristic vector per pile. `band_panel_worst_case_party` carries the two-pile case and asserts
that their SUM does not appear; the strip still measures inside its box, because the clause added no
line.

**`Position` IS IN THE PRODUCER AND CANNOT REACH THIS HOST.** It renders off `pos`, the MAP MARKER's
own stamp; the parties zone reads the raw cohort dicts `update_band_alerts` pushes, and the decoder
emits `current_x`/`current_y` and no `pos` at all. Staging one in the worst case would inflate this
zone's requirement with a row it can never be handed, and whatever was cut to pay for it would be cut
for nothing. The row is live in the Occupants drawer, which is reached through the marker.

**Two changes closed the 28px, in the order this panel's own rules put them, and NEITHER was enough
alone:**

1. **`PARTIES_INSPECTOR_LINE_SEPARATION` 4 → 2.** Padding is the cheapest fix available — nothing is
   lost, only density — and the strip already carries a dedicated constant for exactly this. Nine gaps
   at the worst case, so it pays 18px. It could not pay 28: at 0 the lines touch.
2. **The two ORDERS lines merged into one** (`DetailFormat.expedition_orders_line`) —
   `Orders: 30% left standing · fills 12 Roe Deer`, where `Leaves standing:` and `Fill target:` used to
   be two rows. This is the band zone's SHORT-tier idiom (Morale + Growth, the Food row's fodder clause)
   and it is what that tier chooses over dropping a line: nothing is lost, and two facts that read as
   one sentence cost one row. The producer's own docstring already called them one sentence.

**Neither cut a line, which is the ordering the fix was required to follow** — the strip is the thing
the row above it exists to open, so `Mission:` and `Phase:` (both restated as glyphs by the strip's own
header) stayed the last resort and were not reached.

**The merge is UNCONDITIONAL, unlike the band zone's.** The Occupants drawer has room, but it has no
reason to spend two rows on one sentence, and one spelling is what stops the two hosts wording the
orders differently. It also removed a wart: the old row read `Leaves standing: 30% left standing`.

**Measured after: the worst case reads 294px of the 300px box**, and the frame that pins it is
`band_panel_worst_case_party` — which REPORTS its extent beside asserting the fit, the
`band_panel_vitals_worst_case` rule, because this zone has now been at the edge twice. It also asserts
the strip really renders all seven lines: a strip that quietly stopped emitting one is SHORTER, so it
fits, and every assertion would go green on a state that had stopped measuring what it exists to
measure.

**ONE party row, deliberately.** A second costs the zone another 48px for a structural reason that has
nothing to do with the strip's own height, and mixing the two would leave the reported number
unattributable.

**A SECOND PARTY STILL OVERRUNS THE BOX, and that is measured rather than reasoned: 342px of 300 with
two parties out and the strip open.** It is a different problem with a different answer — the row LIST
needs paging, the way the work board pages against `work_zone_size()` — and no amount of line-budget
work in the strip reaches it. Nothing in this arc addressed it.

## The wide shell's flanks are never narrower than the narrow shell's zone (issue #374)

`ZONE_BAND_WIDTH` is **380** and `ZONE_PARTY_WIDTH` is **`PANEL_WIDTH − PANEL_CHROME_H` = 354**. Both
were 300, and the defect that fixed is the one that reads as absurd once stated: the NARROW shell
hands its single zone the panel's whole strip less chrome — 354 — so **the layout with a whole screen
to spend was giving the same rows LESS width than the layout squeezed into a side dock**. The band
zone CLIPS rather than scrolls, so the missing width came straight off its vitals rows as wraps.

- **The parties zone takes exactly the narrow shell's zone width, and that is the floor the rule
  states**: no wide-shell zone may be narrower than the side dock's. Its compose picker measures 234px
  of that width with three rungs abreast, and its floor chart 300px of it, so it is the width both
  controls are tuned against.
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

`wide_shell_min_width()` follows by derivation to **1190** on a band's three zones, and the flip point with the rail's span
added lands at ~1511px of window (Standard) / ~1523 (Large) — see the chrome-rail bullet above.
`PANEL_CHROME_H` moved UP the const block to sit beside the margins it sums, because
`ZONE_PARTY_WIDTH` now references it and a GDScript `const` may not read one declared below it.

## `PANEL_HEIGHT_WIDE` is the BODY's budget, not the strip's height

The 360 is the box the zones' content is tuned against — `_report_zone_content_extent` reads the band
zone at **299 of a 300px box** on a wide horizontal dock, i.e. one pixel of slack. It used to be spent
as a flat strip height, so the **narrow** shell paid for its own tab bar out of that same 360: a
**265px box for content that needs 273**, which the `clip_contents` zone host then cut with no
scrollbar and no affordance.

`_horizontal_panel_height()` adds `_shell_chrome_height()` — the tab bar in the narrow shell, zero in
the wide one, whose separators are vertical hairlines paid out of the width — before the
`MAX_WIDE_HEIGHT_FRACTION` clamp. **Both shells now hand their zones the identical box, which is the
only arrangement under which one set of tier thresholds can be right for both.**

**On a BOTTOM dock the cut landed at the window's own edge**, which is why the defect read as the
panel "running off the bottom of the screen" rather than as a clipped zone — the Scout and Warrior
cards simply ended mid-stepper. It surfaced at `ui_scale` 1.35 (issue #490 shrinks the logical
viewport, so a bottom dock drops below `wide_shell_min_width()` on a normal monitor) but **it is not a
scale defect**: the narrow shell is reached at 1.0 in any window under ~1511px and lost the same 35px
there. `band_panel_shell_below_threshold` is the 1.0 frame that moved, and it GAINED its work board's
pager row.

**The split of `_panel_width_extent()` out of `_panel_extent()` is what keeps this acyclic**, not
tidiness. The cross axis now depends on which shell is active, and the shell is chosen from the width;
a shell test that built the whole extent would call the height, which calls the shell test. Every
width reader takes `_panel_width_extent()`; only `_panel_extent()` pairs it with the height.
`_shell_chrome_height()` reads through `_shell_is_wide()` rather than the cached `_body_is_wide`, so a
dock layout arriving before `_relayout_body` has re-chosen the shell still sizes the strip for the
shell it is about to get.

**The fix is a shared bound, NOT a `ScrollContainer`** — for the fixed CROSS-AXIS size, which is what
that rule was ever about. The no-scroll rule has since been **narrowed twice rather than deleted**:
the parties LIST scrolls, and since the interface-scale arc the BAND zone does too.
`_assert_scroll_only_where_sanctioned` keeps the rule everywhere else.

**The band zone's exception overturns an argument stated here, and the reason is worth keeping.** The
old rule reasoned that a scrolling band zone would hide vitals behind a gesture on the one surface a
player reads every turn — which is true, and was still the wrong call, because it compared scrolling
against a fantasy. The actual alternative was not "everything visible without a gesture": it was the
SHORT tier **silently deleting** the food-outlook chart and the role cards' hint text. Measured, a
one-column horizontal flank packs vitals + PEOPLE + WORKFORCE into 299px of a 300px box, so at 1920
there was no room for the chart and it simply was not built. **Content the player cannot reach at all
is strictly worse than content behind a scrollbar**, and that is the product rule: a tier may change
DENSITY, never delete a block.

**What it costs, stated honestly.** The full stack is 353–450px in that 300px box, so a third to a
half of the band zone is under the fold on a one-column horizontal dock, and what falls below it is
the Scout/Warrior **steppers** — controls, not readouts. They used to be on screen precisely because
the tier had deleted the chart and the hints to pay for them. Nothing is lost and the scrollbar is
visible, but if "the role steppers must be reachable without a gesture on a bottom dock" is also a
requirement, the lever is a cheaper density cut at SHORT (e.g. the role-card hint from two lines to
one) or a block ORDER that puts controls above the fold — **not** a return to deleting the chart.

The vertical docks are unaffected: the narrow shell gives the zone the full card height, so the
scroll never engages there, and the shipped default edge is LEFT.

## A horizontal dock grows in WIDTH; a vertical one grows in HEIGHT

The panel reserves its **cross** axis — width on a vertical dock, height on a horizontal one. Growing
along the axis you reserve re-emits `reservation_changed` → `MapView.set_reserved_inset` → cache
invalidation, i.e. the map flicker the fixed cross-axis size exists to prevent. Growing along the
**long** axis costs nothing and nobody downstream needs telling. So the UX rule and the architectural
invariant are the same rule, and it is the reason width growth is safe to build on and height growth
is not.

A vertical dock already obeyed it — 380 wide, content flowing down. A horizontal dock did not: the
card is content-width, but its *content* declared a fixed 380 band flank of stacked blocks, so it
never asked for the width and paid in height it did not have. On a wide monitor the card used well
under half the window while the strip stayed a full 360.

**`band_zone_columns()` is PURELY GEOMETRIC** — `(available span − chrome − parties − one work column
− separators) / ZONE_BAND_WIDTH`, clamped to `BAND_ZONE_MAX_COLUMNS` (2). Not one term is content, and
that is load-bearing: a count that varied with what the band holds would make the strip height
content-dependent and reopen the flicker bug. It is the `set_work_columns` / `_affordable_work_columns`
idiom one flank over, and `_zone_span(spec)` — the zone's base width times the columns
`zone_columns()` GRANTED it — is the single definition of a flank's realized width. `_card_width()`,
`work_zone_size()` and `_affordable_work_columns()` all reach it through `_fixed_zone_span()`, and
`_zone_fixed_width(zone)` is the by-name reader.

**`wide_shell_min_width()` is the one place that must NOT read it.** It sums `_spec_width` — the
base, one column each — because the threshold asks what the shell needs to be worth choosing, and a
threshold that read the granted count would depend on a width the granted count is derived from.
That is the cycle; see the banner on `wide_shell_min_width()` itself.

**The two-column flank costs the work board one column.** That is the trade, and it is deliberate: the
work zone is already constrained and pages itself, while the band flank had no way to spend width at
all.

A TOP dock stays at one column — the lateral bounds cost it 704px — which is the geometric rule
working, not an exception.

## A BOTTOM dock yields the HUD's strip only when the card cannot afford to keep it

`Hud.set_reserved_inset` insets `LayoutRoot` on **all four edges**, so a bottom reservation shortens
`ContentRow` across the entire window — and the left/right dock columns live there. The tile card
therefore lost the panel's full 360px of height even in the region the band card never occupies. On a
bottom dock `DockRowController._park` has already emptied `BottomBar` (`visible = false`, minimum
`(0,0)`, one zero-size spacer), so that inset buys nothing and costs the columns everything.

**A naive exemption is WRONG, and was tried and reverted.** The inset silently does a second job: the
card only stays clear of the lateral columns *because those columns are shortened*. At 1920 a 1190px
card centred in its strip starts at x≈204, inside the 360px left column. Taking the exemption forces
the bounds to apply, costing 704px on top of the rail's 321 — the span falls 1599 → 895, under
`wide_shell_min_width()`, and the bottom dock collapses to the narrow tabbed shell (326px at `ui_scale`
1.35). That is a worse trade than the bug.

So the rule is conditional: **the HUD yields iff the card could NOT afford the wide shell with the
bounds applied.** `Main.band_dock_overlays_hud(edge, size, hud, panel)` is its ONE home — deliberately
`static` and node-free so the harnesses can call it instead of restating it. Two clauses: the bottom
bar must have left the strip, and `BandCityPanel.affords_wide_shell_with_bounds()` must hold. The fork
sits at a logical width of **1871** for a BAND page — `wide_shell_min_width() 1190 + rail 321 +
leading ceiling 360 + trailing 0`, confirmed by sweep (*yields* at 1870, *keeps* at 1871).

**THE FORK IS PER-SUBJECT, because the threshold is.** `wide_shell_min_width()` sums the LIVE zone
list, so the four-zone faction page sets its own — ~2250 off a 1569 threshold. The predicate must ask
the function and never a constant: a predicate that assumed three zones while a four-zone page laid
out is the same predicate-versus-consumer mismatch that shipped the 2215–2280 band.

**A variable-width zone lives in the spec, not beside it.** `ZONE_SPEC_WIDTH` is the **base** (one
column) and the optional `ZONE_SPEC_MAX_COLUMNS` says how many it may take; `_spec_width` is the base,
`_zone_span(spec) = base × zone_columns(key)` is the extent, and `_fixed_zone_span` and the host
minimums read the extent. **`wide_shell_min_width()` must keep summing the BASE or it cycles** — the
grant reads `_available_card_span()`, which is tested against the threshold. The cap is the SUBJECT's
to declare because the split is authored: `BAND_ZONE_LAYOUT` declares 2 because
`BandPanelController.build_band_zone` authors a two-way split, and `FACTION_ZONE_LAYOUT` declares
nothing because `FactionRollup.build_band_zone` authors none — a 760px host with a one-column builder
in it is the emptiness the widening exists to remove.

| | 1600 | 1920 | 2560 | 3440 |
|---|---|---|---|---|
| HUD | yields | **keeps** | keeps | keeps |
| column bottom | stops at the strip | **1080 of 1080** | full height | full height |
| band flank | 2 cols | **1 col** | 2 cols | 2 cols |

**A BOTTOM dock charges NO trailing bound** (`_trailing_bound_for`, the one definition, read by both
`_available_card_span()` and the predicate so the layout and the verdict cannot charge different
columns). The right column cannot reach a bottom strip in either branch — below the fork the HUD
yields and `layout_root` is inset wholesale, above it the right dock is clearanced. **The LEADING
bound stays**, because the left column deliberately does run to the window bottom. **A TOP dock
charges both**: the HUD is exempt there (issue #377) and the top-right readouts genuinely share the
strip's row.

That asymmetry is also what fixed the card's centring — residual **0px**, down from 419. No placement
code changed; the card was off-centre *because* `_available_card_span()` was not the true gap.

**1920 pays for its full-height tile column with band-zone density.** Keeping the strip there costs
the card the leading 360, the flank drops 2 columns → 1, and the zone falls from TALL to SHORT: no
food-outlook chart, no role-card hint text, Fodder merged away. It is the documented flank trade
extended down, not a new failure mode — but it is a visible content loss at the commonest resolution,
and **the lever for it is the fork, not the centring**. Above 2432 the change gives back instead:
2432–2560 returns to two columns and 3440's work board goes 3 → 4.

**The predicate reads a column CEILING, never the live width and never the reservation — and the
distinction between those three is the whole of this rule.** They answer different questions:
`lateral_column_widths()` is `max(authored, live)`, a **layout instruction** the card obeys; the
ceiling is *the widest the column can ever get*, which is what a rule may reason **forward** from.
Reading the live width makes the predicate depend on its own output, because a column's live width can
move when its height changes and its height is what the inset decides.

**Reading the RESERVATION instead was the first attempt and it shipped a real defect.** The predicate
took the authored 344 while the card laid out against the live 419, so between **2215 and 2280** the
two disagreed: the HUD kept its strip *and* the card collapsed to the tabbed shell — precisely the
trade this rule exists to refuse. It is stable and reproducible, not a race; the harness only stumbled
into it because a mid-flight resize swept the band. Sabotage with `RIGHT_COLUMN_CEILING := 0.0`
reproduces it deterministically and names the widths.

**`RIGHT_COLUMN_CEILING` is 352, and it is DERIVED FROM THE SCENE rather than measured off a render.**
It sums three named authored terms — `RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH` (320, `TellingPanel`'s own
minimum, the widest card in `RightStack`) + `RIGHT_DOCK_SCROLLBAR_SPAN` (8) +
`RIGHT_DOCK_MARGIN_SPAN` (24) — which is exactly the 344 the column reserves empty, plus the vertical
scrollbar that appears when the stack overflows.

**The ceiling still lives where the bound is CONSUMED, not in the scene**, and that is a separate
point from where it is derived. Authoring `TurnBlock.custom_minimum_size.x` was tried and measured:
`TopBar` packed that block against the right edge behind an expanding spacer with left-aligned
labels, so a node minimum pinned the column's LEFT edge and floated the readouts 142px clear of the
screen — it moved all 80 frames and cost four top-dock states their wide shell. A bound and a layout
instruction are not interchangeable.

**It was 561 until the interface-scale arc, and that number had gone stale.** Issue #450 retired the
top-bar knowledge strip it was measured against (`⚒ Your people know:` plus two in-progress tracks;
the runners-up were metrics 384, Sedentarization 346, demographics 260, turn 78), leaving the right
column as the right dock alone. It survived the merge deliberately — a ceiling that is too large only
makes the predicate conservative, and re-deriving it from an incomplete sweep mid-merge would have
been worse — and was then re-derived properly.

**WHICH DIRECTION IS DANGEROUS IS NOT SYMMETRIC.** Too high costs layout quality: the predicate
refuses the wide shell in windows where the card would fit. Too low is a correctness bug: a readout
wider than the bound is **overdrawn by the card**. So this comes down to the true maximum and no
further, and it carries **no safety margin on purpose** — a margin cannot bound a string, and an
unexplained pad is precisely what 561 became.

**The sweep is evidence, not proof, and the probes are what close the gap.** Both harnesses read
`344.0` in every one of their states (84/84 and 274/274, across four viewports and at `ui_scale`
1.35 — logical widths are scale-invariant), a distribution with no tail at all; 352 appears only in
the deliberately-staged worst case. Fixtures bound fixtures, so the content paths were probed
individually instead of trusted: legend row labels look unbounded but are not (`LegendScroll` leaves
horizontal scroll `AUTO`, so row width never reaches the card — 11 rows of 75-char names moved
nothing), and Victory's `RichTextLabel` has no `fit_content`. **The one genuinely unbounded path is a
card TITLE**: `PanelCard._header` is `fit_content` + `AUTOWRAP_OFF`, so a title's unwrapped width is a
hard card minimum — a 58-char legend title takes the column to 509. Sweeping every title the client
can actually author (the four MapView legend titles and all thirteen overlay-channel labels from the
native decoder, longest `Forage (Human Food Capacity)`) leaves the legend card at 253, 67px under
Telling's 320 and contributing nothing.

**The headroom is asserted, not reported**: `_assert_ceilings_cover_the_widest_right_column` stages
the widest dock content and checks the ceiling still covers the column, now reading `352 / 352`. That
guard was **vacuous before** — it passed with 209px of untested slack — and is load-bearing for the
first time. To raise the ceiling, raise `RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH`; the guard is what tells
you when. What it still cannot see is a title long enough to push a card past 320; closing that means
bounding the title in `PanelCard._header`, not enlarging this.

`left_column_ceiling()` needs no constant of its own, and that is **measured, not assumed**: with a
~400-character terrain label in the tile card the left column is still exactly 360 (region 360, stack
336 inside it), because `LeftScroll` is a `ScrollContainer` with horizontal scrolling live, so its
minimum width does not include its child and **no card can widen the column**. The right column needed
`RIGHT_COLUMN_CEILING` because it holds a SECOND region, `TurnBlock`, which is in no scroll container
and renders 419 against a 344 authored minimum. There is no such second region on the left. With the
trailing charge gone the slack is now **zero** — the card gets exactly 1190 at the fork — so what holds
it is `_assert_ceilings_cover_the_columns` (reading `360 / 360`) and the promise walk, not headroom.
`left_column_ceiling()` returns the left dock's reservation unchanged — that column measures its
authored 360 live — and exists as a named pair so a future left-column overrun has a home rather than
a call site to rewrite. **The event dock still bounds off `right_column_width()`**, deliberately: it
is placing itself, not reasoning forward about someone else's placement.

`DockRowController.parks_for` / `rail_width_for` are **pure functions, not state reads**, because
`Main` is the FIRST listener on `reservation_changed` and the reflow the second — `_is_reflowed` would
answer for the previous reservation.

### The chrome rail stays FLUSH to the screen; the right dock is what clears it

The rail was briefly pinned at `offset_left = -(_bound_trailing + rail_width)` to keep the minimap and
turn orb out of the right column once the HUD stopped yielding. **That was the wrong side of the
problem** — it pushed the chrome 419px inboard and left a band of bare map between it and the screen
edge, which is a visible regression against how the corner had always looked.

The measurement that decided it: on a 2560 bottom dock with the strip top at `y=720`, the right dock's
**content** reaches 302 with nothing in it, 574 at a full Telling page, **718 with the Victory card**
(2px of clearance) and **1151 with an 11-row Terrain Types legend** — 431px *inside* the strip. Both of
those were one keypress away (`V`, `L`), so the region-versus-content distinction did not rescue it: a
flush rail really would sit under the minimap.

**THE LEGEND CARD HAS SINCE BEEN RETIRED, AND THE CLEARANCE STAYS** (`overlay-channels.md`). The
tallest the right dock can now reach is the Telling page plus Victory — 718 of that same 720, i.e.
**two pixels** — so the clearance is holding by a margin no future card can be assumed to leave. What
changed with it is what a HARNESS can stage: `band_panel_preview`'s negative control moved off the
CONTENT and onto the CLIP BOX, which is sized by the dock and the clearance rather than by what is in
it, and is therefore as sharp with an empty dock as with a full one (`harness-band-panel.md`).

So the clearance moved to the right dock's own container.
`Hud.set_right_column_bottom_clearance(px)` adds to `RightDock`'s `margin_bottom`, pushed by
`Main._update_right_column_bottom_clearance` iff the dock is BOTTOM and the HUD kept its strip.
**The region's rect is untouched**, so `lateral_column_widths()` / `right_column_width()` answer
exactly as before and the clearance cannot feed back into the yield rule that produced it. `RightScroll`
shortens with it, so a card of any height is bounded rather than merely currently-short. **The LEFT
column is deliberately not clearanced** — running to the window bottom is the whole point of the
conditional inset.

**The assertion that let this ship is the lesson.** `_assert_rail_is_right_justified` claimed the rail
sat at "strip end less `_bound_trailing`" — phrased in terms of the implementation, so it went green
whichever way the bound was applied. It now claims the **viewport's** right edge, which fails against
the inboard pinning and passes against the flush one.

**Still to know:** the second band column is lost between the fork and the width where the remaining
leading bound stops mattering — the lower edge of that band IS the fork by construction, since below it
no bounds apply. Dropping the trailing charge shrank the band sharply: 2432–2560 now keeps two
columns, where it lost one before. What is left of the trade sits just above 1871.

**At high `ui_scale` the clipping returns, and that is the intended fallback.** On a 2560 canvas,
`ui_scale` 1.0 gives a 1535 span and keeps the strip; 1.35 gives a logical 1896, a span of 871, and
yields. The flip is at roughly `ui_scale` 1.15 there. Printed each run rather than asserted — it is the
interaction of two independent constants, not a contract.

## The strip's height is 418 at ONE band column and 335 at two

> ⛔ **THE HEADLINE NUMBER WAS 360 WHEN THIS SECTION WAS WRITTEN AND IS 418 TODAY** — it went 360 → 440
> → 456 and then, for the first time, DOWN to 418 (see "…AND 456 CAME BACK DOWN TO 418"). Every
> measurement below is quoted against the 360/275 boxes of its own day and is kept as the record of how
> the two-column budget was DERIVED; the derivation is unchanged and `PANEL_HEIGHT_WIDE_TWO_COLUMN` is
> still 335, which `_horizontal_panel_height()`'s `maxf` still makes inert against the one-column budget.

`PANEL_HEIGHT_WIDE_TWO_COLUMN` is **derived, not authored**: `263` (the charted band flank, measured)
`+ 12` slack (one `ZONE_SEPARATION`) `+ 60` chrome (header 38 + card 22) = **335**. `_body_budget()`
picks it off `band_zone_columns()`, which is purely geometric — so the height stays
content-independent and the flicker invariant is untouched, exactly as `_shell_chrome_height()` is
licensed.

**A FLAT lower constant does not work, and the harness proves it**: a one-column horizontal dock still
needs the band zone's ~299 stack, and applying 335 at both counts overflows it by 24. **Top docks are
one column** (the lateral bounds cost them 704px of span), so they keep 360.

**Which zone binds, after the parties list scrolls:** band 263 of 275, work 256 of 275 (it re-pages one
row shorter), parties 158 of 275. Parties and work structurally cannot bind — parties is
head + footer + the list's floor, and work pages itself.

### The floors are stacked, not sequential

The 360 was tried at 230 once the two-column flank needed only 148, and it failed 18 ways. That is the
lesson worth keeping: **removing the tallest thing reveals the next one a few pixels below it.** Four
things press against the same box, and at the time the band flank at 299 of 300 was merely the
*closest* to the ceiling, not the thing setting it.

| what binds it | measured |
|---|---|
| The **parties** zone's worst case — the seven-line inspector strip | **294px** of body; +44 header +22 card chrome **is** 360 |
| The **parked chrome** stack (`DockRowController._required_height`): nav 164 + turn orb 128 + separation | **~322px** of strip — below it the reflow gate declines and a BOTTOM dock silently keeps the minimap and turn orb, undoing issue #324 |

**The PARTIES row is how the 335 was unlocked** — its list now scrolls, so the zone falls from 294 to
158 and the band flank becomes the binder. The parked chrome is unmoved and is the reason 335 is as low
as this goes on a BOTTOM dock: the measured margin is **13px** (was 38), asserted on
`band_panel_dockrow_bottom` with the chrome really parked. Sabotage at a 300 budget reads
`margin -22px` and the predicted cascade follows — chrome un-parks, shell flips narrow, the HUD keeps
its strip.

The width work's recovered 151px still has nowhere to go on the strip; it is spent **inside** the zone
(below). And the tile-column clipping a horizontal dock causes is reduced only by the 25px, not solved.

## AN EMPTY WORK BOARD STILL DECLARES ITS WIDTH, or the zone keeps the LAST band's

Reported from play, on a horizontal dock: *"the empty view is too wide at the work tab, everything is
stretched."* The three POOLS cards were spread across the full span with enormous internal padding, above
the `HudWorkVocab.WORK_EMPTY_HINT` line — measured on a 3440 bottom dock, **1520px of work zone holding
one hint line**, against the 380 a board of one column wants.

**THE MECHANISM IS AN EARLY RETURN, NOT A SIZING RULE.** Since issue #377 the wide shell's card is built
UP from a declared column count (`BandCityPanel._card_width()` reads `_work_columns`), and the ONLY thing
that ever sets that count is `set_work_columns`, reached through `_declare_work_columns` ←
`_declare_work_layout` ← **`_work_board_capacity`**. `BandPanelController._fill_work_zone_column` returns
at its `filtered.is_empty()` branch *before* reaching it, so a band with nothing worked never declared
anything and `_work_columns` kept whatever the previous render left: `WORK_MAX_COLUMNS` (4) on a fresh
panel, and the previous BAND's count after a cycle. The zone was then drawn that many columns wide with no
board in it, and every child of it — the pools cards, the chips, the hint — stretched to fill.

**THE FIX IS ONE DECLARATION ON THAT BRANCH**, `_declare_work_columns(filtered.size(), …)`, whose answer
for a zero-length board is ONE (`_wanted_work_columns` clamps `ceil(0 / rows)` up). No new rule, no new
constant, and still exactly ONE declaration per pass — the branch returns, so it can never run beside
`_work_board_capacity`'s. **Clamping the pools block instead was refused and would have been the wrong
seam**: the width is the defect and the cards are only what shows it, so the chips, the hint and the board
would all still have been sitting in an over-wide zone.

⛔ **IT IS NOT THE ROWS-PER-COLUMN PREFERENCE, AND THE ARITHMETIC SAYS SO.** `_wanted_work_columns(0, rows)`
clamps to 1 with or without the preference, and `_declare_work_layout` takes its `short` branch to the same
answer — the declared count was always going to be 1 if anything had declared at all.

**HOW IT SQUARES WITH THE MATRIX'S "zone w 380 at 1920 BOTTOM".** That measurement was right and could not
see this: every state in it stages a band WITH sources, which reaches `_work_board_capacity` and declares
correctly. The stale count only survives on the path that returns early, and **nothing in
`band_panel_preview` had ever rendered `WORK_EMPTY_HINT`** — the nearest state,
`band_panel_dockrow_ultrawide_empty`, is called empty and is not: its band works no sources of its own but
the world still holds patches and herds, so the board draws.

**THE STATE THAT CLOSES IT IS `band_panel_work_empty_wide`**, and the ORDERING is the reproduction: it
opens the busy 34-source band on an ULTRAWIDE bottom dock, asserts as a PRECONDITION that it earned more
than one column, and only then pushes the unworked band. A state that opened straight onto the empty band
passes with the defect in — which is what a first cut of it did at `DOCKROW_CANVAS`, where the panel
already held one column. The claim is the zone's WIDTH against `ZONE_WORK_MIN_WIDTH`, which is the one an
over-wide zone cannot pass by accident: every pre-existing width assertion in the harness
(`_assert_work_zone_readable`, the content-width walk) asks whether the zone is wide ENOUGH and is
satisfied by one far too wide. It is **paired with liveness** — the hint renders, the POOLS block renders,
and the board really drew zero rows — since a zone rendering nothing is trivially not stretched.

**The `_queue_expanded` branch two lines above returns without declaring too, and is left alone
deliberately.** It inherits the count from the SAME band's previous render (the expanded queue is entered
from a board that had just declared), so every shipped frame reads one column and no stretch is
observable; what the right width for a full-queue list is, is a design question rather than this defect.

## The bottom row SPLITS its chrome to both ends where it fits, and stacks it where it does not

Ray, on a bottom dock: put the minimap at the leading end and the turn orb at the trailing end, the way
they sit when the panel is docked at the TOP and `BottomBar` stays intact. His reasoning — *"there was a
reason for that when the horizontal band was first done. However, now the band panel doesn't take up the
entire space"* — is correct, and the re-measurement below says by how much.

**THE RECORDED REJECTION, QUOTED, because it was right when it was made**: *"A gutter at each end was
built first and rejected on sight with a real minimap in it: the left rail is ~300px, so two opposite
gutters pushed the band zone inward AND stranded dead space around the orb, costing ~562px of row. One
column costs `max(nav, turn)` ≈ 296–302 … That hands the zones ~240px back and drops the wide→narrow flip
from ~1605px of window width to ~1377px."*

**RE-MEASURED, THE SPLIT COSTS 141px OF ROW, NOT 562.** Two things moved:

| | then | now |
|---|---|---|
| nav cluster (`NavBacking`) width | ~296 | 296 (Standard) / 308 (Large) |
| turn cluster width | **260** | **116** — the `Turn N` caption moved into the orb face |
| stacked span | 296 + 25 gutter = **321** | **321** |
| split span | 296 + 260 + 2 gutters = **~606** | 296 + 116 + 2 gutters = **462** |
| **extra cost of splitting** | ~285 | **141** |

The other half of the old objection — *"pushed the band zone inward"* — expired with issue #377: the card
is sized to its CONTENT and floats as an island, so room beside it is slack over live map rather than
width taken off the zones.

### The gate, and what it is measured against

`BandCityPanel._rail_split()` — split iff `viewport − split span − lateral bounds >= wide_shell_min_width()`.
**Existing vocabulary, no new tunable.** Below it the old rejection still governs: two islands plus a card
cannot fit an arbitrarily narrow window, and the chrome stacks exactly as before. **What the `lateral
bounds` term MEANS was the defect** — see "it needed 2012px" below.

It is the shell's MINIMUM rather than `_card_width()`'s current answer **deliberately**: the declared card
width follows the work board's column count, so gating on it would move the minimap to the other end of
the screen when a band gains a source — and it would be a cycle besides (`_card_width` ← `_work_columns` ←
`set_work_columns` ← `_affordable_work_columns` ← `_available_card_span` ← `_rail_span` ← the verdict).

**Measured flip widths** (Standard map, 3-zone band subject, `wide_shell_min_width()` = 1190):

| state | window | leading bound | verdict |
|---|---|---|---|
| `band_panel_dockrow_bottom` | 1920×1080 | **0** — the left column is not in the row | **split** |
| `band_panel_dockrow_column_reaches` | 1920×**540** | **360** — the tile card really does reach the row | **stacked** — 1098 of room, 92 short |
| `band_panel_dockrow_bottom_yield` | 2560 | 0 (HUD yields) | **split** |
| `band_panel_dockrow_ultrawide` | 3440 | 360 → **0** | **split** |

So the split needs **≥ 1652px** of window (1190 + 462), and ≥ 2012 in the one case where the left
column's card really does descend into the strip. `band_panel_preview` prints the live threshold beside
the state (`chrome split threshold — 1652px of window … 1920 splits, 2000 splits`).

#### ⛔ IT NEEDED 2012px, AND THE FEATURE DID NOT EXIST ON THE HARDWARE IT WAS BUILT FOR

The retired reading of the table above was: *"the split needs ≥ 2012px of window while the HUD keeps its
left column (1190 + 462 + 360) … **A 1920 monitor therefore still stacks** — there is genuinely no room
for two ~300px islands beside a 1190px card and a 360px HUD column."* Reported from play on a ~2000px
window: minimap and turn orb still stacked bottom-right. **2000 − 462 − 360 = 1178 against a minimum of
1190 — it failed by 12px**, and a feature that needs 2012px on a 2000px monitor is a feature that does
not exist.

**THE 360 WAS THE LEFT COLUMN'S WIDTH CHARGED AT A HEIGHT THE COLUMN IS EMPTY AT.** `LeftDock` is
`SIZE_EXPAND_FILL` in `ContentRow`, so its REGION runs the window's full height — but the region is not
what a player sees. The dock holds exactly ONE card (`left_dock.add(tile_panel, 10)`, the only
registration there is), and measured on a 1920×1080 bottom dock it stops at **224** against a strip whose
top edge is **662**. The bottom-left of that screen is open map: the chrome was being held off an
obstruction that was 438px away.

**IT IS THE SAME MISTAKE `348e5c09` MADE ON THE TRAILING COLUMN**, which is why the fix is the shape that
one already has. That commit bounded the chrome rail against the right column's reserved REGION and left
a visible band of dead map beside it; the rail is flush now and the right dock's CARDS are held above the
strip instead. `band_panel_preview._right_dock_content_reach`'s own header states the rule —
*"Never `right_dock_region`, whose rect spans the whole row whether or not anything is painted in it"* —
and every word of it was true of the left column too.

**THE FIX IS AT THE BOUND, NOT AT THE GATE OR THE THRESHOLD.** `_rail_split()` is unchanged;
`Main.band_panel_lateral_bounds` (a `static` beside `band_dock_overlays_hud`, so the harnesses call it
rather than restate it) drops the leading term on a BOTTOM dock wherever
`Hud.left_column_content_reach()` — the painted bottom of the left dock's cards, clipped to `LeftScroll`
— stops above the strip. Every consumer of the bound gets the honest number at once: the gate, the
leading island's `offset_left`, the card's centring and `_available_card_span()`. Lowering
`wide_shell_min_width()` or a rail span to clear 2000 would have been tuning a measured quantity to pass
a test.

**IT IS LIVE, AND IT HAS TO BE — the column is ALLOWED to grow into the strip.** Only the RIGHT column
yields its bottom on a bottom dock (`Main._update_right_column_bottom_clearance`: a bottom inset on
`LayoutRoot` would shorten BOTH, and the left column's full height IS the defect that inset exists to
fix). So this is not a worst case to assume away: where the tile card really reaches the row, the bound
comes straight back and the chrome stacks. `band_panel_dockrow_column_reaches` stages exactly that with a
pure window resize — the strip's top edge is 0.4 of the window's height, so a 540-tall window raises it
to 216, under the same card's 224 — and `_assert_leading_bound_matches_the_column` asserts BOTH
directions. With only the tall state, *"the leading bound is 0"* is satisfied by a client that never
charges it, which is a band card drawn straight through the tile card the day one grows.

**ONLY THE BOTTOM EDGE ASKS THE QUESTION.** On a TOP dock the strip is at the top of the window and the
left column's content STARTS there, so the column is always in the card's band; asking there would zero a
bound that is genuinely owed.

**THE WIDE→NARROW SHELL FLIP DID NOT MOVE.** It is 1331 before and after (`shell threshold probes at
1330 / 1331`), and the honest bound cannot move it: below the yield fork (1871) `Main` pushes zeroes
anyway, so the shell's own arithmetic at 1331 never saw a leading bound to drop. The old two-gutter
shape's ~1605 is not approached.

⛔ **WHAT IT DOES COST, MEASURED AND ACCEPTED**: on a 3440 bottom dock with 34 sources and a TWO-column
band flank, the extra 141px takes the work board from four columns to three (`band_panel_dockrow_ultrawide`,
zone 1522 → 1142), with 648px of open map still beside the card. A gate that could refuse *that* would have
to cost both arrangements in board columns, which needs `_fixed_zone_span()` — the flanks at their CURRENT
counts — and that call chain is the verdict again (measured as a 1000-frame stack overflow, not a
theoretical cycle). Restating it through `wide_shell_min_width()` terminates but silently answers a
different question: that sum is the flanks at ONE column each, so where the flank has two it over-counts
the room by 380 and never fires. It was written, measured, and removed rather than left in looking useful.

### The seam

**The panel decides the ARRANGEMENT; the HUD still owns and measures the chrome.** `DockRowController`
declares both clusters' widths (`set_rail_widths(nav, turn)` replacing `set_rail_width(max)`) and parks
into `RAIL_SLOT_TOP` / `RAIL_SLOT_BOTTOM` exactly as it always did; `BandCityPanel` moves the nav cluster's
SLOT HOST between a new leading island and the trailing one, with the parked cluster still inside it. So
the split is invisible to the controller and no ownership rule changed.

`DockRowController._measured_rail_width()` still reports the STACKED width, and that is sound rather than
stale: its one reader is `rail_width_for` → `affords_wide_shell_with_bounds`, the stacked span is the
SMALLER of the two arrangements, and the panel splits only where the LARGER one still clears the shell
minimum — so wherever that predicate says "affords", the arrangement actually chosen affords it too.

The slot ids still read `RAIL_SLOT_TOP` / `RAIL_SLOT_BOTTOM` and now mean "nav" / "turn"; they are kept
rather than renamed because the HUD parks by them and renaming would touch every call site to say the
same thing.

### What the harness had to relearn

Five claims were written against a row with exactly one chrome island, and each failed on a *correct*
split layout:

* **`_assert_parked_chrome_fits`** measured both clusters against the trailing rail — reported the nav
  cluster spilling by ~2,000px. Now each cluster is measured against the island it is actually in (found
  by ancestry), with per-island centring, plus a new liveness claim on the island COUNT: one when stacked,
  two when split.
* **`_assert_card_is_centred`** measured the gap's leading edge from the HUD bound, so a correctly centred
  card read as off-centre by exactly the leading island's span. The leading edge is now measured off
  whatever really stands there — the same rule the trailing edge already followed.
* **`_assert_open_strip_reaches_the_map`** probed from the bound (i.e. into the new island, which is
  supposed to eat clicks) and subtracted the whole rail span from the trailing gap (double-counting the
  leading island, giving a −141px gap). Both edges now come from the shared `_card_gap_lead_edge()` /
  `_card_gap_trail_edge()`, and every island is probed for click-eating, not just the trailing one.
* **`_assert_card_clears_lateral_columns`**'s negative control asked whether the UNBOUND card collides
  with the left column. Where the row splits, the leading island stands in front of the card, so the
  control went vacuous. It now tests the leading-MOST furniture, and the leading island is separately
  asserted to clear the column.
* **Both shell-threshold probes** read `_rail_span()` live at an ULTRAWIDE canvas and then pinned a canvas
  `threshold + span` wide. That span is canvas-dependent now: read where the row splits, it is 141px too
  big, and the probe landed on a window where the row does NOT split, kept the 141px, and came out WIDE
  one pixel below the threshold it was meant to be under. They read the STACKED span now — which is the
  honest term, since the row never splits below the threshold.

**New claim: `_assert_chrome_ends`**, on all three parked states. Split ⇒ the nav cluster is wholly
LEADING of the card and the turn cluster wholly TRAILING of it, in different islands; stacked ⇒ same
island, nav above turn. Order along the axis that matters for each, so neither arrangement passes on the
other's layout, and paired with liveness (both clusters visible with a non-degenerate rect) because chrome
that never rendered is at no end at all.

**And the leak check**: `_assert_chrome_parked(false, …)` now also requires the leading island to be
retired — hidden and zero-width. It is a `_root` child of the PANEL, not of `BottomBar`, so
`DockRowController._home` cannot restore it and nothing else would have noticed a 296px band of chrome
left standing over the map on a vertical dock.

## The band zone's tier reads the whole STACKING BUDGET, and the split is authored

`_band_zone_tier_height()` is the zone box **times the column count**. Both terms are geometric, so the
tier stays content-independent; **one column multiplies by one**, which is what makes every
single-column layout arithmetically identical to before. At two columns 300 × 2 = 600 → TALL, which
restores the food-outlook chart and the role cards' hint text that SHORT hides. Without it the widened
flank filled 49% of the room and rendered bare `Scout` / `− 2 +` steppers — the width complaint
answered by moving the emptiness rather than removing it.

**The blocks are heterogeneous, so which column each lands in is AUTHORED, never reflowed** — and
raising the tier forced the pairing to change. Of four authored candidates measured at TALL against a
300px box, exactly one fits:

| split | measured |
|---|---|
| vitals + PEOPLE \| WORKFORCE | 316 / 193 — overflows |
| vitals + PEOPLE \| outlook + WORKFORCE | 200 / 321 — overflows |
| vitals \| PEOPLE + outlook + WORKFORCE | 130 / 391 — overflows |
| **vitals + outlook \| PEOPLE + WORKFORCE** | **246 / 263** — fits |

It reads as *the larder | the people*, which is defensible on its own terms, but **that is a happy
accident: it is the only one that fits.** Do not treat the pairing as a design principle that would
survive the blocks changing size — re-measure all four.

**A band with no food history builds no chart, and that is turn one** — the first frame of every new
game. Under the charted split that column was the vitals alone, 130 against 263, so there was a
**SECOND authored layout** selected by one boolean (`people_column = PEOPLE if outlook != null else
LARDER`) with PEOPLE as the only block that moved.

### ONE AUTHORED SPLIT NOW — the boolean went with the row that made it necessary (arc #527)

Retiring the `Trade` vitals row took ~26px out of the vitals block, which is the only block the
SHORT column can pay with, and both layouts fell through `band_panel_preview`'s levelness floor. The
answer this file mandates is *re-author the split and re-measure, never lower the floor* — so all four
candidates were re-measured, and the result is that **the two layouts collapsed into one**:

| | LARDER | WORKFORCE | level |
|---|---|---|---|
| **charted** — vitals + PEOPLE + outlook \| WORKFORCE | **290** | 256 | **88%** |
| **chartless** — the same split, no chart | **174** | 256 | **68%** |

`vitals + PEOPLE + outlook | WORKFORCE` is the best CHARTED layout *and* the best CHARTLESS one, so
one split serves both and `people_column` is deleted. PEOPLE now sits in the LARDER column
unconditionally.

### …AND THE KEEPING BLOCK MADE IT FIVE BLOCKS, WHICH IS WHY IT IS A BLOCK AT ALL

`docs/plan_standing_upkeep.md` §2.5 added two keeping role cards and a fund-mode row. Folded into
WORKFORCE they took that block from **256 to 392**, and the flank fell to **44%** against the 65%
floor — with **no re-authoring able to recover it**: with four blocks the best remaining split still
leaves one column carrying a block nearly half the flank. So the keeping pair is its own block
(`_build_keeping_block`), which is what makes a fifth arrangement available. Re-measured:

| | LARDER | the other column | level |
|---|---|---|---|
| **charted** — vitals + PEOPLE + keeping \| outlook + WORKFORCE | **342** | 372 | **92%** |
| **chartless** — the same split, no chart | **342** | 256 | **75%** |

One split still serves both, and it reads as *the band and what it holds | the chart and what it
does*. The LARDER column now OVERFLOWS the 275px box in both cases, which is what the zone's own
scroll is for — see "`PANEL_HEIGHT_WIDE` is the BODY's budget" for why deleting a block to avoid a
scrollbar is the worse trade.

### …AND THE BUILDERS CARD BROKE IT AGAIN, so the split is re-authored (§4.6b)

`docs/plan_standing_upkeep.md` §2.5's builders pool is a THIRD role card in the keeping block, and one
card took that block past what the split above could carry: the flank fell to **56%**, under the 65%
floor. Re-authored and re-measured, all four candidates, at the KEEPING block's new height:

| split | chartless | charted |
|---|---|---|
| vitals + PEOPLE + keeping \| outlook + WORKFORCE (the old winner) | 56% | — |
| vitals + PEOPLE \| keeping + outlook + WORKFORCE | 32% | — |
| vitals + keeping \| PEOPLE + outlook + WORKFORCE | 62% | — |
| **vitals + WORKFORCE \| PEOPLE + keeping + outlook** | **93%** (372 / 345) | **81%** (372 / 461) |

`vitals + WORKFORCE | PEOPLE + keeping + outlook` wins both, so ONE split still serves both cases and
`people_column` stays deleted. It reads as *what the band IS and what it DOES | who they are and what
they hold* — which, like its predecessor, is a defensible reading and NOT why it was chosen: it is the
only candidate that clears the floor.

**THE THIRD ROW IS THE ONE TO RE-MEASURE FROM, NOT TO SUBTRACT.** Subtraction predicted 67% for it and
it measured **62%**, which is this section's own standing warning arriving for the third time in this
flank's history. Re-measure all four; never derive one from another.

**THE FLOOR MOVED, AND ONLY BECAUSE NO SPLIT CLEARS IT.** `band_panel_preview`'s
`BAND_FLANK_BALANCE_FLOOR` went **0.75 → 0.65**. That is the re-calibration this file warns against,
taken only after the re-authoring it mandates: with three chartless blocks there are three orderings
and the rivals measure **32%** and **19%**, so 68% is the CEILING of what the chartless flank can
reach, not a shortfall against a reachable better one. The floor still fails the best rival by a wide
margin, which is what keeps it a real assertion rather than a rubber stamp. **The charted flank has
23 points of slack at 88%** and is no longer the binding case; the chartless one is.

> **A card ORDERING moved this once, and that is worth knowing before the next change.** An
> intermediate role-card layout put the description above the controls; on it WORKFORCE measured
> **332** and the then-charted split read **74% — failing**. Moving the prose to the bottom of the card
> (see "The role cards carry the band's OTHER two kits") took 6px back off the block. The flank is
> sensitive to changes with no obvious geometric content at all.

The split is hand-authored and hand-measured, and it feeds no geometry, so the flicker invariant was
never in play for it.

**The measured numbers do not decompose by subtraction** — separations and spacing differ per grouping,
and both predictions made that way were wrong (188 vs the actual 200; 258 vs the actual 246).
Re-measure; never derive.

**A LOW FILL FIGURE IS A CEILING, NOT A SHORTFALL** — this flank's oldest lesson and the one that
justifies the floor move above. The chartless blocks total what they total against the 600px two
columns offer, and the total is the total however it is dealt out; what the split buys is *where* the
emptiness sits. (With the role cards' pickers the CHARTED flank no longer has emptiness to place — it
OVERFLOWS its box and the zone's own scroll carries the remainder.)

Blocks are emitted in **build** order for one column and by **column** field for two, so the flat stack
stays vitals · PEOPLE · outlook · WORKFORCE.

**The fill assertion measures the WHOLE FLANK against the WHOLE room, plus a separate balance claim.**
Measuring only the deepest column passed the 130/263 flank at 88% — the short column was invisible to
it. Two independent failure modes (uniformly empty = a tier that did not rise; lopsided = the wrong
split) need two claims.

## THE POOLS BLOCK — all three work pools, on the tab that spends them (§4.7)

> #### ⛔ THIS SECTION SUPERSEDES THE KEEPING BLOCK BELOW, WHICH WAS ON THE **BAND** TAB
>
> The passages under it are kept because they still record *why* each control reads the way it does —
> why the keeping pair mounts no kit picker, why the builders card's picker was a defect rather than a
> rendering bug, why the fund mode is the one decision the roles cannot express. Where one says the
> block lives in the band zone, read it against this.

`docs/plan_standing_upkeep.md` §4.7. **The pool was on one tab and its consequences on another**, which
is why its shortfall line went unnoticed in playtest entirely: the Agriculture / Husbandry / Builders
cards sat in the band zone while the sources they pay for, and the queue they fund, were a tab away. The
whole block moves to the **WORK** zone, directly under the zone head and above the BUILD QUEUE, so the
tab reads top to bottom as the building loop — *what you have to spend · what it is spending on · where
new jobs are declared*.

- **The head counts all THREE roles** (`POOLS_ZONE_READOUT_FORMAT`, `N of M on work`) where the retired
  `KEEPING_ZONE_READOUT_FORMAT` deliberately counted two. That is not a widened definition of keeping —
  it is a different question, asked by a block that now holds all three pools.
- **THE CARDS ARE COMPACT, and the prose became a tooltip.** `_build_pool_card` is a name and a stepper
  and nothing else; the three role hints survive verbatim as each card's `tooltip_text`. The band tab's
  cards could afford a description because that zone SCROLLS; this one CLIPS, and a description read
  once cannot cost height on a surface operated every turn.
- **The Builders card's read-only gear line did NOT come along**, and its fact was not lost: the BUILD
  QUEUE head one block below already states `3 builders · Tillage kit` through `_role_kit_id`, the same
  resolution. Two surfaces for one fact, and the survivor is the one adjacent to the jobs it prices.
  `KitRoster.role_gear_line`, its builders `ROLE_AXES` entry and `KIT_ROLE_BUILDERS_BUILD_FORMAT` are
  retired with it.
- **The fund-mode row is ONE line now** — `[Spread][Priority]  Short 5 work of 7 this turn.` — and the
  `SHORT OF KEEPERS` title is deleted. A label over a control whose whole content is two words and a
  number says nothing the control does not. Measured: **67px → 22px**, which took the block from 155 to
  110 and paid for most of the strip growth below. Everything else about the row is unchanged, including
  that it renders only where a web demands work this turn.
- **Its height is reserved through ONE function**, `HudWorkVocab.pools_block_height(has_fund_mode)`,
  called both as the block's `custom_minimum_size` and by `_work_board_capacity`'s chrome term — the
  BUILD QUEUE block's own rule, for the same reason: this zone `clip_contents`.
- **IT ALWAYS RENDERS, unlike the queue block.** An empty queue is furniture explaining an absence; four
  steppers at zero are the controls that fix it.
- ⛔ **IT IS FOUR CARDS SINCE THE `roadwork` POOL LANDED (arc #532), AND THE FOURTH WAS PAID FOR IN
  WIDTH RATHER THAN HEIGHT.** At the shared stepper metric four cards wanted 466px of a box that is
  382 on the bottom dock and 356 on the left, and both ways to buy that width cost a ROW this zone
  does not have — split 3 + 1 the block wanted 420px of a 358px box. So the block took its own
  stepper and title metrics (`HudWorkVocab.POOL_STEPPER_*` / `POOL_CARD_NAME_FONT_SIZE`) and
  `pools_block_height` did not move. The reasoning, the measurements and the invariant that keeps the
  row inside the box are in `.claude/rules/client/roads.md` → "The row of four does not fit"; **do not
  restate them here**, and do not retune those metrics without re-running
  `band_panel_preview._assert_pool_cards_are_level`, which is where a role name overtaking the
  stepper as the card's floor fails.

### THE STRIP GREW, AND THE TWO-COLUMN BUDGET WAS A SILENT CLIP

The Work zone could not hold a pool header, a queue and a board in a 300px box: the worst case —
fund-mode row plus a queued build — measured **331 of 300**, and the only lever inside the zone (the
queue's row cap) could not close it, the pools block being larger than the queue had to give back. Ray's
call was to grow the dock. `BandCityPanel.PANEL_HEIGHT_WIDE` is **440** (was 360), a **380px** zone box
and **41%** of a 1080-high viewport, well inside the `MAX_WIDE_HEIGHT_FRACTION` clamp. Chosen against two
measured criteria, not rounded: the worst case fits with `build_queue_rows_max` at its full ceiling of 3
(284 of 380), and the 1920 bottom dock's board still pages **2 rows** at four sources, which is what it
drew before the pools block existed. The binding criterion was the board, at 376px.

> **⛔ A ONE-COLUMN MEASUREMENT DOES NOT ANSWER FOR THE TWO-COLUMN PATH.** `_body_budget()` picks
> `PANEL_HEIGHT_WIDE_TWO_COLUMN` whenever the band flank earns two columns, which handed every zone a
> **275px** box — *smaller than the 300 they had before the raise* — so the same worst case clipped by
> 9–11px on a wide monitor, silently, in the zone that `clip_contents`. It now returns
> `maxf(PANEL_HEIGHT_WIDE_TWO_COLUMN, PANEL_HEIGHT_WIDE)`.
>
> **The reasoning that let it through is the lesson.** That constant's own derivation surveyed each zone
> and concluded of the work zone: *"pages itself … so a shorter box costs it a board row rather than
> overflowing. It never binds by construction."* True while the zone was head + chips + board + pager —
> every one of which pages or is a single row. **The POOLS and BUILD QUEUE blocks are fixed-height and
> do not page**, so the zone has a hard floor now and binds like any other. A claim about what cannot
> happen expires when the thing it surveyed changes shape.
>
> At the current numbers the two-column branch is **inert** (329 against 440); the `maxf` is kept anyway
> because `BAND_ZONE_TWO_COLUMN_EXTENT` is a live re-measured derivation, so the max is what stops it
> coming back as a regression instead of a saving. `band_panel_pools_wide_two_column` is the frame, and
> the claim is asserted BOTH ways — the zone fits, and the two-column strip is never shorter than the
> one-column budget. Sabotage-verified against the bare constant: 4 failures naming `Zone_work` and
> `short by 11`.

### …AND 440 WAS NOT ENOUGH EITHER, BECAUSE THE INSPECTOR WAS NEVER BUDGETED

Reported from play: the panel *"overflowed its bounds… only when something is selected in the work
list."* **Two arithmetic faults compounding**, and the zone `clip_contents`, so on a bottom dock the cut
lands at the window's own edge and reads as the panel running off the screen.

- **The reservation ignored the row.** `_work_inspector_height(_model)` took the model and never read
  it — the underscore was in the source — forking only on `_work_floor_open`. The strip draws **four
  conditional children** that fork cannot see: the overdraw line, the `note`, the `muted_note` and the
  `ArrivalStrip`, each with its own gap. It now reads the model, using **the builder's own tests
  verbatim**, which is what keeps reserved and drawn one expression.
- **The zone's declared floor never counted the strip at all**, and `build_queue_rows_max` reserved its
  GAP but not its HEIGHT — so the queue could claim rows the zone could only afford while nothing was
  selected. The authored three rows were being drawn on credit.

**`WORK_INSPECTOR_HEIGHT` 118 → 84 and the floor picker 68 → 32.** The old pair carried ~40px of
unexplained slack charged to the zone on every dock; removing it took the floor from 430 to **396** and
is why the lever below is 456 rather than 490. Both are guarded by *reserved ≥ drawn* assertions now,
the rule the pools and queue blocks already followed and this one never did. **`WORK_INSPECTOR_EXTENT`
has since fallen 78 → 58**, which is the strip's one-sentence readout being deleted outright to pay for
the board row's second line — see "THE ROW IS TWO LINES" above.

**`PANEL_HEIGHT_WIDE` 440 → 456**, a **396px** box, and the work zone reads **396 of 396 — zero spare**
with a row selected. That is exact by intent: Ray took the minimal value (*"we don't want to make it
much taller and text can't be cut off"*) and accepted the wide dock's queue staying at **one row plus
`+N more`**. The alternative measured **540**, which restores the queue's three rows and flips the band
flank to TALL, and was declined as half the screen.

> **⛔ THE STRUCTURAL ANSWER WAS BUILT, AND IT WAS THE CHEAPER OF THE TWO THIS CALLOUT NAMED**
> (`docs/plan_standing_upkeep.md` §4.9 item 12d — see "THE WORK INSPECTOR IS A DIALOG" below). **The
> retired demand is quoted rather than deleted, because every measurement in it was right**: *"THERE IS
> NO ROOM LEFT. The next block added to this zone overflows it. The height has been the lever three
> times in one arc and is out of travel; the structural answer — the inspector replacing the board
> rather than stacking under it, or the tab splitting — is unbuilt and is what the next addition should
> force, not a fourth raise. The two-line board row is the worked example of what "out of travel"
> costs. It needed 16px, the height could not give them, and what paid was DELETING A READOUT — the
> inspector's whole sentence, possible only because the row had taken over every clause of it. The zone
> reads 396 of 396 again; there is no second sentence to spend."*
>
> **What forced it was the next addition, exactly as predicted.** Item 12c's kit pair measured the
> zone and found FOUR pixels of spare — and found that the *shipped* priority picker had been asking
> 444 of a 396px box since §4.9 item 9b, unrendered. The answer taken was neither of the two named: the
> inspector left the zone for a **viewport-centred dialog**, which is a third option both of those were
> reaching past. `PANEL_HEIGHT_WIDE` did not move for it and no config value was retuned.

**Why no harness caught it:** the worst-case zone frames were a BOTTOM dock with **nothing selected**,
and every inspector-open frame was a TALL LEFT dock with room to spare. Two frame families, disjoint,
and the defect lived exactly in the gap. `band_panel_pools_wide_selected` is the frame that closes it,
and item 12d's dock/viewport MATRIX (`_render_work_inspector_dialog_states`) is what stops the shape
recurring: eleven configurations × every picker, so no gap between two families is left to hide in.

**A latent scale defect surfaced with the raise and is fixed.** At `ui_scale` 1.35 the new budget
crosses `MAX_WIDE_HEIGHT_FRACTION`, and a band zone BUILT before the header settles baked a 394px
reservation into a 385px host — `zone_size()` composes its answer from live sub-measurements, so the
order matters. `BandPanelController._sync_band_zone_scroll()` on `zones_resized` is the fix.

**Two consequences of the taller strip, both accepted.** A one-column horizontal dock is now the COMPACT
tier rather than SHORT (380 clears `BAND_ZONE_CHART_MIN_HEIGHT`'s 340), which *restores* the compact
food-outlook chart and the role-card hints that SHORT drew away — a content gain, and the three genuinely
SHORT-tier assertions moved to a pinned `SHORT_TIER_PROBE_HEIGHT` where the clamp reproduces a 300px box,
a state a ~660px-high window still reaches. And the FACTION page is now the emptiest thing in the strip
(174 / 155 / 226 / 54 of 380): it is a read-only rollup with short content, so the height is dead space
there.

**The band flank's split was re-measured across all four candidates with the keeping block gone**, and
`vitals + PEOPLE + outlook | WORKFORCE` wins chartless (68%) and charted (88%) alike.
`BAND_FLANK_BALANCE_FLOOR` **stays at 0.65** — 68% is the chartless flank's arithmetic ceiling, best rival
32%, so nothing clears 0.75 and the floor may not rise. `BAND_FLANK_FILL_FLOOR` went **0.60 → 0.50**, and
the ROOM is what moved, not the content: the same 430px of blocks now sits in 760px rather than 550. It
still fails a flank that lost a block (23% / 49%), which is what keeps it a real assertion.

### …AND 456 CAME BACK DOWN TO 418 — the first time this budget has ever SHRUNK

Reported from play: the bottom strip is too tall. The band flank, the workforce and the work column all
end well above the bottom of it, and only the parties column reaches it — its action grid being
bottom-anchored. Asking for three board rows per column does not answer it and cannot: the budget is a
fixed constant and the board is ELASTIC, so fewer rows becomes empty space rather than a shorter strip.

**THE INSPECTOR'S RAISE WAS ONLY PARTLY RE-CLAIMED, WHICH IS WHY THERE WAS ROOM TO GIVE.** §4.9 item 12d
took the strip out of the zone entirely, so the 396-with-a-row-selected measurement that bought 440 → 456
describes a zone that no longer exists — no inspector term survives in `_work_board_capacity` or in
`HudWorkVocab.build_queue_rows_max`. What moved into the space it left is **one** term, not all of it:
`BUILD_QUEUE_ROOM_SETTINGS_HEIGHT` (56), the queue's own settings strip, which the retiring inspector
reservation had been covering by accident. So the over-provision was **38px**, not the ~104 the retired
strip suggests.

**THE WORK ZONE BINDS ALONE, AND ITS FLOOR IS 358px** — derived and measured agreeing to the pixel:

| term | px |
|---|---|
| `ZONE_HEAD_HEIGHT` + `WORK_CHIPS_HEIGHT` | 20 + 26 |
| POOLS block (`pools_block_height(false)`) | 82 |
| BUILD QUEUE at its floor, settings strip open — head 20 + one entry row and the overflow (2 × `WORK_ROW_HEIGHT`) + `BUILD_QUEUE_ROOM_SETTINGS_HEIGHT` 56 | 132 |
| one un-droppable board row (`WORK_ROW_TWO_LINE_HEIGHT`; the board floors at `maxi(1, …)`) | 44 |
| `WORK_PAGER_HEIGHT` | 24 |
| five `ZONE_BLOCK_SEPARATION` gaps | 30 |
| **total** | **358** |

`PANEL_HEIGHT_WIDE` is **418** = 358 + `HORIZONTAL_BODY_CHROME`. **Swept rather than reasoned**: the whole
harness was run at twelve values of the constant and judged by exit status. **416** hands the zones a 356px
box and `band_panel_build_queue_wide` fails with `needs 358px … short by 2`; 418 clips nothing at any dock
or viewport in the matrix. Fund mode does not raise the floor — its 110px pools block buys the queue fewer
rows, so the two move against each other.

**WHAT THE 38px COSTS IS ONE BUILD QUEUE ENTRY ROW, and nothing else.** A wide dock draws one entry and
`+3 more` where it drew two and `+2 more` (`band_panel_preview.WIDE_DOCK_QUEUE_ROWS` 2 → 1, which is
`BUILD_QUEUE_ROWS_MIN`, i.e. the floor). The board's row count is unchanged at every configuration —
2 rows at 1920/1600/1024, 4 at 1440/1366/1280/1152 — and the tall LEFT dock still draws all three entries.

**NO TIER BOUNDARY IS CROSSED ON THE WAY DOWN, which is what makes it cheap.** `_band_zone_tier_height()`
is the box times the COLUMN COUNT, so a one-column flank was already COMPACT at 396
(`BAND_ZONE_TALL_MIN_HEIGHT` is 420, which a one-column horizontal dock never reached) and a two-column one
stays TALL until the box halves. `BAND_ZONE_CHART_MIN_HEIGHT` (340) is crossed at a budget of **402** —
BELOW the 418 floor, so the work zone clips before the flank can re-tier. And a tier drops no content in
any case: *"A TIER NEVER DROPS A BLOCK — the zone scrolls, so content that outgrows the box is reached
rather than lost."*

**THE OTHER THREE ZONES CANNOT BIND, MEASURED.** The band flank SCROLLS — its one-column content is
**442px invariant** at every budget, over its box at 456 and at 418 alike — the two-column flank is 256
(290 charted), and the parties zone's floor is **226px** (head 20 + `PARTIES_LIST_MIN_HEIGHT` 84 + gaps +
a ~110px bottom-anchored mission grid, i.e. roughly half that minimum is the grid).

⛔ **THE ELASTIC BOARD DOES NOT DEFEAT THE EXERCISE, AND THE "396 of 396" READING IS WHY IT LOOKS LIKE IT
MIGHT.** The reserved strip IS this budget plus the shell's chrome, so lowering it genuinely shrinks the
panel and re-insets the map; the zone reporting zero spare is the board REFILLING the smaller box
afterwards, not the panel refusing to shrink. What stops it going lower is the fixed blocks above the
board — pools, queue, chips, pager — which is the 358.

**THE LATENT SMALL-VIEWPORT CLIP THIS WAS EXPECTED TO EXPOSE DOES NOT EXIST.** 1152×720 was suspected of
already clipping at 456, its box being clamped to 337 against the 358 worst case. It does not: at that
viewport the panel picks the NARROW tabbed shell, whose one zone is the whole card, and the state measures
**337 of a 337px box**. `band_panel_queue_settings_tight` is the frame that says so — the matrix walks
every configuration with the INSPECTOR open and never the queue, while the queue-control states open the
strip only at `DOCKROW_CANVAS`, so this was the gap between two disjoint frame families for the fourth
time in this file's history.

## RETIRED — THE KEEPING BLOCK on the band tab, and the rules that outlived its mount point

`docs/plan_standing_upkeep.md` §2.5. Maintenance is a band-level standing role now, so the band zone
carried a `KEEPING` block under WORKFORCE's own: two cards in the SAME family as Scout and Warrior —
**Agriculture** (the plant web) and **Husbandry** (the animal one) — plus a two-way fund-mode pick.
They are staffed by the same `assign_labor <faction> <band> <kind> <workers>` those two use, through
the same `_build_role_card`; nothing about the keeping is a parallel surface.

- **THE CARDS ARE ROWS OF TWO, never one long row.** At the narrow shell's 354px a four-abreast row
  gives each card ~82px, which clips the role name and the kit face alike. The pairing is the split
  the roles already have — the two EXPEDITIONARY roles first, then the two KEEPING ones — so each row
  reads as its own family rather than as an overflow of the one above. **§4.6b's `builders` card makes
  a THIRD row**, alone: it is neither expeditionary nor keeping — it RAISES what the keeping then
  holds — and a builder pushed up beside Husbandry would read as a third kind of keeper. Its own row
  also costs nothing that pairing it would save, the row height being the card's either way.
- **The keeping roles are in the WORKFORCE bar's `Roles` SEGMENT even though their cards are not in
  that block.** The segments partition `working_age`, `effective_idle` already nets these hands out
  of Idle, and a segment that omitted them would stop the key adding up to the head the zone states.
- **THE KEEPING PAIR MOUNTS NO KIT PICKER, and two independent facts say so.** The wire names no
  default kit for either job — there is no `defaultAgricultureKitId` twin of `defaultScoutKitId`, so
  `(default)` would be a guess and `HudBandLaborState.default_kit_id` falls through to the HUNT
  default — and no shipped kit declares a maintenance contribution, so every entry the picker could
  offer moves no number the player can see. `KIT_PICKER_ROLES` is the gate. A picker whose selection
  changes nothing and whose default mark is wrong is worse than none.
- **⛔ THE BUILDERS CARD MOUNTS NO PICKER EITHER, AND ITS REASON IS THE OPPOSITE ONE: a pick there
  moves too much, permanently.** The roster carries two builders kits, one per web — `hurdling`
  (hurdles, animal) and `tillage` (hoes, plant) — and which one a build gets is DERIVED from **that
  queue ENTRY's** own branch (`labor-ui.md` → "A BUILD IS PRICED AT THE **BUILDERS'** KIT"). A card is
  per BAND, so it could only answer a per-ENTRY question with one standing answer; and the only way
  to *send* that answer was a `kit` token on the `builders` row, which the sim honoured as an override
  **over the derivation from then on**. Measured in play: one click put `kit hurdling` on every later
  builders command and pinned a band raising a plant Cultivate to the animal web's tool with no way
  back — `none` being bare-handed rather than a way to un-pin. **The control was the defect, not its
  rendering**, so `KIT_PICKER_ROLES` no longer names the role and `build_kit_row` is never called for
  it. **THAT TOKEN IS NOW REFUSED BY THE SIM** — `handle_assign_labor` rejects a `kit` on this role by
  name — and the per-entry override lands on the QUEUE ROW (§4.7b, below: one job, one kit,
  `(default)` marked as hunting's is), which is where an entry can answer for itself.
- **…AND THE READ-ONLY GEAR LINE WENT WITH IT — the Builders card states no kit at all.**
  `KitRoster.ROLE_AXES` names **`scout` and `warrior` and nothing else**, so `is_band_wide_role` is
  false for `builders`, `role_axis` answers `""`, and `_role_effect_phrase` falls through its match
  to `""`. There is no builders gear line to render.
  > ⛔ **THIS BULLET DESCRIBED A SURFACE THAT NO LONGER EXISTS.** It read *"the card STILL STATES what
  > the pool is carrying, on a read-only gear line: `KitRoster.role_gear_line` renders `Tillage kit ·
  > 8.5 work off a build, per builder · Hoes 38`"*, and credited `ROLE_AXES` with carrying the role's
  > build axis. **The line was retired in §4.7** and the axis went with it, so the sentence outlived
  > its mechanism — and its figure outlived the model twice over: `8.5` is the retired SUBTRACTION
  > form (hoes deliver **+0.5 build work per equipped worker per turn**, an addend; a job's work
  > requirement never changes), and `Hoes 38` is a condition clause off a line nothing draws.
  > **Where the question is answered now:** the crafting panel's kit ledger owns *what the band is
  > carrying*, and the per-entry kit lands on the QUEUE ROW (§4.7b, below), which is the one surface
  > that knows which job it is pricing.
- **IT IS `_role_kit_id`, THE BUILD QUEUE HEADER'S OWN CALL** — one resolution, two surfaces, so the
  card and `3 builders · Tillage kit` cannot name two different webs' tools for one pool. The sim
  publishes the `builders` row's kit already resolved; an unstaffed row is derived client-side through
  `KitRoster.build_kit_for_branch`; and an EMPTY queue derives nothing and reads the bare kit's
  `No kit`, rather than `resolve_selection`'s terminal fall-through to roster order, which presented
  `hurdling` as a decision the player had never made.
- **⛔ THE BUILDERS STEPPER SENDS NO `kit` TOKEN AT ALL, and that is this card's own rule.** Every
  other role's `+` re-states either a stored kit or its job default, so the token is a no-op; echoing
  the DERIVED id back here would pin the pool to whichever web it happened to be building the moment
  the player pressed `+`. `_commanded_role_kit_id` forks on the role for exactly this and answers
  `NO_KIT_ID` on the builders branch — which, with nothing left to write `_role_kit_ids` for that
  role, it now always does. **The fork is kept because it is what STATES the omission is deliberate**;
  collapsing it into the other roles' `_role_kit_id` restores the pin.
- **THE ROLE ALSO NEEDED A BRANCH IN `Main.format_assign_labor`, which had never named it.** The
  `assign_labor` builder matched `scout` / `warrior` / `agriculture` / `husbandry` and answered `{}`
  for anything else, so the Builders card's stepper emitted NO COMMAND AT ALL and the pool could not
  be staffed from the UI. The sim has parsed `builders` since §2.5.
- **THE FUND-MODE ROW IS THE ONE DECISION THE ROLES CANNOT EXPRESS** — `Spread` (fund every source in
  proportion, so everything degrades a little) against `Priority` (fund the biggest investments in
  full and let the marginal ones rot), emitting `upkeep_mode <faction> <band> <mode>` through
  `BandPanelController.upkeep_mode_requested`. **It renders only where either web demands work this
  turn**: a band holding nothing has no split to choose, and a control offered there reads as a
  setting the player forgot to make. The active mode is `primary` and the other `ghost`, the work
  board's filter-chip treatment, and **both stay pressable** — a disabled active mode is
  indistinguishable at a glance from an unavailable one on a control whose whole content is two words.
- **The line beneath states the POOL's own arithmetic, in both directions** (`Short 5 work of 7 this
  turn.` in WARN, or the covered form in HEALTHY). Both figures are summed by
  `HudBandLaborState.upkeep_pool_state` from the wire's per-source fields — one sum per web, skipping
  a row with nobody on the take exactly as `systems::labor::maintenance_shares` skips it — and
  nothing is derived from anything else.

**Frames:** `band_panel_upkeep_mode_spread` / `band_panel_upkeep_mode_priority`, a band short on BOTH
webs rendered under each mode. **The pair is the claim** — the two differ in one lit button and one
word, so either alone says nothing about whether the control reflects the band's own
`upkeepFundMode` — with the NEGATIVE on the reference band beside them (nothing to keep, no control).
`_assert_upkeep_mode_control` adds the press, read off the emitted payload, which no frame can carry.

## The role cards carry the band's OTHER two kits

A wayfinding kit and a warrior kit were applied to the Scout and Warrior rows **silently**: the cards
named neither, and `KitRoster.build_kit_row` was mounted only on the four hunt/forage compose sheets,
so naming a kit on a band-wide role was a command-line act (`assign_labor … scout 3 kit none`). Each
card now reads

```
Scout                                      Warrior
      [−]  1  [+]                                [−]  0  [+]
[🧭 Wayfinding kit  ⌄]                     [🪓 Warrior kit  ⌄]
2-tile sight per vantage · Wayfinding 66   attack 6 defending the camp · Clubs 22
Posts scouts that see around …             Guards the band — matters once …
```

**THE CONTROLS LEAD AND THE PROSE TRAILS, and the gear line is the PICKER's help text.** A card is
read every turn and acted on with two controls; the description is what a player reads ONCE, to learn
what the role is. The gear line states what the SELECTED kit buys and moves when the picker moves, so
nothing may come between the two — `build_kit_row` returns them as ONE block, which makes that
adjacency structural rather than a rule this call site has to remember. **The Builders card keeps the
help text after losing the control it helped**, one slot up in the same order, because what the pool
is carrying is a fact worth stating whether or not the player may change it.

**BOTH CARDS DRAW TO THE HEIGHT OF THE TALLER ONE, and nothing was ever shrinking them.** The row's
`HBoxContainer` stretches a child to the row height wherever the child asks to FILL its cross axis,
and `SIZE_FILL` is `Control`'s own default, which `PanelContainer` does not override (measured). So
the card RECTS were level under the old ordering too; what read as ragged was the CONTENT — the
Scout's three-line description pushed its picker, gear line and stepper 17px below the Warrior's and
left dead space under the Warrior's stepper. With the controls first they land on the same lines in
both cards and only the trailing prose differs in length. The flag is written out at the call site
rather than inherited, because the alternative is a hardcoded minimum height that would be wrong the
next time a description or a kit name changes length; `_assert_role_cards_are_level` is the guard
(Scout wants 193px, Warrior 176px, both RENDER 193px), sabotage-verified against `SIZE_SHRINK_BEGIN`.

**WHICH ITEM BACKS A ROLE IS DERIVED, and the warrior is why it needed a new seam.**
`KitRoster.AXIS_ITEMS` maps an axis to its item, and `attack` maps to the SPEARS — right on a hunt
sheet, wrong here, because a warrior kit buys the same stat off `clubs`. `KitRoster.ROLE_AXES` names
each band-wide role's axis and `JOB_AXIS_ITEMS` / `item_for_axis(job, axis)` resolve the item per JOB,
which is what the axis table's own note always said the lookup was keyed on. **Nothing outside
`KitRoster` names an item**, so a roster that moves the handling of a stat moves the card with it.

**THE LINE IS THE GEAR POPOVER'S OWN WORDS** (`DetailFormat.KIT_ROLE_SCOUT_VANTAGE_FORMAT` /
`KIT_ROLE_WARRIOR_ATTACK_FORMAT`, `kit_item_label`, `kit_condition_face`), reached through
`KitRoster.role_hint`. The Kit row's popover states the identical pair for the identical band
(`▲ Wayfinding 66 — 2-tile sight per vantage`), and two phrasings of one reading is how a card and
the popover above it come to disagree.

**IT FOLLOWS THE SELECTION, NOT THE WIRE'S RESOLVED TIER.** The cohort's `scoutVantageRange` /
`warriorAttack` are quoted at each job's DEFAULT kit, so a card that read them would print
`attack 6 defending the camp` beside a `No kit` selection. `KitRoster.role_gear` resolves the tier
from the SELECTED kit against the band's own item condition — the role twin of `effective_tiers`,
which exists separately precisely because that one is job-blind and would step a warrior down on
spent spears.

**THE PICK EMITS ON THE PRESS**, like the work inspector's policy picker and unlike a compose sheet:
a card has no Send to commit at, so a pick that only moved client state would leave the sim running
the kit the card had stopped naming. The command re-states the role's current head count and moves
only the kit token, which `Main._kit_token` omits when the choice equals the job default — so a
player who never opens the picker emits the byte-identical line they always did. **An UNSTAFFED role
emits nothing**: `assign_labor … scout 0` drops the assignment and the sim resolves no kit for it, so
the pick is held and rides the first `+`.

**THE SELECTION LIVES ON `BandPanelController._role_kit_ids`, keyed `"<band entity>:<role>"`** — zone
state that survives a snapshot, this controller's own remit (`_work_filter`, `_send_hunt_floor`), and
explicitly not a state model's, a model being for a field two clusters read. The role cards had
nowhere to keep per-row state before this; `ComposeState` was rejected because it is the model for
what a *sheet* is composing and a card has no composing act to bracket. It is keyed by BAND because
the cycler walks bands, and it is SEEDED from the wire — the role's own resolved
`LaborAssignment.kitId` — so a fresh session shows what the sim is actually running rather than a
default.

**GREYING: there is none, and the seam is still there.** `build_kit_row` asks `KitRoster.kit_offer`
per entry as it does everywhere; that test answers "offered" for every job but `hunt`, there being no
quarry whose outcome a band-wide kit could fail to change. A future rule lands automatically.

**Two card-shaped deviations from the compose sheets' row, both forced and both measured:**

- **No field key.** The card is already headed `Scout`, and `COMPOSE_FIELD_KEY_WIDTH` (64) of a
  ~137px card leaves the control ~73px. `build_kit_row`'s `key_text` is `""` here.
- **`compact_chrome`** — `HudWidgets.compact` at the WORK zone's own row size and padding. It buys
  both things the card is short of: the ~42px ghost stylebox is a fifth of the card's height, and at
  the default type size `clip_text` rendered the face as **`🧭 Wayfinding ki`**, naming a kit whose
  name it had eaten the end of.

**WHAT IT COSTS THE FLANK, MEASURED:** the WORKFORCE column went **326px** charted against a 275px
two-column box (from ~263), which took the band zone's charted split from 94% level to exactly the
then-75% floor. (Arc #527 has since re-authored the split into ONE layout and re-measured that column
at **256px**; see "The band zone's tier reads the whole STACKING BUDGET" before adding a row to it.) In the one-column horizontal dock
the flank reads **416px of a 300px box**, so the role steppers sit further under the zone's scroll fold
than before; that zone SCROLLS, so nothing is lost, and the standing lever if the steppers must be
reachable without a gesture is a density cut at SHORT (see "`PANEL_HEIGHT_WIDE` is the BODY's budget").
**The card ORDERING is part of that measurement** — with the description above the controls WORKFORCE
measured 332 and the charted split failed the floor at 74%.

**Frame + assertions:** `band_panel_role_kits` (the LEFT dock, both cards). Three claims a picture
cannot make, each sabotage-verified against a DISJOINT mutation — `_assert_role_cards_are_level`
compares the two cards' RENDERED heights behind a precondition that their CONTENT heights differ (two
cards of equal content are level for free) —
`_assert_role_card_gear` compares both cards' face and gear line by EQUALITY against expectations
that name their item LABEL outright (composing them through `item_for_axis` would assert only that
the derivation agrees with itself; dropping the job override fails the WARRIOR card alone, naming
`attack 6 defending the camp · Spears 74`), and `_assert_role_kit_command_carries_the_pick` drives
the picker's real `item_selected` wiring and reads `Main.format_assign_labor`'s line off the emitted
payload — a PAIR, the non-default pick carrying `kit none` and the default pick carrying no tail at
all, since either alone passes on a builder that gets the tail exactly backwards. `cargo xtask
command-guard` carries the other half: it now drives a band-wide role with a non-default kit and
parses `assign_labor 0 <band> scout 2 kit none` with the real server parser, a grammar whose tail was
closed until this.

### …AND ITS FALLBACK WAS ROSTER ORDER, WHICH IS HOW IT NAMED THE WRONG WEB (§4.6b)

Two symptoms, one root, both reported from play: the card displayed **`Hurdling kit`** on a band
raising a Cultivate, and it *"is still forcing me to select 1 kit"* on a band with nothing queued.

**The rule above was right and the FALL-THROUGH under it was not.** With nothing composed, no kit on
the wire (an unstaffed `builders` row publishes none) and no queue head to derive from,
`_role_kit_id` handed `KitRoster.resolve_selection` an empty selection — and that function's terminal
answer is **`selectable[0]`, i.e. ROSTER ORDER**, which `equipment.json` authors as `hurdling`,
`tillage`, `none`. So the card opened on the ANIMAL web's kit and presented it as a decision the
player had made, on every band with an empty queue — and on any band whose head the client cannot
resolve, which is the plant-head report's own shape.

**`KitRoster.bare_kit_id(kits, job)` is the honest answer to *nothing is chosen and nothing can be
derived***: the roster's own entry that carries nothing (`kit_supplies_any` false — the derived
reading of the null kit, which is why nothing spells the id `none`). It stays selectable throughout,
sending the pool out bare being how a player conserves gear.

**IT IS THE `builders` ROW'S ALONE.** Every other role publishes either a stored kit or a job
default, so roster order is unreachable for them; this is the one row where "nothing is derivable" is
a real and common state. `_commanded_role_kit_id` is untouched — the stepper still sends the player's
own pick or nothing, never the derived id.

**WORST CASE THE CARD NOW READS `No kit`, WHICH IS A GAP RATHER THAN A LIE.** A band whose queue head
the client cannot resolve — `head_build_branch` needs an entry at index 0 of the band's own
`buildQueue` — still derives nothing, and the card says so instead of naming a web.
> **It stopped needing a source lookup at all** (§4.9 item 9a). It used to walk the band's rows,
> resolve each one's dict out of `forage_patch_lookup()` / `world_herds()` and ask the SOURCE whether
> it was at position 0 — which asked the *winning* band. The head entry now names its own web in its
> `kind`, so the branch comes off the entry and the lookups are gone with the wrong-band read.

**Asserted PNG-LESS, by EQUALITY, over THREE states** (`_assert_builders_card_kit_faces`): a plant
head, an animal head and the EMPTY queue, because a resolver stuck on one web satisfies any one of
them. Sabotage-verified by dropping the bare-kit fall-through — exactly the empty-queue claim fails,
naming `🎒 Hurdling kit`, i.e. the reported defect in its own words.

## THE BUILD QUEUE BLOCK — the band's own list, above the board's chips (§4.6b)

The band's ordered build queue had **no surface at all**: a player could not see what was queued, in
what order, or which entry the builders were funding. It is a block in the WORK zone, between
`_build_work_head` and `_build_work_chips`.

```
BUILD QUEUE                          3 builders · Tillage kit
▸ 🌱 Cultivate (71, 18)    Cultivating 0% · turn 82            ✕
  ◎ Tame Red Deer               Taming 0% · turn 101           ✕
  ▦ Sow (72, 18)          ⚠ ∞ turns, losing ground…            ✕
```

- **ABOVE THE CHIPS, DELIBERATELY.** The chips filter the BOARD; the queue is the band's own list
  rather than a view of that board, so a block beneath them would read as a filtered subset of it —
  which is also why `_build_queue_models` derives from the FULL model set and never from `filtered`.
- **NO QUEUE MEANS NO BLOCK AT ALL** — zero nodes, zero height, zero chrome. That is the common
  early-game state and it must cost nothing; an empty header or a hint there would be permanent
  furniture explaining an absence.
- ⛔ **IT DID NEED A BAND-SIDE QUEUE ON THE WIRE, AND THIS BLOCK SHIPPED A SLICE WITHOUT ONE.** The
  membership argument held **for the common case** — the sim keeps an entry only while its source
  holds a labor assignment (`LaborAllocation::prune_build_queue` → `holds_build_source`),
  `_work_source_models` admits any source with a take crew, and a source queued by ANOTHER band is
  not in this band's `effective_worker_map` at all. (It is not airtight: a row and a crew are not the
  same thing — "AN ENTRY CAN OUTLIVE ITS ROW'S CREW" below.) **What it did not buy was the ORDER.** `buildQueuePosition` is
  source-addressed and rides the winning band, so a source **both** bands hold publishes the other
  band's rank, and the block sorted on it. The block reads
  `HudBandLaborState.build_queue_keys(band)` — the band's own `buildQueue`, order and all — and
  joins each key to its model; `build_turns` and the rest still come off the source. See "THE ORDER
  IS THE BAND'S OWN" below for the defect and the shape that answers it.
- **THE HEAD MARKER'S SLOT IS RESERVED ON EVERY ROW.** A conditionally-omitted Label shifts every row
  behind the head sideways, which reads as a list that has lost its alignment rather than as a head.
- **THE ROW IS THE BOARD'S OWN UNIT** — exactly `WORK_ROW_HEIGHT` and `HudStyle.work_row_stylebox` —
  so the two lists read at one density and the capacity arithmetic below still divides by that number.
- **THE VERB IS `HudFormat.policy_face`'s, never a second table.** That is the same word and glyph
  the board row's in-progress axis states in its tooltip and the map badge draws, so a rung under way
  cannot be called two things on one screen.
- **THE SOURCE MARK IS GONE FROM THIS ROW, and the face is why it could go.** It was
  `HudWidgets.build_marker_icon` in its own 16px column beside the marker; the face already names the
  source in words AND leads with the destination's rung glyph, so the icon carried nothing the row did
  not. What bought its 20px is in "THE PERCENTAGE IS THE LEG IN FLIGHT'S" below. **The board's rows
  keep theirs** — they are a list of SOURCES whose names lead with no glyph at all.
- **THE DATE GOES THROUGH `DetailFormat.build_completion_value`**, which shares `build_sentinel_value`
  with `rung_row_value`'s countdown — one fork for a positive count, `-2` holding, `-3` rotting, `-4`
  the queue blocked, `-1` no answer. A second fork is precisely how this client has twice been left
  behind by a newly-spelled sentinel. Its ink is `DetailFormat.rung_value_color`, the `Color` twin of
  `rung_value_hex` written in terms of it (a `Label` can do nothing with a hex string).
- **THE DATE COLUMN CLIPS AND THE ROW TOOLTIP CARRIES BOTH FACES IN FULL.**
  `RUNG_BLOCKED_FORMAT` is a whole sentence, and letting it size the row would squeeze the job face
  to nothing on a side dock.
- **THE `✕` ASKS NOTHING.** `unqueue` withdraws a DECLARATION — the banked meter survives it, the row
  keeps its crew and its kit, and re-declaring is one tick of the compose control — so it is the
  parties zone's cancel-versus-recall rule read one surface over. It wears that zone's steady,
  full-opacity `DANGER` treatment for the same reason: a destructive control reads as one.
- **IT EMITS THE CONTROLLER'S OWN `unqueue_requested`, RELAYED by `HudLayer`**, with a payload
  identical key-for-key to `DrawerComposeController`'s — so `Main.format_unqueue` serves both
  surfaces and there is no second command builder.
- **THE ROW IS DRAGGED TO REORDER, AND ITS SETTINGS HOLD THE PER-ENTRY KIT** — §4.7b, below.

### THE DATE IS A COMPLETION TURN, NOT A CHAINED COUNTDOWN (§4.7)

The block used to print `≈42 turns` / `≈61 turns` / `≈98 turns` down the list. Reported from play: those
read as *each job's own span* when they are cumulative — the second entry's 61 already contains the
first's 42 — so the list looked like three independent estimates that happened to rise. It states the
turn each entry is estimated to **complete on** instead: `turn 82 (0%)` · `turn 101 (0%)`. A schedule
answers *when*, and an absolute turn cannot be misread as a duration.

- **ONE SENTINEL FORK SERVES BOTH SPELLINGS.** `DetailFormat.build_sentinel_value` holds the whole of
  the `-1` / `-2` / `-3` / `-4` answer and returns `""` for a real count; `build_countdown_value` and
  `build_completion_value` are each three lines written in terms of it. **A second fork here is exactly
  how this client has twice been left behind by a newly-spelled sentinel**, which is the reason the
  countdown half was extracted in the first place — adding a spelling must not re-open it.
- **ONLY THE QUEUE TAKES THE ABSOLUTE FORM, and that is deliberate.** The queue is a *schedule* whose
  order is the player's own input; a rung row and the compose sheet answer *what does this cost me*,
  which is a duration. Same number, two questions, and neither is a second producer of the other.
- **No singular case.** `turn 41 (0%)` is correct at one turn, so `BUILD_TURNS_SINGULAR` has no twin
  here.
- **The row tooltip carries BOTH readings** — `turn 82 (0%) · 42 turns from now` — which is what
  actually removes the ambiguity rather than trading one half of it for the other.
- Asserted by EQUALITY against a fixture whose current turn AND `buildTurnsRemaining` are both known, so
  the claim is the ADDITION and not that a number rendered; the dates are asserted STRICTLY ASCENDING,
  and a SENTINEL row rides the same frame — a builder that rendered every value as a turn number would
  pass any all-positive fixture. Sabotage-verified by dropping `current_turn +`: exactly one assertion
  fails, naming `["turn 42 (0%)", …] (want ["turn 82 (0%)", …])`.

### THE PERCENTAGE IS THE LEG IN FLIGHT'S, NOT THE DESTINATION'S (§2.8)

Reported from play. A `Sow` ordered on untended ground is a **two-leg** entry — the tended rung
`0 → 50`, then the field rung `50 → 125` — and with nine work banked the tile card correctly read
`Cultivation ≈39 turns (18%)` while the Work tab read **`0%` in two places**: the queue row's date
column (`▸ ▦ Sow (28, 19)   turn 83 (0%)`) and the source row's rung chip (`▦0%`). Both were quoting
the DESTINATION's own meter, which is honestly zero for as long as the crew is still clearing. Neither
number was wrong; they answered a question nobody asked and they contradicted the card.

> **A progress number must never sit at zero while work is going in.** A player watching `0%` for
> thirty-nine turns concludes the job is stuck.

**Both Work-tab readouts state the leg**, and the row loses nothing by it:

| what | reads |
|---|---|
| the row's TITLE | the DESTINATION — `▦ Sow (28, 19)`, what the player ordered |
| the date | the WHOLE CLIMB's completion turn — unchanged |
| the percentage **and its verb** | the leg in flight — `Cultivating 18% · turn 83` |
| the source row's rung chip | the same leg — `🌱18%`, so the two surfaces wear one mark |

- **ONE RE-POINTING FEEDS BOTH**, `RungGates.leg_in_progress` — which takes `rung_in_progress`'s
  ALREADY-RESOLVED answer rather than producing a second one (`build_is_stalled`'s discipline) and
  swaps its rung for the first published leg still owing work. `_work_source_models` calls it once and
  the chip, the chip's hover, `build_stalled` and the queue row's percent-and-verb all read that trio.
- **THE FRACTION IS THE WIRE'S OWN PER-RUNG ONE**, `improvement_progress` at the leg's verb — the
  source's single position clamped into that rung's span, sim-side (`forage::patch_rung_work_done`),
  and **the very number the tile card renders**. Nothing divides `work_remaining` by anything: a second
  derivation of a number the sim publishes is how this arc has shipped defects before.
- **THE LEG IS SCANNED, not `legs[0]`.** The sim drops a rung the position has already paid for, so on
  an honest payload the head IS the leg — but a fixture, or a producer that one day publishes the whole
  branch, can carry a paid leg and a reader taking the head on faith would name a rung nobody works.
- **THE MAP BADGE TAKES THE SAME RE-POINTING**, and that is not scope creep: the plate and the work row
  are held to ONE verdict by `band_panel_preview._assert_work_row_and_badge_agree`, so a leg-aware board
  beside a destination-bound badge is exactly the two-surface disagreement `build_is_stalled` exists to
  stop.
- **THE ENTRY'S PRICE STAYS ON THE DESTINATION.** `build_work_cost` / `build_upkeep_demand` are
  composed off the declared rung, held in a local before the re-point: the row's hover quotes the whole
  climb's bill, and a leg-following price would understate a two-leg job on the one surface that states
  it.
- **THE SENTINELS TAKE THE LEG'S PERCENT AND NO VERB.** Each names a state of the ENTRY — blocked,
  stalled, holding, rotting — which is a fact about the climb rather than about one leg, and a hazard
  face carrying a participle too would put two subjects in one 168px column.

**THE VERB IS WHAT COST THE ROW ITS SOURCE ICON, and the arithmetic is worth writing down** because
the next control added to this row will face the same wall. Measured at the tall LEFT dock: the row's
content line is **338px**, of which the marker, the icon, the `✕` and four separations took 64, leaving
**274** for the two columns that carry information. The face's widest shipped string needs **123**
(`🌱 Cultivate (72, 18)` — asserted UNCLIPPED since a play report), and the date's widest needs **168**
(`Cultivating 100% · turn 999`). 291 into 274 does not go, and **neither column could give**: the face
is a shipped guarantee, and what a clip takes off the end of the date string is the DATE. The icon was
the only slot with no informational duty, and dropping it makes 294. `band_panel_preview.
_report_queue_row_columns` PRINTS both columns and both worst cases rather than asserting them — what a
red line there asks for is a decision, not a failing run.

- **`HudComposeVocab.IMPROVEMENT_RUNNING_LABELS["corral"]` went `Building the pen` → `Penning`** in the
  same measurement. It was the one phrase among four single words, it is the craft's own name, and at
  203px it would have set this column's reservation on its own.
- **The date column is `168.0`, measured and not guessed**, and under-sizing it is not cosmetic here.

Asserted in `band_panel_preview` on FOUR states — the reported two-leg sow (rendered as
`band_panel_queue_legs`), the same board with **one** work unit banked (the turn the defect starts on,
which a claim at 60% cannot reach), a single-leg sow on already-tended ground that must be UNCHANGED,
and the animal twin (`band_panel_queue_leg_animal`, a `corral` on an untamed herd). Each makes four
claims off the SHIPPED formats — the title still names the destination, the date column by EQUALITY
(which pins the verb, the percent and the whole-climb date in one string), and the source row's chip —
and the wanted percent is composed through the tile card's own producer rather than as a literal, so
the claim is that the two surfaces AGREE. Sabotage-verified by returning the destination reading:
exactly **six** fail (both readouts × the two-leg sow, the first turn and the animal twin) while the
single-leg control stays green, printing the played `Sowing 0% · turn 64` and `▦0%`.

### …AND A FENCE RING'S PERCENTAGE IS ITS OWN METER, BECAUSE IT CLIMBS NOTHING

Reported from play. `🐄 Corral Wild Fowl   turn 151 (0%)`, and the percentage never moved for the
ring's whole life. A pen-extension ring is the one queue entry the leg re-pointing above cannot reach:
`BuildJob::ExtendPen`'s destination is the pen rung it **widens**, so there is no leg to climb, only
more of the one the source is already on. The herd reads `Corralled 100%`, `build_verb` answers no rung
in flight, `RungGates.rung_in_progress` is empty, and the ladder credit the column quotes is therefore
**structurally zero for every ring, always** — a plausible-looking row with a number that can only be
zero.

| what | reads |
|---|---|
| the row's TITLE | `Corral <herd>` — a ring derives the verb of the rung it widens, and is UNCHANGED |
| the date | the sim's own ring countdown (`published_ring_turns`) — always correct, and UNCHANGED |
| the percentage | the RING's meter: `SourceForecast.pen_extend_fraction`, in work units |

- **THE RING IS IDENTIFIED FROM WHAT IS ALREADY ON THE WIRE** — `SourceForecast.BUILD_JOB_EXTEND_PEN`,
  the `extend_pen` token `snapshot::population::resolved_build_job` publishes in the row's
  `improvement` slot because a built pen carries no meter for a rung verb to name. No wire field was
  added for it, and none is needed.
- **THE MODEL CARRIES THE QUOTIENT, NOT THE ROW** (`build_ring_progress`), resolved in
  `_work_source_models` where the live herd dict is already in hand — beside the six other build fields
  and for their reason. `PEN_EXTEND_EMPTY_METER` on every entry that is not a ring: the field is
  meaningful only under the branch `improvement` selects.
- ⛔ **IT WAS THE SAME DIVISION THE HERD DRAWER'S BADGE QUOTED, AND THIS ROW IS THE LAST READER LEFT**
  (`docs/plan_standing_upkeep.md` §4.9 item 12c). The dead claim: *"The queue row and the `Fencing N%`
  pill read one ring through one helper, so they cannot state it two ways."* The pill retired with the
  tile card's `Extend pen` button, being a third statement of a meter this row already dates and
  withdraws — so `pen_extend_fraction` now has exactly one caller and the drift it was written against
  is structural rather than disciplinary. See `labor-ui.md` → "The fence ring's meter is a work pair"
  for the units and the zero-denominator rule.

Asserted in `band_panel_preview` as `band_panel_queue_ring` on three claims, the first being the
precondition without which the other two pass for free: that the pen rung is FULL with no rung in
flight (so the ladder genuinely has nothing to credit), that the face still derives the widened rung's
verb, and the date column by EQUALITY — which pins the untouched countdown and the ring's own
percentage in one string. The fixture's pair (30 of 40 work → 75%) deliberately differs from the herd
drawer's (42 of 70 → 60%), so a row quoting the wrong ring fails on the number rather than coinciding
with it.

### A ROW EXPANDS INTO THE JOB'S SETTINGS, AND THE ROW ITSELF TAKES NO SIXTH COLUMN (§4.7a ③)

The crop moved off the compose sheet — *"the CROP TO TEND shouldn't be a selection here as the user can't
do the cultivate here"* — and its first home was a sixth column on this row. **At the narrow shell's
354px that made the row unreadable**: `▸ 🌱 Cultiv…  [Sim pic ⌄]  turn 82 (0%)  ✕`, with both the job
face and the crop name ellipsised into fragments that look like words. The LEFT dock is the shipped
default edge, and a tooltip is not a fix for a list the player SCANS.

So the settings live in a **row expansion** — the WORK board's own inspector pattern, one strip beneath
the clicked row, one open at a time. The entry row goes back to marker · face · date · `✕` (it was
marker · mark · face · date · `✕` until the date column learned a verb — see above).

- **ONE PREDICATE DECIDES BOTH THE INVITATION AND THE CONTENTS** (`_queue_crop_choices`), so a row can
  never offer a click that opens an empty strip. An animal entry has no crop, is not expandable, and
  connects no `gui_input` at all — taming has no crop to commit.
- **THE STRIP IS WHAT MAKES §4.7a ② POSSIBLE**, and the comment says so: the per-entry kit picker lands
  beside the crop here. A row that could not hold five columns was never going to hold six.
- **The height is open-only** and rides the same one expression
  (`build_queue_block_height(entries, rows_max, settings_open)`) that `_work_board_capacity` charges. An
  open strip does not overflow the zone — the BOARD absorbs it and pages one row shorter. **It is the
  LAST expansion in this zone that makes that trade**: §4.9 item 12d took the work inspector's twin
  term out, and the headroom the board needs for THIS strip is now `BUILD_QUEUE_ROOM_SETTINGS_HEIGHT`,
  reserved by `build_queue_rows_max` where the inspector's own reservation had been covering it by
  accident.
- **The face's width is asserted as a MEASUREMENT, not a look** — its laid-out width against the font's
  own width for its string — because that is the one thing that moves when a control takes width off the
  row's single expanding child.
- **The pair is the claim**, and it silently skipped once: the fixture carried no animal entry, so the
  positive half passed alone. A missing animal row now `_fail`s outright rather than skipping, and the
  needle matches `HudFormat.policy_face` (the compose sheet's *"Tame this herd"* label appears nowhere on
  a queue row, whose face reads `◎ Tame Red Deer`).

### THE QUEUE'S OWN CONTROLS — the kit, the drag, the withdrawal (§4.7b)

`docs/plan_standing_upkeep.md` §4.7b. The block could be READ but barely operated: the kit override
had no home at all, the order was `build_order` on the command line, and withdrawing a confirmed entry
did not leave the block until the turn resolved. All three land on the row that already exists.

#### ② THE KIT PICKER, IN THE SETTINGS STRIP, FLOWING

- **EVERY QUEUED ENTRY HAS A KIT, so every queue row expands now** — the hunt/tame rows that carried
  `legs == 0, crop == false` included. That is not a widening of the *never invite a click that opens
  nothing* rule; it is that rule reaching its second setting. `_queue_settings_content` returns
  `{legs, crop, kit}` and the crop stays the plant web's alone.
- **THE WRAP IS COMPUTED, NEVER DISCOVERED.** `HudWorkVocab.queue_settings_one_line(line_width)` is a
  WIDTH PREDICATE both sides read: the strip's `custom_minimum_size` and `_work_board_capacity`'s
  chrome term go through the one `build_queue_settings_height(legs, has_crop, has_kit, one_line)`. A
  Godot flow container that wrapped at LAYOUT time could not tell the reservation how many lines it
  drew, and this zone takes the difference off the bottom of the board in silence. **Neither picker
  ever shrinks** — the widths are fixed (`BUILD_QUEUE_CROP_WIDTH` / `BUILD_QUEUE_KIT_WIDTH`, both 168)
  and the LINE COUNT is what gives. `BUILD_QUEUE_SETTINGS_KEY_WIDTH` is shared by `CROP` and `KIT` so
  the two keys share a left edge when they stack.
- **A LONE CONTROL IS ALWAYS ONE LINE**, whatever the width: an ANIMAL entry has a kit and no crop, so
  it has nothing to wrap against and the predicate must not answer for it.
- ⛔ **NO SHIPPED DOCK REACHES THE ONE-LINE SIDE TODAY, and the numbers are the point.** The work zone
  is one board column wide at every dock the panel ships with — **342px of strip on the tall LEFT dock,
  368 on the 1920 BOTTOM one, against the 408 the pair needs.** One line arrives when the board earns a
  SECOND column, which `_affordable_work_columns` needs ~760px of card span for and a 1920 wide shell
  does not have once both flanks are paid for. So the flow is asserted where it is DECIDED
  (`band_panel_preview._assert_queue_settings_predicate`, both sides of the threshold plus the height
  that follows it) and REPORTED at each dock — which is what makes *"which layouts get one line"* a
  number rather than a look.
- **IT IS A FIXED-HEIGHT PICKER, NOT `KitRoster.build_kit_row`.** That helper returns a two-child
  block whose second child — the `tier_hint` line — is present only when the selected kit has
  something to say, so the row it draws is 22px or ~36px depending on the PICK, and this strip reserves
  its height before it draws it. The LIST is still the roster's own: `KitRoster.kit_entries` was
  extracted out of `build_kit_row` so both hosts share the roster order, the `(default)` mark and the
  greying of a kit that serves the other web, and only the chrome differs.
- **THE `(default)` MARK IS THE DERIVATION, PER ENTRY** — `KitRoster.build_kit_for_branch` off the
  entry's own web (`build_branch_for_kind`), which is the answer the sim resolves when the entry
  reaches the head. **The selection shown is the wire's `build_kit_id`**, the RESOLVED kit, falling
  back to the derivation for an entry the wire has not placed.
- **PICKING THE DERIVED DEFAULT EMITS NO `kit` TOKEN, and that is what CLEARS the override.**
  `Main._kit_token`'s standing rule — omit the token when the selection equals the default — is
  exactly right here, so there is no `default` literal to invent; `none` is bare-handed and is a real
  selection. **No optimistic overlay**: `buildKitId` is captured LIVE, so the recapture the command
  triggers already carries the pick.

#### ③ THE REORDER IS TWO ARROWS PLUS A DRAG — and neither costs the row a pixel

The row's width is spoken for: marker 10 + face + date 168 + a 32px trailing column + four
separations, and the face is **already ellipsised** at its widest shipped value
(`🐄 Corral Thunder Mammoths` needs 189 of the 126 it gets, measured by
`band_panel_preview._report_queue_row_columns`). Any new column comes straight out of the one column
carrying an unclipped-name guarantee, so the whole design question was *which existing slot gives*.
Two of them did, and the arithmetic is what made the placement decidable rather than a preference.

**THE ARROWS ARE THE PRIMARY REORDER, AND THEY TOOK THE `✕`'s COLUMN.** Ray, from play, once the drag
worked at all: a grab handle that only reveals itself under a press is not a control a player finds.
Four placements were prototyped; the one that ships is the only one costing **zero pixels** while
still giving full-height targets — `▲` then `▼` side by side inside
`BUILD_QUEUE_REORDER_WIDTH`, which is `BUILD_QUEUE_UNQUEUE_WIDTH` **stated as arithmetic** rather
than re-typed as a second 32. The split is `(32 − 2) / 2 = 15` each with
`BUILD_QUEUE_REORDER_SEPARATION` between them, and both fill the row's content line (24px inside a
28px row) — a *stacked* pair would have made two ~12px targets, which is the placement this one beat.
Verified as a measurement: `band_panel_preview._assert_queue_reorder_arrows` prints the column at
**32 of 32** and the zone-width guard still reads **354 of 356**, unchanged from before the arrows.

- ⛔ **15px IS UNDER THE GHOST BUTTON'S OWN CHROME, so the arrows trim their SIDES.**
  `HudStyle`'s `BUTTON_PADDING_H` is 11 each — that is why a 9px `✕` needed a 32px column — and two of
  those inside 15 leaves the glyph nothing. `HudWidgets.compact` grew a fourth parameter,
  `padding_h`, defaulting to `KEEP_BUTTON_PADDING_H` (negative, because zero is a real trim a caller
  may want); the arrows are its **one** caller. Everything else keeps `HudStyle`'s side margins, the
  standing rule being that a zone row is short on height and not on width.
- **SAME COMMAND, NOTHING NEW ON THE WIRE.** `▲` sends `build_order … <rank − 1>` and `▼`
  `<rank + 1>`, through the same `_emit_build_order` and the same `build_order_requested` payload the
  drag uses. `build_order`'s semantics are *remove, then insert at* — so one step swaps an entry with
  its neighbour and needs no index of its own (`BUILD_QUEUE_REORDER_STEP`).
- **DISABLED AT THE ENDS, NEVER ABSENT.** `▲` is dead on the head, `▼` on the **last confirmed
  entry** — the wire queue's length, not the drawn page's, since an entry at the bottom of a truncated
  page still has somewhere to fall to. A control that vanished at either end would shift the column
  under the row beside it.
- **A PENDING ROW GETS NEITHER, and keeps the column reserved.** Same rule the drag handle already
  followed: the wire has not placed it, so there is no rank for `build_order` to name.
- ⛔ **AND NO OPTIMISTIC OVERLAY, for the reason spelled out at the end of this section.** The arrows
  inherit the drag's rule because they inherit the drag's command.

**THE DRAG SURVIVES BESIDE THEM** — Ray: *"the drag and drop can stay if it is easy to just use the
code already shipped."* It works now and it costs the row nothing, so the handle stays exactly where
it was. The marker slot is reserved on every row already (that is what lines the faces up) and holds
nothing on a non-head row.

- **The head keeps `▸`** — that marker is load-bearing, it names the entry the pool is funding — and is
  still a handle, demotion being the most likely reorder there is. Every other confirmed row draws
  `BUILD_QUEUE_DRAG_HANDLE` in the quiet ink. **A PENDING row is not draggable**: the wire has not
  placed it, so there is no position to name and nothing to move it above.
- **`Control.set_drag_forwarding`, so no per-row script exists.** The grab is forwarded off the marker
  and the drop off the ROW (a 10px column is not a drop target a player should have to aim at twice).
  The handle takes `MOUSE_FILTER_PASS`, so a plain click still reaches the row's click-to-open —
  Godot only asks for drag data past a movement threshold.
- ⛔ **AND THE ROW'S CLICK-TO-OPEN FIRES ON THE RELEASE, BECAUSE THE PRESS IS WHERE A DRAG BEGINS.**
  This shipped on the press and the reorder was therefore **dead in the real client**: the press
  reached the row through the `PASS` handle, `_toggle_queue_settings` → `_repage_work_zone` freed every
  node in the zone, `_gui_remove_control` nulled the Viewport's `mouse_focus` — which was the marker —
  and by the time the pointer crossed the drag threshold there was nothing left to ask for drag data.
  The gesture degraded to *a click that opens the settings strip*, which is exactly how it played. The
  release is also the event a completed drag CONSUMES (the Viewport performs the drop on the button-up
  and never forwards it to `gui_input`), so a reorder cannot also open the row it moved; and it must
  land INSIDE the row, because `mouse_focus` latches on the press and a release three rows away would
  otherwise still toggle this one. **Any press handler that rebuilds its own subtree kills every drag
  that could start under it** — the general form of the rule.
- **THE DROP INDICATOR COSTS NO HEIGHT.** The block's `separation` is 0, so an indicator drawn
  *between* two rows would need a new term in `build_queue_block_height` on the reservation side as
  well as the render side. `HudStyle.work_row_drop_stylebox` lights one edge of the target row's OWN
  stylebox instead, swapped on nodes the controller holds (`_queue_row_nodes`) rather than by
  re-rendering the block the gesture is standing on.
- ⛔ **A SNAPSHOT MID-GESTURE KILLS THE DRAG, so the Work zone does not rebuild while one is in
  flight.** `Main._apply_snapshot` → `update_band_alerts` → `refresh_snapshot()` → `render_band()`
  rebuilds all three zones and `populations` / `herds` move on essentially every turn; freeing the row
  the pointer holds ends the gesture on the first pixel of movement, the mechanism
  `DrawerComposeController` already documents for the floor drag. `_queue_drag_in_flight()` is read at
  BOTH doors — `_repage_work_zone` (which is also what a window resize comes through) and
  `render_band` — and `_on_queue_drag_end` re-renders once.
  - **The cancel is why `QueueDragWatcher` exists.** Godot announces the end of a drag —
    dropped OR cancelled — as `NOTIFICATION_DRAG_END` to every `Control` and by no other means, and
    this controller is a `RefCounted`. A suppression lifted only by a successful drop freezes the block
    for good.
- **THE QUEUE IS THE RANK.** The drop sends `build_order <faction> <band> <source…> <position>`,
  0-based — §4.9's priority property stored in the queue itself, so this client keeps no rank beside
  it. `build_order` is the one of the three verbs that names a BAND; a kit and a withdrawal are
  properties of an entry every band holding that source shares.
- ⛔ **AND THE POSITION IS AN INDEX INTO THE BAND'S WIRE QUEUE — NOT INTO THE ROWS THE BLOCK DREW.**
  Both the arrows and the drag count in `_queue_rank_keys` (`HudBandLaborState.build_queue_keys`),
  and `_build_queue_models` stamps each drawn row's rank from the same walk. The two lists are not
  the same length: see "AN ENTRY CAN OUTLIVE ITS ROW'S CREW" below for the reachable, persistent
  state in which the wire carries an entry the block cannot draw. Counting in the drawn list instead
  makes every position below a hidden entry short by the number hidden, which the sim resolves
  against its own full queue — a `▼` that silently does nothing, or a row that jumps above something
  the player cannot see.
- ⛔ **THERE IS NO OPTIMISTIC ORDERING, AND THERE MUST NOT BE ONE** (§4.9 item 9a). This slice shipped
  one — `_queue_sort_rank` over a turn-keyed `order` overlay — because `buildQueuePosition` is
  turn-written and the list snapped back on the command's own recapture. The band-side `buildQueue`
  is captured **live** off `LaborAllocation::build_queue`, and `build_order` mutates that allocation
  at command time, so the new order arrives on that same recapture and there is nothing to paper
  over. A client-side ordering beside it would be the **second rank** §4.9 forbids, and the wire
  field's own doc comment says so. `_queue_sort_rank`, `pending_order_for` / `record_pending_order` /
  `drop_pending_order` and `Main`'s rollback branch are all **deleted**; the send and its failure path
  stay, with nothing to roll back.

#### ④ THE WITHDRAWAL MOVED INTO THE SETTINGS STRIP, and still leaves the block on the frame it is pressed

**THE `✕` IS THE SLOT THAT GAVE, because it had somewhere to go.** The arrows above needed 32px and
the row had none spare; every queued entry expands into a settings strip (§4.7a ② — every entry has a
KIT), so the withdrawal moved into that strip, **right-aligned on its LAST control line**. Ray, on the
trade: *"two-click withdrawal is acceptable."* It is the right way round — a reorder is the commoner
act and is one click; a withdrawal is rarer and is now two.

⛔ **IT MUST NOT ADD A LINE, AND THE PREDICATE IS WHAT PAYS FOR THAT.** The strip already stacks to
two lines on every shipped dock and the exclusion rule below leaves that zone reading **396 of 396**,
so a third line would come off the bottom of a clipping board in silence.
`HudWorkVocab.queue_settings_one_line_width()` — the one expression both the reservation and the
builder read, now exported so nothing re-spells it — grew the button's own width and one separation:

| | pickers + keys | `✕` + gap | one line needs | tall LEFT dock has | 1920 BOTTOM has |
|---|---|---|---|---|---|
| before | 408 | — | **408** | 342 | 368 |
| after | 408 | 4 + 32 | **444** | 342 | 368 |

Both shipped docks were already two lines and both still are, so
`build_queue_settings_height` comes out at the same **56px drawn against 56 reserved** it did before
(`_assert_queue_settings_flow` prints the pair). What the term buys is the case that has not arrived:
the moment a board earns a second column and the strip is wide enough for both pickers, the button is
paid for on that line instead of being squeezed off the right edge.

- **`build_queue_settings_height` grew ONE branch with it** — a strip with legs and no pickers at all
  now charges a control line, because the `✕` needs one to ride. Nothing the sim publishes reaches it
  (every queued entry has a kit), and the builder takes the same branch, so a strip cannot draw a
  button on a line it was not paid for.
- **THE BUTTON ITSELF IS UNCHANGED** — same glyph, same DANGER ink, same `BUILD_QUEUE_UNQUEUE_META`
  valued the entry's rank, same `_emit_unqueue` and the same optimistic withdrawal below. **Only its
  host moved**, which is why every harness that found it by that meta finds it in the strip.

⛔ **IT IS KEYED ON THE TURN, NOT ON THE NEXT SNAPSHOT.** The server re-captures and broadcasts after
**every** command, so a "hide it until the next snapshot" rule flickers the row straight back a frame
later. `reconcile_pending` already keys additions on *a snapshot with a NEWER turn*, and the
withdrawal set lives in the same per-band record (beside `assign` / `move`) so it takes that rule and
`_prune_pending_entity` with no second lifecycle.

> **THE REASON NARROWED WHEN THE QUEUE WENT PER-BAND** (§4.9 item 9a). It was *"that capture still
> carries the stale turn-written `buildQueuePosition`"*, and for the block's membership that is no
> longer true: `buildQueue` is live, so `unqueue` drops the entry on the command's own recapture. What
> survives is the **press→reply round trip** — the frames between the `✕` and the server's answer,
> which no wire field can cover — and the rollback for a send that never went. The overlay's OTHER
> job is untouched and was never about the queue: blanking the effective improvement is what returns
> the *work row* to its `⌃` offer face on the same frame.

- **IT CLEARS THE IMPROVEMENT; IT DOES NOT DROP THE PENDING RECORD.** `unqueue` withdraws a
  DECLARATION and leaves the take crew standing — and the same record may hold a pending CREW edit,
  which dropping it would discard. `effective_worker_map` blanks the effective improvement for the
  withdrawn key instead, which is what puts the source's work row back to its `⌃` offer face on the
  same frame, off the one map every readout shares.
- **The payload carries `pending_entity` and `kind`, neither of them a command token.** (The
  WITHDRAWAL's payload — `build_order_requested`'s lost both with its overlay and is now
  `{faction, band_id, x, y, herd_id, position}`, every key a command token.) `Hud` records
  the withdrawal BEFORE emitting (this layer's standing rollback precondition — `Main` handles the
  signal synchronously) and `Main._on_hud_unqueue` hands the payload back to `drop_pending_unqueue`
  when the send does not go, exactly as `_on_hud_assign_labor` does.

#### ⛔ ONE EXPANSION OPEN AT A TIME IN THE WORK ZONE — and it was a live defect

Open a queue row's settings **and** a work row's inspector on a one-column BOTTOM dock — one click
each, both shipped — and `Zone_work` drew **460 into a 396px box (over by 64)**, with the board already
at its `maxi(1, …)` floor and nothing left to give back. **No frame caught it because every
strip-open frame had no inspector and every inspector-open frame had no strip** — two disjoint frame
families, the defect living in the gap, which is the same shape §4.7 found in the inspector's own
height.

`_queue_open_key` and `_work_open_key` are MUTUALLY EXCLUSIVE now: opening either closes the other. It
is the rule both lists already followed internally, read one level up, and it costs nothing. With it
the same dock reads **396 of 396 — zero spare, and no overflow**. `band_panel_queue_settings_exclusive`
is the frame, asserting the exclusion BOTH ways (a builder that never opens the strip would pass one
half) beside `_assert_zone_content_fits`; sabotage-verified by disabling the exclusion, which prints
the 460 above.

**⛔ THE ARITHMETIC HALF OF THAT RULE RETIRED WITH §4.9 item 12d, and the rule is kept for its reading
half.** The 460-into-396 measurement above is HISTORY: the inspector is a viewport-centred dialog and
takes no zone height at all, so the two expansions can no longer overflow anything together. What
survives is that a board row and a queue row are two different subjects, and expanding both states
neither clearly.

#### TWO CONSTANTS THAT READ LOWER THAN THEY DRAW, corrected here

- **`BUILD_QUEUE_UNQUEUE_WIDTH` 22 → 32.** `HudWidgets.compact` squeezes the type size and the
  VERTICAL padding — that is what keeps a control inside a 28px row — and leaves the ghost button its
  horizontal margins, so the reservation was 10px under what the `✕` draws and the row's expanding
  face paid the difference. **That 32 is now the reorder pair's column** (`BUILD_QUEUE_REORDER_WIDTH`
  is defined from it), and the same left-alone side padding is why the arrows are the one caller that
  trims theirs.
- **`BUILD_QUEUE_SETTINGS_HEIGHT` 30 → 34** — a 22px compact picker plus the strip's own 12px of
  `HudStyle.ROLE_CARD_PADDING` (it wears `work_inspector_stylebox`, which is the role card's). A live
  4px under-reserve every time a strip opened, and correcting it is what makes the flow arithmetic
  honest: a term wrong by four is wrong by eight the moment the strip wraps.

**Frames:** `band_panel_queue_settings_stacked` (the tall LEFT dock, both pickers, keys aligned) ·
`band_panel_queue_settings_wide` (the 1920 BOTTOM dock, which stacks too — the measurement that says so
is printed) · `band_panel_queue_settings_exclusive` · `band_panel_queue_drag` (a drag in flight, with
the indicator on the target row and the block asserted NOT to have rebuilt under it) ·
`band_panel_queue_withdrawn` (the row gone, and STILL gone across a re-push of the same fixture on the
same turn — the command's own recapture; the `✕` is pressed there through the STRIP, with a real
`push_input` click).

⛔ **THE ARROWS AND THE STRIP'S `✕` ARE DRIVEN WITH REAL INPUT, NEVER `pressed.emit()`**
(`_assert_queue_reorder_arrows`, `_assert_queue_arrow_click`, `_render_queue_withdrawal_state`). An
emitted signal cannot see a button that is covered, zero-size, disabled or filtered out of the hit
test, and these are three brand-new controls in a row with two pixels of slack and a strip whose
height is reserved rather than measured — the exact shape in which the drag gesture shipped completely
dead and completely green. `_drive_click` pushes the player's own events through the viewport and lets
Godot decide whether anything was pressed; sabotage-verified by disabling the `✕`, which the emitted
form would have pressed anyway.

**`build_kit` and `build_order` are driven in `command_guard`**, both source forms each, because a
well-formed line that means the wrong thing is exactly what that gate exists for. `build_kit` is the
first SOURCE-addressed verb it drives — it names no band, every band holding the source holding the
same entry — so `BandHandle` grew a `SourceAddressed` outcome keyed on the parsed VARIANT rather than
on the harness's own label, which is what stops a band-addressed command being opted out of the handle
check by being relabelled. The `builders` role is swept BARE there now: the sim refuses a `kit` token
on it, and that refusal is in the handler rather than the parser, so a parser-level gate cannot see it.

### THE EXPANSION — the whole queue over the whole Work zone (§4.9 item 9c)

`docs/plan_standing_upkeep.md` §4.9 item 9c. The block draws at most `BUILD_QUEUE_ROWS_MAX` entry
rows plus `+N more`, and **there is no cap on the queue itself** — so a fourth job was queued and
funded with no row, and nothing past the third could be seen, reordered or withdrawn from the UI at
all. The `+N more` row said so and offered nothing.

**THE 3-ROW BLOCK IS UNTOUCHED, AND THAT IS THE DESIGN.** It is a SUMMARY — what the pool is funding,
and what is next — and it stays exactly as wide, as tall and as capped as it was; the ceiling was not
raised and the reservation (`build_queue_block_height`, `_work_board_capacity`) is unchanged. The full
list is a **MODE** over the same zone instead, so it spends **nothing permanent**: `_queue_expanded`
is one bool, and every pixel it uses is a pixel the collapsed zone was already spending on the board.

```
WORK                                    16 sources  +3.48 /turn
POOLS                                              3 of 16 on work
[Agriculture 0] [Husbandry 0] [Builders 3]
BUILD QUEUE ▴                            3 builders · Tillage kit
┌────────────────────────────────────────────────────────────┐ ▲
│ ▸ 🌱 Cultivate (71, 18)   Cultivating 0% · turn 82   ▲ ▼    │ █
│ ⠿ ◎ Tame Red Deer         Taming 0% · turn 101       ▲ ▼    │ █
│ ⠿ 🌱 Cultivate (60, 22)   Cultivating 0% · turn 130  ▲ ▼    │ ░
│                       … every entry, scrolling …            │ ▼
```

**TWO DOORS IN, ONE DOOR OUT.** The `BUILD QUEUE` header is the toggle **both ways** and is available
whenever the block exists — including a queue too short to draw an overflow row. `+N more` is a
second door **IN only**: the expanded view has no overflow row left to press, so the header is the
only way back. Its tooltip changed with it (`BUILD_QUEUE_OVERFLOW_TOOLTIP` had been stale twice over
— it named the command line, which the drag replaced, and then said the hidden entries were out of
reach, which this replaces).

- **THE DISCLOSURE IS `▾` / `▴`, NOT THIS CLIENT'S OTHER CARET PAIR.** `DetailFormat.BREAKDOWN_CARET_*`
  and `hud_crafting_vocab.GROUP_HEAD_CARET_*` fold with `▾`/`▸`, and **`▸` is `BUILD_QUEUE_HEAD_MARKER`
  two rows below this head** — the entry the builders pool is standing on. One glyph meaning *folded*
  on the head and *funded* on a row of the same block is a collision the block cannot afford, so the
  pair is `hud_event_vocab`'s `CARET_DOWN`/`CARET_UP`. It comes out of the head's **EXPANDING spacer**,
  inserted after the title: the right-hand readout states the builders count and their kit and may not
  give up a character. Measured at both shipped docks, the head row is **356px on the tall LEFT dock
  and 382 on the 1920 BOTTOM one** — unchanged by the glyph, which the spacer paid for.
- ⛔ **BOTH DOORS FIRE ON THE RELEASE, INSIDE THE ROW.** `_toggle_queue_expanded` ends in
  `_repage_work_zone`, which frees every node in the zone — *any press handler that rebuilds its own
  subtree kills every drag that could start under it*, the general rule PR #574's autopsy named after
  the queue rows' own toggle shipped on the press and left the reorder gesture dead. The release-inside
  test is `mouse_focus`'s: the latch is taken on the press, so a release three rows away would
  otherwise still toggle the row it started on.

**WHAT THE MODE DOES NOT DRAW, and why a stub board was rejected.** The source board goes, and with it
the chips (they filter the board), the pager (it pages it) and the work inspector (it inspects a row
of it). A board squeezed to one or two rows is neither a list a player can use nor free — the zone has
no pixels to give it. `_work_board_capacity` is **not called at all** in this mode.

**WHAT IT KEEPS, and both are deliberate.** `_build_work_head` stays, so the player knows which zone
they are in. **The POOLS block stays**, directly above the list it funds: §4.7 moved keeping onto this
tab precisely because a pool on one tab and its consequences on another went unnoticed in playtest,
and re-creating that separation one zone down would repeat it.

#### THE THIRD SANCTIONED `ScrollContainer`, AND IT IS THE FIRST CONDITIONAL ONE

The panel is no-scroll by construction with named exceptions (`PARTIES_LIST_NAME` under
`ZONE_PARTIES`, `BAND_ZONE_SCROLL_NAME` under `ZONE_BAND`).
`HudWorkVocab.BUILD_QUEUE_EXPANDED_SCROLL_NAME` (`"BuildQueueList"`) is the third, paired with
`BandCityPanel.ZONE_WORK`, and it is safe for the identical reason: a `ScrollContainer` reports no
minimum on its scrolling axis, and what it DOES report is a fixed number the builder declares from the
zone's own BOX.

⛔ **ITS SANCTION IS CONDITIONAL WHERE THE OTHER TWO ARE UNCONDITIONAL** — it must exist **exactly**
when `_queue_expanded` and never otherwise, because the collapsed zone is the paged, no-scroll board
the whole zone model rests on. Both halves are asserted; *no stray scroll* is a claim a panel that
never expands satisfies for free.

`HudWorkVocab.build_queue_expanded_scroll_height(box_height, pools_fund_mode)` is the ONE expression:
the box less the work head, the POOLS block, the queue head and the two block separations between the
three blocks (the head and the list share one block at `separation` 0, so there is no gap inside it).
**It is not clamped up to a floor** — a dock too short for the mode must FAIL the zone-fit assertion
loudly, which is this zone's standing contract; a floor would turn that into a silent clip.

| dock | WORK zone box | POOLS | list declares | rows it affords | list holds | bar |
|---|---|---|---|---|---|---|
| tall LEFT | 354 × **759** | 82 | **625px** (627 laid out) | 22.3 | 14 | hidden |
| 1920 BOTTOM | 380 × **394** | 82 | **260px** (262 laid out) | 9.3 | 14 | **shown, 8px** |

**THE SCROLLBAR COSTS THE ROW'S JOB FACE 8px AND NOTHING ELSE.** Measured on the same fixture: the
face gets **126px on the tall LEFT dock** (no bar) and **144 on the 1920 BOTTOM one** (bar shown) —
the bottom dock's zone is wider, so it is still ahead. The face is already ellipsised at its widest
shipped value (`🐄 Corral Thunder Mammoths` needs 189), so the bar takes width from a column that was
already trimming, and the zone-width guard still reads **354 of 356**.

#### THE RULING ON "ONE EXPANSION AT A TIME" WHEN THE BOARD IS NOT DRAWN

The rule above was written for a zone holding BOTH lists. **When the board is not drawn at all its
premise is gone**: the work inspector has no host, so at most one expansion is drawn because only one
expandable list is present — the rule holds **STRUCTURALLY rather than by enforcement**. The
enforcement code stays, because collapsing returns to the mixed layout.

⛔ **AND ENTERING THE MODE CLEARS `_work_open_key` / `_work_floor_open`, which is load-bearing rather
than tidy.** The expanded fill never runs the board's own pruning path, so an inspector left open
survives the whole mode and **springs back on the collapse** — beside an open queue settings strip,
which is exactly the 460-into-a-396px-box overflow the exclusion rule closed. Sabotage-verified: with
the clear removed the collapse comes back with the inspector open and the board down a row (9 → 8).

#### THE STATE, AND WHAT IT IS AND IS NOT SCOPED TO

`_queue_expanded` is **NOT reset on a band change** — it is zone MODE, which is the player's, the same
reasoning `_work_filter` and `_work_sort` are kept under; a player comparing two bands' queues would
otherwise have it fold on the first selection.

⛔ **AND IT IS NOT PRUNED FOR AN EMPTY QUEUE EITHER, which it was.** No queue means no block, no block
means no header, and the header is the only way out — so `_fill_work_zone` declines to DRAW the mode
for a band with nothing queued (`if _queue_expanded and not queued.is_empty()`) and falls through to
the collapsed path, which draws no block for an empty queue either. That is the whole of what the "no
way back" argument requires. **Clearing the flag, which the first cut did, cancels the mode for EVERY
band the moment an idle one is selected** — i.e. it re-creates on a three-band cycle precisely the
band-change fold the paragraph above exists to prevent. Asserted by
`_assert_queue_expanded_survives_an_empty_queue`; restoring the prune fails it and the reselection
claim beside it (2).

#### ⛔ THE LIST REMEMBERS WHERE THE PLAYER WAS, ACROSS THE REBUILD ITS OWN CLICK CAUSES

Every in-mode interaction frees the zone: opening a row's settings strip runs `_toggle_queue_settings`
→ `_repage_work_zone`, and an arrow, a drop or a withdrawal takes effect through the returning
snapshot → `render_band`. A list rebuilt at 0 therefore throws the player back to the top on each of
them and once more per turn — and the entries only a scrolled list can reach are exactly the ones the
mode exists to reach. `_queue_expanded_scroll_offset` carries the offset across, captured at the top
of `_fill_work_zone` (the last moment the outgoing node is readable on BOTH fill paths: `_repage_work_zone`
has `queue_free`d it, and `render_band` builds the new zone before `set_zones` frees the old) and
restored by `_build_build_queue_expanded`. It is `CraftingPanel._pending_scroll`'s contract exactly.

- ⛔ **THE RESTORE IS DEFERRED ONE FRAME, AND THAT IS THE WHOLE CORRECTNESS OF IT.** `scroll_vertical`
  is clamped to the CURRENT content extent on the way in, and at the moment the rows are added that
  extent is not the list's — so an assignment made in the builder is silently clamped and the defect
  survives under a fix that reads right. `_restore_queue_scroll_offset` awaits one `process_frame`
  through `_host`, by which point the container has sorted and re-ranged its scrollbar. **A frame is
  needed even though the naive form appears to work at small offsets**: a fresh `VScrollBar` is a
  `Range` and a `Range` ships `max = 100`, so anything under 100px passes through the broken form
  intact — measured at 84px it passes, at 112 it clamps to 100.
- **The clamp is the container's** — a queue that lost entries has a shorter list, and the setter's own
  range check is what stops the restore landing past its new end.
- ⛔ **AND IT DECLINES UNDER A LIVE DRAG.** The edge auto-scroll writes `scroll_vertical` every frame
  of a gesture; a deferred restore resuming beside it would fight the pump. `_queue_drag_in_flight`
  already holds the rebuild off, so the only reachable case is a gesture starting inside the frame the
  restore is waiting out, and the restore checks rather than assuming.
- **ENTERING THE MODE RESETS IT** (`_toggle_queue_expanded`): the offset is a place in ONE list, and a
  fresh expansion opens at the top of the queue.

**THE MODE RE-SPELLS NOTHING.** `_build_build_queue_expanded` calls `_build_build_queue_head`,
`_build_build_queue_row` and `_build_queue_settings_strip` — so the arrows, the `✕`, the drag handle,
the head `▸`, the pending rules and the `▼` end-stop (`_queue_rank_keys(band).size()`) are inherited.
`_queue_row_nodes` is populated the same way, so the drop indicator still reaches its target.
`_queue_settings_state` is asked with `drawn == queued.size()`, so **any** entry can be open — the cap
was what made an entry past the third unconfigurable. `is_head` is still
`_build_queue_row_rank(entry) == SourceForecast.BUILD_QUEUE_HEAD`, the WIRE's head and not the first
drawn row, and every position sent is still an index into `HudBandLaborState.build_queue_keys(band)`.
Sabotage-verified on `_build_hidden_queue_band_fixture`, the one fixture whose drawn and wire lists
disagree: ranking the expansion's own loop on the drawn index fails **seven** assertions.

#### ⛔ EDGE AUTO-SCROLL — a scrolling list whose drag cannot leave the viewport is half a reorder

The arrows name a rank and do not care. The drag names a ROW, and a row it cannot scroll to is a row
it cannot reorder onto. Three mechanisms, each with a defect of its own:

- **THE PUMP IS PER-FRAME, NOT PER-MOTION.** A player who parks the pointer at the edge and holds
  still generates no motion events at all, and a scroll that only advanced on movement would be the
  same dead-gesture shape this arc has already shipped once. `QueueDragWatcher` carries it — it is
  already the one `Control` this `RefCounted` controller owns and the only thing that hears
  `NOTIFICATION_DRAG_END` — with `set_process(true)` on `NOTIFICATION_DRAG_BEGIN` and `false` on
  `DRAG_END`, and two independent guards inside the tick (`_queue_drag_key != ""`, and the expanded
  list mounted).
- ⛔ **ITS CLOCK IS WALL TIME, NOT `_process`'s DELTA, AND THAT IS NOT AN OPTIMISATION.** A frame
  delta is scaled by `Engine.time_scale`, and **every render harness in `tools/` pins that to 0** for
  determinism (`band_panel_preview`, `ui_preview`, `blend_probe` all do; `preview_watchdog` documents
  it). A pump driven by the frame delta advances by **exactly nothing** under the only thing that can
  test it — measured here as `0 → 0px over 45 frames` — so it would have shipped as a feature no frame
  could see. `QueueDragWatcher` measures `Time.get_ticks_usec` instead. A pointer-driven gesture is
  wall-clock by nature: it belongs to the player's hand, not to the simulation's clock.
- **THE DIRECTION TEST READS THE PHYSICAL POINTER** — `Control.get_global_mouse_position`, i.e.
  `Viewport.get_mouse_position` — against the scroll's global rect, gated on the pointer being inside
  it horizontally. A pointer BEYOND the edge keeps scrolling; a player dragging past the bottom expects
  that. It is the same quantity Godot localizes a drop with, which is what makes `Input.warp_mouse`
  able to drive it.
- ⛔ **A FRACTIONAL RATE NEEDS AN ACCUMULATOR.** `scroll_vertical` is an INT and
  `BUILD_QUEUE_AUTOSCROLL_ROWS_PER_SECOND` (**6.0**) × `WORK_ROW_HEIGHT` at 60fps is **2.8px a frame**;
  truncating per frame loses most of the travel. The remainder is carried in `_queue_autoscroll_carry`
  and zeroed whenever the direction is 0 or flips. **One tick may never move the list more than one
  row** (`BUILD_QUEUE_AUTOSCROLL_MAX_TICK_SECONDS`, stated as `1 / rows-per-second` rather than as a
  number of seconds): a hitch hands the pump an arbitrarily long elapsed time, and a multi-row step
  teleports the drop target past the row the player was aiming at.
- ⛔ **THE HOVER MUST BE RE-RESOLVED AFTER A STEP, OR THE DROP LANDS ON A STALE ROW.** Godot resolves
  the drag-over control on MOTION, so auto-scrolling under a stationary pointer moves the rows without
  telling it and both the indicator and the drop keep naming the row that used to be there. One
  zero-`relative` `InputEventMouseMotion` at the current pointer with `MOUSE_BUTTON_MASK_LEFT` held,
  at most once per frame and only mid-drag, is what makes the engine look again — **`Input.parse_input_event`
  is what does it**; `Viewport.push_input` was the documented fallback and was not needed. The
  observable consequence is the frame's own claim: *the drop mark moves while the pointer does not*.
- **THE HOT BAND IS ONE ROW** (`BUILD_QUEUE_AUTOSCROLL_MARGIN := WORK_ROW_HEIGHT`) at each edge of the
  viewport, so a pointer holding a row over the last visible row is already in it.
- **IT CHANGES `scroll_vertical` AND NOTHING ELSE.** No rebuild — `_queue_drag_in_flight()` already
  holds `_repage_work_zone` and `render_band` off for the duration, and freeing the row the pointer
  holds is what ends a drag. Setting the property past its range clamps itself, so the travel stops at
  the bottom with no clamp spelled anywhere.

**Frames:** `band_panel_queue_collapsed_long` (the PAIRED NEGATIVE, first, on the same 14-entry band —
3 rows plus `+11 more`, no expanded list) · `band_panel_queue_expanded_doors` (both doors, both
directions, all real clicks, plus the press-and-slide-off claim) · `band_panel_queue_expanded` ·
`band_panel_queue_expanded_arrows` · `band_panel_queue_expanded_settings` (⛔ the 1920 BOTTOM dock,
the expansion open AND a row's strip open) · `band_panel_queue_expanded_autoscroll` ·
`band_panel_queue_expanded_scrolled` (the 1920 BOTTOM dock, the list scrolled and a below-the-fold
row's strip opened on it) · `band_panel_queue_expanded_hidden_entry`, plus the PNG-less empty-queue
survival block. `harness-band-panel.md` → "The EXPANSION's frames" carries what each one can tell
apart.

### THE ORDER IS THE BAND'S OWN — and three surfaces were asking the wrong band (§4.9 item 9a)

`docs/plan_standing_upkeep.md` §4.9 item 9a carries the model. **`buildQueuePosition` is published per
SOURCE and rides the WINNING band** — among the bands working one source, the one with the soonest
estimate writes it (`BuildEstimateClaims::publish_running`). Two bands holding one source is ordinary,
so that int routinely states **another band's place in another band's line**, and every band-scoped
question asked of it got another band's answer.

**The gesture is where it showed.** Band B's queue is `[X, Y, Z]`; band C also holds Y with the sooner
estimate, so Y publishes `0`. The block tied X and Y at `0`, broke the tie on the key string and drew
**`[Y, X, Z]`**. Dragging Z above X computed `insert = 1` from that list and sent `build_order B Z 1`,
which `move_build_entry` resolved to **`[X, Z, Y]`** — Z behind X, the opposite of the gesture. The
optimistic overlay then painted the requested order until the turn resolved and the list **silently
jumped**.

⛔ **IT WAS NOT FIXABLE HERE, AND A TIE-BREAK IS NOT A FIX.** No band-keyed queue existed on the wire
and the chained date rides the same winner, so this layer held no second signal to recover the true
order from; a cleverer tie-break only picks a different wrong order. The answer is
`PopulationCohortState.buildQueue` — **the band's own entries, in the band's own order, rank = the
vector index**, captured live. This layer reads it and keeps no rank beside it.

**THE THREE CONSUMERS, and what each was really asking**

| was | asked | now |
|---|---|---|
| `_build_queue_models` / `_confirmed_queue_entries` | the block's membership **and order** | `HudBandLaborState.build_queue_keys(band)`, joined to the models by key |
| `HudBandLaborState.head_build_branch` | which web the Builders card's kit derives from | `build_queue_head(band)` — the head entry's own `kind` |
| `DrawerComposeController` (`◷ Queued` vs a running meter) | *are the builders on THIS one* | `HudBandLaborState.is_band_build_head(band, kind, source)` |

`SourceForecast.build_is_queue_head` is **deleted** — it could not answer for a particular band, which
is what all three needed. `BUILD_QUEUE_ROW_META` carries the row's rank **in this band's queue** — its
index into `build_queue_keys(band)`, `NOT_IN_ANY_BUILD_QUEUE` when pending — rather than the
source-addressed wire position, because all five of its readers were band-scoped too. It is **not**
the block's index into the rows it drew; the section below is why.

### AN ENTRY CAN OUTLIVE ITS ROW'S CREW, SO THE DRAWN LIST IS SHORTER THAN THE WIRE'S

The membership argument above — *an entry requires a row, and the board admits any source with a take
crew* — quietly equates a **row** with a **crew**, and the sim does not. The gap is reachable and it
persists:

| step | seam |
|---|---|
| unstaffing a source the band already held KEEPS its row, at zero workers | `LaborAllocation::set_assignment` → `keep_holding` |
| …and `assign_labor` declines to drop that row while the source is QUEUED | `handle_assign_labor` → `if applied == 0 && !source_holds_something && !queued` |
| the membership test asks only whether a row EXISTS, never how many hands are on it | `holds_build_source`, so `prune_build_queue` keeps the entry |
| …and the turn pass spares it for the same reason, so the state survives every turn | `queued.is_none()` guards the lapse in `advance_labor_allocation` |
| the client then drops that row, the board admitting on the take crew | `_work_source_models` → `if workers <= 0 and not pending` |

So a wire queue of `[A, B, C]` legitimately draws as `[B, C]`. **Membership is the drawn list's;
every ARITHMETIC is the wire's** — `_build_queue_models` walks the wire with its own index and lets a
skipped entry spend its rank, the `▼` end-stop is the wire queue's LENGTH (so `B`'s `▲` is enabled,
and only `C`'s `▼` is disabled), and `_queue_drop` removes and re-inserts in the wire's key list,
using the drawn row only to NAME the drop target. The `▸` is the wire's head too: an entry with no row
is still the one the builders pool is standing on, so neither drawn row wears it.

**The zero-crew row is deliberately NOT re-admitted to fix this.** Admitting it would put it back on
the WORK BOARD as well, which `docs/plan_standing_upkeep.md` §2.5 reverted on purpose — a separate
design question. `band_panel_queue_hidden_entry` is the frame.

> **THE COMPOSE-SHEET ONE RE-OPENED A REPORTED PLAY DEFECT BY A SECOND DOOR.** The DECLARED-vs-RUNNING
> fork exists because rendering a mere declaration as a running build was a one-way trap —
> `Cultivating 0 / 50 work (0%)` with no way back off it. A band whose entry stands third in its own
> line hit exactly that face whenever another band had the source at ITS head.

**WHAT STILL READS `buildQueuePosition`, and why it is legitimately SOURCE-addressed.** The `MapView`
→ `tile_info` passthrough behind the tile card and the map's queue badge — the meter belongs to the
**source**, so if any band is raising the rung it is being raised. It names no band. **Anything
band-scoped must not read it**, which is the whole of the rule. (`_rung_is_an_unordered_repair` asked
the same question and is retired with the 99% repair — `labor-ui.md` → "THE OFFER TEST AND THE TRACK
TEST ASK ONE QUESTION".)

> **THE ESTIMATE STILL RIDES THE WINNER, DELIBERATELY.** `build_turns`, the legs, the gear and the
> blocked cause are source-addressed fields and keep the sooner-estimate rule they were designed with
> (`snapshot.fbs`, "IT RIDES THE SAME WINNER"), so a shared entry in B's queue can quote a countdown
> chained down C's. §4.9 records that as out of item 9a's scope rather than as an oversight: the list
> is B's and the date is the best answer anyone has, where before **neither** was.

⛔ **NO FRAME REACHED EITHER OF THE LAST TWO STATES, and both breaks passed the whole suite once.**
`price_plant_build` stamps `patch_build_queue_position = 0` on every priced tile, so every fixture band
happened to BE the winner; and the sheet's head test is reached only with builders standing **and** a
zero meter, which no state combined. `band_panel_queue_shared_source` and
`compose_queued_behind_another_band` are the frames that close the gap — the same shape
`docs/plan_standing_upkeep.md` §4.10 records four times over, and the reason each fix here was
falsified rather than assumed.

### THE HEAD'S KIT DERIVES FROM A **PENDING** ENTRY TOO (§4.7)

The head readout resolved its kit from `head_build_branch`, which needs the entry the **wire** placed at
position 0 — so a build declared this turn derived nothing and the header read **`3 builders · No kit`**.
Reported from play. That fall-through was right when an empty queue was the only underivable case;
declaring from the Work board's `⌃` makes a **pending-only queue the common state**, so it was wrong far
more often than right. `_role_build_branch` falls through to the pending head, read off the block's own
ordered list rather than a second walk. `bare_kit_id` survives for the genuinely empty queue, which is
still the honest answer to *nothing is chosen and nothing can be derived*.
`_assert_builders_card_kit_faces` distinguishes **four** states now — plant head, animal head,
pending-only head, and an empty queue asserted as **no block at all** — because a resolver stuck on one
of them satisfies any three.

### A JUST-DECLARED BUILD IS IN THE QUEUE THE MOMENT IT IS DECLARED

`buildQueuePosition` is a WIRE field, so an entry the player declared this turn has none until the sim
resolves the turn — and the block, derived from that field alone, stayed empty until the next tick.
Reported from play as *"it is very confusing if it doesn't show up the moment I create it."*

The optimistic overlay already carries the declaration (`record_pending_assign` takes the
`improvement`, and `effective_worker_map` merges it), so `_build_queue_models` admits a **second**
set: a model that is `pending`, carries a live `building_glyph`, and is **not in the band's wire
queue**.

> ⛔ **THAT LAST TEST IS WHAT "PENDING" MEANS NOW, AND IT IS NARROWER THAN IT WAS** (§4.9 item 9a). It
> read *"the wire gave this no position"*, which was the same question while the position was
> turn-written. The band-side `buildQueue` is captured LIVE, and a declaration enqueues at command
> time — so an entry is placed, with a real rank, on the command's **own recapture**, and the old test
> would have drawn it **twice**: once confirmed and once at the tail. Pending is therefore exactly the
> **press→reply round trip** and nothing longer.
>
> **A ROW THAT LEAVES THE TAIL A ROUND TRIP EARLIER IS THE INTENDED TRADE, not a regression.** It
> gains its rank, its drag handle and — at index 0 — the `▸`, all of which are now true of it, and its
> date column goes blank rather than `○` because `buildTurnsRemaining` is still turn-written and
> `BUILD_TURNS_NO_ESTIMATE` renders as no line at all. That is the honest face: *the sim has not
> answered about this entry yet*, where `○` says *the sim has not placed it*, and it has.

- **PENDING ROWS SORT TO THE TAIL, after every confirmed entry.** The sim APPENDS, so the end of the
  list is the only honest place for an entry with no position; interleaving would state a fact the sim
  has not made. Within the tail they hold DECLARATION ORDER, read off `pending_assigns_for`'s own
  insertion order, with the same `key` tiebreak the confirmed half uses so the whole list stays a
  TOTAL order under Godot's unstable sort.
- **A PENDING ROW STATES NO DATE.** The countdown is CHAINED down the queue, so there is no answer for
  an entry that is not in the chain and any number there would be invented. The date slot carries the
  client's ONE spelling of pending instead — `○` in `HudStyle.WARN`,
  `FoodIcons.for_status(FoodIcons.STATUS_PENDING)`, the same mark the work rows' status clause and the
  map's dashed-amber overlays wear — and the row tooltip carries that status's own words
  (`HudFormat.status_tooltip_line`), which is where a one-character column has to say what it means.
- **IT GETS NO HEAD MARKER, even when the queue is otherwise empty.** The `▸` is the entry the
  builders pool is standing on, which the sim decides; a `▸` on an unplaced entry promises funding
  nobody has committed. `_build_queue_row_is_pending` is the ONE derivation of "the wire has not
  placed this" — read off `BUILD_QUEUE_ROW_PENDING_KEY`, stamped once onto the model — and the
  block's filter, the marker's suppression and the date all read it.
- **THE `✕` STILL WORKS ON IT, and needs nothing of its own.** `unqueue` names a SOURCE, so
  withdrawing a declaration made a second ago is the same command as withdrawing one placed ten turns
  back — and unticking a build you just made is the most likely thing a player wants from this row.
- **IT COSTS A FULL ROW**, so it goes into the SAME list `build_queue_block_height` counts and
  `_work_board_capacity` subtracts. There is exactly one expression for the drawn height and the
  reserved height (below), and a pending row drawn outside it would slice the board silently.
- **THE RUNG IN FLIGHT IS PART OF THE TEST, not just the declaration.** `record_pending_assign` fires
  on EVERY worker step and carries the improvement forward, so `pending` alone would keep a row here
  after its build completed; `building_glyph` is `RungGates.rung_in_progress`'s already-resolved
  answer, which goes empty the moment the meter does.
- **THEY RECONCILE AWAY FOR FREE** — `reconcile_pending` drops the overlay entry on the first snapshot
  with a newer turn, by which time the wire carries a real position, so the row becomes CONFIRMED
  rather than disappearing. Verified rather than assumed: `_render_pending_queue_states`' fourth state
  advances the turn, re-publishes the patch at position 1 and asserts the block still holds two rows
  with no `○` among them.

**Frames:** `band_panel_build_queue_pending` (one confirmed entry plus one declared this turn, in the
tall LEFT dock) and `band_panel_build_queue_pending_wide` (the BOTTOM dock, so the pending row is in
the HEIGHT BUDGET and not only in the arithmetic — `Zone_work` reads **252px of a 300px box** with the
board still drawing four rows). **The PAIRED NEGATIVE runs FIRST**, on the same band with nothing
declared: without it every claim above passes on a row drawn unconditionally, and the confirmed-only
queue is the state the game spends most of its time in.

### THE WORK ROW HAS THE SAME THREE-WAY ANSWER THE MAP BADGE HAS

A build row's rung slot renders the verb glyph plus `⚠` in `HudStyle.WARN`, **percentage dropped**,
when `SourceForecast.build_is_stalled` says the build is unstaffed or losing ground — the map badge's
own fork, off the same single producer, because the board printing a confident `▦45%` beside a map
plate already warning was reported from play. A build merely PARKED with its keeping covered keeps its
number and its `SIGNAL_DEEP` ink. The verdict, the twin format constants and the frame set are
specified in `labor-ui.md` → "A BUILD THAT IS NOT MOVING DOES NOT GET TO WEAR A PERCENT".

**The BAND's builders pool is resolved ONCE per render** in `_work_source_models`, so every row on one
board is judged against one crew count, and the model carries the answer as `build_stalled` rather
than the row builder asking the question a second way.

### THE BLOCK IS PAID FOR IN `_work_board_capacity`, OR IT SLICES THE BOARD

The work zone `clip_contents`, so a block that draws without being subtracted from the board's room
takes its height off the bottom of the zone SILENTLY — no overflow, no warning, just fewer rows than
the pager thinks it drew. `HudWorkVocab.build_queue_block_height(entries)` is therefore ONE function
that both the builder (as the block's `custom_minimum_size`) and the capacity's `chrome` term call,
plus one more `ZONE_BLOCK_SEPARATION` for the gap the block adds:

```text
rows  = min(entries, BUILD_QUEUE_ROWS_MAX) + (1 if entries > BUILD_QUEUE_ROWS_MAX else 0)
height = ZONE_HEAD_HEIGHT + rows × WORK_ROW_HEIGHT      (0 for an empty queue)
```

**`BUILD_QUEUE_ROWS_MAX` is 3, and the overflow row is the FOURTH rather than a replacement for the
third.** Measured on the 1920 bottom dock: a four-entry queue costs `20 + 4×28 + 6 = 138px` and the
work zone comes out **300px of a 300px box — 0 spare** with the board still paging two rows
(`Page 1 / 2 · 1–2 of 4`). That is a real fit rather than a comfortable one, so **re-measure before
adding anything to this zone**, and the lever if it has to give is the cap.

**A truncated list with nothing under it reads as the whole list**, so the `+N more` row is not
optional — the faction page's standing rule for a capped list, applied to the band's own.

**Frames:** `band_panel_build_queue` (the tall LEFT dock, three entries mixing BOTH webs — a
Cultivate, a Tame and a Sow, since the two webs reach the block through different branches of
`_work_source_models` and a single-web fixture exercises one of them) ·
`band_panel_build_queue_blocked` (the head at `BUILD_QUEUE_BLOCKED`, with every entry behind it
carrying the same sentinel — the sim's own behaviour, and the half a block showing only the head
would misreport) · **`band_panel_build_queue_none`** (the PAIRED NEGATIVE: a band with a work board
and nothing queued, which is what stops every claim above passing on a block drawn unconditionally) ·
`band_panel_build_queue_wide` (the BOTTOM dock, four entries, so the overflow row is in the
measurement). The dates are asserted as STRICTLY ASCENDING because that is what a chained countdown
means — equal dates would pass a "the dates render" check while proving nothing — and the `✕` is
PNG-less, driven through the real handler and read back off `Main.format_unqueue` on BOTH webs, since
either alone passes on a builder that gets the grammar backwards.

## THE WORK INSPECTOR IS A DIALOG, AND THE ZONE STOPS PAYING FOR IT (§4.9 item 12d)

`docs/plan_standing_upkeep.md` §4.9 item 12d. The board's inspector strip is a **persistent,
viewport-centred, NON-MODAL `WorkInspectorDialog`** on its own `CanvasLayer`. It is a **rehost, not a
redesign**: `BandPanelController._build_work_inspector` builds exactly what it built before — the head
line, the conditional notes, the arrivals strip, the links row and whichever of the three pickers is
open — and the only thing that changed is who hosts it and who pays for it.

**THE ZONE HAD FOUR PIXELS AND EVERY EXPANSION OVERFLOWED IT.** Measured on a 1920 bottom dock with a
row selected: the box is **396** and the closed strip asked **392**. Open the *shipped* priority
picker and it asked **444** — over by 48. Open item 12c's kits picker and it asked 436, over by 40.
There was no expansion small enough: `_work_board_capacity` is floored at `maxi(1, …)`, so the board
could give back 4px of `int()` truncation and **zero rows**. Two small bottom docks — **1152×720** and
**1024×768** — already overflowed with nothing expanded at all, against boxes
`MAX_WIDE_HEIGHT_FRACTION` had clamped. **Those measurements are HISTORY now, not live constraints**:
they describe a zone that charged itself for a strip it no longer draws, and none of them can recur,
because no selection and no picker takes a pixel off the zone.

**A WINDOW CANNOT CHANGE A ZONE'S HEIGHT, and that is the whole fix.** `_open_rung_track`'s own
docstring had been making the argument for a smaller piece of content in this same zone — *"the detail
breakdowns are popovers and the destructive confirms are `ConfirmationDialog`s. It costs the zone
nothing at all."* The inspector was the piece it was never applied to. Hosting it off the zone makes
the overflow **impossible** rather than made to fit.

### Four properties, each load-bearing

- **CENTRED OVER THE MAP**, not centred on the panel and not anchored to the row. On a bottom dock
  the panel is a strip with a screen of map above it; on a side dock a column with map beside it — so
  one centre lands over the MAP on every edge and the board stays visible. Floating it off the panel's
  map-facing edge aligned to the selected row reads better and was **declined**: four dock edges means
  four placements plus clamping. **It centred in the raw VIEWPORT until the sections made the card
  340px tall** — see "…AND IT STOPPED PRETENDING TO BE A CRAMPED STRIP" for the measurement that moved
  it onto the room.
- ⛔ **NON-MODAL — no catcher, no scrim.** The card re-targets when another board row is selected,
  which is only possible if the board stays live underneath. `ComposeSheet` IS its own full-viewport
  `MOUSE_FILTER_STOP` catcher; this card covers its own rect and nothing else, `BandComposeFloat`'s
  rule for the same reason (`PanelRoot`'s autopsy in reverse — every pixel it claims is dead map).
- **PERSISTENT, with an explicit dismiss.** It does not close on an outside click; a stepper press on
  a different row is ordinary use. The `✕` in the head closes it, and so does **ESC**.
- **EVERY DOCK, not the horizontal one.** The vertical dock does not need it — its box is the full
  window height less chrome — but a fork means two layouts and two frame families, which is exactly
  the shape that hid both defects above. One behaviour, one code path, one matrix. The vertical dock
  gets the freed rows too.

### ⛔ A `Control` ON A `CanvasLayer`, NEVER A `Popup`

**`Popup` auto-hides on an outside click and on parent focus loss**, which is precisely the dismissal
semantics item 12d forbids. `_open_rung_track` is a `PopupPanel` because it is *transient* (open,
pick, gone); a persistent non-modal surface fighting `Popup`'s auto-hide is the wrong node. The layer
gives the one property the slice needs — **it does not participate in any zone's layout** — with no
focus semantics to fight and no scrim by default.

**`HudLayer.WORK_INSPECTOR_LAYER_INDEX` is 105 and the ladder is stated as relations**:
`BandCityPanel.LAYER_INDEX` (103) → `EventDockPanel.LAYER_INDEX` (104) → this → `COMPOSE_LAYER_INDEX`
(106, now defined as `WORK_INSPECTOR_LAYER_INDEX + 1`) → `Main`'s `PauseLayer` (200). **Above the
event bar** because the bar is `MOUSE_FILTER_STOP` and a picker drawn under it is unreachable rather
than merely obscured — `COMPOSE_LAYER_INDEX`'s own autopsy. **Below the compose layer** because
`ComposeSheet` is a catcher with its card centred inside it, and a centred dialog drawn OVER that
would take the clicks meant for the sheet; so with a sheet open the first click on this card is a
dismissal, exactly the trade already accepted for the Band/City panel itself.

**The rung track and the ring price card still come up over it**, and it is structural rather than
lucky: both are `PopupPanel`s, i.e. embedded SUBWINDOWS, and Godot composites those above every
`CanvasLayer` of the parent viewport. `band_panel_work_inspector_dialog_over_track` is the frame, with
the node kinds asserted beside it — a price card opening behind the surface that spawned it is exactly
the kind of thing that ships.

### The height arithmetic MOVED, it did not die

`BandPanelController._work_inspector_height` keeps its job and changes **consumer**: it stopped being
a term in the ZONE's budget and became the **dialog's own `min_height`**. That is what keeps
*reserved ≥ drawn* a claim anybody can still make — deleting it would have made the rehost
unfalsifiable rather than free. The strip keeps its `WORK_INSPECTOR_META` and its
`custom_minimum_size`, so `_assert_work_inspector_fits` reads exactly what it always read.

**Three terms retired from the zone's budget, and a fourth arrived to replace one of them** (the
inspector's OWN arithmetic then became a sum over three sections — see the section below):

| term | before | after |
|---|---|---|
| `_work_board_capacity`'s `inspector_h` | `_work_inspector_height(inspected)`, and an `inspected` parameter to carry the model | **gone, parameter and all** — so it cannot return as a `0.0` that later grows |
| `WORK_ZONE_GAP_COUNT` | 3 (head→chips, chips→board, board→inspector) | **2** — there is no board→inspector seam |
| `BUILD_QUEUE_ROOM_INSPECTOR_HEIGHT` + its gap in `BUILD_QUEUE_ROOM_GAP_COUNT` | `WORK_INSPECTOR_HEIGHT` (84) + one separation | **gone**; the gap count is 5 |
| `BUILD_QUEUE_ROOM_SETTINGS_HEIGHT` | — | **the wrapped settings strip (56)**, which the retiring inspector term had been paying for by accident |

**THAT LAST ROW IS THE ONE THAT BIT.** A queue row's SETTINGS strip is charged to the BOARD, and the
board is floored at `maxi(1, …)` — so once the queue has claimed enough rows to leave the board at
that floor, an opened strip has nothing to come out of. It never showed while `build_queue_rows_max`
reserved 84px of inspector the queue could not use: the settings strip fitted in its shadow. Taking
the inspector out of that reservation is what exposed it — measured, `Zone_work` drew **414 into its
396px box** the moment a strip opened on a 1920 bottom dock. The replacement is stated as the strip's
own worst case (the wrapped control pair) rather than as a cushion, and **legs are deliberately not
counted**: a multi-leg climb is the rarer entry, and reserving for it would shrink the block on every
dock for a state most bands never reach.

**`WIDE_DOCK_QUEUE_ROWS` IS 2, AND NOT ONE PIXEL OF THE STRIP MOVED TO BUY IT.** The zone's allocation
rule is unchanged, so the freed room goes where it always went: the queue claims up to its authored
cap and the board takes the remainder.

### The lifecycle, and what happens when the row stops existing

The key is `_work_open_key`, exactly as before, and every render reconciles the card against it —
`_fill_work_zone` records what the column builder resolved (`_work_inspected`) and syncs on the way
out, which is what makes the reconcile total across that builder's three exits (an empty board, the
expanded queue, the ordinary path). **Opening a second row RE-MOUNTS rather than closing and
reopening**, so the card stays up and its body changes; a close-and-reopen would blink the surface the
player is working in and throw away the fit it had settled on.

**The card comes down when its row stops existing, and "stops existing" is read widely.** Unassigned
to 0, filtered off the board, the band switched by the cycler, the faction page pinned, the panel
hidden with no bands left, the narrow shell tabbed away from Work, the panel collapsed — every one of
those closes it. Closing is the right answer in all of them for one reason: this card is a surface for
acting on a row, and a card floating over the map with nothing behind it to act on is worse than no
card. `BandCityPanel.shows_zone(zone)` is the seam for the tab/collapse half — `zone_size()` cannot
answer it, being the box a zone's content is BUILT against, and every zone is built on every render
whichever tab is up.

**ESC closes it, fourth in `Main.escape_claimant`'s chain**: pause-resume → compose sheet → targeting
→ **work inspector** → pause. Behind the sheet and behind targeting because it is the OUTERMOST
working surface of the three (the sheet is transient and modal; targeting is a question the client has
asked and is waiting on; this one is still there afterwards, so it yields). Ahead of the pause menu
because a surface with an explicit dismiss must answer ESC before ESC means *leave the game*.

> #### ⛔ A TAB SWITCH IS NOT A RESIZE, AND `zones_resized` COULD NOT CARRY IT
>
> `_sync_work_inspector_dialog` runs only from `_fill_work_zone`, and off the snapshot the only thing
> that reaches it is `BandCityPanel.zones_resized`. **That signal fires on a real move of
> `work_zone_size()` — and `zone_size()` never reads `_effective_tab()`**, every zone being built
> against the same box whichever tab is up, so a tab click moved neither term of
> `_notify_zones_resized`' test and it early-returned. A collapse and a hide genuinely did close the
> card (they zero the box); **the tab was the hole**, and on a side dock it left the card floating over
> the map, anchored to a board that was no longer drawn, until the next snapshot re-render.
>
> **THE FIX IS A SECOND SIGNAL, NOT AN UNCONDITIONAL EMIT OF THE FIRST.** `shown_zone_changed` fires
> from `set_active_tab` when `_shown_zones()` — `shows_zone` asked for every declared zone, in
> `ZONE_KEYS` order — comes back different across the swap. Emitting `zones_resized` instead would have
> re-paged the whole work zone on every tab click, a re-render storm traded for a bug, and the boards
> do not need it: every zone is BUILT on every render, so they are already correct for the new tab.
> **The handler reconciles the float and pages nothing** (`_on_shown_zone_changed` →
> `_sync_work_inspector_dialog`), which is also what brings the card back on the tab back.
>
> **IT IS `shows_zone` IN A LOOP RATHER THAN A SECOND READING OF `_effective_tab`.** The listener asks
> `shows_zone` through `_work_zone_is_on_screen()`; a privately re-derived test on the emitting side
> would be free to disagree with it, which is the one way the signal could announce a swap the listener
> cannot see. The wide shell therefore reports nothing on its own (it draws every zone whatever the tab
> is), and so does a tab the current subject does not declare.

> #### ⛔ …AND THE KEY OUTLIVED THE CARD, SO ESC WAS SWALLOWED
>
> `is_work_inspector_open()` answered `_work_open_key != ""`, and the retired reason is quoted because
> it names a real hazard: *"IT ANSWERS THE KEY, NOT THE NODE. The key is what every render reconciles
> the card against, so a reading taken off `visible` would disagree with it for exactly the one frame
> between a selection and the fill that mounts for it — and ESC arriving in that frame would open the
> pause menu instead."* **That trade bought a one-frame window and paid with a state that persists**:
> every dismiss branch of `_sync_work_inspector_dialog` leaves the key set, so after a collapse, a hide
> or the tab switch above, `Main.escape_claimant` kept returning `ESC_WORK_INSPECTOR`,
> `_unhandled_input` kept calling `set_input_as_handled()`, and the first ESC did nothing visible — the
> pause menu needed a second press.
>
> **BOTH TERMS NOW, AND EACH ANSWERS ITS OWN HALF**: the key is the board row's SELECTION (what
> `close_work_inspector` clears and what a re-render re-mounts from), and `WorkInspectorDialog.is_open()`
> is whether the card is on screen, which is the only thing ESC can take down. The one-frame window is
> not reachable through the toggle — `_toggle_work_inspector` sets the key and mounts inside one call
> stack — and where a fill genuinely cannot mount (no host, a queue drag in flight) no card is drawn and
> the pause menu is the honest claimant.
>
> **BOTH CLAIMS ARE PAIRED, AND THE OLD ONE WAS DRIVING ITS OWN PROOF.** `band_panel_preview`'s
> tab-switch state called `rerender()` immediately after `set_active_tab` — a re-render runs
> `_fill_work_zone`, which ends on the reconcile — so the card came down on the HARNESS's push and the
> claim passed while the real tab click did not. Nothing but `set_active_tab` is called there now, and
> three claims stand on the one state: the card is down, the shell really has swapped (work zone not
> shown, band zone shown), and ESC falls through to `ESC_PAUSE`. **Frame:**
> `band_panel_work_inspector_tab_away`.

### What the matrix asserts, and why it is a matrix

`band_panel_preview._render_work_inspector_dialog_states` walks **eleven dock/viewport
configurations** — LEFT at 1080/900/768/720 and BOTTOM at 1920×1080, 1600×900, 1440×900, 1366×768,
1280×800, 1152×720, 1024×768 — on the file's fullest band (the POOLS block, a four-entry BUILD QUEUE
and a board stacked together; the reference band would report hundreds of pixels of spare and prove
nothing). Both shipped overflows are gone.

**It walked FOUR PICKER STATES per configuration until item 12d's second pass retired them**, and the
loop went with them: there is no expansion left that a click could change, so the zone figure is taken
once and the interesting number moved to the CARD's height against the room it is centred in. That is
a stronger reading of the same property, not a weaker one.

The claims that cannot rot, each paired with its liveness half:

- **the strip is outside every zone and inside the dialog** — the containment claim INVERTED, so it is
  paired with the card really drawing its head, its links row and its close `✕`
  (`_assert_work_inspector_is_a_dialog`);
- **the zone's budget contains no inspector term at all** — `_work_board_capacity` is called with a
  row SELECTED and again with none, and the two answers must be identical, paired with the rendered
  row count not moving and being non-zero. Sabotage-verified: with the retired terms restored the
  claim fails at `rows [2, 1, 1, 1]` (it walked the four picker states then);
- **the board's row capacity went UP** — the 1920 bottom dock draws 1 → **2** rows with a row selected,
  and the small bottom docks 2 → **4**;
- **the dialog is non-modal** — its layer holds the card and nothing else (no catcher), its rect is a
  fraction of the viewport, paired with the board underneath really drawing rows;
- **it survives a re-select** — the SAME card instance holding a DIFFERENT strip, paired with the same
  key still closing it;
- **it is centred over the map** — the card's centre is the ROOM's and its rect does not intersect
  `BandCityPanel.card_rect()`, which is the one claim no arithmetic can make, and the one that caught
  the 340px card running through a bottom dock's panel.

`_assert_work_inspector_worst_case_fits` was the ONE state that ever tested the strip's ceiling and it
tested the bottom dock; it is **kept and retargeted** — the worst-case model is mounted into the real
card, and the ceiling is now asserted to FIT the viewport it is centred in rather than to be an
unreserved 106px risk against a 396px box.

### …AND IT STOPPED PRETENDING TO BE A CRAMPED STRIP — the three SECTIONS

`docs/plan_standing_upkeep.md` §4.9 item 12d, second pass. The card shows **everything at once**:

```
 🌾 Harvest (70, 17)                    ✕      the readout — notes, arrivals, unchanged
 ────────────────────────────────────────
 POLICY      [Everything] [Best] [Learning]
 ────────────────────────────────────────
 PRIORITY    [High] [Normal] [Low]
             <the rank hint>
 ────────────────────────────────────────
 KITS        Harvesters [Harvesting kit ▾]
             Upkeep     [Tillage kit ▾]        ← only where the site OWES upkeep
             Kept at 2 work a turn.            ← ditto; the terms, not the rung word
 ────────────────────────────────────────
 Jump to source                  Unassign
```

**POLICY, PRIORITY and KITS are SECTIONS with their content shown; `Jump to source` and `Unassign`
are pure ACTIONS with no content, so they are the only two buttons left.** That is the whole
distinction the card now draws, and it is what the assertions test: a section reappearing as a link
fails, and a link demoted to a header fails.

#### ⛔ `_work_picker_open` RETIRED ENTIRELY

The state (`WORK_PICKER_NONE`/`_FLOOR`/`_PRIORITY`/`_KITS`), `_toggle_work_picker`, and the
`Change policy` / `Priority` / `Kits` links that toggled them existed **only** because the strip paid
for the tallest picker and could afford exactly one. The dialog competes for no zone height, so the
whole mechanism was rent-control for rent no longer charged. Removing it also deletes a defect class —
a picker left open when the row changed — and makes the card stateless about what it is showing.
`_work_open_key` is now the inspector's only state.

**`WORK_INSPECT_POLICY` ("Change policy") retired with them**: a header names what is below it, so the
POLICY section takes `WORK_INSPECT_POLICY_SECTION` ("Policy"). `WORK_INSPECT_PRIORITY` and
`WORK_INSPECT_KITS` were already nouns and are reused unchanged — the former is also the CRAFTING
panel's bench-link face, which is why it stays a shared const.

#### The height is a SUM now, and the retired MAX is the claim that had to be inverted

| term | value | note |
|---|---|---|
| base `WORK_INSPECTOR_HEIGHT` | 64 | head + actions row + gaps + card padding, unchanged |
| conditional notes ×3 | 20 each | overdraw, `note`, `muted_note` |
| `ArrivalStrip` | 14 | when the schedule has a gap |
| `WORK_INSPECTOR_SECTION_HEAD_HEIGHT` | **27** | rule (1) + gap (6) + label line (14) + gap (6) |
| POLICY section | **59** | head + the floor picker's 32 |
| PRIORITY section | **79** | head + the rank picker's 52 (grid + hint) |
| KITS section | **49** | head + the TAKE control line (22) — every row that has kits |
| …its UPKEEP half | **42** | the second control line (22) + the standing bill (20) — only where the site owes one |
| `WORK_INSPECTOR_ACTIONS_RULE_HEIGHT` | **7** | the hairline above the two actions and its gap |
| a WRAPPED line, each after the first | **17** | `WORK_INSPECTOR_NOTE_WRAP_LINE_HEIGHT`, added per line the sentence takes beyond one |
| **`WORK_INSPECTOR_CEILING_HEIGHT`** | **374** | every conditional child at ONE LINE EACH, on a row with kits — a **floor** on the worst case since the notes started wrapping |

An ordinary KEPT row with no conditional notes measures **300** and a WILD one **258**; the rendered
card is **298** at the fixture's width, on the wild queue row the dialog frames open (the priority hint
and the note it carries). It read *"the rendered card is 340"* while a wild row still drew an Upkeep
picker it had no bill for. **The section rule is
`HudStyle.LINE_SOFT` at `BandCityPanel._make_zone_separator`'s thickness** — the panel's own hairline
vocabulary, turned on its side — and the headers are `HudWidgets.alloc_section_label`, which is what
the allocation panel already heads its sections with. Nothing was invented.

**`WORK_INSPECTOR_SECTION_RULE_THICKNESS` is a TWIN of `BandCityPanel.ZONE_SEPARATOR_THICKNESS`, not a
read of it** — a vocab leaf reaching for a `class_name`d panel script at class load is a cycle waiting
to happen, the same rule `WORK_INSPECTOR_ARRIVALS_STRIP_HEIGHT` follows against `ArrivalStrip`.

#### ⛔ THE CARD'S PROSE WRAPS NOW — the elide was the STRIP's constraint, not the card's

Reported from play, on the shipped card: the material-shortfall note read

```
Short of hurdles — 0.03 of the 0.05 a turn it needs. The bench or a trad…
```

— cut off one word into the only clause that says what to DO about it. **The elide was correct for
every day the inspector was a strip inside the work zone** and is stated in `HudWidgets.
build_status_part`: the zone's box is reserved and `clip_contents`, so an unwrapped `Label` reporting
its whole text as a minimum width clamps the entire tab's column and slices the POOLS readout, the
Builders card's `+` and every board row's stepper off the right edge. Item 12d moved the inspector into
`WorkInspectorDialog` — a card on its own `CanvasLayer`, in no zone's layout, clipping on neither axis
— and the elide simply stayed behind, exactly like the mutually-exclusive pickers the same item
retired.

**FOUR LINES WRAP, AND THE HEAD LINE STILL CLIPS.** The overdraw warning, the `note`, the `muted_note`
and the KITS section's standing bill are sentences a player reads to act, and they take
`HudWidgets.build_wrapping_status_part`. The title keeps `clip_text` — it is an identity, its rung
clause is *designed* to be the first thing to go, and a wrapping title would move every control below
it whenever a species name ran long.

**A SECOND BUILDER, NEVER A WIDER DEFAULT.** `build_status_part`'s `elide` flag is chosen by *can this
host grow?*; wrapping is chosen by *is this line prose?*. Folding them into one tri-state would let a
caller pass "wrap" to a host that clips — and the tile card's flow, the drawer's standing summary, the
parties inspector's detail lines and the build queue's tooltip are all still in reserved-width boxes
that need the elide. `band_panel_work_inspector_width` / `_assert_zone_content_width_fits` still hold
them to it, and `band_panel_parties_inspector_narrow` is the frame that shows one still eliding.

**THE HEIGHT FOLLOWS THE WRAP, MEASURED RATHER THAN ALLOWED FOR.** `_work_inspector_wrap_overflow`
asks the FONT — through `HudWidgets.wrapped_status_part_lines`, at the card's real column width, with
the drawn label's own break flags — how many lines the sentence takes, and adds
`WORK_INSPECTOR_NOTE_WRAP_LINE_HEIGHT` for each one past the first. A fixed "notes get two lines"
allowance was the alternative and was declined: it is right until the first sentence that needs three
and nothing says so. Three properties make the measurement safe to trust:

- **The width is a LOWER BOUND on the drawn column.** `_work_inspector_note_width` is
  `WorkInspectorDialog.CONTENT_WIDTH` less the strip's own padding (**342**), while the card really
  lays the note out at **350** — `AutoSizingPanel.fit_width` never goes BELOW `target_width` and may go
  above. Measuring narrow over-counts lines, which is the safe direction; the drawn rect is asserted
  against that column so the two cannot drift apart.
- **A wrapped line costs 17, not 14, and that is the trap.** `WORK_INSPECTOR_NOTE_LINE_HEIGHT` is a
  ONE-line label's measured height; a `Label` spends its theme `line_spacing` BETWEEN lines, so a
  two-line note draws **31**. The first cut priced wrapped lines at 14 and under-reserved by 3px a
  line — caught by the harness on two states (402 reserved against 407 drawn at the worst case, 334
  against 336 on a kept herd), which is exactly why `reserved >= drawn` is asserted against a
  laid-out label rather than against the arithmetic that produced it.
- **The ceiling stayed a constant and changed meaning.** `WORK_INSPECTOR_CEILING_HEIGHT` is the total
  AT ONE LINE PER NOTE, so it is now a floor rather than the worst case. The equality assertion split
  in two: the reservation is above the ceiling, and the excess is WHOLE wrapped lines and nothing else
  — which still fails on a fifth conditional child of any other height, the structural question the
  equality was really asking.

**THE CARD WAS NOT WIDENED, AND THE MEASUREMENT IS WHY.** The shortfall sentence takes exactly **2**
lines at the shipped column, and the priority hint and the standing bill still fit on one; a wider card
would buy one line back at the cost of the width the head line, the two 3-cell grids and the kit
pickers were all laid out at. `CONTENT_WIDTH` stays `BandCityPanel.ZONE_PARTY_WIDTH`. If a sentence
ever reaches three or four lines the width is the lever to reach for — the card is free-floating and no
zone constrains it — but nothing measured today asks for it.

**AND THE WORST CASE STAGES A WRAP, OR THE CLAIM IS VACUOUS.**
`_assert_work_inspector_worst_case_fits`'s fixture note was `"Animals are drifting off."` — one line,
which makes *"reserved >= drawn with a wrapped note in it"* the old claim under a new name. It is the
shipped shortfall sentence's own shape now, and the harness asserts `Label.get_line_count() >= 2` on
the DRAWN label before it measures anything. Frames: `band_panel_work_material_short` (the sentence
whole, over two lines, in DANGER ink) and `band_panel_work_kits_kept_herd`.

#### The kits hint came back, and the tooltips lost the half it duplicates

It was cut for one reason: *"two kit lines plus a hint would be 64 — 12 over the current max, which
busts the wide shell by 8."* That arithmetic was about a 396px zone box. **The tooltips keep what only
they say** — which of the two pickers this is (the crew's tool against the site's) and the upkeep
one's per-site scope — and lose the `none` sentence the visible line now carries. Deleting them
outright would have lost the distinguishing half.

**The hint is SHORTER than `WORK_PRIORITY_HINT`, and that is a measurement.** The first draft ran seven
characters past it and rendered **ellipsised** in the frame — a hint the card reserved one line for and
could not fit. It names the picker's own face (`No kit`) rather than the wire token, so the line and
the entry read as one thing.

#### ⛔ THE PLACEMENT MOVED FROM THE VIEWPORT TO THE ROOM, and the sections are what forced it

The card was **viewport-centred** for one slice, which was safe while it was 104–156px tall: the
viewport's centre was always over map. At **340** it is not. Measured on a 1920×1080 bottom dock: a
viewport-centred card spans y=370…710 while the panel card starts at **624** — it covered the header
and the top of the very board the rehost exists to free.

**`WorkInspectorDialog._room()` is the viewport inside `VIEWPORT_MARGIN`, cut back to the MAP-FACING
side of the panel card with `ANCHOR_GAP` of clearance** — `BandComposeFloat`'s own rect, and
`BandComposeFloat.map_facing_side` is the ONE table naming which side of a docked card faces the map.
The placement rule is unchanged in KIND: one centre, one rect, no dock-edge fork. What changed is that
*"the board stays visible"* is now structural rather than a consequence of the card being small.

**The cost is a scroll on the four smallest BOTTOM docks.** The room a bottom dock leaves is the map
band above it, and at 1366×768 / 1280×800 / 1152×720 / 1024×768 that is 264–296px against a **298px**
card — the WILD queue row those frames open; a KEPT row is 340 — so `fit_to_content` clamps the card
and its own `ScrollContainer` carries the rest. `band_panel_work_inspector_dialog_tight` is the frame,
and on it the KITS take row and both actions are below the fold. It read *"the KITS upkeep row and
both actions are below the fold"*, which was written while a wild source still drew an Upkeep picker
it had no bill for; that row has no upkeep row to push under the fold at all. **Against the raw
VIEWPORT the card fits every configuration** (298–340 of the 696px a 720-high window leaves, a margin
of 356 or better), so this is a cost of not covering the board rather than of the card being too tall.
A vertical dock has 356–716px of margin everywhere.

⛔ **THE WILD ROW IS SHORTER AND THAT IS NOT A FIX FOR THE CLAMP.** Dropping the Upkeep row takes 42px
off a wild card, which eases the four smallest bottom docks by exactly that much and changes nothing
about the mechanism: a KEPT row on the same dock still scrolls. The clamp is a property of the room a
bottom dock leaves, not of the KITS section.

#### The assertions that had to be inverted, and why they were kept

Three claims on this branch asserted properties the slice then deleted. **Each was inverted, not
deleted** — a claim quietly guarding retired behaviour is worse than no claim:

| was | is |
|---|---|
| `_assert_kits_picker_is_exclusive_and_costs_the_max` — the three pickers are mutually exclusive and the strip reserves the MAX | `_assert_sections_are_drawn_and_cost_the_sum` — all three sections draw on one render and the card reserves the SUM, term for term against the producer, plus *"more than the tallest section alone"* |
| *"with the expansion CLOSED the strip draws neither picker"* | `_assert_kits_section_draws_both_controls` — both pickers, their rosters, `none` on both and the site's BILL, with **no click at all**, staged on a source that stands on a rung. It asserted *"and the HINT"* until the hint retired; `_assert_wild_source_offers_no_upkeep` is its other arm, and neither is worth anything without the other |
| *"the pick CLOSES the picker"* | the PRIORITY section is still drawn after a pick, which is what lets the three levels be pressed in sequence the way a player does |
| *"`Change policy` swapped the strip to the FLOOR picker … the priority picker is GONE with it"* | both grids are on the card at once, each under its own header |
| *"the card is centred in the VIEWPORT"* | the card is centred in the ROOM the dock leaves |

`_press_work_inspector_link` is **deleted** — there is no link left that opens anything, and its last
caller went with the swap frame it drove.

### …AND IT SOMETIMES DREW AT FULL ROOM HEIGHT AROUND 300px OF CONTENT

Reported from play: *"sometimes the job panel is displaying full height, it doesn't always do this when
I bring it up, but I've seen no real pattern."* On a ~1263px window the card spanned ~1178px — the whole
room it may use — while the head line, POLICY, PRIORITY, KITS, the kept-at line and the actions row
occupied the top ~300px, **with no scrollbar and the content not stretched**.

That last detail is what names the cause. A body whose combined minimum were genuinely huge would draw
tall or engage the scroll; it did neither. The card was sized from a measurement that was wrong at fit
time and then never corrected.

**A `VBoxContainer` WHOSE CHILDREN HAVE NOT BEEN SORTED REPORTS ITS AUTOWRAP LABELS AT A WRAP WIDTH OF
ZERO** — one word per line — so the number `refit` reads at the instant of a mount is not the previous
content's wrapping, it is nobody's. Measured on this very body: **736 where it settles at 278**, and
**3773 against 408** on the fullest strip in `band_panel_preview`. 31 of the 51 fits in one harness run
had a same-frame reading that differed materially from the settled one.

**THE ORDERING THAT SHIPS IT.** `refit` coalesces across a frame. A re-mount landing after a fit is
armed and before it resumes leaves that fit measuring a body which has just been replaced; it asks
`fit_to_content` for more than the room has, the room's ceiling wins, the internal scroll is set AUTO
over content that then fits (so no bar draws), and the re-mount's own fit — **the one thing that would
ever have measured the new body — was DISCARDED by the coalescing guard**. Nothing re-fits until some
unrelated event re-mounts, which is the "no real pattern". Reproduced to the pixel in the harness:
card **638 of a 638px room** around 278px of strip.

**THREE CHANGES, AND NONE OF THEM IS A FRAME COUNT** — all three are `ComposeSheet`'s, which had solved
this and which this card was written without:

- **The coalescing DEFERS rather than DISCARDS** (`_fit_requested`, cleared at the start of the run that
  honours it, re-run at the end). One coalesced re-run, never a queue.
- **The height is read a frame AFTER the width is applied**, not in the same pass — Godot's container
  sort is deferred, so a combined minimum read in the pass that just moved the card's width reports the
  previous width's wrapping.
- **`_body.minimum_size_changed` asks for the fit**, wired in `_ready`. This is the one that makes a
  wrong fit RECOVERABLE rather than permanent: every fit is a measurement taken at one instant and the
  card does not choose that instant, so the body says so whenever the number it reports moves, and a fit
  taken mid-rebuild is corrected on the frame it settles. The coalescer collapses the burst a rebuild
  emits, and a re-run that measures the same value applies the same size, so it converges.

**THE CLAIM THAT CATCHES IT IS ABOUT THE CARD'S HEIGHT AGAINST ITS CONTENT'S, AND THAT DISTINCTION IS
THE WHOLE OF WHY THE DEFECT REACHED A PLAYER THROUGH A GREEN HARNESS.**
`_assert_dialog_fits_its_room` passes on a card that IS the room — it printed `386x638 of 1896x638`
against the defect, in the same run that reported it. `band_panel_work_inspector_dialog_remount` drives
the ordering deliberately (the re-mount connected to `process_frame` FIRST, so it runs ahead of the
armed fit's resume — which is where every live re-mount runs, input and applied snapshots both
preceding a handler armed a frame earlier) and asserts the DRAWN height against the strip's, paired
with `_assert_work_inspector_is_a_dialog` and a non-degenerate strip rect, since a card drawing nothing
is trivially not too tall. **The plain mount is measured first as the negative control**, or the claim
says nothing about the re-mount having been the trigger. Sabotage-verified: with the three changes
reverted the plain mount still passes at 298 and the re-mount fails at `638 drawn, content 278 + chrome
20, room 638`.

**The bound is an UPPER one, deliberately** — the card is legitimately SHORTER than its content in a
room too small for it (`band_panel_work_inspector_dialog_tight`), where its scroll carries the rest;
the defect is only ever taller.

## THE ROW IS TWO LINES, AND THE INSPECTOR'S SENTENCE PAID FOR THE SECOND ONE

**Line one is IDENTITY AND CONTROLS** — stripe · glyph · name · the SOURCE-RUNG mark · the
rung-on-offer slot · policy/⚠ marks · the `−/+`. **Line two is the ACCOUNTS, then the FLOOR** —
`+0.97 /turn · 50% left standing` — full width, indented onto the name's own column by
`WORK_ROW_ACCOUNTS_INDENT` (the icon slot plus its separation, derived rather than measured, the
stripe living outside both lines' container). One row is therefore `WORK_ROW_TWO_LINE_HEIGHT` — 44px,
and its three terms are the ones `HudWidgets.build_two_line_stepper` already spends:
`WORK_ROW_HEIGHT`, `TWO_LINE_STEPPER_SEPARATION`, and a part at `ALLOC_SECTION_FONT_SIZE`, whose
measured line height is the one `WORK_INSPECTOR_NOTE_LINE_HEIGHT` states. **The board's own 13px type
has no measured line height anywhere in this client**, which is why the second line takes the strip's
10px register rather than the row's.

**THE ACCOUNTS LEFT LINE ONE BECAUSE 356px DOES NOT HOLD BOTH.** Everything on a row but the name is
fixed-width, and the accounts had a 46px slot (`WORK_ROW_RATE_WIDTH`, retired): so the row fell
through **food → fodder → materials** picking exactly ONE, and the material arm further named one
material and counted the rest (`+0.24 fibre +3`). Both were WIDTH compromises rather than readings —
a hay meadow paying meat AND feed had to choose — and the slot was still taking the name's pixels: at
46px the name measured **96px** and ellipsised on any species longer than `Hunt Red Deer`. On its own
line the list is stated **in full**, `SourceForecast.yield_components` like every other per-turn
readout, and the name column measures **146px**.

> ### THE 16px CAME OUT OF THE INSPECTOR, AND ONLY THE WHOLE SENTENCE WOULD PAY IT
>
> The taller row costs the work zone **16px**, in exactly one state — fund mode on, one queued build,
> a row selected — where the board floors to a single row and that row grows. It is not a function of
> source count: it is the same 16px at 9 sources and at 34, and `PANEL_HEIGHT_WIDE` is out of travel
> (456 + 24 crosses `BAND_ZONE_TALL_MIN_HEIGHT` and flips the band flank's tier).
>
> **The strip's one-sentence readout is what paid, and dropping a CLAUSE of it would have freed
> nothing.** It read *accounts · 50% left standing · ● Working · 3 assigned* on ONE `Label`, so
> deleting the now-redundant accounts left a line surviving its other three clauses at exactly the
> same height. The whole `Label` and its `ZONE_BLOCK_SEPARATION` went instead — 20px, which pays the
> 16 — and `WORK_INSPECTOR_EXTENT` fell **78 → 58**.
>
> **So the FLOOR travelled to line two with the accounts.** Of the three surviving clauses it was the
> only one the row could not otherwise state: the stepper's count IS `N assigned`, and *pending* is
> the row's amber name and its SIGNAL stripe (`band_panel_build_queue_pending` renders both beside a
> confirmed row). `HudComposeVocab.FLOOR_VALUE_FORMAT` is the phrasing the floor presets' tooltips and
> the chart's caption use, so one number is never worded two ways.
>
> **What the strip still carries:** its head (icon + name + `✕`), the overdraw line, the under-kept
> `note`, the `muted_note`, the `ArrivalStrip` when the schedule is gappy, the three SECTIONS with
> their controls, and a TWO-button actions row (`Jump to source` / **`Unassign`**). Every one of those
> is either a control or a warning the row has no room for. **It read "the FOUR links (Jump to source
> / Change policy / Priority / Unassign) and, when open, exactly ONE of the two pickers"** until §4.9
> item 12d's second pass drew every picker at once and demoted three of those links to headers.

**A HUNT ROW'S FODDER IS A STRUCTURAL ZERO** (no animal is harvested for feed) and renders no term;
the material terms read the assignment's **RESOLVED `material_yield`** — what the source actually
credited this turn, never a rate the compose sheet would project (`labor-ui.md` → "AN INEDIBLE QUARRY
QUOTES WHAT IT PAYS"). A trade branch stood between food and fodder until arc #527 (`⇄+0.22`, the
retired `FoodIcons.TRADE_GOODS_GLYPH`), and for one release after it the wolf's row read `+0.00`.
**A row with no CONFIRMED yield states no account clause at all** and its line two is the floor alone,
which is the pending row's ordinary face.

**THE ELIDE SURVIVES AS A FLOOR AND THE FOUR-CROP WORST CASE STILL REACHES IT — by 10px, and the cut
lands on the FLOOR.** A `Label` with no overrun behaviour reports its whole text as its minimum width,
so in this `clip_contents` zone one long line clamps the entire tab's column up to that width and
slices the right edge off every row's stepper (measured at **528px of the 356px zone**, with the name
allocated Godot's 1px floor). What the two-line row promises is that the ACCOUNTS never have to be
cut: all four crops need **241px of a 322px line**, and it is the trailing `50% left standing` that
takes the line to 332. **The floor's only home is that line** — the strip's sentence is gone and
`HudFormat.floor_hint` carries the zone's prose rather than the percentage — so line two carries the
WHOLE of itself on its own hover, `HudWidgets.set_label_tooltip` at `MOUSE_FILTER_PASS` (the rung
slot's rule: STOP across the row's widest control would punch a full-width dead hole in a row that is
one click target). **The name is still what may never yield** — a row the player cannot identify is
useless whatever else it shows.

**THE COST IS PAGING, AND IT IS ACCEPTED.** A page falls from 8 rows to 5, so nine sources is two
pages in most states. The pager already exists for it; the board must not shrink back to fit more,
and no new `ScrollContainer` may be added (the panel sanctions exactly two).

**`SourceForecast.capped_material_components` IS RETIRED**, with `ONE_SLOT_MATERIAL_LIMIT`,
`MATERIAL_COMPONENTS_UNCAPPED` and `MATERIAL_OVERFLOW_FORMAT` — line two has the width and the map's
on-tile plate sizes to its measured run, so no caller had one fixed slot left and an unreachable cap
is a thing the next reader assumes is load-bearing. `signed_material_components` is the plain joiner
again. Frames: `band_panel_work_material_forage` / `band_panel_work_material_crops`, whose four-crop
row reads `+0.06 fibre · +0.07 grape · +0.06 tea · +0.07 tobacco` whole.

### THE PLAYER'S RANK LEADS LINE TWO, AND LEADING IS THE WHOLE ARGUMENT (§4.9 item 9b)

A worked row carries the player's own rank — *where this band gives it up when it cannot cover
everything it holds* — and a marked row prints it at the **head** of line two:

```
High priority · +0.07 tobacco · 50% left standing
Low priority · +0.18 /turn · 50% left standing
```

**A `Normal` row prints NOTHING, and its line two is byte-identical to what it printed before the
mark existed.** The default is the overwhelming majority of rows, and a prefix on it would spend this
board's scarcest resource — line width — saying that nothing has been decided.

**LEADING, BECAUSE THIS LINE ALREADY ELIDES AND THE TRIM LANDS ON THE TAIL.** The four-cash-crop
worst case reaches the elide by design (241px of accounts, 322px of line, the trailing
`50% left standing` taking it past), so a rank hung on the END would be the first thing cut — on the
widest board, which is to say exactly when a famine makes the rank matter. First on the line is the
one position the trim cannot reach. **Measured with the mark on: 310px of that 322px line — 69 mark
+ 241 accounts — so the accounts still render whole (253 allocated) and the floor is still what the
trim takes.** `band_panel_work_priority_widest` is the frame and
`_assert_marked_row_accounts_still_fit` the claim; the number is printed beside it, because a future
account format has to be measured against it.

**IT IS ITS OWN `Label`, NOT A SPLICE INTO THE ACCOUNTS STRING.** The accounts carry
`OVERRUN_TRIM_ELLIPSIS`, an unconditional hover of their own whole text and `MOUSE_FILTER_PASS`;
a spliced prefix would sit inside the text the trim measures AND inside the tooltip that repeats it,
so the accounts would start being cut to make room for a word that never needs cutting. The prefix is
fixed-width and `MOUSE_FILTER_IGNORE` (the accounts own this line's hover), and the 20px
`WORK_ROW_ACCOUNTS_INDENT` is unspent — the mark sits inside the name's column with the accounts, not
beside the stripe. The two share one `HBoxContainer` at **zero separation**: the prefix carries
`WORK_INSPECT_SENTENCE_SEPARATOR` in its own text exactly as the accounts carry the one before the
floor clause, so line two's spacing is stated in one place instead of half in a string and half in a
container constant.

**THE INK IS `SIGNAL` FOR HIGH AND `DANGER` FOR LOW** — this HUD's two ends of *the player has
singled this out*: SIGNAL is what a surface wears when it is the thing being attended to, DANGER what
it wears when it is the thing that gets given up. Both are resolved by
`HudWorkVocab.work_priority_ink`, a **function** rather than a `const` table, because `HudStyle`'s
palette entries are `static var` (the theme is swappable) and a `const` initialised from one is a
parse error.

**⛔ THE WIRE ORDINALS ARE NOT THE SHEDDING ORDER, AND NO GDScript EVER SEES THEM.**
`SourcePriority` numbers `Normal = 0, High = 1, Low = 2` — Normal first because a FlatBuffers scalar
equal to its default costs no bytes — while the band sheds **Low, Normal, High**. The native decoder
therefore inserts the lowercase WORD (`dict/population.rs`), which is also exactly the token
`work_priority` takes, so the picker echoes back the string it was shown and nothing between the
button and the socket has a second spelling to invent. A client handed the number would sooner or
later sort on it and paint that untruth. The words, the reading order (`WORK_PRIORITY_LEVELS`), the
faces, the prefixes and the hint all live in `hud_work_vocab.gd`; `HudWorkVocab.work_priority_of`
normalizes anything unrecognised to `normal`, the `upkeep_fund_mode` rule — a control offering three
choices must not be handed a fourth token that lights none of them.

**THE MARK SURVIVES A PENDING CREW EDIT.** `assign_labor` states no priority — it is
`work_priority`'s alone — so `HudBandLaborState.effective_worker_map` carries the CONFIRMED rank onto
the pending row, exactly as it does the published crew ceiling and the improvement. Without it a High
mark would blank for the one frame the `+` is being clicked in, and the player would watch their own
prefix flicker off their own press.

### …AND THE CONTROL IS ITS OWN SECTION (it was a fourth LINK, with the two pickers exclusive)

⛔ **THE LINK RETIRED WITH `_work_picker_open`** (§4.9 item 12d, second pass) — see "…AND IT STOPPED
PRETENDING TO BE A CRAMPED STRIP". The retired shape: *"The work inspector's links row is Jump to
source · Change policy · Priority · Unassign — the rank beside the floor because the two are the same
kind of control (a standing property of this row, picked from three buttons), and ahead of the
destructive one, which stays last. Measured: the four-link row asks 289px of the 356px side-dock zone,
and the zone's widest content is unmoved at 354 of 356."* The rank is a SECTION now, drawn on every
open card; the row is two pure actions and `Unassign` still stays last. **The control itself did not
change at all**, which is why everything below still holds.

`HudWidgets.build_work_priority_picker` is `build_floor_picker`'s shape down to the shared
`_policy_rung_cell`: three equal cells — **High · Normal · Low**, the current one wearing the primary
treatment — under a single hint line, `WORK_PRIORITY_HINT`:

> When something runs short, the band spends it on high priority first.

**ONE sentence, naming no resource, deliberately.** The rank orders the shedding walk's workers today
and the pen-feed split as of the same slice; a per-consumer list would have to grow every time
another scarcity handler learns to read it.

> #### ⛔ RETIRED — `_work_picker_open`, the three-valued state, and the whole exclusion it enforced
>
> §4.9 item 12d, second pass. **Quoted rather than deleted, because every word of it was true of a
> strip inside a fixed zone**: *"`_work_picker_open` IS A THREE-VALUED STATE, NOT TWO BOOLS. It was
> `_work_floor_open: bool`. Two bools would admit a fourth state — both pickers open — that
> `_work_inspector_height` would have to reserve for, and the strip's tallest state is what the work
> zone's box is sized against. A three-valued state cannot express it: opening either picker CLOSES
> the other by assignment, through the one `_toggle_work_picker`, so the exclusion is structural
> rather than a discipline two link handlers have to keep."*
>
> **BOTH pickers open is the SPECIFICATION now**, so the state, its toggle and the three links that
> drove it are gone and the card is stateless about what it is showing. The one clause that outlived
> the rest is the diagnosis under it — that the tallest state is what a fixed box is sized against —
> and the answer to it was to stop having a fixed box.
>
> **The base extent is unmoved** — the links row gained a fourth link, not a line, so
> `WORK_INSPECTOR_EXTENT` is still 58, and it survived the demotion of three of those links to
> section headers for the same reason. The priority picker is
> `WORK_INSPECTOR_PRIORITY_PICKER_HEIGHT` = the floor picker's three cells **plus one hint line and
> its block gap** (52 against 32). It was the taller of the two, and *"`WORK_INSPECTOR_CEILING_HEIGHT`
> counts IT — the ceiling is a max over the pair, never a sum"* was the point of saying so; the
> ceiling is a SUM over all three sections now (374), so the comparison is history and the 52 is
> simply one term of it.
>
> **THE 444-of-396 THAT PICKER ASKED OF A WIDE DOCK WAS NEVER RENDERED, and item 12c's measurement is
> what found it.** It had overrun the horizontal dock since this slice shipped, for the reason this
> file records twice: every picker-open frame was a tall dock and every wide-dock frame had the
> expansion closed. §4.9 item 12d fixed it BY CONSTRUCTION rather than by a patch — the strip stopped
> competing for zone height — and the dock/viewport matrix is what closes the frame-family gap.

**THE COMMAND IS `work_priority <faction> <band> <x> <y> <level>` / `… <herd_id> <level>`**, emitted
as `BandPanelController.work_priority_requested`, relayed by `HudLayer` and formatted by
`Main.format_work_priority` — `build_order`'s relay path exactly, including how the tile-vs-herd tail
is chosen (a non-empty herd id is the herd form, else two integer tokens name a tile), which is how
the sim's own parser chooses. **It names a BAND** for `build_order`'s reason: the ordering it feeds is
a band's — the shedding walk partitions that band's rows and the pen-feed split serves that band's
stores — where `unqueue` and `build_kit` are source-addressed because their subject is the ground.

**⛔ NO OPTIMISTIC OVERLAY, AND THEREFORE NO ROLLBACK HANDLE.** `LaborAssignment.priority` is captured
LIVE off the allocation the command mutates and the server re-captures after every command, so the
new mark arrives on this command's own recapture — `buildKitId`'s rule and item 9a's `buildQueue`'s.
A local pending copy would be a second statement of one value, which is the drift §4.9 forbids, and a
send that does not go leaves nothing behind to undo. The pick CLOSES the picker, exactly as a floor
pick does.

## The aggregates carry a SIBLING (issues #337 / #449 / #527)

**THE AGGREGATES CARRY A SIBLING TOTAL, NEVER A FOLDED-IN ONE.** The header's food figure and each
chip's food figure stay `actual_yield`-denominated — that is the sim's larder identity — but omitting
a second account entirely made the header *visibly* not add up: `3 sources +0.35 /turn` with a source
paying only that account directly beneath it, reading as contributing nothing. So the per-row rule is
applied one level up: a second total beside the first, shown only when non-zero.

**FODDER IS THAT SIBLING** (issue #449), and it credits the band's `FODDER` store and never the larder,
so folding it into the food figure would break the identity
`larder_delta == income − consumption − pen_feed − raid_forfeit`. The head reads `2 sources +0.20
/turn +0.40 fodder` (`WORK_FODDER_TOTAL_TOOLTIP` making the beside-not-in point) and a chip covering
only hay-bearing patches reads `🌿 1 · 0.40 fodder` — via `SourceForecast.magnitude_components`, the
bare-magnitude twin of `yield_components` (a chip states levels, not deltas, so no `+`). A kind whose
whole set pays fodder alone drops the food term rather than printing a `0.00` denying its sources
produce anything. **The word, never a glyph** — fodder has none, and the `⇄` that marked the retired
third sibling is gone with it (`WORK_TRADE_TOTAL_TOOLTIP` went too).
`_work_component_sum(models, key)` is the zone's ONE summing primitive, so head and chips add the same
rows the same way. Frames: `band_panel_work_fodder` for the positive and
`band_panel_work_trade_totals` for the paired negative — a head that rendered the total
unconditionally passes every claim made on a band that actually grows hay.

**"Sort by yield" is ONE TIER PER ACCOUNT, not a raw magnitude sort** (`_work_sorts_before`):
food-paying sources first by their food figure descending, then fodder payers by theirs — with the
sources paying nothing in any account last, where they belong. Sorting on food *alone* was the bug —
it interleaved every food-less source among the zero rows at the bottom of the board, off page one on
a busy band, the same "this source is worth nothing" reading the per-row work removed. **The tier list
grows AND SHRINKS with the accounts**: it was food-then-trade when a sown hay Field published `0.00`
in both and sank into the pays-nothing tie — the same failure, one account later, which is what bought
the fodder tier — and arc #527 retired the trade tier, taking it from three tiers back to two. A new
account means a new tier here, in the same order the readouts state them.

**Ranking by raw displayed magnitude is a DIFFERENT error and must not be "fixed" back to it.** Two
accounts' figures compare quantities the sim publishes **no exchange rate** between, and under a
control labelled *sort by yield* that asserts one source is the more productive — a claim the game
does not make and the player cannot check. Tiering asserts nothing about an exchange rate; it only
orders attention. **Food leads not because it is worth more per unit** but because the larder is the
live survival constraint the player decides against every turn.

Frames `band_panel_work_trade_rows` (mixed board — `+0.22 hide` beside the deer's `+0.20` and the
patch's `+0.15`) / `band_panel_work_trade_inspector` / **`band_panel_work_trade_totals`** (the
aggregate-suppression path the mixed board cannot reach). **The three keep their names and their
subject moved**: they stage an inedible quarry. `_assert_work_material_readouts` carries the claims,
and the DEER beside the wolf is what makes them bite — "always print the materials" passes every
positive and fails the control. The rule and the axis contract live in `labor-ui.md`.

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
simplification is that the label prefixes already group by kind, so alphabetical order would do — and
that is a claim about today's WORDS rather than about the model. `WORK_FILTER_FORAGE` selects on
`kind == "forage"`, so a board ordered by label alone stops matching the chips above it the moment the
two orders part.

> ⛔ **THEY PARTED UNTIL `docs/plan_standing_upkeep.md` §4.9 item 12c, AND THEY NO LONGER DO — so the
> shipped vocabulary has stopped being the witness.** The dead claim, verbatim: *"A plant row's label
> is resolved through `WORK_ROW_PLANT_FORMATS`, keyed on the crew noun `HudFormat.plant_crew_label`
> returns — so a source whose Cultivate improvement is done renders `WORK_ROW_TEND_FORMAT`,
> `"Tend (%d, %d)"`, while its `kind` stays `forage` (the format is DISPLAY ONLY). Alphabetically
> `Forage < Hunt < Tend`, so a label-only sort renders a band working a wild patch, a herd and a
> Tended Patch as Forage → Hunt → Tend: the forage kind split in two with the hunt block wedged
> between."*
>
> Item 12c collapsed both plant formats into `WORK_ROW_PLANT_FORMAT` (`Harvest (%d, %d)`) and
> **`"Harvest" < "Hunt"`**, so on every board the shipped roster can produce the label order and the
> kind order coincide. **The rule is unchanged; its FALSIFIER had to be re-staged.** Measured:
> dropping the comparator's kind term fails exactly ONE assertion, and it is the synthetic pair
> (`_assert_work_sort_groups_by_kind`, whose labels run opposite to their kinds, marked as synthetic)
> rather than the mixed-rung board this paragraph used to point at.

Sorting on `kind` means no third label prefix can break it. The kind test is a **boolean tier**, which
is exact for the two kinds that exist; a third kind would need an explicit rank, since a boolean
cannot express one. `_work_sorts_before` was the same idiom until fodder made it three — which is the
worked example of that limit, not an exception to it: its tiers are a cascade of `has_component`
tests, one per account, precisely because a bool could not say "food, else trade, else fodder". Arc
#527 retired the middle tier and it is a cascade of TWO today; the cascade stays, because what made a
bool wrong was that the list can grow, not that it happened to be three.

**BOTH comparators tiebreak on the model's `key`, and that is a correctness fix, not tidiness.**
`sort_custom` is **not stable** in Godot, and a tie is reachable in each mode: two herds can carry the
same label (two "Wild Boar" herds produce identical `"Hunt %s"` strings), and two sources can carry
the same rate — two patches at one food figure in the food tier, and every source paying **nothing in
any** account sitting at `0.0` together in the LAST tier. (Not "every source paying no food": each
tier's test is `has_component` on *that tier's own* figure, so a patch paying food and no fodder is in
the food tier and never reaches the fodder comparison, and a hay Field paying only feed is in the
fodder tier and never reaches the pays-nothing tie.) Without the tiebreak neither sort is a total order, so
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


## The under-herded ⚠ reads the POOL's share, and its note names the band's Husbandry role

A Hunt row whose managed herd is under-kept wears the established `⚠` and a WARN note in its
inspector strip. **It measures this herd's SHARE of the band's husbandry pool against the herd's
keeping demand** — `SourceForecast.is_under_kept(live_herd, prefix)`, the one test the herd drawer's
`Keepers` row also calls — and **the instruction attached to it names the WORKFORCE zone's Husbandry
card** (`HudWorkVocab.WORK_ROW_UNDER_HERDED_NOTE`, *"Animals drifting off — raise this band's
Husbandry role."*), with the row TOOLTIP (`WORK_ROW_UNDER_HERDED_TOOLTIP`) stating why the stepper on
the row does not answer it.

**IT HAS BEEN RE-AIMED TWICE, AND BOTH MOVES WERE FORCED.** Containment was originally read off the
HUNTING crew, so the board's stepper and the warning moved the same number and
`SourceForecast.source_worker_cap_state` raised the take cap to `herdersNeeded` so the `+` could
reach it. The three-crew split made the keeping a per-source `maintain` allocation, that floor
retired (`labor-ui.md` → "THE DIP IS RETIRED"), and the warning began counting the keeper crew.
**Maintenance has since left the tile** (`docs/plan_standing_upkeep.md` §2.5): there is no per-source
keeper crew to count, so that reading went to `0 < wanted` on every managed herd in the game and the
⚠ would have been permanently up — a warning measuring the wrong thing, which is worse than none.

**So the trigger is the published SHORTFALL**, which is what the sim's own shed reads
(`herd_herded_fraction` gates it on `upkeep_supplied`). The old objection to a shortfall test — that
it speaks once animals are already leaving — is answered by the rung's GRACE: `neglectGraceRemaining`
counts the forgiven turns and the card's `At risk:` row states the countdown, so the ⚠ still arrives
before the shed does.

- **The note names the ROLE CARD, because that is the only control that moves the number.** It named
  the compose sheet's `KEEPERS` row for one release, which is now a control the player cannot find.
- **The reason goes in the TOOLTIP, not a second strip line.** `_work_inspector_height` reserves ONE
  open height for every row, so a line added here is paid for by every open inspector; a tooltip
  costs no layout at all. `build_status_part` is a bare `Label` with no autowrap, so the note's
  LENGTH is a width budget in the 354px narrow-shell zone — keep a reworded note short, or the strip
  overruns its clipping host and `band_panel_preview`'s recursive bounds assertion says so.
- **The herd DRAWER's own line took the same correction** (`DetailFormat.HERDERS_SHED_FORMAT`, now
  *"…This herd wants N of the band's Husbandry hands."*). It is the same instruction one surface
  over, and it has been stale twice for the same reason. See `herd-readouts.md` → the Herd staffing
  bullet.
- **THE NOTE WINS THE `note` SLOT rather than yielding to whatever was in it.** It shares that slot
  with the overstaff note, and the two could not co-occur while containment came off the hunting crew
  — a herd cannot be short of hunters and overstaffed with them at once. With the take crew separated
  they can, routinely; they are not equal in weight, so the slot is not first-come.
- **A HERD MID-BUILD RAISES NO KEEPER WARNING, and what says so is the METER rather than a zero.**
  `upkeepWorkersNeeded` used to be suppressed mid-build and the gate could read that silence as
  *"nobody is owed keepers, so this is a build"* — it publishes on **both sides of completion** now,
  because since `docs/plan_standing_upkeep.md` §4.6a the keeping pool owes a meter's rate **from the
  first work banked**, at any fullness. So that inference would light this ⚠ on every source being
  improved in the game — on a bill the Husbandry pool really does owe, which is precisely why the
  ⚠ is the wrong shape for it.
  `SourceForecast.is_under_kept` asks `build_is_in_flight` instead, which agrees with the drawer's own
  rung row: a build in flight states its turn count (or its `∞`), never a keeper verdict.
  **The rung that is going up gets its OWN warning instead** — see the section below.
- **THE SHORTFALL IS THE CONFIRMED ONE and there is no optimistic overlay for it.** It is resolved
  sim-side from a pool the client never composes, so a pending edit cannot move it: raising the
  Husbandry role clears the ⚠ on the first snapshot that carries the new share, and never before.

Frames: `band_panel_under_herded`, and the A/B pair `band_panel_keepers_short` /
`band_panel_keepers_staffed` — one herd, one hunt crew, only the herd's POOL SHARE moving. The third
claim rides with them and is PNG-less, because no picture can carry it: a board with twice the
hunters on it looks exactly like the short frame.

## …AND A PART-BUILT RUNG GETS THE SAME ⚠ AND THE SAME NOTE — one test, not two (§4.6a)

A player who starts a Tame and re-tasks the crew loses animals AND a 25-turn meter, and every
keeper-shaped reading on the source used to say `0` of `0` and nothing wrong. **It is the same
silent-loss class as the shed and it wears the same ⚠** — and, since §4.6a, the **same note**.

**IT WAS A SECOND WARNING AND THE SECOND ONE LIED.** `SourceForecast.is_unbuilt_and_unpaid` and
`HudWorkVocab.WORK_ROW_UNBUILT_NOTE` (*"Nobody is building this — staff its BUILDERS."*) existed
because `docs/plan_standing_upkeep.md` §2.4 gave an at-risk meter to **whichever crew owned it** — a
rung that stood was owed its keepers, a rung still going up was owed its builders. **The keeping pool
owes it at every fullness now**, so the note pointed at a lever that does not settle the bill.
`is_under_kept` is the one test on both webs and both sides of completion, the `unbuilt` model flag is
gone, and the row's note comes from `HudWorkVocab.under_kept_note(kind)`.

⛔ **AND THE SURVIVING FLAG IS SPELLED `at_risk`, NOT `under_herded`.** It was named for the animal
web because that is the web every reader of it was on; §2.7's material half made a row short of a
GOOD set it too, with its work account paid in full, and *under-herded* then named a headcount nothing
on the row reads. What the key holds is **this source is losing its rung, for any reason**, which is
the source card's own `At risk:` vocabulary and the spelling `SourceForecast.upkeep_state`'s gate
already uses. Renamed at the producer (`BandPanelController._work_source_models`), at the row tint
that reads it, and in `band_panel_preview`'s three assertions — **no alias was left behind**. The
fixture and frame names that still say `under_herded` describe a herd genuinely short of HERDERS,
which is a state and not this key.

**THE PAIR THAT SURVIVES IS PER WEB, NOT PER CREW.** *Animals drifting off — raise this band's
Husbandry role* on a hunt row, *This ground is slipping — raise this band's Agriculture role* on a
forage one: the same sentence about different consequences and different role cards. What the merge
costs is that neither distinguishes a rung being RAISED from one being HELD — it need not, the player
doing the same thing either way, and the source's own card says which on its rung row.

- **THE WIRE ALREADY CARRIED IT.** Read off the sim rather than assumed: `herd_keeping_rung` answers
  for any owned herd, so `upkeepDemand` is the pastoral rung's `work_per_turn: 1.0 × keeper_load`;
  `herd_upkeep_supply` returns `NO_UPKEEP_DEMAND` when no verb is in flight (and `activity_work(n)`
  for the builders when one is); `advance_husbandry` **zeroes `upkeep_supplied` every turn**, so an
  unstaffed build's shortfall is the whole demand. `upkeepShortfall` is published and derived, and
  `hasNeglectGrace` / `neglectGraceRemaining` come with it.
- **THERE IS NO GATE LEFT.** It was `keepers_wanted == 0` (*"nobody is owed keepers, so this must be
  a build"* — an inference off a field's meaning, dead the moment `upkeepWorkersNeeded` began
  publishing on both sides of completion), then `build_is_in_flight` stating the same thing directly.
  §4.6a removed the need for either: the shortfall alone is the trigger, whatever the meter's
  fullness.
- **THE TRIGGER IS STILL THE SHORTFALL, NOT A CREW COMPARISON** — the one place on either surface
  where that is true, and only because the wire publishes no crew requirement to compare against. It
  is the band's KEEPING pool that came up short, at any fullness (`docs/plan_standing_upkeep.md`
  §2.4), and what a build crew decides is only whether the meter outruns the resulting rot — hence the
  note reads *this rung is sliding back* rather than *nobody is building this*. **A count derived by
  dividing the shortfall would be a client inventing a number the sim never stated**, and there is no
  build-crew threshold to quote beside it either: the rate a builder had to beat retired with the
  fullness test, and the compose sheet states the rate as a standing PRICE instead.
- **IT IS THE KEEPER WARNING**, so there is nothing left to be exclusive with. It still WINS the
  `note` slot over the overstaff note, which is keyed on something else entirely: some hands bringing
  nothing home is a smaller thing than the ground or the flock being lost.
- **BOTH WEBS REACH IT.** The test reads `upkeep_state` off whichever source the row is about, so a
  walked-away Cultivate warns exactly as a walked-away Tame does.
- **The herd drawer's `Keeping:` row forked with it, and then RETIRED** (issue #545). It said *"the
  build's crew holds it"* on a build being worked and *"its builders are not covering that"* when the
  shortfall said otherwise — a fact about the BUILD, stated one row away from the meter the player
  would act on. That fork is the rung row's own `⚠ ∞ turns` now, with the `At risk:` row beneath it
  carrying the cost and the countdown it always did. See `selection-card.md` → "RETIRED — `Keepers:`
  and `Keeping:`".

Frame: `band_panel_unbuilt_rung` (a part-tamed Aurochs, hunters on it, nobody on the improvement — ⚠
up, the rung-in-progress `◎60%` mark beside it, and the BUILDERS note in the strip).


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

### THE KEEPING WARNING ARRIVES AT DECLARE TIME, NOT A TURN LATER (§4.7)

Reported from play: queue a Tame, staff the builders, advance — and *only then* does a warning appear
saying the herd wants a Husbandry hand. **One decision arriving as two warnings a turn apart.** Ray:
*"If we know the very next turn we will need a husbandry worker, we should warn the user at job
creation time, not the next turn."*

The lateness is structural: the keeping bill switches on when the first work is banked (that is when
the source gets an owner), so the warning is a consequence of state that does not exist until the turn
resolves. **But the client can project it** — a rung's standing price is published for an *unstarted*
rung precisely so the offer face can quote it. No wire field, no sim change.

- **The pool card's `⚠` fires the moment the job is queued** — the same mark, extended from *this web
  is short* to also *a queued job will need this web and nobody is on it*. The mark and the `+` that
  fixes it stay the same object.
- **The trigger is UNSTAFFED, never SHORT.** A pool already carrying enough for its holdings plus this
  job is not marked — a mark that fires on adequacy means nothing within a session.
- **One mark, one sentence.** `is_short` and the queued line are exclusive, or a pool that is both
  says the same thing twice. `POOL_CARD_SHORT_META` carries **the mark**, not `is_short`, or every
  harness reading it would be blind to the whole new case.
- **The queue row's tooltip states the job's full price**, both halves, through the same
  `DetailFormat.build_price_clause` the `⌃` uses — so the offer and the queued entry cannot quote one
  price in two wordings.
- **IT IS QUOTED IN WORK, NEVER IN HANDS.** My spec drafted *"Needs 1 Husbandry hand"* and that was
  wrong on the arc's own rule — how many hands a rate takes depends on what they carry, and gear now
  moves that. `A job in this band's queue will need 3 work a turn from Husbandry once it starts, and
  nobody is on this pool.`
- **Zero pixels.** Both pieces ride tooltips and an existing mark; the zone has none to spend.

**The claim that proves it is the THIRD one**: advance a turn and the mark is *unchanged*. The first
two (marked when unstaffed, unmarked when staffed) pass on a warning that still arrives late; only
"no second warning appears" is the defect. Sabotage-verified — removing the declare-time trigger fails
exactly that one with `before false, after true`.

### …AND THE `⌃` IS THE CONTROL THAT DECLARES THE BUILD (§4.7a ①)

The mark was a `Label`. It is a **`Button`** now, and pressing it opens the DESTINATION TRACK — the
declaration is the pick on that card. **§2.8 superseded the one-click form this section describes**: a
queue entry names a destination and lays every rung on the way, so the section above ("…AND THE `⌃`
OPENS A LADDER TRACK") is what the control does today, and everything below is the reasoning it
inherited.

**What it replaced was a trap.** The only way to order a build was the tile compose sheet's
`🌱 Cultivate this patch` checkbox, which is not the commit: the only thing that commits it is a button
reading **`Forage`**, so ticking the box and closing the sheet did nothing at all. Reported from play,
repeatedly.

- **THE SIM'S ORDERING CONSTRAINT IS SATISFIED BY CONSTRUCTION.** `cultivate` / `sow` / `tame` / `corral`
  reach only bands **already working the source** (`queue_build_on_working_bands`), and a work row exists
  precisely because this band works it. That is why the press sends the verb ALONE and no `assign_labor`
  ahead of it.
- **The two building states stay `Label`s.** There is nothing to declare on a rung in flight, and the `⚠`
  on a stalled build is a warning rather than an offer. `MOUSE_FILTER_STOP` on the button so the press
  does not also bubble to the row's own click and open the inspector.
- **The payload is `DrawerComposeController`'s, key for key**, relayed by `HudLayer` to the same
  `improvement_requested` — the arrangement `unqueue_requested` already had with two emitters, so
  `Main.format_improvement` serves both and there is no second command builder.
- **The declaration is written to the pending overlay BEFORE the emit**, so the queue's pending row and
  the row's own `▦0%` land on the frame the mark was pressed. It is `record_pending_assign`'s existing
  `improvement` argument — **never a second overlay** — and `Main` rolls it back through
  `drop_pending_assign` when the send fails. Ordering matters and is not stylistic: see `hud-modules.md`
  → "AN OPTIMISTIC WRITE NEEDS A ROLLBACK", whose rule the `assign_labor` twin had silently drifted from.
- **The `⌃` LEAVES THE SLOT the instant it is pressed** — the overlay's declaration makes
  `RungGates.rung_in_progress` answer, so the building state takes the slot. Proven rather than assumed:
  after three presses the board holds zero `⌃` buttons and three pending queue rows, and clearing the
  overlay restores all three.
- **The tooltip carries the job's PRICE** — `75 work · 4 work a turn from Agriculture to hold` — and no
  turn count. The queue row's date is the sim's own chained answer and a second estimate here would be
  two producers for one number. The price is on this tab because Ray moved it off the compose sheet:
  *"That information should be on the work tab. No need to have it here, it is useless."*

**The press is asserted on BOTH webs**, driven through the real handler with the line read back off
`Main.format_improvement` — either web alone passes on a builder that gets the grammar backwards.
Sabotage-verified three ways: the wrong verb fails the command claim naming it; dropping the overlay
write fails *"all 3 declarations are in the build queue on the same frame (found 0)"* while the command
claim correctly stays green; dropping the unworked-ground fork fails exactly one of the compose sheet's
two remedy frames.

Frames: `band_panel_rung_ready` (a tended patch offers Sow, a tamed pen-ceiling Aurochs offers Corral,
a wild-ceiling Roe Deer offers nothing — the CONTRAST is the point), `band_panel_rung_ready_filter` and
`band_panel_ready_declare`.

### …AND THE `⌃` OPENS A LADDER TRACK, BECAUSE AN ENTRY NAMES A DESTINATION (§2.8)

The sim stores **one position per source** in cumulative work units — `plant:tended` runs `0 → 50` and
`plant:field` `50 → 125` — and a queue entry names a **destination rung** rather than a single rung.
An entry climbs every rung between where the source stands and that destination and stays at the head
until it ARRIVES, so `sow` declared on untended ground is a TWO-LEG climb that costs the whole branch.

**A one-rung mark cannot state that.** Pressing `⌃` used to queue the source's next rung outright, so
the control's whole vocabulary was *this rung, now* on a model whose unit of decision is *how far are
we taking this*. It opens a small **ladder track** instead, and the PICK is the declaration.

```
TAKE IT TO…
  Wild                                banked
  🌱 Tended Patch                where you are
  ▦ Field                     75 work · ≈24 turns
```

- **THE CARD IS A `PopupPanel`, AND THAT IS A CORRECTNESS DECISION.** The work zone reads **396 of
  396** in height and **354 of 356** in width with a row selected, and both budgets ASSERT rather than
  clip — so a track drawn as a block would fail the harness at best and slice the board at worst. A
  Window cannot change any zone's height, which is exactly why the detail breakdowns are popovers and
  the destructive confirms are `ConfirmationDialog`s. It costs the zone nothing at all, and the two
  rendered states run `_assert_zone_content_fits` with the card UP, which is that claim asserted.
- **THE CARD IS REBUILT PER OPEN, NEVER PATCHED** — the track is a function of the source's position,
  the faction's knowledge and whatever entry is queued, and all three move per snapshot. The Window is
  reused because a Window is expensive; its content is not. **Its inner `MarginContainer` is the chrome
  and is never freed** — clearing the Window's own children frees the very margin the next line reaches
  for, and `queue_free` is deferred, so that renders correctly ONCE and opens onto an empty card ever
  after. That is a defect no frame can show and a second open catches immediately.
- **A RUNNING BUILD'S FACE OPENS THE SAME TRACK, and that is what gives the chosen path a live home.**
  *How far are we taking this?* is asked most often mid-climb, and until the track existed the only
  answer was to withdraw the entry and declare again; the work banked is kept either way, being a
  position on the branch rather than a purchase of one rung. **A STALLED build stays a `Label`** — the
  `⚠` is a warning rather than an offer, and a button under it would invite a click that changes
  nothing about why the meter is stuck.
- **SO THE SLOT'S NODE TYPE STOPPED ANSWERING *is this an offer*.** `HudWorkVocab.WORK_ROW_BUILD_KIND_META`
  is on the slot beside the face it drew (`offer` / `building` / `stalled`), because `control is Button`
  now counts a climb as an offer — a wrong answer that no assertion in the block would have flagged.
- **THE COMMAND DID NOT MOVE.** The four verbs always WERE destinations, so picking a rung emits the
  verb that names it and the sim works out the legs: no new token, no new grammar, and
  `_emit_ready_declaration` takes the picked rung instead of reading the model's `ready_policy`.
- **THE `⌃` STILL MEANS *ready*, so a source whose only rung above it is locked shows no mark.** That
  is deliberate: the mark promises the verb is AVAILABLE, and putting a chevron on every wild source in
  the game to advertise a locked rung is the failure `RungGates.next_rung_ready` exists to prevent. A
  locked rung is seen from a track opened on a source that has SOMETHING ready — which is the state a
  player is in whenever the ladder matters.
- **THE HOVER SAYS WHAT THE PRESS DOES.** `WORK_ROW_READY_TRACK_TOOLTIP` replaced
  `WORK_ROW_READY_QUEUE_TOOLTIP_FORMAT`'s promise of a one-click queue; the PRICE line beneath it is
  unchanged and is still `DetailFormat.build_price_clause`'s.

**WHAT A BANKED RUNG SAYS IS THE POINT OF THE READOUT.** It states its STATE and no figure at all —
the fifty work it once cost appears nowhere on the card — because a previous improvement is a
**RECEIPT, NOT A DISCOUNT**: the player is never asked to buy work already bought, and is never
offered it back either. The in-flight leg quotes what is LEFT of it from where the source stands
(twenty of fifty, not fifty), which is the wire's own `workRemaining`.

**AND A DESTINATION IS BARRED BY ANYTHING BELOW IT.** A climb lays every leg, so a locked rung bars
everything above it and those rungs state the blocking rung's own reason rather than a second refusal
invented for them. Offering a destination whose path is refused is a job that queues and then blocks —
which is the state §4.6b's whole `Blocked` vocabulary exists to explain after the fact.

### A MULTI-LEG ENTRY IS ONE QUEUE ROW, WITH ITS LEGS INSIDE

The row names its **destination** (`▸ ▦ Sow (66, 25)`), not the leg it happens to be on: a row headed
`Cultivate` on a `sow` the player ordered would rename their job to its first leg, and rename it again
when that leg finished. The legs are the row's EXPANSION:

```
▸ ▦ Sow (66, 25)              turn 64 (60%)   ✕
  CLIMB
   ▸ 🌱 Tended · 20 work · ≈5 turns
     ▦ Field · 75 work · ≈24 turns
  CROP  [Sim picks ⌄]
```

- **ONE UNIT, deliberately.** Splitting a two-leg climb into two queue rows would offer two `✕`s for
  one withdrawal and two places to drag for one reorder.
- **THE LEG IN FLIGHT IS THE FIRST ONE, and nothing here decides that** — the wire lists them
  first-incomplete first. It wears the queue head's own `▸` and the cyan a rung under construction
  wears on the board, and every other leg reserves the marker's slot, the block's standing rule one
  level in.
- **A LEG THE WIRE DATES WITH A SENTINEL STATES ITS WORK AND NO TURN.** A leg cannot be dated when the
  entry carrying it cannot, and a fabricated number is worse than the silence.
- **THE STRIP'S HEIGHT IS ITS CONTENT, AND THE ARITHMETIC STAYS IN ONE PLACE.**
  `build_queue_block_height` took a lone BOOL precisely so the number lived once; a strip that also
  lists legs has a height that varies, so what a caller states is now the CONTENT (`settings_legs`,
  `settings_crop`) and `HudWorkVocab.build_queue_settings_height` is the one arithmetic both the
  reservation and the render read. **The leg list's own `CLIMB` key costs a line** — forgetting it is
  how a strip draws taller than it was paid for, which this zone answers by clipping the board.
- **BOTH WEBS EXPAND NOW.** The row used to be clickable only where there was a CROP to configure, so
  an animal entry never opened; every entry has legs, so the predicate is *legs or crop* and what
  differs is the strip's content. A row still never invites a click that opens an empty strip.

**Frames:** `band_panel_rung_track` (wild ground, Cultivation known and Seed Selection not — the
LOCKED rung, visible with its reason, beside the open one) · `band_panel_rung_track_banked` (the same
ground one rung up on a faction that knows both crafts — the BANKED rung, and the claim that exactly
ONE row on the card quotes a price) · `band_panel_rung_track_climbing` (a two-leg entry queued to
`plant:field`, opened from the RUNNING slot — the path, the target, and both wire figures by
EQUALITY) · `band_panel_queue_legs` (the queue row opened into its climb). The four are judged as a
SET: a card that marked every rung `open` passes any one of the state claims alone.

### …AND A PLANT RUNG DOES NOT COMMIT UNTIL A CROP IS NAMED (§4.15)

Ray, from play: *"User selects the crop to keep … maybe we default to nothing and force them."* The
`⌃` declared in ONE click and sent **no species token**, so every Sow took the sim's own default — the
**highest-share legal plant**, which considers neither what that plant pays nor the player's take
selection. That is how a fertile tile got committed to a zero-food cash crop.

So picking a plant rung on the track opens a **second page of the same card**, and the CROP is the
declaration:

```
WHAT TO GROW                      ← on a SOW; a Cultivate reads WHAT TO TEND
  🌾 Wild Grain 70%
     38 work · 3.40 food
  🚬 Tobacco 20%
     150 work · 0.00 food · 1.12 tobacco
  Sim picks
     → 🌾 Wild Grain 70%
  ‹ Back
```

- ⛔ **THE TITLE IS PER RUNG, BECAUSE A CULTIVATE GROWS NOTHING.** The tended rung WEEDS — the favored
  share rises toward `tended_weeding_gain`, the volunteers beside it are still wild, and
  `tended_conversion_gain` multiplies that one species' yield vector — so the choice there is which
  plant the band gets good at, not what it plants. Only the FIELD rung plants, forcing the favored
  share to 1.0 and every other to 0. `RUNG_CROP_TITLE_TEND` / `RUNG_CROP_TITLE_GROW`, chosen in
  `build_crop_step` from the improvement it is committing. Reported from play as a nit, and it is the
  MODEL rather than the wording: `WHAT TO GROW` over a Cultivate tells a player the rung does
  something it does not do.
- ⛔ **THE ROWS STATE WHAT EACH CROP PAYS, AND THAT IS THE ACTUAL REPAIR.** Forcing the choice only
  RELOCATES the trap if the list is names and shares: the player picks the dominant plant again,
  because it looks like the obvious answer. Nothing on the path from *this ground is fertile* to *this
  field feeds nobody* states the zero unless a row does — so the FOOD clause is the one clause in
  `HudFloraVocab` exempt from the render-only-when-non-zero rule and prints `0.00 food` outright.
- ⛔ **AND EACH ROW STATES ITS OWN SOW PRICE, WHICH IS THE OTHER HALF OF THE DECISION.** The work
  figure was one number per PATCH — `fieldWorkCost`, struck against the patch's commitment or the
  rung's auto-pick — so every crop in the list was quoted the DEFAULT crop's price while the payoffs
  beside them moved, and the true one appeared only once the leg started and re-quoted. A picker
  exists to weigh work against payoff; with the work half wrong for every crop but one it did the
  opposite. `FloraShareInfo.sowWorkCost` rides each composition entry and is quoted as published.
  It LEADS the row, a price being read before what it buys.
- **A CROP THE WIRE PRICES NO SOW FOR RENDERS NO ROW.** Absence is the sim's "this plant cannot climb
  to a Field on this ground" — the tile-specific legality `can_sow` (a SPECIES ceiling) structurally
  cannot express — so a row would offer a job that cannot be ordered, and a `0` would read as a free
  Sow. It is the same predicate `default_species_for_rung` filters on, so the rows that survive are
  exactly the plants `Sim picks` chooses between.
- ⛔ **NOTHING HERE IS DERIVED FROM ANYTHING ELSE HERE.** A committed patch's published `share` is its
  REWEIGHTED one while `sowWorkCost` is struck on the tile's own basket, which is what the sim charges
  against; and the per-crop price and the patch's own `fieldWorkCost` agree by construction for the
  crop a patch is committed to (asserted sim-side on the encoded envelope) rather than by one being
  computed from the other. Quote each as published.
- **THE FIGURES ARE THE RUNG'S OWN, per plant** — `sow_payoff` / `cultivate_payoff` and their fodder
  and material twins off the composition entry, which is what THIS rung would pay once it stands. The
  per-biomass rates beside them describe the wild stand being gathered today and answer a different
  question; the two rungs differ in KIND rather than by a factor, so the rung is passed in.
- **ONE CLAUSE PER MATERIAL, never a summed materials figure** — the retired trade axis under a new
  name, which the crop picker is the last surface that could reintroduce.
- **`Sim picks` STAYS AND STOPS BEING THE DEFAULT.** `""` is a real instruction on the wire and
  choosing it deliberately is fine. It renders **LAST** (a leading default is what a hurried player
  takes) and its aside names the plant it would resolve to, so it is no quieter about the consequence
  than the rows above it.
- **ANIMAL RUNGS STAY ONE CLICK.** `tame` and `corral` commit no species, so a second step there would
  be a click that answers nothing — `RungLadder.rung_commits_a_crop` is the fork. So does a plant rung
  on a basket carrying no plant it may legally take: the sim accepts a Sow with no token and settles
  it itself, and a step with nothing in it is worse than none.
- **THE CROP GOES FIRST AND THE RUNG SECOND**, which is the commands' own order: the crop rides
  `assign_labor`'s `species` token on the band's existing forage row (`_emit_work_assign`, the queue
  row's picker's path — **no second builder and no wire change**), and the declaration follows so its
  optimistic overlay is the one the rebuilt board reads.
- **IT COSTS THE ZONE NOTHING**, being the same `PopupPanel`; both rendered states run
  `_assert_zone_content_fits` with the card up.
- **THE QUEUE ROW'S OWN CROP PICKER IS UNTOUCHED.** This adds the choice at DECLARATION; changing it
  before the job starts is still the settings strip's.

### …AND THE FIELD ROW STATES ITS PRICE AND NO REASON (§4.15)

`plant:field`'s build cost is scaled by the chosen crop's share of the tile — sowing a crop that
already holds most of the ground is mostly tidying, sowing one that holds a tenth means replacing the
tile — so two Sows are quoted at wildly different work. The declaration path's own Field row states
that price:

```
▦ Field                            150 work
```

- ⛔ **THE PRICE IS THE WIRE'S `fieldWorkCost` AND IS NEVER RE-DERIVED.** It is what will be charged
  for THIS patch's Sow — the crop it is committed to, or the rung's auto-pick where it has none.
- **THE CAUSE IS NOW SHOWN RATHER THAN NARRATED.** A crop-and-share sentence sat beneath the price for
  one release, naming the plant the figure was struck against; the crop step one press away states
  every legal crop's OWN price, so the sentence explained a variation the player can now simply read.
  Ray, on it: *"too wordy. If it is known, we don't need any of that text."* `RUNG_TRACK_SOW_PRICE_
  NOTE_FORMAT`, `RungLadder`'s `ROW_NOTE_KEY`, `_price_note` and `_priced_crop_entry` are all retired,
  and no rung on the track carries an aside but a LOCKED one's gate reason.
- **THE TWO FIGURES AGREE BY CONSTRUCTION FOR THE COMMITTED CROP, and the client asserts rather than
  derives.** The per-crop `sowWorkCost` for the crop a patch is actually committed to IS that patch's
  `fieldWorkCost` — one sim-side expression, checked on the encoded envelope — so neither surface may
  compute the other's number.
- **Only the Sow rung has a per-crop price at all.** Cultivate's cost is unscaled and both animal
  rungs' multiplier is the species', which the row already names — so the crop step quotes work on the
  Sow rung and nothing on the others.

**Frames, judged as a SET:** `band_panel_rung_price_cheap` (uncommitted ground, the patch priced at
its auto-pick's `38 work`) · `band_panel_rung_crop` (the step that rung opens — `38 work · 3.40 food`
against `150 work · 0.00 food · 1.12 tobacco`, and the third plant the wire prices no Sow for absent
entirely) · `band_panel_rung_price_dear` (the SAME basket COMMITTED to the minority crop, the patch
now at `150 work`). A Field row quoting a constant passes either price frame alone, and a picker
quoting one price per patch passes every claim a one-crop basket can state.

## DENIAL is a third MISSION on the parties footer, not a floor on the hunt form

`docs/plan_denial_raid.md`, slice 2. The parties zone's footer offers **three** verbs now — `⚑ Scout`,
`🏹 Hunt`, `💀 Deny` — and the third is a mission rather than a preset because **the thing it changes is
a BOUND, not a number**: `fauna::quantise_animal_take` clamps a hunt's kill to what the party can
carry, so `floor = 0` still only kills what it can haul and there was nothing for a floor to unclamp.
`ExpeditionMission::Deny` drops that arm (`EngagementStop::Never`); the party never stops engaging.

**WHAT THE FORM DOES NOT CARRY IS ITS SPECIFICATION.** `_fill_denial_compose_sheet` renders
QUARRY → PARTY → verdict → take → send and **no floor picker, no floor hint, no crew preset and no
max-useful cap**. Each absence has its own reason and none is an oversight:

- A floor would be a control the **command grammar cannot express**.
  `send_denial_raid <faction> <band> <party_workers> <fauna_id>` is closed at four tokens and a fifth
  is a hard parse error, which is why `Main.format_send_denial_raid` is its own builder rather than a
  branch of `format_send_hunt_expedition` (whose optional floor tail that parser would reject) and why
  the HUD carries a **separate `send_denial_raid_requested` signal** with a payload that has nowhere
  to put one. (The hunt grammar is closed after its floor now too — see `labor-ui.md` → "RETIRED —
  the FILL TARGET" — so the two differ by that one optional token rather than by two.)
- There is **no `expedition_useful_cap` twin**. That cap exists because a hunting raid's delivered
  payload plateaus once the herd's surplus binds; a denial raid has no payload to plateau, and more
  hands always break the herd sooner.

### The stepper's ceiling is the band's IDLE WORKERS, and its floor is the sim's own requirement

**`max_expedition_party_size` is not a rules cap and NO launch form applies it.** It is the wire echo
of the LAST RUNG of `expedition_config.estimate_party_sizes` — the top of the estimate tables' SAMPLED
party axis, and the only quoting bound there is, having absorbed the retired `deny.max_party_quoted` —
and the sim deleted the rules cap for all three launch verbs, so the client's own clamp was the last
thing enforcing it: a band with 16 idle workers was clamped to 8 while the sheet's own refusal told it
to send more hunters. A party past the top rung is **quoted at that rung, with a note naming it**
(below), never refused. All three forms read the band's idle workforce and nothing else — the denial sheet
and the SCOUT branch take `idle` directly, and the hunt form's `assignable` is `idle` under
`expedition_useful_cap`, which is the DEMAND side and is untouched (it is about what the raid can
*use*, not what the rules *allow*). `idle == 0` behaves exactly as before, every spelling yielding 0.
**The `_scout_party_max` helper is DELETED rather than left returning its argument** — a supply
function that clamps nothing is an invitation to put the clamp back. `SourceForecast.expedition_party_cap`
is the surviving named seam, for the herd drawer's expedition branch and the dock's hunt form
(`labor-ui.md`).

**The stepper SEEDS on the reply's `party_needed`** — the smallest party the sim quotes whose raid
SUCCEEDS, i.e. whose kills outpace the herd's regrowth. **`horizon` is not a success and not
`repelled`**, so "the first row that is not `repelled`" is the wrong test and shipped as one:
`SourceForecast.denial_outcome_succeeds` over `DENIAL_SUCCESS_OUTCOMES` is the client's ONE spelling
of the set, and `band_panel_preview` pins it against the verdict table's own `VERDICT_OK` entries. Below it a raid accomplishes literally nothing however long it runs, and nothing
else on the sheet said which number crossed that line. Three invariants:

- **Never seeded to `SourceForecast.DENIAL_PARTY_NEEDED_NONE`.** `0` means the sim quotes no party at
  all, not "send nobody", so the count stays where it was and the verdict line carries the answer.
- **Seeded once per quarry selection AND once per sheet OPENING**, through the hunt form's
  `arm_party_autofill` / `consume_party_autofill` one-shot — one mechanism, two arming sites, so a
  manual `−`/`+` tick survives every later rerender. `TargetingController.choose_quarry` — the ONE
  adoption of a quarry, taken by both the map pick and the tile chooser — arms it, and the footer's
  `💀 Deny` button arms it too, so a sheet that came back up on a quarry it still remembered cannot
  present whatever count the last composition left behind. **The open-site arm is currently a GUARD
  rather than a behaviour**: the same handler calls `_clear_party_quarry()`, so a freshly opened sheet
  has no quarry to seed against and the observable seed still comes from the adoption. It is what keeps
  the invariant true if the open path ever stops clearing.
- **Clamped into `[WORKER_STEP, idle]`.** A requirement above the band's idle workers opens on the most
  it can field, which is honest: the sheet shows both numbers and the verdict still says it is not
  enough.

**The `repelled` refusal names that party whenever there is one.** `DENIAL_VERDICTS`' repelled entry
carries TWO reason strings and `denial_refusal_reason` picks between them on the herd's own
`denial_party_needed`, never on the wording: `reason_counted` takes `[quarry, needed]` and states the
count, and where the sim quotes none the numberless `reason` stands verbatim, because inventing a
figure there would be a promise the sim did not make. `SourceForecast.denial_party_needed` is the ONE
reading of the field, so the stepper's seed and the sentence beneath it cannot quote different numbers.
`DENIAL_OUTCOME_HORIZON`'s reason is deliberately untouched — its remedy is a bigger party, but the
quoted requirement is a fact about the repelled rows.

### The BEYOND-REACH rule is the hunt's, and denial does not inherit it

Reported from play: deer and rabbit a few tiles from camp were not offered as denial targets while
herds further out were. The quarry row, its picker and its chooser are the hunt form's reused
verbatim — **the eligibility rule is not**. A hunting party exists for game the band cannot work from
home, so a nearer herd is a local hunt and that split is correct for it. Denial is not a way of
GETTING food: it is a way of ERASING a herd, and hunting the warren next door at `floor 0` cannot
express that, a hunt being carry-bounded and stopping at the pack. So **a denial raid may name any
herd the band can see and reach, in reach or not, and the hunt's rule is untouched.**

- It stays an **EXPEDITION** and is deliberately not a labor assignment: the party detaches, spends
  turns killing and comes home. That is a real cost in hunter-turns even at zero distance, and denial
  has no floor and no rate to put on the assign dialog.
- The mechanism is a per-mission parameter on the ONE rule, never a second rule —
  `TargetingController.quarry_min_distance(band, mission)`, spec in `targeting.md` → "Command
  Targeting". Every quarry question (`_fill_denial_compose_sheet`'s re-validation, `_build_quarry_row`'s
  picker and tile chooser, the map pick, MapView's glow) passes the OPEN SHEET's
  `_party_compose_mission` through it.
- **The verdict already reads correctly at zero travel** and nothing had to change for it:
  `_denial_turns_from_launch` leaves both ends unshifted at `travel <= 0`, and `denial_turns_clause`
  appends `DENIAL_TRAVEL_SPLIT_FORMAT` only where there IS travel to split off — so a quarry on the
  band's own tile reads *"Rabbit Warren past recovery in ≈5–8 turns from launch"*, never
  *"(0 of them travel)"*. That is why the sentinel for "no band supplied" is `-1` and not `0`.
- **The server gates none of this.** `handle_send_denial_raid` → `outfit_raiding_party` validates a
  resident band, a live herd and a legal party size, and nothing else; the sim's only `hunt_reach`
  test is on the LOCAL `LaborTarget::Hunt` assignment. An in-reach `send_denial_raid` is accepted.

### The readout is a COLLAPSE VERDICT, not a delivery

Its goal is not to kill every animal: it is to push the herd below `ecology.collapse_fraction`, where
growth zeroes and the decline is irreversible, and walk away. So a denial party deliberately publishes
**no `expeditionProjectedDelivery` / `expeditionEtaTurns` / `expeditionTripBound` at all**, and its
`expeditionFloor` (`0.0`) is the mission reporting that it HAS no such lever — never a value it
chose. Every hunt-only readout is therefore gated on
`HudExpeditionVocab.EXPEDITION_MISSION_HUNT` and not on "is a raid".

`SourceForecast` holds the layer, a pure lookup into `HerdTelemetryState.denialEstimates`:
`denial_forecast` → `denial_verdict` (the ONE resolution of the outcome key) →
`denial_verdict_text` / `denial_verdict_bbcode` / `denial_take_bbcode` / `denial_refusal_reason` /
`style_send_denial_button`. **`DENIAL_VERDICTS` holds all four faces of an outcome in one entry** —
line, whether it quotes turns, the button, the severity, the reason — the `HUNT_EMPTY_REFUSALS` idiom,
and for the same reason: three lookups are free to disagree.

- **`repelled` and `horizon` are NOT interchangeable, and the arc has already shipped that confusion
  twice.** `repelled` is a verdict about the **PARTY** (its kills do not outpace the herd's regrowth,
  so no amount of waiting gets there — the remedy is HANDS); `horizon` is a verdict about the
  **CLOCK**. Rendering one for the other blames the herd for the party's problem.
- **NEITHER OUTCOME BLOCKS THE SEND, and the ONE case that does is not an outcome at all.** A raid
  that cannot get there keeps working the herd until it is recalled (`plan_denial_raid.md` §6 Q2), so
  the launch verdict warns (`armed`) and the player is trusted, exactly as a slow hunting raid is —
  **including a party the player has deliberately stepped DOWN below the requirement**, which is the
  `repelled` warn-and-trust case and keeps `Send Anyway (never collapses)`.
  **`SourceForecast.denial_is_short_handed(herd, idle)` is the exception**: `denial_party_needed > idle`
  with a `denial_party_needed > 0`, i.e. the band cannot field the party this herd REQUIRES however it
  dials the stepper. That is a fact about the BAND rather than a choice, so there is no choice to trust
  the player with; the Send goes visible-and-disabled-with-its-reason (`DENIAL_SHORT_HANDED_BUTTON`,
  ghost), the same shape as the sheet's no-quarry branch. **`DENIAL_PARTY_NEEDED_NONE` never disables** —
  `0` is not "not enough hunters" but "no quoted party drives this herd down", which covers a quarry
  nothing can bring into contact (wariness ≥ 1) where more hands never help.
  `denial_short_handed_reason` states BOTH numbers and **SUPERSEDES the repelled refusal rather than
  joining it**: both name the party the sim quotes, off the one `denial_party_needed` reading, so
  printing the pair would state the requirement twice.
- **THE OUTCOME LEADS THE SENTENCE AND THE NUMBER IS A CLAUSE ON IT.** That is the structural form of
  *"never render a blank turn count without its outcome"*: there is no branch in which the number can
  render alone, and none in which its absence renders as silence. An outcome that quotes no turns
  (`repelled`, `horizon`) carries `turns: false` in its entry and the clause is never appended.
- **`0` ON A TURN FIELD MEANS "NOT WITHIN THE HORIZON" ON THAT END, never "immediately"**, and `low`
  is the FEWEST turns.
- **THE EXPECTATION LEADS THE SENTENCE WHEREVER THE SIM BOUNDED ONE, and the spread follows it.**
  Every other number on this sheet — the kill count, the food hauled, the waste left on the range — is
  priced at `turns_to_collapse`, so a verdict leading with any other draw describes a different raid
  from the take line two rows beneath it. Reported from play: a Red Deer raid read *"≈12 turns on a
  good run"* over a take of 180 kills, which is the FORTY-SEVEN-turn expectation's take. Both numbers
  were individually true, which is why nothing caught it.
- **AN UNBOUNDED END IS STATED AS UNBOUNDED, NEVER DROPPED.** The rule this replaced quoted `low`
  alone whenever `high` ran past the horizon and printed no expectation at all — the one figure that
  matched the rest of the sheet was the one never rendered. `denial_turns_clause` has five shapes and
  no branch of it can print a lone optimistic number as though it were the answer:

  | shape | reads |
  |---|---|
  | all three bounded | `in ≈20 turns from launch — between 12 and 31 depending on the run` |
  | `high` unbounded | `in ≈47 turns from launch — as few as 12 on a good run, and a bad one may not finish` |
  | the EXPECTATION unbounded | `only on a good run — ≈12 turns from launch, and the raid is not expected to finish inside the forecast` |
  | `low == high` | the lead figure alone — a degenerate distribution, so `between 8 and 8` would be a spread for nothing |
  | nothing bounded | no clause; the outcome word stands alone |

  "On a good run" is the right words in the THIRD row and nowhere else — there the expectation itself
  ran past the horizon, so luck genuinely is the only way there, and the clause says so outright.
  `denial_turns_phrase` is the LEAD figure alone (and the gate the caveat rides on); the spread lives
  in the clause, so "which number leads" is answerable in exactly one place.
- **THE BAND IS AN ESTIMATE, NOT A PROMISE, AND THE PANEL SAYS SO.** `turns_to_collapse` is an
  integral over many stochastic retreat draws, so a lucky run really can finish sooner than the
  reported low (measured: a seeded raid landed on turn 7 against a reported low of 8). Every form
  wears `≈`, and `DENIAL_ESTIMATE_CAVEAT` rides under any verdict that quotes a number — and under
  none that does not, since a caveat about an absent number reads as one that is there.

### The verdict counts from LAUNCH, and the span is named in the sentence

`turns_to_collapse` counts the turns the party spends **working the herd**. Reported from play: the
verdict read *"Wild Boar past recovery in ≈5–8 turns"* beside a HUNT readout on the same sheet that
had always added its round trip (`HUNT_FORECAST_TRAVEL_BREAKDOWN`) — two missions quoting bare turn
counts that meant different spans, and the denial one short by the walk.

**The OUTBOUND leg is in scope and the RETURN leg is not**, and the asymmetry with the hunt readout is
the reason. A hunt's payload only counts once it is carried home, so its headline is the whole round
trip. A denial verdict is about the **herd crossing a threshold** — an event that happens on the range
the moment the party arrives and starts killing — so the walk home falls after the event the sentence
is about and adding it would over-state the wait for the thing being promised.

- **`SourceForecast.outbound_travel_turns` is a READING of the round trip, never a second measurement**:
  `ceil(round_trip / TRAVEL_LEGS_PER_ROUND_TRIP)`, which is exactly `ceil(one_way / move_rate)` by the
  nested-division identity `ceil(ceil(x)/n) == ceil(x/n)`. There is one definition of travel in this
  client and it mirrors the server's launch feed; a fresh `hex_distance ÷ move_rate` here would be a
  second one free to drift.
- **`denial_forecast` shifts every BOUNDED end and leaves `DENIAL_TURNS_BEYOND_HORIZON` alone.** `0` is
  not a turn count — it says the projection never bounded that end — and turns of walking do not bound
  it either.
- **BOTH SURFACES NAME THEIR SPAN; neither is bare.** The launch sheet passes the band and reads
  *"…in ≈7–10 turns from launch (2 of them travel)"* (the split rendered only where there IS travel to
  split off, the hunt breakdown's own rule); the in-flight drawer passes NO band, carries
  `DENIAL_TRAVEL_UNKNOWN`, and reads *"…in ≈3–5 turns of raiding"*. The sentinel is `-1` rather than
  `0` for the `HUNT_RATE_UNAVAILABLE` reason: a band standing on its quarry has a real zero-turn walk
  and must still read *from launch*.
- **The in-flight surface quotes the raiding span because it cannot honestly quote the other one.** A
  denial mission publishes no `expeditionEtaTurns`, so the party's REMAINING walk is not on the wire,
  and adding the leg from the HOME BAND's tile would quote a distance the party may have finished turns
  ago. Closing that needs a per-party arrival on the wire — server-side work.

### The `horizon` verdict says HOW LONG the forecast is

*"Wild Aurochs is still standing when the forecast runs out"* names a clock the player cannot see — the
same hedge the hunt sheet's *"away many turns"* was, and unactionable for the same reason. Where the
cohort carries `expeditionForecastHorizonTurns` the sentence quotes it:
**`%s is still standing after %d turns%s`**, `line_bounded` on the `DENIAL_VERDICTS[horizon]` entry.

- **The figure rides the SAME clock the turn clause would have.** It is shifted by
  `_denial_turns_from_launch` and closed by `denial_span` — the ONE resolution of *from launch* vs *of
  raiding*, extracted so the sentence and the clause beneath it cannot name two spans in one verdict.
  A launch sheet reads *"…after 68 turns from launch"*; the in-flight drawer, which passes no band,
  reads *"…after 60 turns of raiding"* — **the HUNTING bound, named as such**, rather than a trip
  figure it has no travel term to support.
- **No `≈`, unlike every collapse figure on this sheet.** The band is an estimate over stochastic
  draws; the horizon is a config constant and the walk is arithmetic, so the number is exact and
  wearing the estimate glyph would misdescribe it.
- **The `turns` flag stays `false`.** This outcome has no collapse figures to quote, and the clause
  states the collapse band; what the sentence states is how long the projection ran before giving up.
  The composition therefore lives in `denial_verdict_text` via `_denial_bounded_line`, not in the
  clause.
- **`denial_forecast` takes the horizon off whichever cohort the caller has** — the band on a launch
  sheet, the launched party through the trailing `horizon_cohort` argument in the in-flight drawer
  (`DetailFormat.expedition_collapse_line`). It is a global lever echoed on every cohort, so any
  cohort answers it; `horizon_cohort` is read for the horizon ONLY and never for travel, which is the
  whole reason that caller passes no band.
- **A cohort carrying no horizon keeps the bare `line`.** `0` would render *"after 0 turns"*, which is
  worse than the hedge.
- **PNG-less, driven, and asserted as a set** in `ui_preview`'s `chapters/band_expedition.gd`: the two
  spans by EQUALITY against sentences spelled out there (the pair is the claim — a builder ignoring
  `travel` satisfies the first alone, one that always shifted satisfies the second alone) plus the
  no-lever fallback. There is no rendered `horizon` denial row in either harness, and a sentence is a
  string: a frame would show a plausible verdict whichever clock it quoted.

### The waste is STATED, and it is not dressed as a warning

On a hunt an unhauled kill is an occasional overflow and wears `HUNT_FORECAST_WARN_GLYPH`'s `⚠`; on a
raid it is essentially the whole take and it is the **point** of the mission. `denial_take_bbcode` is
therefore a quiet `INK_DIM` line — `kills ≈55 Wild Boar · brings home 6.00 food · leaves 214.00 on the
range` — with each account rendered only when the quarry pays it (the render-only-when-non-zero rule),
and no alarm glyph anywhere.

**AND IT SALVAGES MATERIALS, which on an inedible quarry is the whole of what it brings home.** The
line stated its kills and stopped for a wolf pack — a mission that destroys and salvages nothing,
which is false: `carry_room_biomass` answers `NO_CARRY_BOUND` for a species paying no provisions, so
the pack never fills and the party hauls every pelt. `DenialRow.delivered_material` is what it lands,
one ` · brings home 22.00 hide` clause per material, **never summed**. **The verb is repeated rather
than shared with the food clause**: that clause is optional, and a shared "brings home" would strand
the materials on a quarry that pays no meat — precisely the quarry this clause exists for. The sim
states no per-material WASTE for a denial row, so the clause below is food alone while this one is a
vector.

**RETIRED — the waste PAIR (arc #527).** The sim published `wastedFood` **and `wastedTrade`** out of
one `HuntYield::apply` over the same wasted biomass, so a kill left on the range took its hides with
it, and stated food-only a raid whose quarry pays in pelts reported its waste as zero. The trade
account is gone, so the clause is a single figure — and `DENIAL_TAKE_TRADE_FORMAT`,
`DENIAL_TAKE_LEFT_TRADE_FORMAT` and `DENIAL_TAKE_LEFT_JOIN` went with it. **Three rules it left
behind:**

- **`SourceForecast.denial_waste_face` is still the ONE spelling of "what was left on the range"**, so
  a second surface that ever states a raid's waste states it in these words.
- **A second figure in one clause needs its own joiner, never the line's clause separator.** The pair
  joined on ` and `, deliberately not on ` · ` — that is what separates this line's own CLAUSES, so
  nesting it inside one clause's subject reads as an extra clause beginning at the second figure and
  ending "on the range".
- **A quarry that wastes nothing renders NO clause, not a zero.** An inedible quarry is exactly that
  case and it is a fact about the PRODUCT: `carry_room_biomass` answers `NO_CARRY_BOUND` for a species
  paying no provisions, the pack never binds, and the party hauls everything. So the fixture that
  proves this clause has to be an EDIBLE quarry, where the pack binds hard — a wolf fixture asserts
  nothing about waste.

**The IN-FLIGHT surface states no waste at all and needed no change.**
`DetailFormat.expedition_collapse_line` renders the collapse verdict and the quoted-party note and
stops there, so there is no second spelling of the waste to keep in step.

### The mission's mark is `💀`, on all three surfaces

`HudComposeVocab.COMPOSE_MISSION_LABEL_DENY` (the footer button), `HudFormat.PANEL_EXPEDITION_DENY_GLYPH`
(the Active-parties row) and `MapView.EXPEDITION_DENY_GLYPH` (the map marker) are one glyph, so the
mission reads the same at every scale. The parties row deliberately renders **no floor glyph** — its
`expedition_floor` is `0.0`, which is a real zone (`strip`), so borrowing the hunt branch's mark would
tag a raid with a pressure it never chose. The map marker likewise takes no phase decoration: the
green food pip is a haul cue, and a denial party's haul is a rounding error it should not advertise.

### The two hunting-party entry points present ONE decision surface

The dock's parties-zone hunt sheet (`BandPanelController._fill_hunt_compose_sheet`) and the herd
drawer's expedition branch (`DrawerComposeController._build_herd_assign_controls`) compose the same
raid. They had drifted into two shapes; they now read as one stack — **Quarry / Policy + chart /
Party / Kit / forecast / Send** — off the same builders.

- **The dock sheet gained the FLOOR CHART and its draggable floor**, from `HudWidgets.build_floor_chart`
  against `SourceForecast.floor_chart_model` — the drawer's own builder and model, never a second
  implementation. **This REVERSES the rule that used to stand here** ("NO SLIDER in this zone… a
  fixed-width dock strip is not where a continuous dial belongs"): the two entry points presenting one
  decision outranks keeping the dock strip spare, and the measurement backs it — the chart needs
  **300 × 132px** and the parties zone gives it 356. `improvement` is `IMPROVEMENT_NONE` and the crew
  noun is the party's: a detached party builds nothing.
- **The chart is GATED ON THE ZONE HAVING ROOM** (`_band_zone_tier != BAND_ZONE_TIER_SHORT`), the
  established `_build_food_outlook_block` idiom. A horizontal dock's parties zone is height-capped and
  CLIPS, and the chart is ~150px of it. **The drag goes with it, and that is a consequence rather than
  a choice**: since slice 4b there is no plain-slider control left to keep — the chart's own floor flag
  IS the dial — so gating the chart necessarily gates the drag, and the SHORT tier keeps the presets
  alone. Only a COMMITTED drag rebuilds the sheet (a rebuild frees the chart and the drag dies with
  it), which is the drawer's expedition rule.
- **The drawer's expedition branch took the dock's inline `Party` row** (`HudWidgets.build_party_stepper_row`)
  in place of its `PARTY` section heading. **The LOCAL branches keep the heading and their crew NOUNS**
  — `Hunters` / `Foragers` / `Herders`, so a managed herd's keepers never read as a hunting party — and
  the crew targets that hang off that heading are a resident crew's controls anyway.

- **The dock sheet took the drawer's boxed `THIS TRIP` readout**, and the builder moved to the
  shared widget layer to make that possible: `HudWidgets.mount_trip_readout` (+ its `_trip_yield_rows`
  helper), lifted out of `DrawerComposeController` where it was private. Both sheets call the one
  builder now. **The dock's one-line sentence and its standalone bound clause went with it** — the
  box's own verdict folds the bound clause in (`SourceForecast.hunt_trip_verdict`), so keeping both
  printed one fact twice. `hunt_forecast_line_bbcode` survives as BOTH sheets' refused-state fallback
  (an empty box is worse than the sentence it replaces); `trip_bound_clause` keeps its `DetailFormat`
  reader and the verdict's own.

- **THE PARTY CAP IS RESOLVED ABOVE THE CHART, AND THE STEPPER ROW IS STILL MOUNTED BELOW IT.**
  `expedition_useful_cap`, `consume_party_autofill` and the `clampi` that settle `_send_expedition_count`
  run before `floor_chart_model` is composed; only the RESOLUTION moved, so the form still reads
  presets → chart → floor hint → Party → Kit. Composing the chart first drew its projection, its crew
  targets and its verdict for a party the stepper beneath then clamped away — on the render where
  autofill arms, which is a floor click, a committed drag or a fresh quarry. **The frame is
  byte-identical either way**, so the guard is `_assert_chart_reads_the_settled_party`
  (`HarvestFloorChart.crew()` against the stepper row's `HudWidgets.PARTY_STEPPER_COUNT_META`) and not
  a picture; the invariant across all three compose sheets, and the model key that carries the crew,
  are in `labor-ui.md` → "THE CAP IS RESOLVED BEFORE THE CHART ON ALL THREE SHEETS".

**Frames:** `band_panel_compose_hunt` (TALL — the chart present, the presets one row across) and
**`band_panel_compose_hunt_short`** (the tier gate, the only state that renders it: chart absent).
`_assert_hunt_sheet_chart` asserts BOTH halves, since a gate stuck on and a gate that never fires are
equally green to the bounds assertion — a clipped chart still reports a rect inside its host.

### A COMPOSE SHEET THE ZONE CANNOT HOLD LEAVES THE ZONE

An OPEN parties compose sheet does not fit a height-capped horizontal dock at all — **641px of a 265px
box WITHOUT the chart** (593px before it took the boxed readout): quarry row, presets, floor hint,
party stepper, kit row, forecast and send, none of which the SHORT tier drops, and the zone hosts
`clip_contents`, so what shipped was a silently sliced form with the Send button in the slice. Gating
the chart is necessary and nowhere near sufficient — trimming the remaining ~380px means deleting most
of the controls.

**So the sheet stops being confined to the box.** When the parties zone cannot hold it, it renders in
**`BandComposeFloat`** — the same single-column layout, the same builders, the same order, in a card
floated beside the panel instead of inside the zone. The two rejected alternatives are worse and both
undo work this arc already did: growing the card while a sheet is open re-introduces the
content-driven reservation (i.e. the map flicker `set_zones` exists to remove), and re-flowing into
columns makes a THIRD layout of a form two recent passes made identical across its two entry points.

- **THE TRIGGER IS A MEASUREMENT, NEVER THE DOCK EDGE.** A short VERTICAL dock and a small window hit
  the same wall, and an edge test misses both. `BandPanelController._party_compose_needed` is what the
  parties zone's whole column demanded — head, party rows, open inspector strip AND the sheet — the
  last time the sheet was rendered inside it; `_party_compose_floats()` compares it against the box
  `BandCityPanel.zone_size(ZONE_PARTIES)` currently offers. Measured: **1057px of a 1055px column** in the
  tall LEFT dock (which does NOT float it — see the slack below) against **641 of 265** in the TOP one.
- **IT IS THE COLUMN'S COMBINED MINIMUM, NOT THE SHEET'S OFFSET PLUS ITS OWN.** The footer is
  bottom-pinned by an `EXPAND_FILL` spacer, so the spacer absorbs exactly the slack and
  `sheet_top + sheet_minimum == box height` holds BY CONSTRUCTION whenever the content fits. The
  positional read — the arithmetic `_assert_zone_content_fits` uses, which is correct for detecting an
  overflow — is degenerate at the boundary and reported "2px over" on a column with 400px to spare.
  `HudComposeVocab.COMPOSE_FLOAT_SLACK` (1px) covers the rounding between a summed minimum and a
  laid-out rect and nothing more.
- **IT IS MEASURED LIVE AND ONE FRAME LATE, because Godot has no synchronous layout.** A DETACHED
  control tree shapes an autowrap `Label` at a wrap width of ZERO — every word on its own line — so a
  build-time `get_combined_minimum_size()` on this sheet over-reports by hundreds of pixels and would
  float it in a side dock that holds it comfortably. `_measure_party_compose` waits one `process_frame`
  and reads the column the panel actually laid out. The cost is that a sheet which GROWS past the box
  mid-composition (a quarry picked in a T/B dock) renders clipped for the single frame before the
  float goes up.
- **THE MEASUREMENT IS A HIGH-WATER MARK for one composing act**, reset by `_close_party_compose` and
  by a panel-band change. The sheet grows as the form is answered, and a mark that tracked every
  shrink would hop the sheet back into the zone the moment a field cleared — a layout change under
  the player's hands. Only the IN-ZONE render is measured: a floated sheet lays out at the float's own
  column, which is never narrower, so trusting that reading could hand the sheet back into a box that
  then clips it.
- **EVERY TEARDOWN PATH GOES THROUGH THE FOOTER BUILDER.** The ✕, a cancel, a send, the last idle
  worker leaving and a panel-band change all rebuild the parties zone, and the no-sheet branch of
  `_build_party_footer` dismisses the float — so there is no list of conditionals that can miss one.
  The two paths that do NOT rebuild the zone (`_close_party_compose` with no panel band,
  `refresh_snapshot` with zero player bands) dismiss it explicitly. A float outliving its sheet is the
  worst outcome available here.
- **A PANEL-BAND CHANGE CLOSES THE WHOLE COMPOSING ACT**, not just the quarry. The quarry already
  cleared there (its travel time and useful party size are band-relative); the mission, the party size
  and the measured requirement belong to that band too.
- **AN UNKNOWN BOX MEANS INLINE, NEVER FLOAT — and a guessed box is an unknown box.**
  `BandCityPanel.zone_size(ZONE_PARTIES)` answers `Vector2.ZERO` while the panel is collapsed, hidden, or
  simply not laid out yet, and `_parties_zone_box()` substitutes `ZONE_FALLBACK_SIZE` (340×360) there —
  a sane LAYOUT guess for the no-dock host and nothing like the ~1055px a tall side dock really offers.
  Deciding the fork against it turns *"I do not know yet"* into *"this overflows"*, and the high-water
  mark then latches it ON for the rest of the composing act: reported from play as an EMPTY hunt sheet
  (`Quarry: Choose…`, a hint, a disabled Send) floating out of a left dock that holds it four times
  over. `_party_compose_floats` reads **`_parties_zone_box_known()`**, which states the absence, and
  answers `false` there. **The asymmetry is the point** — floating is the drastic, instantly-visible
  branch and must be positively justified, where the worst case of staying inline is one clipped frame,
  which is what shipped for months.
- **A MEASUREMENT TAKEN BEFORE THE LAYOUT PASS IS NOT RECORDED AT ALL, AND IT IS THE SHEET THAT SAYS
  SO.** The mark never falls during a composing act, so ONE bad reading latches until the sheet closes
  — which is what made the defect above stick rather than self-correct on the next frame.
  `_party_compose_measurable` is the guard and it has three terms: the panel must be able to state the
  box the mark will be compared against, the parties column must have a rect at all
  (`COMPOSE_MEASURE_MIN_COLUMN_WIDTH`), and **the sheet must have been FITTED to that column**
  (`sheet.size.x >= col.size.x`).
  - **The third term is the fix for the SECOND report of this defect, and the first two do not
    substitute for it.** A column width says nothing about whether the column's contents are laid out,
    because the two are set by different mechanisms: the column is anchored `PRESET_FULL_RECT` into its
    zone host, so Godot gives it the host's width SYNCHRONOUSLY on reparent, while everything inside it
    is sized by the DEFERRED container sort. Measured in that window on the empty hunt form:
    `col.size.x == 356` — perfectly plausible — beside `col.get_combined_minimum_size().y == **1278**`,
    where the laid-out answer is **207**, every autowrap `Label` under it shaping one word per line.
    1278px floats that sheet out of every dock this client has (the tall LEFT dock's box is 1055), and
    the high-water mark holds it there for the rest of the act. **A bare width floor on the SHEET does
    not close it either** — an unsorted `Control` still clamps its size up to its own combined minimum,
    so the unlaid-out sheet reports a non-zero 220×903. Only the RELATION between the two widths
    separates "laid out" from "clamped to its own minimum".
- **THE WAIT IS A BOUNDED RETRY, NOT A SINGLE LOOK** (`COMPOSE_MEASURE_MAX_FRAMES`). One
  `process_frame` is the normal cost, but whether the deferred sort has been flushed by the time the
  coroutine resumes depends on where in the frame the render that armed it ran — which is precisely
  what the harness's timing never reproduced. Waiting another frame is cheap; recording a phantom costs
  the rest of the composing act, and simply returning leaves the mark unmeasured until some later
  render arms a new one. Bounded rather than open, so a sheet whose zone never lays out cannot spin a
  coroutine for the session; giving up leaves the sheet INLINE, the direction the whole fork is biased
  toward.
- **THE MARK BELONGS TO ONE BOX, AND A BOX CHANGE DROPS IT.** `_party_compose_measured_box` records
  which column the requirement was measured against, and `_note_parties_zone_box` — called from the
  ZONE BUILDER, i.e. every render, so no path can forget it — clears the mark when the box moves. A
  dock move, a collapse or a window resize asks a different question, and the previous answer would
  keep a sheet floating in a column it was never measured in.

**`band_panel_compose_hunt_short` ASSERTS the fit now, and it asserts it in three places at once.**
It used to REPORT its extent, because asserting would have failed on the defect it existed to
document. The trap on the other side is that **`_assert_zone_content_fits` passes TRIVIALLY once the
sheet leaves the zone** — an empty box fits anything — so moving the overflow somewhere unmeasured
would look exactly like a fix. The state therefore asserts that the sheet is really gone from the
zone, that the zone holds what is left, that the float fits the VIEWPORT and holds its own content,
and that it clears the panel card, plus the paired negative on `band_panel_compose_hunt` that a dock
with room keeps its sheet. See `harness-band-panel.md` for the assertion set and its sabotage results.

### THE RECALL VERB FOLLOWS THE SIM, AND A CANCEL ASKS NOTHING

`core_sim::handle_recall_expedition` folds a party back **on the spot** when
`cancel_party_standing_in_camp` holds — it is an expedition, its `home_band_entity` resolves, it stands
on that band's own tile, and `pending_reveal` is empty. Anything else takes the ordinary `Returning`
walk home. So the single-recall path is TWO different orders, and the control has to say which:
recalling a party composed and launched in the same turn *"would just go away as if I never created
it"*, and offering it as **Recall** described a round trip that never happens.

- **ONE READING OF THE PREDICATE, in `HudBandLaborState.party_cancels_in_camp`.** All three surfaces —
  the parties-zone row `✕`, the parties inspector strip's link, the Occupants drawer's button — read it
  through `BandPanelController.recall_verb` / `recall_tooltip`, so the verb they show and the ceremony
  the press gets cannot disagree. It lives on the labor MODEL because it is a question about the
  snapshot (the party, and the home band it is already grouped under by `band_parties`), not about a
  panel.
- **THE FOUR TERMS ARE MATCHED EXACTLY**, in the sim's own order. A looser test — say *"the phase is
  hunting and it carries nothing"* — prints **Cancel** over a party that really does walk home, which is
  the same lie in the other direction. Co-location is EXACT and not comm range (the sim's `Returning`
  arm folds back within 2 tiles; doing that on a *recall* would teleport workers home rather than
  cancel an order that had not taken effect), and the pack is NOT a term — "nothing to deliver" is
  about the MAP, `pending_reveal` being the one thing an out-of-band fold-back cannot flush.
- **THE CANCEL BRANCH SKIPS `_confirm_destructive` ENTIRELY.** That dialog exists for an action that
  LOSES something: the work board's unassign-all, or a real recall abandoning a trip in progress. A
  party still in camp has spent no travel and abandoned no haul, and re-launching it is one press of
  the same footer button — so a modal there is ceremony over a decision the player can simply re-make.
  **`Recall all parties (n)` keeps its single confirm and is otherwise untouched**: it acts over a
  MIXED set, and the prompt is the only place that whole scope is stated.
- **The GLYPH does not fork.** A `✕` removes the row on both branches; only the tooltip and the two
  worded controls change (`HudComposeVocab.PARTY_{RECALL,CANCEL}_VERB` / `_TOOLTIP`).
- **`pending_reveal_count` is a DECODER PROJECTION, not the wire field** — see
  `native-extension.md`. The client only ever asks whether a report is owed; the coordinates are a
  scout's accumulated reveals and would be hundreds of tiles per cohort per frame to answer a boolean.

**Judged as a PAIR, and neither half is a claim alone** — a rule that showed one verb everywhere would
satisfy either. `band_panel_preview`'s `_assert_row_recall_confirms` drives the REAL row builder and the
REAL `pressed` handler over three fixtures differing only in the terms under test: a field party
(Recall + dialog + no emit), a camped one (Cancel + emit + no dialog), and a camped one that still owes
a map report (Recall again — the case that separates the predicate from *"is it on the band's tile"*).
Sabotage results are in `harness-band-panel.md`.

### FORM A NEW BAND IS THE FOURTH FOOTER BUTTON, AND IT IS NOT A MISSION (issue #511, `docs/plan_band_fission.md`)

`⌂ Split` sits beside `⚑ Scout` / `🏹 Hunt` / `⚔ Deny` because this is where the player already comes
to divide people out of a band — but it sends no party. It opens the same compose sheet on
`COMPOSE_MISSION_SPLIT`, and pressing its confirm emits **`split_band_requested`**
`{ faction, band_id, workers }`, which `Main.format_split_band` renders as the CLOSED three-token
`split_band <faction> <band> <workers>`. **The retired `settle_expedition_requested` signal, the
parties row's `Settle` control, the inspector strip's link, the Occupants-drawer button,
`party_may_settle` / `settle_blocked_reason` and the whole `PARTY_SETTLE_*` vocabulary block are
GONE** — a scouting party is composed for scouting, so it can no longer found anything.

**IT IS GATED ON WORKERS, NOT ON IDLE WORKERS**, unlike its three neighbours. A split divides the
band; an assignment held by someone who leaves lapses with them, so a band whose every hand is busy
may still split. `_split_worker_pool` is the cohort's `working_age` straight off the dict, which is
the same quantity the sim bounds the command by (`available_workers`) — it used to `floor()` a
fractional cohort, and now there is no fraction to floor, so the stepper's ceiling and the server's
refusal cannot disagree. The footer's `SEND_PARTY_NO_IDLE_REASON` line is therefore scoped
to the three expedition missions — it used to render unconditionally on `idle <= 0`, which put "No
idle workers to spare" directly under a live `⌂ Split`.

**THE SHEET SHOWS THE CONSEQUENCE, BECAUSE THE INPUT IS ONE NUMBER.** Workers stepper → the share it
implies → what the new band would be (people, brackets, dependants/worker, provisions) → the home
band beside its now → the verdict. Everything divides on that one share.

> #### BOTH HALVES ARE APPORTIONED IN **ONE** PASS
>
> `HudFormat.apportion_people_to` exists for this: the sheet apportions the new band's dependants and
> the parent's remainder in a single largest-remainder pass against `band people − chosen workers`.
> Running it separately over each half lets both round the same way and show **31 people leaving a
> band of 30**.
>
> **This is the ONE place the client rounds people, and it is its own arithmetic rather than a second
> opinion on the sim's.** The brackets arrive whole; 9 whole children divided by a 40% share the
> player chose is 3.6 children, and somebody has to decide. That is unlike the PEOPLE block, which
> stopped rounding entirely when the wire started carrying whole people. The chosen worker count is **pinned** to the integer the
> player picked and held out of the apportionment entirely, so the stepper can never disagree with
> the readout.
>
> **EVERY `now` IS THE TWO HALVES ADDED BACK UP**, never a second reading of the band. The first cut
> quoted `_split_worker_pool` on the Workers row's `now` side while its `after` side came out of the
> apportionment, and rendered `16 → 12` beside a new band of 5 — a band counted two ways,
> disagreeing by a person, which is the whole failure the apportionment exists to stop.

**THE FLOORS ARE THE SIM'S; ONLY THE SENTENCES ARE THE CLIENT'S.** `founding_min_workers` and
`founding_parent_min_workers` ride on every cohort, and `split_blocked_reason` composes
`SPLIT_BLOCKED_NEW_TOO_SMALL` / `SPLIT_BLOCKED_PARENT_TOO_SMALL` from them — **both when both hold**,
joined by `SPLIT_BLOCKED_SEPARATOR`, because fixing one otherwise just reveals the other. The client
holds no copy of the rule: a verdict cannot cross the wire when the sheet moves a stepper, since that
would be one field per possible composition. See `.claude/rules/core_sim/fission.md`.

**THE STEPPER'S KEY IS A PARAMETER** on `HudWidgets.build_party_stepper_row`, defaulting to the word
the three expedition sheets want. This sheet passes `SPLIT_STEPPER_LABEL` (`Workers`): a sheet whose
whole claim is *this is not a party* must not label its one input `Party`.


### BOTH ESTIMATE AXES ARE SAMPLED, AND THE SHEET NAMES THE PARTY IT QUOTES

Reported from playtest: on a Wild Fowl flock the drawer laid out a full readout and the dock rendered
**nothing at all** for the same herd. **The shared box did not fix it and was never going to** — both
readouts gate on the same `available`.

`huntTripEstimates` is sampled on two axes and the client knew that about only one:
`hunt_estimate_row` read the nearest sampled FLOOR and then demanded an **exact** party-size match, so
a party above the largest sampled size found no row and every raid readout went silent. The dock
reached such a party and the drawer did not — the dock's stepper **auto-fills to
`expedition_useful_cap`**, whose engagement arm (`expedition_engage_crew`) is deliberately not bounded
by the sampled sizes, while the drawer's count is seeded from the standing staffing (1 on an unworked
herd), which always was.

**The sim's sampled party LADDER is what settled it.** `expedition_config.estimate_party_sizes` is now
`[1, 2, 3, 4, 8, 16, 32, 64]` — dense where one hunter is a large proportional change, sparse where it
is not — plus a short contiguous run at the herd's own requirement on the DENIAL table. Against a
requirement of 1 that denial axis is `{1,2,3,4,5,8,16,32,64}`, so a party of **6** had a row under the
old contiguous axis and finds none under the ladder: an exact match is now strictly worse than it was.

So **both lookups read the nearest sampled party**, exactly as the floor axis already does.
`SourceForecast.nearest_estimate_party` is the party axis's own named seam beside
`nearest_estimate_floor`, and `_row_for_nearest_party` is the one resolution both tables share. **On a
tie the LOWER rung wins** — over-quoting a party's take is the more misleading direction, and the rule
also makes the answer independent of iteration order.

**A nearby row is never presented as though it were exact.** Where the quoted rung is not the selected
party, the sheet renders a quiet line naming both — `SourceForecast.quoted_party_note` over
`HudComposeVocab.PARTY_TRIP_ESTIMATES_QUOTED_FORMAT` / `PARTY_DENIAL_ESTIMATES_QUOTED_FORMAT`, the kit
line's idiom and its reason. It differs from the kit line in one way that matters: the figures still
RENDER, because they are a real answer to a nearby question rather than another kit's numbers. Where
the selected party IS a rung — which the ladder's dense low end and the requirement run make the
common case — no note renders and nothing changes.

The party rides out on BOTH raid forecasts as `SourceForecast.QUOTED_PARTY_KEY`, so the note and the
figures it qualifies come from one lookup rather than two free to disagree. **Four surfaces, one
rule**: the dock's hunt form, the dock's denial form, the herd drawer's expedition branch, and the
in-flight `Collapse:` row (`DetailFormat.expedition_collapse_line`, where the clause rides the row
rather than a line of its own — that producer's output lands in the parties zone's clipped inspector
strip). A launched party is the surface where a between-rungs size is MOST likely, being bounded by
the band's idle workforce and nothing else.

**`expedition_useful_cap`'s plateau scan walks the sampled rungs, not `1..=largest`.** It used to step
every integer and `continue` past the sizes the table did not carry; with the nearest-rung fallback no
size is ever missing, so an unsampled 5 would answer rung 4's row, read as "the payload stopped
rising" and break the scan one rung in.

Guarded by `band_panel_preview._assert_party_past_the_rungs_is_quoted` (the inverted form of the guard
that used to pin the exact match), `_assert_party_ladder_rounding` and
`_assert_denial_quoted_party_note`.

### The KIT row rides both dock sheets, and the denial one carries the honesty rule

Both the hunting-party form and the denial form mount `KitRoster.build_kit_row` **directly under the
party stepper (and its `of N idle` / cap notes) and above everything the kit moves** — the spec, the
effective-tier rule and the command token all live in `labor-ui.md` → "THE KIT IS CHOSEN ON THE
SHEET". Two consequences are this sheet's own:

- **The denial form's payload gains `kit_id` + `default_kit_id`** and nothing else; the four-token
  grammar admits the named `kit <id>` pair and no positional. `_mount_kit_row` /
  `_mount_kit_gate_line` are `BandPanelController`'s two small helpers for it, shared by both forms.
- **When the selection differs from `denial_estimates_kit_id`, the sheet renders the COMBAT GATE and
  the quoted-kit sentence and NOTHING ELSE below the kit hint** — no verdict, no caveat, no take
  line, no counted refusal, no short-handed disable, every one of them being a figure priced for a
  raid the player is not sending. The Send stays live and plainly styled: the raid launches, only its
  length is unquotable. The hunt form takes the same treatment against
  `hunt_trip_estimates_kit_id`, additionally dropping the floor picker's metrics and the demand-side
  party cap.

### Frames

`band_panel_preview`: **`band_panel_compose_deny_kit`** (the picker CLOSED, on a band whose SLED has
run dry — so the hint reads `attack 20.0 · carry 12.0 per hunter · spears 74 · sled dry` and a hint
quoting the roster's fresh 40 fails there and nowhere else; the attack stays EQUIPPED on the same
line, which is what stops "quote the bare tier for everything" passing instead) ·
**`band_panel_compose_deny_kit_open`** (the popup — an embedded subwindow, so it lands in the capture;
the structural claims ride the assertion, a screenshot being unable to say which item carries the
radio dot: this verb's kits and only those, the default TAGGED, `none` LAST, exactly one marked) ·
**`band_panel_compose_deny_kit_mismatch`** (`none` against tables quoted for `big_game`, asserted **by
EQUALITY** over the sheet's lines below the kit hint, because half the claim is what the sheet must
NOT say). Sabotage-verified on two DISJOINT mutations: rendering the table regardless of the kit id
fails the equality assertion alone, naming the verdict, the caveat and the take line it found;
quoting the FRESH tier fails the hint assertion alone. `cargo xtask command-guard` carries the token's
half — it composes a non-default kit on all four grammars and parses every line with the real server
parser.

`band_panel_preview`: **`band_panel_compose_deny_short_handed`** (the ONE refusing frame — the
reference band's 3 idle against the deep-party quarry's requirement of 11: the stepper sitting at the
most it can field, the reason naming both numbers, and a disabled `Not Enough Hunters`. Its companion
is `band_panel_compose_deny_short_party`'s live Send on a party the PLAYER under-sized — asserted as a
pair, since a rule that disabled every repelled raid would pass the disable claim alone) ·
**`band_panel_compose_deny`** (a viable raid — the range verdict, the caveat,
the quiet take line, the primary Send, and the three absent floor surfaces),
**`band_panel_compose_deny_in_reach`** (the same viable form on a quarry standing ON THE BAND'S TILE:
the reach rule relaxed, and the zero-travel verdict reading *"in ≈5–8 turns from launch"* with no
travel split — the equality assertion carries that absence) and
**`band_panel_compose_deny_repelled`** (the SAME herd with only the sim's answer changed, so a
verdict table that answered one outcome for all four would satisfy either alone). Nine assertions ride
them and each is sabotage-verified against a DISJOINT mutation — blanking the outcome line, resolving
every outcome to `past_recovery`, ignoring the entry's `turns` flag, an always-primary Send, a
disabled Send, the hunt's `⚠` on the take, a Policy heading, a floor picker, and an unconditional
caveat. The in-flight half is `ui_preview`'s `expedition_denial_panel`; `cargo xtask command-guard`
parses the emitted `send_denial_raid` line with the real server parser, which is the only thing that
can assert the four-token grammar.

## A HEX CAN HOLD MORE THAN ONE HERD, AND THE MAP CLICK NAMES ONLY THE HEX

Reported from play against the denial sheet: a tile holding a Rabbit warren **and** a Wolf pack picks
one of them and offers no way to reach the other. The mechanism is structural, not a resolution bug —
`TargetingController.try_dispatch` is handed a **`tile_info`**, so `_huntable_herd_on_tile` can only
answer with the hex's first eligible herd, and re-clicking answers the same one. There was no input
anywhere that named a herd within a tile.

**The choice is made on the SHEET, not at the click**, and that follows from the ordering decision
this arc already made. Which of two co-located herds to raid is a comparison of *forecasts* — the
collapse verdict, the raid's payload, the useful party size, whether the quarry is even edible — and
every one of those is a function of the herd that exists only once the form is rendered. Asking at the
click asks before any of the numbers that answer it. It is also where the arc already put the quarry
question ("the herd … cannot be the LAST question"), and a map-side chooser would be a second
floating surface over the map during targeting, which §15 rules out for the compose sheet itself.

- **The control is the `⋯` menu the zone heads already use** (`_build_quarry_choices_menu` →
  `HudWidgets.build_section_menu`), so the panel keeps ONE "there are choices here" glyph. Its entries
  are **radio-check items**: a menu of plain items could not say which herd the sheet is currently
  aimed at, which is half of what the control is for.
- **It appears only where there is a choice** — two or more ELIGIBLE quarries on the picked quarry's
  own hex. One herd is the common case and its row is byte-identical to before, which the frame pair
  `band_panel_compose_hunt` (absence) / `band_panel_compose_deny_two_quarries` (presence) is what
  pins; either claim alone passes on a control rendered unconditionally.
- **The row was ALREADY a live control and the report's "inert" premise is false** — the picked-quarry
  button re-enters the map pick on both branches. What it could not do was reach a herd the map cannot
  address, which is why the fix is a second control rather than a wiring repair.
- **The chooser's width comes out of the PICK, not out of the key.** `Quarry` and the pick both used
  to `EXPAND_FILL`, so a third child halved what the name got — measured, `🐇 Rabbit Warren` came back
  clipped to `Rabbit Warre` on the very frame the chooser exists to serve — and the cure was a
  `SIZE_FILL` written into that branch alone. That special case is **gone**: the key is
  `HudWidgets.build_field_key` now, which takes a DECLARED width and never expands, so the pick is the
  row's only expanding child whether the row has two children or three. The whole field-row family
  (`Band:` · `Kit` · `Quarry`) is specified in `labor-ui.md` → "The compose sheet's FIELD ROWS are one
  family", **including the rule that this row takes the family's chrome and must never take its
  ARROW** — pressing it arms a map pick, and an arrow would promise a list that does not open.
- **`TargetingController.choose_quarry` is THE one adoption of a quarry**, shared by the map click and
  the chooser: same eligibility test, same state, same re-render. `_try_pick_quarry` is written in
  terms of it (it keeps only its two nudges and the pending teardown), so a second spelling cannot
  drift. `eligible_quarries_on_tile` derives the candidate set **LIVE from `world_herds`**, never
  stashed at the pick — herds migrate, and a captured set goes on offering a herd that has walked off
  the tile. It reads the same snapshot array `tile_info.herds` is built from, so the click's own
  resolution and the list cannot disagree about what is standing there.
- **`HudWidgets.MENU_ENTRY_ICON`** is what lets an entry carry the species' bundled ART, absent-is-not-
  empty like `MENU_ENTRY_CHECKED` beside it, capped at `HudWorkVocab.WORK_ROW_ICON_WIDTH` (a
  `PopupMenu` sizes itself around an uncapped 256px source). It exists for `build_marker_icon`'s
  reason: **Unicode ships ONE deer**, so an emoji-only menu would render two roster species
  identically and defeat its own purpose as a chooser.

### RETIRED — the per-quarry state the chooser used to have to clear

The quarry chooser landed beside a FILL TARGET, and that lever's own count was per-herd state
`set_party_quarry` / `clear_party_quarry` had to drop on every re-pick or the next raid would silently
ignore it. The lever is gone (issue #491), so those two mutators write the quarry alone. **The rule
that put it on `ComposeState` in the first place stands and is why this is recorded**: per-quarry
compose state belongs BESIDE the quarry on the model, not on `BandPanelController`, where a
`_clear_party_quarry` had to remember to clear it and the one path that set a quarry without going
through it — a re-pick on the map — carried the previous herd's value onto the new one.

### Frames

`band_panel_compose_deny_two_quarries` — a warren and a wolf pack on ONE hex beyond the band's reach,
rendered on the DENIAL form because that is where it was reported (the row is shared, so the hunt form
takes the identical control from the identical builder). The pair is deliberately a food quarry beside
an **inedible** one: they differ in art, in name and in what the raid brings home, so a chooser that
offered one herd twice could not pass. Six assertions ride it — the chooser exists, it lists exactly
two, it marks exactly the composed one, driving the popup's REAL `id_pressed` re-targets the sheet,
and the re-rendered row marks the herd now composed — plus the absence claim on
`band_panel_compose_hunt`. (A sixth assertion pinned the stale FILL TARGET being dropped on the
switch; it went with the lever.) Sabotage-verified on three DISJOINT mutations, each failing a
different subset: rendering the chooser at one candidate fails the absence claim alone; building the
entries as plain items fails the two marking claims; and dropping `choose_quarry`'s re-render fails
the re-rendered-row claim alone, naming the stale `Rabbit Warren`.

## A SHIPMENT IS THE FIFTH FOOTER BUTTON, AND IT SHARES NO FIELD WITH THE HUNT FORM (arc #527, issue #517)

`BandPanelController._fill_trade_compose_sheet`. The parties footer offers **`📦 Trade`** beside
Scout / Hunt / Deny / Split, and it is a MISSION rather than a mode of the hunt form for the plainest
reason available: there is no field the two have in common. A shipment names another BAND, not a
herd; it carries a manifest, not a floor; and its readout is a mass meter, not a trip forecast. The
form is **DESTINATION → PARTY → CARGO → the mass meter → send**.

**The launch grammar is its own** — `send_trade_expedition <faction> <band> <party_workers>
<destination_band_id> [food <amount>] [material <material_id> <amount>]... [kit <id>]` — whose tail
is a REPEATED manifest, so the HUD carries a **`send_trade_expedition_requested`** signal of its own
whose payload holds a cargo LIST. That is the `send_denial_raid_requested` precedent: a grammar no
other party verb's payload could express gets its own signal, and `Main.format_send_trade_expedition`
is its own builder for the same reason.

### The two hunting entry points stay in step; a shipment has only ONE site

The standing rule is that the dock's hunt sheet and the herd drawer's expedition branch present ONE
decision surface — and it is about HUNTING PARTIES, both of which compose a raid on a herd. A
shipment's subject is a band drawn from the connections list, and the drawer's expedition branch is
reached by selecting a HERD, so there is no second site for trade to stay in step with.
`DrawerComposeController` is deliberately untouched.

### THE TIE IS THE GATE, AND THE FORM TEACHES IT RATHER THAN ENFORCING IT SILENTLY

The destination picker lists the selected band's own connections
(`HudBandLaborState.connections_for_band`, keyed on the durable `band_id`) and nothing else, because
`ConnectionLedger::get(..).strength > NO_TIE` is what the sim gates the launch on.

- **A PARKED tie (strength 0) is listed, DISABLED, carrying its reason** — never hidden. Zero means
  *"we know such a people exist and have no current dealings"*, which is a different statement from
  never having met them, and the thing the player has to learn is that the TIE is what gates trade.
  Hiding a decayed destination teaches that some bands are simply missing.
- **A band with no ties at all gets the sentence, not a dead button.** The `📦 Trade` button is gated
  on IDLE WORKERS like the other three and never on the ties, so the empty case opens a legible form
  saying how a tie forms — an action no control on this sheet can take.
- **The chosen tie is re-resolved LIVE every render**, the hunt form's rule: a tie decays, so a
  destination that was live when the sheet opened can be parked by the time it is sent.

### THE DESTINATION IS REMEMBERED, NEVER SEEN — the arc's keystone, rendered

A connection can only ever grant `Discovered` (`.claude/rules/core_sim/connections.md`), so
`lastSeen{X,Y,Turn}` is where the subject WAS and nothing on this sheet may render it as a live
position. `_trade_destination_notes` states the sighting and its turn in those words, and the walk
quoted from it wears a **`≈`** and the clause *"if they are still there"*. A remembered band behaves
exactly like a remembered herd, which every player has already been taught by a herd that moved.

**The walk is `SourceForecast.outbound_travel_turns`, this client's ONE definition of travel**, asked
about the remembered tile — not a second `distance ÷ move_rate` free to drift from the one the hunt
readout and the server's own launch feed use. It is omitted entirely when the band publishes no move
rate or either tile is unknown, rather than quoting a fabricated 0.

**A tie carries only ids, so the subject's NAME is resolved client-side** through
`HudBandLaborState.band_label_for_id` — the ONE band-naming join in this client, and the same one the
shipment's `Bound for` row and the parties-strip row use, so a band cannot be called three things on
three surfaces. A subject still in `player_bands` is named exactly as the cycler names it (a roster
POSITION, `HudFormat.band_display_name`); one the roster cannot resolve is named by where it was
(`Band near (44, 9)`). The raw `BandId` is a database key and never reaches a label.

### THE MANIFEST IS ONE ROW PER PILE, AND A MATERIAL ROW SHOWS ITS RATING

The larder is one commodity and so one `Food` row; the materials are one row per BATCH, which is one
pile of one material AT ONE RATING — the shape the sim's own store keeps (`BTreeMap` of
`(material, rating band)`). **A mammoth hide and a hare pelt are both `hide`**, and a row that merged
them would offer a quantity of something the band does not hold. The row's face is
`hide · tough: excellent · supple: poor`, spelled with the Crafting panel's own keys so one pile
reads the same wherever it is quoted.

- **The face ELLIPSES and the tooltip carries the whole rating.** A rating vector is verbose by
  nature and this row will not get wider, so the STRING is shortened —
  `OVERRUN_TRIM_ELLIPSIS`, never `clip_text`, which cut mid-word with no mark
  (`bone · dense: excellent · long: fa`) and read as a broken label rather than a shortened one. The
  hover text repeats the whole face beside what the band still holds, so nothing is unreachable.
  **AN AXIS IS NEVER DROPPED FROM THE DATA to make it fit**: the axes are the whole reason a hide and
  a pelt are different rows, so the underlying label always carries every one of them.
- **The `+` steps a whole unit and CLAMPS TO THE PILE**, so a 0.6 pile is reachable in one press
  rather than being unshippable for want of a fractional control.
- **…and the amount that press leaves is EXACT, so the emitted one is FLOORED, never rounded**
  (`Main.cargo_wire_amount`). The clamp puts the band's precise holding on the row — `137.456789` food
  — and `resolve_shipment` refuses on `held < amount`, so a tenth of a unit of legibility on the
  command line is a refused shipment: *"the band holds 137.46 provisions, not 137.50"*, on the very
  press this sheet teaches. Rounding can also carry the manifest's mass over the cap the meter above
  just said it was under. **Two grids bind, and above ~8 units the coarser one is the WIRE's**: the
  line is text, so the server parses it back through an `f32` before quantising to `Scalar`, and an
  amount floored onto the fixed-point grid alone still reconstructs above the pile about 40% of the
  time. The FEED NOTE keeps one decimal — it is prose for a person, and it is not what the server
  reads. `cargo xtask command-guard` drives a fractional pile end to end and compares what the real
  parser reconstructs against the pile in ticks.
- **The state is remembered per BATCH KEY** (`material id | rating bands`), so a snapshot that moves a
  pile's size cannot silently move the player's choice onto a different pile.
- **The command emits one `material <id> <amount>` line per ROW**, in the order the sheet lists them.
  The parser keeps each line separately, so two piles of hide leave as two lines. **Only the FOOD
  lines are summed**, and only because `food` names one commodity.

### THE MASS METER IS A COURTESY; THE SERVER'S REFUSAL REMAINS THE AUTHORITY

```text
mass = Σ food rows + expeditionTradeMaterialCarryWeight × Σ material row amounts
cap  = party_workers × expeditionTradePerWorkerCarry
```

Both terms are per-cohort echoes of the sim's own config, so a tuning change moves the meter and the
refusal together. **Neither is a literal here** — `expeditionPerWorkerCarry` is the HUNT pack and a
client composing a trade cap out of it would be one config edit from quoting a cap
`send_trade_expedition` refuses.

**The mass expression itself lives in ONE place, `DetailFormat.shipment_mass`**, because the in-flight
`Carrying:` row prices the same pack (`band-readouts.md` → "A trade party states WHO IT IS FOR"). This
sheet's `_trade_manifest_mass` only splits its mixed row list into the two accounts that expression
takes. Two copies of a formula are two answers about one pack, and the row's copy had already drifted:
it divided the cargo's FOOD by the mass cap. The meter exists so the player never meets that refusal; an over-cap
or empty manifest disables the send with its reason, which is the "visible and disabled with its
reason" convention this zone uses everywhere.

**The party stepper's ceiling is the band's IDLE WORKERS and nothing else**, the rule all four launch
verbs follow. What bounds a shipment is the meter, not a head count.

### THE FIFTH BUTTON MADE THE FOOTER A GRID

Four launch buttons fit a 354px column at ~62px each; a fifth takes them to ~48, which `📦 Trade`
does not fit — and the zone `clip_contents`, so what shipped for one render was a button **sliced off
the edge** rather than a narrower row. The footer is a `GridContainer` at
`HudComposeVocab.PARTY_FOOTER_COLUMNS` (3) now, wrapping 3 + 2 — the treatment `build_floor_picker`
already gives its six rungs. The second row costs the footer one row of height, which the parties
LIST above it gives up (it is the `EXPAND_FILL` child).

**Frames:** `trade_footer` (all five buttons, and the frame that proves the glyph DRAWS — a mark
missing from this client's fallback font renders as an invisible gap that no assertion catches),
`trade_picker_empty`, `trade_picker_destination`, `trade_cargo_loaded`, `trade_cargo_over_cap`.


## The work row states its RUNG in two registers, and the ring is declared from the mark

`docs/plan_standing_upkeep.md` §4.9 item 12c. **`SourceForecast.standing_improvement` is the ONE fork**
answering *what is this source standing on*, read off the wire's own `current_rung` (`<branch>:<id>`).
Three readers go through it: the board row's rung MARK, the inspector strip's head-line FACE, and the
ring caret's offer test.

> ### ⛔ THE MARK USED TO REASSEMBLE THE POSITION OUT OF EACH WEB'S PRIVATE FLAGS
>
> `_work_source_rung` forked on `is_field` / `is_cultivated` / `corralled` / a
> `domestication >= HUSBANDRY_PROGRESS_COMPLETE` threshold — the very reassembly
> `SourceForecast.improvement_is_done`'s own ⛔ records the sim having replaced with a published
> position. It was harmless while the mark was the only reader. It stopped being harmless the moment
> the strip stated the same rung's FACE: a mark forking one way and a face forking another is two
> answers to one question, drawn on one row.
>
> **`standing_improvement` is an EXACT match, never at-or-above** — that is what makes it different
> from `improvement_is_done`, which answers `true` for Cultivate on a Field. Neither can stand in for
> the other.

**THE HEAD LINE COSTS +0 AND ASKS RATHER THAN COMPOSES.** `Harvest (28, 16) · ▦ Field 100%` rides the
title `Label` that was already there, so the strip's reservation does not move; the label CLIPS, this
zone's standing rule, and the clause is appended rather than given its own child precisely so it is
the first thing to go. `DetailFormat.standing_rung_face` routes through **`rung_row_value`**, the same
fork the tile card's rung row goes through — so the hazard mark, `slipping`/`drifting`, `Lapsed`,
`Held`, `Reverting` and the floored percent all arrive already decided, and the two surfaces cannot
word one rung differently. It takes **no `declared_rung` and no `build_crew`**: both are countdown
terms and a STANDING rung returns on `rung_row_value`'s first branch, so accepting them would
advertise a dependency the face does not have.

**A METER THE WIRE DOES NOT STATE READS FULL, NOT ZERO.** `improvement_progress` answers `0.0` for an
unstated key — indistinguishable from a meter eroded to nothing — and a built corral states no meter
at all, which is why the tile card's own built-corral row passes `CORRAL_PROGRESS_COMPLETE` by hand.
The `> BUILD_METER_UNSTARTED` test is what separates the two; it was inherited from the retired
`SourceForecast.rung_needs_repair`, and the distinction outlived that test.

### `extend_pen` is declared from the standing-rung mark, and it opens a price

**Reported from play**: extending a pen was a button on the TILE card producing a **build queue
entry** — the one queue entry in the game declared from somewhere other than the work tab.

> **THE MECHANICAL REASON IT ENDED UP THERE:** `RungLadder.has_track` is FALSE when nothing sits above
> the standing rung, and `animal:pen` is the top of the animal branch — so a corralled herd's row
> renders **no `⌃` in the ready slot at all**.
>
> > **That same falsehood used to be reachable with the mark still drawn, and the press did NOTHING.**
> > `_open_rung_track` answered it with a bare `return` on an enabled `Button` carrying
> > `MOUSE_FILTER_STOP`, so the click was consumed and did not even fall through to opening the
> > inspector — reported from play on a completed Field. The offer test is the track test now
> > (`labor-ui.md` → "THE OFFER TEST AND THE TRACK TEST ASK ONE QUESTION"), which makes that branch
> > unreachable, and it `push_warning`s rather than returning in silence: a state that can only arrive
> > as a bug should say so where a harness or a dev session sees it. Extending a pen is precisely what you do *after* the
> ladder is finished. `selection-card.md` blamed it on being *"a one-click standing action, not a
> compose flow"*, which was true and was the second reason.

**IT RIDES THE MARK SLOT, NOT THE READY SLOT.** That slot is already a four-way — `⌃▦` offers a rung,
`▦45%` reports one climbing, `⚠▦` reports one stuck, and a fourth reports one lapsed — and a fifth
meaning on the one control for *what could this be* would collapse a distinction the whole feature
draws. The mark sits on **the thing the job acts on**: a ring extends the pen the mark denotes. **A
row already carries two marks and that is the precedent** — a mid-Sow patch reads `🌾` *what it is*
beside `▦ 28%` *what is being built on it*.

**IT OPENS A PRICE, NOT A BARE COMMIT, and item 12 is what made that necessary.** A ring draws
`animal:pen`'s own hurdle pile since §2.7, so a one-click button stated a cost nowhere. `_open_ring_card`
opens the same shape the track's `⌃` opens — what it eats to raise, what it costs to hold, and where
it will stall — through **`RungLadder.ring_row` + `build_ring_card`, which reuse the track's own
`_build_price_asides` / `_hold_price_asides` / `_build_row`**. That is what keeps the caret meaning ONE
thing on every mark that wears it.

> **A RING IS NOT A TRACK ROW, and giving it one would be a lie about the ladder.** The track is one
> POSITION on a branch; a ring is a **repeatable increment with no position**. It is its own small
> card. Its price is `animal:pen`'s own rung cost — **not** the herd's `pen_extend_cost`, which the sim
> stamps only once a ring is accruing and which is therefore the in-flight meter's denominator.

> #### THE CARD QUOTES THE PILE, THE WORK AND THE STANDING BILL — off `corralBuildMaterialCost`
>
> On screen: **`Extend the pen` / `Another ring` · `75 work` / `+ 6 hurdles to raise it` / `then 1
> work · 0.05 hurdles a turn to hold`**, and the WARN stall aside beneath the pile where the shelf
> cannot cover it.
>
> ⛔ **THE PILE LINE IS THE ONE FACT THE CARD'S OWN ARGUMENT NAMES, AND IT SHIPPED WITHOUT IT.** The
> dead claim, kept so its absence is not re-derived a fourth time: *"KNOWN GAP — the card quotes the
> work and the standing bill, and no pile. `ring_row` builds its build-price asides from
> `SourceForecast.build_material_cost`, and that field prices exactly ONE rung: the one DIRECTLY ABOVE
> where the source stands … so `_build_price_asides` returns `[]` on its first line and the card draws
> neither the hurdle pile a ring eats nor the WARN stall aside."* Every clause of that was true;
> **the gap was publication, not model** — `systems::labor::head_ring_leg` was charging the pile all
> along.
>
> **THE FIELD THAT CLOSED IT IS `HerdTelemetryState.corralBuildMaterialCost`** — the whole `animal:pen`
> build pile, unscaled, published at **every** position (on a pastoral herd it equals
> `buildMaterialCost` by construction; on a corralled one it is the only reading of the pile there is),
> pinned by `core_sim/tests/rung_material_quote.rs`. The client half is
> `native/src/dict/subsistence.rs` → `corral_build_material_cost`,
> `SourceForecast.corral_build_material_cost`, and `ring_row` composing `ROW_BUILD_ASIDES_KEY` from it.
>
> ⛔ **`build_material_cost` IS STILL THE WRONG FIELD HERE AND MUST NOT BE PUT BACK.** `AnimalPen` is
> the top of its branch, `above()` is `None`, and `core_sim` publishes an empty pile there
> deliberately — *"Empty at the top of the branch, which is the honest reading rather than a repeat of
> the pen's own"*. The two agree on a pastoral herd, which is exactly what makes them look
> interchangeable; it is their DISAGREEMENT on a corralled one that this card needs.
>
> **THE CARD HAD NO CLAIM ON IT AT ALL, which is how the gap shipped.** The caret's assertion proves
> the mark is pressable and stops one press short. `_assert_ring_card_prices_the_ring` presses it — the
> real control, so the meta, the mouse filter and the handler are in the path — and claims liveness (a
> pressable row named `Another ring`) before the figures: the row's face is `75 work`, the hold aside
> states both currencies, and the pile aside states its **good and amount** against the fixture's own
> numbers, in the quiet ink with no stall warning on a shelf that covers it. That last pair was written
> deliberately partial while the field was missing — asserting the absence would have cemented the gap,
> asserting the presence would have failed on shipped behaviour — and the asides were printed to the
> log so the day the field landed the run said what changed. **Frame:** `band_panel_ring_price`.
>
> ⛔ **THE FIXTURE STILL ERASES `build_material_cost` ON THE PENNED HERD**, and stamps
> `corral_build_material_cost` beside the erasure. `buildMaterialCost` genuinely IS empty on a
> corralled herd, so dropping the erase would model a snapshot the sim never sends — and would let a
> card that regressed to the above-selector pass.

**A RING IN FLIGHT WEARS NO CARET**, which is what stops a second being declared over the first
(`Herd::pen_extending` is the sim's gate and needs no client twin). The gate is
`SourceForecast.pen_ring_is_in_flight` — **the NUMERATOR, not the fraction**: `begin_pen_extension`
sets the flag and `accrue_pen_extension` stamps the cost, so a ring declared this turn has both fields
at zero, and a fraction test would render it as stalled at `0%`. The **percentage** lives on the build
queue row, which is the surface that dates and withdraws the ring.

**THE MARK'S MOUSE FILTER FORKS WITH ITS SHAPE.** A read-only mark takes `PASS` — the only value that
both shows a `Label`'s tooltip and lets the row's click through to the inspector — and the ring mark
takes `STOP`, or the press that opens the card would also open the inspector under it. The ready slot
has forked exactly this way since it gained a pressable face.

> #### ⛔ AND THE HOVERABILITY GUARD SILENTLY STOPPED COVERING THAT ROW
>
> `_assert_rung_labels_are_hoverable` walked `Label`s (`_collect_rung_labels`), so the penned herd's
> mark dropped out of it the moment it became a `Button`: **4 marks became 3 and nothing failed.**
> It walks `_collect_meta_controls` on the shared meta now — which is what the meta was for — counts
> the pressable one separately, and FAILS when a board carrying a penned herd draws none.

### The kit pair is its own SECTION (it was a THIRD PICKER, and that is what made it free)

Item 12c's second half — the take crew's kit beside the SITE's upkeep kit.

⛔ **IT IS DRAWN ALWAYS NOW, on any row that HAS kits** (§4.9 item 12d, second pass). The retired
shape, quoted because the argument is exactly the kind that gets written back: *"It is a picker opened
from the strip's `Kits` link, exactly as `Change policy` and `Priority` are, and the exclusivity is
what pays for it: `_work_picker_open` holds ONE value, the builder's `if/elif` chain draws at most one
expansion, and `_work_inspector_height`'s matching chain reserves the MAX rather than the sum."* Every
figure in the table below is still the control's real height; what expired is that the pair had to be
free. The card competes for no zone height, so all three are terms in a sum and the pair pays for
itself — with the HINT line the exclusivity had cost it.

| picker | costs |
|---|---|
| `WORK_INSPECTOR_POLICY_PICKER_HEIGHT` (floor) | 32 |
| one kit control line | 22 (`WORK_COMPACT_PICKER_LINE_HEIGHT`) |
| `WORK_INSPECTOR_PRIORITY_PICKER_HEIGHT` | 52 (32 + a hint line) |

⛔ **`WORK_INSPECTOR_KITS_PICKER_HEIGHT` (`44`, `2 × WORK_COMPACT_PICKER_LINE_HEIGHT`) AND
`WORK_INSPECTOR_KIT_LINES` (`2.0`) ARE RETIRED, and the flat two is what shipped the defect.** The
retired claim read *"At 44 the kit pair is shorter than the priority picker — which mattered while the
ceiling was a MAX and matters no longer … `WORK_INSPECTOR_KITS_SECTION_HEIGHT` wraps this 44 in a
header and a hint, for 91."* The arithmetic was right and the SHAPE was wrong: the section has two
shapes, and a constant that folds both rows into one number cannot be asked how tall the one-row shape
is. The section is `27 + 22 = 49` at its floor and `+ 42` where the site owes upkeep, and **374 is
unchanged** — `27 + 44 + 20` and `27 + 22 + 22 + 20` are the same 91, so the ceiling did not move.

#### ⛔ THE UPKEEP ROW DREW ON WILD SOURCES, ON BOTH WEBS — reported in play

A **wild** source has no standing rung and therefore **nothing to keep**; upkeep is a property of a
rung (a pen has hurdles to mend, a Tended Patch and a Field have their own bills, a wild stand and a
wild herd have no improvement at all). The card drew `Upkeep [Hurdling kit ▾]` / `Upkeep [Tillage kit
▾]` on them anyway, silently defaulted to a kit, and stood a hint line under it saying *"\"No kit\" is
a real choice — the site worked bare-handed."* — an answer to a question that was itself the defect.
**Both webs behaved identically**; the plant half was not a separate bug.

**The gate is the SITE's published bill and not a re-derived rung test.**
`RungLadder.upkeep_price_terms(source, prefix)` composes the STAMPED pair — `upkeepDemand` (work) and
`upkeepMaterialDemand` (goods) — into terms, and **`[]` is the wild answer**. That list is the ONE
producer: `BandPanelController._work_inspector_has_upkeep` reads its emptiness as the gate and the
card renders its terms as the bill, so a picker cannot draw beside a bill that says nothing. Reaching
for `is_field` / `corralled` / a `domestication` threshold would be a second authority over a question
`SourceForecast` already answers — and would get the mid-climb case WRONG, a source raising its FIRST
rung standing on `wild` while already being billed.

**Either currency alone is enough.** A rung may owe work, goods or both, and a keeping kit speeds the
work half even where the material half is zero — so the gate is a disjunction. `upkeepMaterialDemand`
is empty on every shipped rung but `animal:pen`, which is why reading only the work account would have
been right today and wrong on the next rung that eats a good.

**The line states the TERMS and not the rung word** (`WORK_INSPECT_KITS_UPKEEP_FORMAT`, *"Kept at %s a
turn."*): the head line already names the rung through `DetailFormat.standing_rung_face`
(`Hunt Aurochs · 🐄 Corralled 100%`), and one rung worded twice on one card is how two surfaces come to
disagree about one source. On a wild source the head line states no rung at all, which is the same
verdict read through the other producer — the harness asserts both together.

**The take row (`Hunters` / `Herders` / `Harvesters`) is unconditional**, and
`_work_inspector_has_kits` is now a test on the TAKE job alone. It used to be a conjunction over both
jobs, which additionally meant a roster with no keeping tool suppressed the take picker as well.

#### ⛔ AND NEITHER PICKER MAY RENDER WITHOUT A SELECTION — a blank face is a DEAD CONTROL

Reported in play on a PENDING harvest crew: the `Harvesters` picker showed nothing, and picking from
it changed nothing either. `HudWidgets.build_option_picker` takes the lit INDEX and the FACE as two
separate arguments, so an unresolved kit id produces `select(NO_ENTRY_SELECTED)` over a face
`KitRoster.display_name_for_id` answers `""` for — a perfectly findable, perfectly populated,
perfectly useless control that photographs as an ordinary card.

- **The TAKE picker's fallback is the JOB's default, not a field off the source.** The full autopsy —
  the pending overlay's dropped `kit_id`, the plant web's absent `default_kit_id`, and why the hunt
  web only looked immune — is in `labor-ui.md` → "THE KIT RIDES EVERY CREW EDIT".
- **The UPKEEP picker falls through to `KitRoster.keeping_kit_for`, and the state it answers for is
  REACHABLE.** `resolve_upkeep_kits` walks the BANDS' LABOR ROWS, so a source no band works yet is
  absent from that map and publishes `""` — while the BILL this row is gated on comes off the
  source's own RUNG and is there regardless. A brand-new PENDING assignment on a kept source
  therefore drew the Upkeep row blank. **That fall-through is not a missing-field guard**: it is the
  client's own copy of the derivation the sim applies the moment the assignment lands, and it is
  already what a NAMED row's `(default)` mark is measured against. With nothing stated the derivation
  IS both the selection and the default, which is exactly what an UNNAMED row means — so the ⛔ above
  about the mark coming off `upkeep_kit_named` still holds and no second derivation was introduced.
- **Every kit assertion in `band_panel_preview` asked whether a picker EXISTED and what its ROSTER
  held; none asked what it was SHOWING**, which is why a dead control passed every claim the harness
  had. `_assert_kit_pickers_state_a_selection` is the one that was missing — entries, a lit index and
  a non-empty face, on every state that draws a picker.

> #### ⛔ IT WAS A PERMANENT BLOCK FIRST, AND THAT COST 50px UNCONDITIONALLY
>
> On top of whichever picker was open, on every strip, open or not. Measured on the wide dock with a
> row selected: **442 into a 396px box, over by 46**, plus 5–55px over at every narrow-shell viewport
> the `MAX_WIDE_HEIGHT_FRACTION` clamp binds at.
>
> **The board could not absorb it.** `_work_board_capacity` floors at `maxi(1, …)` and a row is
> `WORK_ROW_TWO_LINE_HEIGHT` = 44px; the strip grew 50 and the zone grew 46, so the board gave back
> 4px of `int()` truncation and **zero rows**. The strip's own `reserved ≥ drawn` passed throughout —
> it was never an under-reserve, it was the zone having nowhere to put an honest reservation.
>
> **The dead reasoning, quoted rather than deleted**: *"THE PAIR IS ONE FLEX-WRAP ROW, NOT TWO
> HAND-PLACED ONES, and that INVERTS the usual trap. A wrapped line normally costs back the row it
> saved invisibly; here the wrap is the INTENDED behaviour, so the height to reserve is the STACKED
> one and the single line is the saving."* Every measurement in it is correct, and the conclusion it
> supports is what killed the shape: **the pair never rode one line on any dock the game ships** (472px
> needed against a widest shipped work zone of 382), so the "saving" was hypothetical while the 50px
> was charged to every strip.
>
> **A picker body has the strip's whole width**, so it stacks unconditionally — which removed the wrap
> predicate, its `one_line` argument, the pickers' declared 168px column and the drift surface between
> a reservation that computed the wrap and a container that performed it.

**TWO PLAN FIGURES WERE WRONG, both re-measured.** A picker in this zone is **22px**, not the plan's
`32 + 6`: that is the COMPOSE SHEET's control, and the block gap is charged once per BLOCK rather than
per line (`WORK_COMPACT_PICKER_LINE_HEIGHT`, the one measured figure the queue settings strip and the
inspector pair both name). And the pair never rides one line, above.

**NO HINT LINE, and the arithmetic is why.** The priority picker's 52 is 32 + a 20px hint; two kit
rows plus a hint is 64 — 12 over the current max, which busts the wide shell by 8. What a hint would
have said (`none` is a real choice, not an empty one — it is how a site is worked bare-handed to
conserve the tool) is in BOTH pickers' tooltips instead, since `none` means the same on either kit.

> #### ⛔ AND AN OPEN PICKER OVERFLOWS THE WIDE DOCK TODAY — ALL THREE OF THEM
>
> Measured with a row selected, wide shell, one board column: the zone reads **396 of 396 with the
> expansion CLOSED**, so it has **4px of spare** and no expansion fits in it.
>
> | open picker | zone needs | over by |
> |---|---|---|
> | kits (44) | 436 | **40** |
> | priority (52) | 444 | **48** |
>
> **This is not the kit pair's defect and predates it**: the priority picker has been overrunning the
> same budget since §4.9 item 9b, and the kit pair is the SHORTEST of the three. No frame had ever
> rendered it, for the reason this file has now recorded three times — **every picker-open frame is a
> TALL dock and every wide-dock frame has the expansion closed**, two disjoint families with the
> defect living in the gap. `WORK_INSPECTOR_CEILING_HEIGHT`'s own note already said the ceiling is
> *"stated because it is UNMEASURED rather than because it is reserved"* and that *"if one is ever
> observed, this is the figure both of that constant's levers move by."* One has now been observed, at
> a single open picker rather than at the full combination.
>
> `band_panel_work_kits_picker` is the frame — its second picker row is visibly clipped by the zone's
> bottom edge — and it REPORTS its extent rather than asserting it (`band_panel_vitals_worst_case`'s
> rule: a red line here asks for a decision, not a failing run).
>
> **The vertical dock is unaffected at every viewport measured** (1080 / 900 / 768 / 720): its zone box
> is the window height less chrome — 939 / 759 / 627 / 579 — against a strip that reserves 128 with the
> pair open.
