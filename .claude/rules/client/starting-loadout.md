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
| `ui/StartingLoadoutPanel.gd` | The free-floating card — three columns (kits / resources / what the resources can build), two budget meters, an **unconditional** commit control and its own reopen pill. **`AutoSizingPanel`, not `PanelCard` + `DockScrollFit`** (`panel-framework.md`): it is measured against the ROOM. **ONE NODE CARRIES BOTH STATES** — the card and the pill are two children and exactly one is visible, so one fit and one placement serve the expanded and dismissed states; the fit measures whichever is showing and `_place` centres the card in the room and puts the pill at the top of it. It renders a payload and emits five intents (`dismissed` / `reopened` / `kit_count_changed` / `material_units_changed` / `commit_requested`) and holds no allocation of its own. `_column` draws NO caption for an empty note, which is what keeps the builds column from carrying a blank row where the other two carry a line |
| `ui/hud/StartingLoadoutController.gd` | The controller half, held by `HudLayer` as `_loadout`. **Holds the allocation, every clamp, both remainders and the "what this builds" arithmetic.** Ingests the window (`set_window`), the parsed equipment config (`set_equipment_config`) and the recipe book (`set_recipes`); relays `set_starting_loadout_requested` onto `HudLayer`'s and pushes its orb half through `attention_changed` |
| `ui/hud/hud_loadout_vocab.gd` (`HudLoadoutVocab`) | The vocabulary leaf — the wire keys, the words, the measured geometry, and the **swatch ring** (`apply_palette`, registered in `HudPalette.apply`) |
| `tools/ui_preview/chapters/starting_loadout.gd` | The preview chapter, LAST in `CHAPTERS` — seven frames and thirty-nine checkpoints, including the orb's two colours. See `harness-ui-preview.md` |

## What the client owns, and what it must not decide

- **THE WINDOW'S LIFETIME IS THE SIM'S.** `CampaignSection.openingLoadout.open` is the authority: the
  picker opens itself on the first frame it reads `true` and the whole surface goes when it reads
  `false`. The client never closes it on its own and never blocks End Turn.
- ⛔ **AN APPLY IS A REPLACEMENT, SO A COMMIT SHUTS NOTHING.** The order may be sent, revised and sent
  again as often as the player likes; **only the TURN ADVANCE closes the window and forfeits what is
  left**. Commit sends the line and collapses the card so the map is readable — the picks stay, the
  reopen pill stays, the orb's row stays — and nothing latches "already committed".
- ⛔ **`open` IS THEREFORE NOT A SUCCESS SIGNAL.** The controller once held an `_awaiting_commit` flag
  and read a still-open window on the next frame as a REFUSAL. That was right while an accepted order
  closed the window and is now exactly inverted: under replacement semantics every SUCCESSFUL commit
  leaves it open, so the branch would post *"that order was refused"* after each one. It is gone, and
  nothing here may go back to inferring a refusal from `open`. **A genuine refusal is not visible on
  this card at all** — `handle_set_starting_loadout` only `warn!`s it to the log stream, so there is
  no client-facing channel for it; surfacing one needs a `command_events` row server-side.
- **THE THIRD COLUMN IS A READOUT, NOT A BUILD PLAN.** `count = floor(min over inputs of
  allocated[material] / required)` — *how many of THIS one thing the whole pile could make*, so two
  rows both reading `×2` are two answers to two separate questions. **The sentence that used to say so
  on screen is deleted**: the column head names its own subject now (`What the resources can build`)
  and the `×N` beside each row is the explanation.
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

## The copy is Ray's, and it is short on purpose

Three of the four column captions were sentences explaining the model behind their column — what a
kit budget is derived from, what a point buys, how the `×N` is computed. Read together they were an
essay beside three lists a player is trying to use ("too much AI speak"), and the fix was not a
shorter explanation but none:

| was | is |
|---|---|
| `KITS_NOTE` "One for every pair of working hands." | "Select kits your band will start with" |
| `RESOURCES_NOTE` "One point buys one unit. Everything arrives middling." | "Select crafting resources the band will start with" |
| `BUILDS_HEAD` "What this builds" — it named no subject, beside two other columns | "What the resources can build" |
| `BUILDS_NOTE` "If you spent the whole pile on one thing." | **deleted**, and `_column` draws no node for it |
| `PANEL_SUBTITLE` "…Nothing is theirs until you commit, and anything unspent is lost when the turn advances." | "What your people carry when they set out." |
| `COMMIT_TOOLTIP` "…The window shuts and whatever is left of either budget is gone." | "You can change this until you end the turn." |

**Do not restore an explanatory clause to any of them**, and note the deliberate absence of a full
stop on the two Ray rewrote.

**THE CARD MAKES NO FORFEITURE CLAIM ANYWHERE.** It was on the commit control's face
(`Set out — forfeit 17 kits and 2 units`) and again in the subtitle's second clause; both are gone.
The remainder is worth saying and is said ONCE, on the **turn orb**, which is the surface that already
counts down to the advance that causes it. The preview chapter asserts the word `forfeit` is absent
from the whole card, so a second copy cannot come back quietly.

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

