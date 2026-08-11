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
| `ui/hud/BandPanelController.gd` | `RefCounted` controller (HUD decomposition Phase 2d, `docs/plan_hud_decomposition.md`) owning the **BAND/CITY PANEL's whole render path** — the last big mass to leave `Hud.gd`. It holds the panel HANDLE (`_panel`), the three public **zone builders** `build_band_zone` / `build_work_zone` / `build_parties_zone` and everything under them (the band zone's vitals/PEOPLE/food-outlook/WORKFORCE + role cards; the work zone's paged board, filter chips, pager, inspector strip and source models; the parties zone's rows, inspector strip, footer and the mission compose sheet), the panel's **cycler + snapshot refresh** (`render_band` / `refresh_snapshot` / `rerender` / `cycle_band` / `focus_band` / `select_expedition` / `focus_labor_source` / `confirm_recall_expedition` / `_push_zone_badges`), and the **zone state that survives a snapshot** — `_work_filter` / `_work_sort` / `_work_page` / `_work_open_key` / `_work_policy_open` / `_work_zone_host` / `_work_zone_band` / `_band_zone_tier` / `_party_open_key` / `_party_compose_open` / `_party_compose_mission` / `_send_expedition_count` / `_send_hunt_policy` — ~1,580 lines, 72 moved functions. **`_band_zone_tier` is why the band and work halves are ONE controller**: it is a bare `int` written by `build_band_zone` and read by `_on_zones_resized`, so splitting them would have straddled it. Hud holds it as `_bandpanel`, constructed in `_ready` after `_disclosures` (the vitals row wires its carets through it). **THE PANEL HANDLE IS PRIVATE** — the two non-moving `HudLayer` readers (`_refresh_disclosure_hosts`, `_render_occupant_drawer`) only ever asked "is a panel injected?", so they ask **`has_panel()`** instead of holding the node. **The injection surface is TWO Callables** (it was nine, then six; the three detail-line ones went with `BandDetailLines`, and the four send-expedition/quarry targeting ones went with `TargetingController`), each retained on HudLayer by the "an injection you still have to hold is relocated, not eliminated" test: `_emit_assign_labor` (owns the `assign_labor_requested` emit + optimistic pending write, so `assign_labor` stays INDIRECT) · `_herd_label_for_id`. Each is reached through a **typed adapter**. The parties zone's send-expedition + quarry verbs (`begin_send_expedition` / `begin_pick_quarry` / `cancel_pick_quarry` / `is_expedition_quarry`) are a typed **`TargetingController`** collaborator now, not four Callables. `_is_player_unit` is a trivial private COPY (the `SelectionCardController` precedent). Collaborators: the SAME `_band_labor` / `_compose` model instances BY REFERENCE, `_selectioncard` (roster lookup + map pinning, for the cycler / labor-source / party jump routing, **plus `selected_terrain_label()`** — the one selection read the vitals rows need), `_disclosures` for `wire_label` ONLY, **`_banddetail` (a typed `BandDetailLines` ref — the vitals label and the parties inspector strip render through it; the three `*_fn` members `_unit_summary_lines_fn` / `_expedition_summary_lines_fn` / `_expedition_row_tooltip_fn` and their adapter wrappers are DELETED, the tooltip being a static `DetailFormat.expedition_row_tooltip` call now)**, and the HUD CanvasLayer as the **host** it `add_child`s its `ConfirmationDialog` into (a `RefCounted` cannot parent — the `TurnOrbController` pattern). **It emits SIX signals, all RELAYED by HudLayer** (the controller never emits a HudLayer signal): `cancel_order_requested` · `send_hunt_expedition_requested` · `recall_expedition_requested` · **`split_band_requested`** · `alert_focus_requested` · `roster_occupant_selected`. **`set_band_city_panel` / `cycle_panel_band` / `focus_panel_band` MUST stay callable on the HUD node** — `Main._wire_band_city_panel` probes all three with `has_method` and binds the latter two to `BandCityPanel`'s `cycle_requested` / `subject_activated`, and a failed probe fails SILENTLY — so HudLayer keeps them as thin delegators. **`_build_allocation_panel` does NOT live on this controller**: it writes the drawer's `%AllocationPanel` node, so it stays with the drawer render dispatch (it moved to `SubjectDrawerController` with that dispatch in Phase 2c-3, still a thin function stacking this controller's three public zone builders; its two siblings on that host, `_build_band_move_actions` / `_build_expedition_panel`, are branches of `_render_occupant_drawer` and travelled with it for the same reason). Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`SelectionCardController`/`DrawerComposeController` convention. Behaviour identical to the old inlined band-panel code |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail (the rail runs along the dock's PLENTIFUL axis — stacked on L/R, one line with the restore toggle right-justified on T/B — and `COLLAPSED_SIZE` is a FLOOR on the strip it reserves, not an answer; see "The collapsed rail runs along the dock's plentiful axis"). Header (stage glyph/name/label + the band's hex coordinates + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse) plus an **ACTION REGISTRY** — a registration seam (`register_action` / `action_invoked`) holding every verb the panel offers, the `⚒` included, rendered on its own BAR row under the header on a vertical dock, on the SUBJECT ROW itself on a horizontal one and on the COLLAPSED RAIL in either, taking zero height wherever it is not the live mount; see "The action registry is ONE list with THREE mount points" — body hosts **AN ORDERED LIST OF NAMED ZONES AT A FIXED CROSS-AXIS SIZE**, declared by the SUBJECT via **`set_zone_layout(specs)`** and filled by **`set_zones(contents)`** (keys `&"band"`/`&"work"`/`&"knowledge"`/`&"parties"`; the panel OWNS and frees them, and frees a content handed in for a zone the layout does not declare). A band declares three, the faction page four — see "THE BODY IS AN ORDERED LIST OF ZONES". Two shells, chosen by the panel's own **WIDTH** (`wide_shell_min_width()` — never a dock-edge test, so a resizable dock needs no special case). **That threshold is DERIVED FROM THE LIVE ZONE LIST, never hand-picked and never a fixed set of terms**: it sums each declared zone's flank (an EXPANDING zone contributing `ZONE_WORK_MIN_WIDTH`, the one readable board column the test exists to protect) plus **one `RAIL_SEPARATOR_SPAN` per GAP** plus `PANEL_CHROME_H` — so a band's three come to 380 + 380 + 354 + 2×25 + 26 = **1190** and the faction page's four to 380 + 380 + 354 + 354 + 3×25 + 26 = **1569**. **It is therefore PER-SUBJECT**: on a window between the two the faction page correctly tabs while a band's page stays abreast, which is also why `set_zone_layout` is called BEFORE the zone contents are built. `ZONE_WORK_MIN_WIDTH` (380) MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` — one readable board column — exactly as `ZONE_WORK_MAX_WIDTH` (1520) mirrors `WORK_COLUMN_MIN_WIDTH × WORK_MAX_COLUMNS`; the two are a PAIR with Hud's column consts and move with them. The chrome term is load-bearing because the threshold is tested against the panel's OUTER `_panel_extent().x` while the zones live in `_interior_size()`. It shipped hand-picked at **900**, which broke the whole 900–1055 band (the derived threshold was 1056 then, before the flanks widened): the work zone came out 224px, Hud clamped to one column, its labels clipped — and the NARROW shell would have given the board the full 874px, so flipping wide early made it ~4× narrower, degrading the thing the wide shell exists to improve. `PANEL_CHROME_H` is a `const`; `_wide_separator_span()` and `_fixed_zone_span()` are FUNCTIONS over `_zone_layout`, shared by `wide_shell_min_width()`, `_card_width()`, `_affordable_work_columns()` and `zone_size()` so none of them can disagree about how much width the chrome eats. (`WIDE_SEPARATOR_SPAN`, the `const` that hard-wired TWO gaps, is deleted — it was the one term a fourth column could not have been added around.) **wide** (in practice T/B) = every declared zone side by side, the flanks fixed at `ZONE_BAND_WIDTH` (380) / `ZONE_PARTY_WIDTH` (`PANEL_WIDTH − PANEL_CHROME_H` = 354 — see "The wide shell's flanks are never narrower than the narrow shell's zone") / `ZONE_KNOWLEDGE_WIDTH` (the same 354, taking the same floor for the same rule), work EXPAND_FILL, `LINE_SOFT` hairlines in every gap, no tab bar; **narrow** (in practice L/R) = the subject's own tab bar under the header + exactly one zone beneath it (active tab = SIGNAL ink + a 2px SIGNAL underline, badges via `set_tab_badge(zone, text, hot)`, selection persisted as `CONFIG_KEY_TAB`). **The cross-axis size is FIXED** — `PANEL_WIDTH` 380 (L/R) / `_horizontal_panel_height()` = the body budget (`PANEL_HEIGHT_WIDE` 360 at one band column, `PANEL_HEIGHT_WIDE_TWO_COLUMN` 335 at two) **plus the active shell's own chrome** (`_shell_chrome_height()`: 0 wide, the tab bar narrow), clamped to `MAX_WIDE_HEIGHT_FRACTION` of the window (T/B) — see "The strip's height is 360 at ONE band column and 335 at two" — so `current_reservation_size()` changes ONLY on dock/collapse/hide/viewport-resize and a content edit can no longer re-emit `reservation_changed` → `MapView.set_reserved_inset` → cache invalidation (the map flicker on every `+` press). **TWO sanctioned `ScrollContainer`s exist in the panel — the PARTIES list and the BAND zone** — and the harness asserts both halves for each: that it exists, and that no OTHER zone has grown one (`_assert_scroll_only_where_sanctioned`, a table of `(node name, owning zone)` pairs, so a scroll under the wrong zone still fails). Everything else is no-scroll by design; the work zone pages itself against **`work_zone_size()`** — a named reader of the KEYED **`zone_size(zone)`**, which is one answer with one parameter rather than a named accessor per zone that a fourth zone would have to add a fifth of — the zone's interior after chrome — e.g. 354×1107 in a 380 L dock, 789×300 in a 1920 bottom dock with the chrome rail sharing that row — and re-pages on the **`zones_resized`** signal). **Zone hosts are plain `Control`s, not containers**, so an over-wide zone content cannot push the card past its fixed cross-axis size; `clip_contents` keeps overflow inside its own zone. Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`, which since issue #377 fans a HORIZONTAL dock's reservation to the map at 0 (the card floats over live map) and a TOP dock's to the HUD at 0 as well (its readouts belong beside the card, not below the strip). On a **BOTTOM** dock the strip also carries **a trailing CHROME RAIL** the HUD parks its stacked bottom-bar chrome into (`rail_slot_host` / `set_rail_width`, issue #324) — a SIBLING of the card, not a cell of its row, and bottom-only since #377 (a top dock never displaces `BottomBar`, so its chrome stays home). See "Band/City dockable panel". See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/hud/BandComposeFloat.gd` | **The parties compose sheet, floated off the panel when its zone cannot hold it** — see "A COMPOSE SHEET THE ZONE CANNOT HOLD LEAVES THE ZONE" for the trigger. An **`AutoSizingPanel`**, not `PanelCard` + `DockScrollFit`: this card is measured against the VIEWPORT rather than against a dock's remaining height, which is the free-floating half of that pair (`panel-framework.md`). Both axes are fitted explicitly, because the node is a plain `Control` and no child minimum ever reaches it. **It is the card and NOTHING more — there is deliberately no full-screen catcher.** `ComposeSheet`, the herd drawer's floating sheet, is a catcher with a card inside it so a click anywhere outside dismisses; that is exactly wrong here, because the DOCK's sheet stays open through a map pick — the targeting banner and the herd glow ride on the sheet still being open while the player clicks a herd — and a catcher would eat that click. `PanelRoot`'s autopsy applies in reverse: a `STOP` control the pointer finds makes the Viewport mark the press handled before `MapView._unhandled_input` sees it, so every pixel this node claims is a pixel of dead map, and it claims only its own rect (`band_panel_preview._assert_float_leaves_the_map_clickable` drives that through `Viewport.push_input`, never off a `mouse_filter` value). **It never overlaps the card it came from, structurally rather than by a clamp**: `_room()` is the viewport inside `VIEWPORT_MARGIN` cut back to the MAP-FACING side of the panel card (`MAP_FACING_SIDE`, the opposite of the docked edge) with `ANCHOR_GAP` of clearance, and the width fit, the height fit and the placement all read that ONE rect — a card too tall for it scrolls, it does not creep back across the seam. **`target_width` is the ZONE width plus this card's own chrome**, never the zone width itself: `AutoSizingPanel`'s width is the OUTER one, and a sheet handed the zone width minus a border, two content margins and a scroll gutter re-wraps, which would falsify the very measurement that floated it. `mount` applies that width BEFORE the frame `refit` waits, or the height fit reads the previous width's wrapping and leaves the card ~100px taller than its content (measured). Its ONE `ScrollContainer` is not a breach of the panel's no-scroll rule — that rule is about content whose height feeds back into a FIXED reservation, and this ceiling is real viewport room — and it stays DISABLED unless `fit_to_content` finds the content taller than the room. It draws in `BandCityPanel.panel_card_stylebox()`, the panel's own, so it reads as the panel's surface rather than a second kind of card |
| `ui/hud/FactionRollup.gd` | **All-`static`, stateless** builder of the FACTION PAGE's FOUR zones (issue #450) — the all-band rollup the cycler pins first. `build_band_zone` (the summed PEOPLE bar + the band page's own vitals rows — Food / Kit / Morale / Growth; a fifth, Trade, went with arc #527's retired account), `build_work_zone` (the whole workforce as one bar and the per-band roster), **`build_knowledge_zone`** (SETTLING, the craft tracks, DISCOVERIES — the fourth column the panel's ordered-list body exists to hold, with a `full` HEIGHT TIER that drops the last of the three in a height-capped horizontal dock) and `build_parties_zone` (every party and the band it left, its NAME jumping to that band — see "THE PARTIES ROW NAMES THE HOME BAND" for why `_summary_row` binds a separate `jump_owner`), plus the `_stat_row` leaf they are built from. Its two new inputs are threaded in as PARAMETERS like every other: the player faction's sedentarization entry and its discovered-site array, read off `FactionReadouts` (`faction_sedentarization` / `faction_discovered_sites`), which is where the PLAYER-FACTION FILTER over those two per-faction wire arrays already lives — a second walk looking for `PLAYER_FACTION_ID` is a second chance to disagree about whose faction is being reported. **It is a shared LAYER rather than a controller because the page is a READOUT** — no steppers, no compose sheet, no open row, nothing that survives a snapshot — so it has no per-cluster state to own, which is the whole of what makes a controller one (`hud-modules.md`). The one thing it needs is threaded in as a PARAMETER: the `HudBandLaborState` instance, plus the faction's `{track: progress}` row and the caller's `herd_label_for_id` Callable (the treatment `HudFormat.panel_expedition_summary` already takes — a stateless layer must not reach for the roster/selection/herd-list state that resolver reads). **IT RE-DERIVES NOTHING**: every total is a SUM over answers the per-band surfaces already give (`DetailFormat.band_net_food` / `band_provisions`, `HudBandLaborState.effective_idle` / `effective_worker_map` / `effective_role_workers` / `band_party_workers`, `FactionReadouts.faction_tracks`), so a band's own page and this one cannot disagree about a number — a rollup with its own food ledger would be a second source of truth for the identity `larder_delta == income − consumption − pen_feed − raid_forfeit` the food arc keeps closed. Dependency direction: it reads `HudWidgets` / `HudFormat` / `DetailFormat` / `SourceForecast` / `HudStyle` / the vocab leaves and `FactionReadouts`' track table, and none of them may read it back |
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
    which is where the `age_children` narrowing bug lived; a duplicate carries the decoder's own types.
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
- **Zone `band` — vitals · PEOPLE · food outlook · WORKFORCE + role cards** (`BandPanelController.build_band_zone`).
  The Food/Morale/Growth/Kit rows are the disclosures — and their breakdowns open in a
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
  not width**, which is the horizontal dock exactly, and that buys the merge treatment: the hay
  stock rides the Food line as `· 128.4 hay` instead of vanishing, because a hay larder has no other
  surface to be legible on. See `band-readouts.md` for the clause and the width it was measured
  against. **A `Trade` row was the one row this tier DROPPED**, on the reasoning that its rate still
  read on the WORK zone header — the whole reason a drop was affordable for it and for nothing else;
  arc #527 retired that account, the row and the header total together, so the tier drops nothing
  today. The `Population … Workers … (Idle …)` LINE is
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
  **WORKFORCE**'s saturated one (`HEALTHY` / `SIGNAL` / `VOICE_INK` / `WARN` / `VOICE_PIGMENT` /
  `INK_FAINT`): two bars,
  same shape, different question — *who they are* vs *what they do* — and they must not read as the
  same chart twice.
  **THE WORKFORCE SEGMENTS PARTITION `working_age`, WHICH IS WHY THE BENCH HAS ONE.** Forage · Hunt ·
  Roles · Parties · **Bench** · Idle, and the head states `n idle of m` off the same
  `HudBandLaborState.effective_idle` — which nets the crafting bench's crew out, a worker at the bench
  being assigned labor (`crafting-panel.md` → "The stepper's ceiling"). Without a segment of its own
  that crew would leave idle and appear nowhere, so the bar would quietly stop adding up to the head
  beside it. `FactionRollup._build_workforce_block` carries the identical segment for the identical
  reason — the two bars are one chart at two scales, and a faction total missing a segment the band
  bar has is the same hole one level up. Scout + Warrior are **CARDS** now (bordered, name · the `−/+` stepper and its
  `assign_labor` emit · **the kit picker and its gear line** · the role's description LAST), not rows
  in a list — the fix for a standing role being indistinguishable from a worked source. See "The role
  cards carry the band's OTHER two kits" below for the picker half and for why the prose trails.
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
  full productivity** · a `⋯` `MenuButton`) · filter CHIPS · the board · pager · inspector strip.
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
  state stages ONE band with a hay larder *and* a pen feed bill, productivity below full, a fertility
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
  **`band_panel_people_map_path`** (the SAME block reached the OTHER way — by clicking the band ON THE
  MAP, through the real `MapView._rebuild_unit_markers` → `refresh_selection_payload` →
  `show_unit_selection` → `BandPanelController.render_band`. `band_panel_people` drives the SNAPSHOT path,
  which re-resolves the brackets from the raw `populations` floats and therefore SELF-HEALS a
  truncating marker copy — so it structurally could not catch the `int()`-narrowed age brackets. This
  state ASSERTS the three PEOPLE brackets sum to the band's own `size`, and was verified to FAIL —
  `sum to 29 but the band holds 30 (raw [9.0, 16.0, 4.0])` — with the narrowing put back. **It also
  carries the Minimal TOE's Kit claims** (`_assert_map_path_states_kit`): the PAYLOAD holds all six kit
  keys — named from `DetailFormat`'s and `SourceForecast`'s OWN constants, since the structural copy
  leaves no key list on MapView to borrow and borrowing one would assert that the copy copies what the
  copy copies — the payload is the WHOLE cohort, spears arrives un-narrowed, and the `Kit` row RENDERS — the payload
  claim being where the leak is, and a marker carrying six keys nothing draws being no fix either. Its
  band comes from **`_kit_band_fixture`, a SEPARATE fixture, and that separation is itself a finding**:
  the `Kit` row costs 26px, the band zone already reads 299 of its 300px box in a height-capped T/B
  dock, so putting the six on the shared `_band_fixture` overflows `Zone_band` by exactly 25px in **13
  states**. Every live band states its kit, so that overflow is real and the kitless fixture was hiding
  it; which SHORT-tier row yields is a design decision and is reported rather than guessed at. The
  needle carries the VALUE (`Spears 74`) and is composed from the fixture's own number — **and it is
  NOT `BAND_KIT_ROW_PREFIX`**, the vitals rows being disclosures, so what renders is the caret's own
  `Kit ▸` and the prefix is consumed by that wrapping) ·
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
register_action(id: StringName, glyph: String, tooltip: String, enabled: Callable = Callable())
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
them by key. A band declares **three** (`BandPanelController.BAND_ZONE_LAYOUT`); the faction page
declares **four** (`FACTION_ZONE_LAYOUT`), the extra being `knowledge`.

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
  before any subject has spoken, so a player who left on `knowledge` must not have that thrown away by
  the bootstrap layout; `_effective_tab` falls back to the first zone that has content whenever the
  live subject does not declare the selected one.

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
/ `Pen feed` rows for a while and they are gone: a band states `Food: 74 (93 turns) · -0.81 /turn` on
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

