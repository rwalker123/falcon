---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{TargetingController,hud_expedition_vocab}.gd"
  - "clients/godot_thin_client/src/scripts/ui/AnnotationRenderer.gd"
---

<!-- Extracted verbatim from lines 181-181;1606-1611;3291-3474 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Command targeting — move-band and expeditions

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/TargetingController.gd` | `RefCounted` controller (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the **COMMAND-TARGETING** cluster — the three remaining targeting flows (**move-band** picks a destination TILE, **send-expedition** outfits a party then picks a TILE, **pick-quarry** is the parties compose sheet's HERD picker) plus the floating top-centre **targeting banner** that guides each. It holds the three pending dicts (`_pending_move_band` / `_pending_send_expedition` / `_pending_pick_quarry`), the banner (`_ensure_targeting_banner` / `_refresh_targeting` / `_current_targeting_info` / `_targeting_banner_bbcode`), the per-flow begin/cancel/dispatch functions (`_try_dispatch_pending_move_band` / `_try_dispatch_pending_send_expedition` / `_try_pick_quarry` / `_huntable_herd_on_tile`) and the wrap-aware `_hex_distance_wrapped`. **Public API:** `begin_move_band` / `begin_send_expedition` / `begin_pick_quarry` / `cancel_pick_quarry` / `is_expedition_quarry` (THE single quarry-eligibility definition — the pick, the sheet's re-validation and MapView's glow all route through it), plus `is_targeting_active` / `cancel_active_targeting` / `try_dispatch` (the last runs the three `_try_*` in the SAME order as before). Hud holds it as `_targeting`, constructed in `_ready` **AFTER `_drawercompose` and BEFORE `_bandpanel`** (which injects it — so `_targeting` must exist first). **It emits its OWN signals, HudLayer RELAYS each** (the `TurnOrbController` pattern; the controller never emits a HudLayer signal): `targeting_changed` (→ `MapView.set_targeting`) · `move_band_requested` · `send_expedition_requested`. **The three reflective delegators STAY on HudLayer** — `is_targeting_active` (Main's escape_claimant path) / `cancel_active_targeting` (Main relays MapView's `targeting_cancel_requested` by name) / `try_dispatch` (called from `show_tile_selection` / `notify_hex_selected`), each probed BY NAME so a `has_method` miss fails SILENTLY. **The injection surface is TWO Callables** — `_resolve_assign_band` (STAYS on HudLayer, DrawerComposeController injects it too; reached through a typed adapter since `Callable.call` returns `Variant`) and `_after_pending_change` (STAYS on HudLayer, the `_emit_assign_labor` pending path owns it) — **plus, as construction-order and gap fixes not in the original decomposition spec: `_compose` (the parties compose's quarry/autofill one-shots, needed by the pick) and a lazily-bound `_bandpanel.rerender()` Callable (`_bandpanel` is built AFTER `_targeting`, so a direct ref is impossible at construction)**. Collaborators: `_band_labor` (`record_pending_move` + the grid pair), `_drawercompose` (the three `close_compose_sheet()` nudges), `_command_feed` (the quarry-pick miss/refusal `note()`s), and the HUD CanvasLayer as the **host** it parents the banner into — via the host's `LayoutRoot` (NOT the bare CanvasLayer) so the banner keeps insetting with the reserved-edge docks exactly as before. Behaviour identical to the old inlined targeting code |
## Command Targeting

Labor allocation is source-centric (assign workers to a source/role, see the **Labor
allocation UI** bullet below). The one remaining **targeting mode** is **move-band** —
picking a destination tile — replacing the old easy-to-miss "select a band…" line.

- **Targeting: move-band + send-expedition + send-hunt-expedition** (`ui/hud/TargetingController.gd`,
  held as `_targeting` — see its Key Scripts row; the whole cluster left `Hud.gd` in a decomposition
  pass): the single-task forage/scout/hunt/follow `_pending_*` flows were retired with labor
  allocation. Three targeting flows remain, all built on the same `_pending_*` →
  `_current_targeting_info()` → `_refresh_targeting()` machinery ON THE CONTROLLER: `_pending_move_band`
  (`command: "move"`, `need: "tile"`), `_pending_send_expedition` (`command: "expedition"`, `need:
  "tile"`, carries the outfitted band + party size), and `_pending_pick_quarry` (`command: "quarry"`,
  `need: "herd"`, plus **`min_distance`** = the band's `hunt_reach` — the party compose sheet's quarry
  PICKER: it carries only the band, dispatches nothing, and returns the clicked herd to the sheet).
  `_current_targeting_info()` returns a descriptor (`{active, command, need, origin_x/y,
  context_label}`) for whichever is set; `_refresh_targeting()` shows the floating **targeting
  banner** (top-centre, `HudStyle.banner_stylebox()`: cyan reticle + command + instruction + Cancel)
  and emits the controller's `targeting_changed(info)` (relayed onto the HudLayer signal). HudLayer's
  `show_tile_selection` + `notify_hex_selected` call `_targeting.try_dispatch(tile_info)`, which runs
  all three pending flows on the click (the tile click carries `tile_info.herds`, which the hunt flow
  resolves its target from).
- **Main forwards** `hud.targeting_changed → map_view.set_targeting` and
  `map_view.targeting_cancel_requested → hud.cancel_active_targeting` (a HudLayer delegator →
  `_targeting.cancel_active_targeting`).
- **MapView draws** the overlay (`AnnotationRenderer.draw_targeting`, reached through MapView's `set_targeting` pass-through): `need == "tile"` draws a reticle on the
  hovered hex (the `need == "band"` path is now unused). Esc / right-click during targeting emit
  `targeting_cancel_requested` instead of panning; the pulse is animated from `_process`.
- **Resolution**: the destination tile click (`_try_dispatch_pending_move_band`) emits
  `move_band_requested` → `Main._on_hud_move_band` → `move_band …`; the expedition-target click
  (`_try_dispatch_pending_send_expedition`) emits `send_expedition_requested` →
  `Main._on_hud_send_expedition` → `send_expedition …`.
- **Scouting expedition** (`docs/plan_exploration_and_sites.md` §2; snapshot
  `PopulationCohortState.isExpedition`/`expeditionMission`/`expeditionPhase`, decoded in
  `native/src/lib.rs population_to_dict` as `is_expedition`/`expedition_mission`/`expedition_phase`,
  flowed onto the MapView unit marker in `_rebuild_unit_markers`; `homeBandEntity` is decoded as
  `home_band_entity` (the outfitting band — powers the Band panel's Active-expeditions section),
  while the persistence-only `expeditionAnnounced`/`pendingReveal*` fields stay undecoded). A
  detached party is a `PopulationCohort` tagged `Expedition` that flows through the same
  `populations[]` array as a band. Surfaced four ways:
  (1) **Distinct map marker** (`MapView._draw_unit` → `_draw_expedition_body`): a hollow,
  faction-tinted **flag disc** (⚑) instead of a resident band's solid dot; when
  `expedition_phase == "awaiting"` a **pulsing amber (WARN) ring** signals idle-at-objective needing
  an order (animated from `_expedition_time` in `_process`, gated on `_has_awaiting_expedition` set
  at marker-rebuild). Resident-band rendering is untouched.
  (2) **Expedition drawer panel** (`Hud._render_occupant_drawer` → `_build_expedition_panel`):
  replaces the labor-allocation panel for a selected expedition (no labor in v1). Drawer text
  (`_expedition_summary_lines`) shows Mission / humanized Phase / Party / Provisions (`turnsOfFood`);
  the panel hosts **Recall** (→ `recall_expedition_requested` → `Main._on_hud_recall_expedition` →
  `recall_expedition …`) + **Move** (reuses `_on_move_band_pressed`; `_resolve_assign_band` returns
  the selected expedition since it's a player unit — Move retargets it via `move_band` unchanged, no
  un-gating needed).
  (3) **Outfit UI** (`Hud._build_allocation_panel` → `_build_send_expedition_controls`): on a
  selected resident band, a "Send scouting expedition" party-size stepper (max =
  `min(idle_workers, max_expedition_party_size)`; the server's hard cap comes from the
  `maxExpeditionPartySize` snapshot field, decoded as `max_expedition_party_size`, defensively
  falling back to idle when absent/0) + a button entering `_pending_send_expedition` targeting.
  (4) The `marker_field_guard` covers the four new marker keys (`is_expedition`,
  `expedition_mission`, `expedition_phase`, `max_expedition_party_size`). The server still rejects
  a genuinely over-cap request with a feed message as a backstop.
- **Hunting expedition** (PR 2, `docs/plan_exploration_and_sites.md` §2b; snapshot
  `PopulationCohortState.expeditionTargetHerd` (string fauna_id) / `expeditionHuntPolicy` (string
  `sustain|surplus|market|eradicate`) / `expeditionCarryCap` (float), decoded as
  `expedition_target_herd` / `expedition_hunt_policy` / `expedition_carry_cap` and flowed onto the
  marker; `expedition_mission` also takes `"hunt"`, `expedition_phase` also takes
  `"hunting"`/`"delivering"`). A hunt party follows a migratory herd, accumulates food up to a carry
  cap, and drops it at the band — the second verb on the same expedition machinery.
  **The in-flight next-delivery forecast** (`PopulationCohortState.expeditionEtaTurns` /
  `expeditionProjectedDelivery` / `expeditionRecurring`, decoded in `native/src/lib.rs` as
  `expedition_eta_turns` / `expedition_projected_delivery` / `expedition_recurring`) is the client's
  "Next delivery: ~N food in M turns" readout — see the parties inspector strip under Band/City. **All
  three MUST be copied onto the unit marker in `MapView._rebuild_unit_markers`** (beside
  `expedition_target_herd` / `expedition_carry_cap`), because the Occupants **detail panel** reads
  `_selected_unit` — which is the marker, NOT the raw population dict — so a field the marker drops
  renders the panel blank even while the Parties ROW (which reads the raw dict) shows it. This is the
  drop-prone-marker-field bug class: `expedition_projected_delivery` is in `marker_field_guard`'s
  `FRACTIONAL_ROUND_TRIP_KEYS` (a continuous float, must not `int()`-narrow), all three in
  `PANEL_CONSUMED_KEYS`. Surfaced:
  (1) **Distinct map marker** (`MapView._draw_expedition_body`): a hollow 🏹 **bow disc** (vs the
  scout's ⚑ flag), keyed on `expedition_mission == "hunt"`. Phase read: `hunting` (gathering) draws a
  small red "working" cue ring; `delivering`/`returning` (hauling home) draw a green food pip.
  (2) **Hunt drawer panel** (`Hud._expedition_summary_lines` branches on mission): Mission "Hunting
  expedition", **Target** herd (`expedition_target_herd`, species via `_herd_label_for_id` → raw id
  fallback), **Policy** (`expedition_hunt_policy`, capitalized), humanized **Phase**
  (Hunting/Delivering/Returning), Party, and **Carried X / cap** (`stores` total vs
  `expedition_carry_cap`, turns from `turnsOfFood`) with a **· FULL** badge at the ceiling. Reuses
  `_build_expedition_panel` (Recall + Move, "Returning"-when-returning treatment — mission-agnostic,
  so hunt parties get it too).
  (3) **Outfit UI** (`Hud._build_send_expedition_controls`): under the shared "Send expedition"
  section (party stepper + "Send scouting expedition"), a **hunt policy radio**
  (`HudWidgets.build_policy_picker(…, _send_hunt_policy)`, Sustain/Surplus/Market/Eradicate, default Sustain)
  with a one-line behaviour hint (`SEND_HUNT_POLICY_HINTS`), then "Send hunting expedition". It enters
  a HERD-targeting pending mode (`_pending_pick_quarry`, `command: "quarry"`, `need: "herd"`) carrying
  the band; the pick resolves to a huntable herd on the clicked hex (`_huntable_herd_on_tile` reads
  `tile_info.herds`), fills the sheet's Quarry row, and the sheet's own Send then emits
  `send_hunt_expedition_requested` → `Main._on_hud_send_hunt_expedition` →
  `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]` (trailing policy;
  server defaults Sustain). No huntable herd on the hex → a command-feed nudge, stays in targeting.
  For `need == "herd"` `AnnotationRenderer.draw_targeting` reticles the hovered hex and glows the herds that are
  **valid quarries — those strictly BEYOND the outfitting band's `hunt_reach`**, never every huntable
  herd. A nearer herd is a LOCAL hunt (the same split `_build_herd_assign_controls` makes between
  "Assign Local Hunt" and the expedition branch), so haloing it would promise a mission the pick then
  refuses. The reach rides the targeting info dict as **`min_distance`** — "a valid target must lie
  strictly farther than this from `origin_x/origin_y`"; every other targeting mode omits it and MapView
  defaults it to **0**, which admits everything and changes nothing for move/scout-tile targeting. The
  MapView test is commented as the RENDER-SIDE MIRROR of `Hud._is_expedition_quarry` — change the two
  together, in both directions.
  (4) `marker_field_guard` covers `expedition_target_herd` / `expedition_hunt_policy` /
  `expedition_carry_cap`. Recall is the unchanged `recall_expedition` (works for hunt parties too).
  (5) **Pre-launch RAID forecast — the delivered payload + waste** (server `5a130e0`): a hunting expedition
  is a **greedy raid** — it grabs the herd's standing surplus above the policy floor in a burst and comes
  home. A party too small to carry a whole animal now **kills one and hauls the fraction its pack holds,
  wasting the rest**, so the readout headlines the delivered PAYLOAD: **the animal count over the turns, the
  FOOD landed, and the WASTE**, `delivers ≈1 Thunder Mammoth over ≈20 turns · ~4 food · ⚠ 75% wasted`. The
  player must know **before** committing workers — and the band-panel launch flow now guarantees they can,
  because it asks for the **QUARRY FIRST, inside the compose sheet**. The old premise ("the herd isn't
  chosen until the targeting step, so the forecast has to hang off the targeting banner") is **inverted and
  gone**, and the hover-forecast + `_hovered_tile_info` with it: the herd is what determines the useful
  party size, the per-policy take, the trip length and whether the raid is worth making, so it cannot be
  the LAST question. The targeting mode is now a quarry **PICKER** (`_pending_pick_quarry` /
  `_on_pick_quarry_pressed` / `_try_pick_quarry`, `command: "quarry"`, `need: "herd"` — still what makes
  MapView glow the huntable herds): it carries only the band, dispatches nothing, and on a hit stores the
  herd id in the sheet and re-renders. **The forecast, the max-useful cap, the ascending per-policy metrics
  and the no-surplus block therefore all live in the FORM**, from the SAME helpers the herd drawer's
  beyond-reach branch uses (`SourceForecast.expedition_policy_takes` · `SourceForecast.expedition_useful_cap` · `SourceForecast.hunt_trip_forecast` →
  `SourceForecast.hunt_forecast_line_bbcode` · `SourceForecast.style_send_hunt_button` · `SourceForecast.hunt_no_surplus_reason`), so the two entry
  points structurally cannot quote different numbers. The line reads cyan
  `delivers ≈N <Herd> over ≈M turns · ~F food` (+ amber `· ⚠ P% wasted`) for a brisk raid, WARN-amber `⚠ … — a slow raid` past `expeditionViabilityWarnTurns` (or `delivers ≈N <Herd>
  over many turns … — a slow raid` for a **long** raid, `turnsToFill == 0`, that ran the whole horizon still
  delivering), amber denial `<Herd> — denial mission … delivers no food` (Eradicate), and DANGER-red
  `⚠ <Herd> is too lean to raid — its surplus is spent` when **`deliveredFood == 0`** (the herd at/below the
  policy floor — a small party on big game delivers a partial with waste and is NOT too lean). The click
  still commits (information, not a gate — except the no-surplus case, which the herd panel's button
  DISABLES; see `%HerdAssignControls`).
  **The food total** is `HuntTripEstimate.deliveredFood` — the sim's forward-simulated landed food (NOT
  `animals × foodPerAnimal`, which counts the whole kill and overstates a partial), set on the returned dict
  as `food` (always present on a delivering forecast); the waste % is `wastedFood / (deliveredFood +
  wastedFood)`. All rendered by the shared `SourceForecast.hunt_forecast_line_bbcode` at **both** entry points (the party
  compose sheet + the herd drawer), so the two can never quote different numbers.
  **The client does ZERO arithmetic for an expedition's raid — it is a pure TABLE LOOKUP.** A band and
  an expedition are different actors and read **different herd fields**; never one for the other:
  - **Expedition → `HerdTelemetryState.huntTripEstimates`** (one entry per policy × party size),
    decoded in `native/src/lib.rs` into `hunt_trip_estimates` on the herd dict, keyed
    `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, animals_taken, delivered_food,
    wasted_food}` (so it flows through `tile_info.herds` untouched — **`delivered_food`/`wasted_food` are
    the newest appended fields, added to this decoder dict in this pass; the decoder has silently dropped
    appended fields 6× now, always audit it first**). `SourceForecast.hunt_trip_forecast` just looks it up:
    `delivers_food == false` → **denial** (Eradicate — "delivers no food", the SIM decides this, the client
    never infers it from the policy string); **`delivered_food == 0`** → **no surplus** (the one blocked
    case — the raid returns empty at every party size; NOT `animals_taken == 0`, which is now ≥ 1 whenever
    there's any surplus since a small party still kills one animal and wastes the uncarried meat); else the
    raid delivers `delivered_food` food (`animals_taken` kills, `wasted_food` rotted), with `turns_to_fill
    == 0` meaning a **long raid** (ran the whole horizon) and `> expeditionViabilityWarnTurns` flagged
    **slow**. `deliveredFood` PLATEAUS with party size once the surplus binds — that plateau is the
    **max-useful** party the stepper caps at (`SourceForecast.expedition_useful_cap`), and the per-policy picker cap is the
    max over party sizes of `deliveredFood / (turnsToFill + travel)`. **Do not re-derive any of this** — the
    sim forward-simulates the raid (the herd's state moves under the party, a horizon bounds the answer) and
    exports the numbers.
  - **Resident band → `huntPolicyCeilings`** (`provisionsPerTurn`, the herd's renewable **flow**),
    decoded as `hunt_policy_ceilings`. This one IS pure client arithmetic, and the schema blesses it:
    `min(workers × huntPerWorkerProvisions, ceiling) × outputMultiplier` (`_hunt_take_rate` →
    `_local_hunt_preview_bbcode`) — but it must still never re-derive the ecology/MSY model.
  Plus the global levers echoed on every cohort (same idiom as `maxExpeditionPartySize`, decoded +
  flowed onto the MapView unit marker + covered by `marker_field_guard`). **Neither of them is an
  input to an expedition's raid** — that is the lookup above. Their real jobs: `expeditionViabilityWarnTurns`
  = the **slow-raid threshold** applied to the **TOTAL** trip (`turnsToFill` HUNTING turns **+** the
  client's round-trip travel — a distant herd trips it on travel alone), and
  `huntPerWorkerProvisions` = the **resident-band local-hunt take rate** (the one legitimate piece of
  client arithmetic, pinned by `exported_snapshot_fields_reproduce_band_hunt_take`). The one-liner
  that keeps this straight: **band = flow arithmetic; expedition = lookup.** Missing estimate /
  levers absent → no forecast line, banner unchanged. (The old `haul` key — `party ×
  expeditionPerWorkerCarry` — is retired: a raid's payload is the sim's `animalsTaken`, not a
  party×lever product. `expeditionPerWorkerCarry` is still decoded onto the marker for completeness but
  no longer feeds the forecast.)
  ui_preview banner states `hunt_forecast_viable` / `hunt_forecast_slow` / `hunt_forecast_no_surplus`
  + `expedition_launch_policy_sustain`; herd-panel expedition states `herd_hunt_forecast_viable` (the
  partial-with-waste Thunder Mammoth: `~4 food · ⚠ 75% wasted`, button ENABLED) / `_slow` / `_surplus` /
  `_no_surplus` (`deliveredFood 0` everywhere → disabled "too lean") / `_eradicate` (denial, enabled),
  the raid set `herd_hunt_boar_raid` (clean, no waste) / `herd_hunt_max_useful` / `herd_hunt_raid_travel`
  (travel-inclusive `over ≈16 turns (8 hunting + 8 travel)`, and the picker caps correctly lower) /
  `herd_hunt_expedition_automax` (a policy click fills the Party to max-useful).
- **Retired verbs (Early-Game Labor slice 3a):** the server now parses-but-ignores
  `follow_herd` / `scout` / `forage` / `hunt_fauna` / `hunt_game`. Every client control that
  emitted them was removed or repointed so nothing is silently dead: the map double-click
  `scout` shortcut was dropped and `follow` repointed to quick-assign hunters; Main's
  `_issue_*`/`_on_hud_follow_herd`/`_on_hud_unit_scout` builders are gone; the Fauna tab's
  follow button, the Terrain tab's Scout Tile button, and the Commands tab's scenario
  Scout/Follow rows were removed (script + `InspectorLayer.tscn` nodes). No code path in
  `Main.gd`/`Hud.gd`/`MapView.gd`/`Inspector.gd` builds any of those five lines.

