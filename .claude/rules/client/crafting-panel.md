---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{CraftingPanel,CraftingPanelController,hud_crafting_vocab}.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/crafting_bench.gd"
---

# Materials & Crafting — the panel that renders the sim's answer

`docs/plan_crafting_and_materials.md` §7. Its own free-floating surface, launched from the Band/City
panel header, showing one band's raw materials, what its kit costs to rebuild, and **why** a given
kit cannot be built right now.

## THE SIM RESOLVES THE REFUSAL; THIS PANEL RENDERS IT

Every refusal (`CraftOffer.reason` + `severity`), every shortfall number and every grade is resolved
sim-side and reproduced **verbatim**
(`.claude/rules/core_sim/crafting.md` → "On the wire"). It is the same rule `kitTiers` exists to
enforce, and it is not a style preference: the derivation needs the band's batch readings, the tool
that bounds a material, the recipe's grade seams and the item's wear quantum, and a client cannot
join those correctly from what rides the wire.

**`reason` + `severity` are the contract, not `available`.** *"Not needed yet"* is a shrug and
*"Short 4.9 bone"* is a problem; they are different strings with different severities precisely so
they can read differently, and a client deriving both from a boolean cannot tell them apart. The
panel therefore renders the reason as it arrives, tinted by the published severity, and composes no
sentence of its own — least of all *"cannot craft"*.

## The three readout rules the design is built on

- **THE LEDGER CARRIES NO CONDITION COLUMN — its four are Item · Tier · Rebuild costs · action.** How
  worn a thing is has ONE home: the Band panel's WORKFORCE role cards, which state the condition of
  the item behind each kit beside the role that kit sets, off `kit_item_conditions`. That is where a
  player asks *"how worn is my gear"*; this panel answers *"what does it cost to rebuild"*, and a
  second copy of condition here is one fact in two places free to disagree. What the ledger keeps of
  the item is the **tier chip** — the grade, and for an item the band owns none of, the fact that it
  owns none.
- **OWNERSHIP IS `count`, NEVER `remaining == 0`.** A batch that runs out of units is REMOVED, so a
  worn-out item and one the band never made both read `remaining 0` — which is why the chip is keyed
  off `count` and says what owning none MEANS (*Bare hands* for a kit, *Not made* for a tool) rather
  than re-deriving a step-down. **It is a statement of ownership, and that is all this surface owes:**
  worn-out and never-made are a distinction about WEAR, so they are told apart where wear is reported.
- **SORTED BY URGENCY — worn first, untouched last, DIMMED rather than hidden.** The player's real
  question is *"what am I about to lose?"*, so the ledger opens on the answer; a kit you own and
  never use is information too. The key is the published `life_severity` rank, then `remaining`
  ascending, so a spent row leads its severity band. **Condition therefore decides the ORDER while
  the table prints none of it** — a ranking is a different use from a readout and restates nothing.

> The design's fuel-gauge rule is unchanged and still governs every surface that *does* show
> condition (`docs/plan_crafting_and_materials.md` §7): a spear at 34% is exactly as deadly as one at
> 100%, so condition reads as a discrete chip in **turns left** and never as a percentage. What moved
> is which surfaces it applies to, not the rule.

### The shrug's dimming is "nothing spent", not "comfortable"

`_is_shrug` requires a NEUTRAL offer severity **and** an item at full condition
(`remaining >= 100`). The first cut required neutral + a `healthy` life severity, and that dimmed a
sled at 42 turns left and a husbandry kit at 28 — real gear in active use, reading as though there
were nothing to think about. `healthy` covers most of an item's life; the shrug is the item that has
never been touched.

**It reaches "untouched" through the NUMBER, not by matching the wording.** `remaining` is published
on a 0–100 scale and every shipped tier's `starting_durability` is 100, so full condition is exactly
the sim's own `Untouched`. Matching the string would be parsing a resolved wording this panel may
render and must not read as data. A tier shipping a lower `starting_durability` simply never dims,
which is the conservative direction — nothing is hidden, one row merely reads at full strength.

## The ledger row is a JOIN, and neither half can answer alone

