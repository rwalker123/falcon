---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{TribeRoster,TribePage,TribePicker,hud_tribe_vocab}.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/tribe_panel.gd"
---

# The tribe on screen (#863)

`docs/plan_labor_at_scale.md` → "The tribe on screen" is the design. This file states how the client holds it.
A tribe is a formed group of the player's own bands; its identity is `tribe_id` (0 = none), carried on every
member's cohort dict with `tribe_member_bands` (band ids, parallel to `tribe_member_names`), `tribe_levy_ceiling`
(0 not in a tribe, 1 none, 2 low, 3 normal, 4 high), `tribe_works` and `tribe_proposal`. A labor row carries
`for_tribe` and, on the Scout row, `scouting_stretch`. `tribe_name` already ends in "tribe".

## The subject walk

- The Band panel's ◀ ▶ walks `[faction page] + one entry per top-level subject` (`TribeRoster.subjects`): a band in no
  tribe is its own entry, a tribe is ONE entry at its first member band's place in player-band order.
- **While a member band's own panel is up ("drilled in") the walk is that tribe's members only**, and the count reads
  n/N of them. Drilled-in means the panel shows one band that carries a `tribe_id` and is not on an aggregate page.
- A band clicked on the map is always that band's own panel; the map never opens the tribe page.
- The tribe page is `BandPanelController._tribe_page_id`, the faction page's twin: a fact about what the panel shows,
  re-resolved by `tribe_id` each snapshot, with `panel_band()` left alone. A tribe that dissolves under the page falls
  back to its first former member's band panel (`_tribe_former_members`). Every gate that greys for the faction page
  asks `_is_aggregate_page()`; `is_faction_page()` answers it too, so a passive redraw cannot steal the page.
- The tribe page and the faction page move no camera.

## Drill-in

A member's panel is its ordinary panel plus a breadcrumb above the title, `‹ Reedwater tribe › Greyfen`
(`BandCityPanel.set_breadcrumb`, `breadcrumb_activated`); the `‹ tribe` half renders the tribe page. A member's
Scouting range is read-only there, `+N tiles · set by the tribe`; the stepper lives on the tribe page and reaches
`tribe_scout_requested` through `_emit_tribe_scout`, addressed by the first member. A labor row with `for_tribe` wears a
`for the tribe` mark beside its name on every Band panel (`_add_for_tribe_mark`, in the four row builders).

## The tribe page (`TribePage`)

Two zones, `Tribe` and `Work`. The Tribe zone: the totalled People bar; the Food vital (stock and net summed; turns are
summed stock over the summed `food_need`, since a tribe has no sim runway); the tribe's one scouting range (stepper
clamped to `scouting_range_max`) with one line under it, either `N scouts ride the tribe's edge` or the first short
stretch (`Greyfen's stretch sees +1 of +2 tiles`), members with `scouting_stretch` 0 ignored; the levy ceiling, a
four-way option picker. No morale, growth or other per-band vital. The Work zone: a Bands section, then Shared works.

- A band line: a `DisclosureTriangle` fold, the underlined name (drills in), `N jobs · N for the tribe · N idle` (zero
  parts dropped; standing roles are not jobs), and the crew number over a three-way bar (at home / for the tribe / idle).
  The number is the three parts summed; the bar's tooltip carries the same sentence. Fold state is
  `_tribe_open_folds["<tribe_id>:<band_id>"]`.
- An opened line is `_build_tribe_band_rows(band)`: the Work zone's own producers (`_work_section_items`) and row
  builders, so a row cannot differ from the band's own page. Rows open the source's sheet as they do there; the work
  inspector is not mounted on the tribe page.
- Shared works: one row per `tribe_works` entry: branch label and `(x, y)`; the target rung (or the standing one when
  `target_rung` is empty); `handed to Greyfen` / `handed to Greyfen, then Pinewold` with the worker count; a `✕ Stop`.

## The `Band:` picker (tile card)

`DrawerComposeController._build_band_picker(…, tribe)` takes a tribe context only from the two cards that start a shared
work: the road ladder card and the tile crew sheet. The hunt, forage and deposit sheets pass none and stay a flat list of
bands. With a tribe present `TribePicker.build_row` lists the tribe in place of its members with a fold beside it;
opening the fold lists the members indented beneath. The fold (`_tribe_picker_open`) starts closed each time a card opens.

- **Default:** the tribe, unless the Band panel is drilled into a member of it. Tile sheet: that member becomes the
  default band; a band that already crews the tile branch also keeps its own row. Road card: the nearest band's tribe,
  unless the road has a keeper in the roster or the panel is drilled into a member of that tribe (old nearest default).
- A tribe pick is addressed by the first member (`TribeRoster.address_band`) for gear, hands and strip reads.
- **Commands, tribe picked:** a road grade or pave sends `tribe_work <f> <b> route <x> <y> <rung>` with the catalog's
  rung id (`route:dirt_road` / `route:paved_road`, off `route_rungs()` by verb, `HudTribeVocab.ROAD_VERB_RUNGS` as the
  fallback); a tile rung sends `tribe_work <f> <b> <branch> <x> <y> <rung_key>` (`RungCardController.open_tile_track`'s
  `tribe` argument, `rung_key_of_verb`); the sheet's commit with no rung sends `tribe_work <f> <b> <branch> <x> <y>`, or
  `tribe_work <f> <b> stop <branch> <x> <y>` while the tribe holds that work. The crew stepper is not drawn for a tribe;
  the sheet says `Joins the tribe's shared works.`
- A member picked: the old commands (`build …`, `assign_labor`).
- Every tribe command leaves through `tribe_command_requested` (`{action, faction, band_id, …}`), relayed by `Hud` and
  spelled by `Main.format_tribe_command`; `tribe_levy <f> <b> none|low|normal|high` and the tribe page's Stop share it.

## Materials & Crafting

`CraftingPanelController` lists the tribe in place of its members with the same fold (`TribePicker.entries`). A picked
tribe (`PAYLOAD_TRIBE_MEMBERS`) hides the rail and shows `CraftingTribeView.build(members)` — loaded by path
(`HudTribeVocab.CRAFTING_VIEW_PATHS`) — or, while that script is absent, one line:
`The tribe has no bench. Each band's bench marks what it makes for the tribe.` ◀ ▶ walk the top-level entries, and a
selected member's walk is its tribe's members. The header ⚒ pressed on the tribe page opens on the tribe's entry.

## The orb row

`AttentionController.tribe_proposal_attention` raises ONE row per `tribe_id` whose first member carries a
`tribe_proposal` with a non-empty key, locating at its `tile_x, tile_y`. The block's text is `<tribe_name> — <sim text>`
and carries the tribe flag; `TurnOrb._on_proposal_pressed` passes it through `proposal_action_requested` and
`Main.format_chief_proposal` answers it `tribe_proposal <f> <b> approve|defer <key>`, never `approve_proposal`.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/hud_tribe_vocab.gd` | `HudTribeVocab` — the wire keys, levy levels, command actions, every on-screen word and size the tribe surfaces use |
| `ui/hud/TribeRoster.gd` | All-`static`: who is in a tribe, the switcher's subjects, member order, the gathering place, the three-way crew split |
| `ui/hud/TribePage.gd` | Builds the tribe page's two zones from a `RefCounted` of collaborators and Callables; emits nothing itself; owns the fold button |
| `ui/hud/TribePicker.gd` | All-`static`: the tribe-in-place-of-members entry list and the `Band:` row with its fold |
| `tools/ui_preview/chapters/tribe_panel.gd` | The chapter: switcher count, tribe page, drill-in, both pickers' command lines, Crafting, the orb row |
