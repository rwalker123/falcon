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
    zero**: a herd holding a fifth of a body counts one, as the sim's own kill step does
    (`min(affordable, max(1, carryable))`). **The ecology phase RIDES this row** rather than standing as
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
- **Herd staffing / "Keepers" row — the under-kept deficit made VISIBLE** (`DetailFormat.herd_summary_lines`;
  snapshot `HerdTelemetryState.herdersNeeded` / `upkeepWorkersNeeded` / `herdedFraction` → decoded in
  `native/src/dict/subsistence.rs` as `herders_needed` (int) / `upkeep_workers_needed` (int) /
  `herded_fraction` (float)). A managed herd needs keepers every turn to HOLD it; unkept it **sheds
  whole animals over its labor capacity into a nearby WILD herd** — the animals *drift off*, tameness
  is never decayed (it leaves with them), and a fully-abandoned herd bleeds out and despawns (fauna
  neglect-escape arc, `docs/plan_fauna_neglect_escape.md`; supersedes the retired tameness-decay
  model). Immediately after the Husbandry row, ONLY when `herders_needed > 0` (0 = wild/unmanaged, so
  `find_world_herd` reports 0 and it never trips), a **Keepers** row shows a calm
  `N — drawn from the band's Husbandry` (neutral ink) or an amber
  `N — under-herded, the Husbandry pool is short here` (WARN, `herders_value_hex`, the shared
  overgrazing/pen-debit path).
  - **IT STATES ONE NUMBER, AND IT IS A DEMAND** (`docs/plan_standing_upkeep.md` §2.5). It was a pair
    `A / N` whose `A` counted the keepers assigned to this herd — the HUNTERS before the crews split,
    then the per-source `maintain` crew — and **maintenance has since left the tile**: a managed herd
    is held out of its band's `husbandry` POOL, so no per-herd crew exists to count and one derived
    from the pool share would be a head count the sim never published. `assigned_keepers_for` and the
    parameter that threaded it in are both gone. `round(herded_fraction · needed)` remains forbidden
    for its own older reason: it reads last turn's RESOLVED fraction.
  - **THE ROW'S PRESENCE AND ITS ALARM ASK DIFFERENT QUESTIONS.** It is SHOWN on `herders_needed > 0`,
    the ownership gate — what this herd will owe — and goes amber only on
    `SourceForecast.is_under_kept`, which tests the published `upkeepShortfall` behind a positive
    `upkeepWorkersNeeded`. Those differ mid-build: while a Tame or a Corral is going up the keeping is
    the build crew's and the sim publishes `0` keepers wanted, so a herd mid-Tame reads a calm
    `Keepers: 4` beside a `Keeping:` row saying the rung is still being built, rather than a warning
    about a shed that is not happening.
  - When under-kept AND `domestication > 0`, a muted consequence line — **`Under-herded — animals are
    drifting off. This herd wants N of the band's Husbandry hands.`** (`HERDERS_SHED_FORMAT`; the
    "tameness slipping" copy is retired, and so are both later ones — "Staff all N herders" named the
    crew that cannot stop it, "Staff N KEEPERS" named a control that no longer exists) — states the
    shed and the one lever that does.
  ui_preview `herd_fully_herded` (calm `4 — drawn from the band's Husbandry`, `herded_fraction` a
  stale 0.4) / `herd_under_herded` (amber, + the drifting-off line, `herded_fraction` a stale 1.0) /
  **`herd_keeping_mid_build`** (the third claim, and the one a pooled readout gets wrong: a herd
  mid-Tame is billed a non-zero upkeep no pool covers, so the `Keeping:` row must say it is being
  BUILT and must not quote the pool). **The first two fixtures are fully TAMED on purpose**: a
  part-tamed herd owes its keeping to the BUILD crew, so a positive keeper demand on one is a shape
  no server can produce.
  The **worker/assignment panel flags it too** (`BandPanelController._work_source_models` hunt branch),
  through the same `is_under_kept` call: the established overhunt ⚠ (amber marks + severity stripe +
  the `⚠` attention filter chip) and the `WORK_ROW_UNDER_HERDED_NOTE` ("Animals drifting off — raise
  this band's Husbandry role.") in its inspector strip, so the shed reads WHEREVER the herd is listed,
  not only in its drawer — see `band-city-panel.md` → "The under-herded ⚠ reads the POOL's share".
  band_panel_preview `band_panel_under_herded` / `band_panel_keepers_short` /
  `band_panel_keepers_staffed`.
- **…and the OTHER shed a managed herd can suffer: a part-built rung nobody is building.** A Tame the
  player walked away from is owed its BUILD crew, so `upkeepWorkersNeeded` is `0`, the `Keepers` row
  reads a calm demand, and the herd sheds anyway — the sim's shed reads `upkeepShortfall` and does
  not care which crew failed to pay it. The drawer's `Keeping:` row is what states it
  (`DetailFormat.UPKEEP_UNBUILT_VALUE` — *"nobody is building it — this rung is sliding back"*, in
  place of the *"still being built — its own crew holds it"* that is true only of a build being
  worked), with the
  `At risk:` row's cost and countdown beside it, and the work board carries the matching ⚠. The rule
  and the field-by-field read of what the wire publishes here are in `band-city-panel.md` → "…AND A
  PART-BUILT RUNG NOBODY IS BUILDING GETS THE SAME ⚠".
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
  `HerdTelemetryState.penUpkeep` / `penFedFraction` → `pen_upkeep` / `pen_fed_fraction`): a penned
  herd cannot graze, so its keeper hauls it food every turn, and **an underfed herd shrinks**. Two
  rows carry that, both in `_herd_summary_lines`:
  - the **Corral** row flips from the "🐄 Corralled" badge to a DANGER-tinted **"⚠ Starving — 40%
    fed"** whenever `PenStatus.is_starving(pen_fed_fraction)` (`_corral_label` / `_corral_value_hex`,
    one tint path, no parallel styling);
  - a **Pen feed** row (only on a penned herd) states the demand — `−1.74 /turn`, WARN amber as a
    standing debit — and, when the keeper came up short, what was actually paid: `−1.74 /turn — only
    40% paid`, DANGER (`_pen_feed_label` / `_pen_feed_value_hex`).
  `pen_upkeep` is this HERD's demand; the band's ledger row is the sim-summed `pen_feed_upkeep`
  across all its pens — the two are never added together, and the client sums neither.
  ui_preview: `herd_domesticated` (fed) / `herd_corral_starving` (40% fed).
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
  `pen_extend_progress`, decoded in `native/src/lib.rs herds_to_array`). A penned herd grazes its own
  fenced footprint and the grass it eats **offsets** the larder bill (`pen_upkeep` is now that offset).
  Three surfaces:
  - **Herd drawer** (`_herd_summary_lines`, corralled branch): a **`Pen: radius R · N tiles`** footprint
    row — `pen_footprint_tiles` is the SERVER's in-bounds count, shown **verbatim** (the closed-form
    hex-disk count is wrong at map edges) — and a **`Fed by pasture NN% · larder N.N food/turn`** feed
    split (`pen_pasture_fraction` × 100 + `pen_upkeep`): a self-feeding pen on lush land reads "100% ·
    larder 0.0" (and the amber Pen-feed debit row disappears), a scrub pen "0% · larder 1.7". The Corral
    / Pen-feed / starving rows above are unchanged.
  - **Extend affordance** (`_build_extend_pen_control`, in the herd `%HerdAssignControls`): on a built
    pen with no ring in flight (`pen_extend_progress == 0`) a **`Fencers` stepper** over an
    **"Extend pen"** button, emitting `extend_pen_requested{faction,x,y,workers}` →
    `Main._on_hud_extend_pen` → **`extend_pen <faction> <x> <y> <workers>`** at the pen anchor (a penned
    herd sits AT `corralled_at`, so its own tile).
    **THE RING GAINED A CREW** (`docs/plan_standing_upkeep.md` §2.2): it rides the same `animal:pen`
    rung as the pen it widens, so it cannot be the one build in the game that is free — it staffs the
    same BUILD allocation and draws on the same finite band. The stepper clamps to the band's published
    `idleWorkers` (the sim REFUSES a crew a band cannot staff rather than trimming it), and the button
    is **disabled at a crew of zero**: the sim would accept `0` and simply never work the ring off, so
    the control states the requirement instead of sending a command that does nothing. The count is
    held on `DrawerComposeController._pen_extend_crew` rather than on `ComposeState` — extend-pen is a
    DRAWER action and never enters a compose sheet, so there is no composition for it to be part of.
    While a ring is being fenced
    (`pen_extend_progress > 0`) the button is replaced by a WARN-amber **"Fencing N%"** badge — the pen
    twin of the corral-build "Building N%" meter. The server rejects an extend at max radius / unowned /
    Herding-unknown with a feed message; the client does not pre-gate (max radius is not on the wire).
  - **Map footprint highlight** (`BandOverlayRenderer.draw_pen_footprint_highlight`, drawn under the herd markers
    when a corralled herd is selected): the fenced hex disk of radius `pen_radius` around the pen anchor,
    in a distinct **enclosure-green** tint (`PEN_FOOTPRINT_FILL`/`_OUTLINE`) — deliberately NOT the gold
    of the roam-range ring, so a fenced footprint reads as a different thing. Reuses the range ring's
    wrapped-column / `_hex_distance` / `_fill_hex` / `_outline_hex` primitives (bounds-clamped by the
    loop). A corralled herd draws no roam-range, so exactly one of the two ever renders.
  ui_preview: `herd_pen_self_feeding` (radius 2 · 19 tiles, 100% · larder 0.0, Extend-pen button) /
  `herd_pen_extending` (mid-extension → "Fencing 60%" badge) / `herd_domesticated` (radius 1 · 7 tiles,
  0% · larder 1.7); map_preview: `map_pasture_pen_footprint` (the green footprint disc, the A/B against
  `map_pasture_herd_range`'s gold roam-range).
