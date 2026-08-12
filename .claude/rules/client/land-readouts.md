---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_flora_vocab.gd"
  - "clients/godot_thin_client/src/scripts/ui/{FoodIcons,TileHabitability,TileClimate}.gd"
---

<!-- Extracted verbatim from lines 2705-2957 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Land readouts — forage, flora, the crop picker, pasture, the meters

- **Forage-patch cultivation readout** (`Hud.gd` `_tile_terrain_lines`): a forage tile's
  intensification state, mirroring the herd Husbandry row. `native/src/lib.rs
  forage_patches_to_array` decodes `foragePatches[]` (`ForagePatchState`) into both the
  snapshot and delta dicts under `forage_patches`; `MapView.display_snapshot` ingests it into
  the tile-keyed `forage_patch_lookup`, and `_tile_info_at` cross-refs it onto `tile_info`
  (`cultivation_progress` / `is_cultivated` / `patch_ecology_phase` / `patch_has_owner` /
  `patch_owner` / `patch_biomass` / `patch_carrying_capacity`, plus the harvest-floor instrument's four
  — `patch_per_worker_biomass` / `patch_regrowth_samples` / `patch_collapse_fraction` /
  `patch_stressed_fraction` — all in `FOW_DISCOVERED_HIDDEN_KEYS` **except
  `patch_carrying_capacity`**, so a remembered tile redacts them but keeps the ceiling (see "Fog
  splits a stock from its CAPACITY" below); the cross-ref is an explicit key list, so a decoded field left
  off it is absent on the plant web alone — see `labor-ui.md` → "THE PATCH'S FORECAST FIELDS REACH THE
  SHEET THROUGH `tile_info`"). The
  card shows a **Cultivation** row: "N%" while the patch is being tended, "🌾 Tended Patch"
  (SIGNAL tint via `_cultivation_value_hex`) once `is_cultivated` — and, beside it, its own
  **Field** row for plant rung 3: "Sowing N%" → "▦ Field" (`patch_field_progress` / `patch_is_field`,
  `_field_label` / `_field_value_hex`). The two are **independent meters on one source** and never
  merge: `Sow` needs no prior patch (seed travels), so a Field may stand on ground that was never
  tended. See `core_sim` intensification ladder — cultivation, and the two-meter split above.
  The patch's stock and its **ecology phase** ride the **`Foraging`** row (see "The tile card's TWO
  FOOD-WEB ROWS" below), not rows of their own. The module's internal `seasonal_weight` is **not**
  printed anywhere (it is a yield coefficient, meaningless to the player); it still drives the sim's
  yield. ui_preview: `food_tile` (Thriving) / `food_tile_stressed` (⚠ Stressed) / `tended_tile`.
- **The tile card's TWO FOOD-WEB ROWS — `Foraging` above `Grazing`, and nothing between them**
  (`SubjectDrawerController._tile_terrain_lines` + its `_graze_stock_lines` / `_stock_value` leaves;
  keys in `HudFloraVocab.FORAGING_KEY` / `GRAZING_KEY`). One row per web, each `stock / ceiling ·
  phase`, with the human web's basket indented beneath it:
  ```
  Height     5  ▬▭▭▭▭▭▭▭▭▭
  Foraging   205 / 205 · Thriving
     🌾 Wild Tubers    38%  (77)
     🧵 Cotton Fields   31%  (64)
     🐄 Hay Grass      31%  (64)
  Grazing    130 / 130 · Thriving
  ```
  This replaced **four interleaved rows** — `Pasture` / `Pasture ecology`, then the module row and
  the basket, then `Forage biomass` / `Ecology` — and each of the three faults it fixed was confirmed
  in playtest by a reader who mistook one web for the other three times:
  - **THE NAMES INVERTED EACH OTHER.** The stock rows were `Pasture` (bare) and `Forage biomass`
    (qualified) while the ecology rows were `Pasture ecology` (qualified) and `Ecology` (bare), so
    the unqualified word meant the ANIMAL web in one pair and the HUMAN web in the other. `Foraging`
    / `Grazing` are named for **who eats it**, the one axis on which they cannot invert.
  - **THE HUMAN WEB WAS SPLIT IN HALF** by the animal one sitting between its two halves. The pair is
    now consecutive, Foraging first — this is a forage-oriented card, and **adjacency is what stops
    the conflation**. A comparison the player cannot make in one glance is not a comparison.
  - **THE PHASE IS INLINE**, not a standing row. `DetailFormat._value_hex` keys BOTH row names to the
    shared `ecology_value_hex`, which matches the phase word wherever in the value it sits, so
    folding the rows forked no styling path: a stressed patch, a stressed pasture and a stressed herd
    still read identically.
  Each row renders only where that web has a stock at all (`patch_carrying_capacity > 0` /
  `graze_capacity > 0`) — never a `0 / 0`, which reads as a starved stock rather than an absent one.
  **AND THE `Foraging` ROW HAS A SECOND GATE THE `Grazing` ROW DOES NOT: the ground must be a
  GATHERING SITE** (`DetailFormat.tile_is_gathering_site`, issue #464). The row's label is a **VERB**,
  and the sim's plant rungs 1–3 all carry `requires_gathering_site`, so on ground that is not one the
  verb is impossible and the row is a property readout for a capability the player does not have.
  It rendered `Foraging 205 / 205 · Thriving` over a Wild Emmer basket on ground no crew could ever be
  put on, while the land row two rows above said `No forage` and the drawer offered no compose block —
  every signal in the block reading *go here*, the card arguing with itself, and the stand being the
  half that was lying. **The asymmetry with `Grazing` is the point, not an oversight**: pasture feeds
  herds with no player action at all, so it is a fact about the ground rather than an offer, and it
  keeps its row on ground nobody gathers. See "The row is an AFFORDANCE" below.
  **A REMEMBERED TILE STATES BOTH ROWS WITH BOTH STOCKS WITHHELD** — see "Fog splits a stock from its
  CAPACITY" below, which is the one home for that rule.
  **The `Forage:` MODULE ROW WAS DELETED OUTRIGHT.** `Riverine / Delta — River Garden` named a
  category the player can neither choose nor change, and the basket says the same thing in the terms
  a decision is actually made in. Nothing replaced it; the module still drives the land row's glyph
  and the sim's yield, and `_format_food_kind_label` + `_value_hex`'s `"Forage"` case went with it.
  ui_preview: `tile_food_layers` (the three-role reference tile) / `food_tile` (staples only) /
  `tile_pasture_stressed` (the phase inline and amber) / `tile_pasture_none` + `tile_panel_no_forage`
  (each web absent) / `tile_sight_remembered` (both webs, capacity only).
- **The tile card's BASKET — the plant COMPOSITION, as the `Foraging` row's decomposition** (Flora
  Roster F1/F5; snapshot `ForagePatchState.composition:[FloraShareInfo]` →
  decoded in `native/src/dict/subsistence.rs` as a `composition` array of
  `{species, display_name, share, role, …}`, cross-refed by `MapView._tile_info_at` as
  `patch_composition`). **One indented row per realized plant, always visible, directly under
  `Foraging`** (`DetailFormat.flora_composition_lines` → `SubjectDrawerController._tile_terrain_lines`):
  a **role icon**, the plant's display name, its **share**, and the **absolute biomass** that share
  amounts to — `🌾 Wild Tubers 38%  (77)`.
  - **THE HEADING AND THE DISCLOSURE ARE BOTH GONE.** A quiet `What grows here` header above the rows
    made the list read as a FOURTH resource standing beside the stocks; the indent under `Foraging`
    says "these decompose the row above" without a word, and always-visible is what lets a player see
    at a glance that (on the reference tile) **62% of what grows here is not food**.
  - **EACH ROW STATES ITS ABSOLUTE, and the three sum to the `Foraging` ceiling EXACTLY.** A share is
    a ratio and cannot be added to anything. The biomass is `percent × patch_carrying_capacity` off
    the ALREADY-ROUNDED percent (so a row's two numbers can never disagree), with the same
    largest-share remainder fold applied a second time — `DetailFormat._flora_biomass_split`. It is
    the CEILING, not the standing stock: the shares describe what the ground GROWS, a property of the
    patch rather than of how hard it has lately been worked.
  - **EACH ROW LEADS WITH ITS CROP ROLE** (`FoodIcons.for_crop_role`, from `FloraShareInfo.role`):
    staple / fodder / cash. The marks are **BUNDLED ART** (`CropRoleSprites`, issue #463) rendered as
    `[img]` BBCode, with the three borrowed emoji (🌾 / 🐄 / 🧵) as a live fallback — see
    `sprites-widgets.md` → the `CropRoleSprites` row for why the art replaced them (COLLISION: two of
    the three still mean something else elsewhere in this HUD) and for the sub-style ~13px forced.
    **The BOX SIZE is threaded in from the host label's own font size** — `flora_composition_lines`
    takes an `icon_px`, resolved by `SubjectDrawerController._role_icon_px()` off `%TileDetail`,
    because a static producer cannot ask a label how big its text is and a literal would be the
    hardcoded pixel size the discoveries strip already refuses to write. `0` means "text only".
    **`""` MEANS UNSTATED, NOT "staple"**: the row renders a blank slot that holds its width —
    `FoodIcons.crop_role_spacer` (a transparent image boxed exactly like a mark), falling back to
    `FLORA_ROLE_ICON_UNSTATED` — because defaulting a missing tag into a real category would invent a
    fact about the plant, and dropping the slot would shift every name in the list out of column. **Never re-derive a role from the payoff fields** — they are rung-2/rung-3
    numbers folding in the weeding and conversion gains, and they read all-zero for a species that
    cannot climb on this ground, which is exactly where the role is still true and useful.
    **THE MARK IS A FOUR-STEP CHAIN, AND THE ROLE IS ITS SECOND STEP** (issue #339):
    `FoodIcons.for_flora_species` (this plant's OWN bundled art, `FloraSprites`) → the role mark above
    → `crop_role_spacer` → `FLORA_ROLE_ICON_UNSTATED`. **SPECIES OUTRANKS ROLE BECAUSE IT IS THE MORE
    SPECIFIC FACT**: the icon's job on this list is to make a row findable at a glance, and *"this is
    Wild Emmer"* locates a row that *"this is a staple"* cannot — three marks cannot separate five
    rows. The role is not demoted by that; it remains the reading on every row without species art —
    **coverage is 32 of 33**, and the one gap is PERMANENT rather than pending: `hay_grass` is the
    roster's only `fodder` species, so the fodder mark already names it uniquely and `icon_prompts.txt`
    ships 32 prompts for 33 species by design. So the role tier is what a hay row wears, not a state
    the family is passing through. **The two never render TOGETHER**
    — species art REPLACES the role mark rather than sitting beside it, because two glyph families
    adjacent at one weight is the axis collision `labor-ui.md` records twice. **There is deliberately
    NO per-species emoji fallback**: the palette collapses the roster (grains all 🌾, nuts 🌰, berries
    🫐, mushrooms 🍄), which is exactly why #339 chose art over an emoji map, so an emoji answered here
    would re-introduce the collapse while SUPPRESSING the role — a distinction the palette CAN carry.
    The `[img]` box is the SAME `icon_px` in both tiers (`FoodIcons.BASKET_ROW_IMG_FORMAT`, renamed
    from `CROP_ROLE_IMG_FORMAT` when it grew a second art family), so a row with species art and a row
    with a role mark occupy identical width and the name column cannot go ragged.
  - **The rows are tinted NEUTRAL ink**, not the ▲/▼ two-tone — a share is descriptive, not a
    good/bad signal. `detail_bbcode` now has ONE indented-sub-row branch that tints by the SIGN GLYPH
    (▲ healthy / ▼ warn / **neither → neutral**), because the leading mark here is a role icon that is
    one of three or nothing at all and no literal can identify these rows. The old pair keyed its
    neutral branch off the single 🌿 sprig every basket row then wore.
  It names the plants the tile's forage capacity is MADE OF — **naming decomposes, it does not add**.
  Three rules: the wire list is **already sorted** (share DESC, then species key ASC) and is rendered
  **verbatim, never re-sorted**; the **displayed percentages always sum to 100** —
  `SourceForecast.flora_basket_entries` folds the rounding remainder into the LARGEST share (the
  first entry); and an empty / absent list renders **no rows** (a biome that carries no forage).
  **The basket itself is NOT in `FOW_DISCOVERED_HIDDEN_KEYS`** — it is a pure function of the BIOME —
  **but it does not render on a remembered tile**, because a remembered tile has no standing STOCK to
  decompose and each row states the biomass its share amounts to. (It is the STOCK that stops it, not
  the capacity, which a remembered tile now keeps: the `Foraging` row above the basket still renders,
  reading `— / 205`.) With nothing to split, the rows would be exactly the free-floating "three more
  resources" list this layout exists to stop.
  ui_preview: `tile_food_layers` (all three roles, and the biomass-remainder test — 38/31/31 of 205
  naively rounds to 206) / `tile_food_layers_unstated` (the same tile with one role missing from the
  wire: that row renders no icon) / `food_tile` / `tile_panel_land` (the fixture's shares naively round
  to 101%, so those frames ARE the percentage rounding test), `tile_growing_here` +
  `tile_growing_here_variant` (TWO Alluvial Plain tiles with DIFFERENT baskets — the visible
  per-tile-realization proof on the card), and `tile_panel_no_forage` (no list → no rows).
  Six `_assert_food_layer_rows` assertions carry what a frame cannot — that the biomasses sum to the
  ceiling, that an unstated role renders no icon while its neighbours keep theirs, that `Grazing`
  follows `Foraging`'s basket with nothing between, that the unstated row **still holds its slot's
  width**, and that the marks are **bundled art rather than the emoji fallback** — each
  sabotage-verified. The last two are #463's, and each exists because of a way the others pass
  vacuously: every assertion in the group survives all three PNGs failing to load (`for_crop_role`
  then answers the emoji and any needle built from it falls back in step), so ONE assertion has to
  name `CropRoleSprites.SPRITE_DIR` directly; and "no icon" does not distinguish a blank slot from no
  slot at all, which is the difference between a tidy column and one untagged plant shifting every
  name in the list. **The slot assertion is POSITIONAL (`begins_with`), never a `contains`** — its
  first cut asked `row.contains(FoodIcons.crop_role_spacer(px))` and PASSED with the spacer file
  deleted, because that helper answers `""` when there is no art and `contains("")` is true of every
  string. An empty needle is this repo's easiest vacuity trap and a helper that degrades to `""` is
  how you walk into it.
  **TWO ROWS, TWO QUESTIONS — the COMMITTED crop BESIDE the standing basket** (`docs/plan_flora_roster.md`
  §4.3, issue #433; `ForagePatchState.committedSpecies` / `committedDisplayName` → decoded in the same
  `forage_patches_to_array` as `committed_species` / `committed_display_name`, cross-refed by
  `_tile_info_at` as `patch_committed_species` / `patch_committed_display_name`). `Crop: Wild Emmer`
  (`FLORA_CROP_ROW`) answers **what you committed to**; the basket beneath it answers **what is actually
  growing right now**. They are different facts and both render, the committed member marked in
  `HudStyle.SIGNAL` inside the list so the eye connects them.
  > **This row was an either/or until #433, and that was a real bug** (caught in playtest). It rendered
  > `Crop:` **instead of** the basket on the reasoning that "committing displaces the rest of the basket"
  > — which the reweight model deleted, and which was never true *during the build* anyway: the species
  > is recorded on the **first worked turn**, ~25 turns before the rung completes, so picking a crop made
  > a 64/36 tile read as 100% that crop while nothing in the sim had moved. Showing both rows is also
  > what makes weeding legible — you watch the losing member fall (36% → 4%) as the build lands.
  `committedSpecies == ""` means **the wild mixed basket**, not "unknown", so the row switches on it
  rather than treating it as missing data. `Crop` is well under `_split_detail_kv`'s 16-char key limit,
  so it aligns as a table row like any other. Committed-ness is patch STATE (unlike the biome-derived
  basket), but it needs no `FOW_DISCOVERED_HIDDEN_KEYS` entry: the row sits under `Forage:`, past the
  discovered early-return, so a remembered tile never reaches it. **The SIGNAL mark is nested inside the
  neutral wrap `detail_bbcode` puts on every 🌿 row, and the indent + sprig stay OUTSIDE it** — that
  renderer branch matches on `begins_with(MORALE_BREAKDOWN_INDENT)` plus the literal glyph, so wrapping
  either one hides the row from its own formatter. ui_preview: `food_tile` (uncommitted) /
  `food_tile_crop` (committed, still building — the basket unchanged beside the Crop row, the bug's own
  frame) / `food_tile_crop_tended` (the completed Tended Patch, the basket visibly weeded). **All three
  states are needed:** a fixture covering only "committed" passes without ever testing the
  building-vs-complete distinction, which is the whole defect.
- **The CROP PICKER — committing is a DECISION, not a server default** (Flora Roster S1,
  `Hud._build_crop_picker` inside `_build_forage_assign_controls`; `FloraShareInfo.canCultivate` /
  `canSow` → decoded beside `share` as `can_cultivate` / `can_sow`). It renders **only under the two
  COMMITTING rungs** (`FLORA_COMMITTING_POLICIES` = Cultivate + Sow) — the extractive rungs gather the
  whole basket and choose nothing, so a crop control there would be noise. One row per basket entry in
  **wire order** (`Wild Emmer 34%`), sharing the F1 percentages through the extracted
  `SourceForecast.flora_basket_entries` (the ONE decomposition of the composition list, rounding already resolved),
  so the picker and the "What grows here" row can never quote different numbers for one plant.
  **THE TWO FLAGS ARE SPECIES-GLOBAL** — "can this plant *ever* climb this rung", not "is this a good
  idea here" — and the gate reads **the composed rung's own flag** (`_flora_entry_allows`), which is
  why Hazel is pressable under Cultivate and greyed under Sow. An illegal entry stays **visible and
  disabled with its reason in the tooltip, never hidden**: that a tile carries Oak Mast you cannot farm
  is information about the LAND, and hiding it would make the tile read poorer than it is.
  **THE HEADER IS PER RUNG, because "commit" is true of only ONE of the two committing rungs**
  (`FLORA_CROP_TEND_HEADER` *"Crop to tend to"* under Cultivate, `FLORA_CROP_PICKER_HEADER` *"Crop to
  commit to"* under Sow, `FLORA_CROP_COMMITTED_HEADER` on an already-committed patch). Sow forces the
  favored species to 100% of the stand (`forage.rs::planted` — a Field has no volunteers), so
  committing is exactly what its picker does; Cultivate only weeds that share UPWARD by
  `tended_weeding_gain` and leaves the rest of the basket standing (`forage.rs::weeded`), so a tended
  patch keeps growing everything it grew before. Calling that a commitment overstates the rung — and
  it is the belief issue #433 already had to delete from the tile card, where a 64/36 tile read as
  100% one crop the moment a crop was picked. `_build_crop_picker` already takes the rung as its
  `policy` parameter, so the split needed none. Asserted as a PAIR (`forage_crop_picker` expects
  `CROP TO TEND TO`, `forage_crop_picker_sow` expects `CROP TO COMMIT TO` *and* the absence of the
  other), because a header hard-wired to either string passes one frame alone. **A
  legal-but-marginal crop is NEVER disabled** — a 20%-share plant is a bad choice, not an illegal one,
  and being free to make it is the decision §4.3 exists to create; only the two flags disable anything.
  The selection (`_forage_assign_species`) is re-resolved **every render** by `_resolve_crop_selection`
  — the player's pick while it is still legal on this tile+rung, else the **highest-share legal** entry,
  which is the sim's own `default_species_for_rung`, so picking nothing and accepting the default behave
  identically. `""` is always a valid thing to send (non-committing rung, nothing legal, or an
  **already-committed** patch, which gets a **locked** picker instead of an editable one, since the
  commitment is one-way until it lapses). **A locked picker still lists the WHOLE basket** — same rows,
  same order, every one disabled, the committed species marked with `HudStyle.apply_button(btn,
  "primary", true)`, i.e. the policy picker's `selected_when_disabled` idiom, and the
  `FLORA_CROP_COMMITTED_HINT` line *beneath* the rows rather than in place of them. It collapsed to a
  lone crop name until #433 and that was a bug (playtest): the tile card two panels away listed
  `56% / 25% / 19%` while the sheet showed `Wild Emmer` alone, so the two surfaces disagreed about
  whether the tile grew one plant or three. **The test a surface has to pass is not "does this code
  reason about the basket" but "can this panel be READ as claiming the tile grows one plant"** — the
  first test is what let this site through the initial sweep. Both surfaces show the **standing**
  basket, never the projected weeded one, so they cannot drift apart mid-build. It rides the existing emit path: `_emit_assign_labor` gained
  a trailing `species` (defaulted `""`, so no other caller changed) → the payload → `Main` →
  `assign_labor <f> <b> forage <x> <y> [floor] [species] <workers>` — the **second** optional token,
  worker count always last, omitted entirely when empty. (The first is the FLOOR, an `f32`; the two
  are disambiguated by whether the token parses as one, which is also why a retired stance word
  cannot slip into either slot.)
  **THE PAYOFF, BESIDE THE SHARE** (`cultivateYieldRatio` / `sowYieldRatio` → `cultivate_yield_ratio` /
  `sow_yield_ratio`, read per rung by `_flora_entry_ratio`): a row reads `Wild Emmer 34% · 2.7×` —
  what committing this tile to this plant yields **relative to gathering it wild**. The sim folds the
  share AND the species' conversion rate into it through the same seams the real payout uses, so the
  client only **formats** it (`FLORA_CROP_RATIO_CLAUSE_FORMAT`, one decimal — the question is "better or
  worse than wild", not a second significant figure); **never do arithmetic on it here**, and note the raw
  per-species rate is deliberately unpublished (meaningless alone, and it would put the payoff formula
  in two places). Below `FLORA_CROP_BREAK_EVEN_RATIO` the row is **WARN-inked and fully pressable** —
  the ratio exists to stop a bad idea being invisible, never to forbid it, so nothing is hidden,
  clamped, sorted by or disabled on it. **`0` is the "cannot climb this rung" SENTINEL, not a number**
  (a real ratio is never 0), so a row greyed by the climbability flags prints no ratio at all.
  **ABOVE 1.0 IS THE NORM, so the verdict wording is keyed to 1.0 and never to an impression of the
  numbers.** The sim's ratio once omitted `tended_regrowth_gain` and understated every Cultivate figure
  by exactly 2× — a genuinely strong crop rendered `0.9×` over a tooltip calling it poor. Fixed sim-side;
  best-country ratios now run 2.3–2.7×. The tooltip therefore has **three tiers**, all relative to
  `FLORA_CROP_BREAK_EVEN_RATIO`: below it *"it loses to simply gathering here"*, at/above
  `FLORA_CROP_STRONG_RATIO` *"strong ground for it"*, and the honest middle *"worth committing to"*.
  Amber is now the exception rather than the rule on good ground, which is the intended read.
  **AND THE `→ then` TERM FOLLOWS THE SELECTED CROP** (`cultivatePayoff` / `sowPayoff` →
  `cultivate_payoff` / `sow_payoff`, carried through `SourceForecast.flora_basket_entries` and substituted by
  `_forecast_for_selected_crop`). Without it the forecast quoted a species-BLIND patch, so committing to
  Ground Nut displayed Wild Emmer's payoff and **the picker appeared to change nothing above it**. Same
  units and output-multiplier convention as the forecast `payoff` it replaces, so this is a
  **SUBSTITUTION, not a calculation** — do no arithmetic on it here. Only `payoff` is substituted; the
  ceiling and per-worker rate still describe the PATCH, which is what caps the stepper. The picker's own
  handler rebuilds the whole controls, so changing the crop moves the line on the same frame — pinned by
  the `forage_crop_then_emmer` / `forage_crop_then_groundnut` pair, whose assertion is that the two
  frames' forecast lines **differ** (`+1.35` vs `+0.45`); asserting the line merely *exists* would pass
  against a hardcoded one. **Carrying the payoff through `SourceForecast.flora_basket_entries` is the load-bearing
  half** — the substitution silently no-ops if the basket entry drops the field, which is exactly how it
  first failed. **A ZERO payoff is SUBSTITUTED, not skipped** (#419): the substitution used to bail on
  `payoff <= 0.0` and leave the *previous* crop's number standing, so picking a crop that pays no food on
  this rung left the `→ then` line asserting food it never delivers. The case is real — a sown Field is
  100% its crop, so a cash crop's `sow_payoff` is exactly `0` — and zero is the honest answer there.
  **A ROW STATES EVERY ACCOUNT IT PAYS, NOT ONE** (issue #419; `.claude/rules/client/labor-ui.md` → "A
  hunt pays TWO products" is the shared rule, and `SourceForecast.has_component` the shared gate). The
  face is COMPOSED — `FLORA_SHARE_FORMAT` base plus a ratio clause, a hay clause and ONE CLAUSE PER
  MATERIAL, each rendered only where its component exists (`_flora_row_face`), and the tooltip composed
  the same way — so a tended staple reads `Wild Emmer 70% · 2.7× · 0.04 fibre` and a cash crop
  `Flax 30% · 0.3× · 0.29 fibre`.
  - **It was three mutually exclusive whole-row formats picked by an if/elif chain**, so a row could
    state exactly ONE account, and the chain detected "cash crop" from the then-live
    `trade_payoff > 0`. **Every staple carried `trade_goods_per_biomass: 0.005`**, so that test fired on
    all 27 of them and printed every crop as trade-only — `Wild Emmer 39% · 0.4 trade`, with the ratio the rung exists to compare
    nowhere on the row. There is deliberately **no `role` on the wire and no threshold**: the components
    self-route, which is the client twin of the sim's own "the vector is the behaviour, the role is a
    display tag never branched on" (`flora_config.rs`). A payoff being non-zero says an account is
    *paid*, never which account *dominates*.
  - **THE NON-FOOD PAYOFFS ARE PER RUNG** — `_flora_entry_fodder_payoff` /
    `_flora_entry_material_payoff` take the `policy` and read `cultivate_*` or `sow_*`, exactly as
    `_flora_entry_ratio` already did. They read `sow_*` unconditionally before, so the Cultivate row
    quoted a *sown Field's* number. The tooltips name the rung's own noun (`FLORA_CROP_RUNG_NOUNS`) for
    the same reason.
  - **TWO decimals on the absolute accounts, one on the ratio.** The ratio's single decimal is the
    deliberate "no second significant figure" choice; the non-food clauses are absolute rates spanning
    two orders of magnitude within one basket (0.04 fibre for a tended grain's volunteers beside 1.08
    for a sown cotton Field on the same ground), and one decimal flattens the small end to `0.0` and
    loses exactly the comparison the row exists for. It is also `SourceForecast.picker_products`'
    precision.
  - **A cash crop's food ratio is a WARN-inked LOSS at rung 2, and must not be exempted.** The old chain
    let a non-food payoff suppress the food verdict entirely; but rung 2 *weeds* rather than replaces,
    so a tended cotton patch really does keep paying its volunteers' calories at a rate below gathering
    the tile wild. That surrendered calorie is the cost its material clause is the benefit of — the
    land-use tension, rendered.
  ### THE PATCH'S OWN MATERIAL RATES ARE A DIFFERENT QUESTION FROM THE PICKER'S

  Two surfaces on this card quote a plant's materials and they answer different questions. Confusing
  them is the easiest mistake here, because both are `{material_id, amount}` rows.

  | | asks | reads | renders on |
  |---|---|---|---|
  | **the CROP PICKER's rows** | what would ONE SPECIES pay if you built on it? | `sow_material_payoff` / `cultivate_material_payoff`, per composition entry, per RUNG | each basket row of the picker |
  | **the PATCH's rates** | what does THIS GROUND pay the crew standing on it now? | `patch_material_per_biomass` / `patch_per_worker_material` | the compose sheet's yields row |

  **THE COMPOSE SHEET'S ROW WAS MISSING FOR A RELEASE, and the client half was one argument.** A tile
  32% cotton and 26% tobacco composed a forage sheet reading `0.24 → 0.18 FOOD · — FODDER` and never
  mentioned the fibre and tobacco the gather actually banks (reported from a screenshot):
  `_forage_yield_model` passed FOUR arguments to `yield_rows` where its hunt twin passed five. Now it
  reads `0.32 → 0.15 FOOD · 0.09 FIBRE · 0.06 TOBACCO`, one row per material beside the food and the
  feed, through the same `forecast_inputs` composition the animal web uses — the keys are
  prefix-aware, so a patch and a herd are one derivation. `labor-ui.md` → "THE PLANT WEB GOT THE SAME
  ARGUMENT" owns the mechanism; **`forage_cash_crop_gather` is the frame.**

  **A TILE-LEVEL RUNG FIGURE WOULD BE WRONG, not merely redundant** — it would sum across the basket,
  and summing is the retired trade axis under a new name. That is why `FORECAST_PAYOFF_MATERIAL_KEYS`
  has HERD rungs only and the plant web is deliberately absent from it.

  ### WHAT A CASH CROP PAYS, PER MATERIAL (arc #527)

  The row's non-food clause used to be a single `trade` scalar. **That account is retired** — the sim
  wrote it on every harvest and read it nowhere, while a `credit_material_yield` beside every credit
  site already accounted the same take's concrete materials — so the row states MATERIALS now, and it
  states them **one clause per material**.

  - **`FloraShareInfo.sowMaterialPayoff` / `cultivateMaterialPayoff`** decode as
    `sow_material_payoff` / `cultivate_material_payoff` on each `composition` entry: an `Array` of
    `Dictionary`, each `{material_id: String, amount: float}` in units per turn, merged by id and
    ordered by the sim. `SourceForecast._material_payoff_rows` normalizes them (dropping a row that
    names no material — an id is what a row is FOR) and `flora_basket_entries` carries them through;
    `DrawerComposeController._flora_entry_material_payoff` picks the rung's own vector.
  - **NEVER SUM THEM INTO ONE FIGURE.** A "materials/turn" total is the retired trade axis under a new
    name, and it re-collapses the very distinction the materials model exists to keep — a mammoth hide
    and a hare pelt are both `hide` and are not the same thing.
  - **THE KEY IS ALWAYS PRESENT, AN EMPTY ARRAY INCLUDED, AND EMPTY IS A REAL ANSWER.** It means *this
    plant pays no material*, which must render as **no clause** — never as a `0.00`. That is the
    render-only-when-non-zero rule reaching one account further out, and it is why the fixture that
    carries it (`fodder_basket_tile_fixture`, both plants, both rungs) states `[]` EXPLICITLY rather
    than omitting the key.
  - **THE TWO RUNGS LEGITIMATELY DIFFER, and the picker renders each rung's own vector.** A sown Field
    is 100% its crop, so a grain Field quotes nothing; a *tended* patch keeps its neighbours as
    volunteers and honestly quotes the fibre they pay. On the reference tile that reads
    `Wild Emmer 70% · 3.2×` under Sow beside `Wild Emmer 70% · 2.7× · 0.04 fibre` under Cultivate —
    the same plant, two rungs, and the tended one is the one with the flax in it.
  - **THE MATERIAL NAMES ITSELF, AND THAT IS THE MARK IT WEARS.** `⇄` earned its job by being ONE mark
    for a whole scalar product; a material has a NAME (`fibre`, `hide`, `tobacco` — the catalogue ships
    no display name, so the id IS the display word), and a name is a better mark than an arrow saying
    only "not food". So a clause reads `0.29 fibre`, exactly as its neighbour reads `1.80 hay`, and
    there is no generic account left for a generic glyph to stand for. **Do not add one.**
  - **A ZERO FOOD SURVIVES BESIDE A MATERIAL CLAUSE, and that pairing is the whole land-use bargain.**
    `_crop_payoff_terms` renders the deal row's food figure through `picker_products`, whose
    `zero_account` keeps the honest `0.00 food` — so a sown cotton Field reads `0.00 food · 0.29 fibre`,
    which is exactly the trade the player is making.
  - **CROP_ROLE_CASH SURVIVES; only what it SAYS changed.** The role is still on the wire and still
    leads its basket row; its mark is `🧵` (`FoodIcons.CROP_ROLE_ICONS`, with `CropRoleSprites`' bundled
    art in front of it), and it now means *this plant pays a material, not calories* rather than *this
    plant pays trade goods*.

  **Frames:** `forage_crop_picker_cash` (Sow — the grain quoting nothing beside `Flax 30% · 0.72 fibre`)
  · `forage_crop_picker_cash_cultivate` (Cultivate — the SAME basket, the grain now quoting
  `0.04 fibre`, which is the two-rungs-differ pair and neither half is a claim alone) ·
  `forage_crop_committed` (the TWO-MATERIAL case: `Flax Fields 21% · 1.42 fibre · 0.31 grape`, which is
  the one frame a summed figure could not fake) · `forage_crop_picker_fodder` (the empty-array case —
  both plants, both rungs, and no material clause anywhere on the list).

  **A PICKER ROW WEARS THE SPECIES' OWN ART, ON THE BUTTON'S `icon` PROPERTY** (issue #339,
  `FloraSprites.texture_for`) — the same per-species family the basket rows above lead with, reaching
  its OTHER host kind. A row is a `Button`, which carries art natively, so it is **NOT** routed through
  `HudWidgets.build_marker_icon`: that builder's host is a `Label` in an `HBoxContainer` and it returns
  a `Control`, and the rule is written on the builder itself with the quarry picker as its precedent
  (`sprites-widgets.md` → "The host widget decides the mechanism"). Set **only when the texture is
  non-null**, so a no-art build is byte-identical rather than merely equivalent — with `expand_icon`
  and `HudFloraVocab.FLORA_CROP_ICON_MAX_WIDTH` (16), held under the 22.0 `FLORA_CROP_ROW_HEIGHT` so a
  256px source cannot set the row's minimum and break the MEASURED cap below; `COMPOSE_QUARRY_ICON_MAX_WIDTH`
  records the same trap on the compose row's WIDTH. Drawn UNTINTED — nothing may set `modulate` on it,
  the map markers' rule — a plant carrying no state and a row's state riding its ink and chrome.

  **SIZING — the picker's LIST scrolls within itself, and the cap is MEASURED**
  (`FLORA_CROP_LIST_MAX_HEIGHT`, derived as `FLORA_CROP_LIST_VISIBLE_ROWS × row + separations`, with the
  rows on the work board's compact idiom via `HudWidgets.compact` — default button chrome pads 9px top AND
  bottom and makes the whole picker unaffordable). The ComposeSheet's `CARD_MAX_HEIGHT` is deliberately
  NOT raised (that cap belongs to every compose card), so the picker lives in the room the sheet has
  left. **Five rows, so NO SHIPPED BASKET EVER HIDES A CROP** — the longest a tile can carry today is 5
  (the navigable-hex valley+fishery blend), and a picker that hides the best crop behind a scroll is the
  guess the payoff ratio exists to remove. Measured: the worst realistic compose (5 plants under
  Cultivate, Sow locked) lands the sheet at **528 of its 560 cap**. The cap is still a live guard, not
  dead code — F5 refines this coarse roster into a fine-grained one and baskets lengthen — and
  `forage_crop_picker_overlong` (a **synthetic 8-plant** tile, longer than any real one, labelled as such
  in the fixture) keeps the scroll path RENDERED so it cannot rot unseen. The marginal-crop warning rides
  each row's TOOLTIP rather than a standing hint line for the same budget reason (a line under the list
  costs ~40px, and the commit button is what pays). **What bought the rows was collapsing the OTHER
  rung's gate reasons, and issue #442 bought them outright instead** — the picker holds four stances
  and no gates at all, so there is no second rung's prerequisites on the card to collapse.
  `HudWidgets.build_policy_picker`'s `gates` parameter, its `collapse_other_gates` opt-in and the
  `▦ Sow — locked (2 requirements unmet)` rendering are all deleted (`labor-ui.md` → "What this
  deleted"); only ONE improvement is ever offered, and its reasons are the improvement control's own
  text. **The ui_preview ASSERTIONS that pinned the height claim survive and must stay green**:
  `forage_crop_picker` asserts the sheet has nothing left to scroll (i.e. `Forage` is on screen — it
  caught a 124px regression eyeballing would have shipped) **and**, in place of the retired collapse
  check, that exactly one improvement control is on the card (the Cultivate box present, no Sow box —
  asked of the CONTROLS, since the whole-sheet text search it replaced passed on an empty sheet);
  `forage_crop_picker_overlong` asserts the same at 8 plants. Change the row count and let the
  assertions answer, never assume. (Byte-diffing frames is **not** a
  valid instrument here — the harness has time-based pulses, so most frames differ run to run.) ui_preview: `forage_crop_picker` (Cultivate, the 5-plant navigable-hex basket —
  default lands on the highest-share legal row `1.4×`, a WARN `0.7×` still pressable, greyed rows with no
  ratio, the list scrolling internally + the assertion) / `forage_crop_marginal` (the all-marginal
  RollingHills tile — every legal crop below `1.0×`, all warn-inked, all pressable, the default still the
  highest-share one) / `forage_crop_picker_overlong` (the SYNTHETIC 8-plant tile — the internal scroll's
  only frame, plus its own on-screen-button assertion) / `forage_crop_picker_sow` (the SAME basket one rung up — only Wild Emmer survives
  and reads `4.2×`, which is what proves both the gate and the ratio are per-rung) /
  `forage_crop_committed` (the locked readout) / `forage_cultivate` (the 3-entry reference tile) /
  `forage_crop_then_emmer` + `forage_crop_then_groundnut` (the selection-tracking PAIR).
- **The forage compose's TWO ZERO-WORKER SUBMITS** (`_build_forage_assign_controls`; playtest defect,
  pre-existing). `workers == 0` is the **sim's unassign** (`server.rs`: *"Unassigning (`workers == 0`) is
  always allowed"*) and the Work zone's unassign paths depend on it, so the submit is gated on **"would
  this change anything"**, never on a raw count — a client-side floor of 1 would fix the no-op and break
  the unassign. `current` (pending-aware standing staffing on this tile for THIS band) splits the two,
  and **the button and the forecast line must agree in each**:
  - **0 and NOT currently assigned** → the command would do nothing: button **disabled**, still reading
    `Forage`, and the forecast drops its `→ then` promise. `Preparing` is staffing-scaled while the
    payoff is not, so an unstaffed row used to read `Preparing: +0.00 /turn → then +1.20 /turn` — a
    sequence the player is explicitly NOT on track for, since an unstaffed build meter never advances.
    It now states the payoff as a CONDITION (`INVESTMENT_FORECAST_UNSTAFFED_FORMAT` — *"Assign foragers
    to begin — prepared, this pays +1.20 /turn"*), keeping the number, which is how you decide the tile
    is worth staffing at all. The copy is deliberately SHORT — `Assign foragers — +1.20 /turn` — because
    the moment one worker is on it the full `Preparing: … → then …` line renders anyway. It doubles as
    the dead button's explanation, per the `_forecast_worker_cap` "a dead button is always explained"
    precedent.
  - **0 and currently assigned** → a real unassign: button **enabled**, renamed `Unassign`, and the
    forecast row is **suppressed entirely** — "assign foragers to begin" above an `Unassign` button
    tells the player two opposite things. No new warning was invented for it: what abandoning costs is
    already on the card in the rung's own policy hint (*"It must stay staffed or it goes feral"*).
  The unstaffed copy lives in the SHARED `_forecast_yield_row`, so the hunt/herd investment rungs get the
  same fix; it takes a `crew_label` so the sentence names hunters/herders/foragers correctly. ui_preview:
  `forage_unstaffed` / `forage_unassign`, each with assertions on the button state AND on the copy, so
  the pair cannot drift back into contradicting itself.
- **The GRAZE layer's wire terms** (Grazing Phase 2a, `docs/plan_grazing_foundation.md`).
  `TileState.grazeBiomass` / `grazeCapacity` / `grazeEcologyPhase` are decoded in
  `native/src/dict/map.rs tile_to_dict` (plain floats, not fixed-point; the ubyte phase code is
  resolved THERE into the same phase *strings* the herd/patch payloads carry, so the client keeps ONE
  ecology vocabulary), cached in `MapView.tile_graze` — **only for tiles that actually carry
  pasture**, mirroring the sim's `GrazeRegistry`, so "no pasture" is an *absent* reading — and
  cross-referenced onto `tile_info` by `_tile_info_at`. The trio **SPLITS across
  `FOW_DISCOVERED_HIDDEN_KEYS`** — and that list holds DECODED DICT KEYS, so it names
  `graze_biomass` / `graze_ecology_phase` (in it) beside `graze_capacity` (deliberately not),
  **not** the camelCase schema fields above. Exactly as the plant web's three split; see "Fog splits
  a stock from its CAPACITY" below. How the row itself reads — and why it sits directly under
  `Foraging` — is "The tile card's TWO FOOD-WEB ROWS" above.
- **Sedentarization — the faction page's SETTLING block** (`Hud.gd` `update_sedentarization`,
  dispatched from `Main.gd`, INGESTED by `FactionReadouts` and rendered by
  `FactionRollup._build_settling_block`): the player faction's `SedentarizationState.score` (snapshot
  `sedentarization[]`) reads as a head plus one row, the row keyed by the STAGE and valued by a
  block-glyph meter — `Settling / soft ▰▰▰▱▱ 62/100`.
  **It was a compact top-bar meter** (`SedentarizationLabel` in `TurnBlock`) until issue #450 retired
  that block. Two of its rules went with the label and one did not: the amber/cyan stage tint and the
  `score < 1.0` hide are gone (both were presentation choices for a one-line strip — the accessor
  hands the raw entry over and the zone decides), while `HudFormat.meter_bar` still takes a **0–100
  SCORE** and the score is already on that scale, so it goes in RAW. Its wider `METER_BAR_CELLS`
  companion went with the strip; the zone draws at the knowledge tracks' 5.
  The soft/hard threshold prompts themselves arrive in the command feed
  (`CommandEventKind::SedentarizationPrompt`). See `core_sim` Campaign Loop — Sedentarization.
- **The Intensification Ladder — THE TWO-METER SPLIT** (`docs/plan_intensification_ladder.md` §4.1;
  the arc's root fix). Two meters advance from one action and they are **different kinds of thing**;
  the client's whole job here is to never let them read as two numbers in a list:
  - **FACTION KNOWLEDGE — the faction page's KNOWLEDGE zone, and the ONLY place a knowledge meter
    appears.** `Hud.update_intensification` (dispatched from `Main.gd`) INGESTS all **five** tracks of
    `IntensificationKnowledgeState` (`intensification_knowledge[]`, decoded in `native/src/lib.rs
    intensification_knowledge_to_array`) — `cultivation` / `seed_selection` / `herding` / `penning` /
    `foddering` — and `FactionRollup._build_knowledge_block` renders one row each, in
    `KNOWLEDGE_TRACK_LABELS` order (each web's ladder, bottom rung first, so the block reads as two
    ladders climbing). A track is hidden until the faction begins it (the row is sparse), reads
    `known` once complete, else a **5-cell** bar + the live percent.
    **THIS WAS A TOP-BAR STRIP** — `⚒ Your people know: Cultivation ✔ · Herding ▰▰▱▱▱ 41%`,
    `IntensificationLabel` in `TurnBlock` — until issue #450 retired that block, and three of its
    rules were the strip's own and went with it: the `⚒ Your people know:` PREFIX (load-bearing there,
    because a bare strip read as a stat of whatever was selected; a zone under its own `KNOWLEDGE`
    head needs no such disclaimer), the bare `✔` badge the prefix's verb allowed, and the
    `KNOWLEDGE_STRIP_TRACKS_PER_LINE` wrap. That wrap is worth remembering as a HAZARD rather than a
    rule: the label lived in a content-sized block where Godot autowrap could not engage, so four
    tracks on one line ran off the right edge (the "Penning clipped" playtest report) and the fix was
    explicit rows. A zone is one row per track and cannot reach that failure.
    **The 5-cell bar survived and is now shared by name**: `FactionRollup.KNOWLEDGE_METER_CELLS` reads
    `FactionReadouts`' own const, so the page and the ingest cannot disagree about what half-learned
    looks like. **`HudFormat.meter_bar` grades a 0–100 SCORE**, so a `0..1` track is scaled by
    `PROGRESS_PERCENT_SCALE` on the way in — passing the bare fraction fills zero cells below 0.5, and
    that is exactly how every meter on the faction page shipped empty once.
  - **PER-SOURCE PROGRESS — the source's own drawer row, never the strip.** A herd's `Husbandry`
    (`domestication`) + `Corral` (`corral_progress`); a patch's `Cultivation` (`cultivation_progress`)
    + `Field` (`patch_field_progress`). Local to ONE source, decays if abandoned.
  - **THE BRIDGE — a gated verb's reason line** (`_hunt_policy_gates` / `_forage_policy_gates`,
    rendered under the policy picker by `HudWidgets.build_policy_picker`). This is the one place the two meet,
    and the one line that teaches the ladder: a KNOWLEDGE reason names the track, its live percent
    and the **practice** that fills it (`Your people know Penning 45% — ♻ Sustain-hunt a tamed herd
    to learn it`); a SOURCE reason names the meter and the **verb** that fills it (`This herd is 40%
    tamed — ◎ Tame it to finish`). Judge on `two_meter_split.png`, which since #442 stages a FULLY
    TAMED herd: the improvement control offers one rung, so a gated Corral needs Tame retired, and the
    KNOWLEDGE reason is the only one that surface can show. The SOURCE reason still renders where the
    gates are read for other purposes; it is `RungGates.hunt_gates`' answer, not the control's.
  - **The `KNOWLEDGE_UNLOCK_NOTES` one-shot feed nudge** fires per track on a real `<1 → >=1`
    transition (player faction only). Note `herding`'s note now names **Tame**, not Corral — see the
    gate reshuffle below.
  See `core_sim` intensification ladder — knowledge.

## The row is an AFFORDANCE, not a property — one predicate, three surfaces

**`DetailFormat.tile_is_gathering_site(tile_info)` is the ONE test** behind all three of the tile
card's answers to *"can anyone work this ground?"*, and it exists because they used to disagree:

| Surface | What it does on ground that is not a gathering site |
|---|---|
| `SelectionCardController._land_row_meta` | reads `No forage` |
| `DrawerComposeController._forage_compose_available` | offers no **Assign foragers** button |
| `SubjectDrawerController._tile_terrain_lines` | **states no `Foraging` row and no basket** (#464) |

The third disagreed with the other two for as long as the block existed. Each had open-coded the same
`String(tile_info.get("food_module", "")).strip_edges() != ""` — except the drawer, which had never
asked the question at all.

**It is the module KEY, never its label** — a tile with no site still ships the label `"None"`, which
would render as a site called "None". And **the wire only ever carries the curated sites**
(`foodModules` ← the sim's `FoodSiteRegistry`, a spatially-quota'd 130–134 entries per map — 8% of
land, biased toward fresh water since #466), so
presence *is* the answer; there is no "carries a food module but is not a site" case to distinguish
client-side, even though that describes most land tiles in the sim.

**THE SIM OWNS THIS RULE NOW, AND DID NOT BEFORE.** Until #464 the gathering-site requirement existed
in *one GDScript predicate* — `_forage_compose_available` — and nowhere else in the game: the sim
accepted `assign_labor … forage` on any patch, so the client was refusing a command the server would
have honoured, and the rule lived where the sim could not see it. It is now
`RungSiteRequirement::requires_gathering_site` on plant rungs 1–3, enforced in the command validators
(`.claude/rules/core_sim/cultivation.md`). The client half is a **reflection** of that rule, not the
rule itself.

**WHAT IS DELIBERATELY NOT GATED.** The remembered (Discovered) branch of `_tile_terrain_lines` takes
no site test and cannot: `food_module` is in `FOW_DISCOVERED_HIDDEN_KEYS`, so a remembered tile has no
reading to gate on, and inferring "not a site" from the redacted key would drop the `Foraging: — / K`
row from **every** remembered hex — the exact card `_assert_fog_stock_parity` exists to prevent. A
remembered card states each web's capacity and withholds its stock; whether the ground can be worked
*now* is a question about the present, which is what a remembered tile does not know. The build meters
(`Crop` / `Cultivation` / `Field`) are likewise ungated — they state a standing INVESTMENT, and hiding
one would hide work already paid for.

**When rung 4 (Farm) drops `requires_gathering_site`, the block returns on the ground it unlocks**, and
that reappearance is the discovery the rung is made of. ui_preview: `tile_panel_ungathered` (the #464
tile — the `tile_food_layers` fixture with its **site keys** cleared together, `food_module` and the
two that describe the same site, on distinct coordinates so the frame is its own; **every patch,
graze and composition key is identical**, which is what makes the pair a controlled comparison) with
four assertions: no `Foraging` row, no basket, `Grazing` still stated, and — the half without which
the rest pass against a producer that stopped emitting food-web rows entirely — the SAME tile as a
gathering site still stating both. Sabotage-verified: dropping the gate fails the first two and leaves
the last two green.

## Fog splits a stock from its CAPACITY, never one web from the other

**On a remembered (Discovered) tile both webs state their capacity and neither states its stock** —
`Foraging: — / 205` over `Grazing: — / 130`, the same two rows in the same order as the live card
(issue #462).

The rule this replaced cut between the WEBS: grass was "a property of the ground, remembered", while
the whole forage-patch payload was "live contents, redacted". Coherent, and it does not actually
separate the two — a stand of wild tubers is no less a property of the ground than a stand of grass —
so it just got applied to one of them. What it cost was a card on which the pasture was the ONLY stock
pair on screen, which is exactly the condition under which a reader carries the grazing capacity into
their model of the forage patch. It caused that in play more than once, including the case that
prompted the tile-card redesign: a floodplain reading `Pasture 130 / 130` while the harvest floor was
computed against the forage patch's 205.

**The line that does separate them runs inside each web.** It rests on a sim guarantee, not on taste:

- **A carrying capacity is ground.** `advance_forage_regrowth` recomputes `K` from the tile *every
  turn* and states the invariant itself — *"THE LAND OWNS `K` … no rung below 4 raises `K` and none
  lowers it, so a commitment changes only what the patch's biomass is made of"* — and `GrazePatch`'s
  is *"the tile's biome-derived graze capacity … the land's property, not any animal's"*. **No player
  action moves either.** So the value the client is sent for a hex it cannot see IS the value that hex
  last showed, and rendering it leaks nothing. That is what makes remembering it honest with **no
  last-known store anywhere in the client** — and there is none; "remembered" here has only ever meant
  "the sim says Discovered, so we hide some of what it sent us".
- **A biomass is live.** It moves every turn as the ground is grazed or gathered, by herds and rival
  bands a remembered tile cannot see, so a remembered reading is stale by construction. The ecology
  phase goes with it, being `classify_ecology_phase`'s reading OF that biomass.

**The sim ships both webs' payload for every tile every turn, with no visibility filter at all**
(`snapshot/map.rs`, `snapshot/subsistence.rs::snapshot_forage_patches` — neither takes a visibility
argument; the sim's only fog gates are the visibility raster and the herd display list). So this
redaction is wholly client-side, and the pre-fix `Grazing 130 / 130` on a fogged hex was not a memory
at all — it was that turn's live value, arriving through a hole in the redaction list.

### Where each half lives, and why the render is a BRANCH rather than a consequence

- `MapView.FOW_DISCOVERED_HIDDEN_KEYS` holds `graze_biomass` / `graze_ecology_phase` /
  `patch_biomass` / `patch_ecology_phase` and **not** `graze_capacity` / `patch_carrying_capacity`.
- `SubjectDrawerController._tile_terrain_lines` derives `stock_known` from the VISIBILITY STATE and
  threads it into the two symmetric leaves `_forage_stock_lines` / `_graze_stock_lines`, which share
  `_stock_value`. **The flag is not inferred from an absent key**: the pair's meaning is positional,
  so a card must state what it states by decision — a leaky fixture (and every ui_preview fixture is
  one; they set `visibility_state` and redact nothing) would otherwise render a false frame.
- `HudFloraVocab.STOCK_UNKNOWN_FORMAT` is the `— / %.0f` face, built structurally from
  `STOCK_UNKNOWN_GLYPH` so the row and the harness searching it cannot drift. The em-dash holds the
  numerator's place rather than the row being dropped, which keeps the pair parallel with the live
  card — and it is the one glyph that cannot be misread as a quantity. **That is the whole point of
  the form**: the `Remembered` chip, the unknown-contents note and the map's mist tint were all
  already on screen when the confusion happened. They label the TILE; nothing labelled the NUMBER.

### An unseen hex says so ONCE, and promises nothing it cannot do

**BOTH unseen states carried the same sentence twice**, plus the Sight chip saying it a third time:

| | chip | the drawer's line (**deleted**) | the roster note (**kept**) |
|---|---|---|---|
| Discovered | `Remembered` | `Last seen — information incomplete. Scout to update.` | `You remember the ground here, but not what's on it now.` |
| Unexplored | `Unexplored` | `Not yet scouted — send a band to reveal this area.` | `Nobody has been here. Send a band to reveal what's on this ground.` |

**The drawer emits ROWS; the one sentence is the roster's** (`_render_unknown_contents_note`, which
renders directly beneath the drawer's label, so the pair read as one paragraph saying one thing
twice). That is the cut — not "keep the better sentence", but *whose job is a sentence at all*.

**The remembered pair also promised what the verb cannot deliver.** Both its forms closed on
scouting, and scouting makes a hex **Discovered** — precisely the state being described — so it can
never take a hex out of it. Current contents need **sight**: a band standing there now. Reported from
play by a player who scouted a hex and found it already back to `Remembered` by the time they reached
camp, i.e. the copy was telling them to redo what they had just done. `OCCUPANTS_UNKNOWN_REMEMBERED`
is therefore trimmed to a bare statement, while **`OCCUPANTS_UNKNOWN_UNEXPLORED` keeps its verb** —
there sending a band genuinely does make the hex discovered. **FoW copy names a verb only where the
verb changes the state being described.**

**An unexplored hex now produces NO drawer rows at all** (nothing about that ground is knowable), the
first state in which `_tile_terrain_lines` returns empty — so `_render_land_drawer` gates
`_tile_detail.visible` on `lines.is_empty()`. A visible empty `RichTextLabel` is not free: it still
claims its line height and the drawer's separation, and would read as a blank gap between the land
row and the note. Frames: `tile_sight_unexplored` / `tile_panel_unseen` / `tile_sight_remembered`.

**AND THE SAME `lines.is_empty()` IS WHAT FORCES THE NOTE** — `_render_land_drawer` passes it to
`_render_unknown_contents_note(force)`, which otherwise skips itself on a NON-empty roster (there the
list's own `OCCUPANTS_UNSEEN_OTHERS_HINT` is the sentence, and the note would be a second copy). The
two rules collide on one real hex: an **Unexplored** tile carrying your own party, which is routine
because the sim excludes expeditions from fog reveal. With no rows, no compose block and the note
suppressed, every child of the drawer is hidden at once and the LAND subject renders as a blank capped
area under the divider — the whole card's content gone. The invariant is therefore a pair: *the LAND
drawer on an unseen hex is never empty, and the card never states the same unseen-contents sentence
twice.* Frame + four assertions: `tile_panel_unexplored_own_band` (two preconditions — no terrain rows,
a non-empty roster — then the note's visibility and its text, asserted on `%OccupantDetail` itself
because a PNG cannot tell a blank drawer from one that rendered fine). Sabotage-verified against the
unconditional roster skip: both content assertions fail, both preconditions still pass.

**The harvest-floor chart still correctly disappears**, and not because the capacity is hidden:
`SourceForecast.floor_chart_model`'s `known` gate needs `patch_regrowth_samples` (redacted) as well as
a capacity, so un-redacting the ceiling cannot light it. Planning a harvest on a hex you cannot see
stays impossible. The same holds for `escapement_room` (biomass-gated) and `take_draws_down`
(curve-gated).

**Guarded by ui_preview's `_assert_fog_stock_parity`** (frame: `tile_sight_remembered`), seven
assertions, each sabotage-verified: both webs render in the live order; each states its capacity with
the stock unknown; neither carries a phase; the basket does not render; the SAME fixture in sight
still states both stocks in full (the half without which the rest pass on a blank card); and — the
half a fixture alone cannot reach — a tile put through the REAL `FOW_DISCOVERED_HIDDEN_KEYS` still
states both capacity rows, reading identically to the unredacted one. That last pair is what would
catch `patch_carrying_capacity` going back into the list, which every other assertion would survive
while the live client shipped a card with no `Foraging` row at all.
