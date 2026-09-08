---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_deposit_vocab.gd"
  - "clients/godot_thin_client/native/src/dict/deposits.rs"
  - "clients/godot_thin_client/tools/ui_preview/chapters/workings.gd"
---

# Workings — the client half of the wood-and-stone producers

The sim side is `.claude/rules/core_sim/extraction.md`, which is authoritative for what every field
on the wire MEANS, and `docs/plan_extraction.md` is the design of record. This file is what the client
does with them. Read the sim one first — most of the traps here are its traps, arriving one layer out.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_deposit_vocab.gd` (`HudDepositVocab`) | The WORKINGS vocabulary leaf — the five rung keys plus the two per-branch `RUNG_ORDER`s, one reader per field on a `deposits` row, the keeping verdict (`owes_keeping` / `is_short` / `is_at_risk`, the bool-before-the-number rule), and **`deposit_row_value`, the ONE composer both surfaces state a working's state with**. §7's fork lives inside it, once (`supply_clause` → `runway_clause`). A vocab module with static funcs, the `hud_route_vocab.gd` shape: it reads `SourceForecast` / `DetailFormat` / `HudSelectionVocab` / `HudLoadoutVocab` / `HudStyle` inside functions only, never in a `const`, so it adds no load cycle |
| `ui/hud/DrawerComposeController.gd` → the `build_workings_drawer_actions` family | The tile card's `Workings ▸` action and the `PopupPanel` it opens (`_open_workings_card` / `_fill_workings_card` / `_build_workings_block` / `_build_workings_crew_row` / `_ensure_workings_card` / `_dismiss_workings_card`), filling `%WorkingsControls` — its own container beside `%RoadLadderControls`, since `%ForageAssignControls` is gated on a gathering site with a band in hand and a working stands on ground that is neither. **One action, one card, one block per WORKING** (below). Its crew stepper is the only control in this family that emits: `assign_labor <f> <b> extract <x> <y> <material> <n>`, through the shared `_emit_assign_labor` |
| `ui/hud/BandPanelController.gd` → the `_workings_roster_*` family | **THE WORKINGS ROSTER, WHOSE HEAD IS THE `Workings` POOL** — `_workings_roster_models` (the band's own `extract` row as the membership filter, the material-led locator, the stable nearest-first sort), `_workings_roster_unseen` (case 2, off the cohort's published `quarrywork_demand` and never a sum of the fog-filtered rows), `_build_workings_roster_head` (the title, the shortfall mark, and the pool's compact stepper — the ONLY control that staffs `quarrywork`), `_build_workings_roster_block` / `_row`. No ROW emits anything: there is no verb that drops a working (below). Its reserved height is resolved once in `_fill_work_zone_column` and spent in BOTH `build_queue_rows_max` and `_work_board_capacity` |
| `ui/hud/hud_work_vocab.gd` → the `WORKINGS_ROSTER_*` family + `ROLE_NAME_QUARRYWORK` | The roster's words, metas, its `WORKINGS_ROSTER_HEAD_HEIGHT` (21, measured) and its `workings_roster_height`, plus the pool's own name, hint and coverage sentence. They live here rather than in `HudDepositVocab` because the geometry of the Work zone belongs beside the three blocks that share it, which is `roadwork_roster_height`'s own rule one pool over |
| `ui/hud/HudWidgets.gd` → `zone_head`'s trailing `title_tooltip` | The one shared-layer change this arc makes: a head whose readout is a CONDITIONAL mark needs its hover on the TITLE, because `readout_tooltip` rides a Label built only where a readout is stated |
| `ui/hud/HudBandLaborState.gd` → `set_deposits` / `deposits` / `quarrywork_pool_state` / `extract_assignment_of` / `effective_extract_workers` | The section held WHOLE (the roster asks a whole-list question), the pool's three cohort fields read with no arithmetic, and the per-working assignment readers — each matching on the tile **and** the material, because that pair is the row's identity |
| `MapView.gd` → `_ingest_deposit_workings` / `_workings_on_tile` / `deposit_tile_lookup` | The per-TILE index the tile card's action reads its rows out of, `_ingest_road_network`'s twin. ⛔ **It does NOT de-duplicate on the tile** — two rows on one hex is the ordinary case here, not a truncated frame — which is exactly where a road-shaped `if not has(tile)` loses one |
| `native/src/dict/deposits.rs` | `deposits_to_array` — one dict per live WORKING, keyed `(tile, material)`. The module header carries the whole field contract: the sparse-and-lazy registry, the `regrowth_rate` fork, the two runway sentinels, and the road's own standing-bill / neglect / build quad verbatim |

## ⛔ THE WORD "QUARRY" IS TAKEN, AND IT MEANS THE HUNTED ANIMAL

The compose sheet's field rows are `Band:` · `Kit` · `Quarry`, and `Quarry` there is the PREY
(`labor-ui.md` → "A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED"). A player reads both surfaces in
the same minute, so **no player-facing string in this arc may use the word for a deposit, a pool or a
roster.** The noun is **Workings** — the sim's own word for a live deposit a band has opened — and the
MATERIAL names the thing being worked (`Wood`, `Stone`).

`quarrywork` survives as the server's command token and as `HudConst.LABOR_KIND_QUARRYWORK`. That is
grammar, not copy: it appears on no label, no hint and no tooltip. `HudWorkVocab.ROLE_NAME_QUARRYWORK`
is `"Workings"`, and the pool card, its hint, its coverage sentence and the roster head all read it.

## ⛔ ONE TILE HOLDS TWO WORKINGS, AND THE CARD IS ONE BLOCK PER *WORKING*

The registry key is `(tile, material)`: a wooded highland carries timber AND rock, and working one is
not working the other. So the tile card's action is **one button opening one card that states them
both**, and the card renders a block per working — material as the head, its own state line, its own
stock row, its own crew stepper, its own bill.

**A card that rendered one block per TILE is the defect the whole `material` field exists to
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
- `extract_assignment_of` / `workers_for_extract` / `effective_extract_workers` all match on the pair,
  or the Stone stepper opens on the Wood crew's count.
- The roster's row meta and the card's block meta are both `"<x>,<y>:<material>"`, so a harness can
  say *this working* rather than *a working on this hex*.

## ⛔ THE READOUT IS DECIDED BY `regrowth_rate > 0`, NEVER BY `branch`

A flint scatter and a quarry are **both `extraction`** and read differently — same skill, same ladder,
and only one of them runs out. `HudDepositVocab.renews` is that fork, it is the only fork, and it lives
inside `deposit_row_value` so the card and the roster cannot answer it two ways:

| `regrowth_rate` | the clause | the field it reads |
|---|---|---|
| `> 0` | the OVER-CUT warning — `⚠ overdrawing`, figures on the hover | `sustainable_take` against `actual_take` |
| `== 0` | the RUNWAY — `75 turns left` | `turns_remaining` |

**Forking on `branch` paints a renewing scatter with a runway that never moves**, and quotes a quarry's
`sustainable_take` of `0` as though it were a bill met. `ui_preview`'s `workings_runway` is the frame
that fails such a client: it stands a finite quarry beside a renewing flint scatter of the SAME branch,
so either arm alone passes and the pair does not.

### The over-cut predicate IS `actual > sustainable`, and on this branch that is correct

The two food webs forbid exactly that comparison and read the sim's `overdraws` flag instead, because
a hunt's kill turn cashes a banked whole animal and spikes `actual` above a steady `sustainable` under
any policy (`labor-ui.md` → "THE ⚠ HAS ONE PRODUCER"). **A deposit take is a RATE with no lump in it**
— `last_take` is an accumulator the band rows add into and `advance_deposits` clears once a turn — and
`dict/deposits.rs` says at the field that the comparison IS the reading (*"actual above sustainable is
over-cutting, which is possible on purpose and warned about rather than refused"*). There is no flag on
this row to read instead.

**The WORD is the food webs' own word.** `SourceForecast.YIELD_OVERDRAW_WORD` was split out of
`YIELD_TOOLTIP_OVERDRAW` — the `YIELD_RENEWABLE_NOTE` / `YIELD_TOOLTIP_RENEWABLE` pair's own structural
idiom — so the deposit row and the food row's hover cannot drift into two words for one idea. Taking
more than a source renews is one idea.

### The runway's two negatives are two sentences

`-1` (`RUNWAY_NOT_APPLICABLE`) means *this deposit RENEWS* and **cannot reach the runway arm by
construction** — that arm is entered only where `regrowth_rate == 0` — so a `-1` seen there is a sim
bug rather than a state, and `runway_clause` states nothing rather than rendering a reading for it.

`-2` (`RUNWAY_NO_TAKE`) means *nobody is cutting it*, and renders **`not being worked`, never
`0 turns left`**. The working WILL run out, just not while it stands idle; a zero there announces an
exhaustion that has not happened. Flattening the two is the defect the split exists to prevent, and
`workings_idle` asserts both halves — the sentence AND the absence of the zero.

## THE CARD IS THE ROAD LADDER'S SHAPE, AND THE COMPOSE SHEET'S IS THE WRONG ONE

A working is a tile-keyed source picked TILE-FIRST, exactly as a road is, which is why
`DrawerComposeController`'s road-ladder family is the model. The two food webs' compose sheet is
reached from a SOURCE ROW and carries a stance, an escapement floor, a policy ceiling, a take-species
chooser and a projection chart — **a deposit has none of those**, and giving it that sheet would invent
parity the simulation does not have.

**So the card is a READOUT with a crew stepper and is deliberately not a rung ladder.** What it
borrows from the road card, verbatim:

- **the rung as the head's second line**, read off `rung` and **never thresholded off
  `build_fraction`** — that meter belongs to the rung being RAISED, which is a different rung;
- **the `Band:` picker at the top**, through the compose sheet's own `_build_band_picker`, so `Band:`
  here and `Band:` there line their value controls up at one declared key width. A pick REFILLS the
  same Window and must not close the card;
- **the build line through the SHARED sentinel fork** (`DetailFormat.build_countdown_value`). A working
  publishes the identical five sentinels a patch, a herd and a road publish — there is deliberately no
  deposit dialect — and `build_blocked_reason` is the shared `BuildGate` vocabulary, rendered as the
  shared aside rather than re-worded;
- **the keeping's bool-before-the-number rule.** `has_neglect_grace == false` means *nothing at risk
  here* — a working on either free floor — and the countdown beside it reuses the "biting now" `0`,
  so `is_at_risk` reads the bool first. A free floor states NO keeping rows at all: a sentence saying
  *free* is a row spent on the absence of a bill.

**What it adds is the crew stepper, and that is the one place a working differs from a road.** A road
is not worked — which is why it has no stepper — and a working IS. The stepper is the TAKE crew; the
keepers are the band-wide pool on the Work tab, and `CARD_CREW_HINT` says so, because a player who
staffed this stepper expecting the bill to be met would watch the working go back anyway.

### `queue_position_of` answers the fork's question with the field the row publishes

`DetailFormat.build_sentinel_value` forks `-1` on *does any band still have this source queued*, which
it reads as a queue POSITION. A deposit row carries membership (`is_queued`) rather than a rank, so
`queue_position_of` maps the flag onto that fork's two answers and **the rank is never invented**.

## ⛔ THE STOCK IS THREE NUMBERS AND `reachable` IS NOT A RESTATEMENT OF EITHER

`stock` is what is standing, `capacity` is what this GROUND holds when full (the terrain's — no rung
may raise it), and `reachable` is what the CURRENT rung can get at. **A low rung standing on a full
seam shows a large capacity and a small reachable, and that gap is the argument for climbing the
ladder** — `extraction:gathering`'s `recovery_fraction` is 0.15, so a surface picker on a 2200-unit
rock body reaches 330 of it and no more. Collapsing any two of the three hides the argument, and
drawing `stock` as *what you can have* promises the player rock the crew cannot cut.

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
on ROWS.** A pool stepper on the block HEAD is the band-wide pool itself: it names no working, staffs
no crew on one, and is the identical control that would otherwise have sat in a card one block up.

**THE PER-ROW RULE IS UNCHANGED AND STAYS ENFORCED: no stepper, no crew count, no kit picker on any
ROW.** The hands that CUT a working are on the tile card's `Workings ▸`, and
`band_panel_preview._assert_the_workings_roster_names_its_workings` asserts that absence on the
stepper's own `−`/`+` faces — **scoped to the ROWS and not to the block**, since a block-scoped search
now finds the head's own control and would pass or fail for the wrong reason.

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

**THE VALUE CELL IS `HudDepositVocab.deposit_row_value`, VERBATIM** — the same composer the card's
state line uses, so the card and the roster cannot disagree about a working's state and §7's fork is
taken once. The row's INK is that composer's own answer too (`deposit_value_color`), keyed on the
hazard mark it puts there rather than on a second test.

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
working's card. **A working-shaped abandon is server-side work and is not this arc's.**

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
- **`extract` is a TARGETED grammar of its own** — `extract <x> <y> <material> <workers>` — and **the
  material is not optional**: one tile can hold two workings, so a line naming only the tile names
  neither. It rides the **`species` token**, which is where the sim's own `extract` arm reads it from
  and which means the same kind of thing on a forage row (*which of the things on this ground are you
  here for*). No floor and no kit token: a deposit has no escapement floor to leave standing, and
  `default_kits.extract` is the bare `none` kit with no picker anywhere on the card, so the tail is
  closed and the line is byte-stable.

**`command_guard` is what keeps the two enumerations in step, and it now drives both.** `quarrywork`
joined `ASSIGN_LABOR_ROLES` (the sweep asserts every role in that list builds a line AND that an
unknown one builds none) and `extract` is the FOURTH targeted drive — the sweep cannot reach it, a role
in that list taking a bare count. `ASSIGN_LABOR_EXPECTED` moved 10 → **12** and is re-derived at
runtime from the list, so the literal cannot go stale unnoticed.

## Tests

### The card — `ui_preview`'s `workings` chapter

`tools/ui_preview/chapters/workings.gd`, appended LAST in `CHAPTERS` so no existing frame moves. Four
frames and eighteen checkpoints; it hands the hex back bare on the way out.

| frame | what only IT can say |
|---|---|
| `workings_two_seams` | ONE hex, TWO blocks, each keyed by its own `(tile, material)` — the claim a one-block-per-tile card cannot pass — plus the stock row's three numbers on the seam whose reach is a fraction of its stock |
| `workings_over_cut` | the RENEWING arm: the state line carries the food webs' own overdraw word, **the same seam cut inside its renewal states nothing** (the pair, since a negative alone passes on a composer that never warns), no runway on a renewing working, the figures on the hover, and the bill and countdown the road card's treatment gives them |
| `workings_runway` | the FINITE arm beside a renewing **flint scatter of the SAME branch** — the one fixture that fails a client forking on `branch` — and both free floors stating no bill at all |
| `workings_idle` | `-2` reads *not being worked*, **never `0 turns left`**, and the block carries the take crew's own stepper |

**Every claim is asked of the shipped composer or of the rendered card**, never of a re-derivation:
the §7 clauses go through `deposit_row_value`, the bill through `upkeep_value`, the countdown through
`reverting_value`. The card is a `PopupPanel`, i.e. a **`Window`**, so a `Control`-rooted finder walks
straight past it — the road ladder's own trap — and the chapter's `_collect_meta` walks every `Node`.

**The fixtures are shaped exactly as `dict/deposits.rs` writes a row**, at the shipped
`extraction.json` proportions (mixed woodland 600 at 0.03, karst highland 3000 at 0.0, a periglacial
scatter at 70), so a claim here is a claim about the wire.

### The pool and the roster — `band_panel_preview`

**`POOL_CARD_COUNT` stays 4 and every pool-card assertion is byte-identical to `origin/main`** —
checked by diff, not by eye — which is the check that says the pools block really did go back. Its
doc block records the 439px and the rejected second row so the next reader does not re-derive them.

The roster's own block asserts, on one fixture whose near hex carries TWO workings: the negative (a
working this band does not work is not listed), the count of THREE with two of them on one tile, the
nearest-first sort tie-broken by MATERIAL, the material-led name cells, the value cell as
`deposit_row_value` verbatim, §7's fork read off two rows of ONE roster, and **no stepper or `✕` on
any ROW** — scoped to the rows.

`_assert_workings_roster_head` is the head's own group, made on the shortfall state AND on case 2:
the stepper is there with both faces, the title carries the pool's hint on its hover, the mark is
flown, and **reserved ≥ drawn is PRINTED** for the head (21 of 21) and for the block (105 of 105, 49
of 49 on case 2). Case 1 asserts the block is absent **and that the stepper goes with it**, since a
missing control looks the same whichever reading produced it.

**Frames:** `band_panel_workings_roster` (the head with its stepper and its `⚠` over three rows, under
a four-card pools block) and `band_panel_workings_roster_unseen` (case 2 — the head still drawn, the
muted line in place of the rows).

## See Also

- `.claude/rules/core_sim/extraction.md` — the sim half: the deposit as a stock with a regrowth rate,
  the `quarrywork` pool's claims, and what each field on the wire means
- `.claude/rules/client/roads.md` — the direct precedent for all three surfaces: the road ladder card,
  the `roadwork` pool and THE ROADWORK ROSTER
- `.claude/rules/client/band-city-panel.md` — the Work zone the pool block and the roster share
- `docs/plan_extraction.md` §7 — which readout a working publishes, and why the rate decides it
