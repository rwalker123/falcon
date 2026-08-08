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
| `ui/hud/HudBandLaborState.gd` | `RefCounted` state model (HUD decomposition Phase 0) — "the digested per-snapshot player world + optimistic overlay": `player_band`/`player_bands`/`panel_band`/`player_expeditions`, `world_herds`, grid scalars, `current_turn`, the `prev_band_sizes` losing-population diff, the `forage_patch_lookup`/`food_module_by_tile` lookups, and the `pending_labor` optimistic overlay. Ingest mutators (`set_turn`/`set_grid(width, height, wrap)`/`set_world_herds`/`set_panel_band`/`ingest_snapshot_bands`/`set_food_modules`/`set_forage_patches`) + the pending API (`record_pending_assign`/`record_pending_move`/`reconcile_pending`/`pending_assigns_for`/`pending_key`) + the moved-on derived readers `effective_worker_map`/`effective_idle`/`effective_forage_workers`/`effective_hunt_workers` (pure functions of `pending_labor` + a band) + the statics `as_schedule` and **`labor_assignments_of(band)`** (the public band-dict `labor_assignments` reader — `DetailFormat` + `AttentionController` reach it as `HudBandLaborState.labor_assignments_of`; it merged HudLayer's `_labor_assignments_of` static into the byte-identical private copy that already lived here, and its four internal callers now call it unqualified. The MapView-side `BandOverlayRenderer._labor_assignments_of_marker` deliberately stays a LOCAL copy — a renderer must not depend on the HUD's band-labor model). Also owns the **thin band-labor readers** every consumer reaches through `_band_labor.` — the roster pair `current_player_bands`/`player_band_by_entity`, the per-source lookups `forage_assignment_of`/`hunt_assignment_of` and their `workers_for_forage`/`workers_for_hunt`/`policy_for_hunt`/`policy_for_forage`/`assignable_forage_workers`/`assignable_hunt_workers` — plus the DERIVED READS over its own tables that the `BandPanelController` shared-layer pass brought home: `find_world_herd` (8 call sites file-wide — herds MIGRATE, so this list, never an assignment's launch-time target, is the authority on where a hunted herd is), `food_module_icon` (+ its `FOOD_SITE_KIND_GAME_TRAIL` key), `effective_role_workers`/`workers_for_role` (the band-wide-role twins of `effective_forage_workers`/`workers_for_hunt`), and **`band_parties`/`band_party_workers`** — the pair that KILLED the band↔parties straddle, since the WORKFORCE bar's Parties segment and the parties zone's row set now read one filter over `player_expeditions()` rather than the band zone calling into the parties zone. Plus the canonical policy-rung consts `HUNT_POLICY_OPTIONS`/`FORAGE_POLICY_OPTIONS`/`DEFAULT_HUNT_POLICY` (the last aliases `SourceForecast`'s; `HudLayer` re-exports all three via `const X = HudBandLaborState.X`). Emits `changed(reason)`, consumed by nothing yet |
| `ui/hud/ComposeState.gd` | `RefCounted` state model (HUD decomposition Phase 2c-1) — "what the player is dialing but has not committed": the tile card's **forage** compose (`forage_key`/`count`/`policy`/`species`/`band` + its autofill one-shot), the herd drawer's **hunt** compose (`hunt_key`/`count`/`policy`/`band` + its own one-shot), the Band panel's PARTIES-zone **party** compose (`party_quarry_id` + its one-shot) on its own clearly-separated accessor group so a later band-panel extraction can take it without unpicking the drawer's, and the open sheet's subject identity (`kind`/`subject`; `COMPOSE_KIND_*` alias to its `KIND_*`). Mutators are named for the transition — `begin_*_source` + `seed_*` (the two-step source-changed re-seed: the caller must resolve the actual band between them), `set_*`, `arm_*_autofill`/`consume_*_autofill`, `reset_*_source` (the harnesses' way to stage a fresh compose), `set_composing`/`clear_composing` — and the three READ-MODIFY-WRITEs get explicit ones so the field is never read and written apart: **`clamp_forage_count`/`clamp_hunt_count`** and **`resolve_forage_species(resolver: Callable)`** (the RMW is the model's; the crop RULES stay with the caller, so it holds no flora knowledge). Pure DATA — which is exactly why **the `ComposeSheet` NODE lives on `DrawerComposeController`**, beside the lifecycle that opens it, rather than on this model. The model instance is SHARED: HudLayer (the parties zone) and that controller (the drawer) hold the same one. Deliberately **NO `changed` signal**, unlike the Phase-0 pair: nothing subscribes (the compose builders re-render explicitly) and unused API is a liability. **`hunt_policy()` is PUBLIC beyond its builder, but its readers are all HERD-DRAWER ones now** (`_tame_stalled_hint` / `_herd_crew_noun`): `HudWidgets.build_policy_picker`'s `selected` fallback — the one real cross-boundary read, where a work-inspector or party-compose render picked up the DRAWER's rung — was DEAD (every caller passed an explicit, provably non-empty `selected`) and is **deleted**; `selected` is a REQUIRED param, so the shared builder now owns none of its callers' state and the drawer/band-panel boundary is structural rather than conventional |
| `ui/hud/DrawerComposeController.gd` | `RefCounted` controller (HUD decomposition Phase 2c-2b, `docs/plan_hud_decomposition.md`) owning the selection drawer's **COMPOSE half** — the other half of the selection card, after `SelectionCardController` took the identity/list one. It holds the **compose-sheet lifecycle** (`_ensure_compose_sheet` / `open_forage_compose` / `open_herd_compose` / `refresh_compose_sheet` / `is_compose_sheet_open` / `close_compose_sheet` / `_compose_anchor_rect`, and the `ComposeSheet` NODE itself), the two **drawer-action builders** (`build_forage_drawer_actions` / `build_herd_drawer_actions` + the standing-summary / compose-open-button / extend-pen factories and their in-place diffing twins), the two big **compose builders** (`_build_forage_assign_controls` / `_build_herd_assign_controls`), and the **compose-only** forecast/gate/picker layer beneath them (`_forecast_worker_cap` / `_forecast_yield_row` / `_is_overdraw` / `_hunt_take_rate` / `_hunt_delivered_and_waste` / `_hunt_avg_window_turns` / `_hunt_policy_takes` / `_payoff_take` / `_local_hunt_preview_bbcode` / `_local_forage_preview_bbcode` / `_forage_policy_takes` / `_forage_policy_gates` / `_hunt_policy_gates` / `_sow_site_refusal_reason` / `_tame_stalled_hint` / the `_flora_entry_*` sub-layer / `_build_crop_picker` / `_build_band_picker`) — ~1,400 lines, 54 functions. It also owns the drawer-actions diff caches `_forage_drawer_shape` / `_herd_drawer_shape` (zero external readers), so a per-snapshot restate still patches nodes rather than tearing them down. The drawer RENDER DISPATCH (`_render_land_drawer` / `_render_occupant_drawer` / `_render_subject_drawer` / the terrain-lines producer + `_tile_detail_lines_cache` / `_fit_subject_drawer`) and the `%AllocationPanel` expedition/band-move branches later left `HudLayer` too, into `ui/hud/SubjectDrawerController.gd` (Phase 2c-3), and call IN here through `refresh_compose_sheet` / `build_forage_drawer_actions` / `build_herd_drawer_actions`. Hud holds it as `_drawercompose`, constructed in `_ready` after `_selectioncard`. **THE INJECTION SURFACE IS EXACTLY THREE CALLABLES** — `_resolve_assign_band` / `_herd_label_for_id` / `_emit_assign_labor`, each retained on HudLayer because it has callers on the other side too (and `_emit_assign_labor` additionally owns the `assign_labor_requested` emit, the optimistic pending write and `_after_pending_change()`, which is why `assign_labor` stays INDIRECT). Each is reached through a **typed adapter** rather than called raw — `Callable.call` returns `Variant`, which would push an untyped value into every consumer. Everything else is a collaborator: the SAME `_compose` / `_band_labor` / `_selection` model instances (BY REFERENCE), `_topbar` for `faction_knowledge` ONLY (the rung gates), `_selectioncard` for `tile_contents_unseen` ONLY, the two drawer-action containers it fills (`%HerdAssignControls` / `%ForageAssignControls`), `tile_panel` READ-ONLY (the rect the sheet floats beside), and the HUD CanvasLayer as the **host** it `add_child`s the `ComposeSheet` into (a `RefCounted` cannot parent — the `TurnOrbController` fork-panel pattern). **Three absorptions shrank that boundary from six injections to three:** `_expedition_party_cap` → `SourceForecast.expedition_party_cap` (expedition forecast math, beside its sibling `expedition_useful_cap`), `_format_food_module_label` + its `FOOD_MODULE_LABELS` table → `HudFormat.food_module_label` (vocabulary, not compose logic), and — the highest-leverage one — the grid-wrap flag `_grid_wrap_horizontal` **onto `HudBandLaborState` as `wrap_horizontal()`**, beside the `grid_width()` it is meaningless without, so the moving set calls `SourceForecast.hex_distance_wrapped(…, _band_labor.grid_width(), _band_labor.wrap_horizontal())` DIRECTLY and the `_hex_distance_wrapped` injection disappeared (that pass-through survives on HudLayer for its other callers). `_band_display_name` went to `HudFormat.band_display_name` for the same reason. **It emits TWO signals, both RELAYED by HudLayer** (the controller never emits a HudLayer signal): `send_hunt_expedition_requested` → `HudLayer.send_hunt_expedition_requested` and `extend_pen_requested` → `HudLayer.extend_pen_requested` (the latter travels because `_build_extend_pen_control`'s only caller and its diffing twin are both inside). **`is_compose_sheet_open` / `close_compose_sheet` MUST stay callable on the HUD node** — `Main._unhandled_input`'s Esc precedence and ~11 ui_preview sites probe them BY NAME, and a `has_method` probe fails SILENTLY — so HudLayer keeps them as thin delegators. Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the `HudWidgets`/`HudFormat`/`TopBarReadouts`/`SelectionCardController` convention. Behaviour identical to the old inlined drawer-compose code |
| `ui/hud/ComposeSheet.gd` | The selection card's **write state** — the floating **compose sheet** (`docs/plan_tile_panel_layout.md` §10-§15). Composing is MODAL BY NATURE (open, decide, commit, done), so the two ~270px compose blocks (`%ForageAssignControls` / `%HerdAssignControls`) left the drawer for a sheet that borrows space only while in use; the drawer keeps the detail rows, a one-line standing summary and an `Assign … ▸` button. **That button wears `primary` while ITS sheet is open and `ghost` at rest — never `armed`**: `armed` is the destructive/warned treatment (DANGER border), and "its sheet is open" is a LIVE state, which this HUD spells in SIGNAL cyan (the Sight chip, the selection accent, the turn orb's calm pulse). **Its card is an `AutoSizingPanel`, NOT a `DockScrollFit` card** — it floats against the VIEWPORT, which is the opposite of what the drawer above needs, and picking wrong misbehaves silently rather than failing (`.claude/rules/client/panel-framework.md`). **Its width is FITTED to its content like its height** — `CARD_WIDTH` is the nominal, not a cap; see "THE CARD IS AS WIDE AS ITS WIDEST ROW" below, and "THE HEIGHT CHROME IS THE HEADER **ROW**" beside it for the same measurement error on the other axis. **`_panel` is held as a member for the assertion, not for the layout** — the `PanelContainer` that draws the card is a real `Container` in a plain `Control`, so its minimum is the one honest measure of what the fit owes. **The node IS the full-screen dismiss catcher with the card as its CHILD**, reusing `NarrativeForkPanel`'s nesting exactly (siblings make the ordering ambiguous and the catcher eats the card's own clicks), pinned to the viewport EXPLICITLY via `_sync_to_viewport` — a hidden Control's anchors never settle, and the full-rect preset would also overwrite the size. **NO SCRIM, and that is the one deliberate departure from the fork panel:** a fork is a story beat demanding attention, an assignment is composed *against* the map (work-range ring, herd position, hunt reach are all live context), so the catcher dismisses without dimming. **And that is also why the catcher dismisses on a real CLICK only, never a wheel tick** (`DISMISS_BUTTONS`, an ALLOWLIST of left/right/middle so a future Godot wheel/extra index stays non-dismissing by default): the catcher is `MOUSE_FILTER_STOP` across the whole viewport, so an idle scroll over the un-scrimmed map lands on it, and dismissing there would throw away the composition mid-read. `NarrativeForkPanel` is deliberately left as-is — a modal scrimmed story beat has no such gesture — so the two diverge here on purpose; do NOT factor out a shared predicate for one differing call site. (**Not** a map-zoom passthrough: the catcher stops the wheel either way, so the map cannot zoom while a sheet is open, and a wheel over the card is absorbed by its own `ScrollContainer`.) Guarded by ui_preview's paired wheel-leaves-OPEN / left-click-CLOSES assertions. The sheet floats BESIDE the selection card (`_place_card`, falling back to the viewport margin) so the list + summary it is editing stay readable. It knows nothing about foraging or hunting: `open(eyebrow, title, subject_key, anchor)` returns the content VBox and the caller fills it. `subject_key` is what lets a per-snapshot refresh tell "the same source, restated" from "a different source, gone" |
| `ui/hud/RungGates.gd` | **All-`static`, stateless** shared RUNG-GATE layer — the one answer to "may this source climb its next rung, and if not, why not?". Extracted from `DrawerComposeController` (issue #412) when the compose sheet stopped being the only surface asking: the Band panel's WORK board marks a source that can climb, and the MAP marks it on the source's own marker — and a renderer must not depend on the HUD's compose controller. Shared-layers-BEFORE-controllers, the same measurement that produced `SourceForecast` and `HudWidgets`. Holds `forage_gates` / `hunt_gates` / `sow_site_refusal_reason` (moved VERBATIM, so the compose sheet's greying is unchanged), **`forage_gates_from_patch`** (the BARE-keyed twin for a raw wire patch — the RAW wire patch carries its keys BARE while the `tile_info` cross-ref `patch_`-prefixes every one of them, and this adapter is the ONE place that mapping is written down. **The prefixing is UNIFORM now (#442)** — `is_cultivated`/`cultivation_progress` were the last unprefixed strays on the cross-ref and are stamped `patch_`-prefixed like their siblings, so there is no longer a mixed convention to remember; reading a `tile_info` key without the prefix silently answers nothing (`hud_compose_vocab.gd` → `BARE_FORECAST_PREFIX` carries the long form)), and **`next_rung_ready`** — the READY test all three surfaces mark from — plus **`knowledge_gate_unmet`** (with its `RUNG_KNOWLEDGE_TRACKS` map: is THIS rung blocked on knowledge specifically? — the same `track < KNOWLEDGE_COMPLETE` test the gate builders make, asked on its own so the compose sheet can suppress that reason **structurally instead of by matching its words**; one caller, for the reason the "A KNOWLEDGE gate renders NO improvement control" section gives). **`wild_fodder_reason` broadens the file's remit** from "may this source climb its next rung" to "…and will the work it is doing actually pay out" — the wild forage patch's fodder credit, which the sim refuses to a faction without Foddering; see "The FODDER account can be real and unbankable at once". **STATELESS IS THE INVARIANT**: the one impurity, faction knowledge, is threaded in as a `knowledge` PARAMETER (`TopBarReadouts.faction_tracks(faction)`, the whole `{track: progress}` row `faction_knowledge` reads one key out of), never reached for. `next_rung_ready` requires all three of OFFERED (husbandry ceiling / `can_cultivate`-`can_sow` + willing ground), UNGATED (the gate functions answer nothing), and NOT-ALREADY-RUNNING (a patch mid-Cultivate is progress, not an opportunity), **highest rung first**. **That ordering is load-bearing on the PLANT web only** and its assertion needed care: `is_cultivated` retires Cultivate, so on a TENDED patch the two rungs are mutually exclusive and an ordering test there passes with the branches swapped (measured). `Sow` needs no prior patch, so a WILD patch on sowable ground is the one shape that clears both gates at once. On the animal web the rungs are always mutually exclusive — Tame retires at a full meter, Corral requires one — so ordering is genuinely not load-bearing there. `TopBarReadouts.faction_knowledge` deliberately does NOT call `RungGates.track`: dependency DIRECTION outranks the one-definition rule for a `float(d.get(k, 0.0))` |
| `ui/hud/HarvestFloorChart.gd` | The compose sheet's **floor instrument** (`docs/plan_harvest_floor.md` §7.3) — a custom-drawn `Control` (the `FoodOutlookChart` / `ArrivalStrip` idiom) putting the standing stock, the draggable floor line, the projection and the food peak on ONE y-axis of `B/K`, with the `learn_multiplier` gradient rail down the right edge. **IT DRAWS; IT DOES NOT MODEL** — every number comes from `SourceForecast.floor_chart_model`, the projection walks the sim's own `regrowthSamples`, the peak is the argmax of those samples rather than `FLOOR_FOOD_PEAK` restated beside them, and negative samples are carried through as decline. It emits ONE signal, `floor_changed(floor, committed)`, and the second argument is the whole contract: a committed change rebuilds the compose controls (which frees this node), a live one must not, or the drag in flight dies with it — see "THE CHART" below. Keyboard-accessible (`FOCUS_ALL`; arrows / Shift-arrows / Home / End), because the floor is the primary control of the panel. Palette through `HudStyle` only — plus `DetailFormat.ecology_tier_color` for the standing-stock band and the **phase zones** behind it (`_draw_phase_zones`, the furthest-back layer: the source's own `collapseFraction` / `stressedFraction` as horizontal Collapsing/Stressed/Thriving bands, so the floor is dragged against the ecology rather than against a remembered number) |
| `ui/hud/KitRoster.gd` | **All-`static`, stateless** shared KIT layer (`docs/plan_denial_raid.md`) — the read over `SubsistenceSection.kits` (`kits_for_job` / `kit_by_id` / `kit_display_name` / `display_name_for_id` / `resolve_selection`), the EFFECTIVE tier a given band gets under a given kit (`unequipped_tier` / `effective_tiers` / `kit_uses` / `condition_of` / `tier_hint`), the honesty test against the estimate tables' own kit ids (`estimates_quoted_kit` / `estimates_apply_to` / `estimates_quoted_note`), and the picker ROW itself (`build_kit_row`). **Its own file because the control appears on FOUR sheets across TWO controllers** — the Band panel's hunting-party and denial forms, the herd drawer's assign-hunters block, the land drawer's assign-foragers block — and a row that has to read identically in four places must have one implementation; the same measurement that produced `SourceForecast` and `HudWidgets`. The ROSTER is snapshot data and lives on `HudBandLaborState` (`kits()` / `default_kit_id(job)`, ingested by `Hud.update_kit_roster` off `Main`'s `kits` + the two default keys), threaded in as a parameter — this layer holds nothing. **Dependency direction: it reads `SourceForecast` / `HudWidgets` / `HudStyle` / the vocab leaves and none of them may read it back** (a `const` cycle between two `class_name`d scripts fails to load the whole client) |
| `ui/hud/SourceForecast.gd` | **All-`static`, stateless** shared forecast/estimate layer (HUD decomposition, phase 2c-2 precursor) — the pure "what will this source give me?" math THREE consumers ask for: the drawer's compose blocks, the Band panel's WORK zone, and its PARTIES zone. Three families: POST-HOC `source_yield_readout` (what a worked source actually produced, incl. the ⚠ overdraw + overstaff/wasted notes) · PRE-COMMIT `forecast_inputs` / `max_useful_workers` / **`source_worker_cap_state`** (the CONFIRMED-row twin of that cap: `(forecast, workers, idle, useful_floor = 0) → {can_add, note}`, beside the ceiling it reads so a worked row and a compose stepper can never gate differently — the trailing floor is what makes that true rather than merely stated, and `herd_crew_floor` is its one definition; the *hold it after* crew is a floor on BOTH twins and therefore lives inside `max_useful_workers`, carried on the forecast as `hold_crew`) / `expected_yield` / `hunt_policy_ceiling` · THE RAID `hunt_trip_forecast` → `hunt_forecast_line_bbcode` / `hunt_trip_returns_empty` / `hunt_empty_refusal` / `hunt_empty_refusal_reason` / `expedition_party_cap` (the SUPPLY side — the band's idle workforce, and NOT `max_expedition_party_size`, which is the LAST RUNG of the estimate tables' sampled party axis rather than a rules cap) / `expedition_engage_crew` / `expedition_useful_cap` (the DEMAND side, untouched) / `expedition_policy_takes` / `style_send_hunt_button` (`style_send_hunt_button` styles a Button off the raid verdict, so it lives WITH the verdict). Plus **THE DENIAL RAID's own layer** (`docs/plan_denial_raid.md`) — `denial_estimate_row` (which, like `hunt_estimate_row`, reads the NEAREST sampled party through the shared `nearest_estimate_party` / `_row_for_nearest_party` pair, and carries `QUOTED_PARTY_KEY` out so `quoted_party_note` can name it) / `denial_forecast` / `denial_verdict` / `denial_turns_phrase` / `denial_verdict_text` / `denial_verdict_bbcode` / `denial_take_bbcode` / `denial_party_needed` / `denial_refusal_reason` / `denial_is_short_handed` / `denial_short_handed_reason` / `style_send_denial_button`, over the `DENIAL_VERDICTS` table — which is a lookup into `denialEstimates` and shares NONE of the raid vocabulary above: denial carries no floor and no delivery ETA, so its readout is a collapse verdict and its Send disables in exactly one case (`denial_is_short_handed` / `denial_short_handed_reason` — the band cannot field the party the herd REQUIRES; a party the player under-sized still launches). The rationale lives in `band-city-panel.md` → "DENIAL is a third MISSION on the parties footer". Plus the shared leaves those need — `format_magnitude`/`format_signed`/`format_yield`/`extractive_take`, `band_tile`/`hex_distance_wrapped`, `herd_display_name`, `is_managed_hunt_source`, and the two one-off leaks into the read-only detail layer, `flora_basket_entries` / `husbandry_ceiling`. **WHY ITS OWN FILE:** the next phase lifts a `DrawerComposeController` out of `Hud.gd`, but this layer is called by the work + parties zones too, so it cannot travel with the drawer; pure injection was measured at **54 Callables** and a `_hud` back-ref would weld an already-pure layer to the god object (and the band-panel extraction would then need a SECOND back-ref to the same place). All three consumers depend on THIS instead. **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache; if a new function needs HUD state, pass it in. The one non-plain-value is the grid-wrap pair (`grid_width`, `wrap_horizontal`), threaded as EXPLICIT PARAMETERS through `hex_distance_wrapped` → `round_trip_travel_turns` → `hunt_trip_forecast` / `expedition_policy_takes` so a stale grid can never be captured; `HudLayer._hex_distance_wrapped` is a one-line pass-through supplying the pair off `_band_labor`, so there is ONE hex implementation (`DrawerComposeController` calls the module directly with the same pair). The **forecast vocabulary constants moved here with the math** (`LABOR_KIND_*` / `LABOR_HUNT_POLICIES` / `DEFAULT_HUNT_POLICY` / `SOURCE_KIND_*` / `FORECAST_*` / `MAX_USEFUL_*` / `HUNT_FORECAST_*` / `SEND_HUNT_*` / `HUSBANDRY_CEILING_*` …) and `HudLayer` **re-exports the still-used ones as aliases** (`const X = SourceForecast.X`, one commented block) rather than redefining them — ONE definition, and every HudLayer call site reads unchanged |

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
`tradePerBiomass` / `fodderPerBiomass`, published identically by `HerdTelemetryState` and
`ForagePatchState`). `SourceForecast.escapement_room` + `forecast_inputs` evaluate

```text
ceiling(floor, account) = max(0, B − floor·K) × <account>PerBiomass
expected(workers, rung) = min(workers × perWorkerYield × <rung>BuildFraction, ceiling(floor))
```

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
the floor watched the verdict move while the food and trade numbers *they were dragging toward* sat
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
Reported from play (`forage_build_dip`): `5 clear it now` · `6 hold it after` two lines above *"7
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
  source is harvesting, not preparing"*); `live_improvement` is that rule, client-side, over the
  `improvement_is_done` test the improvement control ALREADY makes to render DONE instead of RUNNING.
  Before it, the panel said the build was over and priced it as running.
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

`SourceForecast.herd_axis_rates` is the hunt sheet's one resolution of "which component does this
species pay, and at what rate" — and it took `forecast_inputs`' `IMPROVEMENT_NONE` default, so every
take composed from it (`_hunt_take_rate`, `_hunt_delivered_and_waste`, `_hunt_avg_window_turns`) was
priced **undipped** while the worker cap, the chart, both crew targets and the improvement control's
own deal line — which all carry the composed verb — were not. A herd mid-Tame or mid-Corral quoted
~2× (the animal rungs' `yield_fraction_while_building` is **0.5**, not the plant rungs' 0.25) what the
sim would pay, and the sheet contradicted itself inside one card: the deal's *while building* term and
the readout's take are the same quantity and disagreed. `improvement` is now a **required** parameter
there, so no call site can take the identity by omission. (That the two are ONE quantity is also what
later retired the deal line — see "THE DEAL LINE IS DELETED"; the readout is now the only place it is
stated, so the disagreement is no longer expressible.)

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
- **The frame is `herd_build_dip` + `herd_build_dip_none`, judged as a pair** — a Steppe Runner
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
  mins `animals_engaged` into its `carryable`; `floor(min(carry, engaged × fpa) / fpa)` is the same
  arithmetic but divides a product of `fpa` BY `fpa` and can land a whole engagement one animal short
  on a rounding. It also leaves the partial-body branch reading the RAW carry, which is right —
  engagement is never binding there, a party that exists reaching at least one animal.
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
live, because a line on every sheet claims nothing (frames: `forage_build_dip` /
`forage_build_dip_none`, asserted by presence AND by absence through `HudWidgets.CREW_ROW_DIP_META`).
Its wording deliberately avoids `while building` — that was the DEAL LINE's middle term and the phrase
that line was IDENTIFIED by; two labels on one sheet carrying one phrase is how a search for either
finds the other, measured at seven false failures. The deal line is gone (see "THE DEAL LINE IS
DELETED" below) and `ui_preview` now spends that phrase as the needle proving its absence, so the
avoidance is what keeps that needle unambiguous rather than merely tidy.

**The readout is one bounded well with three registers** (`_mount_readout`, `HudStyle.readout_stylebox`):

| register | what it answers | treatment |
|---|---|---|
| **yields** | what this crew brings home at this floor, now and once holding | a header (`PER TURN · NOW → AFTER`) over 15px tabular numbers + 10px uppercase account names (`2.26 → 0.42  FOOD`) |
| **verdict** | which of the crew and the floor is binding | 12px + the severity dot, colour by severity |
| **aside** | the floor's zone hint and its teaching line | 11px `INK_FAINT` under a dashed rule |

**A reading states its unit and NO destination.** All three accounts land in the working band's own
stores — provisions feed the band, fodder feeds the pens it keeps, and #381 moved trade goods
band-local too. A `→ CAMP` tail earned its width only while trade was the odd account out, banked to
the faction-wide stockpile; once every account routed alike it was three identical words on the
readout's widest line, so the suffix is gone rather than made uniformly true.

**THE RENDER-ONLY-WHERE-THE-VECTOR-PAYS RULE SURVIVED THE RESIZE, because the row set is not composed
here.** `SourceForecast.yield_rows` is the STRUCTURAL half of that rule and is now its one definition —
`yield_components`, `picker_products` and `extractive_take_pair` differ only in how they SPELL a
component and all three iterate it. So a cash crop still shows no food row and a wolf shows none
either, and the single surviving zero is still `zero_account`'s. A widget that synthesised a row would
put the false `0.00 food` straight back on the loudest line of the panel.

**THE HUNT WEB'S PER-TURN ROW IS AN ACCOUNT, LIKE EVERY OTHER RATE ON THIS READOUT** — `0.58 → 0.02
TRADE` on a wolf, `1.23 → 0.07  FOOD   0.18 → 0.01  TRADE` on a deer, through
`SourceForecast.yield_rows` and `YIELD_ACCOUNT_UNITS`. **The
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
sentence belong to different functions.** `herd_axis_rates` mirrors the sim's `ratio_axis` and
resolves ONE component (provisions preferred, trade for an inedible quarry) — it has to, because the
whole-animal quantiser divides by a per-animal quantum and a wolf's food quantum is `0`. But that
constrains the **count**, not the credit: a ratio is unit-free, so the sim values one quantised take
in both currencies through `YieldPair::rescaled_to`, and `SourceForecast.rescaled_accounts` is that
crossing client-side — the per-biomass vector as the reference mix, the counted axis coming back
bit-identical. A Wild Boar's local sheet therefore reads `FOOD` **and** `TRADE`, exactly as the same
species raided by an expedition always did (`_trip_yield_rows`), rather than the axis alone.

- **`yield_rows` still decides which rows EXIST**, so this is not "credit every account": a wolf's
  provisions rate is a structural `0`, the crossing answers a structural zero, and no `0.00 FOOD`
  appears. `herd_hunt_pelts_only` is the frame that pins it and `herd_hunt_both_products` its
  positive twin — asserted as a pair, since the negative alone passes on a readout that lost both.
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
travel) · ~1 food · ⇄ ~0 trade goods · ⚠ 20% wasted* — beside a local sheet that laid the same kinds of
fact out in a bounded well, so one panel read two ways. `_mount_trip_readout` composes the same three
registers:

| register | the raid's version |
|---|---|
| **yields** | header `THIS TRIP`, the ANIMAL count leading in the local hunt row's own idiom (its `YIELD_ROW_NUMBER`/`UNIT` overrides, the quarry as the unit, `YIELD_ACCOUNT_NONE` as the account), then food and trade through `SourceForecast.yield_rows`, and the waste on the row's own `waste` slot |
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
preset name over its product line, and a hunt preset carries two accounts (`0.67 food · 0.10 trade`)
where a staple forage preset carries one. That is the content's honest demand at those faces, so the
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
card's chrome from `_header` — the title `RichTextLabel` — where the header ROW is that title beside
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
  fix. `yield_rows` additionally drops it where the two are equal: an arrow to itself is noise.
- **A managed rung-3 source has no burst** — the sim never draws a Field or built Pen down, so its
  `hold_ceiling` IS its ceiling and one reading renders.
- **The unit is hoisted into the header.** Three `/TURN`s were the widest thing on the row and it
  could not afford them once each account stated two numbers. Hoisted, not deleted: a preset's tooltip
  states `up to +0.60/turn` for the ROOM, so something has to mark which kind of number this is. The
  header doubles as the arrow's key, in the crew buttons' own two words.

**THE PRESET FACES STATE NO NUMBER**, only the intent (`♻ Best harvest`); the metric kept its tooltip.
Nine numbers stood across the top of the sheet and every one misled: they are the ROOM (one-off) over a
row of per-turn rates; they rank the presets BACKWARDS from the decision (`Take everything` reads twice
`Best harvest` while paying ~nothing forever); they are in food/trade/fodder units directly above a
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
quotient's crew achieves. Frame + assertion: `forage_build_dip` (`7 clear it now` under `max 7 workers
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
`herd_hunt_pelts_only` — three presets reading `2.03 / 0.90 / 0.22 trade`, no food line, no zeros.

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
- Frames + sabotage-verified assertions: `forage_build_dip` (rises → no flag) and
  `forage_build_dip_decline` (one more hand, falls → the flag returns). **Both halves, or the first
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

**A SIXTH CASE — `forage_build_dip` / `_decline` / `_none`, WHERE THE REGROWTH BEATS THE ROOM.** The
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

### THE SHEET'S NUMBERS ARE REPRICED AT THE CHOSEN KIT, through ONE seam

`KitRoster.repriced_source` hands the ordinary forecast a COPY of the wire's own terms with two
substitutions, so every consumer downstream — the take, the waste, the crew targets, the chart —
picks the kit up without knowing it exists. `DrawerComposeController._kit_priced_source` is the only
caller, and `_hunt_priced_herd` / `_forage_priced_patch` are the only two doors onto it.

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
`clamp(1 − (1 − stayFraction) × dispersion, 0, 1)`. Folding it into the reach instead reprices the take
and the CREW COUNT together, and **the sim does not treat them together**: `fauna::hunt_engage_workers`
sizes a crew on the RAW reach — the hands that can get to the herd — while `HuntParty::stayers` cuts
only what those hands bring down. The fold shipped once and `ui_preview`'s *"the compose stepper caps at
the crew the SIM asks for"* caught it immediately.

`SourceForecast.animals_stayed` is the client mirror of `animals_that_stay` at the quantile a forecast
reads it at (the analytic mean `floor(engaged) × stay`; `animals_engaged` already floors). It is
applied in the sim's own order — **engage → retreat → convert** — and **only where a TAKE is composed**:

| carries the retreat | does NOT |
|---|---|
| `engaged_quantum` → `engagement_reach` → `expected_yield_account` | `engage_workers` / `take_workers` |
| `_hunt_delivered_and_waste`'s `carryable` bound | `engagement_carry`, hence `crew_to_clear` / `crew_to_hold` / `crew_that_reaches` |
| — | `max_useful_workers`' `hold_crew` / `reach_crew` floors |

**Both take producers, because only one of them is `expected_yield`** — the same reason the reach arm
needed both. `_hunt_delivered_and_waste` composes its own `collection` so it can quantise, so an arm
added to the shared layer alone leaves the *rendered* green line unmoved while the cap beside it shifts.

**A source with no retreat stage is byte-identical to before the stage existed.** A patch and a pen
publish no `stayFraction`, `repriced_source` finds no key to substitute, and `forecast_inputs` reads the
wire's own `1` — which `animals_stayed` short-circuits on, so an unbounded engagement passes straight
through. Measured: with the whole substitution live, **zero of `ui_preview`'s 590 assertions move**, no
fixture in that harness publishing the field.

**KNOWN GAP — the DOCK's hunt and denial forms reprice nothing.** `_kit_priced_source` is
`DrawerComposeController`'s alone, so `BandPanelController`'s raid sheets build their floor chart on the
RAW herd. Most of what those forms show is estimate-table lookup, which is quoted at the hunt job's
default kit BY DESIGN and says so (see "THE HONESTY RULE" below) — but the chart is composed
client-side from the herd's own terms and could be repriced honestly. It is left alone because its crew
stepper caps on `expedition_useful_cap`, which IS table-derived: pricing the pills without the stepper
would put the two at odds, which is the defect this section exists to remove. **Measured consequence:
the repricing fix moved 83 of `ui_preview`'s 343 frames and ZERO of `band_panel_preview`'s 74.**

**KNOWN GAP — the chart's PROJECTION is still pre-retreat.** `project_stock`'s engagement bound
(`floor_chart_model`, `take_draws_down`, `crew_that_reaches`' probe walks) reads `engaged_quantum`
without the stay term, so the drawn curve, the settle verdict and the ⚠ overdraw gate all walk a take
larger than the one the readout quotes. It is left whole rather than half-closed **because
`crew_that_reaches` feeds `max_useful_workers`' `reach_crew` floor**, i.e. the stepper cap — retreat-
bounding it would put the sheet back at odds with the sim's `workersNeeded`, which is the regression
this whole section exists to prevent. Closing it means separating that crew answer from the walk it is
derived through. It is the same family as the two gaps recorded below (the local per-turn readout and
the raid sheet's chart are both blind to the FIGHT), and it closes the same way.

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
in. `kit_uses` (tier beats the roster minimum) answers a DISPLAY question only — whether the hint
quotes that component's condition — never a number: `none` spends no durability, so printing
`spears 74` beside it would describe wear it will never cause.

**`stated` is false when the band says nothing about its condition at all** — the key absent, not
zero, `0` being a real reading meaning DRY. The fresh tiers then stand and no condition clause prints,
the "absent terms render no line" convention `hunt_gate_model` already takes.

### THE HONESTY RULE — the estimate tables are quoted for ONE kit

`huntTripEstimates` and `denialEstimates` are computed at the hunt job's **default** kit only, on
every herd; repricing them per kit is scoped out (they are ~95% of snapshot capture). So when the
selected kit differs from `hunt_trip_estimates_kit_id` / `denial_estimates_kit_id`, the sheet **must
not present the table as the answer**. **Compare the ids — never assume the default is selected.**

What the mismatch branch renders instead:

- the **combat gate** (`SourceForecast.hunt_gate_model_at`), composed from wire terms —
  `max(0, attack − defense)` against the species' durability — at the SELECTED kit's effective attack
  rather than at the band's default-kit tier. It is the ONE forecast that stays honest for every kit,
  and for 15 of 20 roster species a bare-handed party's effective attack is 0, so the line says so
  plainly. `hunt_gate_model` is exactly this asked at the band's own tier, so the two can never
  disagree about what a gate is — only about whose attack it is.
- **a sentence naming the kit the withheld numbers belonged to**
  (`KIT_DENIAL_ESTIMATES_QUOTED_FORMAT` / `KIT_TRIP_ESTIMATES_QUOTED_FORMAT`), so "why is there no
  turn count?" is answered on the sheet rather than inferred from an absence.

**Everything derived from the table is suppressed, not merely the headline** — and each of these is a
figure computed for a raid the player is not sending:

| suppressed | why it is table-derived |
|---|---|
| the collapse verdict / the trip readout | the row lookup itself |
| the estimate caveat | it qualifies a turn count that is not being shown |
| the take line / the payload rows | the row's `animals_killed` / `delivered_*` |
| the repelled refusal, the short-handed disable | `denialPartyNeeded`, derived from those rows |
| the trip's bound clause, the empty-raid refusal | the row's `bound` |
| the floor picker's per-preset metrics | `expedition_policy_takes` is a reading of the same table |
| the DEMAND-side party cap (`expedition_useful_cap`) | the payload's plateau is unknown for this kit |

**The send stays LIVE and plainly styled.** The raid is perfectly launchable; only its length is
unquotable. Disabling it would read as the kit being illegal, which it is not — and the launch guard
that re-checks `hunt_trip_returns_empty` is skipped for the same reason, refusing a launch on another
kit's projection being the same lie as quoting one, cast as a silent no-op.

**A LOCAL hunt has no honesty gate to have**: it is priced from the herd's own per-biomass vector and
the band's ceilings, with no estimate table in it. Neither has the forage sheet.

### The command carries `kit <id>`, and OMITS it at the job default

`Main._kit_token` is the one builder, appended by all four grammars (`assign_labor forage` /
`assign_labor hunt` / `send_hunt_expedition` / `send_denial_raid`). It is a **named, space-separated,
order-independent** tail pair, the parser's existing `name value` style, lifted out of the tail before
any positional form is read — so it may sit anywhere after the role and no grammar has to make room
for it. On the denial raid it is the ONE thing the closed four-token grammar admits, because a kit is
a property of the PARTY rather than of the mission.

**It is omitted when the choice equals the job default**, which is also what absent means to the
parser — so a composition that never touched the picker emits the byte-identical line it emitted
before the picker existed. Both the choice and the default therefore
ride the payload: the builder cannot know the default on its own (it is world data), and
`HudLayer._emit_assign_labor` supplies it from `_band_labor.default_kit_id(kind)`.

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

1. **Offered** — an unchecked `CheckBox` naming the next rung and its terms
   (`🌱 Cultivate this patch · then 1.20 food`). A rung you can actually take.
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
2. **Running** — checked and **LIVE**, carrying the build meter AND the rung's payoff
   (`🌱 Cultivating — 60% · then 1.20 food`, the offered box's own `· then` grammar). Unchecking
   it abandons the build (`abandon_improvement`, below).
3. **Done** — a static `Label` naming the state (`🌾 Tended Patch`), with the NEXT rung's checkbox
   beneath it when there is one.

**ONLY ONE improvement is ever offered — the source's next rung.** `RungGates.next_rung_offered` is
that answer and shares its ordering with `next_rung_ready` through the private `_next_rung`: highest
ungated rung first (so sowable wild ground, where both plant rungs clear, offers **Sow**), falling
back to the LOWEST admitted-but-gated rung when none is ready — the nearest thing you could work
toward. The two answers differ on **the gate alone**, and that difference is the difference between a
MARK (promises the verb is available, so a gated rung must not wear one) and a CONTROL (is how the
player discovers the rung exists).

### THE DEAL LINE IS DELETED — its payoff rides the running control's face

The improvement forecast used to render a line of its own beneath the box:

```
0.61 food · 1.25 trade → 0.15 food · 0.31 trade while building → 1.39 food · 0.38 trade
        today                    preparing (WARN-amber)                  payoff
```

**Two of its three terms were already on the sheet.** The MIDDLE term is byte-identical to the
readout's own `PER TURN` headline — the same crew through the same dipped forecast, which is why the
harness assertion pairing them never once disagreed — and the FIRST is the price of building, which
the crew row states qualitatively as a factor (*"building this rung, each carries 50% as much"* on
shipped values). Only
the payoff was unique to it, and the OFFERED state already puts the payoff on the checkbox face. So
the payoff moved to the RUNNING face in the offer's own grammar and the line went:

| state | face |
|---|---|
| offered | `🌱 Cultivate this patch · then 1.39 food` (`IMPROVEMENT_OFFER_FORMAT`) |
| running | `🌱 Cultivating — 40% · then 1.39 food` (`IMPROVEMENT_RUNNING_FORMAT`) |
| running, feed rung | `🐄 Building the pen — 0% · then 0.00 food − 0.14 feed` (`…_FEED_FORMAT`) |
| running, no quoted deal | `🌱 Cultivating — 40%` (`…_BARE_FORMAT`) — never a fabricated `· then +0.00` |

- **The payoff is composed the way the deal composed it**, from `improvement_forecast` at the
  composed floor: the caller's `payoff_face` Callable where there is one (the plant web substitutes
  the CROP, so the box that offers a rung and the box running it can never quote different crops),
  else `picker_products` over the payoff vector × the band's `output_multiplier`.
- **`IMPROVEMENT_DEAL_DEPLETED_NOTE` OUTLIVED THE LINE, deliberately.** A pen whose payoff is 0.00
  under a running feed is a pure loss and that is the most valuable thing this control can say; it
  rides `build_improvement_control`'s note slot now — the same WARN-inked slot the paused-build line
  uses — beside the zero on the face, which still renders in full. Frame + assertion:
  `herd_corral_depleted`.
- **The UNSTAFFED variants died BY DESIGN.** They existed because the today/dip terms are
  staffing-scaled while the payoff is not, so a zero crew read as a sequence it was not on track for.
  With only the payoff left there is no sequence to misread.
- **Assert the pair, never the absence alone** — "the line is gone" also passes on a sheet that lost
  the payoff with it. `forage_unstaffed`, `improvement_running_plant` and `improvement_running_animal`
  each pin the absence (by the `while building` needle) AND the payoff on the face (by
  `IMPROVEMENT_CONTROL_META`), both webs, sabotage-verified in both directions.

The dip itself is unchanged: `build_forecast` is the crew's own forecast with its throughput dipped
per account, and it is what the readout's `PER TURN` row has quoted since §3.1.

**A non-Sustain stance beside a running build is LEGAL and is not an error state.** It defeats itself
through the ecology, not through a gate: the build meter accrues only while the source is Thriving,
and Deplete is what drives it out. The dip rides the LARGER ceiling, so a Deplete builder takes more
now and stalls their own meter. `ui_preview`'s `improvement_deplete_while_building` is that frame.

**The pause line is both webs' now.** A build accrues only while its source is Thriving, and that is
deliberately not a gate (a source's phase swings as it is worked). The sim PAUSES, losing nothing, and
`_improvement_paused_note` states the pause, its cause and the ease-off remedy on the Running control.
It was `_tame_stalled_hint`, animal-only, because the plant web had no control to hang it on.

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

### Unchecking: `abandon_improvement`

**`abandon_improvement <faction> forage <x> <y>` / `abandon_improvement <faction> hunt <herd_id>`**
(alias `abandon`) is what a live Running box sends. It exists because the split otherwise removed a
capability the old model had by accident: when the build verb WAS the policy, picking another policy
always walked a 25-turn commitment away.

**It names a SOURCE, not a verb** — at most one improvement is ever in flight on a source — so it is
targeted by the **web** (`forage` → tile, `hunt` → herd). The SET verbs are targeted by the **verb**
(`tame` names a herd; `cultivate`/`sow`/`corral` name a tile), and **`corral` is the case that proves
the two rules genuinely differ**: a herd's rung addressed by the pen's place. `Main` therefore keeps
`format_improvement` and `format_abandon_improvement` as separate builders, dispatched on whether the
payload's `improvement` is empty — `""` being the wire's own spelling of "building nothing", so the
compose state, the payload and the branch all read one value instead of a parallel flag.

**UNCHECKING IS ALWAYS LEGAL, and nothing may gate it.** No knowledge, no ceiling, no site, no
`Thriving` check — abandoning a *stalled* build is the case it exists for, so gating it on the
conditions that STARTED the build would make the remedy unreachable exactly when it is wanted. Hence
`build_improvement_control` disables **only** an OFFERED box, and only on unmet prerequisites; a
RUNNING box stays live however loudly its pause note reads. A condition that greys a running box is a
bug, not a safeguard.

**It is NOT a cancel-and-refund, and the two webs differ** — verified sim-side, not assumed. The
command does not touch the meter at all; it hands the source back to the rule that already governs an
unimproved one, which is the same state walking the band away reaches:

- **plant** — `cultivation_progress` / `field_progress` BLEED at `decay_per_turn = 0.01` (~100 turns
  to zero) once nobody is improving the patch;
- **animal** — `domestication` / `corral_progress` are KEPT (the animal branch never decays).

So the copy must not promise progress back on either web, and must not imply the plant meter is safe.
`IMPROVEMENT_ABANDON_HINTS` carries one honest line per web on the running control's **tooltip**,
beside the rung's own hint. **Deliberately no confirm dialog**: unchecking is always legal, fully
reversible on the animal web and slow-decaying on the plant one, so a modal would be ceremony over a
decision the player can simply re-make — and the "End it" confirm that used to guard a policy-pick
discard is precisely what this axis split removed.

### What remains SERVER-SIDE

- **No build RATE on the wire.** `progress_per_turn` is sim-side config, so the control cannot say
  `~25 turns` / `~10 turns left`; it states the rung, its terms and its meter percent instead.

---

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
    the worker stepper's cap is that band's `_assignable_hunt_workers` / `_assignable_forage_workers`
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
    standing stock in one haul, the biggest payoff of any rung, in whatever the species pays (meat, ⇄
    trade goods, or both), and the herd gone for good — denial is the END STATE, not a promise that the
    carcasses were thrown away (#337)). **They no longer teach the LADDER** — see "The two compose sheets
    read in ONE grammar" above. **These are
    NOT the expedition hints** (`SEND_HUNT_POLICY_HINTS`): an expedition's Hunting arm banks **both
    products** since #337 (one `HuntYield::apply` per kill — provisions into the party's larder, trade
    goods onto `Expedition::carried_trade` and into the HOME BAND's `stores` at the drop-off/fold-back —
    the faction stockpile until issue #381 moved trade goods band-local), but
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
    per-policy **COMPACT** button metric the local-hunt picker does, over **every account the patch
    pays** since #426 (`[♻ Sustain / 0.60 food · 0.01 trade · 0.20 fodder]` on a hay meadow).
    **Each button is TWO LINES, ONE PER
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
    `X food[ · Y trade][ · Z fodder]` (extractive, `up to X/turn` in the tooltip via
    `POLICY_CAP_FORMAT` / `SourceForecast.extractive_take_pair`) or over
    `→ X food[ · Y trade][ · Z fodder]` (investment, Cultivate/Sow AND Tame/Corral). **Trade reaches
    all four rungs and fodder the two PLANT ones** — the old "trade only ever appears on the two HERD
    rungs" was true of a wire that no longer exists (#426); fodder's herd-side absence is structural,
    no animal being harvested for feed. **The expedition picker no longer shows raid animals** (`≈N` / `EXPEDITION_TAKE_COMPACT`
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
      * `Cultivate` ← `cultivation >= 1` **and** a Thriving patch **and the rung not already built —
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
      reads as ASCENDING. **THE ASCENT IS PER ACCOUNT, AND ONLY THE FOOD AND FODDER COLUMNS HOLD IT.**
      `Deplete` alone carries `market.trade_goods_multiplier` (×4) — a POLICY markup on stripping a
      source for sale — so its trade cell can sit ABOVE Eradicate's while the food cells still ascend
      (measured on a live patch: Deplete 3.24 against Eradicate 1.21). A reader comparing the trade
      column expecting a ladder is reading the wrong invariant, and a fixture that quietly sorted it
      would misrepresent what the player sees; `ui_preview forage_three_accounts` ships the
      non-monotone trade column deliberately. `wasted_yield > 0` renders a muted "· N.N wasted" understaffing note (the low-key
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
      `forage_cultivate_stressed` (1 reason — the ease-off-and-regrow ecology remedy) / `herd_corral`
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

## A source pays a VECTOR OF ACCOUNTS — the render-only-when-non-zero rule (issues #337 / #426, `docs/plan_hunt_yield_model.md`)

A yield routes by ACCOUNT: provisions, trade goods, fodder. A hunt's is the species' own `HuntYield`
times the policy's intensity; a patch's is its per-policy row. The client read only the food account,
so an **inedible** species — a wolf, which pays pelts and no meat — rendered `+0.00` on every rung and
looked like a source worth nothing. **#426 found the same lie on the plant side**, one account further
out: a flax patch and a hay meadow rendered `0.00 food` on every rung for the same reason.

**THE ONE RULE, and it is applied at every surface: render a component only when it is non-zero.**

| source | reads |
|---|---|
| deer (`provisions > 0`, `trade > 0`) | food **and** trade, **food leading** |
| wolf (`provisions == 0`, `trade > 0`) | trade only — **never** a "0 food" line |
| a staple patch | food only — **never** a "0 trade" line |
| flax (a cash crop) | trade only |
| a hay meadow | food · trade · **fodder** — all three, in wire order |

A `0` printed as a number for a component the source does not produce is the false precision this
whole arc exists to remove; it is not "more complete", it is wrong. The one place a zero survives is a
component the source genuinely HAS and did not pay this turn (a worked row's `+0.00 /turn`, a rung
whose ceiling exists and is empty).

**THE ACCOUNT ORDER IS THE WIRE'S, NOT A RANKING** — provisions, trade goods, fodder — so a source
paying two of the three reads the same left-to-right whichever two they are, and the eye finds an
account by position rather than by re-reading the words.

**FODDER IS PLANT-ONLY, and that asymmetry is structural rather than unfinished work.** Fodder is feed
grown for penned animals; `fauna_config::YieldAccounts` fills a structural zero there for every
species, so no herd rung has a fodder term and none ever will. Do not "complete" the herd side by
appending one — there is nothing on the sim side to put in it.

**Trade is stated GENERICALLY** — `FoodIcons.TRADE_GOODS_GLYPH` (`⇄`) plus the words "trade goods". The
sim models a **scalar**, so the client says so: there is deliberately **no per-species noun** (pelt /
ivory / hide). A named good per species is a flavor layer on top of the scalar, explicitly deferred by
the design doc, and inventing one here would put words on the wire's behalf the sim cannot back.

### The shared layer (`SourceForecast`)

- `has_component(rate)` — the single "is this component present?" gate, so every account is judged
  identically everywhere. **Its floor is the DISPLAY's, not the model's** (`COMPONENT_RENDER_MIN`,
  half of the smallest quantity `YIELD_DECIMALS` can show). It read `>= FOOD_FLOW_MIN` (0.001) until
  #426 — the *food-flow* floor, which is a claim about the SIM — while every caller renders at two
  decimals, so a rate in between PASSED the gate and then printed as `0.00`: a single forager on a
  staple patch earns ~0.003 trade goods a turn, and the preview line read `+0.08 /turn · ⇄ +0.00 ·
  0.13 fodder`. **A gate finer than its formatter's resolution admits the very thing it exists to
  stop.** `FOOD_FLOW_MIN` keeps its own separate job — whether the BAND has a food flow at all is a
  question about the sim, not about how many decimals a label shows.
- `format_trade(v)` → `⇄ +0.35`; **`yield_components(food, trade, fodder = 0)`** → `+0.31 /turn · ⇄
  +0.12`, `+0.08 /turn · 0.13 fodder` — the ONE joiner every per-turn readout goes through, so no two
  surfaces can word the vector differently. The fodder term wears the WORD (fodder has no glyph);
  every hunt-side caller leaves it defaulted and reads exactly as before.
- **`magnitude_components(food, trade)`** → `0.20 ⇄ 0.22` — its COMPACT twin for a surface that
  supplies its own framing and states levels rather than deltas (the work zone's filter chips). Same
  rule, same food-leads order, bare magnitudes joined by `COMPACT_COMPONENT_SEPARATOR` (a space, since
  those chips already spend their `·` separating a count from its total).
- **`extractive_take_pair(food, trade, fodder = 0)`** — the rung metric `{compact, full}` for ALL
  THREE pickers. The food-only `extractive_take` the forage picker used is **deleted**, not kept as an
  alias: its justification ("the plant web projects no trade rate") described a wire that no longer
  exists, and one joiner is what keeps the three pickers wearing one face.
- **The rule reaches the INVESTMENT payoffs too**, not just the extractive caps.
  `SourceForecast.FORECAST_PAYOFF_TRADE_KEYS` now spans **all four rungs, both webs** (`corral` →
  `corral_trade`, `tame` → `pastoral_trade`, `cultivate` → `tended_trade`, `sow` → `field_trade`), and
  `FORECAST_PAYOFF_FODDER_KEYS` covers the **plant pair only** (`tended_fodder` / `field_fodder`).
  Together they give `forecast_inputs` a **`payoff_trade`** and **`payoff_fodder`** beside `payoff`,
  and `DrawerComposeController._payoff_take(payoff, payoff_trade, payoff_fodder = 0)` builds the same
  shape `extractive_take_pair` does: `picker_products` on the face,
  the `POLICY_PAYOFF_FULL_FORMAT` food clause joined to the shared `POLICY_CAP_TRADE_FORMAT` trade
  clause in the tooltip, each half only when `has_component`. `POLICY_CAP_TRADE_FORMAT` is bare
  wording with no "up to" in it, which is why it serves both the cap and the payoff despite its name.
  Both `_hunt_policy_takes` and `_forage_policy_takes` emit a face when ANY account is non-zero, so a
  trade-only or fodder-only payoff still gets one rather than falling back to a bare glyph + name.
  **A resolved crop substitutes all three of its own together** — `_flora_entry_payoff` /
  `_flora_entry_trade_payoff` / `_flora_entry_fodder_payoff` in one branch — so a face can never mix
  one crop's food with another's fodder; with no crop resolved the patch's species-BLIND quotes stand,
  which is the right answer for a COMMITTED patch rather than a fallback.
- **`picker_products(food, trade, fodder = 0)`** → `0.60 food · 0.01 trade · 0.20 fodder` — the same
  rule and the same food-leads order **in WORDS and without the sign**, for the ONE surface that has room to name its products: the
  policy picker's two-line rung face (`compact` above is written in terms of it). The picker names
  rather than marks because its line 1 already carries a glyph naming the RUNG, and two glyph families
  in one line at one weight is the axis collision this treatment removed — see the picker notes above
  and `sprites-widgets.md`. **The word is the ACCOUNT's, not the commodity's**: this line says `fodder`
  while the crop-basket rows below it say `hay` (`HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT`),
  because a basket row names one PLANT and what that plant pays, and hay is what hay grass pays.
  **A three-account face did NOT change the picker's column ceiling, and that is a measurement rather
  than an assumption**: a wide-face ceiling of 2 was written for exactly this face and the rendered
  frame refuted it — three abreast comes out ~555px against the deer hunt picker's long-standing ~546,
  and 3 + 3 reads better than 2 + 2 + 2. **"Nothing clips" was the weak half of that measurement and
  has since been made real**: those widths were never compared against the compose card, which was
  pinned at a nominal 340 and let its inner `PanelContainer` grow out of it, so a frame looked right
  at any picker width. The card now fits its content and `_assert_compose_sheet_fits` compares the two
  — so re-adding a column ceiling needs a row that overruns the card, and the harness can now say
  whether one does.- **`trade_rate_of(source)`** — the ONE per-source trade rate, and the owner of the
  `realized_trade_yield` → `trade_yield` fallback. **The sentinel is the VALUE `0`, not an absent
  key**: forage's projection is the documented `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` zero, and the
  decoder inserts the key unconditionally — so the `has("realized_trade_yield")` spelling both readers
  used to carry never once fired, and every forage source's trade read as nothing (a work row with no
  `⇄`, a band Trade row at `+0.00`). `source_yield_readout` and `DetailFormat.sum_realized_trade` both
  call it, which is what keeps a per-source row and the band's headline in agreement by construction.
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
the band's `stores[trade_goods]` and never its larder — so a trade-only hunt must not move the Food
line. (That credit was the FACTION stockpile until issue #381; the account is still separate from the
larder, which is the whole point.) That is
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
averaging-window disclaimer — in the rung tooltips — still derive off the TRADE quantum) · **`herd_hunt_both_products`** (the
same picker on a deer: `2.33 food · 0.34 trade`, food leading) · **`herd_investment_both_products`**
(its INVESTMENT twin — a Wild Boar whose Tame reads `→ 1.48 food · 0.37 trade` and whose greyed Corral
reads `→ 2.95 food · 0.74 trade`, asserted as literal strings against the rungs found by
`HudWidgets.POLICY_RUNG_META`; `herd_tame`'s food-only deer is the companion showing no `0 trade` is
printed) · **`herd_hunt_pelts_raid`** (the wolf as an
expedition target: `delivers ≈3 Grey Wolf over ≈9 turns · ⇄ ~4 trade goods`, primary Send — NOT a
denial) · `herd_hunt_eradicate` (an Eradicate boar raid now delivers `~40 food · ⇄ ~5 trade goods`) ·
`hunt_picker_ascending` (the drawer's standing summary `+0.84 /turn · ⇄ +0.12`) · `food_tile` (the
forage control — food only, no "0 trade") · **`forage_three_accounts`** (the PLANT frame the rule is
judged on since #426 — a hay meadow whose rungs read `0.60 food · 0.01 trade · 0.20 fodder`, and the
frame the picker's three-column ceiling was MEASURED against rather than assumed) ·
**`forage_three_accounts_overdraw`** (the same meadow, Eradicate, three foragers — the fodder account
ALONE carries the warning while the food take sits inside the patch's own regrowth; the crew size is
load-bearing, since one forager overdraws nothing and would pass the claim vacuously) ·
**`forage_dead_season`** (a patch the wire fully DESCRIBES whose every cell is zero — the rungs still
render, they state `0.00 food`, the preview line still speaks and the worker cap stays live at
`MAX_USEFUL_BARREN`. Not `tile_panel_no_forage`, which has no food module and hence correctly no
compose block at all; the difference between "pays nothing this season" and "the wire never described
this patch" is the whole of issue #426). `band_panel_preview`:
**`band_panel_work_trade_rows`** / **`band_panel_work_trade_inspector`** (a food row, a food+trade row
and a trade-only wolf row on one board; the inspector sentence reads `⇄ +0.22 · Deplete · Working`) ·
**`band_panel_work_trade_totals`** (the aggregates — the same band with the deer unassigned, so its
sole hunt pays trade: header `2 sources +0.15 /turn ⇄ +0.22`, chip `🦌 1 · ⇄ 0.22` with the food term
suppressed).
`map_preview`: `map_band_work` (the hunted wolf labels `⇄+0.22 ⇊` beside the deer's `+0.20`).

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
AND still quotes the food + trade ceilings it can bank (`HAY_PEAK_TOOLTIP_FODDER_LOCKED`), while the
known and committed frames quote all three (`HAY_PEAK_TOOLTIP`). Sabotage-verified three ways, each
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
arithmetic is unchanged — `delivered_food <= 0 and delivered_trade <= 0`, still the one blocked case,
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
