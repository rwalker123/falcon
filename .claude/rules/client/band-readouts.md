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


## THE SHORT BAND-ZONE TIER MERGES ROWS RATHER THAN DROPPING THEM

A horizontal (T/B) dock's band zone is height-capped and **CLIPS** rather than scrolling, while having
a whole screen of width. `BandDetailLines.unit_summary_lines`' `compact` flag is that tier saying so,
and it now buys three rows in three different ways — the differences are the rule:

| row | what the SHORT tier does | why |
|---|---|---|
| `Trade` | **DROPPED** | the rate is still stated by the WORK zone's head `⇄` total, so nothing is lost |
| `Fodder` | **MERGED** onto Food as a hay clause | a hay stock has no other home in the client |
| `Growth` | **MERGED** onto Morale as a clause | the fertility breakdown has no other home either |
| `Kit` | **KEPT, at every tier** | a spent kit is stated NOWHERE else and is not recoverable |

**The `Kit` row is what forced the third merge.** Every live cohort states its kit
(`DetailFormat.band_states_kit` is a bare `has()` on the spears key), so the row is shipped behaviour
— and the band zone was already measured at 299 of its 300px box, so one more 26px vitals row put it
25px over in 13 states. Dropping a row was not available: `Trade` is already the one this tier drops,
and `Kit` is the row that cannot be.

**Morale and Growth are the right pair to join.** Both are player-band health scalars, both already
carry disclosure carets, and they read naturally together.

