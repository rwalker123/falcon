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
| `ui/hud/hud_route_vocab.gd` (`HudRouteVocab`) | The road VOCABULARY leaf — the four rung keys + their labels + `RUNG_ORDER`, the tile card's **four** row keys and their formats, one reader per wire field, and one composer per row (`road_row_value` and its `progress_clause` / `bonus_value` / **`upkeep_value`** / `reverting_value`, joined by `road_lines`, with `upkeep_tooltip` for the figures that left the row). It also owns the four `*_value_hex` forks `DetailFormat._value_hex` dispatches to, so a road's ink is decided beside the words it tints. A vocab module with static funcs, the `hud_work_vocab.gd` shape; it reads `SourceForecast` / `DetailFormat` / `HudSelectionVocab` / `HudConst` / `HudStyle` inside functions only, never in a `const`, so it adds no load cycle — **and that contract is what lets `SourceForecast` alias its four `RUNG_KEY_*` at `const` level** rather than spelling the wire's route rungs twice |
| `ui/AnnotationRenderer.gd` → the `ROAD_*` family | The map draw: `draw_road_network` walks `MapView.road_network` (world state read through the `_view` back-ref, exactly as `units` / `herds` are) and `_draw_road` stamps **one HEX per road** — `MapView._hex_center_wrapped` for the placement, `_outline_hex_at` at `ROAD_TILE_RADIUS_FACTOR` of the tile radius for the ring. It is called from `_draw` right after the crisis annotations — above the tile tints, beneath every marker, ring and selection outline, because a road is infrastructure IN the ground rather than something standing on it |
| `ui/hud/RungLadder.gd` → `route_track` / `build_track`'s `title` | **THE ROUTE BRANCH'S ROW PRODUCER, a SIBLING of `track` and never a widening of it** — `track` takes a labor `kind` and a wire source dict, and a road has neither. It emits `track`'s own `ROW_*` shape so the RENDERER is shared (`build_track` gained one optional heading argument and nothing else), walks `HudRouteVocab.route_ladder`'s ordered catalog, and owns the branch's seventh state, `STATE_UNORDERED` — the rung nobody declares. Its two private leaves are `_route_progress_aside` (the meter, on the row DIRECTLY above the standing rung and no other) and `_route_hold_asides` (the standing bill, in the plant/animal branches' own sentence) |
| `ui/hud/RungGates.gd` → `route_gates` / `route_knowledge_reason` | **THE ROUTE ARM of the shared gate layer**, keyed **RUNG KEY** rather than verb (two route rungs declare none, so a verb-keyed table cannot tell `path` from `trail`). Four gates in reading order — the ground, the un-orderable rung, the craft, the keeper — each carrying its own remedy. The craft's NAME is threaded in as a `{knowledge_id: display_name}` parameter off the ladder's knowledge roster, never a table here; its REMEDY is the rung whose `earns_knowledge` names that craft, **looked up through `HudRouteVocab.ladder_rung_teaching` and never inferred from `requires_rung`** |
| `ui/hud/DrawerComposeController.gd` → the `build_road_drawer_actions` family | The tile card's `Road ▸` action and the `PopupPanel` it opens (`_open_road_ladder` / `_emit_road_improvement` / `_ensure_road_ladder` / `_road_ladder_anchor_rect` / `_dismiss_road_ladder`), filling `%RoadLadderControls` — its own container at the BOTTOM of the card, since `%ForageAssignControls` is gated on a gathering site with a band in hand. It emits `road_improvement_requested`, which `HudLayer` relays straight onto `improvement_requested`, and `road_abandon_requested`, which it relays onto `abandon_requested` — both with **no** optimistic overlay write. `_fill_road_ladder` is the card's re-render seam (the band picker calls it) and `_default_road_band` decides which band it opens on |
| `native/src/dict/routes.rs` | `routes_to_array` — **one dict per road TILE**, the `connections.rs` shape. The row's identity is `tile_x` / `tile_y`, which replaced the retired `RouteId`; beside them ride `has_keeper` / `keeper_band_id` (read the bool first — `0` is a real `BandId`) and `keeper_remoteness`, the multiple distance put on that road's price. There is **no path on the row** — a link knows its two endpoints, so the tiles between them are computable. **`route_rungs_to_array` is the file's second producer and answers a different question** — one row per RUNG of the branch, published once per world beside `ladderKnowledge`, carrying no faction and no tile. One field on it is not the rung's: `build_work_per_worker_turn` is the SIM's bare worker output, the same on every row, riding the catalog because the catalog is the set of numbers identical for every road in the world |

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
| `Road` | a road exists on the tile — **the only unconditional row** | the rung it HOLDS, plus `N% to <next rung>` while something is rising above it **or while an entry is declared on it**, plus the branch's own hazard word `washing out` when short | ⛔ **THE PERCENTAGE MUST NOT READ AS "THIS ROAD IS 25% BUILT".** A path at 25% is a COMPLETE path a quarter of the way to becoming a trail, so the rung stands alone as the value and the meter is a qualifier naming where it is GOING. The retired `Wearing in: Trail 25%` row said the opposite, and `build_fraction` is a DIFFERENT rung's meter — a reader that thresholded it would call a fully-worn trail a dirt road on the turn its first traffic banked. **Rendered only below `1.0`**: the wire states exactly `1.0` for a rung just finished AND for the top of the ladder, so the test is a plain comparison |
| *(the bonus)* | the rung saves something on the loss axis | ⛔ **what the rung is BUYING, in ONE clause** — the loss it saves | see below. **UNLABELLED** — `Buys:` was a key doing no work, the value already reading as a benefit. The sight and the span moved to the block's HOVER (`bonus_tooltip`): the row printed all three and wrapped to three lines under a one-word value |
| `Upkeep` | the road actually owes something — **neither free rung does** | ⛔ **WHOSE JOB IT IS, and whether they are covering it** — `Band 3`, `Band 3 (short 1 worker)`, `another people`, or `nobody — grade it again to take it on` | the bill and the keeper count are on the block's HOVER. **They were the row, and together they were gibberish** — see below. **The word is `Upkeep`** — see below too |
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

