---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{PenStatus,FaunaSprites}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandDetailLines.gd"
  - "clients/godot_thin_client/src/scripts/ui/inspector/FaunaPanel.gd"
---

<!-- Extracted verbatim from lines 2475-2704 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Herd readouts — fog gate, ecology, husbandry, corral, the pen

- **Fog gate on live tile contents — "nothing here" ≠ "I can't see what's here"** (`MapView.gd` +
  `Hud.gd`). Herd MARKERS were always Active-gated (`_draw_herd`), but the herd **lookup** wasn't:
  `_herds_on_tile` matched by coordinate with no visibility test, so a fogged hex listed its herds in
  the Occupants roster, let you target them for a hunt, and fed them into the trip forecast.
  - **MapView (source of truth):** `_herds_on_tile` now early-returns on `not _is_tile_visible(col,row)`
    — the SAME gate the renderer uses. It's the single chokepoint (roster / herd-selection click /
    hunt-target click / forecast all read herds through `_tile_info_at` → `tile_info.herds`), so
    "you can only hunt and forecast what you can see" is true by construction. Three sibling leaks
    closed with it: `_herd_at_point` (double-click quick-hunt could hit an undrawn marker), the
    `need == "herd"` targeting glow in `AnnotationRenderer.draw_targeting` (it haloed every huntable herd, fogged ones
    included — the halo WAS the leak), and the `selection_payload` re-resolve of `selected_herd_id`
    (a selected herd that WALKS into fog kept streaming live biomass/ecology + a live forecast; it now
    drops with its marker and the hex falls back to the tile card). **The server still exports every
    herd unfiltered — a wire-level leak tracked separately — so this client gate is LOAD-BEARING, not
    cosmetic. Never read `herds` by coordinate without it.**
  - **Units — same rule, plus the ownership exception** (`_unit_hidden_by_fog`, the ONE definition):
    `hidden == tile not currently visible AND the unit is not ours`. **Your own units are ALWAYS shown,
    including on an Unexplored hex** — that exception is load-bearing, not a courtesy: the sim excludes
    expeditions from fog reveal (`calculate_visibility` runs `Without<Expedition>` — discovery is
    comm-range gated), so a scouting party ROUTINELY stands on an Unexplored tile, and a plain
    visibility gate would erase your own expedition from the map exactly while you're using it. Applied
    at all five leaks: **`_draw_primary_bands`** (had NO gate — foreign bands rendered straight through
    the fog; the worst of them), `_units_on_tile` (roster/click/stack-cycling chokepoint),
    `_unit_at_point` (marker hit-test), `_nearest_unit_sample` (leaked a hidden band's label *and* a
    bearing on it into `tile_info`), and `refresh_selection_payload`'s selected-unit re-resolve (a
    foreign band walking into fog kept streaming live state — now drops its selection, mirroring the
    herd rule). Already-correct (left alone): everything player-scoped — `_draw_supply_links`,
    `BandOverlayRenderer._selected_player_band`, the `need == "band"` targeting glow, band alerts, own work highlights.
    Hud mirrors the exception in `_assemble_roster` (an unseen hex lists your own units, never foreign
    ones, and no herds) and appends `OCCUPANTS_UNSEEN_OTHERS_HINT` ("Out of sight — you can't see
    anything here but your own.") so a lone own-party row never implies the hex is otherwise empty.
    ui_preview: `tile_sight_own_expedition` (the regression guard — own expedition on Unexplored still
    listed + selectable + Move/Recall) / `tile_sight_foreign_hidden` / `tile_sight_foreign_visible`.
  - **Hud (says the truth):** the Tile card leads with a **`Sight:` row** — `In sight` (SIGNAL cyan) /
    `Remembered — not in sight now` / `Unexplored` (both INK_DIM; it states what you KNOW, so it never
    borrows the WARN/DANGER palette) — via `_tile_sight_line` + `DetailFormat.sight_value_hex`. On an unseen hex,
    `_tile_contents_unseen` (which re-reads MapView's `visibility_state` flag — NOT a second visibility
    test) makes `_assemble_roster` list nothing, `_build_forage_assign_controls` offer nothing, and
    `_render_unknown_contents_note` state it in the drawer instead of an empty list
    ("You remember the ground here, but not what's on it now." / "Nobody has been here. Send a band to
    reveal what's on this ground."). **The remembered one states and does not instruct** — it closed
    `… — bands and herds move. Scout it to see.` until issue #462, and scouting cannot take a hex out
    of the very state that sentence describes; the unexplored one keeps its verb because there sending
    a band DOES change the state. That rule, and the drawer sentences deleted alongside it, live in
    `land-readouts.md` → "An unseen hex says so ONCE, and promises nothing it cannot do".
    An EMPTY roster is a claim of emptiness the client can't back up, so it
    is never rendered on a hex you can't see. Terrain rows stay (geography is remembered knowledge;
    occupants are live state). ui_preview states `tile_sight_active` / `tile_sight_remembered` (fixture
    deliberately carries a herd → proves it is NOT listed) / `tile_sight_unexplored`.
- **Herd ecology readout** (`Hud.gd` `_herd_summary_lines`): the selection panel shows
  the group's `ecology_phase` (snapshot `HerdTelemetryState.ecologyPhase`) as an
  **Ecology** row — a neutral "Thriving", or a warned "⚠ Stressed" / "⚠ Collapsing"
  that `DetailFormat.detail_bbcode` tints amber / red (`DetailFormat.ecology_value_hex`, `HudStyle.WARN_HEX`
  / `DANGER_HEX`). A `Collapsing` herd has been overhunted past the point of no return and
  is crashing to local extinction (see `core_sim` Fauna & Wild Game — depensation collapse).
- **Herd grazing range + carrying capacity** (Grazing Phase 2b-iii; `docs/plan_grazing_2b.md` §8,
  `core_sim` Phase 2b-ii — K becomes ecological): make the ecological carrying-capacity model
  *visible*, so the player sees WHY a herd is the size it is. Two wire fields on `HerdTelemetryState`
  (appended after `penFedFraction`), decoded in `native/src/lib.rs herds_to_array` (both snapshot +
  delta share it): **`carryingCapacity`** → `carrying_capacity` (the herd's CURRENT derived K, what it
  caps at on its range) and **`grazeRangeRadius`** → `graze_range_radius` (the hex radius of its
  grazing range: small game 0, big game 1, migratory = its loiter_radius). Surfaced two ways:
  - **Herd drawer rows** (`DetailFormat.herd_summary_lines`): the stock row carries what is standing vs
    the K its range supports as a **`current / max` pair**, in ANIMALS — **`Herd: 15 / 22 · Thriving`**.
    **THE KEY NAMES THE UNIT** (`HERD_STOCK_ROW` / `HERD_STOCK_BIOMASS_ROW`): `Herd` counts animals,
    `Biomass` is the fallback for a species the wire published no `body_mass` for, and the label switches
    WITH the unit rather than staying put — `Herd 821` invites reading 821 as a head count, wrong by the
    body mass. `SourceForecast.animal_count` is the one conversion, and **a positive biomass never counts
    zero**: a herd holding a fifth of a body counts one, as the sim's own carry bound does
    (`animals_the_pack_seats`, a `ceil` with a one-body floor). **Both terms this line used to name are
    gone**: `quantise_animal_take`'s `affordable` arm and its *"cannot spare a whole animal"* early
    return moved to the engagement, where the room is now spent, and `max(1, carryable)` became that
    `ceil`. **The ecology phase RIDES this row** rather than standing as
    an `Ecology:` row of its own, exactly as it does on the tile card's `Foraging` / `Grazing` rows and
    for the same reason (`HudFloraVocab.STOCK_PHASE_CLAUSE_FORMAT`); `_value_hex` keys the stock row
    names to `ecology_value_hex`, which matches the phase word wherever in the value it sits, so folding
    forked no styling. **The herd states NO `Position`** — see `selection-card.md`. The old standalone
    `Carrying cap: ~K` row was merged INTO the pair and removed; the `~` is dropped because a
    `current / max` pair already implies the max is the derived ceiling. A separate **`Range: N tiles`**
    row stays (the ground the herd grazes — the hex-disk count `1 + 3r(r+1)` via `graze_range_label`:
    radius 0 → "Range: 1 tile" singular, 1 → 7, 2 → 19; the SAME count the map ring draws; key ≤
    `DETAIL_KEY_MAX_LENGTH` so `_split_detail_kv` aligns it as a table row beside the stock).
    **Overgrazing is a FEATURE of the pair, and the reason it is a pair and not a fill percentage:** an
    overgrazed herd has `biomass > K`, so the row honestly reads `current > max` (e.g. **`Herd: 21 /
    14`**) — both sides divided by the same body, never clamped — and when `biomass > carrying_capacity ×
    (1 + OVERGRAZE_EPSILON)` a WARN-amber full-width **`⚠ Overgrazing — range can't sustain this herd`**
    row appears beneath (a `DetailFormat.detail_bbcode` branch tinting the sentence with the shared
    `HudStyle.WARN_HEX` — NOT a parallel styling path). This is a **trivial honest comparison of two
    sim-provided numbers**, never a re-derivation of the ecology model (K and graze flow are the sim's).
    **Guards:** `carrying_capacity <= 0` (a herd momentarily on barren range derives K = 0) falls back to
    the bare `Herd: X` (never `X / 0`) and suppresses the overgrazing test; a **corralled** herd (doesn't
    roam-graze a range) suppresses the Range row + overgrazing test entirely (its K is a frozen pen-time
    value), but keeps the merged pair.
  - **Map range ring** (`BandOverlayRenderer.draw_herd_range_highlights`, drawn from `_draw` when a herd is
    selected, under the herd markers): the tiles within `graze_range_radius` of the herd — the EXACT
    ring the sim grazes / derives K over — as a warm graze-amber FILLED region + gold tile outlines
    (`HERD_RANGE_FILL` / `HERD_RANGE_OUTLINE`), deliberately DISTINCT from the band work-range ring's
    faint cyan (a herd's range is a different thing, and both can be on at once) and readable over the
    Pasture overlay (so the ring sits on the actual graze). Reuses the band ring's odd-r `_hex_distance`
    / `_band_effective_col` (seam-wrapped) / `_fill_hex` / `_outline_hex` primitives. `graze_range_radius
    == 0` (small game) → the herd's own single tile. A **corralled** herd draws nothing. Fog-gated via
    `_is_tile_visible` like the herd marker. **CARNIVORE PREY-SENSE RING (Predators Phase 1a):** a
    predator doesn't graze, so its graze ring is meaningless — when the herd carries
    **`prey_sense_radius > 0`** (`HerdTelemetryState.preySenseRadius`, decoded beside `graze_range_radius`;
    `predators.prey_sense_radius` = 4 for a carnivore, 0 for a herbivore, so **`> 0` is BOTH the "this is a
    predator" signal AND the ring radius**) the SAME disk is drawn at that radius in a distinct predator
    ORANGE (`PREY_SENSE_RING_FILL` / `PREY_SENSE_RING_OUTLINE`, echoing MapView's
    `HUNT_DANGER_OVERLAY_COLOR`) **INSTEAD OF** the gold graze ring (a REPLACEMENT, not an addition — the
    same `is_predator` branch swaps radius + colours). A herbivore (`prey_sense_radius == 0`) is unchanged.
  - Verify: ui_preview `herd_grazing_healthy` (`Herd: 15 / 22 · Thriving`, current < max, no warning) /
    `herd_overgrazing` (`Herd: 21 / 14`, current > max → the ⚠ row) / `herd_grazing_small_game`
    (radius 0 → "Range: 1 tile") / `herd_domesticated` (the penned case: the pair with NO Range
    row and no ⚠). **Both stock frames assert the UNIT, not just that a pair rendered** — the fixture's
    1480 biomass ÷ its `body_mass` 100 is 15 animals, so a row that silently kept counting biomass fails
    both halves. The fixture's `body_mass` is pinned to its own `food_per_animal` by the sim's identity
    `food_per_animal = body_mass × provisions_per_biomass`, or it would assert against a herd that could
    not exist; map_preview `map_pasture_herd_range` (the gold graze ring over the Pasture overlay) /
    `map_predator_prey_sense` (a selected Grey Wolf Pack drawing the wide radius-4 ORANGE prey-sense ring —
    a 61-tile disk — beside a herbivore deer; NOT the small gold graze ring).
- **Clear-all / move-band** (`Hud.gd`, Early-Game Labor slice 3b): the single-task
  Scout/Cancel affordance + its optimistic `_pending_transition_bands` machinery were
  **retired** with the labor-allocation model. There is no longer a band-global task to
  cancel — you staff a source down to 0 (`assign_labor … 0`). The **Clear all** button on
  `%AllocationPanel` emits `cancel_order_requested`; `Main._on_hud_cancel_order` sends the
  **repurposed** `cancel_order <faction> <band_bits>` (now clears ALL assignments → fully
  idle). **Move band** is the one remaining targeting flow, and the whole targeting cluster now
  lives in `ui/hud/TargetingController.gd` (held as `_targeting`): the drawer's **Move** button
  connects straight to `TargetingController.begin_move_band`, which enters tile-targeting
  (`_pending_move_band` → `_current_targeting_info` returns `command: "move", need: "tile"`), the
  top-centre banner reads "MOVE … click a destination tile", and the destination click
  (`_try_dispatch_pending_move_band`, driven by HudLayer's `show_tile_selection` / `notify_hex_selected`
  → `_targeting.try_dispatch`) emits the controller's `move_band_requested(payload)` (relayed onto the
  HudLayer signal) → `Main._on_hud_move_band` → `move_band <faction> <band> <x> <y>`. Esc/right-click
  cancel via HudLayer's `cancel_active_targeting` delegator → `_targeting.cancel_active_targeting`.
- **Herd husbandry readout** (`Hud.gd` `_herd_summary_lines`): when a herd's
  `domestication` (snapshot `HerdTelemetryState.domestication`, 0–1) is above 0, a
  **Husbandry** row shows "Domesticating N%" while it's being tamed and "🐄 Domesticated"
  (SIGNAL tint via `_husbandry_value_hex`) once fully domesticated. This is the **per-source** half
  of the two-meter split — THIS herd's own meter (see "The Intensification Ladder" below). Progress
  builds while a band works the herd under the **`Tame`** policy. **There is no health gate**: the
  ecology phase stopped pacing the build when `docs/plan_harvest_floor.md` §3.2 replaced that cliff
  with a rate, so a crew pulling hard on a herd tames it *slowly* rather than not at all, in
  proportion to the floor it holds. What genuinely halts the meter is a crew with **nothing standing
  above its own floor** — no escapement room, so no work — which is what `_tame_stalled_hint`
  surfaces. **NOT under Sustain**, and there is
  no `domesticate` command: both were retired by the ladder arc (`docs/plan_intensification_ladder.md`
  §4.1) — taming as a hidden Sustain side effect, with a visible-but-disabled `Corral` beside it, is
  the exact UX problem that arc exists to fix. See `core_sim` Fauna & Wild Game — Domestication /
  husbandry.
- **Herd staffing — the under-kept deficit made VISIBLE, and the ROW that carried it is retired**
  (`DetailFormat.herd_summary_lines`; snapshot `HerdTelemetryState.herdersNeeded` /
  `upkeepWorkersNeeded` / `herdedFraction` → decoded in `native/src/dict/subsistence.rs` as
  `herders_needed` (int) / `upkeep_workers_needed` (int) / `herded_fraction` (float)). A managed herd
  needs keepers every turn to HOLD it; unkept it **sheds whole animals over its labor capacity into a
  nearby WILD herd** — the animals *drift off*, tameness is never decayed (it leaves with them), and a
  fully-abandoned herd bleeds out and despawns (fauna neglect-escape arc,
  `docs/plan_fauna_neglect_escape.md`; supersedes the retired tameness-decay model).
  - **THE `Keepers:` ROW IS GONE** (issue #545). It stated a standing DEMAND in hands on every managed
    herd, every turn, beside a `Keeping:` row saying the same number again as a rate — and reported
    from play neither could be read. `KEEPERS_ROW`, `HERDERS_STAFFED_FORMAT`, `HERDERS_UNDER_FORMAT`,
    `herders_label` and `herders_value_hex` went with it; `selection-card.md` → "RETIRED — `Keepers:`
    and `Keeping:`" is the autopsy.
  - **WHAT SURVIVES IS THE ALARM, AND IT SAYS THE HEAD COUNT WHEN IT FIRES.** On
    `SourceForecast.is_under_kept` — the published `upkeepShortfall` behind no build in flight — the
    Husbandry row itself takes the ⚠ (`DetailFormat.rung_row_value`'s built-and-short fork), the
    `At risk:` row prices the shortfall and its grace, and a full-width WARN sentence names the
    consequence and the lever: **`⚠ Under-herded — animals are drifting off. This herd wants N of the
    band's Husbandry hands.`** (`HERDERS_SHED_FORMAT`, gated additionally on `domestication > 0`). A
    head count only matters when it is short, which is exactly when this line renders.
  - **THE SENTENCE LEADS WITH THE HAZARD MARK, and that is what makes it AMBER.** `detail_bbcode`'s
    full-width WARN branch tested one known sentence by equality, so this line — the only one in the
    client that says animals are drifting off — rendered in the muted `INK_DIM` a descriptive line
    gets. The branch keys on `HudSelectionVocab.RUNG_HAZARD_GLYPH` now.
  - **`herdersNeeded` IS A DEMAND, NEVER A PAIR** (`docs/plan_standing_upkeep.md` §2.5). It was `A / N`
    whose `A` counted keepers assigned to this herd — the HUNTERS before the crews split, then the
    per-source `maintain` crew — and **maintenance has since left the tile**: a managed herd is held
    out of its band's `husbandry` POOL, so no per-herd crew exists to count and one derived from the
    pool share would be a head count the sim never published. `round(herded_fraction · needed)` remains
    forbidden for its own older reason: it reads last turn's RESOLVED fraction.
  - **A PART-BUILT RUNG NOBODY IS BUILDING SHEDS TOO, and it is a different reading.** The sim's shed
    reads `upkeepShortfall` and does not care which crew failed to pay it, so a Tame the player walked
    away from bleeds while `upkeepWorkersNeeded` answers the BUILD crew's question. `is_under_kept` is
    false there (a build is in flight), so the Husbandry row states `⚠ ∞ turns` — the build's own
    verdict — with the `At risk:` row's cost and countdown beneath it, and the work board carries the
    matching ⚠. The field-by-field read of what the wire publishes is in `band-city-panel.md` → "…AND A
    PART-BUILT RUNG NOBODY IS BUILDING GETS THE SAME ⚠".
  ui_preview `herd_fully_herded` (the covered herd, whose whole claim is now the SILENCE: its rung row
  renders and is bare, with no mark, no bill and no risk row — `herded_fraction` a stale 0.4) /
  `herd_under_herded` (the mark on the Husbandry row, the drifting-off line and the `At risk:`
  countdown, `herded_fraction` a stale 1.0) / **`herd_keeping_mid_build`** (a herd mid-Tame whose build
  IS being paid, asserted as saying nothing at all, against its unpaid twin beside it). **The first two
  fixtures are fully TAMED on purpose**: a part-tamed herd owes its keeping to the BUILD crew, so a
  positive keeper demand on one is a shape no server can produce.
  The **worker/assignment panel flags it too** (`BandPanelController._work_source_models` hunt branch),
  through the same `is_under_kept` call: the established overhunt ⚠ (amber marks + severity stripe +
  the `⚠` attention filter chip) and the `WORK_ROW_UNDER_HERDED_NOTE` ("Animals drifting off — raise
  this band's Husbandry role.") in its inspector strip, so the shed reads WHEREVER the herd is listed,
  not only in its drawer — see `band-city-panel.md` → "The under-herded ⚠ reads the POOL's share".
  band_panel_preview `band_panel_under_herded` / `band_panel_keepers_short` /
  `band_panel_keepers_staffed`.
- **Per-species husbandry ceiling — gate the ladder by species** (Grazing 2d-δ,
  `docs/plan_grazing_2d.md` §4a; snapshot `HerdTelemetryState.husbandryCeiling` → `husbandry_ceiling`,
  decoded in `native/src/lib.rs herds_to_array` beside `ecology_phase`). Not every animal climbs the
  whole ladder — the string says how far this species can go: **`"wild"`** hunt-only, **`"pastoral"`**
  tameable + roams but never pennable, **`"pen"`** (or **empty/absent** ⇒ treated as pen) the full
  ladder. `SourceForecast.husbandry_ceiling(herd)` normalizes it (unknown → `"pen"`, so an un-tagged herd behaves
  exactly as before the field shipped). Two gates, both keyed off it:
  - **Herd drawer** (`_herd_summary_lines`): `"wild"` shows **no** husbandry track at all (no
    domestication / corral / pen rows), just the dim `Wild game — hunt only` hint; `"pastoral"` keeps
    the domestication (Husbandry) row but replaces the whole corral/pen readout with the dim `Herdable,
    not pennable` hint; `"pen"` renders the full ladder. The hints are colon-free, so
    `DetailFormat.detail_bbcode` renders them as dim informational sentences.
  - **Assign controls** (`_build_herd_assign_controls`): the **Corral** rung is withheld for any
    non-`"pen"` species — an OUTRIGHT hide, not a greyed "learn Herding" gate, because penning is
    *impossible* for the species, not merely unlearned. **It is no longer a filter on the policy
    picker (#442)**: the picker offers the four STANCES unconditionally, the build verbs left it for
    the improvement control, and the husbandry ceiling now decides `admits` inside
    `RungGates._next_rung` — the predicate that chooses the ONE rung on offer. Changing
    husbandry-ceiling behaviour means editing that; there is no `.filter` on the picker to find, and
    `HUNT_POLICY_OPTIONS` is deleted. The Extend-pen action is implicitly gated (it only shows on a
    `corralled` herd, which is pen-ceiling by construction).
  ui_preview: `herd_ceiling_wild` (hunt-only, no husbandry track + hint, no Corral policy) /
  `herd_ceiling_pastoral` (domestication kept, "Herdable, not pennable", no Corral policy) —
  the existing `herd_*` states carry no ceiling → the unchanged pen path.
- **Herd corral readout** (`Hud.gd` `_herd_summary_lines`): when a herd's `corralled`
  (snapshot `HerdTelemetryState.corralled`, decoded beside `domestication` in
  `native/src/lib.rs herds_to_array`) is true, a **Corral** row shows "🐄 Corralled"
  (SIGNAL tint). The herd end of the intensification ladder — a penned, domesticated herd.
  While the pen is still being built under the Corral policy (`corralProgress`, decoded as
  `corral_progress`; `0 < p < 1`) the SAME row reports the meter — the animal twin of the tile card's
  Cultivation row. See the Cultivate/Corral investment-rung bullet under **Labor allocation UI**, and
  "The build meter says WORK" below for what a meter row states now.
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`; snapshot
  `HerdTelemetryState.penFedFraction` → `pen_fed_fraction`): a penned herd eats the grass its fenced
  footprint grows plus the fodder its keeper carries in, and **an underfed herd shrinks**. ONE row
  carries that, in `herd_summary_lines` — the **`Fed:`** row below, which leads with the ⚠ and takes
  the DANGER tint whenever `PenStatus.is_starving(pen_fed_fraction)` (`pen_feed_value` /
  `pen_feed_value_hex`, one tint path, no parallel styling).

  **THE CORRAL ROW STATES THE RUNG AND NOTHING ELSE.** It wore that starving face for a while
  (`PEN_STARVING_LABEL`, retired) — and a BUILT rung row renders `<label> <meter %>`, so the row came
  out as **`Corral: ⚠ Starving — 47% fed 100%`**: how FED the herd is beside how BUILT its pen is, two
  unrelated percentages with nothing between them. `corral_built_label` takes no `fed_fraction` and
  returns the badge alone; `corral_value_hex`'s *"if the value contains 'starving', paint it red"*
  special case is DELETED with the label that produced that string, so the Corral row is an ordinary
  rung row tinted by the shared `rung_value_hex` rule. Feed is the `Fed:` row's whole job — the mark,
  the fraction, the split and the remedy.

  **THERE IS NO `Pen feed` COST ROW, and there must not be one again.** A `−1.74 /turn` WARN-amber row
  stated the pen's food demand beside that badge, reading `pen_upkeep` (later `pen_larder_bill`). Both
  fields are retired `(deprecated)` wire slots: **human food is not animal feed**. A pen is fed by land
  and by fodder, so a shortfall has no price to quote — it has a CONSEQUENCE, and the marked `Fed:` row
  is it. The band's food ledger lost its matching `🐄 Pen feed (animals)` row for the same reason
  (`band-readouts.md`), so there is no per-herd figure and no per-band one, and nothing to add together.
  ui_preview: `herd_domesticated` (fed) / `herd_corral_starving` (47% fed).
  **The map flags it too** (`MapView._draw_herd` → `_draw_distress_badge`): a starving pen's marker
  gets a DANGER **ring** (under the glyph) plus a filled DANGER **badge with a hand-drawn "!"** (over
  it). Both are **drawn geometry, never a tint or a font glyph** — a herd marker is a full-color
  **emoji**, so `modulate` merely yields a slightly-darker brown animal (tried, rendered, reverted),
  and a font ⚠ carries emoji presentation and blobs at marker size (the hazard that forced
  `MagnifierButton` + the line-art policy icons to hand-draw). map_preview: `map_herd_starving` — a
  starving pen beside a **fed** one, which is the A/B the tint failed and the badge passes.
  **And the turn orb** surfaces it as the `starving_pen` attention producer — see the orb bullet.
- **The pen is fenced LAND — the pen-economy surface** (Grazing 2d-γ, `docs/plan_grazing_2d.md` §7;
  snapshot `HerdTelemetryState.penRadius` / `penFootprintTiles` / `penPastureFraction` /
  `penExtendProgress` → `pen_radius` / `pen_footprint_tiles` / `pen_pasture_fraction` /
  `pen_extend_progress`, decoded in `native/src/dict/subsistence.rs`). A penned herd grazes its own
  fenced footprint, and that grass is one of the only TWO things that feed it. Three surfaces:
  - **Herd drawer** (`herd_summary_lines`, corralled branch): a **`Pen: radius R · N tiles`** footprint
    row — `pen_footprint_tiles` is the SERVER's in-bounds count, shown **verbatim** (the closed-form
    hex-disk count is wrong at map edges) — and the **`Fed:`** row, which carries the whole feed story
    in FOUR states (`DetailFormat.pen_feed_value`):

    ```
    Fed:  100% — all pasture
    Fed:  100% — 88% pasture · 12% fodder
    Fed:  ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn
    Fed:  ⚠ 40% — 40% pasture · no fodder · needs 12.0 /turn
    ```

    **THE ROW IS NAMED FOR WHAT IT MEASURES, NOT FOR ONE OF ITS TERMS.** It read `Fed by pasture` and
    then listed the OTHER feed source in its value — one source named in the label, the other beside
    it. The headline is `pen_fed_fraction`, how much of its demand the pen actually got, and the terms
    under it say where that came from.
    - **`NN% pasture`** — `pen_pasture_fraction` × 100, the share its own fenced footprint grazed.
    - **`NN% fodder`** — **`fed − pasture`**, the SUBTRACTION of two published shares of the same
      demand, taken on the rounded percentages so the two terms visibly add to the headline and
      clamped at zero (`pen_fed_fraction` is clamped at 1.0, so the pair can round-cross).
    - **`no fodder`** in that term's place when `fodder_draw` is under
      `SourceForecast.FODDER_FLOW_MIN`: nothing was carried in — no store, or no Foddering — which is
      a different fact from a short ration. The draw's MAGNITUDE is never printed; this presence test
      is all it decides.
    - **`all pasture`** when the land covers the pen outright (`pen_pasture_fraction` at 1.0 within
      `PenStatus.FED_EPSILON`): no second term, and by construction no shortfall.
    - **`needs N more/turn`** — `pen_fodder_shortfall`, the sim's own `max(0, penHayNeed − fodderDraw)`
      (`HerdTelemetryState.penFodderShortfall`, decoded in `native/src/dict/subsistence.rs` beside
      `pen_hay_need`), shown only at or above `FODDER_FLOW_MIN`: **a fed pen owes nothing and must not
      read `needs 0.0`**. The `more` is dropped on the `no fodder` state — there is no "more" than
      nothing — so that pen reads `needs N /turn`.
    - **The ⚠ LEADS the value** on `PenStatus.is_starving` (the existing test — the map badge's and the
      turn orb's), and `pen_feed_value_hex` reads that prefix to put the row in DANGER ink. That is the
      tint the Corral row's retired `contains("starving")` case used to carry.

    **THE WORD IS `fodder`, NEVER `hay`.** Fodder is the category — food for livestock, dried hay or
    straw among it — hay is one instance of it, and the band's own store row already says `Fodder`. The
    `compact` tier's merged clause on the band Food line took the same sweep
    (`BandDetailLines.BAND_FOOD_FODDER_CLAUSE_FORMAT`, `band-readouts.md`).

    **THE GROSS DEMAND IS STILL NOT ON THE WIRE, so the row may not quote one.**
    `penPastureFraction` and `penFedFraction` are ratios, `fodderDraw` an absolute, and `penHayNeed` /
    `penFodderShortfall` are GAPS — the gross those shares are shares OF is published nowhere. So the
    fodder term is a subtraction of two SHARES and **never `fodder_draw` divided by a ratio**; a
    readout like *"fodder 0.9 of 2.2"* is not expressible and synthesizing the total is the one thing
    this row must not do. If a future readout genuinely needs the gross, that is a new snapshot field,
    i.e. server-side work. The retired third FEED term, a NET food-larder bill (`pen_larder_bill`, with
    `pen_hay_food` as its food-equivalent twin), went with the model correction above.

    **`penHayNeed` IS STILL DECODED AND IS NO LONGER RENDERED HERE.** It is the gap the LAND leaves —
    what the pen needs grown for it whether or not any arrives — while the row states what would fix
    the pen NOW, which is the shortfall. The band-level roll-up is the cohort's `fodder_need`
    (`band-readouts.md` → "The band's FODDER LEDGER"), which the SIM sums over its pens' `penHayNeed`;
    the two are one fact at two scales, and `band_hay_and_pen` is the frame that carries both at once.

    **ALL FOUR STATES ARE ASSERTED IN ONE BLOCK, over four line-sets** (`ui_preview`,
    `chapters/herd_graze_pen.gd`): what tells them apart is which optional terms are present, so each
    is the others' control and a state checked in a frame of its own leaves the defect living in the
    gap between frames — the shape that has hidden three defects in this arc. The needles are the
    WHOLE value, so a dangling `·`, a doubled `%`, a lost em-dash or a term in the wrong order fails
    there where no PNG could. Beside them: the fodder share pinned as the SUBTRACTION (on a fixture
    where a division answers differently), `needs` present on both starving pens and absent on both fed
    ones, `more` on the drawing pen alone, `hay` and `larder` on none of the four — and **the Corral
    row pinned separately on both starving pens** as the plain `Corral: 🐄 Corralled 100%` badge that
    does not contain the word "starving", which is the whole regression risk of collapsing
    `corral_built_label` down to the badge.

    **The starving fixture is ONE coherent set of numbers**: a 21.3-a-turn demand, 40% grazed, 1.49
    carried in (7%), 12.79 owed by the land and 11.30 still owed after the draw. Its shares and its
    draw are deliberately set apart so a division-based fodder share answers 12% where the subtraction
    answers 7% — an assertion both arithmetics satisfied would be vacuous.
  - **Extend affordance** (`_build_extend_pen_control`, in the herd `%HerdAssignControls`): on a built
    pen with no ring in flight (`pen_extend_progress == 0`) a **`Fencers` stepper** over an
    **"Extend pen"** button, emitting `extend_pen_requested{faction,x,y,workers}` →
    `Main._on_hud_extend_pen` → **`extend_pen <faction> <x> <y>`** at the pen anchor (a penned
    herd sits AT `corralled_at`, so its own tile). **The command names no crew** since
    `docs/plan_standing_upkeep.md` §2.5 — it queues the ring, and the band's `builders` pool raises it
    when it reaches the head.
    **THE RING GAINED A CREW IN §2.2 AND LOST IT AGAIN IN §2.5.** It rides the same `animal:pen` rung
    as the pen it widens, so it cannot be the one build in the game that is free — but what it costs is
    the band's `builders` POOL, not a crew named on the verb. `extend_pen <faction> <x> <y>` is closed
    at three tokens (a fourth is a parse error), so the stepper, its `idleWorkers` clamp and the
    disabled-at-zero button are all gone and the control is a plain button again. `_pen_extend_crew` is
    retired with them; there was never a composition for it to be part of, and now there is no count.
    **What the pen is waiting on is the QUEUE**, which the button's tooltip says and no control here
    can move. While a ring is being fenced
    (`pen_extend_progress > 0`) the button is replaced by a WARN-amber **"Fencing N%"** badge — the pen
    twin of the corral-build "Building N%" meter. The server rejects an extend at max radius / unowned /
    Herding-unknown with a feed message; the client does not pre-gate (max radius is not on the wire).
  - **Map footprint highlight** (`BandOverlayRenderer.draw_pen_footprint_highlight`, drawn under the herd markers
    when a corralled herd is selected): the fenced hex disk of radius `pen_radius` around the pen anchor,
    in a distinct **enclosure-green** tint (`PEN_FOOTPRINT_FILL`/`_OUTLINE`) — deliberately NOT the gold
    of the roam-range ring, so a fenced footprint reads as a different thing. Reuses the range ring's
    wrapped-column / `_hex_distance` / `_fill_hex` / `_outline_hex` primitives (bounds-clamped by the
    loop). A corralled herd draws no roam-range, so exactly one of the two ever renders.
  ui_preview: `herd_pen_self_feeding` (radius 2 · 19 tiles, `Fed: 100% — all pasture`, Extend-pen
  button) / `herd_pen_extending` (mid-extension → "Fencing 60%" badge) / `herd_pen_foddered`
  (`Fed: 100% — 88% pasture · 12% fodder`) / `herd_pen_no_fodder` (the `no fodder` state, appended
  last in its chapter) / `herd_domesticated` (radius 1 · 7 tiles, `Fed: 100% — 0% pasture · 100%
  fodder` — the barren footprint fodder carries outright); map_preview:
  `map_pasture_pen_footprint` (the green footprint disc, the A/B against `map_pasture_herd_range`'s
  gold roam-range).
