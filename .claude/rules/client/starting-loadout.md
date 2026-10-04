---
paths:
  - "clients/godot_thin_client/src/scripts/ui/StartingLoadoutPanel.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/StartingLoadoutController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/OpeningCardController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_loadout_vocab.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/starting_loadout.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/opening_card.gd"
---

# The outfitting picker (issue #629)

A band's outfitting window: **the ONE source of a campaign's starting gear and material.**
`equipment.json` ships `start_stock_fraction: 0.0` and no material declares a start stock, so a band
spawns owning nothing at all and this screen is what it walks away with.

## ⛔ EVERY BAND GETS A WINDOW, AND THE CARD DRAWS ONE OF THEM

The sim opens a window per band and shuts them all on the turn advance
(`.claude/rules/core_sim/starting-loadout.md`). Two kinds, and `loadout_window.parent_band_id` is
which:

| `parent_band_id` | the window | what the picks do | what caps them |
|---|---|---|---|
| `0` (`HudLoadoutVocab.GRANT_PARENT_BAND_ID`) | a **GRANT** — the spawned band's, and a splinter's when it split off a band whose own grant was unspent | **MINT** gear | the window's `carry_capacity` alone |
| any band id | a **TAKE** on that band — what a split opens on the splinter from turn two | **MOVE** gear out of that band's ledger | the window's `carry_capacity`, AND `parent_item_supply` / `parent_material_supply`, each already `holdings + this take's standing units` |

**The card is band-scoped and the controller holds one state per band** (`_bands`, keyed by the
durable `band_id`); `_subject` is the one being rendered. Every clamp, the carry remainder, a split's
food preview, the "what the resources can build" arithmetic and the swatch ring are the client's.

## ⛔ ONE CARRY METER — the two point budgets are gone (#732)

The card spends ONE currency, the band's **load**, against ONE cap, the window's `carry_capacity`
(the band's TOTAL carry, workers × one pack — `.claude/rules/core_sim/band-carry.md` owns the
model). There is no kit-slot budget and no material-point budget any more; both windows read the same
meter.

```text
load = item_carry_weight × Σ EXPANDED item units + material_carry_weight × Σ material units
```

- **`_order_load` is the ONE place the two weights are applied**, so the meter, every row's
  `can_add` and every clamp weigh an order exactly the way the server's `OverCarry` refusal does. A
  kit weighs the items it expands to (`_kit_unit_load`): `big_game` (spears + sled) weighs 2 at the
  shipped weight of 1. The weights ride the campaign section (one per world); the capacity rides each
  band's window.
- **The meter** is `CARRY_REMAINING_FORMAT` (`12 / 78 carry left`) over a stacked bar: the kits as
  ONE segment, each material in its swatch ink, then the remainder. A negative remainder is printed
  negative in `HudStyle.WARN`; a full pack reads in `HudStyle.SIGNAL` (a finished decision, not a
  problem).
- **A row says what one more costs only where that is not obvious** — not at the meter's own unit
  (`CARRY_OBVIOUS_UNIT_LOAD`, 1.0). A two-item kit's uses line ends `· 2 carry`
  (`KIT_CARRY_COST_FORMAT`); a material row reads `N carry each` (`MATERIAL_CARRY_COST_FORMAT`) when
  the material weight is not 1. A one-item kit says nothing about carry.
- **The ceiling is float arithmetic with a tolerance** (`_carry_fits`, `CARRY_EPSILON`), so `3 × 0.1`
  fits `0.3`; a weightless unit never binds (`CARRY_UNBOUNDED`).

### FOOD IS PART OF THE BAR — what a band carries includes its food

The bar's whole width is `carry_capacity`, the band's whole carry. It draws kits, then each material,
then a **food segment** (`FOOD_SEGMENT_COLOR`, a pale wheat derived from the palette in
`apply_palette` via `FOOD_SEGMENT_TINT`), then what is free — and the meter text states what is
actually free, `carry − goods − food`. Under the bar, ONE legend row: a food swatch and one line in
`INK_DIM`:

- **a fixed larder** (`food_fixed` — the opening band, a parent whose grant is open): `Food 78`
  (`FOOD_FIXED_FORMAT`). It explains why the goods room is smaller than the carry; there is no choice
  to state.
- **a splinter** (food yields to goods): `Food 12 of 14`, plus ` · fewer tools leave room for more`
  (`FOOD_ROOM_CLAUSE`) while it is under its share.

⛔ **NEVER WARN INK.** Leaving food with the home band is a CHOICE, not a fault. The amber
*"Take fewer tools to bring more food."* this replaced read as an error (playtest), and a meter that
said `22.3 / 53.3 carry left` while food filled exactly that 22.3 said the pack had room it did not.

The goods allowance the `+` and the over state read is `_goods_allowance`: `carry − food_carried` on
a fixed larder, the whole carry on a splinter — the same cap the sim's `OverCarry` refuses above.

⛔ **WHERE N COMES FROM** (`food_brought_of`). The sim re-resolves the food on every accepted order and
the food is a function of the goods load alone, so:

- **while the card's goods load equals the load of `BAND_HELD`** (the allocation the wire last
  published), the wire's `food_carried` is the truth and N reads it;
- **once a press moves the load**, N is the preview `min(food_share, carry_capacity − load)`, so the
  line moves on the press rather than a frame later.

The test is the LOAD, not `BAND_UNECHOED`: the press renders before its order is queued, so a
list-keyed test would answer "nothing out" on the very render the press produces and lag a press
behind. **A refused order rolls the food back with the picks** and needs nothing of its own for it — a
refusal resets the picks to `BAND_HELD`, so the card weighs the held load again and reads the wire's
food.

**A band's card opens ITSELF once, per band.** `_auto_opened` is keyed by band id, so a split's
splinter stands its own card up on the turn it is made and a card the player dismissed does not come
back on the next snapshot. Every pending band is marked in one pass, so two splits in one turn cannot
queue two pop-ups on two consecutive frames.

**Every card draws its band's OWN rows** (`loadout_window.kits` / `.materials`), which are **gear the
band actually holds** — never a suggestion and never an unspent budget.

