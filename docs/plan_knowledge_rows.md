# The Knowledge Screen, Laid Out in Rows

**Status: PROPOSED.** Nothing here has shipped. This is a follow-on to
`docs/plan_knowledge_screen.md` (SHIPPED), which built the screen; this one changes how it is laid
out, because the layout it shipped with cannot hold the tree the ladder is going to teach.

Prototypes: `docs/knowledge_rows_ux_proposal.html` (this plan, live and clickable) and
`docs/knowledge_layout_ux_proposal.html` (the five options it was chosen from).

The problem, in one line: **domains are the axis that grows, and the shipped layout puts them on the
axis that cannot scroll.**

---

## 1. The measurement — why this is structural, not cosmetic

A domain column has a `COLUMN_MIN_WIDTH` floor of 210px and a `COLUMN_SEPARATION` of 20, and the
detail pane is pinned at `DETAIL_WIDTH` 300 plus padding. So the card's content minimum is a
straight function of the domain count:

```
card ≈ 230 × domains + 336 + chrome
```

| | Domains | Card wants | Source |
|---|---|---|---|
| **Today** | 6 | ~1,716px | plant, animal, route, forestry, extraction + the craft fan |
| **Planned** | 10–12 | 2,636–3,096px | `docs/plan_civilization_steps.md` commits Storage, Belief, Defense and Dwellings, and calls a sixth branch "config" |
| Stress | 24 | ~5,856px | — |

The design viewport is 1920, which docked panels have already taken a bite out of. **Ten to twelve
domains is the planned shape, not the stress case.**

### ⛔ THE CLAMP IS COMPUTED CORRECTLY AND THEN OVERRULED BY THE LAYOUT

It looks as though `refit` already handles this, and it does not. `KnowledgePanel.refit` raises
`max_width` to the room, and `AutoSizingPanel.fit_width` does clamp the request to it — but
`_apply_width` then writes that number to `size.x`, and **Godot will not render a Control below its
`get_combined_minimum_size()`**. The card's combined minimum runs straight through
`KnowledgeScroll`, whose `horizontal_scroll_mode` is `SCROLL_MODE_DISABLED`, and a ScrollContainer
that cannot scroll an axis propagates its child's full minimum on that axis rather than absorbing
it.

**So no clamp will make the columns fit. The content minimum itself has to come down**, which is
what this plan does. Anyone who reaches for "just bound the card to the room" should read this
section first — it is the obvious fix and it does not work.

---

## 2. Decisions — settled, do not re-open

| | Decision |
|---|---|
| **Layout** | **A domain is a ROW**, its rungs running left to right along the rail. Not columns, and still not a graph. |
| **Why rows** | Width becomes a function of **ladder depth**, which the design caps at ~4 rungs and forbids growing; every new branch costs one row of **height**, on an axis that already scrolls. |
| **Detail** | **Inline, beneath the selected row** — not a pinned pane. That returns the pane's 336px and is what lets the card sit at ~820px at any domain count. |
| **Selection** | **A toggle.** Clicking the open knowledge closes it; only ever one is open. |
| **Grouping** | Domains are gathered under **subject areas**, and **the area comes off the config, never a client table**. |
| **Folding** | An area folds to one line that **still says what is inside it**. Folding is not hiding, and the distinction is load-bearing (§5). |
| **Card size** | **Fixed.** The card must not resize as rows open and close. |
| **Rejected: horizontal scroll** | It hides content, which voids the screen's own "filters dim, they do not hide" contract. Re-enabling `KnowledgeScroll`'s horizontal axis is still needed to make the card *shrinkable*, but it is not the layout. |
| **Rejected: a domain rail / master-detail** | Never overflows, but answers "what do my people know about herds" instead of "what do my people know". |
| **Rejected: a flat state-sorted list** | Scales furthest and throws away the ladder shape the rail exists to state. Keep in reserve if the tree ever outgrows rows. |

---

## 3. The screen

```
┌ What your people know ─────────────── 7 known · 4 learning · 2 unknown · 2 unspent ┐
│ Show  [All 13] [Learning now 4] [Ready · unused 2] [New this turn 1]               │
├────────────────────────────────────────────────────────────────────────────────────┤
│ ▾ FOOD                                                    2 learning · 1 unused    │
│      LAND   ●Cultivation ── ◐Seed Selection 62%                                    │
│      HERDS  ●Herding ── ●Penning • ── ◐Foddering 71% (gates nothing)               │
│ ▾ MAKING                                                  2 learning · 1 unused    │
│      FORESTRY   ●Woodcraft ── ○Conservationism                                     │
│      EXTRACTION ◐Quarrying 18%                                                     │
│      CRAFT      ●Tanning   ◐Bone Working 40%   ●Weaving •                          │
│ ▸ WORKS                                                        1 new · 1 to learn  │
└────────────────────────────────────────────────────────────────────────────────────┘
```

