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

- **The group** (`BandOrdersView.build_group`): one line per order in a FIXED order -- Food, Fodder, then
  the materials in wire order (`HudOrdersVocab.shown_orders` ranks them, so the sheets agree; fodder is
  skipped while `has_consumers` is false) -- each with a ▸/▾ caret, icon, name (opens the order's
  sheet), `· 12 turns · keep 20 turns` or `· 12.4 on hand · keep 30`, the state, the order's Priority
  chip, the crew number over a three-way bar (keeps up / refills / short, tooltip carries the numbers).
  **Every order is FOLDED by default**: collapsed shows that line alone. The open state is
  `BandOrdersController.is_open` / `toggle_fold`, keyed `band_id|kind|part` for the session only (never
  saved); a toggle emits `view_changed`, which `BandPanelController` answers with `rerender`.
  **Open**, the order shows today's rows (`_build_work_row` / `_build_extract_row`, the same `WorkStrip`,
  the same `⌃`) then the `↳ Also` lines. A chief's row drops its own `Priority` chip there (the order's
  Priority governs; the row is shorter by `WORK_ROW_PILL_LINE_COST`); a player's row keeps its chip.
  Chief rows with **0 workers** fold into one dim `+N sites in reserve` line after the staffed rows --
  itself a caret toggle (`PART_RESERVE`, remembered the same way) that brings those rows back in full,
  since they can carry a `⌃`. A summary line, a warning line in the tone of the worst shortage, and a
  legend close the group and stay visible folded.
- **A released order** (`chief_owned` false -- "Crew it yourself" on the whole kind) keeps its line in its
  usual place but is no longer drawn as the chief's: icon, name and `· 8.0 on hand` (`· 12 turns`;
  `HudOrdersVocab.released_line_text`, no `keep N`), then a dim `INK_DIM` state `You crew it — Hand it to Orrin`
  (`CREWED_BY_YOU` + `HAND_IT_FORMAT`, `who_lower` for a council) whose link sends the whole-kind
  `claim` (`BandOrdersController.send_claim(band, kind)`; meta `ORDER_HAND_BACK_META`). No caret / fold,
  Priority chip, crew number or crew bar (its rows sit under `Yours`); the name and `›` still open the sheet.
  `state_of` is never consulted for it, no kit clause, and `HudOrdersVocab.chief_orders` filters it out of
  `summary_of` (the `take N of M workers` line and the warning names) and out of the `↳ Also` target kinds.
  A chief-owned order with one released ROW is unchanged (row-level release).
- **Four doors to the order's sheet, one call** (`controller.open_sheet(band, kind)`): the name (an
  inline link: hover lifts the ink to `SIGNAL` and boxes it), the `· … keep N` detail (same link
  treatment, `ORDER_DETAIL_META`), the `Priority: X` chip (a `Button`, `priority_button`, whose border
  lights on hover; it keeps `ORDER_PRIORITY_META`) and a trailing `›` after the crew column
  (`ORDER_OPEN_META`). All four carry the tooltip `Set Orrin's food order` (`HudOrdersVocab.open_tip`;
  council: `Set the elders' food order`).