### ⛔ THE BILL IS A NAME, AND THE FIGURES ARE ON THE HOVER

It was two rows and three figures:

```text
Upkeep      0.0 work a turn · wants 1 keepers
Kept by     Band 3
```

**Both numbers are individually correct and together they are nonsense.** A trail holding 2% of the
dirt road banked owes ~`0.009` work a turn — a meter carrying work is billed a proportional share of
the rung it is climbing toward — and `DetailFormat.format_work_units` rounds that **down** to `0.0`
while `road_upkeep_workers_needed`'s `ceil` rounds the same number **up** to `1`. Two roundings in
opposite directions on one quantity, printed side by side. Reported from play: *"We don't need both
upkeep and kept by. We can show upkeep warnings, like `Upkeep: Band 3 (short 1 worker)`."*

**`ROAD_KEEPER_ROW` (`Kept by`) IS RETIRED AS A VISIBLE ROW AND `Upkeep:` ABSORBED IT.** One row:

| road | row |
|---|---|
| owes nothing | **no row at all** (both free rungs) |
| keeper in the player's roster, bill met | `Upkeep: Band 3` |
| keeper in the player's roster, short | `Upkeep: Band 3 (short 1 worker)` |
| keeper outside the roster | `Upkeep: another people` |
| owes a bill, nobody keeps it | `Upkeep: nobody — grade it again to take it on` |

- ⛔ **THE WORK FIGURE AND THE KEEPER COUNT LEFT THE ROW ENTIRELY**, to the block hover through
  `ctx.row_tooltips` — the seam the payoff row already uses (`upkeep_tooltip`). The exact bill stays
  available and stops being the headline, which is the only arrangement in which `0.0 work a turn`
  can never render as one. **Nothing was deleted**, and a cut that lost the figures rather than
  relocating them is this branch's own recurring failure.
- ⛔ **`short N worker(s)` IS IN WORKERS AND IS DERIVED FROM THE SHORTFALL** —
  `SourceForecast.workers_for_work(upkeepShortfall)`, `ceil(work / PER_WORKER_OUTPUT)`, which is the
  arithmetic the sim runs to publish `upkeepWorkersNeeded` from the demand. **Never `wants −
  assigned`**: `roadwork` is a band-wide POOL and its share of any one road is the sim's answer, so
  the client holds no per-road head count to subtract from — the rule this branch has already shipped
  as a defect twice. It **pluralizes**; the shipped row said `wants 1 keepers`.
- **The remoteness clause moved to the hover with them** (`far from them — ×2.0 the rung's price`).
  It is a real fact with no other surface — `keeper_remoteness` prices BOTH the build pile and the
  standing bill, and distance is a cost that refuses nothing — so it is kept rather than cut; but it
  is rare and long, and length was the complaint.
- ⛔ **DISPLAY ONLY.** `owes_keeping`'s floor, `road_upkeep_workers_needed`, the interpolated demand
  and every sim number are untouched. **The billing is correct**; what was wrong was the sentence.
- **The ink follows the words, not a glyph.** The row carries no hazard mark now, so
  `upkeep_value_hex` forks on `ROAD_UPKEEP_SHORT_MARK` — the composer's own spelling of `(short ` —
  and on `ROAD_KEEPER_NOBODY`. One spelling, two readers; two spellings is how a row comes to say
  *short* in plain ink. It still answers the glyph, because the `Reverting` row dispatches here too.

**The keeper is still the band that BUILT the tile, wherever that band has since walked.**
`route_keeping_claims` walks the roads a band keeps and never reads that band's position, so a camp
four tiles away goes on paying and goes on being served. That fact did not move; only the row it is
stated on did. The NAME is still resolved by the drawer and never by the vocab leaf — a road carries
a `band_id`, this client has one band-naming rule (`HudBandLaborState.band_label_for_id` →
`HudFormat.band_display_name`), and `""` reads `another people`, a road being keepable by a people
you merely know of.

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
- **the link span, and it is a LIVE effect as of slice 13b.** `balance_supply_networks` forms a
  pooling link at `distance <= max(reach_tiles, the weakest tile of the run)`, so *links camps up to N
  tiles apart* states something the player can act on: two camps too far apart to share a larder can
  be joined by a road. The line was authored a slice before the sim consumed the field, and its
  wording was chosen to survive that — it says what the rung **does**, never when it starts doing it,
  so the tense did not have to move when the sim caught up. **Keep it tense-neutral**: a rung's payoff
  is published from the config, so a new rung's line has to read correctly with no client edit.

**A rung buying nothing on every axis RENDERS NO ROW**, which is both free rungs. It used to say so in
words — `nothing — a path the animals made`, in dim ink — and that sentence was **factually wrong, not
merely wordy**: it asserted an ORIGIN the sim does not model. A path is a rung a tile HOLDS, and
the commonest way a tile comes to hold one is the player's own bands walking the same ground and
banking traffic into the meter; nothing about it is a path animals made. The row's absence states the
same fact — both of the floor's terms are at their own neutral — and cannot state a false one beside
it.