- **A ladder domain draws the rail between its rungs; the craft fan draws none.** Unchanged from
  the shipped rule — it is a property of the domain descriptor, not a branch in the renderer. Rotated
  ninety degrees, the rail is the connector between chips rather than a line down a column's edge.
- **A capability that gates nothing** (`is_step` false — `foddering` today) hangs off the end of its
  ladder and is marked, so it reads as hanging off rather than continuing the steps.
- **Filters still dim rather than remove**, under `All`. What a filter now also does is fold the
  areas it does not match (§5).
- **The craft fan is the one shape that does not love a row.** It is unordered and grows with
  `recipes.json`, so it wraps as connector-less chips. If it outgrows one row it gets its own section
  under the ladders rather than a peer row — decide that when it has more than about six crafts.

---

## 4. Selection is a toggle, and the payload already supports it

`PAYLOAD_SELECTED` is a knowledge key, and the empty string already means *nothing selected* — the
detail pane renders its placeholder for it. **So a toggle is "set the key, or set it back to empty",
with no new state.**

- Clicking the **open** knowledge closes it. Clicking a **different** one moves the detail.
- **Only one is ever open**, and this is not fussiness. Several open at once makes the panel's height
  a function of how much the player has poked at it, on a card centred in its room — so it would grow
  in *both* directions from the middle of the screen on every click.
- `Escape` closes it, and the detail carries a `✕`.

### ⛔ THE ORB'S HAND-OVER MUST FORCE OPEN, NEVER TOGGLE

The turn orb's knowledge rows open this screen on a filter and a key
(`plan_knowledge_screen.md` §5, `KnowledgePanelController`). An external open that went through the
same toggle would **close** the row when the player happened to have that exact knowledge open
already — the one case where the orb's row appears to do nothing. Opening from outside sets the key;
it never clears it.

### THE CARD MUST NOT RESIZE AS ROWS OPEN

A detail is wider than a bare ladder row, so a card fitted to its content narrows when the detail
closes and widens when it opens. On a centred panel that is a visible lurch on every click. **The row
layout wants `target_width` to be the panel's actual width rather than the nominal floor it is
today**, so `fit_width` has nothing left to fit. The same goes for height: fit to the tallest state,
or the card breathes vertically too.

---

## 5. Subject areas

Domains grow without limit. **Subject areas do not** — they answer *"what part of the game is this"*,
and that list is short and stable. That is the whole reason to introduce a level: it puts a bound on
the thing that grows.

| Area | Branches today | Where it grows |
|---|---|---|
| **Food** | `plant`, `animal` | fishing; anything that feeds people |
| **Making** | `forestry`, `extraction`, craft | metals, pottery, glass, textiles — gather it, then work it, up to factories |
| **Works** | `route` | storage, water, dwellings, power networks — built once, used by everyone |
| **Reach** | — | trade, navigation (`docs/plan_contact_and_logistics.md`) |
| **Lore** | — | belief, telling, astronomy, medicine, law |
| **War** | — | defense, war |

**This pays off now, not only later.** An area with no domains is never drawn — the same rule that
already forbids an empty column, one level up — so today's six domains render as **three** headings.

### ⛔ THE AREA COMES OFF THE CONFIG, NOT OUT OF A CLIENT TABLE

The panel has been here before. `LADDER_DOMAINS` was a hard-coded client list, and it is why the
route branch's Roadbuilding and Paving had nowhere to appear; the fix was to make the sim publish a
roster and let the panel build itself from it (`.claude/rules/client/knowledge-panel.md`). **A
hard-coded area table reintroduces exactly that bug one level up**: the first branch somebody adds
without editing the client falls out of the screen.

Branches have no record in `intensification_ladder.json` today — `branch` is a string repeated on
each rung — so the change is to give them one:

```json
"branches": {
  "plant":      { "area": "food" },
  "animal":     { "area": "food" },
  "forestry":   { "area": "making" },
  "extraction": { "area": "making" },
  "route":      { "area": "works" }
},
"areas": ["food", "making", "works", "reach", "lore", "war"]
```

