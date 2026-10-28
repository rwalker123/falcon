---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_orders_vocab.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandOrdersController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandOrdersView.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/BandOrdersSheets.gd"
  - "clients/godot_thin_client/tools/band_panel_orders.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/band_orders.gd"
---

# Band orders -- the chief on the Work tab and the sheets (#860)

The sim half is `.claude/rules/core_sim/work-party.md` (the band-order sections and wire paragraph);
the design is `docs/plan_labor_at_scale.md` -> "Band orders -- the chief", with the visual reference
in `docs/prototypes/band_orders/`. This file is the client's: what is drawn from the wire, where, and
what each button sends.

## The wire the client reads

- `band_orders` on a band dict (a band with a leader only): one dict per order -- `kind` (`food`,
  `fodder` or a material id), `target`, `priority` (lowercase word), `chief_owned`, `draw`, `on_hand`,
  `has_site` / `site_x` / `site_y`, `sustainable`, `has_consumers`, `hold_heads` / `hold_wanted`,
  `refill_heads` / `refill_wanted`, `take`. For food and fodder `target` and `on_hand` are **turns** and
  `draw` / `take` are units a turn; for a material all of them are units.
- `order_kind` and `chief_owned` on every labor row: the order the row is listed under (`""` for a
  player-crewed row). `HudOrdersVocab` is the only reader of both.
- The command is `band_order <faction> <band> <kind> set <target> <priority> | release [row] |
  claim [row]`, rendered by `Main.format_band_order` from the `band_order_requested` payload.
  `BandOrdersController` is the only emitter; `HudLayer` relays it. There is no optimistic write: the
  server restaffs at once and the next snapshot carries the order and its rows.

## The Work tab

With a leader and at least one shown order, `BandPanelController._orders_items` puts the group at the
top of the sections list and partitions the rows through `BandOrdersController.side_of`:

- **The group** (`BandOrdersView.build_group`): one line per order in wire order (fodder skipped while
  `has_consumers` is false) -- icon, name (opens the order's sheet), `· 12 turns · keep 20 turns` or
  `· 12.4 on hand · keep 30`, the state, the order's Priority chip, the crew number over a three-way bar
  (keeps up / refills / short, tooltip carries the numbers). Under each line sit **today's rows
  unchanged** (`_build_work_row` / `_build_extract_row`, the same `WorkStrip`, the same `⌃`), then the
  `↳ Also` lines. A summary line, a warning line in the tone of the worst shortage, and a legend close it.
- **The state is the first match of `HudOrdersVocab.state_of`**: hold short -> red Falling; a material
  whose `sustainable < draw` -> amber Falling (the stands regrow too little); refill wanted with no
  heads -> Holding; some heads -> Refilling slowly; nothing wanted -> At target; else green Refilling
  with its ETA (`eta_turns`: `(target - on_hand) * draw / (take - draw)` for turns kinds,
  `(target - on_hand) / (take - draw)` for a material, rounded up, omitted when `take <= draw`).
- **A mixed patch is listed once**, under the row's `order_kind`; its row names every store it fills
  with its own order's first and `/turn` once at the end (`SourceForecast.yield_components_labelled`:
  `+2.40 food · +0.25 fodder · +0.05 fibre /turn`; a row that fills one store keeps today's text), and every other shown
  order it brought product to gets one dashed `↳ Also` line.
- **`Also coming in`** (hide, bone, fibre -- on hand or gained this turn) and **`Yours`** (the player's
  forage / hunt rows while a food order exists, and extract rows of a material that has an order) are
  ordinary collapsible sections (`WORK_SECTION_BY_PRODUCTS`, `WORK_SECTION_YOURS`). Every other row stays
  in today's sections. A leaderless band skips all of it and draws today's tab.

## The sheets

- **Stone and wood** (`DrawerComposeController._build_deposit_assign_controls`): when the band has an
  order for the material, `BandOrdersSheets.build_material_chief` replaces the Assign sheet if the order
  is the chief's (eyebrow = the material name, `Chief's order` box with the portrait, the Keep-on-hand
  stepper, Priority, the Diggers / Foresters box, Next turn, `Crew it yourself`); if the order is the
  player's, today's sheet wears a `could keep it for you` strip with `Hand it to`. The Band picker swaps
  the versions because both render in the same rebuild.
- **Food and Fodder** (`open_order_sheet`, `ComposeState.KIND_ORDER`): the stepper is in turns; the
  sources list is every chief row of the order (Best / Learning, crew, take) and the player's rows that
  bring the order's product, marked `yours`. The fodder sheet's pen list reads each enclosed herd the
  band owns: fodder a turn is `fodder_draw + pen_fodder_shortfall`, and the pasture share of the split bar
  is `pen_pasture_fraction`.
- **Opening a material's order from the Work tab** goes through `BandPanelController.open_site_sheet`,
  which selects the order's site tile before the sheet opens, so the title and stock line are the site's.
- **Hunt and gather sheets**: `_build_herd_assign_controls` / `_build_forage_assign_controls` are thin
  wrappers over the unchanged `_core` builders; after the core they mount a strip under the Band picker.
  A chief's row gets `<Leader> hunts this for the food order.` with `Crew it yourself` (release with the
  row's address) and every control under the strip is disabled except the work strip, `Show on map` and
  `Done`; `Stop working here` is hidden on a chief's row (the sim refuses a hand command on it); a player's row under a chief-owned order gets the dashed offer with `Hand it to` (claim).
- **Council wording**: `HudOrdersVocab.who_verb` conjugates for an elders' council (`The elders keep`).

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_orders_vocab.gd` | `HudOrdersVocab`: every word of the arc, the wire readers, `state_of`, `eta_turns`, `summary_of`, `by_product_rows`. |
| `ui/hud/BandOrdersController.gd` | The sends, the row lookups (`side_of`, `order_for_source`) and the order-sheet request. |
| `ui/hud/BandOrdersView.gd` | The Work tab group, its line, crew bar, `Also` line and legend. |
| `ui/hud/BandOrdersSheets.gd` | The chief's sheet sections: chief box, Keep stepper, material / Food / Fodder bodies, the strips. |
| `tools/band_panel_orders.gd` | `band_panel_preview`'s Work tab states for the group. |
| `tools/ui_preview/chapters/band_orders.gd` | The sheet states, each button pressed and its `band_order` line compared. |

## What is approximated

- A site is named by the map's terrain label through `Main` -> `Hud.set_terrain_label_lookup` ->
  `BandOrdersController.site_name` (`Karst Highland (21, 14)`); the order itself carries no name, and the
  bare coordinates stand in until the lookup is injected.
- The fodder sheet's per-pen fodder is `fodder_draw + pen_fodder_shortfall`; the sim publishes no single
  per-pen demand figure.
- A player row's offer picks the fodder order when the row's `fodder_yield` exceeds its food yield, else
  the food order.
- On screen the word is **fodder** everywhere; `Hay Meadow` and `hay grass` stay as plant and place names.