### KNOWLEDGE IS ITS OWN ZONE, AND THE FOUR-ZONE BUDGET IS MEASURED

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

- **THE PEOPLE BRACKETS ARE SUMMED FRACTIONAL AND APPORTIONED ONCE**, never apportioned per band and
  added. `HudFormat.apportion_people` apportions to `roundi(Σ parts)`, so summing first leaves ONE
  remainder to distribute — apportioning each band and adding the results reproduces the very
  off-by-one that function exists to remove, once per band. On the harness's two-band roster the two
  compositions differ by a whole person (61 against 60), which is what makes the assertion a
  discriminator rather than an identity.
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
| `Carried` (`N / cap` + the `· FULL` badge) | `is_raid`, and a carry cap that is > 0 and met |
| `Next delivery` (`↻` for a recurring party) | `is_hunt` + `has("expedition_projected_delivery")` |
| the trip-bound clause | a non-empty `expedition_trip_bound` |

That party measured **328px of the 300px box**. A DENIAL party is strictly shorter (five lines), and the
quoted-party note a between-rungs party earns rides the `Collapse:` ROW as a clause rather than as a
line — which is this budget's rule already being followed.

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
   be two rows. This is the band zone's SHORT-tier idiom (Morale + Growth, the Food row's hay clause)
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
those are one keypress away (`V`, `L`), so the region-versus-content distinction did not rescue it: a
flush rail really would sit under the minimap.

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

