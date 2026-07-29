---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{ComposeSheet,ComposeState,DrawerComposeController}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{HudBandLaborState,SourceForecast,FoodOutlookChart,ArrivalStrip}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{hud_compose_vocab,hud_work_vocab}.gd"
---

<!-- Extracted verbatim from lines 171-172;179-179;185-186;1776-2474 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Labor allocation UI — the compose sheet and forecasts

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/HudBandLaborState.gd` | `RefCounted` state model (HUD decomposition Phase 0) — "the digested per-snapshot player world + optimistic overlay": `player_band`/`player_bands`/`panel_band`/`player_expeditions`, `world_herds`, grid scalars, `current_turn`, the `prev_band_sizes` losing-population diff, the `forage_patch_lookup`/`food_module_by_tile` lookups, and the `pending_labor` optimistic overlay. Ingest mutators (`set_turn`/`set_grid(width, height, wrap)`/`set_world_herds`/`set_panel_band`/`ingest_snapshot_bands`/`set_food_modules`/`set_forage_patches`) + the pending API (`record_pending_assign`/`record_pending_move`/`reconcile_pending`/`pending_assigns_for`/`pending_key`) + the moved-on derived readers `effective_worker_map`/`effective_idle`/`effective_forage_workers`/`effective_hunt_workers` (pure functions of `pending_labor` + a band) + the statics `as_schedule` and **`labor_assignments_of(band)`** (the public band-dict `labor_assignments` reader — `DetailFormat` + `AttentionController` reach it as `HudBandLaborState.labor_assignments_of`; it merged HudLayer's `_labor_assignments_of` static into the byte-identical private copy that already lived here, and its four internal callers now call it unqualified. The MapView-side `BandOverlayRenderer._labor_assignments_of_marker` deliberately stays a LOCAL copy — a renderer must not depend on the HUD's band-labor model). Also owns the **thin band-labor readers** every consumer reaches through `_band_labor.` — the roster pair `current_player_bands`/`player_band_by_entity`, the per-source lookups `forage_assignment_of`/`hunt_assignment_of` and their `workers_for_forage`/`workers_for_hunt`/`policy_for_hunt`/`policy_for_forage`/`assignable_forage_workers`/`assignable_hunt_workers` — plus the DERIVED READS over its own tables that the `BandPanelController` shared-layer pass brought home: `find_world_herd` (8 call sites file-wide — herds MIGRATE, so this list, never an assignment's launch-time target, is the authority on where a hunted herd is), `food_module_icon` (+ its `FOOD_SITE_KIND_GAME_TRAIL` key), `effective_role_workers`/`workers_for_role` (the band-wide-role twins of `effective_forage_workers`/`workers_for_hunt`), and **`band_parties`/`band_party_workers`** — the pair that KILLED the band↔parties straddle, since the WORKFORCE bar's Parties segment and the parties zone's row set now read one filter over `player_expeditions()` rather than the band zone calling into the parties zone. Plus the canonical policy-rung consts `HUNT_POLICY_OPTIONS`/`FORAGE_POLICY_OPTIONS`/`DEFAULT_HUNT_POLICY` (the last aliases `SourceForecast`'s; `HudLayer` re-exports all three via `const X = HudBandLaborState.X`). Emits `changed(reason)`, consumed by nothing yet |
| `ui/hud/ComposeState.gd` | `RefCounted` state model (HUD decomposition Phase 2c-1) — "what the player is dialing but has not committed": the tile card's **forage** compose (`forage_key`/`count`/`policy`/`species`/`band` + its autofill one-shot), the herd drawer's **hunt** compose (`hunt_key`/`count`/`policy`/`band` + its own one-shot), the Band panel's PARTIES-zone **party** compose (`party_quarry_id` + its one-shot) on its own clearly-separated accessor group so a later band-panel extraction can take it without unpicking the drawer's, and the open sheet's subject identity (`kind`/`subject`; `COMPOSE_KIND_*` alias to its `KIND_*`). Mutators are named for the transition — `begin_*_source` + `seed_*` (the two-step source-changed re-seed: the caller must resolve the actual band between them), `set_*`, `arm_*_autofill`/`consume_*_autofill`, `reset_*_source` (the harnesses' way to stage a fresh compose), `set_composing`/`clear_composing` — and the three READ-MODIFY-WRITEs get explicit ones so the field is never read and written apart: **`clamp_forage_count`/`clamp_hunt_count`** and **`resolve_forage_species(resolver: Callable)`** (the RMW is the model's; the crop RULES stay with the caller, so it holds no flora knowledge). Pure DATA — which is exactly why **the `ComposeSheet` NODE lives on `DrawerComposeController`**, beside the lifecycle that opens it, rather than on this model. The model instance is SHARED: HudLayer (the parties zone) and that controller (the drawer) hold the same one. Deliberately **NO `changed` signal**, unlike the Phase-0 pair: nothing subscribes (the compose builders re-render explicitly) and unused API is a liability. **`hunt_policy()` is PUBLIC beyond its builder, but its readers are all HERD-DRAWER ones now** (`_tame_stalled_hint` / `_herd_crew_noun`): `HudWidgets.build_policy_picker`'s `selected` fallback — the one real cross-boundary read, where a work-inspector or party-compose render picked up the DRAWER's rung — was DEAD (every caller passed an explicit, provably non-empty `selected`) and is **deleted**; `selected` is a REQUIRED param, so the shared builder now owns none of its callers' state and the drawer/band-panel boundary is structural rather than conventional |
| `ui/hud/DrawerComposeController.gd` | `RefCounted` controller (HUD decomposition Phase 2c-2b, `docs/plan_hud_decomposition.md`) owning the selection drawer's **COMPOSE half** — the other half of the selection card, after `SelectionCardController` took the identity/list one. It holds the **compose-sheet lifecycle** (`_ensure_compose_sheet` / `open_forage_compose` / `open_herd_compose` / `refresh_compose_sheet` / `is_compose_sheet_open` / `close_compose_sheet` / `_compose_anchor_rect`, and the `ComposeSheet` NODE itself), the two **drawer-action builders** (`build_forage_drawer_actions` / `build_herd_drawer_actions` + the standing-summary / compose-open-button / extend-pen factories and their in-place diffing twins), the two big **compose builders** (`_build_forage_assign_controls` / `_build_herd_assign_controls`), and the **compose-only** forecast/gate/picker layer beneath them (`_forecast_worker_cap` / `_forecast_yield_row` / `_is_overdraw` / `_hunt_take_rate` / `_hunt_delivered_and_waste` / `_hunt_avg_window_turns` / `_hunt_policy_takes` / `_payoff_take` / `_local_hunt_preview_bbcode` / `_local_forage_preview_bbcode` / `_forage_policy_takes` / `_forage_policy_gates` / `_hunt_policy_gates` / `_sow_site_refusal_reason` / `_tame_stalled_hint` / the `_flora_entry_*` sub-layer / `_build_crop_picker` / `_build_band_picker`) — ~1,400 lines, 54 functions. It also owns the drawer-actions diff caches `_forage_drawer_shape` / `_herd_drawer_shape` (zero external readers), so a per-snapshot restate still patches nodes rather than tearing them down. The drawer RENDER DISPATCH (`_render_land_drawer` / `_render_occupant_drawer` / `_render_subject_drawer` / the terrain-lines producer + `_tile_detail_lines_cache` / `_fit_subject_drawer`) and the `%AllocationPanel` expedition/band-move branches later left `HudLayer` too, into `ui/hud/SubjectDrawerController.gd` (Phase 2c-3), and call IN here through `refresh_compose_sheet` / `build_forage_drawer_actions` / `build_herd_drawer_actions`. Hud holds it as `_drawercompose`, constructed in `_ready` after `_selectioncard`. **THE INJECTION SURFACE IS EXACTLY THREE CALLABLES** — `_resolve_assign_band` / `_herd_label_for_id` / `_emit_assign_labor`, each retained on HudLayer because it has callers on the other side too (and `_emit_assign_labor` additionally owns the `assign_labor_requested` emit, the optimistic pending write and `_after_pending_change()`, which is why `assign_labor` stays INDIRECT). Each is reached through a **typed adapter** rather than called raw — `Callable.call` returns `Variant`, which would push an untyped value into every consumer. Everything else is a collaborator: the SAME `_compose` / `_band_labor` / `_selection` model instances (BY REFERENCE), `_topbar` for `faction_knowledge` ONLY (the rung gates), `_selectioncard` for `tile_contents_unseen` ONLY, the two drawer-action containers it fills (`%HerdAssignControls` / `%ForageAssignControls`), `tile_panel` READ-ONLY (the rect the sheet floats beside), and the HUD CanvasLayer as the **host** it `add_child`s the `ComposeSheet` into (a `RefCounted` cannot parent — the `TurnOrbController` fork-panel pattern). **Three absorptions shrank that boundary from six injections to three:** `_expedition_party_cap` → `SourceForecast.expedition_party_cap` (expedition forecast math, beside its sibling `expedition_useful_cap`), `_format_food_module_label` + its `FOOD_MODULE_LABELS` table → `HudFormat.food_module_label` (vocabulary, not compose logic), and — the highest-leverage one — the grid-wrap flag `_grid_wrap_horizontal` **onto `HudBandLaborState` as `wrap_horizontal()`**, beside the `grid_width()` it is meaningless without, so the moving set calls `SourceForecast.hex_distance_wrapped(…, _band_labor.grid_width(), _band_labor.wrap_horizontal())` DIRECTLY and the `_hex_distance_wrapped` injection disappeared (that pass-through survives on HudLayer for its other callers). `_band_display_name` went to `HudFormat.band_display_name` for the same reason. **It emits TWO signals, both RELAYED by HudLayer** (the controller never emits a HudLayer signal): `send_hunt_expedition_requested` → `HudLayer.send_hunt_expedition_requested` and `extend_pen_requested` → `HudLayer.extend_pen_requested` (the latter travels because `_build_extend_pen_control`'s only caller and its diffing twin are both inside). **`is_compose_sheet_open` / `close_compose_sheet` MUST stay callable on the HUD node** — `Main._unhandled_input`'s Esc precedence and ~11 ui_preview sites probe them BY NAME, and a `has_method` probe fails SILENTLY — so HudLayer keeps them as thin delegators. Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`TopBarReadouts`/`SelectionCardController` convention. Behaviour identical to the old inlined drawer-compose code |
| `ui/hud/ComposeSheet.gd` | The selection card's **write state** — the floating **compose sheet** (`docs/plan_tile_panel_layout.md` §10-§15). Composing is MODAL BY NATURE (open, decide, commit, done), so the two ~270px compose blocks (`%ForageAssignControls` / `%HerdAssignControls`) left the drawer for a sheet that borrows space only while in use; the drawer keeps the detail rows, a one-line standing summary and an `Assign … ▸` button. **That button wears `primary` while ITS sheet is open and `ghost` at rest — never `armed`**: `armed` is the destructive/warned treatment (DANGER border), and "its sheet is open" is a LIVE state, which this HUD spells in SIGNAL cyan (the Sight chip, the selection accent, the turn orb's calm pulse). **Its card is an `AutoSizingPanel`, NOT a `DockScrollFit` card** — it floats against the VIEWPORT, which is the opposite of what the drawer above needs, and picking wrong misbehaves silently rather than failing (`.claude/rules/client/panel-framework.md`). **The node IS the full-screen dismiss catcher with the card as its CHILD**, reusing `NarrativeForkPanel`'s nesting exactly (siblings make the ordering ambiguous and the catcher eats the card's own clicks), pinned to the viewport EXPLICITLY via `_sync_to_viewport` — a hidden Control's anchors never settle, and the full-rect preset would also overwrite the size. **NO SCRIM, and that is the one deliberate departure from the fork panel:** a fork is a story beat demanding attention, an assignment is composed *against* the map (work-range ring, herd position, hunt reach are all live context), so the catcher dismisses without dimming. **And that is also why the catcher dismisses on a real CLICK only, never a wheel tick** (`DISMISS_BUTTONS`, an ALLOWLIST of left/right/middle so a future Godot wheel/extra index stays non-dismissing by default): the catcher is `MOUSE_FILTER_STOP` across the whole viewport, so an idle scroll over the un-scrimmed map lands on it, and dismissing there would throw away the composition mid-read. `NarrativeForkPanel` is deliberately left as-is — a modal scrimmed story beat has no such gesture — so the two diverge here on purpose; do NOT factor out a shared predicate for one differing call site. (**Not** a map-zoom passthrough: the catcher stops the wheel either way, so the map cannot zoom while a sheet is open, and a wheel over the card is absorbed by its own `ScrollContainer`.) Guarded by ui_preview's paired wheel-leaves-OPEN / left-click-CLOSES assertions. The sheet floats BESIDE the selection card (`_place_card`, falling back to the viewport margin) so the list + summary it is editing stay readable. It knows nothing about foraging or hunting: `open(eyebrow, title, subject_key, anchor)` returns the content VBox and the caller fills it. `subject_key` is what lets a per-snapshot refresh tell "the same source, restated" from "a different source, gone" |
| `ui/hud/RungGates.gd` | **All-`static`, stateless** shared RUNG-GATE layer — the one answer to "may this source climb its next rung, and if not, why not?". Extracted from `DrawerComposeController` (issue #412) when the compose sheet stopped being the only surface asking: the Band panel's WORK board marks a source that can climb, and the MAP marks it on the source's own marker — and a renderer must not depend on the HUD's compose controller. Shared-layers-BEFORE-controllers, the same measurement that produced `SourceForecast` and `HudWidgets`. Holds `forage_gates` / `hunt_gates` / `sow_site_refusal_reason` (moved VERBATIM, so the compose sheet's greying is unchanged), **`forage_gates_from_patch`** (the BARE-keyed twin for a raw wire patch — the `tile_info` cross-ref's prefixing is MIXED, `patch_ecology_phase`/`patch_is_field`/`patch_sow_site_refusal` but plain `is_cultivated`, and this adapter is the ONE place that mapping is written down), and **`next_rung_ready`** — the READY test all three surfaces mark from. **STATELESS IS THE INVARIANT**: the one impurity, faction knowledge, is threaded in as a `knowledge` PARAMETER (`TopBarReadouts.faction_tracks(faction)`, the whole `{track: progress}` row `faction_knowledge` reads one key out of), never reached for. `next_rung_ready` requires all three of OFFERED (husbandry ceiling / `can_cultivate`-`can_sow` + willing ground), UNGATED (the gate functions answer nothing), and NOT-ALREADY-RUNNING (a patch mid-Cultivate is progress, not an opportunity), **highest rung first**. **That ordering is load-bearing on the PLANT web only** and its assertion needed care: `is_cultivated` retires Cultivate, so on a TENDED patch the two rungs are mutually exclusive and an ordering test there passes with the branches swapped (measured). `Sow` needs no prior patch, so a WILD patch on sowable ground is the one shape that clears both gates at once. On the animal web the rungs are always mutually exclusive — Tame retires at a full meter, Corral requires one — so ordering is genuinely not load-bearing there. `TopBarReadouts.faction_knowledge` deliberately does NOT call `RungGates.track`: dependency DIRECTION outranks the one-definition rule for a `float(d.get(k, 0.0))` |
| `ui/hud/SourceForecast.gd` | **All-`static`, stateless** shared forecast/estimate layer (HUD decomposition, phase 2c-2 precursor) — the pure "what will this source give me?" math THREE consumers ask for: the drawer's compose blocks, the Band panel's WORK zone, and its PARTIES zone. Three families: POST-HOC `source_yield_readout` (what a worked source actually produced, incl. the ⚠ overdraw + overstaff/wasted notes) · PRE-COMMIT `forecast_inputs` / `max_useful_workers` / **`source_worker_cap_state`** (the CONFIRMED-row twin of that cap: `(forecast, workers, idle, useful_floor = 0) → {can_add, note}`, beside the ceiling it reads so a worked row and a compose stepper can never gate differently — the trailing floor is what makes that true rather than merely stated, and `herd_crew_floor` is its one definition) / `expected_yield` / `hunt_policy_ceiling` · THE RAID `hunt_trip_forecast` → `hunt_forecast_line_bbcode` / `hunt_trip_no_surplus` / `hunt_no_surplus_reason` / `expedition_party_cap` / `expedition_useful_cap` / `expedition_policy_takes` / `style_send_hunt_button` (`style_send_hunt_button` styles a Button off the raid verdict, so it lives WITH the verdict). Plus the shared leaves those need — `format_magnitude`/`format_signed`/`format_yield`/`extractive_take`, `band_tile`/`hex_distance_wrapped`, `herd_display_name`, `is_managed_hunt_source`, and the two one-off leaks into the read-only detail layer, `flora_basket_entries` / `husbandry_ceiling`. **WHY ITS OWN FILE:** the next phase lifts a `DrawerComposeController` out of `Hud.gd`, but this layer is called by the work + parties zones too, so it cannot travel with the drawer; pure injection was measured at **54 Callables** and a `_hud` back-ref would weld an already-pure layer to the god object (and the band-panel extraction would then need a SECOND back-ref to the same place). All three consumers depend on THIS instead. **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache; if a new function needs HUD state, pass it in. The one non-plain-value is the grid-wrap pair (`grid_width`, `wrap_horizontal`), threaded as EXPLICIT PARAMETERS through `hex_distance_wrapped` → `round_trip_travel_turns` → `hunt_trip_forecast` / `expedition_policy_takes` so a stale grid can never be captured; `HudLayer._hex_distance_wrapped` is a one-line pass-through supplying the pair off `_band_labor`, so there is ONE hex implementation (`DrawerComposeController` calls the module directly with the same pair). The **forecast vocabulary constants moved here with the math** (`LABOR_KIND_*` / `LABOR_HUNT_POLICIES` / `DEFAULT_HUNT_POLICY` / `SOURCE_KIND_*` / `FORECAST_*` / `MAX_USEFUL_*` / `HUNT_FORECAST_*` / `SEND_HUNT_*` / `HUSBANDRY_CEILING_*` …) and `HudLayer` **re-exports the still-used ones as aliases** (`const X = SourceForecast.X`, one commented block) rather than redefining them — ONE definition, and every HudLayer call site reads unchanged |
- **Labor allocation UI** (`Hud.gd`, Early-Game Labor slice 3b — `docs/plan_early_game_labor.md`):
  the band is a **labor pool** whose working-age workers are assigned source-centrically to
  in-range sources/roles. There is **exactly one player band today**, captured each snapshot
  into `_player_band` (first player-faction cohort in `update_band_alerts`); assign/move/clear
  all target it. Every player band is also collected into `_player_bands`, which backs the
  **band-picker dropdown** on the herd/tile assign controls (see `%HerdAssignControls` /
  `%ForageAssignControls` below) — an assignment explicitly names WHICH band supplies the
  workers (built for N even though only one exists live). Three runtime-built control sets replace the retired single-task Scout/Cancel,
  Hunt/policy, and Forage buttons:
  - **`%AllocationPanel`** (band drawer, player band only, `_build_allocation_panel`): reads as a
    "current actions" report — a `Population <size> · Workers <working_age> (Idle <n>)` line (spells
    out that only the ~16 working-age labor, not the 30 people — children/elders are dependents;
    `WORKERS_HEADER_FORMAT`, idle from `_effective_idle` so it counts optimistically), a
    **Current actions** section with one `−/+` **worker-stepper** row per staffed Forage tile / Hunt
    herd (from the cohort's `labor_assignments`; an empty-state hint when none). **A Forage/Hunt row is
    TWO lines** (the `status_line` opt-in on `HudWidgets.build_worker_stepper` → a `VBoxContainer`; the
    Scout/Warrior role rows and the compose steppers stay the single-line `HBoxContainer`): **line 1** is
    the resource-glyph title + tile/species (`🌰 Forage (27, 26)`) beside the `−/+` stepper, keeping its
    click-to-jump link; **line 2** is an INDENTED, smaller (`ALLOC_SECTION_FONT_SIZE`), `HFlowContainer`
    that WRAPS carrying the yield + policy glyph + status glyph + any ⚠/overstaff/wasted notes
    (`+0.48 /turn  ♻  ●  · only 2 of 5 working`), so the row reads narrow and never forces the panel
    wider. `HudWidgets.build_two_line_stepper` / `HudWidgets.build_row_name_label` / `HudWidgets.build_status_part` /
    `HudWidgets.add_stepper_controls` factor the title/stepper/status parts so both forms share them. **A row
    states its policy and its status as GLYPHS, not words** — the old
    `[sustain]` / `· pending` word-tags were long and, for pending, redundant with the amber tint.
    Both come from the one glyph registry, `FoodIcons` (`for_policy` / `for_status`; see the
    **action-status vocabulary** header block in `Hud.gd`), and the WORDS move into the row tooltip
    (policy name + its existing `FORAGE_POLICY_HINTS`/`LOCAL_HUNT_POLICY_HINTS` behaviour hint — a
    worked source row is always a RESIDENT band's, so the hunt side reads the local set, never
    `SEND_HUNT_POLICY_HINTS`, whose payoffs differ; `_policy_hint` is the one lookup), plus the
    status in words), composed WITH the tooltip the row already carried (yield readout, overstaffing
    explanation, click-to-focus hint). Two orthogonal layers: **status** = what the action is doing
    (a confirmed local forage/hunt row has no sim phase — it is simply `working` `●`), and
    **`pending`** = a state of the ORDER (composed locally, not yet acknowledged; it rides on ANY row,
    is a modifier rather than a phase member, wins the glyph slot with `○`, and keeps the amber label
    tint). The policy glyph is read off the assignment's `policy` field (populated for forage too); an
    an assignment whose policy is unset falls back to no glyph. **Each source row headlines its per-turn food yield**
    (`… +0.31 /turn`, the assignment's `actual_yield`), with a WARN-tinted `⚠` **overdraw flag** driven by
    the **sim-answered `overdraws` bool** on the assignment (`LaborAssignment.overdraws`, policy-driven:
    `!managed && policy.overdraws()`, false for Sustain and managed/investment sources; decoded in
    `native/src/lib.rs` beside `wasted_yield`). This **replaced** the old client-derived `actual >
    sustainable + ε` test on the confirmed rows, which **false-positived on a hunt's kill turn** — cashing a
    banked whole animal spikes `actual` above the steady `sustainable` even under Sustain, so the row wrongly
    flashed ⚠. A Sustain source reads `… · renewable` (no flag); a Surplus/Deplete/Eradicate forage patch or
    an over-hunted herd trips the flag. A `tooltip_text` spells out actual-vs-sustainable. (The **compose
    previews** still derive it from the steady forecast via `_is_overdraw` — there is no assignment, hence no
    `overdraws` field, at compose time, and the forecast is not a lumpy `actual`.) **Each source row also flags overstaffing** — a
    WARN-tinted `· only N of M working` note (`OVERSTAFF_NOTE_FORMAT`) when `workers > workers_needed`
    (and `workers_needed > 0`), i.e. the source's take was capped at its ceiling so the surplus workers
    idled HERE and should be reassigned; the `tooltip_text` (`OVERSTAFF_TOOLTIP`) explains it. This is
    **orthogonal to the ⚠ overdraw flag** and deliberately NOT the same glyph: overdraw is *ecological*
    (taking past regrowth), overstaffing is *labor* (wasted workers) — a source can be overstaffed while
    perfectly sustainable (every policy has a ceiling), or overdrawn while fully used. `workers_needed
    == 0` (rehydrated, or a pending optimistic assign) means "unknown" → no note, never a
    wrong one.
    **ONE yield row per rung — each rung gets the row that informs ITS decision, never both.** On the
    **local hunt** the EXTRACTIVE four render `_local_hunt_preview_bbcode` (the crew's honest carry-aware
    delivered take, ANIMALS-first — `≈1 Red Deer/turn` — PLUS the sustainability verdict `· renewable` /
    `⚠ overdraws the herd`, and a WARN `· ⚠ N% wasted` suffix when a kill can't be carried; see the
    animals-first preview note below) and the INVESTMENT rung (Corral)
    renders `_forecast_yield_row` (`Preparing: +0.23 → then +1.05` — the dip→payoff deal, which a single
    rate structurally cannot express; Corral draws sustainably, so no overdraw verdict is lost).
    **Forage now mirrors the hunt split** — its EXTRACTIVE rungs render `_local_forage_preview_bbcode`
    (the plant twin, a bare rate + `· renewable` / `⚠ … — overdraws the patch`; no animal rhythm, so no
    waste suffix) and only its INVESTMENT rungs (Cultivate/Sow) keep `_forecast_yield_row`. Rendering
    both on a hunt was a merge artifact: the flat `per_worker_yield`/`ceiling_*` scalars and the
    `hunt_policy_ceilings` list are **two views of ONE sim hunt model** and agree numerically (verified:
    both give +0.54 on a Deplete take — the redundancy was measured before it was removed, not assumed), so
    the second row added no information and **argued with the first** — a HEALTHY-green "Expected yield"
    directly above a WARN-amber "⚠ overdraws the herd" for the same number. (The two overlapping wire
    representations should be collapsed to one server-side; tracked separately.) Both the ⚠ and the note are rendered by `HudWidgets.build_worker_stepper` (`warn` / `note` params)
    off one `SourceForecast.source_yield_readout`, so Forage and Hunt rows share the logic.
    **Each source row leads with its resource glyph** — `FoodIcons.for_site(module)` for a Forage
    row (resolved from `_food_module_by_tile`, the snapshot `food_modules` array pushed by `Main` →
    **`Hud.update_food_modules`**, keyed by tile) and `FoodIcons.for_herd(species)` for a Hunt row —
    the SAME icon the map marker draws, so a source reads identically in the panel and on the map. An
    unresolvable module renders the row bare (no fallback sprig).
    **Each source row's LABEL is clickable — it jumps the map to the source being worked.**
    `HudWidgets.build_worker_stepper`'s optional `on_focus_source` Callable turns the label into an inline link
    Button (`HudStyle.apply_link_button` — plain at rest, hover tint + `SIGNAL` text + pointing-hand
    cursor, a far tighter padding than the boxed ghost chrome); it is a *separate child* from the
    `−`/`+` stepper, which is untouched, and the count stays right-aligned. Both handlers route
    through `BandPanelController.focus_labor_source` — the SAME path the Active-expeditions rows and the turn-orb
    "Jump →" use: `alert_focus_requested` → `MapView.focus_and_select_tile`, plus (herd only)
    `roster_occupant_selected` → `MapView.select_occupant` so the herd's own drawer opens rather than
    whatever occupant the hex auto-selects; `_panel_band` is restored afterwards, so focusing a hex
    that hosts another band can't hijack the panel. **Forage** jumps to the assignment's
    `target_x/target_y` (a patch is a fixed tile). **Hunt** deliberately does NOT — herds MIGRATE, so
    `_focus_hunt_source` resolves the herd's **live** tile from `_world_herds` via `_band_labor.find_world_herd`
    (the Hud mirror of `MapView._herd_by_id`, which the hunted-herd ring already resolves through),
    falling back to the assignment target only when the herd is unknown. `_world_herds` is the
    snapshot `herds` array, pushed each snapshot by `Main` → **`Hud.update_herds`**; it also backs
    `_herd_label_for_id`'s new fallback, so an off-hex hunted herd reads "Red Deer" instead of the raw
    `game_deer_07` id. **Scout/Warrior are band-wide roles with no tile → plain, non-clickable
    labels.** Verified by `band_panel_preview` state `band_panel_source_row_hover` (the harness
    force-hovers the Hunt link, so the affordance shows in a static frame).
    `actual_yield`/`sustainable_yield`/`workers_needed` are decoded per assignment in
    `native/src/lib.rs` (inside
    `labor_assignments`); the band-level food flow (net rate + Gathered/Hunted/Eaten breakdown) lives
    on the **Food summary line**, not here — see "Band food status". Then a **Band roles**
    section with the always-shown **Scout** + **Warrior** rows (even at 0), each with a one-line hint so
    the `−/+` steppers read as "this is how you staff this standing role" (Scout's hint reads "Extends
    the band's sight — more scouts see further"; more staffed scouts extend the band's actual sight
    range, so the effect shows directly in the fog, not as a map-action or a reveal disc). Then
    **Move** / **Clear all**.
    Each stepper re-sends `assign_labor_requested` with the new count (0 removes). **The Forage/Hunt
    Current-actions rows are PER-SOURCE max-useful capped** (mirroring the compose controls' cap): each
    row's `+` is gated on `idle > 0 AND workers < max(max_useful, herd crew)` via
    `SourceForecast.source_worker_cap_state` + `SourceForecast.max_useful_workers` +
    `SourceForecast.herd_crew_floor` (hunt rows only — see the herder-floor bullet below), so a single
    source can't absorb workers past the point they help, and can't be capped below the keepers the sim
    demands either. The Hunt
    row reads its herd's forecast from `_band_labor.find_world_herd(herd_id)` (bare `BARE_FORECAST_PREFIX`); the
    Forage row reads its patch from the new `_forage_patch_lookup` (Main pushes the snapshot
    `forage_patches` → `Hud.update_forage_patches`, mirroring `update_herds`) with the SAME
    `BARE_FORECAST_PREFIX` (the raw wire patch dict carries the forecast fields un-prefixed, unlike
    the `patch_`-prefixed tile_info cross-ref the compose control reads) — the two rows are told apart
    by their `SOURCE_KIND_*`, never by the prefix they share. An unknown forecast
    (`MAX_USEFUL_UNBOUNDED`) falls back to the plain `idle > 0` gate; a source capped at max-useful with
    idle still available spells the reason in the row tooltip (`MAX_USEFUL_CAPPED_TOOLTIP`). **Scout /
    Warrior are band-wide roles with no ceiling — they keep the plain `idle > 0` gate.** Verified by
    `band_panel_preview` state `band_panel_source_cap`.
  - **Optimistic pending feedback** (slice 3b UX): assigning workers or moving the band shows
    immediately, before the next snapshot. `_emit_assign_labor` / `_try_dispatch_pending_move_band`
    record a HUD-local **pending** entry per band entity (`_pending_labor[entity] = {turn, assign:{key→…},
    move:{x,y}}`) and re-render. In the panel, a pending source row reads **amber with the `○` pending
    glyph** (the words live in its tooltip — "Pending — starts when you advance the turn"; the amber
    stays the primary signal, tying the row to the amber pending hex on the map) and the header
    **Idle** counts optimistically (`_effective_idle` = working-age − effective
    assigned, overlaying pending). **Reconciliation is turn-based:** each pending entry is tagged with the
    snapshot `turn` (header tick, set in `update_overlay`); `_reconcile_pending` (called from
    `update_band_alerts` each snapshot) drops entries issued on an OLDER turn — a newer-turn snapshot is
    authoritative confirmation and cleanly absorbs server-side clamping (the snapshot shows the real
    count). Pending is emitted to MapView via `labor_pending_changed` → `set_labor_pending`.
  - **Selected-band map highlights** (`BandOverlayRenderer.draw_band_work_highlights`, drawn when a player band
    is selected, cleared on deselect): the **worked forage tiles** (strong green fill on each
    `forage` assignment's `target_x/y`), the **three range borders** (`_draw_range_border`: a clean
    PERIMETER outline of each reach's hex disk — traced edge-by-edge, drawing an edge only where the
    neighbour across it leaves the disk, NOT a filled tile-by-tile mesh — using the sim's true **odd-r
    hex distance** `hex_distance_wrapped` via `MapView._hex_distance`, so the boundary ==
    actually-in-range; forage **green** at `work_range` (ties to the worked-forage fills), hunt **red**
    at `hunt_reach` when it extends past `work_range` (ties to the hunted-herd rings), scout **azure**
    at `scout_reveal_radius` when scouts are staffed — nested and color-distinct, all at every zoom),
    and the **hunted herds** (red ring on the herd tile + a band→herd link, drawn wherever the herd is
    since hunt reach = `work_range` + leash). **Per-source yield annotations** (`_draw_yield_label`): each staffed forage
    tile / hunted herd is labeled with its per-turn rate (food/turn, from the assignment inside
    `labor_assignments`) as a small drop-shadow number above the tile center (reusing `_draw_marker_glyph`),
    food-income **green**. **A HUNT label headlines `sustainable_yield`** (the steady per-turn rate),
    **a FORAGE label `actual_yield`** — the exact split `SourceForecast.source_yield_readout` uses for the Band
    panel (a hunt's `actual_yield` is the kill-credit PULSE — 0 on a wait turn, a spike on a kill turn —
    so its honest rate is `sustainable_yield`; forage has no pulse, `actual == sustainable`), so the map
    label and the Band panel's hunt headline can never disagree. A source that overdraws (the
    **sim-answered `overdraws` bool** on the assignment — the SAME wire flag the Band panel's
    `SourceForecast.source_yield_readout` reads, NOT the client-derived `actual > sustainable`, which false-positives on a
    hunt's kill turn) reads
    **WARN amber + a `⚠`** — an over-hunted herd, or a non-Sustain forage patch now that the forage
    policy axis can decline one (a Sustain forage gathers at regrowth, so it stays green). The label sits on a **dark rounded banner/pill plate** (`_draw_pill_plate`, the shared
    pill chrome extracted out of `_draw_count_pill` — the `×N`/`+N` badges draw the same primitive):
    bare drop-shadowed text washed out on the light tan biomes (prairie/desert), so the plate is sized to
    the MEASURED text+glyph run plus symmetric padding (`YIELD_LABEL_PLATE_PAD_FACTOR`, a fraction of the
    font size) and centered on the label's existing anchor, near-black + slightly translucent
    (`YIELD_LABEL_PLATE_BG`) so the terrain still reads through. The
    label font scales with the hex radius (clamped) and the whole annotation (plate included) is
    **LOD-suppressed below
    `ICON_MIN_DETAIL_RADIUS`** (like the secondary markers) so far zoom stays clean. Scout/Warrior
    produce no food → no label. **The labels are DEFERRED to the very end of `_draw`** — they are an
    annotation OVER the map, and drawn inline in the highlight pass they were painted over by every
    later layer (the dashed-amber pending overlays, the band→herd links, the hunted-herd rings, and the
    secondary herd/food glyphs — a deer glyph landing squarely on the number). The highlight pass now
    `_queue_yield_label`s each request into `_deferred_yield_labels` (cleared at the top of
    `draw_band_work_highlights`, before its early-outs) and `BandOverlayRenderer.flush_yield_labels()` renders the batch
    as the LAST draw call, after the markers/rings/links/pending/targeting. The LOD gate stays at the
    QUEUE site (`show_yields`), so a far-zoom label is never queued and deferral can't bypass the
    suppression. Guarded by `map_preview` state `map_band_label_overlap` (a herd parked ON a worked
    forage tile + a pending hunt dashing across the hunted herd's label) and `map_band_yield_farzoom`. **Scouting draws its azure range border** (the scout vantage reach `scout_reveal_radius`, when
    scouts are staffed) — a perimeter outline like the forage/hunt borders, NOT a filled reveal disc:
    the old faint-blue scouted DISC was removed because `scout_reveal_radius` is a scout-vantage /
    sight-range value, not a revealed-area radius, and the client can't reconstruct the true LOS-revealed
    area; the border just marks how far the vantage reach extends. The band's actual sight is still
    visible directly in the fog (a wider Active radius). Snapshot fields `work_range` / `hunt_reach` /
    `scout_reveal_radius` are decoded in `native/src/lib.rs population_to_dict` and flowed onto the
    MapView unit marker in `_rebuild_unit_markers` (alongside `labor_assignments`). **Optimistic pending**
    actions for the selected band draw in a distinct **dashed-amber** style (`_draw_band_pending`, fed by
    `set_labor_pending`) — the pending forage tile, the pending hunted herd (dashed ring-hex + dashed
    band→herd link), and the pending move destination (dashed hex + dashed link) — clearly apart from the
    solid confirmed styles, cleared when the snapshot confirms.
  - **Travel destination** (`BandOverlayRenderer._draw_travel_destination`, drawn for the selected traveling unit —
    band OR expedition — from `draw_band_work_highlights`): when the unit reports `is_traveling`, a
    thin cyan line runs from its tile to the destination hex plus a steady (non-pulsing) cyan target
    reticle on it. The target coords (`travel_target_x` / `travel_target_y`, `uint`, `0,0` and ignored
    unless `is_traveling`) are decoded in `native/src/lib.rs population_to_dict` and flowed onto the
    marker in `_rebuild_unit_markers`. **Wrap-aware:** the target is brought into the band's effective
    column frame via `_wrapped_col_delta`, so the line follows the SHORT (possibly seam-crossing)
    wrapped path the sim actually takes rather than shooting the long way across the map. Only the
    selected unit's destination draws (no clutter). Covered by `marker_field_guard`
    (`travel_target_x`/`travel_target_y`/`is_traveling`) and `map_preview` states `map_travel_band` /
    `map_travel_seam` (seam-crossing) / `map_travel_expedition`.
  - **Band-picker dropdown** (`_build_band_picker`, on BOTH assign controls, above the worker
    stepper so it reads "which band → how many workers"): a `Band:` `OptionButton` listing every
    `_player_bands` cohort by positional name ("Band N", via `HudFormat.band_display_name`; the cohort has
    no label field), item metadata = the band `entity`. The selection is the **actor band**:
    `_hunt_assign_band` / `_forage_assign_band` hold the picked entity (defaulting to
    `_resolve_assign_band()` when the selected source changes, else persisted across re-renders);
    the worker stepper's cap is that band's `_assignable_hunt_workers` / `_assignable_forage_workers`
    (its `idle_workers` + any it already staffs on that source, so re-editing isn't capped below
    current staffing), and the Assign emit + optimistic pending key off the picked band. Switching
    the dropdown re-caps the stepper and re-renders. Always shown (single-item with one band, so the
    actor is explicit). Lists **all** player bands — in-range filtering (Forage `work_range` / Hunt
    `work_range` + leash) is deferred to the multi-band slice (needs hunt-leash reach in the snapshot).
  - **`%HerdAssignControls`** (herd drawer, huntable herds, `_build_herd_assign_controls`): the
    band-picker, then a **distance-aware** "Assign hunters" **compose** control — a `−/+` worker/party
    count (`_hunt_assign_count`) + a **policy picker** (`HudWidgets.build_policy_picker`, `_hunt_assign_policy`,
    default `sustain`). **The two policy axes are separated BY BRANCH, and the sim enforces it:** a
    **local** hunt offers `HUNT_POLICY_OPTIONS` (the four extractive rungs **+ the `Corral` investment
    rung**, gated by `_hunt_policy_gates`), while a hunting **EXPEDITION** offers only the extractive
    `LABOR_HUNT_POLICIES` — a detached party follows the herd and builds no pen, `send_hunt_expedition`
    REJECTS Corral server-side, and the sim exports no `hunt_trip_estimates` row for it, so a Corral
    ETA could only ever be a lie. The
    **local** branch renders `LOCAL_HUNT_POLICY_HINTS` under the picker (the band's real payoffs:
    Sustain → the herd stays healthy AND, on a thriving herd, **builds husbandry toward livestock**;
    Surplus → more food now, pushes settling; Deplete → draws the herd down hard, much more
    food now and a fast decline it will not recover from while it lasts — deliberately not oversold;
    Eradicate → **the last hunt**: the whole standing stock in one haul, the biggest payoff of any rung,
    in whatever the species pays (meat, ⇄ trade goods, or both), no craft learned, and the herd gone for
    good — denial is the END STATE, not a promise that the carcasses were thrown away (#337)). **These are
    NOT the expedition hints** (`SEND_HUNT_POLICY_HINTS`): an expedition's Hunting arm banks **both
    products** since #337 (one `HuntYield::apply` per kill — provisions into the party's larder, trade
    goods onto `Expedition::carried_trade` and into the faction stockpile at the drop-off/fold-back), but
    accrues **no husbandry** (a known v1 gap, tracked server-side) — so the expedition set may state a
    trade payoff, never a craft, and the two sets must stay separate. `LOCAL_HUNT_POLICY_HINTS`
    also owns the **`corral`** hint (Corral is a local-hunt-only rung) — which must carry all three
    halves of that bargain: the ~25-turn half-yield build, the ladder's best payoff, and the fact that
    **penned animals can't graze, so you feed them from your larder every turn and an underfed herd
    shrinks**, and it is the set `_policy_hint`
    spells out on a worked Hunt row's tooltip. **The hint is rendered per BRANCH, never once above
    both** — one shared line under the picker would promise an expedition player the band's payoffs. The
    button + command switch on the **wrap-aware hex distance** from the **SELECTED band's** own tile
    to the herd vs that band's **`hunt_reach`** (= `work_range` + hunt leash, decoded as `hunt_reach`
    and flowed onto the marker): **within reach** → a `Hunters` stepper + **"Assign Local Hunt"** →
    `assign_labor hunt <herd_id> <policy> <workers>`; **beyond reach** → a `Party` stepper (cap
    `min(idle_workers, max_expedition_party_size)`) + a distance hint + **"Send Hunting Expedition"** →
    `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> <policy>` (emitted directly, no
    herd-targeting step — the herd is already selected). Every part of the decision (distance, reach,
    band-entity target) keys off the band the picker selects, explicitly threaded — never the faction's
    default band. **Both branches show a LIVE forecast above the button** (everything — band, count,
    policy, herd — is known at compose time, and the block re-renders on every stepper tick / policy
    click, so it's live, not a confirmation; missing levers/ceilings → no line, panel otherwise
    unchanged): the **expedition** branch renders the SAME raid line as the targeting banner
    (`SourceForecast.hunt_trip_forecast` → `SourceForecast.hunt_forecast_line_bbcode`, shared — the two entry points can't quote
    different numbers) and gives the **button itself** the verdict (`SourceForecast.style_send_hunt_button`).
    **A hunting expedition is a GREEDY RAID** (server `5a130e0`): it grabs the herd's standing surplus
    above the policy floor in a burst and comes home. A party too small to carry a whole animal now
    **kills one and hauls only the fraction its pack holds, wasting the rest** (mirroring the local hunt's
    `quantise_animal_take`), so the headline is the delivered **PAYLOAD** — the animal count over the turns,
    the FOOD the party actually LANDS, and the WASTE below it: **`delivers ≈1 Thunder Mammoth over ≈20
    turns · ~4 food · ⚠ 75% wasted`** (`HUNT_FORECAST_DELIVERS_FORMAT` + `HUNT_FORECAST_TRAVEL_BREAKDOWN` +
    `HUNT_FORECAST_FOOD_FORMAT` + a WARN-amber `HUNT_WASTE_SUFFIX_FORMAT`; `animals` =
    `HuntTripEstimate.animalsTaken` (now a KILL count ≥ 1 whenever there's surplus), **food =
    `HuntTripEstimate.deliveredFood`** — the sim's forward-simulated landed food, NOT `animals ×
    foodPerAnimal`, which counts the whole kill and overstates a partial — and waste % =
    `wastedFood / (deliveredFood + wastedFood)`). A high waste % is **informative, not a block** — the
    button stays enabled. **`turnsToFill` is HUNTING turns only** (server `3bb9731` — travel is NOT in it;
    the per-herd estimate table is band-agnostic). The client adds the **round-trip TRAVEL** itself
    (`SourceForecast.round_trip_travel_turns`, matching the server launch feed EXACTLY: `ceil(2 × wrap-aware
    hex_distance(band, herd) / band_move_tiles_per_turn)`) and headlines the **total** trip length, spelling
    the split out via `HUNT_FORECAST_TRAVEL_BREAKDOWN` when travel > 0. `band_move_tiles_per_turn` (a
    LaborConfig scalar echoed per-cohort) is **now decoded in `native/src/lib.rs` and flowed onto the band
    marker** (`_rebuild_unit_markers`, guarded by `marker_field_guard`), so travel lights up on the live
    wire (it degrades to hunting turns only if a snapshot omits it).
    **WARNED vs BLOCKED — the line that matters:** a **slow** raid (the **TOTAL** trip —
    `hunt_turns + round-trip travel`, NOT hunting-only `turnsToFill` — past `viability_warn_turns`;
    see `SourceForecast.hunt_forecast_line_bbcode`'s `total > warn_turns`, so a distant herd is "slow" on travel
    alone) or a **long** raid (`turnsToFill == 0` — ran the whole horizon still
    delivering) is a real tradeoff, so it is WARN-amber `"armed"` + `Send Anyway (≈54 turns)` /
    `Send Anyway (long raid)` and stays **enabled**. A **denial** mission — a quarry that pays NEITHER
    product (`delivers_food == false` **and** `delivers_trade == false`; never the Eradicate rung, which
    delivers) — likewise stays enabled (`SEND_HUNT_DENIAL_BUTTON`, "Send (brings nothing home)"). The ONE blocked case is **no surplus**
    (`SourceForecast.hunt_trip_no_surplus`: **`deliveredFood == 0`**) — the herd is at/below the policy's floor, so the raid
    would return empty at every party size: a mistake with no upside, so the button is **DISABLED**
    (`Herd too lean to raid`). This is `deliveredFood == 0`, **NOT `animalsTaken == 0`** — a small party on
    big game now delivers a partial (`animalsTaken 1`, high waste), which is NOT too lean; only a genuinely
    at-floor herd blocks. Party size cannot fix it — **surplus is a property of the HERD, not the party** —
    so the reason (`SourceForecast.hunt_no_surplus_reason` → `SEND_HUNT_NO_SURPLUS_REASON`) names **no alternative size**
    (the old row-scan / `_recommended_party` / step-up-impossible machinery is retired). `SourceForecast.hunt_estimate_key`
    is the one definition of the `"<policy>:<workers>"` estimate key, shared by the single-cell lookup and
    the max-useful scan.
    **The party stepper caps at MAX-USEFUL on both branches** (`SourceForecast.expedition_useful_cap`): **`deliveredFood`**
    PLATEAUS with party size once the herd's surplus (not the pack) binds, so extra hunters past the plateau
    raid no more food — a table SCAN for the smallest size at which delivered food stops rising, capped there
    with the SAME "max N useful here — more would be idle" note the local hunt uses (`MAX_USEFUL_NOTE_FORMAT`).
    It scans **`deliveredFood`, not `animalsTaken`** — the whole-animal count sits at a leading 1 across every
    small-party size on big game (the leading-zeros bug that fooled the old scan into capping at 1); with
    partials, delivered food rises smoothly, so the cap tracks the true bind. That closes the silent-idle-
    hunter gap the whole pass exists for.
    **Picking a policy AUTO-FILLS the crew/party to that policy's max-useful cap** (`_hunt_assign_autofill`,
    a one-shot set only by a policy CLICK, consumed on the next rebuild before the clamp — the "give me
    everything this herd sustains" default that guarantees zero waste + the full rate). Both branches;
    the manual `−/+` stepper is untouched (it never sets the flag).
    The **band-panel launch flow gates identically, in its own form**: its compose sheet picks the
    quarry first and then styles its Send with the SAME `SourceForecast.style_send_hunt_button` + `SourceForecast.hunt_no_surplus_reason`,
    so a no-surplus herd disables there too. The quarry PICKER itself (`_try_pick_quarry`) deliberately
    does NOT test surplus — no policy is chosen at that point, so the verdict is unknowable; it only
    nudges "No huntable herd there — click on a herd." so a click is never silently swallowed. The **local** branch has no carry cap, so a raid readout is meaningless and
    it instead previews the crew's honest **carry-aware delivered take, ANIMALS-first**
    (`_local_hunt_preview_bbcode` / `_hunt_delivered_and_waste`). A hunt takes WHOLE animals via a
    kill-credit bank, so the crew's raw food throughput
    (`workers × hunt_per_worker_provisions × output_multiplier`, capped by the band's flow ceiling)
    is quantized to the whole bodies it can HAUL: `delivered = min(ceiling, floor(collection ÷
    food_per_animal) × food_per_animal)`. The line reads `≈<delivered ÷ food_per_animal> <animal>/turn`
    (e.g. `≈1 Red Deer/turn`, 2-dp trailing-zero-stripped via `_format_animal_rate`), income-green
    `· renewable` or WARN-amber `⚠ … — overdraws the herd` when the delivered take exceeds the herd's
    Sustain ceiling (the shared `_is_overdraw` test). When the crew can't carry even one whole animal the
    surplus meat rots → a **separate** WARN-amber `· ⚠ N% wasted` suffix (`waste_pct`, its own flag,
    rendered amber even on a green line; overdraw + waste can co-occur). Because the animal rate is a
    long-run average of lumpy whole-animal delivery, EVERY extractive rung shows a **STABLE, always-on
    averaging-WINDOW disclaimer** under the policy picker — `HUNT_AVG_WINDOW_FORMAT`: `This estimate is a
    long-run average over ~<X> turns — you take whole animals, so per-turn delivery varies.` X =
    `_hunt_avg_window_turns(herd, policy)`, derived from the SELECTED policy's raw flow ceiling (NOT the
    crew's current delivered rate), so it is **worker-independent and never blinks out** as the Hunters
    count steps up: `g = ceiling ÷ food_per_animal`; slow/big game (`g < 1`) → `ceil(1/g)` (deer Sustain →
    ~2, mammoth Sustain → ~7), fast game → `ceil(1/frac)`, clamped to `HUNT_WINDOW_MAX_TURNS` (12). Keyed on
    the composed policy (a faster policy averages over a different span), extractive rungs only (an
    investment rung shows a dip→payoff, not a cadence), skipped when the window is unknown (missing
    food_per_animal / ceiling → returns 0). The resident band applies its
    morale/discontent productivity modifier at payout, an expedition does not; when `food_per_animal` is
    unknown the line degrades to the old smoothed `≈ +X /turn · renewable` food line (unchanged). **The
    two branches read DIFFERENT herd fields**
    (see "Hunting expedition" below): the expedition line is a pure LOOKUP into the sim's
    forward-simulated `hunt_trip_estimates` (`HERD_TRIP_ESTIMATES_KEY`, zero client arithmetic — a
    `carryCap / rate` division is WRONG for Surplus/Deplete), while the local line is carry arithmetic over
    the band's flow ceiling `hunt_policy_ceilings` (`HERD_BAND_CEILINGS_KEY`, via `_hunt_delivered_and_waste`
    / `SourceForecast.hunt_policy_ceiling`; `_hunt_take_rate` still backs the food-line fallback). The ecology/MSY model
    is NEVER re-derived client-side.
    Distance uses Hud-local mirrors of MapView's odd-r `_hex_distance` /
    `_wrapped_col_delta`, fed grid width + wrap via `Hud.set_grid_dimensions` (Main forwards the
    snapshot `grid` key). Compose state re-seeds from current staffing when the selected herd changes.
    Covered by ui_preview states `herd_verbs` (local) / `herd_hunt_expedition` (single far band) /
    `herd_hunt_band_near` + `herd_hunt_band_far` (two bands, one herd — picker flips local↔expedition),
    plus the live-forecast states `herd_hunt_forecast_viable` (Mammoth Sustain: cyan "delivers ≈8 …
    over ≈6 turns" + primary button) / `herd_hunt_forecast_slow` (Red Deer Sustain, 54 turns past the
    warn line → amber "⚠ … — a slow raid" + "Send Anyway (≈54 turns)") / `herd_hunt_forecast_surplus`
    (the SAME Red Deer on Surplus: a deeper floor → more animals, brisk turns) /
    `herd_hunt_forecast_no_surplus` (collapsing Wild Fowl at its floor → animalsTaken 0 → red "too lean
    to raid" + disabled button) / `herd_hunt_forecast_eradicate` (a REAL delivery, not a denial —
    `delivers ≈12 Red Deer over many turns · ~24 food · ⇄ ~6 trade goods — a slow raid` + amber "Send
    Anyway (long raid)"), the RAID + max-useful set `herd_hunt_boar_raid` (the server's measured Wild Boar,
    1 hunter → "delivers ≈5 Wild Boar over ≈7 turns · ~20 food", ascending per-policy compact `≈N` picker
    buttons — glyph + metric, name-in-tooltip) / `herd_hunt_max_useful` (2 hunters → "delivers ≈8 … over ≈8 turns"; a 3rd raids no more, so
    the stepper caps at 2 with "max 2 workers useful here — more would be idle") /
    `herd_hunt_raid_travel` (the SAME boar 8 tiles from a band carrying a move rate → the client adds the
    round trip: "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food", cap still 2) /
    `herd_hunt_no_surplus` (a herd stripped to its floor → 0 animals at every size → disabled "Herd too
    lean to raid") / `herd_hunt_eradicate` (the boar on Eradicate → the whole-stock windfall in both products, ordinary
    Send), and
    `herd_hunt_local_sustain` /
    `herd_hunt_local_overdraw` (local branch, animals-first: green `≈0.14 Red Deer/turn · renewable` vs
    amber `⚠ ≈0.27 Red Deer/turn — overdraws the herd`) /
    **`herd_hunt_local_eradicate`** (the frame the LOCAL Eradicate hint is judged on: the rung's picker face
    reads the ladder's top take `💀 Eradicate / 2.40 food · 0.36 trade`, and the hint below describes the one-haul windfall +
    the permanent end state — never "no food, no trade"), and the carry-aware set
    `herd_hunt_delivered_clean` / `herd_hunt_delivered_waste` / `herd_hunt_automax` /
    `herd_hunt_big_game_window` (see the animals-first preview + "up to X/turn" cap notes above).
  - **`%ForageAssignControls`** (Tile card, food-module tiles, `_build_forage_assign_controls`): the
    band-picker, then a sustain/surplus/deplete/eradicate **policy picker** (`HudWidgets.build_policy_picker`,
    `_forage_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`) — carrying the SAME ascending
    per-policy **COMPACT** button metric the local-hunt picker does. **Each button is TWO LINES, ONE PER
    AXIS — the rung's glyph + NAME over its product line** (`[♻ Sustain / 0.96 food]
    [⬆ Surplus / 1.92 food] [⇊ Deplete / 2.88 food] [💀 Eradicate / 4.80 food] [🌱 Cultivate / → 1.20 food]
    [▦ Sow]`), a hunt rung carrying both products (`[⇊ Deplete / 2.70 food · 0.41 trade]`). **THE ONE-LINE
    `<glyph> <metric>` FACE IT REPLACED WAS AN AXIS COLLISION** (playtest, issue #337 follow-up): the rung
    glyph (`♻ ⬆ ⇊ 💀`) and the trade-goods glyph `⇄` sat adjacent in one line at one weight saying
    different things — *which rung* vs *which product* — and dropping the rung NAME left `⬆` beside `⇊`
    reading as good-vs-bad rather than as neighbouring rungs of one ladder. Naming the rung in text is
    what defuses that, so `POLICY_ICONS` is UNCHANGED; the products move to words
    (`SourceForecast.picker_products`) because trade goods have no tintable pictogram (see
    `sprites-widgets.md`). **Line 2 renders one step SMALLER and one step quieter than line 1**
    (`POLICY_PICKER_METRIC_FONT_SIZE` 13 against the default control size line 1 keeps by carrying NO
    override, and `POLICY_PICKER_METRIC_ALPHA` 0.72): the name leads the glance and the numbers answer
    it — at one size `0.32 food · 0.08 trade` competed with `♻ Sustain` instead of supporting it. **No `+`
    sign on these numbers**: every rung
    is a gain, so a sign carries no information here (it stays on the work rows and map labels, where it
    contrasts against consumption), and the render-only-when-non-zero rule still governs — a wolf rung
    reads `2.70 trade` alone, never `0.00 food · 2.70 trade`.
    **TWO FONT SIZES CANNOT LIVE IN ONE `Button.text`, so a rung is a CELL, not a button**
    (`HudWidgets._policy_rung_cell`): a zero-margin `MarginContainer` holding the `Button` (empty `text`,
    but still the box, the click, the disabled state, the focus and the tooltip) with the two-Label stack
    painted over it, inset by `POLICY_PICKER_PADDING_V` 4 / `POLICY_PICKER_PADDING_H` 6 and every overlay
    control `MOUSE_FILTER_IGNORE` so the click reaches the button beneath. The MarginContainer is what
    SIZES the cell — a `Button` is not a Container and would not grow to fit children, which is exactly why
    it cannot be the parent — and it is the CELL that carries `SIZE_EXPAND_FILL` into the grid now.
    **THE TINT IS ONE COLOUR, DERIVED ONCE, and that invariant is the whole reason the single-`Button.text`
    face existed:** `HudStyle.button_font_color(variant, disabled, selected)` is asked ONCE **by the picker
    loop**, handed to `_policy_rung_cell` as a parameter, and line 2 is that same
    colour at `POLICY_PICKER_METRIC_ALPHA`, so a selected, disabled, standing-but-gated — or any future
    warned — rung moves BOTH
    lines by construction (the greyed `🐄 Corral` in `hunt_picker_ascending` is the frame). Never give line 2
    a colour of its own. The cell takes the tint rather than re-deriving it from the variant because the
    disabled tint now depends on whether the rung is the SELECTED one, which the cell does not know. `modulate` was the other candidate and is worse here: it inherits to children but
    multiplies the BOX too, so a disabled rung would be dimmed twice, once by the disabled stylebox's own
    faded fill. That the theme's `font_color` reaches a Button's `text` and nothing else is why
    `button_font_color` was split out of `apply_button` (which now feeds from it) — a hand-built face and a
    themed one read one table.
    **The rung Button carries its policy as `HudWidgets.POLICY_RUNG_META`**, the one stable handle on a
    rung: `band_panel_preview._picker_rung_buttons` finds them by that meta and recurses past the cells,
    because a face match on `btn.text` would now find an empty string and pass every assertion vacuously.
    **The picker is a `GridContainer` of AT MOST `POLICY_PICKER_COLUMNS` (3)**, and 3 is a CEILING, not a
    target: `grid.columns = clamp(explicit columns or options.size(), 1, 3)`. Six rungs read **3 + 3**, the
    four extractive rungs read **3 + 1** with Eradicate alone on the second row, and a caller passing an
    explicit `columns` is clamped DOWN, never up. Four abreast shipped first and read wrong: it made the
    expedition launch picker a different creature from the local hunt beside it, and it set the widest
    compose card's width off a row that never needed to be that wide (the deer picker's minimum width fell
    **714 → 444px**, the wolf's 396 → 296, and the forage sheet 554 → **546** of its 560 cap — the wrap the
    forage picker already had got SHORTER, since each row costs one 13px line instead of a 16px one). The
    lone rung is **not** stretched across the row: a GridContainer gives it its COLUMN's width, so it sits
    under the first cell above at exactly that cell's width, which reads deliberate rather than orphaned.
    **`ZONE_POLICY_PICKER_COLUMNS` (2) is the one picker that does not follow** — see
    `band-city-panel.md`; measured at 3 it overruns the L/R dock's ~354px zone by ~90px and the frame comes
    back sliced. Each `*_policy_takes` helper emits a **`{compact, full}` pair** per policy: the
    compact string is the face's SECOND LINE, the verbose full string moves to the tooltip. Extractive rungs →
    compact `0.96 food` (`SourceForecast.picker_products(ceiling, trade)`, fed by `_forage_policy_takes` off `SourceForecast.forecast_inputs`),
    full `up to +0.96/turn` (`POLICY_CAP_FORMAT` — the tooltip keeps the sign and the unit, being the one
    place that says "up to"). INVESTMENT rungs on BOTH pickers → compact `→ 1.48 food · 0.37 trade`
    (`POLICY_PAYOFF_COMPACT` over the SAME `SourceForecast.picker_products` the extractive rungs use, so
    the payoff obeys the render-only-when-non-zero rule too: a boar's Tame names both products, a
    pure-meat species' names food alone, a plant rung is always food-only; the arrow is what keeps it
    from reading as a take today), full `builds toward +1.48/turn · ⇄ +0.37 trade goods/turn`
    (`POLICY_PAYOFF_FULL_FORMAT` joined to the shared `POLICY_CAP_TRADE_FORMAT` clause by
    `TRADE_COMPONENT_SEPARATOR`, the same shape `extractive_take_pair` builds) — the
    `tended_yield`/`field_yield` (forage) or `pastoral_yield`+`pastoral_trade` /
    `corral_yield`+`corral_trade` (hunt) they build toward, NOT
    the prep dip, which reads below Sustain and was identical for both hunt rungs (quoting it made
    taming/penning look worse than hunting); a locked rung may still show its payoff, the gate-reason line
    (under the picker) explains the lock. **The tooltip carries the VERBOSE metric the face compacts** —
    every button's `tooltip_text` leads with `<Name> — <full metric>` (`POLICY_TOOLTIP_NAME_FORMAT`, e.g.
    `Sustain — up to +0.96/turn`, `Tame — builds toward +1.20/turn`), and a gated button appends its gate
    reasons below that (so a hover tells you what the rung costs to unlock as well as what it pays). A rung
    with **no** metric (the work inspector's picker, which passes no `takes`; a metric-less gated rung) is
    **line 1 alone** — glyph + name — so a button is never a lone glyph and never a lone number. The three
    pickers — forage / local hunt / expedition — wear an **identical** face: `<glyph> <Name>` over
    `X food[ · Y trade]` (extractive, `up to X/turn` in the tooltip via `POLICY_CAP_FORMAT` /
    `SourceForecast.extractive_take`) or over `→ X food[ · Y trade]` (investment, Cultivate/Sow AND
    Tame/Corral — the trade half only ever appears on the two HERD rungs, the plant web projecting no
    trade rate). **The expedition picker no longer shows raid animals** (`≈N` / `EXPEDITION_TAKE_COMPACT`
    is retired) — `SourceForecast.expedition_policy_takes` now emits each policy's **MAX obtainable food/turn**, the max
    over party sizes of `deliveredFood / trip_turns` (`trip_turns = turnsToFill + round-trip travel`), so it
    is **worker-independent** (never blinks as the Party stepper steps) and the four read ASCENDING Sustain <
    Surplus < Deplete. **Eradicate DELIVERS like every other rung** (#337) and carries its own rate; a rung
    falls back to its name + skull glyph only when its cells land nothing at all (an inedible quarry, or a
    `trip_turns` of 0). **Picking a policy AUTO-FILLS the
    foragers to that policy's max-useful cap** (`_forage_assign_autofill`, the forage twin of
    `_hunt_assign_autofill` — a one-shot set only by a policy CLICK, consumed on the next rebuild before the
    clamp; the manual `−/+` stepper never sets it). It carries a
    **forage-appropriate**
    behaviour hint (`FORAGE_POLICY_HINTS` — "gather at the patch's regrowth" etc., NOT the herd-cull
    hints), an "Assign foragers" Foragers `−/+` count (`_forage_assign_count`), and a
    **range-aware** **Forage** button → `assign_labor forage <x> <y> <policy> <workers>` (the policy is
    the optional token the sim accepts before the worker count; the policy persists across re-renders
    and re-seeds from the tile's current forage policy via `_policy_for_forage` when the tile changes).
    Mirrors `%HerdAssignControls`' policy affordance. Foraging is
    **stationary** gathering — there is **no forage-expedition fallback** — so the button gates on the
    **wrap-aware hex distance** from the **SELECTED band's** own tile to the forage tile vs that band's
    **`work_range`** (the plain `workRange` field, NOT `hunt_reach`; already decoded/on the marker):
    **within range** → enabled **Forage**; **beyond range** → the button is **disabled** + an
    out-of-range hint (`"(x,y) is N tiles away — beyond this band's forage range (R)"`), no alternative.
    Reuses the same `_hex_distance_wrapped` / `SourceForecast.band_tile` / grid-dim plumbing and explicit
    selected-band threading as the herd hunt. Covered by ui_preview states `food_tile` (in range) /
    `food_forage_out_of_range` (single far band) / `food_forage_band_near` + `food_forage_band_far`
    (two bands, one tile — picker flips enabled↔disabled).

  - **Cultivate / Sow / Tame / Corral — the FOUR INVESTMENT rungs** (on BOTH assign controls; the
    sim's `FollowPolicy::Cultivate` / `Sow` / `Tame` / `Corral`, and `INVESTMENT_POLICIES` names the
    set). The extractive four take from a wild source; these pay an **up-front cost** — while the
    source is being prepared it yields only its dip ceiling, then steps up a rung. Each ladder runs a
    verb **twice**, one per rung-transition (`docs/plan_intensification_ladder.md` §2):
    *plants:* wild --`cultivate`--> **Tended Patch** --`sow`--> **Field**;
    *animals:* wild --`tame`--> **Pastoral herd** --`corral`--> **Pen**.
    **Kind-specific and the sim rejects the cross pairing**: Cultivate + Sow are forage-only
    (`FORAGE_POLICY_OPTIONS`), Tame + Corral hunt-only (`HUNT_POLICY_OPTIONS`) — and both hunt rungs
    are offered on a **local hunt only** (a detached party follows the herd and builds nothing, so the
    expedition keeps the extractive `LABOR_HUNT_POLICIES`, as does the send-expedition launch picker).
    - **These are POLICIES, not standalone commands.** They ride the existing
      `assign_labor … <policy> <workers>` path, exactly as Cultivate/Corral always have — so `Tame`
      and `Sow` needed **zero** new command wiring. The server *also* exposes convenience verbs
      (`tame <faction> <herd_id>` / `sow <faction> <x> <y>`, which switch the policy on bands already
      working the source), but the client does not use them: the picker composes band + workers +
      policy in one act, and routing through a second verb would fork the emit path.
    - **The husbandry CEILING hides a rung outright; knowledge only greys it.** Both hunt rungs are
      filtered against `HerdTelemetryState.husbandryCeiling` (Grazing 2d-δ): Corral needs `"pen"`,
      Tame needs anything above `"wild"` (and retires once `domestication >= 1` — its meter is full
      and Corral is what's next). Hidden, never greyed, because no amount of knowledge or work will
      ever let you pen a `"pastoral"`-ceiling species — greying it would imply a reachable
      prerequisite. Knowledge = "I know how"; ceiling = "this animal allows it" (§4.2, decoupled).
      The **one** exception is a rung the band is already standing on, which is re-admitted so it can
      be seen and cleared — see the sheet-renders-the-standing-rung invariant below.
    - **Disabled-with-reason-AND-remedy, never hidden.** `HudWidgets.build_policy_picker(on_pick, selected,
      options, gates)` renders a gated option **greyed, with every reason in the tooltip (one per
      line) AND spelled out under the row**, so the player discovers the rung and its prerequisites
      *before* acting. `gates` maps **policy → `Array[String]` of reasons** (read only through
      `HudWidgets.gate_reasons`); **1 reason** renders the compact one-liner `🌱 Cultivate — <reason>`, **2+**
      render a `🐄 Corral needs:` header + one indented `· <reason>` bullet each (a reason now carries
      its remedy, so two on one line would not fit).
      **Each reason states what's missing + live progress + the action that fixes it** — naming the
      prerequisite alone told the player a door was locked without saying where the key is. **A reason
      is one of exactly two kinds, and the split is the point** (see the two-meter split above): a
      KNOWLEDGE reason is fixed by **practice** and names the ♻ Sustain glyph (pulled from
      `FoodIcons.POLICY_ICONS`, i.e. literally the button beside it) — `Your people know Penning 45%
      — ♻ Sustain-hunt a tamed herd to learn it`; a SOURCE reason is fixed by that rung's **verb** —
      `This herd is 40% tamed — ◎ Tame it to finish`.
      **THE GATE RESHUFFLE (§4.3) — one knowledge per transition, and the client encodes it in
      `_hunt_policy_gates` / `_forage_policy_gates`** (mirroring the sim's `assign_labor` validation):
      * `Cultivate` ← `cultivation >= 1` **and** a Thriving patch **and NOT already `is_cultivated`** —
        a finished patch retires Cultivate outright (`GATE_REASON_ALREADY_TENDED_FORMAT`, "Already a
        Tended Patch — ♻ Sustain-forage it to harvest"), because re-running the verb only pays the low
        prep dip forever. The completed reason SUPERSEDES the prep prerequisites (a done patch's
        Thriving/knowledge gates are moot). On a done patch the band has no Cultivate assignment (the
        sim retires a completed rung onto Sustain), so a composed Cultivate there is *stale* and falls
        back to Sustain: the "Preparing → then" prep line disappears and the forecast reads the Sustain
        harvest. A band that IS still standing on it renders it selected + gated — see the
        sheet-renders-the-standing-rung invariant below.
      * `Sow` ← `seed_selection >= 1` **and** the ground will take seed (see the Sow site gate below)
        **and NOT already `patch_is_field`** — a finished Field retires Sow the same way
        (`GATE_REASON_ALREADY_FIELD_FORMAT`). Deliberately **no** Thriving gate: sown ground starts at
        the reseed floor (i.e. Collapsing), so a health gate would forbid the very case the rung exists for.
      * `Tame` ← `herding >= 1`. **Herding gates Tame ALONE now** — it no longer gates Corral.
      * `Corral` ← **`penning >= 1`** (the new rung-3 knowledge) **and** `domestication >= 1`.
      Two more remedies are the *opposite* of "work harder", because their conditions are stocks, not
      policies: the **patch-ecology** gate (a fully staffed Sustain takes the whole regrowth and holds
      a Stressed patch Stressed forever) reads `Patch is Stressed — ease workers off and let it regrow
      to Thriving`; and `_tame_stalled_hint` (below) says the same of a stalled tame. The gates are
      re-validated every render, since a source can leave Thriving under a standing selection.
      **Known gap (pre-existing):** `_hunt_policy_gates` does NOT check herd
      **ownership** — the tracks are per-faction, so a herd tamed by ANOTHER faction reads as
      available client-side while the sim rejects the assign.
    - **THE COMPOSE SHEET NEVER RENDERS A POLICY THE BAND IS NOT ON** (issue #420, both compose
      builders). A gated rung is normally not a legal composition and is reset to Sustain — but **a
      gated rung that IS the band's standing assignment renders SELECTED AND GATED, with its remedy**,
      so clearing it is something the player *does* rather than something that happens to them. The
      reset therefore fires only when the composed policy is gated/structurally invalid **and** differs
      from the standing one. The standing rung is `policy_for_forage` / `policy_for_hunt` read **only
      when `workers_for_forage` / `workers_for_hunt > 0`**: those helpers answer with the DEFAULT for an
      unstaffed source, so calling them blind would make every fresh sheet look as though the band were
      standing on Sustain and the reset would never fire on a genuinely stale composition.
      The case this exists for is the one the sim deliberately leaves alone — a patch that drops out of
      Thriving mid-build **keeps** its Cultivate assignment and merely pauses accrual
      (`.claude/rules/core_sim/cultivation.md`, "neither lost nor silently switched"). Forcing the
      default there claimed the band had been switched, and every number under it — the rung faces,
      `max_useful_workers`, the stepper cap — was computed for the *displayed* rung while the sim
      worked the source under the real one (the reported `Now 2` over a stepper capped at `1`).
      **On the hunt side the standing rung is also RE-ADMITTED to the option set when the
      husbandry-ceiling pass filtered it out** — the fully-tamed `Tame` case, which the ceiling pass
      retires outright. Re-admission is for the standing rung ALONE and never general: the
      "an out-of-ceiling rung is HIDDEN, never greyed" rule holds for every other case, and it cannot
      widen the picker past what the sim allows, since the sim refuses to put a band on a rung its
      species' ceiling forbids. A re-admitted `Tame` carries its own gate reason,
      `GATE_REASON_ALREADY_TAMED_FORMAT` ("Already fully tamed — ♻ Sustain-hunt it to harvest"), in the
      register of the plant pair; the reason is computed whether or not the rung is offered, so
      visibility stays the option set's business and the gate stays the gate's.
      **`HudWidgets.build_policy_picker` renders selected-and-gated as a distinct state** — the two
      used to be mutually exclusive by construction, and the plain disabled treatment fades the border
      to `LINE_SOFT` and the text to `INK_FAINT`, erasing the only mark of which rung is current and
      leaving a picker that looks unselected. `HudStyle.apply_button`'s `selected_when_disabled` (and
      `button_font_color`'s matching `selected`) keeps the variant's own border and text HUE at
      `BUTTON_SELECTED_DISABLED_TEXT_ALPHA`, so the rung reads "you are doing this, and it is a dead
      end". The face's two lines still take ONE tint, now passed INTO `_policy_rung_cell` as a
      parameter rather than re-derived there, because the disabled tint depends on selectedness the
      cell does not know. ui_preview `forage_cultivate_paused` (the primary frame) /
      `forage_cultivate_done_standing` / `herd_tame_done_standing` (the re-admission) /
      `herd_tame_done_other_rung` (the blast-radius guard: a band standing on Sustain sees no Tame).
      **A SIM-SIDE GAP RIDES WITH IT:** `validate_labor_policy` rejects a *new* Cultivate assign on a
      non-Thriving patch, and re-staffing a paused build re-issues exactly that command, so the crew
      count cannot be changed while the pause lasts (unassign is exempt — `workers == 0` skips
      validation, so the remedy the sheet offers is always reachable).
    - **A RETAINED DRAWER-ACTIONS CLOSURE NEVER CAPTURES ITS SUBJECT — it re-resolves through
      `_live_herd` / `_live_tile_info` at press time.** The two drawer-action builders diff against a
      shape signature (`_herd_actions_shape` / the forage twin) and PATCH a same-shape restate in
      place, deliberately keeping the compose-open button's `pressed` connection intact so a
      per-snapshot restate does not tear the drawer down. Those signatures carry the subject's
      **IDENTITY and nothing else** — the herd id / tile key leads, plus the structural slots — and
      that is the design, not an omission: `domestication`, `herders_needed`,
      `herders_needed_if_managed`, biomass and the patch's `patch_*` forecast fields move every turn
      without changing the drawer's structure, so folding them in would rebuild on every tick and
      restore the reflow flash the patch path exists to remove. A closure holding a captured dict is
      therefore frozen at whatever turn the drawer was last fully rebuilt, and the sheet opens against
      it: the turn taming started, the drawer read `Domesticating 4%` / `Herders: 3 / 4 —
      under-herded` while the sheet beside it still said `This herd is 0% tamed` and `max 3 workers
      useful here`, healing only on the NEXT snapshot when `refresh_compose_sheet` rebuilt against
      `_selection.herd()`. `_live_herd(herd_id, fallback)` / `_live_tile_info(subject_key, fallback)`
      read the selection model at CALL time and fall back on an id/key mismatch (so a harness staging
      a subject without touching `_selection` works off its own fixture), which makes the open path
      and the per-snapshot refresh path read the SAME dict by construction. **Every** closure on both
      builders routes through them — the compose-open buttons and the seven band-picker / stepper /
      policy-picker / crop-picker rebuild calls inside `_build_herd_assign_controls` /
      `_build_forage_assign_controls`, since a sheet opened from a stale button would otherwise re-pin
      the stale dict on every stepper tick. The subject-key-leads rule in both signatures is
      orthogonal and stays: it handles a SUBJECT change, this handles same-subject/stale-DATA.
      ui_preview `herd_compose_reopen_fresh` (one herd id across two turns, the restate asserted to
      take the patch branch), beside the subject-change pair
      `herd_assign_button_targets_selected_herd` / `forage_assign_button_targets_selected_tile`.
    - **A herd is MANAGED — its crew are Herders, not Hunters — from the moment the sim asks for
      keepers.** `SourceForecast.is_managed_hunt_source` reads `corralled` **or** full
      `domestication` **or `herders_needed > 0`** or a composed Corral. The third clause is the sim's
      own statement that this herd owes a crew: `herders_needed` is ownership-gated
      (`fauna::herd_herders_needed`), so it goes positive the moment the herd becomes owned — part-way
      through taming, well before the meter completes — and it is the SAME field and the SAME `> 0`
      test the drawer's `Herders: A / N` row gates on, so the sheet's stepper/title and the drawer row
      can no longer disagree. Composing **`tame` is deliberately NOT a clause**: a still-wild herd
      being tamed reports `herders_needed == 0`, is not yet owned, and its crew genuinely hunts at a
      reduced take that turn, so "Hunters" is honest there; Corral is a clause because it builds the
      pen the keepers hold. ui_preview `herd_compose_reopen_fresh` asserts the noun flips on the
      in-place-patched button, and `herd_fully_herded` / `herd_under_herded` render the drawer form.
    - **`_tame_stalled_hint` — the one silent rule, said out loud.** Taming accrues only while the
      herd is **Thriving**, but that is deliberately NOT a gate: a herd's phase swings as it is
      hunted, so refusing the verb would be un-actionable churn. The sim just **pauses** the meter
      (progress is neither lost nor switched). Silence would recreate exactly the hidden-rule problem
      this arc exists to kill, so whenever `Tame` is composed on a non-Thriving herd the drawer states
      the pause, its live phase, that progress is safe, and the ease-off remedy (WARN amber).
      ui_preview `herd_tame_stalled`.
    - **The Sow SITE gate — the refusal is an ANSWER, not a bool.** Only ~**46 of 4160** tiles (1.1%)
      will take seed, so "why can't I sow here?" is *the* question rung 3 provokes — and the client
      **cannot re-derive** it (no per-biome capacity table, no hydrology). The sim ships the verdict
      as a stable key on `ForagePatchState.sowSiteRefusal` (`""` / `too_poor` / `too_dry` /
      `too_poor_and_too_dry`), resolved through the same `RungSiteRequirement::refusal` seam the `sow`
      command gates on, and `_sow_site_refusal_reason` maps it to `SOW_REFUSAL_REASONS` — each naming
      the fault AND pointing at rung 4 (Worked Land — irrigation/the plough), in the manual's voice.
      An **unknown key still refuses** (fail closed: the sim gates the command regardless, so a button
      offered here would only fail unreadably). This is the only gate reason on either ladder a player
      answers by **moving** rather than by working. ui_preview `forage_sow_too_dry` /
      `forage_sow_too_poor`.
    - **The forecast states the deal.** `SourceForecast.forecast_inputs` maps an investment policy's ceiling to the
      DIP yield and additionally returns its `payoff`; `_forecast_yield_row` (now INVESTMENT-only) then
      reads **`Preparing: +0.24 /turn → then +1.20 /turn`** — the deal, not a single rate — both halves
      scaled by the band's `output_multiplier` like every other forecast. The managed source reports
      per-worker == ceiling, so the stepper caps at **1 worker**, as it should.
      **Corral's payoff is GROSS** (`corralYield` does NOT deduct the pen's feed), so its row never
      shows the payoff bare (`FORECAST_FEED_KEYS`, the rungs with a running cost — Corral only; a
      tended patch has none): `Preparing: +0.75 /turn → then +5.40 /turn − 1.74 feed`. `penUpkeep` is
      **one field with one meaning on both sides of the decision** — the feed this pen demands, *or
      would demand once built*, at the herd's current biomass, on the SAME basis `corralYield` uses —
      so the subtraction is a pure difference of two numbers the sim exported for THIS herd and the
      client models no ecology. (It is **demanded**, not paid: the *paid* figure is the cohort's
      `penFeedUpkeep`, and `penFedFraction` is their ratio. Don't cross the wires.)
      **A ZERO PAYOFF IS DATA — it must never be suppressed.** The pen harvests by constant
      escapement, so a herd at/below `K/2` honestly pays **+0.00** until it rebuilds: penning it would
      eat feed forever and pay nothing. The row renders both zeros in full and **emphasizes** them —
      WARN-amber plus `⚠ Too depleted to pen — it would eat feed and pay nothing until the herd
      rebuilds` (`INVESTMENT_FORECAST_DEPLETED_NOTE`) — rather than blanking the 0 as "no data". A
      player who pens a depleted herd because the UI declined to show them a zero has been actively
      misled. ui_preview `herd_corral_depleted`.
    - **TAME's dip — like EVERY herd ceiling — rides the list; its PAYOFF is a scalar.** A herd's only
      wire representation of a per-policy ceiling is the `huntPolicyCeilings` LIST, so no herd rung has a
      `FORECAST_CEILING_KEYS` entry (that dict is now the FORAGE PATCH's ceiling map and only that);
      `SourceForecast.forecast_inputs(src, kind, prefix, policy)` branches on the **caller-stated `kind`**
      (`SOURCE_KIND_HERD` / `SOURCE_KIND_FORAGE`) and resolves every herd policy —
      Sustain/Surplus/Deplete/Eradicate, Tame, Corral — through `SourceForecast.hunt_policy_ceiling`, falling back to the
      list's Sustain row for an unrecognized policy. **It must NEVER branch on the prefix**: a herd dict
      and a raw wire forage-patch dict both carry the forecast fields bare, so they share the ONE
      `BARE_FORECAST_PREFIX` and a prefix test cannot separate them. That is not merely a convention —
      the bare case was deliberately collapsed from two same-valued consts (`HERD_FORECAST_PREFIX` /
      `WIRE_FORAGE_PATCH_PREFIX`, both `""`) into one **because** a herd-sounding name for the empty
      string invited exactly that test: `prefix == HERD_…` read as discriminating, was not, and sent the
      Current-actions Forage row down the herd branch, where no `hunt_policy_ceilings` key exists —
      collapsing its ceiling to 0 and leaving the row's `+` button permanently dead. Nor may it infer the
      kind from the dict's shape (`has("hunt_policy_ceilings")` would misread a herd whose snapshot
      omitted the list). The `prefix` parameter survives for the FORAGE scalar key lookup only. The PAYOFF, by contrast, IS a real scalar: `HerdTelemetryState.pastoralYield` (the
      pastoral MSY once tamed, the twin of `corralYield`), decoded as `pastoral_yield` and mapped in
      `FORECAST_PAYOFF_KEYS` → so Tame is a full investment rung (`forecast["investment"] == true`) and
      renders the SAME dip→payoff row as Cultivate/Sow/Corral: `Preparing: +<dip> → then +<pastoralYield>`
      (no feed term — Tame has no running cost). `INVESTMENT_POLICIES` still names the set (an investment
      rung must never fall through to the extractive `renewable / ⚠ overdraws` preview), and both hunt
      investment rungs' picker buttons wear the `→ +Y/turn` PAYOFF (Tame `→ pastoralYield`, Corral
      `→ corralYield`) via `_hunt_policy_takes` — NOT the during-building dip, which reads below Sustain
      and was identical for both, making taming/penning look worse than hunting. The payoff shows even on
      a gated/greyed rung (the gate-reason line explains the lock). ui_preview `herd_tame` /
      `two_meter_split` (gated Corral still quotes its payoff).
    - **Progress meters — one row per rung, never merged.** Tile card: `Cultivation N%` → `🌾 Tended
      Patch`, joined by its own **`Field`** row — `Sowing N%` → the SIGNAL-tinted **`▦ Field`**
      (`patch_field_progress` / `patch_is_field`, `_field_label` / `_field_value_hex`). Herd drawer:
      `Husbandry: Domesticating N%` → `🐄 Domesticated`, joined by `Corral: Building N%` → `🐄
      Corralled`. **A patch carries BOTH plant meters at once** (a Field may stand on ground that was
      never tended — seed travels, so `Sow` needs no prior patch), so they are two independent rows.
      A completed **Field** deliberately reads as a *different thing* from a Tended Patch — different
      word, different glyph — not as a bigger percentage; that IS rung 3's readout test.
      `Sowing`/`Building`/`Fencing` share one build-verb convention.
    - **Knowledge-unlock nudge.** `_ingest_intensification` keeps the per-faction tracks (all four,
      driven off `KNOWLEDGE_TRACK_LABELS` — adding a rung's knowledge is a label entry + a decoder
      field, never an edit there) and fires a ONE-SHOT `KNOWLEDGE_UNLOCK_NOTES` command-feed note the
      turn a track crosses to complete. Only a real `<1 → >=1` transition fires it (a track already
      complete on first snapshot / a rehydrated save is silent), player faction only, keyed per
      faction+track.
    - **Wire fields decoded in `native/src/lib.rs`** (snapshot + delta share `herds_to_array` /
      `forage_patches_to_array`). **This decoder has now FOUR times silently dropped appended fields
      — check it FIRST when a new field "arrives as zero".** `ForagePatchState`: `ceilingCultivate` /
      `tendedYield` → `patch_ceiling_cultivate` / `patch_tended_yield`, and the five slice-6a fields
      `fieldProgress` / `isField` / `ceilingSow` / `fieldYield` / `sowSiteRefusal` →
      `patch_field_progress` / `patch_is_field` / `patch_ceiling_sow` / `patch_field_yield` /
      `patch_sow_site_refusal` (MapView cross-refs all onto `tile_info` with the `patch_` prefix; ALL
      are in `FOW_DISCOVERED_HIDDEN_KEYS`, mirroring their rung-2 twins). `HerdTelemetryState`:
      `corralYield` / `corralProgress` / `domestication` / `huntPolicyCeilings`
      (the herd's SOLE ceiling representation — the sim exports one row per
      `FollowPolicy::HUNT_POLICIES`, i.e. the four extractive rungs **plus `tame` and `corral`**, so
      the investment DIPS ride it too; the old per-policy scalars `ceilingSustain` / `ceilingSurplus` /
      `ceilingDeplete` / `ceilingEradicate` / `ceilingCorral` are retired `(deprecated)` schema slots and
      are no longer decoded) +
      **`bodyMass` → `body_mass`** (a real appended field, the 4th drop; BIOMASS, surfaced for
      completeness — it **cannot** drive the rhythm, see below) and **`foodPerAnimal` →
      `food_per_animal`** (slot 72, the food-unit quantity the rhythm actually divides by) and
      **`pastoralYield` → `pastoral_yield`** (Tame's payoff, the pastoral twin of `corralYield`) and
      the trade halves of both payoffs, **`pastoralTrade` → `pastoral_trade`** / **`corralTrade` →
      `corral_trade`** (the newest slots, appended after `tradePerAnimal`; each is read as ONE pair
      with its food sibling, so a prepared herd's rung face names both products) → bare keys
      on the herd dict. `LaborAssignment`: `actualYield` / `sustainableYield` / `workersNeeded` +
      **`wastedYield` → `wasted_yield`** (the understaffing signal, also dropped) +
      **`overdraws` → `overdraws`** (the sim-answered overhunting ⚠ for the confirmed rows/map labels,
      policy-driven `!managed && policy.overdraws()`) → per-assignment keys
      inside `labor_assignments`. `IntensificationKnowledgeState`: `cultivation` / `herding` +
      slice-4's `seedSelection` / `penning` → `seed_selection` / `penning` (present — the "Penning 0%"
      playtest report was NOT a decoder drop; see the kill-rhythm/knowledge notes below).
    - **The hunt row headlines the honest RATE, never the kill-credit PULSE** (`SourceForecast.source_yield_readout`,
      slice 8b UX + the local-hunt UX cleanup): a Current-actions Hunt SUMMARY row + the local-hunt preview
      show `sustainable_yield` (the smoothed per-turn take), not `actual_yield` (0 on a wait turn, a spike on
      a kill turn — the "+0.00 /turn" lie). **The summary row is now JUST the food rate + glyphs** — it reads
      `Hunt <species> +X /turn ♻ ●` (food rate, policy glyph, status glyph). The **animals-per-turn cadence
      (`≈<rate> <animal>/turn`) belongs to the COMPOSE-PREVIEW line only** (`_local_hunt_preview_bbcode` /
      `_format_animal_rate` — `sustainable_yield ÷ food_per_animal`, up to 2 dp, trailing zeros stripped;
      fast game `≈1.3 Marsh Fowl/turn`, big game `≈0.15 Woolly Mammoth/turn`): on a summary row the food rate
      is enough, so the cadence suffix was dropped there (the old `_hunt_row_animal_rate` / `HUNT_RHYTHM_SEPARATOR`
      helpers are gone). The **old fast/slow flip** (`_hunt_kill_rhythm`'s `≈1 Mammoth / N turns` slow form)
      had already been retired — its jarring format switch confused the reading. **The preview cadence divides
      FOOD by FOOD** — the rate (`sustainable_yield`, provisions) by **`food_per_animal`**
      (`HerdTelemetryState.foodPerAnimal`, slot 72 = `body_mass × provisions_per_biomass` = the sim's
      `SourceYieldForecast::body_mass_yield`, one animal's worth of yield in provisions). It must **NOT**
      divide by `body_mass` (BIOMASS): with `provisions_per_biomass 0.02` that reads ~50× too long. A herd
      whose `foodPerAnimal` is 0/unknown → no cadence drawn (the honest rate still shows). The **hunt policy
      picker** (`HudWidgets.build_policy_picker(…, takes)`, fed
      `_hunt_policy_takes` off `huntPolicyCeilings`) shows each rung's **CAP** as the product line on the
      button face's second row (`2.70 food · 0.41 trade`; full `up to X/turn` — `POLICY_CAP_FORMAT` — in the tooltip; the shared const also
      used by the forage picker — the source's worker-independent ceiling, FOOD units, distinct from the
      crew's carry-aware per-turn preview line below the picker) so Sustain < Surplus < Deplete < Eradicate
      reads as ASCENDING. `wasted_yield > 0` renders a muted "· N.N wasted" understaffing note (the low-key
      mirror of the WARN overstaff note). A MANAGED
      (corralled/pastoral, owed a herder crew, or composing-Corral) herd's local crew are **Herders**,
      not Hunters (`SourceForecast.is_managed_hunt_source` → the stepper + "Assign …" title noun; the
      full clause list and why `tame` is excluded are above), since `workersNeeded` there is
      the herding crew (max herders, haulers), not a hunt party. The in-progress Cultivation tile-card
      row leads with the **"Preparing N%"** build-verb, matching the herd's "Domesticating N%".
    - ui_preview (slice-8b UX + the local-hunt cleanup): `hunt_actions_rhythm` (two Current-actions Hunt
      SUMMARY rows — each `Hunt <species> +X /turn ♻ ●` with NO `≈… /turn` animals-per-turn cadence; the
      big-game row also keeps the muted `· 1.90 wasted` under-crewed note) / `hunt_picker_ascending` (the local picker + the preview's per-crew line,
      "Hunters" stepper on a wild herd) / **`herd_hunt_delivered_clean`** (2 hunters → `≈1 Red Deer/turn ·
      renewable` + the four ascending `up to +2.33/+3.50/+5.00/+7.00 /turn` cap buttons) /
      **`herd_hunt_delivered_waste`** (1 hunter can't carry one whole deer → green `≈0.65 Red Deer/turn ·
      renewable` + amber `· ⚠ 35% wasted`) / **`herd_hunt_automax`** (a policy click auto-fills the crew to
      the max-useful cap — count sits at 4) / **`herd_hunt_big_game_window`** (mammoth: auto-max staffs the
      20 carriers, `≈0.15 Woolly Mammoth/turn` + the averaging-window disclaimer `This estimate is a
      long-run average over ~7 turns — you take whole animals, so per-turn delivery varies.`; the deer
      `delivered_*` states carry the same disclaimer reading ~2 turns at every worker count) /
      `herd_hunt_local_sustain` +
      `herd_hunt_local_overdraw` (green vs amber `⚠ … — overdraws the herd`) / `hunt_crew_herders`
      (a corralled herd → "Herders" stepper + "Assign herders") / `knowledge_penning_climbing`
      (Penning 34% climbing in the top strip) / `food_tile` (the "Cultivation Preparing 60%" row).
    - ui_preview: `forage_cultivate` (enabled + the Preparing→then forecast + the feed nudge) /
      `forage_cultivate_locked` (1 reason — knowledge + its Sustain-forage remedy) /
      `forage_cultivate_stressed` (1 reason — the ease-off-and-regrow ecology remedy) / `herd_corral`
      (enabled + `Corral: Building 40%`) / `herd_corral_locked` (1 reason — the herd 40% tamed +
      **`◎ Tame it to finish`**, the copy fix: it used to say "♻ Sustain-hunt this Thriving herd",
      the hidden rule the arc exists to kill) / `herd_corral_locked_both` (**2 reasons** — the `🐄
      Corral needs:` header + bullets, gated on **Penning** with Herding fully known, so the frame
      guards the §4.3 reshuffle). Slice 6b adds: **`two_meter_split`** (THE headline frame — the
      top-bar knowledge strip + this herd's own meter + the bridging gate reason, all at once) /
      `herd_tame` / `herd_tame_stalled` / `forage_sow` (enabled, `Preparing: +0.02 → then +2.40` —
      near-zero dip, 2× tended payoff) / `forage_sow_locked` (2 reasons, one fixed by practice and one
      only by moving) / `forage_sow_too_dry` / `forage_sow_too_poor` (the two refusals must read
      differently) / `forage_field_building` (`Sowing 45%` beside `🌾 Tended Patch`) / `forage_field`
      (`▦ Field`) / `forage_cultivate_done` (a COMPLETED Tended Patch with a standing Cultivate: 🌱
      Cultivate greys "Already a Tended Patch — ♻ Sustain-forage it to harvest", the "Preparing → then"
      line is GONE, and the policy falls back to Sustain's extractive preview `+0.32 /turn · renewable`) /
      `forage_sow_done` (a completed Field: ▦ Sow greys "Already a Field …" the same way, one rung up).
  - **Pre-commit yield forecast** (on BOTH assign controls): setting up a forage/hunt assignment used
    to give no feedback — you staffed 6 workers, committed, advanced a turn, and only then learned 5
    were wasted. The sim now streams, on `ForagePatchState` and `HerdTelemetryState` alike, a
    `perWorkerYield` plus one take ceiling per policy — all food/turn at the source's **current
    biomass**, exported at `output_multiplier = 1.0` — enough to compute the take *while composing*.
    **The two source kinds carry the ceilings differently, and that asymmetry is load-bearing:** the
    PATCH has flat scalars (`ceilingSustain` / `ceilingSurplus` / `ceilingDeplete` / `ceilingEradicate`,
    plus `ceilingCultivate` / `ceilingSow`) because it has no policy list; the HERD has ONLY the
    `huntPolicyCeilings` list (its identically-named scalars are deprecated slots — a new policy costs
    the herd no schema change).
    `expected(workers, policy) = min(workers × per_worker_yield, ceiling[policy])` (the ceilings are
    already biomass-clamped, so that `min` IS the take) and `max_useful_workers(policy) =
    ceil(ceiling[policy] / per_worker_yield)`. Decoded in `native/src/lib.rs`
    (`herds_to_array` bare / `forage_patches_to_array`, both the snapshot + delta paths), carried to
    the controls via the herd dict and — for the patch — via `forage_patch_lookup` → `_tile_info_at`
    as `patch_`-prefixed keys (in `FOW_DISCOVERED_HIDDEN_KEYS`, so a remembered tile redacts them).
    Two affordances, both recomputed on **every** stepper *and* policy change (both already re-render
    the controls): a live forecast line (scaled by the **selected band's `output_multiplier`** — the sim
    exports at 1.0), and a **worker-stepper cap** of
    `min(idle-worker cap, max_useful_workers(policy))` — the `+` goes dead at the cap and, when
    max-useful is the binding one, a `"max N worker(s) useful here — more would be idle"` note
    explains why (a Deplete/Eradicate ceiling exceeds Sustain's, so switching policy moves the cap).
    **THE LOCAL-HUNT USEFULNESS CEILING IS `max(take/prepare max-useful, herd crew)`, AND BOTH CAP
    TWINS READ IT FROM ONE PLACE — `SourceForecast.herd_crew_floor(herd, forecast)`.** A managed herd
    needs its herding crew EVERY turn to HOLD the herd, but the take/prepare side alone ignores that (a
    Corral rung's prep forecast reports "1 worker suffices to prepare"; a Wild Fowl herd's Sustain take
    saturates at 2 workers while the sim asks for 3 keepers), which pins the player below the crew and
    the herd then sheds animals it cannot hold. The floor is a RAISE, never a new cap, and an UNBOUNDED
    forecast stays unbounded, so the maintenance crew is always staffable and the "max N useful here"
    note reads the corrected N. Auto-max on policy-select fills to it. A wild herd reports
    `herders_needed 0`, so `max(useful, 0)` is a no-op everywhere else.
    * **Which field it reads is keyed on the FORECAST's own `investment` flag, not on a policy-name
      table** — `forecast_inputs` sets that flag from `FORECAST_PAYOFF_KEYS`, whose keys are exactly
      `INVESTMENT_POLICIES`, so the two agree by construction while `SourceForecast` stays free of the
      compose vocabulary (it references no `Hud*Vocab` module at all, and that is the invariant).
      An INVESTMENT rung reads the ownership-INDEPENDENT `herders_needed_if_managed`, because the rung
      is what MAKES the herd managed: a still-wild herd reports `herders_needed == 0` until the sim
      takes ownership, so the plain field would pin the player at the 1-worker prep count and the herd
      would read under-herded the moment it became theirs. An extractive rung reads the plain
      `herders_needed`. The two fields are equal on an already-managed herd, so it is safe either way.
    * **THE COMPOSE STEPPER AND THE WORK-BOARD ROW ARE TWINS, and the floor is what makes the promise
      true.** `DrawerComposeController._forecast_worker_cap(forecast, assignable, useful_floor)` and
      `SourceForecast.source_worker_cap_state(forecast, workers, idle, useful_floor)` sit beside each
      other so a worked row and a compose stepper can never gate differently; the floor reached only
      the compose side at first, so the board flagged a herd `⚠ under-herded` and then disabled the very
      `+` that would staff the missing keeper. `BandPanelController._work_source_models` passes
      `herd_crew_floor(live_herd, hunt_forecast)` on the HUNT branch (the LIVE herd from
      `find_world_herd` — herds migrate) and the FORAGE branch passes nothing, a patch owing no crew.
    * **Local hunt ONLY** — the expedition party has no herding crew, so
      `SourceForecast.expedition_useful_cap` is left alone. The Herders drawer row (`A / N —
      under-herded`), the work row's own ⚠ and the shed consequence line all read the SAME
      `herders_needed`, so the cap, the rows and the consequence never contradict.
    * `band_panel_preview` **`band_panel_work_herder_floor`** is the frame and the assertion: a managed
      Wild Fowl herd owing 3 keepers whose take saturates at 2, staffed at 2 with idle workers free —
      the row must carry the under-herded ⚠ AND keep its `+` live, and both twins must gate at 3.
    **When the *labor* cap binds instead** (idle workers run out *below* the usefulness ceiling), the
    silent-disable case is filled by a companion note — `LABOR_BOUND_NOTE_FORMAT` = `"N of M useful —
    free up idle workers to send more"` (M = the usefulness ceiling, so it tracks the selected policy;
    the expedition's party-size sub-case, `idle >= max_party_size`, reads `PARTY_SIZE_BOUND_NOTE_FORMAT`
    = "N of M useful — at the max party size"). The cap value is unchanged (still `min(labor,
    usefulness)`); only the note now names *which* ceiling bound and the M you're working toward, so a
    disabled `+` is never mute. (`SourceForecast.expedition_useful_cap` scans the full estimate table for M even past
    the fieldable party, so "of M" can exceed the party you can currently staff.)
    **ONE forecast row per rung, and forage now mirrors the local hunt exactly** (`Hud.gd`): an
    **INVESTMENT** rung (Cultivate/Sow — the `_forage_assign_policy in INVESTMENT_POLICIES` branch)
    keeps `_forecast_yield_row`'s dip→payoff deal (`Preparing: +X /turn → then +Y /turn`); an
    **EXTRACTIVE** rung renders `_local_forage_preview_bbcode` — the plant twin of
    `_local_hunt_preview_bbcode` — a bare rate + verdict (`+2.74 /turn · renewable`, or WARN-amber
    `⚠ … — overdraws the patch` on Deplete/Eradicate via `_is_overdraw` against the Sustain-ceiling
    yield), through the SAME `HudWidgets.forecast_label` RichTextLabel at `ALLOC_SECTION_FONT_SIZE` the hunt line
    uses. This retires the old `"Expected yield:"` prefix for extractive forage (`FORECAST_LABEL_FORMAT`
    is gone and `_forecast_yield_row`'s non-investment `else` branch was unreachable and removed — its
    only two callers, hunt via `forecast_active` and forage via the `INVESTMENT_POLICIES` guard, both
    gate on an investment rung) and fixes the gap where an Eradicate/Deplete forage rendered no overdraw
    warning. Shared helpers `SourceForecast.forecast_inputs` / `SourceForecast.max_useful_workers` / `SourceForecast.expected_yield` /
    `_forecast_worker_cap` / `_forecast_yield_row` (investment-only now) serve both controls. **Guards:**
    `per_worker_yield == 0` (a dead-season tile) → no row,
    no cap, never a divide-by-zero; a **tended patch / corralled herd** reports every ceiling ==
    `per_worker_yield` ⇒ max-useful 1, policy irrelevant. Applied to the **local hunt only** — an
    expedition accumulates toward a carry cap over several turns of travel, so the herd's per-turn
    ceiling is not the bound on its party size. The **post-hoc** `"· only N of M working"` overstaffing
    note on the allocation rows stays: it still covers a source whose biomass FELL after you staffed
    it. ui_preview: `food_tile` / `forage_forecast_cap` / `tended_tile` / `herd_hunt_band_near`.

  All emit `assign_labor_requested(payload)` (payload: `faction/band/kind/workers/x/y/herd_id/policy`);
  `Main._on_hud_assign_labor` formats the `assign_labor …` text command. **Clear all** emits
  `cancel_order_requested` (the repurposed `cancel_order` = clear-all → fully idle). The roster
  glyph keeps reading the still-populated `activity` (now the largest-worker
  kind: `idle|forage|hunt|scout|warrior`) and `hunt_mode`. `harvestTask`/`scoutTask` are always
  null server-side and no longer decoded. **Convenience shortcut:** double-clicking a herd on the
  map (`MapView.herd_quick_hunt_requested` → `Main._on_map_herd_quick_hunt` → `Hud.quick_assign_hunters`)
  assigns the player band's idle workers to hunt that herd at Sustain — a no-op with a command-feed
  note when there are no idle workers (never silently nothing).

