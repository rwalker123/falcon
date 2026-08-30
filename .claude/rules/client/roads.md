---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_route_vocab.gd"
  - "clients/godot_thin_client/src/scripts/ui/AnnotationRenderer.gd"
  - "clients/godot_thin_client/native/src/dict/routes.rs"
---

# Roads — the client half of the intensification ladder's third branch

The sim side is `.claude/rules/core_sim/routes.md`, which is authoritative for what every field on
the wire MEANS; this file is what the client does with them. Read that one first — most of the traps
here are its traps, arriving one layer out.

> ## ⛔ THE WIRE ROW IS A **TILE** NOW, AND THE GDScript HALF STILL READS THE PATH
>
> `docs/plan_standing_upkeep.md` §4.13b replaced the stored path object with a **per-tile**
> improvement: a road is one tile, with one **keeper**, its own rung, its own meter and its own decay.
> `RouteState` accordingly carries `tileX` / `tileY` / `hasKeeper` / `keeperBandId` /
> `keeperRemoteness` — and **no `id`, no `pathX`, no `pathY`**. `native/src/dict/routes.rs` publishes
> that shape.
>
> **The GDScript below has not been re-written for it, and nothing errors.**
> `MapView._ingest_road_network` reads the retired halves through `get`-with-default, so it builds an
> empty path for every road: `AnnotationRenderer.draw_road_network` draws nothing, and the tile card's
> road rows go with it because `road_tile_lookup` is keyed off the same zip. A silent blank, not a
> crash — which is why it is written here rather than left to be found.
>
> Three sections of this file describe the retired model and are wrong until that work lands: the
> polyline draw, the per-SEGMENT fog gate (there are no segments — a road is one tile, and the sim's
> gate is now simply *"have you seen that tile"*), and *"a band keeps the roads under its own feet"* —
> **the catchment is the KEEPER**, and a band four tiles from a road it graded goes on paying for it.
> What distance costs is `keeperRemoteness`, priced into the bill and refused nowhere.
>
> **Two things this file says that became TRUE rather than false:** a route rung *does* declare a verb
> now (`grade` / `pave`), so `SourceForecast.RUNG_KEY_IMPROVEMENTS` has a genuine gap; and
> `holds_link_to_tiles` is still authored-not-consumed, so the future tense still stands.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_route_vocab.gd` (`HudRouteVocab`) | The road VOCABULARY leaf — the four rung keys + their labels + `RUNG_ORDER`, the tile card's five row keys and their formats, one reader per wire field, and one composer per row (`road_row_value` / `wearing_in_value` / `keeping_value` / `reverting_value` / `buys_value`, joined by `road_lines`). It also owns the three `*_value_hex` forks `DetailFormat._value_hex` dispatches to, so a road's ink is decided beside the words it tints. A vocab module with static funcs, the `hud_work_vocab.gd` shape; it reads `SourceForecast` / `DetailFormat` / `HudSelectionVocab` / `HudStyle` inside functions only, never in a `const`, so it adds no load cycle |
| `ui/AnnotationRenderer.gd` → the `ROAD_*` family | The map draw: `draw_road_network` walks `MapView.road_network` (world state read through the `_view` back-ref, exactly as `units` / `herds` are) and `_draw_road` stamps one polyline per road through `MapView._unwrapped_path_points`, the connected-path idiom. It is called from `_draw` right after the crisis annotations — above the tile tints, beneath every marker, ring and selection outline, because a road is infrastructure IN the ground rather than something standing on it |
| `native/src/dict/routes.rs` | `routes_to_array` — **one dict per road TILE**, the `connections.rs` shape. The row's identity is `tile_x` / `tile_y`, which replaced the retired `RouteId`; beside them ride `has_keeper` / `keeper_band_id` (read the bool first — `0` is a real `BandId`) and `keeper_remoteness`, the multiple distance put on that road's price. There is **no path on the row** — a link knows its two endpoints, so the tiles between them are computable |

## ⛔ A ROAD IS NOT AN ORDER PATH, AND THE OBVIOUS NAME WAS ALREADY TAKEN

`AnnotationRenderer._routes` — fed by `MapView`'s `snapshot["orders"]`, drawn by `draw_routes`, and
covered by `map_preview`'s `"routes"` state — is the per-faction **ORDER PATH** overlay: the
waypoints a player's own movement orders are following, coloured by FACTION, which vanish when the
order does. A road is a world object with a fixed stamped path, owned by nobody, that outlives every
band that walks it, and it is coloured by RUNG.

**So the client noun for this branch is `road` everywhere** — `MapView.road_network` /
`road_tile_lookup`, `AnnotationRenderer.draw_road_network`, `HudRouteVocab`, `ui_preview`'s
`road_tile_*` frames and `map_preview`'s `map_road_network`. Do not rename either into the other, and
do not "unify" the two draw passes: they read different sections, live in different layers of `_draw`
and answer different questions.

## The rung ladder is ONE INK AT FOUR OPACITIES — a correction made against a rendered frame

The first cut walked the palette's own ink ladder (`LINE_SOFT` → `INK_FAINT` → `INK_DIM` → `INK`),
which is faintest-to-strongest **on the HUD's dark ground and INVERTED on the map's**. Rendered over
tan steppe in `map_road_network`, the game trail drew as a near-black hairline carrying the most
contrast on the frame while the paved road drew as pale grey — the ladder read backwards.

**A rung has to read as PROMINENCE on ground of any tone, and only opacity does that.** So the
prominence ladder is `HudStyle.INK` at `ROAD_OPACITY_GAME_TRAIL` → `_TRAIL` → `_DIRT_ROAD` →
`_PAVED_ROAD` (0.30 / 0.52 / 0.74 / 0.94), derived at draw time — the map's own idiom for a themed
overlay tint, the one `MapView.SUPPLY_LINK_COLOR` already uses. The WIDTH ladder rides beside it
because thickness reads before tint at map zoom.

- **A road in SHORTFALL draws in `HudStyle.DANGER`, at the TOP of the opacity ladder** and at a fixed
  mid-rung width. It is losing a real investment, which is the same news a starving pen's ring
  carries, so it borrows the client's existing at-risk ink rather than inventing one — and an alarm
  that faded with the rung would be quietest on the trail nobody notices going.
- **An unknown rung draws at the floor rather than vanishing.** A road is a real thing whatever the
  ladder calls it.
- **The fog gate is per SEGMENT, and it is `Discovered` rather than `Active`.** The sim publishes a
  road to a faction that has explored at least ONE of its tiles, so a road can reach the client with
  a tail running over ground nobody of yours has ever stood on; drawing that tail would paint
  infrastructure across unexplored fog. `_is_tile_visible` would be the wrong test in the other
  direction — it demands `Active`, and a road does not wander off, so a remembered one is remembered
  truly. A partly-drawn road is the honest picture.

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

**ONE BLOCK PER ROAD** — a hex may carry more than one, each its own investment with its own bill, so
they are never summed into a hex total.

### The five rows, and what each is guarding against

| Row | Says | The trap it avoids |
|---|---|---|
| `Road` | the rung it HOLDS, badged, plus the branch's own hazard word `washing out` when short | the rung STRING is the bool — a reader that thresholded `build_fraction` would call a fully-worn trail a dirt road on the turn its first traffic banked |
| `Wearing in` | the meter on the rung being RAISED, named where `RUNG_ORDER` knows the destination | that meter is a DIFFERENT rung's, so it is a different row. **Rendered only below `1.0`**: the wire states exactly `1.0` for a rung just finished AND for the top of the ladder, so the test is a plain comparison and a `100%` row would be a row with nothing to say |
| `Keeping` | the bill, the SHORTFALL where there is one, and the keepers the bill wants | all three figures are published; `demand − supplied == shortfall` holds verbatim on the wire and the keeper count is the sim's own `ceil`. **The game trail reads `free — nobody keeps a game trail`**, not `0 work a turn`: the floor declares no upkeep at all, and that is the whole of what makes it free |
| `Reverting` | the COUNTDOWN, and `now` at zero | `0` means it is reverting NOW. It renders only while the road is genuinely short, because a road whose bill is met reads its rung's full grace + 1 — *"walk away and you have this long"* — which is not news |
| `Buys` | ⛔ **what the rung is BUYING** | see below |

### ⛔ THE `Buys:` ROW IS THE POINT OF THE WHOLE READOUT

The route ladder is deliberately **not** a straight upgrade path: a road is cheaper to travel and
dearer to keep, and the player is meant to pave only where the traffic pays for the upkeep. Without a
visible statement of what a rung buys, every road reads as pure cost and the decision the branch
exists to create is invisible — §4.9 item 12's *"a tax, not a ladder"* trap, on the client side of
the wire. It is the one row on the card that states a PAYOFF, which is why it is tinted rather than
left in plain ink beside the amber bill above it.

Three clauses, each off a published field:

- **the friction it saves**, as the percentage of the base loss it takes off. `friction_multiplier`
  is the fraction of that loss a bound network pays, so `0.6` renders as *40% less lost between
  bands* — a presentation of the published multiplier, never a re-derivation of a sim answer.
- **whether it is lighting its tiles**, off the RESOLVED `grants_sight`, because a client cannot
  re-derive *"is the bill met"* (that is a comparison against the stamped basis with the sim's own
  epsilon). Its other half is said out loud: a built road whose bill is unpaid reads *dark until its
  keeping is paid*, because a road **goes dark BEFORE it decays** and a clause that merely vanished
  would read as a rung that never lit anything. Gated on the shortfall rather than on "unlit",
  because the GAME TRAIL lights nothing even with its (interpolated) bill paid in full.
- **the link span, in FUTURE TENSE, and that is not a style choice.** `holds_link_to_tiles` is
  authored on every route rung and **not yet read by the sim** — nothing in `balance_supply_networks`
  consumes it; that is slice 13b. Rendering it in the present tense would state an effect that is not
  in play, so it reads *will hold a link N tiles out*.

A rung buying nothing on every axis says so — `nothing — a path the animals made`, in dim ink. Both
of the floor's terms are at their own neutral, and that is a LIVE reading: *"this rung is worth
nothing"* is precisely what the branch's floor means.

## The `roadwork` pool, and what a fourth card cost

`roadwork` is an ordinary band-wide standing role in exactly the grammar `agriculture` and
`husbandry` use — `assign_labor <faction> <band> roadwork <n>`, one more arm on
`Main.format_assign_labor`'s shared role branch, one more card in the Work tab's POOLS block. Its
hint names STANDING ON THE ROAD, because that is literally the catchment: a band keeps the roads
under its own feet and nothing else, there is no radius, and a band that steps one tile off its own
road stops paying for it and stops being served by it.

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
band whose only standing cost is a road it stands on must still be offered the split.

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

## What is NOT wired, and is not an omission

- **No route rung appears in `SourceForecast.RUNG_KEY_IMPROVEMENTS` or its inverse.** That pair maps a
  rung to the VERB that builds it, and no route rung declares one — traffic wears a road in, so the
  branch appends no build-queue entry and draws nothing from the builders pool. The tables' own note
  says a route rung goes into both or into neither; neither is the answer.
- **No kit picker on the Roadwork card, and `KitRoster.KEEPING_JOB_BUILD_BRANCHES` gains no entry.**
  `default_kits.roadwork` is the bare `none` kit, so road keepers work bare today; that is intended,
  and the day a barrow declares a `build_work` stat serving `route` the existing seam picks it up.

## Tests

`ui_preview`'s `land_readouts` chapter carries the five frames — one per rung plus a road in
shortfall — and, beside them, the fifteen claims a picture cannot make, run against the REAL producer
(`SubjectDrawerController._tile_terrain_lines`) rather than a re-derivation. They assert what the rows
SAY: the floor's free bill and its `nothing` payoff, the meter naming the rung ABOVE the one held, a
complete rung printing no meter row at all, the shortfall stated against the bill with the keepers it
wants, the countdown, the gone-dark clause, and `0` reading `now` rather than `in 0 turns`. The
fixture's demand and shortfall are deliberately different numbers, so a row that printed the gross
demand as the shortfall fails while one that printed a correct subtraction would not.

`map_preview`'s `map_road_network` is the draw's own frame: five roads laid parallel, one at each
rung and one in shortfall, plus a one-tile road the draw must bail on.

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
