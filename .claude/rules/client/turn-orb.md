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
| `ui/TurnOrb.gd` / `ui/TurnOrb.tscn` | The bottom-right **turn orb** (replaces the old "Advance Turn" button): calm cyan pulse when the attention registry is empty, else a severity-tinted count badge + a reasons popover (see "Turn orb & attention model"). **THE TURN NUMBER IS ON THE FACE** — the `Turn N` caption that used to sit to the orb's LEFT is gone (and `CLUSTER_WIDTH` fell from 260 to `ORB_DIAMETER + EDGE_MARGIN_RIGHT` = 116 with it, which is what stops the orb reading off-centre in a dock-row rail; the count badge is drawn INSIDE `_orb_area`, so nothing overhangs and no extra width is needed for it — the right inset stays IN the width because the cluster is the right-flush `BottomBar` child and `_layout`'s own right offset would otherwise squeeze the orb by 16px). **The word `TURN` sits ABOVE it, CURVED along the face's own circle** (`TURN_WORD`, uppercase to match this HUD's eyebrow vocabulary — `WORK` / `PARTIES` / `AT THE FIRE`), wearing the current accent at `TURN_WORD_ALPHA` so it inherits the calm-cyan / severity tint while staying subordinate: the number is the information, the word is only its label. **Four things about it are load-bearing.** (1) **IT DRAWS IN ITS OWN OVERLAY, A `Control` CHILD OF `_face`** — never in `_on_orb_area_draw`: `_face` is a child of `_orb_area`, so every draw command `_orb_area` issues (the pulse, the base ring, the count badge) renders BEHIND the face's stylebox, and the word drawn there would be invisible under the filled face. The overlay is `PRESET_FULL_RECT` + `MOUSE_FILTER_IGNORE` with `draw.connect(_on_turn_word_draw)`, reusing the exact `draw.connect` idiom already in the file — no new script, nothing relocated. (2) **Curved text is not a `Label`** — it is per-glyph `draw_char` with a per-glyph rotation (the "hand-draw it rather than fight a font" idiom `MagnifierButton` establishes), and **every advance comes from the FONT** (`get_char_size`), never an assumed uniform width, or the letters space unevenly and the word looks drunk. `turn_word_metrics()` is the ONE place that arithmetic lives (advances → `arc_length = Σ advances + TURN_WORD_TRACKING × (n−1)` → `arc_angle = arc_length / radius`, radius = `FACE_DIAMETER × 0.5 × TURN_WORD_ARC_FRACTION`), read by BOTH the draw and the ui_preview guard so the guard cannot measure something the renderer does not. Canvas +y is DOWN, so the apex is `-PI/2`, the run starts at `-PI/2 − total/2` (centred on the apex, measured: midpoint −89.2°), and each glyph is placed at `centre + radius·(cos a, sin a)` with `draw_set_transform(pos, a + PI/2)` — baseline tangent to the arc, glyph upright relative to it. (3) **`draw_set_transform_matrix(Transform2D.IDENTITY)` AT THE END IS MANDATORY** — a transform left set corrupts every subsequent draw call on that canvas item. (4) **The word draws iff there IS a number to label** (`_show_turn_word`, now `_anim_phase == ANIM_NONE`): the number never leaves the face, so the one case with nothing to label is the resolve animation, which scatters it onto the orbit ring — hovering does NOT hide it. Deliberately ONE named branch, so a later "TURN ‣‣" verb phrase is a one-line flip. The overlay `queue_redraw()`s from **both** `_refresh_face_text` (hover + registry change) and `_style_face` (accent change) — a stale word beside a re-tinted number is the likeliest bug here. **Tuned by rendering, and `TURN_WORD_FONT_SIZE` (11) is the TOP of the usable range**: at 10px the run is legible but thin at a 1:1 raster; at 11px the run spans 84° of the circle, its ink reaches 31 of the face's 37px radius (≈4px clear of the 2px border) and sits **8px above a 30px number's cap line, 11px above a 4-digit 23px one**. ui_preview asserts the ARITHMETIC (drawn pixels cannot be asserted): `arc_angle < TURN_WORD_MAX_ARC_ANGLE` (a third of the circle — a word wrapping past that is a font/tracking bug, deliberately NOT clamped, since silently squeezing a broken layout hides the fault) and `radius + ascent ≤ FACE_DIAMETER × 0.5` (34.9 of 37 at 11px, so ~2px of headroom — a bump needs the frames re-read), plus the two VISIBILITY invariants, which is where the old "hides on the glyph swap" assertion went: hovering does NOT hide the word, and the word IS hidden while the number is scattered (driven through the real gate — a face click — then settled back). **Its type size is MEASURED, never tabled** (`_turn_font_size`): step down from `TURN_FONT_SIZE_MAX` (30) until `font.get_string_size(...).x` fits `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at `TURN_FONT_SIZE_MIN` (13) — a per-digit-count table drifts the moment the theme font changes, and one fixed size either clips turn 1200 or wastes the face on turn 1. Measured: 1 / 47 / 999 all sit at 30px, **1200 steps down to 23px** and fits at 53 of 53. `_style_face`'s `font_color = accent` is untouched, so the number carries the same calm-cyan / severity tint the glyph did. **THE NUMBER NEVER LEAVES THE FACE, AND THE HINT BELOW IT FOLLOWS THE CLICK SEMANTICS** (`_refresh_face_text`) — `_on_face_pressed` BRANCHES on the registry, so the face must not promise what a click will not do; but the number is the information the orb exists to show, and swapping it away to say so was the wrong trade. The affordance is a small glyph BELOW the number instead, drawn in its own `_face_hint` overlay and appearing on hover: registry **EMPTY** → `HINT_GLYPH_ADVANCE` (`▸▸`), tooltip names the turn it advances TO; registry **NON-EMPTY** → `HINT_GLYPH_REVIEW` (`▴`) — an UP-caret, because the reasons popover opens ABOVE the orb, and deliberately NOT the advance pair, which would promise an advance the click does not perform — tooltip names the count and "click to review"; **RESOLVING** → no hint at all, the face is not clickable. **Both glyphs are geometric-shapes triangles, and that is a RENDERING decision, not a style one**: the old face-filling affordance was `‣‣` (U+2023, the triangular BULLET) at 26px, which it could afford with the whole face to itself — a bullet's ink is only ~0.2em, so at hint size it rasterizes to two featureless blobs (rendered, seen, replaced by U+25B8, the glyph `ADVANCE_LABEL` already wears). **Tuned by rendering**: at `HINT_FONT_SIZE` 22 with `HINT_BASELINE_FRACTION` 0.89 the hint's ink sits **5.1px below a 30px number's baseline and 6.9px clear of the 2px border**, an even split of the 17.9px band; at a 4-digit 23px number the gap above opens to 8.1px and the border clearance is unchanged, because the hint is positioned off the FACE, not off the number. Re-evaluated from `set_turn`, the hover handlers, **`_recompute`** and every animation transition, so entries arriving while the pointer rests on the face cannot strand a hint. Verified by ui_preview `turn_orb_turn_4digit` (the four-probe fit assertion **plus the curved word's three**, judged for clearance at TRUE size on that frame — the widest number is the tightest case) with `turn_orb_fork_blocks`' click-semantics assertion unchanged, and by `band_panel_dockrow_bottom` (the orb as it actually reads in the dock row's rail, at 1:1). Re-emits `focus_requested` (jump) / `advance_requested` so Main's advance/jump wiring is unchanged; palette from `HudStyle`, all geometry/severity/kind as named constants ; the attention contract also carries an optional **`blocking: bool`** (default false) — the **end-turn GATE**: while any entry sets it the popover's `Advance ▸` is `disabled` and wears the reason. A **non-locating** row (`x < 0`) now emits **`panel_requested(kind)`** instead of a jump, so the orb never learns what a fork is |
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
  - **`under_kept_rung` / `under_kept_herd`** (warn) — an improved source the band's **keeping pool is
    not covering**, on the neglect clock. **This producer exists because there is nowhere else the
    warning could live** — the work board lists ASSIGNMENTS, and the pool is a band-level role, so a
    source can be perfectly staffed with gatherers and still be losing its rung. A player can only be
    told by something that finds them, which is what the orb is for.
    **The urgency rides the detail TEXT, not a card row** — a persistent countdown on the tile card
    would be a permanent readout of a condition that is usually irrelevant. Detail names the pool, the
    bill and the clock: `Husbandry short 1 work — sheds in 3 turns` /
    `Agriculture short 1.5 work — lapses in 2 turns`.

    > #### ⛔ RETIRED — `unworked_rung` / `under_crewed_herd`, AND THE CREW COUNT THEY COMPARED
    >
    > They asked *"is anybody WORKING this source"* — the plant one gated on
    > `forage_effort_at(...).workers > 0`, the animal one compared `hunt_effort_on(...).workers`
    > against `herders_needed` and rendered `"2 of 4 keepers — sheds in 3 turns"`. **Both sides of that
    > comparison are wrong under a pooled model**: the left is a TAKE crew, and keeping has been a
    > band-level pool since `docs/plan_standing_upkeep.md` §2.5, so there are no per-source keepers to
    > count. `SourceForecast` already recorded the identical shape as retired one surface over — the
    > orb kept it and substituted the hunt crew, which is worse, because a hunting party is normally
    > smaller than a keeper demand and so it fired on **every managed herd**.
    >
    > **Reported from play**: a herd with 6 keepers supplying 9.0 against a demand of 8.27 —
    > `upkeepShortfall == 0`, fully covered — with the orb crying under-crewed while the herd card
    > correctly stayed silent. One screen, two answers, and the wrong one was the one that found the
    > player.
    >
    > **Both producers now call `SourceForecast.is_under_kept`, the herd card's own gate**, which reads
    > the sim's published `upkeepShortfall` — the same number the decay and the shed act on. One
    > function, both surfaces. The roster threading below went with the crew reads.
    >
    > **What this deliberately stops raising**: a built patch nobody harvests but the pool keeps. There
    > is no mechanism by which that decays, so there was nothing to warn about. What it newly raises is
    > a harvested-but-underpaid source, which is the thing that actually loses the rung.
  - **BOTH read the wire's countdown; neither computes one.** `neglectGraceRemaining` is
    `(grace + 1) − neglect`, so **`0` means the penalty is biting NOW** and `N > 0` means it bites in
    N more unworked turns. **`hasNeglectGrace == false` means nothing is at risk** — a wild patch, a
    wild herd — and that bool is the whole reason the zero is safe to publish: without it, "nothing to
    lose" and "losing it this turn" would both read `0`. **A source with the bool false renders no
    countdown at all.** Rows sort biting-now above still-counting-down.
    ui_preview `turn_orb_unworked_rung`.

  ### RETIRED — a producer that asks "is this source WORKED?" must be handed THIS snapshot's roster

  > **The problem this solved is gone because the question is gone.** Neither producer asks whether a
  > source is worked any more — both read the sim's own published `upkeepShortfall`, which is a fact
  > about the SOURCE and needs no roster, no crew fold and no snapshot ordering. `_under_kept_rung_at`
  > takes no `bands` argument. **The reasoning below is kept because it still governs any producer
  > that folds crews before the ingest**, and because the build-before-ingest ordering it describes is
  > unchanged.

  `_unworked_rung_attention(player_bands)` and `_under_crewed_herd_attention(band, player_bands)` took
  the incoming roster and thread it into `HudBandLaborState.forage_effort_at(x, y, bands)` /
  `hunt_effort_on(herd_id, bands)`, whose `bands` parameter means "fold the crews over THIS list";
  empty (every other caller) means the ingested one. **That is forced by the build-before-ingest
  ordering above** — the producers run before `ingest_snapshot_bands`, so `current_player_bands()` is
  still LAST turn's, and a crew the client has not ingested yet reads as absent: every improved patch
  the player is working alarms as unworked, and an under-crewed row quotes last turn's keeper count
  (`0 of 4` for a band being seen for the first time, i.e. on the first snapshot after a load). The
  JUMP routing deliberately keeps the default — `on_turn_orb_focus` runs on a click, long after the
  ingest, where the ingested roster IS the current one. Both halves are pinned by ui_preview's
  `turn_orb_unworked_rung`, whose fixture carries a WORKED patch as a negative control precisely so a
  roster read that sees no crews fails the row COUNT.

  - **`crew_handoff`** (**info**, NON-locating, `_crew_handoff_attention`) — a build finished this turn
    and its crew MOVED: onto the finished rung's keeping if that rung declares an upkeep, back to the
    idle pool if it does not (`docs/plan_standing_upkeep.md` §2.3). **It is an attention row because of
    WHEN it has to be read**: the sim announces it on the feed, which is a log, and the whole point of
    the hand-off is that the player can re-task those hands *before ending the turn*.
    **INFO, not warn** — nothing is wrong; a build finished, which is the good news, and the row hands
    back a decision rather than reporting a loss. It sorts below every real problem.
    **The label is the SIM's own sentence, verbatim** (*"3 of your cultivate crew stay on (31, 18) to
    keep it"*): the sim knows which rung finished, how many hands moved and where they went, and a
    second phrasing here would drift from the feed's. The detail is the client's own *what to do about
    it* clause, forked on `status=carried_to_upkeep` vs `status=freed`.
    **It is fed by the COMMAND STREAM, not the roster** — `Hud.ingest_command_events` also hands the
    array to `AttentionController.ingest_command_events(events, turn)`, which keeps the matching rows
    until the turn changes. Held rather than read live because `Main` dispatches `command_events` and
    `populations` separately and the attention array is rebuilt from the populations pass: a producer
    reading the event array directly would answer empty on every frame but one, and the rows would
    flicker away mid-turn — exactly when the player is deciding what to do with those hands.
    **THE FRAME IS NOT THE FILTER — THE EVENT'S OWN `tick` IS, and `seq` is the other half.** This
    producer is a WINDOW on one turn, and it went in reading every row on whatever array the frame
    brought. `command_events` is per-frame HISTORY whose SHAPE varies by frame kind, so that had two
    concrete failure modes: a **full snapshot** — the initial connect, or the resync answer to a
    dropped delta — carries the whole `command_events_retention_turns` ring, which re-dated twenty
    turns of hand-offs to now and flooded the orb; and a **mid-tick recapture delta** re-ships every
    row since the turn baseline, announcing this turn's hand-offs twice. So a row joins the window
    only if its own `tick` is the turn the window describes, and only if its `seq` has not been taken
    for that turn (`0` being the unsequenced sentinel, admitted rather than dropped — the tick filter
    already bounds the set). **The two filters answer different questions** — WHICH TURN, and SEEN
    ALREADY — and neither substitutes for the other. `Main`'s dispatch comment said re-ingesting a
    ring was harmless because *"both consumers accumulate and de-duplicate"*; this was a **third**
    consumer that did neither, and that comment now names all three and what each does instead.
    **Non-locating AND affordance-less**, which is a third state beside `Jump →` and `Open ▸`. The
    event names its source in words and carries no coordinates, so a jump would be a guess; and a
    turn may finish several builds, so there is no ONE panel the row could open either. It therefore
    wears **no label at all** — `HudAttentionVocab.ATTENTION_KINDS_WITH_A_PANEL` is the allowlist
    `TurnOrb` renders `Open ▸` from, and `crew_handoff` is deliberately not on it. An `Open ▸` that
    does nothing when pressed is a promise the row cannot keep; the detail says where those hands
    are in words instead (*"they are idle — the band's work board has them"*). Capped at
    `ATTENTION_HANDOFF_MAX_ROWS` with an overflow row, for the off-screen-popover reason.

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
  overflow row) / **`turn_orb_resolving`** + **`turn_orb_hint_advance` / `turn_orb_hint_review` / `turn_orb_hint_4digit`** (the resolving gate and the hover hint — see "The resolving gate" below).

