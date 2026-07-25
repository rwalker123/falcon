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
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail. Header (stage glyph/name/label + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse), body hosts **THREE NAMED ZONES AT A FIXED CROSS-AXIS SIZE** via **`set_zones(band, work, parties)`** (keys `&"band"`/`&"work"`/`&"parties"`; the panel OWNS and frees them). Two shells, chosen by the panel's own **WIDTH** (`WIDE_SHELL_MIN_WIDTH` — never a dock-edge test, so a resizable dock needs no special case). **That threshold is DERIVED, never hand-picked**: `ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + ZONE_WORK_MIN_WIDTH + WIDE_SEPARATOR_SPAN + PANEL_CHROME_H` = 300 + 300 + 380 + 50 + 26 = **1056**. `ZONE_WORK_MIN_WIDTH` (380) MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` — one readable board column — exactly as `ZONE_WORK_MAX_WIDTH` (1520) mirrors `WORK_COLUMN_MIN_WIDTH × WORK_MAX_COLUMNS`; the two are a PAIR with Hud's column consts and move with them. The chrome term is load-bearing because the threshold is tested against the panel's OUTER `_panel_extent().x` while the zones live in `_interior_size()`. It shipped hand-picked at **900**, which broke the whole 900–1055 band: the work zone came out 224px, Hud clamped to one column, its labels clipped — and the NARROW shell would have given the board the full 874px, so flipping wide early made it ~4× narrower, degrading the thing the wide shell exists to improve. `WIDE_SEPARATOR_SPAN` / `PANEL_CHROME_H` are `const`s (a `const` cannot call `_wide_separator_span()`), shared by the threshold, `_wide_content_cap()`, `_wide_separator_span()` and `_interior_size()` so they cannot drift. **wide** (in practice T/B) = the three zones side by side, band/parties fixed `ZONE_BAND_WIDTH`/`ZONE_PARTY_WIDTH` (300), work EXPAND_FILL, `LINE_SOFT` hairlines between, no tab bar; **narrow** (in practice L/R) = a Band·Work·Parties tab bar under the header + exactly one zone beneath it (active tab = SIGNAL ink + a 2px SIGNAL underline, badges via `set_tab_badge(zone, text, hot)`, selection persisted as `CONFIG_KEY_TAB`). **The cross-axis size is FIXED** — `PANEL_WIDTH` 380 (L/R) / `PANEL_HEIGHT_WIDE` 360 clamped to `MAX_WIDE_HEIGHT_FRACTION` of the window (T/B) — so `current_reservation_size()` changes ONLY on dock/collapse/hide/viewport-resize and a content edit can no longer re-emit `reservation_changed` → `MapView.set_reserved_inset` → cache invalidation (the map flicker on every `+` press). **There is deliberately no `ScrollContainer` anywhere in the panel** (no-scroll by design; the work zone pages itself against **`work_zone_size()`**, the zone's interior after chrome — e.g. 354×1107 in a 380 L dock, 1244×298 in a 1920 bottom dock — and re-pages on the **`zones_resized`** signal). **Zone hosts are plain `Control`s, not containers**, so an over-wide zone content cannot push the card past its fixed cross-axis size; `clip_contents` keeps overflow inside its own zone. Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`. On a HORIZONTAL dock the card's row also carries **a trailing CHROME RAIL** the HUD parks its stacked bottom-bar chrome into (`rail_slot_host` / `set_rail_width`, issue #324 — see "Band/City dockable panel"). See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
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
- **Header chrome.** Settlement **stage glyph + name + stage label**
  (`set_header` — glyph/label from the band marker's `settlement_stage_icon` /
  `settlement_stage_label`, neutral glyph fallback), a `◀ n/N ▶` **cycler**
  (`set_cycler`) over `_player_bands`, a 2×2 **dock chooser** (active edge
  highlighted), and a **collapse** toggle. `cycle_requested(delta)` → Main relays
  to `Hud.cycle_panel_band`.
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
  population header block.
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
- **THE TRAILING CHROME RAIL — the HUD's bottom-bar chrome SHARES a horizontal dock's row** (issue #324).
  `_build` wraps `_panel_column` in a `_card_row` HBox and appends `_rail` AFTER it, so the row is
  `content column · rail` and the rail sits at the TRAILING end. The **card itself stays
  `PRESET_FULL_RECT`** deliberately: a bottom dock reads as ONE continuous bar, and insetting the card
  would break it into visual islands — the chrome sits ON the card, only the content column is inset.
  - **ONE column, not a gutter at each end.** A leading + trailing pair was built first and rejected on
    sight with a real minimap in it: `NavBacking` is ~300px wide, so two opposite gutters cost ~562px of
    row, pushed the band zone inward AND stranded dead space around the orb. One column costs
    `max(nav, turn)` — **≈296–302 depending on map aspect** (296 Standard, 302 Large; the rail width
    tracks the minimap's `grid_width / grid_height`), i.e. the NAV cluster, since the turn cluster shrank
    to 116 when its `Turn N` caption moved into the orb face — plus one `RAIL_SEPARATOR_SPAN` gutter, so
    **321 of row** on Standard. That hands the zones ~240px back (work zone
    688 → **923** at 1920×1080) and drops the wide→narrow flip from ~1618px of window width to
    **~1377px** (1056 + the span; across the whole `MapSizes.OPTIONS` roster, ~1377–1389). **The
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
    `_panel_extent()` is deliberately **untouched** — it is documented as the card's OUTER size and the
    card is still full-width — and `_position_seam()` needs no change either: the seam spans `_root` and
    correctly accents the whole reserved strip.
  - **`set_rail_width` can NEVER re-emit `reservation_changed`**, and that is what stops a feedback loop
    (HUD pushes a width → panel relayouts → no emit → no `Main` fan-out → no HUD reflow). The rail spends
    only the LONG axis; the reservation is the CROSS one (`_cross_axis_size`, which reads only the
    collapse flag, the dock edge and the viewport).
- **Zone `band` — vitals · PEOPLE · food outlook · WORKFORCE + role cards** (`BandPanelController.build_band_zone`).
  The Food/Morale/Output rows are the disclosures — and their breakdowns open in a POPOVER, never
  inline (see Band food status: inline growth is what clipped this very zone); the `Population … Workers … (Idle …)` LINE is
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
  from a worked source. **The zone yields by HEIGHT TIER** (`_band_zone_tier`, measured against the
  zone box — never the dock edge): full chart + hinted cards at/above `BAND_ZONE_TALL_MIN_HEIGHT`, a
  compact chart above `BAND_ZONE_CHART_MIN_HEIGHT`, and below it **no chart and hint-less cards** (a
  360px T/B dock). A tier change re-renders the zones; anything else just re-pages the board — that
  is what `_on_zones_resized` distinguishes, and skipping it lands a tall-shell band zone in a short
  box where its host silently clips it.
- **Zone `work` — THE PAGED BOARD** (`BandPanelController.build_work_zone` / `_fill_work_zone`). Header (`WORK` ·
  n sources · total /turn · a `⋯` `MenuButton`) · filter CHIPS · the board · pager · inspector strip.
  **The chips ARE the summary and the filter** (All / 🌿 Foraging n · rate / 🦌 Hunting n · rate / ⚠ k,
  the last hidden at k = 0), replacing collapsible group headers. Rows are ONE line at a fixed
  `WORK_ROW_HEIGHT`: severity stripe (WARN overdrawing/overstaffed, SIGNAL pending) · glyph · clipped
  label · rate · policy/⚠ marks · the existing `−/+`. **Capacity is derived ENTIRELY from
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
  **A source STANDING ON AN INVESTMENT RUNG is the case that picker cannot express, and it is handled
  HERE rather than by widening the picker.** `model["policy"]` can legitimately be `corral`/`cultivate`/
  `tame`/`sow`, none of which is in the offered four — so the radio highlighted **nothing** (reading as
  an unset control on a very-much-set assignment) and a press silently re-sent `assign_labor` at an
  extractive policy, **discarding a ~25-turn ladder build with no confirmation and no cue**. Two fixes,
  both keyed on `policy in INVESTMENT_POLICIES`: (1) the strip renders a WARN line above the picker
  naming the standing rung and what a pick costs (`WORK_INSPECT_STANDING_INVESTMENT_FORMAT`, built from
  the shared `HudFormat.policy_face` glyph+name vocabulary — the picker's own gate-reason lines use it too, so a
  rung cannot read one way beside the buttons and another in the dialog), and it reserves
  `WORK_INSPECTOR_STANDING_LINE_HEIGHT` on top of `WORK_INSPECTOR_POLICY_HEIGHT` via the shared
  **`_work_inspector_height(model)`** that BOTH `_work_board_capacity` and the strip itself measure from
  (the work-board rule: every reserved height is what the element actually draws at); and (2) the pick
  routes through the same **`_confirm_destructive`** behind "Unassign all work" / "Recall all parties"
  (`_on_work_policy_picked` → `_commit_work_policy`), naming the rung being ended, the source, and the
  rung replacing it. **The EXTRACTIVE path is byte-for-byte unchanged** — no confirm, immediate emit —
  and `band_panel_preview`'s two CONTROL assertions exist to keep it that way. The strip does NOT show
  the build's progress: the work model carries no `corral_progress`/`cultivation_progress`, and this is
  not worth new plumbing (the source's own compose control has the meter).
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
  **CONTAINMENT IS NOT COMPLETENESS, and that distinction is a second assertion.** Content the box
  cannot hold gets CLIPPED, and clipped content still reports a rect *inside* its host — so the
  bounds assertion passes on a frame that is visibly sliced (the Food/Morale inline breakdown cut the
  WORKFORCE row mid-glyph and erased both role cards, with every assertion green). `band_panel_preview`
  therefore also runs **`_assert_zone_content_fits`**: for every visible descendant of a zone host,
  `top + get_combined_minimum_size().y` must fit the zone box. It recurses past the zero-minimum
  plain-`Control` wrappers `HudWidgets.wrap_zone` produces (they report no minimum, so measuring them alone
  proves nothing) and stops at the first control that DOES report one — its minimum already accounts
  for its children. Run it beside the other two at every state.
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
  **`band_panel_work_policy_investment`** (a Hunt row standing on `corral` — no rung lit, the WARN
  standing line above the picker) and **`band_panel_work_policy_extractive`** (the same picker on the
  `sustain` row beside it). Four assertions, and which ones may move is the whole point: the two RED
  ones ride the investment frame — the standing line is RENDERED, and pressing a real rung button
  raises a `ConfirmationDialog` while `assign_labor_requested` does NOT fire (both verified to FAIL
  before the fix, the second emitting immediately) — while the two CONTROLS ride the extractive frame
  and must pass BEFORE and AFTER: a pick emits immediately with no dialog, and **exactly one** rung
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
  chartless fixtures never overflow).
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
  slice the minimap. `_reflow_round_trip`: bottom → left → bottom → left, asserting the clusters came home
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

## Work rows and the two hunt products (issue #337)

A board row's rate column is a single fixed width, so it shows the product the source actually PAYS:
food when there is food (unchanged for every forage patch and edible quarry), else the trade rate with
`FoodIcons.TRADE_GOODS_GLYPH` — `⇄+0.22` on a hunted wolf pack, never the `+0.00` that said the hunt
was worth nothing. `_work_row_rate_text` is the one definition. The **inspector strip** has room for
the pair and states both (`SourceForecast.yield_components`), which is where an edible quarry's trade
shows. The zone's header total and the filter chips stay **food-denominated**: they mirror the sim's
`food_income`, and trade goods never enter the larder. Frames `band_panel_work_trade_rows` /
`band_panel_work_trade_inspector`; the rule and the axis contract live in `labor-ui.md`.