> ### ⛔ THERE IS NO DRAFT. THE SIM OUTFITS A BAND WHEN IT MAKES IT, AND EVERY PRESS ORDERS
>
> The sim applies a band's default outfit at the moment the band is created — the opening band at
> world start, a splinter at its split — through the same path a player's `set_starting_loadout`
> takes. So a band nobody touches keeps its default, and there is no such thing as an allocation
> waiting to be filled in.
>
> **The card therefore holds no state the sim does not know about.** A `+`/`−` on a kit or material
> row emits `set_starting_loadout` for that band there and then, carrying the whole allocation as it
> stands AFTER the press. `_write_pick` is the ONLY writer of either pick map outside adoption, which
> is what makes the invariant checkable: **a local value is only ever written together with a command
> being sent.** A press the clamp refuses writes and sends nothing — the allocation is unchanged, so
> there is no replacement to make.
>
> **WHAT THIS CLOSED, proven from a recorded session:** a player filled a splinter's card in, never
> pressed `Set out`, ended the turn, and the band got **nothing**. The server had received exactly one
> `set_starting_loadout` that whole game, for the other band — while the card and the turn orb both
> said the band was outfitted, because both read the card's own local copy. The draft was the defect,
> so the draft is deleted rather than defended.
>
> **DELETED WITH IT, and nothing may rebuild any of it**: `BAND_TOUCHED_KITS` / `BAND_TOUCHED_MATERIALS`
> (the `id -> true` sets that separated a player's rows from the sim's), `_clamp_picks` / `_clamp_order`
> (the whole-order re-fit those sets ordered), `BAND_SEEDED` / `BAND_PUBLISHED`, and `_prefill_claimed`
> with the campaign pre-fill path it guarded. Every one of them existed to reconcile two copies of one
> allocation; there is one copy now.
>
> **A row at ZERO is erased rather than stored**, so the pick maps ARE the allocation: a `{gathering: 0}`
> entry is a row nobody holds, it drops out of the composed line anyway, and keeping it made "is
> anything ordered" answer yes to an empty take.

> ### ⛔ AN OPTIMISTIC WRITE NEEDS A ROLLBACK, AND THE SEND'S OUTCOME IS ONLY KNOWN IN `Main`
>
> A press has to move the number under the player's finger on the frame it was pressed, so the picks
> are written before the send's outcome is known. The payload therefore carries the allocation as it
> stood BEFORE the press (`REVERT_KITS` / `REVERT_MATERIALS`, which `format_set_starting_loadout`
> ignores), and `Main._on_hud_set_starting_loadout` hands the whole payload back to
> `HudLayer.revert_starting_loadout` → `StartingLoadoutController.revert_order` when
> `_send_formatted_command` answers `false`. It restores both halves, drops the entry the send queued,
> and RE-RENDERS — `hud-modules.md` → "AN OPTIMISTIC WRITE NEEDS A ROLLBACK" is the general rule and
> `_on_hud_assign_labor` the worked example.
>
> **The handle is the WHOLE allocation rather than the one row that moved**, and that is not the
> `pending_key` rule being broken: the verb is a whole-order replacement, so there is no other
> un-acknowledged edit on the band for a whole-allocation restore to discard — every local write has
> been sent as it was made.
>
> **It is reached by a `has_method` PROBE, which fails SILENTLY**, so `revert_starting_loadout` is a
> thin `HudLayer` delegator and stays one. No retry, no queue of unsent commands.

> ### ⛔ A PUBLISHED ALLOCATION IS ADOPTED UNLESS IT IS THIS CARD'S OWN ECHO
>
> A split re-fits the PARENT's standing allocation down to its reduced carry and re-materializes the
> band from it (`fission::rebalance_partitioned_grant`). The wire is the authority on what a band
> holds, so the card adopts those rows whole and unclamped — a card that kept its own copy would draw
> the pre-split rows against the post-split carry, a negative meter reproduced client-side out of
> stale state. A band the sim has genuinely left over its carry must READ as over rather than be
> trimmed into looking fine.
>
> **`BAND_UNECHOED` is what tells an echo from a move.** The sim recaptures after every dispatched
> command, so each press comes back as a frame restating it; the list holds the orders this card has
> SENT and not yet seen come back, oldest first, in the same `{kits, materials}` map shape the
> published allocation is read into. A published allocation FOUND in that list settles it and every
> order before it (`slice`) and the card keeps what it is showing; anything else clears the list and
> is adopted.
>
> ⛔ **COMPARING AGAINST WHAT WAS SENT, NOT AGAINST WHAT WAS LAST SEEN, AND ONE VALUE IS NOT ENOUGH.**
> Press twice quickly and the first press's echo lands while the card is already showing the second's:
> against a last-seen copy — or against only the LAST order sent — that echo reads as a change and
> drags the card back a step, visibly, on every pair of presses. The list is what makes an echo
> recognisable however many are in flight. The preview's `loadout/flicker` claims are the guard, and
> they fail at `(4, want 5)` the moment the test is removed.

> ### ⛔ A SPLINTER'S CARD DRAWS ITS DEFAULT TAKE, AND A PRESS ON IT ORDERS THE WHOLE THING
>
> The split's take is **kit-denominated** and published in those same rows, so the card draws the
> allocation the band is already standing on. The row the player does NOT touch is exactly the row a
> replacement must still name: an order carrying only the pressed kit would hand the rest of the dowry
> back to the parent.
>
> It drew ZERO for one iteration, while the take was a bare per-item manifest no kit allocation could
> express — and an apply being a REPLACEMENT, the first press then ordered *take nothing* and handed
> the splinter's whole dowry back.
>
> **Nothing here special-cases an empty tail**, and nothing may start to: an empty order still means
> *take nothing*, on a take exactly as on a grant. What makes a press safe is that the card is not
> empty when the take is not.