- **The kit clause.** Folding hid the rows' own kit warnings, so the order line's state grows an amber
  `· short 3 hoes, 2 spears` (`HudOrdersVocab.kit_clause`) when its chief rows are short. Per row,
  `row_kit_short` reads the row's `kit_toe` (`required - filled`, rounded up, short by
  `pool_toe_row_is_short`) under the roster display name lower-cased (`_item_display_names`, never an
  id); the order sums them, sorted by count descending. `BandPanelController._note_kit_short` collects
  the rows (name = the row's own Work-tab title) and `build_group` takes them as `kit_by_kind`. The
  clause is a link (`ORDER_KIT_META`) in an `HFlowContainer` beside the state, so it wraps under it on
  a narrow dock; the tooltip is one `Tended Patch (29, 24) needs 3 hoes` line per row. Visible folded
  and open; absent when nothing is short. Pressing it emits `BandOrdersController.crafting_requested`,
  which `Hud` answers with `CraftingPanelController.open_for(band)` -- the bench has no
  open-at-recipe seam, so it opens the bench on that band as the header's `⚒` does (open, never toggle).
- **The state is the first match of `HudOrdersVocab.state_of`**: (1) `sustainable < draw`, any kind ->
  amber `Falling X a turn — <why>` (`the land gives too little` food, `the patches give too little`
  fodder, `the stands regrow too little` a material); (2) hold short AND `take < draw` -> red
  `Falling — N short of keeping up`; (3) hold short, or refill short with some refill heads -> amber
  `Refilling slowly — N short` (hold short + refill short); (4) refill wanted with no refill heads ->
  amber Holding; (5) `on_hand >= target` -> `At target` (never below the target); (6) green Refilling
  with its ETA (`eta_turns`: `(target - on_hand) * draw / (take - draw)` for turns kinds,
  `(target - on_hand) / (take - draw)` for a material, rounded up, omitted when `take <= draw`). A band
  whose take outruns its draw is therefore rising however short its crew is, and reads Refilling slowly.
- **The summary warning names the orders**, joined `Food`, `Food and Wood`, `Food, Stone and Wood`: red
  `Not enough hands to keep up — Food is running down.` (`are` for several) when any order is in state 2;
  else amber `Not enough hands for every order — Food and Wood wait.` when any is hold / refill short.
- **A mixed patch is listed once**, under the row's `order_kind`; its row names every store it fills
  with its own order's first and `/turn` once at the end (`SourceForecast.yield_components_labelled`:
  `+2.40 food · +0.25 fodder · +0.05 fibre /turn`; a row that fills one store keeps today's text), and every other shown
  order it brought product to gets one dashed `↳ Also` line.
- **`Also coming in`** (hide, bone, fibre -- on hand or gained this turn) and **`Yours`** (the player's
  forage / hunt rows while a food order exists, and extract rows of a material that has an order) are
  ordinary collapsible sections (`WORK_SECTION_BY_PRODUCTS`, `WORK_SECTION_YOURS`). Every other row stays
  in today's sections. A leaderless band skips all of it and draws today's tab.

## Nothing in the group may ask for more width than the zone

The Work tab sits in a scroll with no horizontal scroll, so any child whose MINIMUM width exceeds the zone
(less the vertical scrollbar) pushes the whole tab wider than the dock and the right edge is clipped: the
crew bars, the `↳ Also` line, the summary and the `Also coming in` rows. The group therefore wraps
instead of declaring a width: the order line's name and `· … keep N` detail sit in an `HFlowContainer` (the
detail drops under the name, whole), the state label always wraps and the kit clause is its own wrapping
link under it (an `AUTOWRAP_OFF` state beside the clause in a flow asked for the whole sentence as its
minimum -- 349px of a 356px zone, 5px over once the scrollbar took 12), the `↳ Also` text wraps, and the
by-product row's left label wraps. Nothing in the group is elided to a tooltip any more.
`band_panel_orders.gd` runs `_assert_zone_content_fits` (height and width) on every frame it saves.
The side dock is 440 wide (`BandCityPanel.PANEL_WIDTH`); the wrapping is what holds at any width.

## The sheets

- **Stone and wood** (`DrawerComposeController._build_deposit_assign_controls`): when the band has an
  order for the material, `BandOrdersSheets.build_material_chief` replaces the Assign sheet if the order
  is the chief's (eyebrow = the material name, `Chief's order` box with the portrait, the Keep-on-hand
  stepper, Priority, the Diggers / Foresters box, Next turn, `Crew it yourself`); if the order is the
  player's, today's sheet wears a `could keep it for you` strip with `Hand it to`. The Band picker swaps
  the versions because both render in the same rebuild.
- **Food and Fodder** (`open_order_sheet`, `ComposeState.KIND_ORDER`): the stepper is in turns; the
  sources list is every chief row of the order (Best / Learning, crew, take) and the player's rows that
  bring the order's product, marked `yours`. The fodder sheet's `What eats it` list reads every tamed or
  penned herd the band's people keep that draws fodder (`_band_eating_herds`; a tamed herd its range feeds
  outright draws none and is not listed): fodder a turn is `fodder_draw + pen_fodder_shortfall`, and the
  range share of the split bar is `pen_pasture_fraction`. The order's `has_consumers` is "keeps a tamed or
  penned herd".
- **Where an order sheet sits.** `open_order_sheet` anchors the shared `ComposeSheet` to the Band panel's
  CARD rect (`BandPanelController.card_rect_if_shown`, read-only; a zero rect while the panel is hidden,
  in which case the selection card's rect is the anchor as for every other sheet). `ComposeSheet._place_card`
  tries RIGHT of the anchor, else LEFT of it, else (an anchor too wide for either: a top / bottom dock)
  ABOVE or BELOW it, whichever has more room, else the viewport margin. y stays the anchor's, clamped.
  An order has no tile subject, so the sheet used to land at the top-left margin, over a left-docked panel.
- **Wire rows name their tile `target_x` / `target_y`**, never `x` / `y`: the source and stand lists read
  those. (`x` / `y` belong to herd dicts and the Work-tab models, which is where the pen line and
  `BandOrdersController` read them.)
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
- The fodder sheet's per-herd fodder is `fodder_draw + pen_fodder_shortfall`; the sim publishes no single
  per-pen demand figure.
- A player row's offer picks the fodder order when the row's `fodder_yield` exceeds its food yield, else
  the food order.
- On screen the word is **fodder** everywhere; `Hay Meadow` and `hay grass` stay as plant and place names.
