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
  `patch_owner` / `patch_biomass` / `patch_carrying_capacity`, all in `FOW_DISCOVERED_HIDDEN_KEYS`
  so a remembered tile redacts them). The
  card shows a **Cultivation** row: "N%" while the patch is being tended, "🌾 Tended Patch"
  (SIGNAL tint via `_cultivation_value_hex`) once `is_cultivated` — and, beside it, its own
  **Field** row for plant rung 3: "Sowing N%" → "▦ Field" (`patch_field_progress` / `patch_is_field`,
  `_field_label` / `_field_value_hex`). The two are **independent meters on one source** and never
  merge: `Sow` needs no prior patch (seed travels), so a Field may stand on ground that was never
  tended. See `core_sim` intensification ladder — cultivation, and the two-meter split above.
  It also shows an **Ecology** row (`patch_ecology_phase`) for **every** tile carrying a patch —
  cultivated or not, directly under **Forage biomass**. The phase gates whether cultivation can
  accrue at all, so it is the tile's headline condition; it is deliberately **not** gated on
  `is_cultivated` (it was, which hid it on exactly the ordinary forage tiles that needed it).
  Named and rendered **identically to the herd's Ecology row** — same `_ecology_phase_label`
  (neutral `Thriving`, warned `⚠ Stressed` / `⚠ Collapsing`) and the same `DetailFormat.ecology_value_hex`
  amber/red tint applied by `DetailFormat.detail_bbcode`, which now keys one shared `"Ecology"` case
  for both surfaces. The module's internal `seasonal_weight` is **not** printed on the `Forage:`
  row (it is a yield coefficient, meaningless to the player); it still drives the sim's yield.
  ui_preview: `food_tile` (Thriving) / `food_tile_stressed` (⚠ Stressed) / `tended_tile`.
  It also shows a **Forage biomass** row — `Forage biomass: 84 / 120` (`biomass` /
  `carryingCapacity`, decoded in `forage_patches_to_array`) — the patch counterpart to a herd's
  **Biomass** row, so a foraged patch reads like wild game does ("how much there is"). Foraging draws
  the biomass down and it regrows logistically toward the capacity (sim default 120). Rendered only
  when `patch_carrying_capacity > 0`, so a plain food-module tile with no patch stays bare.
