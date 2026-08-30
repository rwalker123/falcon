---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_route_vocab.gd"
  - "clients/godot_thin_client/src/scripts/ui/AnnotationRenderer.gd"
  - "clients/godot_thin_client/native/src/dict/routes.rs"
  - "clients/godot_thin_client/src/scripts/ui/hud/RungLadder.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/RungGates.gd"
---

# Roads — the client half of the intensification ladder's third branch

The sim side is `.claude/rules/core_sim/routes.md`, which is authoritative for what every field on
the wire MEANS; this file is what the client does with them. Read that one first — most of the traps
here are its traps, arriving one layer out.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_route_vocab.gd` (`HudRouteVocab`) | The road VOCABULARY leaf — the four rung keys + their labels + `RUNG_ORDER`, the tile card's **five** row keys and their formats, one reader per wire field, and one composer per row (`road_row_value` and its `progress_clause` / `bonus_value` / `upkeep_value` / **`keeper_value`** / `reverting_value`, joined by `road_lines`). It also owns the four `*_value_hex` forks `DetailFormat._value_hex` dispatches to, so a road's ink is decided beside the words it tints. A vocab module with static funcs, the `hud_work_vocab.gd` shape; it reads `SourceForecast` / `DetailFormat` / `HudSelectionVocab` / `HudConst` / `HudStyle` inside functions only, never in a `const`, so it adds no load cycle — **and that contract is what lets `SourceForecast` alias its four `RUNG_KEY_*` at `const` level** rather than spelling the wire's route rungs twice |
| `ui/AnnotationRenderer.gd` → the `ROAD_*` family | The map draw: `draw_road_network` walks `MapView.road_network` (world state read through the `_view` back-ref, exactly as `units` / `herds` are) and `_draw_road` stamps **one HEX per road** — `MapView._hex_center_wrapped` for the placement, `_outline_hex_at` at `ROAD_TILE_RADIUS_FACTOR` of the tile radius for the ring. It is called from `_draw` right after the crisis annotations — above the tile tints, beneath every marker, ring and selection outline, because a road is infrastructure IN the ground rather than something standing on it |
| `ui/hud/RungLadder.gd` → `route_track` / `build_track`'s `title` | **THE ROUTE BRANCH'S ROW PRODUCER, a SIBLING of `track` and never a widening of it** — `track` takes a labor `kind` and a wire source dict, and a road has neither. It emits `track`'s own `ROW_*` shape so the RENDERER is shared (`build_track` gained one optional heading argument and nothing else), walks `HudRouteVocab.route_ladder`'s ordered catalog, and owns the branch's seventh state, `STATE_UNORDERED` — the rung nobody declares. Its two private leaves are `_route_progress_aside` (the meter, on the row DIRECTLY above the standing rung and no other) and `_route_hold_asides` (the standing bill, in the plant/animal branches' own sentence) |
| `ui/hud/RungGates.gd` → `route_gates` / `route_knowledge_reason` | **THE ROUTE ARM of the shared gate layer**, keyed **RUNG KEY** rather than verb (two route rungs declare none, so a verb-keyed table cannot tell `path` from `trail`). Four gates in reading order — the ground, the un-orderable rung, the craft, the keeper — each carrying its own remedy. The craft's NAME is threaded in as a `{knowledge_id: display_name}` parameter off the ladder's knowledge roster, never a table here; its REMEDY is the rung whose `earns_knowledge` names that craft, **looked up through `HudRouteVocab.ladder_rung_teaching` and never inferred from `requires_rung`** |
| `ui/hud/DrawerComposeController.gd` → the `build_road_drawer_actions` family | The tile card's `Road ▸` action and the `PopupPanel` it opens (`_open_road_ladder` / `_emit_road_improvement` / `_ensure_road_ladder` / `_road_ladder_anchor_rect` / `_dismiss_road_ladder`), filling `%RoadLadderControls` — its own container at the BOTTOM of the card, since `%ForageAssignControls` is gated on a gathering site with a band in hand. It emits `road_improvement_requested`, which `HudLayer` relays straight onto `improvement_requested` with **no** optimistic overlay write |
| `native/src/dict/routes.rs` | `routes_to_array` — **one dict per road TILE**, the `connections.rs` shape. The row's identity is `tile_x` / `tile_y`, which replaced the retired `RouteId`; beside them ride `has_keeper` / `keeper_band_id` (read the bool first — `0` is a real `BandId`) and `keeper_remoteness`, the multiple distance put on that road's price. There is **no path on the row** — a link knows its two endpoints, so the tiles between them are computable. **`route_rungs_to_array` is the file's second producer and answers a different question** — one row per RUNG of the branch, published once per world beside `ladderKnowledge`, carrying no faction and no tile |

## ⛔ A ROAD IS NOT AN ORDER PATH, AND THE OBVIOUS NAME WAS ALREADY TAKEN

`AnnotationRenderer._routes` — fed by `MapView`'s `snapshot["orders"]`, drawn by `draw_routes`, and
covered by `map_preview`'s `"routes"` state — is the per-faction **ORDER PATH** overlay: the
waypoints a player's own movement orders are following, coloured by FACTION, which vanish when the
order does. A road is a world object IN THE GROUND — one tile, belonging to no faction, outliving
every band that walks it — and it is coloured by RUNG. (Belonging to no faction is not the same as
having nobody on the hook: exactly one band KEEPS each tile, which is a JOB. **The word `owned` is
retired from this arc.**)

**So the client noun for this branch is `road` everywhere** — `MapView.road_network` /
`road_tile_lookup`, `AnnotationRenderer.draw_road_network`, `HudRouteVocab`, `ui_preview`'s
`road_tile_*` frames and `map_preview`'s `map_road_network`. Do not rename either into the other, and
do not "unify" the two draw passes: they read different sections, live in different layers of `_draw`
and answer different questions.

## ⛔ THE FLOOR RUNG IS `route:path` / `Path`, AND IT WAS RENAMED OUT OF A FALSE ORIGIN

It was `route:game_trail` / `Game trail`. Nothing in the simulation makes one: exactly ONE pass banks
route work — the pooling-link pass in `core_sim/src/supply.rs` — and no animal has ever worn a step
of any road. The commonest way a tile comes to hold the floor rung is the player's own bands walking
the same ground while their food pools, so `Road  Game trail · 1% to trail` was the client telling
him an animal made the path his own trade traffic wore in.

It is the second half of the fix that deleted the tile card's `nothing — a path the animals made`
clause (issue #566, below): that sentence and this rung name asserted the same unmodelled cause, and
removing only the sentence left the cause standing in the value beside it. The wire key is
`route:path` **exactly**, matched to what the sim publishes.

**The hunting food-site `kind == "game_trail"` is a DIFFERENT concept and keeps its name** —
`HudBandLaborState.FOOD_SITE_KIND_GAME_TRAIL`, the `MapView` marker colour table, `SiteSprites`' art
key. That one names where game is hunted, which the sim does model. Grep before renaming either.

## THE DRAW IS A HEX, NOT A POLYLINE — and the stamp is INSET

A road is one tile, so the draw stamps its own hex and a run of road reads as a chain of stamps. That
is the honest picture rather than a concession: the two ends of a long road can genuinely stand at
different rungs and be kept by different bands, which a single polyline could not say.

**`ROAD_TILE_RADIUS_FACTOR` (0.62) is why the ring is inset rather than flush**, and both halves of
the reason are visual. A flush ring sits exactly on the hex grid's own edges and reads as a GRID LINE
rather than as a thing on the ground; and two adjacent road tiles SHARE that edge, so a run would
draw its interior seams at twice the weight of its outside. Inset, the run reads as a chain of stamps
with ground showing between them.

**The placement is `_hex_center_wrapped`, the single-tile idiom** — a road names its tile by its DATA
column, so the stamp goes on the copy of that column the viewport is over. `_unwrapped_path_points`
is the connected-path helper and has nothing left to unwrap here.

## The rung ladder is ONE INK AT FOUR OPACITIES — a correction made against a rendered frame

The first cut walked the palette's own ink ladder (`LINE_SOFT` → `INK_FAINT` → `INK_DIM` → `INK`),
which is faintest-to-strongest **on the HUD's dark ground and INVERTED on the map's**. Rendered over
tan steppe in `map_road_network`, the path drew as a near-black hairline carrying the most
contrast on the frame while the paved road drew as pale grey — the ladder read backwards.

**A rung has to read as PROMINENCE on ground of any tone, and only opacity does that.** So the
prominence ladder is `HudStyle.INK` at `ROAD_OPACITY_PATH` → `_TRAIL` → `_DIRT_ROAD` →
`_PAVED_ROAD` (0.30 / 0.52 / 0.74 / 0.94), derived at draw time — the map's own idiom for a themed
overlay tint, the one `MapView.SUPPLY_LINK_COLOR` already uses. The WIDTH ladder rides beside it
because thickness reads before tint at map zoom.

- **A road in SHORTFALL draws in `HudStyle.DANGER`, at the TOP of the opacity ladder** and at a fixed
  mid-rung width. It is losing a real investment, which is the same news a starving pen's ring
  carries, so it borrows the client's existing at-risk ink rather than inventing one — and an alarm
  that faded with the rung would be quietest on the trail nobody notices going.
- **An unknown rung draws at the floor rather than vanishing.** A road is a real thing whatever the
  ladder calls it.
- **ONE ROAD IS ONE VISIBILITY QUESTION**, and it is `Discovered` rather than `Active`. The
  per-SEGMENT gate this replaced existed because a stored path could run off the explored map with
  only one of its tiles seen; per tile the sim's own gate — *have you seen that tile* — is the whole
  of the test, and `_road_tile_known` is called once per road. `_is_tile_visible` would be the wrong
  test in the other direction — it demands `Active`, and a road does not wander off, so a remembered
  one is remembered truly.

## The tile card is the road's readout, and it sits ABOVE the remembered-tile early return

A road is IN THE GROUND, so it is a property of the LAND, and the land drawer
(`SubjectDrawerController._tile_terrain_lines`) is the one surface in the client whose subject is a
piece of ground. The rows are appended with the RIVERS — terrain-intrinsic permanent geography —
which puts them **above `_tile_terrain_lines`' Discovered early return**, and that placement is the
sim's own fog gate read back: a road is published to a faction that merely *remembers* the ground, so
appending below that return would drop the whole block from every hex the sim went to the trouble of
sending it for. An UNEXPLORED hex is already covered — the producer returns before either.

The rows reach the drawer through `tile_info["roads"]`, stamped by `MapView._tile_info_at` from
`road_tile_lookup`, the forage patch's own cross-ref idiom. It is deliberately **not** in
`MapView.FOW_DISCOVERED_HIDDEN_KEYS`, for the same reason.

**ONE BLOCK PER ROAD**, and per tile that is exactly one — the registry is keyed by tile sim-side.
`road_tile_lookup` stays an ARRAY per hex anyway, so a duplicated row would render twice rather than
vanish silently, and the drawer's block loop needs no special case.

### ⛔ EVERY ROW IS CONDITIONAL, AND A FREE PATH IS ONE ROW

The block printed **five rows on every road in the world** (`Road` / `Wearing in` / `Kept by` /
`Keeping` / `Buys`), which on the commonest road in the game — a free path — read:

```text
Road        🛣 Path
Wearing in  Trail 25%
Keeping     free — nobody keeps a path
Buys        nothing — a path the animals made
```

Four rows to say that a thing costs nothing and does nothing, two of them prose whose whole content is
*no*. It is one row now (issue #566): **a row that would say "none" is not rendered at all.**

```text
Road        Path · 25% to trail
```

**The rows read like the rows above them on the card** — `label · value · qualifier`, the shape
`Foraging  90 / 100 · Thriving` and `Grazing  9 / 10 · Thriving` already use, clauses joined by
`ROAD_CLAUSE_SEPARATOR`. A road-specific style on a card of ecology rows reads as a different card's
row.

| Row | Rendered when | Says | The trap it avoids |
|---|---|---|---|
| `Road` | a road exists on the tile — **the only unconditional row** | the rung it HOLDS, plus `N% to <next rung>` while something is rising above it, plus the branch's own hazard word `washing out` when short | ⛔ **THE PERCENTAGE MUST NOT READ AS "THIS ROAD IS 25% BUILT".** A path at 25% is a COMPLETE path a quarter of the way to becoming a trail, so the rung stands alone as the value and the meter is a qualifier naming where it is GOING. The retired `Wearing in: Trail 25%` row said the opposite, and `build_fraction` is a DIFFERENT rung's meter — a reader that thresholded it would call a fully-worn trail a dirt road on the turn its first traffic banked. **Rendered only below `1.0`**: the wire states exactly `1.0` for a rung just finished AND for the top of the ladder, so the test is a plain comparison |
| *(the bonus)* | the rung saves something on the loss axis | ⛔ **what the rung is BUYING, in ONE clause** — the loss it saves | see below. **UNLABELLED** — `Buys:` was a key doing no work, the value already reading as a benefit. The sight and the span moved to the block's HOVER (`bonus_tooltip`): the row printed all three and wrapped to three lines under a one-word value |
| `Upkeep` | the road actually owes something — **neither free rung does** | the bill, the SHORTFALL where there is one, and the keepers the bill wants | all three figures are published; `demand − supplied == shortfall` holds verbatim on the wire and the keeper count is the sim's own `ceil`. **The word is `Upkeep`** — see below |
| `Kept by` | the road has a keeper, or owes a bill nobody is paying | ⛔ **WHOSE JOB THIS ROAD IS**, and what distance is charging them for it | see below |
| `Reverting` | the road is at risk | the COUNTDOWN, and `now` at zero | `0` means it is reverting NOW. It renders only while the road is genuinely short, because a road whose bill is met reads its rung's full grace + 1 — *"walk away and you have this long"* — which is not news |

**RETIRED — `ROAD_GLYPH` (🛣).** It led the rung row's VALUE on a row whose KEY already reads `Road`,
so it said the word twice. The plant/animal badges it was copied from lead rows whose keys name a rung
(`Cultivation`, `Corral`) rather than the thing itself.

### ⛔ THE WORD IS `Upkeep`, AND IT SHARES ITS KEY WITH THE BAND'S MATERIAL BILL

The row read `Keeping:` for a slice — a second word for the thing
`HudDisclosureVocab.DETAIL_ROW_UPKEEP` names on the band's own card and `upkeepDemand` /
`upkeepSupplied` name on the wire. One concept, one word: `ROAD_UPKEEP_ROW` is the literal `"Upkeep"`
and **both rows land on the same arm of `DetailFormat._value_hex`**, which is the point rather than a
collision. The band's bill is recognized there by the shared runway spelling and returns first; the
road's carries no runway and no material context, and falls through to `upkeep_value_hex`, which
answers WARN only on the hazard mark its own composer put there — so a band bill that declines the
runway tint reads the plain ink it always did.

**AND THE FREE READING IS GONE WITH THE ROW.** `free — nobody keeps a path` was a sentence
spent on the absence of a bill, on every road a current game can contain. The floor declares no upkeep
at all, and the row's ABSENCE states that without a line.

### ⛔ THE `Kept by:` ROW IS THE ONLY SURFACE A REAL DECISION HAS

**The keeper is the band that BUILT the tile, wherever that band has since walked.** `route_keeping_claims`
walks the roads a band keeps and never reads that band's position, so a camp four tiles away goes on
paying and goes on being served. Nothing else in the client can say that, which is why the row exists.

- **The NAME is resolved by the drawer, never by the vocab leaf.** A road carries a `band_id` and this
  client has exactly one band-naming rule (`HudBandLaborState.band_label_for_id` →
  `HudFormat.band_display_name`), so `SubjectDrawerController` looks it up and hands `keeper_value` a
  LABEL. A band outside the player's roster resolves to `""` and reads **`another people`** — a road
  really can be kept by a people you merely know of, and a raw id is a fact nobody can act on.
- ⛔ **THE REMOTENESS CLAUSE IS THE POINT.** `keeper_remoteness` is the multiple the sim quoted when
  the keeper took the tile on, applied to BOTH the build pile and the standing upkeep — so a road
  beyond `route_range.base_tiles` is dearer for exactly one reason and this is the only row that says
  which. **Distance is a cost and never a wall**: nothing refuses the road, so without this the player
  sees a bill larger than the rung says with no explanation anywhere. It is a PRESENTATION of the
  published multiple; the client never multiplies anything.
- **It rides the KEEPER's row rather than the bill's** because it is a fact about the job, not about
  the rung — and it reads in plain ink for the same reason: a keeper at a distance is a price, not an
  alarm.
- **NO ROW AT ALL on a road that owes nothing and nobody keeps** — that is the free floor, where there
  is no job to be on the hook for. A road that OWES a bill with no keeper is the opposite
  case and says so in **amber**: the keeping band is gone, so it is decaying towards nobody, and
  **re-issuing `grade` / `pave` is how another band picks it up**. The clause says that rather than
  naming an adopt verb, because adoption *is* the same act as building.

### ⛔ THE PAYOFF ROW IS THE POINT OF THE WHOLE READOUT, AND IT CARRIES NO LABEL

The route ladder is deliberately **not** a straight upgrade path: a road is cheaper to travel and
dearer to keep, and the player is meant to pave only where the traffic pays for the upkeep. Without a
visible statement of what a rung buys, every road reads as pure cost and the decision the branch
exists to create is invisible — §4.9 item 12's *"a tax, not a ladder"* trap, on the client side of
the wire. It is the one row on the card that states a PAYOFF, which is why it is tinted rather than
left in plain ink beside the bill below it.

**`Buys:` was a key doing no work** — *40% less loss* already reads as a benefit, and the label only
narrowed the column the sentence had to live in. So the value renders bare — and it is ONE clause,
the loss figure, the sight and the span having moved to the block's hover.

> #### ⛔ THE KEY IS A BLANK, NOT ABSENT, AND THAT IS STRUCTURAL
>
> `ROAD_BONUS_ROW` is `" "`. `DetailFormat.detail_bbcode` renders a colon-free line **full width and
> CLOSES the open `[table=2]` to do it** (`_split_kv` refuses `idx <= 0`, so a genuinely keyless line
> is unreachable as a table row) — and this row sits in the MIDDLE of the block, so a keyless payoff
> would split the tile card's one table in two and the road's keys would stop sharing a column with
> `Foraging` / `Grazing` below them. A blank key keeps the row in the table and the value in the value
> column, which is what "bare" has to mean here. `DetailFormat._value_hex` dispatches on it to
> `bonus_value_hex()`, which **takes no value**: the row is emitted only where the rung buys
> something, so there is no second reading to fork on.

Three clauses, each off a published field:

- **the friction it saves**, as the percentage of the base loss it takes off. `friction_multiplier`
  is the fraction of that loss a bound network pays, so `0.6` renders as *40% less lost between
  bands* — a presentation of the published multiplier, never a re-derivation of a sim answer.
- **whether it is lighting its tiles**, off the RESOLVED `grants_sight`, because a client cannot
  re-derive *"is the bill met"* (that is a comparison against the stamped basis with the sim's own
  epsilon). Its other half is said out loud: a built road whose bill is unpaid reads *dark until its
  upkeep is paid* — the row above it's own word — because a road **goes dark BEFORE it decays** and a clause that merely vanished
  would read as a rung that never lit anything. Gated on the shortfall rather than on "unlit",
  because the PATH lights nothing even with its (interpolated) bill paid in full.
- **the link span, in FUTURE TENSE, and that is not a style choice.** `holds_link_to_tiles` is
  authored on every route rung and **not yet read by the sim** — nothing in `balance_supply_networks`
  consumes it; that is slice 13b. Rendering it in the present tense would state an effect that is not
  in play, so it reads *links N tiles out* — future tense carried by *links … out* rather than by a
  `will hold` the wording pass cut.

**A rung buying nothing on every axis RENDERS NO ROW**, which is both free rungs. It used to say so in
words — `nothing — a path the animals made`, in dim ink — and that sentence was **factually wrong, not
merely wordy**: it asserted an ORIGIN the sim does not model. A path is a rung a tile HOLDS, and
the commonest way a tile comes to hold one is the player's own bands walking the same ground and
banking traffic into the meter; nothing about it is a path animals made. The row's absence states the
same fact — both of the floor's terms are at their own neutral — and cannot state a false one beside
it.

## The `roadwork` pool, and what a fourth card cost

`roadwork` is an ordinary band-wide standing role in exactly the grammar `agriculture` and
`husbandry` use — `assign_labor <faction> <band> roadwork <n>`, one more arm on
`Main.format_assign_labor`'s shared role branch, one more card in the Work tab's POOLS block.

⛔ **ITS HINT NAMES THE ROADS THE BAND BUILT, NOT THE GROUND IT IS STANDING ON.** The catchment is the
KEEPER: a band keeps the tiles it graded or paved, wherever it has since walked, and what distance
costs is priced into each road's own `keeper_remoteness` rather than into whether the bill exists. The
hint said the opposite for a slice — *"the roads this band is standing on"*, which was true of the
stored-path model — and under the per-tile model that reading sends a player to move camp in order to
escape a bill that follows them regardless.

> #### ⛔ AND THE CORRECTION LANDED IN THE HINT AND NOT IN THE TOOLTIP EIGHTY LINES BELOW IT
>
> `HudWorkVocab.UPKEEP_POOL_COVERAGE_ROUTE_FORMAT` — the card's own `tooltip_text` — went on reading
> *"the roads this band stands on need %s"* for a slice after `ROADWORK_ROLE_HINT` directly above it
> had been fixed AND had written down why. **One model, two player-facing strings, one of them
> corrected**: the wrong one was the tooltip a player opens precisely when the bill surprises them.
> Both name the roads the band BUILT now.
>
> ⛔ **ITS SECOND CLAIM WAS FALSE TOO** — that the route sentence names no queue *because a route rung
> takes no builder and appends no build-queue entry*. That is the FREE FLOOR alone: `grade` / `pave`
> append an ordinary `BuildQueueEntry` funded by the band's `builders` pool, exactly like every rung
> on the other two branches. **The sentence still names no queue, and the true reason is the FIGURE**
> — the road pool's `asked` is the cohort's published `roadwork_demand` verbatim, `BandPanelController`'s
> road branch deliberately not going through `_pool_coverage` (the road rows are fog-filtered, so
> summing them client-side would understate a bill the band still owes). A sentence promising a
> queued half the number does not carry would be the worse of the two errors.

> ### ⛔ AND `roadwork` COULD NOT BE STAFFED AT ALL, WHICH IS `builders`' DEFECT ONE ROLE LATER
>
> A role passes TWO gates: `sim_runtime::command_text`'s grammar and the server's own
> `handle_assign_labor`. `roadwork` was in the server's list and **not in the grammar** — and the
> client's native bridge parses a line there BEFORE it sends, so every `assign_labor … roadwork n`
> was refused inside the client with nothing failing anywhere. It is the identical hole `builders`
> fell through, and `command_guard`'s role sweep is what found it both times.

### ⛔ THE BILL IS A COHORT FIELD. DO NOT SUM THE ROAD ROWS.

`HudBandLaborState.roadwork_pool_state` reads `roadwork_demand` / `roadwork_supplied` /
`roadwork_shortfall` off the band and does no arithmetic. The `routes` rows are **fog-filtered**, so a
road out of sight would silently drop out of any client-side total while the band certainly still
owes its keeping — `fodder_need`'s rule, load-bearing for the identical reason. The demand is summed
sim-side BEFORE the head-count gate, so a band with nobody on `roadwork` publishes the bill it is
failing to pay rather than a reassuring zero: it is the alarm.

That is also why `HudWorkVocab.upkeep_pool_coverage_line` grew an **optional**
`POOL_COVERAGE_SHORTFALL_KEY`. Where the sim publishes the gap, that is the coverage test and the
client does not subtract; the two food webs publish no such band-level roll-up, so they keep the
projection-vs-demand test. The reader tests `has()` rather than defaulting, because a defaulted `0.0`
would read as *this pool covers everything* and clear the mark on every card in the game.

The road bill also counts toward the fund-mode row's *"is there anything to fund"* gate: route
keeping runs through the same `distribute_upkeep_pool` under the band's own `upkeep_fund_mode`, so a
band whose only standing cost is a road it KEEPS must still be offered the split.

### The row of four does not fit at the shared metrics, and the zone had no row to give

Measured, not assumed. A pool card's floor is its STEPPER, and at `WORKER_STEPPER_*` that is ~112px:
four of them wanted **466px** of a WORK zone box that is **382** on the bottom dock and **356** on the
left, and `band_panel_preview`'s overflow probe named every control that ran off the right edge. Both
ways to buy that width cost a ROW, and the zone has none — split 3 + 1 the block wanted **420px of a
358px box**, which is the build queue drawing nothing.

**So the width came out of the CONTROL.** `HudWorkVocab.POOL_STEPPER_*` and
`POOL_CARD_NAME_FONT_SIZE` are the pools block's own metrics and nobody else's: the WORKFORCE zone's
Scout and Warrior cards sit two to a row and have width to spare, so narrowing their steppers would
shrink a control for no reason. The horizontal trim (`POOL_STEPPER_PADDING_H`, through
`HudWidgets.compact`'s opt-in `padding_h`) is the load-bearing half — `HudStyle` pads a button 11px
each side, which alone floors it near 30px whatever `custom_minimum_size` says.

⛔ **THE STEPPER MUST STAY THE CARD'S FLOOR**, and `band_panel_preview._assert_pool_cards_are_level`
is where that fails. Four cards fit a 356px strip only while each is ~83px, and each is ~83px only
while a fixed METRIC is the floor rather than a role NAME, which is content: the moment a name
becomes the floor, one longer role name silently pushes the row past the zone's edge.

## ⛔ THE ROUTE BRANCH'S SURFACE IS A LADDER, NOT A BUTTON PER VERB

`grade` and `pave` worked on the command channel and **nothing in the HUD issued them**. Every other
ladder verb is declared from a WORK ROW (`BandPanelController._emit_ready_declaration`), and roads
deliberately have no work row — *"a road isn't active like hunting or foraging is, so you don't need
the tile workers"* — so the route branch was the one branch with no way to press it.

**The answer is not a button per verb.** Highways and railways are RUNGS, so a control per verb grows
one control per rung forever; and a single verb-named button is worse still, because it forces ONE
refusal string and cannot answer *"paving is out of reach but railroad is not"*. **The unit the
player presses is the LADDER**: one action opens the whole branch, one row per rung in climb order,
each carrying its own price, its own payoff and its own gate.

**A RUNG ADDED TO `intensification_ladder.json` MUST APPEAR WITH NO CLIENT CHANGE.** That is the
requirement the whole design is arranged around, and it is what makes
`SubsistenceSection.routeRungs` — not a client table — the authority for every label, price, payoff
and gate reason on the card.

### The wire catalog, and the one table it must NOT be read from

`RouteRungState`, published once per snapshot beside `ladderKnowledge`: `rungKey` · `order` ·
`displayName` · `verb` · `unlockKnowledge` · `requiresRung` · `workCost` · `upkeepWorkPerTurn` ·
`frictionMultiplier` · `holdsLinkToTiles` · `grantsSight` · `earnsKnowledge`. Per WORLD, carrying no faction and no
tile, so it is diffed whole like `kits` and cleared at the world boundary
(`FactionReadouts.reset_world_state`) — a delta never restates it, so the previous game's rungs would
otherwise still be on the card.

⛔ **`HudRouteVocab.RUNG_LABELS` MAY NOT NAME A LADDER ROW.** That table is the tile card's readout
vocabulary and is a hard-coded four; a fifth rung read through it renders as its raw wire key. The
sheet names every row from `catalog_display_name`, which is the sim's own word.

**THE FOUR `""` FIELDS ARE STATES, NOT ABSENCES**, and each reads as its own row: `verb` is empty on
a rung nobody declares, `unlockKnowledge` on one nothing gates, `requiresRung` at the floor, and
`earnsKnowledge` on a rung that teaches nothing (the floor, and the top, which has nothing above it
to open).

### `RungLadder.route_track` is a SIBLING of `track`, and `build_track` is shared

⛔ **`track` TAKES A LABOR `kind` AND A WIRE SOURCE DICT, AND A ROAD HAS NEITHER** — no crew, no
per-source forecast row, no queued entry publishing legs, no key prefix. Widening it would push every
plant and animal call site through a branch it cannot use. So the PRODUCER is a sibling and the
RENDERER is not: `route_track` emits `track`'s own `ROW_*` shape and hands it to `build_track`, which
gained one optional `title` argument and nothing else. A row is a row on any branch — same name, same
face, same asides in the same order — and a second render loop would drift.

**Three of the six original states are unreachable here, structurally**: `path` and `target` name
legs of a QUEUED entry and no road publishes one.

> #### ⛔ A RUNG IS ONE LINE, AND EVERYTHING ELSE IS ONE HOVER AWAY
>
> The card printed up to SIX lines per rung — a state word, a price aside, an approach, a remoteness
> clause, a payoff and a standing bill, plus one wrapped sentence per unmet gate. Reported from play
> as *"the most wordy dialog I think I've ever seen"*, and the diagnosis was right: none of it was
> wrong, all of it was at once, and the decision the row exists for — *can I afford this yet* — was
> the hardest thing on it to find.
>
> ```text
> RAISE IT TO…
> Path          where you are
> Trail         30% · wearing in
> Dirt Road     110 work · needs Roadbuilding
> Paved Road    260 work · needs Paving
> ```
>
> **The face is `<figure> · <nearest refusal>`**, and where the rung is buildable the figure IS the
> button with no refusal beside it. The row carries a finished `ROW_FACE_KEY` because only its
> producer knows which refusal is nearest; `ROW_BUILD_ASIDES_KEY` / `ROW_HOLD_ASIDES_KEY` /
> `ROW_REASONS_KEY` stay EMPTY, so `build_track`'s aside loops emit nothing without needing to know
> the branch apart.
>
> ⛔ **NOTHING WAS DELETED — IT MOVED TO `ROW_TOOLTIP_KEY`.** The payoff, the standing bill, the
> remoteness multiple and EVERY refusal are on the hover. **A cut that loses the detail instead of
> relocating it is the failure mode here and a PNG cannot see it**, which is why the fixtures assert
> tooltips beside faces.
>
> **The tooltip goes on the LINE and on the FACE both.** A `Button` is `MOUSE_FILTER_STOP` and
> answers a hover with its own tooltip, so one left only on the row would never show over the single
> control the pointer is actually aiming at.
>
> **THE METER RIDES THE `wearing in` ROW AND NO OTHER.** That row has no figure of its own; every
> other leads with a price, and the tile card's `Road` line one block up already states the same
> percentage — so repeating it beside a price is duplication rather than a second reading.
>
> ⛔ **AND THE NAME COLUMN IS NARROWER ON THIS BRANCH** (`ROAD_LADDER_NAME_WIDTH`, 96px against the
> shared 150px). At the shared width the value column is 142px of a 292px card and
> `110 work · needs Roadbuilding` does not fit — the row clipped, which was half of why the card read
> badly. It rides the ROW (`ROW_NAME_WIDTH_KEY`) rather than widening `build_track`'s signature,
> because the plant and animal tracks want the wider column they have.

> #### ⛔ THE SEVENTH STATE — `STATE_UNORDERED`, and only the route branch can produce it
>
> A route rung may declare NO verb: the path and the trail above it are worn in by traffic and
> nobody orders them. `locked` is a lie in the one direction that matters — it reads *you may not*,
> where the truth is *there is nothing to order and it is rising anyway* — so the row takes its own
> state, its own face word (`HudWorkVocab.RUNG_TRACK_STATE_WORN_IN`, *wearing in*) and a hover
> naming what raises it. **The word lives in `HudWorkVocab` beside the other six**, so the
> state enumeration stays one table; that block's own header records what an unworded state costs.

⛔ **EVERY ORDERED RUNG LEADS WITH ITS PRICE, REFUSED OR NOT.** A rung a player may plan toward has
to be one they can plan against, which is `RungLadder`'s own rule for the material pile arriving one
currency over. The refusal is the face's SECOND clause rather than a replacement for it, which is
what retired the price aside the row used to stack beneath itself.

**THE HOVER'S ORDER IS THE SENTENCE** — what it costs to build AND to keep, what it does, what
distance adds, then every refusal. The price line renders **only where the rung owes upkeep**: on a
rung that is free to hold it would restate the face and add nothing, and a line that says nothing is
what this cut removed.

### The gates, keyed on the RUNG and not on the verb

`RungGates.route_gates(road, ladder, knowledge, labels, band)` — the shared, stateless layer, because
a renderer must not depend on a controller and the sheet, the card and any later map mark must not
disagree about what is climbable.

⛔ **KEYED ON THE RUNG KEY, which is the one place this branch differs from the other two.** Two
route rungs declare no verb, so a verb-keyed table would hold two entries spelling `""` and could not
tell `path` from `trail`.

⛔ **A REFUSAL IS A `{kind, short, long}` RECORD, NOT A STRING.** A row has width for ONE clause and
its hover has room for every sentence, and one string cannot be both — it was long enough to wrap a
292px row and still too short to keep its remedy. `route_row_refusal` picks the row's clause off
`GATE_ROW_PRIORITY`; `route_tooltip_refusals` hands the hover all of them.

⛔ **AND THE WORD `locked` IS GONE.** A row reading `locked` above a reason said it twice — the
reason alone IS the state, and the row stays disabled by its ink and by being a `Label` rather than a
`Button`. The fixtures assert the word appears nowhere on the card.

Five gates. **The APPEND order is `GATE_ROW_PRIORITY`**, so the row's pick is the first entry
carrying a short form and the hover reads in the order the row chose from — one list, not two, which
is what stops a row and its own tooltip disagreeing about what matters:

| # | gate | reason names |
|---|---|---|
| # | gate | row says | hover says |
|---|---|---|---|
| 1 | **nobody declares it** — `verb == ""` | *(the state's own word)* | traffic raises it; there is no order to give. **Stated ALONE** — a craft or a keeper beside it would read as a prerequisite for something not on offer |
| 2 | **another band already keeps this tile** — `has_keeper` and a `keeper_band_id` that is not the actor's | `Band 2 keeps it` | …and they must give it up first |
| 3 | **no keeper to name** — no acting band | `pick a band` | whoever builds a road keeps it |
| 4 | **the craft** — `unlockKnowledge` below `KNOWLEDGE_COMPLETE` | `needs Paving` | the live %, and — as the remedy — the rung whose `earnsKnowledge` names it |
| 5 | **the ground** — `requiresRung` unmet | `needs a trail` | `Needs a trail first.` |

⛔ **THE GROUND GATE SINKS TO LAST, AND THAT IS NOT AN ORDERING WHIM.** *Needs a trail first* names a
rung the ladder is already displaying two lines up under `where you are` — it is the one refusal the
player cannot miss, so it earns least on a line that holds one clause. Everything above it names
something that is NOT on screen.

**The two keeper gates outrank the craft** because no amount of learning helps a tile that is already
somebody else's job; and `pick a band` outranks the craft because it is the only gate on this card
the player closes with a click rather than with a campaign.

> #### ⛔ GATE 4 WAS MISSING, AND THE ROW RENDERED READY ON A TILE THE SIM ALWAYS REFUSES
>
> `road_verb_refusal` rejects `grade` / `pave` outright when `Road::keeper` names a band other than
> the one issuing the verb — **one band keeps a road tile, never two**, and the refusal is what makes
> co-payment unrepresentable rather than merely discouraged. The ladder asked the other four and not
> this one, so the row was pressable, the command went out, and the player got a command-failure
> event where a greyed row with a reason belonged.
>
> **It needed no wire change**: `has_keeper` and `keeper_band_id` are already on the `routes` row the
> ladder reads, and reading the bool FIRST is load-bearing (`0` is a real `BandId`).
>
> **It is asked ONCE for the whole ladder**, being a fact about the TILE rather than about a rung —
> and only when a band is actually selected, since with none picked there is no *another* to name and
> gate 5 is the honest thing to say instead. The two must never both fire: they are one word apart in
> the player's head, and *pick a band first* beside a named refusal reads as the card contradicting
> itself.
>
> ⛔ **THE KEEPER'S NAME IS RESOLVED BY `DrawerComposeController`, never by the gate layer** — a road
> carries a `band_id`, this client has exactly one band-naming rule
> (`HudBandLaborState.band_label_for_id`), and `RungGates` is stateless and holds no roster. The label
> is threaded in exactly as the tile card's `Kept by:` row threads it, and `""` reads
> `ROAD_KEEPER_FOREIGN` — a real state, a road being keepable by a people you merely know of.

⛔ **AND THE CONSEQUENCE IS INTENDED: A ROAD CANNOT BE BUILT ON BARE GROUND.** `dirt_road` requires
`trail` and a trail is reached only by traffic, so roads are upgraded where people already walk. Do
not add a path around it.

**THE CRAFT IS NAMED FROM THE LADDER'S KNOWLEDGE ROSTER, never from a table here** —
`FactionReadouts.knowledge_labels()` inverts it once and it is threaded in as a PARAMETER, the
statelessness `RungGates` is built on.

> #### ⛔ THE REMEDY NAMES THE RUNG THAT **TEACHES** THE CRAFT, AND IT IS LOOKED UP
>
> It read `requiresRung` — *the rung directly beneath the gated one* — for a slice, on the reasoning
> that a route knowledge is earned by holding the rung below the one it opens. **That is a property
> of the four rungs that ship, not of the ladder.** A trail teaches Roadbuilding and does sit
> directly under the dirt road it opens; `intensification_ladder.json` is free to have a rung teach a
> craft that opens something two rungs up, and the inference then names the WRONG rung.
>
> **It matters because it is a REMEDY.** Every other consequence of that inference would be a
> cosmetic slip; this one tells the player to go and stand on the wrong ground. So the pairing rides
> the wire as `earnsKnowledge` and `HudRouteVocab.ladder_rung_teaching` is the lookup.
>
> **The sentence it produced was byte-identical on the shipped catalog**, which is exactly why the
> defect was invisible and why the fixture that catches it has to state a catalog where the two rungs
> differ — see Tests.

The sentence is a HEAD (what is missing) plus a REMEDY appended only where there is one, because two
facts arrive independently — whether the roster NAMES the craft and whether any rung on this branch
TEACHES it — and a format per sentence would need four consts and a fork to choose between them. A
roster with no name states the progress anyway; **a branch where nothing teaches the craft drops the
remedy clause entirely**, that being a real state (a route rung may be gated on a craft another
branch earns) and a dangling *"keep a  carrying traffic"* being worse than a head alone.

### Where the action sits, and what decides that it is there at all

**THE BOTTOM OF THE TILE CARD, with the Forage and Hunt actions** — `%RoadLadderControls`, its own
`VBoxContainer` after `%HerdAssignControls` in `SubjectBody`. Not a row inside `%ForageAssignControls`:
that container is gated on the tile being a GATHERING SITE with a band in hand, and a road crosses
ground that is neither. Not a button inside the `Road` readout row either — the card's rows are
readouts and a control in one would make it the only place on the card where a stat line is also a
control.

**IT IS SHAPED LIKE A PLAIN ACTION, NOT LIKE FORAGE OR HUNT.** Those open a COMPOSE sheet because
they take a worker count; `grade` / `pave` take none and a trailing count is a parse error — they
DECLARE, and the hands come separately from `assign_labor <faction> <band> builders <n>`.

**LABELLED `Road ▸`, THE BRANCH'S NOUN**, matching the readout row's key one block up. Never a verb:
`grade` stops being the whole story the day a non-road rung lands, and the control would then be
named after one of its steps.

⛔ **IT APPEARS EXACTLY WHERE THE `Road` READOUT ROW APPEARS** — a tile carrying a road — so nothing
grows on a hex with no road. **The GATING is carried by the ROWS INSIDE the ladder**, each disabled
with its own reason, and never by hiding the action: a branch a player cannot climb today is still a
branch they must be able to read and plan against.

**THE CARD IS A `PopupPanel`, the destination track's own idiom.** The selection card is
height-capped and scrolls internally, so a ladder drawn as a block would push the card's own rows out
of view on the frame it opened; a Window changes no layout at all. Its CONTENT is rebuilt per open
and never patched — the rung, the meter, the knowledge and the acting band all move per snapshot —
while the panel node is reused because a Window is expensive.

### The words

`buys`, `lost between bands` / `lost hauling`, `lights its tiles` and `links N tiles out` are retired
from every road surface, row and hover alike. What the branch says now is **`40% less loss`**,
**`you can see along it`** and **`links camps up to 10 tiles apart`** — and a payoff needs no label,
because a benefit already reads as one.

### Remoteness is STATED, never multiplied

Every row quotes the rung's **BASE** `workCost` as published. Where the road's own
`keeper_remoteness` is above `ROAD_REMOTENESS_AT_HOME` the multiple rides the row's HOVER —
`Far from your band, so it costs ×2.0.` **The client multiplies nothing**: folding the two would put
a copy of the sim's pricing formula where it can drift.

The tile card's `Kept by` row keeps its own wording (`far from them — ×2.0 the rung's price`), and
the two are not a drift: that row states what the EXISTING keeper is being charged, this one states
what the prospective builder would be. Different subject, different sentence.