> ### ⛔ A REFUSED ORDER IS AN EVENT ROW, BECAUSE NO BAND ROW MOVES
>
> A refused `set_starting_loadout` changes nothing the sim holds, and populations ship as diffs — so
> the frame after it carries no row for the band, `_ingest_window` never runs, and the optimistic
> picks would stay on screen as gear the band does not hold. The sim reports it instead as one
> `command_events` row: `kind == "starting_loadout"`, `label == "Outfit failed"`, the reason as
> `detail`, and the refused band's durable id as `band` (`0` = about no band). An accepted order emits
> nothing.
>
> `HudLayer.ingest_command_events` fans the array to `StartingLoadoutController.ingest_command_events`
> with the current turn. A row is acted on only when it is a `starting_loadout` row of the PLAYER's
> faction, names a band the card holds a window for, has a `seq` above the card's cursor
> (`_event_seq_cursor` — a full snapshot re-sends the whole ring, and a refusal applied twice would
> reset picks the player has since remade), and has `tick` equal to the current turn (an older turn's
> window is shut). A row with no `seq` cannot be de-duplicated and is dropped. The cursor resets in
> `reset_world_state`, `seq` being per world.
>
> ⛔ **A REFUSAL SETTLES ONE ORDER — THE OLDEST IN FLIGHT — AND NOTHING ELSE.** The sim runs a
> band's orders in the order they were sent, and `Main` ingests populations before command_events, so
> by the time a refusal is read every earlier ACCEPTED order has echoed and left `BAND_UNECHOED`: the
> refused one is its oldest entry, and only that entry is dropped. If the list is then empty — or was
> empty on arrival, as after a reconnect — the band goes back to **`BAND_HELD`**, the allocation the
> sim last published for it (refreshed on every `_ingest_window`, echo or adoption alike). If later
> orders are still out the picks are left alone: each is a whole replacement, so the LATEST order's
> own echo or refusal settles what the band holds. Resetting every in-flight order instead dragged
> the card back past an order the sim then accepted and left the line over it. `BAND_HELD` is read by
> nothing but a refusal, which is what separates it from the retired `BAND_PUBLISHED`: the pick maps
> are still the one allocation.
>
> ⛔ **A REFUSAL LINE NEVER SITS OVER A STATE THE SIM PUBLISHED AFTER IT.** Any accepted echo, and any
> adoption whose allocation actually MOVED (an unchanged republish is not news — every populations
> frame re-ingests every band), clears `BAND_REFUSAL`. And because populations land first, a refusal
> read in the SAME snapshot as such a publish is the older fact — the capture carrying both was taken
> after the sim ran every order in it — so it settles its order but draws no line.
> `_published_this_snapshot` holds those bands; `begin_snapshot`, called from `HudLayer.update_overlay`
> (the first thing `Main` calls in every snapshot), clears it.
>
> **The line** sits last in the card's head — under the carry meter and a split's food lines,
> directly above the columns it just reset — in `HudStyle.WARN`: `Not taken: <detail>. Picks reset
> to what the band holds.` (`REFUSAL_FORMAT`) when the refusal reset the picks to `BAND_HELD`, and
> only `Not taken: <detail>.` (`REFUSAL_IN_FLIGHT_FORMAT`) when later orders are still in flight and
> the picks were left standing — `BAND_REFUSAL_RESET` records which, so the line never claims a reset
> that did not happen. A trailing full stop on the detail is not doubled in either. The detail is the
> sim's own words (an over-carry order reads *"the order weighs 15.0 against a carry of 14.0"*, the
> `OverCarry` shape), so no kit or item is named client-side. It is capped at two lines
> (`max_lines_visible` + ellipsis) and carries the whole text on its tooltip. It is cleared by that
> band's next stepper press (`_write_pick`) or by a later publish (above); a press the clamp refuses
> sends nothing and so clears nothing.
>
> The same row reaches the event dock as an unlisted kind — `DEFAULT_RUNG` (routine, off at the
> default detail level) on the World channel, its prose detail shown verbatim — like the other
> command refusals that ride their verb's kind. The Telling, the opening card and the turn orb's
> hand-off producer all filter it out.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/StartingLoadoutPanel.gd` | The free-floating card — a head carrying ONE carry meter (`_carry_meter`), the food legend row under the bar, quiet ink (`_build_food_lines`) and the refusal line (`_build_refusal_line`), in that order; three columns (kits / resources / what the resources can build); a **band switcher** drawn only while two or more windows are open, a footer control that CLOSES the card (it sends nothing: every stepper press already orders) and its own reopen pill. **`AutoSizingPanel`, not `PanelCard` + `DockScrollFit`** (`panel-framework.md`): it is measured against the ROOM. **ONE NODE CARRIES BOTH STATES** — the card and the pill are two children and exactly one is visible, so one fit and one placement serve the expanded and dismissed states; the fit measures whichever is showing and `_place` centres the card in the room and puts the pill at the top of it. It renders a payload and emits five intents (`dismissed` / `reopened` / `band_selected` / `kit_count_changed` / `material_units_changed`) and holds no allocation of its own — the footer control emits `dismissed` like the ✕, `commit_requested` having gone with the deferred order. **A row's `+` is enabled from the ROW's own `can_add`**, never re-derived from the meter — kits weigh what they expand to, so one more two-item kit can overfill a pack a one-item kit still fits, and on a take the supply cap is per ITEM, so one kit row can be exhausted while the next is free. `_fit_expanded_height` ends with `_card.queue_sort()` (see "THE FIT IS TWO FRAMES"). `_column` draws NO caption for an empty note, which is what keeps the builds column from carrying a blank row where the other two carry a line |
| `ui/hud/StartingLoadoutController.gd` | The controller half, held by `HudLayer` as `_loadout`. **Holds ONE allocation PER BAND — what that band HOLDS, never a draft — every clamp, the carry remainder (`_order_load` / `_signed_carry`), a split's food (`food_brought_of`) and the "what this builds" arithmetic.** Ingests the campaign's half (`set_campaign_loadout` — the pick list, the craftable ids and the two carry weights; **the material pre-fill is read by nothing**), **the windows off the band roster** (`set_bands`, fed the player bands `HudLayer.update_band_alerts` has already filtered), the parsed equipment config (`set_equipment_config`) and the recipe book (`set_recipes`). `_write_pick` is the one press handler and the one sender; `_send_order` composes the line and queues it in `BAND_UNECHOED`; `revert_order` is the rollback `Main` reaches through `HudLayer.revert_starting_loadout`; `ingest_command_events` takes a server REFUSAL off the event stream and settles the oldest in-flight order, falling back to `BAND_HELD` when nothing else is out; `begin_snapshot` marks the snapshot boundary (see "A REFUSED ORDER IS AN EVENT ROW"). Relays `set_starting_loadout_requested` onto `HudLayer`'s and pushes its orb half through `attention_changed` |
| `ui/hud/hud_loadout_vocab.gd` (`HudLoadoutVocab`) | The vocabulary leaf — the wire keys, the words, the measured geometry, and the **swatch ring** (`apply_palette`, registered in `HudPalette.apply`) |
| `ui/hud/OpeningCardController.gd` | The OPENING HAND-OFF, held by `HudLayer` as `_opening` — see "THE OPENING CARD" below. Collects every tick-0 `narrative_beat` off `ingest_command_events` (de-duplicated by `tick\|label\|detail`, arrival order), takes `StartingLoadoutController.opening_grant_held`, and decides in ONE deferred `_resolve`. Owns the `OpeningCardPanel` node, parented into the HUD layer. `reset_world_state` re-arms it; `is_open` / `dismiss` back `HudLayer.is_opening_card_open` / `dismiss_opening_card`, which `Main.escape_claimant` probes BY NAME |
| `tools/ui_preview/chapters/opening_card.gd` | The opening card's chapter, appended LAST in `CHAPTERS` because every block starts on a world boundary. Three frames (`opening_card`, `opening_card_handoff`, `opening_card_late`) and 32 checkpoints: the window-then-beats frame, the button / scrim click / ESC hand-offs, once per world, a later splinter untouched, no story, a story a snapshot late, and a story after the window shut |
| `tools/ui_preview/chapters/starting_loadout.gd` | The preview chapter, LAST in `CHAPTERS` — seventeen frames and **180 checkpoints**, including the orb's two colours, the no-dead-space bound, the press-sends-an-order claims, the refused-send rollback, the TAKE arc appended after them (its food line, the food preview and the wire's echo, a refused take rolling the food back — `starting_loadout_take_refused` — and the back-to-back presses `_assert_card_holds_its_column` pins) and, last, the ADOPTION pair: its own band, a press made, the band's allocation re-published against a shrunken carry and taken whole, then two presses whose first echo must not pull the card back, and finally the REFUSED pair (`starting_loadout_refused`, `_refused_long`): a press reset by a `starting_loadout` event row, the line in warning ink, an earlier turn's row and a re-sent row ignored, the next press clearing it, and a long reason held to two lines; then A refused with B still in flight (`starting_loadout_refused_in_flight`: the line shown without the word "reset", B's pick kept, only B awaiting its echo, B's echo clearing the line) and B's echo plus A's refusal in ONE snapshot leaving no line. **Every fixture band publishes the last order this card sent for it** (`_held_kits`), which is what a server does. Its kit fixture is the **shipped nine-kit roster**, `none` included so the picker has something to drop. See `harness-ui-preview.md` |

## THE OPENING CARD — the world's first auto-open is the Telling's, and it hands off here

The sim says its opening on tick 0 as ordinary `narrative_beat` command events (the cold open, then
`guidance.food_and_the_split`, whose last sentence tells the player to choose what to carry). They
reach the Telling panel as every beat does, and they ALSO stand on a modal card
(`OpeningCardPanel`, the fork's look) **in place of this card's first auto-open**. Its one button,
`HudLoadoutVocab.OPENING_HANDOFF_LABEL` (*Choose what we carry*), opens the opening band's outfitting
card.

- **The hold is the controller's, the decision is `OpeningCardController`'s.** With
  `yield_opening_grant` set (`HudLayer` sets it when it wires the card), the world's FIRST auto-open,
  when it is a GRANT, renders the card, puts it at its reopen pill and emits `opening_grant_held`
  instead of expanding. Every later auto-open — a splinter's, grant or take — is untouched.
  `_first_auto_open_seen` is reset by `reset_world_state`, so the hold is once per world.
- ⛔ **THE TWO INPUTS ARRIVE IN EITHER ORDER, SO THE DECISION IS DEFERRED.** `Main` dispatches
  `populations` (the window) BEFORE `command_events` (the beats) within one snapshot, so the opening
  frame holds the window before the story has been read. Both inputs only record, and one
  `call_deferred` `_resolve` decides at the end of the frame: a held band and a story → the card; a
  held band and no story (a save loaded mid-opening, beats absent) → this card opens as it always did.
  A story arriving in a LATER snapshot while the held band's window is still open raises the card
  over this one (put away to its pill) and hands back to it; a story after the window shut raises
  nothing (`has_window`).
- ⛔ **THE OUTFITTING CARD IS NEVER LOST.** The held card is already rendered and at its pill, so
  the window is reachable even before the hand-off, and EVERY way off the opening card — the button,
  a click on the scrim, ESC (`Main.escape_claimant` → `ESC_OPENING_CARD`, right after the pause
  menu) — emits `handed_off`, which opens the held band.
- **Once per world.** `_shown` is cleared only by `reset_world_state`; a full snapshot re-sending
  the ring and the roster raises nothing.

## What the client owns, and what it must not decide

- **THE WINDOW'S LIFETIME IS THE SIM'S.** `PopulationCohortState.loadoutWindow.open` is the authority,
  per band: a band's card opens itself on the first frame it reads `true`, that band's state is
  dropped when it reads `false` (or the window is absent), and the whole surface goes when the last
  one shuts. The client never closes one on its own and never blocks End Turn.
- ⛔ **AN APPLY IS A REPLACEMENT, SO A SEND SHUTS NOTHING.** The order may be sent, revised and sent
  again as often as the player likes; **only the TURN ADVANCE closes the window and forfeits what is
  left**. Nothing latches "already sent", and the footer control is not a send at all — it collapses
  the card so the map is readable, and the rows, the reopen pill and the orb's row all stay.
- ⛔ **`open` IS THEREFORE NOT A SUCCESS SIGNAL.** The controller once held an `_awaiting_commit` flag
  and read a still-open window on the next frame as a REFUSAL. That was right while an accepted order
  closed the window and is now exactly inverted: under replacement semantics every SUCCESSFUL commit
  leaves it open, so the branch would post *"that order was refused"* after each one. It is gone, and
  nothing here may go back to inferring a refusal from `open`. A genuine refusal arrives on its own
  channel — see "A REFUSED ORDER IS AN EVENT ROW" below.
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

- ⛔ **THE CAMPAIGN SECTION CARRIES NO KIT PRE-FILL, AND `openingLoadout.materialDefaults` HAS NO
  CLIENT READER AND MUST NOT GROW ONE.** `materialDefaults` stays on the wire for the AI seat's
  grant-window pre-fill alone; the SIM applies the default spread when it makes the band, so it is
  already in `loadout_window.kits` / `.materials` by the time a card is drawn, and a client seeding
  from the campaign section as well would draw — and then ORDER — twice the gear the band holds. What
  the client still reads out of that section is the **pick list** (a grant's offered materials, in the
  profile's own order), the **craftable recipe ids** and the **two carry weights**
  (`item_carry_weight` / `material_carry_weight`). ⛔ **The window's counts arrive ALREADY FITTED to
  the band's `carry_capacity`** — the sim's `fit_to_carry` scales the kit and material defaults
  together, proportionally and floored, when it applies them. **Draw them as-is**; a second clamp here
  would disagree with the first, and with a spread comfortably inside the carry that disagreement
  would be invisible unless the individual counts are checked — which is why the preview asserts each
  one rather than the total.
- **The kit roster rides `SubsistenceSection.equipmentConfigJson`** — the picker parses that blob (it
  is the only HUD consumer of it; the Workbench is the other reader) and drops the carry-nothing entry
  **by its EMPTY `uses`**, never by matching `none`. A roster that renamed that entry is still
  excluded; one that gave it items rightly starts offering it.
- **A recipe's `inputs` and `work` ride `SubsistenceSection.recipes`** — the same book the crafting
  ledger and the knowledge screen read. `HudLayer.update_crafting_catalogues` fans it to a third
  reader rather than the picker re-deriving a cost from anywhere else.

## The TAKE mode — three things change on the card, and nothing else

`parent_band_id == 0` renders as a grant. A take reads the SAME carry meter against its own
`carry_capacity` (the band's whole carry); what changes is:

- **Each row is ALSO capped by what the home band can SUPPLY**, through the row's own `can_add`
  rather than a meter: a kit row against the EXPANDED item supply (`_take_kit_ceiling`), a material
  row against its `parent_material_supply` line — whichever of that and the carry binds first. ⛔ **The
  sim publishes only items some kit carries** — `bone_awl`, `loom` and `tanning_frame` are the three
  no kit `uses`, they are the knowledge-gated bench tools, and **shop equipment stays with the
  workshop that built it** rather than walking out with a splinter. So the supply is already the pile
  a kit row can draw down, and re-deriving that filter off the roster here would be a second copy of
  `EquipmentConfig::item_is_kit_carried`.
- **The resources column lists what the home band HOLDS**, not the profile's pick list — the pick
  list binds the grant and deliberately not a take, or a material a band crafted for itself would be
  untransferable to its own splinter. The swatch ring is indexed by a material's position in *this
  window's* list, so two cards can paint one material differently; each card is internally
  consistent, which is the property a key needs.
- **The copy says where the gear comes from.** `PANEL_SUBTITLE_TAKE_FORMAT` names the home band. The
  meter's `carry left` is the band's own pack room on either window, so it states no loss: **what a
  take leaves behind is not forfeited on the advance, it simply stays with the home band**, which is
  why the orb's take arms never say `unspent` (below).

> ### ⛔ A TAKE'S KIT CAP CANNOT BE DRAWN PER KIT ROW
>
> `equipment.json` maps kits to items almost one-to-one, and the single exception is **`sled`, used by
> both `big_game` and `trapping`** — so five of each needs TEN sleds and the rows are not independent.
> `_take_kit_ceiling` EXPANDS every other kit in the order through the roster's `uses` lists
> (`_expanded_items`, counting a repeated `uses` entry rather than de-duplicating it) and prices this
> row against what is left per item, which is the arithmetic `apply_starting_loadout` refuses on.
> A per-row cap would draw a ceiling the server does not honour, in both directions: it would offer
> `trapping` after the sleds were gone, and withhold it again after some were given back.

## The band switcher, and why the orb cannot do its job

**The card carries one tab per open window, drawn only for two or more** (`BAND_TABS_MIN_ROWS`) —
one window is the ordinary case and a tab naming the only band there is says nothing the title does
not. It is one of TWO ways to a second band's card, the other being that band's own orb row (see the
orb section below); it was the only one while a row carried no subject.

**The card names its band** (`PANEL_TITLE_FORMAT`, resolved through `HudFormat.band_name` — the
client's one naming rule). A split can leave two windows open at once, and a card headed only *"the
band"* would leave the player composing an order for a band they cannot identify.

## One colour vocabulary, and it means "material"

A swatch is a material's identity, resolved ONCE in `StartingLoadoutController._material_rows` and
drawn identically in the resources column, in the legend above the recipe list and on every recipe's
cost row — so the three read as one key rather than as three tables that can disagree.

`HudLoadoutVocab.SWATCH_COLORS` is a **RING indexed by a material's position in the published pick
list**, not a table keyed by id: the list is `start_profiles.json` data, so a profile offering a
material this build has never heard of must still get a swatch, where a table would hand it the
fallback ink shared with everything else.

**The carry bar speaks the same key**: each material's segment is drawn in its swatch ink, so the pile
reads in the colours the third column prices it in. **Nothing else in the panel is tinted by
identity** — the kits are ONE segment (`BUDGET_SPENT_COLOR`), interchangeable pairs of hands with
nothing to tell apart, because a second colour vocabulary beside the legend would be read as part of
it.

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
| `COMMIT_TOOLTIP` "…The window shuts and whatever is left of either budget is gone." | `CLOSE_TOOLTIP` "You can change this until you end the turn." |
| `COMMIT_CLEAR_LABEL` "Set out" — the face of the control that DEFERRED the order | `CLOSE_LABEL` "Done" |

**Do not restore an explanatory clause to any of them**, and note the deliberate absence of a full
stop on the two Ray rewrote.

**The strings the per-band arc and the carry arc (#732) added are written in that same register — one
short declarative, one fact each**, and none of them explains a model:

| const | reads | the one fact |
|---|---|---|
| `PANEL_TITLE_FORMAT` | `Outfit <band>` | which band this card is for, there being more than one |
| `PANEL_SUBTITLE_TAKE_FORMAT` | `What they take from <home band>.` | the gear comes out of the home band's ledger rather than being minted |
| `CARRY_REMAINING_FORMAT` | `12 / 78 carry left` | the pack room left — the remainder alone; a `28 of 30 packed` clause beside it would be the same fact subtracted from itself |
| `KIT_CARRY_COST_FORMAT` / `MATERIAL_CARRY_COST_FORMAT` | `<items> · 2 carry` / `2 carry each` | what one more of THIS row costs, said only where it is not 1 |
| `FOOD_BROUGHT_FORMAT` | `Food 12 of 14` | the food a split walks out with, against its share |
| `FOOD_FIXED_FORMAT` / `FOOD_BROUGHT_FORMAT` + `FOOD_ROOM_CLAUSE` | `Food 78` / `Food 12 of 14 · fewer tools leave room for more` | the food legend under the bar, quiet ink |

The per-band arc's `SUPPLY_REMAINING_FORMAT` (*"18 / 18 left at home"*) went with the take's supply
meters: a take reads the carry meter like a grant, and its supply caps live on the rows.

⛔ **AND THE FOOTER CONTROL MAY NEVER READ AS A SEND AGAIN.** `Set out` was true while it was the
only thing that sent; every press orders now, so that face would promise the one thing this card no
longer holds back — and a player who read it and never pressed it would believe they had lost what
they picked, which is the defect this arc closed. The preview asserts the face is `Done` AND that the
words `Set out` appear nowhere on the card.

**THE CARD MAKES NO FORFEITURE CLAIM ANYWHERE.** It was on the commit control's face
(`Set out — forfeit 17 kits and 2 units`) and again in the subtitle's second clause; both are gone.
The remainder is worth saying and is said ONCE, on the **turn orb**, which is the surface that already
counts down to the advance that causes it. The preview chapter asserts the word `forfeit` is absent
from the whole card, so a second copy cannot come back quietly.

## ⛔ THE FIT IS TWO FRAMES, AND THE SECOND ONE IS NOT OPTIONAL

Reported from the real client: **picking a single kit grew the card by roughly 400px**, all of it dead
space between the bottom of the kit list and the footer rule, with the three columns rendering
identically. Nothing in the content can do that — a pick changes an ink, a stepper value, a meter
label and one bar segment. Only the FIT can.

`refit()` waits one frame for the rebuilt body to be laid out, fits the WIDTH, **waits a second
frame**, and only then reads the height. The second wait is the whole point:

- `fit_width` resolves the card to its CONTENT's width, which is not `target_width` — the three
  columns want 916 against a 900 nominal — so the width really does move.
- **Applying a width does not lay the body out.** The container sorts on the next layout pass, so a
  height read in the same pass is the wrapping of a column that no longer exists.
- This card is full of `AUTOWRAP_WORD_SMART` labels — the subtitle, both column notes, every empty
  notice — and **an autowrapping label's minimum HEIGHT is a function of the width it was last laid
  out at**. Measured against a stale narrow width they approach one word per line, which is a card
  hundreds of pixels taller than its content with all of it as slack in the scroll region.

**`ComposeSheet.refit` shipped exactly this bug and was fixed exactly this way**
(`harness-ui-preview.md` → "a latent fit race was fixed", where it cost one frame 19px). It is
repeated here rather than shared because the two cards have different chrome and different collapsed
states.

**`_fit_pending` spans BOTH frames**, so a re-entrant `refit()` cannot interleave halves, and every
exit path clears it. **The collapsed branch is re-checked after the second wait** — a dismissal
landing between the two frames has already had its own `refit()` dropped by `_fit_pending`, so this is
the call that must notice; without it the reopen pill is left wearing a 900px panel, which is the
state the collapsed branch exists to prevent.

### ⛔ …AND THE CARD IS RE-SORTED AFTER THE HEIGHT IS APPLIED

A second race in the same fit, fixed by `_card.queue_sort()` at the end of `_fit_expanded_height`
(after `fit_to_content`). It predates #732.

- **A render rebuilds every autowrapping label, and for ONE frame they report inflated minimums** —
  measured at a width of nothing, the header asks 712px where it settles at 199, the body 1083 where
  it settles at 556.
- **A render that lands on the very frame the previous fit finished** let the card (a
  `PanelContainer`) sort its column against those minimums: 1857px of column in an 839px card.
- **Nothing re-sorted it when the labels settled.** The card's own size had not changed, so no resize
  reached it, and a SHRINKING child minimum does not queue a sort either. The scroll's expand flag
  swallowed the excess (a **1596px scroll in an 839px card**) and the footer was laid out past the
  card's bottom edge, for good.

**The cadence is press-settle-press** — a player clicking a stepper, each render landing right after
the last two-frame fit. Presses with no settle between them do not reproduce it: their renders share
one frame and one sort. The height read in `_fit_expanded_height` is the settled one, so that is the
point to have the card lay its column out against it.

**Pinned by the `loadout/take_back_to_back` claims** (`_assert_card_holds_its_column`): three presses
on the take card one `_settle` apart, then the scroll region must fit inside the card and the footer
must end on it. They fail without the `queue_sort`. Unlike the race above, THIS one the walk does
reproduce, because the presses are spaced to the fit's own two frames.

## ⛔ …AND THE WALK NEVER REPRODUCED IT, WHICH IS ITSELF THE FINDING

The fix above is reasoned from the code, **not from a staged failure**. Every reproduction attempt
came back stable at 761px: five kit rows and nine; a tall room and one shortened to 684 with the
internal scroll on; `_settle` and bare `process_frame`s; press, unpress and re-press. The harness's
`_settle` does `process_frame → force_draw → process_frame`, and a draw flushes the deferred container
sort the minimum-size read depends on — so this walk hands the card the very layout pass whose absence
is the bug.

**Proven rather than assumed**: with the two-frame split reverted to the original single pass, the run
is still green. Do not read the walk's silence here as evidence the card is correct.

What the walk DOES carry is a **bound**: `_assert_no_dead_space`, asked in every card state the
chapter renders. Its shape was got wrong once and the wrong version is worth knowing —

> The first version compared the panel against `PanelContainer.get_combined_minimum_size()` and
> **skipped itself when the internal scroll was on**. A card that has grown past the room's ceiling
> turns the scroll ON, so the skip fired *precisely when the defect was present*: with 400px of dead
> space injected it printed nothing at all and the run stayed green.

It asks the SCROLL REGION instead, where the space would actually be, one-sided so it needs no skip:
the region may be **shorter** than the body wants (the room's ceiling doing its job, the scrollbar
carrying the rest) but never **taller**. With 400px injected it fails at `343 px of slack` on exactly
the picked and spent states.

## The footer says how to get the card back, and when you cannot

> `The turn orb reopens this. Ending the turn closes it for good.`

Two short declaratives, one fact each, in the subtitle's quiet ink — it is guidance, not a warning.
`HudStyle.WARN` on this card is kept for the refused-order line and an
over-carry meter. A player who has dismissed the card has no other way to learn either fact.

⛔ **IT IS NOT THE RETIRED FORFEITURE CLAIM.** That one said COMMITTING shuts the window, which is
false — an apply is a replacement and the order may be revised as often as the player likes. This
names the TURN, which is true: `close_opening_window` is the only writer that ever clears `open`. The
two are one word apart in the source, so the preview asserts both facts AND keeps the `forfeit`
negative beside them.

**If it ever has to be trimmed, the SECOND sentence survives** — the way back is discoverable by
pressing things; the deadline is not.

**It costs no row and no height.** It fills the leading end of a footer the card was already spending
on the button, with the existing spacer still holding `Set out` hard right. **MEASURED: the card is
587px before and after, and the sentence renders on ONE line** at the card's width — no wrap.

## ⛔ A BAND VERB PUTS THE CARD AWAY

On a fresh game the card opens itself and floats over the right-hand column of a band verb's sheet,
which is where the Split sheet's stepper `+` and its confirm sit. So **every band verb that is
dispatched — Move, Scout, Deny, Trade, Split — collapses the expanded card to its reopen pill**, the
same `collapse()` its own Done/✕ reaches through `_on_dismissed`. Nothing is lost: every pick was sent
as it was made (`_write_pick` → `_send_order`), and the pill brings the card back.

- The edge is `BandPanelController.band_verb_opened(mission)`, emitted by `dispatch_verb` after its
  enabled gate. `HudLayer` relays it to `StartingLoadoutController.collapse_for_verb`; the two
  controllers never talk directly.
- **Only an EXPANDED card yields.** A card already at its pill, or a surface with no window open, is
  left as it is: `collapse()` on a closed surface would raise a pill for a window that has shut.
- Guarded end to end by `live_seat_probe`'s assertion 2 (`harness-live-seat.md`), which fails by name
  — *pressing Split did not put the outfit card away to its pill* — when the relay is cut.

## Two measurements that a screenshot is the only witness for

- **The column seam is 2px where the horizontal rules are 1px.** The HUD renders at a fractional
  canvas scale, so a 1px vertical line lands on a sub-pixel boundary whose coverage depends on where
  its column falls — and with three equal-stretch columns the two seams fall differently: the first
  rendered and the second vanished outright. A rule that draws on one side of a card and not the other
  is worse than no rule.
- **The unspent part of the carry bar is `HudStyle.LINE`, not `LINE_SOFT`.** The soft hairline ink
  measured within a few values of `PANEL_SOLID` and drew as no bar at all on a meter nothing had been
  spent from — the state the meter is most needed in.
- …and a **remainder of ZERO draws no segment**: `HudWidgets.build_composition_bar` floors every
  segment's stretch ratio so a one-unit segment stays a visible sliver, which turns a zero-count
  segment into a permanent sliver of *nothing left* on a full pack.

## The orb's rows — ONE PER BAND with an open window, in two colours

The loadout is a producer on the generic attention hub (`ATTENTION_KIND_OPENING_LOADOUT`), folded in
through `TurnOrbController.set_loadout_attention` — its own half for `_knowledge_attention`'s reason:
it is produced by a cluster whose ingest is not the orb controller's, so a snapshot that moves a
window without moving the band alerts must still be able to replace it alone. The half is guarded
against re-pushing an unchanged value, and **that guard matters MORE now that every band can have a
window**, not less: it is empty on every turn nobody is outfitting, which is most of a campaign, and
an unguarded push would cost a deep copy of the band half and an orb redraw on every snapshot of it.

⛔ **THE ROW IS PRESENT WHILE THE WINDOW IS, SPENT OR NOT.** The card is dismissible and this row's
`Open ▸` is the guaranteed way back to it, so a producer that fell silent once the carry was full
would strand a player who had finished picking, put the card away, and then wanted to revise before
ending the turn. What moves is the SEVERITY and the WORDING:

| window | state | severity | reads |
|---|---|---|---|
| either | **over its allowance** (`_over_allowance`): the carry remainder NEGATIVE, or — on a take — an item or material standing above the home band's supply | `warn` → `HudStyle.WARN` | `Band over its carry` / `Windmere — 6 carry over`; a take's supply overdraw adds the bare count nouns, `3 carry, 2 kits over` |
| GRANT | room left for at least the cheapest single unit | `warn` → `HudStyle.WARN` | `Band not fully outfitted` / `Brackwater — 1 carry unspent` |
| GRANT | the remainder is below the cheapest single unit (`_grant_is_complete`) | `ready` → `HudStyle.READY` | `Band outfitted` / `Brackwater — everything is picked` |
| TAKE | nothing ordered — reachable only by CLEARING the card, a fresh splinter drawing its default take | `warn` → `HudStyle.WARN` | `Band not fully outfitted` / `Thornhollow — nothing taken yet` |
| TAKE | an order standing | `ready` → `HudStyle.READY` | `Band outfitted` / `Thornhollow — 3 kits, 4 resources` |

⛔ **`not FULLY outfitted`, BECAUSE A BAND IS NEVER UNOUTFITTED ANY MORE.** The sim applies a band's
default outfit when it makes the band, so every band this row speaks for is already carrying gear;
what the warn arm reports is carry still to MINT, which the turn advance forfeits.

⛔ **"FULL" IS "NOTHING MORE FITS", NOT "EXACTLY ZERO".** The currency is a float with two weights, so
a grant is complete when its remainder is below the lightest thing a press can add
(`_cheapest_unit_load` — the smaller non-zero of the two weights): a 0.5 left over beside one-unit
materials is a full pack. The bare
*"Band not outfitted"* was true while the card held an unsent draft over an empty band, and would now
contradict the card beside it — which is showing the kits those people are holding. The READY arm's
`Band outfitted` / `everything is picked` is unchanged and is now true BY CONSTRUCTION rather than by
the card's say-so: the rows it counts are the sim's own.

⛔ **A TAKE NEVER SAYS `unspent`.** That word is on the orb because a grant's remainder is GONE on the
turn advance; supply a take leaves behind stays with the home band and is lost by nobody. So a take's
arms report what IS taken. A take is *outfitted* as soon as an order stands: it forfeits nothing by
leaving supply at home, so "everything drawn" is not a state anyone is working towards.

> ### ⛔ OVER THE CARRY IS NOT FULLY SPENT, AND IT IS THE FIRST ARM FOR THAT REASON
>
> The completeness test was `remaining <= 0` over a remainder **clamped at zero**, so a band holding
> more than its window allows passed it and the orb called it done. Reported from a live run, on the
> two-budget card #732 retired: a resources meter reading **`-6 / 22 left`** beside a row saying
> *everything is picked*. `<=` was doing double duty for *nothing left* and *less than nothing*, and
> the clamp is what hid the difference.
>
> `_over_allowance` asks the **unclamped** carry remainder (`_signed_carry`, which is what the card's
> own meter draws), and every clamped reader (`carry_left`) is written in terms of it — so the one
> place a negative can be seen is the one place it is asked about. **A state this row cannot word is
> exactly the state it must not paint green.**
>
> It covers a TAKE's supply as well: a supply that shrank under a standing take (an onward split)
> leaves the take standing on more than the home band holds (`_items_over_supply` /
> `_materials_over_supply`), with no meter to show it. The sim bug that produced the reported `-6` is
> fixed and **nothing here relies on that**.

### The row names its BAND, and `Open ▸` reaches it

Every window is one band's, so with two open the rows were identical and unattributable — *"Band
outfitted / everything is picked"*, twice, directly beneath two idle-worker rows that DID name their
bands. Two fixes, and they are separate failures:

- **The band leads the DETAIL** (`ATTENTION_DETAIL_BAND_FORMAT`), which is where the idle rows put
  theirs. The take arm dropped its own `from <home band>` clause with this: it was there only because
  two rows needed telling apart, and the subject's name does that properly. The home band is still
  named on the card, in the subtitle, which is where it belongs.
- **The row carries `HudAttentionVocab.ATTENTION_PANEL_SUBJECT`** — its band id — and
  `TurnOrbController` opens THAT band (`open_band`). Before, a row carried a kind and no band, so the
  press opened whichever band the card was already showing: both rows wear `Open ▸` whatever it
  reaches, so only pressing one can tell. The card's band switcher remains the other way in, for a
  player who never opens the popover.

It is **NOT `blocking` in either state**: closing the window is the sim's business, so the
`Advance ▸` footer stays live and this row only ever warns. It is NON-LOCATING and on
`ATTENTION_KINDS_WITH_A_PANEL`.

⛔ **THE REMAINDER IS NAMED WHATEVER THE CONTROL THAT SPENDS IT IS NAMED** — `carry`, the meter's own
word (`1 carry unspent`, `6 carry over`). It once read `2 units unspent` beside a picker column headed
`RESOURCES`, and a player asked what a unit was — the orb had invented a noun for a budget the card
already named. The preview asserts the row's DETAIL as well as its label (and the noun `unit` absent),
because both arms carry the same label whatever the remainder is worded as, so a label-only claim
cannot see this.

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
| in, per world | `CampaignSection.openingLoadout` → `opening_loadout` on the snapshot dict (`native/src/dict/campaign.rs`) — the pick list, the material pre-fill (for the AI seat, never read here), the craftable recipe ids, and the two weights an order is priced in, `itemCarryWeight` / `materialCarryWeight` → `item_carry_weight` / `material_carry_weight`. ⛔ **No `open`, no carry capacity and no kit pre-fill**: a window is a fact about one band |
| in, per band | `PopulationCohortState.loadoutWindow` → `loadout_window` on each cohort dict (`native/src/dict/population.rs`): `open`, `carryCapacity` → `carry_capacity` (the band's TOTAL carry, which the goods load is weighed against — not net of food), `foodShare` / `foodCarried` → `food_share` / `food_carried` (a splinter's full larder share and what the sim says has crossed; both 0 on the opening band), `parentBandId`, the two supplies and the rows. **`kits` / `materials` are what the band HOLDS — the sim applies a band's default outfit at its creation, so they are NON-EMPTY on a fresh band of either kind** — they carry the split's kit-denominated default take, which is what the card opens on; `parentItemSupply` lists only items some kit carries, so a bench tool is never offered as claimable. Decoded inside `population_to_dict`, so the full and delta paths get it from one place — the sim whole-diffs these tables and the change that matters is `open` going false on the turn advance. It reaches the picker through `HudLayer.update_band_alerts` → `set_bands`, off the roster that method already filters to the player's own bands (parties excluded: a detached party is those same people walking somewhere, not a band to outfit) |
| out | `set_starting_loadout <faction> <band> [kit <id> <n>]... [material <id> <n>]...`, built by `Main.format_set_starting_loadout` and emitted on **every stepper press**. **The band is positional and required**, and it is the durable `band_id` rather than the ECS `entity` — asserted by `cargo xtask command-guard`, which drives the card's real commit control. **The whole allocation every time, never a diff** — the verb fails closed and whole. An EMPTY tail is a real order (*spend nothing*), which is the one place that formatter departs from its neighbours and returns a line rather than `{}` |