`CraftOffer.outputItemId` is the key. The **offer** supplies the name, the group, the refusal and the
shortfalls; **`equipment_batches` grouped by `itemId`** supplies the tier, the grade and the count
(plus `life_severity` + `remaining`, which the urgency sort ranks on and nothing renders); the
**recipe book** supplies the rebuild cost. That is why the table is built from all three rather than
off any one array.

**Three published `equipmentBatches` fields are read by NO client surface** — `life`, `quanta_left`
and `quantum_noun`, the condition WORDING and its unit. They are still correct and still on the wire;
the role cards answer condition off `kit_item_conditions` instead, so nothing consumes them today.

- **The cost cell prefers a shortfall's `required` over the recipe's input amount** where the offer
  publishes one: it is already net of the bench tool's material efficiency, and it is the number the
  refusal beside it was computed against. A short material is tinted so the eye finds it without
  reading the button.
- **The tier chip when the band owns none is keyed off the published `group`** — `Bare hands` for a
  kit, `Not made` for a tool — and a STOCK recipe's cell states what a pass yields instead
  (`→ 6 cordage`, from the recipe's outputs). The wire carries no chip for a tier that does not
  exist, because there are no units at a tier; what the panel says is what owning none MEANS for that
  group, which is a statement of ownership rather than a re-derived step-down.
- **The Item cell's second line is a join of published fields, never an authored table**: a TOOL
  names the material it bounds (`materials[].tool_item_id`), a STOCK recipe names the characteristic
  its input is judged on (`inputs[].reads_axis`), and a KIT row names the craft that makes it, at the
  sim's own `display_name`. A join that finds nothing renders no second line.

## MAKE IS THE ASSIGNMENT — which is why there is no Crafter role card

Pressing **Make** emits `set_bench <faction> <band> recipe <id>` and the sim draws idle workers onto
the job; the running row's button reads *On the bench* and is spent; the `− n +` stepper emits
`bench_crew <faction> <band> workers <n>`. **One job at a time**, so the panel never has to explain a
queue.

Scout and Warrior are standing roles with nothing to point at; crafting always has a subject — the
thing being made — so it is staffed at the bench like a worked source. That also sidesteps a measured
constraint: the Band panel's WORKFORCE zone reads 326px against a 275px box and its column split sits
*exactly* on `band_panel_preview`'s levelness floor (`band-city-panel.md` → "The band zone's tier
reads the whole STACKING BUDGET"), so a third role card there would have to be paid for by
re-authoring that split.

**`set_bench` sends the recipe and NOT a crew.** The grammar admits `[workers <n>]`, and omitting it
is the point: a client-chosen crew would be a second answer to a question the sim already answers.

### The stepper's ceiling is `idle + the crew at the bench`, which is NOT the band's idle count

**IDLE MEANS IDLE.** `PopulationCohortState.idleWorkers` is `working_age − assigned − bench.workers`
— the sim's `BandWorkforce::idle()`, the one seam every head-count reading resolves through,
including the clamp `assign_labor` makes — because a band's people are spent on the labor assignments
AND on the bench, and a bench crew is not a `LaborTarget`. `HudBandLaborState.effective_idle` makes
the same subtraction over its optimistic overlay, so the WORKFORCE zone's three "n idle" sites and
`FactionRollup`'s faction total report hands that are actually free.

**The stepper asks a different question and so takes a different number**
(`HudBandLaborState.benchable_workers` = `effective_idle + bench.workers`, the sim's
`BandWorkforce::benchable()`). Re-crewing a bench does not require freeing its crew first — those
hands stay put while the job is swapped — so a ceiling of `idle` alone would pin the stepper to the
crew already standing there and make `+` dead on every running bench.

> `effective_idle` STAYS a local computation rather than reading the published `idleWorkers`: the `+`
> steppers gate on an OPTIMISTIC idle, so a just-issued assign counts before the turn resolves, and
> that overlay (`pending_labor`) is exactly what the wire's answer cannot carry. The bench crew has no
> such overlay — a `bench_crew` edit shows on the next snapshot — so the published crew is the honest
> third term.

## It is the FREE-FLOATING case, hence `AutoSizingPanel`

`panel-framework.md`'s test: the card is measured against the viewport, not against a dock's
remaining height, so `PanelCard` + `DockScrollFit` is the wrong half of the pair and would misbehave
silently. Both axes are fitted explicitly, the node being a plain `Control` that no child minimum
ever reaches.

**THE VIEWPORT IS NOT THE ROOM ONCE ANYTHING IS DOCKED.** A docked panel reserves a strip of one
screen edge and the map and the HUD both live in the remainder (`panel-framework.md` →
"Reserved-edge docking"); a free-floating card measured against `get_visible_rect()` is measured
against a rectangle nothing else in the client is using, so it grows over the dock and over whatever
overlays the edge it just claimed. The seam is **`AutoSizingPanel.room_bounds`** — the Control the
registry has ALREADY inset, i.e. the HUD's `LayoutRoot`, handed down `HudLayer` → `setup(host,
band_labor, room_bounds)` → the panel node. `available_room(margin)` and `fit_to_content`'s ceiling
both come off that one rect, so the placement and the height fit cannot disagree about how much room
there is. It is opt-in and `null` keeps the raw viewport, which is right for a card that IS a
reserver: the Inspector reserves its own edge and must be measured against the whole window.

**Shrinking the card costs nothing but a shorter scroll viewport** — `fit_to_content` turns the
internal scroll on exactly when the content did not fit the room it was given, so a ledger that
outgrows a short room scrolls rather than the card growing out of the room.

**THE CARD IS PARKED AT THE TOP OF THE ROOM BEFORE THE HEIGHT FIT AND CENTRED ONLY AFTER IT.**
`fit_to_content` derives its real ceiling from `global_position.y` — the room BELOW the card — so
fitting a card that is currently centred at its previous (small) height throws away everything above
it. Measured: a ledger with room for every row was clamped to four of them, by exactly the height its
own centring had put above it.

## Launching it: one button in the header, and it carries no subject

An `_make_icon_button` beside the cycler and the dock chooser — the same builder the collapse toggle
and the `◀`/`▶` arrows use. **The header is subject-independent chrome**, so ONE button serves a band
page and the faction page, and the band zone's 300px budget is untouched.

Which band it opens on is `BandPanelController`'s answer, not the panel's: from a band page that
band, from the faction page the last band loaded — which is sitting on the model already, because
`render_faction` deliberately never touches `_panel_band`. It cannot be empty either: `render_faction`
returns early on an empty roster and `refresh_snapshot` hides the whole panel at zero bands, so a
visible header always has a band behind it.

**The two controllers never hold each other.** `BandCityPanel.crafting_requested` →
`BandPanelController.crafting_requested(band)` → `HudLayer` → `CraftingPanelController.toggle_for`,
and the panel's own two command signals relay back out through `HudLayer` to `Main`. That mediation
is the coordinator pattern `hud-modules.md` requires; a direct edge between the two controllers is
the thing it exists to prevent.

## The subject is a BAND ENTITY, re-resolved every snapshot

Holding the dict would freeze the bench's progress and the ledger's life the moment a turn ticked;
holding the entity and looking it up in `player_bands()` keeps the panel live, exactly as
`BandPanelController._resolve_panel_band` keeps the dock live. A band that leaves the roster CLOSES
the panel rather than stranding it on a band that no longer exists — and **a world change closes it
too**, because a new world renumbers entities from the same low range and a panel left open would
silently re-resolve onto a different band's bench (`HudLayer.reset_world_state`).

**The catalogues live on the controller, not on a state model.** `hud-modules.md`'s test is whether
two or more clusters read a field; exactly one reads these. They are ingested as ONE call
(`update_crafting_catalogues`) for the reason the kit roster is: a recipe book without its materials
renders a rail with no craft tracks and costs in materials the panel cannot name. The dispatch is
gated on **`craft_knowledge`**, the one of the four that moves in play — a craft is LEARNED, the
other three are per-world constants — and the other three are passed as `null` rather than `[]` when
the frame does not carry them, so absence means unchanged instead of "the world has no materials".

## PROVENANCE IS DEFERRED, and when it lands it is a POPOVER

Where a batch came from and what it earns per turn — *Hunted — Mammoth, 8 turns ago · +0.42/turn* —
is a second question, and putting it on the rail row made the rail a wall of prose that buried the
numbers a recipe actually reads. It belongs in a `DisclosureController`-style popover off the
material row, the idiom this client already has for Food / Morale / Growth / Trade / Kit.

**It must be a popover rather than an inline expansion, and `band-readouts.md` records that as a
correctness rule rather than a style one**: expanding inline grew a label *after* its zone had picked
a height tier, and the `clip_contents` host silently sliced the rows beneath it. A Window cannot
change a zone's height.

## What the rail deliberately does not show

One group per material the band HOLDS, one row per batch, its amount and its characteristic **bands**.
No provenance, no per-turn rate, and **no catalogue of materials the world does not yet contain** —
"On hand" means on hand.

**THE BAND RATES THE AXIS, NOT THE MATERIAL.** `tough: excellent · supple: poor` is a mammoth hide —
excellent at being tough, which is right for a sled and wrong for cordage. It is not a claim that the
hide is good, which is what lets ordinary quality words coexist with there being no best hide. Which
chips read as a strength and which as a weakness comes from the ENDS of the published legend
(`characteristic_bands`, ascending), never from a cut point typed client-side — the sim owns those,
and a client with its own would disagree with the word beside them.

The group header carries the material's craft and its track: the meter is
`progress / completion_threshold`, both published, so the client draws no scale of its own, and the
craft's name is the sim's `display_name` because the client never maps a craft id to English.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/CraftingPanel.gd` | The surface (`AutoSizingPanel`): header + `Band:` picker/cycler, the 250px material rail, the bench well and its crew stepper, and the four-column ledger (Item · Tier · Rebuild costs · action) in three groups. Emits `closed` / `band_selected` / `cycle_requested` / `make_requested` / `crew_changed` and holds no snapshot state — `render(payload)` is its whole input. Builds its rows as a VBox of `HBoxContainer`s rather than a `GridContainer` because a GROUP HEAD spans the width and a grid cannot span. **The ACTION column is sized by the refusal under the button, not by the button** — a published `reason` is a whole clause, and it was the column's 132px that wrapped every one of them onto two lines and inflated every row |
| `ui/hud/CraftingPanelController.gd` | `RefCounted` controller: owns the panel node (parented into the HUD CanvasLayer — a `RefCounted` cannot `add_child`), holds the four per-world catalogues, resolves the subject band by ENTITY each snapshot, and turns the panel's five signals into `set_bench_requested` / `bench_crew_requested`. It also carries the **room** the card is bounded by (`HudLayer.layout_root`, handed in through `setup` and set on the panel as `room_bounds`) — handed down rather than looked up, a `RefCounted` reaching into its host's tree being the coupling this pattern exists to avoid. `HudLayer` holds it as `_crafting`, relays both signals, and refreshes it from the same per-snapshot seam the Band/City dock uses |
| `ui/hud/hud_crafting_vocab.gd` | The vocabulary leaf (`HudCraftingVocab`) — ALL-`const`, zero funcs, zero vars: the wire keys, the chrome words, the severity→tint table and the geometry measured off the prototype. **Nothing here is a refusal, a grade or a shortfall** — those are the sim's — and there is no condition wording at all, that reading belonging to the role cards. `BandCityPanel` reads its `LAUNCH_GLYPH` / `LAUNCH_TOOLTIP` back, so the header button and the panel it opens cannot drift |
| `tools/ui_preview/chapters/crafting_bench.gd` | The harness chapter: the prototype's own band (every ledger state at once), a bare band with an idle bench, the bench-bound band that separates idle from benchable, and the RESERVED-EDGE state. Its assertions are the claims no picture can carry — the refusal rendered verbatim with its number, the urgency order, the shrug dimmed **while a used row is not** (asserted as a pair, since a panel dimming every neutral row satisfies the first half alone), and the ownership/condition pair: the tier chip states ownership **and** no condition wording reaches the ledger, over a fixture that deliberately keeps publishing `life` so the negative is not vacuous. **`crafting_panel_reserved_edges` is the height bound**: `Main` is never instanced here, so the chapter pushes a left and a bottom reservation into `Hud.set_reserved_inset` by hand (the `event_dock` chapter's idiom), asserts the card's rect sits inside `layout_root`'s on both axes with its ledger scrolling internally, and releases them again. **The "it got shorter than state 1" claim is the vacuity guard** — every rect test passes trivially on a card that already fitted |

---
