---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{BandDetailLines,FactionReadouts,DetailFormat}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{BandFoodStatus,TileHabitability,TileClimate,TileSurvivability}.gd"
---

<!-- Extracted verbatim from lines 191-191;193-193;2958-3192 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Band readouts — demographics, food, morale, wellbeing, tile facts


## THE SHORT BAND-ZONE TIER MERGES ROWS RATHER THAN DROPPING THEM

A horizontal (T/B) dock's band zone is height-capped and **CLIPS** rather than scrolling, while having
a whole screen of width. `BandDetailLines.unit_summary_lines`' `compact` flag is that tier saying so,
and it now buys three rows in three different ways — the differences are the rule:

| row | what the SHORT tier does | why |
|---|---|---|
| `Fodder` | **MERGED** onto Food as a fodder clause | that larder has no other home in the client |
| `Growth` | **MERGED** onto Morale as a clause | the fertility breakdown has no other home either |
| `Upkeep` | **KEPT, at every tier** | a standing bill in goods is stated nowhere else on this panel |

**A fourth row, `Trade`, was the one this tier DROPPED**, on the reasoning that its rate was still
stated by the WORK zone's head — the whole reason a drop was affordable there and nowhere else. Arc
#527 retired the account and the row with it, so the tier drops nothing today; the rule survives it,
and a future row with another home on the panel is the one that yields.

**A THIRD ROW IS WHAT FORCED THE THIRD MERGE, AND IT IS NOT THE ROW IT WAS.** The band zone was
measured at 299 of its 300px box, so one more 26px vitals row put it 25px over in 13 states, and
dropping a row was not available: `Trade` was already the one this tier dropped (and has since been
retired outright). The row that could not be dropped was `Gear` when the merge was made; that row is
retired now and the `Upkeep:` standing bill took its height, so the arithmetic is unchanged and the
un-droppable row is the material bill — see "The `Gear` row is retired from BOTH pages" at the foot
of this file. A spent kit is stated by the crafting panel's ledger and by the event dock's `kit_life`
seams; a band's standing bill in goods has no second home.

**Morale and Growth are the right pair to join.** Both are player-band health scalars, both already
carry disclosure carets, and they read naturally together.

**BOTH `[url]` METAS SURVIVE, which is why a merge beats a drop.** The vitals block is ONE
`RichTextLabel`, so a row is a line and merging two is joining two strings: the Growth clause carries
the identical clickable run a standalone row wears (`DetailFormat.inline_disclosure_label`, which
delegates to the same `_key_cell`), on the same label — so both popovers keep working. The clause
carries its OWN tint rather than inheriting the morale value cell's, exactly as the fodder clause does.

**The carets have to be read from the CONTROLLER, not from the context, and that is a real trap.**
Every other disclosure is drawn by `detail_bbcode` from a context this producer fills on its LAST
line, so a clause built mid-producer sees an empty `disclosures` and silently falls back to the plain
word — losing the caret and the click with it (measured: the merged line rendered `Growth 188%`).
`_band_growth_clause` assigns `ctx.disclosures = _disclosures.state()` before building the run.

### The width trap, and what pays for it

The vitals label is `AUTOWRAP_WORD`. **A merged line that does not fit WRAPS, and costs back the very
row the merge saved** — a fix that measures as no fix, with nothing failing: a wrapped line still sits
inside the zone rect, so the bounds assertion passes and the frame is silently one row taller. So:

- **the morale CAUSE clause is dropped at SHORT.** `— harsh terrain (Karst Cavern Mouth)` is the
  longest run this row can carry; with it the merged line measures 500px in a 380px column and the
  zone goes 22px over. The trend GLYPH stays, so the row still says morale is falling, and the cause is
  recoverable from the popover this row's own caret opens.
- **Growth drops its `of normal` anchor at SHORT** (`DetailFormat.GROWTH_VALUE_SHORT_FORMAT`). The
  anchor is what makes a standalone `150%` legible; beside a `%` morale reading on the same line the
  bare percentage is unambiguous, and the disclosure restates the factors in full.

Measured after: the merged Morale+Growth run is **251px of a 380px column** and the zone's worst case
is back to **299px of its 300px box**. `band_panel_preview._assert_merged_morale_growth_fits` is what
holds that — the Food row's twin, and the only assertion that can see a wrap.