## ⛔ A QUEUED ROAD WAS INVISIBLE ON EVERY SURFACE, WHICH READS AS A FAILED COMMAND

Reported from play at turn 122. Ray graded a tile and **nothing anywhere showed it had worked**.
Three things were true at once and only the third was wrong:

1. **The `grade` landed** — the keeper was set (which is why `Stop keeping this road` was offered)
   and the entry was on the wire.
2. **It was banking zero, correctly.** A `Tame` sat at the head of that band's queue, and the road
   build arm runs for the **head entry only**, so every builder was on the aurochs.
3. ⛔ **The entry drew nothing.** `BandPanelController._work_source_models` walks the band's LABOR
   ROWS and admits `forage` / `hunt` alone — and **a road is the one build source with no labor row
   at all**, deliberately, since a road is not worked like a patch or a herd and has no take crew to
   staff. So `_build_queue_models`' `not by_key.has(key): continue` dropped it silently.

**A system obeying every rule while the UI shows nothing is indistinguishable from a command that
failed.** Ray went looking for the warning in the `Roadwork` pool — which is the KEEPING pool, where
a build draws `builders`, and the road was on the free floor owing nothing. Both of those readings
were correct; neither was in scope.

### The queue block draws the road, and the ROW IS NOT A SPECIAL CASE OF THE SKIP

⛔ **`_build_queue_models`' rule is unchanged and stays right**: *an entry that does not draw still
spends its rank*, which is what keeps `build_order` indexing the WIRE's list. What was wrong is that
a road **has** a resolvable source; it is simply not a labor row. So the block consults a second map:

- `_road_queue_models(band)` → `{pending_key: model}` over `HudBandLaborState.roads()`, joined on the
  entry's `target_x` / `target_y`, which is the road row's own identity. A road tile the snapshot does
  not carry is still skipped — that is the genuinely-unresolvable case arriving honestly.
- **Every word comes from `HudRouteVocab`**, so the queue row and the tile card cannot say different
  things about one road. The destination is `next_rung_key`'s derivation (the rung above the held one)
  and never a rung read off the entry, **whose `kind` is `roadwork` and names no rung**.
- **The face is the rung and the tile** (`Dirt Road (64, 17)`), from the CATALOG's `display_name` and
  never `RUNG_LABELS`. `Grade (64, 17)` would name one STEP of the branch, and a tile can carry a road
  AND a patch at once — the coordinates alone would draw two rows a player cannot tell apart.
- ⛔ **THE REORDER IS THE FIX.** Ray's road was behind a Tame with no way to say *put the road first*.
  A row that drew and could not be promoted would leave him exactly where he started, so the `▲`
  being ENABLED on a rank-1 road is its own assertion.
- **The model carries ONE LEG, and the leg is not decoration.** `_queue_settings_content` decides
  whether a row is expandable and the `✕` lives in that expansion, so a road with no legs, no crop
  and no kit would draw a row nobody could take back. A road's climb genuinely IS one leg.
  `SourceForecast.BUILD_LEG_NAME_KEY` is why it renders: the food webs derive a leg's word from its
  improvement VERB, and `rung_badge_word` is a hard-coded four that answers `""` for `grade`/`pave`.
- ⛔ **THE `✕` IS `unqueue`, NOT `abandon`.** Withdrawing a declaration and putting a road down are
  different verbs with different consequences — the meter and the keeper survive the first and not the
  second — and the ladder's `Stop keeping this road` already owns the second. `format_unqueue` takes
  the tile form and needed no change.

> #### ⛔ `roadwork` NAMES TWO THINGS ON THE WIRE, AND THE TILE IS WHAT TELLS THEM APART
>
> It is a band-wide standing ROLE (`assign_labor <faction> <band> roadwork <n>`, one slot per band)
> **and** it is the `BuildSource::kind()` of a queued ROAD, one per TILE. Both reach
> `HudBandLaborState.pending_key`, so its road arm is gated on a real tile: an entry keys
> `roadwork:64,17` and the role — asked at `-1, -1` like every band-wide role — keeps the bare kind it
> has always had. **Without the tile two queued roads share one key**, and the block would join both
> entries to whichever road it found first, draw one row for two jobs, and send that row's rank for
> both.

> #### ⛔ THE DATE COLUMN SAYS `Queued`, NOT `⚠ Stalled`
>
> A road publishes no chained `buildTurnsRemaining` — it has no source row for the sim to stamp one on
> — so the client CHOOSES which *there is no number* this is, and the two render very differently.
> `BUILD_TURNS_NO_ESTIMATE` on a ranked entry reads **`⚠ Stalled 0%`**, a claim that something is
> wrong; nothing is, the road being behind a head that takes every builder exactly as designed.
> `BUILD_TURNS_NOT_YET_ESTIMATED` is that state's own reading — **`Queued 0%`**, no hazard mark — and
> its own note says why in the same words: *"a build one command old with a staffed pool on it is not
> a stall"*. Putting `⚠ Stalled` on a correctly-waiting road is the reported defect one column over.
>
> **`keeping_role_name` gained a ROUTE arm for the same class of reason.** The queue row's price
> clause names the pool that pays the standing bill, and `source_kind_for_labor` is a two-way alias
> over the two FOOD WEBS whose `else` is `SOURCE_KIND_HERD` — so a road handed to it came back an
> animal and the clause said `Husbandry`, a card that cannot move a road's bill.

### The ladder says where the press lands, and what the estimate is measured from

