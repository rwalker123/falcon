# The Knowledge Screen

**Status: SHIPPED.** All four slices of §9 have landed — A (the overlay migration), B (the screen and
its launcher), C (the two attention rows and the System note's retirement), D (the
`ready_for_improvement` channel). Sections carrying an **AS BUILT** note say where the implementation
corrected the design; the engineering rationale lives in `.claude/rules/client/knowledge-panel.md`,
`turn-orb.md`, `band-readouts.md` and `overlay-channels.md`, which is where to edit it from here.
Prototype: `docs/knowledge_screen_ux_proposal.html` (the eight-option comparison it came from is
`docs/knowledge_visibility_ux_proposal.html`).

The problem: the intensification ladder's knowledge is earned by practice, announced once into the
event dock's System channel, and otherwise invisible. A player is never told a track finished, never
told what the new knowledge lets their hands do, and — once the tree grows — never told they are
sitting on discoveries they have not spent.

---

## 1. Decisions — settled, do not re-open

| | Decision |
|---|---|
| **Surface** | A **free-floating panel**, launched from the Band/City header's **action bar**, beside `⚒`. Same launcher pattern as the Crafting panel (`ACTION_CRAFTING` → `crafting_requested`). |
| **The Know tab** | **Removed.** The faction page returns to three tabs: Faction · Work · Parties. |
| **Layout** | Domains as columns, each holding a short ladder. **Not** a graph — the rung engine models ~4 rungs and grows by adding branches, so the screen never needs pan, zoom, or edge routing. |
| **Jump on knowledge rows** | **Dropped.** A discovery unlocks a verb across the whole map; there is no one hex, and `focus_on_tile` needs one. Knowledge rows are non-locating and open the screen. |
| **The bottom-bar slot** | **Cut.** The action-bar pip carries the unspent count. |
| **The overlay picker** | Moves out of the Inspector onto the **minimap border**. The Inspector is a modding tool and is not normally on screen. |
| **The `⌃` glyph** | Rejected. Ships as a drawn icon on the `_stage_glyph_sprite` seam. Direction: the **cairn**. *(As built, issue #581: the cairn ships from `ui/HudSprites.gd`; the launcher takes it on `Button.icon` rather than that seam, which is for a Label — see §8.)* |

---

## 2. What actually exists to render

**The prototype shows 36 nodes; the game has 8.** That is deliberate — it was built to prove the
layout survives the tree it will have. What ships must render **what exists** and gain domains as
they gain nodes. **Never draw an empty domain column.**

| Domain | Nodes today | Source |
|---|---|---|
| **Land** | `cultivation`, `seed_selection` | `intensification_knowledge` (per-faction `{track: 0..1}`) |
| **Herds** | `herding`, `penning`, `foddering` | same |
| **Craft** | `tanning`, `weaving`, `bone_working` | `craft_knowledge` (`CraftingPanel.PAYLOAD_CRAFT_KNOWLEDGE`) |
| Routes / War / Telling | none | not built — the columns appear when the branches do |

`Settling` (sedentarization) and `Discoveries` (discovered sites) are **not** knowledge nodes — they
are not earned by practice and unlock no verb — so neither moves to the screen. But the Know zone is
their current home, so deleting the tab has to rehome both: **Settling → the Faction zone** (it is
what the faction has *become*, which is that zone's question), and **Discoveries → the Faction zone**
too, beside the `◈ Discoveries N` count that already reports them.

### Node states

`known` · `learning` (0..1 progress) · `not begun`. A track at `0.0` is **shown, greyed** — the
current `_build_knowledge_block` skip is what makes the ladder invisible to a new player, and
removing it is half the value of this arc.

### The "unspent" state

**Derived, not persisted, and not "never used".** Nothing in the sim or the client records that a
verb has ever been exercised, and inventing a persisted latch buys a claim we cannot honour across a
reinstall. Define it as **nothing is using this right now**:

- **Ladder knowledge** — no source the faction works currently stands on the rung it unlocks
  (`forage::patch_rung` / `fauna::herd_rung` against the rung's `unlock_knowledge`).
- **Craft knowledge** — the faction holds, or is making, nothing made of that craft: no recipe of it
  whose output the bands carry (`count` / `amount`, never `remaining`), and none on a bench.

> **AS BUILT (Slice B): the craft test is what the faction HOLDS, not what the ledger LISTS.** The
> line above read *"no recipe requiring that craft appears in the faction's kit ledger"* and has been
> corrected, because a plan that names the rejected test is a plan the next slice implements. The
> crafting panel publishes **ONE ROW PER RECIPE, ALWAYS** — that is its stated contract — so a
> recipe's mere presence is true of every craft on every turn and would answer *in use* for all of
> them forever. The bench arm is the other half: without it, a faction building its first loom reads
> unspent for the whole time the loom is being built.

This needs **zero new state** and is arguably the better signal: it re-surfaces if the player
abandons the thing. The label follows the meaning — *"Known · nothing is using it"*, not
*"never used"*. If a true never-used record is wanted later it needs a sim field; that is a
different slice.

---

## 3. The screen

```
┌ What your people know ─────────────────── 5 known · 2 learning · 1 unknown · 2 unspent ┐
│ Show  [All 8] [Learning now 2] [Close 1] [Ready · unused 2] [New this turn 1]          │
├──────────────────────────┬──────────────────────────┬───────────┬────────────────────┤
│ LAND            ladder   │ HERDS           ladder   │ CRAFT fan │  <detail pane>     │
│ │● Cultivation    known  │ │● Herding        known  │ ● Tanning │                    │
│ │◐ Seed Selection   62%  │ │● Penning        known  │ ◐ Bone 40%│                    │
│                          │ │◐ Foddering        71%  │ ○ Weaving │                    │
└──────────────────────────┴──────────────────────────┴───────────┴────────────────────┘
```

- **Ladder domains** draw a rail down the left; **Craft draws none** — a craft is learned by working
  its material and gates recipes, not a next rung. The shape is a property of the domain descriptor,
  not a branch in the renderer.
- **Filters** are counts over the same list: `all`, `learning`, `close` (≥60%), `unused`, `new this
  turn`. Non-matching nodes dim rather than disappear, so the shape of the tree stays legible.
- **Detail pane** on select: *what it lets you do* (or *what it changes*, for a knowledge that
  re-colours rather than unlocks) · *where, now* (a live count that opens the relevant surface) ·
  *needs* (for a locked node) · *how it is learned*.
- **Nothing is clickable in the tech-tree sense.** No queue, no research order, no pathing. If it
  reads as a planner it has taught the wrong thing.

### Vocabulary

`KNOWLEDGE_UNLOCK_NOTES` in `FactionReadouts` is already the "what it lets you do" copy and should
become the standing text rather than a fire-once notification string. The **PRACTISE** line ("you
learn Penning by working herds you have already tamed") exists nowhere in the client today — it is
in a Rust doc comment on `intensification_ladder.json`. It has to be authored as client vocabulary.

---

## 4. The launcher

- Register a second action beside `ACTION_CRAFTING` in `BandCityPanel` — same
  `{id, glyph, tooltip, enabled}` descriptor, same `action_invoked` signal, same three mounts
  (bar / subject row / collapsed rail).
- The **pip** on the button carries the unspent count. It is derived fresh every push and **does not
  clear when the screen is opened**.

> **AS BUILT (Slice B): OPENING THE SCREEN DOES NOT CLEAR THE PIP, and this line said it did.** What
> clears an unspent count is USING the knowledge; a pip that went quiet on a *look* would tell the
> player they had dealt with something they had not. `unspent_count` is derived rather than latched,
> so it goes away exactly when a source starts standing on the discovery — the honest trigger, and
> the one the state's own definition already gives. **Slice C is implemented from §4 and §5**, which
> is why the correction lands here rather than only in the rule file.
- **Delete the Know tab**: the faction page drops to three zones. `FactionRollup.build_knowledge_zone`
  and its callers go with it; its Settling and Discoveries blocks are rehomed per §2.

---

## 5. The attention rows

Two producers, both **non-locating** (`x < 0`, so they render `Open ▸`):

1. **A freshly-learned discovery** — one row per track completed this turn.
2. **The unspent backlog** — ONE aggregate row (`"2 discoveries unspent"`), never one row per
   discovery, or 36 unlocks over a campaign become 36 rows.

`TurnOrbController._on_turn_orb_panel_requested` branches on exactly one kind today (`decision`).
Add a knowledge kind that opens the screen on the `unused` filter. **A non-locating kind with no
branch renders an affordance that does nothing** — `hud_attention_vocab` says this outright, and
`crew_handoff` deliberately wears no affordance for that reason.

The existing one-shot System note (`FactionReadouts._announce_knowledge_unlock`) is superseded by
producer 1 and should be retired, not left to double-report.

> ## ⛔ AS BUILT (Slice C): **ONE PRODUCER SHIPPED, NOT TWO. Producer 2 was cut.**
>
> Engineering rationale in `.claude/rules/client/turn-orb.md`; four corrections to the paragraphs
> above.
>
> **PRODUCER 2 WAS BUILT, RENDERED, AND REMOVED BEFORE THE ARC LANDED — do not re-add it.** The orb
> is for EVENTS and for LOSSES IN PROGRESS; an unspent discovery is a STANDING CONDITION, so its row
> never went away and the orb never returned to its calm all-clear pulse. Measured: it moved 400 of
> the render harness's frames simply by adding one to the count badge on every frame that draws the
> orb. A permanently-lit attention hub teaches the player to stop looking at it, which costs more
> than the nudge is worth — and **§1 had already given the unspent count a home**, the action bar's
> PIP, which is mounted on all three of the Band/City panel's layouts including the collapsed rail
> and clears on the same honest trigger. The row was the same standing fact on a second surface, and
> on the one surface whose whole value is being quiet when nothing needs you. The player has also
> already been told: producer 1 announced the discovery the turn it landed.
>
> `turn_orb.gd` asserts the ABSENCE against a faction sitting on four unspent discoveries, so
> re-adding the row fails a test rather than quietly relighting the orb.
>
> **The surviving producer takes the branch, and it opens on `new`, not `unused`.** The paragraph
> above names `unused` because it was written for producer 2's row; producer 1's row names a
> discovery, so it lands on the list holding it. That still needed a new entry point —
> `open_on_filter` — because the live filter is controller state that survives a close, so `open()`
> reopens on whatever the player last set.
>
> **THE ORDERING WAS THE REAL WORK, AND IT IS NOT A COMMENT.** `build_band_attention` runs thirty
> lines before the turn diff the producer reads is rolled, so a producer built beside the band ones
> names the PREVIOUS turn's discovery in a row that renders perfectly plausibly. The knowledge row is
> a THIRD registry half instead, filled by one `HudLayer` seam that rolls the diff and pushes the pip
> and the row on adjacent lines — which also makes it correct on a delta carrying knowledge but no
> populations, one that never reaches `update_band_alerts` at all.
>
> **RETIRING THE NOTE DID NOT RETIRE ITS COPY.** `KNOWLEDGE_UNLOCK_NOTES` is the knowledge screen's
> *"what it lets you do"* line (`KnowledgeRoster` reads it); `KNOWLEDGE_UNLOCK_LABELS`,
> `_knowledge_announced` and `FactionReadouts`' last Callable injection went. **A completed discovery
> is now announced on the turn orb and nowhere else — it leaves the event log entirely**, which is
> this section's intent rather than a side effect.

---

## 6. The overlay migration — modular, and no traces left

**Two requirements, and the second is the one that costs care.**

### 6a. Move it

`Inspector → Map → OverlaySection` moves to a picker mounted on the **minimap's top border**
(`MinimapPanel`): a small button that opens a popover holding the channel list, the selected
channel's description, its legend, and the `stub data` marker.

### 6b. Do not carry the god file with it

`ui/inspector/OverlayPanel.gd` is 308 lines doing four jobs, and **two of them grow a branch per
channel**. Moving it as-is relocates the problem. Split it:

| New module | Kind | Holds |
|---|---|---|
| `OverlayChannels` | all-`const` + `static`, **a registry** | The **client-side** channel descriptors — `""` (No Overlay), `terrain_tags`, and later `ready_for_improvement` — each `{key, label, description, legend_kind, available}`. Merged with the server-published `overlays.channels` payload. **Adding a channel is one registry entry**, exactly as `WorkbenchPages.PAGES` works for pages. |
| `OverlayLegend` | all-`static`, stateless | Renders a legend from a descriptor + `MapView.current_overlay_legend()`. **Generic**: a `ramp` channel gets that channel's own legend rows; a `facts` channel gets the count lines its provider returns. No channel is named here. |
| `OverlayPicker` | the widget | The list + the legend mount + the selection, pushed to `MapView.set_overlay_channel`. Knows no channel by name. |

> **AS BUILT (Slice A): the legend source is `current_overlay_legend()`, NOT `overlay_stats_for_key`.**
> The row above said `overlay_stats_for_key` and has been corrected, because a plan that names the
> rejected source is a plan the next slice implements. `overlay_stats_for_key` reports min/avg/max over
> EVERY tile, and the map-wide minimum for `pasture` and `forage` is the sea — the exact reading those
> two channels' own legend builders exist to avoid (`.claude/rules/client/overlay-channels.md` → "Zero
> pasture is NOT low pasture"). `MapView` already publishes a per-channel legend that gets this right,
> so the renderer takes those rows and there is ONE producer for the map's legend and the picker's.

**The two hardcoded blocks that must not survive the move:**

- `_update_overlay_channels`'s inline `terrain_tags` label/description/availability block → a
  registry entry with an `available` predicate.
- `_refresh_overlay_panels`'s **Culture and Military placeholder tabs** — two hand-written titles and
  descriptions for two channels, in a function that would grow a branch per channel forever. These
  are Inspector-era stubs; their content is exactly what the generic legend produces. **Delete them
  and the `OverlayTabs` subtree.**

`MapView.set_overlay_channel` also special-cases `terrain_tags`. Leave the render path alone in this
slice, but do not add a second special case — a new channel must be a registry entry plus whatever
raster/derivation it needs, never an `if key ==` in `MapView`.

### 6c. Cleanup checklist — no traces

- `Inspector.gd`: remove the `overlay_panel` member, its `reset()` / `apply_typography()` /
  `set_map_view()` forwards, and the `overlay_panel.ingest(...)` call. **Keep** the
  `terrain_palette` / `terrain_tag_labels` / `crisis_annotations` side-routes in `_ingest_overlays` —
  those belong to Terrain and Crisis, not to the overlay panel, and the function survives for them.
- `InspectorLayer.tscn`: delete the `Map/MapVBox/OverlaySection` node and its `OverlayTabs` subtree.
- Delete `ui/inspector/OverlayPanel.gd` and its `.uid`.
- `.claude/rules/client/overlay-channels.md`: its `paths:` frontmatter names
  `ui/inspector/OverlayPanel.gd` — update to the new modules, and move the channel table's home with
  the code.
- `.claude/rules/client/inspector-panels.md`: drop the OverlayPanel row.
- Check the harnesses: `ui_preview` / `menu_preview` chapters that render the Map tab will change
  frames. Re-read, do not just re-baseline.

---

## 7. The `ready_for_improvement` channel

**"Climb a rung" is the intensification arc's INTERNAL vocabulary** — `RungGates`, `SourceForecast`
and this plan keep it. It does not survive contact with a player: a hex asked to "climb a rung" is a
metaphor the game never taught. The player-facing word is **improve**, which is why the channel
shipped as `ready_for_improvement` and not as the `ready_to_climb` the slice was designed under.

The map already draws `⌃` on any source that can climb a rung
(`SecondaryMarkerRenderer`, driven off `RungGates` + the faction's knowledge row). What is missing is
the **aggregate** view and the teaching.

- A registry entry with a `facts` legend: *"104 sources · 61 patches, 38 herds"* + the nearest
  unworked coordinate.
- **The unlock never lights the map.** The attention row states a count; the player who wants to see
  them all turns the channel on. Nothing anywhere gets a timed highlight — that is the whole reason
  this is a channel and not an event.
- Needs `RungGates` evaluated across all of the faction's sources rather than one selected source.
  Measure that before assuming it is cheap.

---

## 8. Out of scope, and why

- **Provenance** (*practised* / *taught by another faction* / *found at a site*). The prototype shows
  it because it is the right model once contact can teach you things — but
  `KnowledgeLedgerEntryState` carries no acquisition field, so it needs a schema addition. Separate
  arc.
- **The Routes / War / Telling domains.** No nodes exist. The screen must not draw empty columns.
- **The icon asset.** The cairn in the prototype is an SVG sketch. Shipping needs bundled art at the
  action bar's and the collapsed rail's sizes.

> **AS BUILT (issue #581): the cairn SHIPPED, on TWO mechanisms chosen by the HOST WIDGET.**
> `assets/icons/hud/cairn.png`, resolved through the new `ui/HudSprites.gd`
> (`for_mark(HudKnowledgeVocab.LAUNCH_MARK)`). The action-bar launcher takes it as the `Button.icon`
> PROPERTY — a `Button` carries art there, never as a child — on all three mounts, collapsed rail
> included; the turn orb's `knowledge_learned` row takes it as a `TextureRect` built in place of its
> `Label`, exactly one of the pair ever existing. `HudKnowledgeVocab.LAUNCH_GLYPH` (`▲`) is no longer
> a placeholder: it is the FALLBACK both surfaces draw when the art fails to load. The orb's copy is
> UNTINTED — the fill is what carries the silhouette, and the severity accent is on the stripe beside
> it. `.claude/rules/client/knowledge-panel.md` and `turn-orb.md` carry the mechanics.
- **A "never used" record.** See §2 — derived "nothing is using it" ships now; a true history needs
  a sim field.

---

## 9. Sequencing

Each slice is its own PR and lands on its own.

| Slice | What | Depends on |
|---|---|---|
| **A** | The overlay migration — registry / legend / picker split, the minimap mount, the Inspector cleanup (§6) | nothing |
| **B** | The knowledge screen + the action-bar launcher; delete the Know tab (§3, §4) | nothing |
| **C** | The attention producer + the `panel_requested` branch; retire the System note (§5) | B |
| **D** | The `ready_for_improvement` channel (§7) | A |

> **AS BUILT: C shipped ONE producer, not two.** This row said *two* — the freshly-learned row and an
> aggregate `"N discoveries unspent"` row. The second was built, rendered and cut: the orb is for
> events and losses in progress, and a standing backlog row never goes away, so the orb never returns
> to its calm all-clear pulse. §1 had already given that count the action-bar PIP. The full reasoning
> is in §5's banner and in `.claude/rules/client/turn-orb.md`, and `turn_orb.gd` asserts the row's
> ABSENCE — so re-adding it fails a test rather than quietly relighting the orb.

**A is independent of the whole knowledge arc** and is the cleanest thing to do first — it is a
self-contained cleanup with a visible win, and it leaves the registry D needs.

---

## 10. Setting a fresh worktree up

Both steps are one-time per checkout, and skipping either makes every scene fail to parse with
`Identifier "X" not declared in the current scope` — which reads exactly like a broken tree.

1. **`cargo xtask godot-build`** — a fresh worktree has no native extension, so every scene dies on a
   missing `libshadow_scale_godot` dylib.
2. **`godot --headless --path clients/godot_thin_client --import`** — a fresh worktree has no
   `.godot/` cache, so the global class registry is empty and no `class_name` resolves.

`workbench_preview` fails on `main` — *"content column is 15.0px wider than the surface allows
(375.0 > 360.0)"* — and is unrelated to this arc. The other five render harnesses pass.

---

## See Also

- `docs/knowledge_screen_ux_proposal.html` — the prototype this plan describes
- `docs/knowledge_visibility_ux_proposal.html` — the eight options it was chosen from
- `docs/plan_intensification_ladder.md` — the rung engine and the knowledge pattern
- `docs/plan_contact_and_logistics.md` §Q4 — the route branch, the next domain to appear
- `.claude/rules/client/overlay-channels.md` · `.claude/rules/client/band-city-panel.md` ·
  `.claude/rules/client/turn-orb.md`