**TALL and COMPACT keep both rows and the cause clause**, asserted by
`_assert_growth_row_not_merged` at each: structurally, off the BBCode, since `detail_bbcode` opens
every row with `[cell]` and a merged clause is preceded by the clause separator instead
(`DetailFormat.DISCLOSURE_URL_OPEN` is that needle). The COMPACT probe is PNG-less — the tier needs a
435-515px canvas and this band's COMPACT content overruns that box by ~143px whatever the vitals do,
which is a property of the tier and not of the merge.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/BandDetailLines.gd` | `RefCounted` producer (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the **STATEFUL band/party detail-line producers** — the rows a BAND or a PARTY shows in whichever detail surface hosts it: `unit_summary_lines(unit, terrain_label, ctx, compact, with_position)` (Food · Fodder — on EVERY player band, live or dormant — · **Upkeep**, the standing MATERIAL bill, on a band that holds something which eats a good and NOWHERE else · Morale · Growth · Position, registering the Food/Morale/Growth disclosures through `DisclosureController` as it emits them; the **Trade** row and its disclosure were retired with the account by arc #527) and `expedition_summary_lines(unit, ctx)` (Mission · Target + its live `(x, y)` · **Orders** · Phase · Carried/Provisions · Next delivery · the trip-bound clause · Position — the **Orders** row being the floor alone since issue #491 retired the fill target it was merged with, still ONE row via `DetailFormat.expedition_orders_line` because this producer's output lands in a `clip_contents` strip capped at ~300px; see `band-city-panel.md` → "The parties strip's SEVEN lines"), plus the private row builders `_band_food_line` / **`_band_material_upkeep_line`** (the good in the WORST state and its runway, registering a fifth `Upkeep` disclosure whose popover states every good — see "THE STANDING MATERIAL BILL" at the foot of this file; **`_band_kit_line` and its `Gear` row are RETIRED** with `BAND_KIT_ROW_*` and the 22px `Zone_band` measurement their entry budget respected) / `_band_morale_line` / `_morale_breakdown_lines` and the DORMANT twin `_band_fodder_dormant_line` (the shared gate itself moved to `DetailFormat.band_has_fodder_economy` when the faction rollup started asking it). **The two trailing flags are DIFFERENT QUESTIONS and must not be folded together**: `compact` is the band zone's HEIGHT TIER (it merges Fodder onto the Food line and Growth onto Morale), while `with_position` is the host saying whether it states the band's coordinates somewhere ELSE — the Band/City dock does, in its panel header, in every tier. **There is no `_band_output_line`**: productivity reads on the WORK zone's head now (see the Civilization Wellbeing bullet below). **It is the stateful HALF of a three-way split**: the PURE producers became `DetailFormat` statics (`herd_summary_lines`, the expedition tooltip trio). (`_format_stockpile_label` was the third piece of that split, via `HudFormat.stockpile_label`; both it and the accessible-stockpile rows it served are retired — see the accessible-stockpile note further down this file.) Hud holds it as `_banddetail`, constructed in `_ready` AFTER `_disclosures` and BEFORE `_bandpanel`; **both detail hosts share the one instance** — the Occupants-card drawer (`Hud._render_occupant_drawer`) and `BandPanelController`'s vitals label + parties inspector strip, which is what retired three of that controller's nine Callable injections. **THE INJECTION SURFACE IS ONE CALLABLE** — `_herd_label_for_id`, which cannot fold onto `HudBandLaborState` because it reads THREE collaborators (`_selectioncard.find_roster_herd` AND `_selection.herd()` AND `_band_labor.find_world_herd`); `_is_player_unit` is a trivial private COPY (the `SelectionCardController` / `BandPanelController` precedent). **IT NEVER SEES THE SELECTION MODEL**: the old producers read `_selection` at exactly two sites, both `tile_info()["terrain_label"]` for the morale row's "it's the hex you're on" payload, so that ONE display string is now a `terrain_label` PARAMETER and both hosts resolve it through the new `SelectionCardController.selected_terrain_label()`. It also owns `_food_flow_present`, which is a **private handshake between `_band_food_line` (writer) and `unit_summary_lines` (its only reader)** — the formatter has never seen it, so it is deliberately not on the `DetailFormat.Context`. Consts follow the `DetailFormat` rule (a const lives here iff every reader moved here): the Fodder/FULL-badge/morale-arrow/contribution-label vocabulary came (the stockpile-row vocabulary went with those rows). The disclosure `DETAIL_ROW_*` / `BREAKDOWN_KIND_*` protocol vocabulary lives in `hud_disclosure_vocab.gd` and `MORALE_CAUSE_*` in `DetailFormat.gd` — read back as `HudDisclosureVocab.X` / `DetailFormat.X`, NOT as `HudLayer.X`; `Hud.gd` defines none of them |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale and output warn/critical thresholds; morale carries the `color_for_morale`/`hex_for_morale` pair because it really has both a `Label` host and a BBCode host, while **output carries `color_for_output` ALONE** — its one surface is the WORK zone head, which is `Label`s), shared by MapView's band dot and Hud's food/morale lines + alerts |
| `ui/TileSurvivability.gd` | Single source of truth for the sim's TEMPERATURE-MORTALITY model — the range outside which `systems::population` kills, food or no food. **TWO INDEPENDENT TAILS**, each with its own onset, slope and ceiling: `set_model(cold_onset, cold_scale, cold_max, heat_onset, heat_scale, heat_max)` adopts the constants the sim publishes per-run (`MapSection.temperatureSurvivability` → the native's `overlays.survivability_{cold,heat}_{onset_temp,mortality_scale,max_mortality}`, all six or none), pushed from `MapView._ingest_overlay_channels` on the same presence test the climate cut points use. `has_model()` gates every readout — no published model, no survivability claim. `survivable_min()`/`survivable_max()` are the two ONSETS themselves, so the survivable band is the interval `[cold_onset, heat_onset]` and **not** a deviation from an ambient; `death_rate(temp)` mirrors `active_temperature_tail` + `temperature_fraction` in `core_sim/src/systems/population.rs` (below the cold onset priced by the cold tail, above the heat onset by the heat tail, zero between, each capped by its own ceiling), with `is_lethal` / `is_cold` reading off it. **It is the TILE's base rate**: the sim's per-bracket vulnerabilities are applied after the cap and are deliberately not published, a tile not knowing who stands on it. Consumed by `SelectionCardController._tile_chip_descriptors` (the CLIMATE chip's ⚠, tint and hover — the warning has no chip of its own since the four-pill strip was merged down) and by `MapView._draw_temperature_lethality` / `_build_temperature_legend` (the map overlay's hatch, contour and Lethal row) — one authority, so the card and the map cannot disagree about which ground kills. **It answers about the MODEL, never about how a rate is printed:** the `<0.1%` floor that keeps an unprintably small rate off a rounded zero lives in `HudSelectionVocab`, not here |
- **RETIRED — the demographics readout, and the wire section with no client reader.** The player
  faction's age structure (`PopulationDemographicsState`, snapshot `demographics[]`) rendered as the
  top-bar line `Pop 100  👶34 🛠51 🧓15`, and issue #450 deleted it along with the whole top-right
  block — **`Hud.update_demographics`, the `FactionReadouts` ingest and `Main`'s dispatch are all
  gone**, so the section joins `accessibleStockpile` as a wire table the client no longer reads.
  **The faction page's PEOPLE bar is what replaced it, and it is a better answer to the same
  question**: it sums the BANDS' own whole brackets (`FactionRollup._build_people_block`), so the
  head count cannot disagree with the bands it is made of — where a per-faction total beside it
  would have been a second source of truth.
  The **dependency ratio** had already left this line before that (a faction average hides the band
  that is in trouble — see the PEOPLE block in `band-city-panel.md`), which left it stating a
  composition the bar draws. See `core_sim` Campaign Loop — Population & Demographics.
- **Wondrous Sites (discovered)** (snapshot `discovered_sites[]`, per-faction like
  `sedentarization`/`demographics`; each entry `{faction, sites:[{x,y,site_id,category,display_name,
  glyph}]}` with `category`/`display_name`/`glyph` resolved server-side — client renders the provided
  glyph/name, no client-side site config; undiscovered sites are never sent). Decoded in
  `native/src/lib.rs discovered_sites_to_array` into both the full-snapshot and delta dicts under
  `discovered_sites`. Surfaced three ways, all filtered to `PLAYER_FACTION_ID`:
  (1) **The faction page's KNOWLEDGE zone** (`FactionRollup._build_discoveries_block`), reading the
  cache `Hud.update_discoveries` fills. **It was a top-bar readout** — a compact `◈ Discoveries N`
  line plus a strip of one mark per distinct site KIND, `DiscoveriesRow` in `TurnBlock` — until issue
  #450 retired that block; the INGEST and its player-faction filter stayed exactly where they were, so
  only the rendering moved. **THE TWO NUMBERS MEAN DIFFERENT THINGS AND ARE BOTH RIGHT:** `N` is
  `sites.size()`, the count of INSTANCES found (a site's identity is its tile `(x, y)`), while the
  KINDS are what get one entry each — so three peaks read `N = 3` behind one peak. Never "reconcile"
  them to a unique count. **The zone states both in full** — the head counts instances, the rows count
  kinds — which is what the strip had no room to do and why the pair was regularly misread there.
  The strip's art precedence (`WonderSprites` → the server glyph → `DISCOVERIES_UNKNOWN_GLYPH`) went
  with it: the zone is a column of text rows and resolves no art. `WonderSprites` keeps its map-marker
  consumer, which is (2) below.
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
  **Band food flow on the Food line** (snapshot `PopulationCohortState.foodIncome`/`foodConsumption`,
  decoded as `food_income`/`food_consumption`, flowed onto the
  MapView unit marker + guarded by `marker_field_guard`): for a **player** band with real flow,
  `_band_food_line` appends the **steady net per-turn rate** — `Food 15 (19 turns) · +0.76 /turn` —
  where **net = `DetailFormat.band_net_food` = income − food_consumption − raid_forfeit**, tinted green (≥0) /
  red (<0). **The income term is the fix:** `_band_food_income = Gathered + Hunted = Σ per-source
  `realized_yield`** (the honest long-run average of the lumpy take, client-summed from the same values
  as the breakdown rows), so the net **no longer swings turn-to-turn** the way the old lumpy
  `food_income`-based net did (0 on a hunt's wait turn, a spike on its kill turn). It is summed from the
  breakdown rows rather than off any band-level wire total, so the net's income half can never disagree
  with the Gathered/Hunted rows beneath it. (A cohort-level `foodIncomeAverage` was added for exactly
  this and then **retired as redundant** — a separately-computed total is a second source of truth that
  can drift from the rows. Don't reintroduce it; the sum IS the contract.) **The ledger has THREE terms:**
  income, what the PEOPLE eat, and `raid_forfeit`.

  **A PEN IS NOT A TERM, and the row that made it one is retired.** `penFeedUpkeep` was a fourth term —
  the food a band handed its penned herds each turn — and it was a **modelling defect**: human food is
  not animal feed. A pen eats the grass its fenced footprint grows plus the hay its keeper carries in
  (FODDER, a separate store that never converts to FOOD), and what those two leave uncovered makes the
  pen **UNDERFED** rather than billing the people. The larder draw was short-circuiting exactly that
  starvation path — a pen whose pasture failed took food out of its keepers' mouths instead of
  withering. So the wire field is `(deprecated)`, the decoder publishes no `pen_feed_upkeep` key, and
  the `🐄 Pen feed (animals)` breakdown row is gone with `DetailFormat.band_pen_feed`. **Do not
  reintroduce any of it**, and do not re-derive one client-side by summing herds.

  **`raid_forfeit`** (Predators Phase 3, `PopulationCohortState.raidForfeit`) is the ledger's only debit
  beyond consumption: food a predator raided off the larder THIS turn. The client
  **must not** re-derive it, and raids are **EPISODIC**, so this term is present only the turn a
  raid lands and the forward FOOD OUTLOOK chart deliberately does NOT project it (a past loss is not a
  steady drain). The full identity `larder_delta == income − consumption − raid_forfeit` is
  pinned by `integration_tests/tests/{pen_food_ledger,raid_food_ledger}.rs`, and asserted client-side on
  a pen-keeping band by `ui_preview`'s `band_pen_keeper` state — arithmetic AND the absence of any
  animal-feed row, since a resurrected row is invisible in a PNG.
  The turns-to-empty stays only in the `(N turns)` figure; it is not
  repeated. The `Food` label is a **click-to-open disclosure** (a `▸/▾` caret) opening a
  **category breakdown** in a **POPOVER** — indented `▲ +X  Gathered` / `▲ +Y  Hunted` / `▼ −Z  Consumed`
  / `▼ −V  ⚔ Lost to raids` rows (Gathered/Hunted = Σ per-source `actual_yield`
  by kind, Consumed = `food_consumption` — the label lost its `(people)` qualifier with the animals' row
  it contrasted against;
  **Lost to raids = `raid_forfeit`, shown only the turn a raid landed** (`DisclosureController.food_breakdown_lines` /
  `DetailFormat.FOOD_LABEL_RAID_FORFEIT`, the crossed-swords glyph matching the `predator_raid` command-feed
  alert) — **the people and the raiders draw the same larder but are DIFFERENT stories**, so they are different
  rows), rendered through the **shared morale-breakdown path** in `DetailFormat.detail_bbcode` (income ▲
  green, debits ▼ amber). ui_preview: `band_pen_keeper` (a pen-keeping band: net +4.73 = 5.88 − 1.15,
  the pen a pure credit and no answering debit anywhere) /
  `band_pen_starving` (the same pen underfed — income collapses to 1.32 with the shrinking herd, and
  that is the ledger's ONLY trace of it; the alarm lives on the herd drawer) / `predator_band_raided` (raided band: the
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
- **RETIRED — the band's TRADE row** (issue #381, retired by arc #527). Trade goods were the second
  product of the very sources the Food row totals, so the row stated a band's stock and its per-turn
  rate in the Food row's own shape (`Trade: 12.0 · +1.36 /turn`) with an income-only disclosure under
  it. **The sim retired the account** — it was written on every harvest and read by nothing, while a
  `credit_material_yield` beside every credit site already accounted the same take's concrete
  materials — so the row, its breakdown, `DetailFormat.band_trade_stock` / `sum_realized_trade` /
  `band_trade_income` / `band_has_trade_flow`, `BandDetailLines._band_trade_line` +
  `BAND_TRADE_ROW_FORMAT`, `DisclosureController.trade_breakdown_lines`,
  `HudDisclosureVocab.BREAKDOWN_KIND_TRADE` / `DETAIL_ROW_TRADE`, `HudConst.STORE_ITEM_TRADE_GOODS`
  and the three `band_panel_trade_*` frames are all gone. **Three rules it left behind are still
  live and are why this stub exists:**
  - **The one-decimal treatment on a stock row.** The Fodder row cites this row as its precedent, and
    that citation now points at a retirement — the rule stands on its own (a whole-unit larder reads
    whole, an accumulating sub-unit stock reads to one decimal).
  - **A standing row reads `+0.00`, it does not vanish.** "Absent" and "present but zero" are one
    glance apart, and the playtest report that produced this row was a reader taking the first for
    "this band cannot do that at all". Any successor account row inherits it.
  - **A DISPLAY floor is not the sim's floor.** The trade gates read `SourceForecast.has_component`
    where the Food side keeps `FOOD_FLOW_MIN`, because rates land in that gap routinely and a gate
    finer than its formatter's resolution admits exactly what it exists to stop
    (`labor-ui.md` → "The shared layer").

  **Nothing in the HUD reads `faction_inventory` any more.** The left-dock `Stockpiles` card's
  `HudLayer.update_stockpiles` (and `Main`'s dispatch to it) went with the card back in #381;
  `MapPanel.apply_update` still consumes the snapshot key for its scenario description.

- **The band's FODDER LEDGER — a ROW, or a CLAUSE on the Food line.** It **is the Food line, beat for
  beat**: a two-term summary, with the flows that move it in a click-to-open disclosure beneath.

  ```text
  Fodder: 100.0  (100 turns)
    ▲ +5.0  Grown
    ▼ -6.0  Pens
  ```

  the STOCK (`fodder_store`) and the RUNWAY (`turns_of_fodder`) on the row; the two RATES
  (`fodder_income` / `fodder_need`) behind the caret — the fodder twins of `food_income` /
  `food_consumption` / `turns_of_food`, all in FODDER units.

  - **THE RATES CAME OFF THE ROW, AND THAT IS A SHAPE FIX RATHER THAN A WIDTH ONE.** The row read
    `Fodder: 100.0 · need 6.0/turn · growing 5.0/turn · 100 turns` and **wrapped to two lines in the
    narrow drawer column** for it: it carried on ONE line what the Food row has always split between
    a summary and a pull-down, in a client whose disclosure module states at the top of its own file
    that breakdown rows are NEVER appended inline (inline growth in a fixed-height zone is what
    clipped the Band panel once already). Shortening the clauses would have been a width answer to a
    shape problem; the row is the Food row's shape now, and the pair is one click away.
  - **THE RATE PAIR IS STILL THE POINT, and the slow trap is still why.** A pen's fenced footprint
    has a FIXED carrying capacity and its herd does not, so a pen that feeds itself off the land
    today becomes fodder-dependent as the herd grows — and nothing announced that until animals began
    dying. `Grown` against `Pens` is that trap, and the popover is where it is now read.
    `DisclosureController.fodder_breakdown_lines` builds the two rows through
    `DetailFormat.fodder_breakdown_row`, which shares the food breakdown's indent, ▲/▼ and tint
    (`_breakdown_row`) and differs in ONE thing: the number is at the fodder account's own ONE
    decimal, because `+5.00 Grown` under a `100.0` stock would state one account at two precisions.
  - **AND THE `need` CLAUSE'S AMBER WENT WITH THE CLAUSE.** The runway already says a larder is
    draining — a finite, shrinking number under the shared thresholds — so a separate WARN on one
    term was a second rule for one idea, and dropping it is what stops the two larders disagreeing
    about what "worrying" looks like. What judges the fodder larder now is
    `DetailFormat.fodder_is_concerning` — **the food test on the fodder account** (draining at all, or
    a runway inside `BandFoodStatus.warn_turns()`) — and all it does is tint the row's CARET, exactly
    as Food's does. The popover never opens itself.
  - **`_band_fodder_falls_short` SURVIVED THE DELETION, with one reader left**: the `compact` tier's
    merged clause, which is the only host with no room to state the pair it is a verdict about. It
    was `_band_hay_falls_short` while the row said `hay`.
  - **THE SIM SUMS `fodder_need`, AND THE CLIENT MUST NOT.** Herd rows are fog-filtered, so a pen out
    of sight would silently drop out of a client-side total the band certainly still owes — the same
    mistake the retired `pen_feed_upkeep` was minted to avoid.
  - **THE RUNWAY IS THE FOOD LINE'S, NOT A SECOND SPELLING OF IT.** `turns_of_fodder` comes off the
    sim's own `larder_runway_turns`, **999 sentinel included**, so it renders through
    `DetailFormat.food_turns_text` and the whole value cell tints through
    `BandFoodStatus.hex_for_turns` — the same renderer, the same map, and the same `Context` →
    `_value_hex` handshake the Food row makes (`Context.fodder_turns`, reset per render beside
    `food_turns`). There is no second constant, no second branch and no second phrasing of "turns of
    buffer left" anywhere in the client.
  - **THE ROW LABEL IS THE REGISTRATION KEY.** `BAND_FODDER_ROW_FORMAT` is spelled FROM
    `HudDisclosureVocab.DETAIL_ROW_FODDER` rather than typed again, because a renamed row still
    registering under the old key loses its caret silently.
  - **EACH BREAKDOWN ROW IS OMITTED BELOW `SourceForecast.FODDER_FLOW_MIN` rather than printed as a
    zero**, so a band with no Fields shows `Pens` alone rather than `+0.0 Grown`. A larder with
    neither flow registers no disclosure at all — `register` declines an empty payload — so a caret
    never promises rows that are not behind it.
  - **The controller holds a DICTIONARY of registered rows**, keyed by row label, so this is the
    FIFTH concurrent disclosure (Food, Fodder, Upkeep, Morale, Growth) rather than a second one
    competing for a slot. `_is_concerning` gained a `BREAKDOWN_KIND_FODDER` case beside them.

  **THE ROW IS UNCONDITIONAL, AND `HAS FODDER **OR** OWES A BILL` NOW PICKS ITS FORM.** The test is
  `DetailFormat.band_has_fodder_economy` — the ONE test behind every spelling of "this band has a
  fodder larder", so no two surfaces can disagree about when one exists. It was store-only for one
  release and **that hid exactly the band that most needs the row**: pens owing hay, nothing
  stockpiled, so `fodder_store == 0` and the line that would have said *you owe 6.0 a turn and grow
  none of it* never rendered at all. (It had carried a second clause before that — *or it pays a pen
  bread bill it could offset with hay* — which went out with the pen's FOOD bill; the replacement is
  the same shape read off the right account.) The store term takes `FODDER_FLOW_MIN` rather than the
  food-scale `FOOD_FLOW_MIN` it was written with: this is a fodder quantity printed at ONE decimal,
  and the finer floor admitted a store it then rendered `Fodder: 0.0`.

  > ⛔ **THE GATE NO LONGER DECIDES WHETHER THE ROW EXISTS, and the rule it replaces was the opposite
  > one.** This file said *"a forager band with no animals never sprouts an empty Fodder line"*, and
  > that reasoning is retired: an account a player has never met is invisible on precisely the bands
  > whose player has never met it, which is where discoverability matters. Reported from play. Every
  > player band renders a `Fodder:` row now; the gate chooses between the LIVE form above and the
  > DORMANT form below. **A FOREIGN band still renders none** — that gate is `_is_player_unit` and is
  > untouched, for the reason the Food row has it: a rival's larder is not ours to count.

  **THE DORMANT ROW — `Fodder: —`, dim, with the reason on the block's hover.**
  `BandDetailLines._band_fodder_dormant_line`, `BAND_FODDER_DORMANT_ROW_FORMAT`.
  - **A DASH, NEVER A ZERO.** The live format on an empty larder renders `Fodder: 0.0  (∞)`, and a
    full-ink zero beside a healthy infinity reads as *this band has fodder and is fine* — the exact
    opposite of what the state means, and what a bare gate deletion ships. The em-dash is the glyph
    this HUD already uses for an account with no quantity to state, and the const is read from
    `HudComposeVocab.YIELD_LOCKED_GLYPH` rather than typed again (`HudFloraVocab.STOCK_UNKNOWN_GLYPH`
    is the same idea one account over).
  - **THE DIM IS A SELF-TINTED RUN INSIDE THE VALUE CELL**, the `BAND_FOOD_FODDER_CLAUSE_FORMAT`
    idiom — `_value_hex`'s Fodder case keys on the RUNWAY spelling and would leave this in neutral
    ink. The runway context stays at its `NAN` reset: writing this band's `turns_of_fodder` (999 for
    a band that drains nothing) would tint the dash HEALTHY green.
  - **NO CARET, AND NOTHING REGISTERED.** `fodder_breakdown_lines` produces no rows for a band with
    neither flow, so the dormant branch registers no disclosure at all and `_key_cell` draws a plain
    dim key. An empty pull-down is worse than no pull-down.
  - **TWO REASONS, TWO SENTENCES, because they are not the same news.** Without **Foddering** the
    band cannot bank hay at any price — the craft is a whole rung away, taught by keeping a penned
    herd — so the hover is the forage panel's own words:
    `BAND_FODDER_LOCKED_TOOLTIP_FORMAT` is spelled from `HudFloraVocab.FODDERING_NOT_LEARNED_CLAUSE`,
    the clause factored OUT of `GATE_REASON_WILD_FODDER_FORMAT` so both surfaces state one lock once
    (the patch-only remedy — *or commit this patch to a fodder crop* — stays on the gate reason, a
    band row having no patch). With Foddering learned and no pen kept, nothing is wrong, so
    `BAND_FODDER_DORMANT_TOOLTIP` says calmly what the row WILL hold.
  - **THE LIVE FODDERING PERCENT IS REACHABLE, through a TYPED collaborator.** Knowledge is
    faction-scoped and no band dict carries it, so `BandDetailLines` holds `FactionReadouts` for this
    ONE reading — the cluster `BandPanelController` and `DrawerComposeController` already hold by type
    — and the class header's *"the injection surface is ONE CALLABLE"* is unchanged. A producer built
    without one answers `0.0`, which reads as "not learned": the honest answer for a client that has
    been told nothing.
  - **THE HOVER IS THE BLOCK'S, AND BOTH HOSTS MUST ATTACH IT.** `[hint=…]` does not parse in this
    Godot build (`DetailFormat.block_tooltip`), so the sentence rides the label's `tooltip_text`.
    `SubjectDrawerController` already did that for the Occupants drawer;
    `BandPanelController._build_vitals_label` did NOT, so the same row was dim with no explanation in
    the dock alone until it did.
  - **THE `compact` (SHORT) TIER STAYS GATED.** That tier trades the row for a stock clause on the
    Food line, and a dim `— fodder` clause states nothing the row it rides on does not — so a
    dormant larder puts no clause there, and the tier's measured worst case is unmoved.

  **The ONE decimal is `SourceForecast.format_fodder`, and `FODDER_DECIMALS` is the number
  `FODDER_FLOW_MIN` is defined as half of.** Fodder is the coarse account — a stock in the hundreds
  where a food rate is in hundredths — so a stock and the rate that drains it share one renderer here
  where food splits them (`format_stock` vs `format_magnitude`); giving them two would only let them
  drift. The `/turn` marking a rate rides the caller's format string in the tight, no-space spelling
  `POLICY_CAP_FODDER_FORMAT` already uses, as distinct from `YIELD_PER_TURN_SUFFIX`'s spaced form.

  **In the `compact` (SHORT band-zone tier) host it is instead ` · 128.4 fodder` appended to the Food
  line** (`BAND_FOOD_FODDER_CLAUSE_FORMAT`) — **the word its own standalone row and the pen's `Fed:` row
  use**; it read `hay` while the pen rows did, and one larder called two things across two tiers of the
  same panel is the confusion that sweep removed (`herd-readouts.md`). Lowercase, which is what keeps
  `band_panel_preview`'s merge guard honest: the standalone row's KEY is `Fodder`, the clause is
  `128.4 fodder`, and `contains` is case-sensitive, so the two needles cannot both fire on one host,
  carrying its own colour rather than inheriting the Food row's value tint — a starving
  band's hay stock is not itself a red reading, and the net rate beside it already sets the
  precedent for a self-tinted run inside that value cell. **That colour is the shortfall WARN**, on
  the `_band_fodder_falls_short` condition that is now this clause's ONLY reader: the tier has no height
  for the rate pair, and the widened gate now puts a `0.0 hay` clause on a band with a bill it cannot
  pay, which in neutral ink would read as *fine*.
  - **Merged, not dropped, and the asymmetry with the retired Trade row is the whole point.**
    `compact` says HEIGHT is scarce and width is not — it is the horizontal dock — so a row with
    another home (Trade, on the WORK head) was dropped and the one with none is folded sideways.
  - **THE MERGE IS SAFE BECAUSE IT WAS MEASURED, and the vitals label is `AUTOWRAP_WORD`** — a
    merged line one pixel too wide WRAPS and costs back the very row the merge bought, invisibly
    (two lines of a rendered vitals block look exactly like two rows). `band_panel_preview`'s
    `_assert_merged_food_row_fits` measures the realistic worst case — a big draining larder, three
    digits of provisions, three of turns, a signed rate and a three-digit hay stock — in the label's
    OWN font at its OWN size plus the `[table=2]` gutter: **353px in the 380px column, 27 spare**.
    Re-run it if the band zone ever narrows.
  - **The measuring trap, which cost a false failure first time round:** `[table]` rows carry **no
    line break into `get_parsed_text()`** — the whole vitals block comes back as one concatenated
    string (measured: 916px for three rows) — so a naive per-line split measures the entire block
    and reports a wrap on a label that fits comfortably. The row is cut out of the parsed text by
    **the NEXT row's key** (Morale), never by a newline.
  - **Frames + assertions** (`ui_preview`, `chapters/band_expedition.gd`): `band_hay_short` (100
    fodder, 6.0 owed against 5.0 grown — a comfortable-looking stock on a draining larder, so the
    RUNWAY and the amber caret are what say so) / `band_hay_breakdown` (**the pull-down OPEN**, the
    two flows in the shared popover — appended last in the block so no frame before it moves) /
    `band_hay_covered` (4.0 owed against 6.0 grown, runway `∞`) /
    `band_hay_empty_store` (pens owing 6.0, no Fields, an EMPTY store — **the case the old gate
    hid**) / `band_hay_and_pen` (the band's ledger in the Band/City dock beside its pen's own
    `Fed: ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn` in the tile drawer, the two scales
    of one fact in one frame) / **`band_fodder_dormant`** (a LIVE larder and a DORMANT one in ONE
    render — see below).
    **The five claims are made over FOUR line-sets in ONE block**, the fourth being a forager band,
    which used to render no row and now renders the dormant one: every claim here is a CONTRAST —
    warned against calm, live against dormant — and a contrast checked one half at a time is not
    checked. Falsified: dropping the warn (2), restoring the store-only gate (1), printing the
    sentinel raw (1), dropping the rate clauses (3), re-gating the row (9), dropping the dim
    treatment (2), dropping the hover (5), putting a caret on the dormant row (2).
    `band_panel_preview`'s `_vitals_worst_case_band_fixture` carries the ledger too: the widened row
    is by some distance the longest optional row a band can hold, so a worst case seeding the stock
    alone stopped being one.
  - **`band_fodder_dormant` HOLDS BOTH FORMS AT ONCE, and the SETUP ORDER is what makes that
    possible.** A selected player band is the DOCK's subject wherever a dock exists — the Occupants
    drawer then renders a one-line pointer at it — so a docked HUD has exactly ONE band-detail
    surface. The frame selects the forager band with **no panel injected**, which takes the drawer's
    own fallback path, and injects the dock afterwards: neither `set_band_city_panel` nor
    `render_band` re-renders the drawer, so the dormant row stays on the left while the hay band's
    live row renders on the right. The frame asserts that precondition too — a drawer that had
    flipped to the pointer would photograph one fodder row instead of two and look perfectly tidy.
  - **A `[url=` SEARCH OVER THE PRODUCED LINES CANNOT SEE A CARET.** The producer emits plain
    `Key: value` strings and `detail_bbcode` draws the clickable run, so that needle is VACUOUS —
    measured: a dormant branch wrongly registering a disclosure left it green. The honest question is
    `DisclosureController.state()`, read BETWEEN the two productions (`unit_summary_lines` clears the
    rows on entry, so the dictionary describes the last band produced), with the live band's own
    registration beside it as the paired positive.

- **THE FACTION PAGE'S `Fodder:` ROW — the Food row, on the other larder** (`FactionRollup._fodder_line`).
  `Fodder: 100.0 · −1.0 /turn` over a per-band drill-down, directly beneath Food and built beat for
  beat like it: two terms, no faction runway, one clickable row per band, the alert in place of the
  figure that would hide the band it is about.
  - **IT SUMS THE WAY THE FOOD ROLLUP SUMS — client-side, per band, out of the answers that band's
    own page gives.** `DetailFormat.band_fodder_store` for the stock and `band_net_fodder` for the
    rate, never re-subtracted here, exactly as Food reads `band_provisions` / `band_net_food`. There
    is no published faction total for either account and none may be invented: a rollup that did its
    own arithmetic is a second source of truth for a figure the band page already states.
    `band_fodder_store` was added for this — the fodder twin of `band_provisions` — and
    `BandDetailLines` reads it too, so one key is spelled in one place.
  - **NO FACTION RUNWAY, for the Food row's reason exactly**: turns-of-fodder is one larder against
    one band's pens. The per-band rows carry it through `DetailFormat.food_turns_text`, **999
    sentinel included**, and the ALERT — `BandFoodStatus.is_critical` on `turns_of_fodder`, in DANGER
    ink — is what reaches the summary. One severity rule for both larders, the choice
    `fodder_is_concerning` already made for the band row's caret.
  - **THE RATE IS SPELLED AT THE FODDER ACCOUNT'S OWN RESOLUTION.** `SourceForecast.format_yield_fodder`
    is `format_yield`'s twin at one decimal; the food scale's two would print `-1.00 /turn` and state
    a precision this account does not have. It takes the SPACED `YIELD_PER_TURN_SUFFIX` rather than
    the tight `POLICY_CAP_FODDER_FORMAT` spelling — that rule is about a fodder figure riding inside
    a longer clause, and here the figure stands alone in the value cell the food rollup's own rate
    fills one row above.
  - **EVERY BAND GETS A DRILL-DOWN ROW, including one with no hay** (`Band 1  0.0 · +0.0  ∞`). The
    Food drill-down lists the whole roster and this one does too: *which band holds the fodder* is
    the question the page exists to answer and "none of them" is a real answer, while a filtered list
    would state a faction total over a subset of the bands that make it up.
  - ⛔ **AND IT HAS A DORMANT FORM TOO, ON THE BAND ROW'S OWN GATE FOLDED ACROSS THE ROSTER.** It
    shipped for one round without one, on the reading that the faction page's Food row has no gate
    and "exactly like Food" was the spec. That was wrong in the way that matters: a faction with no
    fodder anywhere read `Fodder: 0.0 · +0.0 /turn` in FULL INK — a live-looking readout for an
    economy that does not exist — while every one of its own bands read the dim `—`, so the two pages
    disagreed about one fact and the disagreement read as a defect. The reason the band row got a
    dormant form applies unchanged one scale up.
  - **ONE GATE, NOT TWO SPELLINGS.** `_any_fodder_economy` is a FOLD of
    `DetailFormat.band_has_fodder_economy` over the bands and nothing else — whatever the per-band
    test admits, the page admits. A faction-scoped predicate of its own (`store > 0`, the obvious
    one) goes LIVE on a sub-floor crumb every band on the page is already calling dormant, which is
    the divergence the fold exists to make impossible. `band_panel_preview` stages the roster ON that
    floor for exactly this reason.
  - **THE DORMANT ROW IS BUILT BY THE BAND ROW'S OWN BUILDER.** `DetailFormat.fodder_dormant_row`
    took the vocabulary and the two-sentence hover off `BandDetailLines` when this landed: a const
    lives where every one of its readers can reach it, and a static rollup must not reach into a
    stateful producer. One builder is also what stops the band's dim dash and the faction's coming to
    mean different things. The faction's Foddering comes off the `knowledge` row threaded into
    `build_band_zone` (through `RungGates.track`, the client's one reader of a `{track: progress}`
    row) — a faction-scoped figure read at the scale it actually lives at.
  - **A DORMANT FACTION ROW REGISTERS NO DISCLOSURE**, so it wears no caret: with no band holding a
    larder there is nothing worth opening.
  - ⛔ **THE FACTION PAGE NOW DROPS THE PREVIOUS RENDER'S CARETS, AND DID NOT BEFORE.**
    `_build_vitals_label` calls `DisclosureController.clear_rows` before it builds its lines, exactly
    as `BandDetailLines.unit_summary_lines` does and for the identical reason. `_disclosure_state` is
    per-render and this page never cleared it, so a row that registers on one render and NOT on the
    next kept the caret — and the stale payload — it had last time. **Found by eye on the dormant
    `Fodder:` row**: a faction whose last pen is gone re-rendered a dim dash still wearing `▸`, over
    a per-band card built from larders it no longer had. `Kit` and `Growth` both return `""` the same
    way and carried the same latent bug; the one call fixes all three.
  - **Frames + assertions** (`band_panel_preview`): the row is in `band_panel_faction`'s vitals-key
    walk, `band_panel_faction_fodder` is the **drill-down OPEN**,
    **`band_panel_faction_fodder_dormant`** holds the dormant faction row and a LIVE band row in ONE
    render (the panel is DETACHED after the page is painted, so the drawer takes a band down its own
    fallback path while the panel goes on drawing the page it already rendered — the mirror of
    `ui_preview`'s `band_fodder_dormant` trick), and `_assert_faction_fodder_row`
    carries the sum, the fodder-resolution rate (asserted AGAINST the food-scale spelling), the
    row-per-band COUNT, and the runway/∞ pair. The roster's second band is given the `band_hay_short`
    ledger and the first is left with none, so the faction total is one band's — distinguishable from
    an average and from a list filtered down to the bands that have a larder.
  - **THE OPEN-CARD FRAME RENDERS BEFORE THE PAGE'S OTHER ASSERTIONS, and that is load-bearing.** The
    breakdown popover is an EMBEDDED subwindow and hides the moment GUI focus moves;
    `_assert_faction_party_row_jumps_home` presses a summary row's link, and the re-render that
    follows frees the focused button and takes the card down with it (measured — open, then gone one
    process frame later). So the photographable state goes first and closes its own popover behind
    it.
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
  - **Productivity is NOT a detail row — it reads on the Band panel's WORK zone head** as
    `Output 62%`, still only when `output_multiplier < OUTPUT_FULL` (1.0) and still graded ink →
    amber → red on the same buckets (config `band_status_config.json` `output.{warn,critical}` =
    `0.85`/`0.60`; near-full reads neutral ink, *not* green — it's a productivity note, not a
    "good"). It ties productivity to morale wherever it renders; what moved is WHERE its consequence
    is visible — the multiplier scales every rate that head already prints, so it qualifies them
    in place instead of sitting as a row in a height-capped column that shows none of them. That
    host is built out of `Label`s rather than BBCode, so the bucket lookup it calls is
    **`BandFoodStatus.color_for_output`** — a `Color` accessor and not a hex one precisely because a
    `Label` takes an `add_theme_color_override` and never a `[color=…]` tag. **It has no
    `hex_for_output` twin, and it must not grow one back "for symmetry" with
    `color_for_morale`/`hex_for_morale`**: that pairing exists because morale really is rendered by
    two hosts, and this multiplier now has exactly ONE surface. `DetailFormat.Context.output`, the
    renderer's `"Output"` tint branch and `hex_for_output` itself all went with the row — no emitter
    or caller survived — so nothing in the detail path carries the scalar any more. The head item's placement, gate and vocabulary are
    specified in `band-city-panel.md` → Zone `work`.
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
      → red like the **Output** readout rather than the morale/food green palette: normal growth is
      normal, not a "good", so the top bucket is neutral ink even at 188%. Unlike Output it shows at
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
  returns `BAND_UNKNOWN "—"`) — **except on lethal ground, where the chip renders degrees alone**;
  see the fallback note below, which turned that skip from cosmetic into a safety case. The readout
  was **informational only** — neutral ink, no HEALTHY/WARN/DANGER tint — until issue #614 merged the
  lethal-temperature warning into it; it now wears `DANGER` exactly when `TileSurvivability` says the
  ground kills, and neutral ink otherwise. What survives of the old rule is that **a band NAME never
  earns a tint on its own** (`TileClimate.gd`'s class docs carry the reversal in full).

### The climate BAND and the SURVIVABLE range are different numbers (issue #614)

`systems::population` kills on a per-turn fraction that is **independent of food** and applied to
every age bracket. The survivable range is an INTERVAL the sim states outright — at the shipped
tuning `[0.0, 40.0] °C`. It was reported as a **lethal Temperate tile**: with the cold onset then at
6 °C and `climate.boreal_max_temp` at 3.0, a 3.7 °C hex was labelled **Temperate**, rated **Fair**
habitability, showed **100 % morale**, and killed every turn. A band died over 58 turns on exactly
that hex, with a full larder, and no surface in the client said anything at all.

> #### ⛔ TWO INDEPENDENT TAILS — the symmetric model is retired and must not come back
>
> It was `min((|temp − ambient| − tolerance) × scale, cap)`: one midpoint, one deviation, one slope,
> one ceiling. **Cold and heat are not symmetric phenomena** — extreme heat is survivable with shade
> and water in a way −57 °C is not — so each side now carries its own onset, slope AND ceiling:
>
> | tail | onset | slope | ceiling |
> |---|---|---|---|
> | cold | 0.0 °C | 0.00175 /° | 10 % |
> | heat | 40.0 °C | 0.00176 /° | 3 % |
>
> A symmetric form cannot express that, and the way it failed is worth keeping: pinning the heat
> onset to mirror the cold one about an 18 °C ambient put heat death at **30 °C — a warm summer day**.
> `ambient_temperature` is not an input to mortality at all any more, and **re-deriving one from the
> two onsets would be rebuilding the model that was removed**.
>
> **THE COLD ONSET LATER MOVED 6 °C → 0 °C** ("6 °C is not cold"), with the slope 0.00159 → 0.00175
> so the tail still reaches its ceiling at −57 °C. That closed the overlap the issue was reported for:
> 0 °C is also `TileClimate`'s `polar_max_temp`, so **Boreal and Temperate ground is entirely
> survivable and only Polar ground kills**. The two thresholds agreeing is a coincidence of tuning and
> **not** a coupling — nothing in the client reads a climate band to decide lethality, and a lethal
> Temperate tile is simply no longer a state the game can reach.
>
> **The heat tail is unreachable on today's maps and that is deliberate, not dead code.** Worldgen
> tops out near 31 °C; both tails are calibrated to the ±57 °C range issue #622 opens up. Every
> heat-side fixture in the harnesses is therefore hotter than the generator can currently produce,
> and each says so at its definition — "correcting" one to a reachable value drops it below the onset
> and **silently deletes that harness's only heat coverage**, with nothing turning red.

**The two sets of thresholds are kept apart on purpose.** They come from different configs
(`demographics_config.json`'s `cold`/`heat` blocks vs the climate ladder), they do not line up, and
neither can be derived from the other — so the sim publishes the mortality model as its own per-run
table (`MapSection.temperatureSurvivability`, appended beside `climateBands` and carried on the same
cadence) and `ui/TileSurvivability.gd` is its only reader. Nothing in `TileClimate` consults it and
nothing in it consults `TileClimate`; a client that tried to tie the two together would be inventing
the threshold the whole Climate Authority arc exists to stop it inventing.

What the card says, and it is **ONE chip** (`SelectionCardController._tile_chip_descriptors`):

- the **Climate chip carries the number** — `Polar · -10.0 °C`. A band is a bucket wide enough to
  hold both comfortable and lethal ground, and until #614 the temperature was on no surface of the
  card at all, so a warning had nothing to be checked against.
- **…and on killing ground it IS the warning**: `⚠ Polar · -10.0 °C` in `HudStyle.DANGER`, with the
  hover naming what the ground does to the people and the rate it does it at —
  `1.8% increased mortality per turn due to severe cold`. Survivable ground keeps the same chip in
  neutral `INK_DIM` with no hover.

### ⛔ THE WARNING WAS ITS OWN PILL FOR ONE ITERATION, AND FOUR PILLS IS TOO MANY

It shipped as a separate `survivability` chip sitting directly after climate — `Fair` ·
`Temperate · 5.0 °C` · `⚠ Lethal cold` · `Fertile` — and a four-pill strip is more than a player
reads. The two pills were two readings of the SAME temperature, and that temperature was already on
the climate face, so the ⚠ and the tint moved onto it and the second chip is gone (with
`CHIP_SURVIVABILITY_COLD` / `_HEAT` and `is_at_max_rate`; the tooltip constants and
`survivability_percent_text` moved rather than went).

**It merged with CLIMATE and NOT with HABITABILITY, and that is arithmetic rather than taste.**
Habitability is `terrain attrition + terrain hardness + (|T − 18| − 9) × 0.004`, Hostile at ≥ 0.09;
on terrain carrying no attrition penalty, temperature alone does not reach Hostile until −13.5 °C,
while people start dying at 0.0 °C. **A warning folded into habitability would be silent across a
13.5-degree band of lethal-but-Fair ground** — and it was a 19.5-degree one when the onset was 6 °C,
which is precisely where the original defect lived. Climate has no such gap: it is the same number the
mortality model is struck from.

**THE CHIP SLOT IS `climate` IN BOTH STATES, which moves work onto the patch path.** The strip
rebuilds only when the SET of slots changes (`_tile_chip_slots`), so a tile crossing the survival
line under a live snapshot now takes the in-place branch — the node that was red and hoverable is the
very node that must come back neutral and inert. `_update_chip` re-applies the stylebox, the font
colour and the tooltip, and `ui_preview` asserts the flip in BOTH directions with the node identities
pinned, so it cannot pass by way of a rebuild that hid a stale tint.

**THE MISSING-CUT-POINTS CASE IS NOW A SAFETY CASE, NOT A COSMETIC ONE.** The chip used to render
only where `TileClimate.has_bands()`; hiding it when the sim published no band cut points cost
nothing while the warning was elsewhere, and would take the ONLY warning off the card now. So the
gate is `has_bands() OR is_lethal`: with the mortality model published and the bands absent, lethal
ground still renders `⚠ 3.7 °C` — degrees alone, no band name. Survivable ground with no cut points
still renders nothing, which is what keeps the fallback from becoming "always show something".

The chip is fog-gated for free — temperature is a static property of ground already explored, and
`_tile_chip_descriptors`' `VISIBILITY_UNEXPLORED` early-return keeps it off never-visited hexes
without a second test.

### ⛔ The hover said everything EXCEPT that people die, and rounded the rate away

The first hover led with the rate and then explained the arithmetic behind it:
`−4.6 % of every age bracket per turn, regardless of food. 3.7 °C is 2.3 °C past the 6.0 °C survival
line.` — with a `…, at the configured maximum rate.` variant when the model's cap was binding. On a
tile 0.02 ° inside the tail it rendered, verbatim:

> `−0.0 % of every age bracket per turn, regardless of food. 6.0 °C is 0.0 °C past the 6.0 °C survival line.`

**Both faults were presentation; the model was right throughout.** `is_lethal` and `death_rate` were
answering correctly for a tile whose real rate was 0.04 % under the tuning of the day (0.003 % under
the two-tail model that replaced it), and neither they nor the map overlay changed. What was wrong was printing numbers at a precision where they stop meaning anything, inside
a sentence that spent all its words not saying the thing:

- a positive rate **rounded to `0.0 %`** and picked up a leading minus on top, so a killing hex read
  as *nothing is happening*;
- `6.0 °C is 0.0 °C past the 6.0 °C survival line` **states nothing, three times over**.

The hover is now ONE clause per tail (`HudSelectionVocab.CHIP_SURVIVABILITY_TOOLTIP_{COLD,HEAT}`), and
the three things cut from it must not creep back — `ui_preview` asserts two of them ABSENT on an
ordinary lethal tile precisely because only a negative can hold a deletion:

- **the second sentence.** The degrees are on the Climate chip immediately beside the pill; restating
  them to derive a distance the player did not ask for is what buried the verb.
- **the capped variant.** Which term of the model is binding is not a thing a player reading a
  mortality figure needs told. `TileSurvivability.is_at_max_rate` went with it rather than being left
  as a public method with no caller.
- **"regardless of food".** True, and not what the hover is for.

**"per turn" is in the copy deliberately.** Without it the figure reads as a one-off, and a player who
takes a rate as a total rather than a per-turn compounding one has been under-warned in exactly the
way this issue exists to fix. It matters MORE under the two-tail tuning, which prices ordinary cold
ground in fractions of a percent: 0.4 % sounds like nothing until it is 0.4 % every turn.

The rate is formatted by `HudSelectionVocab.survivability_percent_text`, the one place the wire's
FRACTION becomes a percentage, and it obeys two rules: **no leading minus** ("increased mortality"
carries the direction, and the minus is what produced `−0.0 %`), and **never a rounded zero** — a real
rate below what `CHIP_SURVIVABILITY_PERCENT_DECIMALS` can show prints the BOUND, `<0.1%`. The floor
value and its face are DERIVED from that precision constant rather than written twice, so changing the
precision moves the format, the threshold and the `<0.1%` face together. It needs no new threshold on
the model: it is purely how a number too small to print is printed.

## THE ONE-SHOT UNLOCK NOTE IS RETIRED, AND ITS COPY IS NOT

`_announce_knowledge_unlock` posted `"<Track> learned"` plus the unlock sentence to the event dock's
System channel the turn a track crossed `KNOWLEDGE_COMPLETE` — fire-once-ever per faction+track, and
for a long time the only thing in the client that said a discovery had happened at all. The turn orb's
freshly-learned row supersedes it (`docs/plan_knowledge_screen.md` §5), so it and
`KNOWLEDGE_UNLOCK_LABELS`, `_knowledge_announced` and this cluster's last Callable injection
(`_note_sink`) are gone. `FactionReadouts` takes no constructor argument at all now.

**A completed discovery is therefore announced on the TURN ORB and nowhere else. It leaves the event
log entirely**, which is the intent rather than a side effect: the orb finds the player wherever they
are looking and the log is read after the fact, if at all, and two surfaces reporting one event from
two independently-derived diffs is how they come to disagree about which turn it happened on.

> ### ⛔ `KNOWLEDGE_UNLOCK_NOTES` SURVIVED THE ANNOUNCEMENT IT WAS WRITTEN FOR
>
> The table looks like part of the retirement and is not: **`KnowledgeRoster` reads it** for the
> knowledge screen's detail pane, under its *"What it lets you do"* head, and `HudKnowledgeVocab`
> deliberately does not re-author those sentences so the screen and any other surface naming a
> discovery cannot describe it differently. Deleting it with the announcement takes a line off the
> detail pane, and the harness says so — *"knowledge detail — the unlock line is FactionReadouts' own,
> not a second copy"* — but only if you look.
>
> It is also still the DECLARED SET of tracks that unlock something. `_ingest_intensification` no
> longer walks it (that loop was the announcement), so its readers are now the roster and that
> assertion.

**The ingest lost its `previous` value with the announcement**, and that is the whole of what changed
behaviourally here: comparing each track's prior reading against the new one was the client's only
"a track just completed" detector, and it existed solely to fire the note. The surviving diff is
`KnowledgePanelController`'s, which asks a different question — not fire-once-ever, but *since the
turn ticked*, over BOTH knowledge webs at once and off the roster the screen itself draws. See
`knowledge-panel.md`.

## The knowledge strip's FIFTH track is a capability, not a rung transition

`foddering` rides the wire's knowledge list beside the ladder's rung-transition tracks and is a
different kind of thing: **no rung waits on it** — which the roster states outright as `is_step:
false`, derived from the ladder rather than declared client-side. The other four are one per transition
(`wild --cultivation--> tended --seed_selection--> field`,
`wild --herding--> pastoral --penning--> pen`), so the list reads as the ladder itself; Foddering is
what the **pen rung teaches** (the corral rung's `earns_knowledge`), and what it buys is every fodder
seam a faction has — the pen's hay draw, the pen's `K` fodder term, and the **wild** forage patch's
fodder credit.

- **It renders LAST**, after `penning`, so it reads as the animal ladder's continuation rather than
  as a sixth rung of anything. `KNOWLEDGE_TRACK_LABELS` is both the label table and the render order,
  and `_ingest_intensification` rebuilds each faction's row from that table alone — so adding a track
  is a label entry plus a decoder field, never an edit to the ingest.
- **The strip needed no layout work**, and that was verified in a rendered frame rather than assumed:
  it already wrapped at `KNOWLEDGE_STRIP_TRACKS_PER_LINE` (2), so the fifth track opens a third row
  (`forage_fodder_known`, where every track is non-zero at once).
- **Its unlock note names no new VERB**, because this discovery unlocks none. The other four notes say
  which policy became available; this one says what the capability BOUGHT — *"Hay you gather now goes
  into the fodder store and feeds your pens."*
- The gate reason it produces is the compose sheet's, not the strip's:
  `RungGates.wild_fodder_reason`, in `labor-ui.md` → "The FODDER account can be real and unbankable at
  once".

## The band's KIT — three consumables, a clock each, and a cliff (`docs/plan_hunt_through_combat.md` §4.8)

The minimal TOE ships **three kits**, and their whole point is that they only ever fall: they are
start-stocked, not craftable, and running one dry drops its role to the unequipped tier **for good**.
All six wire fields shipped with **no consumer in the client at all** — `huntingKitDurability` /
`sledKitDurability` / `basketKitDurability`, plus the three resolved tiers `hunterAttack` /
`huntCarryPerWorkerBiomass` / `forageCarryPerWorkerBiomass` — so a player could not see their
equipment dying, only its consequences.

**ONE KIT, ONE JOB, and the pairing IS the readout.** Spears raise `attack`; a **sled** carries the
**hunt** (a carcass is one lumpy object you drag out whole); **baskets** carry the **forage** web
(berries are loose and bounded by what you can hold). The two carry tiers are separate wire fields
with separate durabilities behind them — a band can be out of baskets with its sled untouched — so
**neither may ever be rendered on the other's row**. That substitution, baskets boosting the hunt, is
the defect slice 5 corrected in the sim, and a UI that repeats it is out of reach of every sim test.

### The row is the clock; the disclosure is the cliff

> ⛔ **THE ROW HALF OF THIS SPLIT IS RETIRED** (`docs/plan_standing_upkeep.md` §4.9 item 12). It was
> `BandDetailLines._band_kit_line`'s `Kit: Spears 87 · Sled 54 · Baskets dry`, and it is gone from the
> band page and the faction page both — see "The `Gear` row is retired from BOTH pages" at the foot of
> this file. **`DisclosureController.kit_breakdown_lines` is untouched**, and the reasoning below is
> its reasoning: everything about what a kit DOES, how it is worded and how coverage is counted still
> governs the crafting panel's ledger, the compose sheet's role hint and both harnesses' driven claims.
> The bullets that describe the ROW specifically are marked where they occur.

`DisclosureController.kit_breakdown_lines` composes one line per item the band carries. The split it
was built for was what each half could honestly answer at its size: the row said *how long have I got
and which side of the line am I on*, and only the popover has room for *what each one is doing for me,
and what happens when it stops*. The second half is what survives.

- **PERFORMANCE IS FLAT UNTIL EXPIRY, so nothing here may be scaled by the remaining condition** —
  no bar, no gauge, no gradient. Durability and performance are orthogonal axes: a kit at 3 performs
  exactly as one at 97 and then stops, and any taper drawn here states a model the sim does not have.
  The popover's closing sentence (`DetailFormat.KIT_BREAKDOWN_CLIFF_NOTE`) is what stops a player
  reading `87` and `54` as rates; without it the two numbers invite exactly that.
- **A spent kit reads as the WORD `dry`, in DANGER ink, never as `0`.** The number is not the point —
  which side of the cliff the role is on is — and a zero beside two live conditions reads as a
  quantity on the same scale rather than as a state change.
- **The gate is `has()`, never `> 0`** (`DetailFormat.band_states_kit`). A dry kit IS `0` and is the
  single most important reading on the row, so only an ABSENT field may suppress it; a `> 0` gate
  would hide the loss it exists to announce, and a defaulted `Spears 0` on a pre-TOE cohort would
  report equipment destroyed that was never there.
- **RETIRED WITH THE ROW: the WARN caret and its two predicates.** `DetailFormat.band_kit_is_dry` and
  `band_kit_is_short` existed only to tint that caret and were **deleted** with it — a live-looking
  predicate with no reader is worse than none. **The two states they judged are still the right two**,
  and the event dock's `kit_life` line is what announces them now: wearing down is not a fact to shout
  about (nothing the player does changes its rate) while running dry is a permanent step down, so a
  remaining-condition threshold would either cry wolf every turn or fire after the loss; and a
  SHORTFALL is the second permanent-feeling state, a band holding ten spears for seventeen hunters
  having no dry item at all and still fighting with half a party. The coverage counting itself
  (`DetailFormat.kit_coverage`, read by `kit_breakdown_row`) stayed, because the popover states it.
- **All three are listed even on a band that neither hunts nor forages today.** Each wears on its own
  quantum (spears per animal killed, the sled per biomass hauled, baskets per biomass gathered), so
  this turn's activity does not predict which kit is closest to running out.
- **…AND THE EXPANDED ROSTER'S THREE ARE LISTED EVEN WITH NOBODY ON SCOUT OR WARRIOR**, which is the
  same rule one roster wider. The popover answers *what does this band's gear do*, not *what is it
  doing this turn*, and a row that vanished when a role was unstaffed would hide the cliff exactly
  when the player is deciding whether to staff it again.
- **RETIRED (ROW) — *"It survives the `compact` (SHORT band-zone) tier, unlike the retired Trade
  row"*.** That tier's spare row is the `Upkeep:` standing bill now, and a spent kit is stated by the
  event dock's `kit_life` line and by the crafting panel's ledger.

**Assertions (`ui_preview`, `chapters/band_expedition.gd`) — DRIVEN AND PNG-LESS SINCE THE ROW WENT.**
The five `band_kit_*` frames are retired with it (`band_kit`, `band_kit_expanded`, `band_kit_bare`,
`band_kit_short`, `band_kit_forage_short`), and every claim they carried about the COMPOSITION now
reads `kit_breakdown_lines` directly — the swap cross-check (the sled's line must quote the hunt's
carry and never the forage web's, and the basket's the reverse), bare hands on all three roles with the
two carries STILL not swapped at the unequipped tier, and the negative that a band stating no kit yields
**no breakdown at all**, without which the positives pass on a producer that emits unconditionally. The
fixtures' conditions are deliberately DIFFERENT numbers (`fixtures_band.gd`), because two kits sharing
one value would pass every assertion with their accessors swapped.

### A partly-armed band must stop reading as a fully-armed one (issue #520)

`remaining` says how much LIFE is left in an item and nothing said how many PEOPLE it reaches, so a
band holding four spears for seventeen hunters rendered **byte-identically** to one holding seventeen.
`KitItemCondition.workersHolding` / `workersOnQuotedJob` are the sim's answer and
`DetailFormat.kit_coverage` is the one reading of them: `{stated, holding, short, headcount}`, all
three WHOLE PEOPLE.

- **THE DENOMINATOR IS PUBLISHED, AND ALL FOUR JOBS COME THROUGH ONE PATH.** `workersOnQuotedJob` is
  the head count of the job the row is quoted at, resolved off the SAME coverage that produced
  `workersHolding`, so the pair provably describes ONE job. The hunt had a private path
  (`Σ hunt_crews.workers`) for as long as that was the only job head count on the wire, and it does
  not any more — **it must not grow one back**, because a second denominator is a second answer: a
  client that kept it reports this band's spears as short and its baskets as fine, exactly backwards.
- **THE TWO ZEROS ARE A RENDERING CONTRACT, and the guard is in `kit_coverage` alone.**
  `workersOnQuotedJob == 0` is *nobody is staffed on that job* — `0 of 0`, nothing was needed, not a
  warning — and a POSITIVE denominator with `workersHolding == 0` is the sharpest shortfall there is,
  every worker on a staffed job at the unequipped tier. **The early return is belt-and-braces rather
  than load-bearing** (the apportionment already answers `short 0` for `0/0`, so deleting it moves
  nothing), and the failure it documents is the one that IS observable: a "helpful" fallback to the
  hunt head count for an unstaffed job lights a perfectly sound piece of gear.
- **THE COUNTS ARE APPORTIONED, NOT ROUNDED APART** (`HudFormat.apportion_people`). Both halves are
  fractional, and rounding each on its own gives a `4 of 17` whose remainder is 13.
- **THE NOUN IS `workers`, NOT `hunters`.** One path for four jobs means one clause, and it cannot
  name the job: the row carries a head count, and which job produced it is resolved sim-side. Nobody
  holding a live item takes its OWN sentence (`KIT_COVERAGE_BREAKDOWN_NONE_FORMAT`) because
  *"only 0 of 4"* is the arithmetic where *"none of your 4"* is the fact — and that reading is
  reachable with the item LIVE, an item needing a full crew (`workers_per_unit > 1`) equipping nobody
  until the job is staffed to it. Both clauses are spelled from one shared tail
  (`KIT_COVERAGE_SHORT_NEEDLE`, the `RECOVERY_GUIDANCE_TEXT` idiom) so a harness needle finds either.
- **RETIRED (ROW) — the three-ink rule and the entry budget.** `_band_kit_line` inked a live item that
  reached everybody neutral, a spent one DANGER and a live-but-short one WARN, and showed only
  `BAND_KIT_ROW_MAX_ENTRIES` (3) entries, dry-first then short — a fourth wrapped and overflowed
  `Zone_band` by 22px. **That 22px measurement retires with the row**: nothing on either page grows per
  item any more. The three-state reading itself survives in the popover's own wording.
- **A SHORT ITEM'S POPOVER ROW TAKES ▼, and that generalises the glyph rather than overloading it.**
  The two-tone rule was `equipped ? ▲ : ▼`; it asks whether the item is SOUND now, so the whole row
  tints WARN and the words say which state it is in — `— bare hands` for the cliff,
  `· only 10 of 17 hunters carry one` for the shortfall. A green row carrying an amber clause is what
  it replaces, and that read as fine.
- **RETIRED (ROW) — the faction tally.** It read (`FactionRollup._kit_line`):
  The tally was dry bands alone, so a band holding ten spears for seventeen hunters was reported as
  equipped on the one surface a player uses to find WHICH band needs gear — the same reassuring-direction
  error one scope up. The two states share ONE count deliberately (the row is a count of bands worth
  opening, not a diagnosis) and the drill-down row's note says which: `FACTION_KIT_DRY_NOTE` leads
  `FACTION_KIT_SHORT_NOTE` on a band in both states, because the step down is permanent and a shortfall
  is the band outgrowing its gear, which crafting can answer.

**The faction claim and `_assert_faction_kit_counts_the_short_band` are retired with that row.** What
replaced its discovery path is the event dock's `material_shortfall` Alert, which NAMES the band and
carries a jump to it.

**Assertions (`chapters/band_expedition.gd`), driven:** the same seven items at the same wear as
`with_equipped_kit`, so the ONLY thing that can move the claim is the coverage. The
popover states the sentence on the SPEARS' line
and **not on the SLED's** (both are hunt items and only one is short, so a clause rendered per BAND
rather than per ITEM fails here); a short kit is never called bare hands; and **the unstaffed job is
asserted on that same band**, which keeps no pen, since the two zeros are one glance apart and only a
state carrying both can show the readout telling them apart.

**The four-job case** — two baskets among four gatherers, with the hunt
perfectly equipped and every item at full condition, i.e. the band that was unreadable while the hunt
owned the only denominator. **The claim that the SPEARS say nothing is the load-bearing half**: a
client still dividing everything by the hunt's 17 states the baskets as short too, so asserting the
basket fraction alone passes on the wrong denominator.

⛔ **AND THE ROW HALVES OF THOSE TWO CLAIMS WENT WITH THE ROW.** They asserted
`KIT_COVERAGE_ROW_FORMAT` against the rendered vitals block, which no longer states an item condition
at all; the popover halves above are what each claim now is.

`band_kit` / `band_kit_expanded` / `band_kit_bare` moved with this arc and are now sharper: the dry
baskets sit over a STAFFED forage job, so they read `Baskets dry (0/4)` and *"— bare hands · none of
your 4 workers carry one"* — the tier stepped down, and four people feel it, stated as the two facts
they are. Their handling-gear row is the quiet zero in every one of them.

### The other tiers, and the kit each is quoted at (`.claude/rules/core_sim/equipment.md`)

The roster grew **handling gear**, **wayfinding gear** and **clubs**, and those three items were
label-only in this readout for one reason: the popover pairs an item with the resolved tier it sets,
and the cohort published none for a scout's vantage or a warrior. It publishes them now
(`scout_vantage_range` / `warrior_attack`), so each has a row — `▲ Wayfinding 66 — 2-tile sight per
vantage`, `▲ Clubs 22 — attack 6 defending the camp`. The crook states its JOB rather than a tier
(`▲ Crook 45 — keeping and raising animals`), the build axis having no flat per-band field.

> ⛔ **A THIRD FIELD, `pen_carry_per_worker_biomass`, RODE THIS SET AND IS DELETED** (issue #543).
> This section listed it beside the other two and quoted the row it drew:
> `▲ Handling gear 45 — pen collection 12.0 per keeper`. **Carry is carry** — a fact about the people
> and their gear, never about the ground they stand on — so a pen is collected on
> `hunt_carry_per_worker_biomass`, which the SLED's row already states.

- **`kit_id` ANSWERS FOR THE HUNT JOB ALONE, and pairing either of the last two with it quotes the
  wrong kit's tier.** On a resident band that id is the HUNT default, so it covers `hunter_attack`
  and `hunt_carry…` (a pen is worked from a Hunt row and collected on that same haul); the vantage and
  the warrior's attack resolve through `default_scout_kit_id` / `default_warrior_kit_id`, the same
  asymmetry `forage_carry…` has always had with the forage default. Nothing in the popover reads a kit
  id at all — the sim has already resolved every tier — which is what makes the trap unreachable
  rather than merely avoided.
- **The two ATTACK rows say WHICH FIGHT they are for.** Spears and clubs set the same `attack` stat
  off different items, and a band really does hold two numbers for it — 20 on the hunt, 6 defending
  the camp — so a bare `attack 6` beside a bare `attack 20` would read as one of them being wrong.
- **THE VANTAGE IS TILES AND MUST NOT TAKE THE CARRIES' FORMAT.** It is a distance, not a biomass
  rate, and the sim rounds it to whole tiles when a posted vantage reveals — hence
  `KIT_VANTAGE_DECIMALS` and the `%s-tile sight per vantage` phrasing, which also sidesteps the
  `sight 1 tiles` a bare-handed scout would otherwise print.
- **An item is a CLOCK and its axis is a TIER, and the two are read from different places** — which
  is why a live item can sit beside a bare tier: the crook's row states its job at condition 45 while
  the band's build contribution is whatever its own `kit_tiers` row says.
  > ⛔ It illustrated that with the pen: *"the shared roster equips no kit on the pen axis, so a band
  > on the stalking kit collects its pen at 12 with its handling gear at 45 … so the pen row cannot
  > pass by quoting the sled's 40."* With `EquipmentStat::PenCarry` deleted the pen row IS the sled's
  > row, and the fixtures stamp no second carry.

**Assertions (`band_panel_preview`, NO FRAME):** `_assert_gear_breakdown_states_every_kit` reads
`DisclosureController.kit_breakdown_lines` off the producer and asks each row BOTH what it must say
and what it must not (the vantage never a per-worker rate, the clubs never `hunter_attack`, the SLED
its haul once and no `pen collection` clause behind it), plus the sled's own pairing so the rows
cannot have been added by making every one quote one number. Sabotage-verified: pairing the clubs row
with `hunter_attack` fails exactly the clubs assertion, naming `attack 20 defending the camp`.

> ⛔ **THIS SAID `band_panel_kit_expanded` — the dock's own gear popover.** There is no such frame:
> the `Gear` row that opened that popover is retired from both pages (§4.9 item 12, recorded above),
> and `kit_breakdown_lines` has **no live UI caller** today — the composition is what is asserted, and
> it is kept because the crafting panel's kit ledger is where these rows are headed.

### The HANDLING GEAR's row says BOTH the jobs it does

**The item is `hurdles` now, and the client's own label reads *Hurdles*** — it was `husbandry_gear`,
a name describing the KIT it sat in rather than the object, and the object is portable fence panels
you work a beast into. `DetailFormat.KIT_LABEL_HURDLES` moved with the id, so the popover row and the
roster's own display name cannot disagree about one item; `hoes` joined `KIT_ITEM_LABELS` beside it
and deliberately has **no breakdown row**, the build axis having no flat per-band field for a row to
pair it with (`labor-ui.md` → "THE BUILDERS' KIT IS DERIVED PER QUEUE ENTRY" on the client side).

The crook raises what a worker delivers to the `Tame` and `Corral` builds (issue #515,
`.claude/rules/core_sim/equipment.md` → "The build axis"). Its row states the job it does and, above
neutral, that contribution: `keeping and raising animals · +0.5 work a turn per keeper on a tame or a
pen`.

> ⛔ **THE ROW LED WITH A PEN COLLECTION RATE AND NO LONGER DOES.** It read
> `pen collection 40.0 per keeper · +0.5 work a turn per keeper on a tame or a pen`, the section
> arguing that *"the handling gear binds a slaughter at a pen AND raises what a worker delivers …
> so a row quoting only the pen rate describes the payoff at the top of the ladder and says nothing
> about the climb."* The gear stopped binding the slaughter when hurdles became a MATERIAL (§4.9
> item 12) and `EquipmentStat::PenCarry` was deleted outright (issue #543); what a pen collects is
> the SLED's haul, on the sled's own row. The build clause — the half that argument was added for —
> is what the row now leads with.

> ⛔ **THE CLAUSE READ `8.5 work off a tame or a pen, per keeper` AND BOTH HALVES OF THAT ARE
> RETIRED.** `build_work` is an ADDEND on what an equipped worker DELIVERS per turn, never units off
> the job — a job's work requirement never changes (`docs/plan_standing_upkeep.md` §4.8) — and the
> magnitude moved with the meaning, `8.5` being the old subtraction's units and `0.5` the rate's
> (`core_sim/src/data/equipment.json` → `_comment_durability` owns the round trip). The item is the
> **crook** too: `hurdles` became a MATERIAL at §4.9 item 12.

- **IT IS WORK UNITS, NOT A MULTIPLIER, and the wire field changed with the wording.**
  `EquipmentStat::BuildRate` is retired (`docs/plan_unit_costed_work.md` §6): a multiplier on the
  crew cancels the job's cost, so `×1.5` saved the same PERCENTAGE of turns on a garden and on a
  farm — the shape the work-costed arc exists to escape. `KitRoster.KIT_BUILD_WORK_KEY`
  (`buildWorkPerWorker`) is what the row reads, and the old `buildRate` is **frozen at its neutral
  `1` on the wire and no longer decoded at all**. That is not tidiness: a reader left on it renders
  `> 1.0` for no kit in the game, so the clause silently disappears **and** `KitRoster.kit_offer`
  stops offering the kit that carries the handling gear on a herd being tamed, which is the one job
  that gear is for.
  **The gear's worth is now qualified by a `build_work_branch`** — hurdles serve the ANIMAL web and
  hoes the PLANT one — and this row is unaffected, being about a herd either way; what reads the pair
  is `KitRoster.build_kit_for_branch`, which the Builders card and the build queue's header both
  resolve their stated kit through.
- **The clause is appended only ABOVE NEUTRAL, and its absence is a real reading.** A contribution of
  `0` means the gear is changing no build — because it is spent, or because this band's hunt job is
  on a kit that does not carry it — and `0 work` costs a line's width to say *no*. The row's own
  condition already carries that news. (It used to say *"and its stepped-down pen rate"* too; there is
  no pen rate on this row any more.)
- **THE VALUE COMES OFF THE BAND'S OWN `kit_tiers` ROW, not a flat cohort field**, and there is no
  flat twin on the wire. The flat per-band fields answer for a readout with *no* kit selected; a build
  always has one (its job's default), so `KitRoster.band_kit_tiers(band, band.kit_id)` is the honest
  lookup — the same one the "quoting the FRESH tier" rule already prescribes. It is also the safe
  shape: `PopulationCohortState` derives `Default`, which answers `0` — which under the work model is
  the honest neutral rather than the *"this crew builds nothing at all"* a defaulted multiplier was.
- **Resolved in `DisclosureController`, not `DetailFormat`**, so the pure format layer keeps
  depending on nothing.
- Pinned **three ways** (`ui_preview`, `compose_rungs`), because each alone passes on a broken
  renderer: present on a band whose hunt kit carries the gear; **absent** on the same band reading a
  kit that does not (or a suffix stamped on every row passes the first); **absent** again once the
  gear is dry (or a clause read off the fresh roster rather than the band's worn row passes both) —
  with a liveness assertion that all three really rendered the row.

## The forecast's BAND rides beside the expectation, never in place of it (§6.4)

`LaborAssignment` gained `actualYieldLow`/`High` (and, until arc #527 retired the account,
`tradeYieldLow`/`High`), and `actualYield` became
the take's **expectation** over the retreat seed. `SourceForecast.yield_range_clause` renders it as a
muted ` · likely 6.00–11.00` on the row and the same clause on its tooltip, through the row's existing
`muted_note` channel — so all three hosts of `source_yield_readout` (the work board's rows, the
drawer's standing summary, the stepper's status line) show it without a channel of their own.

**IT SHIPPED DEGENERATE, AND THAT WAS THE POINT.** When the readout landed, wariness was `0` across
the roster and `hit_chance` `1.0`, so every stage took its exact identity at every quantile,
`low == actual == high` bit-for-bit, and the clause was `""` on every source in the game — every
existing readout byte-identical. **Slice 7 authored the roster's wariness and the band turned on with
no client change at all**, which is the whole return on shipping it inert: a wild hunt's rows now
carry a real ` · likely 6.00–11.00`. It stays `""` wherever nothing is stochastic — the whole plant
web, a pen, and every **resolved** row, which is degenerate by construction since the take has
happened.

- **The presence test is at the FORMATTER's resolution** (`has_yield_range`), the call `has_component`
  makes and for the same reason: bounds that round to one printed string are one number on screen, so
  a raw `low != high` would render `0.31–0.31` as a range.
- **The accounts are read as a VECTOR and none substitutes for another.** That rule was written when a
  wolf's food band was honestly all-zero while its trade band was the whole of what the raid paid; the
  trade account is retired (arc #527), so the wire carries the food pair alone today and an inedible
  quarry now states no band at all. **The vector reading stays** — the next account with a range on it
  inherits it, and a food-only range could not state such a take.
- **The keys travel PRESENCE-SENSITIVELY** through `HudBandLaborState.OPTIONAL_YIELD_KEYS`. A
  `get(…, 0.0)` default would hand the readout `0.0–0.0` — equal, therefore silent — on an assignment
  that never carried them, which makes "no band published" and "the band is a point" the same rendered
  answer by luck rather than by construction.
- **The headline does not move.** The row still headlines `realized_yield`, the steady average; the
  band is about this turn's expectation, which is a different number, and folding them would put a
  range on a figure that has none.

**Frames:** `herd_hunt_yield_range` / `herd_hunt_yield_point` (`chapters/hunt.gd`), judged as a PAIR —
the second is the shipped case, and without it the first passes on a readout that decorates every row.

## A launched DENIAL raid states a COLLAPSE, where a hunt party states a delivery

`docs/plan_denial_raid.md` §3. `BandDetailLines.expedition_summary_lines` splits its raid branch in
two: **`is_raid`** (hunt OR deny) gates the rows the two missions share — `Target:` with the target's
live `(x, y)`, and `Carried:` — while **`is_hunt`** alone still gates the `Orders:` row (the floor and
the fill target until issue #491 retired that lever, merged into one row), `Next delivery:` and the
trip-bound clause. That asymmetry IS the mission: a denial party's `expeditionFloor` reads `0.0`
because it has no such order, and it publishes no delivery projection at all, so rendering either
would put a lever on screen the command grammar cannot even express.

What stands in the delivery's place is **`DetailFormat.expedition_collapse_line`** — a `Collapse:` row
whose value is `SourceForecast.denial_verdict_bbcode`. The sim publishes **no per-party collapse
field**, so the line reads the TARGET HERD's own `denialEstimates` row for the party's `size`: the
same table, the same row and therefore the same sentence its launch sheet quoted, which is what stops
the promise made at launch and the readout in flight from drifting. It renders `""` when the target is
gone from telemetry — the `Target:` row above already says the herd is not there — or when the herd
carries no row for that party size.

- **The value carries its OWN `[color]`**, the `_band_food_line` precedent, because a verdict's
  severity is a fact about the FORECAST and not about the row key, so it cannot come from
  `_value_hex`'s key registry.
- **The `Carried:` row is deliberately kept and reads near-empty**, which is the mission's own cost.
  Suppressing it would hide the one number that says what a raid banks on the way home.
- The row also rides the Active-parties row TOOLTIP (`DetailFormat.expedition_row_tooltip`), gated on
  the deny mission alone, since a compact row cannot carry a sentence.

Frame + assertions: `ui_preview`'s **`expedition_denial_panel`**, which asserts the mission label, the
range verdict, the three ABSENT hunt readouts and the surviving `Carried:` row, beside three PNG-less
claims about the verdict's structure (a `repelled` outcome carrying a full turn band still quotes no
number; an unbounded `past_recovery` still names its outcome; and the two degenerate band forms). Each
is sabotage-verified against a different mutation. The launch half and the vocabulary live in
`band-city-panel.md` → "DENIAL is a third MISSION on the parties footer".

## The Food line's TRANSFERS are breakdown rows, and the headline is the four-term STEADY rate

Arc #527, issue #517. The larder identity the sim pins is

```text
larder_delta == foodIncome − foodConsumption − raidForfeit
                + transferReceived − transferSent
```

**The BREAKDOWN states all six; `DetailFormat.band_net_food` sums the first four.** The two are
answering different questions and the split is deliberate: the headline is a per-turn RATE, and the
transfer pair is what CROSSED a larder over the snapshot window — a past event, which the itemized
rows are the right place for.

**The reason the pair cannot ride the headline is the number printed BESIDE it.** The sim's
`turnsOfFood` runway is computed from per-source income and excludes transfers entirely, so a folded-in
headline makes the `/turn` rate and the `(N turns)` runway *on the same row* compute on different
bases — two numbers on one line that cannot agree. Matching the sim's basis is the point; the red
flash is only how it shows.

And it does flash. A shipment is bounded only by the manifest the player builds — up to the whole
larder — unlike `raidForfeit`, which is capped at a fraction of one turn's income. A band with income
6 and consumption 5 that sends 40 printed **`-39.0` in DANGER red under a WARN caret**, then `+1.0`
the next frame, on an economy that had not changed.

**What this costs, stated plainly: the steady headline does NOT reflect a neighbour's recurring
supply-network contribution.** `balance_supply_networks` moves food between co-networked larders every
turn, and that genuinely is a standing part of a band's economy. Closing it properly means the SIM
projecting steady transfers forward (issues #547 / #548) — a client-side fold-in of a past window is
not that number and cannot be made into one.

- **Two named magnitudes, never one signed net**, matching `raidForfeit` beside
  them: a band that both sends and receives inside one window is doing something, and a net renders
  that as nothing having happened.
- **THE ROWS READ THE PER-TURN PAIR, AND THE LEDGER'S PAIR IS NOT RENDERABLE** (issue #517). The wire
  carries both: `transferReceived` / `transferSent` accumulate over the PUBLICATION window — a
  `send_trade_expedition` debits the larder when the command is applied, between two published
  frames, so they span exactly the interval a client's own `larder_delta` measures — and are cleared
  the moment the turn's capture reads them. The sim then re-captures from live components after every
  dispatched command, so on any command-refreshed frame that pair reads `0.0` and both rows vanish;
  a real 0.56-food transfer showed nothing the instant the player did anything. `transferReceivedTurn`
  / `transferSentTurn` (`DetailFormat.band_transfer_received_turn` / `_sent_turn`) are per-turn state
  on the cohort, re-read unchanged by a recapture and equal to the accumulating pair on the turn's own
  frame, and they are what the two `⇄` rows are made of. **Every other row in this breakdown —
  Gathered, Consumed, pen upkeep, raid forfeit — is a per-turn value that survives a recapture**, so the
  ledger now reads on one basis throughout, and the accumulating pair stays on the wire for the
  identity it closes and for nothing this panel draws.
- **Rendering whichever of the two is non-zero is not the fix.** It would put the launch draw back in
  the `⇄ To other bands` row the instant the command lands, at the price of a row whose meaning
  depends on when it was looked at. The cost of not doing it is that a shipment no longer flashes into
  that row at launch — the larder total drops immediately and the shipment sheet confirms the send.
- **They enter `band_has_food_flow` even so**, or a band whose only movement this turn was a
  transfer loses its net line and its whole breakdown — the rows are what the gate is protecting. It
  reads the **per-turn** pair for the same reason the rows do: a gate on the accumulating pair goes
  false on a refreshed frame while the rows it protects still have values.
- **FOOD ONLY.** Materials cross between bands as well (the network pools them per rating, a shipment
  carries them) and there is deliberately no materials identity: a material's account is the batch
  store, and a scalar total of hide and bone is the retired trade axis under a new name.

**The breakdown gets a row each** (`DisclosureController.food_breakdown_lines`) — `⇄ From other
bands` as an income row, `⇄ To other bands` as a debit — each omitted at zero exactly like Lost to raids
and Lost to raids, so a band nobody trades with renders the ledger it always did.

**ONE glyph for both rows, and it is an ARROW rather than a handshake.** Consumed and Lost to raids
each carry their own mark because they are different debits; these two are one fact in two
directions, and the row's own words say which way. The emoji that says "deal" (🤝) is **not in this
client's fallback font and renders as an invisible gap** — no tofu box, nothing to notice — which is
the silent-failure class `Typography.gd` was retired for; `⇄` comes from the Arrows block the ▸/◀/▲▼
carets already draw from.

**Frames:** `trade_food_ledger` (a band carrying both terms, whose headline states the steady rate
alone) and `trade_food_transfers` (the same row OPENED, which is the only state that can say the two
terms are itemized at all — the headline says nothing about them by design).

**The command-refreshed frame is PNG-LESS and is asserted as a PAIR with them** (`chapters/trade.gd`):
the same band with the accumulating pair zeroed and the per-turn pair intact, asserting both rows are
still itemized. **A picture cannot make that claim** — the two states differ only in which field the
rows were read from, and the broken one renders no rows at all rather than wrong ones. The pair is
what forces the reading: a client on the per-turn pair passes both, and so does one that renders
whichever term is non-zero, so the accumulator is ZEROED there rather than merely left behind.
Sabotage-verified — pointing the rows back at the accumulating pair fails exactly those two and
nothing else in the run.

## A trade party states WHO IT IS FOR and WHAT IS IN THE PACKS, and nothing else

`BandDetailLines._shipment_summary_lines`, reached by an early return from
`expedition_summary_lines` — a shipment borrows NONE of the raid's rows. It has no quarry, no floor,
no delivery ETA and no trip bound, so every `is_hunt` / `is_raid` branch stays closed to it, and the
`Provisions:` row beneath them would restate a pack this mission states properly. Four rows at most
(`Mission` / `Bound for` / `Phase` / `Carrying`, plus `Position` in the drawer), comfortably inside
the parties strip's seven-line worst case, which is a HUNT party's.

- **`Bound for` renders a NAME and never `expeditionDestinationBand`** — the id is the key
  `send_trade_expedition` addresses and must never reach a label. The name comes from
  `HudFormat.expedition_destination_label`, which is **the one resolution the parties-strip row and
  the destination picker also use**, so a band cannot be called three things on three surfaces:
  - **the sim's published `expeditionDestinationName` when it is non-empty** — it is resolved at
    LAUNCH and carried on the mission, because the destination is precisely the thing a party
    outlives (a band walks away, leaves the viewer's sight, or is gone while the shipment is still
    bound for it), and the day a second faction lands (#513) a FOREIGN band's name can only come from
    the sim, this client holding no roster to resolve one from;
  - **else this client's own label for that band**, joined on the id through
    `HudBandLaborState.band_label_for_id` — a roster POSITION, the same `Band 2` the cycler, the band
    picker and the event dock's `band=` swap give it.
  **It is empty on every live shipment today, and that is the sim declining to guess.** Bands have no
  names in this game; the field was briefly filled from the sending path's `StartingUnit.kind` — the
  unit ARCHETYPE, the same `"BandForager"` for every seeded band — which made the row disagree with
  what the rest of the HUD called that same band. A wrong name is worse than none: none has a
  fallback. A destination neither tier can name renders **no row**, rather than the raw `BandId`.
- **`Carrying` weighs the WHOLE PACK against `expeditionCarryCap`, whose lever is the MISSION's.** A
  raid's cap is the provisions ceiling it fills before delivering; a shipment's is what its people can
  carry out, and what the sim checks it against is `food + expeditionTradeFodderCarryWeight × fodder
  + expeditionTradeMaterialCarryWeight × Σ materials`. So the numerator is that mass —
  `DetailFormat.shipment_cargo_mass` — and the hay and materials
  trailing the row are its SPELLING, not a second cargo beside it. Reading `expeditionCargoFood` alone
  over that cap rendered a party carrying 2 food and 10 hide against a cap of 12 as `2.0 / 12.0`: a
  full pack shown as one-sixth full.
- **HAY IS A TERM, AND FOOD IS NOT, because the mass is denominated in food** (issue #590). Food
  counts as itself at weight 1, so the leading figure already reads as food-and-then-some; every
  account carried at a weight of its OWN says so, which is the hay and the materials. The hay term
  reads `6.0 hay` — the player's word, where the wire says `expeditionCargoFodder` and the command
  says `fodder` — in `SourceForecast.format_fodder`'s one decimal rather than the materials', hay
  running on a far coarser scale than food. **Below `FODDER_FLOW_MIN` there is NO TERM, never
  `0.0 hay`**, the rule every material row already keeps.
- ⛔ **HAY IS NEVER ADDED TO THE FOOD IT RIDES BESIDE.** The two are different larders at the
  destination and never convert, so a `Carrying:` that summed them would promise a food delivery the
  destination's larder is never going to see. The frame asserts that sum ABSENT for the materials'
  reason: a row quoting the total still renders a perfectly plausible number.
- **ONE mass expression, shared with the compose sheet's meter** (`DetailFormat.shipment_mass`, called
  by `BandPanelController._trade_manifest_mass` and by this row). The pre-launch price and the
  in-flight report are the same pack asked about twice, and two copies of the formula are two answers.
  It takes all three accounts and both carry-weight levers as PARAMETERS, so a caller that omits a
  term is a compile error rather than a quiet under-price.
- **`_shipment_cargo_clause` is NOT `_party_pack_clause`.** The pack clause reads `material_batches`,
  the party's OWN kit — what a scout skinned on the road, and what a trade escort carries for
  itself — while the shipment is the cargo store beside it. Rendering one for the other would let an
  escort's gear read as goods bound for another people. **One term per material, never summed**, the
  same contract; the frame asserts the SUM is absent, because a row that added hide to bone still
  renders two plausible numbers and every other assertion passes.

Frame: `trade_party_panel`.

## THE STANDING MATERIAL BILL, and the `Gear` row it replaced (`docs/plan_standing_upkeep.md` §2.7)

**A PEN FRAYS ITS FENCE EVERY TURN IT STANDS; A ROAD WASHES OUT.** What a band has BUILT costs it a
rate in goods beside the rate in hands, and nothing on either page said so. The `Upkeep:` row is that
bill, and it is the Fodder row beat for beat:

```
Upkeep ▸  2 hurdles  (67 turns)
  Hurdles
    ▼ -0.05  Wanted
    ▲ +0.02  Arriving
    2  On the shelf
```

⛔ **THE ROW NAMES ONE GOOD, AND THAT IS WHAT KEEPS IT HONEST.** Six hurdles and two rope are not eight
of anything — a summed figure here is the retired `Trade:` scalar rebuilt out of its own replacement,
which is the flattening the materials model exists to refuse. The good quoted is the one in the WORST
state (the shortest runway, i.e. the one that runs out first); the rest are one click down, ONE BLOCK
PER GOOD in the popover, where growth is free. **Inline growth in a fixed-height zone is what clipped
`Zone_band` once already**, and this ledger grows with the number of goods a band owes, which is config.

⛔ **THE SIM SUMS `materialUpkeepNeed` AND THIS CLIENT MUST NOT.** `fodderNeed`'s own rule for
`fodderNeed`'s own reason: herd rows are FOG-FILTERED, so a total rebuilt from the pens on screen
silently drops one out of sight the band still owes for. `DetailFormat.band_material_bill` reads the
published per-good figures and folds nothing. **The FACTION page's per-band fold is a different
operation entirely** and is exactly what the Food and Fodder rows above it already do.

**THE RUNWAY IS THE FOOD ROW'S IDEA ON THIS ACCOUNT** — the shelf against the gap the arrivals leave,
`BandFoodStatus.UNLIMITED_TURNS` (the shared `∞`) where nothing is draining — and the value cell tints
through `hex_for_turns` off `Context.material_turns`, so a short good takes the danger ink with no
second severity rule beside it. `material_upkeep_is_concerning` is the caret's test, the FOOD test on
this account.

### NO DORMANT FORM — the one place it parts company with Fodder

A band with no fodder economy still draws a dim `Fodder  —` because there is a *"you could have this"*
story to tell: the Foddering craft is a thing to go and learn. **A band holding nothing that eats a good
has no such story.** A standing bill is a CONSEQUENCE of what you have built, so a row promising one
before anything is built would be a readout for an economy the player has not chosen to have. It draws
**no row at all** and registers **no caret**, and the faction row is absent for the same reason when
not one band on the roster owes a good.

The faction drill-down likewise lists **only the bands that owe** — the Fodder drill-down lists the
whole roster because *which band holds the hay* is a question every band answers, and *"none"* is a real
answer to it; a band that has built nothing which eats a good has no value to state in this register at
all. **NO FACTION RUNWAY**, for the Food row's reason: an average of one-band shelves describes no band
that exists, so the per-band rows carry it and the ALERT reaches the row.

### The `Gear` row is retired from BOTH pages

**IT DID NOT COMPRESS TO A LINE, AND SOMETHING ELSE ALREADY OWNS IT.** The band row was a bounded
summary of an unbounded list — three of however many items the server publishes, dry-first, the rest
behind a caret — because a fourth entry wrapped and overflowed `Zone_band` by 22px. The faction row was
an alert and a drill-down over durabilities that never aggregated. The CRAFTING panel's kit ledger
states every item in full, and the Builders card's own gear line was retired in §4.7 for exactly this
reason.

Gone with it: `BandDetailLines._band_kit_line`, `BAND_KIT_ROW_*` (including the `BAND_KIT_ROW_MAX_ENTRIES`
budget and the 22px measurement it respected), `FactionRollup._kit_line` / `_kit_face`,
`HudWorkVocab.FACTION_KIT_DRY_NOTE` / `_SHORT_NOTE` / `_ALL_EQUIPPED`, `HudDisclosureVocab.DETAIL_ROW_KIT`
/ `BREAKDOWN_KIND_KIT`, and `DisclosureController`'s kit arm of `_is_concerning`.

⛔ **THE `DetailFormat` KIT LEAVES STAY** — `band_states_kit`, `kit_coverage`, `kit_condition_face`,
`kit_is_equipped`, `kit_item_label` and the label/durability tables — because `KitRoster.role_hint` (the
compose sheet) and the crafting panel's ledger still read every one of them.
`DisclosureController.kit_breakdown_lines` stays too: it is the one composed statement of what each item
DOES, and both harnesses now assert on it directly rather than through a popover that no longer opens.

**WHAT REPLACED THEM IS NOTIFICATION.** `equipment.json`'s `life_readout` seams reach the event dock as
`kit_life` (warn → Notable, danger → Alert), and a `material_shortfall` Alert NAMES THE BAND — which is
what replaced the faction row's `⚠ N bands` → *which band* drill-down.

**THE COMPACT (SHORT) TIER TRADED ONE ROW FOR THE OTHER.** That tier merges Growth onto Morale to pay
for a row it cannot drop; the row it had gained was `Gear`, and the `Upkeep:` bill took its height. Net
zero rows in every tier, and the tier keeps a fact rather than trading one away.

## WHICH LINK THE GOODS CROSSED — the two transfer rows (issue #548)

`balance_supply_networks` moves goods between the player's own camps every turn, and a shipment moves
more of them; before this arc the Food popover said only `⇄ From other bands` and the Fodder popover
said nothing at all. **The row names the KIND OF LINK the goods crossed**, and there are exactly two:

| label | what it is |
|---|---|
| `⇄ Local exchange` | `balance_supply_networks` — the automatic balancing between camps within reach of one another. Nobody orders it and nobody built it |
| `⇄ Trade route` | a shipment: a party arriving with cargo, or the draw one takes when it launches. **This one the player did** |

That distinction is the whole readout, and it is what a player can act on — one of the two is a thing
they built, and the other happens whether they look or not.

⛔ **IT NAMES THE LINK AND NEVER THE COUNTERPARTY, and the counterparty version was BUILT AND
REJECTED.** Bands have no names in this game (issue #615), so every named row was either a
placeholder or a `Band 4` — and because a name list is variable-length it dragged a whole
pixel-fitting apparatus behind it (a measured column, a per-row lead, a two-pass fit, `+N more`
overflow, a `neighbors` fallback) purely to stop rows wrapping. **Two fixed phrases cannot wrap**, and
all of that machinery came out with them.

⛔ **AND NOTHING SAYS "POOLED".** One anonymous pot is how `balance_commodity` is implemented, not
what happens in the world: each camp holds its own stores and hands some of them to a short neighbor.
⛔ **And it is "neighbor"** — the copy is US-spelled.

⛔ **NO PROSE, ANYWHERE.** No mechanism sentence, no radius, no range warning, no footer. A first cut
carried all four and the verdict was *"how many useless words can you put in a panel"*. The rows are
the readout; `tools/ui_preview/chapters/supply_network.gd` asserts that every produced line is an
indented breakdown row, so a sentence cannot creep back in unnoticed.

### The shape of the terms

**DIRECTION IS THE SIGN'S JOB, exactly as on every other row of these breakdowns** — ▲ green in, ▼
amber out, decided by `food_breakdown_row` from the number it is handed. That is why one phrase serves
both directions and there is no `From` / `To` pair.

**ONE NETTED ROW PER KIND: received less sent, signed.** At most two rows per account. A camp that
took 3.00 in and sent 2.00 out down its routes reads one `⇄ Trade route +1.00`.

⛔ **NETTING IS THE DECIDED BEHAVIOUR FOR THESE FOUR TERMS, AND IT IS THE OPPOSITE OF THE RULE THE
GENERIC PAIR STATES.** That pair is two rows on purpose — *a band that both sends and receives in one
window is doing something, and a net would render that as nothing having happened* — and **it is
unchanged and still two rows**. Only the link-kind terms net. **The consequence is that a turn whose
arrivals and departures cancel exactly shows no row for that kind**: the net falls under the account's
floor and is omitted, like every other flow in this ledger.

**THE NETTING HAPPENS IN THE CLIENT, and the wire keeps four received/sent pairs per account.** The
sim publishes what it counted; the readout decides what to show. Splitting the two decisions costs
nothing and leaves both figures available to any surface that later wants the gross pair.

**THE LOCAL PAIR NEVER CANCELS ANYWAY.** `balance_commodity` puts a node in `sends` or in `wants` for
a given commodity, never both, so at most one of that pair is non-zero on a turn. Nothing
special-cases it: a shape that holds because of an invariant one module away breaks silently when that
invariant moves, and the cost of not exploiting it is a subtraction against zero.

**ONE CONSTANT PER LABEL, READ BY BOTH LEDGERS.** `DetailFormat.TRANSFER_LABEL_LOCAL` /
`TRANSFER_LABEL_ROUTE` — the fodder popover uses those very strings rather than a copy. Two accounts
wording one event two ways is a drift that has already had to be undone once. The rows differ in
exactly one thing, the number's resolution, which `fodder_breakdown_row` owns.

Each row is omitted when its NET magnitude falls below the account's existing floor
(`SourceForecast.FOOD_FLOW_MIN` / `FODDER_FLOW_MIN`), so a camp that exchanged nothing renders
nothing — never `⇄ Local exchange +0.00`.

### What is on the wire, and why there is no fallback

⛔ **NEITHER LEDGER HAS A PRESENCE CHECK OR A FALLBACK FORK.** The food ledger briefly kept its
generic `⇄ From other bands` / `⇄ To other bands` pair behind `band_has_link_transfers`, for frames
carrying none of the four food keys. No such frame exists: `dict/population.rs::population_to_dict`
is the only path that builds a cohort dict and it inserts all eight **unconditionally**, so the
generic arm was unreachable on any real snapshot and the one thing keeping it alive was a fixture
staging a state no server can send. It is gone, along with its two label constants — a fork nothing
can take is dead code, and tolerating an absent field is the back-compat this repo has no shipped
client or save to need.

All eight keys are on the wire and decoded — `transfer_local_received_turn`,
`transfer_local_sent_turn`, `transfer_route_received_turn`, `transfer_route_sent_turn` and the
`fodder_` prefixed four. Both ledgers simply render the rows whose figures they were given, and a
camp that exchanged nothing renders no transfer row at all — every term present and zero, which is
what the decoder actually produces. They are **per-turn** figures, on the basis every other row of both breakdowns is on — a
row read off an accumulator vanishes the instant a dispatched command re-captures the frame, which is
the defect issue #517 fixed on the generic pair and which must not be reintroduced one ledger over.
There are deliberately no accumulating twins of the eight: the larder identity stays closed by
`transfer_received` / `transfer_sent`.

### `band_net_fodder` IS ON THE RUNWAY'S BASIS, AND THE LOCAL ARM IS WHAT PUTS IT THERE

`DetailFormat.band_net_fodder` is `fodder_income − fodder_need + (fodder_transfer_local_received_turn
− fodder_transfer_local_sent_turn)`. The sim's `turnsOfFodder` reads
`fodder_store ÷ (drain − income − net local crossings)`, so the rate and the runway compute the same
way; without the local arm a band whose hay is rising because a neighbour feeds it showed a
*lengthening* runway beside a *negative* rate, on one row. This is not an internal figure — it tints
the `Fodder:` caret through `fodder_is_concerning`, and `FactionRollup` prints it as a signed
per-turn number — so the disagreement was on screen twice.

⛔ **LOCAL ONLY, NEVER ROUTE.** The rule the sim states and this matches exactly: **local crossings
are a rate and count; route crossings are events and do not.** Two camps within reach pool every turn
for as long as they stay there, so projecting that forward is what a forecast is for; a shipment lands
once, and annualising one delivery into a standing per-turn rate is the mistake arc #527 refused.
There is deliberately no method on either side that nets both arms.

⛔ **THE FOOD ACCOUNT IS NOT ON THIS BASIS.** `band_net_food` excludes transfers entirely, by arc
#527's decision recorded above — the two larders answering the same question differently is that
decision's consequence, not a drift to be reconciled.

**The fodder keys are separate from the food ones on purpose**: hay and grain cross the same links on
the same turn in different amounts, and a ledger reading the other account's figure would look
plausible on every frame.