Ray: *"it isn't obvious that the road will show up in the build queue, so we need something to
indicate that when the job is selected."* The buildable rung's `ROW_BUILD_ASIDES_KEY` states it:

- **empty queue** — `joins this band's build queue, and starts now`;
- **anything ahead** — `joins this band's build queue behind <head> and N more`.

⛔ **AND IT RIDES A ROW MERELY OFFERED — THE ROW BEING BUILT CARRIES NO PLACEMENT LINE AT ALL.**

The card used to state the placement a second way once the road was already on the list —
`waiting behind (4, 33) — the estimate runs from when it starts` — and reported from play it was
garbage two ways at once. **It explained a number the player was already looking at**: the row one
line up states the progress AND the turns, so a note saying what that estimate runs from qualified a
figure nobody had to hunt for. And on the reported screenshot **the head of the band's queue WAS the
road the card was open on**, so the sentence named the road as the thing it was waiting behind.

The surviving form answers what the row genuinely cannot — *what would this press do* — which is why
it stays while the other went: the deleted one restated the row. `ROAD_LADDER_QUEUE_WAITING_FORMAT`
and `ROAD_LADDER_QUEUE_ESTIMATE_NOTE` are retired, and `ROAD_LADDER_QUEUE_BEHIND_FORMAT` with them —
it shared their `— %s` tail and there was nothing left to fill it. `_route_queue_aside`'s
`with_estimate_note` parameter went too: with one live value it read as a fork somebody still had to
think about, so the ONE caller gates on `is_building` instead. **The `builders <= BUILD_CREW_NONE`
warn aside is untouched and still rides the row being built.**

- ⛔ **THE HEAD IS NAMED BECAUSE IT IS THE WHOLE QUESTION.** The head takes every builder, so it alone
  decides when this road starts; the rest are a count, which is what a reorder is measured against.
  The subject comes from `HudWorkVocab.build_queue_subject` — the queue block's own vocabulary, so the
  two surfaces cannot name one entry two ways — and it is the SUBJECT rather than the row's face,
  `Tame Wild Aurochs` reading mid-sentence as another panel quoted rather than as English.
- ⛔ **THE TURNS ESTIMATE IS KEPT.** `110 work · ≈39 turns` silently assumed the builders were free,
  and **deleting the figure would throw away the one thing the row can say about the price of the
  job** — it is exact once the builders reach the entry. The aside named what it was measured FROM
  for a slice; that clause is retired above, the row being built now stating the progress and the
  turns together where a qualifier only restated them.
- **It re-renders with the `Band:` picker**, a different band being a different line to stand in.
- **An UNKNOWN queue draws no line at all**, never the empty-queue sentence: those are different
  facts and only one of them is reassuring.

### …and the `Road` row stops reading as un-declared

`progress_clause` appended nothing at a zero meter, so a road that was queued but had banked nothing
read as a bare `Trail` — identical to ground nobody has touched. Ray: *"when it is in the queue, it
should show a % complete in the road panel instead of continuing to make it look like it isn't
queued."*

| road | `Road:` row |
|---|---|
| nothing declared, nothing banked | `Trail` |
| **declared, nothing banked yet** | `Trail · 0% to dirt road` |
| work banked | `Trail · 12% to dirt road` |

- ⛔ **ONE SPELLING, AND IT IS `ROAD_PROGRESS_FORMAT`.** The zero reading and the working reading are
  one sentence about one climb; a `queued for dirt road` clause beside it would be a second phrasing
  that then has to be told apart from the first.
- ⛔ **AND A ZERO IS A CLIMB ONLY WHERE ONE HAS BEEN ORDERED.** Ground nobody has ordered anything on
  has no climb to report, and printing `0%` there would invent one — which is why the negative half
  (an un-queued road at zero states its rung BARE) is asserted beside the positive.
- **The destination is the existing derivation** (`RUNG_ORDER` above the held rung), not a rung read
  off the entry.
- ⛔ **THE `Upkeep` ROW IS UNCHANGED AND STAYS ABSENT HERE.** A road on the free floor owes nothing
  and draws no row; **a declaration must not conjure a bill.**