## The KNOWLEDGE producer — a third half of the registry, and its own ordering rule

`docs/plan_knowledge_screen.md` §5. **`knowledge_learned`** — ONE ROW PER TRACK COMPLETED THIS TURN,
non-locating, `info`, opening the knowledge screen on its `New this turn` filter
(`knowledge-panel.md`). Label `"<Discovery> learned"`, over the node's own player-facing name, so one
format covers the ladder tracks and the craft fan alike. It supersedes
`FactionReadouts._announce_knowledge_unlock`, the one-shot System-channel note, which is retired —
see `band-readouts.md`.

It wears `Open ▸`, so it needs an entry in `ATTENTION_KINDS_WITH_A_PANEL` **and** a branch in
`TurnOrbController._on_turn_orb_panel_requested` — one decision made twice. A kind on the list with
no branch renders an affordance that does nothing, which is the state `crew_handoff` avoids by being
on neither.

> ### ⛔ THE UNSPENT BACKLOG IS NOT A SECOND PRODUCER, AND THAT IS A DECISION RATHER THAN AN OMISSION
>
> §5 asked for one — an aggregate `"N discoveries unspent"` row — and it was built, rendered and then
> cut before the arc landed. **THE ORB IS FOR EVENTS AND LOSSES IN PROGRESS; AN UNSPENT DISCOVERY IS
> A STANDING CONDITION.** Its row therefore never went away, and the orb never returned to the calm
> all-clear pulse — measured, it moved 400 of the harness's frames simply by adding one to the count
> badge on every frame that draws the orb. A permanently-lit attention hub teaches the player to stop
> looking at it, which costs more than the nudge is worth.
>
> **The nudge already has a home §1 gave it**: the action bar's PIP, mounted on all three of the
> Band/City panel's layouts including the collapsed rail, derived fresh and cleared by USING the
> knowledge. The row was the same standing fact on a second surface — and on the one surface whose
> whole value is being quiet when nothing needs you.
>
> The player has also already been told: the discovery was announced by the row above, the turn it
> landed.
>
> **`turn_orb.gd` asserts the ABSENCE against a faction sitting on four unspent discoveries** — an
> exact row count plus the all-clear on the following turn — so re-adding the row in any wording
> fails a test rather than quietly relighting the orb.