---

## A hunt pays TWO products — the render-only-when-non-zero rule (issue #337, `docs/plan_hunt_yield_model.md`)

The sim's hunt yield is a **vector**: the species' own `HuntYield` (provisions **and** trade goods)
times the policy's intensity. The client read only the food half, so an **inedible** species — a wolf,
which pays pelts and no meat — rendered `+0.00` on every rung and looked like a source worth nothing.

**THE ONE RULE, and it is applied at every surface: render a component only when it is non-zero.**

| species | reads |
|---|---|
| deer (`provisions > 0`, `trade > 0`) | food **and** trade, **food leading** |
| wolf (`provisions == 0`, `trade > 0`) | trade only — **never** a "0 food" line |
| a forage patch (no trade projection yet) | food only — **never** a "0 trade" line |

A `0` printed as a number for a component the species does not produce is the false precision this
whole arc exists to remove; it is not "more complete", it is wrong. The one place a zero survives is a
component the source genuinely HAS and did not pay this turn (a worked row's `+0.00 /turn`).

**Trade is stated GENERICALLY** — `FoodIcons.TRADE_GOODS_GLYPH` (`⇄`) plus the words "trade goods". The
sim models a **scalar**, so the client says so: there is deliberately **no per-species noun** (pelt /
ivory / hide). A named good per species is a flavor layer on top of the scalar, explicitly deferred by
the design doc, and inventing one here would put words on the wire's behalf the sim cannot back.

### The shared layer (`SourceForecast`)

- `has_component(rate)` — the single "is this component present?" gate (`>= FOOD_FLOW_MIN`), so food
  and trade are judged identically everywhere.
- `format_trade(v)` → `⇄ +0.35`; **`yield_components(food, trade)`** → `+0.31 /turn · ⇄ +0.12` — the ONE
  joiner every per-turn readout goes through, so no two surfaces can word the pair differently.
- **`magnitude_components(food, trade)`** → `0.20 ⇄ 0.22` — its COMPACT twin for a surface that
  supplies its own framing and states levels rather than deltas (the work zone's filter chips). Same
  rule, same food-leads order, bare magnitudes joined by `COMPACT_COMPONENT_SEPARATOR` (a space, since
  those chips already spend their `·` separating a count from its total).
- **`extractive_take_pair(food, trade)`** — the rung metric `{compact, full}` (the food-only
  `extractive_take` survives for the forage picker, whose plant-side trade is not projected).
- **The rule reaches the INVESTMENT payoffs too**, not just the extractive caps.
  `SourceForecast.FORECAST_PAYOFF_TRADE_KEYS` (`corral` → `corral_trade`, `tame` → `pastoral_trade`
  — HERD rungs only, the plant web projecting no trade rate) gives `forecast_inputs` a
  **`payoff_trade`** beside `payoff`, and `DrawerComposeController._payoff_take(payoff,
  payoff_trade)` builds the same shape `extractive_take_pair` does: `picker_products` on the face,
  the `POLICY_PAYOFF_FULL_FORMAT` food clause joined to the shared `POLICY_CAP_TRADE_FORMAT` trade
  clause in the tooltip, each half only when `has_component`. `POLICY_CAP_TRADE_FORMAT` is bare
  wording with no "up to" in it, which is why it serves both the cap and the payoff despite its name.
  `_hunt_policy_takes` emits a face when EITHER component is non-zero, so a trade-only payoff still
  gets one; `_forage_policy_takes` passes `0.0` explicitly rather than relying on a default.
- **`picker_products(food, trade)`** → `0.96 food · 0.24 trade` — the same rule and the same food-leads
  order **in WORDS and without the sign**, for the ONE surface that has room to name its products: the
  policy picker's two-line rung face (`compact` above is written in terms of it). The picker names
  rather than marks because its line 1 already carries a glyph naming the RUNG, and two glyph families
  in one line at one weight is the axis collision this treatment removed — see the picker notes above
  and `sprites-widgets.md`.
- `hunt_policy_trade_ceiling` reads **`hunt_policy_trade_ceilings`**, the trade twin of
  `hunt_policy_ceilings`. Two dicts keyed by the same policy strings rather than one dict of pairs:
  the decoder fills both in ONE pass over the single wire list, so they cannot drift, and every
  existing food-only reader is untouched.

### THE AXIS — what a divide-by-a-quantum consumer must use

`herd_yield_axis` / `herd_axis_rates` are the client mirror of the sim's `ratio_axis()`: the first
component with a POSITIVE rate, **provisions preferred** so every edible species divides exactly as it
did before this arc, trade for an inedible one. `forecast_inputs` returns the resolved triple
(`axis_per_worker` / `axis_ceiling` / `axis_per_animal`) beside the raw per-component fields, and
**everything that divides by a per-animal quantum reads the triple** — the kill rhythm, the
carry-aware delivered take (`_hunt_delivered_and_waste`), the averaging window
(`_hunt_avg_window_turns`), and the whole-animal worker cap in `max_useful_workers`. A wolf's
`food_per_animal` is honestly `0`, so a food-only derivation divides by zero and silently produces
nothing at all. The animals-per-turn line needs no currency word either way: **an animal count is a
ratio, and a ratio is unit-free.**

### NEVER clamp a per-herd preview with `huntPerWorkerProvisions`

`PopulationCohortState.hunt_per_worker_provisions` is a **species-BLIND** per-cohort echo of the global
`hunt.provisions_per_biomass` — the cohort has no herd in scope, so it cannot know the quarry is
inedible, and quoting its positive food rate against a wolf's all-zero food ceilings is exactly what
manufactures phantom food. The species-aware per-herd rates are `HerdTelemetryState.perWorkerYield` /
**`perWorkerTrade`**, and the local-hunt preview clamps with THOSE, per component. (The cohort field
survives as the expedition **outfit** lever, before a target is chosen.)

### `deliversFood` WAS REDEFINED — re-read every branch that keys off it

It no longer means "this is not a denial mission". It now means **the quarry is edible**. Consequences,
all of them live in `hunt_trip_forecast` / `hunt_forecast_line_bbcode` / `expedition_policy_takes`:

- **Eradicate DELIVERS.** It banks a whole-stock windfall like every other rung, and its raid line
  quotes that payload instead of "denial mission, delivers no food".
- A **denial** raid is now one that lands NOTHING IN EITHER CURRENCY (`delivers_food == false` **and**
  `delivers_trade == false`) — a property of the QUARRY, never inferred from the policy string.
- **"Too lean to raid"** tests `delivered_food <= 0 **and** delivered_trade <= 0`; reading food alone
  would call every wolf raid empty.
- `expedition_policy_takes` gates each component on its OWN `delivers_*` flag, and
  `expedition_useful_cap` scans the plateau on the component the quarry pays (an inedible species
  delivers 0 food at every party size, so a food-only scan finds no plateau and the party stepper
  loses its cap).

### The one-slot surfaces show the product the species PAYS

Two readouts have a single narrow slot and cannot carry a pair — the **work-board row's** fixed-width
rate column (`BandPanelController._work_row_rate_text`) and the **map's** on-tile yield label
(`BandOverlayRenderer._draw_yield_label`). Both show food when there is food (so every forage patch
and edible quarry is unchanged) and otherwise the trade rate marked with the glyph: `⇄+0.22`. The
work **inspector strip** beside the row states both components in full, which is where a deer's trade
shows.

**`trade_yield` IS NOT FOOD INCOME.** The Food line's Gathered/Hunted breakdown and the band's
`food_income` (`DetailFormat.band_food_income` / `sum_realized_yield` / `band_net_food` /
`band_has_food_flow`, and the arrivals schedule) still sum `actual_yield` alone — trade goods credit
the faction stockpile and never the larder — so a trade-only hunt must not move the Food line. That is
what keeps the larder identity closed for an inedible quarry, and it is why the answer for an
AGGREGATE is never "add trade to the food total".

**But an aggregate that omits trade entirely is the same lie one level up.** The work zone's header
read `3 sources +0.35 /turn` with a `⇄+0.22` wolf row directly beneath it — the arithmetic visibly did
not add up, and the one source paying only trade read as contributing nothing to the band. So the
render-only-when-non-zero rule applies to totals too, as a **SIBLING**: `3 sources +0.35 /turn ⇄
+0.22`, and `🦌 2 · 0.20 ⇄ 0.22` on the per-kind chips (`SourceForecast.magnitude_components`, the
bare-magnitude twin of `yield_components` — a chip states levels, not deltas). A band with no
trade-paying source renders exactly as before. Details in `band-city-panel.md`.

**When you add an aggregate, ask which of the two it is** — a *larder* figure (food alone, by the
identity above) or a *productivity* figure (both products, each when non-zero). Nothing else in the
client currently sums or counts across sources: the parties header counts parties and workers, and the
attention producers key off `idle_workers` / `turns_of_food` / pen status, never "this source yields
no food", so a trade-only source is already productive to them. **Do not add a "produces nothing"
empty-state that tests food alone.**

**Frames.** `ui_preview`: **`herd_hunt_pelts_only`** (the frame the arc is judged on — a wolf's four
rungs read `0.90 / 1.35 / 1.95 / 2.70 trade`, no food line, no zeros, and the animals-first preview +
averaging-window disclaimer still render off the TRADE quantum) · **`herd_hunt_both_products`** (the
same picker on a deer: `2.33 food · 0.34 trade`, food leading) · **`herd_investment_both_products`**
(its INVESTMENT twin — a Wild Boar whose Tame reads `→ 1.48 food · 0.37 trade` and whose greyed Corral
reads `→ 2.95 food · 0.74 trade`, asserted as literal strings against the rungs found by
`HudWidgets.POLICY_RUNG_META`; `herd_tame`'s food-only deer is the companion showing no `0 trade` is
printed) · **`herd_hunt_pelts_raid`** (the wolf as an
expedition target: `delivers ≈3 Grey Wolf over ≈9 turns · ⇄ ~4 trade goods`, primary Send — NOT a
denial) · `herd_hunt_eradicate` (an Eradicate boar raid now delivers `~40 food · ⇄ ~5 trade goods`) ·
`hunt_picker_ascending` (the drawer's standing summary `+0.84 /turn · ⇄ +0.12`) · `food_tile` (the
forage control — food only, no "0 trade"). `band_panel_preview`:
**`band_panel_work_trade_rows`** / **`band_panel_work_trade_inspector`** (a food row, a food+trade row
and a trade-only wolf row on one board; the inspector sentence reads `⇄ +0.22 · Deplete · Working`) ·
**`band_panel_work_trade_totals`** (the aggregates — the same band with the deer unassigned, so its
sole hunt pays trade: header `2 sources +0.15 /turn ⇄ +0.22`, chip `🦌 1 · ⇄ 0.22` with the food term
suppressed).
`map_preview`: `map_band_work` (the hunted wolf labels `⇄+0.22 ⇊` beside the deer's `+0.20`).

---

## The work row carries TWO axes — the standing RUNG and the verb in flight

A board row's mark column used to carry one glyph, `FoodIcons.for_policy(policy)`, and a policy is
**what the band is doing right now**. It is not what the source **is**. The two come apart the moment
a ladder rung completes: a patch under construction wears 🌱 Cultivate, and the turn the build lands
the policy reverts to `sustain`, the glyph reverts to ♻, and a ~25-turn investment leaves no trace on
the board where labor is managed. `Forage (14, 11) +0.97 ♻` on a Tended Patch and
`Forage (11, 9) +0.61 ♻` on plain wild ground were the same row, distinguishable only by a yield
number — which is exactly what cannot be read back as a rung.

So the row carries a **SOURCE-RUNG mark** as its own reserved slot, left of the policy/⚠ marks:

| Rung | Mark | Where the glyph already lives |
|---|---|---|
| wild (plant or animal) | *(none)* | — |
| Tended Patch | 🌾 | `DetailFormat.CULTIVATION_GLYPH` (also `cultivation_label`'s) |
| Field | ▦ | `DetailFormat.field_glyph()` → `FoodIcons.POLICY_ICONS[sow]` |
| pastoral (tamed, unpenned) | ◎ | `DetailFormat.pastoral_glyph()` → `FoodIcons.POLICY_ICONS[tame]` |
| penned (corralled) | 🐄 | `DetailFormat.CORRAL_GLYPH` (also `corral_label`'s) |

**BOTH marks stay on the row.** A Tended Patch being Sustained and one being Depleted are different
situations, and collapsing them into a single glyph loses the more dangerous one.

**WILD IS THE ABSENCE OF A MARK.** Rung 1 is where every source starts, so glyphing it would put a
mark on every row in the game to say nothing has happened yet.

**THE HIGHER RUNG WINS, and the test order is what enforces it.** A Field is *also* `is_cultivated`
and a penned herd is *also* fully domesticated, so `BandPanelController._work_source_rung` tests
`is_field` before `is_cultivated` and `corralled` before `domestication` — reversed, every rung-3
source would wear its rung-2 mark, which is the one distinction the mark exists to draw.

**The animal side BORROWS the `tame` verb's ◎ because it has no rung glyph of its own.**
`DetailFormat.husbandry_label` (Domesticated) and `corral_label` (Corralled) both wear 🐄, so reusing
that for the pastoral rung would make pastoral and penned indistinguishable. `FoodIcons.gd`'s own
design note — each verb wears the glyph of THE RUNG IT BUILDS — is what makes ◎ the pastoral herd's
mark rather than a new invention.

**The glyph-only accessors exist because the labels weld glyph to words.** `cultivation_label`
returns `"🌾 Tended Patch"`, which a one-glyph column cannot take, and slicing that string would make
the mark a function of the wording. `DetailFormat` therefore exposes the four marks on their own
(`CULTIVATION_GLYPH` / `field_glyph()` / `pastoral_glyph()` / `CORRAL_GLYPH`) and the labels are
written *in terms of them* — one home per glyph, in both directions.

### Reading the rung off the wire

- **Forage** — `_band_labor.forage_patch_lookup()[Vector2i(x, y)]`, keys **BARE**: `is_cultivated`,
  `is_field`, `committed_display_name`. The `patch_`-prefixed spellings belong to the `tile_info`
  cross-ref MapView stamps on, and reaching for them here silently reads nothing (`hud_compose_vocab.gd`
  → `BARE_FORECAST_PREFIX` carries the long form of that trap).
- **Hunt** — `_band_labor.find_world_herd(herd_id)`, keys `corralled` and `domestication`
  (`>= DetailFormat.HUSBANDRY_PROGRESS_COMPLETE` is tamed). The LIVE herd, never the assignment's
  launch-time target: herds migrate, and the rung travels with the animals.

No new snapshot field was needed — `_work_source_models` already read both dicts for the worker cap
and threw the rung fields away.

### The row's width budget

The mark is a **third fixed slot** (`HudWorkVocab.WORK_ROW_RUNG_WIDTH`, 16px + one
`WORK_ROW_SEPARATION`) in a row whose every non-label element is fixed-width, so the label absorbs it:
at `WORK_COLUMN_MIN_WIDTH` (380) the label keeps ~156px, which still holds the longest real label
(`Hunt Woolly Mammoth`) un-ellipsised. `WORK_ROW_MARKS_WIDTH` is **unchanged at 20** — the rung has
its own slot rather than being crowded into the policy/⚠ one, because two glyph families at one
weight in one column is the axis collision `HudWidgets`' two-line rung face was built to remove.

**The slot is reserved on EVERY row, wild ones included.** The label is the only `SIZE_EXPAND_FILL`
child, so everything after it is effectively right-anchored: a slot that appeared only on tended rows
would shift the rate column row-to-row and the board would read ragged.

**The mark is `HudStyle.SIGNAL`-tinted** — a standing rung is a completed investment, the same
treatment `cultivation_value_hex` / `field_value_hex` / `corral_value_hex` give it in the detail
readouts. That colour is also the second thing separating the two glyph families at 13px. It reaches
▦ and ◎ (text-presentation symbols inherit `font_color`) and **not** 🌾 or 🐄, which carry their own
emoji colours — the same asymmetry `FoodIcons.gd` documents for the picker.

**The mark Label is `MOUSE_FILTER_PASS`, deliberately not `HudWidgets.set_label_tooltip`.** A Label
defaults to `IGNORE`, which makes `tooltip_text` a silent no-op; the shared helper's fix is `STOP`,
which would turn the slot into a dead hole in a row whose whole body is a click target. PASS shows the
tooltip *and* lets the press bubble to the row's `gui_input`. It carries
`HudWorkVocab.WORK_ROW_RUNG_META` as its stable handle — `FoodIcons.SITE_ICONS` already spends 🌾 on
`savanna_grassland`, so a harness that found the mark by its glyph would find the row's SOURCE icon
instead.

### Frames

`band_panel_preview`: **`band_panel_work_rungs`** (the board the marks are judged on — five rows, one
per rung, on ONE band, every row on ♻ Sustain so the frame cannot be passed by the verb, and a wild
forage row present as the control that absence reads as wild rather than as a missing glyph) ·
**`band_panel_work_rungs_wide`** (the same five in the wide shell). `_assert_work_row_rungs` pins the
glyph on all five INCLUDING the wild row's empty one, and `_assert_rung_labels_are_hoverable` pins the
tooltip + PASS pair that a rendered frame structurally cannot show. The paged board's patches
(`_many_source_patch_fixtures`) carry rungs on a stride, so `band_panel_work_page` /
`band_panel_work_wide` / `band_panel_shell_{below,at}_threshold` show the marks at real density and at
the narrowest legal column. Those fixtures carry **rung fields only** — no `per_worker_yield` /
`ceiling_*` — so `max_useful_workers` stays unbounded and the steppers gate exactly as they did before
patches were pushed into those frames at all.
