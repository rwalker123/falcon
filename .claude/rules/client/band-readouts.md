---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{BandDetailLines,TopBarReadouts,DetailFormat}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{BandFoodStatus,TileHabitability,TileClimate}.gd"
---

<!-- Extracted verbatim from lines 191-191;193-193;2958-3192 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Band readouts — demographics, food, morale, wellbeing, tile facts

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/BandDetailLines.gd` | `RefCounted` producer (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the **STATEFUL band/party detail-line producers** — the rows a BAND or a PARTY shows in whichever detail surface hosts it: `unit_summary_lines(unit, terrain_label, ctx)` (Food · Fodder · Morale · Output · Position · the accessible-stockpile rows, registering the Food/Morale disclosures through `DisclosureController` as it emits them) and `expedition_summary_lines(unit, ctx)` (Mission · Target + its live `(x, y)` · Policy · Phase · Carried/Provisions · Next delivery · Position), plus the private row builders `_band_food_line` / `_band_morale_line` / `_band_output_line` / `_morale_breakdown_lines` / `_accessible_stockpile_lines`. **It is the stateful HALF of a three-way split**: the PURE producers became `DetailFormat` statics (`herd_summary_lines`, the expedition tooltip trio) and `_format_stockpile_label` became `HudFormat.stockpile_label`. Hud holds it as `_banddetail`, constructed in `_ready` AFTER `_disclosures` and BEFORE `_bandpanel`; **both detail hosts share the one instance** — the Occupants-card drawer (`Hud._render_occupant_drawer`) and `BandPanelController`'s vitals label + parties inspector strip, which is what retired three of that controller's nine Callable injections. **THE INJECTION SURFACE IS ONE CALLABLE** — `_herd_label_for_id`, which cannot fold onto `HudBandLaborState` because it reads THREE collaborators (`_selectioncard.find_roster_herd` AND `_selection.herd()` AND `_band_labor.find_world_herd`); `_is_player_unit` is a trivial private COPY (the `SelectionCardController` / `BandPanelController` precedent). **IT NEVER SEES THE SELECTION MODEL**: the old producers read `_selection` at exactly two sites, both `tile_info()["terrain_label"]` for the morale row's "it's the hex you're on" payload, so that ONE display string is now a `terrain_label` PARAMETER and both hosts resolve it through the new `SelectionCardController.selected_terrain_label()`. It also owns `_food_flow_present`, which is a **private handshake between `_band_food_line` (writer) and `unit_summary_lines` (its only reader)** — the formatter has never seen it, so it is deliberately not on the `DetailFormat.Context`. Consts follow the `DetailFormat` rule (a const lives here iff every reader moved here): the Fodder/FULL-badge/morale-arrow/contribution-label/stockpile-row vocabulary came, the disclosure `DETAIL_ROW_*` / `BREAKDOWN_KIND_*` protocol vocabulary and `MORALE_CAUSE_*` stayed on `HudLayer` and are read back as `HudLayer.X` |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale warn/critical thresholds + `color_for_morale`/`hex_for_morale`), shared by MapView's band dot and Hud's food/morale lines + alerts |
- **Demographics readout** (`Hud.gd` `update_demographics`, dispatched from `Main.gd`): the player
  faction's age structure from `PopulationDemographicsState` (snapshot `demographics[]`) shows as a
  top-bar line (`Pop 100  👶34 🛠51 🧓15  dep 96/100`, `DemographicsLabel` in `TurnBlock`) — total
  head-count, the three brackets, and the **dependency ratio** `(children+elders)/working` per 100
  workers, tinted amber when dependents outnumber workers / cyan on a healthy labor surplus. Hidden
  until the faction has population. See `core_sim` Campaign Loop — Population & Demographics.
- **Wondrous Sites (discovered)** (snapshot `discovered_sites[]`, per-faction like
  `sedentarization`/`demographics`; each entry `{faction, sites:[{x,y,site_id,category,display_name,
  glyph}]}` with `category`/`display_name`/`glyph` resolved server-side — client renders the provided
  glyph/name, no client-side site config; undiscovered sites are never sent). Decoded in
  `native/src/lib.rs discovered_sites_to_array` into both the full-snapshot and delta dicts under
  `discovered_sites`. Surfaced three ways, all filtered to `PLAYER_FACTION_ID`:
  (1) **Top-bar readout** (`Hud.gd update_discoveries`, dispatched from `Main.gd`): a compact
  `◈ Discoveries N` line followed by a **strip of one mark per distinct site KIND**
  (`DiscoveriesRow` in `TurnBlock` — a `Label` for the text + a sibling `DiscoveriesStrip` HBox for the
  marks; the row hides/shows as one unit, cyan), hidden when 0. **THE TWO NUMBERS MEAN DIFFERENT THINGS
  AND ARE BOTH RIGHT:** `N` is `sites.size()`, the count of INSTANCES found (a site's identity is its
  tile `(x, y)`); the strip shows KINDS, so three peaks read `N = 3` behind one peak mark. Never
  "reconcile" them to a unique count.
  **KEYED ON `site_id`, LIKE THE MAP RENDERER** — this was the last consumer in the client still keying
  site presentation on the `glyph` string, and it had both failure modes that choice implies: two
  distinct site types sharing one glyph (the fixture's `sky_arch` reuses `great_peak`'s ⛰) **collapsed
  into a single strip entry** while the count beside it stayed right — the strip silently disagreeing
  with its own number — and a catalog row with an empty glyph **vanished from the strip entirely**.
  Dedupe is now on `site_id` (the sim's stable catalog key from `sites_config.json`), falling back to
  the `(x,y)` pair ONLY when `site_id` is empty, so a catalog-pruned site still appears once rather
  than disappearing. Presentation resolves in strict precedence: **`WonderSprites.for_site_id`** →
  the same bundled sprite the map marker draws (reused, never a second art table) · non-empty `glyph`
  → the server's emoji as text · neither → the named `DISCOVERIES_UNKNOWN_GLYPH` (`◇`) fallback, because
  **a site the player has FOUND must never render as blank space**. Sprite `TextureRect`s are boxed to
  the label's **derived** `get_theme_font_size` (never a hardcoded pixel size) and drawn
  aspect-preserved, so the strip keeps the text baseline it had when every entry was an emoji. Strip
  children are rebuilt and freed each snapshot (this runs per update — do not leak nodes). Verify on
  ui_preview `discoveries.png`, whose fixture is built to prove exactly the two cases the glyph could
  not distinguish: the `great_peak`/`sky_arch` glyph collision must render TWO marks (sprite + emoji),
  and a repeated `great_peak` instance must lift the count without adding a mark.
  (2) **Map markers** (`MapView.gd`): ingested into `discovered_sites` + a `discovered_site_lookup`
  (`Vector2i → site`) mirroring `food_modules`; `SecondaryMarkerRenderer.draw_discovered_site` draws the
  site's **bundled sprite where we have art for its `site_id`** (`WonderSprites` — see its row above; the
  sprite is resolved BEFORE the `glyph == ""` guard) and the server's `glyph` (drop-shadow, no backing disc)
  otherwise — and unlike the fauna/food tables that emoji path is **live**, since the site catalog is
  data-driven and can outgrow the art. Either way in a fixed **edge slot** via the shared secondary-marker system (see Map markers below),
  gated on `_visibility_state_at != "unexplored"` (persists on any known/remembered tile — Discovered OR
  Active — since a site is permanent geographic knowledge, unlike the Active-only food-site/herd markers).
  (3) **Tile card** (`Hud._tile_terrain_lines`): a `Site: <display_name>` row (from `_tile_info_at`'s
  `discovered_site_lookup` cross-ref → `site_name`), shown before the FoW discovered early-return since
  it's known knowledge. The server also pushes a `SiteDiscovered` command-feed entry, which renders
  generically via the server-provided `kind`/`label` (no client kind→label map needed). See
  `core_sim` — Wondrous Sites.
- **Band food status** (snapshot `PopulationCohortState.turnsOfFood` / `activity` / `supplyNetworkId` /
  `stores[]`, decoded in `native/src/lib.rs` `population_to_dict` as `turns_of_food` / `activity` /
  `supply_network_id` / `stores{item:qty}`).
  > **THE FIELD IS A COUNT OF TURNS, AND ITS NAME NOW SAYS SO** (it was `daysOfFood`; renamed
  > end-to-end — wire, decoder, config key `food_turns.{warn,critical}`, and every client helper).
  > It is the
  > **larder runway — turns until the larder empties, WITH income counted** — resolved sim-side by
  > walking the per-source `arrivalSchedule`s (falling back to `larder / net_drain`, and to the
  > `999` not-food-limited sentinel when the band is net-positive). It used to be
  > `larder / consumption`, i.e. *"how long if you stop hunting"*, which read badly pessimistic for
  > any band with real income and visibly contradicted the FOOD OUTLOOK chart beneath it. Because
  > it walks the same schedules the chart does, the header and the chart now agree. **This game
  > counts turns, never days** — `DetailFormat.food_turns_text` is the single place the unit is
  > spelled, and it spells it from the shared `DetailFormat.FOOD_RUNWAY_UNIT` const. **The Food-row
  > threshold tint in `DetailFormat.detail_bbcode` keys on that SAME const** (both now live in that
  > one file, precisely so they cannot drift), because it recognizes the row by finding the
  > unit word in the rendered value — the guard tested a bare `"day"` literal after the renderer had
  > moved to turns, so a starving band's Food row rendered in neutral ink and only the `∞` case
  > tinted. Never spell the unit at either site with a literal. One consequence to keep in mind: the runway assumes income *holds*, so a
  > band whose income nearly covers its drain reports a very long runway and reads green until that
  > income lapses. The old figure warned earlier by assuming the worst.
  The green/amber/red warn·critical thresholds and the
  runway→color mapping live in one place, `ui/BandFoodStatus.gd` (config `src/config/band_status_config.json`,
  key `food_turns.{warn,critical}`; `999` = not food-limited → ∞). Surfaced three ways:
  (1) `MapView._draw_band_status` draws a food-runway dot on each **player** band
  (`_is_player_unit`); (2) `Hud._band_food_line` adds a `Food  <N>  (<D> turns)`
  row to the band selection panel, tinted by the thresholds via `DetailFormat.detail_bbcode`
  — **player bands only** (`_is_player_unit`, the same gate Morale uses, and for the same
  reason: **a rival's larder is not ours to see**). A foreign cohort carries no
  `turns_of_food`/`stores` on the wire, so rendering the row for one **fabricated knowledge**
  — a healthy-green `Food 0 (∞)`, the UI claiming we'd counted a larder we cannot observe.
  A foreign band's drawer now shows only what is honestly observable from outside: its
  **Position**, plus the name/size on its roster row. The reset of the disclosure context
  (`_food_flow_present` / `_selected_band_food_turns` / `_disclosure_state`) lives at the top
  of `_unit_summary_lines`, NOT inside `_band_food_line` — the skipped call must not leave the
  previous render's caret or food-runway tint behind;
  (3) `MapView._draw_supply_links` faint-chains player bands sharing a `supply_network_id` (`0` = solo).
  **Band food flow on the Food line** (snapshot `PopulationCohortState.foodIncome`/`foodConsumption`/
  **`penFeedUpkeep`**, decoded as `food_income`/`food_consumption`/`pen_feed_upkeep`, flowed onto the
  MapView unit marker + guarded by `marker_field_guard`): for a **player** band with real flow,
  `_band_food_line` appends the **steady net per-turn rate** — `Food 15 (19 turns) · +0.76 /turn` —
  where **net = `DetailFormat.band_net_food` = income − food_consumption − pen_feed_upkeep − raid_forfeit**, tinted green (≥0) /
  red (<0). **The income term is the fix:** `_band_food_income = Gathered + Hunted = Σ per-source
  `realized_yield`** (the honest long-run average of the lumpy take, client-summed from the same values
  as the breakdown rows), so the net **no longer swings turn-to-turn** the way the old lumpy
  `food_income`-based net did (0 on a hunt's wait turn, a spike on its kill turn). It is summed from the
  breakdown rows rather than off any band-level wire total, so the net's income half can never disagree
  with the Gathered/Hunted rows beneath it. (A cohort-level `foodIncomeAverage` was added for exactly
  this and then **retired as redundant** — a separately-computed total is a second source of truth that
  can drift from the rows. Don't reintroduce it; the sum IS the contract.) **The ledger has FOUR terms, not two:**
  a band keeping a corral pays its penned herd's feed straight off the larder every turn (a confined
  herd cannot graze), and that debit is in *neither* of the other two. Omitting it made the row **lie** —
  a Red Deer pen overstated the surplus by ~1.74/turn against a band that eats ~1.2, and the larder then
  drained with no explanation. **The fourth term is `raid_forfeit`** (Predators Phase 3,
  `PopulationCohortState.raidForfeit`): food a predator raided off the larder THIS turn, the raid twin of
  pen feed — same larder, a different decision (guard the camp vs feed the herd). Like pen feed the client
  **must not** re-derive it; unlike pen feed raids are **EPISODIC**, so this term is present only the turn a
  raid lands and the forward FOOD OUTLOOK chart deliberately does NOT project it (a past loss is not a
  steady drain). The full identity `larder_delta == income − consumption − pen_feed − raid_forfeit` is
  pinned by `integration_tests/tests/raid_food_ledger.rs`.
  `penFeedUpkeep` is the food the sim **actually paid** this turn summed across every pen the band
  keeps; the client **must not** re-derive it by summing the herds' `penUpkeep` (the sim owns every
  yield number — see `core_sim/CLAUDE.md` → Pre-commit Yield Forecast; the identity
  `larder_delta == income − consumption − pen_feed` is pinned by `integration_tests/tests/pen_food_ledger.rs`).
  The turns-to-empty stays only in the `(N turns)` figure; it is not
  repeated. The `Food` label is a **click-to-open disclosure** (a `▸/▾` caret) opening a
  **category breakdown** in a **POPOVER** — indented `▲ +X  Gathered` / `▲ +Y  Hunted` / `▼ −Z  Eaten
  (people)` / `▼ −W  🐄 Pen feed (animals)` / `▼ −V  ⚔ Lost to raids` rows (Gathered/Hunted = Σ per-source `actual_yield`
  by kind, Eaten = `food_consumption`, Pen feed = `pen_feed_upkeep`, shown only when a pen is kept;
  **Lost to raids = `raid_forfeit`, shown only the turn a raid landed** (`DisclosureController.food_breakdown_lines` /
  `DetailFormat.FOOD_LABEL_RAID_FORFEIT`, the crossed-swords glyph matching the `predator_raid` command-feed
  alert) — **people, animals and raiders all draw the same larder but are DIFFERENT decisions**, so they are different
  rows), rendered through the **shared morale-breakdown path** in `DetailFormat.detail_bbcode` (income ▲
  green, debits ▼ amber). ui_preview: `band_pen_feed` (fed pen: net +2.99 = 5.88 − 1.15 − 1.74) /
  `band_pen_starving` (part-paid feed, net −0.53 red) / `predator_band_raided` (raided band: the
  `⚔ Lost to raids −1.20` row + the crimson Warrior "⚠ Predator nearby" alert). No flow → the bare `Food N (N turns)` line,
  no net/disclosure.
  **THE BREAKDOWN OPENS IN A POPOVER, NEVER INLINE — and that is a correctness rule, not a style
  one.** Expanding it in place grew the vitals `RichTextLabel` (`fit_content = true`) by several
  lines AFTER `BandPanelController.build_band_zone` had already picked its height tier from
  `_zone_box().y`; the Band panel's zone box is FIXED by design and its hosts `clip_contents`, so
  the extra lines silently sliced the WORKFORCE key row mid-glyph and ate BOTH role cards. A Window
  cannot change a zone's height — the same reason the section `⋯` menus are `MenuButton`s and the
  destructive confirms are `ConfirmationDialog`s. (The work board's budgeted inline inspector strip
  is the other idiom and does NOT apply: in the SHORT tier the chart is already dropped and the role
  cards already hint-less, so there is nothing left to spend but PEOPLE/WORKFORCE — the content.)
  The popover is a `PopupPanel` styled through `HudStyle.card_stylebox()`, anchored under the vitals
  label, dismissed by clicking away / Esc / clicking the caret again; the caret flips `▸`→`▾` only
  while THAT row's popover is up.
  **The auto-show-when-concerning rule is GONE and is now a TINT** — a popover that popped itself
  open on a snapshot would be worse than the clipping it replaced, so `DetailFormat.food_is_concerning`
  (net-negative OR runway below warn, mirroring `DetailFormat.morale_is_concerning`) instead renders the row's
  caret in **WARN** rather than SIGNAL: the invitation to read the breakdown stays visible without
  anything opening itself. `band_panel_food_concerning_*` is that frame.
  **The Food + Morale rows share ONE disclosure mechanism, and it is now the SAME in both hosts** —
  `_register_disclosure` (stashes the rows into `_breakdown_payloads`, keyed `"<kind>:<entity>"`,
  and records the caret) / the `DisclosureController` meta dispatch / popover. The `[url]` meta IS
  that key, so the handler needs no band lookup and **the old `is_panel` fork is gone**: it existed
  only to route the inline re-render, and one click behaviour needs no routing. The label + click are
  wired on BOTH the Occupants-card drawer's `%OccupantDetail` and the dockable Band/City panel's
  per-render vitals label, each binding ITSELF as the popover's anchor.
