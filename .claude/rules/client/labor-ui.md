---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{ComposeSheet,ComposeState,DrawerComposeController}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{HudBandLaborState,SourceForecast,FoodOutlookChart,ArrivalStrip}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/{hud_compose_vocab,hud_work_vocab}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/RungGates.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/HarvestFloorChart.gd"
---

<!-- Extracted verbatim from lines 171-172;179-179;185-186;1776-2474 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Labor allocation UI — the compose sheet and forecasts

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/HudBandLaborState.gd` | `RefCounted` state model (HUD decomposition Phase 0) — "the digested per-snapshot player world + optimistic overlay": `player_band`/`player_bands`/`panel_band`/`player_expeditions`, `world_herds`, grid scalars, `current_turn`, the `prev_band_sizes` losing-population diff, the `forage_patch_lookup`/`food_module_by_tile` lookups, and the `pending_labor` optimistic overlay. Ingest mutators (`set_turn`/`set_grid(width, height, wrap)`/`set_world_herds`/`set_panel_band`/`ingest_snapshot_bands`/`set_food_modules`/`set_forage_patches`) + the pending API (`record_pending_assign`/`record_pending_move`/`reconcile_pending`/`pending_assigns_for`/`pending_key`) + the moved-on derived readers `effective_worker_map`/`effective_idle`/`effective_forage_workers`/`effective_hunt_workers` (pure functions of `pending_labor` + a band) + the statics `as_schedule` and **`labor_assignments_of(band)`** (the public band-dict `labor_assignments` reader — `DetailFormat` + `AttentionController` reach it as `HudBandLaborState.labor_assignments_of`; it merged HudLayer's `_labor_assignments_of` static into the byte-identical private copy that already lived here, and its four internal callers now call it unqualified. The MapView-side `BandOverlayRenderer._labor_assignments_of_marker` deliberately stays a LOCAL copy — a renderer must not depend on the HUD's band-labor model). Also owns the **thin band-labor readers** every consumer reaches through `_band_labor.` — the roster pair `current_player_bands`/`player_band_by_entity`, the per-source lookups `forage_assignment_of`/`hunt_assignment_of` and their `workers_for_forage`/`workers_for_hunt`/`policy_for_hunt`/`policy_for_forage`/**`source_crew_pool_forage`/`source_crew_pool_hunt`** (the ONE pool both of a compose sheet's steppers draw on — it replaced the four per-activity `assignable_*` ceilings, which were each right about ONE command and wrong side by side on a sheet that edits both)/**`unstaffed_build_forage`/`unstaffed_build_hunt`** (a rung this faction has DECLARED and put nobody on, read off the CONFIRMED wire row alone — see `selection-card.md` → "A build DECLARED with nobody on it is a fourth state") — plus the DERIVED READS over its own tables that the `BandPanelController` shared-layer pass brought home: `find_world_herd` (8 call sites file-wide — herds MIGRATE, so this list, never an assignment's launch-time target, is the authority on where a hunted herd is), `food_module_icon` (+ its `FOOD_SITE_KIND_GAME_TRAIL` key), `effective_role_workers`/`workers_for_role` (the band-wide-role twins of `effective_forage_workers`/`workers_for_hunt`), and **`band_parties`/`band_party_workers`** — the pair that KILLED the band↔parties straddle, since the WORKFORCE header's `· N away` clause and the parties zone's row set now read one filter over `player_expeditions()` rather than the band zone calling into the parties zone. **`band_party_workers` feeds a HEADER CLAUSE, not a bar segment** — the sim removes a party's members from the parent cohort on launch, so its sum sits OUTSIDE the `working_age` the WORKFORCE segments partition (`band-city-panel.md` → "PARTIES ARE A HEADER CLAUSE"). Plus the canonical policy-rung consts `HUNT_POLICY_OPTIONS`/`FORAGE_POLICY_OPTIONS`/`DEFAULT_HUNT_POLICY` (the last aliases `SourceForecast`'s; `HudLayer` re-exports all three via `const X = HudBandLaborState.X`). Emits `changed(reason)`, consumed by nothing yet |
| `ui/hud/ComposeState.gd` | `RefCounted` state model (HUD decomposition Phase 2c-1) — "what the player is dialing but has not committed": the tile card's **forage** compose (`forage_key`/`count`/`policy`/`species`/`band` + its autofill one-shot), the herd drawer's **hunt** compose (`hunt_key`/`count`/`policy`/`band` + its own one-shot), the Band panel's PARTIES-zone **party** compose (`party_quarry_id` + its one-shot) on its own clearly-separated accessor group so a later band-panel extraction can take it without unpicking the drawer's, and the open sheet's subject identity (`kind`/`subject`; `COMPOSE_KIND_*` alias to its `KIND_*`). Mutators are named for the transition — `begin_*_source` + `seed_*` (the two-step re-seed: the caller must resolve the actual band between them, and `seed_*` records that band as `forage_seeded_band()` / `hunt_seeded_band()` so an ACTOR-band change re-seeds like a source change — see "THE COMPOSITION RE-SEEDS ON A SOURCE CHANGE **OR AN ACTOR-BAND CHANGE**"), `set_*`, `arm_*_autofill`/`consume_*_autofill`, `reset_*_source` (the harnesses' way to stage a fresh compose), `set_composing`/`clear_composing` — and the three READ-MODIFY-WRITEs get explicit ones so the field is never read and written apart: **`clamp_forage_count`/`clamp_hunt_count`** and **`resolve_forage_species(resolver: Callable)`** (the RMW is the model's; the crop RULES stay with the caller, so it holds no flora knowledge). Pure DATA — which is exactly why **the `ComposeSheet` NODE lives on `DrawerComposeController`**, beside the lifecycle that opens it, rather than on this model. The model instance is SHARED: HudLayer (the parties zone) and that controller (the drawer) hold the same one. Deliberately **NO `changed` signal**, unlike the Phase-0 pair: nothing subscribes (the compose builders re-render explicitly) and unused API is a liability. **`hunt_policy()` is PUBLIC beyond its builder, but its readers are all HERD-DRAWER ones now** (`_tame_stalled_hint` / `_herd_crew_noun`): `HudWidgets.build_policy_picker`'s `selected` fallback — the one real cross-boundary read, where a work-inspector or party-compose render picked up the DRAWER's rung — was DEAD (every caller passed an explicit, provably non-empty `selected`) and is **deleted**; `selected` is a REQUIRED param, so the shared builder now owns none of its callers' state and the drawer/band-panel boundary is structural rather than conventional |
| `ui/hud/DrawerComposeController.gd` | `RefCounted` controller (HUD decomposition Phase 2c-2b, `docs/plan_hud_decomposition.md`) owning the selection drawer's **COMPOSE half** — the other half of the selection card, after `SelectionCardController` took the identity/list one. It holds the **compose-sheet lifecycle** (`_ensure_compose_sheet` / `open_forage_compose` / `open_herd_compose` / `refresh_compose_sheet` / `is_compose_sheet_open` / `close_compose_sheet` / `_compose_anchor_rect`, and the `ComposeSheet` NODE itself), the two **drawer-action builders** (`build_forage_drawer_actions` / `build_herd_drawer_actions` + the standing-summary / compose-open-button / extend-pen factories and their in-place diffing twins), the two big **compose builders** (`_build_forage_assign_controls` / `_build_herd_assign_controls`), and the **compose-only** forecast/gate/picker layer beneath them (`_forecast_worker_cap` / `_forecast_yield_row` / `_is_overdraw` / `_hunt_take_rate` / `_hunt_delivered_and_waste` / `_hunt_avg_window_turns` / `_hunt_policy_takes` / `_payoff_take` / `_local_hunt_preview_bbcode` / `_local_forage_preview_bbcode` / `_forage_policy_takes` / `_forage_policy_gates` / `_hunt_policy_gates` / `_sow_site_refusal_reason` / `_tame_stalled_hint` / the `_flora_entry_*` sub-layer / `_build_crop_picker` / `_build_band_picker`) — ~1,400 lines, 54 functions. It also owns the drawer-actions diff caches `_forage_drawer_shape` / `_herd_drawer_shape` (zero external readers), so a per-snapshot restate still patches nodes rather than tearing them down. The drawer RENDER DISPATCH (`_render_land_drawer` / `_render_occupant_drawer` / `_render_subject_drawer` / the terrain-lines producer + `_tile_detail_lines_cache` / `_fit_subject_drawer`) and the `%AllocationPanel` expedition/band-move branches later left `HudLayer` too, into `ui/hud/SubjectDrawerController.gd` (Phase 2c-3), and call IN here through `refresh_compose_sheet` / `build_forage_drawer_actions` / `build_herd_drawer_actions`. Hud holds it as `_drawercompose`, constructed in `_ready` after `_selectioncard`. **THE INJECTION SURFACE IS EXACTLY THREE CALLABLES** — `_resolve_assign_band` / `_herd_label_for_id` / `_emit_assign_labor`, each retained on HudLayer because it has callers on the other side too (and `_emit_assign_labor` additionally owns the `assign_labor_requested` emit, the optimistic pending write and `_after_pending_change()`, which is why `assign_labor` stays INDIRECT). Each is reached through a **typed adapter** rather than called raw — `Callable.call` returns `Variant`, which would push an untyped value into every consumer. Everything else is a collaborator: the SAME `_compose` / `_band_labor` / `_selection` model instances (BY REFERENCE), `_topbar` for `faction_knowledge` ONLY (the rung gates), `_selectioncard` for `tile_contents_unseen` ONLY, the two drawer-action containers it fills (`%HerdAssignControls` / `%ForageAssignControls`), `tile_panel` READ-ONLY (the rect the sheet floats beside), and the HUD CanvasLayer as the **host** it `add_child`s the `ComposeSheet` into (a `RefCounted` cannot parent — the `TurnOrbController` fork-panel pattern). **Three absorptions shrank that boundary from six injections to three:** `_expedition_party_cap` → `SourceForecast.expedition_party_cap` (expedition forecast math, beside its sibling `expedition_useful_cap`), `_format_food_module_label` + its `FOOD_MODULE_LABELS` table → `HudFormat.food_module_label` (vocabulary, not compose logic), and — the highest-leverage one — the grid-wrap flag `_grid_wrap_horizontal` **onto `HudBandLaborState` as `wrap_horizontal()`**, beside the `grid_width()` it is meaningless without, so the moving set calls `SourceForecast.hex_distance_wrapped(…, _band_labor.grid_width(), _band_labor.wrap_horizontal())` DIRECTLY and the `_hex_distance_wrapped` injection disappeared (that pass-through survives on HudLayer for its other callers). `_band_display_name` went to `HudFormat.band_display_name` for the same reason. **It emits TWO signals, both RELAYED by HudLayer** (the controller never emits a HudLayer signal): `send_hunt_expedition_requested` → `HudLayer.send_hunt_expedition_requested` and `extend_pen_requested` → `HudLayer.extend_pen_requested` (the latter travels because `_build_extend_pen_control`'s only caller and its diffing twin are both inside). **`is_compose_sheet_open` / `close_compose_sheet` MUST stay callable on the HUD node** — `Main._unhandled_input`'s Esc precedence and ~11 ui_preview sites probe them BY NAME, and a `has_method` probe fails SILENTLY — so HudLayer keeps them as thin delegators. Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`FactionReadouts`/`SelectionCardController` convention. Behaviour identical to the old inlined drawer-compose code |
| `ui/hud/ComposeSheet.gd` | The selection card's **write state** — the floating **compose sheet** (`docs/plan_tile_panel_layout.md` §10-§15). Composing is MODAL BY NATURE (open, decide, commit, done), so the two ~270px compose blocks (`%ForageAssignControls` / `%HerdAssignControls`) left the drawer for a sheet that borrows space only while in use; the drawer keeps the detail rows, a one-line standing summary and an `Assign … ▸` button. **That button wears `primary` while ITS sheet is open and `ghost` at rest — never `armed`**: `armed` is the destructive/warned treatment (DANGER border), and "its sheet is open" is a LIVE state, which this HUD spells in SIGNAL cyan (the Sight chip, the selection accent, the turn orb's calm pulse). **Its card is an `AutoSizingPanel`, NOT a `DockScrollFit` card** — it floats against the VIEWPORT, which is the opposite of what the drawer above needs, and picking wrong misbehaves silently rather than failing (`.claude/rules/client/panel-framework.md`). **Its width is FITTED to its content like its height** — `CARD_WIDTH` is the nominal, not a cap; see "THE CARD IS AS WIDE AS ITS WIDEST ROW" below, and "THE HEIGHT CHROME IS THE HEADER **ROW**" beside it for the same measurement error on the other axis. **`_panel` is held as a member for the assertion, not for the layout** — the `PanelContainer` that draws the card is a real `Container` in a plain `Control`, so its minimum is the one honest measure of what the fit owes. **The node IS the full-screen dismiss catcher with the card as its CHILD**, reusing `NarrativeForkPanel`'s nesting exactly (siblings make the ordering ambiguous and the catcher eats the card's own clicks), pinned to the viewport EXPLICITLY via `_sync_to_viewport` — a hidden Control's anchors never settle, and the full-rect preset would also overwrite the size. **NO SCRIM, and that is the one deliberate departure from the fork panel:** a fork is a story beat demanding attention, an assignment is composed *against* the map (work-range ring, herd position, hunt reach are all live context), so the catcher dismisses without dimming. **And that is also why the catcher dismisses on a real CLICK only, never a wheel tick** (`DISMISS_BUTTONS`, an ALLOWLIST of left/right/middle so a future Godot wheel/extra index stays non-dismissing by default): the catcher is `MOUSE_FILTER_STOP` across the whole viewport, so an idle scroll over the un-scrimmed map lands on it, and dismissing there would throw away the composition mid-read. `NarrativeForkPanel` is deliberately left as-is — a modal scrimmed story beat has no such gesture — so the two diverge here on purpose; do NOT factor out a shared predicate for one differing call site. (**Not** a map-zoom passthrough: the catcher stops the wheel either way, so the map cannot zoom while a sheet is open, and a wheel over the card is absorbed by its own `ScrollContainer`.) Guarded by ui_preview's paired wheel-leaves-OPEN / left-click-CLOSES assertions. The sheet floats BESIDE the selection card (`_place_card`, falling back to the viewport margin) so the list + summary it is editing stay readable. It knows nothing about foraging or hunting: `open(eyebrow, title, subject_key, anchor)` returns the content VBox and the caller fills it. `subject_key` is what lets a per-snapshot refresh tell "the same source, restated" from "a different source, gone" |
| `ui/hud/RungGates.gd` | **All-`static`, stateless** shared RUNG-GATE layer — the one answer to "may this source climb its next rung, and if not, why not?". Extracted from `DrawerComposeController` (issue #412) when the compose sheet stopped being the only surface asking: the Band panel's WORK board marks a source that can climb, and the MAP marks it on the source's own marker — and a renderer must not depend on the HUD's compose controller. Shared-layers-BEFORE-controllers, the same measurement that produced `SourceForecast` and `HudWidgets`. Holds `forage_gates` / `hunt_gates` / `sow_site_refusal_reason` (moved VERBATIM, so the compose sheet's greying is unchanged), **`forage_gates_from_patch`** (the BARE-keyed twin for a raw wire patch — the RAW wire patch carries its keys BARE while the `tile_info` cross-ref `patch_`-prefixes every one of them, and this adapter is the ONE place that mapping is written down. **The prefixing is UNIFORM now (#442)** — `is_cultivated`/`cultivation_progress` were the last unprefixed strays on the cross-ref and are stamped `patch_`-prefixed like their siblings, so there is no longer a mixed convention to remember; reading a `tile_info` key without the prefix silently answers nothing (`hud_compose_vocab.gd` → `BARE_FORECAST_PREFIX` carries the long form)), and **`next_rung_ready`** — the READY test all three surfaces mark from — plus **`knowledge_gate_unmet`** (with its `RUNG_KNOWLEDGE_TRACKS` map: is THIS rung blocked on knowledge specifically? — the same `track < KNOWLEDGE_COMPLETE` test the gate builders make, asked on its own so the compose sheet can suppress that reason **structurally instead of by matching its words**; one caller, for the reason the "A KNOWLEDGE gate renders NO improvement control" section gives). **`wild_fodder_reason` broadens the file's remit** from "may this source climb its next rung" to "…and will the work it is doing actually pay out" — the wild forage patch's fodder credit, which the sim refuses to a faction without Foddering; see "The FODDER account can be real and unbankable at once". **STATELESS IS THE INVARIANT**: the one impurity, faction knowledge, is threaded in as a `knowledge` PARAMETER (`FactionReadouts.faction_tracks(faction)`, the whole `{track: progress}` row `faction_knowledge` reads one key out of), never reached for. `next_rung_ready` requires all three of OFFERED (husbandry ceiling / `can_cultivate`-`can_sow` + willing ground), UNGATED (the gate functions answer nothing), and NOT-ALREADY-RUNNING (a patch mid-Cultivate is progress, not an opportunity), **highest rung first**. **That ordering is load-bearing on the PLANT web only** and its assertion needed care: `is_cultivated` retires Cultivate, so on a TENDED patch the two rungs are mutually exclusive and an ordering test there passes with the branches swapped (measured). `Sow` needs no prior patch, so a WILD patch on sowable ground is the one shape that clears both gates at once. On the animal web the rungs are always mutually exclusive — Tame retires at a full meter, Corral requires one — so ordering is genuinely not load-bearing there. `FactionReadouts.faction_knowledge` deliberately does NOT call `RungGates.track`: dependency DIRECTION outranks the one-definition rule for a `float(d.get(k, 0.0))` |
| `ui/hud/RungLadder.gd` | **All-`static`, stateless** shared LADDER-TRACK layer (`docs/plan_standing_upkeep.md` §2.8) — the one answer to *"what does this source's branch hold, where does it stand on it, and how far may the player send it?"*. `RungGates` answers *may this source climb its NEXT rung*, which is the right question for a MARK; a queue entry names a **destination** now and lays every rung between where the source stands and there, so the picker has to state the WHOLE branch and `next_rung_ready` structurally cannot. `track(kind, source, prefix, improvement, knowledge)` walks `SourceForecast.rung_branch_for_kind` bottom rung first and puts every rung in exactly one of six states — `banked` (already paid for, and it contributes NO figure: a previous improvement is a RECEIPT, NOT A DISCOUNT) · `standing` · `path` · `target` · `locked` · `open` — beside its own owing and its own chained date; `has_track` is the *is there anything to offer* test a caller asks before floating a card, and `build_track(rows, on_pick)` renders it, a **`Button` where the rung may be picked and a `Label` where it may not** (the improvement control's own shape-is-the-statement rule — a greyed button on a locked rung offers an act the sim refuses). ⛔ **IT RE-DERIVES NEITHER THE WORK NOR THE TURNS**: a leg's owing and its chained date are `SourceForecast.build_legs`' rows, read where the queued entry publishes them, and a rung NO entry covers has no leg — its owing is the per-rung `workCost − workDone` pair the wire publishes for exactly that pre-commit question (the same two numbers `forage::plant_build_legs` subtracts), and it states **no date at all**, a chain being computed against a build queue this client cannot see. **STATELESS IS THE INVARIANT** — faction knowledge is a `knowledge` PARAMETER and the press handler a `Callable`, `RungGates`'s own treatment. Its two OUTRIGHT bars (`HudFloraVocab.GATE_REASON_SPECIES_NEVER_TAMED` / `_PENNED` / `GATE_REASON_CROP_CANNOT_CLIMB_FORMAT`) are the one place a rung `RungGates` WITHHOLDS is rendered instead: a mark promises the verb is available, a track says what the branch holds, and a rung silently missing from it reads as a shorter ladder. A rung barred from BELOW takes the blocking rung's own reason (`GATE_REASON_PATH_BLOCKED_FORMAT`), because a climb lays every leg and offering a destination whose path is refused is a job that queues and then blocks |
| `ui/hud/HarvestFloorChart.gd` | The compose sheet's **floor instrument** (`docs/plan_harvest_floor.md` §7.3) — a custom-drawn `Control` (the `FoodOutlookChart` / `ArrivalStrip` idiom) putting the standing stock, the draggable floor line, the projection and the food peak on ONE y-axis of `B/K`, with the `learn_multiplier` gradient rail down the right edge. **IT DRAWS; IT DOES NOT MODEL** — every number comes from `SourceForecast.floor_chart_model`, the projection walks the sim's own `regrowthSamples`, the peak is the argmax of those samples rather than `FLOOR_FOOD_PEAK` restated beside them, and negative samples are carried through as decline. It emits ONE signal, `floor_changed(floor, committed)`, and the second argument is the whole contract: a committed change rebuilds the compose controls (which frees this node), a live one must not, or the drag in flight dies with it — see "THE CHART" below. Keyboard-accessible (`FOCUS_ALL`; arrows / Shift-arrows / Home / End), because the floor is the primary control of the panel. Palette through `HudStyle` only — plus `DetailFormat.ecology_tier_color` for the standing-stock band and the **phase zones** behind it (`_draw_phase_zones`, the furthest-back layer: the source's own `collapseFraction` / `stressedFraction` as horizontal Collapsing/Stressed/Thriving bands, so the floor is dragged against the ecology rather than against a remembered number) |
| `ui/hud/ForecastQuery.gd` | **The client's half of the command socket's SECOND direction** (`sim_runtime/proto/command.proto` -> "THE QUERY CHANNEL") — a `RefCounted` seam owning the request-id sequence, the SUBJECT/KEY split (`subject_of` = kind + band + herd, `key_of` = that plus the kit, party and floor), the `{state, answer, error}` a sheet renders off (`view`), the stale-answer window (`STALE_AFTER_MSEC`), the settled test the crew one-shots gate on (`answer_settled`) and the `answered(subject)` signal every consumer redraws from. **It owns NO socket**: `Main` injects the sender and pumps `CommandBridge.poll_query_replies` in through `deliver` / `expire_stale`, so the HUD asks questions without reaching the network and every state is drivable from a harness with no server. **Its own object because THREE sheets across TWO controllers compose a raid** and each needs the same four things — an id, a rule for which reply is still wanted, a rule for what to show while waiting, and a re-render when the answer lands; two copies would drift the moment one learned to keep its last answer and the other did not. `Hud` holds the ONE instance and fans `answered` out to `_drawercompose` / `_bandpanel` / `_drawer`. **`reset()` is a WORLD-BOUNDARY cache clear and `HudLayer.reset_world_state` is its only production caller** — a subject is kind + band + herd, and a new world hands out both handles again (band ids restart low, herd ids are species + index), so a held answer matches the new world's composed key exactly and renders the previous world's numbers as `STATE_READY`; the shape and the reset contract are `.claude/rules/core_sim/world-handoff.md`. **The no-retry rule is scoped to the SERVER's token class** (`TRANSPORT_RETRY_AFTER_MSEC`): a `query_error` names something wrong with the QUESTION, which the sheet composed itself, so it is never re-asked — but `QUERY_ERROR_TRANSPORT` names a dead socket, which heals, so it is re-askable once the backoff has elapsed (not on the next render, which `ask` reaches once per render and would spin the socket; not never, which strands a sheet on `No forecast available (transport)` for the session after a server restart). The failure keeps rendering through the retry, so a server coming back is ONE transition rather than a flicker. See "THE RAID'S NUMBERS ARE ASKED FOR" below |
| `ui/hud/KitRoster.gd` | **All-`static`, stateless** shared KIT layer (`docs/plan_denial_raid.md`) — the read over `SubsistenceSection.kits` (`kits_for_job` / `kit_by_id` / `kit_display_name` / `display_name_for_id` / `default_kit_for` / `resolve_selection`), the EFFECTIVE tier a given band gets under a given kit — **READ off the band's own `kitTiers` row, never re-derived** (`band_kit_tiers` / `effective_tiers` / `_resolved_tier` / `unequipped_tier` / `equipped_tier` / `kit_item_ids` / `condition_of` / `tier_hint`) — the BAND-WIDE ROLE cards' own tier and gear line (`ROLE_AXES` / `is_band_wide_role` / `role_axis` / `role_gear` / `role_hint`), the OFFER test that decides which kits a quarry may be worked with (`attack_reaches` / `attack_against` / `effective_attack_against` / `kit_uses` / `kit_supplies_any` / `kit_offer` / `kit_is_offered` / `hunt_gate_closes` / `gate_closed_source` — see "A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED"), the resolve-then-reprice seam and the CARRY AXIS it prices on (`carry_axis_for` / `priced_source` / `repriced_source` — the axis is the SOURCE's, a penned herd overriding its job's; see "A PENNED herd is priced — and described — on the KEEPER'S carry"), and the picker ROW itself (`build_kit_row`). **The honesty trio `estimates_quoted_kit` / `estimates_apply_to` / `estimates_quoted_note` is RETIRED with the per-herd estimate tables**: a forecast is a query answered for the composed kit, so there is no other kit's numbers to disown. **`attack_reaches` takes the ROW the attack is read from** — the roster entry for the fresh offer test, the band's `kitTiers` row for the worn gate — so a kit's size window and its attack can never come from two different rows. **Its own file because the control appears on FOUR sheets across TWO controllers** — the Band panel's hunting-party and denial forms, the herd drawer's assign-hunters block, the land drawer's assign-foragers block — **and on the WORKFORCE zone's two band-wide role CARDS** — and a row that has to read identically in six places must have one implementation; the same measurement that produced `SourceForecast` and `HudWidgets`. The ROSTER is snapshot data and lives on `HudBandLaborState` (`kits()` / `default_kit_id(job)`, ingested by `Hud.update_kit_roster` off `Main`'s `kits` + the four job defaults), threaded in as a parameter — this layer holds nothing. **Dependency direction: it reads `SourceForecast` / `HudWidgets` / `HudStyle` / `DetailFormat` (for `role_hint` alone, from inside a function body) / the vocab leaves and none of them may read it back** (a `const` cycle between two `class_name`d scripts fails to load the whole client) |
| `ui/hud/SourceForecast.gd` | **All-`static`, stateless** shared forecast/estimate layer (HUD decomposition, phase 2c-2 precursor) — the pure "what will this source give me?" math THREE consumers ask for: the drawer's compose blocks, the Band panel's WORK zone, and its PARTIES zone. Three families: POST-HOC `source_yield_readout` (what a worked source actually produced, incl. the ⚠ overdraw + overstaff/wasted notes) · PRE-COMMIT `forecast_inputs` / `max_useful_workers` / **`source_worker_cap_state`** (the CONFIRMED-row twin of that cap: `(forecast, workers, idle, useful_floor = 0) → {can_add, note}`, beside the ceiling it reads so a worked row and a compose stepper can never gate differently — the trailing floor is what makes that true rather than merely stated, and `herd_crew_floor` is its one definition; the *hold it after* crew is a floor on BOTH twins and therefore lives inside `max_useful_workers`, carried on the forecast as `hold_crew`) / `expected_yield` / `hunt_policy_ceiling` · THE RAID `hunt_trip_forecast` → `hunt_forecast_line_bbcode` / `hunt_trip_returns_empty` / `hunt_empty_refusal` / `hunt_empty_refusal_reason` / `expedition_party_cap` (the SUPPLY side — the band's idle workforce, and NOT `max_expedition_party_size`, which is the LAST RUNG of the estimate tables' sampled party axis rather than a rules cap) / `expedition_engage_crew` / `expedition_useful_cap` (the DEMAND side, untouched) / `expedition_policy_takes` / `style_send_hunt_button` (`style_send_hunt_button` styles a Button off the raid verdict, so it lives WITH the verdict). Plus **THE DENIAL RAID's own layer** (`docs/plan_denial_raid.md`) — `denial_forecast` / `denial_verdict` / `denial_turns_phrase` / `denial_verdict_text` / `denial_verdict_bbcode` / `denial_take_bbcode` / `denial_party_needed` (a read of the REPLY, not of a table) / `denial_refusal_reason` / `denial_is_short_handed` / `denial_short_handed_reason` / `style_send_denial_button`, over the `DENIAL_VERDICTS` table — which is composed from the QUERY's reply row (`denialEstimates` is retired) and shares NONE of the raid vocabulary above: denial carries no floor and no delivery ETA, so its readout is a collapse verdict and its Send disables in exactly one case (`denial_is_short_handed` / `denial_short_handed_reason` — the band cannot field the party the herd REQUIRES; a party the player under-sized still launches). The rationale lives in `band-city-panel.md` → "DENIAL is a third MISSION on the parties footer". Plus the shared leaves those need — `format_magnitude`/`format_signed`/`format_yield`/`extractive_take`, `band_tile`/`hex_distance_wrapped`, `herd_display_name`, `is_managed_hunt_source`, and the two one-off leaks into the read-only detail layer, `flora_basket_entries` / `husbandry_ceiling`. **WHY ITS OWN FILE:** the next phase lifts a `DrawerComposeController` out of `Hud.gd`, but this layer is called by the work + parties zones too, so it cannot travel with the drawer; pure injection was measured at **54 Callables** and a `_hud` back-ref would weld an already-pure layer to the god object (and the band-panel extraction would then need a SECOND back-ref to the same place). All three consumers depend on THIS instead. **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache; if a new function needs HUD state, pass it in. The one non-plain-value is the grid-wrap pair (`grid_width`, `wrap_horizontal`), threaded as EXPLICIT PARAMETERS through `hex_distance_wrapped` → `round_trip_travel_turns` → `hunt_trip_forecast` / `expedition_policy_takes` so a stale grid can never be captured; `HudLayer._hex_distance_wrapped` is a one-line pass-through supplying the pair off `_band_labor`, so there is ONE hex implementation (`DrawerComposeController` calls the module directly with the same pair). The **forecast vocabulary constants moved here with the math** (`LABOR_KIND_*` / `LABOR_HUNT_POLICIES` / `DEFAULT_HUNT_POLICY` / `SOURCE_KIND_*` / `FORECAST_*` / `MAX_USEFUL_*` / `HUNT_FORECAST_*` / `SEND_HUNT_*` / `HUSBANDRY_CEILING_*` …) and `HudLayer` **re-exports the still-used ones as aliases** (`const X = SourceForecast.X`, one commented block) rather than redefining them — ONE definition, and every HudLayer call site reads unchanged |

## THE HARVEST AXIS IS AN ESCAPEMENT FLOOR, NOT A STANCE (`docs/plan_harvest_floor.md`, issue #455)

**This section supersedes every passage below that names a harvest STANCE.** Those passages are kept
because they still document *why* each behaviour exists; where one says `sustain` / `surplus` /
`deplete` / `eradicate`, read it against this.

The four stances are **deleted from the sim**, not deprecated in the client. What replaced them is one
number per assignment — **the escapement floor**, where the crew stops, as a fraction of the source's
carrying capacity:

| Axis | Question | Wire | Values |
|---|---|---|---|
| **Floor** | how much do I leave standing? | `LaborAssignment.floor` / `expeditionFloor` | any `0.0..=1.0` |
| **Improvement** | what am I building? | `LaborAssignment.improvement` | `""` · `cultivate` · `sow` · `tame` · `corral` |

`FollowPolicy` does not exist. `LaborAssignment.policy`, `PopulationCohortState.huntMode`,
`expeditionHuntPolicy`, `HuntTripEstimate.policy`, `foragePolicyCeilings` and `huntPolicyCeilings` are
all retired `(deprecated)` wire slots that **read zero or empty** — a client still reading them shows
the player nothing and looks like a rendering bug rather than a contract break.

### The client COMPOSES the ceiling, and that permission has a boundary

There is no per-stance ceiling row on either web. What ships instead is the **terms**: `biomass` (B),
`carryingCapacity` (K) and the source's **per-biomass yield vector** (`provisionsPerBiomass` /
`fodderPerBiomass`, published identically by `HerdTelemetryState` and
`ForagePatchState`). `SourceForecast.escapement_room` + `forecast_inputs` evaluate

```text
ceiling(floor, account) = max(0, B − floor·K) × <account>PerBiomass
expected(workers)       = min(workers × perWorkerYield, ceiling(floor))
```

> #### THE BUILD FRACTIONS ARE OFF THE WIRE — the sim publishes no dip to fold in
>
> **A build is raised by the band's own BUILDERS POOL** (`docs/plan_standing_upkeep.md` §2.5): the
> verb *declares* and names no crew (`cultivate|sow|tame|corral <target…>`, and `extend_pen`
> likewise), and the hands stand on `assign_labor <faction> <band> builders <n>` — so the gatherers or
> hunters beside a build carry exactly what they carried before and there is no factor left to
> multiply anything by. `ForagePatchState.cultivateBuildFraction` / `sowBuildFraction` and
> `HerdTelemetryState.tameBuildFraction` / `corralBuildFraction` are `(deprecated)` slots the sim no
> longer writes, and the native reader no longer inserts their dict keys — so **a GDScript expression
> multiplying by one is reading a key that is not there**. The two `*CrewNeeded` slots went the same
> way with `crew_needed` (`yield-forecast.md` → "`workers_needed` IS THE TAKE'S OWN COUNT"), and what
> rides those tables now is the upkeep quartet: `upkeepDemand`, `upkeepSupplied`, `upkeepShortfall`
> and `upkeepWorkersNeeded` — the **maintain** activity's own `workers_needed`, in keepers, beside the
> take's own in haulers.
>
> **The dip prose below this line describes GDScript that still folds one in**, which is the shape the
> client-side pass has to unwind; the wire contract above is what it must unwind toward.

**This is a deliberate, narrow exception to "the sim exports the answer".** That rule exists because a
hunt's TAKE is rounded to whole animals — `floor(ceiling / bodyMass)` is not linear, so no client can
re-derive it. This expression is different in kind: linear and exact in terms already on the wire. The
division of labour is **the client draws the curve, the sim states the take**. Do not let the
composition creep from ceilings into takes; `SourceYield.actual` on a committed assignment is still
the sim's answer, quantisation and all.

**THE BUILD FRACTION MULTIPLIES THE CREW, NOT THE CEILING** (`plan_harvest_floor.md` §3.1). It rides
`per_worker*` in `forecast_inputs` — hence `max_useful_workers`' divisor and `expected_yield`'s crew
term — while every ceiling stays undipped. It moved because dipping the ceiling made a deeper floor
build for FREE (a fraction of a bigger standing stock still filled the crew's baskets), and it is what
leaves the ceiling linear in the floor and therefore composable at all. The player-visible
consequence: a crew big enough to saturate the source's stock pays **no** dip, so the remedy for a
slow build is HANDS — at the shipped 50% carry, twice as many; at the 25% that fixture cans, four
times (`BandFx.cultivating_forage_band_fixture`'s `workers_needed` went 2 → 12 on exactly that arithmetic).
**The harness fixtures carry their own `*_build_fraction` wire values** (`STALE_VERB_BUILD_FRACTION`
0.25, `HERD_DIP_BUILD_FRACTION` 0.5), so a config re-dial does NOT move them — which is what keeps
these frames pinned to the arithmetic they were built to prove rather than to a balance number. `expected_yield_account` therefore has **no
`ceiling_scale` parameter**: `improvement_forecast` carries a whole `base_forecast` and a whole
`build_forecast`, and the deal's two terms are the same call against each. **Any surviving `×
fraction` on a ceiling is wrong, and it looks plausible.**

**A RUNG-3 MANAGED SOURCE HAS NO FLOOR AXIS** (`SourceForecast.source_is_managed`). A Field and a
built Pen are yours — you control their reproduction — so `SourceYieldForecast::managed` pays one
`managed_production` at every floor. The wire still carries their raw `biomass`/`carryingCapacity`/
rates (facts about the herd or the crop), so composing an escapement ceiling on one is **silently
wrong**; the ceiling is the rung's own payoff field (`field_yield` / `corral_yield`). Rung 2 — a
Tended Patch, a pastoral herd — is still a wild stand being drawn down and takes the composition.

**IT IS THE STANDING RUNG THAT DECIDES THIS, NEVER THE COMPOSED ONE**, and the predicate therefore
takes NO improvement argument. A crew mid-`Sow` or mid-`Corral` is still drawing the WILD stand down
at a dipped carry, which is precisely what the sheet has to price; reading the composed rung would
quote a source that does not exist yet — the escapement chart would blank on a stand still being
harvested, and a pastoral herd's ceiling would swap to `corral_yield` while its animals are still
being hunted off the range. The function carried an **unused** `improvement` parameter through five
call sites for a while, with a docstring claiming the opposite ("the COMPOSED improvement counts
too"); the behaviour was right and the comment was the defect, so the parameter went with it — a dead
argument that reads as an invitation is worse than no argument at all.

### THE CONTROL: three intent presets over a chart whose floor line IS the dial

`HudWidgets.build_floor_picker` + `build_floor_chart` replaced `build_policy_picker`. The picker's
face, its two-line rung cell, its `POLICY_RUNG_META` handle and its 3-column ceiling are unchanged —
what changed is that a button is a **shortcut to a value** rather than one of the axis's members.

| preset (`SourceForecast.FLOOR_PRESET_*`) | floor | label (`HudComposeVocab.FLOOR_PRESET_LABELS`) |
|---|---|---|
| `strip` | 0.00 | Take everything |
| `peak` | 0.50 (`FLOOR_FOOD_PEAK`, the sim's `MSY_BIOMASS_FRACTION`) | Best harvest |
| `learn` | 0.80 | Learn from it |

**A PRESET THE PLAYER IS NOT ON MUST NOT LIGHT UP.** `floor_preset_for` answers `""` for anything
between two presets — which is most of the dial once the chart is dragged — and lighting the nearest
one instead states a floor the crew is not holding. Naming is not settled (plan §10 Q2), which is why
the labels live in the vocab module.

The chart is built on the compose sheets ONLY — the work inspector's strip and the parties zone get
the presets without it, because a fixed-width dock strip is not where a continuous dial belongs and
the source's own sheet has the room. The plain `build_floor_slider` it replaced (`FLOOR_STEP` = 5%,
and `FLOOR_SLIDER_META` with it) is deleted; `FLOOR_STEP` survives as the chart's **coarse keyboard
step**, so a player who learned that granularity keeps it.

### THE CHART: the stock bar and the projection are ONE instrument (§7.3)

`HarvestFloorChart` draws four things on ONE y-axis — fraction of carrying capacity — over the phase
zones, which is the whole point of merging them: the standing stock as a band from the baseline to `B/K`, the floor as a
draggable horizontal line, the projection beneath it, and the food peak marked where the sampled
curve actually peaks. A gradient rail down the right edge encodes `learn_multiplier` with a marker at
the floor. **The rail is a fact about THESE PEOPLE ON THIS GROUND**, not a knowledge meter — a tile
knows nothing — which is why it lives on the source's sheet rather than in the faction strip.

**THE STOCK BAND WEARS THE SOURCE'S REPORTED PHASE, AND THE PHASE ZONES STAND BEHIND IT.**
`ecologyPhase` is on the wire as a word and `DetailFormat.ecology_tier_color` turns it into the same
green/amber/red the roster dot wears, so the bar's colour and the floor's position share one
coordinate system. **The BOUNDARIES ship too now** — `collapseFraction` / `stressedFraction` on both
source tables, decoded in `native/src/dict/subsistence.rs` — as fractions of `carryingCapacity`, i.e.
**the same units the floor is in**, which is the whole reason they are drawable: a floor and a phase
band are the same kind of object. `SourceForecast.phase_zones` restates the pair as the ladder
`B/K < collapse → collapsing`, `< stressed → stressed`, else thriving, and `HarvestFloorChart`
paints it as horizontal bands **behind every other layer** (`ZONE_FILL_ALPHA`, plus one hairline per
real threshold — the topmost band's `1.0` is the plot's ceiling and separates nothing). The stock
band keeps its own tint over them, so a healthy stock still reads green across a red zone.
**They are read PER SOURCE and never cached as client constants**: a herd's cuts come from the RUNG
it stands on (`fauna::herd_ecology` resolves wild / pastoral / pen, each with its own ecology block),
so one global pair would be right for a wild herd and wrong for a penned one — the same mistake the
sampled growth curve exists to prevent. A source whose cuts are absent or out of order draws no zones
rather than a guessed ladder.

> **On the animal web the first boundary is the Allee point.** `collapseFraction` is both where the
> phase word changes and where `regrowthSamples` turns negative — one config field in the sim, so the
> zone edge and the curve's sign change are two views of one cliff. `floor_chart_herd_allee` renders
> them together on purpose; a render where they part is a real disagreement, not a nudge to make.

**THE PATCH'S FORECAST FIELDS REACH THE SHEET THROUGH `tile_info`, AND A DECODED FIELD IS NOT YET A
DELIVERED ONE.** The forage sheet reads its patch off `tile_info` with the `patch_` prefix, and
`MapView._tile_info_at` copies the `forage_patches` row across **key by key, from an explicit list** —
so a field the decoder emits but that list omits is silently absent on the plant web and present on
the animal one (a herd dict travels whole). `perWorkerBiomass` and `regrowthSamples` were exactly
that: decoded since 4b, never copied, so against a live sim every forage patch answered
`known == false` — **no chart at all**, and both crew targets absent for want of a priceable crew —
while both preview harnesses passed, because their fixture adapter seeds that pair itself.
`patch_collapse_fraction` / `patch_stressed_fraction` are cross-refed with them, all four listed in
`FOW_DISCOVERED_HIDDEN_KEYS` under the one rule the whole patch payload follows. **Appending a source
field is TWO wirings on the plant web**, and only the second is visible in the panel.

**IT HAPPENED A THIRD TIME, and that is why the list is now GUARDED rather than merely documented.**
`material_per_biomass` / `per_worker_material` reached `ForagePatchState` and the decoder with the
follow-up that gave a patch its material account, and the cross-ref took neither — so a wild stand of
56% tobacco rendered a PER TURN box naming the fodder and never the tobacco, on a client whose
`_forage_yield_model` was already composing the vector correctly. Both preview harnesses passed for
the same structural reason they passed the first time: their fixtures seed `tile_info` themselves, so
**no frame in either harness exercises the cross-ref at all**, and a second seeded frame never can.
`tools/patch_crossref_guard.gd` closes it from the other end — it decodes the real fixture envelope
and asserts a PARTITION over `_tile_info_at`'s output, so a newly appended wire field fails at the
wiring instead of in a panel weeks later (`harness-headless-guards.md` has its claims and its
mutations).

**THE POINTER IS THE DRAG'S ONLY AFFORDANCE, so the whole plot wears `CURSOR_VSIZE`.** The chart is
drawn, not assembled from grabbable widgets, and the drag target is the entire plot rather than the
floor line — grabbing a 1px line would be unusable, and the line is where the value IS, not where you
have to press. So nothing about the chart's shape says it can be dragged and the cursor has to; it is
the prototype's `cursor: ns-resize` on the chart element, and scoping it to the line would advertise
a target narrower than the real one. **Reported from play, after the chart had already shipped** —
a screenshot cannot carry a cursor, so no frame could show its absence and the harness assertion
(`floor_chart_drawn_down`, sabotage-verified) is the only thing that can see it regress.

`set_model` sets it from `_has_floor_axis()`, the same test `_gui_input` refuses on, so a chart with
no dial keeps the plain arrow **and** accepts no press — the pointer can never promise a drag the
handler then declines. A model with `known == false` draws only its backing, and a press on one used
to emit a floor for a source that has no floor axis, which the sheet would then commit.

**THE DRAG COMMITS ON RELEASE, and that is a constraint rather than a preference.** Every floor change
rebuilds the compose controls, which `queue_free`s the chart — and Godot routes motion events to the
node that took the press, so a rebuild mid-drag ends the drag on the first pixel. `floor_changed`
therefore carries a **`committed` flag**: false while dragging (the sheet refills only the readings
that follow the floor, through `DrawerComposeController._refresh_floor_live` and the live hosts it
keeps), true on release, keyboard step or preset. Rebuilding per motion event does not merely cost
more — it cannot work. Keyboard: arrows step (Shift takes `FLOOR_STEP`), Home strips, End leaves
untouched; the value is quantised to whole percent, the resolution `floor_percent` displays at, so the
flag, the preset test and the command cannot disagree.

**THE LIVE SET IS A REGISTRY, AND WHAT IT LEFT OUT WAS THE POINT OF THE PANEL.** It began as two named
keys, the crew targets and the verdict — and the **YIELDS ROW was outside it**, so a player dragging
the floor watched the verdict move while the yield numbers *they were dragging toward* sat
frozen until release. Reported from play. `_register_live(hosts, host, model, workers, fill)` now takes
any container plus the `fill(host, model, workers)` that refills it, and `_refresh_floor_live` walks
the list, so the rule is stated once: **anything whose value — or whose PRESENCE — depends on the
floor belongs in the set — the yields, both crew targets, the verdict, the teaching line and the
locked-account reason — and anything that does not must stay out, or the drag pays for work it does
not need.** Adding a reading is one call, not a new key plus a new type test plus a new branch.

**PRESENCE counts, and it is the half that reads as safe to omit.** A reading whose *number* is
constant can still belong here: the locked fodder reason (#485) says the same words at every floor,
but raising the floor takes the fodder row away, so a sentence resolved once before the render goes on
explaining a `—` no longer on screen. "It does not move under a drag" is a claim about the string, and
the registry is about the ROW.

The assertion that can see this is a **CHANGE, never a presence**: a stale yields row is a perfectly
valid, perfectly findable node, so "the row is still there" passes with the bug fully restored. The
harness captures `Readout.yields_text` before driving `floor_changed(f, committed = false)` and requires it to
differ after (sabotage-verified by taking the yields host back out of the set — exactly that one
assertion fails, the chart-survives and verdict-re-read pair beside it still passing).

**THE CAP IS RESOLVED BEFORE THE CHART ON ALL THREE SHEETS.** The chart, the crew targets and the
verdict are all read against a CREW, so composing them ahead of `clamp_*_count` made the panel state a
verdict for a crew the stepper then refused to show (a full patch reading *"already at the floor"*
beside a stepper the same pass had zeroed). The drawer's two branches resolve first —
`DrawerComposeController`'s hunt one always did, its forage one since that fix — and the DOCK's hunt
form (`BandPanelController._fill_hunt_compose_sheet`) does now: it composed `floor_chart_model` from
`_send_expedition_count` while `expedition_useful_cap`, `consume_party_autofill` and the `clampi` that
settle that count all ran ~30 lines below the chart. **The mount order is unchanged and must stay** —
presets → chart → floor hint → stepper → kit — so what moved is the RESOLUTION, not the row: the cap
block sits above the chart and the stepper row is built from the already-settled count further down.

**THE ASSERTION IS RENDERED-AGAINST-RENDERED, because the disagreement lasts ONE FRAME.** It shows on
the render where autofill arms (a floor click, a committed drag, a fresh quarry) and the next rerender
reconciles it, so a capture taken after the settle is clean — measured: the dock frame is
**byte-identical with the defect restored**. `floor_chart_model` therefore carries the crew it was
composed against (`workers`), `HarvestFloorChart.crew()` reads it back off the LIVE model (never a
build-time copy, so a chart refreshed in place cannot answer staler than it draws), and the party
stepper row carries the count it was BUILT with as `HudWidgets.PARTY_STEPPER_COUNT_META` — neither
side a controller field, so a sheet that clamps its member correctly and still hands the old number to
the chart fails. Frames + assertions: `floor_chart_full` (forage) and
`band_panel_preview._assert_chart_reads_the_settled_party` on `band_panel_compose_hunt` (the dock hunt
form), which seeds the party back to `WORKER_STEP` before arming the fill and asserts the fill really
moved it — without that vacuity guard the two numbers agree for free. Sabotage-verified by restoring
the compose-then-clamp order: exactly the crew claim fails (`chart 1, stepper 2`) with the vacuity
guard still green.

### THE GROWTH CURVE IS SAMPLED, AND THE CLIENT INTERPOLATES IT

`ForagePatchState.regrowthSamples` / `HerdTelemetryState.regrowthSamples` — the source's own per-turn
biomass delta at evenly spaced fractions of `K`, decoded as `regrowth_samples`. **This is the OTHER
half of the boundary this client already sits on**: where a closed form exists the sim ships the terms
and `SourceForecast` evaluates it (the escapement ceiling), where one does not the sim ships answers
and `SourceForecast.regrowth_at` **interpolates** between them — never fits, extrapolates or smooths.
The curve is two different functions (a patch is logistic with a reseed floor and no Allee term; a
herd has critical depensation below `collapse_fraction`), so a GDScript copy would drift invisibly:
a wrong curve still looks like a curve.

- **The animal curve goes NEGATIVE below the Allee point and the plant curve never does.** Render the
  negatives as DECLINE. Clamping them is the instinctive thing to do with a chart and it draws a herd
  crashing to extinction as a herd sitting still — the asymmetry that makes floor `0` END a herd and
  only set a patch back. `floor_chart_herd_allee` is that frame, beside `floor_chart_drawn_down`'s
  patch flattening onto its floor; the pair of assertions on it is sabotage-verified against a clamp.
- **The food peak is DERIVED from the samples** (`growth_peak_fraction`, the argmax), never restated
  as `FLOOR_FOOD_PEAK` beside them: one number derived two ways is how the mark and the curve start
  disagreeing the first time either moves.
- **`project_stock` does NOT quantise the take to whole animals.** A hunt take is
  `floor(ceiling / bodyMass)` — not linear, so the sim owns it — and what the chart draws is the
  crew's smoothed carry against the source's own growth: a projection, not a promise.
  `PROJECTION_HORIZON_TURNS` is the chart's x-axis and the verdict's patience in one number, so the
  drawn curve and the sentence beneath it agree by construction.

### THE TWO CREW TARGETS (§7.6), AND THE TERM THEY DIVIDE BY

A floor and a crew are independent statements, so "how many workers" has **two** answers and both are
stated and clickable:

| target | expression |
|---|---|
| ***clear it now*** | `max(max(0, B − f·K) ÷ min(perWorkerBiomass × dip, bodyMass × engageRate × dip), crew_that_reaches)` — closed form, deliberately not rounded to whole animals: this is a count of hands, and a crew that over-carries simply finishes the draw. **The floor on the reaching crew is not a rounding**, see below |
| ***hold it after*** | the interpolated regrowth AT the floor ÷ the same carry, **rounded up to one body on a whole-animal source** and then `max`ed with the crew that can REACH that drop — `SourceForecast.take_workers`, the ONE mirror of the sim's `fauna::hunt_take_workers`, which `max_useful_workers` also calls instead of open-coding the same `max(haul, engage)` |

**A TAKE IS BOUNDED BY REACH AS WELL AS BY CARRY, SO BOTH TARGETS DIVIDE BY THE SMALLER OF THE TWO**
(`docs/plan_hunt_through_combat.md` §2). `SourceForecast.engagement_carry` states the engagement stage
in the room's own units — `bodyMass × engageRate × dip`, the biomass one hunter brings into CONTACT per
turn — so the *clear* target stays one quotient and the `min` inside it is the sim's own
`min(carryable, engaged)` read backwards. Reported from play on a Red Deer herd (`bodyMass 15`,
`engageRate 1`, carry 40) with ~16 deer standing: six hunters CARRY sixteen and REACH six, so
`6 clear it now` named a crew that needs three turns — beside a per-turn readout and a worker cap that
had been engagement-aware since the take's own arm landed. **`engageRate <= 0` answers
`ENGAGEMENT_UNBOUNDED`**, and so does a source with no body, so the `min` collapses to the carry and
every forage patch and every pen is byte-identical to before the arm existed (measured: of the 249
`ui_preview` frames, exactly ONE moves — the herd that publishes the field).

**NEITHER TARGET CAN NAME A CREW THE STEPPER REFUSES TO REACH.** Both are clickable, and a click is
clamped to the same cap the `+` obeys — so the cap floors on the *hold* number **and on the reaching
crew** (see §7.2). A clickable target the control beside it cannot reach is the panel arguing with
itself.

**THE QUOTIENT ALONE NAMES A CREW THAT CLEARS NOTHING, WHEREVER THE REGROWTH BEATS THE ROOM.** The
room is what stands above the floor *today*; `project_stock` regrows before it takes, so a crew that
cannot out-carry the regrowth across the band it has to cross never empties the room — not this turn,
not in any number of turns. Where the regrowth is the larger of the two, the bare quotient is
therefore **smaller** than `crew_that_reaches`, and it is the more attractive of the two pills.
Reported from play (`forage_build_crew`): `5 clear it now` · `6 hold it after` two lines above *"7
foragers would reach the floor"* — three numbers, no two of which agree. The honest reading of *clear
it now* is "the crew that empties the room this turn", and a crew that never empties it does not
qualify, so the target floors on the reaching crew; the one-turn drain still wins wherever the room is
large, which is the case the label was written for.

**`hold it after` BEING LESS THAN THE REACHING CREW IS CORRECT AND MUST STAY** (6 against 7 on that
frame). Descending, a crew has to beat the **peak** regrowth in the band it crosses; at the floor it
only has to match the regrowth **there**. Two different questions about two different stocks — the
numbers agreeing would be the bug.

`perWorkerBiomass` is a wire field on both source tables and **closed the KNOWN GAP below**: the crew
throughput used to be recovered as `perWorkerYield / provisionsPerBiomass`, which is `0/0` on exactly
the sources that most need it — a sown Field of flax, cotton or hay, and a wolf. With it,
`forecast_inputs` prices every account by the crew that works it, and the `crew_unknown` escape hatch
(and `PER_WORKER_BIOMASS_UNKNOWN` with it) is **deleted**: `expected_yield_account` no longer quotes a
source's whole ceiling in place of a crew term it could not compute.

**A ZERO THROUGHPUT IS A READING, NOT AN ABSENCE.** The patch's term folds in the tile's seasonal
weight, so a dead-season patch honestly moves no biomass per gatherer. `can_price_crew` is the one
guard between it and every quotient; both targets answer `NO_CREW_ANSWER` there and **render not at
all**, rather than a `0` that would read as "nobody is needed". Frame + sabotage-verified assertion:
`forage_dead_season`.

**A VERB THE SOURCE HAS ALREADY BUILT DIPS NO CREW** (`SourceForecast.live_improvement`, read by
`build_dip` and by both compose builders). `ComposeState.seed_*` runs only when the SOURCE changes, so
a composition OUTLIVES the build it named: the turn a Cultivate completes the sim clears the
assignment's `improvement`, the improvement control drops to its DONE label — and the composed verb
sat on unread, multiplying every crew term by the rung's 0.25 `yield_fraction_while_building` and
re-issuing itself on the next commit. Reported from play: a finished Tended Patch reading
`2 foragers · +0.41 /turn` on the card beside a sheet asking for **6 hold it after**, a crew that is
only right if a forager carries ~2 biomass where the sim's own rate says ~6.

- **The wire cannot answer this and the source's own done flags can.** `BuildDips::for_branch`
  publishes `Some(fraction)` for BOTH rungs whatever the source has climbed — only a rung-3 *managed*
  forecast carries `NOTHING_LEFT_TO_BUILD` — so a positive fraction is not "a build is available
  here". The sim's `BuildDips::of` already states the rule in prose (*"a crew standing on a finished
  source is harvesting, not preparing"*); `live_improvement` was that rule, client-side, over the
  `improvement_is_done` test the improvement control ALREADY made to render DONE instead of RUNNING.
  Before it, the panel said the build was over and priced it as running. **Its successor asks the
  METER rather than the done flags**, which answers this case and the one no flag can reach — a rung
  that has eroded back below its cost and is building AGAIN while its ground is still tended.
- **THE SEASONAL WEIGHT IS NOT A SUSPECT HERE, and it is worth knowing why.** A 4× on the crew looks
  like a seasonal term (`perWorkerBiomass` is `per_worker_biomass_capacity × seasonal_weight`), but
  worldgen sets every food module to `INITIAL_SEASONAL_WEIGHT` **1.0** and no system moves it, and the
  sim threads the one `FoodModuleTag::seasonal_weight` into `forage_per_worker_biomass`,
  `forage_take`'s worker cap and the snapshot alike. A live patch publishes **8.0**; any other carry
  the panel divides by came from the dip.
- **The assertion is a CROSS-CHECK against the sim's own published take, never a restatement of the
  crew math.** Both halves of the sheet dipped together, so any test comparing the sheet with itself
  passes with the bug fully restored. `forage_stale_verb` therefore states the standing assignment's
  rate FROM THE TILE'S OWN WIRE TERMS (`min(crew × carry, regrowth) × rate`, the way `forage_take`
  composes it) and requires the *hold* target to price a forager no slower than that rate implies.
  Sabotage-verified: it comes back with exactly the played numbers, `HOLD 6` and `5.92 vs 1.97
  biomass/forager`.
- **The fixture is at the SHIPPED constants on purpose** — K 195, the 8.0 carry, `r` 0.25, a basket
  converting at 0.03325 food/biomass — because the defect is only visible at a live patch's
  proportions: `crew_to_hold` divides a regrowth the LAND owns by a carry the CREW owns, and a fixture
  whose regrowth is small beside its carry rounds the whole 4× away (`forage_cultivate_done`, which
  stages the same stale verb, reads `1 hold it after` either way).

### THE ANIMAL TAKE IS QUANTISED **AFTER** THE DIP, SO A BUILD MOVES THE WASTE LINE

> **⚠ SUPERSEDED BY "THREE CREWS PER SOURCE" (`docs/plan_standing_upkeep.md` §2.2).** Everything below
> about the during-build dip describes a model where ONE crew both gathered and built. A source
> carries three independent allocations now: a build takes NOTHING off what the gatherers carry, and
> `build_dip` / `<rung>BuildFraction` / `crew_needed` are all deleted. The passages are kept because
> they still record *why* each surface reads the way it does — read them against that section.


`SourceForecast.herd_axis_rates` is the hunt sheet's one resolution of "which component does this
species pay, and at what rate" — and it took `forecast_inputs`' `IMPROVEMENT_NONE` default, so every
take composed from it (`_hunt_take_rate`, `_hunt_delivered_and_waste`, `_hunt_avg_window_turns`) was
priced **undipped** while the worker cap, the chart, both crew targets and the improvement control's
own deal line — which all carry the composed verb — were not. A herd mid-Tame or mid-Corral quoted
~2× (the animal rungs' `yield_fraction_while_building` is **0.5**, not the plant rungs' 0.25) what the
sim would pay, and the sheet contradicted itself inside one card: the deal's *while building* term and
the readout's take are the same quantity and disagreed. `improvement` is now a **required** parameter
there, so no call site can take the identity by omission. (That the two are ONE quantity is also what
later retired the deal LINE — see "THE PAYOFF LIVES IN THE READOUT"; the readout's own row is the
only place that quantity is stated now, so the disagreement is no longer expressible.)

- **THE DIP MULTIPLIES THE COLLECTION, AND `quantise_animal_take` RUNS AFTER IT.** The sim composes
  `collection = workers × per_worker × build_dip` and *then* takes
  `killed = min(affordable, max(1, carryable))`. That `max(1, …)` is why a build is not a scaling on
  this web: a dipped crew that can no longer carry a whole body **still kills one and wastes the
  rest**, so the take falls by less than the dip while the WASTE percentage appears from nothing. A
  dip applied to the ceiling, or to the delivered figure after quantisation, produces a number that is
  wrong in a way that still looks plausible.
- **The SUSTAIN reference stays at `IMPROVEMENT_NONE`, stated out loud** (`_hunt_yield_model`). It is
  the line the take is *judged against* — the herd's own renewable yield, a fact about the animals —
  and a bar that moved with the crew's own dip could not judge that crew. Mechanically the ceiling it
  reads back is dip-invariant anyway (the dip rides `per_worker` alone), so the argument for writing
  the argument is legibility: without it the site reads as the one call the pass missed, and the
  "obvious completion" would quietly move the reference onto the dipped vector's axis.
- **The overdraw GATE takes the live verb too.** `_herd_take_draws_down` was deliberately asked
  undipped "to match its undipped takes"; that premise died with them, and a gate walking a crew four
  times the one being quoted reports a drawdown the sheet does not claim.
- **The frame is `herd_build_crew` + `herd_build_crew_none`, judged as a pair** — a Steppe Runner
  mid-Tame (the roster's heaviest **tameable** animal; the heavier mammoth is `wild`-ceilinged and can
  never be tamed, so a Tame composed on one would stage a build the sim refuses) at the shipped rates,
  with four hunters carrying one whole body undipped and two thirds of one under the build. It reads
  `≈1 Steppe Runners/turn · ⚠ OVERDRAWS THE HERD` hunting and `≈0.67 · RENEWABLE · ⚠ 33% WASTED`
  gentling. The herd's regrowth deliberately sits **between the two carries**, which is what makes the
  gate's verb load-bearing rather than decorative — and the no-build twin is what proves the fix is
  not simply every number scaled down.

### A HUNT TAKE HAS A THIRD BOUND: WHAT THE PARTY CAN REACH (`docs/plan_hunt_through_combat.md` §2)

`HerdTelemetryState.engageRate` — how many animals ONE hunter brings into contact per turn — is the
third arm of the take beside the stock above the floor and the party's carry, and the client had
neither the field nor the arm. Measured in play on a Wild Fowl herd with **one** hunter: the compose
sheet read **≈307 birds/turn** where the sim pays **ten** (`1 × engage_rate 10`), and said *"max 2
workers useful here — more would be idle"* while ~470 birds stood above the floor and each hunter
reached ten. The number was 30× out and **the advice was backwards** — the very hands the take was
short of were the ones being called idle.

The two halves land in `SourceForecast` and are the client mirror of three `core_sim/src/fauna.rs`
functions, named for them so the pairing is legible: `animals_engaged` (the sim's own), `engage_workers`
(`hunt_engage_workers`) and `take_workers` (`hunt_take_workers`, the `max(haul, engage)` the worker cap
and the *hold* target both size themselves with). Two units adapters sit beside them and hold the ONE
composition of the `engageRate × dip` pair: `engagement_per_worker` (animals a worker reaches) and
`engagement_carry` (that same reach in BIOMASS, which is what a crew target divides by).

```text
reach(workers)  = floor(workers × engageRate × dip) × <account>PerAnimal
engageCrew      = ceil((floor(ceiling / bodyMass) + 1) / (engageRate × dip))
```

- **`engageRate <= 0` MEANS UNBOUNDED, never "reaches nothing"** (`NO_ENGAGEMENT_STAGE`). It is the
  wire's finite reading of the sim's `f32::INFINITY` — a **pen** publishes it (a penned animal is not
  stalked) and the whole **plant web** gets it by never publishing the field — so both consumers drop
  the term and forage and corrals are byte-identical to before the arm existed. Getting that backwards
  regresses two whole webs, which is why the frames are an A/B on ONE herd with only this field moving.
- **The dip rides engagement exactly as it rides carry.** Omitting it re-opens the closed defect where
  a building crew and a harvesting crew reach the same count and the build is free.
- **THE TAKE'S ARM IS APPLIED TO THE ANIMAL COUNT, NOT TO `collection`.** `_hunt_delivered_and_waste`
  mins `animals_stayed` into its own kill count; `floor(min(carry, engaged × fpa) / fpa)` is the same
  arithmetic but divides a product of `fpa` BY `fpa` and can land a whole engagement one animal short
  on a rounding.
- **THE `max(1.0)` SITS ON THE CARRY ARM ALONE, INSIDE THE `min`** — the sim's own
  `killed = affordable.min(carryable.max(1.0)).min(brought_down)`, and the reason
  `_hunt_delivered_and_waste` is ONE expression rather than a carryable-versus-partial-body pair. A
  party that cannot carry a whole animal still kills one and wastes the rest, which is a fact about
  the PACK; a party that brings down three quarters of an animal has brought down three quarters of
  an animal and floors at nothing. While the carry quotient was the only arm that could go below one
  body the two were indistinguishable, so a `carryable < 1` branch could price delivery as the crew's
  whole raw `collection`. With the engagement arm in the same `min` that is false, and it read as a
  cliff the wrong way up: a Wild Boar crew of six (`engage_rate` 0.33, `stay` 0.75 ⇒ 1 engaged, 0.75
  stayed) quoted **4.80 food/turn** — its entire carry throughput, twenty boar, for a take of 0.18 —
  then FELL to 0.36 at seven hunters, the readout dropping as the crew grew. Guarded by
  `chapters/hunt.gd`'s `_engagement_quantisation_assertions`, PNG-less and driven: the played pair by
  equality, plus MONOTONICITY over 1..12 hunters, which is the property the branch violated and the
  one that catches its return in another species' numbers. **Collapsing the two branches moved no
  frame** — the one expression is byte-identical to the pair everywhere but where the engagement binds
  below one body, and no `ui_preview` fixture is in that state.
- **THE CARRY CLAMP IS CHARGED PER BODY, NOT PER TURN**, and that is the other half of why one
  expression replaces two branches. `killed` is BODIES PER TURN and goes fractional below one — a body
  every `1/killed` turns — but a body lands WHOLE on the turn it drops, and the crew hauls one turn's
  `collection` off it while the rest rots where it fell. So delivery is `killed × min(fpa, collection)`
  and the clamp rides the per-BODY term. Averaging the kill and clamping THAT by the carry credits the
  crew meat no single turn could hold: on a party whose collection, whose ceiling and whose 0.6-body
  cadence all coincide it reads the full ceiling with no waste, where the honest answer is `0.6 ×
  0.6 = 0.36` of a body's food and 40% wasted — 1.67× too high, and silent about the meat on the
  ground. **A CEILING BELOW ONE BODY IS SMOOTHED RATHER THAN ZEROED**, which is where the sheet parts
  from `quantise_animal_take` (it floors the room to whole animals first, so `affordable < 1` ⇒ no
  take at all): the `now → after` row needs the fractional reading, the holding ceiling being one
  turn's regrowth and routinely a fraction of a body. Guarded by `chapters/hunt.gd`'s cadence herd,
  whose three arms coincide at 0.6 of a body with the engagement UNBOUNDED — the one shape that
  isolates the carry clamp from the arm above it.
- **The reach arm had to reach BOTH producers, and only one of them is `expected_yield`.** The hunt
  sheet's per-turn row comes from `_hunt_delivered_and_waste` (which composes its own `collection` so
  it can quantise), not from `expected_yield_account` — so an arm added to the shared layer alone would
  have left the *rendered* number carry-bound while the cap beside it moved. `herd_axis_rates` carries
  the `engage_rate`/`dip` pair for exactly that reason.
- **`expected_yield_account` takes the account's own per-animal quantum as a third KEY**, the same
  passed-in-not-switched-on convention its per-worker/ceiling pair already follows. An account with no
  whole-animal quantum (fodder; a source the wire states none for) has **no** engagement arm rather
  than a zero one.
- **Frames: `herd_hunt_engagement_bound` / `herd_hunt_engagement_unbounded`** — one Wild Fowl, one
  hunter, one floor, `engage_rate` the only difference. Bound reads `0.03 FOOD` under *"26 of 47
  useful — free up idle workers to send more"*; unbounded reads `0.80 → 0.05 FOOD` under *"max 2
  workers useful here"*, which is the honest advice for a pen. Both takes are stated by the harness's
  own `HerdFx.hunt_take_oracle` (the sim's `quantise_animal_take` restated in food, now taking
  `engaged`),
  never by asking the sheet what it thinks.

> **THE CHART'S CREW TARGETS AND ITS PROJECTION CARRY THE ARM TOO.** `crew_to_clear` /
> `crew_to_hold` / `crew_that_reaches` and the `project_stock` walk the verdict is written off all
> bound their per-turn take by engagement (see "THE TWO CREW TARGETS" above and `project_stock`'s
> `engage_total`), so the bound frame reads **`47 clear it now`** under a verdict naming the crew that
> could draw the herd down at all — where it used to offer *"2 clear it now"* for a herd two hunters
> would take 47 turns to clear, over a sentence promising the floor next turn. The pair of frames is
> what pins BOTH halves: the bound one asserts the reach quotient and a `slow` verdict, the unbounded
> one that the same herd answers the carry quotient and an `ok` verdict again, so the arm is seen to
> DROP rather than merely shrink. Each half is sabotage-verified against a different mutation —
> carry-only targets fail the two target assertions, a carry-only projection fails the verdict one.

### THE ASIDE'S TEACHING LINE — what the top half of the dial is FOR

**THE PEAK ZONE STATES NOTHING, AND THE ENTRY IS EMPTY IN THE VOCABULARY.** *"The food peak — the most
food this source can pay, turn after turn, forever"* is the definition of the preset the player just
clicked, restated: it names no consequence, offers no comparison and asks for no decision, so a player
who reads it knows exactly what they knew before. It is struck from `FLOOR_ZONE_HINTS` rather than
suppressed per surface — worthless copy is worthless on all five consumers — and the entry stays
present-but-empty so the five zones remain one enumeration: `HudFormat.floor_hint` answers `""` and
every consumer renders no line.

The other four each state something the number does not, and **`strip`'s is load-bearing**: it is the
only place the sheet says floor 0 is irreversible on the animal web, and the reaching verdict drops its
own *"then holds it"* clause there on the understanding that this line carries the consequence.

> **EMPTYING AN ENTRY HERE SILENCES IT ON EVERY CONSUMER — five of them**: the compose readout's aside,
> the expedition compose sheet, the work-row hint, the send-hunt banner and the expedition tooltip.
> That is the intent for a line worth nothing anywhere and a REGRESSION for a line worth something
> somewhere, which is how the peak line was blanked once before, for a reason true of one surface. The
> **EXPEDITION** sheet is where such a blanking surfaces first and it is worth knowing why: it has no
> chart, so the floor hint is the WHOLE of what it says a floor means, and a raid rendered three
> unexplained preset buttons and an empty label. Reported from play.

**Assert the peak's silence as a PAIR with the strip zone's warning**, on the table AND on a rendered
aside: a lone negative is satisfied by emptying the whole table, which is the mistake this pairing
exists to catch. `forage_three_accounts` carries the table + strip half; `herd_hunt_expedition` carries
the rendered half (its trip readout renders no aside at all at the peak, rather than a dashed rule over
empty space).

The aside's other line states the **live learning rate**, and it is the only thing in the client that
makes slice 3 visible to a player: `intensification::learn_multiplier` is `floor / the food peak`, so
the number moves as the floor is dragged and the chart's gradient rail only gestures at it.

- Working the source at a floor above zero → **`SIGNAL` cyan**, `Teaching cultivation at ×1.44 — a
  higher floor teaches faster.` With the build box ticked the tail becomes **`and building at the
  same rate.`** instead, because since slice 3 one multiplier paces the lesson and the build meter
  alike — a builder is told the two move together rather than told again to raise the floor.
- Otherwise the aside's own faint ink, naming **which** of the sim's two non-degeneracy ends this is
  (`docs/plan_harvest_floor.md` §3): `Teaching nothing: nothing is left standing.` at floor `0`,
  where the multiplier itself is zero, and `Teaching nothing: nothing is being taken.` where the
  escapement room is empty and the sim's work predicate is false. **Naming the end is the point** —
  a blank line there would leave the player unable to tell "this dial does nothing here" from "this
  source teaches nothing at all", which are different facts.
- A rung that declares no lesson renders **no line**, not an empty one. `SourceForecast.rung_lesson`
  reads the **standing** rung, highest first, mirroring the sim: the same crew learns Herding on a
  wild herd and Penning on a tamed one, so a herd mid-Corral still teaches Penning while it builds.
- **A LESSON THE FACTION HAS ALREADY LEARNED IS NOT TAUGHT AGAIN.** `rung_lesson` keys off the
  SOURCE's rung and nothing else, so a wild patch went on reading `Teaching cultivation at ×1.00`
  for the rest of the game — reported from play. The line does TWO jobs and only one dies with the
  lesson: since slice 3 one multiplier paces the craft AND the build meter, so a known lesson keeps
  the BUILDING half (`Building at ×1.00 — a higher floor builds faster.`,
  `TEACHING_BUILD_ONLY_FORMAT`) while a build is in flight and renders **no line at all** when there
  is none. The `TEACHING_NOTHING_*` ends are UNLEARNED-only for the same reason: they name why no
  lesson is being earned, which is not a question for someone who already has it.
  - **The track is resolved from the NEXT RUNG UP, not stored beside the word** — the lesson a
    standing rung teaches IS the knowledge that gates the rung above it (a wild patch teaches
    `cultivation`, which gates Cultivate), and `RungGates.RUNG_KNOWLEDGE_TRACKS` already writes that
    mapping down. A second column in `RUNG_LESSONS` would be a second spelling of it, free to drift
    the first time a rung's knowledge is renamed. `SourceForecast.rung_lesson_known` is that
    resolution and takes `knowledge` as a PARAMETER, threaded through `floor_chart_model` from
    `DrawerComposeController`'s `_player_knowledge()`: this layer is all-`static` and holds no
    snapshot, so it must never reach for a faction's tracks.
  - **Assert the PAIR, never the empty half alone** — emptying the line unconditionally satisfies a
    lone negative. The plant A/B is `floor_chart_drawn_down` → **`forage_lesson_known`** (one patch,
    one crew, one floor; only the faction's Cultivation moves) and the animal A/B is
    `herd_corral_gated` → `herd_corral_ungated`. The KNOWN-plus-build half rides
    `improvement_running_plant` / `improvement_running_animal`, asserted as *the word "Teaching" is
    gone* AND *the building sentence is there*. That is also why the chart block re-dials Cultivation
    down to a part-learned value: at the all-complete dial the frames above it leave behind, a WILD
    patch teaches nothing and the live-drag assertion would be comparing two empty strings.
- It is a function of the floor, so it lives in the `_refresh_floor_live` registry with the yields and
  the crew targets.

**The gate reasons dropped their tail clause — on this sheet's terms, not everywhere.** They used to
close with *"faster the more you leave standing"*, which this line now says live and quantified, so
carrying both stated one fact twice in one panel. What a gate reason must still carry is what is
missing, how far along it is, **and the action that fixes it** — the rule recorded above, that naming
a prerequisite alone tells a player a door is locked without saying where the key is. That is why the
remedy stays and only the redundant tail went: the work board, the map marks and the work inspector
have no aside to carry it.

### A KNOWLEDGE gate renders NO improvement control on the compose sheet — and only here

The tail clause went first; the whole reason followed it, for the same reason one step further. On
**this** sheet a knowledge gate's line is both **redundant** and **vacuous**: the aside two rows above
states the identical lesson live and quantified (`Teaching cultivation at ×1.38 — a higher floor
teaches faster`), and the reason's remedy — *forage a wild patch to learn it* — names **the very work
the sheet is composing**. It told the player to do what they were in the middle of doing, under a
sentence that had already said it better.

So `DrawerComposeController._build_improvement_control` drops that reason and, **when it was the only
one, builds no control at all.**

- **The drop and the suppression are ONE change, not a change and a consequence.** Dropping the reason
  and falling through to the OFFERED branch leaves an unchecked, live, clickable box — with its crop
  picker beneath it on the plant web — for a build the sim rejects outright: strictly worse than the
  line that was removed, and it is what the code did before the `return` was added. `ui_preview`
  asserts the absence on both webs (`forage_cultivate_locked`, `herd_corral_gated`), each with the
  crop-list / DONE-label companion that keeps the absence specific rather than a sheet that failed to
  build.
- **The test is STRUCTURAL, never the reason's words.** `RungGates.knowledge_gate_unmet(rung,
  knowledge)` is the same `track < KNOWLEDGE_COMPLETE` test the gate builders make, asked on its own
  over `RUNG_KNOWLEDGE_TRACKS` (one knowledge per transition, §4.3). The builders append the knowledge
  reason FIRST, so the reason to drop is `reasons[0]`. Matching on text would break silently the first
  time a reason is reworded.
- **ONLY the knowledge reason goes.** A SOURCE gate is a fact the player cannot learn anywhere else on
  this sheet and cannot fix by working — this ground will never take seed, this patch is Stressed,
  this animal is not yet tamed — so it still renders and still LEADS the control. `forage_sow_locked`
  is the frame that pins both halves at once: both reasons are live there, and the ground's refusal is
  what the control says.
- **Discovery is the aside's job now, and that is the acceptable trade.** A player on a wild patch who
  has not learned Cultivation never sees the word *Cultivate* on this sheet; what they see is the
  craft being earned (`Teaching cultivation at ×1.00`) beside the faction strip's own progress meter,
  and the checkbox appears the turn the knowledge completes. That is a progression — the lesson is
  named while it is being earned — rather than a permanently greyed row restating a rule the panel
  already teaches. **It only holds because the aside is there**, which is why the absence assertions
  are paired with a live teaching-line assertion in the same frame rather than standing alone.
- **EVERY OTHER SURFACE KEEPS THE FULL REASON.** The work board, the map marks and the work inspector
  have no aside and no floor dial, so the knowledge reason is the only thing that can say what is
  missing and what fills it. `RungGates.forage_gates` / `hunt_gates` are untouched and still answer
  both kinds; the suppression lives at the compose sheet's render, and `knowledge_gate_unmet` has
  exactly one caller for that reason.
- **A consequence worth knowing: the compose sheet can no longer render a SECOND reason.** Each rung
  gates on one knowledge plus at most one source condition, so with the knowledge half suppressed the
  lead line is all there is. `build_improvement_control`'s note slot is still live for the RUNNING
  state's pause line — do not delete it — but no compose-sheet gate reaches it.

**It carries its own meta (`HudWidgets.READOUT_TEACHING_META`), and that is not decoration.** Its
aside siblings — the idle note and the floor hint — move with the floor too, so an assertion that
*the aside changed* is satisfied by either of them and says nothing about this sentence. Measured:
blanking the teaching note entirely still passed a whole-aside comparison. The live-drag assertion
reaches the line by meta and is sabotage-verified against both a blanked note and a frozen one.

### THE SHEET'S LOWER HALF: A ROW-LABELLED CREW LINE OVER A BOUNDED READOUT

> **⚠ SUPERSEDED BY "THREE CREWS PER SOURCE" (`docs/plan_standing_upkeep.md` §2.2).** Everything below
> about the during-build dip describes a model where ONE crew both gathered and built. A source
> carries three independent allocations now: a build takes NOTHING off what the gatherers carry, and
> `build_dip` / `<rung>BuildFraction` / `crew_needed` are all deleted. The passages are kept because
> they still record *why* each surface reads the way it does — read them against that section.


The panel's subject is the chart and the numbers under it. It did not read that way: the three intent
presets rendered at the theme's default size (the largest type on the sheet), the crew was a body-size
heading with the stepper flung to the far right and two full-width boxed targets on a row of their
own, and the take / verdict / notes were three unrelated lines at roughly one size with the floor's
teaching line — the panel's least urgent information — standing between the chart and the stepper as a
wrapped paragraph. Reported from play, against the prototype the panel was built from.

**The type scale is now explicit at every rung, and the presets are the quietest control on the
sheet.** `POLICY_PICKER_NAME_FONT_SIZE` (12) / `POLICY_PICKER_METRIC_FONT_SIZE` (11) keep the
name-leads-numbers-support step they always had, one register lower; a preset the player is not on also
reads `INK_DIM` rather than the `INK` `button_font_color` gives every ghost BUTTON, because a preset is
a shortcut to a value rather than an action. **The improvement control moved with it** — its face
carried NO size override, which said "the same size the stance rungs' names wear" only for as long as
those carried none either, so the tie is written out (`POLICY_PICKER_NAME_FONT_SIZE` on all three of
its label/checkbox states) instead of implied. The two are peer AXES and must not drift apart again.

**The crew is ONE line** (`DrawerComposeController._mount_crew_row`): a row-label in the treatment
every other section label gets (`alloc_section_label`), then the stepper inline at the left and both
targets as **pills** (`HudStyle.apply_pill_button`) in a wrapping flow beside it. The shape carries the
distinction — the stepper is a control you operate, a target is a value you jump to — and a pill's face
is a two-Label stack over an empty-`text` Button for the same reason a rung's is, so **the count a
harness reads lives on `HudWidgets.CREW_TARGET_COUNT_META`**. A `btn.text.split(" ")[0]` on one now
yields `int("")` == 0, which is a REAL reading of that control (*nothing needs clearing*) and would
have passed silently.

**THE BUILD DIP IS STATED ON THAT ROW, beside the two numbers it explains** (`build_crew_dip_note`,
from the chart model's own `build_dip` — one resolution, so the note and the targets cannot come from
two reads). A crew preparing a rung carries its `yield_fraction_while_building` — **0.50 on all four rungs of
both webs** since the plant pair was raised off its legacy 0.25 to match the animals
(`.claude/rules/core_sim/intensification.md`) — so six foragers move 24 biomass a turn where the
patch's published throughput says 48 —
and every impossible-looking number on a building sheet follows from that one factor. The only other
cue was a ticked box further down, which says a build is running and never says its price; without the
line, *"six foragers cannot out-take one patch"* is inexplicable. It renders **only** where a dip is
live, because a line on every sheet claims nothing (frames: `forage_build_crew` /
`forage_build_crew_none`, asserted by presence AND by absence through `HudWidgets.CREW_ROW_DIP_META`).
Its wording deliberately avoids `while building`, and that avoidance has now survived two different
reasons. It was first the DEAL LINE's middle term and the phrase that line was IDENTIFIED by; two
labels on one sheet carrying one phrase is how a search for either finds the other, measured at seven
false failures. The line was then deleted and the phrase served as the harness's needle for its
absence. The deal is BACK in the readout (see "THE PAYOFF LIVES IN THE READOUT" below) and
`SourceForecast.YIELD_ROW_HEADER_WHILE_BUILDING` prints the phrase as the yields caption, so the
collision is live again and the avoidance keeps the crew note about the CREW's carry rather than
about the take above it. The absence needle is retired.

**The readout is one bounded well with four registers** (`_mount_readout`, `HudStyle.readout_stylebox`):

| register | what it answers | treatment |
|---|---|---|
| **yields** | what this crew brings home at this floor, now and once holding | a header (`PER TURN · NOW → AFTER`) over 15px tabular numbers + 10px uppercase account names (`2.26 → 0.42  FOOD`) |
| **deal** | what the rung on the table will pay once it stands | ONE row — a 10px uppercase key beside a 12px `SIGNAL` value — rendering only where there is a rung to state |
| **verdict** | which of the crew and the floor is binding | 12px + the severity dot, colour by severity |
| **aside** | the floor's zone hint and its teaching line | 11px `INK_FAINT` under a dashed rule |

**A reading states its unit and NO destination.** Both accounts land in the working band's own
stores — provisions feed the band, fodder feeds the pens it keeps. A `→ CAMP` tail earned its width
only while the retired trade account was the odd one out, banked to the faction-wide stockpile; once
every account routed alike it was identical words on the readout's widest line, so the suffix is gone
rather than made uniformly true.

**THE RENDER-ONLY-WHERE-THE-VECTOR-PAYS RULE SURVIVED THE RESIZE, because the row set is not composed
here.** `SourceForecast.yield_rows` is the STRUCTURAL half of that rule and is now its one definition —
`yield_components`, `picker_products` and `extractive_take_pair` differ only in how they SPELL a
component and all three iterate it. So a sown hay Field still shows no food row and an inedible quarry
shows no row at all, and the single surviving zero is still `zero_account`'s. A widget that synthesised
a row would put the false `0.00 food` straight back on the loudest line of the panel.

**THE HUNT WEB'S PER-TURN ROW IS AN ACCOUNT, LIKE EVERY OTHER RATE ON THIS READOUT** — `1.23 → 0.07
FOOD` on a deer, and `0.11 HIDE` on an inedible quarry, both through `SourceForecast.yield_rows` and
`YIELD_ACCOUNT_UNITS` — a material's account IS its id, so the table's fallback yields the id as the
unit. (The wolf read nothing at all for the one release between the trade account's retirement and
`material_per_biomass` reaching the wire.) **The
whole-animal reading belongs to the CHART above it** — the escapement curve and its `leave 50% · ≈1
Grey Wolf` handle — and to the raid's whole-trip payload, which has no `/turn` and no `now → after`;
a per-turn row wearing `WILD FOWL/TURN` states a rate in a currency the band's stores do not keep,
and the `per turn · now → after` header over it then keys a number that cannot be spent.
`HUNT_DELIVERED_FORMAT` stays composed from the two halves (`HUNT_ANIMAL_RATE_FACE_FORMAT` +
`_UNIT_FORMAT`) for the one-line preview SENTENCE, where the animal rhythm is the point. Both preview
producers are split the same way: a `_*_yield_model` states the rows, the joined text, the overdraw
flag and the waste note, and `_local_*_preview_bbcode` is a thin formatter over it — one derivation,
two surfaces.

**A HUNT IS COUNTED ON ONE AXIS AND CREDITED IN EVERY ACCOUNT IT PAYS, and the two halves of that
sentence belong to different functions.** `herd_axis_rates` resolves the axis a quantised take is
counted on — it has to, because the whole-animal quantiser divides by a per-animal quantum. **Since
arc #527 that axis is PROVISIONS and nothing else**, there being no second scalar account left to fall
back to for an inedible quarry, so the resolution is an alias rather than a choice. But it constrains
the **count**, not the credit: a ratio is unit-free, so the sim values one quantised take through
`YieldPair::rescaled_to`, and `SourceForecast.rescaled_accounts` is that crossing client-side — the
per-biomass vector as the reference mix, the counted axis coming back bit-identical.

- **`yield_rows` still decides which rows EXIST**, so this is not "credit every account": an inedible
  quarry's provisions rate is a structural `0`, the crossing answers a structural zero, and no
  `0.00 FOOD` appears — while the MATERIAL rows beside it state what the take is actually worth.
  `herd_hunt_pelts_only` is the frame that pins it and `herd_hunt_both_products` its positive twin —
  asserted as a pair, since the negative alone passes on a readout that lost every account.
- **The `after` reading rescales the same way**, or an arrowed row would key one account's holding
  rate beside another account with none.
- **BOTH branches of `_hunt_yield_model` cross the same way** — the quantised take and the smoothed
  degrade path differ in whether the take is quantised, never in what a take pays, and the degrade
  path's sentence goes through `yield_components` for the same reason.
- **The assertion is a CROSS-CHECK, not a restatement**: `ui_preview` recomposes the pair from the
  sim's own two steps — `HerdFx.hunt_take_oracle` for the count, then the species' whole-animal quanta
  (`body_mass_yield`) for the crossing — while the client rescales through the per-biomass vector, so
  the two arrive at one answer by different routes. Sabotage-verified in both directions: dropping
  the crossing fails the both-products pair, crediting both accounts naively fails the wolf's
  no-food-row line.

**THE DASHED RULE IS DRAWN, AND A WIDTH-1 `draw_line` IS INVISIBLE HERE.** Godot has no dashed border
on any `StyleBox`, so `HudWidgets.build_dashed_rule` draws it — but a `draw_line` with an explicit
width builds a QUAD one unit tall, and this client renders through a `canvas_items` stretch at a
fractional scale (~0.78), so that quad covers 0.78 of a device pixel and whether it rasterises at all
depends on where the rule lands. It vanished entirely, and it vanished just as completely painted in
`SIGNAL` cyan — which is what ruled out "too faint". Godot's **thin-line primitive** (`width <= 0`, the
default) is one DEVICE pixel whatever the scale; `draw_rect` of the same height fails the same way. The
rule also needs `resized.connect(queue_redraw)`: a Control draws once on entering the tree, before its
container has laid it out, so the only pass runs at `size.x == 0` and the dash loop never iterates.

**THE EXPEDITION BRANCH TAKES THE SAME READOUT BOX, RE-KEYED FROM A RATE TO A TRIP.** It answered with
one wrapped sentence carrying five facts — *delivers ≈1 Wild Boar over ≈18 turns (2 hunting + 16
travel) · ~1 food · ⚠ 20% wasted* — beside a local sheet that laid the same kinds of
fact out in a bounded well, so one panel read two ways. `_mount_trip_readout` composes the same three
registers:

| register | the raid's version |
|---|---|
| **yields** | header `THIS TRIP`, the ANIMAL count leading in the local hunt row's own idiom (its `YIELD_ROW_NUMBER`/`UNIT` overrides, the quarry as the unit, `YIELD_ACCOUNT_NONE` as the account), then the yield accounts through `SourceForecast.yield_rows`, and the waste on the row's own `waste` slot |
| **verdict** | `SourceForecast.hunt_trip_verdict` — how long the party is away and where those turns go, `slow` past the band's warn line and on an unbounded raid |
| **aside** | the floor-zone hint alone |

**What must NOT carry over is the per-turn framing**, and all three registers are where it would have:
`per turn · now → after` keys a rate and a transition into a holding state, and a raid has neither — it
is one bounded errand whose numbers are taken once. Hence `EXPEDITION_TRIP_ROW_HEADER`, no `after` on
any row, and a verdict about the trip's LENGTH rather than about which of the crew and the floor binds
(a party is fixed at launch; there is no contest to adjudicate).

- **There is still NO chart and NO crew targets**, and those absences are asserted beside the box. A
  raid's trip is the sim's forward simulation, not a per-turn drawdown by a resident crew, so there is
  no floor curve to walk and no holding crew to price — and without the pair, "made it look like the
  local sheet" could quietly come to mean "gave it a chart".
- **The aside carries the floor hint and nothing else.** The local readout's teaching line has no
  counterpart: an expedition accrues no husbandry, the gap `FLOOR_LEARNING_HINT_EXPEDITION` already
  names, so a teaching rate here would quote a multiplier the party never earns. A zone with nothing to
  say renders no aside at all rather than a dashed rule over blank space.
- **The three non-delivering states keep the one-line form** — no estimate at all, a denial quarry that
  pays neither product, a herd stripped to its floor — because each has exactly one thing to say and an
  empty box would read as a raid delivering nothing measurable rather than one being refused.
  `SourceForecast.hunt_trip_delivers` is that branch, and `hunt_forecast_line_bbcode` stays: the
  send-hunt banner is its second caller and keeps the sentence.
- **It re-renders with the whole compose block** (every stepper tick, every preset click) and is
  deliberately OUTSIDE `_register_live` — that registry exists to keep readings alive under a chart
  DRAG, and this branch has no chart.

It takes the crew row as before (the stepper alone — no chart model means no floor axis, hence no
targets to price), so the shared spine is unchanged.

### THE CARD IS AS WIDE AS ITS WIDEST ROW, and `CARD_WIDTH` is only where that starts

`ComposeSheet.CARD_WIDTH` (340) is the sheet's **nominal** width — where a narrow sheet reads and the
floor no sheet goes below. It was never a cap, and treating it as the card's actual width is what
put the third intent preset and the `Hunt Here` button over the card's edge in play. The card is an
`AutoSizingPanel`, i.e. a plain `Control`, so **no child minimum ever reaches it**: pinning its rect
to 340 did not make the content fit 340, it only stopped the card from admitting how wide the content
is (`panel-framework.md` → "`AutoSizingPanel` IS A PLAIN `Control`"). What actually rendered was the
inner `PanelContainer` growing out of the card, while `_place_card`'s right-edge clamp and its
off-screen fallback were still computed from 340.

**Every sheet crossed it; the hunt sheet crosses it hardest.** Measured against the 306px the 340
card left usable (its 13+13 stylebox margins and the scroll gutter):

| sheet | widest row demands | fitted card |
|---|---|---|
| forage, staple patch (`food_tile`) · expedition (`herd_hunt_expedition`) · wolf (`herd_hunt_pelts_only`) | **336** | 370 |
| local hunt, two accounts (`herd_hunt_local_sustain`, `herd_hunt_big_game_window`) | **384** | 418 |
| forage, three accounts (`forage_three_accounts`, `floor_chart_full`) | **529** | 563 |

The binding row is the **intent-preset grid** in every one of them — three cells abreast, each a
preset name over its product line, and a hay-meadow preset carries two accounts
(`0.24 food · 0.40 fodder`) where a staple forage preset carries one. That is the content's honest demand at those faces, so the
card grows to it: `_fit_width` measures the body's rows AND the header row beside them (the header is
outside the scroll and carries the subject's name), and the ceiling is the **viewport** — declared per
fit, for the same reason the height ceiling is (`refit`).

**The scroll gutter is reserved unconditionally.** The sheet's ceiling is the viewport, so window
height turns the internal scrollbar on and off, and the bar is laid OVER the body's right edge rather
than narrowing it — a gutter reserved only while scrolling would clip the widest rows on the frame it
appears and jump the card's width on every fit. Its width is asked of the bar, not named as a
constant, and full-width rows fill it, so it costs no visible asymmetry.

**The assertion is a MEASUREMENT against the card's real width, never against `CARD_WIDTH`** —
`ui_preview._assert_compose_sheet_fits` fails when any row (header included) demands more than the
card's fitted width less its chrome and gutter. Pinning the constant would fail every sheet that
legitimately grew. It runs on nine states spanning all three branches and both webs, and is
sabotage-verified: dropping the `_fit_width` call fails exactly those nine and nothing else. A frame
alone cannot hold this claim in either direction — the overflowing `PanelContainer` renders a
plausible-looking card at the wrong width, which is why the regression reached play.

### THE HEIGHT CHROME IS THE HEADER **ROW**, NOT THE TITLE LABEL

The height fit is the same story one axis over, and it went wrong the same way. `refit` composed the
card's chrome from `_header` — the title label — where the header ROW is that title beside
the ✕ `Button`, and **the button is the taller of the two** (41 against 20 at the shipped faces). So
the chrome ran **21px short** on every sheet, while `_fit_width` had measured `_header_row` all along.
The two fits now read the same node.

**The 21px split into two failures, and the second is the one that reached play.**

- **In a roomy window it cost the card's RECT, not a pixel of render.** `CARD_EXTRA_PADDING` absorbed
  12 of it and the `PanelContainer` — a real `Container` inside a plain `Control` — grew the remaining
  **9px out the bottom of the card**, silently, since a `Control` does not clip. Every frame looked
  right; what was 9px wrong was the number `_place_card` clamps against the viewport and the number
  `fit_to_content` compares to decide anything.
- **In a SHORT window it silenced the scroll.** `fit_to_content` asks "must this scroll?" by comparing
  that same chrome-derived desired height against the room below the card, so an understated desired
  height sails under a ceiling the real content does not clear: the scroll stayed **DISABLED** on a
  sheet that genuinely did not fit, the panel ran out of the card and off the bottom of the screen,
  and `Hunt Here` was sliced. Reported from play. The internal scroll exists for exactly this case
  (the note above records the same button sliced once before, when a fixed 560px `max_height` clipped
  a four-species basket), and a chrome that under-measures is how it fails to arm.

**The assertion measures the PANEL'S OWN MINIMUM, because re-deriving the chrome would agree with
`refit` by construction.** Godot aggregates that minimum from the real children, so it is an
independent answer; a check written out of the same header + separation + margin expression would pass
with the bug fully restored. `ui_preview._assert_compose_sheet_card_holds_its_content` rides on all
nine `_assert_compose_sheet_fits` states and requires the card to be at least as tall as the panel it
draws. It needs no viewport-clamped branch: where a sheet is genuinely taller than the room beneath it
the scroll comes on, a scrolling `ScrollContainer` stops propagating its child's height, the panel's
minimum collapses and the card contains it again — **a card clamped short with the scroll still off is
precisely the failure.**

**THE CLAMPED REGIME NEEDS ITS OWN CHECK, AND A GENEROUS SQUEEZE PROVES NOTHING.** Every rendered
state runs in a viewport far taller than its sheet, so all nine assert only the roomy case.
`_assert_compose_sheet_scrolls_when_clamped` squeezes one of them through `bottom_margin` — the one
term of `max_available` that `refit` does not re-declare per fit, so the REAL `refit` runs with the
REAL chrome and the canvas is untouched (shrinking it would re-render every later frame and cost the
harness its bit-identity reference). **The room it leaves is the panel's own minimum**, because the
window in which a correct and a short chrome disagree is exactly the size of the error: an aggressive
first cut clamped to 200px and **passed with the bug restored** — at that room the sheet scrolls
either way. At `panel_min` the correct chrome asks for `panel_min + CARD_EXTRA_PADDING` and must
clamp; the short one asks for `panel_min − 9` and does not. All three of its assertions (the squeeze
really clamped, the scroll came on, the card holds the panel) are sabotage-verified to fail, alongside
eight of the nine roomy ones.

#### …AND THE HEIGHT MUST BE READ A FRAME AFTER THE WIDTH IS FITTED

`refit` waits a frame *before* it measures anything, for the reason above — and then fitted the
card's WIDTH and read `_body.get_combined_minimum_size().y` in the same pass. Godot's container sort
is deferred, so that height is the **previous** width's wrapping: the same one-frame staleness the
leading `await` exists for, arriving one step further in and after it.

**It was latent until a few words of copy moved a sheet across a line boundary.** Naming the keeping
role on the offered face's standing price (`… · 2 work a turn from Agriculture to hold`) put
`forage_fodder_locked` 19px — one line — over what `refit` had fitted to, and the card rendered
**782px against a panel demanding 789 with the internal scroll still DISABLED**, which is exactly the
state `_assert_compose_sheet_card_holds_its_content` was written to catch. Nothing about that state
is special except that it was the only one within a line of the boundary; every sheet had the race.

`refit` therefore waits a **second** frame after `_fit_width` / `_place_card`, re-checking `visible`
and the two nodes on the way out like the first wait does. Measured across all eleven states that
assertion rides: the mis-fitted one moves to its correct 801, and every other card, panel minimum and
body minimum is unchanged to the pixel — so this is a fit that was wrong on one state rather than a
resize of the family. `_fit_pending` is already false by then, so a refit arriving during the second
wait is not swallowed.

### THE SHEET DISMISSES ON PRESS **AND** RELEASE, BOTH OUTSIDE THE CARD

`ComposeSheet` mounts a full-viewport catcher at `MOUSE_FILTER_STOP` and used to close on
`event.pressed` alone. Its geometry settles **asynchronously for at least two frames** after it
renders: `_body.minimum_size_changed → refit → _place_card`, `refit` re-arms itself, and `_place_card`
has two boundary flips that move the card by hundreds of pixels (the beside-the-anchor → hug-the-left
branch, and the height clamp).

**So a player pressing a control during that window lost their composition silently.** The card moved
out from under the pointer between the frame they saw and the frame they clicked, the press hit the
catcher, and the sheet vanished. The window reopens on *any* later re-render that changes the body's
height — a forecast reply landing, a per-snapshot refresh.

**One condition fixes it**: dismiss requires the press **and** the release, both outside the card. A
press that lands where the card *was* is then harmless. **Deliberately no timer, no frame-count guard
and no "recently moved" flag** — a second mechanism guarding the first is worse than the bug. Escape
and the `✕` are different paths and are untouched.

The pair that matters: a press outside then a drag ONTO the card must not dismiss, and a press on the
card then a release outside must not dismiss. Both are asserted, and the sabotage (restoring
press-only) fails exactly the three negatives while the positive stays green.

> **This is also the `ui_preview` flake, wearing a synthetic pointer.** `compose_band_switch_forage`
> failed and passed clean three times: the harness pressed a rect it had computed, the card had moved,
> the press hit the catcher, the sheet closed, and **five assertions failed as a cascade from one bad
> press** — which read as five independent problems. `_pick_actor_band` now settles first, **asserts
> the press point is inside the card**, waits on `about_to_popup` rather than counting frames, and
> **asserts the sheet survived the press**. That last line is what turns the cascade into one legible
> failure naming the rect it aimed at. The pointer drive itself was NOT retired — driving the real
> control is the whole value of that state, and this repo has been bitten by a faked signal passing
> through a dead picker.

### A RUNG ERODED BELOW ITS COST OFFERS THE `⌃` AGAIN (§4.7)

A Tended patch that decayed even slightly below its cost could never be repaired. `§2.4` says it
should be — *"repairing it is a fresh decision the player makes by putting it back in the queue"* — and
the sim's locks are open now (`intensification.md` → "A RUNG ACHIEVED BUT SHORT IS REPAIRABLE"). The
client's own two suppressions were the last lock: `next_rung_ready` filtered on `improvement_is_done`,
which reads the achieved FLAG (true at 99%), and the work row forced `ready` empty whenever
`rung_in_progress` answered, which it does at any partial meter. **The row read the rung as *done* and
*in progress* at once, and each suppression hid the other.**

`RungGates.rung_has_room` replaces the bare done test, and `_rung_is_an_unordered_repair` clears
`building` for a repair that is undeclared and unqueued — so the existing `⌃` path is restored whole,
with no new glyph and no new slot. **A repair is a climb**; the mark already means *this source can
climb*.

**Two guards the first cut needed, both found by the harness rather than by review:**

- **`improvement_is_done` is also true when a HIGHER rung retires this one.** A Field sown from wild
  ground carries `cultivation_progress == 0` forever, so a naive test re-offered Cultivate on every
  finished Field. The test reads the rung's **own** flag.
- **An absent meter reads 0 and is indistinguishable from "eroded to nothing"**, which put a spurious
  `⌃` on an unimproved patch. It requires `progress > BUILD_METER_UNSTARTED`.

**Plant-only, and the fixture says why**: on the animal web `improvement_is_done` *is* the meter test,
so done-and-short is a contradiction there and no honest fixture can produce one.

> **RETIRED BEFORE IT SHIPPED — a `build_crew == 0` fork on the `-1` face.** An eroded unqueued source
> now publishes no estimate, and it was tempting to render that as *"No estimate"* rather than
> `⚠ Stalled`. It is wrong: `RungDef::build_accrual`'s `eligible` reads the stock against the floor and
> **takes no crew count**, so `-1` means *the gate refused* at any staffing. This client shipped a
> crew-gated version of exactly that once and fixed it; the attempt is recorded at both sites so it is
> not tried a third time.

### THE HEADLINE IS **NEXT TURN'S** TAKE, NOT THIS INSTANT'S ROOM (§4.7)

Reported from play: a patch at **102** against a floor of **103**, regrowing and being harvested back
to 102 every turn. The work board read `+0.96 /turn`. This sheet read **`PER TURN 0.00 FOOD`** and
*"takes nothing until it grows past 103."*

**Both were computed correctly and only one described what happens next.** The board quotes the sim's
forward projection; the sheet quoted the instantaneous room `B − floor × K`, which is empty by one
animal. Ray: *"there really is something to take each turn… It is just taking the math too literally.
It might be better to show what will be taken the 'next' turn."*

- **The caption is `NEXT TURN`** (`NEXT TURN · NOW → AFTER` on the walk) and the figure is
  `expected_next_turn_yield` — what the crew will actually draw. At equilibrium that IS the regrowth,
  which is why it reconciles with the board automatically rather than by a second agreement.
- **The sim regrows BEFORE it harvests** — `advance_forage_regrowth` / `advance_herds` run a whole
  `TurnStage` ahead of `advance_labor_allocation` — so the forward room is the honest basis, not an
  optimistic one. Verified against the schedule, not assumed.
- **ZERO STAYS REACHABLE AND BECOMES HONEST.** A source far enough below its floor that next turn's
  growth will not cross it really does pay nothing, and then the sheet says so with the same *until it
  grows past N* sentence — which is now true exactly when it is shown, instead of whenever the room
  happened to be empty. `forage_at_floor` (`0.15 FOOD`, holding) and `forage_below_floor` (`0.00 FOOD`,
  *until it grows past 50*) are the pair.
- **The at-floor SUPPRESSION went with it.** The `now → after` walk was withheld on a source already at
  its floor, which is why the sheet had nothing left to show; the forward headline is what made the
  suppression unnecessary. They were one defect.
- **The new verdict is `At the floor and holding it — taking only what grows back.`**
- **Scoped to the two ceilings the headline reads.** The floor presets still quote the ROOM (`up to
  +N/turn`, takeable once) and `max_useful_workers` still divides the ROOM — those answer *different*
  questions, and re-pointing them moved a dozen unrelated assertions. The sustainability bar DID move
  onto the take's basis, or a crew taking exactly what the patch offers at the peak tripped
  `⚠ OVERDRAWS THE PATCH`.

> **KNOWN GAP — the hunt web's quantisation was not re-derived.** `_hunt_delivered_and_waste` reads the
> forward ceiling like the plant web now, but its *waste* and animal-count quantisation still work off
> the old basis. Worth a look if a herd sheet at its floor reads oddly.

### THE VERDICT LINE IS THE POINT OF THE REDESIGN (§7.1)

The four-stance picker let a player select Eradicate with one worker and never eradicate anything:
the stance said what was intended and nothing said whether the crew could do it. Crew and floor are
independent statements now, so `SourceForecast.harvest_verdict` compares them and says **which is
binding**, in the raid verdict's own ok/slow/blocked severity vocabulary:

| state | severity | reads |
|---|---|---|
| the crew reaches the floor | `ok` | *Reaches the floor in 9 turns, then holds it — taking only what grows back.* |
| the crew settles short of it | `slow` | *This crew can't draw it that low. It settles at 62% and holds there — 11 gatherers would reach the floor.* |
| the crew reaches a floor NOTHING GROWS AT | `ok` | *Reaches the floor in 2 turns.* — no second clause |
| nothing stands above the floor | `blocked` | *Already at or below the floor. This crew takes nothing until it grows past 98.* (a herd: *…past ≈11 Red Deer*) |
| no crew at all | `blocked` | *No one assigned. Nothing is taken and it grows back on its own.* |

**A VERDICT MAY NOT PROMISE AN AFTERMATH THE SOURCE CANNOT REACH.** Reported from play: a Rabbit
Warren at `Take everything` read `0 hold it after` beside *"…then holds it — taking only what grows
back"*. At floor 0 a herd is gone; there is nothing to hold and nothing regrows, so the panel was
contradicting its own crew target. The clause is DROPPED rather than reworded — what stripping costs
is already the aside's `FLOOR_STRIP_CONSEQUENCE` sentence, and a verdict restating it says one fact
twice. **The discriminator is `regrowth_at(samples, floor) > 0`, NOT the web and NOT floor 0**: a
patch stripped to 0 reseeds from bare ground, so it genuinely holds at 0 paying what grows back and
keeps the full sentence. `floor_chart_model` resolves it from the same samples the projection walks
and `crew_to_hold` divides, so the verdict's promise, the *hold it after* count and the readout's
`after` reading are three consequences of one number.

> The web branch (`kind != SOURCE_KIND_HERD`) is the plausible wrong fix and it **passes both obvious
> assertions** — on a stripped herd and a stripped patch, "is a herd" and "cannot regrow" coincide.
> The case that separates them is a HEALTHY herd at a floor it still regrows at, which must KEEP the
> clause; sabotage found the pair vacuous without it.

The settle point and the reach turn both come off the SAME projection the chart draws.
`crew_that_reaches` names the remedy — a crew must out-carry the largest regrowth in the band it has
to cross, which is a closed form, with a few probe steps past it for "reaches, but not within the
drawn horizon". The floor-0 flavours the prototype spells out are deliberately NOT here: what
stripping costs is already the FLOOR HINT's sentence (`FLOOR_STRIP_CONSEQUENCE`), and a verdict
restating it says one fact twice.

**THE HEADLINE TAKE IS A BURST, NOT A RATE, AND THE ROW NOW SAYS BOTH.** `expected_yield_account` is
`min(workers × per_worker, ceiling)` where the ceiling is the ROOM above the floor — everything
standing there, takeable ONCE. A crew big enough to clear that room in a turn or two therefore had a
one-off quantity labelled `/TURN`, and on a full patch the headline could be 5× the rate the source
actually sustains. Each account states `now → after`, where `after` is the same `min` against
`hold_ceiling` (one turn's regrowth at that floor, through the same per-biomass vector).

Four things follow, and each is load-bearing:

- **The second reading rides the SAME row, per account.** The three accounts are one biomass flow
  through a fixed vector — check any picker: `9.16 / 7.64 / 6.24` then `4.58 / 3.82 / 3.12`, the same
  ratio halved — so a second ROW of three numbers would carry ONE new fact in three slots. The
  comparison is also per account, so the two numbers touch.
- **It renders only where the crew REACHES the floor**, gated on the walk's `reached_turn` — the same
  walk the verdict one line down narrates ("Reaches the floor in 3 turns"). A crew that settles short
  never enters the holding state, and promising it a held rate is the failure this reading exists to
  fix. **And only where no BUILD is composed** — see "A COMPOSED BUILD SUPPRESSES THE FLOOR WALK"
  below. Both tests are the yield models' (`_walks_to_the_floor`), so the rows and the caption over
  them answer one question.
- **"Differs from the take" is asked of the FORMATTED strings, because that is what an arrow claims.**
  `yield_rows` drops the second reading where the two are equal — an arrow to itself is noise — and
  that test read `is_equal_approx` on the raw floats while the row renders through `format_magnitude`
  at `YIELD_DECIMALS`. Any pair closer than the display's own resolution therefore drew `0.26 → 0.26
  FOOD`, reported from play beside a second account correctly reading `0.90 → 0.87`. It is the same
  mistake `COMPONENT_RENDER_MIN` records one function along: **a gate finer than its formatter's
  resolution admits exactly what it exists to stop.** Driven, PNG-less, in `chapters/improvements.gd`
  — a rounding pair and a visible pair asserted together, since suppressing every arrow satisfies the
  negative alone, plus a precondition that the rounding pair really is two different floats.
- **A managed rung-3 source has no burst** — the sim never draws a Field or built Pen down, so its
  `hold_ceiling` IS its ceiling and one reading renders.
- **The unit is hoisted into the header.** Three `/TURN`s were the widest thing on the row and it
  could not afford them once each account stated two numbers. Hoisted, not deleted: a preset's tooltip
  states `up to +0.60/turn` for the ROOM, so something has to mark which kind of number this is. The
  header doubles as the arrow's key, in the crew buttons' own two words.

**THE PRESET FACES STATE NO NUMBER**, only the intent (`♻ Best harvest`); the metric kept its tooltip.
Nine numbers stood across the top of the sheet and every one misled: they are the ROOM (one-off) over a
row of per-turn rates; they rank the presets BACKWARDS from the decision (`Take everything` reads twice
`Best harvest` while paying ~nothing forever); they are in food/fodder units directly above a
chart whose axis is BIOMASS; and they are worker-independent, so they alone sat still while the sheet
under them moved with the stepper.

**A STOCK IS NOT A RATE.** `SourceForecast.format_stock` prints whole biomass, matching the tile
card's own `Foraging 35 / 100`; `format_magnitude`'s two decimals are the food-RATE rule and spending
them on a stock prints `1075.00`, claiming precision the number lacks.

**A HERD'S STOCK IS COUNTED IN ANIMALS, AND ONE FUNCTION SAYS SO.** `SourceForecast.stock_face`
renders a standing quantity in the unit its source counts in — `98` for a patch, `≈11 Red Deer` for a
herd (`animal_count`, floored at one body). **Both surfaces that name the floor's THRESHOLD read it
from there**: the chart's flag and the at-floor verdict beneath it. They are two statements of one
number and they diverged the moment the flag learned to count animals while the verdict went on
quoting `grows past 1075` — caught in a rendered frame, not in review — so the cure is that no second
rendering exists, not two kept in step by hand. `floor_chart_model` binds `body_mass` / `quarry` once
and passes them BOTH to `harvest_verdict` and out on the model, for the same reason.

**THE FLAG LEADS WITH THE PERCENT, ON BOTH WEBS** — ONE `FLOOR_FLAG_FORMAT`, `leave 50% · 98` on a
patch and `leave 50% · ≈11 Red Deer` on a herd, and **`HarvestFloorChart` branches on nothing**: it
supplies the order, `stock_face` supplies the unit. **A flag on a draggable control has to move when
you drag it.** Biomass has a value per `FLOOR_STEP`; an animal count over a K of ~21 has ~21, so an
animal-first flag sits unmoved across a tenth of the drag and reads as a stuck control. Once the
percent must lead for that reason on fauna, leading with it on flora costs nothing and stops one
control swapping its terms when the player clicks from a patch to a herd. It is also the honest
order: the sim's floor IS a K-fraction (`B − floor·K`, quantised later at the kill) and
`classify_ecology_phase`'s cut points are fractions of that same K, so the percent is the axis and
the quantity is the gloss. `≈` is the vocabulary the rest of the sheet already uses for a rounded
animal count. Both webs are asserted, with `==` rather than `contains` — `contains` passes on either
order, and without the patch's line the suite could not tell "fauna converted" from "everything
converted".

### §7.2: THE HOLD NUMBER IS THE CEILING ON USEFULNESS, AND IDLE CREW IS REPORTED, NEVER RELEASED

**`max_useful_workers` FLOORS ON BOTH PROJECTION-DERIVED CREWS** (`forecast_inputs` carries them as
`hold_crew` / `reach_crew`, from the same `crew_to_hold` / `crew_that_reaches` the chart's targets and
verdict render):

```text
max_useful = max(ceil(room / (carry × dip)), hold_crew, reach_crew, <the caller's crew floors>)
```

The two terms answer different questions — the quotient is *"clear the room standing THIS turn"*, the
hold crew is *"take the regrowth EVERY turn"* — and the quotient cannot bound the other. The limit
case is the proof: a source sitting exactly at its floor has no room, so the quotient is `0` and the
cap read *no workers are useful here* beside a positive *hold it after* the stepper then refused to
reach. Telling a player to drop a crew they need next turn is the same arithmetically-defensible,
practically-false answer the verdict line exists to remove. It is folded in **inside
`max_useful_workers`**, not at the call sites, which is what keeps the two cap twins
(`source_worker_cap_state`, `DrawerComposeController._forecast_worker_cap`) unable to gate
differently — the failure this file already records once, when a floor reached only the compose side.
A rung-3 managed source is excluded (`hold_crew` answers 0): the sim never draws a Field or a built
Pen down, so its cap stays `production / perWorkerYield`. Frames + assertions: `floor_chart_full` (no
room, cap 1, the stepper renders that 1) and `herd_hunt_pelts_only`, where the wolf read `5 hold it
after` under `max 4 workers useful here` and the press of the target is now asserted to land the
stepper on 5.

**`reach_crew` IS THE SAME ARGUMENT ONE STEP ALONG, and it is what makes the *clear* target
clickable.** Where the regrowth beats the room the crew that draws the source down is LARGER than the
one-turn quotient, so the cap reported those hands useless while the verdict beside it was naming them
as the remedy — and *clear it now*, now floored on that number, would have named a count the `+`
refused. It is a floor for its own sake too: hands between the quotient and the reaching crew draw the
stock further down every turn instead of settling above the floor, which is strictly more than the
quotient's crew achieves. Frame + assertion: `forage_build_crew` (`7 clear it now` under `max 7 workers
useful here`, the press asserted to land the stepper on 7).

**BOTH PROJECTION-DERIVED FLOORS CARRY THE ENGAGEMENT PAIR, and that is what keeps this promise true
on the animal web.** `hold_crew` / `reach_crew` read `engageRate` off the SOURCE and hand it to
`crew_to_hold` / `crew_that_reaches` with the dip, so the numbers the cap floors on are the same ones
the two pills render — a cap floored on a carry-only reading beside an engagement-aware target is the
same "clickable target the stepper refuses" defect one layer down. A patch publishes no such field and
a pen publishes `NO_ENGAGEMENT_STAGE`, so both floors answer exactly what they answered before.

Workers above the *hold* number contribute nothing once the source is holding at its floor, and they
are **still never released**: at-the-floor is the most **reversible** condition in the model — drop
the floor, or let the season move the hold number, and they are wanted again — and this repo only
rewrites an assignment for PERMANENT conditions (an out-of-range lapse, a completed build retiring
its verb).

**The panel no longer NARRATES it.** `2 of your 3 foragers go idle once it is holding — only 1 can
carry what grows back` was arithmetic over two numbers a centimetre above it: the stepper's count and
the *hold it after* pill's. That pill is also a BUTTON that sets the count, so the remedy was never a
sentence away either. The cap still floors on the same number, so "idle" still means *above the hold
crew* wherever it is computed.

### BARREN MEANS BARREN ON EVERY ACCOUNT — and the axis alias is what broke that

`max_useful_workers` divides by the axis pair and returns `MAX_USEFUL_BARREN` (1) when the axis prices
no crew. **That test was written when the axis was a CHOICE** (issue #337: `axis_per_worker` resolved
to whichever of provisions/trade the species actually paid, so an inedible quarry was capped on the
account it pays), and **arc #527 retired the trade half without the test noticing** — the axis triple
became a plain alias of the food pair, so "the axis prices nothing" quietly became "this source pays
no food", and the barren branch began firing on every hay meadow, flax stand and tobacco patch in the
game. Reported from play on a wild basket of Tobacco 56% + Hay Grass 44%: `max 1 worker useful here —
more would be idle`, printed beneath that same sheet's `13 clear it now`, its `2 hold it after` and a
verdict naming 2 foragers as the remedy, with the `+` dead at 1. Four numbers, no two agreeing, and
the one the stepper obeyed was the wrong one.

**So the branch asks the other accounts first.** `off_axis_useful_workers` is the same saturating
quotient asked of the FODDER pair and of the per-material vector row by row, and it answers
`NO_CREW_ANSWER` only when nothing off-axis prices a crew either — which is the one reading that means
*barren*. Three properties are load-bearing:

- **The MAX across accounts, never the min or the first.** The cap says *beyond this crew nobody adds
  anything*, so it is the largest crew any single account can still use. On a wild source every
  account is one biomass flow through a fixed per-biomass vector, so the quotients agree and the `max`
  is free; on a rung-3 managed source the payoffs are independent and it is doing real work.
- **The material vector is asked ROW BY ROW and unioned BY ID**, the standing rule for that account —
  a summed materials/turn figure is the retired trade axis under a new name.
- **The two projection-derived crew floors resolve ABOVE the branch**, so a no-food source keeps §7.6's
  promise that neither crew-target pill may name a crew the stepper refuses. They are denominated in
  BIOMASS, so they answer for a hay meadow exactly as for a wheat one. **The barren answer itself takes
  no floors** — a source can price a crew (`per_worker_biomass > 0`) while paying into nothing at all,
  and flooring that on the targets would staff hands against a take of zero, which is the parking
  `MAX_USEFUL_BARREN` exists to refuse.

**The pair is the assertion, and both halves live in `ui_preview`**: `forage_no_food_basket` (the
reported tile — the cap clears 1, reaches both rendered targets, and the *clear it now* press lands
the stepper there) and `forage_dead_season` beside it in the same frame's claims, still capped at 1.
"Not barren" is trivially satisfied by a cap that stopped answering at all, so neither claim is worth
anything without the other.

### ONE HINT RULE, NOT TWELVE ROWS

`FORAGE_POLICY_HINTS` / `LOCAL_HUNT_POLICY_HINTS` / `SEND_HUNT_POLICY_HINTS` are **deleted**. A stance
table had a row per name; a floor has no names, so the only thing sayable about a value is its
position relative to the **food peak** — which is the whole meaning of the dial.
`SourceForecast.floor_zone` is that classification and `HudFormat.floor_hint` composes the sentence:

| zone | glyph (`FoodIcons.FLOOR_ZONE_ICONS`) | reads |
|---|---|---|
| `strip` (= 0) | 💀 | take everything — **plus the web's own consequence** |
| `drawdown` (0 < f < peak) | ⇊ | spending the source's future for calories now |
| `peak` (≈ 0.5) | ♻ | the most food, forever |
| `learning` (peak < f < 1) | ⬆ | buying ladder progress with calories |
| `untouched` (= 1) | ⊘ | nothing taken — and a crew with nothing standing above its floor learns and builds nothing |

Only **two** facts are composed in rather than tabulated, and both are real: what STRIPPING costs
differs by web (`FLOOR_STRIP_CONSEQUENCE` — a patch reseeds from bare ground, a herd is gone for good)
and a detached party accrues no husbandry, so the learning zone's promise is false for a raid
(`FLOOR_LEARNING_HINT_EXPEDITION`). Three glyphs are inherited verbatim from the stances they replace
(⇊ ♻ 💀 already meant their zone's thing and are legibility-proven at 12–13px); ⬆ was Surplus's and now
reads as RAISING the floor, which is safe because nothing renders both vocabularies.

**The knowledge gate-reasons name the floor too.** `intensification::learn_multiplier` is `floor /
the food peak`, so "leave more standing" is literally how you learn faster and there is no rung to
name: `Your people know Herding 45% — ♻ hunt a wild herd to learn it, faster the more you leave
standing`.

### Where a floor is READ, and where it is MARKED

- The compose sheets: `ComposeState.forage_floor()` / `hunt_floor()` (floats, clamped on the way in),
  seeded from `HudBandLaborState.floor_for_forage` / `floor_for_hunt`.
- A worked row / map yield label: the assignment's own `floor`, marked with its ZONE glyph
  (`FoodIcons.for_floor_zone`). **A continuous number cannot wear one glyph per value**, and the zone
  is the whole of what one mark can honestly say; the exact percent is in the tooltip and in the work
  inspector's sentence (`50% left standing`, `HudComposeVocab.FLOOR_VALUE_FORMAT`).
- A party row: `expedition_floor`, same glyph, gated on the hunt MISSION — a scout reports `1.0`,
  which is a real zone but not one it chose.

### The commands

`assign_labor <f> <b> forage <x> <y> [floor] [species] <workers>` / `… hunt <herd_id> [floor]
<workers>`; `send_hunt_expedition <f> <b> <party> <fauna_id> [floor]`. **The optional token is a
NUMBER**, formatted to `Main.FLOOR_COMMAND_DECIMALS` (2) — never `str(float)`, which would put
`0.30000000000000004` on the line. The four stance words are **rejected by name** at parse
(`CommandParseError::RetiredStanceToken`), so a stale emitter fails loudly instead of being silently
reinterpreted as a crop key; the two optional forage tokens stay disjoint because a floor only ever
parses as a float and a species key never does. `_emit_work_assign`'s `RESTATE_STANDING_FLOOR`
sentinel is outside `0..1` because every real floor **including 0** is a value a crew edit must not
overwrite.

### The raid table is the ONE place the sim still exports rows

A raid's trip length is a bounded forward simulation with no closed form, so there is no expression to
hand over. `huntTripEstimates` **samples** the continuum (`snapshot::RAID_FORECAST_FLOOR_SAMPLES` =
`0.0, 0.15, 0.30, 0.50, 0.80`) × party size.

**BOTH AXES ARE SAMPLED AND BOTH ARE READ AT THEIR NEAREST MARK.** The party axis is
`expedition_config.estimate_party_sizes`, an ascending LADDER (`[1, 2, 3, 4, 8, 16, 32, 64]`), so most
party sizes fall between two rungs; `SourceForecast.nearest_estimate_party` is its named seam beside
`nearest_estimate_floor` and `_row_for_nearest_party` the one resolution the hunt and denial tables
share, **the lower rung winning a tie** (over-quoting a party's take is the more misleading direction).
The launch command still sends the player's exact floor and exact party. Where the quoted rung is not
the selected party the sheet says so — `quoted_party_note`, over `QUOTED_PARTY_KEY` on the forecast —
and the rationale lives in `band-city-panel.md` → "BOTH ESTIMATE AXES ARE SAMPLED".
**The rows are SCANNED, never key-built** — the wire key is `"<floor>:<party>"` with the floor rendered
by Rust's `f32` Display (`0`, not `0.0`), so a GDScript rebuild would have to reproduce Rust float
formatting and a near-miss finds nothing *silently*. The decoder therefore also inserts `floor` and
`party_workers` as FIELDS on each row.

#### `turnsToFill == 0` means `horizon` AND NOTHING ELSE

`HuntTripForecast::turns_to_fill` is an `Option<u32>` rendered as `0` on the wire, and the sim reserves
`None` for [`HuntTripBound::Horizon`] alone: a raid that ends by driving the herd extinct reports the
turn it ended on like any other, because the live arm's lost-herd guard turns the party for home in
that same turn. **`SourceForecast.RAID_TURNS_UNBOUNDED` + `raid_is_unbounded` are the ONE reading of
that sentinel**, so the one-line form, the trip verdict and the Send button cannot answer it three
ways — and a `herd_lost` row, which carries a real turn count, can never take a "many turns" branch on
any of them.

**A floor-`0` (`Take everything`) raid is exactly the row that used to publish it**, so a mission whose
whole purpose is to finish by emptying the range read on three surfaces at once as a trip that never
completes: `delivers ≈12 Red Deer over many turns`, `Away many turns — still delivering at the end of
the forecast`, `Send Anyway (long raid)`. It now quotes a real total under
`TRIP_BOUND_CLAUSES[herd_lost]` — *the herd is wiped out before the party's load is made up* — and the
ordinary primary Send. The consequence that follows is correct rather than incidental: the
`Take everything` preset gains a rate in `expedition_policy_takes`. (It also used to bring the FILL
TARGET control back, that lever needing a bounded length to price a step against; the lever is retired
— see "RETIRED — the FILL TARGET".)

**That scan asks the RAID, not the total** (`raid_is_unbounded(cell_hunt_turns)`). Its skip used to
test `turns_to_fill + travel <= 0`, so a horizon cell on a DISTANT herd read `delivered ÷ travel` — a
rate for a raid the sim says never finished, made entirely out of the walk — while the same cell on a
near herd correctly showed none.

**Assert the two as a PAIR.** `herd_hunt_forecast_eradicate` (strip-bare, `herd_lost`, a real total)
and `herd_hunt_forecast_horizon` (a slow breeder at the food peak the party can neither fill nor
exhaust) are the corpus's only delivering rows on the two branches, so *"never says `many turns`"*
would otherwise pass on a client that could no longer say it at all.

#### An unbounded raid quotes a FLOOR, and the floor is `horizon + round-trip travel`

The three surfaces above no longer hedge. `PopulationCohortState.expeditionForecastHorizonTurns` —
`expedition_config.hunt.forecast_horizon_turns`, echoed onto every cohort in the
`expeditionViabilityWarnTurns` idiom and read here through `SourceForecast.forecast_horizon_turns` —
is the scale every "never completed" sentinel on this wire is relative to, and ONE lever serves both
raid tables (the sim's `denial_projection_at` and `hunt_trip_forecast_seeded` read the same field), so
there is nothing for a client to pick wrongly between.

> **IT IS NOT A TRIP LENGTH.** The horizon bounds the **hunting** alone — `turnsToFill` excludes travel
> — and the round trip is a separate, already-known term, so the floor on the whole trip is
> `horizon + round_trip_travel_turns`. The bounded verdict reads *"Away ≈36 turns — 18 hunting, 18
> travel"*; the unbounded one must be a lower bound on **that same span** or the two cannot be compared
> and the player is no better off than with "many". Quoting the bare horizon understates the trip by
> the entire walk, and **a number wrong in the reassuring direction is worse than the hedge it
> replaces**.

`hunt_trip_forecast` composes both floors on the long branch — `RAID_HUNT_TURNS_FLOOR_KEY` (the
horizon) and `RAID_TURNS_FLOOR_KEY` (that plus the travel it has just added) — and
`raid_floor_is_known` is their ONE reading, the `raid_is_unbounded` rule one layer along: the line, the
verdict and the button must not answer three ways. The copy:

| surface | bounded | unbounded |
|---|---|---|
| one-line form | `delivers ≈5 Wild Boar over ≈12 turns (7 hunting + 5 travel)` | `delivers ≈6 Steppe Bison over more than 68 turns (more than 60 hunting + 8 travel)` |
| trip verdict | `Away ≈12 turns — 7 hunting, 5 travel.` | `Away more than 68 turns — more than 60 hunting, 8 travel. Still delivering at the end of the forecast.` |
| Send button | `Send Anyway (≈54 turns)` | `Send Anyway (more than 68 turns)` |

The hunting half wears *"more than"* and the travel half does not — travel is exact, and hedging it
would claim less than the client knows. `TRIP_BOUND_CLAUSES[horizon]` still renders NOTHING, on the
understanding that the verdict itself carries *"still delivering at the end of the forecast"*.

**A cohort carrying no horizon keeps the hedge** (`FORECAST_HORIZON_UNKNOWN`, `0` — the sim pins the
published value positive). *"More than 0 turns"* is the one reading worse than *"many"*, so the
`*_NO_HORIZON_*` fallbacks exist for a fixture that predates the lever and for nothing else.

**The frame that can tell the fix from the bug is `herd_hunt_horizon_travel`**, and its pairing half
cannot: `herd_hunt_forecast_horizon`'s band carries no move rate, so its trip is all hunting and
`horizon` and `horizon + travel` are the same number. The travel frame raids the same
never-completing Steppe Bison from the 8-tiles-out band, and asserts all three surfaces by
**EQUALITY** against sentences spelled out in the chapter — a `contains` would pass on a line quoting
the horizon alone, those two lines sharing every word. Sabotage-verified by returning the bare
horizon as `turns_floor`: exactly those three fail, each naming both the wanted and the found string.

### §7.7: a zero belongs to an account the source actually pays

The render-only-when-non-zero rule always kept ONE zero — a component that exists and paid nothing
this turn is worth reading — but *which* component was hardcoded to food. On the animal web that is a
claim the wire contradicts: a wolf's `provisionsPerBiomass` is `0`, it pays pelts and no meat ever, so
`0.00 food` on one is not an empty reading but a **false** one. `SourceForecast.zero_account_of` reads
the per-biomass vector (a structural fact, not this turn's ceiling) and `picker_products` /
`yield_components` / `extractive_take_pair` take it as `zero_account`. A source with no positive rate
in any account answers `YIELD_ACCOUNT_NONE` and its caller renders **no line at all**. Frame:
`herd_hunt_pelts_only` — an inedible quarry whose presets quote **hide and no food**, and no zeros.
`zero_account_of` still answers `NONE` for it (it names which SCALAR zero to print, and a material has
none to nominate), and the material rows are what make the readout non-empty anyway — which is why
`forecast_is_known` takes the material vector as a witness of its own. Asserted as a PAIR with
`herd_hunt_both_products`, whose deer prints a live FOOD line: "renders nothing" and "is correctly
silent" are one picture, and only the paired frame separates them.

### Per-account divergence is GONE on the plant web, and that is the model

A plant take is one BIOMASS quantity through three fixed rates (`forage::forage_take`: *"both operands
are the same biomass through the same rates, so the two components agree on which side binds"*), so
every account overdraws or none does. The `or` in `_local_forage_preview_bbcode`'s verdict is
therefore inert there — kept because it costs nothing and the animal web's quantised take is not
obliged to stay that way. `forage_three_accounts_overdraw` used to pin the divergence; it now pins
that the verdict tracks the FLOOR.

### THE ⚠ IS GATED ON THE PROJECTION, because the ceiling test is a fact about the FLOOR

`_is_overdraw` compares the take against the **food-peak** ceiling — and on a source standing at or
below that peak the ceiling is `0`, so the test degenerates to "something is being taken at a floor
below 0.5". That is a statement about where the dial sits, not about what is happening to the stock,
and the two can disagree outright: reported from play, `⚠ OVERDRAWS THE PATCH` rendered two lines
above *"It settles at 53% and holds there"* — the panel saying the patch falls and grows in one
breath. Both sentences now read the ONE projection: the flag survives only if
`SourceForecast.take_draws_down` says the stock ends the horizon below where it stands today.

- **The gate is subtractive, and answers `true` where there is nothing to consult** — no capacity, no
  published curve, a rung-3 managed source — so a flag is never suppressed on the strength of a walk
  that was never taken.
- **THE GATE WALKS THE VERDICT'S OWN PROJECTION, TERM FOR TERM — the live build verb AND the
  engagement bound.** Both were once left out on the grounds that the gate is subtractive and a
  faster-falling walk only leaves the flag standing, and both readings were wrong for the same
  reason: a flag kept by a projection the panel does not believe is exactly the contradiction the
  gate was introduced to remove. The dip went in when the takes stopped being priced undipped (see
  "THE ANIMAL TAKE IS QUANTISED **AFTER** THE DIP"); the reach arm went in with the crew targets,
  because a party that reaches 1.3 biomass of bird a turn against 2.5 of regrowth is not drawing the
  herd down however much its carry says otherwise — carry-only, `herd_hunt_engagement_bound` would
  fly `⚠ OVERDRAWS THE HERD` above *it settles at 84% and holds there*.
- Frames + sabotage-verified assertions: `forage_build_crew` (rises → no flag) and
  `forage_build_crew_decline` (one more hand, falls → the flag returns). **Both halves, or the first
  passes vacuously on a gate that silenced everything.** The animal web's is
  `herd_hunt_engagement_bound`, asserted as the **EQUALITY** of the gate's answer and the verdict's —
  the pairing IS the claim, so a fixture that stops rising fails nothing while a gate that stops
  agreeing fails at once — over a precondition that the carry-only walk says the opposite (it lands
  on the floor, 0.805 → 0.500, where the bound walk climbs to 0.84).

### CLOSED — the patch's per-worker vector, and the derivation that stood in for it

`ForagePatchState` publishes `perWorkerYield` (the FOOD throughput) and deliberately no
`perWorkerTrade`/`perWorkerFodder`; the per-policy row that carried all three went with the stances.
The client recovered the one biomass throughput all three accounts share as
`per_worker_yield / provisions_per_biomass` — exact, and **undefined on exactly the sources that pay
no food** (a sown Field of flax, cotton or hay grass; a wolf), where it answered
`PER_WORKER_BIOMASS_UNKNOWN`, `forecast_inputs` raised `crew_unknown`, and `expected_yield_account`
quoted what the SOURCE offers rather than multiplying by a throughput it did not have.

**`perWorkerBiomass` on both source tables ended that**, and all three of those names are deleted —
see "THE TWO CREW TARGETS" above. The justification once offered for the field's absence ("a
policy-blind scalar cannot state a policy-dependent rate") described the retired `Deplete` trade
markup; no factor of any kind now rides the depth of the draw, so one scalar states it honestly.

### The fixture adapter, in both preview harnesses

Every fixture states its take as the retired per-stance table, so a floorify adapter converts one to
the other in ONE place per harness (`ForageFx.floorify` for `ui_preview`,
`band_panel_preview`'s own `_floorify`) rather than rewriting ~50 literals. It **pins the old `sustain` row to the food
peak** (Sustain took the renewable yield; the peak is the floor that pays the most forever), so every
frame's headline number at the default floor is what the fixture was tuned to show. Two repairs it has
to make, both because the four-row model let a fixture be internally inconsistent: a source with a
positive Sustain ceiling standing BELOW `K/2` has no escapement room at all (its stock is raised to
`FIXTURE_STOCK_FRACTION` of K), and a source with **no capacity** has no floor axis at all — `max(0,
B − floor·K)` is `B` everywhere when K is 0, so every preset would quote one number and the picker
would silently claim the dial does nothing.

**IT ALSO SEEDS THE GROWTH TERMS AND THE PHASE BANDS NO PRE-4b FIXTURE CAN CARRY**
(`ForageFx.seed_growth_terms`), and the chart needs the first two — without a curve it renders nothing at all, which would silently drop the instrument
out of ~50 frames. `per_worker_biomass` is recovered EXACTLY from the fixture's own numbers where they
can state it (which leaves every existing expected-yield line unchanged) and falls back to the config's
throughput where the recovery is `0/0`; `body_mass` is derived from the fixture's own per-animal /
per-biomass pair on whichever account the species pays, so the whole-animal rounding cannot disagree
with the rates beside it. **THE HARNESS IS STANDING IN FOR THE SIM WHEN IT SEEDS `regrowth_samples`,
and that is the one place a growth model may be written in GDScript** — the constants are the shipped
config's and the two SHAPES are the ones the sim publishes: a patch lifted to its reseed floor and
therefore never negative, a herd declining at `collapse_rate` below its Allee threshold and therefore
negative there. A fixture that flattened that asymmetry would let the chart clamp a herd's crash and
still look right. A fixture that states its own terms keeps them — which is how `_dead_season_tile_fixture`
holds `per_worker_biomass` at a genuine **0**. The phase BANDS are seeded on both webs from
`FIXTURE_COLLAPSE_FRACTION` / `FIXTURE_STRESSED_FRACTION`, and the first of those is the **same
constant the animal curve's Allee branch reads** — deliberately, because it is one field in the sim:
splitting them here would let a fixture draw a chart whose red band and whose crash begin at
different heights, which is exactly the disagreement `floor_chart_herd_allee` exists to catch.

> **A pre-bands fixture may state a phase word its own stock contradicts**, and the zones now make
> that visible: `_dead_season_tile_fixture` reports `collapsing` at `B/K = 0.5`, which sits in the
> Thriving band. The sim cannot produce that pair (`core_sim/tests/ecology_bands_on_the_wire.rs` pins
> the word against the cuts), so it is a fixture artefact, not a client one.

### The 4b frames, and what each breaks differently

A chart compiles, runs, exits 0 and is visibly wrong, so each case is rendered AND looked at. All five
are in `ui_preview`: **`floor_chart_full`** (a floor above a nearly-full stock — nothing to clear, the
cap standing at the HOLD crew rather than collapsing with the room, and the floor flag flipping BELOW
its line) · **`floor_chart_drawn_down`**
(a Stressed patch worked to a floor it reaches, the curve descending and then running FLAT along the
line) · **`floor_chart_herd_allee`** (the herd below `collapse_fraction`, whose curve must fall AWAY
toward extinction — the frame the sampled curve exists for) · **`herd_hunt_pelts_only`** (the wolf:
the readout has no food line and the chart does not care, a floor being a fraction of BIOMASS) ·
**`forage_dead_season`** (`perWorkerBiomass == 0`, so both crew targets are absent rather than zero).
`Q.find_meta_node` / `Readout.crew_target_count` / `Readout.verdict_severity` reach the three new controls by
IDENTITY (`FLOOR_CHART_META` / `CREW_TARGET_META` / `VERDICT_META`) — the chart carries no text at all
and the other two wear faces made of live numbers, so a text match would find nothing and pass.

**A SIXTH CASE — `forage_build_crew` / `_decline` / `_none`, WHERE THE REGROWTH BEATS THE ROOM.** The
played frame: `K 195`, ~9 biomass standing above a 45% floor while the patch regrows ~12, worked by
six foragers at this fixture's quarter carry (`_building_patch_tile_fixture` — the stale-verb patch's
basket and 8.0 carry, and a canned 0.25 that was the ladder's value when the frame was built; the
ladder ships 0.50 now, and the fixture keeps 0.25 deliberately, being a proof about the arithmetic). It is the only fixture
in the set whose crew carry lands just UNDER the source's own regrowth, and all three defects above
need exactly that: the *clear* quotient falls below the reaching crew, the food-peak ceiling is zero so
the ⚠ degenerates, and the dip is what makes the numbers look impossible. **THE DEFECT WAS THAT THE
NUMBERS CONTRADICTED EACH OTHER, NOT THAT ANY OF THEM WAS MISCOMPUTED** — the settle point, the
teaching multiplier, the three account rates and the preset ceilings all check out against the shipped
config, and `6 hold it after` under a reaching crew of 7 is correct (see "THE TWO CREW TARGETS"). So
every assertion here is a RELATION between rendered numbers rather than a literal: *clear ≥ the crew
the verdict names*, the press of that pill lands the stepper on it, the projection rises (no ⚠) and
falls one hand later (⚠ returns), the dip note present with a build and absent without. The fixture
also carries the STANDING assignment, without which the tile card reads `Cultivation Reverting` beside
a sheet composing the opposite — a part-built meter with nobody on the tile is a different state.

**THE DRAG CONTRACT IS PINNED BY A PNG-LESS TRIPLE** on `floor_chart_drawn_down`, since no frame can
show it: the harness drives `floor_changed(f, committed = false)` on the live chart, then asserts the
chart node SURVIVES, the verdict has re-read, and the YIELDS text has CHANGED. **The settle before the
assertions is load-bearing** — `queue_free` is deferred, so a rebuild leaves the old chart both valid
and findable for the rest of that frame, and every same-frame form of the survival assertion passes
with the bug restored (measured, twice). Each is sabotage-verified against a different mutation:
forcing the rebuild fails the first, no-oping `_refresh_floor_live` fails the second, and taking the
yields host back out of the live set fails the third and only the third.

**`_compose_forage` GOES THROUGH `ForageFx.floorify` NOW, LIKE ITS HERD TWIN.** Most states pass a FRESH
fixture to it rather than the object `_show_tile` already converted, so the sheet was built from a
dict the adapter had never seen. That was invisible while the adapter only rewrote ceilings — the
fixture builders seed those themselves — and stopped being invisible the moment it also had to seed
the growth terms: every compose sheet opened that way lost its chart.

**The compose SPINE does not include the chart or the targets**, deliberately: the spine's HEAD
assertion is `band → policy → stepper` and the expedition sheet builds no chart (a raid's trip is the
sim's forward simulation, not a per-turn drawdown), so folding the chart in would fail a sheet that is
correct.

---

## THE PRE-LAUNCH FIGHT IS DOWN TO ONE LINE, AND IT ONLY EVER REFUSES

Two lines used to sit between the kit row and the forecast on the herd sheet, and both are retired
(reported from playtest — they were noise between the control the player had just used and the
numbers they were reading):

- **`One hunter brings 10 Wild Fowl into contact.`** — `SourceForecast.hunters_per_animal_face`. A
  fact about the SPECIES that never moved with anything being dialled. The reach it stated is already
  spent where it is actionable: the crew targets and `expedition_engage_crew` divide by the same
  quotient, so the number reaches the player as a party ceiling rather than as an announcement. Gone
  with `HUNTERS_PER_ANIMAL_FORMAT`, `ANIMALS_PER_HUNTER_FORMAT`, `HudWidgets.HUNTERS_PER_ANIMAL_META`
  and `Readout.hunters_per_animal_line` — the whole chain had exactly one render site.
- **`0.1 hunter-turns to bring one Wild Fowl down (attack 20 against defense 0).`** — the WINNABLE
  face of `hunt_gate_model_at`. Same argument: a species constant printed above a forecast that
  already prices the whole trip. `HUNT_GATE_EFFORT_FORMAT`, `HUNT_GATE_EFFORT_DECIMALS` and the
  model's `hunter_turns` field went with it.

**`HUNT_GATE_BLOCKED_FORMAT` STAYS, and the gate now renders only when `blocked`.** The refusal — *"⚠
Your hunters cannot hurt X — attack N against its defense M. No party size changes that"* — is the
one thing here a player cannot get anywhere else, and since the kit picker landed it is also the
**honesty mechanism for the `none` kit**: with the estimate tables suppressed for a kit they were not
quoted at, this is what still answers what the party can and cannot hurt.
`band_panel_compose_deny_kit_mismatch` asserts on it **by equality**, so removing the wrong face
fails there loudly.

**`has_engagement_stage` SURVIVES, and it is not an oversight.** It still gates the block: a PEN
publishes `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked and not fought — and without the
gate a pen would wear the refusal on a fixture that carries real `defense` and `durability`. The
harness pins that (`hunt.gd`'s pen negative).

The model's remaining shape is `{stated, blocked, effective_attack, text}`, and **`text` is non-empty
only when `blocked`**. `durability` is still the STATED-ness test even though no surviving face
quotes it: a species the roster cannot resolve reads `0`, and answering `blocked` about one whose
defence could not be looked up would refuse a hunt over a gap in the data.

### …AND A CLEARED GATE IS THE REASSURING HALF OF A SPLIT PARTY (issue #520)

The gate is composed at ONE attack tier, and on a partly-equipped band that tier is the **best-equipped
crew's** — the sim reads `hunterAttack` off `huntCrews[0]`. So a cleared gate says *somebody* can take
this and says nothing about the rest, which is wrong in the reassuring direction on exactly the band
the arc is about: ten spears among seventeen hunters take a Red Deer with ten of them and with none of
the other seven, at any headcount.

`SourceForecast.hunt_crew_split_model(band, herd, quarry, kit_id)` is the rest of the sentence —
`{stated, armed, barred, text}`, both counts WHOLE PEOPLE — and `HudWidgets.mount_hunt_crew_split` is
the ONE builder both gate hosts mount it through (`DrawerComposeController`'s herd sheet and
`BandPanelController._mount_kit_gate_line`), `mount_trip_readout`'s reason: two controllers rendering
one line is how a copied control drifts.

- **IT IS THE GATE'S COMPLEMENT AND NEVER ITS COMPANION.** A refused fight renders the refusal and no
  split (`0 of 17` beside *"no party size changes that"* is one sentence twice); a cleared gate renders
  the split and no refusal. Both hosts call it from the gate's `else`, and the two frames assert both
  directions.
- **WARN, not DANGER.** The hunt is possible for part of the party, so it must not take the refusal's
  ink, which in this HUD means *you cannot do this at all*.
- **IT HAS ITS OWN META** (`HudWidgets.HUNT_CREW_SPLIT_META`), the two-readers rule the gate and the
  retired reach line already followed. `HUNT_GATE_META` is read BY VALUE — `true` exactly when the
  fight is refused — so a split line wearing it would answer *"the sheet says this fight is winnable"*
  to a harness asking whether it is blocked.
- **IT COUNTS THE PARTY BEING COMPOSED, NEVER THE BAND'S ROSTER — on every sheet that has a stepper.**
  The gear covers a **prefix** of whoever is sent (the sim's own coverage model, `equipment.md` → "The
  partition is by ITEM SET"), and the crews arrive best-equipped first, so the first `party_workers`
  hunters take the best gear the band holds. A band-level sentence over a `HUNTERS 6` stepper reads as
  *"7 of my 6 are bare-handed"* — which is what shipped first, and it is a NEW wrong number rather than
  a stale one. `min(selected, armed prefix)` is arithmetic over two published counts; nothing here
  resolves a tier, a step-down or a coverage of its own.
  - **All three hosts pass one** — the herd drawer's `_compose.hunt_count()` and BOTH Band-panel raid
    sheets' `_send_expedition_count`. The dock's hunting-party and denial forms are compose sheets with
    party steppers, not band readouts, so "the band panel has no selected headcount" is false for them.
    `SourceForecast.HUNT_CREW_PARTY_UNSET` is the band-level reading and exists for a host that
    genuinely has no party.
  - **A PARTY LARGER THAN THE PUBLISHED CREWS states nothing.** `huntCrews` divides the band's CURRENT
    hunt workers and a compose stepper draws on idle ones too, so past that head count the wire's
    division does not describe the party and `10 of 13` out of a 12-strong division would invent a row.
- **IT STATES NOTHING IN SIX CASES, each for its own reason.** One crew (the shipped case — a uniform
  band publishes exactly one row, never an empty list). The party fits inside the armed prefix (there
  is no split in THIS party, whatever the rest of the band holds). Nobody armed (the refusal is
  rendering instead). Nobody on the hunt (a band with no hunters publishes one crew at `workers 0`,
  which is nothing to say rather than a shortfall of zero out of zero). A party the crews do not cover,
  above. And **a kit that is not the band's quoted one** — the crews are resolved against
  `PopulationCohortState.kitId`, so quoting them under a kit the player has just picked would describe
  a division that does not exist for that choice. The common case is unaffected, both hunt defaults
  resolving to the same id.
- **THE TWO COUNTS ARE APPORTIONED** (`HudFormat.apportion_people`): crew workers are floats, so
  rounding each side alone yields a 4 and a 2 that do not make 6.
- **THE TAIL CLAUSE IS READ OFF `item_ids`, NOT ASSUMED.** A barred crew holding nothing is
  *bare-handed*; one holding something the defence still shrugs off *holds too little gear*, which is
  the shipped case whenever the party's sled goes round and its spears do not.

**Frames:** `herd_hunt_gate_split` (`chapters/hunt.gd`, appended last in the combat-gate block) — the
same mammoth and the same party size as `herd_hunt_gate_effort`, with only the band's crew division
moving, asserting the refusal is absent AND the sentence by EQUALITY (both candidate readings differ
by a word or a digit, which a `contains` cannot separate, and the band-level reading of those same
rows is `4 of your 17` against the party's `4 of your 6`). Two negatives ride with it, each covering a
different way the line can be wrong: `herd_hunt_gate_effort`'s UNIFORM control, without which the
claim passes on a sheet that annotates every band, and a PNG-less re-compose of the SAME band and
quarry at a party that fits inside the armed run, which must state nothing.

**THE FIXTURE'S ARMED RUN IS SIZED UNDER THE HARNESS PARTIES ON PURPOSE** (`BandFx`'s
`KIT_SHORT_SPEARS_ARMED` = 4 of a 17-strong hunt roster). A shortfall that only bites above ten
hunters is silent on every compose frame in the corpus, so the positive claim would be unreachable —
which is the shape this arc shipped with first.

## THE RAID'S READOUT IS ONE BUILDER, IN THE SHARED WIDGET LAYER

`HudWidgets.mount_trip_readout` (+ `_trip_yield_rows`) is the boxed `THIS TRIP` section — the payload
as a yields row, the trip's length as the verdict, the floor's meaning as the aside. It was private
to `DrawerComposeController` while the Band panel's dock sheet answered the same question with a
one-line bbcode sentence, and the two drifted exactly as a copied control does. Both sheets call it
now; everything it needs (`trip`, the quarry's name, the composed floor) arrives as a PARAMETER,
which is what let it move at all.

Only a DELIVERING trip reaches it (`SourceForecast.hunt_trip_delivers`); the refused states keep the
one-line form on **both** sheets, an empty box being worse than the sentence it would replace.

---

## THE KIT IS CHOSEN ON THE SHEET, and it moves every figure below it (`docs/plan_denial_raid.md`)

A crew is sent out with a **named kit** from a roster the sim publishes once per world
(`SubsistenceSection.kits` + `defaultHuntKitId` / `defaultForageKitId`). A kit is a MASK over the
three TOE components — `effective_equipped(component) = kit uses it AND the band still has condition
in it` — so it moves the FIGHT (the attack tier) and the HAUL (the carry tier) alike.

**ONE ROW, FOUR SHEETS, one builder.** `KitRoster.build_kit_row` is mounted **directly under the
party/crew stepper and above the forecast** on the Band panel's hunting-party form, its denial form,
the herd drawer's assign-hunters block and the land drawer's assign-foragers block. A kit describes
the crew, which is why it sits with the crew; every number under it is a function of it, which is why
it sits above them.

**…AND ON THE TWO BAND-WIDE ROLE CARDS, which are not sheets and take two documented deviations.**
The WORKFORCE zone's Scout and Warrior cards mount the same builder with no field key and with
`compact_chrome`, over a hint of their own (`KitRoster.role_hint` — a carry-axis wording says nothing
about a vantage), and they COMMIT ON THE PRESS rather than at a Send. The spec is
`band-city-panel.md` → "The role cards carry the band's OTHER two kits", including the per-ROLE AXIS
table (`ROLE_AXES`) that prices a Scout on its vantage and a Warrior on its `attack`, and the
`KitOption.item_ids` read that keeps a warrior's condition clause off the spears.

**The control is a native `OptionButton`** — `HudWidgets.build_option_picker` — not a pill row, the
roster growing toward a dozen kits that a row of pills cannot hold in a 354px dock column. No
per-entry art: the client ships none per kit, and repeating ONE job glyph down every row is noise
rather than a distinction.

**IT WAS A `MenuButton` WEARING A `⌄` IN ITS OWN TEXT, AND THE CARET IS WHY IT CHANGED.** A
`MenuButton` draws no arrow, so the affordance had to be baked into the face — where `clip_text` eats
it the moment the label reaches the button's edge. `Gathering kit` does, so the FORAGE sheet showed no
caret at all (in the string, never drawn), while the hunt sheet's rendered as a small low-baseline mark
that read as a stray comma beside the themed arrow the `Band:` picker one row above already drew: one
cause, two symptoms, and two mechanisms that were never going to match. An `OptionButton` reserves the
arrow's width as an internal right margin, so the icon is drawn OUTSIDE the text's clip rect and no
face can push it off — **and a glyph in `KIT_PICKER_FACE_FORMAT` would now be a second affordance
saying the same thing, the one that clips.** It also marks the current entry NATIVELY (its popup items
are radio-check items and it checks the selected one itself), which is the behaviour
`HudWidgets._fill_menu_popup` hand-rolls through `MENU_ENTRY_CHECKED` for the `⋯` menus that still
need it.

**The FACE is stated separately from the LIST, and that is not decoration.** `select()` writes the
item's own text into `text`, so the builder takes a `face` it applies afterwards — because the two are
different sentences: the list tags the job default with a suffix that must not appear on the face, and
the face carries the job glyph the list deliberately omits.

- **The menu lists only kits whose `jobs` covers the sheet's verb.** A kit named for a job outside its
  own list is a COMMAND FAILURE server-side, never a silent fall back to the default.
- **It opens on the sheet's current kit, seeded from the job default** (`resolve_selection`: the
  composed id if the job still offers it, else the default, else the first kit the job lists). The
  hunt and denial missions share one composed id — they are one sheet under two missions, both on the
  `hunt` job — and the resolve re-validates every render, so a selection can never survive into a verb
  that would refuse it.
- **The job default is MARKED, not separated.** `none` gets none of that treatment either: it is an
  ordinary kit that grants nothing — not an error, not an "override", not divided off — and it renders
  last because `equipment.json` authors it last. **This client sorts nothing.**
- **There is deliberately NO disabled/unavailable state.** Every kit in today's roster is always
  selectable; a worn component degrades the tier rather than removing the kit, and the wire carries no
  availability field to invent one from.

### The compose sheet's FIELD ROWS are one family — `Band:` · `Kit` · `Quarry`

Three rows, three widget TYPES, three different modules building them: the band picker
(`DrawerComposeController._build_band_picker`), the kit picker (`KitRoster.build_kit_row`) and the
Band panel's quarry button (`BandPanelController._build_quarry_row`). They read as one stack because
all three go through **`HudWidgets.build_field_key`** for the label and take
**`HudStyle.apply_button(…, "ghost")`** for the box — which is what makes the height ONE number
(42px, the stylebox's own content margins, identical for a `Button` and an `OptionButton`) rather than
a constant somebody has to keep in step.

**The KEY takes a DECLARED width (`HudComposeVocab.COMPOSE_FIELD_KEY_WIDTH`) and does not expand**, so
the value control is the row's only expanding child. Both obvious alternatives were measured and both
lose: a natural-width key starts each control against its own word (`Kit` 22px, `Quarry` 55) — the
ragged edge this removes — and an `EXPAND_FILL` key splits the row 50/50, which on a ~245px sheet
leaves the control ~119px, too narrow for `🧺 Gathering kit` plus its arrow. That was the shipped
shape, and it is also why the quarry row used to drop its key to `SIZE_FILL` in the three-child branch
alone: with a declared width the chooser simply takes its width out of the pick's share and the
special case is gone.

**THE QUARRY ROW IS PRESENTED AS ONE OF THE FAMILY AND IS NOT ONE OF THEIR KIND.** Pressing it ARMS A
MAP PICK — quarries are chosen spatially (glow rings, the targeting banner, the in-reach refusal
nudge) and the eligible herds are scattered across the map rather than enumerable in a sensible list —
so it takes the family's key width, chrome, height and left-aligned face and **must never take
dropdown chrome**. An arrow there would promise a list that does not open, which is worse than the
inconsistency it would paper over. The one list it does offer is the `⋯` chooser at the end, and only
where a hex genuinely holds more than one eligible quarry.

**The PARTY stepper row is deliberately not in the family.** It is a control you operate rather than a
value box, so its key still expands and its `−/+` sits at the row's trailing edge.

#### A PICKER STATES ITS OWN SELECTION, INCLUDING "NOTHING"

`HudWidgets.build_option_picker` calls `button.select(…)` **unconditionally** — the caller's
`selected_index`, or `HudWidgets.NO_ENTRY_SELECTED` where the caller has nothing chosen — and it calls
it BEFORE the `face` write, which selecting overwrites.

**`OptionButton.add_item` SELECTS the first selectable item it is handed** (measured: a fresh button
reads `selected == -1`, and reads `0` the moment one item is added). So a picker built for an
unanswered field came back internally holding an entry while `face` painted `Choose…` over it — and
Godot then **declines to report a pick of the entry it believes is already current**: no
`item_selected`, so no `on_pick`. The one entry a player could not choose was the very one the widget
had seated itself on, which is the FIRST live entry of every list.

**It shipped as a dead control**, because the shipment sheet's destination list had exactly one live
tie: the popup opened, the entry highlighted, the click closed it, and nothing happened — no face
change, no message, `Send shipment…` still greyed. The kit picker and the drawer's `Band:` picker
carry the same swallow whenever they are handed no selection; one `select` fixes all three, because
the defect is the widget lying about its state rather than anything the callers do.

**A caller passes `NO_ENTRY_SELECTED` rather than a bare `-1`**, so "nothing is chosen" is a thing a
call site can say. Re-picking the entry a picker already shows is still a no-op — that IS the state
`select` now states truthfully, and nothing is lost by it, since a pick that changes nothing has
nothing to re-render.

### The `Band:` picker opens on the band the player is LOOKING AT

`Hud._resolve_assign_band` answers which band a sheet composes for — and every command it can emit
targets — in three rungs: **the selected player unit → the PANEL band → `player_band()`**. Both
compose sheets seed their picker from it (`begin_hunt_source` / `begin_forage_source`), and it is
injected into `DrawerComposeController` and `TargetingController` alike, so the sheet, the map pick
and the move all name one band.

**The middle rung is the whole of it, and it exists because the first rung is empty exactly when a
sheet is open.** Selecting a HERD or a TILE clears `HudSelectionState`'s unit (`select_herd` /
`select_tile` drop it by design), and selecting a herd or a tile is *how* a compose sheet is opened.
The resolver therefore used to reach `player_band()` — the FIRST player-faction cohort in
`update_band_alerts` — on the ordinary path. That was harmless while a faction had one band and
became a defect the moment an expedition could found a second (issue #510): the sheet composed for
the PARENT while the Band/City panel showed the colony, so the picker read `Band 1` under a `Band 2`
header and the crew stepper capped at the parent's spent idle workers ("1 of 30 useful — free up idle
workers to send more"). Every number was arithmetically honest and about the wrong band.

**The panel band is the right subject because it survives everything a sheet does to the selection.**
Selecting a herd or an empty tile leaves it intact (`band-city-panel.md` — the panel persists across
selection changes), the faction page leaves it alone as the subject the cycler walks back into, and
`refresh_snapshot` re-resolves it every turn. It is the only piece of state that still names the band
the player was reading when they clicked the map.

**It is re-resolved LIVE by entity; the stored dict is never returned.** `set_panel_band` keeps
`unit.duplicate(true)`, a copy taken at render time, and this answer feeds `source_crew_pool_hunt` /
`source_crew_pool_forage` — the very idle counts the steppers cap against — so returning that copy
would put a stale crew under the steppers this fix exists for. A lookup that FAILS is not staleness
either: the panel band only ever comes from `player_bands()`, so an entity the roster no longer lists
is a band that has left the world, and the last rung takes that case rather than addressing a command
to it. The panel rung carries the same `_is_player_unit` guard the selection rung does.

**Verified by `ui_preview`'s `compose_panel_band_hunt` / `compose_panel_band_forage`**
(`chapters/hunt.gd`, appended last). Both sheets are asserted, because they are two injection sites
and one passing says nothing about the other. Three numbers are staged deliberately unlike each other
— a parent with NO idle crew, the colony's live 2, and a panel copy stale at 9 — so each wrong rung
fails as its own distinct answer instead of hiding inside another's. Sabotage-verified by restoring
the bare `player_band()` fallback: exactly those six assertions fail (`Band 1`, entity 841, a stepper
capped at 0) and nothing else in the run does.

#### …AND ON A WORKED SOURCE IT OPENS ON THE BAND ALREADY WORKING IT

There is a rung AHEAD of that ladder, and it is asked only where there is a source to ask about: **the
band already working this tile or this herd.** `DrawerComposeController._band_working_source` walks
`current_player_bands()` and takes the ladder's answer if it is one of the crews, else the first
worker in ROSTER ORDER, else the ladder's answer unchanged.

- **It lives in the compose builders, NOT in `Hud._resolve_assign_band`.** That resolver is shared
  with move-band and targeting, neither of which has a source in hand — a band is being moved, not a
  patch being staffed — so the source-aware rung belongs where the source is.
- **The test is PENDING-AWARE** (`effective_forage_workers` / `effective_hunt_workers`), so a
  just-issued assign counts and the sheet does not bounce to another band for the turn before the
  snapshot confirms it.
- **The existing ladder WINS A TIE**, which is what keeps the panel band the subject wherever it is
  one of the crews. Only a ladder answer that works the source *not at all* is displaced.
- **The drawer's closed read state already answered this way** — `_standing_assignment` scans the
  roster for the first standing assignment on the source — so the summary line and the sheet the
  button opens now name one band by construction rather than by coincidence.

#### THE COMPOSITION RE-SEEDS ON A SOURCE CHANGE **OR AN ACTOR-BAND CHANGE**

A crew, a floor, a crop and a build are facts about ONE band's standing assignment, so switching the
actor invalidates them exactly as switching the source does. `ComposeState.seed_*` records the band it
seeded from (`forage_seeded_band()` / `hunt_seeded_band()`, `NO_BAND_ENTITY` until a first seed) and
both builders re-seed on `source_changed or <web>_seeded_band() != <the resolved band's entity>`,
evaluated **after** the band resolves — before it, the comparison is against the band being left.

**`set_forage_band` / `set_hunt_band` stay bare writes, and clearing the compose key is NOT the fix.**
The picker's callback writes the band and rebuilds; the rebuild is where the standing assignment can
be read, which is the same two-step `begin_*_source` + `seed_*` already exists for. Clearing the key
instead would run `begin_*_source`, which re-defaults the band from the resolver and discards the
player's own pick.

**What it cost while the seed was source-only** (reported from play, on a tile worked by Band 3 with
two foragers): the sheet opened on Band 1 — out of forage range, no idle crew — because the compose
key was already this tile from an earlier interaction, so the band resolver never ran again. Switching
the picker to Band 3 moved every LIVE reading (`2 of 4 useful`, the standing-crew line) while the
COMPOSED count stayed at Band 1's. A composed 0 against a standing 2 makes the commit button
**`Unassign`** — one press from stripping two real foragers off the tile — and takes the improvement
control with it, under the crew-0-on-a-worked-source rule, correctly applied to the wrong state.

**Frames + assertions** — `compose_working_band_forage` / `compose_band_switch_forage` and their hunt
twins (`chapters/hunt.gd`, appended after the panel-band pair). The FIXTURE is the claim: three bands
where the ladder's answer works neither source and the other two work both, at two standing crews
neither of which is `WORKER_STEP` (a stepper reading 1 cannot tell a re-seed from the
no-standing-assignment fallback). Four claims per state — the picker's FACE, the STEPPER, the commit
VERB and the improvement control's PRESENCE — over a vacuity guard that dials the crew to 0 on a
working band and requires the sheet really does say `Unassign` and really does drop the control.
**The picker is driven with real pointer input**, face then popup row, never a hand-emitted
`item_selected`. Sabotage-verified on two DISJOINT mutations: restoring the bare `set_*_band` write
fails the eight re-seed claims (`got Unassign`, the played defect, on both webs) and leaves the
default claims green; removing the working-band rung fails exactly the four default claims
(`Band 2, got Band 1`).

#### …AND THE COMPOSITION DIES WITH THE SHEET

**An uncommitted edit must not outlive its own sheet.** Reported from play: drop HUNTERS 4 → 2, close
WITHOUT committing, reopen — and the sheet still showed 2 over a band that still had 4 on the herd. A
number nothing in the game is keeping is worse than no number.

`_on_compose_sheet_closed` was only ever dropping *which sheet is open* (`clear_composing`); the
*what is dialled* half — the crews, the floor, the crop, the declared rung — is keyed on the SOURCE
and survived every close. It now calls `reset_forage_source()` + `reset_hunt_source()` beside it,
which clears the key the `source_changed` test above compares against, so **the next open takes that
existing branch and re-seeds from the band's own row**. There is deliberately no seed-on-open call:
two seeding paths over one composition is how a sheet comes to open on a crew nobody has.

**BOTH webs are reset on any close**, not just the kind that was open. One sheet is on screen at a
time and this is the only place either composition ends, so clearing the pair leaves no way for the
other web's stale dial to survive into a later session.

**The seed reads the WIRE, so a just-committed edit reads back at the previous snapshot's value until
the turn resolves** — and that is the coherent answer, not a gap: the drawer's own standing summary
beside it reads the same confirmed assignment (`_standing_assignment`), and the optimistic overlay
carries no build crew at all (`assign_labor` never states one), so a pending-aware seed would restore
the take and silently revert the builders.

**It re-pointed one `ui_preview` state.** `forage_crop_picker_sow` dialled its rung BEFORE the first
open, which `_show_tile`'s close now re-seeds away — the plant twin of the trap `_compose_herd`'s
docstring has always recorded. The fix is the documented one: open, dial, re-open.

#### THE STANDING-CREW LINE IS GONE

The sheet's first control is its `Band:` picker. There is no `Now N` line above it — the drawer's own
one-line summary states the standing crew in the read state the `Assign … ▸` button opens from, and
the sheet states the composition being dialed. `COMPOSE_NOW_STAFFED_FORMAT` and
`COMPOSE_PENDING_SUFFIX` are deleted with it.

### THE SHEET'S NUMBERS ARE REPRICED AT THE CHOSEN KIT, through ONE seam

`KitRoster.repriced_source` hands the ordinary forecast a COPY of the wire's own terms with two
substitutions, so every consumer downstream — the take, the waste, the crew targets, the chart —
picks the kit up without knowing it exists. `DrawerComposeController._kit_priced_source` is the only
caller, and `_hunt_priced_herd` / `_forage_priced_patch` are the only two doors onto it.

**THE PER-WORKER SUBSTITUTION IS TWO LOOPS, BECAUSE ONE ACCOUNT IS A VECTOR.**
`SOURCE_PER_WORKER_KEYS` is the SCALAR list (`per_worker_biomass`, `per_worker_yield`) and
`SOURCE_PER_WORKER_VECTOR_KEYS` is `per_worker_material`, scaled row by row through
`SourceForecast.scaled_material_rows` — **one ratio for every row**, the materials being one biomass
flow through a fixed per-biomass vector exactly as the two scalars are. The vector sat UNREPRICED for
the whole life of this seam, and "just add the key" was never the repair: `float(out[key]) * ratio`
throws on an `Array`, so the list could not simply grow. What the player saw was a correctly reduced
FOOD line beside an unmoved HIDE line — a worse kit over-stating the materials it would bring home, a
better one under-stating them — on BOTH webs, since `expected_materials` clamps
`min(workers × per_worker_material, ceiling)` off whatever rate reaches it. `band_panel_preview`'s
`_assert_kit_reprices_the_source` asserts the RATE by ratio, its no-op twin at the reference tier,
and the take through `expected_materials` itself, at a crew deliberately below the saturating one so
the CREW arm binds — plus the plant-web half on the no-retreat patch beside it.

**THE DENOMINATOR IS THE ROSTER'S OWN EQUIPPED TIER, NOT THE SOURCE'S `perWorkerBiomass`.** The ratio
is `effective_carry / KitRoster.equipped_tier(kits, axis)` — the maximum across the roster on that
axis, the exact twin of `unequipped_tier`'s minimum. Every kit that USES the component publishes the
`labor_config` capacity there and every kit that does not publishes the bare tier, so the maximum IS
the rate the sim published the source at (`snapshot/capture.rs` → `kit_roster_states` resolves it
through the take path's own seam).

- **In production the two coincide on the animal web and DIFFER on the plant one.** A herd publishes
  `labor_config.hunt.per_worker_biomass_capacity` verbatim; a patch publishes
  `forage.per_worker_biomass_capacity × seasonalWeight`, while a `KitOption`'s forage tier is stated
  **before** the tile's weight (`equipment.md` says so in as many words). Dividing by the patch's own
  number therefore divides the season back out and multiplies a season-free tier by it, so the crew's
  rate comes out season-BLIND — wrong in the direction that looks right, worldgen pinning every weight
  at `1.0` today.
- **It is also what the harness fixtures need.** `ForageFx.seed_growth_terms` RECOVERS an absent
  `per_worker_biomass` as `per_worker_yield / provisions_per_biomass`, which is exact against the
  fixture's own hand-authored rates and has nothing to do with anyone's carry — measured at **286.67
  against the roster's 40** on the deer fixtures, i.e. a ratio of 0.14 applied to every per-worker term
  on the sheet. That moved five `ui_preview` assertions (three crew-count claims, the dipped-take line
  and the oracle pair) and it is not a fixture bug: a canned rate is not a claim about a band.
- **`repriced_source` is consequently NOT idempotent** — its reference is no longer a field the
  substitution overwrites — so each producer prices at its own top and never hands a priced dict to
  another producer that prices too. `_hunt_delivered_and_waste` takes `_hunt_yield_model`'s already-priced
  herd and says so.

**FOUR SHAPES OF THIS SHEET READ A SOURCE, and a call site that reached for the raw dict is how three
of them were missed.** Reported from play as *the take moved with the kit but the crew numbers did
not*: `forecast_inputs` (the stepper cap), `floor_chart_model` (**both crew-target pills**),
`herd_axis_rates` (the quantised take and its waste) and `_hunt_take_rate` (the degrade path). The
chart is the one the report was actually about — *clear it now* and *hold it after* are its numbers,
not the forecast's — and priced on one side only, a pill names a count the `+` refuses.

**THE RETREAT RIDES `stay_fraction` AND NEVER `engage_rate`.** The kit's `dispersion` multiplies the
quarry's own wariness, and the substitution is `snapshot.fbs`'s own formula,
`clamp(1 − (1 − stayFraction) × dispersion, 0, 1)`. It is applied to its OWN field rather than folded
into the reach because the two stages are separately observable — `engage_rate` is a fact about the
quarry, `dispersion` moves the retreat alone — and the fold makes Big-game and Trapping quote the
identical hunt on a herd whose whole difference is how much of what they reach stands still.

`SourceForecast.animals_stayed` is the client mirror of `animals_that_stay` at the quantile a forecast
reads it at (the analytic mean `floor(engaged) × stay`; `animals_engaged` already floors), applied in
the sim's own order — **engage → retreat → convert**.

**IT PRICES THE CREW AS WELL AS THE TAKE, and that reverses what this file used to say.** A hand that
keeps one animal in four draws a stock down a quarter as fast, so the crew that draws it down at all
is four times as large — which makes every crew answer on this sheet a quotient of what STAYS:
`engage_workers`, `take_workers` and through them `crew_to_hold` and `max_useful_workers`, beside
`engagement_carry`'s `crew_to_clear` / `crew_that_reaches`, which divided by the retreat all along.
The doctrine that survived here for a while was that `engage_workers` mirrors
`fauna::hunt_engage_workers`' raw-reach sizing and must not cut, so the stepper cap could not disagree
with the sim's `workersNeeded`; the consequence was a cap sized **82** on a played Wild Boar herd
beside a *clear it now* pill naming **108** — the sheet offering a crew the panel then refused to let
the player assign. 108 is the honest number, `server-dev` is making the matching change in
`fauna::hunt_engage_workers` / `hunt_take_workers`, and `stay` is a REQUIRED parameter on
`engage_workers` / `take_workers` / `crew_to_hold` / `engagement_carry` so no call site can take the
raw reach back by omission.

**`stay <= 0` answers a crew of NONE, not an infinite one.** Nothing the party reaches ever stands, so
the take is identically zero at every size — there is no number of hands that achieves it, and the
crew needed to achieve it is therefore none. `take_workers`' `max()` then keeps the haul crew, exactly
as it does for a source with no engagement stage at all.

**Both take producers carry it, because only one of them is `expected_yield`** — the same reason the
reach arm needed both. `_hunt_delivered_and_waste` composes its own `collection` so it can quantise,
so an arm added to the shared layer alone leaves the *rendered* green line unmoved while the cap
beside it shifts.

**A source with no retreat stage is byte-identical to before the stage existed.** A patch and a pen
publish no `stayFraction`, `repriced_source` finds no key to substitute, and `forecast_inputs` reads the
wire's own `1` — which `animals_stayed` short-circuits on, so an unbounded engagement passes straight
through. Measured twice: with the whole substitution live, **zero of `ui_preview`'s 590 assertions
move**, and with the retreat reaching the crew answers as well, **zero of its frames move** — no
fixture in that harness publishes the field.

### THE DOCK'S RAID CHART IS PRICED TOO, AND IT IS THE ONLY THING ON THAT SHEET THAT CAN BE

`KitRoster.priced_source` is the resolve-then-reprice seam and **both controllers call it** —
`DrawerComposeController` for the drawer's three sheets, `BandPanelController._fill_hunt_compose_sheet`
for the dock's raid chart. It lives in `KitRoster` rather than on a controller for the reason that
layer exists at all: a second copy of a resolve is how one entry point comes to quote a kit the other
does not, and this arc has now paid for that twice.

**The dock's OTHER figures cannot carry a kit, and that is the honesty rule rather than an omission.**
The trip readout, the preset metrics and the demand-side party cap are all readings of
`huntTripEstimates`, which the sim quotes at the hunt job's DEFAULT kit and does not reprice — so under
a mismatched selection they are suppressed outright. The chart is the exception because it is composed
CLIENT-SIDE from the herd's own wire terms, which makes it, beside the combat gate, the only thing on
that sheet still answering for the kit the player actually picked.

**Both halves of the substitution are real for a raid.** `advance_expeditions` resolves the party's own
kit and runs `HuntParty::stayers` exactly as a resident hunt does, so `dispersion` belongs there; the
carry tier scales the party's throughput the same way.

### THE PROJECTION CARRIES THE RETREAT, AND SO NOW DOES EVERY CREW ANSWER BESIDE IT

`dispersion` reaches a chart at all only because `project_stock`'s engagement bound takes the stay
term. Without it the dock's sheet could be priced and still render identically for every kit, the
curve being the one thing it draws.

**Only two things on this sheet still read the RAW reach, and both are readings of ONE TURN's contact
rather than of a crew:**

| divides by what STAYS | the RAW reach |
|---|---|
| `project_stock`'s bound at all three walks (`floor_chart_model`, `take_draws_down`, `crew_that_reaches`' probes) | `animals_engaged` — how many this party touches THIS turn, before the retreat is applied to it |
| `engagement_carry`, hence `crew_to_clear` and `crew_that_reaches` | `engagement_per_worker` — the pair's composition, which `animals_stayed` is then applied to |
| `engage_workers`, hence `take_workers`, hence **`crew_to_hold`** and `max_useful_workers` | — |

**The two surviving raw readings are INPUTS to the retreat, not answers that skip it**: every consumer
of either passes it straight into `animals_stayed`. What changed is the third row — `crew_to_hold` was
the last crew answer sized on the raw reach, on the grounds that it IS `fauna::hunt_take_workers` and
the stepper cap floors on it, and the cost was a cap **below** the *clear it now* pill rendered beside
it (82 against 108 on the played Wild Boar). The remark this file still makes about the two crew
targets disagreeing stands and is a different point: *clear* and *hold* ask about different stocks, so
their answers may differ — what they may not do is disagree about how many animals a hand lands.

**Measured on one herd (`wariness 0.75`, `engageRate 4`, a party of 3):** the passive device walks the
herd to its floor (`settled_fraction` 0.50) where the spear line settles at **0.70**, *clear it now*
reads **50 hands against 13**, `crew_to_hold` **5 against 2**, and the stepper cap **61 against 16**.
Guarded by `band_panel_preview._assert_dock_chart_carries_the_kit` and
`_assert_kit_reprices_the_source`, whose two kits differ in `dispersion` ALONE (same carry, so the
carry half cannot account for a unit of it) over a locally-built roster — `BandFx.kit_roster_fixture()`
ships no `dispersion` at all, so asserting through it would compare a kit against itself. Beside them
`chapters/hunt.gd`'s `_retreat_crew_assertions` holds the invariant the whole change exists to
restore, over five species: **the cap reaches every crew target the same sheet renders.**

**It moved ZERO frames and ZERO assertions in either harness**, measured by stashing the change and
re-rendering: no rendered fixture on either side publishes `stayFraction`, so `animals_stayed`
short-circuits and every existing number is untouched. Which is also why the claim had to be a driven
one — see the liveness section below for the general form of that trap.

**Assertions, in `band_panel_preview._assert_kit_reprices_the_source`** — the whole substitution is
arithmetic, so it is unit-tested rather than rendered. The reference claim needs a source whose
published rate DIFFERS from the roster's (in production they coincide, so a live-shaped fixture passes
with either denominator and says nothing), and the end-to-end pair is the claim that matters: on one
herd at one crew the passive device lands **8.0 food against the spear's 2.0** while
`max_useful_workers` answers **16 either way**. The pairing IS the assertion — a retreat folded into the
reach moves both and would satisfy the first half alone.

> **`band_panel_preview` IS THE ONLY HARNESS THAT CAN SEE THE FOLD, and it is worth knowing why.** No
> `ui_preview` fixture publishes `stayFraction` at all, so the substitution has no key to touch there
> and a retreat folded into `engage_rate` is a NO-OP across all 343 of its frames — the assertion that
> historically caught this (*"the compose stepper caps at the crew the SIM asks for"*) could not catch
> it today. Sabotage-verified: restoring the fold fails exactly the two claims above and nothing else
> in either harness, the crew count going **16 → 61** while the take moves with it. Adding a
> `stay_fraction` to a `ui_preview` herd fixture would move that herd's frames, so the guard stays
> here, where it is arithmetic and free.

### THE REPRICING NEEDS A LIVENESS GUARD, AND EVERY OTHER ASSERTION IS BLIND TO IT

**A dead repricing returns the source unchanged, which is exactly what every fixture was tuned
against** — so it goes green everywhere, and a frame cannot see it either: a sheet quoting one kit's
numbers under another kit's name is a perfectly plausible sheet. It has now died twice.

1. The ratio divided by the SOURCE's published rate. On a canned fixture that is recovered from the
   fixture's own rates, so it was a meaningless number rather than no number — five assertions failed
   and the cause read as unrelated.
2. Fixing (1) introduced the silent one. `effective_tiers` answered short keys (`"forage_carry"`)
   while the roster spelled them `forage_carry_per_worker_biomass`; `_kit_priced_source` read the
   tier with the short key and `equipped_tier` with the same string, which no roster entry carries,
   so **the reference came back `0` and the substitution short-circuited entirely**. Every kit on
   every compose sheet quoted identical numbers with only the hint line above them moving. Reported
   from play. The whole `ui_preview` suite was green at 590/590 — *because* the feature was dead.

**`band_panel_preview._assert_kit_reprices_the_source` structurally cannot catch either**: it calls
`KitRoster.repriced_source` DIRECTLY with numeric arguments, so it exercises the arithmetic and never
the seam that feeds it. Both deaths were in the feed.

So `compose_rungs.gd` carries a PNG-less block that drives `DrawerComposeController`'s OWN producers
at the REAL roster and asserts the numbers **MOVE** between two kits — the basketed patch against the
bare-handed one, and the sledded herd against the sledless one:

- **It asserts a RATIO, never magnitudes** (`bare × equipped == basketed × unequipped`), so a re-tuned
  `equipment.json` moves the fixture and the expectation together.
- **Both surfaces the report named**, because the two deaths moved neither and a fix that repriced the
  forecast while leaving the take on the raw source would satisfy only one: the PER TURN take
  (`_forage_yield_model`) and the crew the sheet asks for (`max_useful_workers`).
- **Each half opens with a PRECONDITION that the source states a per-worker rate at all.** That is not
  ceremony — the first draft priced a `world_herds_fixture()` row, which is a ROSTER entry carrying no
  rate, and the ratio claim passed vacuously at `0.0 against 0.0`. The precondition caught it on the
  first run.
- **The crew is `KIT_LIVENESS_FORAGERS` (2) deliberately**: at a crew that saturates the patch's own
  ceiling both kits quote the ceiling and the take stops moving, which would make the claim pass on a
  dead repricing again.

Measured live: forage `0.064` against `0.32` (the basket's 8.0 over the bare hand's 1.6), take
`+0.13 /turn` against `+0.64 /turn`, crew **15 against 3**; hunt `0.09` against `0.3`.
Sabotage-verified by restoring the mis-spelled key — the three forage claims fail reading
`0.32 against 0.32`, `+0.64 against +0.64` and `3 against 3`, which is the reported screenshot exactly,
while the hunt half correctly stays green (a different call site).

> **THE RENDERED FRAMES ARE NOT THE GUARD AND CANNOT BE.** Every `ui_preview` compose state composes at
> its job's DEFAULT kit, where the ratio is 1 and repricing is a legitimate no-op — so of 343 frames,
> **exactly ONE** (`herd_hunt_gate_blocked`, whose band has run its kit dry) differs between a live
> repricing and a dead one. Judging this feature by frames means judging it by one frame that moves for
> a reason adjacent to it.

### A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED, AND ITS TAKE IS ZERO

Reported from play on the expanded roster: the compose sheet offered **Trapping** and **Husbandry**
against a Red Deer as ordinary choices and quoted each a real take, for a hunt that brings home
**exactly nothing**. `KitRoster.attack_against` — written precisely to resolve a kit's attack against
a named animal — had **no callers**, so the sheet applied the trap's `dispersion 0` (nothing flees, so
the take reads BETTER) while never applying its `attackMaxBodyMass 1.0`. Above that bound the snare
grants nothing, the party falls back to the bare hand's `attack 1`, and the sim's
`max(0, attack − defense)` refuses the hunt. Measured against the shipped roster, **three of the four
options on a Red Deer sheet took nothing** and all four were presented alike.

**The rule, and it introduces no config — every term is something the kit already declares against
something the source already publishes:**

> Offer a kit as selectable only if something it declares can change this source's outcome.

`KitRoster.kit_offer` answers `{offered, reason}` for one (kit × source) pair, on two rules:

| rule | what it reads | who it withholds |
|---|---|---|
| **the weapon cannot reach the quarry** | `attack_against(kit, body_mass, bare)` through `SourceForecast.hunt_gate_model_at` | a snare against a Red Deer; anything bare-handed against a defended species |
| **the kit's contribution is an axis this source cannot read** | `kit_uses(…, pen_carry)` against the herd's `corralled`, **and** `kit_uses(…, build_work_per_worker)` against `RungGates.hunt_rung_remains` | the husbandry kit on a herd that is neither penned nor able to climb |

- **`none` is NEVER greyed, and nothing spells its id to arrange that.** `kit_supplies_any` asks
  whether the kit beats the roster's bare-handed tier on *any* axis; a kit that beats none of them
  grants nothing anywhere, so there is no source it can be inapplicable *to*. It is the free
  bare-handed comparison the whole wear model exists to protect, and a future `fishing` kit with an
  empty `uses` inherits the treatment — which is the test of whether `none` has been special-cased.
> #### THE BUILD AXIS IS ASKED FIRST, and it is what stopped the pen rule from LYING (issue #515)
>
> The rule the doc stated was *"offer a kit only if something it declares can change this source's
> outcome"*; the rule the code ran was `kit_uses(pen_carry) and not penned`. Those agreed only while
> `pen_carry` was the handling kit's whole payload. Once the gear also declared a build axis — which
> speeds `Tame` and `Corral` — the kit was still withheld on the very herd the player was taming,
> **stating a reason that had become false**: *"what it adds is only used on a penned herd"* is not
> true of gear that is doing its work on that animal right now.
>
> `kit_offer` now asks the build axis first, and a kit that can speed a rung the herd has left to
> climb is **offered outright** — so the weapon rule below never runs on it either. Hurdles do not
> have to bring a deer down to be the right thing to carry while you are gentling one.
>
> **"A rung left to climb" is `RungGates.hunt_rung_remains`, the same seam the rung picker admits
> rungs with** (ceiling above the standing rung, and that rung not already finished), *not* a second
> ceiling comparison — two copies is how the picker comes to offer a Corral the kit list has already
> called impossible. It is asked with **no exclusion**, unlike the picker's: a herd mid-`Tame` is
> "progress, not an opportunity" to the picker, but it is exactly where build gear does its work.
>
> It is knowledge-blind like every other term here, and resolved at the FRESH tier: what a kit *can*
> change is a property of (kit × quarry), and a faction that has not learned Penning yet will learn it
> while still holding this herd.
>
> **Pinned as a pairing, on the two species' real ceilings** (`ui_preview`, `compose_rungs`): a **Red
> Deer** is `wild`-ceiling and never climbs, so the handling kit is still withheld there for its own
> reason; a **Rabbit Warren** pens, so the same kit on the same roster in the same run is offered. A
> rule that simply stopped greying anything fails the deer half.

- **A PEN is exempt from the weapon rule**, gated on the same `has_engagement_stage` predicate the
  gate LINE is mounted behind: a penned animal is slaughtered rather than stalked. Without it a
  corralled Red Deer would withhold every kit but the spear line.
- **The pen rule is asked FIRST**, so a kit states the same reason on every quarry. The husbandry kit
  fails both tests on a Red Deer — it carries no weapon either — and *"what it adds is only used on a
  penned herd"* is the fact about the KIT, where *"nothing it carries can bring down a Red Deer"* is a
  fact about the deer that would then go unsaid on a rabbit, where the same kit is withheld anyway.
- **Greyed, NOT hidden, and it states its reason on its own face.** *"A snare cannot hold a Red Deer"*
  is a fact about the world worth teaching once, and invisibility is exactly what let this ship
  unnoticed. The reason rides the entry's `label` and is repeated in its `tooltip`, because a disabled
  popup row is the one control here a player cannot reliably hover.
- **`resolve_selection` skips a withheld kit at every step**, so a trapping selection made on a warren
  falls through to the default when a Red Deer's sheet opens rather than surviving as a greyed row the
  picker is opened on.

> #### WEAR MUST NOT ENTER THE CHOICE — the load-bearing constraint
>
> **Which kits are offered, and which is default, are properties of (kit × quarry) resolved at the
> FRESH tier.** What the sheet QUOTES for the selected kit, and what the hint line says, are the
> band's own worn tiers as before.
>
> | question | wear? |
> |---|---|
> | Is this kit greyed on this quarry? | **NO** — fresh tier |
> | Which kit is the default? | **NO** — fresh tier |
> | What take does the sheet quote? | **YES** — `effective_tiers` |
> | What does the hint say? | **YES** — `spears 74`, `sled dry` |
>
> So a band whose spears are dry, looking at a Red Deer, still sees the stalking kit **listed,
> selectable and default**, quoting zero, with the hint explaining the spears are gone. If wear drove
> the list, the picker would silently reshuffle between turns and the player could not tell a kit that
> *cannot* work on this animal from one that has merely *worn out*.

**The "no disabled state" rule in `build_kit_row`'s doc was about WEAR and still holds** — a worn
component degrades the tier rather than removing the kit. Applicability is a different axis, and the
doc now says so in as many words rather than leaving the next reader to read the two as a
contradiction and "fix" one of them.

#### Filtering the LIST is not enough: the gate is priced too

**The Band panel's raid chart reprices with no picker in sight** (`BandPanelController` calls
`KitRoster.priced_source` directly), so the quoted number has to be honest on its own.
`priced_source` therefore asks `hunt_gate_closes` **before** any repricing and answers
`gate_closed_source` — every per-worker currency substituted flat to zero — when the fight is refused.
The retreat is deliberately *not* substituted beside it: a stay fraction describes what a party keeps
of what it brings down, and this one brings nothing down.

- **Here the band's wear DOES apply**, this being the quoted number rather than the choice:
  `effective_attack_against` composes the two floors — outside the weapon's size window the item was
  never in play, and inside it the band's own condition decides — and it is the ONE resolution the two
  gate LINES (`DrawerComposeController`, `BandPanelController._mount_kit_gate_line`) now share. Both
  used to read `effective_tiers["attack"]` unbounded, i.e. a trapping sheet cleared a gate the sim
  shuts.
- **The quarry's terms come off `src`, which IS the herd on the hunt job.** What this stateless layer
  may not do is consult `HudBandLaborState`, and it does not: roster, band and source are all
  parameters.
- **Because the offer test resolves at the fresh tier, a withheld kit is never the one priced** — so
  the reachable case for this branch is WEAR: a band with dry spears against a Red Deer,
  `max(0, 1 − 1)`, which is exactly the state the constraint above insists stays selectable.

**Coverage** — `compose_rungs.gd`'s `_kit_offer_states`, over a locally-built roster (the shared
`BandFx.kit_roster_fixture()` carries neither a trapping nor a husbandry kit, and adding them would
re-list every hunt picker in both harnesses). Three frames — `herd_kit_offer_red_deer`, the same sheet
with the picker OPEN (the closed face names the selected kit alone, so only the popup can show a
withheld row and its reason), and `herd_kit_offer_rabbit` — plus nine assertions. **The pair of
quarries is the claim**: a rule that greyed the trapping kit everywhere satisfies the deer half alone,
and the positives beside it (the spear line untouched and default, `none` never withheld) are what
stop "grey everything" passing. The zero-take half is PNG-less and DRIVEN through
`_hunt_priced_herd` — a per-worker rate is a number, and a sheet quoting the wrong one renders a
perfectly plausible forecast. Sabotage-verified on two disjoint mutations: an unconditional
`kit_offer` fails exactly the three greying claims and leaves every positive green; disabling the
`priced_source` gate fails exactly the zero-take claim, naming the `0.09` it would have quoted.

#### The `big_game` kit is called the **Stalking kit**

Four roster entries name a practice or a role and one named its prey, which misleads: a player reading
*"Big-game kit"* on a rabbit sheet concludes it is the wrong tool when it merely performs worse. Only
`display_name` moved — the id `big_game` is unchanged, so no sim code, test or wire contract does.
The label is spelled in the harness fixtures and in `WorkbenchVocab`'s worked example, which moved
with it.

### The hint states the EFFECTIVE tier, never the fresh one

`KitOption`'s numbers are for a FRESH kit. The band's real condition is on its own cohort
(`hunting_kit_durability` / `sled_kit_durability` / `basket_kit_durability`), and a component the kit
uses but the band has run dry delivers the UNEQUIPPED tier — so quoting the roster's 40 to a band with
a spent sled is a lie of exactly the class this arc keeps correcting.

**No "does this kit use that component?" test is needed, and that is the point of the form.** A kit
that does not use a component already publishes the unequipped tier on that axis, so stepping down
there is a no-op and the whole rule collapses to one line per axis:

```text
effective(axis) = kit(axis) when the band still has condition in the component, else unequipped(axis)
```

**The unequipped tier is read off the ROSTER ITSELF** — the minimum across it on an axis IS that
axis's bare-handed tier, because every kit publishes the unequipped value on each axis it does not
use. No second copy of the TOE table, and no client-side knowledge of which component each kit masks
in.

**`stated` is false when the band says nothing about its condition at all** — the key absent, not
zero, `0` being a real reading meaning DRY. The fresh tiers then stand and no condition clause prints,
the "absent terms render no line" convention `hunt_gate_model` already takes.

#### A PENNED herd is priced — and described — on the KEEPER'S carry

**The carry axis is a property of the SOURCE, not of the job**, and `KitRoster.carry_axis_for(job,
src)` is the one place that is decided. A corralled herd is worked from a Hunt row, so the job-keyed
`JOB_CARRY_AXES` priced a pen on the SLED's tier while the sim collects one on
`EquipmentStat::PenCarry`, which only the husbandry kit supplies. A sled drags a carcass in off the
range; a pen stands at the camp.

**Neither half of that error was visible, because on the shipped roster they CANCEL.** Husbandry and
stalking both carry a sled, so both sat at the sled's equipped tier and every hunt kit quoted a pen
the same number — under-stating the kit the pen exists for and over-stating every kit that carries a
sled and no handling gear, into one plausible-looking sheet. Only a driven assertion can hold it;
`ui_preview`'s `chapters/compose_rungs.gd` states the claim as a triple (the wild reading unmoved, the
husbandry kit at the reference, the sled-only kit at the bare keeper's tier), because the pen pair
alone is satisfied by pricing everything on the pen axis and the wild reading alone by no fix at all.

- **The corral state comes off `src`, and that is not a reach for state** — on the hunt job `src` IS
  the herd, handed in as a parameter exactly like the body mass the weapon's size window is tested
  against, and read through the same `QUARRY_CORRALLED_KEY` the offer test and the fight's gate use.
- **The reference tier moves with the axis, in one expression.** `equipped_tier(kits, carry_key)` is
  the denominator; switching the axis without switching the reference resolves it to `0` off a roster
  that states nothing there, the repricing short-circuits, and every kit quotes identical numbers —
  which is exactly how the forage spelling bug shipped.

#### …so the hunt HINT is gated on the source too

A hunt row works two different things through one verb, and they read disjoint axes, so `tier_hint`
takes the quarry (`build_kit_row` already had it, for the greying) and states what will actually be
read. A WILD herd is stalked and hauled — `attack`, the sled's carry, spears and sled — byte-identical
to what the line rendered before the pen axis existed. A PEN is collected: `pen 40.0 per keeper`, then
the handling gear's condition and the SLED's.

- **The tier line and the condition clauses answer different questions at a pen, which is why the sled
  appears under one and not the other.** Only `pen_carry` sets the rate, but the sim charges a pen
  slaughter over TWO quanta — the handling gear for what was butchered, the sled for what was hauled
  home — so the sled's TIER is a number nothing on the sheet will read while the sled's CONDITION is
  wear the player is paying. No attack and no spears: a penned beast is slaughtered rather than
  stalked, it publishes no engagement stage (the predicate the gate LINE is mounted behind), and the
  sim charges no weapon for the kill.
- **The pen line is gated on the SOURCE, not on the KIT, and the difference is the point.** Gating it
  on the kit printed a pen tier for a husbandry kit against a wild herd — a tier nothing would read —
  and withheld it from a sled-only kit at a pen, which is the one place a player needs it: at a pen,
  `pen 12.0 per keeper` beside `pen 40.0 per keeper` is the whole visible difference the handling gear
  buys. The condition CLAUSES are the kit's own `item_ids` list and are not gated on the source at all
  — see "THE HINT NAMES THE KIT'S OWN ITEMS" below.
- **The pen carry is read off the band's row like every other tier**, `BandKitTiers` carrying
  `penCarryPerWorkerBiomass` and `scoutVantageRange` alongside the fought, hauled and gathered axes.
  Those two arrived on the table last and were the two the client had to answer off the ROSTER's fresh
  tier in the meantime — so a band whose handling gear had run dry read `pen 40.0 per keeper` while the
  sim collected 12, wrong in the reassuring direction. The per-key fall-through that stood in for them
  is gone with the gap: `_row_tier` reads the row and nothing else, and a whole-row absence (a band the
  wire has not described yet) is the only case the roster still answers.
- **`KIT_SCOUT_VANTAGE_KEY` HAS a consumer now** — the WORKFORCE zone's role CARDS, which carry a
  picker and a gear line each (`band-city-panel.md` → "The role cards carry the band's OTHER two
  kits"). `role_gear` reads the same row, so the WARRIOR card reads the band's sim-resolved `attack`
  under the warrior kit — clubs, not spears — and the SCOUT card its sim-resolved vantage under the
  wayfinding kit, 1 tile once that gear is spent rather than the roster's fresh 2.
- **`hurdles` / `hoes` / `wayfinding` / `clubs` also joined `DetailFormat.KIT_ITEM_LABELS`**, so the
  band's `Gear` summary row names them instead of falling through to the raw wire ids — and each has a
  row in the kit BREAKDOWN too, that popover pairing an item with the resolved tier it sets and the
  cohort publishing all three (`band-readouts.md` → "The other three tiers, and the kit each is quoted
  at"). **The popover's rows and the picker's hint answer different questions off different fields**:
  the popover states this band's tier at each JOB'S DEFAULT kit (the cohort's flat fields), the hint
  states what the kit under the cursor would grant (that kit's `kitTiers` row).

### A BUILD IS PRICED AT THE **BUILDERS'** KIT, WHICH IS NOT THE ONE UNDER THE CREW STEPPER

Reported from play: the Builders card offered the **Husbandry kit** to raise a Cultivate. The roster
now carries **two builders kits, one per web** — `hurdling` (hurdles, `animal`) and `tillage` (hoes,
`plant`) — `husbandry` has given up the `builders` job entirely, and which kit a queue entry gets is
DERIVED from that entry's own branch (`equipment.md` → "THE BUILDERS' KIT IS DERIVED PER QUEUE
ENTRY"). Three client consequences, and the first is a defect the sheets shipped:

- **BOTH COMPOSE SHEETS PRICED THE BUILD AT THEIR OWN PICKER'S KIT** —
  `KitRoster.build_gear(band, kit_id)` with the hunt sheet's selection, and with the GATHERING kit on
  the forage sheet, which declares no build axis at all. The picker under the crew stepper chooses
  what the TAKE crew carries; what speeds a build is what the BUILDERS carry, and those are two
  different rows. `DrawerComposeController._build_gear_for(band, kind)` is the one seam now:
  `KitRoster.builders_kit_for` resolves the entry's kit and `build_gear` is asked for the entry's
  BRANCH, so a row serving the other web contributes the neutral `0.0` exactly as the sim's
  `serves_branch` does.
- **THE ENTRY'S BRANCH IS THE SHEET'S OWN WEB** (`KitRoster.build_branch_for_kind`) — a patch is a
  plant build and a herd an animal one, the same fact `systems::labor` stamps a queue entry with — so
  no new wire field was needed to know which web a sheet is composing for.
- **`kit_offer` WITHHOLDS A BUILDERS KIT WHOSE TOOL SERVES THE OTHER WEB**, with its reason, and that
  is the same rule as the snare against a Red Deer asked one job over: it takes a `build_branch`
  parameter rather than a quarry, because the builders stand on no source. **What it feeds is a
  RESOLUTION, not a picker.** It reached a control while the Builders role card mounted one, which
  greyed the inapplicable kit and stated why; that card now states its kit on a read-only line
  (`band-city-panel.md` → "THE BUILDERS CARD MOUNTS NO PICKER EITHER"), so the rule's one live reader
  is `KitRoster.resolve_selection`'s selectable list — which is what keeps the card and the build
  queue's header off a kit the entry's web cannot use. `build_kit_row` no longer takes the parameter
  at all, no caller having one to pass. `none` is still never withheld, carrying nothing to be
  inapplicable with.

> #### THE ROW PUBLISHES THE **RESOLVED** KIT, AND ONE CASE IS THEREFORE UNRESOLVABLE HERE
>
> `LaborAssignment.kitId` on the `builders` row is the sim's answer *after* the derivation — a kit
> named on the row, else the roster's answer for the queue HEAD's web — so the client cannot see
> whether it was NAMED. `KitRoster.builders_kit_for` recovers every case but one, and the recovery
> is what preserves sending the pool out bare:
>
> | the row publishes | the head's web | this entry gets |
> |---|---|---|
> | a kit serving THIS entry's web | any | that kit |
> | a kit serving NEITHER this entry's web NOR the head's | a web with a kit | that kit — it can only be a **pin** |
> | `none`, with a head queued | a web with a kit | `none` — a **deliberate** bare-handed pool |
> | `none`, with the queue EMPTY | — | the roster's kit for this web (the FIRST build a player declares) |
> | a kit serving the HEAD's web but not this entry's | a web with a kit | the roster's kit for this web ⚠ |
>
> **The last row is the one that can be wrong**, and only for a player who pinned the kit the head
> would have derived anyway: the sheet quotes `tillage` on a plant entry where the sim will keep the
> pin and take nothing off it. Closing it is SERVER-side — the stored id, or a "was it named" flag,
> beside the resolved one.
>
> **The empty-queue row is why the derivation cannot simply be skipped.** Until the player commits
> there is no queue, so the row publishes `default_kits.builders` (`none`), and a sheet reading that
> literally quotes the very first Cultivate in a game as bare-handed — then jumps the moment the
> decision has already been taken.

### THE SHEET OPENS ON THE KIT **THIS QUARRY** WANTS (`equipment.md` → "Which kit a QUARRY wants is DERIVED")

`default_kits.hunt` is one id for the whole job and could not express *which kit this animal wants*,
so the sim derives a per-herd one and publishes it as `HerdTelemetryState.defaultKitId` — decoded as
`default_kit_id`, the newest LIVE slot on that table, following the two `(deprecated)` `*EstimatesKitId`
ones the forecast query retired. On a Rabbit Warren it is the trap: a
spear party's approach loses three animals in four to the `wariness 0.75` retreat where the trap's
`dispersion 0` keeps all of them, so a sheet opening on the job's Stalking kit defaulted the player
onto a ~4× worse tool on exactly the quarry the roster has a right one for.

**`KitRoster.default_kit_for(job, source, job_default_id)` IS THE ONE PRECEDENCE**, and its whole
value is that three surfaces cannot answer it differently: `resolve_selection` (what the sheet opens
on) and `build_kit_row`'s `(default)` mark both call it. A picker
that opened on the trap and printed `(default)` on the spear would contradict itself on every
small-game herd, which is why the mark is asserted BESIDE the selection rather than trusted to follow
it. Only a HUNT row has a source that publishes one; the forage web's patches carry no such field, so
passing them through the same call is what keeps both webs on one seam.

**THE HONESTY TEST THIS DEFAULT ONCE HAD TO KEEP IN STEP WITH IS RETIRED.** The two per-herd estimate
tables were quoted at ONE kit and a sheet composing another had to refuse them; the forecast QUERY
takes the kit as an argument, so a sheet's numbers are always its own and there is nothing left for a
per-quarry default to fall out of step with.

**THE COMPOSED KIT IS DROPPED ON A SOURCE CHANGE, and without that the whole thing is reachable
exactly once per session.** Every render writes the RESOLVED id back onto `ComposeState`, so a kit
resolved on a Red Deer reads as *the player's own choice* on the next warren — and a composed choice
outranks any default, correctly. `ComposeState.reset_hunt_kit` (called from the drawer's existing
`source_changed` branch, beside `seed_hunt`) and `set_party_quarry` / `clear_party_quarry` clearing
`_party_kit_id` are what make the herd's own default reachable on every sheet. The kit was the odd one
out: the count, the floor and the improvement have always re-seeded per source. A pick made ON this
animal still survives its own re-render, which is the distinction — the reset is on the SOURCE
changing, never on a render.

**THE COMMAND'S OMISSION COMPARATOR HAD TO MOVE WITH IT, and it is `assign_labor`'s alone.**
`Main._kit_token` omits `kit <id>` when the selection equals the payload's `default_kit_id`, and an
absent token on a Hunt row now means *the HERD's default* to the sim (`equipment.md` → "It is resolved
SIM-side"). Measured against the job default, a player who deliberately picks Stalking on a warren
emits no token and the sim runs Trapping — the silent substitution the named path refuses, arriving
through the absent-token door. `HudLayer._emit_assign_labor` therefore supplies
`KitRoster.default_kit_for(kind, _band_labor.find_world_herd(herd_id), _band_labor.default_kit_id(kind))`.
**The two RAID verbs deliberately keep the job default**: `resolve_raid_kit` still resolves
`default_kits.hunt` for an absent token, so measuring them against the herd's would omit the token for
a selection the sim would then not run.

**Frames + assertions** — `herd_quarry_default_red_deer` / `herd_quarry_default_rabbit_warren`
(`chapters/compose_rungs.gd`), rendered in that order with NO `reset_hunt_source` between them, so the
warren's claim is made with the deer's `big_game` sitting in the compose state and the drawer's own
source-change reset is the thing under test. Sabotage-verified on two DISJOINT mutations: pointing
`resolve_selection` back at the job default fails the warren's selection and its precondition (naming
`big_game`) while the two `(default)` marks stay green; pointing
`build_kit_row` back at it fails exactly the two mark claims, printing `Trapping kit` unmarked beside
`Stalking kit  (default)` — the self-contradiction, demonstrated.

#### THE HINT NAMES THE KIT'S OWN ITEMS — `KitOption.itemIds`, not an axis→item guess

The tiers are bare numbers and name nothing, so a condition clause had to decide which ITEM produced
one. `AXIS_ITEMS` was that decision — `attack → spears`, `hunt_carry → sled`, `forage_carry →
baskets` — and it is not a fact about a kit but about the SHIPPED ROSTER, so the Trapping kit read
`attack 20.0 · carry 40.0 per hunter · spears 100 · sled 100`: gear it does not carry, quoted at the
SPEARS' remaining condition. A band with fresh traps and worn-out spears read exactly backwards.

The `uses` list is on the wire now (`equipment.md` → `KitOption.itemIds`, verbatim, in config order —
weapon first, haul aid after), so:

- **`tier_hint` iterates `kit_item_ids(kit)`** and `condition_of(band, item_id)` is keyed by the ITEM.
  The number of clauses follows the KIT rather than the job: `big_game` and `trapping` state two,
  `gathering` one, `none` **none at all** — an empty list is a real answer, never "unknown".
- **`kit_uses(kits, kit, axis_key)` is GONE, not kept beside it.** It inferred membership by asking
  whether the kit's tier on an axis beat the roster's bare-handed one, which cannot tell `traps` from
  `spears` — both `attack`, both at the same tier. Membership is stated; it is read.
- **The item NAMES ITSELF.** `KIT_COMPONENT_SPEARS` / `_SLED` / `_BASKETS` are deleted from
  `hud_compose_vocab.gd`; the two hint formats take the wire's own id.

**`AXIS_ITEMS` IS GONE, AND SO IS THE REPRICING IT WAS WRONG FOR.** `effective_tiers` had to decide,
per AXIS, whether the item supplying that tier still had condition — and the wire states a kit's items
but not which of them supplies which tier, so no client-side inference over the roster recovers it (two
kits supply `attack` from different items, so neither a set-cover nor a positional-order rule answers).
A band with fresh traps and dry spears was repriced to the bare hand under `trapping`. The sim publishes
the answer now: **`PopulationCohortState.kitTiers`** — one row per roster kit, resolved against THAT
band's live wear ledger (`equipment.md` → "`kitTiers` — the resolved per-band answer"). So:

- **`effective_tiers` is a LOOKUP** (`band_kit_tiers(band, kit_id)`, the ONE reader of the field) and
  re-derives nothing. `stated` is false when the band publishes no row for that kit, and the ROSTER's
  fresh tiers then stand — the "absent terms render no line" convention, unchanged.
- **THAT WHOLE-ROW ABSENCE IS THE ONLY FALL-BACK LEFT.** A per-KEY one stood beside it while the table
  carried three axes and the client wanted five, and it is gone with the gap (see the pen bullet under
  "…so the hunt HINT is gated on the source too"): `_row_tier` reads the axis off the row, and an axis
  the row omits reads `TIER_ABSENT` rather than the roster's fresh number. That is the same direction
  `condition_of` errs in — quoting a fresh tier for gear the server never confirmed is precisely the
  reassuring lie this field was published to end, and a client that kept a per-key fall-through would
  tell it again the moment a row arrived malformed.
- **THE MASS WINDOW RIDES THAT ROW TOO, and a gate must read it from there.** `attack_min_body_mass` /
  `attack_max_body_mass` are on `KitOptionState` as well, but those are the FRESH-KIT reference: a spent
  item contributes no bound any more than it contributes a tier, so a kit whose mass-bounded weapon has
  run dry has NO size window rather than its fresh one. `KitRoster.attack_against(kit, band, body_mass,
  unequipped)` takes the band for exactly this. Mixing the two sources is the failure the field removes
  wearing different clothes — it would quote a band with dry traps the bare hand's attack (right) inside
  the TRAPS' 1 kg ceiling (wrong), so a bare-handed party after a rabbit would be told it had no weapon
  for it.
- A band fixture must therefore **state its `kit_tiers` rows, all five axes of them**, not just its
  item conditions (`BandFx.kit_tiers_rows`). Authored, never derived from the conditions beside them:
  deriving them is writing the guess the field replaced, and a client that had put the guess back
  would agree with it. **A row that OMITS an axis exercises the absence path rather than the real
  one** — which is what kept the pen and the vantage untested while they were missing from the wire,
  and is why the worn fixtures now step each of them down at the item that supplies it.

**Assert the two kits as a PAIR** — `big_game` alone passes under the old guess (its attack really does
come from spears) and `trapping` alone would pass on a hint naming every item there is — plus `none`
as the third, whose empty list is what a print-everything hint fails. PNG-less and DRIVEN over a
constructed roster in `chapters/compose_rungs.gd`: the shipped fixture roster stages no `trapping`
entry (a fourth kit changes the picker's contents on every rendered kit state), and what the hint SAYS
is a string — a frame shows a plausible line whichever item it names.

### THE RAID'S NUMBERS ARE ASKED FOR — one question, one answer, no honesty gate

The two pre-launch raid forecasts used to ride the per-turn snapshot: every huntable herd carried a
`huntTripEstimates` (floor x party) table and a `denialEstimates` (party) table, both computed at the
hunt job's DEFAULT kit over a FRESH component set. They were ~93% of a turn's capture, and they were
wrong for anyone who had worn their gear or picked another kit — so the sheets had to compare
`hunt_trip_estimates_kit_id` / `denial_estimates_kit_id` against the selection and REFUSE to present
the table as the answer when they differed.

**Both tables are gone and so is the whole apology.** The forecast is a request/response on the command
socket (`ForecastQuery.gd`, and `sim_runtime/proto/command.proto` -> "THE QUERY CHANNEL"), answered for
the exact `(band, kit, party, floor)` the sheet is composing. A sheet's numbers are always its own, so
there is no nearest rung to name and no other kit's raid to disown. Retired with it:
`KitRoster.estimates_quoted_kit` / `estimates_apply_to` / `estimates_quoted_note`, the two herd kit-id
wire keys, the four `*_QUOTED_FORMAT` sentences in `hud_compose_vocab.gd`, and the sampled party axis's
`quoted_party_note`.

**THE SEAM IS ONE OBJECT AND THREE SHEETS ASK THROUGH IT.** The Band panel's hunting-party and denial
forms and the herd drawer's expedition branch each compose the ask and read the reply back through one
key (`_raid_forecast_view` / `_denial_forecast_view`, one per sheet, so the key it asks under is the key
it reads). `Main` injects the transport and pumps the replies; the HUD never reaches the network.

- **The ask is IDEMPOTENT on the composed key**, so a rebuild that moves nothing costs nothing, and
  every rebuild that DOES move something — a stepper tick, a kit switch, a committed floor — is exactly
  a re-query. A floor DRAG never reaches it: only a committed change rebuilds the sheet.
- **A QUERY TRIGGERS NO SNAPSHOT**, deliberately, so `ForecastQuery.answered` is the only thing that
  can tell a sheet to redraw. A sheet must never wait on a frame.
- **What renders while the answer is in flight** is the pending line (`RAID_FORECAST_PENDING` /
  `DENIAL_FORECAST_PENDING`) in place of the readout box, never zeros beside a live verdict — and the
  COMBAT GATE beside it, which is composed from wire terms and stays honest with no reply at all. A
  refusal renders `FORECAST_FAILED_FORMAT` carrying the server's own token; the Send stays LIVE either
  way, because the raid is launchable and only its length is unquotable.
- **A superseded answer stands for `STALE_AFTER_MSEC` (400 ms) and is not badged.** Measured live, a
  warm round trip is 48-63 ms — three or four frames — so a stepper tick shows the previous party's
  numbers for a moment rather than blanking, which reads as "this raid has no forecast". The FIRST
  query of a session measured **1264 ms**, so the pending line is what a player sees on the very first
  sheet they open.

#### `expedition_policy_takes` CHANGED MEANING, and the preset buttons now move with the crew

It reads the reply's `per_preset` rows, which the sim answers **at the composed party**. It used to
scan the table's whole party axis and take the MAX over it — so the preset faces quoted a best case
reachable only by a crew the band might not have, and they did not move when the player stepped the
Party stepper. They move with it now, and a one-hunter party's presets read a one-hunter party's rates.

That is a visible behaviour change with no new control: the same three buttons, answering a question
about the crew actually being composed rather than about the best crew the table happened to sample.
`SourceForecast.preset_floors()` is the ONE list of floors the ask carries and the reply answers in, so
the ask and the read cannot index the presets differently.

#### THE IN-FLIGHT `Collapse:` ROW ASKS ON THE LAUNCHED PARTY'S OWN BEHALF

`DetailFormat.expedition_collapse_line` used to read the target herd's snapshot `denialEstimates` row
for the party's size. That table is gone, and `DetailFormat` is an all-`static` layer that may hold no
request id and no socket — so the answer is a PARAMETER, exactly as the resolved `target_herd` beside
it already is. `BandPanelController.launched_party_denial_view(exp)` composes the question and both
hosts pass the result down: the dock's parties strip and its row tooltip directly, the Occupants drawer
through the same public method (`SubjectDrawerController`), so there is one request-id stream.

- **A detached party IS a band**, so `DenialRaidForecastQuery` takes it unchanged: its own `band_id`,
  its own `kit_id` (the kit it was OUTFITTED with — an expedition prices its whole life from the choice
  made at launch and never re-resolves against its home band's stock), and its `size` as the party.
- **`max_party_workers` is that same size**, which is a statement rather than a stand-in: the argument
  bounds the sim's search for the smallest party that breaks the herd, and the only party this surface
  is about is the one already out there.
- **Every state renders SOMETHING** once the party is a denial party with a live target — the verdict,
  or the row saying the answer is still coming or has failed. A row that appeared only on success would
  pop into a height-capped strip a frame after it opens and change its height under the player. An
  EMPTY view is the third case and renders NOTHING: it means a caller with no seam to ask through, not
  a question awaiting an answer.
- The verdict still passes **no band**, so it reads "...of raiding" rather than "...from launch" — a
  launched party's remaining walk is not on the wire, and adding the walk from the HOME BAND's tile
  would quote a leg the party may have finished turns ago.

#### THE CREW ONE-SHOT AND THE ANSWER IT IS MADE OF

A crew auto-fill is a one-shot armed by a floor click or a fresh quarry and consumed by the next render
— which is, by construction, the render that has just ASKED. The two sheet families resolve that
differently and the difference is load-bearing:

- **The two HUNT sheets spend it immediately.** Their fill target is a CAP, and the count is re-clamped
  to that cap on every render, so a fill spent against the no-answer fallback still converges on the
  reply's plateau a frame later. Holding it instead DEADLOCKS: the ask is skipped at a party of 0,
  which is exactly the state the fill exists to leave, so the answer never comes and the sheet renders
  no forecast at all.
- **The DENIAL sheet waits** (`ForecastQuery.answer_settled`). `denialPartyNeeded` is a REQUIREMENT,
  not a cap — nothing re-applies it, so a seed spent early is simply lost and the sheet opens on the
  stepper's floor instead of on the party the sim quotes. Waiting is safe there because that stepper
  never sits at 0 (`HudConst.WORKER_STEP` is both its floor and its initial value), so the question is
  always asked and the answer always settles. **A refusal counts as settled**, so a dead socket cannot
  leave the seed armed for the rest of the composing act.

**A LOCAL hunt asks nothing and never did**: it is priced from the herd's own per-biomass vector and
the band's ceilings, with no table and no query in it. Neither has the forage sheet.

### The command carries `kit <id>`, and OMITS it at the job default

`Main._kit_token` is the one builder, appended by all four grammars (`assign_labor forage` /
`assign_labor hunt` / `send_hunt_expedition` / `send_denial_raid`). It is a **named, space-separated,
order-independent** tail pair, the parser's existing `name value` style, lifted out of the tail before
any positional form is read — so it may sit anywhere after the role and no grammar has to make room
for it. On the denial raid it is the ONE thing the closed four-token grammar admits, because a kit is
a property of the PARTY rather than of the mission.

**It is omitted when the choice equals the default the SIM would resolve for an absent token**, which
is what absent means to the parser — so a composition that never touched the picker emits the
byte-identical line it emitted before the picker existed. Both the choice and that default therefore
ride the payload: the builder cannot know the default on its own (it is world data), and
`HudLayer._emit_assign_labor` supplies it through `KitRoster.default_kit_for` — the HERD's own default
on a Hunt row, `_band_labor.default_kit_id(kind)` everywhere else. See "THE SHEET OPENS ON THE KIT
**THIS QUARRY** WANTS" above for why the two raid verbs stay on the job default.

**THE KIT RIDES EVERY CREW EDIT, for the improvement axis's reason.** `BandPanelController._emit_work_assign`
restates the row model's own `kit_id` (off `LaborAssignment.kitId`): an omitted token means "the job's
default", so a `+`/`−` on the work board that dropped it would silently re-kit a crew the player
deliberately sent out bare-handed. A band-wide role (scout / warrior) carries `""` and emits nothing —
it consumes no component and has no kit axis, which is "no selection to make", never "no kit".

---

## An assignment has TWO axes: the STANCE and the IMPROVEMENT (issue #442)

**This section supersedes every passage below that treats a build verb as a value of `policy`.** Those
passages are kept because they still document *why* each behaviour exists; where one asserts that
Cultivate/Sow/Tame/Corral are rungs of the policy picker, read it against this.

A labor assignment carries two independent facts, and they are two wire fields:

| Axis | Question | Wire | Values |
|---|---|---|---|
| **Stance** | how hard do I pull? | `LaborAssignment.policy` | `sustain` · `surplus` · `deplete` · `eradicate` |
| **Improvement** | what am I building? | `LaborAssignment.improvement` | `""` · `cultivate` · `sow` · `tame` · `corral` |

`policy` is **always** a stance and is **never rewritten by the sim**. The design and its rationale
live with the arc that owns it — `.claude/rules/core_sim/intensification.md` → "An assignment has TWO
axes" (and `docs/plan_investment_rung_toggle.md`). What follows is only the client half.

**The stance ROW is the existing policy picker, narrowed to four rungs and never gated.** Every
per-kind option list is gone (`HudBandLaborState.HUNT_POLICY_OPTIONS` / `FORAGE_POLICY_OPTIONS`): both
webs, a resident band and a detached party alike, offer `SourceForecast.LABOR_HUNT_POLICIES` and
nothing else. `build_policy_picker` lost its `gates` parameter, the greyed-and-explained rendering it
drove and the `collapse_other_gates` height opt-in with it — a stance has no prerequisite and never
retires, so no rung of that picker can be disabled.

**The improvement ROW is one control below it** (`HudWidgets.build_improvement_control` +
`DrawerComposeController._build_improvement_control`), in exactly one of three states:

1. **Offered** — an unchecked `CheckBox` naming the next rung
   (`🌱 Cultivate this patch`). A rung you can actually take; what it PAYS reads in the readout
   beneath (see "THE PAYOFF LIVES IN THE READOUT" below).
1b. **Gated** — **a `Label`, NOT a disabled checkbox** (`HudWidgets.IMPROVEMENT_STATE_GATED`), whose
   own text IS the first unmet prerequisite, rung glyph and all: `▦ This ground is rich but too dry to
   farm — …`. **The offer wording is gone, not greyed**, and the crop picker does not render —
   committing is what is refused, so there is nothing to configure. The shape says which kind of
   thing this is: a checkbox is a CHOICE, an unmet prerequisite is a FACT, and the DONE state is a
   Label for the same reason. It shipped once as a greyed checkbox reading "Cultivate this patch ·
   then 0.04 food…" with the reason on a line beneath — an offer the player cannot accept sitting
   directly above the sentence explaining that they cannot accept it, over a live, clickable crop
   list. Found in play, not by the harness (`forage_sow_locked`, `improvement_offered_gated`).
   **On the COMPOSE SHEET this state is reached by a SOURCE gate only** — a knowledge gate builds no
   control here at all, which is why the example above is the ground's refusal and not the knowledge
   line it used to be; see "A KNOWLEDGE gate renders NO improvement control on the compose sheet".
2. **Running** — a static `Label` in `HudStyle.SIGNAL`, carrying the build meter and its turn estimate
   (`🌱 Cultivating 30 / 50 work (60%) — ≈10 turns`). It was a checked, live `CheckBox` whose
   uncheck sent `abandon_improvement`; a build in flight is a FACT the meters state now, and the lever
   is the band's **Builders role card** (see "RETIRED — `abandon_improvement`" below; it was the
   BUILDERS stepper beneath this control until §2.5 took the build crew off the tile).
3. **Done** — a static `Label` naming the state (`🌾 Tended Patch`), with the NEXT rung's checkbox
   beneath it when there is one.

> #### ⛔ THERE IS NO CHECKBOX ANY MORE — THE SHEET JUDGES A RUNG, THE WORK TAB DECLARES IT (§4.7a ①)
>
> Every state above is a `Label` now (the OFFERED one a `RichTextLabel`, see below). The passages in
> this file that describe a tick, an untick, `on_toggle` or a `disabled_reason` are kept for the
> reasoning they carry and are **read against this**.
>
> **The trap it removes**: the `🌱 Cultivate this patch` checkbox was not the action — the only thing
> that committed it was a button reading **`Forage`** — so ticking it and closing the sheet did nothing
> at all. Reported from play as *"I just click cultivate and not the Forage button — that seems
> completely unnatural."* The committing act is the Work board's `⌃` mark (`band-city-panel.md` → "…AND
> THE `⌃` IS THE CONTROL THAT DECLARES THE BUILD"); the sheet keeps the FORECAST, which is what a 28px
> work row cannot hold.
>
> **The four faces, and none of them reads as an offer to act except by naming the control elsewhere:**
>
> | state | line |
> |---|---|
> | AVAILABLE | `🌱 Cultivate this patch from the Work tab.` |
> | QUEUED | `🌱 Cultivate this patch · ◷ Queued` + `⚠ Not started — nobody is on this band's Builders role.` |
> | RUNNING | `🌱 Cultivating 30 / 50 work (60%) — ≈7 turns` |
> | DONE | `🌾 Tended Patch`, with `▦ Sow a field here from the Work tab.` beneath |
>
> - **ONE LINE, AND `Work tab` IS A LIVE LINK.** It shipped for an hour as a fact line plus a smaller
>   remedy note beneath, and Ray's verdict was that one line is all it needs and the pointer should be
>   clickable. `work_tab_requested(band_entity)` is a `DrawerComposeController` signal relayed by
>   `HudLayer` to the panel — **the compose sheet never reaches the dock itself**.
> - **THE LINK CARRIES THE ACTING BAND, and shipping it without one was a defect.** It named the tab
>   alone, so from the FACTION page it landed on the faction's Work **rollup** — a list of bands, with
>   no `⌃` anywhere on it — delivering the player to a surface that cannot do what the sentence
>   promised. The band it carries is the one the sheet's `Band:` picker names, which is the band whose
>   `⌃` will queue the job and whose pool will pay for it.
>   - **The guard is *not already this band*, never *is the faction page*.** The faction page is the
>     reported symptom; a panel cycled to a DIFFERENT band is the same defect, and a guard written
>     against the symptom would miss it.
>   - **It routes through `jump_to_band_entity`**, the faction page's own drill-down path, whose note
>     forbids a second way to make a band the subject — *"a popover row reaches a band the same way the
>     cycler does … rather than by a second path that could drift from it."*
>   - **The tab is set AFTER the jump.** `render_band` re-declares the zone layout and arriving from the
>     four-zone faction page can flip the shell, so a tab set first is overwritten by the render.
>   - **`entity`, not `band_id`** — nothing here builds a command, and every overlay reader keys on the
>     client-local handle.
>   - An unresolvable band still switches the tab, so a bad handle cannot swallow the interaction.
>   - **The jump is COUNTED, not inferred** (`alert_focus_requested` emissions: 1 when it must jump, 0
>     when the panel is already there), which is what catches *always jumps* and *never jumps* alike.
>     Four cases: the faction page, a different band, the right band already, and an unresolvable one.
> - **What it still does NOT do is focus the source's ROW** on that board — that needs a public focus
>   seam the board does not have.
> - **AVAILABLE is the one state built as a `RichTextLabel`**, and the reason is layout, not style:
>   `build_inline_link` returns a `Button`, which is atomic — a `[Label][link][Label]` sentence cannot
>   break inside either half and overflows the ~245px card. An inline `[url]` flows. Every other state
>   stays a `Label`.
> - **THE PRICE IS NOT ON THIS SHEET.** `50 work · 2 work a turn to hold` moved to the `⌃` mark's
>   tooltip — Ray: *"That information should be on the work tab. No need to have it here, it is
>   useless."* `_improvement_offer_face` and `IMPROVEMENT_OFFER_PRICED_FORMAT` are retired;
>   `DetailFormat.build_price_clause` survives with its one caller moved, **keeping `from Agriculture`**,
>   which is a closed defect (it read as a demand on the crew under the stepper) and must not be
>   re-opened by stripping it there.
> - **THE CROP PICKER LEFT ENTIRELY**, `extra_rows` with it — Ray: *"the CROP TO TEND shouldn't be a
>   selection here as the user can't do the cultivate here."* It is a setting of a job, and the job's
>   settings are the queue row's expansion (`band-city-panel.md` → "A ROW EXPANDS INTO THE JOB'S
>   SETTINGS"). It still commits through the **same** `assign_labor` builder — the crop was never a
>   queue-entry field, only a `species` token on the forage row — so nothing moved on the wire.
> - **THE LIMIT IS STATED RATHER THAN RELAXED, and that was a decision.** A source the band does not work
>   has no work row, so the `⌃` cannot reach it; the alternative was relaxing the sim's rule, which is a
>   different membership test for the queue *plus* a per-entry band id on the wire, and buys one saved
>   click on ground you are about to staff anyway. So the unworked sheet says
>   `🌱 Send gatherers here first, then Cultivate this patch from the Work tab.` **The two sentences are
>   a PAIR and are asserted as one** — a builder that always prints one passes any single check.

**ONLY ONE improvement is ever offered — the source's next rung.** `RungGates.next_rung_offered` is
that answer and shares its ordering with `next_rung_ready` through the private `_next_rung`: highest
ungated rung first (so sowable wild ground, where both plant rungs clear, offers **Sow**), falling
back to the LOWEST admitted-but-gated rung when none is ready — the nearest thing you could work
toward. The two answers differ on **the gate alone**, and that difference is the difference between a
MARK (promises the verb is available, so a gated rung must not wear one) and a CONTROL (is how the
player discovers the rung exists).

### THE PAYOFF LIVES IN THE READOUT — the faces are bare, and the deal is back as a labelled row

The compose sheet states an improvement's terms as **a labelled row inside the `PER TURN` readout**,
directly under the take and above the verdict:

```
PER TURN · WHILE BUILDING
0.64  FOOD    RENEWABLE
ONCE TENDED  1.20 food
```

**That example is the whole readout of a building sheet, and the absences in it are load-bearing.**
There is no arrow on the reading and no `now → after` in the caption: a composed build suppresses the
floor walk outright (see "A COMPOSED BUILD SUPPRESSES THE FLOOR WALK" below), because `ONCE TENDED` is
already a *later* and two unlabelled ones in one box is the confusion that rule exists to prevent.

**This reverses an earlier deletion, and the reason it was reversed is worth keeping.** The payoff
had been moved onto the CHECKBOX FACE (`🌱 Cultivate this patch · then 1.20 food`), which put
it one line above a `PER TURN` box quoting a *different* number for the same source — the dipped take
— with nothing on either saying which question each was answering. The terms of the bargain and the
take they are compared against were in two registers that did not know about each other; putting the
payoff INSIDE that box is what joins them. The caption is what says which take the headline is (see
the three-state table below), which is the other half of the same repair.

| state | face |
|---|---|
| offered | `🌱 Cultivate this patch` (`IMPROVEMENT_OFFER_BARE_FORMAT`) |
| gated | `▦ This ground is rich but too dry to farm — …` (`IMPROVEMENT_GATED_FORMAT`) |
| running | `🌱 Cultivating — 40%` (`IMPROVEMENT_RUNNING_BARE_FORMAT`) |
| done | `🌾 Tended Patch` (`IMPROVEMENT_DONE_FORMAT`; Corral's carries its upkeep) |

The `· then` formats and the `payoff_face` Callable that fed them are **deleted**, faces having no
terms left to compose. What survives on the control is the rung, the meter and the notes.

**The deal block is a SIBLING of the yields flow, never rows inside it** (`HudWidgets.build_improvement_deal`,
`IMPROVEMENT_DEAL_META`). Two harness contracts read that flow structurally — `Readout.yields_header`
takes the caption as `parent.get_child(index - 1)`, and both webs' take assertions parse the flow's
joined text by splitting on an account word — so a deal term folded in corrupts both, silently. Its
own block, its own meta, its own reader (`Readout.improvement_deal_text`).

**When it renders:**

| condition | block |
|---|---|
| no rung composed and none ungated-offered | not rendered at all |
| a rung OFFERED, or a rung COMPOSED, at any crew | the payoff row — the block's only row |

**IT IS ONE ROW, AND A SECOND ONE HAS BEEN TRIED AND RETIRED.** The block briefly carried a
`WITHOUT THE BUILD` row above the payoff, stating the crew's UNDIPPED take, on the reasoning that
ticking the box makes the headline the dipped figure and hides the baseline it is a fraction of. It
went for two reasons and the second is the one to remember:

- **The baseline is one click away.** Unticking the box shows it, live, in the register the player
  is already reading — so the row spent a line of the panel restating something the control directly
  above it produces on demand.
- **WHERE THE DIP COSTS NOTHING THE ROW PRINTED THE HEADLINE BACK.** The build fraction multiplies
  the CREW, not the ceiling, so a crew big enough to saturate the source pays no dip at all — and on
  one the undipped take and the dipped take are the same number. The panel then read
  `0.96 → 0.15 FOOD` over `WITHOUT THE BUILD 0.96 food`, under a crew note saying *each carries 50%
  as much*: correct arithmetic, unreadable panel.

`Readout.improvement_deal_rows` pins the count at 1 on both webs, and
`deal_repeats_a_yields_number` pins the general form — no magnitude in the deal may restate one the
take above it already prints. **A `contains` claim cannot see either return**: a re-added baseline
row satisfies every one of them, and its numbers are legitimate.

- **A GATED rung quotes no payoff, here or anywhere.** The gated control spends its whole slot on the
  unmet prerequisite deliberately — a number you cannot act on is noise at the moment you are told you
  cannot act — so `_improvement_deal_rung` answers `""` for one, and the readout does not put back what
  that branch removes. `improvement_offered_gated` / `forage_sow_locked` are the frames.
- **The row is floor-independent, so the block is deliberately OUT of the live registry.** A payoff
  is a property of the finished rung and `_improvement_deal_row` asks `improvement_forecast` at the
  FOOD PEAK, the floor `_improvement_payoff_terms` already quotes it at — nothing in the block moves
  under a drag, and that registry's own rule is that a host which does not move stays out of it.
- **The payoff follows the SELECTED CROP, resolved ONCE per sheet.** The forage builder computes
  `_crop_payoff_terms` a single time against `_improvement_deal_rung` and threads that string into the
  readout; the crop picker is built for the same rung. One seam, so the list and the terms cannot name
  different crops — the issue-#419 invariant, in its third home. The hunt sheet's equivalent is one
  `_improvement_payoff_terms` call.
- **A payoff of `""` renders NO row** — never a fabricated `0.00`. `improvement_forecast` answers `{}`
  for a rung the wire does not describe, and the bare face formats already followed the same rule.
- **Corral's feed rides the payoff row's value** (`IMPROVEMENT_DEAL_FEED_FORMAT`, `… − 0.14 feed`),
  because `corralYield` is GROSS and the row would otherwise promise a rate the pen never nets.
- **`IMPROVEMENT_DEAL_DEPLETED_NOTE` has now outlived TWO homes for the zero it explains** — a deal
  line's third term, then the control's face, now this row — and has stayed on
  `build_improvement_control`'s note slot throughout, because it is a warning about the RUNG rather
  than a footnote to whichever register prints the number. Frame: `herd_corral_depleted`.
- **The yields caption says which take it is.** A composed build passes a `while_building` FLAG — not
  a caption string — into `build_yields_row`, which is the only place that knows whether the readings
  also carry a holding rate, and `SourceForecast.yield_row_header` resolves the THREE states in ONE
  place:

  | building? | arrow present? | caption |
  |---|---|---|
  | no | no | `per turn` |
  | no | yes | `per turn · now → after` |
  | yes | — | `per turn · while building` |

  It shipped once as an UNCONDITIONAL override while a building row could still carry an arrow, which
  left the row's own `0.64 → 0.15` reading with no key at all on exactly the sheets that most need
  one — a caption that has stopped explaining a mark still on screen, which is worse than the
  ambiguity it was added to fix. Two call sites deciding separately is how that happened, so no
  caller composes a caption of its own; a caller with no per-turn rate AT ALL (the raid's trip
  payload) supplies its own `header` and never reaches the resolver. **`has_after` is still READ on
  the building branch's way in rather than assumed**, because the rows decide it and a widget
  inferring it from the flag would be a second opinion — but the resolver has nothing to do with it
  there, the fourth combination being unreachable:

  > #### A COMPOSED BUILD SUPPRESSES THE FLOOR WALK — both the `after` readings and the caption's key
  >
  > The readout was stacking two unrelated meanings of *later* with nothing marking them apart. The
  > row's `now → after` is the **floor walk at the CURRENT rung** — the burst take now, the steady
  > rate once the source is drawn to its floor, ~13 turns out on the reported frame. The `ONCE TENDED`
  > row directly beneath the caption is the **payoff at the NEXT rung**, after a ~25-turn build.
  > `PER TURN · WHILE BUILDING, NOW → AFTER` sat one line above `ONCE TENDED 1.20 food` and the
  > labelled row was read as the caption's *after*. Reported from play.
  >
  > So while an improvement is composed the sheet states ONE transition, and it is the one being
  > decided. **Nothing is lost**: the verdict two lines down narrates the walk in prose (*"Reaches the
  > floor in 13 turns, then holds it — taking only what grows back"*), which is why the fix is
  > suppression rather than a second label.
  >
  > **It is gated at the MODEL, in `DrawerComposeController._walks_to_the_floor`** — one seam both
  > `_forage_yield_model` and `_hunt_yield_model` compose their `after` dict through, beside the
  > `reaches` test that was already there. The caption's `has_after` is derived from the rows the
  > model emits, so gating at the render instead would let the arrow and the key over it disagree,
  > which is the failure this resolver has already paid for once.
  >
  > **Assert the caption AND the readings, or the claim is half made.** With the both-true caption
  > deleted, a model that kept its `after` renders the SAME `per turn · while building` caption over
  > rows still drawing arrows — measured: the header-only assertion passes with the suppression fully
  > reverted, and only the row claim fails. `improvement_running_plant` (plant) and
  > `improvement_running_animal` (animal, whose model rescales a quantised take through code the
  > plant one shares none of) each pin both halves. The plant frame carries the **non-vacuity** half
  > too: both claims are free on a crew that never reaches its floor, so the same patch, crew and
  > floor are re-composed with the box UNTICKED and must state the walk in full.
  >
  > `forage_unstaffed` is no longer the pair to a both-true frame — it is a crew of 0 with a build
  > composed, i.e. two reasons for one answer.
- **Assert the PAIR, never the absence alone** — "the payoff left the face" also passes on a sheet
  that lost the payoff entirely. Every frame that pins the face's `· then` absence
  (`IMPROVEMENT_PAYOFF_NEEDLE`, kept and re-pointed) also pins the payoff's presence in the readout by
  `IMPROVEMENT_DEAL_META`: `improvement_running_plant`, `improvement_running_animal`,
  `forage_crop_then_emmer` / `_groundnut` (which additionally prove the terms MOVE with the crop),
  `forage_unstaffed`, `forage_sow`, `herd_investment_both_products`, `herd_investment_corral_offer`,
  `herd_corral_depleted`.
- **THE SOW RUNG WAS UNPROVEN UNTIL ITS FIXTURE WAS REPAIRED, and the bug was in the harness.**
  `BaseFx.seed_forage_rows` converts the `patch_ceiling_*` authoring shorthand only *if the fraction
  key is absent* — and `food_tile_fixture()` has already run it once, writing
  `patch_sow_build_fraction = 0.0` and erasing the shorthand — so `ForageFx.sowable_tile_fixture`'s
  restated `patch_ceiling_sow` was ignored on the re-seed, `improvement_forecast` answered `{}` for
  Sow, and the rung quoted no deal on any frame in the corpus. It states the FRACTION outright now
  (`ForageFx.SOW_BUILD_FRACTION`, the docstring's own "a fixture that states a fraction outright
  wins"), and `forage_sow` composes the rung through the three-line idiom so the frame renders the
  selected Sow its own comment has always claimed. Its assertion is the rung's ASYMMETRY read off
  the rung's dipped headline against its payoff — `0.01 FOOD` while building against
  `ONCE SOWN 2.40 food` — as an ORDERING rather than against literals, since pinning either
  magnitude would pin this fixture's arithmetic.

**THE CONTROL SITS ABOVE THE READOUT ON BOTH SHEETS**, which is what makes the block legible at all:
… crew stepper → kit row → improvement control (+ crop picker) → `PER TURN` readout → action button.
It used to follow the readout, which put the terms BELOW the box they price. The spine
(`Spine.collect_compose_spine`) is unmoved — it tags on `IMPROVEMENT_CONTROL_META` and the readout
carries no spine tag — so the two webs' order assertions read exactly as before.

**`CREW_BUILD_DIP_NOTE_FORMAT` still avoids the phrase "while building", and the reason changed
again.** It was worded away from it because the deleted deal line's middle term carried it; the
harness then used the phrase as that line's absence needle; and the readout's caption prints it now.
The collision is live again, so the wording stays — the crew note is about the CREW's carry, not the
caption over the take. The `while building` absence needle is retired.

The dip itself is unchanged: `build_forecast` is the crew's own forecast with its throughput dipped
per account, and it is what the readout's `PER TURN` row has quoted since §3.1.

**A non-Sustain stance beside a running build is LEGAL and is not an error state.** It defeats itself
through the ecology, not through a gate: the build meter accrues only while the source is Thriving,
and Deplete is what drives it out. `ui_preview`'s `improvement_deplete_while_building` is that frame.

**On a LABOUR-BOUND crew it takes no more now, which is the trap at its sharpest** — and that frame's
crew is deliberately one (`ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS`, sized under the food peak's
dipped crew count). Since the dip moved onto the CREW (§3.1) a deeper floor frees a ceiling such a
crew cannot reach, so the rendered take is identical at both floors while the build rate the same
crew earns falls with the floor. **The frame's assertion said the opposite for a while and passed
anyway**, because it compared the whole yields STRING and that string carried a `now → after` reading
which did move; suppressing the walk under a composed build is what exposed it, and the claim is now
the equality — agreeing with the floor-independence claim beside it, which is the same number — over
a teaching-line companion that keeps it non-vacuous.

**THE RUNNING CONTROL STATES NO PAUSE, and the phase-keyed line that did is retired.** It was
`_tame_stalled_hint` (animal-only), then `_improvement_paused_note` on both webs, and both were true
of a sim that stopped a build outside `EcologyPhase::Thriving`. `docs/plan_harvest_floor.md` §3.2
replaced that cliff with a RATE, so the note fired on any non-Thriving source — a Cultivate on a
Stressed patch rendered `⚠ Paused — … this only advances while Thriving` beneath a meter the same
face showed advancing, and its remedy (ease workers off) was backwards, the FLOOR being what paces the
build. `IMPROVEMENT_PAUSED_FORMAT` went with it, and **the running control's `notes` array now carries
the pen's zero-payoff warning alone**.

What the sheet says in its place is what it already said better: the aside's live
`Building at ×0.30 — a higher floor builds faster`, and — for a build that genuinely accrues nothing,
which is an empty escapement room and never a phase — the turn estimate dropping out entirely.
`RungGates`' "deliberately NOT gated" note points at the pace, not at a note that no longer exists.

**The one asymmetry that survives, and must be kept:** the **Corral** done-state label carries the
pen's per-turn fodder upkeep (`🐄 Penned · 1.74 fodder/turn upkeep`) and the **Tame** one does not — a
penned herd cannot graze and a pastoral one still can, and a standing obligation belongs with the
standing state. `improvement_done_penned` and `improvement_done_animal` assert the two halves.

**Committing sends TWO commands, in this order:** `assign_labor` (crew + stance + crop) and then the
improvement's own verb (`cultivate` / `sow` / `tame` / `corral`, formatted by `Main.format_improvement`
off `HudLayer.improvement_requested`). The order is load-bearing — the sim's improvement commands act
on the bands **already working** the source, so a verb aimed at an unworked one is rejected outright.
`assign_labor` deliberately does not carry the improvement at all, which is what closes the
**re-staffing gap**: changing the crew of a *paused* build used to re-issue a command whose gates the
pause had failed, so the crew could not be changed at all.

### The two compose sheets read in ONE grammar

The forage sheet and the local-hunt sheet ask the same two questions in the same act, and a player
moving between them is reading ONE control layout. They now do, top to bottom:

```
band picker → stance picker → stance hint → crew stepper (+ its cap note) → forecast → improvement → commit
```

The hunt sheet used to put its crew stepper directly under the band picker — staff first, decide
after — which is the wrong order for the decision and a gratuitous difference besides. **Only the
POSITION moved**: the cap is still recomputed from the composed stance before the stepper renders (a
stance click re-renders and may auto-fill the crew) and the forecast still reads the current crew.
The **expedition branch follows the same spine**, its stance hint slot carrying the distance refusal;
it builds no improvement control, a detached party building nothing.

**A CREW OF 0 IS THREE STATES, on BOTH local sheets.** `workers == 0` means two different things and
the sim settles which: `assign_labor` skips validation entirely at 0, so an unassign is always legal.
The submit is therefore gated on whether it would CHANGE anything, never on the raw count (a
client-side floor of 1 would fix the no-op and break the unassign the Work zone's own Unassign link
depends on):

| crew | source worked by this band? | button | improvement control |
|---|---|---|---|
| > 0 | either | the verb (`Forage` / `Hunt Here`) | rendered |
| 0 | **yes** | `UNASSIGN_BUTTON`, **live** | **suppressed** |
| 0 | no | the verb, **dead** + a hint explaining it | rendered |

`HudComposeVocab.UNASSIGN_BUTTON` is ONE const for both sheets — two consts holding "Unassign" is how
the two drift apart — and the hunt web's dead-button hint is per CREW NOUN (`HUNT_NOOP_HINTS`, keyed by
the crew label the sheet already resolved, so a managed herd asks for a herder and a wild one for a
hunter). The hint yields to a cap note that has already explained the stepper, so the panel never states
one fact twice. **The improvement suppression is the same judgement on both webs**: what abandoning
costs is stated in the rung's own hint, so offering to START a build in the act of abandoning the source
both states one fact twice and says two opposite things. The `current` the test reads is the
PENDING-AWARE standing crew (`effective_*_workers`), so a just-issued assign counts. **The EXPEDITION
branch is deliberately not in this family** — a raid is a launch, not an edit of a standing assignment,
so there is no crew to hand back and a party of 0 is simply refused. ui_preview: `forage_unstaffed` /
`forage_unassign` and their twins `herd_hunt_unstaffed` / `herd_hunt_unassign`, the latter pair asserting
the rename, the dead button AND the absent improvement control (with a positive-crew open beside it, so
the absence is a change rather than a sheet that never offers that herd a rung).

**`ui_preview` asserts the ORDER, because a frame cannot.** `Spine.compose_spine` reduces an open sheet to
its structural controls — band picker, stance picker, crew stepper, improvement, each found by meta or
by node type, with the prose between them deliberately excluded — and the three sheets' spines are
captured at `food_tile` / `herd_hunt_expedition` / `herd_hunt_local_sustain`. Every sheet must open on
the shared HEAD (`band → policy → stepper`) and the two LOCAL spines must be EQUAL. Sabotage-verified:
putting the stepper back above the picker fails all three, naming the order it found.

**A stance hint states the rung's consequence for the SOURCE, and does not teach the ladder.** The
hunt Sustain hint carried "…is also how your people learn the next rung's craft: Herding on a wild
herd, Penning on a tamed one", and Eradicate opened its end clause with "No craft is learned". Both are
true and both were the **improvement line's** subject: a gated improvement control rendered
`◎ Your people know Herding 0% — ♻ hunt a wild herd to learn it` directly above the commit button,
exactly while the knowledge is incomplete. **That line has since gone too, and the lesson it carried
now lives in the ASIDE's teaching line** (`Teaching herding at ×1.38 …`), which says the same thing
live, quantified and at every floor — so the hint still must not teach the ladder, and the sentence
that superseded it is one row further up rather than gone. Two surfaces stating
one rule left the hunt hints markedly longer than the forage hints they sit beside for no information.
What must NOT be cut is a rung's own consequence — the Deplete decline, Eradicate's permanence, the
Corral feed cost.

**The averaging-window disclaimer lives in the rung BUTTON'S TOOLTIP** (`HUNT_AVG_WINDOW_FORMAT`,
appended under the tooltip's `<Name> — <metric>` line via the OPTIONAL `note` key of the rung's take
pair, which `_hunt_policy_takes` fills and the forage/expedition pickers leave unset). It is
load-bearing — a hunt lands WHOLE animals, so a player who reads `0.68 food/turn` off a mammoth and
then goes six turns with nothing would reasonably conclude the readout lied — but it is a caveat on ONE
number, and as a standing body line it cost the hunt sheet a wrapped sentence the forage sheet has no
counterpart for. The window computation is unchanged (`_hunt_avg_window_turns`, per rung, unknown
windows skipped); only where it renders moved.

**The commit button is a VERB and never restates the sheet's own header.** The sheet is already titled
`ASSIGN HUNTERS <herd>` / `ASSIGN FORAGERS <patch>`. So: forage `Forage` (unchanged), local hunt
**`Hunt Here`** (was "Assign Local Hunt" — "Here" is what carries the local-vs-expedition distinction),
expedition **`Send Expedition`** (was "Send hunting party"). **The `Send` STEM is load-bearing**:
`SourceForecast.style_send_hunt_button` rewrites that same button with the raid verdict — `Send Anyway
(≈54 turns)` / `Send Anyway (long raid)` / `Send (brings nothing home)` — so the resting face must be
the verb they vary. The disabled `Herd too lean to raid` is the one face that leaves the stem, and
deliberately: it is a refusal, not a send. Harnesses reach the send button through
`HudWidgets.SEND_HUNT_CONFIRM_META`, never by face, which is why a rename does not touch
`tools/command_guard.gd`.

**The forage sheet's green forecast line is NOT redundant with the rung face, and stays.** The face
shows the stance's worker-INDEPENDENT ceiling; the green line is what the CURRENT crew takes, scaled by
the band's `output_multiplier`, and it carries the sustainability verdict that becomes
`⚠ … — overdraws the patch`. They coincide only at the cap.

### What this deleted

Each existed only to undo the overload:

- **`INVESTMENT_POLICIES`** and every `policy in INVESTMENT_POLICIES` branch — the wire answers that
  question with a field now.
- **`forecast["investment"]`** and `FORECAST_PAYOFF_KEYS`-used-as-a-rung-test. `forecast_inputs` is
  stance-only; the payoff keys stay as *payoff* lookups and stop being how a surface asks "is this a
  build?". `herd_crew_floor` and `is_managed_hunt_source` take the improvement axis instead.
- **The selected-and-gated rung state (#420)** — the compose-sheet reset logic that rendered a
  standing-but-retired rung, `HudWidgets.build_policy_picker`'s gate machinery, the three
  `GATE_REASON_ALREADY_*` reasons, the gate-reason LAYOUT vocabulary
  (`GATE_REASON_LINE`/`HEADER`/`BULLET`/`COLLAPSED_*`/`TOOLTIP_SEPARATOR`) and the work inspector's
  standing-investment WARN line + discard confirm. **`HudStyle.apply_button`'s `selected_when_disabled`
  SURVIVES** — the flag was written for #420 but the crop picker's committed row is a genuine second
  caller (marked-and-locked is a real state for a readout), so only its #420 caller went.
- **The per-kind option lists** and the husbandry-ceiling filtering / standing-rung re-admission pass
  that maintained them.

## ONE CREW PER SOURCE, AND A `builders` POOL ON THE BAND (`docs/plan_standing_upkeep.md` §2.5)

> ### ⛔ THIS SECTION SUPERSEDES EVERY PASSAGE BELOW THAT GIVES A SOURCE ITS OWN BUILD CREW
>
> Those passages are kept because they still record *why* each surface reads the way it does; where
> one names a BUILDERS stepper, a per-source build crew, `improvement_workers`, `_mount_build_crew_row`
> or a verb carrying a worker count, read it against this.

A source carries **one** worker allocation from one band — its TAKE crew — and the compose sheet has
one stepper. The build's hands left the tile the way the keeping's did one slice earlier:

| Activity | Where the hands are stated | Command |
|---|---|---|
| **take** | the compose sheet's crew row (`_mount_crew_row`) | `assign_labor <f> <b> forage\|hunt … <n>` |
| **keeping** | the Band panel's `agriculture` / `husbandry` role cards | `assign_labor <f> <b> agriculture\|husbandry <n>` |
| **building** | the Band panel's **`builders`** role card, beside them | `assign_labor <f> <b> builders <n>` |

**A VERB DECLARES AND NAMES NOBODY.** `cultivate <f> <x> <y>` / `sow` / `corral` / `extend_pen` and
`tame <f> <herd>` **append an entry to the band's build queue**, and the whole `builders` pool funds
the **head** of that queue until its meter fills, then the next. **The trailing worker count those
verbs took for one slice is now a PARSE ERROR** — `sim_runtime::command_text` refuses the extra
token — so a stale emitter fails loudly rather than being reinterpreted.

**THE WITHDRAWAL IS ITS OWN VERB, and that is the live defect this folded in.**
`unqueue <faction> <x> <y>` / `unqueue <faction> <herd_id>` drops the queue entry and leaves the row,
its take crew, its kit and the meter exactly as they are. The crew-zero form it replaced —
`cultivate <f> <x> <y> 0` — *set* `improvement = Some(verb)` with nobody on it, and
`forage::patch_build_verb` honours a declaration at a zero meter, so the source read as building,
permanently, with no undo. `abandon` puts a source with work already banked down and is **command
line only in this slice**; so are `build_order` and the queue list itself (slice 7).

### THE SHEET COMPOSES ONE THING, AND §3.1 IS THE RULE THAT KEEPS IT THAT WAY

⛔ **DO NOT ADD A HYPOTHETICAL BUILD-CREW CONTROL IN ANY FORM.** With the pool at zero the sim
honestly publishes *no estimate*, and the tempting repair is a proposed crew to re-price it — which
is exactly the per-source build staffing this slice deletes, re-implied by a slider.

What the sheet shows instead is three published facts, all available before anything is queued: the
rung's `<rung>WorkCost` (the pile), its `<rung>UpkeepDemand` (the rate to hold it, forever) and the
turn count — quoted at the ACTING BAND'S OWN `builders` pool, which is the one number a player can
move and the one this sheet does not own. `ui_preview`'s countdown states assert the ABSENCE by
COUNT (`Readout.stepper_count == COMPOSE_STEPPERS_PER_SHEET`), which is what catches a control
re-added under any meta, any label or none.

### THE DECLARED STATE'S CREW TEST IS *ARE THEY ON THIS ONE*, WHICH IS THE QUEUE'S HEAD

The improvement control renders RUNNING only where work is banked **or** the pool is staffed AND
`SourceForecast.build_is_queue_head` says this entry is the one it is on. A staffed pool says nothing
about an entry waiting third in line, and reading it as *in flight* would put the one-way
`Cultivating 0 / 50 work (0%)` Label back on every queued-and-waiting rung — the state the DECLARED
checkbox was built to escape.

### THE COMMIT SENDS TWO COMMANDS, AND THE SHRINKING CREW GOES FIRST

One press, two commands, each judged on its own against the hands free **at that moment** — so the
order is part of the composition. `DrawerComposeController._commit_source` decides it, and the rule
is *whichever crew is shrinking goes first*.

It is provably sufficient rather than merely usually right. Each command reads back only its OWN
activity's crew on this source (`LaborAllocation::idle_for`, `set_assignment`'s `standing` term), so
the take is affordable iff `take ≤ idle + standing take` and the build iff
`build ≤ idle + standing build`.

> **⛔ THE SERVER SIDE OF THAT PAIR IS GONE.** `LaborAllocation::idle_for` and the improvement verbs'
> affordability refusal retired with the per-source build crew (`docs/plan_standing_upkeep.md` §2.5):
> a verb states *what* to raise and never *who*, so there is no per-source build number left to judge
> against anything. The builders are a band-level role, clamped by `assign_labor` against the band's
> idle hands exactly as scout and warrior are, and `set_assignment` no longer nets a standing build
> crew back out. What survives of the reasoning is the pool bound below, which is the one the sim
> still enforces. The pool clamp below guarantees
`take + build ≤ idle + standing take + standing build`, and that inequality forces at least one of
the two to hold already: a shrinking build is affordable outright, and a build that is NOT shrinking
leaves the take inside its own ceiling.

**It was a fixed take-then-build order, and that was correct only while the take stepper could not
exceed `idle + standing take`.** The shared pool is what made the other direction reachable — a
player moving two hands off a Cultivate and onto the gathering now composes a take the old order
would have clamped away *silently*, `assign_labor` trimming rather than refusing, and a trim to zero
DROPS the row and takes the build's own declaration with it.

**The swap does not reintroduce the "staff it first" rejection.** An improvement command reaches only
bands already working the source, and the build-first branch is taken only when the standing build
crew is positive — which means a staffed row exists to carry the verb.

### THE KEEPING WAS A THIRD CREW HERE, AND IT LEFT THE TILE (§2.5)

**There was a `KEEPERS` stepper under the readout on both sheets, and there is nothing left for it to
staff.** Maintenance is a band-level standing role now — `agriculture` for the plant web, `husbandry`
for the animal one — so a source is not staffed with keepers at all; it is paid a SHARE of its band's
pool. The whole control retired with the `maintain` command: `_mount_maintain_row`, its verdict
label, `maintain_requested` on `DrawerComposeController` and `HudLayer`, `Main.format_maintain` /
`_on_hud_maintain`, `ComposeState`'s two keeping counts and their accessors,
`HudBandLaborState.maintain_workers_for_*` / `assignable_maintain_workers_*` /
`assigned_keepers_for`, `HudWidgets.CREW_ROW_MAINTAIN_META` / `MAINTAIN_VERDICT_META`, and
`HudComposeVocab`'s whole `CREW_MAINTAIN_*` block.

**What replaced it is a card, not a stepper on a source**: the Band panel's KEEPING block, specified
in `band-city-panel.md` → "THE KEEPING BLOCK". `upkeep_mode_requested` is the one signal the roles
cannot express, and `Main.format_upkeep_mode` is its builder.

**The per-source READOUT stayed, and then HALF of it went too** (issue #545). `upkeepDemand` /
`upkeepSupplied` / `upkeepShortfall` still ship per patch and per herd, but the land card and the herd
drawer render the `At risk:` row ALONE: the standing `Keeping:` bill was reported from play as
unreadable, stating a cost every turn on a source where nothing was wrong. **A rung's keeping only
becomes a decision when it is SHORT**, which is exactly when `At risk:` renders. The full autopsy,
and the four-hazard rule that makes the resulting silence safe, are in `selection-card.md` →
"ONE ROW PER LIVE METER".

### THE SHEET IS ONE TRANSACTION OVER BOTH CREWS, AND IT IS CLAMPED AS ONE

`assign_labor` clamps and says so; the improvement verbs and `extend_pen` **refuse**,
naming the idle count (*"Cultivating needs 9 workers — the band has 4 idle."*). A silent trim on a
build is how the gathering it was meant to improve gets disbanded — so the sheet must never offer a
crew the band cannot staff.

**The pool is `idle + EVERY crew this band has committed on THIS source`**
(`HudBandLaborState.source_crew_pool_forage` / `_hunt`), and each stepper's ceiling is that pool
minus what the **other** stepper currently proposes: the take is capped at `min(pool − builders,
max-useful)` and the build at `pool − take`, resolved in that order so the build's ceiling reads the
take AFTER it has been clamped.

**IT REPLACED A PAIR OF PER-ACTIVITY CEILINGS, and the pair is what the bug was.** `idle + this
source's take` and `idle + this source's builders` were each the ceiling the sim judged ONE command
against (`LaborAllocation::idle_for`, now retired — see the callout above), and read on their own
they were correct. Read SIDE BY SIDE on a sheet that edits both, they describe a band with more
hands than it has in one direction and fewer in the other: reported from play, a band with four
hunters and no idle workers dropped HUNTERS to 2 and **BUILDERS stayed disabled at a maximum of 0**,
because that stepper's ceiling was computed against COMMITTED state and could not see the sheet's own
pending edit. The two hands only appeared after a commit, a close and a reopen.

**The pool cannot be crossed, which is the other half.** `take + build ≤ pool` holds by construction
— each stepper is capped at the pool minus the other — so no composition the sheet offers can exceed
what the band has. What that *did* newly reach is a take above `idle + standing take`, which one
command order cannot afford; the ordering rule above is the other half of this fix and neither is
correct without it.

**The keeping twins went with the keeping** — a role's stepper clamps on the band's plain idle count,
exactly as scout's and warrior's do.

**WHAT SURVIVES OF THE POOL IS THE STANDING TERM, AND IT IS WHAT MAKES A FULLY-ALLOCATED BAND
EDITABLE.** Clamped at `idle` alone a band with every hand committed caps at **0**: the player could
take a crew to nothing and never put it back, on the one source where the decision matters most. So
`source_crew_pool_*` is `idle + this source's own take crew` — a plain per-activity ceiling, which is
what it was before the two-crew sheet and is what it is again (`docs/plan_standing_upkeep.md` §2.5).
`compose_pool_take_full` / `compose_pool_take_freed` are RETIRED with the shared pool they staged;
`forage_reopened_crews` carries the surviving half.

**AND THE CAP NOTE HAS ONE REMEDY AGAIN.** `SourceForecast.LABOR_BOUND_NOTE_FORMAT` reads *"N of M
useful — free up idle workers to send more"*. `BUILD_BOUND_NOTE_FORMAT` — the same line naming this
sheet's own builders stepper first — is deleted with that stepper: there is one control here, so
there is one nearer lever, and `_forecast_worker_cap` takes no `build_crew` argument.

**Every clamp reads the wire's `idle_workers`, NOT `HudBandLaborState.effective_idle`.** The two agree
about the builders now (see below), but they still answer different questions: the sheet's ceiling is
judged against a REFUSAL the sim makes, and `idleWorkers` IS `BandWorkforce::idle()` — every staffed
hand across every activity and role, minus the bench — where `effective_idle` is an OPTIMISTIC answer
carrying the pending overlay. A ceiling composed from the optimistic one would offer a crew on the
strength of a command the server has not acknowledged.

### `effective_idle` SUMS `staffed_total`, AND FOR ONE RELEASE IT DID NOT

That helper summed each merged row's `workers` — the TAKE crew alone — so a band with three hands on
a Cultivate reported three idle who were already spent. Reported from play as **`3 idle of 18` beside
`Forage 9 · Hunt 6 · Idle 3`**, on a band whose every hand was committed: the builders miscounted AND
invisible, one defect wearing two faces.

**The sim was never in doubt.** `LaborAllocation::assigned_total` sums
`LaborAssignment::staffed_total`, and `HudBandLaborState.staffed_total` is that same sum client-side,
with `effective_idle` written in terms of it. What made the disagreement expressible at all was that
`effective_worker_map` did not copy the second crew through, so no consumer of that map could see a
build crew even to sum it.

> **THE SECOND CREW HAS SINCE LEFT THE ROW** (`docs/plan_standing_upkeep.md` §2.5).
> `improvementWorkers` is off the wire, `BUILD_WORKERS_KEY` and the map's copy of it are retired, the
> pending overlay preserves nothing but the declaration, and `staffed_total` is `workers` alone
> again. **The invariant survives its own mechanism, one shape over**: the builders are an ordinary
> `builders` ROW of the same list, so `effective_idle` still counts them — and a reader that started
> filtering that list by KIND would put the same phantom hands back.

**What the miscount cost was every ceiling built on top of it** — the four role cards' steppers, the
pen-extend crew, `benchable_workers` and so the crafting bench's own — each offering a crew the sim
then refused, while the panel showed the phantom hands that justified the offer.

**THE SWEEP FOUND THREE MORE CONSUMERS AND ONE DELIBERATE NON-CONSUMER**, and the distinction is the
useful part:

| consumer | what it was doing | now |
|---|---|---|
| `BandPanelController._build_workforce_block` | segments summed to `working_age − builders` | a **`Build`** segment (`HudWorkVocab.WORKFORCE_KEY_BUILD`, `SIGNAL_DEEP`) beside Forage and Hunt, counted off the band's **`builders` ROLE** since §2.5 |
| `FactionRollup._build_workforce_block` | the same hole one scale up | the same segment, the two bars being one chart at two scales. **Its `Roles` segment was missing the KEEPING pair as well** — the band zone's twin has counted them since they landed — so a faction keeping anything lost those hands off a chart that partitions the same `working_age` its header sums |
| `BandPanelController._work_source_models` | admitted a row on `workers > 0`, so a source with ONLY builders left the board | **back to the take crew**, §2.5: there is no per-source build crew to admit a row on, and the queue that spends the pool is a list of its own (slice 7). The inspector's `N building` clause went with it |
| `HudBandLaborState._effort_on` | sums the take crew alone | **unchanged, deliberately** — its readers ask *"is anybody HARVESTING this"*, and a builder harvests nothing |

**The builders are their own SEGMENT rather than folded into Forage/Hunt**, for the reason the bench's
is its own: those two name what a crew TAKES, `effective_idle` nets a builder out, and a bar whose
segments are supposed to partition `working_age` must account for every hand it removes from Idle.
`band_panel_preview`'s `_assert_people_matches_workforce` is what catches a missing one — it sums the
RENDERED chips against `working_age`, so a bar short of the build segment fails by exactly its count.

### ONE crew is readable, and the sheet opens on it

**`LaborAssignment` publishes `workers`, and that is the whole of a row's staffing** — the take crew.
Its `improvementWorkers` twin was write-only for one slice, readable for one more, and is off the
wire (`docs/plan_standing_upkeep.md` §2.5). `maintainWorkers` went the same way one slice earlier.

- **The take stepper seeds from the band's own row** (`seed_forage` / `seed_hunt`). A reopened sheet
  opens on what the band HAS, which is what makes a restate possible: the commands SET rather than
  add, so a stepper opening at `0` on a staffed source would offer only the choice to unstaff it.
- **`_emit_improvement` HAS NO CREW HALF TO TEST.** Its no-op test was *"same verb AND same crew"*,
  so a sheet opened and closed on a staffed build sent nothing; the verb states no crew now, so an
  unchanged declaration is simply not an order and the test is the verb alone. Unticking is a
  DIFFERENT command (`unqueue`), not the same one at zero.
- **`forage_reopened_crews` KEPT ITS NAME THROUGH BOTH RETIREMENTS and its subject is down to one
  crew.** What it proves now is that a band with `idle_workers == 0` can still restate the take it
  already has — the standing term in `source_crew_pool_forage`, which is the surviving half of the
  retired shared pool — and that the sheet mounts **exactly one stepper**, asserted by COUNT rather
  than by the absence of a retired tag, since a restored row would carry no tag and a tag search
  would pass vacuously.

### The crop seeds from the ASSIGNMENT, not from the ground

`seed_forage` used to clear the composed crop outright, on the reasoning that a crop pick belongs to
the PATCH it was made on. True of a tile the band does not work; wrong for one it does — and the
assignment carries the player's own `species` now.

**It is the SELECTION, and it is deliberately not `ForagePatchState.committedSpecies`.** The patch's
field is what the GROUND is committed to and only exists once a crew has worked it; the assignment's
exists from the moment the player chose. **On unworked ground it is the only record there is**, so a
sheet reopened there re-resolved to the tile's dominant plant and silently re-pointed a 25-turn
commitment. `HudBandLaborState.species_for_forage` is the reader; `""` stays a real instruction
(*"pick the tile's dominant legal plant for me"*) rather than an absent one.

### THE DIP IS RETIRED, and so is `crew_needed`

`yield_fraction_while_building` said *"this crew is preparing ground, not gathering"* — true of a
SHARED crew and of nothing else. With the allocations split, **a build takes nothing off what the
gatherers carry**, and what it costs is the hands standing on it. Deleted, not defaulted to `1.0`:

- `SourceForecast.build_dip` / `FORECAST_BUILD_FRACTION_KEYS` / `NO_BUILD_DIP` are gone, and with them
  the `dip` parameter on the whole engagement family (`engagement_per_worker` / `engage_workers` /
  `engagement_carry` / `take_workers` / `animals_engaged` / `crew_to_clear` / `crew_to_hold` /
  `crew_that_reaches` / `engaged_quantum`) and the `improvement` parameter on `forecast_inputs`,
  `hold_crew`, `reach_crew`, `take_draws_down` and `herd_axis_rates` — every one of which existed only
  to reach the fraction.
- `improvement_forecast` carries ONE forecast. The *preparing* middle term (`build_forecast`,
  `build_fraction`) is gone: the take is the same number before, during and after the build, so a
  second reading of it would state one fact twice. **What now decides a rung is unquotable is its
  own `workCost`** — `BUILD_WORK_COST_NONE` is the wire saying it prices no such job here, which the
  dip fraction used to be.
- `YIELD_ROW_HEADER_WHILE_BUILDING` and the `while_building` flag are gone from `build_yields_row` /
  `yield_row_header` / `_mount_readout` / `_fill_yields_host`. The caption is keyed by `has_after`
  alone. (The floor-walk SUPPRESSION survives — `_walks_to_the_floor` — for its own reason: two
  unlabelled "laters" in one box.)
- `plant_crew_floor` and `<rung>CrewNeeded` are gone. That floor existed because the cap divided a
  DIPPED ceiling, so a 25-turn improvement asked for fewer hands than gathering the same ground; the
  quotient is honest now. **`workers_needed` fell 12 → 3 on the reference patch**, and that is the
  same retirement seen from the wire.
- **`herd_crew_floor` and `useful_floor` went with them, on BOTH cap twins**
  (`SourceForecast.source_worker_cap_state`, `DrawerComposeController._forecast_worker_cap`). They
  raised the TAKE cap to a managed herd's `herdersNeeded`, because one crew both hunted the animals
  and held them — a cap sized on the take alone went dead below the count the sim asked for while the
  same row rendered the under-herded ⚠. **Those keepers are the MAINTAIN allocation now**, with their
  own stepper, their own ceiling and their own command, so flooring the take stepper on them staffed
  one crew against another crew's demand. The *hold* crew is still folded in and is a different
  thing: it is a fact about this take at this floor, not a demand a KIND of source makes, so it stays
  inside `max_useful_workers` where both twins pick it up. Frames: `herd_tame_worker_cap` and
  `herd_tame_worker_cap_sustain`, now a pair asserting the two read the SAME cap.
- The crew row's *"— building this rung, each carries 25% as much"* note is gone
  (`build_crew_dip_note`, `CREW_BUILD_DIP_NOTE_FORMAT`, `CREW_ROW_DIP_META`); the row it explained
  now carries no penalty to explain.

### The build's closed form lost its floor term, and then lost the RATE too

```text
gear(b)  = min(b, buildWorkSaturatingCrew) × buildWorkPerWorker
net(b)   = b × buildWorkPerWorkerTurn − meterRotPerTurn
turns(b) = ceil((workCost − workDone − gear(b)) / net(b))
```

`b` is the BAND'S `builders` POOL, and it is READ rather than composed (`docs/plan_standing_upkeep.md`
§4). It was the improvement control's own stepper for one slice; that stepper is retired with the
per-source build crew, so `SourceForecast.build_turns_at` is called with
`HudBandLaborState.workers_for_role(band, "builders")` — the ACTING band's, the one the commit names —
and the gear is resolved over those same builders, a tool's contribution being a rate per worker.

**IT WAS KEPT RATHER THAN DELETED for two reasons that outlived the stepper.** The Builders role card
wants the same curve (drag the pool, watch the head's date fall), and `build_turns_closed_form.rs`
pins this expression against the sim's — two producers of one estimate that are required to agree at
the committed crew.

**THE CARD'S OWN `build_crew` IS A DIFFERENT QUESTION AND HAS A DIFFERENT ANSWER.**
`HudBandLaborState.build_crew_forage` / `build_crew_hunt` fold the pool across every band that WORKS
the source, because *"is anybody building this"* is a fact about the source; the sheet asks *"what
would MY band's pool make of it"*, which is one band's. The restriction on the fold is what keeps a
single staffed builder from putting a crew on every source on the map.

#### THE RATE IS NOT A TAX ON BUILDING, and the term left behind is the ROT (§4.6a)

**The maintenance rate used to be supplied by the BUILD crew while a meter was being raised**, so only
its surplus was progress and the pace was `work_cost / (crew − rate)`. That was defensible while the
build crew stood on the tile. **The keeping pool owes the rate at every fullness now**
(`docs/plan_standing_upkeep.md` §2.4 — the fullness test decided who paid, and pooling both crews
broke it in two directions at once), so a build crew supplies nothing towards it and its **whole
output is progress**.

**What can still stop a build finishing is the ROT** — what an under-kept meter loses per turn — so
the `∞` pair survives with a new denominator:

- **`meterRotPerTurn`** on `ForagePatchState` and `HerdTelemetryState` is what the source's at-risk
  meter is losing right now, in work units, read through `SourceForecast.meter_rot_per_turn`. Always
  meaningful, never a sentinel: `0` when the keeping covers it, when the source is inside its grace,
  and — structurally — on **every animal source**, neither animal rung declaring a `meter_decay`
  because its penalty is a shed.
- **It is PER SOURCE and takes no improvement**, and it is a **constant with respect to the stepper**,
  which is exactly why the sim publishes it rather than the client composing it: the crew the player
  is dragging moves the progress and never the rot. The client holds neither the grace state nor the
  rung's decay rate, so re-deriving it is not available either.
- **On a rung NOBODY HAS STARTED the quote is `workCost / crew`.** Nothing is banked, so nothing can
  rot. **That is not issue #545 returning**: one builder against `plant:tended` really does bank one
  work a turn now, and the 2.0 is the keeping pool's bill.
- **A FIFTH ANSWER RIDES THE SAME FIELD SINCE §4.6b** — `BUILD_TURNS_QUEUE_BLOCKED` (`-4`), the
  band's builders staffed and standing on an entry its own gate refuses. It is not this form's to
  return: the client evaluates the arithmetic and a queue is not arithmetic, so it reaches
  `build_turns_remaining` from the wire alone and `build_pace` classifies it as a FOURTH arm
  (`BUILD_PACE_BLOCKED`). Blocked ≠ holding ≠ rotting ≠ silent, and a client rendering three of them
  cannot derive the fourth.
- `core_sim/tests/build_turns_closed_form.rs` still pins the two producers equal at the committed
  crew, and `SourceForecast.BUILD_BALANCE_HOLDS` is still the ONE cut point the two `∞` answers fork
  on.

> **ONE THING TO WATCH, and it is the sim's to answer.** On shipped config no plant rung bleeds faster
> than one worker banks — `plant:tended`'s `meter_decay.per_turn` is 0.5, `plant:field`'s 0.75 — and
> the animal web rots at nothing by construction. So neither `∞` is currently reachable at a staffed
> build crew, and the harness stages the rot directly in order to render them.

#### `<rung>UpkeepDemand` IS THE STANDING PRICE, AND IT IS NEVER A THRESHOLD

`cultivationUpkeepDemand` / `fieldUpkeepDemand` on `ForagePatchState`, `tameUpkeepDemand` /
`corralUpkeepDemand` on `HerdTelemetryState` — each the LADDER's rate for that rung, published
unconditionally **exactly as `workCost` is**, read through
`SourceForecast.FORECAST_BUILD_UPKEEP_DEMAND_KEYS` / `build_upkeep_demand`.

They arrived (issue #545) as the term the closed form subtracted, because the source's own
`upkeepDemand` reads `0` on a source with no progress and so vanished at exactly the moment the sheet
quotes a rung nobody has started. **The subtraction is gone and the fields are not**: what they answer
is *what will holding this cost me, every turn, forever*, which is the half of a commitment the
one-off `workCost` cannot state.

- **It renders on the OFFERED face beside the build's price**, through
  `DetailFormat.build_price_clause` and `HudComposeVocab.BUILD_PRICE_UPKEEP_FORMAT` —
  `🌱 Cultivate this patch — 50 work, ≈25 turns · 2 work a turn from Agriculture to hold`. **In WORK,
  never in hands**, and nothing compares it to a crew. A rung the wire prices no rate on states no
  standing clause.
- **IT NAMES THE POOL THAT PAYS, and for a release it did not.** The clause read
  `· 2 work a turn to hold`, and reported from play it was meaningless in that context: the rate
  never said WHO owes it, so on a sheet whose every other number is about the crew under the stepper
  it read as a demand on that crew. It is not — it is the band's AGRICULTURE or HUSBANDRY role, and
  those cards are the only controls that move it. `build_price_clause` therefore takes the SOURCE
  kind, and the word is `HudWorkVocab.keeping_role_name`, i.e. the same per-web pair the work row's
  under-kept note and the blocked-queue remedy already key on — so no two surfaces can send the
  player to different cards.
- **IT DELIBERATELY DOES NOT SAY `then`.** `· then ` is the RETIRED payoff clause's own phrase
  (`🌱 Cultivate this patch · then 1.20 food`) and is still the needle every *"the face quotes no
  payoff"* assertion greps for; a standing price wearing it makes those assertions find this clause
  instead. Same avoidance, same reason, as the crew note's refusal to say `while building`.
- **A RUNNING face carries no price at all** (it carries the meter), so this is the OFFERED and
  DECLARED states' alone — which is where the decision is being made anyway.
- **The rung is picked with the SAME key table the cost is**, so price, meter and rate can never name
  three different rungs. That is the whole safety argument for a per-rung table rather than a scalar.
- **`upkeepDemand` keeps its own meaning and its own readers.** It resolves through the AT-RISK rung,
  which is the right answer for *what is this source losing* and the wrong one for *what would this
  rung cost to hold*. Both ship; nothing may substitute one for the other.
- **The plant pair is still deliberately NOT in `MapView.FOW_DISCOVERED_HIDDEN_KEYS`, and for ONE
  reason now.** Both plant rungs declare `scaled_by: flat`, so the figure is the ladder's and reads
  identically on every patch in the game — there is no live patch state in it to leak. The second
  reason it used to carry — that redacting it would cost the closed form its rate term — died with the
  subtraction. **`patch_meter_rot_per_turn` IS redacted**, beside the shortfall it is derived from,
  and nothing is lost by that: a remembered tile's whole build payload is redacted, so the estimate
  already answers `BUILD_TURNS_NO_ESTIMATE` there for want of a cost.
- **`cargo xtask decode-guard` goes RED on `meterRotPerTurn` and is re-recorded** (`--write-golden`).

#### THERE IS NO BUILD-CREW THRESHOLD LEFT TO STATE

`_mount_build_crew_row` named one — the quoted rung's rate, as `2 work a turn holds it — the surplus
is progress` beside the stepper, then as that row label's tooltip over `HudWidgets.
BUILD_WORK_FLOOR_META`. **The mechanism it warned about no longer exists**, so the note is the exact
failure this arc keeps producing: a warning outliving its mechanism. `SourceForecast.min_build_work`,
`HudWidgets.BUILD_WORK_FLOOR_META`, `HudComposeVocab.CREW_BUILD_FLOOR_TOOLTIP` and the harness's
`ForageFx.build_work_floor` / `build_work_floor_tooltip` are all deleted, and the smallest useful
build crew is one hand. **The row that answered *how many hands* is retired too** (§2.5) — the
question is the Band panel's Builders role card's now — and the compose face still says which way the
meter is moving at the pool the band has, in its own ink.

**A CREW AT OR BELOW THE ROT NEVER FINISHES, and that is an ANSWER rather than an absence.** It is
its own sentinel, kept apart from `BUILD_TURNS_NO_ESTIMATE`
because the two render differently — see "∞ IS THE ANSWER" below.

#### THE "NO ANSWER" BOUNDARY IS *IS THERE WORK BANKED*, NOT *IS ANYONE STAFFED*

`build_turns_at` refused to price a crew of nobody, on the premise that nobody has promised anything.
**The sim's own boundary moved** (§4.6a): with the keeping pool holding a meter at any fullness, zero
builders is a real, common and reportable state, and the wire publishes the meter's fate for it —
`-2` where the keeping covers it, `-3` where it does not. Both are this same closed form at `b = 0`,
so the client evaluates it there rather than dropping out, and the two producers agree at a committed
crew of zero as they do at any other.

- **Nothing banked AND nobody on it is still no answer** — that is the DECLARED state, and its own
  *not started* warning speaks for it.
- **The WORK PREDICATE IS ASKED AT EVERY STAFFING, INCLUDING NONE.** It was gated on
  `workers > BUILD_CREW_NONE` for one pass, on the reasoning that nothing accrues at zero builders
  anyway — **true, and not this predicate's question.** `RungDef::build_accrual`'s `eligible` carries
  `crew_is_working_the_source`, which reads the STOCK against the floor and takes **no crew count at
  all**, so the sim answers `-1` there whatever the staffing. The gate made the sheet answer the
  neutral `held` on a floor-starved half-built Cultivate the card correctly called `⚠ Stalled` —
  **two producers disagreeing about one meter**, which is the exact thing the closed-form equality
  exists to prevent. Found by review; the `per_worker_turn <= 0` guard beside it was gated the same
  way and was ungated with it. Frame: `tile_meter_stalled`, which asserts the card's hazard and the
  sheet's `BUILD_TURNS_NO_ESTIMATE` for the same crew of zero.
- **`-2` WITH NO BUILDERS IS NOT A HAZARD.** That is the whole point of the boundary moving, and it is
  `BUILD_PACE_HELD` — see `selection-card.md` → "THE HELD ROW IS THE ONE STATE HERE THAT MUST NOT BE
  MARKED".
- **AND THE COMPOSE FACE SAYS `held`, NOT A NEUTRAL `∞`.** `DetailFormat.build_turns_clause` takes the
  crew for it (`HudComposeVocab.BUILD_TURNS_HELD`), so the sheet and the card say the **same word**
  about the same state — the property that makes a two-producer pair trustworthy. **The `∞` may not be
  spent on a benign state**: it is `FOOD_UNLIMITED_GLYPH`, shared with the larder runway on the
  strength of a player learning a mark once and reading it everywhere, so using it where nothing is
  wrong teaches that it sometimes means nothing is wrong — the hazard rule running backwards. The ink
  stays neutral either way, which is why without the word this face would be the arc's loudest mark in
  its quietest colour, saying nothing. `BUILD_CREW_ANY` is the default for a caller with no staffing in
  hand and keeps the `∞`, a warning wrongly withheld being the failure this family exists to prevent.
- **The map badge reads the same verdict**, through `SourceForecast.build_is_losing` — a bare-prefix
  reader beside `unstaffed_build_of`, so a map renderer need not reach for a HUD vocabulary module to
  spell a key prefix. A LOSING meter drops its percentage (a falling number is the same lie as a
  frozen one); a parked one keeps it, because that number is honest.

**THIS IS THE ONE COMPARISON THE CLIENT STILL MAKES, and it survives only because the sim cannot
answer the question it asks.** `buildTurnsRemaining` publishes its answers for the source as the sim
sees it — chained down the band's queue — so every crewless surface READS the verdict; the sheet is
pricing the rung a player is COMPOSING, which is by definition one the sim has not queued. **The sim's two boundaries are honoured here by
construction rather than by a second opinion**: an UNSTAFFED source takes the `workers <=
BUILD_CREW_NONE` branch and answers `BUILD_TURNS_NO_ESTIMATE`, and a rung whose gate refuses it is
never priced at all — a GATED control spends its whole slot on the reason and quotes no price, so this
is never reached for one.

**`learn_multiplier(floor)` is no longer a factor.** It scaled the accrual when one crew did both
jobs (*a crew pulling hard on the source it is improving builds slowly*); a build crew is not pulling
anything, so the sim's `build_accrual` takes no floor and neither does this. **The floor is still a
parameter**, because the WORK PREDICATE reads it: `crew_is_working_the_source` still gates Cultivate
and Tame, so a floor standing above the patch's stock leaves no room and the estimate drops out.

That retirement reaches the aside too: `TEACHING_RATE_BUILD_TAIL` and `TEACHING_BUILD_ONLY_FORMAT`
are deleted, `teaching_note` takes no `building`, and `floor_chart_model` takes no `improvement` at
all. A lesson the faction already knows now leaves the aside **silent** on a building sheet, which is
what it always meant: the top of the dial buys that source nothing further.

### WHAT IT COSTS TO HOLD IT, on every surface that shows the improvement

`SourceForecast.upkeep_state(src, prefix)` is the ONE reader of the four published fields
(`upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` / `upkeepWorkersNeeded`) plus the
`hasNeglectGrace` / `neglectGraceRemaining` pair. **The shortfall is READ, never derived from
`demand − supplied`** — it IS what the meter decays by, and a client that subtracted would be a second
authority over the number the whole readout exists to make legible.

`DetailFormat.at_risk_lines` renders it identically on **both webs** — the land card and the herd
drawer both append it — as an `At risk:` row, and ONLY when the keeping is underpaid.

**THE STANDING BILL IS RETIRED** (issue #545). `Keeping: the pool covers 1 of 2 work — worth 2
keepers` rendered on every source that owed anything, which is every held rung in the game, and
reported from play it could not be read: a rung's keeping becomes a decision only when it is SHORT.
The wording it had was right for the question it answered — `upkeepSupplied` is this source's SHARE of
the band's pool, not the keepers standing on it, so it said `the pool covers` rather than reading as a
staffing verdict that would send the player looking for a stepper that no longer exists — and the
question stopped being worth a row. `selection-card.md` → "RETIRED — `Keepers:` and `Keeping:`" is the
full autopsy, including the four-hazard rule that makes the resulting silence safe.

**`upkeepWorkersNeeded` IS STILL PUBLISHED AND STILL A SIZE RATHER THAN AN ORDER** — what this
source's keeping is worth in hands — and `keepers_wanted` still reads it, for the one line that
survives the retirement: `HERDERS_SHED_FORMAT`, which quotes the count precisely because a head count
only matters when it is short.

**`keepers_wanted` / `is_under_kept` sit beside it over the same reader**, and the animal web's two
under-kept surfaces both call it: the herd drawer's Husbandry row MARK and the work board's
under-herded ⚠. **`is_under_kept` IS A SHORTFALL TEST NOW, and it had to become one.** It compared a herd's
`maintain` crew against `upkeepWorkersNeeded` — the right question while keepers were staffed per
source, and unanswerable once maintenance left the tile: that count is `0` on every managed herd in
the game, so the ⚠ would have been permanently up. What the sim decides per source is its share, and
`upkeepShortfall` is exactly *"the share did not cover this one"* — the same number the decay and the
shed read. **The old objection to a shortfall test is answered by the GRACE**: `neglectGraceRemaining`
counts the forgiven turns, which the `At risk:` row beside the warning states, so the notice still
arrives before the animals go. See `band-city-panel.md` → "The under-herded ⚠".

**BOTH WARNINGS ARE GATED ON THE METER NOW, AND BOTH WOULD OTHERWISE HAVE BROKEN IN OPPOSITE
DIRECTIONS** when `upkeepWorkersNeeded` began publishing mid-build. They took their `kind` for it, so
each can ask `build_is_in_flight` of the right web:

- **`is_under_kept` requires NO build in flight.** Left on `crew > 0 and short` it would light the
  under-herded ⚠ on every source mid-Tame or mid-Cultivate, on a row whose remedy is a different
  sentence.
- **`is_unbuilt_and_unpaid` requires one.** It was `keepers_wanted == 0 and short` — *"nobody is owed
  keepers, so this must be a build"* — which is an inference off a field's meaning rather than a state
  test, and it goes permanently FALSE mid-build: the mid-build warning would have silently stopped
  existing on both webs, with the keeper warning firing in its place.

> #### THE MERGE LEFT NOTHING ROUTING THE CARD'S MARK, AND THAT NEEDED FIXING
>
> `is_under_kept` answers for the SOURCE — one pool, one shortfall — and the `build_is_in_flight` gate
> was incidentally doing a second job: keeping the mark off the built row of a patch whose FIELD is the
> short meter. With the gate gone, both rows lit.
>
> **Only ONE meter on a source is ever at risk** — the newest one carrying work, which is what
> `forage::patch_unwinding_rung` resolves and what `upkeepDemand` / `upkeepShortfall` /
> `meterRotPerTurn` are published *for*. `SourceForecast.at_risk_rung` is the client's transcription of
> that newest-first walk (the same table and the same direction `build_verb` uses), and
> `rung_is_under_kept` is what puts the source's answer on the row it belongs to.
>
> **It decides which ROW displays a number, never the number.** The shortfall is the sim's and is
> untouched; routing it is the client's job and the client already does it for the build verb. It is
> NOT `build_verb` itself, and the difference is a full meter: that one answers *what is being built*
> and so returns `IMPROVEMENT_NONE` at a meter standing at its cost — which is precisely when the rung
> is being maintained, and therefore when it is at risk.
>
> **The withheld mark costs the player nothing**, because the routing withholds it from one ROW and
> never the fact: `at_risk_lines`' source-level `At risk:` row still states what the shortfall costs
> and how many turns are left.

**THE TWO MERGED, BECAUSE THE DISTINCTION HAD NOTHING LEFT UNDER IT** (§4.6a). They were two tests
because the two states were owed by **different crews** — a rung that STOOD was owed its keepers, a
rung still going up was owed its BUILDERS. One pool owes both now, at every fullness, so the shortfall
means the same thing, the remedy is the same sentence, and keeping them apart had the work board
telling a player to **staff BUILDERS for a bill the keeping pool owes**. `is_under_kept` is the one
test, `build_is_in_flight` no longer gates it, and `is_unbuilt_and_unpaid`, `WORK_ROW_UNBUILT_NOTE`,
`WORK_ROW_UNBUILT_TOOLTIP` and the work row's second `unbuilt` model flag are all deleted.

**WHAT THE MERGE COST, stated plainly**: the note no longer distinguishes a rung being RAISED from one
being HELD. It does not need to — the player does the same thing either way — and the source's own card
still says which, on the rung row that carries the meter. What replaced the pair is a per-WEB pair
instead (`HudWorkVocab.under_kept_note` / `under_kept_tooltip`, keyed on the row's labor kind), because
*animals drifting off* and *ground slipping* are different consequences with different role cards.

**The shortfall is still what it fires on, and no headcount substitutes**: the wire publishes no
crew requirement for it, so a count derived by dividing it would be the client inventing a number the
sim never stated. **There is no build-crew threshold to quote beside it either** — see "THERE IS NO
BUILD-CREW THRESHOLD LEFT TO STATE" above.

**THE EDGE IS A CLIFF, WHICH IS WHY THE COUNTDOWN IS ON THE CARD.** A completed meter sits exactly at
its own cost, so the FIRST bleeding turn drops it below and the rung is **lost** — three unkept turns
costs a tended patch, two costs a Field. A player who loses a 25-turn investment with no warning reads
it as a bug, so the warning stands wherever the improvement does rather than only in an alert.

**A METER STILL BEING RAISED IS OWED KEEPERS, AND ITS DEMAND IS NOT ZERO** (§4.6a). A patch
mid-Cultivate publishes a non-zero `upkeepDemand` — and **the band's KEEPING pool is what covers it**,
from the first work banked until the last. That is the fullness test's deletion seen from this
surface: the sim used to leave an unbuilt rung out of the pool and credit its BUILD crew instead, so a
half-built meter whose builders walked away could not be held by idle keepers and a held rung that
dipped commandeered the band's builders. One pool, at every fullness, and the readouts below did not
have to move for it.

**THAT FORK USED TO BE A ROW AND IS NOW THE RUNG ROW'S OWN `∞`** (issue #545). The retired sentence
said *"still being built — its own crew pays 2 work a turn"* when the bill was met and *"its builders
are not covering that — this rung is sliding back"* when it was not — a fact about the BUILD, stated
one row away from the meter the player would act on. `BUILD_TURNS_NEVER` is the same fact where it
belongs, and the `At risk:` row beneath it prices what the shortfall costs. `build_is_in_flight` is
still the state test both remaining warnings gate on, and it is still asked structurally rather than
inferred off a zero keeper count — `upkeepWorkersNeeded` publishes on both sides of completion, so
that inference has been dead since it began doing so. Frames: `herd_keeping_mid_build` (with its
unpaid twin asserted beside it, PNG-less, since only the shortfall separates the two states) and
`improvement_rung_slipped`'s land card.

### RETIRED — `abandon_improvement`, and what walking away is now

**Unchecking a running build is gone, and so is the command it sent** (`docs/plan_standing_upkeep.md`
§2.4). It existed to clear an assignment's STORED `improvement`, back when that field was the
commitment; the verb is DERIVED from the meter now, so there is no stored authority left to clear and
a command that cleared a derived value would either do nothing or fight the derivation.

**UNTICKING SENDS `unqueue <faction> <source>`** — a verb of its own, not the set verb at a smaller
crew. `DrawerComposeController._emit_improvement` is where the two meet: an unchecked box emits
`unqueue_requested`, a changed declaration emits `improvement_requested`, and a source with neither a
live meter nor a declaration sends nothing at all. `Main.format_improvement` and
`Main.format_unqueue` are the two builders, and the split is the grammar's: one names a RUNG, the
other names a SOURCE (two integer tokens are a tile, one token is a herd id — the sim's parser's own
rule). `format_abandon_improvement`, `Main._on_hud_improvement`'s empty-improvement branch,
`IMPROVEMENT_ABANDON_HINTS` and `IMPROVEMENT_TOOLTIP_SEPARATOR` are all deleted.

**A RUNNING BUILD CANNOT BE WITHDRAWN FROM THE SHEET AT ALL, and that is honest rather than a gap.**
Its control is a `Label`, the verb is derived from the METER, and what puts down a source with work
already banked is `abandon` — command line only in this slice. `ui_preview`'s
`_assert_walk_away_emits` asserts exactly that pair: the running control is a Label, the commit sends
no improvement order, and `format_unqueue` renders the line the withdrawal WOULD carry for that
source.

> #### ⛔ THE SERVER VERB TAKES NO CREW AT ALL, AND THE WITHDRAWAL IS `unqueue` — BOTH HALVES NOW LIVE
>
> This callout landed while only the SERVER had moved; the client half is built, and the section
> above is written against it. What follows is the mechanism
> (`docs/plan_standing_upkeep.md` §2.5). `cultivate|sow|tame|corral <target…>` and `extend_pen` no
> longer accept a trailing worker count — the `workers` field is `reserved` on every one of their
> proto messages, and the text parser refuses the extra token. A verb **declares**: it appends an
> entry to the band's build queue, and the hands stand on
> `assign_labor <faction> <band> builders <n>`.
>
> So a crew-zero send is not a walk-away and never was one: it set `improvement = Some(Cultivate)`
> with no builders, and `forage::patch_build_verb` honours a declaration whenever its meter is at
> zero, so the source went on reading as **building, with no builders**, permanently and with no
> undo.
>
> **The undo is its own verb now**: `unqueue <faction> <x> <y>` / `unqueue <faction> <herd_id>` drops
> the queue entry and leaves the row, its take crew, its kit and the meter exactly as they are, and
> `abandon` on the same grammar puts the whole **holding** down. What the client renders of the
> in-between state is honest about it: a declared, unstaffed rung is `IMPROVEMENT_STATE_DECLARED` —
> a live, ticked box over the *not started* warning, the readout for a build that is going nowhere.
>
> `.claude/rules/core_sim/intensification.md` → "THE UNDO IS `unqueue`, AND `abandon` PUTS THE WHOLE
> SOURCE DOWN" carries the server-side mechanism.

**THE RUNNING CONTROL IS A `Label`, and the shape rule the GATED state already stated covers three of
the five.** The control's TYPE says whether this is a CHOICE or a FACT: a `CheckBox` is a choice
(OFFERED and DECLARED), and running/done/gated are Labels told apart by
`HudWidgets.IMPROVEMENT_STATE_META` (added for exactly that, since the type no longer separates them).

### A DECLARATION IS NOT A BUILD, and rendering one as RUNNING was a ONE-WAY DOOR

`build_verb` honours a declaration at a zero meter, and the control took that answer as *running*. So
a player who ticked `cultivate` on a band with no free hands got a `Label` reading
**`🌱 Cultivating 0 / 50 work (0%)`** with the *not started* warning under it and **no box left to
untick** — reported from play. The declaration was expressible and its withdrawal was not.

**The control now asks whether a build is ACTUALLY in flight**, which is the wire's own two facts:
work banked on its meter (`improvement_progress > BUILD_METER_UNSTARTED`), or the band's `builders`
pool staffed AND this entry at the HEAD of the queue it funds
(`SourceForecast.build_is_queue_head`). Neither ⇒ **`IMPROVEMENT_STATE_DECLARED`**: the OFFER's own
face (verb and price, through the one `_improvement_offer_face` both states share) on a **ticked,
live** checkbox, over the *not started* note — with the pen's zero-payoff note riding beneath exactly
as it rides a running build, that being a warning about the RUNG rather than about work in flight,
which is why one predicate (`_rung_pays_nothing_under_its_feed`) serves both.

- **THE HEAD TEST IS THE HALF §4.6b ADDED, and without it the one-way door comes back.** The whole
  pool goes on the head entry, so a staffed pool says nothing about an entry waiting third in line;
  reading it as *in flight* would put the `Cultivating 0 / 50 work (0%)` Label back on every
  queued-and-waiting rung.
- **Unticking sends `unqueue`**, which really does withdraw the declaration — the crew-zero form it
  replaced re-SET it. There is no second lever to agree with: the BUILDERS stepper that used to sit
  beneath this control is retired.
- **A DECLARED box is NEVER disabled, however few hands the band has** — and neither is the OFFER one
  branch down, since §2.5. Declaring costs no hands at all now, so there is nothing to refuse in
  advance.
- **The *not started* note is GATED ON THE POOL rather than unconditional**, and it names the ROLE.
  It fired on every declared rung while DECLARED meant *nobody is building this*; a declaration is
  queued now, so a band with builders on the role really is going to raise it and the face beside the
  note quotes the date. With the pool empty it is as true as it ever was — and it reads *"nobody is on
  this band's Builders role"*, because *"set the builders below"* pointed at a control that no longer
  exists, which is the warning-outliving-its-mechanism failure this arc keeps producing.
- **`BUILD_UNSTAFFED_UNSTARTED` can no longer reach the RUNNING branch** — it is *no crew and no work*,
  i.e. exactly `not in_flight` — so that branch's notes carry `BUILD_SLIDING_NOTE` alone. A dead
  `elif` there would read as a live case.

### RETIRED — the DEAD OFFER, because declaring costs no hands

An OFFERED control greyed out with `HudComposeVocab.BUILD_NO_HANDS_REASON` on a band whose build pool
was empty, refusing in advance the click that produced the one-way state above: ticking it declared a
build WITH a crew, and the sim refused a count the band could not staff rather than trimming it.

**A VERB DECLARES NOW** (`docs/plan_standing_upkeep.md` §2.5). Ticking appends a queue entry, which is
legal and free whether or not anybody stands on the `builders` role — an entry that is waiting costs
nothing and loses nothing, the keeping pool holding its meter — so there is nothing left to refuse and
`BUILD_NO_HANDS_REASON` is deleted. What says nobody is building it is the DECLARED control's own
*not started* note, and what fixes it is the Band panel's Builders role card. `compose_offer_no_hands`
keeps its name and its fixture and asserts the inverse: on a band with NO idle hands the box is
OFFERED, LIVE, and mounts no builders control of any kind.

**`disabled_reason` survives as the ONE thing that disables a box**, with no caller passing one today.
It was `disabled = not notes.is_empty()`, which was dead logic (a gated rung reaches the GATED branch,
never this one) and which would have silently killed the box the moment a live state grew a note of
its own — which is exactly what DECLARED does.

**THE UNGATED RULE OUTLIVED BOTH CONTROLS IT WAS WRITTEN FOR, and it is now a rule about the SHEET.**
Nothing may withhold the remedy on a stalled build — that is when a player reaches for it — so no
state of this control gates itself away, however loudly the sheet reads. It was first about the OFFER
(a dead offer that hid its box), then about the BUILDERS row mounted on a RUNNING control; §2.5 retired
both, and the remedy the rule protects is now the Band panel's Builders card, which no compose sheet
can withhold at all. `improvement_no_room_plant` is the frame where withholding would look
defensible — and its own state moved with the mechanism: the patch there is unqueued at a meter of
zero, so it renders DECLARED rather than RUNNING.

### A BUILD THAT IS NOT MOVING DOES NOT GET TO WEAR A PERCENT — one verdict, two surfaces

`SourceForecast.build_is_stalled(src, progress, build_workers)` is the ONE answer to *"should this
build wear a `⚠`?"*: `true` when the rung is declared-and-never-started with nobody on it
(`unstaffed_build_of` → `build_is_unstaffed`), or when the WIRE says its meter is going backwards
(`build_is_losing`, which classifies the sim's own `buildTurnsRemaining`). `false` for a climbing
build **and** for one merely PARKED with its keeping covered — `BUILD_PACE_HELD` is a decision, not a
failure, and its number is honest.

**IT EXISTS BECAUSE THE MAP AND THE WORK BOARD ANSWERED DIFFERENTLY.** `BandOverlayRenderer`'s source
badge composed the pair itself and dropped the percentage; `BandPanelController._build_work_row` had
no such fork and printed `▦45%` in `HudStyle.SIGNAL_DEEP` whatever the staffing. Reported from play as
the map showing an alert the WORK tab did not — one screen, two verdicts, and the wrong one was the
one with the number on it. **Two careful copies was the shape of the defect, so the fix is one
producer**: both surfaces call this and neither re-derives it from a crew count or from the percentage
sitting beside it.

- **The inputs are the caller's ALREADY-RESOLVED rung**, not a second resolution: `progress` is the
  meter `RungGates.rung_in_progress` just answered with, so the warning and the glyph provably
  describe the same verb.
- **`build_workers` is the BAND's `builders` POOL** (`docs/plan_standing_upkeep.md` §2.5) — a verb
  declares and names no hands. The board resolves it ONCE per render for the whole model set
  (`effective_role_workers`, pending-aware like every other readout on that panel); the map SUMS it
  across bands, because *"nobody is building this"* is a claim about the SOURCE.
- **The two faces are deliberate TWINS rather than a shared constant.** A HUD controller must not
  import a map renderer, so `HudWorkVocab.WORK_ROW_BUILDING_UNSTAFFED_FORMAT` (`%s⚠`) and
  `BandOverlayRenderer.BADGE_UNSTAFFED_FORMAT` (`%s⚠ `, with the trailing space its plate needs for
  the crew count) are separate strings that cross-reference each other. What must not be duplicated is
  the VERDICT, and it is not.
- **The row's TOOLTIP forks with the face.** A hover still quoting `45% done` beside a `⚠` restates
  the exact number the mark has just withdrawn, so a stalled build takes
  `WORK_ROW_BUILDING_UNSTAFFED_TOOLTIP_FORMAT`, which names both remedies (the Builders role, or the
  source's keeping) because the row cannot tell which half fired without stating a number it has
  refused to state.

**Frame + assertions:** `band_panel_preview`'s `band_panel_work_build_states` — ONE band, FOUR forage
rows differing only in their meter and the wire's countdown (climbing · declared-with-nobody-on-it ·
losing ground · parked-with-keeping-covered), with all four faces and all four inks asserted by
EQUALITY. **The SET is the claim**: a row builder that marked everything passes the two stalled
claims, and one that marked nothing passes the two healthy ones. `_assert_work_row_and_badge_agree`
beside it is the two-surface claim no per-surface assertion can make — each is perfectly
self-consistent while contradicting the other — and its counts (four build rows, exactly two stalled)
are what stop a hard-wired verdict agreeing with itself.

### THE BUILD LINE'S STATE IS ITS COLOUR, AND THE PROSE THAT SAID IT IS DELETED

`2 work a turn holds it — the surplus is progress` sat beside the BUILDERS stepper, one line under the
build line whose own ink says which side of that rate the crew is on. It was prose doing a colour's
job, and it is gone; the state goes on the `Cultivating` / `Domesticating` line itself, three ways:

| pace | net | the face reads | ink |
|---|---|---|---|
| `BUILD_PACE_GROWING` | > 0 | a real turn count | `HudStyle.HEALTHY` |
| `BUILD_PACE_HOLDING` | = 0 | `∞ turns` | `HudStyle.WARN` |
| `BUILD_PACE_LOSING` | < 0 | `∞ turns` | `HudStyle.DANGER` |

**`SourceForecast.build_pace(turns, unstaffed_state)` is the one classifier, and the discipline is
that it CLASSIFIES rather than derives.** It reads the sentinel the estimate already answered with and
the unstaffed state the meters already decided; comparing a composed `work_per_turn` against zero
would be a second opinion about a number the sim owns — the drift that once quoted `≈50 turns` for a
build that never moved.

- **THE WIRE SPELLS BOTH `∞` STATES AND EACH HAS ITS OWN PACE.** `sim_schema::BUILD_METER_HOLDS`
  (`-2`, the crew exactly at the rate) answers `HOLDING`; `BUILD_METER_ROTS` (`-3`, the crew under it)
  answers `LOSING`, and that is the red the schema promises for work already bought and now bleeding.
  The amber covered both for one slice — the conservative reading while there was one sentinel — and
  reading it that way *after* the split told a player whose build was being destroyed exactly what it
  tells one merely treading water. **`BUILD_PACE_HELD` is the fourth** (§4.6a): the same `-2` with
  NOBODY on it, which is a build parked on purpose rather than a crew wasting a turn. It is in neither
  the colour table nor `improvement_pace_stops`, and the fall-through IS the render — neutral ink, no
  mark. `BUILD_UNSTAFFED_SLIDING` retired into this fork; the wire, not the staffing, says which.
- **A CHECKBOX takes only the two stopping paces** (`HudWidgets.improvement_pace_stops`): green on an
  unticked offer would read as an achievement on a rung nobody has started.
- **The RATE is not lost, and it is not on this row either.** It became the offered face's STANDING
  PRICE (`· 2 work a turn to hold`), which is what it always was — see "`<rung>UpkeepDemand` IS THE
  STANDING PRICE" above. The BUILDERS row states no number of its own.

**Frames**: `improvement_turns_lone_crew` (red, `∞` — a lone builder under a short-kept patch's own
rot) / `improvement_turns_full_crew` (green,
`≈10 turns`) — one patch, one floor, only the builders moving — and `improvement_rung_slipped` (red,
the sliding meter). `tile_build_unstaffed` carries the DECLARED checkbox and `compose_offer_no_hands`
the dead offer with its reason.

**What the retired tooltip said is still true and still differs by web**: unstaffing a plant build
lets its meter BLEED at the rung's `decay_per_turn` (~100 turns to zero) while an animal meter is KEPT
(`domestication` is monotone-up and the pen rung declares no decay). Nothing is refunded on either.
That reaches the player through the source's own rung row and the `At risk:` row beneath it, which
state the live shortfall and the turns of grace left rather than a hypothetical about a control. **Deliberately
no confirm dialog** — a stepper tick is not a destructive act, and it is re-made by ticking back.

### THE BUILD VERB IS DERIVED FROM THE METER, AND THE DECLARATION ONLY ANSWERS AT ZERO

`SourceForecast.build_verb(src, prefix, kind, declared)` is the client's transcription of
`forage::patch_build_verb` / `fauna::herd_build_verb`, and the ONE answer to *"is a build in flight
here, and which rung"*:

| meter | state | who declares |
|---|---|---|
| **zero** | nothing in flight | **the player** — a wild patch could climb to tended *or* be sown |
| **between zero and its cost** | building that rung, **implied** | nobody — the progress banked on it IS the answer |
| **at its cost** | maintaining | nobody |

**NEWEST METER FIRST**, exactly as the sim resolves it, so a Field with progress governs the tended
ground beneath and a `Cultivate` declared on a Field is dead rather than stalled. `declared` is the
assignment's `improvement` (or a box the player just ticked) and is honoured **only at a zero meter**,
which is what makes a spent one inert rather than something to clean up.

- **`live_improvement` is GONE**, and this is more than its replacement. That helper filtered a verb
  naming an already-BUILT rung; the derivation does that AND the opposite — it adopts a build the
  meters say is running where nothing was ever declared.
- **IT PUT THE REPAIR ON SCREEN.** Completion freed the declaration, so a completed rung that eroded
  back below its cost re-entered the *building* state with nothing set: the sheet rendered its DONE
  label, the work board's badge went quiet, and there was no way back but re-issuing the command the
  player had never withdrawn. **A player who has paid for a rung and watched it slip adds HANDS.**
- **FULLNESS AND ACHIEVEMENT STAY ORTHOGONAL.** This reads the meter's fullness (who pays the rate);
  `improvement_is_done` reads the stamped retention bar (what the ground pays out). A patch at 90% is
  **building** — a repair — and **still tended**. Folding them would make a rung's LOSS and a rung's
  REPAIR one edge.
- **EVERY SURFACE READS IT.** The compose control's RUNNING branch, both sheets' `standing`/`composed`
  pair at the commit, and `RungGates.rung_in_progress` — which is the work board's row mark and the
  map badge, and which was keyed on the stored verb until this. Frame: `improvement_rung_slipped`.
- **A FIXTURE THAT WANTS AN OFFER MUST NOW SAY SO.** A patch carrying progress IS building it, so the
  reference tile's own 0.6 meter makes it a patch mid-Cultivate wherever it is used;
  `BaseFx.unbuilt(tile)` zeroes both meters, re-prices the absolutes and drops the keeping with the
  rung (a patch on no rung owes nothing, and the demand is a term of the build's pace now).

### CLOSED — the build's PRICE and its turn estimate are on the wire now

This section read *"No build RATE on the wire — `progress_per_turn` is sim-side config, so the
control cannot say `~25 turns` / `~10 turns left`"*. That was true of a normalized meter and is not
true of a work-costed one (`docs/plan_unit_costed_work.md` §8): a rung declares a fixed size in WORK
UNITS, a crew produces work units per turn, and **TURNS ARE THE OUTPUT** — so the sim publishes the
absolutes AND the answer.

- The improvement control's RUNNING face states the meter in work
  (`🌱 Cultivating 30 / 50 work (60%) — ≈20 turns`) and its OFFERED face quotes the job before the
  player commits (`🐄 Pen this herd — 75 work, ≈19 turns`). `workCost` is published whether or not a
  build is in flight, which is what makes the pre-commit quote possible at all.

#### TWO SURFACES ASK DIFFERENT QUESTIONS, so the estimate has two producers

**The compose sheet EVALUATES the estimate; the tile card and the herd drawer RENDER the sim's.**
That is the boundary `.claude/rules/core_sim/yield-forecast.md` → "THE BOUNDARY, stated once" draws —
*where a closed form exists the sim ships the TERMS and the client evaluates it; where one does not,
the sim ships ANSWERS* — and the build's turn count is the one row that ships **both shapes**,
because the sheet has a crew stepper and a floor slider and the card has neither.

| surface | producer | why |
|---|---|---|
| compose sheet, both faces | `SourceForecast.build_turns_at` — the ceiling's discipline | there is a PROPOSAL to price: the stepper's crew, the slider's floor, the picker's kit. *Add hands and watch it drop* is the whole point of the reading, and it sits beside the control that moves it |
| tile card · herd drawer | `SourceForecast.build_turns_remaining` — the `penFeedUpkeep` discipline | no crew control, so the only question is *what is happening here*, which is exactly what the sim's answer for the committed crew says |

```text
working  = improvement ∈ {cultivate, tame} ⇒ max(0, biomass − floor × carryingCapacity) > 0
gear(w)  = min(w, kitTiers[kit].buildWorkSaturatingCrew) × kitTiers[kit].buildWorkPerWorker
left(w)  = workCost − workDone − gear(w)
turns(w) = 1 where left(w) <= 0, else ceil(left(w) / (w × buildWorkPerWorkerTurn × learn_multiplier(floor)))
```

**The SOURCE carries one term and the KIT carries the other**, and which side each sits on is what
makes the estimate re-price when the picker moves:

| term | rides | why |
|---|---|---|
| `buildWorkPerWorkerTurn` | the source row | it is what one worker banks on THIS source. **Read, never assumed to be the `1.0` it is today** — the sim writes worker output as a sum of terms so a future buff can land there, and a client holding the constant would quote a number the sim disagrees with |
| `buildWorkPerWorker` · `buildWorkSaturatingCrew` | `PopulationCohortState.kitTiers[]`, read through `KitRoster.build_gear` | both facts behind them — the units this band holds and each unit's reach — are the BAND's ledger, so a rung nobody has started still has a quote and the sheet prices the kit the picker is OFFERING rather than whatever the crew last carried |

**The `min` is on the HEAD COUNT, and it is what makes the gear half exact rather than approximate**:
coverage arms a prefix of the party, so an eleventh worker with ten sets of hurdles between them
contributes nothing — without it the sheet quotes a build finishing sooner the more hands are added to
it. The floor term is `learn_multiplier`, not a second spelling of `floor / FLOOR_FOOD_PEAK` — that
helper is the client's one copy of the sim's `MSY_BIOMASS_FRACTION`, and a peak written out again is
how the sheet and the chart's teaching rail come to disagree about what a floor buys.

**THE WORK PREDICATE IS PART OF THE FORM, and it rides TWO rungs and not four.** `RungDef::
build_accrual`'s `eligible` carries `systems::labor::crew_is_working_the_source` — *is anything
standing above this assignment's floor?* — on **Cultivate** and **Tame**, so a floor above the
source's own stock fraction accrues nothing while `learn_multiplier` is at its LARGEST. A sheet
without the term quoted the fastest estimate on the whole axis for a build the sim was not advancing,
beside a tile card correctly rendering no turn line at all. `SourceForecast.escapement_room` is the
client's copy of `max(0, B − floor·K)`, and this is the ONE reader of it that is not a ceiling —
admitted for the same reason the ceilings are, that the sheet prices a crew and a floor nobody has
committed. **`Sow` and `Corral` omit it in the sim and must omit it here**: bare ground stands below
every floor by construction, so requiring room would make rung 3's create-from-nothing case
unquotable, and a pen is fenced around a herd already drawn to its keeper's floor.

> **`buildWorkFromGear` on the SOURCE is a different question and must not be read here.** It is the
> RESOLVED contribution for the crew that worked that source this turn — the tile card's and the herd
> drawer's `−17 work off this job` line — so it answers for a committed crew and a committed kit, and
> a stepper cannot move it.

**Evaluated at the COMMITTED crew and floor the two producers agree exactly**, and that equality is
the safety argument for having both: a sheet that could disagree would lie about the very decision
the card then reports differently. Pinned sim-side on the exported snapshot by
`core_sim/tests/build_turns_closed_form.rs`.

**The control is in the live-refresh registry**, and by that registry's own rule: its value depends
on the floor, and a floor DRAG may not rebuild the sheet (the rebuild frees the chart and the gesture
dies with it). The crew half needs nothing — a stepper tick rebuilds the sheet outright.

`SourceForecast.BUILD_TURNS_NO_ESTIMATE` (`-1`) renders as **no clause at all** on both producers; a
`0` in its place promises a build about to land. The sheet's producer answers it for a crew of
nobody, a rung the wire prices nothing on, a source banking nothing per worker-turn, and a crew
standing over an empty escapement room — each of them *there is no question here yet* rather than an
answer about a crew the player has stated. The **sim** answers it for the same class of states, plus
two more of its own that no client may re-derive: an unstaffed source, and a build whose knowledge,
site or species gate does not hold. **A crew that is merely too SMALL is the other sentinel**
(`BUILD_TURNS_NEVER`), and it must render on every producer: see "∞ IS THE ANSWER" below.

### ∞ IS THE ANSWER FOR A CREW THAT NEVER FINISHES, AND IT IS AMBER

`BUILD_TURNS_NEVER` renders as `∞ turns` on **both** compose faces — the running face's tail and the
offered face's price — through `DetailFormat.build_turns_clause`, which spells every count in one
place. It wears no `≈`: every other reading there is an estimate that could come in early or late, and
this one is not an estimate at all.

**AND ON THE TILE CARD AND THE HERD DRAWER, WHICH IS WHERE IT MATTERS MOST.** It is `-2` on the wire
(`sim_schema::BUILD_METER_HOLDS`, beside `NO_BUILD_TURNS_ESTIMATE`'s `-1` and `BUILD_METER_ROTS`'
`-3`), so `SourceForecast.build_turns_remaining` reads it rather than flattening every negative, and
`DetailFormat.rung_row_value` renders it as the rung row itself — `⚠ ∞ turns (42%)`, in the same
amber. The two were ONE sentinel for a release and this row was silent for it, so the state a player
has to act on was visible only while dragging a stepper they may never open. **The `at this crew` tail
retired with the indented sub-row it lived on** (issue #545): the row IS the crew's answer now, and the
state and the count are one string that cannot disagree. The card's own half of this — the four-hazard
fork, the shared tint rule keyed on the mark, and the two sim boundaries a client-side comparison
would blur — is in `selection-card.md` → "ONE ROW PER LIVE METER".

**THE GLYPH IS THE LARDER RUNWAY'S, AND THE MEANING IS INVERTED.** `DetailFormat.FOOD_UNLIMITED_GLYPH`
is `∞` on the Food line for *your larder never empties*; on a build it is the worst news the sheet can
carry. So `BUILD_TURNS_NEVER_GLYPH` is that same const — a player who has learned the mark on the food
line reads it here without being taught twice — and the **INK** is what says which way it points:
`HudWidgets.build_improvement_control`'s `warn_face` inks the whole face `HudStyle.WARN`, the
treatment the shortfall rows wear. A face is one `Label`/`CheckBox` and takes one colour, so the
warning cannot ride the clause alone, which is the honest treatment anyway: what is wrong is the whole
proposition on that line. The ink is decided by `SourceForecast.build_pace` and applied through
`HudWidgets.improvement_pace_color`, so the ∞ and its colour cannot appear without each other.

**`DetailFormat.build_turns_never` IS RETIRED, and it was one of the sites the split slipped past.**
Its doc called it *"the single test both compose faces gate their warning ink on"* and it had been
reached by nobody since the pace classifier took that job — a three-state fork cannot be gated on a
bool. What made the corpse worth deleting rather than leaving is that it went on special-casing `-2`
alone: a reader checking whether this client had followed the sentinel split would have found a *yes*
that meant nothing.

### THE SECOND `∞` IS RED, AND THE CARD SAYS *losing ground* IN WORDS

`sim_schema` split `BUILD_METER_ROTS` (`-3`) out of `BUILD_METER_HOLDS` (`-2`), and the difference is
what the player is being told: **holding wastes the crew's turn, rotting destroys work already paid
for.** The client did not follow, and every symptom of that was a silence in the reassuring direction:

- `SourceForecast.build_turns_remaining` accepted `>= 0` and `-2` and mapped everything else to *no
  estimate*, so a real, staffed, priced build that was actively bleeding banked work rendered as the
  STALLED hazard — *a gate refuses this, no crew size fixes it* — when the remedy is precisely more
  hands. Before issue #545's stalled fallback it rendered as **no row at all**.
- `build_pace` fell to `BUILD_PACE_UNKNOWN`, i.e. neutral ink on the compose face, for the same value.
- **It was reachable at the most common early staffing there was**: one builder on a Cultivate against
  `plant:tended`'s rate of `2.0` netted `−1`, and the A/B frames in `chapters/improvements.gd` had been
  staging it since the pace landed and reading amber. **§4.6a moved that ground**: the rate is not a
  build term any more, so what a crew races is the meter's ROT — which no shipped plant rung produces
  faster than one worker banks. The A/B stages the rot directly for it (`_short_kept_food_tile`), and
  whether the state is reachable in PLAY is the sim's question rather than this client's.

**BOTH ANSWERS WEAR THE `∞`, because both are true of the meter**; what separates them is the INK, and
on the card also the WORDS. `HudSelectionVocab.RUNG_ROTTING_FORMAT` renders
`⚠ ∞ turns, losing ground (42%)` in `HudStyle.DANGER` against the holding row's `⚠ ∞ turns (42%)` in
amber, and **`RUNG_ROTTING_PHRASE` is passed INTO the format rather than spelled inside it** — the
phrase the row prints is the needle `DetailFormat.rung_value_hex` keys the red off, the same
one-definition shape the BUILT badges use. That tint test runs **first**, because the rotting row
leads with the hazard mark like every other failure state (it must, or the mark stops meaning
*something is wrong here*), so an amber branch tested first swallows it and the split exists on the
wire and nowhere on screen.

**A compose FACE gets the ink and not the words**, because it is one Control taking one colour and its
line already carries the meter and the price. The card has a whole row to spend and spends it.

**`build_turns_at` FORKS ON THE SAME CUT POINT** (`SourceForecast.BUILD_BALANCE_HOLDS`, the client's
copy of `intensification::BUILD_BALANCE_HOLDS`), which is not optional tidiness: the sheet and the
card are two producers of one estimate and are required to agree at the committed crew. A sheet
answering HOLDS where the card answers ROTS quotes amber for the crew the card paints red — the two
disagreeing about the very decision the stepper is being dragged through.

**The harness was pinning the defect, which is the part worth remembering.**
`chapters/improvements.gd` declared `UNKNOWN_BUILD_TURNS_SENTINEL := -3` and asserted that value
*"renders as NO answer"* — a claim written when `-3` really was unspelled, left in place when the wire
grew it. **A sentinel-is-unknown claim has to be re-aimed the day the schema spells that value**; it
is now `-4`, one past the last one defined, and it moves again the next time the schema grows.

**ON THE OFFER THE FACE GOES AMBER AND THE BOX STAYS LIVE.** A crew below the rot makes the job
unfinishable, not illegal — the player may staff it anyway and add hands next turn. **The tint has to
be applied AFTER `HudStyle.apply_checkbox` and on every interaction slot**
(`HudWidgets.CHECKBOX_LIVE_FONT_COLOR_SLOTS`): that helper writes `font_color` plus hover/pressed/focus
itself, so an override set before it is silently overwritten and one set on `font_color` alone reverts
to neutral ink under the pointer.

**AND THE STEPPER STATES NO THRESHOLD — THEN THE STEPPER ITSELF WENT** (§4.6a, then §2.5).
`_mount_build_crew_row` carried a threshold for two slices — `2 work a turn holds it — the surplus is
progress` beside the stepper, then that rate as the row label's tooltip — naming a bar a build crew
had to clear before any of its output was progress. **No rung declares such a bar**: the keeping pool
owes the rate at every fullness, so one hand is the smallest useful build crew and the rate is a PRICE
the offered face quotes. The whole chain went with it — `SourceForecast.min_build_work`,
`HudWidgets.BUILD_WORK_FLOOR_META`, `HudComposeVocab.CREW_BUILD_FLOOR_TOOLTIP` and the harness's two
readers — and one slice later the ROW went too, with `HudWidgets.BUILD_CREW_ROW_META` and
`HudComposeVocab.CREW_ROW_BUILD_LABEL`, a verb having stopped naming a crew at all.

**IT HAD ALREADY BEEN A HEAD COUNT AND THAT WAS WRONG TWICE OVER** (`min_build_crew`, the published
`upkeepWorkersNeeded`): this model is denominated in work units end to end, so *"2 hold it"* quietly
reintroduced the worker as the unit, and the field behind it reads `0` on a source with no progress,
so the note fell silent on exactly the pre-commit quote it existed for. It was restated in work, and
then the mechanism under it was deleted. **Both corrections point the same way** — the note was
answering a question the model had stopped asking, which is why what replaced it is a price rather
than a smaller warning.

**Frames + assertions** (`chapters/improvements.gd`, the `improvement_turns_*` A/B — one patch, one
floor, only the band's `builders` POOL moving, staffed through `BandFx.staff_builders` where the
harness used to dial a stepper): the lone crew's `∞` **and** its warning ink, the four-hand crew's
`≈10 turns` **and** its absence of that ink, and the ABSENCE of any threshold on both sides.
Each half is asserted as a PAIR: an always-amber face and an always-quiet one each satisfy one claim
alone. **The A/B is staged on a SHORT-KEPT patch now** (`_short_kept_food_tile`, a stated
`meter_rot_per_turn` with the `patch_upkeep_shortfall` that explains it), because the reference patch
rots at nothing and every staffed builder on it climbs — which is the model working, and would have
asserted the `∞` away had the fixture not moved with it.

**A JOB THE GEAR ALONE PAYS OFF IS `BUILD_FINISHES_IN_ONE_TURN`, NOT "no estimate"** — the client's
transcription of `intensification::BUILD_FINISHES_IN_ONE_TURN`, which the sim returns for the same two
states: the work is already banked, or the crew's gear covers the job outright
(`LadderConfig::effective_build_cost` is unfloored, so a well-equipped crew drives the bar to or below
zero). Both finish on the first worked turn (`docs/plan_unit_costed_work.md` §6.2), which is an
ANSWER — and the two constants must not be conflated, because withholding the line broke this arc's
own headline claim at exactly the crew that demonstrates it: **it is reachable on shipped config**, a
start-stocked band's 26 `hurdles` at 8.5 apiece covering a 50-unit Tame at six keepers,
so the estimate fell 25 → 13 → 4 → 2 → *nothing* as hands were added, beside a tile card correctly
reading `≈1 turn at this crew`.

**And the count is SPELLED in one place, for both faces** — `DetailFormat.build_turns_clause`, which
forks the singular (`≈1 turn` / `≈25 turns`). The two faces quote one estimate about one job, so a
build one turn out reading `≈1 turns` on the sheet beside the tile card's own `≈1 turn (98%)`
(`HudSelectionVocab.RUNG_TURNS_ONE_FORMAT`) would be the same number worded two ways on one screen.
`BUILD_PRICE_TURNS_FORMAT` / `IMPROVEMENT_RUNNING_TURNS_FORMAT` therefore take the clause already
spelled and never a raw count.
- The percentage is **still the `*_progress` fraction**, never `workDone / workCost` re-derived here.
  The wire ships both and they are exactly each other; dividing client-side would be a second
  authority over one meter. `HudSelectionVocab.BUILD_METER_WORK_FORMAT` /
  `DetailFormat.build_meter_value` are **the compose sheet's alone now** (issue #545) — the tile card
  and the herd drawer state a rung's turns and its percentage, the work absolutes being what you read
  while COMPOSING a build rather than while glancing at one.

---

- **Labor allocation UI** (`Hud.gd`, Early-Game Labor slice 3b — `docs/plan_early_game_labor.md`):
  the band is a **labor pool** whose working-age workers are assigned source-centrically to
  in-range sources/roles. **The player has as many bands as it has founded** — an arrived expedition
  can start a life where it stands (issue #510) — so every player-faction cohort is collected each
  snapshot into `_player_bands`, which backs the **band-picker dropdown** on the herd/tile assign
  controls (see `%HerdAssignControls` / `%ForageAssignControls` below) and the Band/City panel's
  cycler; an assignment explicitly names WHICH band supplies the workers.
  `player_band()` — the FIRST of that list — is the last-resort fallback and nothing else; which band
  a sheet actually composes for is `Hud._resolve_assign_band`'s answer, below. Three runtime-built
  control sets replace the retired single-task Scout/Cancel, Hunt/policy, and Forage buttons:
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
    **THE MUTED `· N wasted` NOTE IS THE ANIMAL WEB'S ALONE**, gated on `kind` inside
    `source_yield_readout` so every surface that reads it follows at once. One wire field carries two
    opposite facts: on a herd `wasted_yield` is `killed_biomass − carried`, meat that really rotted and
    a genuine call for more hands; on a patch it is `escapement_room − take`, stock the crew did not
    reach, which `systems/labor.rs` says outright is *"not lost, it simply stays in the stock and
    regrows"*. It also fired on the wrong side of the condition there — `room > take` is positive
    whenever a crew does not clear the whole room in ONE turn, which is the state the compose sheet
    **recommends** (its `hold it after` target sits far below its `clear it now` one), so a player who
    staffed the sustainable number was told they wasted food every turn forever, with no action that
    would clear it. On a herd `killed > carried` is genuinely exceptional, which is why the note never
    read wrong there. Understaffing a BUILD *is* a real loss (a Cultivate or Tame accrues at
    `min(workers / crew_needed, 1)` and decays when neglected) — but this note never carried that
    signal, so silencing it on flora costs nothing. **Assert the PAIR**: no forage fixture carries a
    non-zero `wasted_yield`, so a frame assertion passes with the bug fully present, and a lone
    negative is satisfied by silencing the note on both webs.
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
  - **Band-picker dropdown** (`_build_band_picker`, on BOTH assign controls, at the TOP of the sheet —
    above the stance picker and the worker stepper alike, so it reads "which band → which stance → how
    many workers"): a `Band:` `OptionButton` listing every
    `_player_bands` cohort by positional name ("Band N", via `HudFormat.band_display_name`; the cohort has
    no label field), item metadata = the band `entity`. The selection is the **actor band**:
    `_hunt_assign_band` / `_forage_assign_band` hold the picked entity (defaulting to
    `_resolve_assign_band()` when the selected source changes, else persisted across re-renders);
    the worker stepper's cap is that band's `source_crew_pool_hunt` / `source_crew_pool_forage`
    (its `idle_workers` + any it already staffs on that source, so re-editing isn't capped below
    current staffing), and the Assign emit + optimistic pending key off the picked band. Switching
    the dropdown re-caps the stepper and re-renders. Always shown (single-item with one band, so the
    actor is explicit). Lists **all** player bands — in-range filtering (Forage `work_range` / Hunt
    `work_range` + leash) is deferred to the multi-band slice (needs hunt-leash reach in the snapshot).
  - **`%HerdAssignControls`** (herd drawer, huntable herds, `_build_herd_assign_controls`): the
    band-picker, then a **distance-aware** "Assign hunters" **compose** control — a **policy picker**
    (`HudWidgets.build_policy_picker`, `_hunt_assign_policy`, default `sustain`) and, BENEATH it, the
    `−/+` worker/party count (`_hunt_assign_count`); stance first, then the crew that staffs it, the same
    order the forage sheet reads in. **The two policy axes are separated BY BRANCH, and the sim enforces it:** a
    **local** hunt offers `HUNT_POLICY_OPTIONS` (the four extractive rungs **+ the `Corral` investment
    rung**, gated by `_hunt_policy_gates`), while a hunting **EXPEDITION** offers only the extractive
    `LABOR_HUNT_POLICIES` — a detached party follows the herd and builds no pen, `send_hunt_expedition`
    REJECTS Corral server-side, and the sim exports no `hunt_trip_estimates` row for it, so a Corral
    ETA could only ever be a lie. The
    **local** branch renders `LOCAL_HUNT_POLICY_HINTS` under the picker (each rung's consequence FOR THE
    HERD: Sustain → take only the renewable yield, it stays healthy; Surplus → more food now, pushes
    settling; Deplete → draws the herd down hard, much more food now and a fast decline it will not
    recover from while it lasts — deliberately not oversold; Eradicate → **the last hunt**: the whole
    standing stock in one haul, the biggest payoff of any rung, in whatever the species pays, and the
    herd gone for good — denial is the END STATE, not a promise that the carcasses were thrown away
    (#337). **The `⇄ trade goods` half of that sentence went with the account (arc #527)**: the rung
    names the meat and the end state, and what an inedible species pays is quoted by the rung's own
    METRIC line rather than by the hint — the floor presets state its materials off
    `material_per_biomass`, which is a number and not a noun the prose has to carry). **They no longer
    teach the LADDER** — see
    "The two compose sheets read in ONE grammar" above. **These are
    NOT the expedition hints** (`SEND_HUNT_POLICY_HINTS`): an expedition's Hunting arm banks the kill's
    provisions into the party's larder and into the HOME BAND's `stores` at the drop-off/fold-back, but
    accrues **no husbandry** (a known v1 gap, tracked server-side) — so the expedition set may state a
    payoff, never a craft, and the two sets must stay separate. `LOCAL_HUNT_POLICY_HINTS`
    also owns the **`corral`** hint (Corral is a local-hunt-only rung) — which must carry all three
    halves of that bargain: the ~25-turn half-yield build, the ladder's best payoff, and the fact that
    **penned animals can't graze, so you feed them from your larder every turn and an underfed herd
    shrinks**, and it is the set `_policy_hint`
    spells out on a worked Hunt row's tooltip. **The hint is rendered per BRANCH, never once above
    both** — one shared line under the picker would promise an expedition player the band's payoffs. The
    button + command switch on the **wrap-aware hex distance** from the **SELECTED band's** own tile
    to the herd vs that band's **`hunt_reach`** (= `work_range` + hunt leash, decoded as `hunt_reach`
    and flowed onto the marker): **within reach** → a `Hunters` stepper + **"Hunt Here"** →
    `assign_labor hunt <herd_id> <policy> <workers>`; **beyond reach** → a `Party` stepper (cap
    `min(idle_workers, max_expedition_party_size)`) + a distance hint + **"Send Expedition"** →
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
    `Send Anyway (long raid)` and stays **enabled**. A **denial** mission — a raid that brings NOTHING
    home (`delivers_food == false` **and** an empty `delivered_material`; never the Eradicate rung,
    which delivers, and never an inedible quarry whose hides land) — likewise stays enabled (`SEND_HUNT_DENIAL_BUTTON`, "Send (brings nothing home)"). The ONE blocked case is **no surplus**
    (`SourceForecast.hunt_trip_no_surplus`: **`deliveredFood == 0`**) — the herd is at/below the policy's floor, so the raid
    would return empty at every party size: a mistake with no upside, so the button is **DISABLED**
    (`Herd too lean to raid`). This is `deliveredFood == 0`, **NOT `animalsTaken == 0`** — a small party on
    big game now delivers a partial (`animalsTaken 1`, high waste), which is NOT too lean; only a genuinely
    at-floor herd blocks. Party size cannot fix it — **surplus is a property of the HERD, not the party** —
    so the reason (`SourceForecast.hunt_no_surplus_reason` → `SEND_HUNT_NO_SURPLUS_REASON`) names **no alternative size**
    (the old row-scan / `_recommended_party` / step-up-impossible machinery is retired). `SourceForecast.hunt_estimate_key`
    is the one definition of the `"<policy>:<workers>"` estimate key, shared by the single-cell lookup and
    the max-useful scan.
    **The party stepper caps at MAX-USEFUL on both branches** (`SourceForecast.expedition_useful_cap`): the
    delivered payload PLATEAUS with party size once the herd's surplus (not the pack) binds, so extra hunters
    past the plateau raid no more food — the cap is the LAST party at which the payload was still RISING
    (`HuntTripForecastReply.useful_cap`, so `useful_cap + 1` is the first party that adds nothing, and a
    stepper seeds and clamps ON it rather than one above),
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
    long-run average of lumpy whole-animal delivery, EVERY extractive rung carries a **STABLE
    averaging-WINDOW disclaimer IN ITS BUTTON TOOLTIP** — `HUNT_AVG_WINDOW_FORMAT`: `This estimate is a
    long-run average over ~<X> turns — you take whole animals, so per-turn delivery varies.` (It stood as
    its own line under the picker until the compose-sheet consistency pass moved it onto the rung whose
    metric it qualifies — see "The two compose sheets read in ONE grammar" above.) X =
    `_hunt_avg_window_turns(herd, policy)`, derived from THAT rung's raw flow ceiling (NOT the
    crew's current delivered rate), so it is **worker-independent and never blinks out** as the Hunters
    count steps up: `g = ceiling ÷ food_per_animal`; slow/big game (`g < 1`) → `ceil(1/g)` (deer Sustain →
    ~2, mammoth Sustain → ~7), fast game → `ceil(1/frac)`, clamped to `HUNT_WINDOW_MAX_TURNS` (12). Keyed per
    rung (a faster policy averages over a different span), extractive rungs only (an
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
    a real payload over a REAL total, ordinary Send — a floor-`0` raid completes by emptying the range;
    `herd_hunt_forecast_horizon` / `herd_hunt_horizon_travel` are the raids that do not, and they quote
    `Send Anyway (more than N turns)`), the RAID + max-useful set `herd_hunt_boar_raid` (the server's measured Wild Boar,
    1 hunter → "delivers ≈5 Wild Boar over ≈7 turns · ~20 food", ascending per-policy compact `≈N` picker
    buttons — glyph + metric, name-in-tooltip) / `herd_hunt_max_useful` (2 hunters → "delivers ≈8 … over ≈8 turns"; a 3rd raids no more, so
    the stepper caps at 2 with "max 2 workers useful here — more would be idle") /
    `herd_hunt_raid_travel` (the SAME boar 8 tiles from a band carrying a move rate → the client adds the
    round trip: "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food", cap still 2) /
    `herd_hunt_no_surplus` (a herd stripped to its floor → 0 animals at every size → disabled "Herd too
    lean to raid") / `herd_hunt_eradicate` (the boar on Eradicate → the whole-stock windfall, ordinary
    Send), and
    `herd_hunt_local_sustain` /
    `herd_hunt_local_overdraw` (local branch, animals-first: green `≈0.14 Red Deer/turn · renewable` vs
    amber `⚠ ≈0.27 Red Deer/turn — overdraws the herd`) /
    **`herd_hunt_local_eradicate`** (the frame the LOCAL Eradicate hint is judged on: the rung's picker face
    reads the ladder's top take `💀 Eradicate / 2.40 food`, and the hint below describes the one-haul
    windfall + the permanent end state — never "no food"), and the carry-aware set
    `herd_hunt_delivered_clean` / `herd_hunt_delivered_waste` / `herd_hunt_automax` /
    `herd_hunt_big_game_window` (see the animals-first preview + "up to X/turn" cap notes above).
  - **`%ForageAssignControls`** (Tile card, food-module tiles, `_build_forage_assign_controls`): the
    band-picker, then a sustain/surplus/deplete/eradicate **policy picker** (`HudWidgets.build_policy_picker`,
    `_forage_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`) — carrying the SAME ascending
    per-policy **COMPACT** button metric the local-hunt picker does, over **every account the patch
    pays** since #426 (`[♻ Sustain / 0.60 food · 0.20 fodder]` on a hay meadow).
    **Each button is TWO LINES, ONE PER
    AXIS — the rung's glyph + NAME over its product line** (`[♻ Sustain / 0.96 food]
    [⬆ Surplus / 1.92 food] [⇊ Deplete / 2.88 food] [💀 Eradicate / 4.80 food] [🌱 Cultivate / → 1.20 food]
    [▦ Sow]`), a hay-meadow rung carrying both accounts (`[⇊ Deplete / 2.70 food · 0.90 fodder]`).
    **THE ONE-LINE
    `<glyph> <metric>` FACE IT REPLACED WAS AN AXIS COLLISION** (playtest, issue #337 follow-up): the rung
    glyph (`♻ ⬆ ⇊ 💀`) and the then-live trade-goods glyph `⇄` sat adjacent in one line at one weight
    saying different things — *which rung* vs *which product* — and dropping the rung NAME left `⬆`
    beside `⇊` reading as good-vs-bad rather than as neighbouring rungs of one ladder. Naming the rung
    in text is what defuses that, so `POLICY_ICONS` is UNCHANGED; the products stay in words
    (`SourceForecast.picker_products`) because an account other than food has no tintable pictogram —
    fodder never had one, and arc #527 retired the `⇄` that was the argument's original subject (see
    `sprites-widgets.md`). **Line 2 renders one step SMALLER and one step quieter than line 1**
    (`POLICY_PICKER_METRIC_FONT_SIZE` 13 against the default control size line 1 keeps by carrying NO
    override, and `POLICY_PICKER_METRIC_ALPHA` 0.72): the name leads the glance and the numbers answer
    it — at one size `0.32 food · 0.08 fodder` competed with `♻ Sustain` instead of supporting it. **No `+`
    sign on these numbers**: every rung
    is a gain, so a sign carries no information here (it stays on the work rows and map labels, where it
    contrasts against consumption), and the render-only-when-non-zero rule still governs — a sown hay
    Field's rung reads `0.90 fodder` alone, never `0.00 food · 0.90 fodder`.
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
    themed one read one table. **`ui_preview._assert_two_line_face_states` (issue #383) is what fails when
    they drift**: it renders this cell enabled and DISABLED into an offscreen SubViewport and reads back
    each line's peak luminance, so line 2 left bright beside a faded box is caught in pixels — the state no
    live caller reaches today, and the reason nothing else could see it. **The `modulate` refusal is a
    SEPARATE claim in that block** (`_face_modulate_is_identity`), because a luminance reading cannot tell
    the double-dim shape from a properly tinted one — it dims the pixels just as convincingly.
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
    **EVERY picker follows it now**, including the Band panel's two: the zone's own 2-column clamp is
    retired (see `band-city-panel.md`). It existed because the long faces overran the L/R dock's ~354px
    zone at 3 abreast; the faces are one word each now and the grid measures 234px of a 356px column. Each `*_policy_takes` helper emits a **`{compact, full}` pair** per policy: the
    compact string is the face's SECOND LINE, the verbose full string moves to the tooltip. Extractive rungs →
    compact `0.96 food` (`SourceForecast.picker_products(ceiling, fodder)`, fed by `_forage_policy_takes` off `SourceForecast.forecast_inputs`),
    full `up to +0.96/turn` (`POLICY_CAP_FORMAT` — the tooltip keeps the sign and the unit, being the one
    place that says "up to"). INVESTMENT rungs on BOTH pickers → compact `→ 1.48 food`
    (`POLICY_PAYOFF_COMPACT` over the SAME `SourceForecast.picker_products` the extractive rungs use, so
    the payoff obeys the render-only-when-non-zero rule too: a plant rung may name food and fodder, a
    herd rung names food alone; the arrow is what keeps it
    from reading as a take today), full `builds toward +1.48/turn`
    (`POLICY_PAYOFF_FULL_FORMAT`, the same shape `extractive_take_pair` builds) — the
    `tended_yield`/`field_yield` (forage) or `pastoral_yield` / `corral_yield` (hunt) they build toward,
    NOT
    the prep dip, which reads below Sustain and was identical for both hunt rungs (quoting it made
    taming/penning look worse than hunting); a locked rung may still show its payoff, the gate-reason line
    (under the picker) explains the lock. **The tooltip carries the VERBOSE metric the face compacts** —
    every button's `tooltip_text` leads with `<Name> — <full metric>` (`POLICY_TOOLTIP_NAME_FORMAT`, e.g.
    `Sustain — up to +0.96/turn`, `Tame — builds toward +1.20/turn`), and a gated button appends its gate
    reasons below that (so a hover tells you what the rung costs to unlock as well as what it pays). A rung
    with **no** metric (the work inspector's picker, which passes no `takes`; a metric-less gated rung) is
    **line 1 alone** — glyph + name — so a button is never a lone glyph and never a lone number. The three
    pickers — forage / local hunt / expedition — wear an **identical** face: `<glyph> <Name>` over
    `X food[ · Z fodder]` (extractive, `up to X/turn` in the tooltip via
    `POLICY_CAP_FORMAT` / `SourceForecast.extractive_take_pair`) or over
    `→ X food[ · Z fodder]` (investment, Cultivate/Sow AND Tame/Corral). **Fodder reaches the two PLANT
    rungs only**, and that absence is structural rather than unfinished — no animal is harvested for
    feed. (A third account, `trade`, reached all four until arc #527 retired it; the crop picker's own
    per-material clauses are a BASKET-ROW readout and never reach a rung face — see `land-readouts.md`
    → "WHAT A CASH CROP PAYS, PER MATERIAL".) **The expedition picker no longer shows raid animals** (`≈N` / `EXPEDITION_TAKE_COMPACT`
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
    sim's `Improvement::Cultivate` / `Sow` / `Tame` / `Corral` since #442 — they are their OWN
    control, not rungs of this picker; `SourceForecast.FORAGE_IMPROVEMENTS` / `HUNT_IMPROVEMENTS` name the
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
      * `Cultivate` ← `cultivation >= 1` **and the rung not already built —
        which is `is_cultivated` OR `is_field`**, since Sow can skip rung 2 (see "A FIELD IS NOT
        NECESSARILY `is_cultivated`") —
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
        (`GATE_REASON_ALREADY_FIELD_FORMAT`).
      * `Tame` ← `herding >= 1`. **Herding gates Tame ALONE now** — it no longer gates Corral.
      * `Corral` ← **`penning >= 1`** (the new rung-3 knowledge) **and** `domestication >= 1`.

      > **NO RUNG ON EITHER WEB CARRIES A HEALTH GATE, and `Sow`'s site refusal is the only SOURCE
      > gate the compose sheet can still render.** `docs/plan_harvest_floor.md` §3.2 replaced a CLIFF
      > with a RATE: a crew pulling hard on the ground it is clearing builds slowly, in proportion to
      > its escapement floor (`intensification::learn_multiplier`), rather than being stopped — so the
      > phase PACES a build and never forbids one, and with nothing left to lapse the gate had nothing
      > to guard. `validate_cultivate` says so in as many words, and `server::tests::
      > cultivate_is_accepted_on_a_stressed_patch` pins the absence positively. Sowable ground starts
      > at the reseed floor (i.e. Collapsing), which is why rung 3 never had one either. **A phase term
      > in `RungGates` would refuse a command the sim accepts** — the defect class issue #464 records
      > for the gathering-site rule, arriving from the other direction.
      >
      > What the player is told instead is the PACE, live and quantified: the compose sheet's aside
      > reads `Building at ×1.60 — a higher floor builds faster` (see "THE ASIDE'S TEACHING LINE").
      > **Nothing on the control mentions the phase at all** — the running control's pause note is
      > retired, and with it the compose sheet's last reader of
      > `HudFloraVocab.ECOLOGY_PHASE_THRIVING` (see "THE RUNNING CONTROL STATES NO PAUSE").

      The gates are re-validated every render.
      **Known gap (pre-existing):** `_hunt_policy_gates` does NOT check herd
      **ownership** — the tracks are per-faction, so a herd tamed by ANOTHER faction reads as
      available client-side while the sim rejects the assign.
    - **ISSUE #420's WHOLE MECHANISM IS RETIRED** (issue #442 — see "An assignment has TWO axes"
      above). It existed because a build verb was a value of `policy`, which produced a state a radio
      cannot express: a band STANDING on a rung the picker had to grey (a Cultivate patch that slipped
      out of Thriving, a Tame herd that finished taming). The answer was to render that rung SELECTED
      AND GATED — a reset that fired only when the composed rung was gated *and* differed from the
      standing one, a husbandry-ceiling re-admission pass so the hidden standing rung came back, three
      `GATE_REASON_ALREADY_*` reasons, and a bespoke `selected_when_disabled` button state. All of it
      is gone: a stance is never gated and never retires, so the compose sheet's reset can now only
      fire on a malformed composition, and a build that finishes becomes a static DONE LABEL — which
      cannot be selected-and-gated. **The behaviour it protected is preserved and improved**: a patch
      that drops out of Thriving mid-build keeps its improvement and merely pauses, and the control
      now SAYS SO in a WARN line naming the cause and the remedy, on both webs, instead of leaving the
      player to read a greyed rung. `_forage_policy_gates` / `_hunt_policy_gates` keep every gate they
      had; they simply feed the improvement control rather than the picker.
      **Known gap (pre-existing):** `_hunt_policy_gates` does NOT check herd **ownership** — the
      tracks are per-faction, so a herd tamed by ANOTHER faction reads as available client-side while
      the sim rejects the assign.
      **THE SIM-SIDE GAP THAT RODE WITH IT IS CLOSED** (#442): `validate_labor_policy` rejected a
      *new* Cultivate assign on a non-Thriving patch and re-staffing a paused build re-issued exactly
      that command, so the crew of a paused build could not be changed at all. `assign_labor` no
      longer asserts an improvement, so it no longer re-runs the improvement's gates — crew size is a
      stance-side edit.
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
    - **A STALLED Tame is stated by the ABSENCE of its estimate, and the phase says nothing.** The
      silent rule this axis used to have was `_tame_stalled_hint`: taming accrued only while the herd
      was **Thriving**, that was deliberately not a gate (a herd's phase swings as it is hunted), and
      the drawer said so in a WARN line. Both the gate and the line are gone —
      `docs/plan_harvest_floor.md` §3.2 made the FLOOR pace the build instead — so what genuinely
      stalls a Tame is an empty escapement room, and the sheet states that by quoting no turns at all
      beside an aside that already reads *"Nothing is taken — the whole stock stays standing. A crew
      with nothing to work learns nothing and builds nothing."* ui_preview `herd_tame_stalled` is
      re-fixtured onto exactly that: a Stressed herd composed at `FLOOR_MAX`, which is both the
      sharpest case (×2.00 is the largest multiplier on the axis) and the retired line's own trigger.
    - **The Sow SITE gate — the refusal is an ANSWER, not a bool.** Only ~**46 of 4160** tiles (1.1%)
      will take seed, so "why can't I sow here?" is *the* question rung 3 provokes — and the client
      **cannot re-derive** it (no per-biome capacity table, no hydrology). The sim ships the verdict
      as a stable key on `ForagePatchState.sowSiteRefusal` (`""` / `not_gathering_site` / `too_poor` /
      `too_dry` / `too_poor_and_too_dry`), resolved through the same `RungSiteRequirement::refusal` seam
      the `sow` command gates on, and `_sow_site_refusal_reason` maps it to `SOW_REFUSAL_REASONS` — each
      naming the fault AND what to do about it, in the manual's voice: the two GROUND readings point at
      rung 4 (Worked Land — irrigation/the plough), so their promise is a "not yet".
      **`not_gathering_site` is neither of those things.** It SUPERSEDES the ground readings rather than
      joining them (the sim short-circuits on it, so a thin AND dry non-site still ships this verdict
      alone — a refusal naming three faults teaches two the player cannot act on), and no rung below
      Farm relaxes it, so its line points at other GROUND instead of at a later turn. It is **latent but
      not dead** on the client: `DrawerComposeController._forage_compose_available` gates on the same
      `DetailFormat.tile_is_gathering_site` test, so the sheet never opens on ground that would earn it
      — while it is the verdict the sim ships for the large majority of patch tiles, and it becomes
      reachable the moment rung 4 drops `requires_gathering_site`.
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
      (no feed term — Tame has no running cost). Since #442 the deal is `improvement_forecast`'s three-term
      line on the improvement control; the payoff keys still name the set (an investment
      rung must never fall through to the extractive `renewable / ⚠ overdraws` preview), and both hunt
      investment rungs' picker buttons wear the `→ +Y/turn` PAYOFF (Tame `→ pastoralYield`, Corral
      `→ corralYield`) via `_hunt_policy_takes` — NOT the during-building dip, which reads below Sustain
      and was identical for both, making taming/penning look worse than hunting. The payoff shows even on
      a gated/greyed rung (the gate-reason line explains the lock). ui_preview `herd_tame`. **The
      gated case moved off the picker with the axis split**: a gated improvement's control text IS its
      reason and quotes no payoff at all (`herd_corral_gated`), the number being noise at the moment
      you are told you cannot act on it.
    - **Progress meters — one row per rung, never merged.** Tile card: `Cultivation N%` → `🌾 Tended
      Patch`, joined by its own **`Field`** row — `Sowing N%` → the SIGNAL-tinted **`▦ Field`**
      (`patch_field_progress` / `patch_is_field`, `_field_label` / `_field_value_hex`). Herd drawer:
      `Husbandry: Domesticating N%` → `🐄 Domesticated`, joined by `Corral: Building N%` → `🐄
      Corralled`. **A patch carries BOTH plant meters at once** (a Field may stand on ground that was
      never tended — seed travels, so `Sow` needs no prior patch), so they are two independent rows.
      A completed **Field** deliberately reads as a *different thing* from a Tended Patch — different
      word, different glyph — not as a bigger percentage; that IS rung 3's readout test.
      `Sowing`/`Building`/`Fencing` share one build-verb convention.
    - **Knowledge-unlock nudge.** `_ingest_intensification` keeps the per-faction tracks (all FIVE
      since #485 added Foddering — the four rung gates plus the pen rung's own capability — driven off
      `KNOWLEDGE_TRACK_LABELS`, so adding a knowledge is a label entry + a decoder
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
      **`pastoralYield` → `pastoral_yield`** (Tame's payoff, the pastoral twin of `corralYield`).
      **The trade halves of both payoffs are RETIRED with the account (arc #527)** — `pastoralTrade` /
      `corralTrade` are gone from the wire and `pastoral_trade` / `corral_trade` from the decoder, so a
      prepared herd's rung face names food alone → bare keys
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
      button face's second row (`2.70 food`; full `up to X/turn` — `POLICY_CAP_FORMAT` — in the tooltip; the shared const also
      used by the forage picker — the source's worker-independent ceiling, FOOD units, distinct from the
      crew's carry-aware per-turn preview line below the picker) so Sustain < Surplus < Deplete < Eradicate
      reads as ASCENDING. **THE ASCENT IS PER ACCOUNT**, and both surviving columns hold it.
      The one column that did NOT was the retired trade account: `Deplete` alone carried
      `market.trade_goods_multiplier` (×4) — a POLICY markup on stripping a source for sale — so its
      trade cell could sit ABOVE Eradicate's while the food cells still ascended (measured on a live
      patch: Deplete 3.24 against Eradicate 1.21), and `forage_three_accounts` shipped that
      non-monotone column deliberately. The harvest-floor arc retired the markup (a deeper floor earns
      more only by taking more BIOMASS) and arc #527 retired the account itself, so the frame carries
      food and fodder alone. **The rule survives the exception**: a future account with a policy markup
      on it would break the ladder again, and a fixture that quietly sorted it would misrepresent what
      the player sees. `wasted_yield > 0` renders a muted "· N.N wasted" understaffing note (the low-key
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
      20 carriers, `≈0.15 Woolly Mammoth/turn`; its Sustain rung's TOOLTIP carries the averaging-window
      disclaimer, `This estimate is a long-run average over ~7 turns — you take whole animals, so
      per-turn delivery varies.`, which a rendered frame cannot show — the deer states' rungs carry the
      same note reading ~2 turns, at every worker count) /
      `herd_hunt_local_sustain` +
      `herd_hunt_local_overdraw` (green vs amber `⚠ … — overdraws the herd`) / `hunt_crew_herders`
      (a corralled herd → "Herders" stepper + "Assign herders") / `knowledge_penning_climbing`
      (Penning 34% climbing in the top strip) / `food_tile` (the "Cultivation Preparing 60%" row).
    - ui_preview: `forage_cultivate` (enabled + the Preparing→then forecast + the feed nudge) /
      `forage_cultivate_locked` (1 reason — knowledge + its Sustain-forage remedy) /
      `forage_cultivate_stressed` (the SAME wild basket ⚠ Stressed with Cultivation KNOWN — an OFFERED
      live checkbox over its crop list, and no ecology refusal anywhere on the sheet; the A/B partner
      of the frame above, and the frame the no-health-gate rule is judged on) / `herd_corral`
      (enabled + `Corral: Building 40%`) / **`herd_corral_gated` + `herd_corral_ungated`** (the
      Penning gate as an A/B on ONE fully-tamed herd: the reason as the control's own text, then the
      live box once Penning is known — nothing about the animal changes between them, which is the
      claim). They replaced `herd_corral_locked` / `herd_corral_locked_both`, whose 40%-tamed herd
      documented a gated Corral that #442 no longer renders: only ONE rung is offered, so a part-tamed
      herd is offered **Tame** and Corral never appears. **The SOURCE half of the Corral gate is
      consequently unreachable in this control** — `This herd is N% tamed — ◎ Tame it to finish` can
      only apply while Tame is what the control offers — and the KNOWLEDGE half is the whole of what a
      gated Corral can say. (`RungGates.hunt_gates` still answers both; the work board and the map read
      it for other purposes.) Slice 6b adds: **`two_meter_split`** (THE headline frame — the
      top-bar knowledge strip + this herd's own DONE meter + the bridging gate reason, all at once, on
      the fully-tamed herd that gate needs) /
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
    **BOTH SOURCE KINDS NOW CARRY THEIR CEILINGS AS A PER-POLICY LIST, and the asymmetry that used to
    be load-bearing is gone** (#426). The HERD has `huntPolicyCeilings`; the PATCH has
    `foragePolicyCeilings`, one row per rung carrying BOTH halves of `min(w × per_worker, ceiling)` in
    all three accounts. The patch's six flat `ceiling*` scalars are retired `(deprecated)` wire slots
    with no reader — the same treatment the herd's identically-named scalars already had — so a new
    policy costs neither kind a schema change, and `SourceForecast.FORECAST_CEILING_KEYS` (the table
    that mapped a forage policy to one of those scalars) is DELETED. **A patch's per-worker term rides
    the row too**, deliberately, and only the FOOD one survives as a patch-level scalar
    (`perWorkerYield`). **The per-policy ceiling arrays this paragraph indexes are themselves retired**
    — a continuous floor cannot be answered by four rows — so the shape it describes is history: the
    client composes `ceiling(floor) = max(0, B − floor·K) × rate` from the per-biomass vector, and
    `expected(workers, floor) = min(workers × per_worker × dip, ceiling(floor))`, with
    `max_useful_workers` dividing by the DIPPED carry. The old market markup that made a trade rate
    policy-dependent is deleted with the stance axis. Decoded in `native/src/lib.rs`
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
      the IMPROVEMENT axis (`improvement != ""` since #442), so the two agree by construction while `SourceForecast` stays free of the
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
    free up idle workers to send more"` (M = the usefulness ceiling, so it tracks the selected policy).
    The cap value is unchanged (still `min(labor, usefulness)`); only the note now names *which*
    ceiling bound and the M you're working toward, so a disabled `+` is never mute.
    **THERE IS ONLY ONE SUPPLY CONSTRAINT, SO THERE IS ONLY ONE NOTE.** `PARTY_SIZE_BOUND_NOTE_FORMAT`
    ("N of M useful — at the max party size") and the branch that chose it are DELETED with the cap
    they described: `SourceForecast.expedition_party_cap` answers the band's idle workforce alone, so
    freeing idle workers is ALWAYS the remedy and the party-size branch could never be taken. A branch
    that cannot be reached is worse than no branch — it reads as a case someone must have measured. The
    one `ui_preview` state that staged it (`herd_hunt_party_size_bound`, `idle 6 >= max party 2`) went
    with it rather than being re-described: at that fixture the sheet now renders the max-useful note,
    so the frame could only have shown one note under another one's name. (`SourceForecast.expedition_useful_cap` scans the full estimate table for M even past
    the fieldable party, so "of M" can exceed the party you can currently staff.)
    **ONE forecast row per rung, and forage now mirrors the local hunt exactly** (`Hud.gd`): an
    **INVESTMENT** rung (Cultivate/Sow — a composed IMPROVEMENT since #442, not a policy branch)
    keeps `_forecast_yield_row`'s dip→payoff deal (`Preparing: +X /turn → then +Y /turn`); an
    **EXTRACTIVE** rung renders `_local_forage_preview_bbcode` — the plant twin of
    `_local_hunt_preview_bbcode` — the take + a verdict (`+2.74 /turn · renewable`, or WARN-amber
    `⚠ … — overdraws the patch`), through the SAME `HudWidgets.forecast_label` RichTextLabel at
    `ALLOC_SECTION_FONT_SIZE` the hunt line uses.
    **BOTH HALVES OF IT ARE PER ACCOUNT SINCE #426.** The take is `SourceForecast.yield_components`
    over all three (`SourceForecast.expected_yield_account` applies `min(w × per_worker, ceiling)` to
    each, since the sim caps each account against its own ceiling and a patch whose LABOR binds on
    food can be CEILING-bound on fodder the same turn) — it read the food account alone, so a flax
    patch previewed `+0.00 /turn · renewable`, "staff this and get nothing, sustainably". And the
    overdraw test runs per account with **ANY** of them carrying the line: it compared food against
    food, so a crew stripping a meadow's hay while sitting inside its food regrowth read green with
    both sides of the test at 0. ANY rather than ALL, because the warning is about the PATCH, and one
    account drawn past its regrowth draws down the same patch. This retires the old `"Expected yield:"` prefix for extractive forage (`FORECAST_LABEL_FORMAT`
    is gone and `_forecast_yield_row`'s non-investment `else` branch was unreachable and removed — its
    only two callers, both reached through `_build_improvement_control` since #442, both
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

## A source pays a VECTOR OF ACCOUNTS — the render-only-when-non-zero rule (issues #337 / #426 / #527)

A yield routes by ACCOUNT: **provisions and fodder**. A hunt's is the species' own `HuntYield` times
the crew's take; a patch's is its per-biomass vector. The client read only the food account, so a hay
meadow rendered `0.00 food` on every rung and looked like a source worth nothing.

**THE ONE RULE, and it is applied at every surface: render a component only when it is non-zero.**

| source | reads |
|---|---|
| a deer | food only |
| a staple patch | food only |
| a hay meadow | food **and** fodder, **food leading** |
| a sown hay Field | fodder only — **never** a "0 food" line |
| an inedible quarry | its **MATERIALS**, each naming itself — and never a "0 food" line |

A `0` printed as a number for a component the source does not produce is the false precision this
whole arc exists to remove; it is not "more complete", it is wrong. The one place a zero survives is a
component the source genuinely HAS and did not pay this turn (a worked row's `+0.00 /turn`, a rung
whose ceiling exists and is empty).

**THE ACCOUNT ORDER IS THE WIRE'S, NOT A RANKING** — provisions, then fodder — so a source reads the
same left-to-right whichever accounts it pays, and the eye finds an account by position rather than by
re-reading the words.

**FODDER IS PLANT-ONLY, and that asymmetry is structural rather than unfinished work.** Fodder is feed
grown for penned animals; `fauna_config::YieldAccounts` fills a structural zero there for every
species, so no herd rung has a fodder term and none ever will. Do not "complete" the herd side by
appending one — there is nothing on the sim side to put in it.

### THE THIRD ACCOUNT WAS TRADE GOODS, AND IT IS RETIRED (arc #527)

`trade_goods` was written on every harvest and read by nothing: every credit site already had a
`credit_material_yield` beside it accounting the SAME take's concrete materials, so the scalar was a
flattened duplicate of the model the crafting arc exists to protect — a mammoth hide and a hare pelt
are both `hide` and are not the same thing. The sim retired the axis; **every client surface that
spelled it is gone with it**, and this is the list, because each one is a place where "put the number
back" looks like the obvious repair:

| retired | what it was |
|---|---|
| `FoodIcons.TRADE_GOODS_GLYPH` (`⇄`) | the mark every non-food component of a yield wore |
| `SourceForecast.YIELD_ACCOUNT_TRADE`, `format_trade`, `trade_rate_of`, `PICKER_TRADE_PRODUCT_FORMAT`, `POLICY_CAP_TRADE_FORMAT`, `TRADE_TOOLTIP_FORMAT`, `TRADE_RANGE_*_KEY`, `YIELD_RANGE_TRADE_CLAUSE_FORMAT`, `HUNT_FORECAST_TRADE_FORMAT`, `DENIAL_TAKE_TRADE_FORMAT`, `DENIAL_TAKE_LEFT_TRADE_FORMAT`, `DENIAL_TAKE_LEFT_JOIN` | the account's whole vocabulary |
| `FORECAST_PAYOFF_TRADE_KEYS`, `FORECAST_TRADE_PER_BIOMASS_KEY`, `FORECAST_PER_WORKER_TRADE_KEY`, `FORECAST_TRADE_PER_ANIMAL_KEY` | its wire keys |
| `YIELD_AXIS_PROVISIONS` / `YIELD_AXIS_TRADE`, `herd_yield_axis` | the axis CHOICE (below) |
| `HudConst.STORE_ITEM_TRADE_GOODS`; `DetailFormat.band_trade_stock` / `sum_realized_trade` / `band_trade_income` / `band_has_trade_flow`; `DisclosureController.trade_breakdown_lines`; `HudDisclosureVocab.BREAKDOWN_KIND_TRADE` / `DETAIL_ROW_TRADE`; `HudWorkVocab.WORK_TRADE_TOTAL_TOOLTIP` / `FACTION_TRADE_STOCK_FORMAT` | the band's Trade row, its breakdown, the faction rollup's row and the work head's total |
| `FaunaPanel.HERD_TRADE_GOODS_YIELD_PER_BIOMASS` | a hardcoded client-side copy of a sim rate |

**WHAT REPLACES IT IS A VECTOR, AND ONLY ON THE CROP PICKER.** A harvest's non-food, non-feed product
is MATERIALS — `fibre`, `hide`, `tobacco` — and the wire states them **per material**
(`FloraShareInfo.sowMaterialPayoff` / `cultivateMaterialPayoff`, decoded as `sow_material_payoff` /
`cultivate_material_payoff` on each `composition` entry). **Never sum them into one materials/turn
figure, anywhere**: that is the retired axis under a new name, and it re-collapses the distinction the
materials model exists to keep. The one surface that quotes them is the crop picker, one clause per
material — `land-readouts.md` → "WHAT A CASH CROP PAYS, PER MATERIAL".

**A MATERIAL NEEDS NO GLYPH, AND MUST NOT BE GIVEN ONE.** `⇄` earned its job by being ONE mark for a
whole product, because the sim modelled a scalar; a material has a NAME, and a name is a better mark
than an abstract arrow saying only "not food". So a material row reads `0.29 fibre`, exactly as its
neighbour reads `1.80 hay`, and there is no generic account left for a generic mark to stand for.

### AN INEDIBLE QUARRY QUOTES WHAT IT PAYS, IN THREE FIELDS

A wolf's `provisionsPerBiomass` is `0` and it used to pay `trade_goods_per_biomass: 0.02`, so it had
a rate. Retiring the axis took that away and nothing replaced it for one release: the compose sheet
quoted no rate, the board row and the map label read `+0.00`, and the pelts still landed in the
band's `MaterialStore` every turn. **Three wire fields close it**, and which one a surface reads is
the whole of the rule:

| field | on | what it is | who reads it |
|---|---|---|---|
| `material_per_biomass` | a herd **and a patch** | what ONE UNIT of its biomass is MADE OF, per material | the ceiling, composed at the dragged floor |
| `per_worker_material` | a herd **and a patch** | what ONE WORKER brings home per turn, per material | the crew term of the preview's `min` |
| `corral_material` / `pastoral_material` | a herd | what the built RUNG pays once it stands | the two investment rungs' payoff faces |
| `material_yield` | a labor assignment | what the source actually CREDITED this turn | the board row, the map label, the inspector strip |
| `delivered_material` | a `HuntTripForecast` row | what a whole TRIP lands, per material | the launch sheet's raid line, its readout box, its preset faces |
| `material_batches` | a cohort (`PopulationCohortState`) | what is IN THE PACK right now, per pile | the in-flight party's `Carried:` row |
| `delivered_material` | a `DenialRow` too | what a DENIAL raid salvages, per material | the denial sheet's take line |

- **THE TWO RATES ARE THE FOOD SIDE'S OWN TERMS, ONE ACCOUNT OUT.** `material_per_biomass` is the
  twin of `provisionsPerBiomass`, so `forecast_inputs` composes `ceiling(floor) = max(0, B − floor·K)
  × rate` per material through the identical `escapement_room` the two scalars go through — which is
  why it is a per-biomass RATE and not a pre-composed ceiling: it has to answer at whatever floor the
  player has dragged to. `per_worker_material` is the twin of `perWorkerYield` and carries the build
  DIP for the same reason `per_worker` does. `SourceForecast.expected_materials` is the `min` over
  the pair, unioned BY ID rather than zipped by position: a rate with no ceiling is a herd standing at
  its floor, which takes nothing and correctly renders no row.
- **THERE IS ONE MATERIAL CEILING, NOT THE SCALARS' ROOM/HOLD PAIR.** `forecast_inputs` publishes
  `material_ceiling` and no `hold_material_ceiling`, and `expected_materials` takes no ceiling
  selector. It had both for a release and the hold arm was **unreachable** — its one caller passed the
  room at every call site — so a ceiling was computed on every forecast for a reader that did not
  exist. **A per-material `after` reading is not something any surface states**: the sheet's
  `now → after` is food and fodder alone. The selector was also a way to be wrong quietly, an unknown
  key reading as *every ceiling is zero*, i.e. a silent empty take. The pair comes back the day a
  surface genuinely wants the material `after`, with that surface as its caller.
- **`material_yield` IS NOT A FORECAST, AND READING IT AS ONE IS THE TRAP.** The sim seeds it EMPTY
  on a pre-commit row **by design** — projecting materials needs the take in BIOMASS while the
  forecast resolves in currency space, where an inedible species has no positive axis. So an empty
  answer means either "pays no material" or "no take has resolved yet", and both render as no row.
  **A compose sheet must therefore read the two RATES**, and `_hunt_material_rows` is the one place
  that composition lives (`DrawerComposeController`). `SourceForecast.material_rows_of` is the
  resolved reader, the material twin of `fodder_rate_of`, and it likewise has no
  realized/projected sibling to fall back from.
- **THE MATERIAL ARM IS INDEPENDENT OF THE FOOD AXIS, and that independence is the whole point.**
  Both food paths in `_hunt_yield_model` bail on an inedible quarry — its per-animal quantum is
  honestly `0`, so there is nothing to quantise and nothing to smooth — and the model returned `{}`
  there, which is what left the sheet quoting nothing. The material branch answers on its own, with
  **no overdraw verdict**: the sustainability bar is the food peak's ceiling in the account the take
  is measured in, and this take is measured in none of it. The drawdown is real; the client has no
  material sustainable-yield to judge it against and must not invent one.
- **A MATERIAL VECTOR IS A `forecast_is_known` WITNESS.** `zero_account_of` cannot answer this — it
  names which SCALAR zero is worth printing, and a material's empty answer renders as no row, so it
  has no zero to nominate. Without the extra arm a wolf reads `known == false` everywhere: no
  floor-preset caps, no compose readout, no worker cap. That is the client calling a fully-described
  herd undescribed because the one account it pays is not a scalar.
- **MATERIALS ARE ROWS OF `yield_rows`' OWN VECTOR**, appended after the two scalars under the
  identical non-zero gate, so `yield_components` / `picker_products` / `magnitude_components` /
  `extractive_take_pair` gained the account without a second code path to spell it. **A material
  row's ACCOUNT IS ITS OWN ID** — the material names itself, `YIELD_ACCOUNT_UNITS` has no entry for
  it, and the unit falls back to the id, which is the display word. (That fallback used to be `""`,
  which printed a bare `0.22` with nothing saying what it was.) **They also answer the ZERO
  question**: a source paying a material pays SOMETHING, so no zero survives beside it, and a wolf
  never reads `0.00 food · 0.22 hide`.
### THE PLANT WEB GOT THE SAME ARGUMENT, ONE RELEASE LATE (the cash-crop compose sheet)

A tile 32% cotton and 26% tobacco composed a forage sheet reading `0.24 → 0.18 FOOD · — FODDER` and
never mentioned the fibre and tobacco the gather actually banks. Reported from a screenshot. The
client half was **one argument**: `_forage_yield_model` ended with

```gdscript
var rows := SourceForecast.yield_rows(actual, actual_fodder, zero_account, after)
```

— four arguments, where its hunt twin already passed five. **The bug is worth naming precisely,
because "the plant web has no materials" was never true and nothing in the code said it was**: the
composition was shared, the keys were prefix-aware, and the only thing missing was the call.

- **ONE COMPOSITION SERVES BOTH WEBS, which is why this was a call and not a derivation.**
  `forecast_inputs` reads `prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY` and
  `prefix + FORECAST_PER_WORKER_MATERIAL_KEY`; a patch spells them `patch_`-prefixed on `tile_info`
  and bare in `forage_patch_lookup()`, a herd spells them bare. So the patch's ceiling composes at the
  dragged floor by the same `max(0, B − floor·K) × rate` rule, and `expected_materials` clamps
  `min(workers × rate, ceiling)` per material exactly as the hunt sheet does.
- **THE PATCH'S PER-WORKER TERM HAS THE SEASONAL WEIGHT ALREADY FOLDED IN**, as `per_worker_biomass`
  does — so it is honestly EMPTY in a dead season and **nothing may divide by it**. A client that
  re-applied the weight would double it; one that recovered a rate by dividing by it would divide by
  zero on the very tile the emptiness describes.
- **THE FLOOR PRESETS QUOTE IT TOO, and the wild-fodder LOCK does not reach it.** That gate is
  Foddering's — a claim about FEED and about whether the faction has learned to keep a pen — and a
  gatherer banks a cash crop's fibre whether or not it ever has.
- **THE CROP PICKER IS A DIFFERENT QUESTION AND STAYS ONE.** Its `sow_material_payoff` /
  `cultivate_material_payoff` are per PLANT and per RUNG — what one species would pay if you built on
  it. These two are the PATCH's rates for the wild rung being gathered right now. A tile-level rung
  figure would sum across the basket, and summing is the retired axis under a new name.

**THE TWO INVESTMENT RUNGS ON THE ANIMAL WEB WERE THE OTHER MISS.** `corral_yield` /
`pastoral_yield` are PROVISIONS, so an inedible quarry's are honestly `0` and both rungs advertised
`0.00 food` or nothing at all — the two rungs a player would actually take on such a species offering
no reason to take them. `FORECAST_PAYOFF_MATERIAL_KEYS` (`corral_material` / `pastoral_material`) is
the material half of that payoff, read by `improvement_forecast` into `payoff_material` and by
`forecast_inputs`' MANAGED branch into the built rung's ceiling. **The plant web is deliberately
absent from that table**, for the crop-picker reason above.

**AND THE DENIAL ROW.** A denial raid on an inedible quarry stated its kills and stopped — a mission
that destroys and salvages nothing, which is false: the party hauls every pelt (`carry_room_biomass`
answers `NO_CARRY_BOUND` for a species paying no provisions, so the pack never fills). `DenialRow`'s
own `delivered_material` is what it brings home, one ` · brings home 22.00 hide` clause per material.
**The verb is repeated rather than shared with the food clause**, because that clause is optional and
a shared verb would strand the materials on a quarry that pays no meat — precisely the quarry the
clause exists for. The sim states no per-material WASTE for a denial row, so the waste clause stays
food alone while the haul clause beside it is a vector.

**Frames.** **`forage_cash_crop_gather`** — the screenshot's tile, on the WILD rung with no
improvement composed, reading `0.32 → 0.15 FOOD · 0.09 FIBRE · 0.06 TOBACCO`. Five claims: the crew
composes at all, each material is quoted, **each has a ROW OF ITS OWN** (a structural claim, not a
needle for the sum's digits — that needle collided with the food row's `after` reading exactly once),
and the FOOD row still reads, or "quote the materials" would be satisfied by a sheet that stopped
quoting the food. `herd_hunt_pelts_only` gained the two rung payoffs, asserted PNG-less through
`improvement_forecast` → `_payoff_terms`: that wolf's `husbandry_ceiling` is `wild`, so the sheet
renders no Tame or Corral rung to read a face off, which is correct behaviour and exactly why the
claim cannot be made on the render. `band_panel_compose_deny`'s edible boar is the live control for
the denial take, whose inedible half is likewise asserted on the producer.

### THE EXPEDITION HALF: what a raid LANDS, and what is in the pack on the way home

A raid on an inedible quarry read as a DENIAL MISSION — "brings nothing home" — for the release in
which the trade account was gone and no per-trip material figure existed. It was false: the sim banks
the hides. Two fields close it, at two different registers.

**`delivered_material` — the LAUNCH sheet, and it is a PAYLOAD rather than a rate.** It rides every
row of the `HuntTripForecast` reply (`at_composed` and each `per_preset`), projected off the same
carried biomass `delivered_food` is, so the two readouts of one raid cannot disagree.

- **THE DENIAL AND EMPTY TESTS BOTH HAD TO LEARN ABOUT IT, and forgetting the second is the subtle
  bug.** `hunt_trip_forecast` resolves the payload FIRST, then: *denial* is `delivers_food == false`
  **and no material** — a raid that hauls something is a real delivery whatever account that
  something is in — and *empty* is `delivered_food <= 0` **and no material**. A food-only empty test
  sends every material-landing raid down the returns-empty branch, printing a refusal at a party that
  is walking home loaded. `herd_hunt_pelts_raid` asserts both negatives, because fixing only the
  denial branch moves the frame from one wrong sentence to another.
- **THE ONE-LINE FORM ROUNDS TO WHOLE UNITS, THE READOUT BOX DOES NOT, and that is the register
  difference rather than a drift.** `_raid_payload_suffix` appends `· ~3 hide` beside `· ~20 food`:
  this line quotes a TRIP, and a `0.22` beside a `~20 food` would read as a per-turn number smuggled
  onto a per-trip line. It is the ONE place a material is rounded — every other material readout in
  the client is a rate at `YIELD_DECIMALS`. The box (`HudWidgets._trip_yield_rows`) states the exact
  magnitudes under the animal count, through `yield_rows` like every other readout, so an inedible
  quarry's trip reads `≈5 GREY WOLF` over `2.75 HIDE` and nothing else.
- **THE PRESET FACES DIVIDE BY THE TRIP** (`expedition_policy_takes`), because that metric is a
  per-turn rate — the max obtainable over party sizes. Same payload, two spellings, one for each
  register. A rung reaches the picker when it pays SOMETHING: gating on food alone left an inedible
  quarry's rungs blank, which is the "worth nothing" reading this whole arc removes.

**`material_batches` — the IN-FLIGHT pack, and it is a different SHAPE from everything else here.**
`PopulationCohortState.materialBatches` is resolved from `cohort.stores` with **no resident-band
gate**, so a detached party's carried materials were on the wire the whole time and nothing rendered
them: a scout hauled a wolf home and the UI never mentioned the hide.

- **A BATCH IS NOT A PAYOFF ROW.** Everything else in this arc is `{material_id, amount}` — one figure
  per material. A batch is one pile of one material **AT ONE RATING**, carrying a characteristic
  vector, so two piles of `hide` at different readings are **two terms and must not be merged**. That
  distinction is the entire reason the trade scalar was retired; summing batches rebuilds it out of
  its own replacement.
- **IT IS A CLAUSE ON `Carried:`, NEVER AN EIGHTH LINE.** The parties inspector strip's budget is
  fully spent at SEVEN lines in a ~300px clipping zone, and that section's own rule for a new fact is
  the band zone's SHORT-tier idiom: two facts that read as one sentence cost one row. What the party
  is carrying home IS the `Carried:` sentence. `band-city-panel.md` → "The parties strip's SEVEN
  lines" owns the budget.
- **THE PER-AXIS READINGS ARE THE CRAFTING PANEL'S REGISTER, deliberately not restated.** That panel
  renders every batch's amount beside its `tough: excellent` chips for the band this party folds back
  into; the strip answers *what is coming home and how much*, in a box that cannot afford a
  characteristic vector per pile. **If piles of one material become common on a party, the fix is the
  batch's band NAME inside its term — never a sum.**
- **THE BATCH KEYS ARE `HudCraftingVocab`'s, read rather than re-declared**, and the amount wears its
  `BATCH_AMOUNT_FORMAT`, so a pile reads the same to one decimal wherever it is quoted. **A SCOUT
  CARRIES THE CLAUSE TOO** (on its `Provisions:` row): a scouting party that walks over a kill banks
  its materials exactly as a raid does, and the mission it was sent on is no reason to hide its pack.

**Frames.** `herd_hunt_pelts_raid` — the wolf as an expedition target, now a real delivery reading
`≈5 GREY WOLF` over `2.75 HIDE` with an ordinary Send, asserted on three claims (not denial, not
empty, names the hide). `band_panel_worst_case_party` — the strip's own worst case, whose
`Carried: 18 / 18 (5 turns) · 4.5 hide · 1.2 hide · FULL` carries TWO piles of one material at two
ratings; the third assertion there is that their SUM does not appear, which is the claim a fixture
with one pile per material could not make.

**Frames.** `herd_hunt_pelts_only` — the compose sheet reading `0.11 HIDE` at the crew the stepper
landed on, with the floor presets quoting hide in their tooltips and `strip` quoting strictly more
than `peak` (the claim that the ceiling composes AT a floor rather than being a constant repeated
four times). It is asserted as a PAIR with `herd_hunt_both_products` and carries BOTH halves: no
`0.00 FOOD`, **and** a live hide rate — the negative alone is satisfied by a readout that prints
nothing. `band_panel_work_trade_rows` — the resolved row, `+0.22 hide` beside the deer's `+0.20`,
with the deer as the control that must be UNCHANGED. `map_preview`'s `_assert_yield_label_component`
drives the full fall-through, including the two negatives (food still leads a source paying food AND
a material; fodder still beats a material).

### The shared layer (`SourceForecast`)

- `has_component(rate)` — the single "is this component present?" gate, so every account is judged
  identically everywhere. **Its floor is the DISPLAY's, not the model's** (`COMPONENT_RENDER_MIN`,
  half of the smallest quantity `YIELD_DECIMALS` can show). It read `>= FOOD_FLOW_MIN` (0.001) until
  #426 — the *food-flow* floor, which is a claim about the SIM — while every caller renders at two
  decimals, so a rate in between PASSED the gate and then printed as `0.00`: a single forager on a
  staple patch earned ~0.003 of the then-third account a turn, and the preview line read
  `+0.08 /turn · ⇄ +0.00 · 0.13 fodder`. **A gate finer than its formatter's resolution admits the very
  thing it exists to stop.** `FOOD_FLOW_MIN` keeps its own separate job — whether the BAND has a food
  flow at all is a question about the sim, not about how many decimals a label shows.
- **`yield_components(food, fodder = 0)`** → `+0.31 /turn`, `+0.08 /turn · 0.13 fodder` — the ONE
  joiner every per-turn readout goes through, so no two surfaces can word the vector differently. The
  fodder term wears the WORD (fodder has no glyph); every hunt-side caller leaves it defaulted.
- **`magnitude_components(food, fodder = 0)`** → `0.40 fodder` — its COMPACT twin for a surface that
  supplies its own framing and states levels rather than deltas (the work zone's filter chips). Same
  rule, same food-leads order, bare magnitudes joined by `COMPACT_COMPONENT_SEPARATOR` (a space, since
  those chips already spend their `·` separating a count from its total).
- **`extractive_take_pair(food, fodder = 0)`** — the rung metric `{compact, full}` for ALL THREE
  pickers. The food-only `extractive_take` the forage picker used is **deleted**, not kept as an
  alias: one joiner is what keeps the three pickers wearing one face.
- **The rule reaches the INVESTMENT payoffs too**, not just the extractive caps.
  `FORECAST_PAYOFF_FODDER_KEYS` covers the **plant pair only** (`tended_fodder` / `field_fodder`) and
  gives `forecast_inputs` a **`payoff_fodder`** beside `payoff`; `DrawerComposeController._payoff_take`
  builds the same shape `extractive_take_pair` does. **A resolved crop substitutes all of its own
  together** — `_flora_entry_payoff` / `_flora_entry_fodder_payoff` / `_flora_entry_material_payoff` in
  one branch — so a face can never mix one crop's food with another's fodder.
- **`picker_products(food, fodder = 0)`** → `0.60 food · 0.20 fodder` — the same rule and the same
  food-leads order **in WORDS and without the sign**, for the ONE surface with room to name its
  products: the policy picker's two-line rung face (`compact` above is written in terms of it). The
  picker names rather than marks because line 1 already carries a glyph naming the RUNG, and two glyph
  families in one line at one weight is the axis collision that treatment removed. **The word is the
  ACCOUNT's, not the commodity's**: this line says `fodder` while the crop-basket rows below it say
  `hay`, because a basket row names one PLANT and what that plant pays.
- **`fodder_rate_of(source)`** — the ONE per-source FEED rate (issue #449). **A plain read is the whole
  of it**: there is deliberately no `realized_fodder_yield`, a realized rate being a forward PROJECTION
  only the animal web makes while fodder is paid by the plant web alone, so a projected-fodder field
  would be a constant zero on the only web that can pay it. Its retired sibling `trade_rate_of` had a
  `realized_trade_yield` sentinel to dodge and needed a `> 0` test; nothing here does. **It reaches the
  work board only because `fodder_yield` is in `HudBandLaborState.OPTIONAL_YIELD_KEYS`** — a key not
  copied through `effective_worker_map` does not exist as far as the board, its chips and its header
  totals are concerned, whatever the decoder published.
- **`MATERIAL_PAYOFF_ID_KEY` / `MATERIAL_PAYOFF_AMOUNT_KEY` + `material_payoff_rows`** — the two keys
  of one per-material row and the normalizer every material vector runs through. A row naming no
  material is dropped: an id is what a row is FOR, and a nameless amount could only be rendered as the
  summed scalar this arc refuses. Beside it: **`scaled_material_rows`** (one vector times a scalar —
  the room, the dip, the band's output multiplier; every material scales by the SAME factor, because
  they are one biomass flow through a fixed per-biomass vector), **`expected_materials`** (the
  `min(workers × per_worker, ceiling)` clamp), **`material_rows_of`** (the RESOLVED yield off an
  assignment) and **`signed_material_components`** (every material, signed, joined — and `""` when
  there is nothing to say, which is the gate both one-slot surfaces test).

### THE AXIS IS PROVISIONS, AND IT IS NO LONGER A CHOICE

`herd_axis_rates` resolves the per-worker rate, the ceiling and the one-animal quantum a quantised take
divides by. It used to pick between provisions and trade (the sim's `ratio_axis()` rule: the first
component with a positive rate) because an inedible quarry's food quantum is honestly `0` and a
food-only derivation divides by zero. **With the trade account retired there is no second scalar to
fall back to**, so `herd_yield_axis` and `YIELD_AXIS_*` are deleted and the `axis_*` keys on
`forecast_inputs` are aliases of the food terms rather than a resolution — kept under their own names
because they mark WHICH question a consumer is asking. An inedible quarry answers zeros there, and every
consumer's own guard (`per_animal > 0`, `has_component`) turns its readouts off rather than quoting a
false food rate. The animals-per-turn line needs no currency word either way: **an animal count is a
ratio, and a ratio is unit-free.**

### NEVER clamp a per-herd preview with `huntPerWorkerProvisions`

`PopulationCohortState.hunt_per_worker_provisions` is a **species-BLIND** per-cohort echo of the global
`hunt.provisions_per_biomass` — the cohort has no herd in scope, so it cannot know the quarry is
inedible, and quoting its positive food rate against an inedible species' all-zero food ceilings is
exactly what manufactures phantom food. The species-aware per-herd rate is
`HerdTelemetryState.perWorkerYield`, and the local-hunt preview clamps with THAT. (The cohort field
survives as the expedition **outfit** lever, before a target is chosen.)

### `deliversFood` WAS REDEFINED — re-read every branch that keys off it

It no longer means "this is not a denial mission". It now means **the quarry is edible**. Consequences,
all of them live in `hunt_trip_forecast` / `hunt_forecast_line_bbcode` / `expedition_policy_takes`:

- **Eradicate DELIVERS.** It banks a whole-stock windfall like every other rung, and its raid line
  quotes that payload instead of "denial mission, delivers no food".
- A **denial** raid is one that **brings nothing home** — `delivers_food == false` **and** an empty
  `delivered_material`. Its `delivers_trade` half went with the axis (arc #527), which briefly made
  every inedible quarry a denial mission; `delivered_material` is what took it back out, because a
  raid that hauls hides is a real delivery whatever account that something is in. Still a property of
  the QUARRY and its payload, never inferred from the policy string.
- **"Too lean to raid"** tests `delivered_food <= 0` **and** an empty `delivered_material` — a
  food-only test sends every material-landing raid down that branch and prints a refusal at a party
  walking home loaded.
- `expedition_policy_takes` gates the food component on `delivers_food` and reaches the picker when a
  rung pays food **or** a material; `expedition_useful_cap` scans the plateau on food alone — an
  inedible species delivers 0 food at every party size, so the scan finds no
  plateau, which is the honest reading of a raid with nothing to bring home.

### The one-slot surfaces show the product the species PAYS

Two readouts have a single narrow slot and cannot carry a pair — the **work-board row's** fixed-width
rate column (`BandPanelController._work_row_rate_text`) and the **map's** on-tile yield label
(`BandOverlayRenderer._draw_yield_label`, whose choice is split out as `_yield_label_rate_text` so a
harness can ask it — a draw call renders to a canvas and no assertion can read a glyph back off one).
Both fall through **food → fodder → materials**, in the wire's own order: food when there is food (so
every forage patch and edible quarry is unchanged), else the fodder rate spelled with the WORD
(`+0.40 fodder`) — fodder has no glyph, and borrowing another account's would say the wrong thing —
else the MATERIALS, each naming itself (`+0.22 hide`). A trade branch stood between food and fodder
until arc #527; the material arm is what replaced it, one release later. The work **inspector strip**
beside the row states the whole vector in full.

**THE MATERIAL ARM STATES EVERY MATERIAL, NOT THE FIRST ONE.** Naming one of a vector picks a winner
the sim does not name, and summing them is the retired trade axis under a new name. A species pays
few materials; the board column's width is a MINIMUM rather than a clip and the map plate sizes to
its measured run, so a two-material label is wide rather than truncated — which is a legibility
question for `map_band_label_overlap`, not a reason to state less than the truth. Both surfaces gate
on `SourceForecast.signed_material_components` answering `""`, so "pays no material" is one call
rather than a condition each re-derives, and a source that genuinely produced nothing in every
account still falls through to its honest food zero.

**The fodder rung is the one this pair was reported on** (issue #449): a sown hay Field pays no
provisions, so with only one option both surfaces read `+0.00` on a tile that was filling the band's
fodder store every turn. Its own threshold is the food branch's (`YIELD_LABEL_COMPONENT_MIN` on the
map, `SourceForecast.has_component` on the row), so neither account can be shown at a magnitude the
other would have been hidden at.

**A HUNT call site passes NO fodder argument, and that is a decision rather than an omission.** No
animal is harvested for feed, so a hunt row's fodder is a structural zero and passing it would only
offer the label a fall-through it can never take. **It DOES pass the materials**, which is the arm
that closes an inedible quarry's `+0.00` — see "AN INEDIBLE QUARRY QUOTES WHAT IT PAYS" above.

**AN AGGREGATE STATES EVERY ACCOUNT ITS ROWS PAY, and that rule outlived the account it was written
for.** The work zone's header once read `3 sources +0.35 /turn` with a `⇄+0.22` wolf row directly
beneath it — the arithmetic visibly did not add up. The cure was a SIBLING total rather than a folded-in
one, and the fodder sibling is what survives of it: a band that grows hay heads `+0.40 fodder` beside
the food figure and chips `🌿 1 · 0.40 fodder`, under the same non-zero gate. **A sibling, never a
summand** — fodder credits the band's FODDER store and never the larder, so folding it into the food
figure would break the identity the Food line is denominated in
(`larder_delta == income − consumption − pen_feed − raid_forfeit`). Details in `band-city-panel.md`.

**When you add an aggregate, ask which KIND it is** — a *larder* figure (food alone, by the identity
above) or a *productivity* figure (**every** account the sources pay, each when non-zero). Nothing else
in the client currently sums or counts across sources: the parties header counts parties and workers,
and the attention producers key off `idle_workers` / `turns_of_food` / pen status. **Do not add a
"produces nothing" empty-state that tests food alone.**

**Frames.** `ui_preview`: **`herd_hunt_pelts_only`** (the inedible quarry, quoting `0.11 HIDE` — and
still asserted as a PAIR with `herd_hunt_both_products`, whose deer prints a live FOOD row, because
"prints nothing" and "is correctly silent" are the same picture and only a frame that still prints
tells them apart) · **`herd_hunt_pelts_raid`** (the same wolf as an expedition target — a REAL
delivery now, reading `≈5 GREY WOLF` over `2.75 HIDE`, asserted on three claims: not denial, not
empty, names the hide) · `hunt_picker_ascending` · `food_tile` ·
**`forage_three_accounts`** (the PLANT frame the rule is judged on since #426 — a hay meadow whose
rungs read `0.24 food · 0.40 fodder`, and the frame the picker's three-column ceiling was MEASURED
against rather than assumed) · **`forage_three_accounts_overdraw`** (the same meadow, three foragers —
the fodder account ALONE carries the warning while the food take sits inside the patch's own regrowth)
· **`forage_dead_season`** (a patch the wire fully DESCRIBES whose every cell is zero — the rungs still
render, they state `0.00 food`, and the worker cap stays live at `MAX_USEFUL_BARREN`).
`band_panel_preview`: **`band_panel_work_trade_rows`** / **`band_panel_work_trade_inspector`** /
**`band_panel_work_trade_totals`** (the board and its aggregates with an inedible quarry on it — the
frames keep their names and their subject moved: the wolf row reads `+0.00 /turn` because the wire
states no rate for it, not because the client dropped one). `map_preview`: `map_band_work`.

---

## The FODDER account can be real and unbankable at once (issue #485)

`ForagePatchState.fodderPerBiomass` states what the LAND pays. Whether the working band can BANK it is
a second fact, and it now ships beside the ladder: `IntensificationKnowledgeState.foddering`, decoded
as `foddering` in `native/src/dict/subsistence.rs`. The sim credits a **wild** patch's fodder take only
to a faction that has learned Foddering, or on a patch already **committed** to a crop — committing IS
the bid, so a committed patch is paid unconditionally (`systems/labor.rs`:
`patch.species.is_some() || knows(faction, FODDERING)`). Foddering is earned by **keeping a penned
herd** (the corral rung's `earns_knowledge`), so a pre-pastoral band structurally cannot have it: a
forager band on a wild hay meadow read `1.07 → 0.85 FODDER` on the compose sheet and banked nothing,
with no feedback anywhere.

**`RungGates.wild_fodder_reason(committed_species, knowledge)` is the one answer**, and it broadens
that file's remit from *"may this source climb its next rung"* to *"…and will the work it is doing
actually pay out"* — the same shape of answer (what is missing, its live progress, the remedy) asked
of the same threaded-in knowledge, which is why it belongs beside the rung gates rather than in a
second gate layer.

- **It takes the committed-species STRING, not the patch dict**, because every caller has already read
  that key and the `patch_`-prefixed-vs-bare trap that file documents is not worth re-entering.
- **The test reads the PUBLISHED commitment (`patch_committed_species`), never the COMPOSED
  improvement.** A Cultivate the player has ticked but not committed is not a bid the sim has
  accepted, and quoting it would unlock a credit that is still being refused.
- **It is a 0..1 learning meter, not a bool** — only `HudConst.KNOWLEDGE_COMPLETE` opens the credit,
  exactly like every other track, which is why the locked frame is fixtured part-learned rather than
  at zero.
- **TWO remedies, and this is the only gate reason on either web with two.** Both are real and reached
  from different ends of the game: learn the craft by keeping a pen, or commit this patch to its crop —
  and the improvement control that does the second is directly below the line on the same sheet.
  `HudFloraVocab.GATE_REASON_WILD_FODDER_FORMAT` takes the percent plus the two rung glyphs, the idiom
  `GATE_REASON_HERD_DOMESTICATED_FORMAT` already uses.

### What the readout does with it

`DrawerComposeController._forage_yield_model` resolves the lock once and, when it is locked AND the
fodder take is non-zero:

- **The fodder ROW keeps its UNIT and loses its NUMBER** — `HudComposeVocab.YIELD_LOCKED_GLYPH`, the
  em-dash, the one glyph that cannot be misread as a quantity (the reasoning
  `HudFloraVocab.STOCK_UNKNOWN_GLYPH` records for a fogged stock). **Hiding the account would be the
  hidden gate this repo forbids**: the ground really does grow hay, and a player who cannot see the
  account cannot see what learning Foddering would buy them.
- **The number renders `INK_FAINT` whatever the row tint is**, through a per-row
  `HudWidgets.YIELD_ROW_MUTED` flag. The tint passed to `build_yields_row` is a WHOLE-ROW parameter —
  it says how the TAKE reads — and one account being unbankable is not a property of the take, so a
  locked reading has to be able to sit beside two live ones in an overdrawing row.
- **The row drops its `after` reading.** An arrow to a rate nobody banks is noise, and its mere
  presence would key the row header's `now → after` off an account that has neither.
- **The joined sentence (`YIELD_MODEL_TEXT`) composes with fodder `0.0`.** A sentence has no room for
  the reason, so it must not promise the account at all — the readout can qualify a number, prose
  cannot. **That branch has no PLAYER-FACING reader today**, and knowing so is what stops the next
  reader treating it as a live prose surface: `YIELD_MODEL_TEXT` is read only by
  `_yield_preview_bbcode`, whose two callers (`_local_forage_preview_bbcode` /
  `_local_hunt_preview_bbcode`) are reached from the `ui_preview` chapters alone — the drawer's
  standing summary composes from `SourceForecast.source_yield_readout` instead. The term is kept
  because the producer is shared and the rule is what a re-wired preview must obey, not because a
  frame shows it.
- **THE OVERDRAW TEST KEEPS THE FODDER CEILING COMPARISON UNCHANGED, and deleting it is the plausible
  wrong move.** The take draws the same biomass down whether or not the crew banks the hay, and on a
  hay-only patch that comparison is the only drawdown signal there is.

The reason travels out on the model (`YIELD_MODEL_LOCKED_REASON`) and **`_mount_readout`'s aside closure
reads it off the SAME `yields_at` answer the yields host is built from**, at the same floor and the same
crew. What guarantees the muted row and the sentence explaining it cannot disagree is **not** a single
evaluation — `_mount_readout` calls `yields_at` three times per refresh (the emptiness probe, the yields
host, the aside). It is that the yield models are **pure** and every one of those calls passes
**identical arguments**, which `_live_floor` / `_live_reaches` enforce by having one definition apiece.
The hunt web needs no branch and no parameter: its model carries no such key, so the read answers `""`
and no line renders.

**IT IS IN THE LIVE SET, and its PRESENCE is what depends on the floor.** The sentence itself does not
move — it states what the FACTION is missing — which is exactly the reasoning that first put it outside
the registry, resolved once before the render. That was wrong: it is gated on the fodder take being
non-zero, and raising the floor (or stepping the crew to 0) takes the take, the row and the `—` away,
leaving a sentence explaining a mark no longer on screen. The rule this file already states is the whole
rule — **anything whose value *or whose presence* depends on the floor belongs in the set.**

It renders **FIRST in the aside**: it explains a `—` the player is looking at, while the floor hint and the
teaching line are standing copy. It carries **its own meta**
(`HudWidgets.READOUT_LOCKED_ACCOUNT_META`) for the reason `READOUT_TEACHING_META` exists — the aside's
other lines move with the floor and this one does not, so an assertion on "the aside changed" testifies
about this sentence in neither direction.

### What the FLOOR PRESETS do with it — they quote no fodder term at all

The readout is not the only surface on the sheet that composes a fodder ceiling. `_forage_floor_takes`
feeds `forecast["ceiling_fodder"]` into `SourceForecast.extractive_take_pair`, whose `full` string rides
each preset button's `tooltip_text` — so hovering `♻ Best harvest` read `up to +0.12/turn · +0.40
fodder/turn`, a quantity the sim will refuse, ONE CONTROL ABOVE a readout marked `— FODDER` and an aside
saying the hay stays in the field. **Where the fodder is locked, the preset tooltips quote no fodder term
at all** — not a dash, not a zero, no clause.

- **Because a tooltip is one flat string with nowhere to hang a reason**, and the sheet already states
  the lock ONCE, in the register built to explain it: the muted row plus the aside directly below the
  presets. Dropping the clause hides nothing that is not still on screen — the account's EXISTENCE is
  stated by the row that keeps its `FODDER` unit — whereas quoting the number states something the sim
  will refuse.
- **The account's own ZERO goes with its ceiling.** On a hay-ONLY patch fodder is `zero_account_of`'s
  answer, so merely passing `0.0` would print `up to +0.00 fodder/turn` on every preset — the refusal
  again, now at a number the ground contradicts. The zero account is stepped to
  `SourceForecast.YIELD_ACCOUNT_NONE`, which renders no line, and the preset keeps its name-only tooltip.
- **One predicate, through `DrawerComposeController._wild_fodder_lock`**, which both surfaces call and
  which is the only spelling of `RungGates.wild_fodder_reason` on this sheet. Two predicates over one
  gate is exactly how the presets came to quote a ceiling the row below them was already refusing.
- **Scoped to the FORAGE presets and to the WILD take.** The hunt picker has no fodder account to drop.
  Neither the CROP PICKER's rows nor the improvement control's PAYOFF faces (`Hay Grass 30% · 1.80 hay`,
  the `→ … fodder` terms) are touched: they quote what COMMITTING to the crop would pay, and a committed
  patch's hay is credited unconditionally — committing IS the bid — so gating them would state a refusal
  that does not exist.

**Frames — a THREE-STATE set on one patch, judged as a set**, because a lone negative here is satisfied
by silencing the account everywhere: `forage_fodder_locked` (wild meadow, Foddering part-learned → the
`—`, the reason by meta, the food row still a live number) · `forage_fodder_known` (the SAME patch,
Foddering complete → a live fodder number, no lock line, and the five-track strip) ·
`forage_fodder_committed` (the same patch COMMITTED with Foddering at 0 → a live number and no lock
line — the half that pins `species.is_some()`, without which the whole thing passes as "gated on
knowledge alone"). Sabotage-verified in both directions: pinning the lock ON fails the known and
committed states (and `floor_chart_drawn_down`'s two-account `now → after` claim), pinning it OFF fails
exactly the locked state's three assertions — no state passes under both.

**The PRESET TOOLTIPS ride the same three frames, asserted as a PAIR** (reached by
`HudWidgets.POLICY_RUNG_META` through the chapter's `_policy_rung_tooltip`, never by face text, since a
preset's face carries no metric at all): the locked frame's `♻ Best harvest` quotes no fodder clause
AND still quotes the food ceiling it can bank (`HAY_PEAK_TOOLTIP_FODDER_LOCKED`), while the
known and committed frames quote both (`HAY_PEAK_TOOLTIP`). Sabotage-verified three ways, each
failing a DISJOINT set: restoring the refused clause fails the locked negative alone; dropping the
fodder clause unconditionally fails the known and committed twins (plus `forage_three_accounts`' two
wire-order claims, which stand on the same tooltip); and blanking the locked tooltip outright fails the
"not merely blanked" half alone — which is why the pair is what is asserted and not the negative.

**The live-set claim is a DRIVEN DISAPPEARANCE, never a presence**, for the reason the frozen-yields
triple records: a stale sentence is a perfectly valid, perfectly findable node. `forage_fodder_locked`
captures the line, drives `floor_changed(f, committed = false)` to a floor above the stock, and requires
the line AND the fodder row to be gone while the chart survives. Sabotage-verified by resolving the
reason once before the render — exactly that one assertion fails, and only it.

**The hay-meadow block of `chapters/forage_accounts.gd` dials Foddering complete**, and that is a
fixture repair rather than a convenience: every meadow in it is WILD, so at the ladder's default dial
the frames that exist to show three live accounts would mute the third.

## The plant web's crew noun follows the STANDING RUNG

Every surface for a sown Field said *forage* / *Foragers* — the wrong verb, not merely an awkward
one. Reported from play.

**THE ANIMAL WEB HAS THE SAME RULE AND ITS COMMIT VERB WAS THE LAST THING NOT FOLLOWING IT.**
`_herd_crew_noun` has always resolved Hunters/Herders — a penned or fully-tamed herd is kept, not
hunted — and the eyebrow, the stepper and the drawer's `Assign … ▸` all read it. The commit button
did not: it was hard-coded to `ASSIGN_LOCAL_HUNT_BUTTON`, so an `ASSIGN HERDERS` sheet over a
`Herders` stepper committed with **`Hunt Here`**. Reported from play. `HUNT_ASSIGN_BUTTONS` keys the
verb off that already-resolved label, the twin of `PLANT_ASSIGN_BUTTONS` and of the `HUNT_NOOP_HINTS`
table that was already keyed that way — so on both webs the stepper's noun, the button's verb and the
dead-button hint's singular are three readings of ONE answer.

The verb is derived from the noun identically on both: Foragers→Forage, Tenders→Tend, Hunters→`Hunt
Here`, Herders→`Herd Here`. `Here` carries the local-vs-expedition distinction against the expedition
branch's `Send …`, which is why it appears on the animal verbs and on neither plant one.

> **The two webs differ on the IN-FLIGHT case, deliberately.** The plant resolver reads done-flags
> only — a crew mid-Sow is still foraging the wild stand, which is what the build dip charges them
> for — while `is_managed_hunt_source` counts a composed Corral, because keepers building a pen have
> already stopped hunting. Same rule, different answer, because the builds differ in what they do to
> the crew's work.
>
> Assert both webs as PAIRS. A verb hard-coded the other way satisfies every managed-source assertion
> on its own; the wild-herd line is what makes the managed one mean something.

**THE LADDER CONFIG IS THE AUTHORITY, and it already drew the distinction.** Each plant rung declares
a `harvest` primitive in `core_sim/src/data/intensification_ladder.json`: `wild` → `worker_take`,
`tended` → `worker_tend`, `field` → `worker_tend`. A managed source is never gather-drawn (the sim's
`is_managed()` branch), so a crew standing on one is not foraging at all. Two nouns, keyed on the
rung:

| rung | crew | commit verb | drawer button | work-board row |
|---|---|---|---|---|
| wild | `Foragers` | `Forage` | `Assign foragers ▸` | `Forage (x, y)` |
| Tended Patch · Field | `Tenders` | `Tend` | `Assign tenders ▸` | `Tend (x, y)` |

`Tenders` spans BOTH upper rungs deliberately. `Farmers` was considered and rejected — it reads right
on a Field and wrong on a Tended Patch, and three nouns to learn is worse than two.

**`HudFormat.plant_crew_label(src, prefix)` is the ONE resolver and every surface goes through it.**
The failure it exists to make unexpressible is a sheet whose header says one noun over a stepper
saying another — which is exactly what the animal web shipped once (`_herd_crew_noun`'s eyebrow
resolved against a stale improvement). It lives in `HudFormat` rather than `SourceForecast` because
that layer's stated invariant is that it references **no `Hud*Vocab` module at all**; `HudFormat` is
the "how the HUD SAYS a thing" layer and already reads both `SourceForecast` and `HudComposeVocab`.
The verb and the dead-button hint are keyed off the label it returns (`PLANT_ASSIGN_BUTTONS` /
`PLANT_NOOP_HINTS`, the shape `HUNT_NOOP_HINTS` already had), so noun, verb and singular are three
readings of one answer.

**ONE TEST ANSWERS BOTH UPPER RUNGS.** `improvement_is_done(…, CULTIVATE)` carries
`FORECAST_RETIRED_BY_HIGHER_RUNG`, so it is true on a Tended Patch AND on a Field sown straight from
wild ground (where `is_cultivated` is honestly false forever). A separate `SOW` test would be a second
spelling of the same answer, free to drift.

**A BUILD IN FLIGHT KEEPS THE WILD NOUN.** The resolver reads the source's DONE FLAGS and never a
composed improvement, so a crew part-way through a Cultivate or a Sow stays `Foragers`: those people
really are foraging the stand *and* clearing ground, which is precisely what the build dip charges
them for. The noun moves only when the rung COMPLETES. **This is where the plant web parts from the
animal one** — `_herd_crew_noun` DOES read the composed axis, because a herd being penned owes keepers
before the pen exists, while a patch owes nobody anything until it is managed. A naive "is an
improvement composed / under way?" test gets exactly this case wrong.

**DISPLAY ONLY.** The command is still `assign_labor` with kind `forage`; `LABOR_KIND_FORAGE`, the
work filter key, the `WORKFORCE` bar's `Forage` segment and every wire name are untouched. The two
aggregate surfaces that span mixed rungs — the workforce bar and the work-board filter chip — keep
`Forage` for that reason: they name a category over both rungs, not a crew.

**Frames + assertions.** `ui_preview` `plant_crew_wild` / `plant_crew_tended` / `plant_crew_field`
(the wild-sown Field, so the two upper rungs answer through DIFFERENT flags) / `plant_crew_wild_building`
/ `plant_crew_wild_sowing`, each asserting all four surfaces **and**, independently of the expected
noun, that the eyebrow and the stepper agree on that frame. Sabotage-verified four ways: pinning the
resolver to either noun fails exactly the other rung's states, the naive build-in-flight test fails the
two in-flight states, and resolving the eyebrow separately fails the consistency assertion alone.
`band_panel_preview`'s `_assert_work_row_rungs` pins the board's verb beside the rung MARK — two
orthogonal answers off one patch dict, so one passing cannot stand in for the other.

**The commit buttons carry `HudWidgets.COMPOSE_COMMIT_META`** (both sheets), because their face is the
thing every assertion here is ABOUT: finding one by text could only confirm the string the caller
already assumed. The three `ui_preview` sites that reached the forage commit / open button by face were
repointed at it, and the bare `assert` beside one of them became `_assert_hud` — under sabotage it broke
the headless run into the debugger and hung the suite, which is the hazard that rule already records.

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

**THE HIGHER RUNG WINS, and the test order is what enforces it.** A penned herd is *also* fully
domesticated, so `BandPanelController._work_source_rung` tests `is_field` before `is_cultivated` and
`corralled` before `domestication` — reversed, every rung-3 source would wear its rung-2 mark, which
is the one distinction the mark exists to draw.

> **A FIELD IS NOT NECESSARILY `is_cultivated`, and assuming it was cost a defect.** `Sow` needs no
> prior patch, so a Field sown from wild ground carries `cultivation_progress == 0` forever. Ordering
> alone therefore does not retire the lower rung — `SourceForecast.improvement_is_done` carries
> `FORECAST_RETIRED_BY_HIGHER_RUNG` for it, mirroring the sim's `forage_rung_already_built`
> (`Cultivate => patch.is_managed()`). Without it a completed Field OFFERED `Cultivate this patch`:
> a live checkbox for a build the server treats as already built, which its own docstring records as
> having "stalled forever, silently". Reported from play. The animal web needs no such term — Corral
> demands a herd already tamed, so its rung 2 cannot be skipped.

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

---

## RETIRED — the FILL TARGET, the party-side twin of the floor (issue #491)

**A raid's length is a species-and-kit constant, and the lever that moved it is GONE.** This section
records why it existed and why it is not coming back; the arc's other §5.2 half — the trip's BOUND —
is live and is documented where it renders (`SourceForecast.TRIP_BOUND_CLAUSES`, the trip readout's
verdict, `DetailFormat.expedition_trip_bound_line`).

Reported from PLAY: eight hunters after a Wild Fowl flock read *"away ≈43 turns — 31 hunting, 12
travel"*, with no control that moved the number. The mechanism (§5.1) is that a raid ends when its
pack fills, the pack is measured in **carry** and the take in **reach**, so

```text
turns_to_fill = carry / (engage_rate × stay_chance × body_mass)
```

and **party size cancels out** — both the pack and the rate scale linearly with hunters. The party
stepper was not a weak lever there, it was **structurally not one**.

**The fix shipped was a second dial and the fix that stands is a re-tune.** The fill target ("come
home with N animals") was the only player lever on that quotient, and what it was reaching for was an
escape from the trips nobody wants — Wild Fowl at 88 turns against Mammoth at 1.1. That spread is a
**tuning** problem in the species-and-kit terms above, and it is tracked as one on issue #491. Asking
the player to hold a second dial to work around a config spread is the kind of lever this arc spent
four fixes removing elsewhere.

**What went, in one list:** `ComposeState._party_fill_target` / `_hunt_fill_target` and their
accessors · `HudWidgets.build_fill_target_control` (+ `FILL_TARGET_META` / `_VALUE_META` /
`_TURNS_META`) · `SourceForecast.raid_fill_target_model` / `clamp_fill_target` / `raid_target_binds` /
`raid_target_turns` / `raid_animals_per_turn` / `NO_FILL_TARGET` / `RAID_TURNS_UNKNOWN` and the
`FILL_TARGET_*` label vocabulary · `TRIP_BOUND_FILL_TARGET` and its clause (the sim retired the
`HuntTripBound::FillTarget` variant with the lever, so no live row can carry it) · both compose
sheets' `BRING HOME` block · the `fill_target` argument to `SourceForecast.hunt_trip_forecast` and to
`Main.format_send_hunt_expedition`, and the `fill_target` key on the
`send_hunt_expedition_requested` payload.

**`send_hunt_expedition <faction> <band> <party> <fauna_id> [floor]` is CLOSED after the floor**, like
`send_denial_raid`'s four-token grammar: a second positional is an `UnexpectedArgument` parse error
rather than an ignored token, so a stale emitter fails loudly. `cargo xtask command-guard` parses both
launch sites' emitted lines with the real server parser, which is the only thing that can assert it.

**`DetailFormat.expedition_orders_line` KEPT ITS ROW and lost its target clause.** That row was merged
out of two lines to buy the parties inspector strip 18px of its ~300px budget; it stays merged and now
states the floor alone, because the budget is what forced the merge and a second orders row must not
come back for the next order a party learns to carry. See `band-city-panel.md` → "The parties strip's
SEVEN lines"; the worst-case strip still measures **294px of the 300px box** (the row count is
unchanged — only the row's width fell).

**The escapement graph STAYS on the expedition sheet**, and the crew targets stay off it. The note
that used to justify its absence — *"a raid's trip is a forward simulation, not a per-turn
drawdown"* — was wrong on its own terms: a party's per-turn take is the same `min(room, carry,
engagement)` a resident crew's is, so the curve IS the raid's. What the picture cannot show is where
the trip ENDS, which is the bound clause's job alone now.

## The FIGHT is stated before the party leaves (`docs/plan_hunt_through_combat.md` §2.1 / §6.5)

The combat gate produces a real outcome that reads as a bug unexplained: **hunters die and nothing is
killed.** So the hunt compose sheet says two things under its crew row, in that order — how many
hunters one animal takes, and whether these hunters can hurt it at all.

| line | expression | source |
|---|---|---|
| **the reach** | `1 / (engageRate × dip)`, through `SourceForecast.hunters_per_animal_face` | `HerdTelemetryState.engageRate` |
| **the gate** | `max(0, hunterAttack − defense)`, and above it `durability / that`, through `SourceForecast.hunt_gate_model` | `PopulationCohortState.hunterAttack` + the herd's `defense` / `durability` |

**THE SIM EXPORTS NO VERDICT, and that is the boundary rule working rather than an omission.** Both
expressions are linear and exact in terms already on the wire, so they ship as TERMS and the client
asks itself the question — a "can this band win" boolean would be an answer to something the client
can compute. What was genuinely missing is `durability`, which is what turns *"you cannot"* into
*"you cannot, and with spears it would take 62 hunter-turns"*.

- **`engageRate` is composed through `engagement_per_worker`, never re-divided.** That is the ONE
  composition of the `engageRate × dip` pair every crew target in this file already divides by, so the
  sentence and the stepper's cap cannot disagree; the dip rides it for the reason it rides them (hands
  gentling a herd are hands not stalking it).
- **Two phrasings, pivoting on `ENGAGED_AT_LEAST`.** The roster spans `0.05` to `10`, so a mammoth
  reads `20 hunters bring one … into contact` and a warren `One hunter brings 10 … into contact` —
  the same quotient read from its two sides, and `2.1`'s own "reads as" column. The pivot is the sim's
  own engagement floor rather than a display constant.
- **THE EFFORT FIGURE IS NOT DIVIDED BY THE PARTY.** The herd's accumulated wounds are deliberately
  not exported (damage carries between turns, so a part-worn quarry needs fewer), and the sim already
  answers duration where it matters in `huntTripEstimates` — a per-party turn count here would be a
  second, always-pessimistic duration model competing with it. It is hunter-turns for ONE hunter.
- **The client cannot state the counterfactual `§6.5` quotes.** *"…and with SPEARS it would take 62"*
  needs the equipped attack tier, which is `equipment.json` and not on the wire; what ships is the
  effort at THIS band's own tier, and the refusal names its `attack` beside the quarry's `defense` so
  the lesson still reads as the weapon rather than the headcount.
- **`defense` and `durability` must not be blurred**: defense is whether a hit counts at all,
  durability is how many counting hits it takes. The first decides the refusal, the second the effort.

### The engagement stage is the gate on BOTH lines, and that is the byte-identity

A **pen** and the whole **plant web** publish `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked
and a berry does not fight back — so `SourceForecast.has_engagement_stage` suppresses the pair and
neither sheet moves. The negative is asserted on a pen fixture that carries a REAL `defense` and
`durability` (`chapters/hunt.gd` `_combat_gate_pen`), so the silence is demonstrably the engagement
gate's doing rather than a fixture that omitted the terms.

### KNOWN GAP — the local per-turn readout does not carry the gate

The compose sheet's `PER TURN` row is client-composed from `forecast_inputs` (escapement × carry ×
engagement) and has **no combat term**, so a sub-gate party reads a positive take directly beneath a
line saying it would kill nothing. The SIM's own answers are correct — `SourceYield.actual` and
`huntTripEstimates` both resolve the fight, so a standing row and a raid forecast quote zero — and
§6.5's "two signals from different paths" is satisfied by those; it is the local pre-commit CURVE that
is gate-blind. Closing it means threading `hunterAttack` into `forecast_inputs`, which would put a
band-scoped term into a source-scoped layer and ripple through `max_useful_workers`, both crew targets
and the chart.

**Frames + assertions (`chapters/hunt.gd`):** `herd_hunt_gate_effort` (a speared party — the reach
line plus `62.5 hunter-turns`) · `herd_hunt_gate_blocked` (**the same mammoth, the same party size,
bare hands** — an A/B on the WEAPON, since only the band's kit moves between them), which also pins
that the reach line is UNCHANGED: contact is not the gate, and twenty bare-handed hunters do walk up
to a mammoth. Each line carries its own meta (`HudWidgets.HUNTERS_PER_ANIMAL_META` /
`HUNT_GATE_META`, the latter valued `true` while the fight is unwinnable) because they are composed
from disjoint wire terms and a single handle would let one break while an assertion on the other kept
passing.

---

## AN EMPTY RAID IS EMPTY FOR ONE OF TWO REASONS (`docs/plan_hunt_through_combat.md` §4)

**This section supersedes every passage above that calls the blocked raid "no surplus".** The
arithmetic is unchanged — `delivered_food <= 0`, still the one blocked case (its
`delivered_trade <= 0` conjunct went with arc #527's retired account),
still not `animals_taken == 0` — and what changed is the SENTENCE it renders.

That branch used to assert the herd was at its floor, and before the take resolved through the fight
that was the only way to reach it. It is not any more: **a party that cannot bring one animal down
inside the projection's horizon lands here too, with the herd's surplus standing untouched.** Reported
from play on a THRIVING Wild Aurochs herd — ten of eleven animals, four affordable above a 50% floor —
refused to a party of one as *"too lean to raid — its surplus is spent"*, two rows below the sheet's
own line saying it takes several hunters to bring ONE aurochs into contact.

**A wrong explanation is worse than a wrong number**: the remedies are opposites — wait for the herd
to rebuild, against send more hunters — so one sentence cannot serve both, and the one that shipped
sent the player to fix the thing that was not broken.

### The sim already tells them apart, and the client never infers it from the numbers

`HuntTripBound` names the stop that ended the projection, and exactly three of its five are reachable
with nothing delivered. `PackFull` / `FillTarget` cannot be: both require a load, and a load is a
delivery.

| `bound` | what it means here | the line, the button, the remedy |
|---|---|---|
| `floor` | the standing surplus is spent | *"%s is too lean to raid — its surplus is spent"* · `Herd too lean to raid` · wait, lower the floor, hunt it locally |
| `horizon` | the projection ran its length and the party never killed | *"%s stands above your floor — but this party cannot bring one down"* · `Party can't make the kill` · more hunters, better kit, smaller game |
| `herd_lost` | the quarry dies out under a raid that never made up a load | *"%s is gone before the party can make up a load"* · `Nothing left to raid` · leave it standing, find another quarry |
| anything else | unattributed | *"%s — the raid would return empty"* · `Raid returns empty` · names NEITHER side |

**`HUNT_EMPTY_REFUSALS` holds all three faces of one refusal in ONE entry** — `line`, `button`,
`reason` — and `hunt_empty_refusal` is the single resolution of the key. The button used to be a lone
`const`, which is exactly how a face reading *"Herd too lean to raid"* could sit under a line naming
the party: the same misattribution, one control further on. Adding a cause means adding all three.

**The unattributed entry is not back-compat, it is a refusal to guess.** Every live estimate row
carries a bound; a row that does not is a fixture bug, and it should read as unexplained rather than
as somebody's fault. Guessing is how the defect happened.

**`hunt_empty_refusal_reason` takes the FORECAST as well as the herd**, and `hunt_trip_no_surplus` is
renamed **`hunt_trip_returns_empty`** — it says THAT, never WHY, and the two were one function only
while there was one why.

### The raid's max-useful party carries the engagement arm now

`expedition_useful_cap` scans the sim's table for the party size at which the delivered payload stops
rising. **A scan can only report a bind it watches the payload run into**, and neither half of that
held on the reported herd:

- **A payload flat at ZERO is not a plateau.** Every sampled size delivered nothing, and the
  rise-then-break scan read that flatness as *"the first hunter was all that was useful"* — capping the
  party at ONE and printing `max 1 worker useful here` beside `6 hunters bring one Wild Aurochs into
  contact`. A size is useful when it lands something.
- **The engagement crew is past the end of the table.** `SourceForecast.expedition_engage_crew` floors
  the plateau on `engage_workers` over the room above the floor — the SAME primitive
  `max_useful_workers` reaches through `take_workers`, which is the whole point: a second definition of
  the engagement crew is what let the local sheet and the raid sheet drift. It is the **engage half
  only**, deliberately: the haul half is sized on `perWorkerBiomass`, a RESIDENT crew's throughput,
  while a raid hauls in its pack (`expedition.hunt.per_worker_carry`, not on the wire) — and the pack
  side is precisely what the plateau scan already watches.

It is a FLOOR on the demand side, never a cap: `assignable` still binds below it, so the note becomes
the labor-bound *"6 of 20 useful — free up idle workers to send more"* rather than calling the missing
hands idle. A detached party builds nothing, so the dip is `IMPROVEMENT_NONE`'s, resolved through
`build_dip` rather than written as a bare `1.0`.

**A herd with no engagement stage answers `0` and nothing moves** — a pen, the whole plant web, a
species the roster cannot resolve — which is the byte-identity this arc holds each time the arm reaches
a new consumer. Measured: of 271 `ui_preview` frames, exactly TWO move, and both are the flat-zero
raids whose party the scan used to clamp to one.

### Frames + assertions

**`herd_hunt_party_cannot_kill`** (`chapters/hunt.gd`) is the reported case: a Thriving Wild Aurochs at
`B/K` 0.91 with four animals affordable, `engageRate` 0.25, a speared party of ONE, and a table whose
every cell is `horizon` with a zero payload. It asserts the line names the party AND not the herd, the
disabled Send wears the same entry's face, the reason carries the party's remedies and none of the
herd's, the ceiling is the engagement crew (20) and not `max 1 worker useful here`, and the reach line
is unchanged beneath it.

**Judge it as a PAIR with `herd_hunt_no_surplus`**, whose every cell is `floor` and which delivers the
identical zero: without that half every claim above passes on a sheet that blames the party for
everything. It carries the herd-side line and — publishing no `engageRate`, so nothing floors its
ceiling — the zero-plateau claim.

Sabotage-verified six ways, each failing a DISJOINT set: resolving every bound to the FLOOR entry
(**the old misattribution, restored**) fails the line, the button and the reason; hard-coding the
button's face fails the button alone; ignoring the forecast in the reason fails the reason alone;
dropping the engagement floor fails the ceiling alone; treating a zero payload as a plateau fails the
lean herd's claim alone; and resolving every bound to the PARTY entry fails the herd-side control.

**The fixtures had to learn the rule too.** `HerdFx.clean_raid_bound` stamps a zero-payload row with
the bound the sim would actually report — `floor` above a floor of 0, `herd_lost` at 0 — because a row
carrying `pack_full` with an empty payload is a herd no live server can produce, and it would fall to
the unattributed entry and make every assertion about *which* refusal is rendered testify about
nothing.

### STILL OPEN — the raid sheet's CHART is composed against the pre-fight model

The expedition branch's escapement chart walks `project_stock`, whose per-turn take is
`min(room, carry, engagement)` and carries **no combat term** — so on `herd_hunt_party_cannot_kill` it
draws the herd being drawn down to the floor by a party the sheet's own refusal says kills nothing.
It is the same gap "KNOWN GAP — the local per-turn readout does not carry the gate" records one
section up, reaching the raid sheet through the chart the fill-target slice brought back, and it closes
the same way: threading `hunterAttack` into the projection, which puts a band-scoped term into a
source-scoped layer and ripples through `max_useful_workers`, both crew targets and the verdict.

---

## THE FORAGE SHEET NAMES THE PLANTS AGAIN — the selective gather's chip row

A `Forage` assignment carries the species its crew carries home (`LaborAssignment.takeSpecies`, an
empty list meaning *the whole basket* and byte-identical to every assignment sent before the field
existed). The control is **one row of chips on the compose sheet**, mounted under the kit row and
above the improvement control, so the sheet reads **band → floor → crew → kit → what we carry home →
what we are building → the terms**.

**A CHIP IS WRITTEN EXACTLY AS THE TILE CARD WRITES THE SPECIES** — `🌾 Wild Emmer 45% (40)` —
composed from the card's OWN consts (`FoodIcons.for_crop_role`, `HudFloraVocab.FLORA_SHARE_FORMAT`,
`FLORA_SHARE_BIOMASS_CLAUSE_FORMAT`), never a second spelling. One plant reads one way in this client
or the card and the sheet start disagreeing about the same stand the first time either moves.

**THE BRACKETED NUMBER IS OFF THE WIRE.** `ForagePatchState.compositionStandingBiomass` is
index-aligned with `composition` and the decoder **folds it onto the entry it belongs to** as
`standing_biomass` rather than publishing a parallel array — the schema says a client must read the
two as one object, and folding makes that structural. It also means the patch's cross-ref needs NO
new key: `composition` travels whole in `patch_composition`, so the two-wirings trap that has bitten
the plant web three times cannot reach this field. A plant the wire quotes no quantity for renders NO
clause rather than a `(0)`; `flora_basket_entries` carries `has_standing_biomass` beside the value
for exactly that, `0.0` being a real reading. **The tile card's basket rows read the same key** —
`DetailFormat._flora_biomass_split` used to re-derive `share × rounded stock`, which agreed with the
wire in production and was a second producer of one question; it now reads the wire and keeps only
its display-side remainder fold, so the two surfaces cannot drift (`land-readouts.md` → "EACH ROW
STATES ITS ABSOLUTE").

### Three states, and the third is the point

| state | mark | ink |
|---|---|---|
| **included** — nothing ticked anywhere, so everything is coming home | `☑` / `◉` | `INK_DIM` on both mark and face |
| **picked** — explicitly ticked | the same shape, lit | `SIGNAL` (take) / `HEALTHY` (crop) mark, `INK` face, filled pill |
| **excluded** — dimmed, because something else was picked | `☐` / `○` | `INK_FAINT` on both |

**The DEFAULT state must read as faintly INCLUDED rather than as off** — with nothing ticked every
plant really is coming home, and an OFF shape there says the opposite. **Ticking every species
collapses back to the default** (`ComposeState.toggle_forage_take_species` takes the basket size for
it): "all of them" and "the whole basket" are one instruction, and keeping them apart would put a
selection on the wire saying nothing the omission does not.

**THE MARK MOVES A STEP WITH THE FACE, and that is what makes DEFAULT and EXCLUDED tell apart at the
HUD's real type size.** The two shapes differ by ~2px of a 13px glyph, so on a rendered frame the
distinction is carried by WEIGHT; leaving both marks at one ink halves that gap for nothing.

### Foraging takes several, cultivating takes one, and the SHAPE carries it

Same chips, same place, two different acts — so the affordance forks and the label does not. A square
box takes several (the take selection); a round one takes exactly one (the crop a Cultivate or Sow
commits the ground to). `single_pick` is `composed_improvement != IMPROVEMENT_NONE`, which on this
sheet means a rung already declared or running, and the crop still travels as `assign_labor`'s
`species` token exactly as it did.

> **THIS PARTLY REVERSES §4.7a ③, AND THE NARROWING IS THE POINT.** The crop picker left this sheet
> because *"the CROP TO TEND shouldn't be a selection here as the user can't do the cultivate here"* —
> a sheet that could not declare a rung had no business configuring one. What comes back is not that
> picker: on a plain gather the row is a TAKE control and offers no crop at all, and it only becomes a
> crop picker where a rung is **already in flight**, i.e. exactly where the player CAN do the
> cultivate here. The `⌃` on the Work board still declares; this states which plant it commits to.

**The consequence line states the cost, and it differs by verb** (`HudFloraVocab.TAKE_NOTE_*`): a
gatherer leaves the plants nobody picked standing, a cultivator weeds them out. The
cultivate-with-nothing-picked line NAMES the crop the game would settle on, because silence there is
the game choosing for the player without saying so.

**WHETHER THE CROP WAS CHOSEN OR SETTLED IS THE MODEL'S TO REMEMBER.** `resolve_forage_species`
writes its answer back every render, so from the second render on the player's pick and the game's
default are the same string and a before-and-after comparison reads every settled crop as a chosen
one. `ComposeState._forage_species_chosen` is the fact: written by the chip, written by `seed_forage`
(the band's row IS the player's stated intent), and cleared by the resolver whenever it MOVES the
value — a fall-back being the game's answer however it got there.

### The chips PRICE THEMSELVES, and the sheet is one arithmetic

A sheet whose forecast moved for the worker stepper and sat still for the chips taught that toggling
was free when it is the entire decision. `provisionsPerBiomass` on the patch is the BASKET AVERAGE
and cannot quote a narrowing at all — so the wire carries the same quantity **per plant**,
`ForagePatchState.compositionProvisionsPerBiomass` and `compositionFodderPerBiomass`, index-aligned
with `composition` exactly as the standing biomass is and folded onto the same entries by the
decoder. Both are at the patch's standing rung with the favored crop's conversion gain already in,
and **neither is pre-scaled by share** — the share sits on the entry beside them, which is what makes
a SUBSET composable at all.

```text
available = max(0, biomass − floor·K) × Σ_S share
rate      = Σ_S (share × rate) ÷ Σ_S share        <- the rate WITHIN the selection
take      = min(workers × perWorkerBiomass, available) ; food = take × rate
```

**⛔ NEVER SUM THESE RATES ACROSS SPECIES.** Without the shares the sum is not a total of anything;
`SourceForecast.selection_rates` is the ONE place in this client that composes them, and it is a
weighted MEAN.

#### The composition is expressed as a SOURCE, not as a second take model

`SourceForecast.narrowed_source(src, prefix, rates)` returns the patch as the ticked plants alone —
`biomass`, `carrying_capacity` and the regrowth curve each scaled by `Σ share`, the three per-biomass
accounts substituted, and `per_worker_yield` / `per_worker_material` re-composed off the throughput
(the wire's are that throughput at the BASKET's rates, so they are multiplied out again rather than
nudged). `DrawerComposeController._forage_take_source` is its one caller, and everything below it on
the sheet that answers about the TAKE reads that dict through the code it already read the whole
patch through:

| reads the narrowed patch | reads the RAW patch |
|---|---|
| the forecast, hence the worker cap and its `max N useful here` note | the basket itself |
| the floor presets' per-preset takes | the commit crop |
| the chart, hence both crew-target pills | the improvement control and its deal row |
| the readout, hence the `now → after` walk | `rung_lesson_known`, the build's own terms |

**A SECOND TAKE MODEL WAS THE ALTERNATIVE AND IS THE WRONG SHAPE.** It would drift from the
whole-basket one the first time either moved, and the `now → after` walk — which is precisely what
the player must see move when a chip is ticked — would have had to be written twice.

**THE STAND SCALES AND THE CREW DOES NOT.** A worker's basket does not shrink because they walk past
the flax, so `per_worker_biomass` is untouched while the stand takes the share; scaling `biomass` and
`carrying_capacity` together is what makes `escapement_room` return `Σshare × (B − floor·K)` exactly.

**THE CHART IS NARROWED TOO, and that is not a slip.** Both its crew pills are clickable and both
are clamped to the stepper's cap, so a cap divided from the ticked plants' stand beside pills drawn
from the whole basket's would name a count the `+` refuses — the panel arguing with itself, which
this file already records once. Uniform scaling leaves the stock FRACTION `B/K` untouched, so the
curve's shape, the floor's position on it and the phase bands behind it are exactly the whole
patch's; what shrinks is the absolute biomass, which is the selected plants' stand and is the number
the chips state.

#### THE IDLE WARNING IS THE STEPPER'S OWN NOTE, and it moves with the chips now

There is deliberately no second sentence under the chip row. The crew stepper already carries
`max N useful here — more would be idle`, and that N divides the NARROWED patch's ceiling — so
ticking a scarce plant lowers it and the stepper says so in the words it already uses for every other
way of running out of useful hands. A `_take_idle_note` reading the standing row's `workersNeeded`
was a SECOND producer of one verdict (the shape this arc has shipped three defects of) and is
retired.

#### THE MATERIAL ACCOUNT COMPOSES PER MATERIAL ID — the case the feature was argued on

**Baskets are made of fibre and baskets are what let a gatherer carry more food, so *tick cotton, see
how much fibre* is the first thing a player tries.** `material_per_biomass` on the PATCH is
basket-averaged, so for one release that question was answered with an apology;
`ForagePatchState.compositionMaterialPerBiomass` is the same quantity **per plant** — one
`SpeciesMaterialRates` per composition entry, a wrapper table only because FlatBuffers has no
vector-of-vectors — and `selection_rates` composes it through the identical weighted mean the two
scalars take, applied **per material id**.

```text
rate[m] = Σ_S (share × amount[m]) ÷ Σ_S share     <- the SAME denominator on every material
```

- **MERGE BY ID, NEVER BY LAST WRITE.** Two ticked plants both paying `fibre` compose into ONE fibre
  rate — which is what a rate means, and what the store sums the same way. A last-write-wins
  composition passes every single-species selection and is wrong by a factor the moment cotton stands
  beside flax, which is why the harness's merge claim is arithmetic on the composition rather than a
  reading off a rendered take.
- **THE DENOMINATOR IS THE WHOLE SELECTION'S SHARE, on every material.** A plant that pays no fibre
  contributes a zero to the fibre mean rather than leaving the mean to the plants that do — the
  selection is one crew gathering one stand, and dividing each material by only its own payers would
  quote `flax + oak mast` the same fibre rate as `flax` alone.
- **THE ROWS' ORDER IS THE BASKET'S**, kept as a first-seen list beside the per-id sums, so the
  rendered rows do not reshuffle between renders.
- **⛔ NEVER SUM THEM INTO ONE materials/turn FIGURE**, here or anywhere: that is the retired trade
  scalar under a new name. **And never merge two species' CHARACTERISTIC readings** — the sim
  deliberately merges rows by id only WITHIN one plant, because averaging two species' readings
  invents a plant that is not growing there.
- **EMPTY MEANS "NO ROW", NEVER ZERO, and presence still needs its own key.** A grain pays no material
  and says so with an empty list, which composes as a zero contribution and renders nothing; an entry
  the wire's wrapper vector never reached is a server that stated nothing, and only THAT makes the
  selection unquotable.

#### …AND WHAT IS STILL NOT KNOWN IS SAID OUT LOUD

ONE silence survives, and it rides the model as `YIELD_MODEL_NOTES` — an `Array[String]` of asides
`_fill_yields_host` renders under the rows — for `YIELD_MODEL_LOCKED_REASON`'s reason: whoever
evaluates this model at a floor and a crew gets the rows and the reason they read that way together.

**A narrowing the wire priced no per-species rate for quotes NOTHING** (`TAKE_UNQUOTED_NOTE`). The
composition is a weighted mean, so one missing term is not a term that can be left out of it, and
there is nothing else this client holds that a rate could be recovered from. **A `0.0` rate is NOT
this case** — a cash crop pays no food and says so, the selection is fully quoted, and `yield_rows`'
own render-where-it-pays rule then decides whether a FOOD row exists at all. Presence travels on the
entry's `has_*` key for exactly that reason, and it is checked on all THREE accounts.

**`_wordless_take_model` is what that silence renders through** — an empty row set carrying only the
aside, so the sheet says why rather than going blank. **A narrowing to cash crops alone no longer
reaches it**: cotton's fibre is a composed row like any other now, and what still lands there is a
selection that genuinely pays into nothing.

### The selection rides EVERY commit, in full, never as a delta

Re-issuing `assign_labor` without a `take:` token **CLEARS** the selection sim-side, exactly as it
clears the floor and the commit crop. Three consequences, and each is a place the selection would
otherwise be lost silently:

- **The compose sheet sends `_compose.forage_take_species()` on every press**, and the empty answer is
  what keeps a composition that never touched the chips emitting the byte-identical line it emitted
  before the chips existed.
- **`seed_forage` seeds it from the band's own row**, so a sheet reopened over a narrowed crew
  restates what the band HAS rather than widening it back on the next commit.
- **`BandPanelController._emit_work_assign` restates it**, the rule the kit and the improvement
  already follow: a `+`/`−` on the work board that dropped the token would widen a crew the player had
  narrowed to one plant. It is read off `HudBandLaborState.take_species_for_forage` rather than the row
  model, which carries no take selection.

**The command token is `take:emmer,flax` — a PREFIX, lifted out of the tail wherever it sits, like
`kit`.** The forage tail's two optional positionals are already told apart by shape and a third would
be indistinguishable from the commit species. `Main._take_species_token` omits it on an empty
selection, which is what an absent token means to the parser.