> #### ⛔ A FULL METER ON A QUEUED ROAD MEANS **NOTHING HAS STARTED** — `queued_progress` is the reader
>
> Reported from play: a freshly-graded dirt road drew `Queued 100%` in the build queue while the tile
> card's own climb clause fell silent. Neither surface was lying about the number it was handed.
>
> **`buildFraction` ANSWERS FOR THE RUNG AT RISK, WHICH ON A FRESH DECLARATION IS THE RUNG HELD.**
> `routes::road_build_fraction` measures against `road_at_risk_rung`, which returns `standing.raising`
> only where something is banked in it and otherwise falls back to `standing.held` — so a road at
> trail-top with nothing yet banked into `dirt_road` is measured against the TRAIL, which is complete,
> and the wire honestly publishes `METER_FULL`. The queue row read that raw as *100% of the dirt
> road*; the tile card, which suppresses its clause at a full meter, read it as *nothing is rising*
> and said nothing at all.
>
> **`HudRouteVocab.queued_progress(road)` is the one reader ALL THREE surfaces go through**: a meter
> at or above `ROAD_METER_COMPLETE` on a road whose entry is queued reads `ROAD_METER_UNSTARTED`, so
> the queue row draws `Queued 0%`, the card draws `Trail · 0% to dirt road` and the rung ladder's
> BUILDING row draws `0% · ≈55 turns`. A road with work banked is untouched, which is what keeps the
> readings one sentence rather than a fork.
>
> ⛔ **THE LADDER WAS THE THIRD SURFACE AND IT WENT ON READING THE METER RAW FOR A SLICE**, drawing
> `100%` on a road that had not started. Its row is unconditional — a road on the building branch is
> BY DEFINITION queued — so the call takes no flag. **`_route_climbing` had to move with it**: it
> answered `false` at a full meter, which is right for a road NOBODY has ordered (the top of the
> ladder, and a rung just finished, are both *nothing is rising*) and is exactly the misreading here,
> the full meter belonging to the rung HELD. The queue is tested FIRST and short-circuits, so the
> un-queued arm is untouched.
>
> ⛔ **THE ROW'S TURNS NEEDED NO CHANGE, AND THE REASON IS WORTH KNOWING.** `RungLadder._route_turns`
> already resets a banked fraction at or above `ROAD_METER_COMPLETE` to `NOTHING_BANKED` before it
> divides — its own note says a reader that netted `1.0` off the pile would quote `≈1 turn` for a
> 260-work paving nobody has started — so it draws the same boundary `queued_progress` does and
> quotes the whole price. `≈55 turns` on a freshly-declared 110-work dirt road at two builders is
> that working. **`_route_meter_clause` on the `STATE_UNORDERED` approach row is likewise untouched**:
> that row states a rung being worn in by TRAFFIC, which nobody queues.
>
> ⛔ **AND `progress_clause` TAKES THE QUEUED FLAG RATHER THAN A SECOND SPELLING.** Its full-meter
> suppression is correct for an UN-queued road — the top of the ladder, and a rung just finished, both
> honestly state their rung bare — so the reader is applied before the suppression test and never
> beside it.
>
> #### ⛔ THREE FIXTURES STAGED A METER THE SIM CANNOT PUBLISH, WHICH IS HOW THIS SHIPPED
>
> They set `build_fraction: 0.0` on a road HOLDING a trail — a reading no road can have, since a held
> rung is by definition complete and a road with nothing banked above it is measured against that
> rung. So every frame that would have rendered the defect rendered the fix instead, and every
> assertion on them passed. They state `METER_FULL` now, and `band_panel_preview`'s
> `ROAD_QUEUE_METER` / `ROAD_QUEUE_ZERO_PERCENT` pair is the claim: **the two differing IS the
> assertion**, so a reader that passed the wire's meter through fails naming the played
> `Queued 100%`. Sabotage-verified — exactly one claim fails, and it is that one.
>
> **The general rule this file has now paid for twice**: a fixture staging a value the sim cannot
> produce makes its own frame assert nothing, and it does it silently.

> #### ⛔ ONE PREDICATE, TWO SURFACES, AND THE JOIN STAYS AT THE CALL SITE
>
> `HudRouteVocab.is_queued(road, queued_tiles)` is the only test, so the tile card cannot say a road
> is un-queued while the queue block is drawing a row for it. The SET is derived by
> `HudBandLaborState.road_queue_tiles()` — over **every** player band, because *has anybody of mine
> ordered this* is a faction question and a road queued by Band 2 reading un-queued under Band 1 is
> the same invisibility one band over — and threaded into `road_lines` exactly as `keeper_label` and
> the branch's bare work rate already are. **The leaf holds no roster and no queue, and teaching it to
> walk one would give it both.**

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

### ⛔ THE PLAYER CHOOSES WHO KEEPS IT — a `Band:` picker at the TOP of the card

The acting band was `DrawerComposeController._resolve_assign_band()` alone — **whichever band the
left panel happened to be showing** — and there was no picker at all. A tile graded while reading
Band 3's page became Band 3's job for good, and the player never made that decision. **It is the same
defect `_band_working_source` was written to close for the compose sheet**; roads escaped it because
a road has no work row to infer an owner from, and there is no third option: `grade` / `pave` carry a
band token that IS the keeper, so somebody has to be named out loud.

- **A field row ABOVE `RAISE IT TO…`**, built with `HudWidgets.build_field_key` +
  `build_option_picker` through the compose sheet's own `_build_band_picker` — so `Band:` here and
  `Band:` there line their value controls up at one declared key width and cannot drift
  (`labor-ui.md` → "The compose sheet's FIELD ROWS are one family"). Who keeps it is decided before
  which rung, and every row below reads differently once it moves.
- **The options are the player's bands in ROSTER order**, named through this client's one band-naming
  rule, so a band here and the same band on the dock cannot be called two different things.
- **THE DEFAULT IS THE NEAREST BAND**, wrap-aware (`SourceForecast.hex_distance_wrapped`), and the
  reason is a PRICE rather than a convenience: distance multiplies **both** the build pile and the
  standing bill (`keeper_remoteness`), so the nearest band is also the cheapest keeper this road can
  have — the one a player would have picked anyway. First in roster order wins a tie, so the default
  is deterministic; a band the grid cannot place is skipped rather than counted as distance zero.
- ⛔ **EXCEPT WHERE THE ROAD ALREADY HAS A KEEPER IN THE PLAYER'S ROSTER — THEN IT IS THE KEEPER.**
  Re-issuing on a road you already keep is the ordinary case (trail → dirt → paved), and defaulting
  away from the keeper would open the card on a rung the sim refuses outright (*another band keeps
  it*) — a card greying its own live row on the frame it appears.
- ⛔ **A PICK RE-RENDERS THE ROWS IN PLACE AND MUST NOT CLOSE THE CARD.** `_fill_road_ladder` refills
  the same Window; `popup()` is the OPEN's business alone. Every gate on the track is resolved against
  the acting band (`RungGates.route_gates(…, band, keeper_label)`) and the turns estimate is priced at
  that band's own `builders` pool, so a row left standing after a pick would offer a rung the newly
  chosen band cannot have, at a pace it cannot keep.
