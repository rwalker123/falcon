# The Knowledge Screen

**Status:** design settled, not implemented. Prototype: `docs/knowledge_screen_ux_proposal.html`
(the eight-option comparison it came from is `docs/knowledge_visibility_ux_proposal.html`).

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
| **The `⌃` glyph** | Rejected. Ships as a drawn icon on the `_stage_glyph_sprite` seam. Direction: the **cairn**. |

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
- **Craft knowledge** — no recipe requiring that craft appears in the faction's kit ledger.

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
- The **pip** on the button carries the unspent count and clears when the screen is opened.
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
| `OverlayChannels` | all-`const` + `static`, **a registry** | The **client-side** channel descriptors — `""` (No Overlay), `terrain_tags`, and later `ready_to_climb` — each `{key, label, description, legend_kind, available}`. Merged with the server-published `overlays.channels` payload. **Adding a channel is one registry entry**, exactly as `WorkbenchPages.PAGES` works for pages. |
| `OverlayLegend` | all-`static`, stateless | Renders a legend from a descriptor + `MapView.overlay_stats_for_key`. **Generic**: a `ramp` channel gets min/avg/max; a `facts` channel gets the count lines its provider returns. No channel is named here. |
| `OverlayPicker` | the widget | The list + the legend mount + the selection, pushed to `MapView.set_overlay_channel`. Knows no channel by name. |

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

## 7. The `ready_to_climb` channel

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
- **A "never used" record.** See §2 — derived "nothing is using it" ships now; a true history needs
  a sim field.

---

## 9. Sequencing

Each slice is its own PR and lands on its own.

| Slice | What | Depends on |
|---|---|---|
| **A** | The overlay migration — registry / legend / picker split, the minimap mount, the Inspector cleanup (§6) | nothing |
| **B** | The knowledge screen + the action-bar launcher; delete the Know tab (§3, §4) | nothing |
| **C** | The two attention producers + the `panel_requested` branch; retire the System note (§5) | B |
| **D** | The `ready_to_climb` channel (§7) | A |

**A is independent of the whole knowledge arc** and is the cleanest thing to do first — it is a
self-contained cleanup with a visible win, and it leaves the registry D needs.

---

## 10. Known blocker

`main` currently fails to parse: `Identifier "HudPalette" not declared in the current scope`, which
takes down `GameLaunch.gd` and every preview harness (`scripts/preview.sh` hangs until the
watchdog reaps it at 180 s). Fix or confirm fixed before starting — every slice here needs the PNG
harnesses.

---

## See Also

- `docs/knowledge_screen_ux_proposal.html` — the prototype this plan describes
- `docs/knowledge_visibility_ux_proposal.html` — the eight options it was chosen from
- `docs/plan_intensification_ladder.md` — the rung engine and the knowledge pattern
- `docs/plan_contact_and_logistics.md` §Q4 — the route branch, the next domain to appear
- `.claude/rules/client/overlay-channels.md` · `.claude/rules/client/band-city-panel.md` ·
  `.claude/rules/client/turn-orb.md`