### THE ORDERING TRAP, AND THE MECHANISM CHOSEN AGAINST IT

**`build_band_attention` runs at `Hud.update_band_alerts` step 2; `_knowledge.refresh_snapshot()` —
which rolls the turn diff producer 1 reads — runs THIRTY LINES LATER.** A knowledge producer built
beside the band ones therefore reads the PREVIOUS turn's diff and names the wrong discovery, in a row
that renders entirely plausibly. It happens to come out right today only because `Main` dispatches
`update_intensification` before `update_band_alerts` and that section rolls the diff too — a
coincidence of which sections a delta carries, not a guarantee. This is the recorded defect one field
over: the attention producers running before `ingest_snapshot_bands`, so every improved patch alarmed
as unworked on the first snapshot after a load.

**The mechanism is a THIRD REGISTRY HALF plus a single seam, not an ordering comment.**
`TurnOrbController` caches `_knowledge_attention` beside `_band_attention` and `_push_attention`
folds three arrays into the one `set_attention` replace. `HudLayer._refresh_knowledge_readouts` is
what fills it on a snapshot: it rolls the diff and pushes the pip and the row on three adjacent
lines, and every seam that used to call `refresh_snapshot` + `_push_knowledge_pip` calls it instead.
(The world boundary pushes it once more, over a diff `KnowledgePanelController.reset_world_state`
has just dropped — nothing to roll there, and the point is to clear the old world's row.)
So the ordering cannot be broken by a reorder somewhere else, and the knowledge row is also correct
on a delta carrying knowledge but no populations — which never reaches `update_band_alerts` at all.

`AttentionController.knowledge_attention` is a **`static func` taking the flattened roster**
(`KnowledgePanelController.nodes`) and no collaborator, which is what puts it outside the hazard:
every other producer there is a method because it reads the band/labor model, and this one can only
be run against the list handed to it.

### Verification — `tools/ui_preview/chapters/turn_orb.gd`

**The chapter clears the faction's tracks for its band states and puts back what it inherited.** The
knowledge producer is faction-wide and rides this same registry, and the walk's earlier chapters push
tracks without always advancing the turn — so the screen's diff has not rolled since whichever of them
last did, and **the first turn tick in this chapter rolls everything they taught in between onto one
orb**. Measured: a `Cultivation learned` row riding State 6, which makes the ALL-CLEAR states not clear
and adds a fifth row to the under-kept block's negative-control COUNT. Both restores are load-bearing
in the other direction too — `compose_rungs` runs three chapters later and gates its hunt-compose
frames on that knowledge, so leaving the tracks cleared moved four of its frames into judging a crew
stepper under knowledge nobody meant to change.

**The block stages FOUR tracks finishing in one turn and leaves all four UNSPENT**, which is what
lets one staging carry both claims: the producer must emit four rows, one per track and each naming
its own, and the registry must hold those four and NO backlog row. Then the turn ticks with nothing
newly taught and the same four still unspent: the row goes quiet and the orb reads ALL-CLEAR, which
is the property the cut exists to buy. `turn_orb_knowledge` is the frame; every count, every label
and the affordance are ASSERTED, because all of them render fine when wrong.

**Its band half is emptied at the CACHE (`set_band_attention([])`), never by ingesting a calm band.**
`update_band_alerts` is not inert: `ingest_snapshot_bands` overwrites the walk's `player_band` /
`player_bands` / `prev_band_sizes`, which later chapters render against.

**AND IT HANDS BACK THE TURN, not just the tracks.** The block drives turns of its own, and
`docks_legend`'s `reserved_dock` puts the orb FACE in a frame — so a chapter that wandered off to
turn 611 and stayed there changes that number for a reason that has nothing to do with what it was
testing. The tracks go back at TWO different turns, because the screen's diff only rolls when the
turn moves: one push would roll the taught set out of the baseline and announce whatever the
inherited tracks hold that the taught set did not, landing a stray row on some later chapter's
frame; the second rolls the inherited set against itself and reports nothing.

**Three leaks, all of them shared walk state, none of them visible to the exit status** — the tracks,
the band roster and the turn. Each was found by pixel-diffing every frame against a run at `main` and
asking what had moved OUTSIDE the orb's own corner.

## The resolving gate

A turn takes a round trip to the server, and until this existed the orb said nothing about it: the
face stayed live, so **mashing it queued N advances while the server was still resolving turn 1**
(issue #376). The orb now gates itself and shows the work in progress.

**`_resolving` is THE ONE FLAG.** The click gate (`_on_face_pressed` returns immediately), the
Button's own `disabled` state, the ring's sweep arc, the hint's suppression, the footer's block
reason and the animation's liveness all read that single bool, so they cannot disagree. It is raised
by `_begin_resolving()` from **both** advance emitters — `_on_face_pressed`'s empty-registry branch
and `_on_advance_pressed` (the popover footer) — immediately after the `advance_requested` emit.

**The answer is "a `set_turn` with a DIFFERENT value."** `_resolve_from_turn` is captured at request
time and is what "different" is measured against; the client has no other signal that the command
was applied. `set_turn` starts the RE-FORM, and **the gate lifts when that re-form COMPLETES, not
when the snapshot lands** — one flag, one lifetime, one exit (`_finish_resolving`). A newer turn
arriving mid-re-form is absorbed rather than restarting the flight, because `_digits_text()` reads
`_turn` live.

**THE RE-FORM IS ONLY EVER ENTERED FROM THE RING**, and that is what makes its `k` honest. The
re-form starts at `k = 1.0` (`1.0 - _ease_in_out(0)`) — fully out on the orbit — so entering it from
a scatter still in flight teleports the glyphs the rest of the way, and that fired on **every healthy
turn**: measured live, the server answers **0–57 ms** after the click, i.e. two or three frames into
the 0.30 s scatter, at `k ≈ 0.35`. So an answer arriving mid-scatter is HELD on `_resolve_answered`
and the scatter's own completion branch routes to `ANIM_REFORM` instead of `ANIM_ORBIT`. Structural,
not a patched start-`k`: there is no state from which the re-form can begin anywhere but the ring, so
no start-`k` bookkeeping exists to get wrong. The flag is cleared at both ends of the lifetime
(`_begin_resolving` / `_finish_resolving`). **A consequence worth stating: the break-apart is always
seen in full** — an acknowledgement too brief to perceive is not an acknowledgement — and on a fast
turn that spends the whole of `RESOLVE_SCATTER_SEC`, which is the lever for the trade.

**Two clocks, two deltas — `RESOLVE_MAX_STEP_SEC` (0.05).** The animation clocks (`_orbit_phase`,
`_anim_time`) take the **clamped** step, because a frame longer than that was a hitch, not motion: a
single 2 s frame — a world reveal, a full snapshot, the window losing focus — otherwise consumes the
entire 0.34 s re-form in one step and the digits jump to their resting places. Clamped, a hitch plays
as one step; at a genuine sustained 20 fps the clamp IS `delta` and nothing changes.
**`_resolve_elapsed` keeps the RAW delta** — it is a wall-clock safety net, and clamping it would push
the fail-open past the real 8 s in proportion to how badly the client was stalling, i.e. latest
exactly when it is needed most. `tools/ui_preview.gd` takes its step slice FROM this constant
(`TURN_ORB_ANIM_STEP_SEC`), or a harness stepping in bigger slices would silently under-advance and
capture the wrong phase.

**`RESOLVE_TIMEOUT_SEC` (8s) FAILS OPEN, and that is the point.** A rejected or dropped advance
(server down, command never applied) produces no new snapshot *ever*, and a permanently dead orb is
unrecoverable for the player. A measured turn round-trip is ~10ms of sim plus tens of ms of client
apply (`turn-profiling.md`), so 8s is ~100x the healthy cost and cannot fire on a real turn. The
timeout is **not a special case**: it calls the same `_begin_reform()` toward the UNCHANGED number,
i.e. "the answer was: nothing moved", so the number re-forms in place and the gate lifts through the
one exit. It accumulates only while AWAITING (scatter/orbit), never during the re-form.

**The animation is the issue's own wish — the number breaks apart, circles, and re-forms into the
next one.** Three overlay `Control` children of `_face` now exist for the same reason the curved word
does (everything `_orb_area` draws renders BEHIND the face's stylebox): `_turn_word`, `_face_hint`,
and `_face_digits`, which owns the flight. While any phase is active `_face.text = ""` and the
overlay draws the digits itself; the Button takes its string back at `_finish_resolving`. Phases are
`ANIM_SCATTER` (0.30s, ease-OUT onto the ring, size lerping down to `RESOLVE_DIGIT_FONT_SIZE`) →
`ANIM_ORBIT` (indefinite, `RESOLVE_ORBIT_PERIOD` per revolution, slots at `i·TAU/n`) → `ANIM_REFORM`
(0.34s, ease-in/out back in, size lerping up). **The re-form's flyers are built from the NEW string**,
which is what makes a digit-count change (9 → 10, 999 → 1000) need no old-to-new matching at all —
the slot count simply follows the string. **Resting positions are computed from the same font and the
same MEASURED size (`_turn_font_size`) the Button uses**, centring the run and putting the baseline
at the face's vertical centre offset by ascent/descent exactly as a centred Button does; verified on
`turn_orb_clear_click_advances` (the scatter at t=0), whose overlay-drawn `42` occupies the *same*
pixel bbox as the Button-drawn one in `turn_orb_clear` — if it did not, the hand-back would pop.
Glyphs stay UPRIGHT (no per-glyph rotation, unlike the word): the orbit carries the motion. No
`draw_set_transform*` is used in either new overlay, so there is none to clear.

**The ring drops the calm pulse while resolving** and draws a rotating sweep arc instead — the pulse
means "nothing needs you", which is exactly the wrong thing to say mid-turn. Both run off the SAME
`_orbit_phase`, so the arc and the glyphs read as one motion; `_begin_resolving` resets that phase so
the animation is a pure function of its own elapsed time (which is what makes the ui_preview frame
reproducible). The count badge is orthogonal and keeps drawing whenever the registry is non-empty.

**The footer's advance now has TWO reasons to be dead, and one channel for them.**
`_advance_block_label()` returns `""` while the advance is live, `ADVANCE_RESOLVING_LABEL`
("Resolving…") while `_resolving`, else `ADVANCE_BLOCKED_LABEL` when `has_blocking_entry()`;
`disabled` and the ghost-vs-primary treatment key off that string being non-empty, so a third reason
later is one branch and nothing else. Resolving wins over blocked — a turn already in flight is the
more immediate truth. `has_blocking_entry()` is unchanged.

**The face's `disabled` stylebox is not optional.** Four states were hand-styled and `disabled` was
not, so the default theme's disabled look would leak in the moment the gate raised; it is the
`normal` box with the border alpha cut to `FACE_DISABLED_BORDER_ALPHA` — which IS the "dimmed face".

**ui_preview guards it**, and the harness has to step the clock itself: `Engine.time_scale = 0`
means `_process` sees `delta == 0`, so the re-form would never finish on its own — the same hazard
`_flush_tweens` handles for the client's one Tween. `_settle_turn_orb_resolve(answer_turn)` pushes the
answer through `update_overlay` and steps the REAL `_advance_resolve_animation` by a fixed slice until
`is_resolving()` clears, with a cap that `push_error`s — so it doubles as proof the animation
terminates. States: **`turn_orb_clear_click_advances`** (the gate at t=0 — dimmed face, number still
at rest, sweep arc; plus the assertion the issue is actually about, that a SECOND `_on_face_pressed`
emits NO further `advance_requested`, and that the popover's Advance wears "Resolving…" disabled),
**`turn_orb_resolving`** (mid-orbit, the frame the flying digits and the sweep arc are judged on at
true size), and **`turn_orb_hint_advance` / `turn_orb_hint_review` / `turn_orb_hint_4digit`** (the two
hover hints, plus the widest number's clearance).

## The first frame of a loaded world raises fewer rows, and that is correct

A player who saves with 4 attention rows and loads into 2 has not lost anything. The orb's producers
split into two kinds, and only one kind can speak on the first frame of a world:

- **STANDING conditions**, read straight off the snapshot in front of them — starving (Producer 1),
  idle workers (3), awaiting orders (4), the starving pen (5) and the unkept plant/animal rungs
  (6, 7). These survive a load intact, because the frame that restores the world states them.
- **DIFFS and EVENTS**, which need a previous turn *this client process has actually seen* —
  losing-population (2), the crew hand-off (8), and the knowledge `learned` rows. A load reloads
  `Main.tscn`, so `HudBandLaborState`, `_handoffs` and the knowledge diff are all new and empty.

**Producer 2's guard is the shape to copy**: `prev_band_sizes().has(entity)` asks *"do I have a
previous observation of this band"* and says nothing when it does not. It does **not** compare
against `0` and announce a collapse, which is the tempting bug — a freshly loaded world genuinely has
no previous turn to compare against, and inventing one would report a population crash that never
happened. The row returns on the next tick, on a comparison that is real.

**So the missing rows are honest and no code should make them appear.** What was NOT honest is the
other direction: until the knowledge turn-diff was fixed
(`.claude/rules/client/knowledge-panel.md` → "A world that arrives already knowing things"), the tick
after a load *grew* three false `"<Track> learned"` rows for knowledge earned long before the save.
A count that changes across a load is expected; rows that appear out of nothing were the defect.