- **The `pick a band` gate did not become dead.** It fires where the player holds no bands at all,
  which is also the state in which the picker is not drawn — an empty selector states nothing.

### ⛔ THE ROW SAYS HOW LONG, AND THE CREW IS THE ACTING BAND'S

`Dirt Road — 110 work` at one builder is ~110 turns and the card said nothing about it, while every
other build surface in the game states a turns estimate. `route_track` had carried
`ROW_TURNS_KEY: BUILD_TURNS_NO_ESTIMATE` on every row since the branch shipped; it is filled now.

- **ON A BUILDABLE ROW IT RIDES THE FACE** (`110 work · ≈39 turns`, `HudWorkVocab.RUNG_TRACK_COST_FORMAT`),
  **and on a REFUSED one it rides the HOVER**. The face holds one clause beside the price and on a
  refused rung that clause is the refusal; `110 work · ≈110 turns · needs Roadbuilding` is exactly the
  wordiness this card was cut down from. A rung a player plans toward is still one they can plan
  against, which is what the hover line is for.
- ⛔ **IT IS THE CLOSED FORM'S OWN SUPPLY SEAM, NOT A SECOND ESTIMATOR.**
  `SourceForecast.pool_work_supply` is precisely what `build_turns_at` divides by, so a road and a
  Cultivate are paced by one expression. `build_turns_at` ITSELF cannot be reused: it reads its cost,
  its banked work and its rate off a prefixed SOURCE dict and **a road has no source row** — which is
  also why `buildTurnsRemaining` is a no-op for roads sim-side. `RungLadder._route_turns` is the
  arithmetic, and it is four lines.
- ⛔ **THE BARE RATE IS READ OFF THE WIRE, AND THE CLIENT SPELLS `PER_WORKER_OUTPUT` NOWHERE.**
  `RouteRungState.buildWorkPerWorkerTurn` carries it — the sim's own worker output, unscaled, before
  gear and before any multiplier. `HudRouteVocab.catalog_build_work_per_worker_turn` is the reader for
  a caller holding a rung; `branch_build_work_per_worker_turn` is the reader for one holding only a
  road. See "The rate rides the catalog" below for why a branch-wide number lives on a per-rung table
  and why reading one row is honest.
- **THE PILE IS THE RUNG'S BASE PRICE, matching the figure beside it**, less whatever the meter has
  banked against the row DIRECTLY above the standing rung. ⛔ **`build_fraction == 1.0` means *nothing
  is rising*, not *this rung is paid for*** — the wire states it for a rung just finished AND for the
  top of the ladder — so it is netted off nothing, or the card quotes `≈1 turn` for a 260-work paving
  nobody has started. `_route_meter_clause` draws the same boundary for the same reason.
- **The remoteness multiple is quoted apart from the estimate**, as it is from the price: folding it
  in would put a copy of the sim's pricing formula here.
- ⛔ **ZERO BUILDERS IS AN ANSWER, NOT AN ABSENCE.** With nobody on the pool the row states its price
  bare and the REMEDY rides beneath it as an aside (`ROAD_LADDER_NO_BUILDERS_ASIDE`, `build_aside`,
  in the warn ink) — a blank column where every other row states a duration reads as a client that
  failed to work it out. **And the rung stays orderable**: declaring a road with an empty pool is a
  legal, ordinary act (the entry waits at the head of the queue for hands), so this is a note and
  never a gate.
- ⛔ **NO FALLBACK ANYWHERE, IN EITHER READER.** A missing or zero rate answers
  `BUILD_TURNS_NO_ESTIMATE` on the ladder and `BUILD_CREW_NONE` — no *short N* clause at all — on the
  tile card. **Never `1.0`, and never an infinity.** A substituted constant is the transcription
  coming back through the side door, and the sim writes worker output as a SUM OF TERMS: a copy goes
  stale in silence the day a second term lands, which is precisely what a default would hide. The
  test is BEFORE the division in `_route_turns` rather than after it — a zero rate reaching
  `pool_work_supply` would answer whatever the KIT alone pays, which on a branch no kit serves is
  also `0` and would look like the same refusal for a different reason.
- **The kit is asked for with `KitRoster.BUILD_BRANCH_ROUTE` (`"route"`, `RungBranch::Route`'s own
  wire spelling), which answers `{}` for every shipped kit** — no equipment declares a `build_work`
  effect serving the branch — so a road is priced at bare hands, which is the truth about the shipped
  roster rather than a gap. **Asking with `BUILD_BRANCH_NONE` instead would be wrong**: that means *no
  branch test at all* and would credit the crook's 0.5 against a road.

### The rate rides the CATALOG, and one row answers for the branch

`buildWorkPerWorkerTurn` is the one field on `RouteRungState` that is **not derived from the rung** —
it is `intensification::PER_WORKER_OUTPUT`, identical on every row. It rides the catalog because the
catalog is exactly the set of numbers that are the same for every road in the world; on `RouteState`
it would repeat itself once per road tile on the map, which is the same argument that put `workCost`
and `frictionMultiplier` there.

**So `branch_build_work_per_worker_turn` reads the FIRST row and that is honest rather than sloppy.**
`core_sim/tests/route_wire.rs` asserts every rung publishes the same value, so a client that walked
all four looking for agreement would be re-running the sim's own test — and would still have to pick
one when they disagreed.