- **Band morale readout** (snapshot `PopulationCohortState.morale`, decoded in `native/src/lib.rs`
  `population_to_dict` as `morale`, a 0–1 float on each cohort dict; flowed into the MapView unit marker
  in `_rebuild_unit_markers`): a band can shrink while well-fed when a harsh tile erodes morale until
  births fall below elder mortality. `BandFoodStatus.gd` owns the morale thresholds too (config key
  `morale.{warn,critical}` = `0.40`/`0.25`, just above the ~0.20 birth floor) and the mirrored
  `color_for_morale`/`hex_for_morale` helpers (same green/amber/red palette, but a plain scalar — no
  "unlimited" sentinel). `Hud._band_morale_line` adds a `Morale: <N>%` row to the drawer **for player
  bands only** (`_is_player_unit`), tinted by `hex_for_morale` via `DetailFormat.detail_bbcode` (same
  stash-then-tint pattern as the Food row, using `_selected_band_morale`).
- **Morale trend + named cause** (snapshot `PopulationCohortState.moraleDelta` / `moraleCause`, decoded in
  `native/src/lib.rs` `population_to_dict` as `morale_delta` (raw Scalar/1e6, signed) / `morale_cause`
  (int; `0=None,1=Terrain,2=Cold,3=Unrest`), flowed into the MapView unit marker): "low morale" named the
  symptom, not the cause — the morale drivers live server-side and were discarded each turn until the
  cohort started exporting the per-turn trend + dominant negative driver. `Hud._band_morale_line` appends
  a trend arrow (`▼` falling / `▲` rising / none when `|morale_delta| < MORALE_TREND_EPSILON`) and, when
  falling, the plain-language cause via `_morale_cause_label` — `Terrain`→"harsh terrain", `Cold`→"harsh
  climate" (the server penalty fires on hot **or** cold deviation, so not literally "cold"),
  `Unrest`→"unrest". `Terrain` appends the band's `_selected_tile_info.terrain_label` in parens
  (`Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)`) — the "it's the hex you're on" payload. A
  band that has not ticked yet reports `morale_delta 0 / cause None` and the row degrades to a bare
  percentage. **That is not a rollback case** — the checkpoint clones `PopulationCohort` whole, so a
  restored band keeps its delta and cause; the sentinel answers for a cohort no turn has resolved.