### The command has not moved

`HudLayer.improvement_requested` → `Main.format_improvement`, which already carries the route verbs'
extra band token (`Main.IMPROVEMENT_BAND_TARGETED`). No new command, no new token, no change to the
formatter. The relay is `DrawerComposeController.road_improvement_requested` → `HudLayer`, and it is
deliberately **not** named `improvement_requested` on that controller: that name was retired there
when the rung checkbox stopped being the commit, and reusing it would read as the pair coming back.

⛔ **THE RELAY CARRIES NO OPTIMISTIC OVERLAY WRITE.** `_on_work_row_improvement_requested` records a
pending LABOR ROW before it relays, which is right for a `⌃` on the work board; a road has no work
row, so there is nothing to record and nothing a failed send would have to roll back. The payload
therefore carries no `pending_entity` — the same shape as the `extend_pen` relay beside it.

### Tests

`ui_preview`'s `land_readouts` chapter carries **five frames** and the claims a picture cannot make,
driven through the REAL action and the REAL formatter. Read as a set they are the argument the
feature exists to make: `road_ladder_gated` (a path, with every rung above it refused and each for
its own reason — nothing on it pressable), `road_ladder_grade` (the same branch one rung up with
Roadbuilding learned: `grade` OPEN, priced and pressable), `road_ladder_pave` (the top rung on a
REMOTE dirt road, quoting the base price with the multiple as its own clause),
`road_ladder_other_keeper` (a dirt road **a second band keeps**, with the actor selected — the top
rung refused and the keeper NAMED) and `road_ladder_no_keeper` (the same road with no band selected
— the top rung refused, and refused for that reason ALONE).

