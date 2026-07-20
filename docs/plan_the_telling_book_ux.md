# The Telling — the Book UX

**Status: design, ready to implement (client-only).** The narrative panel stops being a
grow-and-cap scroll log and becomes a **book that assembles itself as the narrator's medium
matures** — oral recitation → painted wall → written record. This is a pure client slice: the
medium is already on the wire (`voiceMedium`, consumed by `TellingPanel.set_voice_medium`) and no
schema, server, or `core_sim` change is needed. Design context: `docs/plan_the_telling.md` (the
engine) and `docs/Emergent Narrative.md` §7 (the maturing voice). Predecessor: PR #134 landed the
scroll-log `TellingPanel`; this replaces its rendering model.

## Why

Two problems, one fix.

1. **The panel fights the dock.** Today `TellingPanel` is a `DockScrollFit` card that grows with
   content and is capped against the right dock's remaining height (`ui/hud/DockScrollFit.gd`). That
   machinery exists *only* because the card grows without bound. A **fixed-size page** removes the
   whole class of sizing bugs — the card never grows, so the dock's own `RightScroll` trivially
   stacks Telling + Victory + Terrain Types with no bespoke height math.

2. **The medium arc has no on-screen payoff.** Today medium maturation is carried by the title +
   accent + a hairline rule and nothing else (`TellingPanel.MEDIUM_STYLES`). The concept doc frames
   the narrator *maturing* as itself a narrative arc. A book whose **capabilities grow with the
   medium** makes `voice.medium_written`'s catalog line — *"The marks on the rock do not tire, do not
   misremember, and do not die"* — describe a UI affordance the player literally just earned:
   scrollback.

## The one design decision (settled)

**`oral` is *current-utterance* lossy, not ephemeral and not soft.** At `oral` the page holds the
**current speaking turn's beats** (a turn may carry 1–2), but nothing older, and there is **no
backward access**. You never lose a beat mid-viewing, but you cannot review a prior turn's telling
until the civilization can write it down. The loss is **presentational only** — the 40-entry
retention buffer keeps everything, so `painted`/`written` reveal history the sim was holding all
along, honouring "the voice never lies." The named tradeoff: in the early game you can miss reading a
beat permanently, and earning `written` is what buys back scrollback.

## The model

One page engine, three medium-gated presentations. The controller keeps its full retention buffer
(`_entries`, cap 40 — unchanged, still the backfill/dedup source of truth) and **derives** the
display from it every render.

### Pages

- A **page = one speaking turn's beats**: the entries sharing a `tick` (each ingested entry already
  carries `tick`). Group `_entries` by `tick`, ascending, into `_pages` on each render. A turn with
  no beats produces no page — "page = a turn that had something to say", so there are never blank
  pages. Multiple beats on one tick share one page.
- `_page_index` — the page currently in frame.
- `_at_newest` — `_page_index == _pages.size() - 1`.
- `_unread` — a page newer than the visible one exists and the player has not turned to it.

### Fixed size