- **Civilization Wellbeing — productivity, itemized morale, recovery** (see
  `docs/plan_civ_wellbeing.md`; snapshot `PopulationCohortState.outputMultiplier` /
  `discontentFraction` / `lastEmigrated` / `lastImmigrated` / `grievance` + the four signed
  Layer-1 contributions `moraleSettling` / `moraleTerrain` / `moraleClimate` / `moraleUnrest`,
  decoded in `native/src/lib.rs population_to_dict` as `output_multiplier` / `discontent_fraction`
  / `last_emigrated` / `last_immigrated` / `grievance` (telemetry only, not displayed in P1) /
  `morale_settling` / `morale_terrain` / `morale_climate` / `morale_unrest`, all flowed onto the
  MapView unit marker in `_rebuild_unit_markers`). Player-band drawer only (`_unit_summary_lines`):
  - **Output row** (`_band_output_line`): `Output: N%` shown when `output_multiplier < OUTPUT_FULL`
    (1.0), placed just under Morale. Tinted ink → amber → red by `BandFoodStatus.hex_for_output`
    (config `band_status_config.json` `output.{warn,critical}` = `0.85`/`0.60`; near-full reads
    neutral ink, *not* green — it's a productivity note, not a "good"). Ties productivity to morale.
  - **Itemized morale breakdown** (`_morale_breakdown_lines`): the four signed contributions
    (their sum IS `morale_delta`) as indented sub-lines (e.g. `    ▲ +1.0%  settling`). Only
    contributions above `BandFoodStatus.morale_breakdown_epsilon()` (config `morale.breakdown_epsilon`
    = `0.002`) list. Labels: `settling`, `harsh terrain (<terrain_label>)` (matches the headline cause
    treatment), `harsh climate`, and `unrest`/`culture` by sign. `DetailFormat.detail_bbcode` tints each
    row two-tone by its sign glyph (▲ = HEALTHY green, ▼ = WARN amber — deliberately not a rainbow);
    the indented breakdown lines are intercepted before the KV split. The **Morale row is a
    click-to-open disclosure identical to Food, opening in the SAME popover** (the `▸/▾` caret +
    `meta_clicked` share `DisclosureController.register` / its meta dispatch / its popover,
    keyed `"morale:<entity>"`). Like Food it no longer auto-expands: `DetailFormat.morale_is_concerning` (below
    warn **or** falling past `MORALE_TREND_EPSILON`) tints the caret WARN instead. The contributions
    always compute so the good state can be opened too; the disclosure is offered only when there's
    actually something to show (a contribution above epsilon, or the concerning recovery line) —
    `_register_disclosure` declines an empty payload and no caret renders.
  - **Recovery guidance** (`RECOVERY_GUIDANCE_TEXT`): a dim `↑ Recover: move to Hospitable ground ·
    Scout · Hunt` line (the real levers, NOT harvest), appended under the breakdown **only when
    morale is concerning** (a healthy band that manually expands its breakdown is not told to
    "recover"). `_split_detail_kv` skips lines beginning with `↑` so it renders as a dim sentence.
  - **Growth row + itemized fertility breakdown** (`_band_growth_line` / `_fertility_breakdown_lines`;
    snapshot `PopulationCohortState.fertilityHunger`/`fertilityReserve`/`fertilityTrend`, decoded in
    `native/src/dict/population.rs cohort_scalars` as `fertility_hunger`/`fertility_reserve`/
    `fertility_trend`, flowed onto the MapView unit marker + guarded by `marker_field_guard`). Growth
    used to slow for reasons the player could not see itemized: they had the *inputs* (the larder, the
    Food line) and the *effect* (the People bar), and nothing between them. This is the exact parallel
    of the morale breakdown above — same click-to-open disclosure, same popover, same
    `DisclosureController.register` path — for the birth path's three named factors
    (`docs/plan_population_growth_model.md`).
    - **`Growth: 188% of normal`** — the band's birth rate as a share of the base rate the sim would
      otherwise apply, i.e. the PRODUCT `fertility_hunger × fertility_reserve × fertility_trend`
      (`DetailFormat.band_fertility`). Tinted by `BandFoodStatus.hex_for_fertility` (config
      `band_status_config.json` `fertility.{warn,critical}` = `0.75`/`0.40`), which grades ink → amber
      → red like the **Output** row rather than the morale/food green palette: normal growth is normal,
      not a "good", so the top bucket is neutral ink even at 188%. Unlike Output the row shows at
      EVERY level, because it is what the disclosure hangs on and "why is growth slow?" has to be
      findable in the good state too. **It can exceed 100%**, which is why the value spells its anchor
      out rather than leaving a bare percentage to read as a cap.
    - **The breakdown rows are MULTIPLIERS, not signed deltas** — `    ▼ ×0.60  short rations` /
      `    ▲ ×1.05  larder reserve` / `    ▼ ×0.25  larder shrinking`. They reuse the morale
      breakdown's indent + ▲/▼ sign glyph so `DetailFormat.detail_bbcode`'s shared indented-sub-line
      branch tints them (no parallel styling path), but these factors combine by PRODUCT where the
      morale contributions combine by SUM: three signed percentages that refuse to add up to the
      headline would invite exactly the arithmetic they cannot support, whereas `0.60 × 1.05 × 0.25`
      reads down to the `16%` above it. `hunger` is only ever ≤ 1 and `reserve` only ever ≥ 1, so each
      of those labels states its one direction outright; `trend` is two-sided and forks on sign
      (`larder growing` / `larder shrinking`) the way the morale row's culture/unrest does. Only
      factors off the neutral 1.0 by more than `fertility.breakdown_epsilon` (`0.002`) list, so a
      thriving band's disclosure names what is HELPING rather than showing no-op rows.
    - **NO DATA IS NOT A FAMINE, and the sentinel is a ZERO RESERVE.** The factors are derived per
      turn, so a cohort no turn has resolved publishes all zeros — **not** a restored one, which
      keeps them, since the checkpoint clones the cohort whole. `BandFoodStatus.fertility_is_projected`
      reads that off `fertility_reserve` (a computed reserve is `1 + bonus × ramp` ≥ 1 by
      construction, while `hunger` and `trend` both legitimately reach 0) and the producer emits **no
      Growth row and no disclosure at all** rather than a fabricated `0% of normal`. `MapView`
      deliberately defaults the three marker keys to **`0.0`, not the neutral `1.0`**, for the same
      reason — a neutral default would fabricate a "normal growth" reading for a band that published
      none. The sim's own no-data rule (an unprojected `trend` scores neutral) then falls out on this
      side for free: a neutral factor renders as nothing, never as a deficit.
    - ui_preview: `band_growth_expanded` (188% neutral ink, disclosure naming the two helping factors,
      `hunger` neutral so its row is omitted) / `band_growth_collapsed` (16% red under a WARN caret,
      all three factors off neutral — the frame that proves the rows multiply out to the headline) /
      `band_growth_unprojected` (a band no turn has resolved yet: NO Growth row). band_panel_preview:
      `band_panel_morale_expanded_*` carries the collapsed Growth row in the dock host.
  - **Action morale hints**: the Scout button tooltip (`MORALE_HINT_SCOUT`, "(+morale)") and the four
    persistent Hunt/Follow policy tooltips (Sustain/Surplus/Deplete/Eradicate get `MORALE_HINT_PERSISTENT`
    appended, "(+morale/turn)") advertise the positive levers; the one-shot Single policy does not.
- **Tile-card Habitability** (snapshot `TileState.habitability`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `habitability` (raw Scalar/1e6; band-independent per-turn morale drain of the tile's
  terrain + temperature, ≥0, bigger = harsher), stored in `MapView.tile_habitability` keyed by
  `Vector2i` and copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a
  `Habitability: <rating>` row (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present). `ui/TileHabitability.gd` is the single
  source of truth — config `src/config/tile_habitability_config.json` (`habitability.{hospitable_max,
  fair_max,harsh_max}` = `0.02`/`0.05`/`0.09`) buckets the drain into Hospitable/Fair/Harsh/Hostile,
  tinted HEALTHY/INK/WARN/DANGER via `hex_for_rating` in `DetailFormat.detail_bbcode` (mirrors the
  `BandFoodStatus` bucketing pattern). The Karst Cavern Mouth (~0.0825) reads "Harsh" (amber).
  With the latitude climate + cold-morale tolerance dead-band (see `core_sim`), temperate
  mid-latitudes read "Hospitable", the equator "Hospitable/Fair", and poles/high-alt/caverns
  "Harsh/Hostile" — the config buckets (`0.02`/`0.05`/`0.09`) spread cleanly across that range,
  so no re-tune was needed.
- **Tile-card Climate** (snapshot `TileState.temperature`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `temperature` (°); temperature is now a **latitude + elevation** climate
  (equator-in-the-middle, poles cold) with a small element jitter, NOT the old element
  checkerboard — see `core_sim`), stored in `MapView.tile_temperature` keyed by `Vector2i` and
  copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a `Climate: <band>` row
  next to Habitability (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present so rehydrated tiles degrade
  gracefully). **The band CUT POINTS are the SIM's, not the client's** (Climate Authority arc,
  `docs/plan_climate_authority.md`): the sim derives a tile's BIOME from a temperature-based
  `ClimateBand` and PUBLISHES the cut points per-map in the snapshot (`MapSection.climateBands` →
  the native surfaces `overlays.climate_{polar,boreal,temperate}_max_temp`, °C — mirroring the
  `elevation_sea_level` precedent). `MapView._ingest_overlay_channels` adopts them via
  `TileClimate.set_cut_points(...)` (presence-based like the sea level; a per-map constant that
  persists across deltas). `ui/TileClimate.gd` is the single source of truth for the LABELS and the
  classification, which mirrors `climate::climate_band_for_temperature` (`core_sim/src/climate.rs`)
  EXACTLY — inclusive upper bounds, a tile AT a cut point sits in the colder band: `temp <=
  polar_max → Polar`, `<= boreal_max → Boreal`, `<= temperate_max → Temperate`, else `Tropical`.
  The **client's own `cool_min` (3.0) threshold is RETIRED** — it could show a biome and a climate
  that disagree, the exact defect this arc removes; `tile_climate_config.json` is emptied and no
  longer read (its whole 5-band `tropical_min/warm_min/temperate_min/cool_min` scheme is gone). The
  four band names mirror the sim's own vocabulary (Polar/Boreal/Temperate/Tropical) so the label can
  never drift from the band the sim decided. **Fallback:** until the sim publishes cut points (older
  sim / table absent — a bug, not a supported case), `TileClimate.has_bands()` is false and
  `Hud._tile_terrain_lines` SKIPS the Climate row rather than inventing a threshold (`band_for`
  returns `BAND_UNKNOWN "—"`). The row is **informational** — neutral ink, no HEALTHY/WARN/DANGER
  tint, so it doesn't overload the Habitability row's warning semantics.
