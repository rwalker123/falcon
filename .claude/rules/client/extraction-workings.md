---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_deposit_vocab.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/RungLadder.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/RungGates.gd"
  - "clients/godot_thin_client/native/src/dict/deposits.rs"
  - "clients/godot_thin_client/tools/ui_preview/chapters/workings.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/DrawerComposeController.gd"
---

# Workings — the client half of the wood-and-stone producers

The sim side is `.claude/rules/core_sim/extraction.md`, which is authoritative for what every field
on the wire MEANS, and `docs/plan_extraction.md` is the design of record. This file is what the client
does with them. Read the sim one first — most of the traps here are its traps, arriving one layer out.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_deposit_vocab.gd` (`HudDepositVocab`) | The WORKINGS vocabulary leaf — one reader per field on a `deposits` row, one reader per field on a `deposit_rungs` CATALOG row (`catalog_*`) plus the branch-filtered walks over it (`branch_ladder` / `ladder_next_entry` / `ladder_rung_teaching`), the keeping verdict (`owes_keeping` / `is_short` / `is_at_risk`, the bool-before-the-number rule), and the FIVE composers the three surfaces state a working with: `deposit_lines` (the tile card's rows), `deposit_row_value` (the roster's value cell), `deposit_verdict` / `runway_aside` / `deal_label`+`deal_value` (the sheet's readout). §7's fork lives inside it, once (`renews` → `supply_clause`/`runway_clause`/`deposit_verdict`/the DIAL). It also owns the ESCAPEMENT layer (issue #650): `rung_floor_fraction_of` / `per_worker_biomass_of` / `regrowth_samples_of`, the ONE `max` composition (`composed_floor`), the forecast-shaped view of a working (`forecast_source`) and the room above the dial (`room_next_turn`), plus the three-state take/runway readers (`assigned_cutters` / `assigned_take` / `stated_take` / `stated_runway`) and the standing rung's lesson (`standing_lesson` / `standing_lesson_known`). A vocab module with static funcs, the `hud_route_vocab.gd` shape: it reads `SourceForecast` / `DetailFormat` / `HudFormat` / `HudComposeVocab` / `HudSelectionVocab` / `HudLoadoutVocab` / `HudWorkVocab` / `HudConst` / `RungGates` / `HudStyle` inside functions only, never in a `const`, so it adds no load cycle |
| `ui/hud/DrawerComposeController.gd` → the `build_deposit_drawer_actions` family | The tile card's TWO compose actions and the sheet behind them (`_fill_deposit_branch` / `_tile_workings_of_branch` / `open_deposit_compose` / `_build_deposit_assign_controls` / `_build_deposit_offer_line` / `_mount_deposit_readout` / `_deposit_source_key`), filling `%ForestryAssignControls` and `%ExtractionAssignControls` — **one container per BRANCH**, since a wooded highland offers both at once. The ESCAPEMENT half is `_deposit_chart_model` (the ONE place the two floors are composed and the teaching note re-priced at the dial's own value), `_deposit_floor_takes` (the presets' hover metric) and `_deposit_yield_model` (the take, the `now → after` pair and the `renews` gate on the yields note). Its commit is the only thing it emits: `assign_labor <f> <b> extract <x> <y> <material> [floor] <n>`, through the shared `_emit_assign_labor` |
| `ui/hud/SubjectDrawerController.gd` → `_tile_terrain_lines`' deposit loop | Where the ROWS are appended — with the rivers, **above the Discovered early return**, and the one place the catalog join is resolved and threaded in. The ROAD block is composed on that same fog-safe side and appended LAST on both branches (`roads.md`); the deposits are emitted where they are composed |
| `ui/hud/RungLadder.gd` → `deposit_track` / `_deposit_pile` / `_deposit_tooltip` / `deposit_building_verb` | The deposit branches' TRACK — `route_track`'s sibling, emitting the same `ROW_*` shape into the SAME `build_track` renderer |
| `ui/hud/RungGates.gd` → `deposit_gates` / `deposit_gates_for` / `deposit_row_refusal` / `deposit_tooltip_refusals` / `_deposit_crew_refusal` / `_deposit_craft_refusal` | The FIVE refusals, keyed on the RUNG rather than the verb, as `{kind, short, long}` records in the route branch's own shape. `cutters` is a REQUIRED parameter, a `deposits` row publishing no crew — see "the gates" below |
| `Main.gd` → `format_improvement`'s `IMPROVEMENT_MATERIAL_TARGETED` arm | The three verbs' GRAMMAR — `fell|coppice|quarry <faction> <x> <y> <material>`, `cultivate`'s shape with the material on `assign_labor extract`'s own trailing position. A fourth ARM beside the herd-targeted and band-targeted ones, refusing on an empty material as the band arm refuses on `IMPROVEMENT_NO_BAND`. The verb list is `SourceForecast.DEPOSIT_IMPROVEMENTS`, which exists because a token SHAPE is not something the wire's rung catalog states |
| `ui/hud/BandPanelController.gd` → the `_workings_roster_*` family + `_open_deposit_track` / `_emit_deposit_declaration` / `_workings_roster_cutters` / `_deposit_track_queue` / `_deposit_ladder` | **THE WORKINGS ROSTER, WHOSE HEAD IS THE `Workings` POOL AND WHOSE ROWS DECLARE** — `_workings_roster_models` (the band's own `extract` row as the membership filter, the material-led locator, the stable nearest-first sort), `_workings_roster_unseen` (case 2, off the cohort's published `quarrywork_demand`), `_build_workings_roster_head` (the title, the shortfall mark, and the pool's compact stepper — the ONLY control that staffs `quarrywork`), `_build_workings_roster_block` / `_row`. Each ROW carries the declaring `⌃` and nothing else. `_workings_roster_cutters` is the CREW gate's whole input — the take crew on one working, keyed through the `(tile, material)` pair like every other join here, and pending-aware; it is resolved at BOTH call sites (the row's `has_track` probe and the open), because a mark that appeared on a working the card would refuse is the disagreement the gate exists to end. Its reserved height is resolved once in `_fill_work_zone_column` and spent in BOTH `build_queue_rows_max` and `_work_board_capacity` |
| `ui/hud/hud_work_vocab.gd` → the `WORKINGS_ROSTER_*` family + `ROLE_NAME_QUARRYWORK` + `RUNG_TRACK_STATE_GROUND_GIVES` | The roster's words, metas, its `WORKINGS_ROSTER_HEAD_HEIGHT` (21, measured), its `workings_roster_height`, the pool's own name/hint/coverage sentence, the row mark's own handle and hover, and the seventh rung state's SECOND word. They live here rather than in `HudDepositVocab` because the geometry of the Work zone belongs beside the three blocks that share it and the state enumeration is one table |
| `ui/hud/HudWidgets.gd` → `zone_head`'s trailing `title_tooltip` | The one shared-layer change the roster made: a head whose readout is a CONDITIONAL mark needs its hover on the TITLE, because `readout_tooltip` rides a Label built only where a readout is stated |
| `ui/hud/DetailFormat.gd` → `Context.deposit_rows` + its `_value_hex` arm | ⛔ **THE ONE ARM OF THAT DISPATCH KEYED ON MEMBERSHIP RATHER THAN ON A LITERAL.** Every other row key is a constant; a working's is `Wood` or `Stone` — `materials.json`'s ids, config this client may not spell — so the PRODUCER says which keys it wrote, exactly as `row_tooltips` does |
| `ui/hud/HudBandLaborState.gd` → `set_deposits` / `deposits` / `quarrywork_pool_state` / `extract_assignment_of` / `workers_for_extract` / `effective_extract_workers` / `source_crew_pool_extract` | The section held WHOLE (the roster asks a whole-list question), the pool's three cohort fields read with no arithmetic, and the per-working readers — each matching on the tile **and** the material, because that pair is the row's identity |
| `ui/hud/ComposeState.gd` → the `deposit_*` group | The composition: a source key, a crew, **a floor and its autofill one-shot**, the acting band and the band it was seeded from, and a kit. Still **no take species, no commit crop and no second axis** — a working takes one material and its rung is declared from the Work board. The floor is real on BOTH branches and offered on one: on a finite seam the member sits at its default and rides the command as the sim's own. `seed_deposit(count, floor)` seeds both from the band's own `extract` row |
| `ui/hud/HudBandLaborState.gd` → `floor_for_extract` | The dial's SEED, and the rule it exists to keep: a reopened sheet seeds from the ASSIGNMENT, never from `DepositState.floor` (see the note under the decoder's row) |
| `MapView.gd` → `_ingest_deposit_workings` / `_workings_on_tile` / `deposit_tile_lookup` | The per-TILE index the card's rows and its two actions read out of, `_ingest_road_network`'s twin. ⛔ **It does NOT de-duplicate on the tile** — two rows on one hex is the ordinary case here — and it holds the frame's rows **by reference** with its own profile span (`layers.deposits`), this being the widest section the client ingests |
| `native/src/dict/deposits.rs` | `deposits_to_array` — one dict per DEPOSIT-BEARING TILE, keyed `(tile, material)`, carrying the live working's state where a band has opened one — and `deposit_rungs_to_array`, the per-world CATALOG for both branches, `route_rungs_to_array`'s twin. The module header carries the whole field contract. The escapement four are appended last: `floor` · `rung_floor_fraction` · `per_worker_biomass` · `regrowth_samples`, the curve through the SHARED `subsistence::regrowth_samples_packed` so an ABSENT vector stays EMPTY (*no curve was sent*) and a quarry's all-zero one stays a reading (*this does not grow*) |

## ⛔ THE WORD "QUARRY" IS TAKEN, AND IT MEANS THE HUNTED ANIMAL

The compose sheet's field rows are `Band:` · `Kit` · `Quarry`, and `Quarry` there is the PREY
(`labor-ui.md` → "A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED"). A player reads both surfaces in
the same minute, so **no player-facing string in this arc may use the word for a deposit, a pool or a
roster.** The crew nouns are **`Foresters`** and **`Diggers`**, the pool's noun is **`Workings`** —
the sim's own word for a live deposit a band has opened — and the MATERIAL names the thing being
worked (`Wood`, `Stone`).

**The RUNG's own name is the exception, and it is not a violation**: `extraction:quarry`'s
`display_name` is `Quarry` on the wire, so the ladder row, the pointer line's verb and the deal row's
`ONCE QUARRIED` all say it. That is the sim naming a rung, not the client naming this branch, and it
appears only where a rung is the subject.

`quarrywork` survives as the server's command token and as `HudConst.LABOR_KIND_QUARRYWORK`. That is
grammar, not copy: it appears on no label, no hint and no tooltip. `HudWorkVocab.ROLE_NAME_QUARRYWORK`
is `"Workings"`, and the pool card, its hint, its coverage sentence and the roster head all read it.

## ⛔ THREE SURFACES, ONE PER QUESTION — AND THE `Workings ▸` POPUP WAS A FOURTH

Issue #650. The branch shipped with a single tile-card action opening a `PopupPanel` that was a
readout, a crew stepper, a bill and a countdown at once — **a fourth UX pattern in a client that
already has three**, reached tile-first from a card whose own rows said nothing about the ground.

It is replaced by the three surfaces this client already ships, one per question a player asks:

| the question | the surface |
|---|---|
| *what is on this ground* | a `Key: value` ROW per material on the TILE CARD, plus a blank-key payoff row where the rung buys something |
| *put a crew on it* | `Assign foresters ▸` / `Assign diggers ▸` — the FORAGE SHEET's spine, with the elements a deposit has no concept for absent |
| *take it up a rung* | the shared `RungLadder` TRACK, on the WORK BOARD where `cultivate` and `sow` are pressed |

**Gone with the popup:** `WORKINGS_ACTION_LABEL` / `_META`, `WORKINGS_CARD_META`,
`WORKINGS_CREW_STEPPER_META`, `WORKINGS_BLOCK_META`, `%WorkingsControls`, the whole
`_open_workings_card` / `_fill_workings_card` / `_build_workings_block` / `_workings_row` /
`_build_workings_crew_row` / `_build_workings_band_picker` / `_ensure_workings_card` /
`_dismiss_workings_card` family, `_default_workings_band` (the sheet resolves its actor through the
shared `_band_working_source` ladder) — and in the vocab, `CARD_TITLE`, `CARD_BLOCK_HEAD_FORMAT`,
`CARD_STOCK_ROW` / `CARD_STOCK_FORMAT` / `stock_value`, `CARD_CREW_ROW`, `CARD_BUILD_ROW`,
`CARD_UPKEEP_ROW` and `CARD_REVERTING_ROW`.

**What survived it, and why each has a reader:** `upkeep_value` and `reverting_value` state the
FIGURES on a hover (`deposit_card_tooltip` / `deposit_roster_tooltip`), `build_value` is the ladder's
face for the row being built, `supply_tooltip` is unchanged, and `CARD_CREW_HINT` is the crew
section's hover on both sheets.

## ⛔ ONE TILE HOLDS TWO DEPOSITS, AND EVERY SURFACE IS KEYED `(tile, material)`

A wooded highland carries timber AND rock, and working one is not working the other. So the tile card
draws **one row per MATERIAL**, the hex grows **one action per BRANCH**, and each sheet is about ONE
working.

**A surface that rendered one row per TILE is the defect the whole `material` field exists to
prevent**, and it is a defect that reads as a working simply not being there. Every join in this arc
therefore carries both halves:

- `HudBandLaborState.pending_key` gained a FIFTH token for it. Without it a band's Wood edit and its
  Stone edit on one hex overwrite each other in the optimistic overlay — `roadwork`'s
  two-queued-roads trap with a second axis. It is OPTIONAL and no other kind reads it, so every
  existing call site is untouched.
- `effective_worker_map` reads the material off the CONFIRMED row for the same reason, or the pending
  row keys apart from the row it is shadowing and both draw.
- `Hud._emit_assign_labor` reads the material off the `species` token **on the `extract` kind alone**:
  on a forage row that same token is the CROP COMMIT, which is not part of that row's identity and
  must not enter its key.
- `extract_assignment_of` / `workers_for_extract` / `effective_extract_workers` /
  `source_crew_pool_extract` all match on the pair, or the Stone sheet opens on the Wood crew's count.
- **The compose SUBJECT KEY is `x,y:material`** (`DrawerComposeController.DEPOSIT_SUBJECT_KEY_FORMAT`),
  so `ComposeState`'s `_deposit_key` cannot let one hex's two sheets overwrite each other's crew.
- The roster's row meta and the row's declaring mark are both `"<x>,<y>:<material>"`, so a harness can
  say *this working* rather than *a working on this hex*.
- `_standing_assignment_extract` is `_standing_assignment`'s TWIN rather than a fourth arm of it, for
  exactly this reason: a tile-keyed lookup would answer the Wood row for a Stone sheet.

## ⛔ EVERY LABEL, PRICE, PAYOFF AND GATE COMES OFF `SubsistenceSection.depositRungs`

Issue #650's other half. The branch shipped with `HudDepositVocab.RUNG_LABELS` — a hard-coded five —
and the wire publishes a catalog now: one row per rung of BOTH branches, per world, carrying
`rungKey · branch · order · displayName · verb · unlockKnowledge · requiresRung · earnsKnowledge ·
workCost · upkeepWorkPerTurn · buildMaterialCost · buildMaterialId · buildWorkPerWorkerTurn ·
yieldPerWorkerTurn · recoveryFraction · regrowthMultiplier · minDepositCapacity`.

**So a rung added to `intensification_ladder.json` appears on all three surfaces with NO client
edit** — the property the route branch's own catalog bought, arriving here for the same reason. It is
diffed WHOLE like `kits` and `routeRungs` and cleared at the world boundary
(`FactionReadouts.reset_world_state`), because a delta never restates a per-world constant and the
previous game's rungs would otherwise still be on the card.

⛔ **RETIRED WITH IT: `RUNG_LABELS`, `RUNG_ORDER_FORESTRY`, `RUNG_ORDER_EXTRACTION`, `rung_label`,
`next_rung_key` and `next_rung_label`.** The five `RUNG_KEY_*` consts SURVIVE — they are the wire's
own join keys, spelled once for the harnesses that stage a catalog and a working standing on one of
its rungs.

⛔ **ONE VECTOR CARRIES TWO LADDERS, WHICH IS WHY `branch` IS A FIELD.** `order` is a climb order
WITHIN a branch, so every walk either filters on `branch` first or is a bug — a shared order offers a
coppice above a quarry. `branch_ladder` is the filter and nothing reads the raw array.

## ⛔ A ROW DESCRIBES THE GROUND; THE WORKING IS ITS STATE (issue #650)

The section publishes **one row for every DISCOVERED tile that holds a deposit** — `foragePatches`'
own shape — and merges the registry's live `DepositSource` in where a band has opened one, deriving
the opening state where none has. So **the presence of a row is not the presence of a working**, and
most rows on a revealed map are ground nobody has touched: 3,245 of them on the shipped 80x52 at full
reveal, against `foragePatches`' 2,113.

Publishing the registry alone is what made the whole feature unreachable — the affordance is built
off these rows, and the registry is filled lazily by a crew being put on a working, so a fresh world
offered no way to open the first working anywhere on the map.

**`HudDepositVocab.is_unopened` is the client's reading of the difference, and it is a FINGERPRINT
rather than a flag.** The wire carries no *has a band opened this* bool, so the test is the whole of
`DepositSource::opening`'s state — owes no keeping, nothing banked on the ladder
(`ladder_position <= LADDER_UNSTARTED`), a seam at full `capacity`, nothing queued, and nothing taken
last turn. Every term of it moves the moment anybody does anything to the ground.

⛔ **`actual_take == 0` ALONE IS NOT THAT TEST, and reading it as one hides a working the player is
paying for.** A crew takes nothing in a dead season, behind a stalled build, or with the stock at its
rung's floor — all three of which the conjunction excludes on a different field. What the fingerprint
cannot separate is a working standing at its branch's free floor on a full seam with nothing queued
and nothing taken, which is field-for-field the opening state and costs the player nothing.

**What untouched ground SAYS, on each surface.** The tile card's row is the seam's own figure plus the
free floor's name (`Wood  600 · Deadfall`) — a full seam states ONE number, no hazard word of either
kind, and no payoff row, because a floor buys nothing over itself. The two actions still appear, which
is what OPENS the working. The ROSTER never lists it at all, membership being the band's own `extract`
row. And `deposit_row_value` — the roster's composer, which does meet untouched rows the band holds —
appends `DEPOSIT_UNOPENED_WORD` and returns, carrying no over-cut clause, no runway, no hazard mark.

⛔ **`not being worked` IS THE IDLE WORKING'S SENTENCE AND MUST NOT BE FLATTENED INTO THIS ONE.**
`RUNWAY_NO_TAKE` on a seam somebody opened and walked away from is a real reading — that working WILL
run out — and on ground with no working it is a reading of a thing that does not exist.

**The ROSTER is unaffected, and that is by construction rather than by a filter added for this.**
`_workings_roster_models` skips any row the band has no `extract` assignment on, so untouched ground
is never listed however many rows arrive; the pool's three figures are cohort fields with no
client-side arithmetic, so the count and the shortfall mark cannot move either.

### The section is the widest the client ingests, so the ingest holds it BY REFERENCE

Measured live (80x52, `earthlike` / seed 0 / `late_forager_tribe`, release server, fog OFF so every
row publishes), on the frames that carry the section:

| | `layers.deposits` | `sites.forage` |
|---|---|---|
| with the ingest's `duplicate(true)` | **7.5 – 8.3 ms** | 0.9 – 1.5 ms |
| holding the rows | **2.0 – 2.2 ms** | 0.9 – 1.7 ms |

3,245 deep dict copies a frame, in other words, for a section a quarter of `foragePatches`' bytes —
and the copy bought nothing: nothing downstream stamps a derived key onto a deposit row, and
`HudBandLaborState` has always held the same array by reference. The rule and its two halves are
`turn-profiling.md` → "Snapshot sub-trees are HELD BY REFERENCE"; `snapshot_alias_guard` pins this
ingest with the other five, asserting BOTH of one hex's rows by identity so a lookup that kept one of
them cannot pass. **Its profile span is its own** (`MapView.PROFILE_LAYERS_DEPOSITS`) rather than
folded into the road network's, which is where it sat while it was invisible.

## ⛔ THE BUTTON'S SECOND LINE IS IN THE WORKING'S OWN ACCOUNT, NEVER IN FOOD

The `Assign foresters ▸` / `Assign diggers ▸` control's second line is the shared standing summary
(`DrawerComposeController._standing_summary_model` → `SourceForecast.source_yield_readout`), the same
one a patch and a herd render. **A working pays no food**, so its zero belongs to its MATERIAL:
`♻ 2 foresters · +0.00 wood`, never `· +0.00 /turn`. The mechanism is one rule shared with the
plant and animal webs — `labor-ui.md` → "a WORKED ROW's zero account is decided by its KIND" — and it
is keyed off the row's `material`, the same half-identity every other join in this arc carries.

⛔ **THE NUMBER IS THE SIM'S AND THE UNIT IS THE CLIENT'S.** `LaborAllocation.last_yields` is written
by turn resolution and, for `Forage` and `Hunt`, by the assign-time forecast seed
(`core_sim/src/bin/server.rs` → `seed_source_yield`); a working's material take rides the same
`material_yield` slot. What the client decides is which account an empty take is empty IN — never
whether to project one, which would be the forecast/actual split that seam exists to close.

**`workings_just_assigned` is the frame for it**, and it is a PAIR on one hex: a forage crew and a
forester crew both committed this turn, nothing resolved on either. Every other frame in this chapter
was authored with a resolved take, which is exactly why the harness rendered a figure where the live
game rendered `+0.00 /turn` — the broken state was the one state nothing staged. Its claims are the
UNITS, not the figures, so a seeded row renders more information through the same assertions rather
than failing them. `_working_band_fixture` was corrected with it: it carried the take in
`actual_yield`, the FOOD account, which no `Extract` row can pay.

## ⛔ `DepositState.floor` IS A REPORT, AND NOTHING ON THE SHEET SEEDS FROM IT

The wire's `floor` is where THIS TURN's crews stopped, kept at the DEEPEST floor any band cutting the
working named (`DepositSource::last_floor`, a minimum because a floor is not additive), and `0` where
nobody cut it — the identity of the composition's `max` rather than a strip order.

⛔ **A COMPOSE SHEET STATES WHAT *ONE BAND* IS ASKING FOR, so its dial seeds from that band's own
`extract` row** (`HudBandLaborState.floor_for_extract`), which is `labor-ui.md`'s crop-seeding rule
applied to the floor. Seeding from the source-level field would silently adopt another band's deeper
order; and on ground nobody has opened it reads `0`, which is *strip it bare* — the one value that
must never be reached by accident, which is exactly why `floor_for_extract` answers the DEFAULT for an
absent assignment rather than the wire's zero.

**Its consequence is already `reachable`**, which the sim composes at exactly this value — so the
field is decoded for the row's completeness and has no GDScript reader. Adding one is not the way to
answer a question about a composition.

## ⛔ THE READOUT IS DECIDED BY `regrowth_rate > 0`, NEVER BY `branch`

A flint scatter and a quarry are **both `extraction`** and read differently — same skill, same ladder,
and only one of them runs out. `HudDepositVocab.renews` is that fork, it is the only fork, and every
composer that needs it calls it rather than re-deriving:

| `regrowth_rate` | the tile row / the roster cell | the FLOOR DIAL | the sheet's readout note | the sheet's verdict | the sheet's aside |
|---|---|---|---|---|---|
| `> 0` | the OVER-CUT word — `⚠ overdrawing` | offered — presets + chart | `renewable` (HEALTHY), or the overdraw flag + the SHEET's noun (WARN) | the SHARED harvest one — *Reaches the floor in 19 turns.* / *At the floor and holding it* / *settles at 64% — 7 foresters would reach the floor* | the floor hint and the teaching line |
| `== 0` | the RUNWAY — `75 turns left` (roster only) | none: rock does not come back | nothing: it renews nothing | *Gathering reaches 330 of 2,200. A quarry would reach 1,870.* | *Runs out in 275 turns at this rate.* |

⛔ **THE RENEWING SHEET'S VERDICT USED TO BE THE OVER-CUT SENTENCE AND IS NOT ANY MORE** (issue #650).
*Cutting 7.2 a turn against 4.5 that grows back* is true and is an OBSERVATION; with a dial there is a
question to answer instead — does THIS crew get the stand down to where it was told to stop, and if
not how many hands would. **The over-cut fact is not lost**: it is the `⚠` on the yields row directly
above. `HudDepositVocab.deposit_verdict`'s renewing arm still ships and is still reached — a renewing
working the wire sent no CURVE for has no walk, and that arm is what the readout falls back to.

**Forking on `branch` paints a renewing scatter with a runway that never moves**, and quotes a
quarry's `sustainable_take` of `0` as though it were a bill met. `ui_preview`'s `workings` chapter
stands a finite quarry beside a renewing flint scatter of the SAME branch, so either arm alone passes
and the pair does not.

### The over-cut predicate IS `actual > sustainable`, and on this branch that is correct

The two food webs forbid exactly that comparison and read the sim's `overdraws` flag instead, because
a hunt's kill turn cashes a banked whole animal and spikes `actual` above a steady `sustainable` under
any policy (`labor-ui.md` → "THE ⚠ HAS ONE PRODUCER"). **A deposit take is a RATE with no lump in it**
— `last_take` is an accumulator the band rows add into and `advance_deposits` clears once a turn — and
`dict/deposits.rs` says at the field that the comparison IS the reading. There is no flag on this row
to read instead.

**The WORD is the food webs' own word.** `SourceForecast.YIELD_OVERDRAW_WORD` was split out of
`YIELD_TOOLTIP_OVERDRAW` — the `YIELD_RENEWABLE_NOTE` / `YIELD_TOOLTIP_RENEWABLE` pair's own structural
idiom — so the deposit row and the food row's hover cannot drift into two words for one idea. Taking
more than a source renews is one idea.

**IN TWO REGISTERS, AND THE SHEET TAKES THE ONE ITS NEIGHBOURS TAKE.** A one-line clause on the tile
card or the roster states the bare adjective (`over_cut_word`, *overdrawing*); a compose readout states
the consequence in the source's own noun (`HudComposeVocab.LOCAL_OVERDRAW_NOTES` — *overdraws the
patch* / *the herd* / *the seam*). The deposit readout is the same widget as the other two sheets now,
so it wears their register; `LOCAL_EXTRACT_OVERDRAW_NOTE` is a third ROW of that table rather than a
word this vocabulary spells for itself.

### ⛔ AND THE TAKE IT COMPARES IS THE ASSIGNMENT'S, BECAUSE `actualTake` IS WRITTEN AT TURN RESOLUTION

`DepositState.actualTake` is `0` for the whole frame between the press and the turn, so the sheet read
`Cutting 0 a turn` and *Nobody is cutting it* directly under a headline stating the rate that same
press had just committed to. **The sim declined to seed it and the reasoning is sound**
(`.claude/rules/core_sim/extraction.md`): it is a `+=` accumulator across bands, so an assign-time
write doubles under a re-assign and clobbers under a second band, both silently — and it is the
denominator of `turnsRemaining` and half the over-cut pair, so seeding it would put a projection on
both published readouts.

**So the three states are told apart from the ASSIGNMENT ROW, whose terms are all on the wire** —
`workers`, and the SEEDED `materialYield` (`core_sim/src/bin/server.rs` → `seed_source_yield`, whose
`Extract` arm prices the take through the very seam the turn takes). `HudDepositVocab.stated_take` and
`stated_runway` are the two readers, and the runway is `reachable ÷ that rate`, floored — linear and
exact, the same division the sim makes.

| state | how it reads |
|---|---|
| **nobody assigned** | the wire's `RUNWAY_NO_TAKE` sentence — *not being worked* / *Nobody is cutting it* |
| **assigned, nothing cut yet** | the FORECAST — this crew's seeded rate, and the runway at it |
| **cut last turn** | the realized figure — `actualTake` and the published `turnsRemaining` |

⛔ **THE PUBLISHED READING LEADS AND THE ASSIGNMENT IS THE FALLBACK, never the other way round.** A
resolved turn is a fact and a forecast is a promise; the seeded rate stands in for it only where
nothing came out at all. That ordering is also what keeps the SOURCE-level figures honest on a working
two bands cut: `actualTake` sums every band, and the assignment arm — one band's row — is reached only
when the working paid out nothing, where there is at most this turn's new crew to describe.

### The runway's two negatives are two sentences

`-1` (`RUNWAY_NOT_APPLICABLE`) means *this deposit RENEWS* and **cannot reach the runway arm by
construction** — that arm is entered only where `regrowth_rate == 0` — so a `-1` seen there is a sim
bug rather than a state, and `runway_clause` / `runway_aside` state nothing rather than rendering a
reading for it.

`-2` (`RUNWAY_NO_TAKE`) means *nobody is cutting it*, and renders **`not being worked`** on the roster
and `DEPOSIT_RUNWAY_ASIDE_IDLE` on the sheet, **never `0 turns left` and never `Runs out in 0 turns`**.
The working WILL run out, just not while it stands idle; a zero there announces an exhaustion that has
not happened.

**It is also the sentinel `stated_runway` forks on**, and the two readings it now covers are the top
two rows of the table above: a `-2` beside a crewed assignment is the FORECAST's cue, and a `-2` beside
no assignment at all is the only place the idle sentence still renders. **The ROSTER and the TILE CARD
keep the wire's own reading** — `runway_clause` / `supply_clause` take no assignment — so a working a
band has just crewed still reads *not being worked* there for one frame.

## THE TILE CARD: ONE ROW PER MATERIAL, AND FOUR SHAPES IN ALL

`HudDepositVocab.deposit_lines` is `HudRouteVocab.road_lines`' twin, and it keeps that composer's two
rules: only the material row is unconditional, and every JOIN is resolved at the CALL SITE and
threaded in (`SubjectDrawerController._tile_terrain_lines` resolves the catalog, so the vocab leaf
holds none). **The rows sit with the RIVERS, above the Discovered early return, and are EMITTED
there**, which is the sim's own fog gate read back: a deposit is published to a faction that merely *remembers* the
ground.

| key | rendered when | value |
|---|---|---|
| **`Wood`** / **`Stone`** — the MATERIAL names itself | the tile carries that deposit | `stock · rung · hazard`, joined by the road's own middot. `600 · Deadfall` at a full seam; `412 of 600 · Felling` once anything is taken; `… · ⚠ going back` where the keeping is short, or `… · ⚠ overdrawing` where a renewing seam is being cut over its renewal |
| **`" "`** (blank) | the rung buys something over its branch's FREE FLOOR | the payoff — `reaches 85% of the seam` off `recoveryFraction`, `grows back twice as fast` off `regrowthMultiplier` |

⛔ **A ROW THAT WOULD SAY "none" IS NOT RENDERED**, the road block's rule (issue #566): at either free
floor a material is exactly ONE row, and a rung that buys nothing on both axes draws no payoff row.
`Felling` is such a rung — it reaches what a deadfall reaches and renews no faster — which is what
makes the absence a real state rather than a corner case.

⛔ **NEVER `0 / 0`, AND NEVER A RATIO ON A FULL SEAM.** A deposit at capacity has had nothing taken
out of it, so the second figure would be a comparison with itself; and a tile with no deposit of that
material has no row at all.

### ⛔ THE PAYOFF ROW'S KEY IS A BLANK, NOT ABSENT, AND THAT IS STRUCTURAL

`DetailFormat.detail_bbcode` renders a colon-free line **full width and CLOSES the open `[table=2]`
to do it** (`_split_kv` refuses `idx <= 0`, so a genuinely keyless line is unreachable as a table
row) — and this row sits in the MIDDLE of the card, so a keyless payoff would split the card's one
table in two and every key below it would stop sharing a column with `Foraging` / `Grazing`.

**`HudRouteVocab.ROAD_BONUS_ROW` IS THE SAME BLANK, AND THE SHARING IS THE POINT** rather than a
collision: one unlabelled payoff row, one ink, whichever branch emitted it —
`DetailFormat._value_hex` dispatches both to `bonus_value_hex()`, which takes no value because the row
is emitted only where the rung buys something.

⛔ **THE `Foraging` BASKET ROWS ARE NOT THE PRECEDENT.** They indent with
`DetailFormat.MORALE_BREAKDOWN_INDENT` and are routed to the full-width sub-row branch, which closes
the table — which is exactly what a row in the middle of this block may not do. The `workings` chapter
asserts the rendered markup keeps exactly ONE `[table=`, which is the only place the split is visible.

### ⛔ NO UPKEEP ROW, NO COUNTDOWN, NO SHORTFALL FIGURE — AND THE FIGURES MOVED RATHER THAN DYING

`land-readouts.md` records those as retired from the plant web on Ray's own instruction — *"way too
wordy… get rid of the short 2 work completely and make the tooltip be the text"*. So the hazard is a
**word** in the row's clause list and the figures ride the BLOCK's `tooltip_text`
(`ctx.row_tooltips`, never `[hint=…]`, which this Godot build does not parse):

- `deposit_card_tooltip` — `supply_tooltip`'s §7 pair (or the runway), plus `Holding it: <bill>` where
  the working owes one.
- `deposit_roster_tooltip` — the same, **plus the neglect COUNTDOWN**, and it is built ON TOP of the
  card's so the two cannot drift. The countdown lives on the Work board's hover and nowhere else,
  because that block's own head staffs the pool that would stop the slide.

### ⛔ THE THREE-NUMBER STOCK ROW WENT WITH THE POPUP, AND ITS ARGUMENT DID NOT

`stock` / `capacity` / `reachable` on one line were the case for climbing the ladder — a surface
picker reaches 330 of a 2,200-unit rock body and no more — and on a `label · value · qualifier` card
there is no room for three figures. **The argument moved to the two surfaces that can carry it**: the
tile card's PAYOFF row (*reaches 85% of the seam*, off the catalog rather than off the working) and
the compose sheet's VERDICT (*Gathering reaches 330 of 2,200. A quarry would reach 1,870.*).

⛔ **AND NEITHER IS DERIVED FROM `reachable / capacity`.** That ratio clamps to the STOCK, so it falls
as the rock is worked while the rung's reach never moves — a payoff computing it would quietly begin
quoting a different number every turn. Both read `recoveryFraction` off the catalog against the
working's own `capacity`, which is the pair the sim publishes for exactly this sentence.

### ⛔ THE MATERIAL ROW'S INK IS THE ONE `_value_hex` ARM KEYED ON MEMBERSHIP

Every other arm of that dispatch compares against a literal row key. A working's key IS its material —
`materials.json`'s ids, config this client may not spell — so there is no constant to compare against,
and `DetailFormat.Context.deposit_rows` is how the producer says which keys it wrote. `row_tooltips`
one field up is the same shape for the same reason: only the producer knows.

## THE TWO COMPOSE SHEETS — the forage sheet's spine, and one of them has the dial

⛔ **AND NOT THE ROAD LADDER CARD'S SHAPE, WHICH IS WHAT THE POPUP WAS.** A working is worked, so the
thing the player is composing is a CREW — which is exactly what a compose sheet is for. A deposit has
no stance, no policy ceiling and no take-species chooser, and **the sheet with those elements ABSENT
is the right shape**; a Window with a stepper in it was a fourth one.

⛔ **THE ESCAPEMENT FLOOR IS NO LONGER ONE OF THE ABSENCES, AND IT IS NOT ABSENT ON ONE BRANCH AND
PRESENT ON THE OTHER EITHER** (issue #650). Every `extract` row carries a floor — the sim deliberately
does not fork — and the FORK IS THE CLIENT'S: the dial is offered where `HudDepositVocab.renews` is
true and nowhere else, because rock does not come back and *leave half the seam* on a quarry means
never getting half the seam. So the forestry sheet grew the three intent presets over the draggable
chart, the renewing half of the EXTRACTION branch grew them too (a flint scatter is `extraction` and
renews), and a finite seam keeps exactly the shape it had.

Top to bottom, with `_build_deposit_assign_controls` the one builder:

1. **the `Band:` picker**, through the compose sheet's own `_build_band_picker`, so `Band:` here and
   `Band:` there line their value controls up at one declared key width. The actor defaults through
   the shared `_band_working_source` ladder, asked with the `(tile, material)` pair.
2. **the FLOOR PRESETS over the DRAGGABLE CHART, on a working that renews and on no other** — the
   SHARED `HudWidgets.build_floor_picker` / `build_floor_chart` fed a `SourceForecast.floor_chart_model`,
   the same three builders the forage sheet mounts, so a floor means the same thing on a wood as on a
   patch. **The chart IS the dial**, so a finite seam draws neither: a disabled or empty one would be
   furniture explaining an absence.
3. **the crew row** through `_mount_crew_row`, its section label the branch's crew noun uppercased.
   **The two CREW-TARGET PILLS arrive with the dial and with nothing else** — both are answers about a
   FLOOR (*clear it now* / *hold it after*), so a finite seam passes an EMPTY model and
   `_mount_crew_row`'s own `known` gate drops them rather than a branch here. That mount's trailing
   `label_tooltip` carries `CARD_CREW_HINT`, the sheet's one place to say that these hands CUT and the
   hands that HOLD are a pool on another panel.
4. **the `Kit` row** through `_mount_kit_row`, at `KitRoster.JOB_EXTRACT`. **The shipped roster
   declares no take gear on either branch**, so `build_kit_row` mounts nothing today — the honest
   answer rather than an empty picker, and the row appears by itself the day a felling axe declares a
   take stat. ⛔ **THE CREW IS HANDED ON**: omitting it is what made the forage sheet's shortfall line
   mute for the whole life of that line.
5. ⛔ **NO SPECIES CHIPS** — a deposit takes one material by construction.
6. **the improvement POINTER LINE**, in the retired-`_emit_improvement` pattern and through the SAME
   `HudWidgets.build_improvement_control` the forage sheet's offered rung uses: `⛏ Quarry this rock
   from the Work tab.` / `🌲 Coppice this stand from the Work tab.`, with `Work tab` a live `[url]`.
   The VERB comes off the **next rung's** catalog entry and the ground's noun off the branch's own
   table. ⛔ **THE SHEET EMITS NO IMPROVEMENT VERB** — `assign_labor` is the only command it sends,
   which is the shipped contract and is not being reopened. Where the band works nothing here the line
   takes its UNWORKED arm (*Send diggers here first, then …*), the sim's rule being that an
   improvement verb reaches only bands already working the source.
7. **the readout box** — the take with the MATERIAL as the account name (and `next turn · now →
   after` where there is a floor to settle at), the DEAL as its own `IMPROVEMENT_DEAL_META` block, the
   VERDICT, and — on a finite seam — the runway under the dashed rule.
8. **the commit button** — `Cut` / `Dig` at a crew above zero, `Unassign` at zero on a working this
   band holds, dead with a hint at zero on one it does not. The forage sheet's two zero-crew cases,
   verbatim. **It sends the floor** in forage's own position and forage's own decimal precision:
   `assign_labor <f> <b> extract <x> <y> <material> [floor] <n>`. A finite working sends the sheet's
   DEFAULT rather than omitting the token, which resolves to the same number sim-side and keeps the
   line one shape.

### ⛔ `max(rungFloorFraction, floor)` — ONE COMPOSITION, IN ONE NAMED FUNCTION

**`HudDepositVocab.composed_floor` is the only place in this client the two floors are put together**,
and everything downstream reads the model's own `floor` back through `_live_floor` rather than
composing the pair a second time. The rung's floor is `1 − recovery_fraction` — what the standing rung
cannot reach — and the crew's is the dial; both are *an amount left standing*, so a crew stops at
whichever is GREATER.

⛔ **ADDED, THEY DOUBLE-COUNT ON EVERY RUNG.** `extraction:gathering` strands 85% of a rock body, so a
sum would draw a gathering crew stopping 85% of the seam short of where it really stops — at every
dial position, on the one ground where the composition is not a no-op. **Every FORESTRY rung recovers
`1.0`**, so the `max` is the identity on the branch that most obviously has the dial and load-bearing
on the renewing extraction ground beside it; `_scatter_working` in the harness is that fixture.

`DrawerComposeController._deposit_chart_model` composes it once per render and once per live drag, and
feeds the result to `floor_chart_model` — which is what hands it to `project_stock`, to both crew
targets, to the verdict and to `HudDepositVocab.room_next_turn`.

⛔ **THE TEACHING LINE IS THE ONE READING TAKEN AT THE PLAYER'S FLOOR INSTEAD.** `systems::labor`'s
`Extract` arm passes the ROW's own floor to `intensification::learn_multiplier` and reaches the rung's
floor only through the workability predicate (`reachable_before`), so a line composed at the max would
promise a gathering crew ×1.70 for a dial they set to zero.

### ⛔ THE ROOM IS THE DIAL'S, AND ON A FINITE SEAM IT REPRODUCES `reachable`

`HudDepositVocab.room_next_turn` is the shared `escapement_room_next_turn` asked of a working — this
turn's growth first, then what stands above the COMPOSED floor — and it is what the take, the
max-useful cap and the readout are all measured against, so none of them can be struck at a different
point on the dial. `reachable` on the wire is the sim's reading at the floor LAST turn's crews worked
to, which is the wrong number to cap a composition with.

**On a finite working the two are the same figure by ARITHMETIC rather than by a branch**: rock's curve
is all zeros, so the growth term is nothing and the room is `stock − rung floor × capacity`, which is
`extraction::deposit_reachable` at a crew that named no floor. What still reads `reachable` is the
RUNWAY, whose numerator it is.

### ⛔ THE READOUT IS BUILT DIRECTLY, NOT THROUGH `_mount_readout` — AND STILL

The shared mount wires every register off a `floor_chart_model` and drops the **VERDICT** outright
when that model is not `known` — which is every FINITE seam, since a quarry publishes an all-zero
curve and has no projection to walk. That verdict is the one sentence the whole branch turns on. So
`_mount_deposit_readout` assembles the four SHARED widgets in the same order and the same registers,
takes the verdict from the walk where there is one and from `HudDepositVocab.deposit_verdict` where
there is not, and puts the runway aside under the finite arm alone.

⛔ **AND THE `renewable` NOTE ON THE YIELDS ROW IS THE SAME FORK.** `_fill_yields_host` draws for
three webs and composed that note from the overdraw flag alone, so a quarry read `1.20 STONE
RENEWABLE`. `YIELD_MODEL_RENEWS` is the gate; **its absence means `true`, which is a structural fact
about the two food webs rather than a fallback** — a patch reseeds and a herd breeds, so neither has a
`false` to state.

**The deal row is its own block and never a row inside the yields flow** — two harness contracts read
that flow structurally, so a deal term folded in would corrupt both silently. Its label is the rung's
verb in the past tense (`once felled` / `once coppiced` / `once quarried`, three suffix RULES rather
than three special cases) and its value is `yieldPerWorkerTurn × the crew`, the sim's own arithmetic
before the reachable stock caps it.

### ⛔ THE CREW NOUN IS PER BRANCH, NEVER PER RUNG

The `Harvesters` rule (`labor-ui.md`): *a build in flight does not move the noun*. A crew cutting a
coppice is still `Foresters`, and a second word would be the plant web's retired `Foragers`/`Tenders`
fork arriving on a third branch. `BRANCH_CREW_NOUNS` / `BRANCH_COMMIT_VERBS` / `BRANCH_GROUND_NOUNS`
are three flat tables, each answering one question.

**The sheet's TITLE is the biome/terrain label** — the slot the food-module label fills for forage.
The crew noun already says which of the hex's two workings this sheet is about, so the title does not
repeat the material.

### ⛔ THE CAP IS THE SMALLER OF THE BAND'S HANDS AND WHAT THE WORKING CAN USE

A crew takes `min(crew × perWorkerBiomass, the room above the composed floor)` in a turn, so a hand
beyond that quotient carries nothing home and the `+` must not offer it —
`HudDepositVocab.max_useful_cutters`, with `CUTTERS_UNCAPPED` where the wire prices no rate (a client
that has not been sent a row), which leaves the band's own pool as the only ceiling. The forage sheet's
max-useful rule, arrived at from the SEAM rather than from a forecast this branch does not publish.

⛔ **THE RATE IS `DepositState.perWorkerBiomass`, NOT THE CATALOG'S `yieldPerWorkerTurn`.** They are
the same number for the rung the working STANDS on, and the wire one is published for
`build_work_per_worker_turn`'s reason: the sim writes worker output as a sum of terms, so a client
reading the config's figure goes stale in silence the day a second term lands. The catalog rate
survives on the DEAL row, which quotes a rung nobody stands on yet and has no published throughput.

**AND IT IS RESOLVED BEFORE THE CHART** — the forage sheet's own load-bearing order. The chart, both
crew targets and the verdict are read against a CREW, and reading them against a count the stepper is
about to clamp away makes the panel state a verdict for a crew it then refuses to show.

## THE LADDER — the deposit branches' two tracks, on the Work board

⛔ **HOSTED WHERE THE PLANT AND ANIMAL BRANCHES ARE DECLARED, which is the WORKINGS ROSTER's own
row.** The plant and animal ladders are opened from a work row's `⌃`; the deposit branches have no
work row (`_work_source_models` admits `forage` and `hunt` alone), and the roster is the block that
already lists exactly the workings a band holds, in the same zone, one gesture from the pools that
fund them. So the row carries the identical mark — the chevron plus the next rung's own policy glyph —
and it opens the identical `_ensure_rung_track` Window through the identical `RungLadder.build_track`.

⛔ **THIS IS NOT roads.md's ROSTER RULE BEING BROKEN.** That rule forbids **a stepper, a crew count
and a kit picker** on a ROW, because a per-row worker count would re-introduce the per-tile work row
`docs/plan_standing_upkeep.md` §4.13b retired. A declaring mark is none of the three: it names no
crew, staffs nobody, and opens the card `cultivate` and `sow` are ordered from. **The per-row
prohibition is unchanged and still asserted** — `band_panel_preview` scans the ROWS for the stepper's
own `−`/`+` faces and for the road roster's `✕`, and asserts the mark beside them.

`RungLadder.deposit_track` is `route_track`'s SIBLING, not a widening of `track`: that producer takes
a labor `kind` and a prefixed forecast source dict, and a working has neither. It emits `track`'s own
`ROW_*` shape into the SAME renderer, which is what keeps one rung reading one way on every card.
Three of the six original states are unreachable here structurally — `path` and `target` name legs of
a queued entry, and a deposit publishes membership (`is_queued`) rather than a destination.

### ⛔ THE FREE FLOOR RENDERS AS A FACT, NOT A PRICE OF ZERO

*THE SHAPE IS THE STATEMENT: a button is a CHOICE, and a banked rung, the rung you stand on and an
unmet prerequisite are all FACTS.* A working standing on `forestry:deadfall` reads `where you are` in
the live ink — a `Label`, no price, no `0 work`, no `free`, no disabled control. The road branch
already deleted its prose versions of this (`"free — nobody keeps a path"`) as *"four lines of prose
to say that the commonest road in the game costs nothing and does nothing."*

**A verbless rung ABOVE the standing one takes the seventh state** (`STATE_UNORDERED`) and SUPPLIES
its own word: `HudWorkVocab.RUNG_TRACK_STATE_GROUND_GIVES` (*the ground gives it*), because
`RUNG_TRACK_STATE_WORN_IN` is the route branch's traffic metaphor and describes nothing that happens
to a deposit. **The state enumeration is still ONE table** — which is why the word lives in
`HudWorkVocab` beside the other seven — and what differs per branch is which word the PRODUCER puts on
the row; `_row_face`'s own fallback for that state stays the route branch's.

### ⛔ A PRICED ROW QUOTES NO TURNS

`250 work · 1.50/turn upkeep`, and the material aside `+ 8 wood to raise it` beneath it through the
SHARED `_build_price_asides`. **No estimate**: it would be divided by a builders pool that may be on
another job entirely and would ignore the queue the press joins — the route branch's own finding,
arriving here before it could ship. The two figures a row states are the ones no crew moves.

**A row being BUILT is the one exception**, and it quotes the SIM's chained countdown
(`build_value` → `DetailFormat.build_countdown_value`, the five sentinels a patch, a herd and a road
all publish) rather than an estimate of this client's. `0%` there is the receipt that the press
landed, which is what makes a working `quarry` distinguishable from a failed one.

**The stall clause is DERIVED here, not published.** A deposit row carries no per-turn material draw
of its own, so `_build_price_asides` weighs the band's shelf against the pile exactly as a pen's
hurdles are — the plant and animal branches' path, not the route branch's.

### The gates, keyed on the RUNG and not on the verb

⛔ **BOTH free floors declare no verb**, so a verb-keyed table would hold two entries spelling `""`
and could not tell a deadfall from a stone scatter. A refusal is a `{kind, short, long}` record in the
route branch's own shape (`HudRouteVocab.GATE_*_KEY` — one record shape for every branch), and the
PRIORITY is per branch (`HudDepositVocab.GATE_ROW_PRIORITY`).

| # | gate | row says | hover says |
|---|---|---|---|
| 1 | **nobody declares it** — `verb == ""` | *(the state's own word)* | the ground already offers this; there is nothing to order. **Stated ALONE**, the loop `continue`s past every other gate |
| 2 | ⛔ **the SITE** — `minDepositCapacity` above this tile's `capacity` | `too small` | `Wants ground holding 100; this one holds 70.` |
| 3 | ⛔ **the CREW** — nobody on the working | `no crew` | `Nobody is on this working. Put diggers on it before you order a rung.` |
| 4 | **the craft** — `unlockKnowledge` below `KNOWLEDGE_COMPLETE` | `needs Quarrying` | the live %, plus the learn-it-from remedy where a rung on this branch teaches it |
| 5 | **the ground** — `requiresRung` above the standing rung | `needs a felling` | `Needs a felling first.` |

⛔ **THE SITE GATE IS NEW TO THIS BRANCH** — the route branch has no placement rule at all — and it is
the whole of *you cannot quarry just anywhere*: it is what refuses a quarry on a 70-unit periglacial
scatter. **BOTH figures are published**, the threshold on the catalog and the capacity on the working,
so nothing here transcribes a rule the config owns.

⛔ **AND IT OUTRANKS THE CRAFT.** It is the one refusal here that no amount of learning or standing
will ever close — this ground will never take a quarry — so telling a player to go and learn Quarrying
for a 70-unit scatter is wrong advice. The GROUND gate sinks to LAST for the route branch's own
reason: it names a rung the track is already displaying one line up.

⛔ **THE CREW GATE IS THE ONLY REFUSAL ON THIS CARD WHOSE CAUSE IS NOWHERE ON THE SURFACE IT IS READ
FROM.** `queue_build_on_working_bands` filters `workers > 0` — `cultivate`'s shipped rule applied
unchanged, and the sim helper is not touched — so a working held at `felling` with its cutters pulled
off cannot be `coppice`d until somebody is put back on it. And the roster lists a 0-crew working (one
is still held and still owes, which is the whole reason the pool exists), so **the ladder opens on
that state in one click**. The roads.md per-row prohibition forbids a crew count on the row it is
opened from and the sheet that staffs the working is on another surface, so without the gate every
rung refuses with nothing anywhere near the card saying why.

⛔ **IT SITS BELOW THE SITE GATE AND ABOVE THE CRAFT**, which is the site gate's own argument read
twice. A 70-unit scatter will never take a quarry however many diggers stand on it, so *put diggers on
it* is wrong advice there and the SITE keeps the row. A craft, by contrast, is the branch's long game
while this bites TODAY and closes in one gesture — and it is the one refusal a player cannot see the
cause of from here, the craft's own progress being on the knowledge screen.

**THE REMEDY NAMES THE BRANCH'S OWN CREW NOUN**, lowercased into the sentence: *put foresters on it* on
a wood, *put diggers on it* on a rock, off the same `BRANCH_CREW_NOUNS` table the two compose sheets
title themselves with. A branch this client has never heard of takes the unnamed form (*a crew*) rather
than printing a blank — the craft gate's own named/unnamed pair, one gate over.

**IT IS A FACT ABOUT THE WORKING RATHER THAN ABOUT THE RUNG**, so every ordered row carries the
identical refusal and the card reads as one statement; it is appended per rung anyway, the gate record
shape being per rung. The **free floor is untouched**: gate 1 declares no verb and `continue`s past
every other gate, so a rung nobody orders is never refused for want of a crew to order it with.

⛔ **THE CREW IS THE TAKE CREW, PENDING-AWARE, AND IT IS A REQUIRED PARAMETER** on both
`RungGates.deposit_gates` and `RungLadder.deposit_track`. A `deposits` row publishes no crew —
membership is the band's own `extract` row on the `(tile, material)` pair — so a defaulted argument
would be this client guessing at the one input it cannot read, and the roster's own membership filter
and this gate would then answer differently about the same working. `effective_extract_workers` is what
every caller resolves it through, and pending-aware is the CORRECT reading rather than merely the kind
one: `handle_assign_labor` mutates the band's `LaborAllocation` the moment the command arrives, so a
crew staffed this frame is a crew the verb sent next frame will find.

⛔ **THE CRAFT'S REMEDY NAMES THE RUNG THAT *TEACHES* IT, LOOKED UP THROUGH `earnsKnowledge`** — never
`requiresRung`. The two coincide on the five shipped rungs, which is exactly why reading the wrong one
looks correct; a gate reason is a REMEDY, so naming the wrong rung sends the player to stand on the
wrong ground. `""` — nothing on this branch teaches it — drops the remedy clause entirely, that being
a real state.

### THE PRESS — `fell|coppice|quarry <faction> <x> <y> <material>`

The row's press emits the rung's verb through the existing improvement path
(`BandPanelController.improvement_requested` → `Main.format_improvement`) and carries no
`pending_entity`, the declaration landing on the working's own build meter rather than on the row's
crew — the road ladder's relay shape.

⛔ **THE MATERIAL IS THE POINT, AND ITS PRESENCE IS STRUCTURAL RATHER THAN INCIDENTAL.** A working is
keyed on the `(tile, material)` PAIR because one hex holds two — a wooded highland holds timber AND
rock — so a declaration carrying the tile alone does not merely under-specify the order, it raises the
OTHER working's ladder, and the resulting line parses perfectly. So the pair is refused in three
places rather than appended in one:

- **`Main.IMPROVEMENT_MATERIAL_TARGETED`** is a fourth ARM of `format_improvement`, beside the
  herd-targeted (`tame`) and band-targeted (`grade`/`pave`) ones, and it answers `{}` on an empty
  material exactly as the band arm answers `{}` on `IMPROVEMENT_NO_BAND`. **The arm is chosen on the
  VERB, off `SourceForecast.DEPOSIT_IMPROVEMENTS`, never on whether the payload happens to carry a
  material** — a payload-shape test would let a stripped payload fall through to the three-token arm
  and emit a line the parser rejects for a reason nothing on screen explains.
- **`_emit_deposit_declaration` reads the PAIR off the `deposits` row itself** (`tile_of` /
  `material_of`) and returns on `MATERIAL_NONE` beside its existing return on a negative tile, so what
  is sent is what the card was built from rather than anything recovered from the roster row's label.
- **`command_guard` drives all three verbs** through `Main.format_improvement` and asserts the
  materialless refusal, so every one of them is parsed by the REAL server parser
  (`sim_runtime::command_text::parse_command_line`) and a verb dropped from the client's own list fails
  by count. The Rust half classifies them `PlaceAddressed`, so what it proves is the PARSE.

**THE `command_guard` HALF CANNOT SEE A WRONG MATERIAL, only a missing one** — a line naming the other
working of the same hex parses — so the click path's claim lives where the click path is real:
`band_panel_preview` finds the rock's `⌃` by its own `(tile, material)` handle, presses the ladder row
it opens, and asserts the emitted line is `quarry 0 72 18 stone` by EQUALITY. The near hex's other
working is wood, so a declaration that lost the material, or took its neighbour's, reads as a different
line there.

**THE PRESS CLOSES THE CARD AND NAVIGATES NOWHERE**, which is the road ladder's press with its second
half dropped rather than a departure from it. That press ends on the acting band's Work tab because
the card it is made from floats over the tile drawer and the queue it joins is a panel away; this card
is anchored to a row of the Work tab's own workings roster, so the board the declaration lands on is
already the surface under the card and a `show_work_tab` would re-render the zone the player is looking
at to put them where they already are.

⛔ **NO `assign_labor` RIDES WITH IT.** This band demonstrably works this working — that is why the
roster lists it — which is the whole of the sim's *an improvement command reaches only bands already
working the source* rule. The one state where that is not enough is a working held at ZERO cutters,
and the CREW gate is what states it (above).

## THE `Workings` POOL, AND WHAT A FIFTH CARD COST

`quarrywork` is the FOURTH keeping role, staffed exactly as `roadwork` is —
`assign_labor <faction> <band> quarrywork <n>`, one more arm on `Main.format_assign_labor`'s shared
role branch. **One pool for BOTH deposit branches**, which is the sim's own split: forestry and
extraction differ on KNOWLEDGE and on nothing a keeper does.

⛔ **IT IS THE ONE KEEPING POOL WITH NO CARD IN THE POOLS BLOCK.** That row is full at four and the
height a fifth would cost cannot be found anywhere in the zone (below), so its stepper — and the
shortfall mark and coverage sentence the card would have carried — live on the WORKINGS ROSTER
block's own head, which already draws in the same zone directly where the question is asked.

**ITS BILL IS A COHORT FIELD. DO NOT SUM THE DEPOSIT ROWS.**
`HudBandLaborState.quarrywork_pool_state` reads `quarrywork_demand` / `_supplied` / `_shortfall` off
the band and does no arithmetic, on `roadwork_pool_state`'s rule and for its exact reason: the
`deposits` rows are **fog-filtered**, so a working out of sight drops out of any client-side total
while the band still owes its keeping. The demand is summed sim-side BEFORE the head-count gate, so a
band with nobody on the pool publishes the bill it is failing to pay rather than a reassuring zero —
that is the alarm. It rides the optional `POOL_COVERAGE_SHORTFALL_KEY` path, tested with `has()`: a
defaulted `0.0` would read as *this pool covers everything* and clear the mark on every card in the
game.

**Its hint and its coverage sentence name the workings the band OPENED**, not the ground it stands on,
and **the two were written from one model in one pass** — which is the road pair's own lesson, where
the identical correction landed in `ROADWORK_ROLE_HINT` and not in
`UPKEEP_POOL_COVERAGE_ROUTE_FORMAT` eighty lines below it. There is no third copy.
`QUARRYWORK_ROLE_HINT` and `UPKEEP_POOL_COVERAGE_DEPOSIT_FORMAT` are the whole set.

**And it counts toward the fund-mode row's *is there anything to fund* gate**, for the road bill's
reason on a fourth pool: a band whose only standing cost is a felling working it opened is exactly the
band this branch creates, and it has the same split to make.

### ⛔ THERE IS NO FIFTH POOL CARD — THE ROW HAS NO WIDTH AND THE ZONE HAS NO HEIGHT

A pool card's own minimum is its STEPPER and reads **83px** at `POOL_STEPPER_*` (measured off
`band_panel_preview._assert_pool_cards_are_level`, which prints it). So:

| cards abreast | width wanted | bottom-dock box | left-dock box |
|---|---|---|---|
| 4 (the shipped row) | `4 × 83 + 3 × 6` = **350** | 382 | 356 |
| **5** | `5 × 83 + 4 × 6` = **439** | 382 | 356 |
| 3 | `3 × 83 + 2 × 6` = **261** | 382 | 356 |

The horizontal trim that bought the FOURTH card is already at 4 against `HudStyle`'s authored 11, so
there is nothing left to take out of the CONTROL — and `_assert_pool_cards_are_level` exists to fail
the moment a role NAME becomes a card's floor instead.

> #### ⛔ AND A SECOND ROW OF CARDS WAS BUILT, SWEPT AND REJECTED — 62px THE ZONE HAS NOWHERE TO GET
>
> A 3 + 2 split fits every dock on WIDTH (261px of 356) and costs **62px** of HEIGHT — one
> `POOL_CARD_HEIGHT` plus one `ROLE_CARD_SEPARATION` — which takes
> `HudWorkVocab.pools_block_height` from 82/110 to 144/172 and the WORK zone's own floor from **358**
> to **420**. `band_panel_build_queue_wide` and `band_panel_queue_settings_wide` then fail at
> `needs 420px … short by 62`, the ordinary wide states land 6px over once the board and the queue
> have given back what they can, and `band_panel_work_inspector_dialog_bottom` drops from 2 board
> rows to 1.
>
> **`BandCityPanel.PANEL_HEIGHT_WIDE` was re-swept the way its own note describes — the whole
> `band_panel_preview` matrix, every dock and viewport — and it has no value left to land on.** Two
> bounds pin it from opposite sides, each read off a full run:
>
> | value | zone box | verdict |
> |---|---|---|
> | **480** (= 420 + `HORIZONTAL_BODY_CHROME` 60) | 420 | every `Zone_work` claim clears — and `band_panel_band_columns_two` fails: the two-column band flank holds **413 of 840 = 49%** against `band_panel_preview.BAND_FLANK_FILL_FLOOR`'s 50% |
> | **474** | 414 | fails BOTH — `needs 420 … short by 6` and the flank at 413 of 828 |
> | **473** | 413 | the flank claim clears (413 ≥ 413) and the work zone is `short by 7` |
>
> The band flank's one-column content is fixed at 413px and a two-column flank is offered `2 × box`,
> so every pixel added there makes that flank emptier — which is the *"the bottom strip is too tall,
> the columns end well above the bottom of it"* report that took `PANEL_HEIGHT_WIDE` **456 → 418** in
> the first place, arriving from the other direction. **And at the matrix's shortest viewport that
> constant is inert anyway**: `_horizontal_panel_height()` clamps to `MAX_WIDE_HEIGHT_FRACTION`, so at
> 1152x720 the box is `720 × 0.6 − 35 − 60` = **337** whatever the budget says, and
> `band_panel_queue_settings_tight` needs 398 there — a fraction of `(398 + 60 + 35) / 720` =
> **0.685** against the 0.6 cap.
>
> **So `PANEL_HEIGHT_WIDE` ends this arc UNTOUCHED at 418**, its own decomposition still summing to
> 358 with the POOLS term at 82, and the `quarrywork` pool's control lives on the workings roster's
> own head instead — which costs the zone **1px** (below). Neither of the two remaining levers was
> moved to fit the arc: the height cap and the harness's 50% fill floor are both judgements about how
> much of a short window this strip may eat.

## THE WORKINGS ROSTER — the pool says how many, the roster says WHICH

The pool card says `Workings 2`; nothing else in the client would say WHICH two. That is the exact
defect the roadwork roster was built to fix, so this is its twin —
`BandPanelController._build_workings_roster_block`, a **block inside the existing Work zone** directly
under the pools block that raises the question, not a fourth zone.

**The catchment is the band's own `extract` ROW, which is this arc's one deliberate departure from the
route branch.** A working publishes no keeper: it is held by the band that WORKS it, so membership is
asked of `extract_assignment_of` and the `deposits` row supplies the state. **A row held at ZERO
cutters still counts** — a working with no crew is still held and still owes, which is the whole reason
the pool exists.

### ⛔ ITS HEAD *IS* THE `Workings` POOL, AND THAT IS NOT roads.md's ROSTER RULE BEING BROKEN

The pools block one line up has no room for a fifth card (above), and **this block's head already
draws and is already paid for** — so the head carries the block title, the shortfall mark and a
compact stepper on the pool cards' own `POOL_STEPPER_*` metrics, driving the same
`assign_labor <faction> <band> quarrywork <n>` the card would have sent.

roads.md's rule reads *"⛔ IT IS A ROSTER, NOT A WORK BOARD: no stepper, no crew count, no kit
picker"*, and its stated REASON is that **a ROW** offering a worker count would re-introduce, by the
back door, the per-tile work row `docs/plan_standing_upkeep.md` §4.13b retired. **The prohibition is
on ROWS, and on those three controls.** A pool stepper on the block HEAD is the band-wide pool itself;
a declaring `⌃` on a ROW is the control `cultivate` and `sow` are pressed with, naming no crew and
staffing nobody.

**THE PER-ROW RULE IS UNCHANGED AND STAYS ENFORCED: no stepper, no crew count, no kit picker on any
ROW**, and `band_panel_preview._assert_the_workings_roster_names_its_workings` asserts that absence on
the stepper's own `−`/`+` faces and on the road roster's `✕` — **scoped to the ROWS and not to the
block**, since a block-scoped search now finds the head's own control and would pass or fail for the
wrong reason. The hands that CUT a working are on the tile card's two compose sheets.

**AND THE HEAD STATES THE BILL, NOT JUST THE COUNT.** The retired card's two readings move with it:
the shortfall MARK (`UPKEEP_POOL_SHORT_MARK`, in `HudStyle.WARN`) and the coverage SENTENCE, both
decided by the ONE composer the four cards use (`HudWorkVocab.upkeep_pool_coverage_line`), so the
glyph and the words cannot disagree. The three figures are read off the cohort with **no client-side
arithmetic** — the `deposits` rows are fog-filtered, and that rule is unchanged and load-bearing.
**The hint rides the TITLE's own hover** rather than the mark's: the mark is conditional, and a calm
band would otherwise have nowhere to read what this pool does.

**ITS HEIGHT IS `WORKINGS_ROSTER_HEAD_HEIGHT` = 21px, MEASURED — `ZONE_HEAD_HEIGHT` plus ONE.**
`HudWidgets.zone_head` declares 20 as a MINIMUM and an `HBoxContainer` grows to its tallest child, so
the compact stepper's own button minimum sets the row. That 1px goes through the SAME
single-resolution seam the block already used — `_fill_work_zone_column` resolves
`workings_roster_height` once and hands it to both `build_queue_rows_max` and `_work_board_capacity` —
so nothing in the zone's arithmetic learns about the head separately.
`_assert_workings_roster_head` prints reserved beside drawn (21 of 21; the block 105 of 105), which is
what makes the constant a measurement.

**`HudWidgets.zone_head` gained a trailing `title_tooltip`** for it, the way `readout_tooltip` was
added: the readout Label is built only where a readout is stated, so a head whose readout is a
CONDITIONAL mark would lose its hover on exactly the calm band that most needs the words.

**The name cell is the MATERIAL plus the road roster's locator** — `Wood · 4 tiles E`. A working has
half a name the road lacked, and two workings on one tile are two rows that differ in nothing else,
which is why the material leads. The locator is `_roadwork_roster_locator` verbatim, so its
`SourceForecast.compass_bearing` in DRAWN space and its wrap-aware column delta are one implementation.

**THE VALUE CELL IS `HudDepositVocab.deposit_row_value`, and it names its rung out of the CATALOG** —
so the rung reads `Felling` rather than `forestry:felling`, and a raw wire key there is the honest
answer for a rung the catalog does not carry rather than a formatting slip. The row's INK is that
composer's own answer too (`deposit_value_color`), keyed on the hazard mark it puts there rather than
on a second test. Its HOVER is `deposit_roster_tooltip` — the figures the one-line cell cannot carry,
including the neglect countdown, which lives here and nowhere else.

**Rows sort by distance ascending, tie-broken by TILE and then by MATERIAL**, so a wooded highland's
two rows cannot swap frame to frame.

**Fog: three cases, and the middle one must not be got wrong.** Nothing held in sight AND no demand →
no block at all (`workings_roster_height` answers `0`); demand > 0 with zero visible → the block
renders with ONE muted line saying so, **never an empty roster beside a non-zero count**; workings
visible → rows, capped at `ROADWORK_ROSTER_ROWS_MAX` with the build queue's own `+N more` foot. The cap
and the foot are SHARED with the road roster deliberately: the two sit in one zone answering the same
shape of question, and two caps would be two answers to *how long may a roster be here*.

⛔ **CASE 1 TAKES THE POOL'S STEPPER WITH IT, AND THAT IS A DECISION RATHER THAN AN OVERSIGHT.** With
the control on the head, a band holding no working and owing no bill has nowhere to staff
`quarrywork` — which is the honest arrangement: there is nothing to keep, so there is nothing to
staff, and the stepper appears exactly when there is something for it to pay for. **A fallback control
there would be a live stepper for a bill of zero**, which is the furniture-explaining-an-absence the
pools block's own note refuses one block up. **Cases 2 and 3 both draw the head**, so a band that owes
anything can always reach it — and case 2 is the one that matters, a band owing a bill it can see no
working for being exactly the band that needs to staff the pool. All three are asserted, the case-1
absence included, because a missing control looks the same whichever reading produced it.

**Its height is paid for in BOTH reservations** — resolved once in `_fill_work_zone_column` and handed
to `build_queue_rows_max` AND `_work_board_capacity`. **Its GAP is counted apart from the road
roster's**: the two blocks appear independently, so a band with one pays one gap and a band with both
pays two, and folding the pair into one term is the 6px disagreement
`BUILD_QUEUE_ROOM_ROSTER_GAP_COUNT` exists to prevent, on a second block.

### ⛔ AND IT CARRIES NO `✕`, BECAUSE NO VERB DROPS A WORKING

The road roster's drop is `abandon <faction> <x> <y>`, and `abandon` **does not reach a working** —
checked, not assumed. `BuildSourceRef::target()` resolves a tile pair to `forage_source(tile)`, i.e. a
`LaborTarget::Forage`, and `LaborTarget::same_source` pairs an `Extract` row only with another
`Extract` row of the same `(tile, material)`; `release_roads_at` beside it touches the `RoadRegistry`
alone. So the verb drops the faction's forage holding and the tile's road and leaves every working
standing.

**A `✕` here would emit a command that destroys something else on the same hex**, which is worse in a
roster than on a card — a roster invites bulk use. Unstaffing is the take crew's own `0`, on the
working's compose sheet. **A working-shaped abandon is server-side work and is not this arc's.**

## THE ROLE HAD TO PASS BOTH GATES, AND THIS IS THE THIRD TIME

A role passes TWO gates: `sim_runtime::command_text`'s grammar and the server's own
`handle_assign_labor`. `builders` and then `roadwork` each shipped **refused inside the client** for a
slice because they were in the server's dispatch and not in that grammar — the native bridge parses a
line there BEFORE it sends, so every staffing command was refused locally with nothing failing
anywhere.

`quarrywork` and `extract` were both in the server's dispatch and in NEITHER the grammar nor
`Main.format_assign_labor`. Both were added in the same pass as the controls that emit them:

- **`quarrywork` joins the closed role arm** (`"scout" | "warrior" | "agriculture" | "husbandry" |
  "roadwork" | "quarrywork" | "builders"`), a bare worker count and no tail but the kit.
- **`extract` is a TARGETED grammar of its own** — `extract <x> <y> <material> [floor] <workers>` —
  and **the material is not optional**: one tile can hold two workings, so a line naming only the tile
  names neither. It rides the **`species` token**, which is where the sim's own `extract` arm reads it
  from and which means the same kind of thing on a forage row (*which of the things on this ground are
  you here for*). **The FLOOR follows it in forage's own position** (issue #650), a validated NUMBER at
  `Main.FLOOR_COMMAND_DECIMALS` — never `str(float)` — with the four retired stance words refused BY
  NAME at parse. Still no kit token: `default_kits.extract` is the bare `none` kit with no picker
  anywhere in reach, so the tail is closed after the worker count.
- ⛔ **THE FLOOR IS SENT ON BOTH BRANCHES EVEN THOUGH ONLY ONE IS ASKED FOR ONE.** A finite seam has no
  dial, so its payload carries the sheet's default — which is what an omitted token resolves to
  sim-side (`DEFAULT_ESCAPEMENT_FLOOR`) — and the line keeps ONE shape rather than two.

⛔ **THE SHEET'S KIT ROW IS MOUNTED AND ITS SELECTION HAS NO TOKEN TO RIDE.** `extract`'s grammar is
closed after the worker count, and the shipped roster offers `extract` no kit at all — so the row
never draws and the mismatch is inert. **A take tool added to `equipment.json` needs the token first**,
or the sheet would show a picker whose answer the line silently drops.
`HudBandLaborState.default_kit_id` answers `NO_KIT_ID` for `JOB_EXTRACT` explicitly rather than by
fall-through, on `builders`' own reasoning: falling through would mark the HUNT kit as this job's
default and `Main._kit_token` would then omit the token for a selection the player made.

**`command_guard` is what keeps the two enumerations in step, and it now drives both.** `quarrywork`
joined `ASSIGN_LABOR_ROLES` (the sweep asserts every role in that list builds a line AND that an
unknown one builds none) and `extract` is the FOURTH targeted drive — the sweep cannot reach it, a role
in that list taking a bare count. `ASSIGN_LABOR_EXPECTED` moved 10 → **12** and is re-derived at
runtime from the list, so the literal cannot go stale unnoticed.

## Tests

### The three surfaces — `ui_preview`'s `workings` chapter

`tools/ui_preview/chapters/workings.gd`, appended LAST in `CHAPTERS` so no existing frame moves. It
pushes its own rung CATALOG **and its own knowledge ROSTER** through the real ingest — the three
deposit crafts are absent from the shared `fixtures_knowledge.gd` ladder, and a craft's word is what a
gate's refusal and the sheet's teaching line both take their name from — stages its own band (see
below), and hands the hex back bare on the way out; an EMPTY `deposits` array there means the ground
holds nothing rather than nobody having worked it.

⛔ **THE FIXTURES CARRY THE ESCAPEMENT FOUR AT THE SHIPPED CONFIG FIGURES**, and the curve is built the
way the SIM builds it (`_deposit_regrowth_samples` — the seed read INSIDE the logistic term, never
lifted onto the stock), so rock's samples come back all-zero by the same arithmetic that keeps a
quarry at zero. `_scatter_working` is the one fixture where the `max` composition is not a no-op: it
renews, so it is offered the dial, and it stands on a rung that strands 85% of the scatter.

| frame | what only IT can say |
|---|---|
| `workings_tile_card` | ONE hex, TWO material rows, each keyed by its own material — the claim a tile-keyed surface cannot make — plus the row's `stock · rung · hazard` shape, the rung named by the WIRE, **no bill, countdown or shortfall figure anywhere on the card**, those figures present on the block's HOVER, the countdown on the ROSTER's hover and not on this one, and no payoff row on two rungs that buy nothing |
| `workings_payoff_rows` | the blank-key row, BOTH of them — `reaches 85% of the seam` on the quarry and `grows back twice as fast` on the coppice, the two AXES the two branches' ladders buy — and the structural claim: the rendered markup keeps exactly ONE `[table=`, which is the only place a keyless payoff's table split is visible |
| `workings_forestry_sheet` | `Assign foresters ▸` — the eyebrow naming the BRANCH's crew, the crew row naming it again, **the floor presets AND the chart, because the ground grows back**, the pointer line naming the next rung and linking the Work tab, the deal row at the composed crew, the sheet's own overdraw note, the SHARED harvest verdict, and the commit verb `Cut`. It also asserts the arm that verdict REPLACED still composes — `deposit_verdict` on a curveless working — which no frame can show beside it |
| `workings_extraction_sheet` | `Assign diggers ▸` on the rock beside it — the OTHER branch's noun, **neither preset nor chart nor crew pill**, the verdict the finite ladder turns on (*Gathering reaches 330 of 2200. A quarry would reach 1870.*), the runway aside at this rate, the IDLE seam's own sentence beside it (never `Runs out in 0 turns`), and `Dig`. **It is the NEGATIVE that makes the forestry sheet's dial mean something**: same chapter, same hex, one fork |
| `workings_floor_strip` / `_peak` / `_learn` | the dial at each of the three intent presets on ONE stand — the preset lit, the chart drawn, the take in the working's own MATERIAL at every position, the `now → after` arrow agreeing with the sheet's own verdict (both gated on the same walk), and the teaching line at the shared `learn_multiplier` for that floor. **The stand is a nearly-full one on purpose**: the chapter's working fixture sits BELOW the top preset's floor, so on it the third frame would be empty for a reason that has nothing to do with the dial |
| `workings_floor_held` | a wood standing EXACTLY at its floor — the shared *At the floor and holding it*, the take collapsing to what the stand puts back, and NO arrow, the two readings being one number. The crew is read BACK off the sheet, since a reopened sheet seeds from the band's own row |
| `workings_floor_stripped` | the dial DRAGGED to the bottom, live (`committed = false`) — the chart the pointer holds still alive, the readings refilled in place, the `⚠ overdraws the seam` mark and the strip consequence in the working's own noun |
| `workings_fresh_runway` | **a crew committed this turn on a seam that has cut nothing** — the FORECAST runway (`reachable ÷ the seeded rate`) where the sheet used to read *Nobody is cutting it*, the same seam with no crew still reading it, and the renewing verdict quoting the SEEDED take rather than a zero. Three states, three readings, one frame plus two producer claims |
| `workings_unopened` | **the state a player meets first** — both branches' free floors on one hex, untouched: the full seam as ONE figure under the floor's own name, no hazard word of either kind, and the IDLE quarry still reading `not being worked`, which is the pair `is_unopened` exists to keep apart |

**The LADDER's row states are asserted over the PRODUCER, without a frame**: the SITE gate on a
70-unit scatter, the CRAFT gate on a body big enough for a quarry (with the remedy naming the rung
that TEACHES it), the free floor as a FACT, a priced row leading with its pile and its standing bill
and quoting no turns, its material aside, and a row mid-build quoting the sim's own countdown. **A
frame can show one of those and the branch has five** — and the rendered ones live in
`band_panel_preview`, the track being opened from the Work board.

⛔ **THE CREW GATE IS ASSERTED AS AN A/B OVER ONE WORKING**, which is what makes the claim about the
CREW rather than about a gate that refuses unconditionally: the same rock, the same learned craft, only
`LADDER_CUTTERS` → `LADDER_NO_CUTTERS` moving. Three claims ride it — the row states `no crew` and
offers no press, the hover names the remedy in the EXTRACTION branch's own crew noun, and the free
floor is untouched by it (gate 1 declares no verb, so a rung nobody orders is never refused for want of
a crew to order it with). **The SITE gate's precedence is asserted beside them**, on the 70-unit
scatter at zero cutters: the row must lead with its SIZE and must NOT carry the crew's short form,
because *put diggers on it* is wrong advice on ground that will never take a quarry.

⛔ **EVERY OTHER LADDER CLAIM IN THE CHAPTER NOW STATES `LADDER_CUTTERS` EXPLICITLY, and a default
would have silenced them all.** The crew gate refuses every ordered rung on a working nobody holds, so
a track asked at zero renders `no crew` on the very rows the site, craft, ground and price claims are
about — each would then pass or fail for a reason that has nothing to do with what it names.

⛔ **THE CHAPTER STAGES ITS OWN BAND, AND WITHOUT ONE EVERY SHEET CLAIM IS ABOUT A CREW OF ZERO.** It
runs LAST, after twenty-five other chapters, so the roster it inherits is whichever one the previous
chapter left; a band with no idle worker clamps the stepper to 0, which renders a perfectly ordinary
sheet — no take, no deal row, and the pointer line's *send crews here first* arm instead of its live
one. **Measured**: that is exactly how the chapter first failed, on four claims that said nothing
about the code under test.

**Every claim is asked of the shipped composer or of the rendered surface**, never of a
re-derivation, and the fixtures are shaped exactly as `dict/deposits.rs` and
`deposit_rungs_to_array` write a row.

### The pool, the roster and the TRACK — `band_panel_preview`

**`POOL_CARD_COUNT` stays 4 and every pool-card assertion is byte-identical to `origin/main`** —
checked by diff, not by eye. Its doc block records the 439px and the rejected second row so the next
reader does not re-derive them.

The roster's own block asserts, on one fixture whose near hex carries TWO workings: the negative (a
working this band does not work is not listed), **the two UNTOUCHED rows** — one of them on a hex
whose other material this band DOES hold, which a tile-keyed membership test would list — the count of
THREE with two of them on one tile, the nearest-first sort tie-broken by MATERIAL, the material-led
name cells, the value cell as `deposit_row_value` verbatim **asked with the same catalog the row was
built from**, the rung named by the catalog rather than by its wire key, §7's fork read off two rows of
ONE roster, and **no stepper, crew count or `✕` on any ROW** — scoped to the rows.

⛔ **IT PUSHES A RUNG CATALOG, AND WITHOUT ONE THE STATE IS EVIDENCE OF NOTHING.** Every value cell
names its rung out of it, so a roster with no catalog behind it draws `forestry:felling` in each one;
and the row's `⌃` is built from `RungLadder.has_track` over its rows, so the mark does not draw at all
and every claim about it passes vacuously. It is cleared with the deposits on the way out, a per-world
constant being exactly the thing a later state inherits.

**`band_panel_workings_track` is the LADDER's own frame**, and this is the one harness that can render
it: the track is opened from the roster row's mark and `ui_preview` stands up no Band panel. It
asserts one mark per row with somewhere left to go, each keyed to its own `(tile, material)`, the
branch's own two rungs and no more (the two ladders are ONE vector, so a walk that forgot to filter on
`branch` would offer a coppice here), the floor stated as a FACT rather than a price of zero, the
quarry LEADING with its pile and its standing bill, **no `≈` estimate on it**, and its material aside.

⛔ **AND THE PICK, WHICH IS THE ONE CLAIM NO OTHER SURFACE CAN MAKE.** The chain is the player's: the
roster row's `⌃`, the track row it opens, the payload off `HudLayer.improvement_requested`, and
`Main.format_improvement` — with the LINE asserted by equality (`quarry 0 72 18 stone`), because a
payload can carry a perfectly good material and still be formatted into the three-token grammar. The
mark is found by the rock's own `(tile, material)` handle and the near hex's other working is wood, so
a declaration that lost the material or took its neighbour's reads as a different line. The card
closing on the press is asserted beside it.

⛔ **THE BLOCK LEARNS THE THREE DEPOSIT CRAFTS, AND THAT IS WHAT MAKES THE PICK REACHABLE.** The
standing knowledge row this harness renders every other state against carries the four
rung-transition tracks and none of the deposit branches', so a quarry row asked against it is refused
on its CRAFT — a `Label`, which no press can reach. `_workings_knowledge_row` EXTENDS that row rather
than replacing it (the states around this block are rendered against those four), and
`_restore_workings_roster_fixture` pushes the standing row back: a push replaces a faction's whole
row, so the crafts learned here would otherwise ungate every rung on every state after it.

**`band_panel_workings_track_no_crew` is the CREW gate's own frame**, opened on the FAR row — the
fixture's crewless working, staged at `WORKINGS_NO_CUTTERS` since the roster's membership test needed a
row held at nobody. It asserts the mark still DRAWS on it (a working with nobody on it is still held,
so hiding the mark would be a different lie), the quarry row refused for want of a crew, that row still
LEADING with its pile (a rung refused today is one the player is planning toward, and a price hidden
behind a refusal is a price nobody can plan against), that it is a `Label` and offers no press at all,
and the hover naming the remedy in the branch's own crew noun.

⛔ **THE CARD IS A `PopupPanel`, i.e. a `Window`, so a `Control`-rooted finder walks straight past
it** — the road ladder's own trap. Its rows are read through the same `_rung_track_states` /
`_rung_track_faces` pair the plant track's states use, which recurse through the Window.

**Frames:** `band_panel_workings_roster` (the head with its stepper and its `⚠` over three rows, under
a four-card pools block), `band_panel_workings_track` (the ladder open over it) and
`band_panel_workings_roster_unseen` (case 2 — the head still drawn, the muted line in place of the
rows).

## See Also

- `.claude/rules/core_sim/extraction.md` — the sim half: the deposit as a stock with a regrowth rate,
  the `quarrywork` pool's claims, and what each field on the wire means
- `.claude/rules/client/roads.md` — the direct precedent for all three surfaces: the tile card's
  conditional-row block, the ladder track and its gates, the `roadwork` pool and THE ROADWORK ROSTER
- `.claude/rules/client/labor-ui.md` — the compose sheet's spine and its field-row family
- `.claude/rules/client/selection-card.md` — what may go on the tile card at all
- `.claude/rules/client/land-readouts.md` — the retired keeping rows, and why the figures live on a hover
- `.claude/rules/client/band-city-panel.md` — the Work zone the pool block, the roster and the track share
- `docs/plan_extraction.md` §7 — which readout a working publishes, and why the rate decides it