**BOTH `[url]` METAS SURVIVE, which is why a merge beats a drop.** The vitals block is ONE
`RichTextLabel`, so a row is a line and merging two is joining two strings: the Growth clause carries
the identical clickable run a standalone row wears (`DetailFormat.inline_disclosure_label`, which
delegates to the same `_key_cell`), on the same label — so both popovers keep working. The clause
carries its OWN tint rather than inheriting the morale value cell's, exactly as the hay clause does.

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
| `ui/hud/BandDetailLines.gd` | `RefCounted` producer (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the **STATEFUL band/party detail-line producers** — the rows a BAND or a PARTY shows in whichever detail surface hosts it: `unit_summary_lines(unit, terrain_label, ctx, compact, with_position)` (Food · Fodder · **Trade** · Morale · Growth · Position, registering the Food/Morale/Growth/Trade disclosures through `DisclosureController` as it emits them) and `expedition_summary_lines(unit, ctx)` (Mission · Target + its live `(x, y)` · **Orders** · Phase · Carried/Provisions · Next delivery · the trip-bound clause · Position — the **Orders** row being the floor and the fill target as ONE line, `DetailFormat.expedition_orders_line`, because they are one sentence and this producer's output lands in a `clip_contents` strip capped at ~300px; see `band-city-panel.md` → "The parties strip's SEVEN lines"), plus the private row builders `_band_food_line` / `_band_trade_line` / **`_band_kit_line`** (the three consumable kits and how much is left of each, registering a fifth `Kit` disclosure — see "The band's KIT" below) / `_band_morale_line` / `_morale_breakdown_lines` and the shared gate `_band_has_fodder_economy`. **The two trailing flags are DIFFERENT QUESTIONS and must not be folded together**: `compact` is the band zone's HEIGHT TIER (the SHORT tier drops Trade and merges Fodder onto the Food line), while `with_position` is the host saying whether it states the band's coordinates somewhere ELSE — the Band/City dock does, in its panel header, in every tier. **There is no `_band_output_line`**: productivity reads on the WORK zone's head now (see the Civilization Wellbeing bullet below). **It is the stateful HALF of a three-way split**: the PURE producers became `DetailFormat` statics (`herd_summary_lines`, the expedition tooltip trio). (`_format_stockpile_label` was the third piece of that split, via `HudFormat.stockpile_label`; both it and the accessible-stockpile rows it served are retired — see the accessible-stockpile note further down this file.) Hud holds it as `_banddetail`, constructed in `_ready` AFTER `_disclosures` and BEFORE `_bandpanel`; **both detail hosts share the one instance** — the Occupants-card drawer (`Hud._render_occupant_drawer`) and `BandPanelController`'s vitals label + parties inspector strip, which is what retired three of that controller's nine Callable injections. **THE INJECTION SURFACE IS ONE CALLABLE** — `_herd_label_for_id`, which cannot fold onto `HudBandLaborState` because it reads THREE collaborators (`_selectioncard.find_roster_herd` AND `_selection.herd()` AND `_band_labor.find_world_herd`); `_is_player_unit` is a trivial private COPY (the `SelectionCardController` / `BandPanelController` precedent). **IT NEVER SEES THE SELECTION MODEL**: the old producers read `_selection` at exactly two sites, both `tile_info()["terrain_label"]` for the morale row's "it's the hex you're on" payload, so that ONE display string is now a `terrain_label` PARAMETER and both hosts resolve it through the new `SelectionCardController.selected_terrain_label()`. It also owns `_food_flow_present`, which is a **private handshake between `_band_food_line` (writer) and `unit_summary_lines` (its only reader)** — the formatter has never seen it, so it is deliberately not on the `DetailFormat.Context`. Consts follow the `DetailFormat` rule (a const lives here iff every reader moved here): the Fodder/FULL-badge/morale-arrow/contribution-label vocabulary came (the stockpile-row vocabulary went with those rows). The disclosure `DETAIL_ROW_*` / `BREAKDOWN_KIND_*` protocol vocabulary lives in `hud_disclosure_vocab.gd` and `MORALE_CAUSE_*` in `DetailFormat.gd` — read back as `HudDisclosureVocab.X` / `DetailFormat.X`, NOT as `HudLayer.X`; `Hud.gd` defines none of them |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale and output warn/critical thresholds; morale carries the `color_for_morale`/`hex_for_morale` pair because it really has both a `Label` host and a BBCode host, while **output carries `color_for_output` ALONE** — its one surface is the WORK zone head, which is `Label`s), shared by MapView's band dot and Hud's food/morale lines + alerts |
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
- **The band's TRADE row** (issue #381; the per-assignment `realizedTradeYield`/`tradeYield` wire
  fields). Trade goods are the SECOND product of the very sources the Food row totals, and until this
  they had no home in the panel the player commands from — the UI was a standalone inspector tab
  predating the dock, plus a small unlabelled left-dock `Stockpiles` card. Both are retired; the dock
  carries it, in the Food row's shape:

  ```
  Food:  74  (93 turns) · -0.81 /turn
  Trade: 12.0 · +1.36 /turn
  ```

  - **The stock carries ONE decimal** (`%.1f`, as the Fodder row does), where the Food row's provisions
    are whole units. The sim accumulates sub-unit trade income rather than rounding it off each turn, so
    an integer readout would put that accumulation back on screen as a `0` stuck for ~100 turns beside a
    visibly non-zero rate. `band_panel_preview`'s `_assert_forage_trade_counted` pins the rendered
    `12.0 · +0.08`.

  - **BOTH NUMBERS ARE GENUINELY THIS BAND'S**, which they were not when the row first shipped. Trade
    goods used to live only in the faction-global `FactionInventory`, so the stock had to wear a
    `(faction)` caveat — and a caveat is the wrong answer to a wrong number. **The sim now keeps them
    in the band's own `stores`**, the third key beside provisions and fodder: a band holds what it
    produces until a trade network reaches it, and `balance_supply_networks` pools it with same-faction
    bands inside `SupplyNetworkConfig.reach_tiles` (default 3) — which is the "within N hexes you need
    no route" rule, and it works because that balancer is commodity-generic. The stock is read exactly
    the way the Food row reads the larder (`DetailFormat.band_trade_stock` ↔ `band_provisions`), and it
    needed **no schema or decoder change**: `PopulationCohortState.stores` already ships every key.
    **`accessible_stockpile` was NOT that store, and its rows are RETIRED.**
    It published `inventory.stockpile(faction)` **whole**, gated only on the band sitting within a
    `stockpile_access_radius` of the faction's START position — a half-built proximity idea whose
    shipped radius was `0`. So `Stockpile: radius 0` / `Available: 2 Trade Goods` printed the faction
    total under a band-scoped heading, appeared only while the band had not left the start hex, and sat
    beside a Trade row saying the same number. The rows, their consts, the producer,
    `HudFormat.stockpile_label` (its last reader) and MapView's marker copy are all gone;
    `marker_field_guard` no longer lists the key. Sim-side the lever and the computation are gone too,
    so **the field always arrives absent**; the wire table and this decoder survive unread — see
    `.claude/rules/core_sim/yield-forecast.md` → "`accessibleStockpile` is an unread wire table".
  - **ALWAYS emitted for a player band, reading `+0.00 /turn` when it earns none.** Trade is a standing
    account of the band's economy, not a conditional feature like the Fodder row; a row that vanished
    at zero read in playtest as "this band cannot trade at all" rather than "it earns none right now".
    Zero renders in neutral ink, not green — a band earning nothing is not a "good", the same call the
    Output readout makes at full output. No sign branch exists: nothing consumes trade goods, so the rate
    cannot come out negative and a DANGER arm would be unreachable.
  - **The trade gates are the DISPLAY floor, `SourceForecast.has_component`** — both the row's
    healthy-tint/caret gate (`DetailFormat.band_has_trade_flow`) and the two breakdown rows — where the
    Food side's equivalents keep the sim-side `FOOD_FLOW_MIN`. Trade rates land in that gap routinely
    (one forager on a staple patch earns ~0.003/turn), so the finer floor rendered them green at
    `+0.00` behind a live caret whose popover listed `▲ +0.00 Gathered`. See `labor-ui.md` →
    `has_component` for why a gate finer than its formatter admits what it exists to stop.
  - **The disclosure is INCOME-ONLY** (`DisclosureController.trade_breakdown_lines`): Gathered/Hunted,
    every row ▲. Nothing consumes trade goods, so there is no Eaten/Pen-feed/raid analogue and no debit
    row exists to write — which is also why `_is_concerning` answers **false** for it outright: there
    is no trade analogue of starvation. A zero row therefore wears NO caret (`register` declines an
    empty payload), so it is honestly inert rather than opening an empty popover.
  - **THE PER-SOURCE RATE IS `SourceForecast.trade_rate_of`, AND ITS SENTINEL TEST IS THE VALUE, NOT
    THE KEY.** `realized_trade_yield` is **0 on every FORAGE source** — a missing *projection*, NOT a
    claim that plants sell nothing: `core_sim/src/forage.rs`'s
    `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` says so outright ("a KNOWN GAP, not a claim that plants
    sell nothing… Do not let a reader treat this as 'plants have no trade value'"), and the trade a
    gather actually earned ships in `trade_yield` (`labor.rs`: "A cash crop's harvest really does sell
    (Flora Roster F4)"). **Both readers spelled the fallback as `has("realized_trade_yield") ? … :
    trade_yield`, which is DEAD CODE** — `native/src/dict/population.rs` inserts that key
    UNCONDITIONALLY, so `has()` is always true on live data and the `0.0` sentinel won every time.
    Playtest caught it as a band reading `Trade +0.00 /turn` beside a forage patch its own compose
    sheet quoted at `0.04 trade`. Testing `realized > 0` is what makes the fallback fire. **The same
    dead spelling sat in `SourceForecast.source_yield_readout`, so a forage patch's trade had never
    rendered on a WORK-board row either** — one helper now serves both, which is also what keeps this
    headline equal to the sum of the rows the player can open. The consequence to state plainly: the
    headline is forward-projected for hunt sources and this-turn-actual for forage ones, so it is not
    the smoothed average the Food headline uses.
  - **The SHORT band-zone tier drops the row** (`unit_summary_lines`' `compact` parameter, passed by
    `BandPanelController._build_vitals_label`) — the row-level twin of that zone's existing
    food-outlook-chart gate, and for the same measured reason: the T/B dock's band zone is ~300px and
    CLIPS what it cannot hold, and this row measures **26px**. Trade is still legible there on the
    WORK zone header's `⇄` total.
  - **Nothing in the HUD reads `faction_inventory` any more.** The card's `HudLayer.update_stockpiles`
    (and `Main`'s dispatch to it) went with the card; `MapPanel.apply_update` still consumes the
    snapshot key for its scenario description. Nothing here needs a world-boundary reset either — the
    row renders off the band dict, which every snapshot restates.
  - band_panel_preview: `band_panel_trade_expanded_left` (earning, disclosure open — its fixture's
    forage patch carries the LIVE shape `trade_yield 0.04` beside `realized_trade_yield 0.0`, and
    `_assert_forage_trade_counted` pins both the `+0.08` total and the Gathered row, since the broken
    and fixed frames differ by two characters; mutation-verified to fail at `+0.04`) /
    `band_panel_trade_zero` (a band earning none — the row is PRESENT at `+0.00`, **asserted**, since
    "absent" and "present but zero" are one glance apart and the difference is the whole playtest
    report) / `band_panel_trade_short_tier` (the T/B gate, also **asserted** — a dropped row and a row
    clipped off a `clip_contents` zone are the same picture).
- **The band's FODDER (hay) larder — a ROW, or a CLAUSE on the Food line.** Shown only for a band
  with a fodder economy at all: hay in store, or a pen feed bill it could offset with hay
  (`_band_has_fodder_economy`, the ONE test behind both spellings, so the two can never disagree
  about when the larder exists). `Fodder: 128.4` in its own right, at the ONE decimal the Trade row cites this row as its precedent for;
  **in the `compact` (SHORT band-zone tier) host it is instead ` · 128.4 hay` appended to the Food
  line** (`BAND_FOOD_HAY_CLAUSE_FORMAT`), in the `hay` vocabulary the flora basket rows already use,
  carrying its own `INK_DIM` colour rather than inheriting the Food row's value tint — a starving
  band's hay stock is not itself a red reading, and the net rate beside it already sets the
  precedent for a self-tinted run inside that value cell.
  - **Merged, not dropped, and the asymmetry with the Trade row is the whole point.** `compact` says
    HEIGHT is scarce and width is not — it is the horizontal dock — so the row that has another
    home (Trade, on the WORK head) is dropped and the one with none is folded sideways.
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
  returns `BAND_UNKNOWN "—"`). The row is **informational** — neutral ink, no HEALTHY/WARN/DANGER
  tint, so it doesn't overload the Habitability row's warning semantics.

## The knowledge strip's FIFTH track is a capability, not a rung transition

`IntensificationKnowledgeState.foddering` rides beside the ladder's four rung-transition tracks and
is a different kind of thing: **no rung waits on it**. The other four are one per transition
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

`BandDetailLines._band_kit_line` emits one vitals row, `Kit: Spears 87 · Sled 54 · Baskets dry`, and
`DisclosureController.kit_breakdown_lines` hangs the popover under it — the Food/Morale/Growth/Trade
idiom, a fifth `BREAKDOWN_KIND_*`. The split is what each half can honestly answer at its size: the
row says *how long have I got and which side of the line am I on*, and only the popover has room for
*what each one is doing for me, and what happens when it stops*.

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
- **The caret is WARN only once a kit has RUN OUT** (`DetailFormat.band_kit_is_dry`). Wearing down is
  not a fact to shout about — nothing the player does changes its rate — while the step down is
  permanent, so a remaining-condition threshold would either cry wolf every turn or fire after the
  loss.
- **All three are listed even on a band that neither hunts nor forages today.** Each wears on its own
  quantum (spears per animal killed, the sled per biomass hauled, baskets per biomass gathered), so
  this turn's activity does not predict which kit is closest to running out.
- **It survives the `compact` (SHORT band-zone) tier, unlike Trade.** The Trade row is a rate the WORK
  zone's head restates; a spent kit is stated nowhere else in the client and is not recoverable.

**Frames + assertions (`ui_preview`, `chapters/band_expedition.gd`):** `band_kit` (one kit dry, two
intact — the row) · `band_kit_expanded` (the popover, and **the swap cross-check**: the sled's line
must quote the hunt's carry and never the forage web's, and the basket's the reverse) · `band_kit_bare`
(every kit dry — bare hands on all three roles, and the two carries STILL not swapped at the
unequipped tier) · plus the PNG-less negative that a band stating no kit renders **no Kit row**,
without which the three frames above pass on a row that renders unconditionally. The fixtures' three
conditions are deliberately three DIFFERENT numbers (`fixtures_band.gd`), because two kits sharing one
value would pass every assertion with their accessors swapped.

## The forecast's BAND rides beside the expectation, never in place of it (§6.4)

`LaborAssignment` gained `actualYieldLow`/`High` and `tradeYieldLow`/`High`, and `actualYield` became
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
- **The two accounts are read as a VECTOR and neither substitutes for the other.** A wolf's food band
  is honestly all-zero while its trade band is the whole of what the raid pays, so a food-only range
  could not state its take at all.
- **The four keys travel PRESENCE-SENSITIVELY** through `HudBandLaborState.OPTIONAL_YIELD_KEYS`. A
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
the fill target, merged), `Next delivery:` and the trip-bound clause. That asymmetry IS the mission: a
denial party's `expeditionFloor` reads `0.0` and its `expeditionFillTarget` `0` because it has no such
orders, and it publishes no delivery projection at all, so rendering any of them would put a lever
on screen the command grammar cannot even express.

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
