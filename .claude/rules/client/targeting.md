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

> **There is no hunting expedition** (`docs/plan_civilization_steps.md` §One work party). A herd past
> the band's apron is an ordinary hunt whose crew posts a work party (`labor-ui.md` → "A FAR SOURCE IS
> AN ORDINARY SHEET", `band-city-panel.md` → "THERE IS NO HUNT VERB"). The one herd pick is the
> denial raid's, and its rule is "ONE QUARRY RULE" below.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/TargetingController.gd` | `RefCounted` controller (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the **COMMAND-TARGETING** cluster — the band verbs' map picks (**move-band** picks a destination TILE, the **verb tile pick** is Scout's, **pick-quarry** is Deny's HERD pick) plus the floating top-centre **targeting banner** that guides each. It holds the three pending dicts (`_pending_move_band` / `_pending_verb_pick` / `_pending_pick_quarry`), the banner (`_ensure_targeting_banner` / `_refresh_targeting` / `_current_targeting_info` / `_targeting_banner_bbcode`), the per-flow begin/cancel/dispatch functions (`_try_dispatch_pending_move_band` / `_try_verb_pick` / `_try_pick_quarry` / `_huntable_herd_on_tile`) and the wrap-aware `_hex_distance_wrapped`. **Public API:** `begin_move_band` / `begin_verb_pick` / `send_expedition_to` / `trade_destination_at` / `tie_at` / `live_tie_tiles` / `set_preselect` / `clear_preselect` / `is_preselect_on` / `begin_pick_quarry` / `cancel_pick_quarry` / `choose_quarry` / `quarry_chooser` / `disarm_verb_picks` / `is_verb_pick_armed` / `note_hover` / `refresh_hover` / `banner_text` / `banner_tooltip` / `is_expedition_quarry` (THE single quarry-eligibility definition — the pick, the chooser, the hover banner and MapView's glow all route through it), plus `is_targeting_active` / `cancel_active_targeting` / `try_dispatch` (the last runs the three `_try_*` in the SAME order as before). Hud holds it as `_targeting`, constructed in `_ready` **AFTER `_drawercompose` and BEFORE `_bandpanel`** (which injects it — so `_targeting` must exist first). **It emits its OWN signals, HudLayer RELAYS each** (the `TurnOrbController` pattern; the controller never emits a HudLayer signal): `targeting_changed` (→ `MapView.set_targeting`) · `move_band_requested` · `send_expedition_requested` · `verb_pick_cancelled` (→ `BandPanelController.on_verb_pick_cancelled`). **The reflective delegators STAY on HudLayer** — `is_targeting_active` (Main's escape_claimant path) / `cancel_active_targeting` (Main relays MapView's `targeting_cancel_requested` by name) / `notify_targeting_click` (MapView's `targeting_clicked`, → `try_dispatch`) / `notify_hex_hovered` (MapView's `tile_hovered`, → `note_hover`), each probed BY NAME so a `has_method` miss fails SILENTLY. **The injection surface is TWO Callables** — `_resolve_assign_band` (STAYS on HudLayer, DrawerComposeController injects it too; reached through a typed adapter since `Callable.call` returns `Variant`) and `_after_pending_change` (STAYS on HudLayer, the `_emit_assign_labor` pending path owns it); an armed verb pick carries the SHEET's own `commit` / `hover` Callables, so the controller holds no reference to `BandPanelController`. Collaborators: `_band_labor` (`record_pending_move` + the grid pair + the roster and ties the picks resolve against), `_drawercompose` (the three `close_compose_sheet()` nudges), `_note_sink` (the picks' miss/refusal notes, to the event dock's System channel), and the HUD CanvasLayer as the **host** it parents the banner and the Deny chooser into — via the host's `LayoutRoot` (NOT the bare CanvasLayer) so the banner keeps insetting with the reserved-edge docks exactly as before |
## Command Targeting

### THE BAND VERBS' TARGET IS THE LAST STEP, AND THE CLICK COMMITS (issue #529)

Move, Scout, Deny, Trade and Split are one list (`HudComposeVocab.BAND_VERBS`), pressed from the Band
panel's action bar or the tile panel's band drawer, and routed to ONE dispatch
(`BandPanelController.dispatch_verb`). Move arms its tile pick straight away. The other four open their
SHEET on the band's own drawer (the band is selected on its own hex), and the pending verb — which one,
for which band, anchored on which hex — is shared state on `ComposeState` (`open_verb` / `clear_verb`).

| verb | the sheet | its send | the click |
|---|---|---|---|
| Move | — | — | emits `move_band_requested` |
| Scout | party + kit | arms `begin_verb_pick(band, scout, commit)`, tile | `send_expedition_to` the clicked tile |
| Deny | [Prey] + party + kit (+ the verdict, caveat, take, refusal once a prey is set) | with a prey: commits `send_denial_raid`; without: arms `begin_pick_quarry(band, deny, commit, hover)`, herd | `send_denial_raid` at the clicked herd; a hex holding several eligible herds opens the chooser first |
| Trade | To + porters + pack + cargo | with a destination: commits `send_trade_expedition`; without: greyed, its reason saying to pick a band | — (the destination is the pre-selection click below) |
| Split | workers | commits `split_band` itself | — |

- **An open Deny or Trade sheet HIGHLIGHTS every target its pick would accept, and a click on one
  PRE-SELECTS it.** The sheet registers `TargetingController.set_preselect(band, mission, choose)` on
  every render and `_reset_verb_state` clears it (`clear_preselect`), so the highlight lives exactly as
  long as the sheet, armed or not. While no pick is armed the descriptor is PASSIVE
  (`TARGETING_PASSIVE_KEY`): MapView draws the Deny sheet's herds through the herd glow (`need: "herd"`,
  `min_distance`) and the Trade sheet's live-tie bands through an explicit ring set
  (`TARGETING_HIGHLIGHT_TILES_KEY`, `live_tie_tiles` — parked ties excluded), with no reticle and no
  banner, and it is NOT targeting: `is_targeting_active` is false, so Esc, right-click and every other
  click behave normally. `MapView.targeting_click_captures` takes only a click on a highlighted target
  (`AnnotationRenderer.highlighted_at`) into `targeting_clicked`, and `try_dispatch`, with nothing
  armed, runs the ONE pre-select path for both verbs (`_try_preselect`): the sheet's `choose` is handed
  the same target dictionary a commit gets — the herd (the chooser first for a hex holding several), or
  the tied band standing there. Nothing is sent and nothing is selected; the sheet then shows its `Prey`
  or `To` row with a `✕` to clear it, and its Send commits straight away. **Trade has no armed pick**:
  the pre-selection click is the only way its destination is set, and a parked tie's band is never
  ringed, so a click on it sets nothing.