- **Tile-card "What grows here" — the plant COMPOSITION** (Flora Roster F1,
  `docs/plan_flora_roster.md` §2; snapshot `ForagePatchState.composition:[FloraShareInfo]` →
  decoded in `native/src/lib.rs forage_patches_to_array` as a `composition` array of
  `{species, display_name, share}`, cross-refed by `MapView._tile_info_at` as
  `patch_composition`). A **SECTION** directly under `Forage:` — a quiet `What grows here` header, then
  **one indented 🌿 row per realized plant** (`🌿 Wild Grain 45%` / `🌿 Ground Nut 30%` / `🌿 Berry Scrub
  25%`) so the per-tile basket scans down the card the way the compose sheet's crop picker reads
  (`DetailFormat.flora_composition_lines` → `SubjectDrawerController._tile_terrain_lines`; the render is F5,
  upgraded from the earlier one-line `What grows here: A · B · C` value). The rows reuse the food/morale
  breakdown's 4-space `MORALE_BREAKDOWN_INDENT` but are tinted **neutral ink**, not the ▲/▼ two-tone — a
  share is descriptive, not a good/bad signal — so `DetailFormat.detail_bbcode` keys a dedicated branch off
  the shared 🌿 sprig (`FoodIcons.DEFAULT`, tested BEFORE the morale-indent branch since they share the
  indent). **No per-species flora icons yet** — the whole basket wears the one generic plant glyph; a
  per-species flora icon set is the roster-side F5 follow-up. It names the plants the tile's forage capacity
  is MADE OF — **naming decomposes, it does not add**: the shares sum to 1, so this says what the Forage
  number already on the card consists of; nothing about the economy changed. Three rules: the wire list is
  **already sorted** (share DESC, then species key ASC) and is rendered **verbatim, never re-sorted**; the
  **displayed percentages always sum to 100** — independent rounding can total 99/101, so
  `SourceForecast.flora_basket_entries` folds the remainder into the LARGEST share (the first entry), which
  is what stops a decomposition visibly failing to decompose; and an empty / absent list renders **no header
  and no rows** (a biome that carries no forage). **Deliberately NOT in `FOW_DISCOVERED_HIDDEN_KEYS`** — it
  is a pure function of the BIOME, like the terrain label or the river edges, so a remembered tile still
  knows what grows there (never-seen tiles are already covered by the `unexplored` redaction, and nothing on
  the patch can change it). ui_preview: `food_tile` / `tile_panel_land` (the fixture's shares naively round
  to 101%, so those frames ARE the rounding test), `tile_growing_here` + `tile_growing_here_variant` (TWO
  Alluvial Plain tiles with DIFFERENT baskets — Wild Emmer 70%/Flax 30% vs Cotton 55%/Flax 45% — the visible
  per-tile-realization proof on the card), and `tile_panel_no_forage` (no list → no section).
  **ONE ROW, TWO STATES — the COMMITTED crop** (Flora Roster S1, `docs/plan_flora_roster.md` §4.3;
  `ForagePatchState.committedSpecies` / `committedDisplayName` → decoded in the same
  `forage_patches_to_array` as `committed_species` / `committed_display_name`, cross-refed by
  `_tile_info_at` as `patch_committed_species` / `patch_committed_display_name`). Once a band works
  the patch under `Cultivate`/`Sow` it **commits to a single crop and the rest of the basket is
  displaced** — so the same row slot renders `Crop: Wild Emmer` (`FLORA_CROP_ROW`) **instead of** the
  basket, never beside it: the tile is one plant now, and listing the wild mix would name plants that
  no longer grow there. `committedSpecies == ""` means **the wild mixed basket**, not "unknown", so
  the row switches on it rather than treating it as missing data. `Crop` is well under
  `_split_detail_kv`'s 16-char key limit, so it aligns as a table row exactly like the key it
  replaces. The composition list stays on the wire either way — the card CHOOSES, it does not fall
  back. Committed-ness is patch STATE (unlike the biome-derived basket), but it needs no
  `FOW_DISCOVERED_HIDDEN_KEYS` entry: the row sits under `Forage:`, past the discovered early-return,
  so a remembered tile never reaches it. ui_preview: `food_tile_crop` (the committed twin of
  `food_tile` — the two frames differ in exactly that row).
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
  is information about the LAND, and hiding it would make the tile read poorer than it is. **A
  legal-but-marginal crop is NEVER disabled** — a 20%-share plant is a bad choice, not an illegal one,
  and being free to make it is the decision §4.3 exists to create; only the two flags disable anything.
  The selection (`_forage_assign_species`) is re-resolved **every render** by `_resolve_crop_selection`
  — the player's pick while it is still legal on this tile+rung, else the **highest-share legal** entry,
  which is the sim's own `default_species_for_rung`, so picking nothing and accepting the default behave
  identically. `""` is always a valid thing to send (non-committing rung, nothing legal, or an
  **already-committed** patch, which gets a locked read-only readout instead of an editable picker, since
  the commitment is one-way until it lapses). It rides the existing emit path: `_emit_assign_labor` gained
  a trailing `species` (defaulted `""`, so no other caller changed) → the payload → `Main` →
  `assign_labor <f> <b> forage <x> <y> [policy] [species] <workers>` — the **second** optional token,
  worker count always last, omitted entirely when empty.
  **THE PAYOFF, BESIDE THE SHARE** (`cultivateYieldRatio` / `sowYieldRatio` → `cultivate_yield_ratio` /
  `sow_yield_ratio`, read per rung by `_flora_entry_ratio`): a row reads `Wild Emmer 34% · 2.7×` —
  what committing this tile to this plant yields **relative to gathering it wild**. The sim folds the
  share AND the species' conversion rate into it through the same seams the real payout uses, so the
  client only **formats** it (`FLORA_CROP_ROW_FORMAT`, one decimal — the question is "better or worse
  than wild", not a second significant figure); **never do arithmetic on it here**, and note the raw
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
  first failed.
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
  rung's gate reasons — and that collapse is OPT-IN, deliberately narrow** (`HudWidgets.build_policy_picker`'s
  trailing `collapse_other_gates`, default **false**): three wrapped paragraphs explaining why *Sow* is
  refused while the player composes a *Cultivate* answer a question they did not ask and cost about a
  third of the card, so the forage compose asks for the collapse **only while a COMMITTING rung is
  selected** — i.e. exactly when the crop picker is on the card competing for height. **Every other
  picker (hunt, expedition, work board) and every non-committing forage compose is unchanged**, because
  spelled-out reasons are also how the ladder TEACHES: `forage_cultivate_locked`, `forage_sow_locked`,
  `herd_corral_locked*` and `two_meter_split` all exist precisely to show a NON-composed rung's full
  prerequisites, and a blanket collapse would put each of those frames' whole subject in a tooltip. When
  it does fire, the other rung reads `▦ Sow — locked (2 requirements unmet)` with the full list in the
  line's own tooltip (via `HudWidgets.set_label_tooltip` — a `Label` ignores the mouse by default, so a bare
  `tooltip_text` there is a silent no-op). **Four ui_preview ASSERTIONS pin all of this** and must stay
  green: `forage_crop_picker` asserts the sheet has nothing left to scroll (i.e. `Forage` is on screen —
  it caught a 124px regression eyeballing would have shipped) **and** that the collapse fired where it
  was bought; `forage_crop_picker_overlong` asserts the same at 8 plants; and `forage_sow_locked` +
  `two_meter_split` assert the collapse did **not** leak — the blast-radius guard for the shared picker.
  Change the row count and let the assertions answer, never assume. (Byte-diffing frames is **not** a
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
- **Tile-card Pasture rows — the ANIMAL-edible twin of Forage biomass** (`Hud._tile_terrain_lines`;
  Grazing Phase 2a, `docs/plan_grazing_foundation.md`). `TileState.grazeBiomass` / `grazeCapacity` /
  `grazeEcologyPhase` are decoded in `native/src/lib.rs tile_to_dict` (plain floats, not fixed-point;
  the ubyte phase code is resolved THERE into the same phase *strings* the herd/patch payloads carry,
  so the client keeps ONE ecology vocabulary), cached in `MapView.tile_graze` — **only for tiles that
  actually carry pasture**, mirroring the sim's `GrazeRegistry`, so "no pasture" is an *absent*
  reading — and cross-referenced onto `tile_info` by `_tile_info_at`. Two rows:
  `Pasture: 236 / 240` and `Pasture ecology: ⚠ Stressed`. The pair with `Forage biomass` **is** the
  point: what HUMANS can eat here (seeds/nuts/tubers, food-module tiles only) vs what ANIMALS can eat
  here (grass/browse, nearly every land tile) — *your best farm is usually not your best pasture*.
  - **Rendered only when `graze_capacity > 0`** — on a glacier the card prints **nothing**, never
    `0 / 0` (which would read as a starved pasture rather than an absent one). ui_preview
    `tile_pasture_none`.
  - **The ecology row reuses the shared path** — `_ecology_phase_label` + `DetailFormat.ecology_value_hex`, the
    same neutral/amber/red tint a stressed herd or a stressed forage patch gets. It carries its own
    row KEY (`PASTURE_ECOLOGY_KEY`) purely so a forage tile does not print two rows both named
    "Ecology"; `DetailFormat.detail_bbcode` keys both to the one helper — the styling path is not forked.
  - **Pasture is REMEMBERED knowledge, not live state** — it is emitted BEFORE the Discovered
    early-return and is deliberately **not** in `FOW_DISCOVERED_HIDDEN_KEYS`. Grass is a property of
    the GROUND (you can read a steppe from a ridge) and the biome above it is already remembered; what
    a remembered tile redacts is live *contents* (the bands and herds standing on it).
  - ui_preview: `food_tile` (the healthy pair — `Forage biomass 84 / 120` beside
    `Pasture 240 / 240 · Thriving`) / `tile_pasture_stressed` / `tile_pasture_none`.
- **Sedentarization meter** (`Hud.gd` `update_sedentarization`, dispatched from `Main.gd`):
  the player faction's `SedentarizationState.score` (snapshot `sedentarization[]`) shows as a
  compact top-bar block-glyph meter (`▰▰▰▰▰▱▱ 62/100 · soft`, `SedentarizationLabel` in
  `TurnBlock`), tinted amber (soft) / cyan (hard) by stage and hidden until the score is
  meaningful. The soft/hard threshold prompts themselves arrive in the command feed
  (`CommandEventKind::SedentarizationPrompt`). See `core_sim` Campaign Loop — Sedentarization.
- **The Intensification Ladder — THE TWO-METER SPLIT** (`docs/plan_intensification_ladder.md` §4.1;
  the arc's root fix). Two meters advance from one action and they are **different kinds of thing**;
  the client's whole job here is to never let them read as two numbers in a list:
  - **FACTION KNOWLEDGE — the top-bar strip, and the ONLY place a knowledge meter appears.**
    `Hud.update_intensification` (dispatched from `Main.gd`) renders all **four** tracks of
    `IntensificationKnowledgeState` (`intensification_knowledge[]`, decoded in `native/src/lib.rs
    intensification_knowledge_to_array`) — `cultivation` / `seed_selection` / `herding` / `penning`,
    in `KNOWLEDGE_TRACK_LABELS` order (each web's ladder, bottom rung first, so the strip reads as
    two ladders climbing). Prefixed **`⚒ Your people know:`** (`KNOWLEDGE_STRIP_PREFIX`) — that
    prefix is load-bearing: it is what stops the strip reading as a stat of whatever is selected.
    A track is hidden until the faction begins it (the row is sparse), reads a bare `✔`
    (`KNOWLEDGE_KNOWN_BADGE` — the prefix already supplies "know") once complete, else a
    **5-cell** bar + the live percent. **The narrow bar + the bare ✔ are not cosmetic**: at the
    shared 10-cell `HudFormat.meter_bar` width plus the word "learning", four tracks overflowed the top bar
    and clipped the last one off-screen (caught in `two_meter_split.png`). `HudFormat.meter_bar(score, cells)`
    takes the width as an explicit param, so Sedentarization is untouched. **AND the strip WRAPS** —
    even narrowed, four tracks on one line ran off the right edge (the "Penning clipped" playtest
    report), so `update_intensification` groups the tracks into rows of `KNOWLEDGE_STRIP_TRACKS_PER_LINE`
    (2) joined by explicit `\n` (the prefix rides the first row). The label lives in the content-sized
    right-docked `TurnBlock`, so Godot autowrap can't engage without a bounded width — the explicit rows
    are what guarantee no track is ever lost off the edge, at any window width or ladder length.
  - **PER-SOURCE PROGRESS — the source's own drawer row, never the strip.** A herd's `Husbandry`
    (`domestication`) + `Corral` (`corral_progress`); a patch's `Cultivation` (`cultivation_progress`)
    + `Field` (`patch_field_progress`). Local to ONE source, decays if abandoned.
  - **THE BRIDGE — a gated verb's reason line** (`_hunt_policy_gates` / `_forage_policy_gates`,
    rendered under the policy picker by `HudWidgets.build_policy_picker`). This is the one place the two meet,
    and the one line that teaches the ladder: a KNOWLEDGE reason names the track, its live percent
    and the **practice** that fills it (`Your people know Penning 45% — ♻ Sustain-hunt a tamed herd
    to learn it`); a SOURCE reason names the meter and the **verb** that fills it (`This herd is 40%
    tamed — ◎ Tame it to finish`). Judge on `two_meter_split.png`.
  - **The `KNOWLEDGE_UNLOCK_NOTES` one-shot feed nudge** fires per track on a real `<1 → >=1`
    transition (player faction only). Note `herding`'s note now names **Tame**, not Corral — see the
    gate reshuffle below.
  See `core_sim` intensification ladder — knowledge.
