---
paths:
  - "clients/godot_thin_client/src/scripts/ui/StartingLoadoutPanel.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/StartingLoadoutController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_loadout_vocab.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/starting_loadout.gd"
---

# The opening loadout picker (issue #629)

The turn-one outfitting window: **the ONE source of a campaign's starting gear and material.**
`equipment.json` ships `start_stock_fraction: 0.0` and no material declares a start stock, so a band
spawns owning nothing at all and this screen is what it walks away with.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/StartingLoadoutPanel.gd` | The free-floating card — three columns (kits / resources / what this builds), two budget meters, a commit control and its own reopen pill. **`AutoSizingPanel`, not `PanelCard` + `DockScrollFit`** (`panel-framework.md`): it is measured against the ROOM. **ONE NODE CARRIES BOTH STATES** — the card and the pill are two children and exactly one is visible, so one fit and one placement serve the expanded and dismissed states; the fit measures whichever is showing and `_place` centres the card in the room and puts the pill at the top of it. It renders a payload and emits five intents (`dismissed` / `reopened` / `kit_count_changed` / `material_units_changed` / `commit_requested`) and holds no allocation of its own |
| `ui/hud/StartingLoadoutController.gd` | The controller half, held by `HudLayer` as `_loadout`. **Holds the allocation, every clamp, both remainders and the "what this builds" arithmetic.** Ingests the window (`set_window`), the parsed equipment config (`set_equipment_config`) and the recipe book (`set_recipes`); relays `set_starting_loadout_requested` onto `HudLayer`'s and pushes its orb half through `attention_changed` |
| `ui/hud/hud_loadout_vocab.gd` (`HudLoadoutVocab`) | The vocabulary leaf — the wire keys, the words, the measured geometry, and the **swatch ring** (`apply_palette`, registered in `HudPalette.apply`) |
| `tools/ui_preview/chapters/starting_loadout.gd` | The preview chapter, LAST in `CHAPTERS` — five frames and twenty-four checkpoints. See `harness-ui-preview.md` |

## What the client owns, and what it must not decide

- **THE WINDOW'S LIFETIME IS THE SIM'S.** `CampaignSection.openingLoadout.open` is the authority: the
  picker opens itself on the first frame it reads `true` and the whole surface goes when it reads
  `false`. The client never closes it on its own and never blocks End Turn — the window shuts on the
  first turn advance whether or not an order was sent, so the client's job is to WARN.
- **A COMMIT IS OPTIMISTIC AND THE NEXT FRAME IS THE ANSWER.** The order goes, the card collapses. If
  `open` is still `true` on the next frame the sim REFUSED (it fails closed and whole), so the card
  comes back carrying `REFUSAL_NOTICE`. `_awaiting_commit` is that one-shot expectation. Assuming
  success is how a player ends up believing they are outfitted with nothing spent.
- **THE THIRD COLUMN IS A READOUT, NOT A BUILD PLAN.** `count = floor(min over inputs of
  allocated[material] / required)` — *how many of THIS one thing the whole pile could make*. Two rows
  both reading `×2` are two answers to two separate questions, which is what `BUILDS_NOTE` says on
  screen.
- **THE GATED BENCH TOOLS ARE ABSENT, NOT GREYED.** The list is filtered on the published
  `craftable_recipe_ids` and on nothing else. Sniffing a craft offer's refusal SENTENCE would turn a
  player-facing string into a machine contract; re-testing `requires_knowledge` client-side would be a
  second copy of the sim's own rule.

## The two catalogues it JOINS onto

Neither the kit roster nor a recipe's input costs is copied into the loadout section, and neither
should be:

- **The kit roster rides `SubsistenceSection.equipmentConfigJson`** — the picker parses that blob (it
  is the only HUD consumer of it; the Workbench is the other reader) and drops the carry-nothing entry
  **by its EMPTY `uses`**, never by matching `none`. A roster that renamed that entry is still
  excluded; one that gave it items rightly starts offering it.
- **A recipe's `inputs` and `work` ride `SubsistenceSection.recipes`** — the same book the crafting
  ledger and the knowledge screen read. `HudLayer.update_crafting_catalogues` fans it to a third
  reader rather than the picker re-deriving a cost from anywhere else.

## One colour vocabulary, and it means "material"

A swatch is a material's identity, resolved ONCE in `StartingLoadoutController._material_rows` and
drawn identically in the resources column, in the legend above the recipe list and on every recipe's
cost row — so the three read as one key rather than as three tables that can disagree.

`HudLoadoutVocab.SWATCH_COLORS` is a **RING indexed by a material's position in the published pick
list**, not a table keyed by id: the list is `start_profiles.json` data, so a profile offering a
material this build has never heard of must still get a swatch, where a table would hand it the
fallback ink shared with everything else.

**Nothing else in the panel is tinted by identity.** The kit meter's bar is one spent segment against
its remainder — kits are interchangeable pairs of hands with nothing to tell apart — because a second
colour vocabulary beside the legend would be read as part of it.

## Two measurements that a screenshot is the only witness for

- **The column seam is 2px where the horizontal rules are 1px.** The HUD renders at a fractional
  canvas scale, so a 1px vertical line lands on a sub-pixel boundary whose coverage depends on where
  its column falls — and with three equal-stretch columns the two seams fall differently: the first
  rendered and the second vanished outright. A rule that draws on one side of a card and not the other
  is worse than no rule.
- **The unspent half of a budget bar is `HudStyle.LINE`, not `LINE_SOFT`.** The soft hairline ink
  measured within a few values of `PANEL_SOLID` and drew as no bar at all on a budget nothing had been
  spent from — the state the meter is most needed in.
- …and a **remainder of ZERO draws no segment**: `HudWidgets.build_composition_bar` floors every
  segment's stretch ratio so a one-unit segment stays a visible sliver, which turns a zero-count
  segment into a permanent sliver of *nothing left* on a fully spent budget.

## The orb's row

An unfinished loadout is one more producer on the generic attention hub
(`ATTENTION_KIND_OPENING_LOADOUT`), folded in through `TurnOrbController.set_loadout_attention` —
its own half for `_knowledge_attention`'s reason: it is produced by a section the band loop never
sees, and on a delta carrying only `opening_loadout` that loop does not run at all. The half is
guarded against re-pushing an unchanged value, because it is EMPTY from turn two onward and an
unguarded push would cost a deep copy of the band half and an orb redraw on every snapshot for the
rest of a campaign.

It is **WARN and deliberately NOT `blocking`**: closing the window is the sim's business, so the row
says what is unspent and the `Advance ▸` footer stays live. It is NON-LOCATING and on
`ATTENTION_KINDS_WITH_A_PANEL`, so its `Open ▸` brings the picker back.

## Wire and command

| direction | contract |
|---|---|
| in | `CampaignSection.openingLoadout` → `opening_loadout` on the snapshot dict (`native/src/dict/campaign.rs`). Decoded on **BOTH** the full and delta paths — the sim whole-diffs the table, and the one change that matters is `open` going false on the first turn advance |
| out | `set_starting_loadout <faction> [kit <id> <n>]... [material <id> <n>]...`, built by `Main.format_set_starting_loadout`. **The whole allocation every time, never a diff** — the verb fails closed and whole. An EMPTY tail is a real order (*spend nothing, close the window*), which is the one place that formatter departs from its neighbours and returns a line rather than `{}` |