- **The send arms the pick with the sheet's values captured** as a `commit` Callable
  (`TargetingController.PICK_COMMIT_KEY`), and the sheet stays open with its send drawn `armed` (a
  toggle, `HudComposeVocab.VERB_SEND_ARMED_STYLE`); pressing it again takes the pick down. The sheet
  re-arms on every render while armed, which only swaps the pick's callables in place, so an edit made
  while the pick is up is what the click sends.
- **The click commits and selects nothing.** A left click while targeting is active is MapView's
  `targeting_clicked`, never `tile_selected` (`Main` relays it to `HudLayer.notify_targeting_click` →
  `try_dispatch`), so neither the selection nor the Band panel's subject moves off the band. A valid
  click calls the commit, which emits the command and closes the sheet (`close_verb_form`, which also
  clears the pending verb). A commit that answers a refusal posts it and leaves the pick armed.
- **Invalid clicks stay armed**: a hex with no eligible herd posts `PREY_PICK_MISS`, and a Deny click
  on a herd the band cannot field the required party for posts
  `SourceForecast.denial_short_handed_reason` — the one refusal the commit itself makes.
- **The Deny chooser** (`_open_quarry_chooser`) is a `PopupMenu` at the pointer, one entry per eligible
  herd on the clicked hex (`eligible_quarries_on_tile`, derived live from `world_herds`, entries built
  through `HudWidgets.fill_menu_popup`). Under an armed pick choosing one runs `choose_quarry`, the one
  adoption, which re-checks eligibility and commits; under the passive highlight it pre-selects.
  Dismissing it leaves the pick (or the highlight) as it was.