⛔ **THE ROAD→CATALOG JOIN STAYS AT THE CALL SITE.** `hud_route_vocab.gd` is a vocabulary leaf that
holds no catalog, and that is load-bearing: it is what lets `SourceForecast` alias its four
`RUNG_KEY_*` at `const` level without a load cycle. So the rate is **threaded in as a parameter** to
`road_lines` / `upkeep_value` / `workers_short_of`, exactly as `keeper_label` already is and for the
identical stated reason — `SubjectDrawerController` resolves it (it holds `_topbar` for this one
field) and hands it over. **Do not teach the leaf to join a road to the catalog on `rung_key`.**

`RungLadder` needs no threading: `_route_turns` is already handed the catalog ENTRY it is pricing, so
it reads `catalog_build_work_per_worker_turn` directly.

### ⛔ AND A ROAD CAN BE PUT DOWN AGAIN — the abandon row at the BOTTOM

The picker above makes the keeper a choice; this is what makes it a reversible one. `unqueue`
withdraws a DECLARATION and is wired up, but **once any work is banked the verb that releases a
keeper is `abandon`**, which was command-line only — so a road handed to the wrong band could not be
taken back from the UI at all.

- **Offered ONLY where the keeper is in the player's roster.** A road nobody keeps has nothing to put
  down and a road another people keeps is not yours to drop; in both cases the control would emit a
  command the sim refuses, which is the shape the ladder's own gated rows exist to avoid.
- **It emits `abandon <faction> <x> <y>`** through `road_abandon_requested` → `HudLayer.abandon_requested`
  → `Main.format_abandon`, written beside `format_unqueue`, which is its sibling and the shape it was
  copied from. **The press closes the card before it emits**, the rung presses' own rule.
- **It carries NO band token**, unlike `grade` / `pave` — see below.

> #### ⛔ IT NAMES A PLACE, NOT A ROAD — AND THE ROW HAS TO SAY SO
>
> `handle_abandon` drops **the faction's labor rows on that tile** as well as the road's keeper and
> its queue entry. The sim's own comment says why: the verb names a *place*, a tile may carry a road
> as well as a patch, and dropping one without the other would be silently partial on exactly the
> tiles where a band both farms and keeps a road.
>
> **So where the tile also carries work of this faction's, the row carries a second line naming what
> else goes down with it** (`ROAD_LADDER_ABANDON_ALSO`, over `forage_assignment_of` across the
> roster — a tile test, because `abandon` is a tile command and a hunt names a herd). On bare ground
> it is a plain button.
>
> ⛔ **DO NOT ADD A ROAD-ONLY ABANDON.** The sim has no such verb, and a client emitting a command
> narrower than the sim implements would be lying about what the button does.

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
> is threaded in exactly as the tile card's `Upkeep:` row threads it, and `""` reads
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