## The strip's height is 360 at ONE band column and 335 at two

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
adjacency structural rather than a rule this call site has to remember.

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

## Work rows carry ONE account, and the aggregates carry a SIBLING (issues #337 / #449 / #527)

A board row's rate column is a single fixed width, so it shows the account the source actually PAYS,
falling through **food → fodder**: food when there is food (unchanged for every forage patch and
edible quarry), else the fodder rate spelled with the WORD — `+0.40 fodder` on a sown hay Field, never
the `+0.00` that said the source was worth nothing. `_work_row_rate_text` is the one definition. The
**inspector strip** has room for the pair and states both (`SourceForecast.yield_components`).

**A TRADE branch stood between the two until arc #527** — `⇄+0.22` on a hunted wolf pack, marked with
the retired `FoodIcons.TRADE_GOODS_GLYPH`. With that account gone an inedible quarry has no
fall-through and reads `+0.00`, the wire quoting a herd no per-turn material figure at all. **A hunt
row's fodder is a structural zero** (no animal is harvested for feed), so a hunt call site passes no
fodder argument and the animal web has no second account to fall through to.

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

Frames `band_panel_work_trade_rows` (mixed board) / `band_panel_work_trade_inspector` /
**`band_panel_work_trade_totals`** (the aggregate-suppression path the mixed board cannot reach).
**The three keep their names and their subject moved**: they stage an inedible quarry, whose rows now
read `+0.00 /turn` because the wire states no rate for it rather than because the client dropped one.
The rule and the axis contract live in `labor-ui.md`.

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
range` — with the account rendered only when the quarry pays it (the render-only-when-non-zero rule),
and no alarm glyph anywhere.

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
may still split. `_split_worker_pool` is `floor()` of the cohort's `age_working`, which is the same
quantity the sim bounds the command by (`available_workers`), so the stepper's ceiling and the
server's refusal cannot disagree. The footer's `SEND_PARTY_NO_IDLE_REASON` line is therefore scoped
to the three expedition missions — it used to render unconditionally on `idle <= 0`, which put "No
idle workers to spare" directly under a live `⌂ Split`.

**THE SHEET SHOWS THE CONSEQUENCE, BECAUSE THE INPUT IS ONE NUMBER.** Workers stepper → the share it
implies → what the new band would be (people, brackets, dependants/worker, provisions) → the home
band beside its now → the verdict. Everything divides on that one share.

> #### BOTH HALVES ARE APPORTIONED IN **ONE** PASS
>
> `HudFormat.apportion_people_to` exists for this: the sheet apportions the new band's dependants and
> the parent's remainder in a single largest-remainder pass against `band people − chosen workers`.
> Running `apportion_people` separately over each half lets both round the same way and show **31
> people leaving a band of 30** — precisely the bug the PEOPLE block's apportionment was written to
> prevent, reintroduced on a new surface. The chosen worker count is **pinned** to the integer the
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