- **The hover states what the click would commit to.** `MapView.tile_hovered` also reaches
  `HudLayer.notify_hex_hovered` → `note_hover`, and an armed pick's `hover` Callable
  (`PICK_HOVER_KEY`) answers for that hex; a non-empty answer replaces the banner's instruction with
  `→ <answer>` (`HudComposeVocab.VERB_HOVER_ARROW` / `VERB_HOVER_DETAIL_FORMAT`):
  - **Deny** — `DENY Saltmarch → Wild Boar · <verdict>`, the collapse verdict for the captured party
    and kit (`BandPanelController._deny_hover_detail`). The forecast is a SERVER query
    (`ForecastQuery`, idempotent on its key, so hovering one herd twice asks once); until it answers
    the line is `DENIAL_FORECAST_PENDING`, and its answer re-states the banner through `refresh_hover`.
    A hex holding several eligible herds names the first and counts the rest (`VERB_HOVER_MORE_FORMAT`,
    `Rabbit Warren +1 more`). A band short of the herd's requirement reads the short-handed sentence
    in place of the verdict. **The take line rides the banner's TOOLTIP, not the banner**: the verdict
    alone runs ~120 characters, and with the take beside it the banner passed 1500px — so a hover may
    answer a Dictionary (`PICK_HOVER_TEXT_KEY` / `PICK_HOVER_TOOLTIP_KEY`) and the banner shows the
    text and carries the tooltip (`banner_tooltip`).
  - **Scout** states no hover detail. **Move** states one only for a resident band hovering a hex
    farther than its `move_ferry_reach_tiles` while any `long_move_leaves_*` field is non-zero: an
    amber *"Too far to carry it all — leaves … behind"* built from those published fields, the sim's
    own forecast of the shed (`.claude/rules/core_sim/band-carry.md`). The client mirrors none of
    the rule. A hex with nothing to state keeps the base prompt.
- **Every banner names the band by its NAME** (`HudFormat.band_name`), never `Band <id>`. The command
  token and its instruction come from `DENY_PICK_COMMAND` / `VERB_PICK_COMMAND_*` / `MOVE_COMMAND` and
  the `BANNER_INSTRUCTIONS` table.
- **Esc cancels the pick alone.** `cancel_active_targeting` (the banner's Cancel, Esc, right-click)
  emits `verb_pick_cancelled`, which `HudLayer` routes to `BandPanelController.on_verb_pick_cancelled`:
  the sheet re-renders with its values and its send un-armed. A second Esc is `Main.escape_claimant`'s
  `ESC_VERB_FORM` — behind `ESC_TARGETING` — which closes the sheet (`HudLayer.is_verb_form_open` /
  `close_verb_form`). `disarm_verb_picks` is the verb's OWN teardown and announces nothing.
- **The Trade pre-selection accepts only a LIVE tie** (`trade_destination_at` over `tie_at`,
  `HudBandLaborState.tie_is_live`, strength above `TIE_STRENGTH_NONE`). A tied band still in the roster
  is found where it stands; one that is not is found where the tie last saw it.
- **A pick's refusal notes post under the verb's name** (`_pick_note_title_for`), with
  `PREY_PICK_NOTE_TITLE` for a mission that names no verb.

Labor allocation is source-centric (assign workers to a source/role, see the **Labor
allocation UI** bullet below). The one remaining **targeting mode** is **move-band** —
picking a destination tile — replacing the old easy-to-miss "select a band…" line.

- **Targeting: the band verbs' three picks** (`ui/hud/TargetingController.gd`, held as `_targeting` —
  see its Key Scripts row; the whole cluster left `Hud.gd` in a decomposition pass): the single-task
  forage/scout/hunt/follow `_pending_*` flows were retired with labor allocation. Three targeting flows
  remain, all built on the same `_pending_*` → `_current_targeting_info()` → `_refresh_targeting()`
  machinery ON THE CONTROLLER: `_pending_move_band` (`command: "move"`, `need: "tile"`),
  `_pending_verb_pick` (`command: "scout"` / `"trade"`, `need: "tile"`, carries the band, the verb's
  mission and the sheet's `commit` / `hover`), and `_pending_pick_quarry` (`command: "deny"`,
  `need: "herd"`, plus **`min_distance`** — the band and the sheet's `commit` / `hover`).
  `_current_targeting_info()` returns a descriptor (`{active, command, need, origin_x/y,
  context_label}`) for whichever is set; `_refresh_targeting()` shows the floating **targeting
  banner** (top-centre, `HudStyle.banner_stylebox()`: cyan reticle + command + instruction + Cancel)
  and emits the controller's `targeting_changed(info)` (relayed onto the HudLayer signal). **The
  `command` token IS the banner's lead word, uppercased** (`_targeting_banner_bbcode`), so it is a
  player-facing string rather than plumbing: the herd pick reads `DENY  Saltmarch — click a herd to
  deny`. MapView keys its halo off `need`, never off this token. HudLayer's `notify_targeting_click` (MapView's
  `targeting_clicked`) calls `_targeting.try_dispatch(tile_info)`, which runs all three pending flows on
  the click (the click carries `tile_info.herds`, which the herd pick resolves its target from).