The tile card's `Upkeep` HOVER keeps its own wording (`far from them — ×2.0 the rung's price`), and
the two are not a drift: that one states what the EXISTING keeper is being charged, this one states
what the prospective builder would be. Different subject, different sentence — and both are hovers
now, the row having no room for either.

### The declaration's command has not moved, and the RELEASE gained a builder

`HudLayer.improvement_requested` → `Main.format_improvement`, which already carries the route verbs'
extra band token (`Main.IMPROVEMENT_BAND_TARGETED`). No new command, no new token, no change to the
formatter. The relay is `DrawerComposeController.road_improvement_requested` → `HudLayer`, and it is
deliberately **not** named `improvement_requested` on that controller: that name was retired there
when the rung checkbox stopped being the commit, and reusing it would read as the pair coming back.

**`abandon` IS THE SECOND EDGE, AND IT IS A NEW BUILDER RATHER THAN A NEW COMMAND**: the verb has
always existed and had no surface, so `Main.format_abandon` was written beside `format_unqueue` —
its sibling in shape and its opposite in scope. `DrawerComposeController.road_abandon_requested` →
`HudLayer.abandon_requested` → that builder; the relay is deliberately not folded into
`unqueue_requested`, the two commands doing different things to different state.

⛔ **THE RELAY CARRIES NO OPTIMISTIC OVERLAY WRITE.** `_on_work_row_improvement_requested` records a
pending LABOR ROW before it relays, which is right for a `⌃` on the work board; a road has no work
row, so there is nothing to record and nothing a failed send would have to roll back. The payload
therefore carries no `pending_entity` — the same shape as the `extend_pen` relay beside it.

### Tests

`ui_preview`'s `land_readouts` chapter carries **six frames** and the claims a picture cannot make,
driven through the REAL action and the REAL formatter. Read as a set they are the argument the
feature exists to make: `road_ladder_gated` (a path, with every rung above it refused and each for
its own reason — nothing on it pressable), `road_ladder_grade` (the same branch one rung up with
Roadbuilding learned: `grade` OPEN, priced, DATED and pressable, under a `Band:` picker),
`road_ladder_no_builders` (that same live row with nobody on the pool — the price bare and the remedy
beneath it), `road_ladder_pave` (the top rung on a REMOTE dirt road, quoting the base price with the
multiple as its own clause, and offering the abandon row because the road is already this band's),
`road_ladder_other_keeper` (a dirt road **a second band keeps** — the card OPENS on that keeper with
the rung live, then the picker is driven to the actor and the same rung goes refused and NAMES the
keeper, in place) and `road_ladder_no_keeper` (the same road with no band on the roster at all — the
top rung refused, and refused for that reason ALONE).

**`road_ladder_other_keeper` IS NOW THE PICKER'S OWN CLAIM AS WELL AS THE GATE'S**, and it had to
become one: with the keeper as the default, the *another band keeps it* refusal is unreachable until
the player moves the picker. Driving it (`item_selected.emit`, which is what a click does) asserts the
default, the re-render, the card surviving it, and the gate — in that order, on one frame.

**The turns figures are written out and each one is a claim about a different term.** `≈39 turns` is
`ceil(110 × 0.7 / 2)` — a row quoting the whole price would say `55`; `≈130 turns` is `ceil(260 / 2)`
over a meter of exactly `1.0` — a row that netted that `1.0` off would say `≈1 turn`.

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
else's people to a standing bill. **Which band the token names is the PLAYER'S choice** — see the
`Band:` picker above.

⛔ **AND `abandon` NAMES NO BAND, WHICH IS THE OTHER HALF OF THE SAME CONTRAST.** `Main.format_abandon`
emits `abandon <faction> <x> <y>`: the verb names a *place* and drops every band of the faction's
holding on it. A builder that helpfully added a band token there would be inventing grammar, and one
that dropped `grade`'s would grade the wrong hex — both handles are integers in a positional grammar,
so either mistake still PARSES. `ui_preview` asserts both whole lines for exactly that reason.

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
exactly one line and the other three keys are absent from the producer's output; the rung row reads
`Path · 30% to trail` and **never** the retired `Trail 30%`, which is the wording that read as a
part-built road; a complete rung states the rung bare and the top rung states no approach at all; the
countdown; the gone-dark clause naming *upkeep*; and `0` reading `now` rather than `in 0 turns`.

**AND THE `Upkeep` ROW IS ASSERTED IN FIVE READINGS PLUS TWO ABSENCES**, because the shipped row
passed every claim the chapter then held while saying `0.0 work a turn · wants 1 keepers` — nothing
asserted what it said. The name alone; the name with `(short 2 workers)`; **one** worker short reading
`worker`, which the shipped row could not do; `another people`; `nobody — grade it again to take it
on`. The absences are **`Kept by` on no road at all** and the retired figures — neither `work a turn`
nor `wants` may appear on the row again, whatever the numbers behind it are. Those two are spelled as
LITERALS on purpose: the consts they came from are gone, and an assertion naming a deleted symbol
does not fail, it fails to *compile*, and the chapter goes silent rather than red.

⛔ **ONE FIXTURE IS RAY'S OWN ROW** — `ROAD_DEMAND_HAIRLINE` = `0.009` work a turn wanting one keeper,
above `UPKEEP_WORK_MIN` and so genuinely rendered. It is the exact pair that read as gibberish (a
floor to `0.0` beside a `ceil` to `1`), and it asserts both halves of the fix at once: the row says
`Band 2`, and the hover still carries `0.0 work a turn · 1 keeper` exactly as the sim published it.

The shortfall fixture's demand and shortfall are deliberately different numbers, so a row that
converted the DEMAND to workers (which would say `4`) fails while one converting the shortfall passes.

**Falsified**: making the `Upkeep` row unconditional again fails exactly two claims, both about the
free trail — the one-row composition and the absent bill — and nothing else in the run.

**THE QUEUED READING IS A PAIR, AND THE SEEDING IS PART OF THE TEST.**
`_assert_a_declared_road_says_it_is_climbing` runs one fixture twice: with no entry on the wire the
row states its rung BARE, and with a `roadwork` entry staged on the acting band's `build_queue` the
same road reads `Trail · 0% to dirt road`. **A state that needs a queue must SEED one** — a frame
named for the queued reading that renders the un-queued one is this arc's own recurring trap, and it
has bitten twice. Beside them: declaring conjures no `Upkeep` row, and a road already carrying work
still states its real percentage, which is what says the fix OPENED the zero case rather than
replacing the reading above it.

`road_ladder_queued` is the ladder's half — the same live `grade` row on a band holding two jobs,
asserting the sentence, the SURVIVING estimate beside it, and the ABSENCE of the empty-queue reading
(without which a producer printing both sentences passes).

**Falsified**: emptying `HudBandLaborState.road_queue_tiles()` fails **exactly one** claim, the
queued row's, and nothing else in the run.

⛔ **AND `road_ladder_declared` ONE STATE BELOW IT IS THAT SENTENCE'S ABSENCE — THE PAIR IS THE
CLAIM.** The same road with the `grade` standing on THIS tile draws **no placement aside of either
form**, and the negative is named per surviving form rather than by a shared needle, so a producer
resurrecting either is caught. It cannot stand alone: a producer that had dropped every aside passes
a lone negative, and the state above it drawing its `joins …` sentence is what stops that. Its other
two claims are Change 2's — `0% · ≈55 turns`, and the price GONE from that row — over the untouched
rung above still quoting `260 work`.

**Falsified**: passing the raw `build_fraction` back into the building row's figure fails **exactly
one** claim, the progress one, naming the `100%` the wire honestly publishes for a completed trail.
The price-absence claim beside it correctly still passes — a raw meter renders a METER face, not a
price — which is why the progress claim is an EQUALITY against the whole face and not a `contains`.

`band_panel_preview`'s `band_panel_queue_road` is the BUILD QUEUE block's own frame — a road row
beside a herd row, which is the picture the reported defect made unobtainable — and it carries eight
claims: the paired negative (the same road, no entry, no row), the row drawing at all, its face, its
wire rank, the head marker being on the Tame in front of it, its `▲` being ENABLED, the `✕` in its
settings strip, and the `unqueue 0 64 17` that `✕` transmits through the REAL formatter. **Falsified**:
breaking `_road_at`'s tile join fails **exactly one** claim — the row count — and nothing else.

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