**`road_ladder_other_keeper` STAGES A SECOND BAND, and that is what the claim needs.** With only the
actor on the roster every keeper is either itself or a stranger, and *another people* — a true
sentence — would prove nothing about the label plumbing. Staged second, the keeper resolves to
`Band 2` through the roster index, so the frame tests the gate and the naming rule together.

**Half the claims are ABSENCES**, which is what a rendered frame cannot carry: no action at all on a
hex with no road; not one pressable row on a path's ladder; the price stated ONCE on an open row; no
approach clause where the meter reads exactly `1.0`; **no *pick a band* complaint on the frame where
a band IS selected and another band holds the tile** — the discriminator between the two keeper
gates; and no ground or craft complaint on the no-keeper frame, where both are ready.

**The catalog fixture is a TRANSCRIPTION of the shipped ladder, deliberately not a derivation** — one
that recomputed it would pass against a producer that had stopped producing one — and the expected
sentences are written out for the same reason: an expectation recomposed from the producer's own
format string passes against a producer that has stopped composing it.

**`PAVED_HOLD_ASIDE` reads `then 0.9 work a turn` against a config that says `0.95`**, and
that is the shipped string: the value arrives as an `f32` and `DetailFormat.format_work_units` rounds
it down at one decimal. An expectation "corrected" to the config's digits fails against a client
doing nothing wrong.