- **Main forwards** `hud.targeting_changed → map_view.set_targeting`,
  `map_view.targeting_cancel_requested → hud.cancel_active_targeting` (a HudLayer delegator →
  `_targeting.cancel_active_targeting`), `map_view.targeting_clicked → hud.notify_targeting_click` and
  `map_view.tile_hovered → hud.notify_hex_hovered` (beside the tooltip's own `show_tooltip`).
- **MapView draws** the overlay (`AnnotationRenderer.draw_targeting`, reached through MapView's `set_targeting` pass-through): `need == "tile"` draws a reticle on the
  hovered hex (the `need == "band"` path is now unused). Esc / right-click during targeting emit
  `targeting_cancel_requested` instead of panning, and a left click emits `targeting_clicked` instead of
  selecting (`_emit_targeting_click`, the visibility-redacted `tile_info` with no `selected_tile` write);
  the pulse is animated from `_process`. `focus_and_select_tile` still selects — it is the cycler's and
  the attention jump's path, not a click.
- **Resolution**: the destination tile click (`_try_dispatch_pending_move_band`) emits
  `move_band_requested` → `Main._on_hud_move_band` → `move_band …`; the Scout pick's click
  (`send_expedition_to`) emits `send_expedition_requested` → `Main._on_hud_send_expedition` →
  `send_expedition …`.
- **The scouting party's kit rides the payload** beside the default `Main._kit_token` omits it at, so
  a composition that never touched the picker emits the line it emitted before the picker existed.
  Whose kit it is and why the job is not `hunt`: `labor-ui.md` → "The `expedition` job".
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
  `recall_expedition …`) + **Move** (`.connect()`ed straight to `TargetingController.begin_move_band`,
  which with no argument resolves the selected player unit — the expedition — and retargets it via
  `move_band`).
  (3) **Outfit UI**: the **Scout** band verb — the Scout sheet (a party stepper capped at the band's
  idle workers, and the kit) in the band's drawer, whose send arms the tile pick; the click is the
  order. See "THE BAND VERBS' TARGET IS THE LAST STEP".
  (4) The `marker_field_guard` covers the four new marker keys (`is_expedition`,
  `expedition_mission`, `expedition_phase`, `max_expedition_party_size`). The server still rejects
  a genuinely over-cap request with a feed message as a backstop.
- **The herd pick** (`_pending_pick_quarry`, `need: "herd"`) — Deny's — resolves a huntable herd on
  the clicked hex (`_huntable_herd_on_tile` reads `tile_info.herds`); no eligible herd on the hex → a
  command-feed nudge, and the pick stays armed. For `need == "herd"` `AnnotationRenderer.draw_targeting`
  reticles the hovered hex and glows the herds that are valid quarries. The bound rides the targeting
  info dict as **`min_distance`** — "a valid target must lie strictly farther than this from
  `origin_x/origin_y`"; every other targeting mode omits it and MapView defaults it to **0**, which
  admits everything and changes nothing for move/scout-tile targeting. The MapView test is commented as
  the RENDER-SIDE MIRROR of `TargetingController.is_expedition_quarry` — change the two together, in
  both directions.

  **ONE QUARRY RULE: EVERY HERD AT A KNOWN DISTANCE.** `is_expedition_quarry(band, herd)` /
  `eligible_quarries_on_tile(band, x, y)` / `begin_pick_quarry(band, commit, hover)` take no mission,
  and the bound that goes on the wire as `min_distance` is **`QUARRY_NO_REACH_BOUND` (`-1`)**, so the
  pick and the glow are one number rather than two derivations. `-1` rather than `0` because the test
  is *strictly farther than*: at `0` a herd on the band's OWN tile would fail it, and at `-1` the
  unknown distance (`-1`) still does, which is how "an unknown distance is never a quarry" falls out
  of the same comparison. The quarry pick is the Deny pick: it has its own banner
  (`DENY … — click a herd to deny`) and posts its refusal notes under the verb's name.

  `marker_field_guard` covers `expedition_target_herd` / `expedition_carry_cap`. Recall is the
  unchanged `recall_expedition`, mission-agnostic.
- **Retired verbs (Early-Game Labor slice 3a):** the server now parses-but-ignores
  `follow_herd` / `scout` / `forage`; `hunt_fauna` / `hunt_game` are deleted outright and no
  longer parse. Every client control that
  emitted them was removed or repointed so nothing is silently dead: the map double-click
  `scout` shortcut was dropped and `follow` repointed to quick-assign hunters; Main's
  `_issue_*`/`_on_hud_follow_herd`/`_on_hud_unit_scout` builders are gone; the Fauna tab's
  follow button, the Terrain tab's Scout Tile button, and the since-deleted Commands tab's scenario
  Scout/Follow rows were removed (script + `InspectorLayer.tscn` nodes). No code path in
  `Main.gd`/`Hud.gd`/`MapView.gd`/`Inspector.gd` builds any of those five lines.