`areas` is the **display order**, and it is not optional: the areas are peers, so first-seen order off
the rungs would reshuffle the whole screen whenever a rung was added — the same defect that made
column order unstable before the roster carried it.

Each `ladder_knowledge` roster row then carries `area` beside the `branch`, `order` and `is_step` it
already carries, and `KnowledgeRoster` groups domains into areas the way it groups nodes into domains
now. **The craft fan is not a ladder branch**, so its area is named where its nodes are built.

**Two fallbacks, both load-bearing:**

1. A branch whose descriptor **names no area** still draws, under a fallback heading.
2. An area the client **has no word for** still draws, under its own capitalized token — exactly what
   `HudKnowledgeVocab.domain_label` already does for an unknown branch.

A knowledge that vanishes because a config edit was incomplete is the worst failure this screen has,
and it is one it has actually shipped once.

**The labels stay client-side copy**, as the branch labels do now: the wire says `food` and a player
reads *Food*. That table lives beside `DOMAIN_BRANCH_LABELS` in `HudKnowledgeVocab`, where the
localization pass can reach it.

### FOLDING IS NOT HIDING, AND THAT IS WHY THE DEFAULTS ARE WHAT THEY ARE

This looks like it collides with a shipped rule, so state it plainly. A track at `0.0` is drawn
greyed rather than skipped, and the filters dim rather than remove — both exist because a new player
who has learned nothing must still be **shown there is something to learn**. A folded heading keeps
that promise: it stays on screen and says what is inside it (*"3 to learn"*, *"2 learning · 1
unused"*). That is different in kind from the old `_build_knowledge_block` skip, which drew nothing
at all.

- **Everything starts open.** Never fold an area because it is empty of progress — that is the old
  bug wearing a caret, and for a new player it would fold the entire screen.
- **A filter folds the areas it does not match**, and unfolds them on the way back to `All`. Under
  `All` the dim-don't-hide rule is untouched; under a filter, folding is what makes the filter useful
  at twenty-four domains rather than leaving the player to scroll past dimmed rows.
- **A hand-made fold outlives a filter change** and is forgotten when the screen closes. There is no
  persisted UI state on this screen and this does not need to invent any.

---

## 6. Out of scope

- **No graph, no pan, no zoom, no edge routing.** Unchanged and not re-opened.
- **No research queue.** The screen is a reading, not a planner — the review question for any change
  here (`.claude/rules/client/knowledge-panel.md`).
- **No persisted UI state.** Fold state dies with the screen.
- **No change to what "unspent" means.** Derived, never persisted, and not "never used".
- **The craft fan's own section**, if it outgrows a row. Decide it when there are more than ~6 crafts.

---

## 7. Sequencing

Each slice is its own PR and lands on its own. **The client half does not wait on the schema half**:
group on an `area` that is absent from every row and every domain lands under one fallback heading,
which is a working screen.

| Slice | What | Depends on |
|---|---|---|
| **A** | Rows instead of columns; inline detail; the toggle; fixed card size (§3, §4) | nothing |
| **B** | The `branches` descriptor + `areas` order in the ladder config; `area` on the `ladder_knowledge` roster row; publish it (§5) | nothing |
| **C** | Group into areas, folding, filter-driven folding, per-area tallies (§5) | A and B |

A is the whole visible win and is client-only. B is a config + schema + sim slice with no client
change. C is small once both have landed.

---

## 8. Setting a fresh worktree up

Both steps are one-time per checkout, and skipping either makes every scene fail to parse with
`Identifier "X" not declared in the current scope` — which reads exactly like a broken tree.

1. **`cargo xtask godot-build`** — a fresh worktree has no native extension, so every scene dies on a
   missing `libshadow_scale_godot` dylib.
2. **`godot --headless --path clients/godot_thin_client --import`** — a fresh worktree has no
   `.godot/` cache, so the global class registry is empty and no `class_name` resolves.

---

## See Also

- `docs/knowledge_rows_ux_proposal.html` — the prototype this plan describes
- `docs/knowledge_layout_ux_proposal.html` — the five layouts it was chosen from, each stress-tested
  from 6 to 24 domains
- `docs/plan_knowledge_screen.md` — the arc that built the screen this one re-lays-out
- `docs/plan_civilization_steps.md` §"The improvement catalog is the intensification ladder" — the
  branches that are coming, and why the domain count is going to double
- `docs/plan_intensification_ladder.md` — the rung engine and the knowledge pattern
- `.claude/rules/client/knowledge-panel.md` — the engineering rationale this plan must not contradict