The card is a **fixed height** (`PAGE_HEIGHT`, a named const — size it to hold ~3 short prose beats
+ their gloss; judge on the ui_preview frames). Drop the `DockScrollFit.fit` call and the
`_dock_scroll` dependency. Keep the inner `ScrollContainer` **only** as an overflow fallback for a
rare over-long single page (a turn with several long beats); the *card's* height no longer depends on
content, which is the entire point. `refit()` becomes a no-op that at most re-clamps the inner scroll
(keep the method so `Hud._refit_right_dock`'s call stays valid — with a fixed card, a sibling's
visibility flip no longer changes the telling's height, so there is nothing to refit).

### The three rungs (per-medium presentation)

A per-medium table drives three levers. `mediumId` is free-form by design, so this is a **table with
an `oral` fallback**, never a match on the shipped three (same discipline as `MEDIUM_STYLES`).

| medium | furniture | `leaf_back` | `retain_pages` |
|---|---|---|---|
| `oral` | none | ✗ | ✗ — current utterance only |
| `painted` | accumulation marks + position | ✗ | ✓ — walk forward, no back |
| `written` | full book: page number + edges + ‹ › leaf controls | ✓ | ✓ — leaf both ways |

- **`retain_pages = false` (oral):** the visible page is **always the newest** (`_page_index` is
  pinned to `_pages.size() - 1`). Turning forward = acknowledging a new utterance and snapping to it;
  earlier pages are unreachable. No page chrome at all — just the recitation, styled as today's prose
  (the accent hairline may stay; page numbers/edges/leaf controls do **not** appear). This is the
  current-utterance rule made literal: oral memory does not keep the previous telling.
- **`retain_pages = true, leaf_back = false` (painted):** the visible page can lag the newest;
  turning forward steps `+1` (you can walk through unread pages one at a time — the wall retains
  them), but there is no step back. Furniture is minimal: a **retaining-surface** cue that
  *accumulates* — a row of marks / a `page k of n` position with **no** back control and **no**
  numbered edges. The first sense that the surface remembers.
- **`retain_pages = true, leaf_back = true` (written):** one page at `_page_index`, full book chrome
  — a **page number** ("III / VII" or "Page 3 / 7"), page-edge styling, and **‹ prev / › next** leaf
  controls. Leaf freely in both directions. This is the earned scrollback.

**Restraint still governs the look.** The HUD is dark and stays dark (see the `TellingPanel` note in
`clients/godot_thin_client/CLAUDE.md`): a light "parchment" page reads as a rendering bug, not a
chronicle. The book furniture is carried by **line-art edges, a page-number label, the accent, and
the leaf glyphs** — not by a change of background. Reuse `HudStyle` throughout; no hardcoded hexes.
Leaf controls are **line-art** (‹ ›, or hand-drawn like `MagnifierButton` if a font glyph blobs at
size — judge at true size), never emoji.

### Turning the page — yields to the reader

The visible page **never moves on its own.** New beats arriving (`ingest_events`) rebuild `_pages`
and, if they add a page beyond the visible one, set `_unread` — they do **not** move `_page_index`.
This is the page-turn twin of today's tail-scroll-yields-to-reader rule: being yanked to a new page
mid-sentence is worse than not turning.

Two ways the page turns:

1. **Player input.** Clicking the page body (or a dedicated "next" affordance; ‹ › in written)
   turns the page. In oral this snaps to newest; in painted it steps `+1`; in written it steps in the
   chosen direction. Turning to the newest clears `_unread`.
2. **Turn advance (catch-up).** Expose `reveal_newest()`. `Hud` calls it when the player advances the
   turn (the existing `next_turn_requested` path) so a player who moves on is caught up to the latest
   telling. Mid-turn beats only mark unread; *advancing* reveals. (In oral/painted `reveal_newest()`
   jumps to the last page; in written it jumps to the last page too — advancing is "catch me up",
   distinct from leafing back to read history.)

`_unread` surfaces as a subtle indicator (an accent pulse on the "next" affordance / a small "a new
telling waits" cue) — restrained, `HudStyle`, no emoji.

### The page-turn animation — motion matures with the medium

**Motion mirrors the furniture.** The animation plays **only when the player TURNS the page** (a leaf
control, or `reveal_newest()` catch-up, or — for `oral` — a new utterance replacing the last), **never on
a beat merely arriving to a retaining medium**: that only marks `_unread`, so animating it would fight the
yields-to-reader rule. Each medium's motion is a short, snappy tween (`PAGE_TURN_DURATION`, ~0.18s):

- **`oral` → a CROSSFADE in place.** Oral has no leafing; its only "turn" is one recitation replacing the
  last, so the outgoing page fades out and the incoming fades in at the same spot. No spatial motion. (Oral
  pins to the newest on every beat arrival, so this fires on the utterance-replacement — that IS oral's turn,
  and it has no held page for the yields rule to protect.)
- **`painted` → the incoming page RISES** from `PAGE_RISE_OFFSET_RATIO · PAGE_HEIGHT` below with a fade (new
  marks drifting up onto the wall — accumulation).
- **`written` → a horizontal SLIDE**, direction = leaf direction: leaf **forward** → outgoing exits left /
  incoming enters from the right; leaf **back** → the reverse. The real page turn, and the payoff that
  `written` is the actual book.

**Mechanism.** A clipped fixed-height page frame (`clip_contents`) holds the incoming page (the
`ScrollContainer`) and an **outgoing snapshot** (a second `RichTextLabel` carrying the pre-swap BBCode). A
tween drives a single `progress` 0→1 that `_apply_turn_frame` maps to the two nodes' position + alpha per the
medium's motion; the primary label is already showing the FINAL page, so the outgoing snapshot animates OUT
while the incoming animates IN.

**Interruption-safe is mandatory.** Every turn kills the running tween and has already re-painted the final
page statically, so a rapid second turn / a medium change / a collapse always settles to the correct static
state — the end state after any interruption equals the plain static render, never a half-slid/half-faded
page. The initial population (empty → first telling) and a reset (telling → empty) do **not** animate.

**Restraint still governs.** The dark HUD stays dark — the motion is position + alpha only, no new chrome. No
page-turn *curl*; a turn is a state change with a brief transition, not an effect to sit through.

### Collapse

Keep the existing collapse toggle + its `telling_collapsed` pref (same `user://narrative.cfg`
`[narrative]` section — **do not add a prefs file**). Fold the collapse control into the new header
row alongside the page furniture. Collapsed = the page frame hides, the header stays (so the player
can expand it back), exactly as today.

### What does NOT change

- The retention buffer (`_entries`, cap `ENTRY_LIMIT` 40), `handles_kind()` (the feed/telling split
  definition), `ingest_events` dedup + backfill semantics, `set_voice_medium` / the `MEDIUM_STYLES`
  title+accent table, `accent_for()` (still shared with `NarrativeForkPanel`).
- The command-feed split — narrative kinds still render here, receipts stay in the feed.
- No wire, schema, server, or `core_sim` change.

## Files

- `clients/godot_thin_client/src/scripts/ui/TellingPanel.gd` — the rewrite: page derivation, the
  per-medium mode table, fixed-size rendering, the leaf/reveal interactions, `reveal_newest()`.
- `clients/godot_thin_client/src/ui/HudLayer.tscn` — the `TellingPanel` subtree may need node
  additions for the header row / leaf controls / page-number label (author them under `CardContent`
  per the `PanelCard` content contract — one `%`-named `VBoxContainer`, no anchor-positioned children
  inside a card).
- `clients/godot_thin_client/src/scripts/Hud.gd` — construct the controller without the
  `right_dock_scroll` arg (or leave the arg and ignore it — implementer's call); wire
  `reveal_newest()` into the turn-advance path; `_refit_right_dock` / `refit()` reconciled to the
  fixed size.
- `clients/godot_thin_client/tools/ui_preview.gd` — update `telling_panel_oral` /
  `telling_panel_written` and add a `telling_panel_painted` state; add a state that shows the
  **unread** indicator and a written state parked on a **non-last page** (so backward-leafing is
  visibly available). Keep `telling_and_feed` and the `dock_default_layout` placement assertion
  working.
- `clients/godot_thin_client/CLAUDE.md` — rewrite the two `TellingPanel` rows to describe the book
  model (retire the "grows / `DockScrollFit` / newest-at-bottom scroll" language).

## Verification

Per the ui_preview harness (`ui-preview-harness` memory / `tools/ui_preview.tscn`, run **without**
`--headless`). The book is medium-gated, so the decisive frames are the three rungs side by side:

- `telling_panel_oral` — the current utterance, **no** page furniture, no leaf controls, no page
  number. One turn's beats only.
- `telling_panel_painted` — the accumulating wall: multiple recent turns visible / a position cue,
  **no** back control, minimal furniture.
- `telling_panel_written` — the full book: page number, edges, ‹ › leaf controls; parked on a
  **non-last** page to prove backward-leafing is present.
- an **unread** frame — the "a new telling waits" cue with the page held on an older page (the yields
  rule).
- `telling_and_feed` — unchanged intent: the fixed telling page coexists with a legible command
  feed.
- `dock_default_layout` — the placement assertion (`telling_panel.get_parent() == right_stack`) still
  holds; the fixed page now takes a **stable** slice of the right dock rather than a computed one.

The animation is verified in the same PNG harness by DRIVING a turn and FREEZING the tween mid-transition
(`debug_freeze_turn_at`), so the outgoing and incoming pages coexist in one captured frame:

- `telling_turn_written_mid` — the two pages offset **horizontally** (one sliding out left, one in from the
  right), with the ‹ › book furniture.
- `telling_turn_painted_mid` — the incoming page **risen partway + fading in** over the fading outgoing one.
- `telling_turn_oral_mid` — both pages at **partial alpha in the same spot** (the crossfade), no furniture.
- `telling_turn_interrupted` — a rapid second turn: an **assertion** that the visible page equals the
  expected final page and no outgoing overlay survives (the interruption-safe settle).

The settled `telling_panel_*` end-state frames use the non-animating `debug_jump_to` park, so they capture
the static book, not a tween mid-flight.

Every frame judged in a real window (the harness renders one); a green screenshot is not a passing
test — the placement/behaviour assertions carry the guarantees.

## Deliberately out of scope (this slice)

- **Per-medium copy** — the same wardrobe line renders under every medium; medium is presentational
  only (a documented non-goal server-side — do not author per-medium strings).
- **`archive`** — the concept's fourth medium is not shipped by the sim (`docs/plan_the_telling.md`
  → the maturing voice), so there is no fourth book rung to build.
