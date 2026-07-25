---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{AttentionController,TurnOrbController,hud_attention_vocab}.gd"
  - "clients/godot_thin_client/src/scripts/ui/TurnOrb.gd"
---

<!-- Extracted verbatim from lines 176-177;201-201;3193-3290 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The turn orb and the attention model

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/TurnOrbController.gd` | `RefCounted` controller (HUD decomposition Phase 1b, `docs/plan_hud_decomposition.md`) owning the **turn-orb / attention / fork** cluster — the orb wiring, the narrative-fork panel (The Telling), and the attention-registry ASSEMBLY (`_push_attention` folds the band/expedition half + the snapshot-driven `_pending_fork_attention` fork producer into ONE `TurnOrb.set_attention`; the fork row `blocking`s the orb's `Advance ▸` — the client-side end-turn gate). Hud holds it as `_turnorb`, constructed in `_ready` AFTER `_telling` + `_command_feed` (it needs both), handed `turn_orb` + the HUD CanvasLayer (`self`, the host it `add_child`s the fork panel into, since a RefCounted can't). Owns `_pending_forks` / `_stance_axes` / `_band_attention` / `_auto_opened_forks` / `_fork_panel` (all moved off HudLayer). Keeps thin reflective delegators for the five methods Main reaches by reflection (`update_pending_forks`/`update_stance_axes`/`update_voice_medium`/`has_pending_fork`/`note_unanswered_fork`). **Emits its OWN signals; HudLayer RELAYS each** (the controller never emits a HudLayer signal): `answer_fork_requested` → `HudLayer.answer_fork_requested`, `advance_requested` → `next_turn_requested.emit(1)` (after `_telling.reveal_newest()`), `focus_requested(x,y)` → `AttentionController.on_turn_orb_focus`. **Two seams to band/labor live in `AttentionController`, not here:** `HudLayer.update_band_alerts` builds the band half via `AttentionController.build_band_attention` and feeds it here through `set_band_attention(attention)` (the write half of `_band_attention`; the two internal callers push directly), and the orb's "Jump →" band routing (`AttentionController.on_turn_orb_focus` → `_awaiting_expedition_at` / `_starving_pen_at` / `BandPanelController.focus_labor_source`) lives on that controller, reached through the relayed `focus_requested`. `set_turn` forwards to the orb so `update_overlay`'s fan-out no longer touches the node directly. `note_unanswered_fork` routes straight through `_command_feed.note`. Behaviour identical to the old inlined turn-orb code |
| `ui/hud/AttentionController.gd` | `RefCounted` controller (HUD decomposition, `docs/plan_hud_decomposition.md`) owning the OTHER half of the turn-orb attention model from `TurnOrbController` — **PRODUCING the band/expedition attention rows and ROUTING their "Jump →"** (the orb WIDGET, the fork producer and the registry ASSEMBLY/fold stay on `TurnOrbController`). Hud holds it as `_attention`, constructed in `_ready` AFTER `_bandpanel` (its expedition/pen jumps reuse the panel's own focus paths). **`build_band_attention(player_bands, player_expeditions)`** builds the five producers — the three inline band ones (1 starving / 2 losing-population / 3 idle-workers), 5 `_starving_pen_attention`, then 4 `_awaiting_orders_attention` — in the exact append order the old `update_band_alerts` loop produced; `HudLayer.update_band_alerts` hands the result to `_turnorb.set_band_attention(...)`. **THE ATTENTION BUILD MUST RUN BEFORE INGEST:** Producer 2 reads `_band_labor.prev_band_sizes()`, which `ingest_snapshot_bands` OVERWRITES for next turn, so `update_band_alerts` was restructured into a PURE roster-split loop → `build_band_attention` → ingest (a read-before-write — build before ingest, or every band silently stops reporting decline). `build_band_attention` deliberately does NOT ingest — that stays on `HudLayer`. `band_number == i + 1` (the resident band's positional counter, matching the band-picker + panel header). Jump routing (`on_turn_orb_focus` → `_awaiting_expedition_at` / `BandPanelController.select_expedition`, else `_starving_pen_at` / `BandPanelController.focus_labor_source`, else the fallback) lives here now, wired to `_turnorb.focus_requested`. **THE INJECTION SURFACE IS ONE CALLABLE** — `_herd_label_for_id` (stays on HudLayer, reads three collaborators, reached through a typed adapter since `Callable.call` returns `Variant`); `HudBandLaborState.labor_assignments_of` is a public `static func` reached as a class-name static (the `DetailFormat` idiom — no injection). **It emits its OWN `alert_focus_requested`, RELAYED by HudLayer** (the `TurnOrbController` pattern; a second relayer into that one signal alongside `_bandpanel`'s is fine — Main connects to it once). Collaborators: `_band_labor` + `_bandpanel`. Behaviour identical to the old inlined producers |
| `ui/TurnOrb.gd` / `ui/TurnOrb.tscn` | The bottom-right **turn orb** (replaces the old "Advance Turn" button): calm cyan pulse when the attention registry is empty, else a severity-tinted count badge + a reasons popover (see "Turn orb & attention model"). **THE TURN NUMBER IS ON THE FACE** — the `Turn N` caption that used to sit to the orb's LEFT is gone (and `CLUSTER_WIDTH` fell from 260 to `ORB_DIAMETER + EDGE_MARGIN_RIGHT` = 116 with it, which is what stops the orb reading off-centre in a dock-row rail; the count badge is drawn INSIDE `_orb_area`, so nothing overhangs and no extra width is needed for it — the right inset stays IN the width because the cluster is the right-flush `BottomBar` child and `_layout`'s own right offset would otherwise squeeze the orb by 16px). **The word `TURN` sits ABOVE it, CURVED along the face's own circle** (`TURN_WORD`, uppercase to match this HUD's eyebrow vocabulary — `WORK` / `PARTIES` / `AT THE FIRE`), wearing the current accent at `TURN_WORD_ALPHA` so it inherits the calm-cyan / severity tint while staying subordinate: the number is the information, the word is only its label. **Four things about it are load-bearing.** (1) **IT DRAWS IN ITS OWN OVERLAY, A `Control` CHILD OF `_face`** — never in `_on_orb_area_draw`: `_face` is a child of `_orb_area`, so every draw command `_orb_area` issues (the pulse, the base ring, the count badge) renders BEHIND the face's stylebox, and the word drawn there would be invisible under the filled face. The overlay is `PRESET_FULL_RECT` + `MOUSE_FILTER_IGNORE` with `draw.connect(_on_turn_word_draw)`, reusing the exact `draw.connect` idiom already in the file — no new script, nothing relocated. (2) **Curved text is not a `Label`** — it is per-glyph `draw_char` with a per-glyph rotation (the "hand-draw it rather than fight a font" idiom `MagnifierButton` establishes), and **every advance comes from the FONT** (`get_char_size`), never an assumed uniform width, or the letters space unevenly and the word looks drunk. `turn_word_metrics()` is the ONE place that arithmetic lives (advances → `arc_length = Σ advances + TURN_WORD_TRACKING × (n−1)` → `arc_angle = arc_length / radius`, radius = `FACE_DIAMETER × 0.5 × TURN_WORD_ARC_FRACTION`), read by BOTH the draw and the ui_preview guard so the guard cannot measure something the renderer does not. Canvas +y is DOWN, so the apex is `-PI/2`, the run starts at `-PI/2 − total/2` (centred on the apex, measured: midpoint −89.2°), and each glyph is placed at `centre + radius·(cos a, sin a)` with `draw_set_transform(pos, a + PI/2)` — baseline tangent to the arc, glyph upright relative to it. (3) **`draw_set_transform_matrix(Transform2D.IDENTITY)` AT THE END IS MANDATORY** — a transform left set corrupts every subsequent draw call on that canvas item. (4) **On hover with an EMPTY registry the face swaps to `GLYPH`, and the word goes with it** (`_show_turn_word`, keyed on the `_face_shows_glyph` flag `_refresh_face_text` writes): it labels the number, so with no number it labels nothing. Deliberately ONE named branch, so a later "TURN ‣‣" verb phrase is a one-line flip. The overlay `queue_redraw()`s from **both** `_refresh_face_text` (hover + registry change) and `_style_face` (accent change) — a stale word beside a re-tinted number is the likeliest bug here. **Tuned by rendering, and `TURN_WORD_FONT_SIZE` (11) is the TOP of the usable range**: at 10px the run is legible but thin at a 1:1 raster; at 11px the run spans 84° of the circle, its ink reaches 31 of the face's 37px radius (≈4px clear of the 2px border) and sits **8px above a 30px number's cap line, 11px above a 4-digit 23px one**. ui_preview asserts the ARITHMETIC (drawn pixels cannot be asserted): `arc_angle < TURN_WORD_MAX_ARC_ANGLE` (a third of the circle — a word wrapping past that is a font/tracking bug, deliberately NOT clamped, since silently squeezing a broken layout hides the fault) and `radius + ascent ≤ FACE_DIAMETER × 0.5` (34.9 of 37 at 11px, so ~2px of headroom — a bump needs the frames re-read), plus that the word hides while the face shows the glyph. **Its type size is MEASURED, never tabled** (`_turn_font_size`): step down from `TURN_FONT_SIZE_MAX` (30) until `font.get_string_size(...).x` fits `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at `TURN_FONT_SIZE_MIN` (13) — a per-digit-count table drifts the moment the theme font changes, and one fixed size either clips turn 1200 or wastes the face on turn 1. Measured: 1 / 47 / 999 all sit at 30px, **1200 steps down to 23px** and fits at 53 of 53. `_style_face`'s `font_color = accent` is untouched, so the number carries the same calm-cyan / severity tint the glyph did. **THE HOVER SWAP FOLLOWS THE CLICK SEMANTICS, and that is the part to get right** (`_refresh_face_text`) — `_on_face_pressed` BRANCHES on the registry, so the face must not promise what a click will not do: registry **EMPTY** → rest shows the number, hover swaps to `GLYPH` at `GLYPH_FONT_SIZE` (keeping the one affordance that says "this advances", which the bare number would otherwise remove) and the tooltip names the turn it advances TO; registry **NON-EMPTY** → rest shows the number and hover does **NOT** swap (a `‣‣` there would promise an advance a click will not perform — it opens the popover) with the tooltip naming the count and "click to review". Re-evaluated from `set_turn`, the hover handlers **and `_recompute`**, so entries arriving while the pointer rests on the face cannot strand the glyph. Verified by ui_preview `turn_orb_turn_4digit` (the four-probe fit assertion **plus the curved word's three**, judged for clearance at TRUE size on that frame — the widest number is the tightest case) with `turn_orb_fork_blocks`' click-semantics assertion unchanged, and by `band_panel_dockrow_bottom` (the orb as it actually reads in the dock row's rail, at 1:1). Re-emits `focus_requested` (jump) / `advance_requested` so Main's advance/jump wiring is unchanged; palette from `HudStyle`, all geometry/severity/kind as named constants ; the attention contract also carries an optional **`blocking: bool`** (default false) — the **end-turn GATE**: while any entry sets it the popover's `Advance ▸` is `disabled` and wears the reason. A **non-locating** row (`x < 0`) now emits **`panel_requested(kind)`** instead of a jump, so the orb never learns what a fork is |
- **Band alerts → the turn orb** (`Hud.gd` `update_band_alerts`, dispatched from `Main.gd` on the
  snapshot `populations`): the standalone left-dock **Alerts panel was removed** and its alerts folded
  into the turn-orb attention model (see next bullet). `update_band_alerts` is now a **PURE roster-split
  loop → `AttentionController.build_band_attention` → ingest**: it splits the player faction into
  bands/expeditions, then the controller PRODUCES the orb's `attention` array, then `_band_labor` ingests.
  **The build must precede the ingest** — Producer 2 (losing-population) reads `prev_band_sizes()`, which
  `ingest_snapshot_bands` overwrites, so a read-before-write ordering is load-bearing (the whole reason the
  loop was restructured). The producers + their Jump routing live in `AttentionController`, NOT here (see
  its Key Scripts row). NOTE: cohorts carry no top-level band label in the snapshot — names fall back to a
  positional "Band N"; a server-side band-label field would make names authoritative.