> #### ⛔ ONE CLAIM RUNS AGAINST A CATALOG OF ITS OWN, AND IT HAS TO
>
> On the shipped ladder *the rung beneath the gated one* and *the rung that teaches its craft* are the
> SAME rung on both steps, so every frame above passes under either rule — which is how the
> `requiresRung` inference survived a slice. `_twisted_teaching_catalog` moves ONE field and nothing
> else: **the trail earns Paving** while `paved_road` still requires `dirt_road`. The remedy must then
> read *keep a trail*, and the inference reads *keep a dirt road* — which is also what the shipped
> catalog reads, making it the sharpest discriminator available. It is built by MUTATING the shipped
> fixture rather than restating it, so a rung added to one arrives in the other and the two cannot
> drift into testing different ladders. PNG-less: both rules render a perfectly plausible card.

**Falsified, three times.** Disabling the *no band picked* gate (`if false and band.is_empty()`)
makes an unavailable rung render live and fails **exactly two** claims, both on
`road_ladder_no_keeper`. Restoring the `requiresRung` inference in place of `ladder_rung_teaching`
fails **exactly two** others, both on the twisted catalog — and *nothing on the shipped one*, which
is the measurement that says the inference was undetectable without that fixture. Dropping gate 4
(`if false and taken_by_another`) fails **exactly two** on `road_ladder_other_keeper` — the row goes
pressable and the keeper goes unnamed — while the third claim on that frame, the ABSENCE of the
*pick a band* reason, correctly still passes: losing one gate must not make its neighbour fire.
Every other chapter stayed green through all three.