## The orb's row — ONE row for the whole window, in two colours

The loadout is a producer on the generic attention hub (`ATTENTION_KIND_OPENING_LOADOUT`), folded in
through `TurnOrbController.set_loadout_attention` — its own half for `_knowledge_attention`'s reason:
it is produced by a section the band loop never sees, and on a delta carrying only `opening_loadout`
that loop does not run at all. The half is guarded against re-pushing an unchanged value, because it
is EMPTY from turn two onward and an unguarded push would cost a deep copy of the band half and an
orb redraw on every snapshot for the rest of a campaign.

⛔ **THE ROW IS PRESENT WHILE THE WINDOW IS, SPENT OR NOT.** The card is dismissible and this row's
`Open ▸` is the guaranteed way back to it, so a producer that fell silent once both budgets were
clear would strand a player who had finished picking, put the card away, and then wanted to revise
before ending the turn. What moves is the SEVERITY and the WORDING:

| state | severity | reads |
|---|---|---|
| anything unspent | `warn` → `HudStyle.WARN` | `Band not outfitted` / `1 kit unspent, 2 units unspent` |
| both budgets clear | `ready` → `HudStyle.READY` | `Band outfitted` / `everything is picked` |

It is **NOT `blocking` in either state**: closing the window is the sim's business, so the
`Advance ▸` footer stays live and this row only ever warns. It is NON-LOCATING and on
`ATTENTION_KINDS_WITH_A_PANEL`.

### `ready` is a new rung on the orb's ladder, and it had to be the BOTTOM one

`ready` means *a standing requirement is now MET*. `info` was the wrong home: that rung means neutral
NEWS — a build finished, a discovery landed — which is a statement about something that HAPPENED, and
on three of the four themes it is not even a cool colour.

- **It ranks below `info`**, so a satisfied loadout never takes the orb's accent off a real warning
  anywhere else; it paints the orb only when it is the highest entry present.
- ⛔ **EVERY RANK IN `TurnOrb.SEVERITY_RANK` IS NOW >= 1, and that is the load-bearing half.**
  `_highest_severity_color` seeds a "best so far" and replaces only on a STRICTLY greater rank, so a
  severity sitting at rank 0 can never paint the orb whatever colour it maps to. The whole ladder was
  shifted up by one rather than giving the new rung the 0 that would have made it silently inert, and
  the seed is a named `RANK_NONE` below all of them — which also means an UNKNOWN severity now paints
  (in the fallback ink) instead of leaving the orb on a colour no entry asked for.
  **Sabotage-verified**: with `ready` back at rank 0 the orb comes back CREAM on a fully-picked
  loadout and exactly one assertion fails.

### `HudStyle.READY` is authored per palette, and two of the four had to be retuned

**Do not reach for `SIGNAL`** — it means *calm, nothing needs you*, a statement about the absence of
news rather than about a thing being done — and **do not reach for `MapView.OVERLAY_FALLBACK_COLOR`**,
which is a near neighbour on the console palette and means "an overlay channel with no ramp of its
own".

Each theme carries its own value in its own register, because a single hex lands on top of something
in at least two of them: **loam's `SIGNAL` is already a blue and console's is already a cyan**, so a
blue "satisfied" reads there as *nothing in particular*. Loam's answer is a TEAL — separated by hue
rather than by temperature, the only axis left in a palette whose calm accent is the colour this token
wanted — and console's is pushed to a frank azure to clear its cyan. Console's is also **the one hex
literal in a block of float literals**, which is fine because the key is NEW: the "leave the unchanged
theme exactly unchanged" rule is about round-tripping values that already existed.

The separation is asserted **as data, over all four palettes at once**, against each theme's own
`SIGNAL` / `WARN` / `DANGER` / `HEALTHY` — plus a second claim that the four values are DISTINCT,
without which the first passes on one hex that happens to clear every theme's accents. Both were
tightened by the assertion rather than the assertion being loosened to admit them (loam and console
went 0.20 → 0.25 and 0.17 → 0.25 against their own `SIGNAL`).

## Wire and command

| direction | contract |
|---|---|
| in | `CampaignSection.openingLoadout` → `opening_loadout` on the snapshot dict (`native/src/dict/campaign.rs`). Decoded on **BOTH** the full and delta paths — the sim whole-diffs the table, and the one change that matters is `open` going false on the first turn advance |
| out | `set_starting_loadout <faction> [kit <id> <n>]... [material <id> <n>]...`, built by `Main.format_set_starting_loadout`. **The whole allocation every time, never a diff** — the verb fails closed and whole. An EMPTY tail is a real order (*spend nothing, close the window*), which is the one place that formatter departs from its neighbours and returns a line rather than `{}` |