- **Turn orb & attention model** (`ui/TurnOrb.gd` + `ui/TurnOrb.tscn`, last `BottomBar` child;
  `docs/plan_hud_nav_turn_orb.md`): the bottom-right orb replaces the "Advance Turn" button and
  is a **generic attention hub**. Readiness = the attention registry is **empty** → a calm cyan
  `SIGNAL` pulse ("nothing needs you"); any entries → the pulse stops and a **count badge** tinted
  by the highest severity shows. **The orb face always advances the turn** (`_on_face_pressed`): with
  an **empty** registry the click emits `advance_requested` directly (no popover — an empty popover has
  nothing to review, and once mis-stretched to full height it pushed its own `Advance ▸` footer
  off-screen, trapping the player); with **entries** it toggles a **reasons popover** (built at
  runtime, `HudStyle.card_stylebox()`) — one row per entry (severity stripe + kind icon + label +
  detail + right-aligned `Jump →`), highest-severity first, plus an `Advance ▸` footer. The orb
  knows nothing about producers; it renders a list of generic **Attention** dicts:
  `{kind, severity ("info"|"warn"|"critical" → SIGNAL/WARN/DANGER), label, detail, x, y}` where
  `x < 0` = non-locating (renders `Open ▸`, a no-op stub for now). Kind→icon (in `TurnOrb.gd`):
  `starving`→🍖, `losing_population`→📉, `idle_workers`→🛠, `awaiting_orders`→▮▮ (read from
  `FoodIcons.STATUS_ICONS` — the same glyph the Band panel's awaiting row wears), unknown→●.
  Row labels **clip** and `POPOVER_WIDTH` is sized to the widest producer row: a row's inner HBox is
  anchored to its Button (not a container child), so an over-wide label used to spill its `Jump →`
  outside the card instead of widening it. Wiring stays stable via Hud
  relays: a row's jump → `focus_requested` → `alert_focus_requested` → `MapView.focus_on_tile`
  (the same centering the retired Alerts panel used); the footer → `advance_requested` →
  `next_turn_requested(1)`; `update_overlay` pushes the turn number via `set_turn`. The **four live
  producers** (all in `AttentionController.build_band_attention`, each pushed with the tile
  `current_x`/`current_y` so Jump locates it) — the folded-in Alerts panel, plus the expedition one. The
  first three run in one loop over the player faction's BANDS:
  - **`starving`** (critical) — `BandFoodStatus.is_critical(turns)`; label `"<band> starving"`, detail = `_food_turns_text(turns)`.
  - **`losing_population`** (warn) — shrank vs the previous snapshot (`_prev_band_sizes`); label `"<band> losing population"`, detail = `_decline_reason(days, morale, morale_cause, last_emigrated)` (`— starving` / `— people leaving` / `— harsh terrain|climate|unrest` / `— low morale`).
  - **`idle_workers`** (warn) — `idle_workers > 0`; label `"N idle workers"`, detail = band name. Supersedes the old `activity == idle` alert (a worker count is more actionable).

  - **`starving_pen`** (warn, `_starving_pen_attention`) — a pen this band keeps whose feed it could
    not pay: the herd is **shrinking every turn** and a 25-turn investment is draining away (it
    recovers if fed, so the player must hear about it *while it is reversible*). Label `"<Species> pen
    starving"`, detail `"40% fed — the herd is shrinking"`, icon = the corral 🐄 (`FoodIcons.POLICY_ICONS`).
    **Found via the band's own Corral labor assignments, never a scan of `herds`** — a herd carries no
    owner field client-side, so scanning would alarm on a RIVAL's pen. Its **Jump routes to the HERD**
    (`_starving_pen_at` → `BandPanelController.focus_labor_source`, the Band panel's Hunt-row path), so the drawer that
    explains the alert actually opens. **On the double-report question:** a pen only goes unfed when
    the keeper's larder came up short, so the same empty larder usually also trips `starving`
    (critical) on that band. They are **not one alert twice** — one cause, two different losses (the
    people are dying / the herd is dying), two subjects, two jumps, two remedies — but only **one gets
    to shout**: the band's row stays critical, this one rides below at WARN. ui_preview
    `turn_orb_starving_pen` renders exactly that pair.
  - The detail line is deliberately terse: orb rows **clip at `POPOVER_WIDTH`**, and appending the
    keeper's name ("· Band 1") pushed this row past it (rendered, seen cut, shortened).

  The fourth (`_awaiting_orders_attention`) runs over the **EXPEDITIONS** split out of that loop:
  - **`awaiting_orders`** (warn) — an expedition in `ExpeditionPhase::Awaiting`: parked at its
    objective, burning provisions, doing nothing until the player acts. Structurally the same class
    as idle workers (a demand on the player, an efficiency loss, not a crisis) — hence WARN, and
    hence it belongs on the orb rather than only on a band panel you happen to have open. **One row
    per party, not one aggregate** (each is a separate decision with its own destination; idle
    workers genuinely IS one aggregate): label = the phase words from `EXPEDITION_PHASE_LABELS`
    ("Awaiting orders"), detail = `"<mission> · <objective>"` (mission from
    `EXPEDITION_MISSION_LABELS`; objective = the followed herd for a hunt party, the party's tile for
    a scout). Capped at `ATTENTION_AWAITING_MAX_ROWS` — the popover is positioned ABOVE the orb, so an
    unbounded list would climb off-screen and take the `Advance ▸` footer with it — with the remainder
    folded into one `"+N more awaiting orders"` row that jumps to the first party past the cap (so
    even the aggregate row is actionable, not a dead `Open ▸` stub). **Its Jump reuses the Band
    panel's expedition-row path**: `AttentionController.on_turn_orb_focus` resolves an awaiting expedition
    standing on the jumped-to tile (`_awaiting_expedition_at`) and routes through
    `BandPanelController.select_expedition` (recenter + pin that exact expedition so its drawer opens),
    falling back to the plain `alert_focus_requested` recenter for the band-located producers.

  A sixth producer is snapshot-driven rather than band-derived:
  - **`decision`** (critical, **`blocking`**, NON-locating) — a pending narrative fork (The Telling).
    One row per fork, label `"A question awaits an answer"`, detail = the fork's narration truncated
    to the row width (the orb clips; the full telling is the panel's job). It is the **client-side
    end-turn gate**: the server never blocks turn resolution and auto-expires an unanswered fork to
    its defer branch. Because `set_attention` is a **full replace**, it folds into
    `update_band_alerts` via `_push_attention()` (which concatenates the cached `_band_attention`
    with `_pending_fork_attention()`) — a second `set_attention` call would wipe every band row, and
    re-invoking `update_band_alerts` would consume `_prev_band_sizes` and eat the losing-population
    alert. **Gating covers the ORB ONLY**: `Inspector._send_turn` (the dev toolbar + autoplay) is
    deliberately NOT gated — autoplay disables itself on a failed advance, so a hard gate there would
    deadlock the dev loop — but it is not silent either: `Inspector.set_turn_advance_observer` →
    `Main._on_inspector_turn_advanced` → `Hud.note_unanswered_fork()` posts a command-feed receipt.

  The orb severity-sorts (critical floats up), so a starving band tops the popover. Future producers
  (`war` / `decision`) are stubs the model already fits — one producer each, **no orb changes** (the
  awaiting one needed only a kind→icon entry). ui_preview: `turn_orb_fork_blocks` (**the gate's own frame + behavioural assertion**: with a blocking fork seeded a face click must NOT emit `advance_requested`, and the popover's Advance must be `disabled` — the inverse of `turn_orb_clear_click_advances`) / `narrative_fork_panel` + `narrative_fork_panel_warm` (the panel on the REAL authored `soft_drift.long_chase` copy, both registers) / `telling_panel_oral` + `telling_panel_written` (the Telling panel on six REAL authored beats from `beat_definitions.json`, incl. the catalog's longest line `cold_open.bone_ground` so wrapping is exercised — the pair is the medium maturation, same copy, only title + accent age) / **`telling_and_feed`** (**the frame that proves the split**: the Telling panel holding six beats while the command feed still shows four fully-legible receipts — before PR-C two beats pushed every receipt off; the old `narrative_feed` state, which tested prose-vs-receipt styling *inside* the feed, was retired with the behaviour it tested) / `turn_orb_attention` (the three band
  producers) / **`turn_orb_turn_4digit`** (the turn number ON the orb face and the MEASURED type fit — it walks
  turns 1 / 47 / 999 / 1200 and asserts, for each, that the face's string IS the number, that the chosen size
  sits inside `[TURN_FONT_SIZE_MIN, TURN_FONT_SIZE_MAX]`, and that it actually fits
  `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`; the 4-digit turn is the case a fixed size would clip. It is
  ALSO the **curved `TURN` word's** frame — `_assert_turn_word_clears` rides it, asserting the run's arc
  angle stays under `TURN_WORD_MAX_ARC_ANGLE`, that `radius + ascent` stays inside the face, and that the
  word hides while the face shows the advance glyph; the CLEARANCE between word and number is judged by
  eye here, at true size, against the widest number) / `turn_orb_awaiting_orders` (awaiting rows + idle workers coexisting, incl. the cap's
  overflow row).