## What is NOT wired, and is not an omission

- **The route ladder is in `SourceForecast.RUNG_KEY_IMPROVEMENTS`, its inverse AND `RUNG_BRANCHES`.**
  It used to be in none of them, correctly: no route rung declared a verb. `grade` and `pave` closed
  that gap, so the branch goes into all three — the tables' own note is *both or neither*, and the
  four rung keys are ALIASED off `HudRouteVocab` rather than spelled twice. **The free floor is two
  rungs deep and both map to `IMPROVEMENT_NONE`**, exactly as the two `wild` rungs do: a path and
  the trail above it are worn in by traffic alone, and neither is something a band builds.
  `ReadyForImprovement` is unaffected — it reads `current_rung` off patches and herds, and no road
  carries one.
- **No kit picker on the Roadwork card, and `KitRoster.KEEPING_JOB_BUILD_BRANCHES` gains no entry.**
  `default_kits.roadwork` is the bare `none` kit, so road keepers work bare today; that is intended,
  and the day a barrow declares a `build_work` stat serving `route` the existing seam picks it up.

## `grade` and `pave` are the only TILE verbs that name a BAND

`Main.format_improvement` gained a third arm, `IMPROVEMENT_BAND_TARGETED` (read off
`SourceForecast.ROUTE_IMPROVEMENTS`, so the branch's verbs are spelled once): `grade <faction> <band>
<x> <y>`, which is `cultivate`/`sow`'s grammar plus a band token. A patch's keeper is whoever is
already foraging it; **a road has no work row at all**, so who will keep the tile has to be said out
loud — and issuing the verb declares the job and names the keeper in the same act.

**An absent band is a REFUSAL, not a default.** `IMPROVEMENT_NO_BAND` builds no line, because a road
with nobody on the hook is not a road the sim will accept and guessing one would commit somebody
else's people to a standing bill.

⛔ **THE EXTRA TOKEN IS WHY `command_guard` DRIVES BOTH VERBS.** Both handles are integers in a
positional grammar, so a builder that emitted the four-token tile form would still PARSE — the sim
would read the band as the x coordinate and grade a tile nobody asked for, with nothing failing
anywhere. The guard asserts the parsed band equals the fixture's `band_id`, whose `entity` is a
deliberately different number.

## Tests

`ui_preview`'s `land_readouts` chapter carries the six frames — one per rung, a road in shortfall, and
a REMOTE one whose keeper is being charged a multiple for the distance — and, beside them, the claims
a picture cannot make, run against the REAL producer
(`SubjectDrawerController._tile_terrain_lines`) rather than a re-derivation. They assert what the rows
SAY and, since the block became conditional, **what the card does NOT draw**: the free floor composes
exactly one line and the other four keys are absent from the producer's output; the rung row reads
`Path · 30% to trail` and **never** the retired `Trail 30%`, which is the wording that read as a
part-built road; a complete rung states the rung bare and the top rung states no approach at all; the
bill reads under `Upkeep`; the shortfall is stated against the bill with the keepers it wants; the
countdown; the gone-dark clause naming *upkeep*; and `0` reading `now` rather than `in 0 turns`. The
fixture's demand and shortfall are deliberately different numbers, so a row that printed the gross
demand as the shortfall fails while one that printed a correct subtraction would not.

**Falsified**: making the `Upkeep` row unconditional again fails exactly two claims, both about the
free trail — the one-row composition and the absent bill — and nothing else in the run.

`map_preview`'s `map_road_network` is the draw's own frame: five RUNS laid parallel, one at each rung
and one in shortfall, plus a **lone tile that must DRAW**. That last one inverted with the model — it
was the degenerate case a one-point polyline had to bail on, and per tile it is the ordinary case, so
a renderer that still thinks in polylines fails visibly on it.

**`command_guard` sweeps the role**, which is what proves `assign_labor … roadwork` survives
`sim_runtime::command_text::parse_command_line` — the parser the native bridge runs BEFORE it sends,
and the one `builders` was missing from for a whole slice while every other gate stayed green.

> ### ⛔ A ROLE ADDED TO THE SWEEP SPENDS THE FIXTURE BAND'S WORKFORCE
>
> `command_guard._drive_assign_labor_kits` puts `PARTY_WORKERS` on EVERY band-wide role, and each is
> recorded as a PENDING assignment the instant it is emitted — so by the end of the sweep the fixture
> band's whole workforce is optimistically spoken for and `effective_idle` is 0. The parties compose
> sheet does not render at a compose pool of 0, so `_drive_send_trade_expedition` found no destination
> row, no cargo rows and no confirm button, and reported four failures about a shipment form that had
> simply never been drawn.
>
> **It bit when `roadwork` landed — the sweep's sixth entry, the one that took the pool past zero —
> but the coupling was always there and the next role would have found it again.** The trade drive now
> calls `reconcile_pending` at a later turn first, which is exactly what a fresh snapshot does to those
> entries, so it starts from the band the wire describes rather than from the previous drive's
> optimism.

## See Also

- `.claude/rules/core_sim/routes.md` — the arc: why a road does not follow a camp, the four rules, the
  span, the keeping, and the wire contract every field here is read under
- `band-city-panel.md` — the WORK zone's height and width budgets the pools row is squeezed into
- `selection-card.md` — the land drawer these rows are appended to
- `map-renderers.md` — `AnnotationRenderer`'s other four const families, and `_unwrapped_path_points`
