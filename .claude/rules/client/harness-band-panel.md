---
paths:
  - "clients/godot_thin_client/tools/band_panel_preview.gd"
  - "clients/godot_thin_client/tools/band_panel_preview.tscn"
  - "clients/godot_thin_client/tools/band_panel_trade_tab.gd"
  # `command_guard` is gated HERE, not in `harness-headless-guards.md`, because the rationale it
  # needs is the KIT PICKER's: it `preload`s `BandFx.kit_roster_fixture()` — one of the tree's three
  # cross-harness preloads — and the "compose a NON-DEFAULT kit on every path, and write the id
  # BEFORE the sheet opens" rule is what keeps its assertion capable of failing.
  - "clients/godot_thin_client/tools/command_guard.gd"
  - "clients/godot_thin_client/tools/command_guard.tscn"
  # The DENIAL raid arc below spans two harnesses: `expedition_denial_panel` is a `ui_preview` state
  # living in this chapter, so the chapter loads this file as well as `harness-ui-preview.md`.
  - "clients/godot_thin_client/tools/ui_preview/chapters/band_expedition.gd"
---

<!-- Split out of .claude/rules/client/test-harnesses.md, which was itself extracted from
     clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e.
     The pseudo-table cells this file carries were re-wrapped at 100 columns; no wording changed. -->

# The `band_panel_preview` harness

The Band/City dockable-panel PNG harness, and the arcs whose frames ride it.

## The leading bound is asserted in BOTH directions, and one of them is a window resize

`_assert_leading_bound_matches_the_column` is the claim whose absence shipped a chrome split that did not
exist on the hardware it was built for. `BandCityPanel._rail_split` charges the card's leading bound, and
that bound was the left column's authored **360** applied unconditionally — so the split needed **2012px**
of window and a 2000px monitor stacked the minimap and the turn orb at one end. **Nothing failed**: every
frame in this file that asserted the split was taken on a 3440 ultrawide, where 360px of over-charge is
affordable, and the two 1920 states asserted the STACKED arm as if it were the design.

**The pair is the whole assertion.** On `band_panel_dockrow_bottom` (1920×1080) the left dock's one card
stops at 224 against a strip starting at 662, so the bound must be **0** and the chrome splits; on
`band_panel_dockrow_column_reaches` — **the same 1920 width, a 540-tall window** — the strip's top edge
rises to 216, the same card is genuinely in the row, the bound must be **360** and the chrome stacks. A
client that always zeroed the bound passes the first and fails the second; the old client passes the
second and fails the first.

**IT IS A PURE WINDOW RESIZE, NOT A STAGED FIXTURE.** The strip's top edge sits at 0.4 of the window's
height, so shrinking the window walks it under the card the rule reads — no invented content, and the same
producer under test in both frames. `DOCKROW_COLUMN_REACHES_HEIGHT` is 540 because under ~520 the chrome
stops parking at all and the state would be testing a different rule.

**THE EXPECTATION IS THE HARNESS'S OWN MEASUREMENT.** `_left_dock_content_reach` walks `LeftStack` and
clips to `LeftScroll` — the shape `_right_dock_content_reach` already had for the other column — rather
than asking `Hud.left_column_content_reach`, which is the producer the bound is derived from and would
agree with it whatever either answered.

### `_assert_card_clears_lateral_columns` measures the LEFT column as drawn content now

It bounded the left dock as a REGION, on the reasoning that the region really is full-height. The region
is; the one card in it is not, and a region-shaped claim there forbids the card room the HUD is not using
— which is precisely the argument this same function already made for the RIGHT column and the reserve
`348e5c09` was fixed out of. Both halves read painted content, with a liveness conjunct (`cards > 0`) so
"the card clears the column" cannot pass on an empty dock.

**Its negative control moved to the column's WIDTH.** Where the column stops above the strip there is
nothing in the row for an unbound island to hit, so a control asked against the painted rect would be
unsatisfiable rather than merely vacuous. What it still answers is the question the bound is about: would
the leading furniture, unbound, be inside the column's width at all?

### Every probe that clears the bounds must put `Main`'s back, not `lateral_column_widths()`'s

`_main_lateral_bounds` exists because three probes restored the bounds by pushing
`Hud.lateral_column_widths()` straight onto the panel. That was `Main`'s behaviour once; it is not now
(`Main.band_panel_lateral_bounds` drops the leading term where the column is not in the row), and a probe
that restores the old shape leaves the panel bounded by a rule the live client does not have — measured,
it silently un-split the chrome for every state that ran after it, so the frame and the assertions
disagreed about the same panel. The rule is a `static` for exactly this reason; ask it, never restate it.

## The faction page's `Fodder:` row and its open drill-down

**One frame and twelve `: PASS`** — measured `138 / 803` → `139 / 815` on one windowed run, `assert
OK` unchanged at 436 (the claims go through `_assert_band_panel`, not the zone-fit helpers). The
frame is **`band_panel_faction_fodder`**, the drill-down OPEN under `Fodder ▾ 100.0 · -1.0 /turn`.

**IT RENDERS BEFORE THE PAGE'S OTHER ASSERTIONS, AND THAT IS LOAD-BEARING.** The breakdown popover is
an EMBEDDED subwindow, so it hides the moment GUI focus moves — and `_assert_faction_page` drives real
controls: `_assert_faction_party_row_jumps_home` presses a summary row's link, and the re-render that
follows frees the focused button and takes the card down with it. Measured directly: the popover
opened, and was gone one `process_frame` later, leaving a frame that photographed a CLOSED caret and
an assertion that read `_breakdown_popover_key` as empty. So the state sits immediately after
`band_panel_faction`'s own save, before anything is pressed, and closes its own popover behind it.
**Do not move it below the assertions to keep the block tidy.**

**`_faction_roster`'s SECOND band was given a fodder larder, and the first was deliberately left
without one.** The faction stock is therefore one band's — distinguishable from an average, and from
a drill-down filtered down to the bands that have a larder — and the open card carries a band WITH
hay beside a band with none, which is the contrast a roster of two equal larders could not make. The
numbers are `ui_preview`'s `band_hay_short` ledger, so the two harnesses describe one shape.

**The rate is asserted AGAINST the food scale as well as for its own.** `format_yield_fodder` prints
`-1.0 /turn` where `format_yield` would print `-1.00 /turn`; a page spelling its two larders' rates
at two resolutions is the drift the paired claim exists to catch, and the positive alone passes on a
build that prints both.

**Falsified by registering the faction disclosure with an EMPTY row set: 7 failures**, and the split
is the point — the summary row's sum and rate still PASS (the row renders fine), while every
drill-down claim fails. A card with nothing in it looks entirely plausible in a thumbnail.

### …and its DORMANT form, beside a live row

**One more frame and eleven `: PASS`** — thirteen when it landed (measured `139 / 815` → `140 / 828`,
`assert OK` still 436), two fewer since the dormant row's two hover sentences were retired and the
pair asserting them became ONE negative: no hay word reaches the block's `tooltip_text` at all.
The frame is **`band_panel_faction_fodder_dormant`**: the faction page's dim `Fodder  —` with no
caret, beside a band's live `Fodder ▸ 100.0  (100 turns)` in the drawer.

**BOTH STATES SHARE ONE RENDER, by DETACHING the panel rather than by selecting into it.** A selected
player band is the dock's subject wherever a dock exists, so `set_band_city_panel(null)` is called
*after* the dormant faction page has been painted: dropping the controller's reference does not touch
the zones the panel is already drawing, so the page stays on screen while the drawer takes a band with
a real hay ledger down its own fallback path. `ui_preview`'s `band_fodder_dormant` is the mirror image
of the same trick. The frame asserts that precondition, because a drawer that had gone to the
band-panel pointer would photograph one fodder row instead of two and look entirely tidy.

**THE RESTORE MUST NOT CYCLE.** `_panel_is_faction` is never cleared by detaching, so a `CYCLE_PREV`
on the way out walks OFF the page rather than back onto it — and every faction assertion below then
reads a band's own vitals. Measured as four unrelated-looking failures (`PEOPLE reads the whole
faction`, the Food alert, the runway, the weighted morale) before the cycle was dropped;
`refresh_snapshot` re-renders the page on the push by itself.

**TWO NEEDLES FOR ONE ROW, because `detail_bbcode` splits `Key: value` into two `[cell]`s.** The
rendered BBCode never contains `Fodder: `, so the page is searched for the row's DIM run nested
inside the neutral value tint, while the band's line — asked of the producer — still carries the key.
Both come off `DetailFormat`'s own consts, so neither can drift from what is drawn. A needle written
as the producer's line silently found nothing on the page (measured).

**THE GATE CLAIM IS STAGED ON THE FLOOR, not on zero.** `_fodderless_faction_roster` strips every
larder to HALF `SourceForecast.FODDER_FLOW_MIN` — a real quantity the shared per-band test refuses
and a `store > 0` faction gate would admit — so "the two agree" is a discriminator rather than a
restatement that zero is zero.

**Falsified four ways.** Dropping the dim at the shared builder: **4** here and **2** in `ui_preview`,
which is itself the evidence that one builder serves both scales. Registering a disclosure on the
dormant row: **2** (the rendered caret and the registration, which are different claims — a payload
the row merely failed to draw a caret for is still live). Swapping the fold for an independent
`store > 0` gate: **8**, including the explicit agreement claim. Removing the page's new
`clear_rows` call: **2** — the stale-caret defect this arc found, which is what that guard is for.

## The faction page lost its fourth zone (the knowledge screen, slice B)

`docs/plan_knowledge_screen.md` §4 deleted the Know tab, so `band_panel_faction_knowledge` is **gone**
and the page declares THREE zones. What moved here, and why each is not a loss of coverage:

- **`_assert_faction_knowledge_zone` → `_assert_faction_band_zone_blocks`**, asked of the `band` zone,
  because Settling and Discoveries were rehomed there. The craft-track claims went with the tracks to
  `ui_preview`'s `knowledge_panel` chapter. **It is still asked on the `band` TAB and that is a
  constraint of the shell**: the narrow shell parents ONLY the active tab's zone, so a zone read from
  another tab has never been laid out and every one of its rows measures zero — the keyless-row scan
  would report every row keyless.
- **`_assert_faction_band_zone_scrolls`**: on the wide dock the zone's Discoveries, Settling, Mobility
  and Open Borders are all PRESENT inside the sanctioned scroll and the stack overflows its viewport,
  which is what stops the presence claims passing on a box that needs no scroll.
- **`_assert_faction_zone_layout` asserts THREE hosts by EQUALITY**, which is also what asserts the
  absence of a `Zone_knowledge` host — a deleted tab leaving a live column behind it.
- **`FACTION_SHELL_MIN_WIDTH` is two gaps between three columns**, and the equality claim beside the
  bracket is now the only thing pinning that count: the three-zone derivation coincides with a band's,
  so the bracket alone would pass on either.
- **`FACTION_TAB_LABELS` drops `Know`, and that absence is half of what the const asserts** — a strip
  still offering a fourth tab would be offering one with nothing behind it.

**ALL 126 FRAMES MOVED, and every one was diff-boxed rather than re-baselined.** 122 are a single
button- or action-row-sized box: the `▲` the second launcher puts on whichever mount the dock calls
for. The other four are the faction page itself — the band zone gaining two blocks, the body dropping a
column, the tab strip losing a tab — and `band_panel_collapsed`, where the RAIL grew a glyph and the
strip widened 5px through the documented `COLLAPSED_SIZE`-is-a-FLOOR mechanism.

**The faction band zone's extent is read from `_report_zone_content_extent`**: the full block (589px)
against the box it scrolls in, and `_assert_faction_band_zone_scrolls` asserts every block present
inside the sanctioned scroll on the bottom dock.

**A clean run is 221 frames / 1663 `PASS` / 553 `assert OK`, exit 0 — RE-MEASURED, and this line is
the harness's ONLY tally.**

**THE EAT-FIRST RULE'S RETIREMENT MOVED IT 193 / 1456 / 557 → 193 / 1453 / 557** on the work-party
branch: five claims out (the ate line, the deficit line, its supplied-postings absence, its DANGER
ink, the crew line's ink), two in (no row carries a retired line; every party line is quiet ink).

**THE DENY SHEET'S PRE-SELECTED PREY MOVED IT 216 / 1602 / 541 → 219 / 1629 / 547**: three frames
(`band_panel_compose_deny_prey`, `_short_handed`, `_two_prey`), the restored sheet take, short-handed
and pelt claims, and the highlight, pre-selection, clear and chooser claims.

**THE BAND VERBS' TARGET MOVED LAST (issue #529) AND MOVED IT 222 / 1601 / 565 → 216 / 1602 / 541**:
the Deny sheet's per-herd frames became hover-banner claims on three frames, and the sheet-content
claims about the take line, the caveat and the seeded party went with the sheet rows they judged.

**THE BAND VERBS (issue #529) RE-MEASURED IT AT 222 / 1601 / 565**, on a tree that had also taken main
since the last figure, so the move is not one change's delta. The verb block it added is "The BAND
VERBS block" below.

**PR #738's REVIEW FIXES MOVED IT 216 / 1526 / 566 → 220 / 1533 / 566**: the returned-shipment pair
(`trade_tab_shipment_cancelled`, `trade_tab_shipment_returned`), the anchor-gone close
(`trade_tab_list_anchor_gone`) and the after-layout tier (`trade_tab_wide_full_after_render`).

**MERGING #731's TRADE TAB WITH MAIN MOVED IT 193 / 1454 / 559 → 216 / 1526 / 566**, re-measured on the
merged tree rather than summed: the Trade tab's states (`tools/band_panel_trade_tab.gd`, run last) on
top of main's.

**THE POOL CARD'S `ⓘ`/`⚠` SPLIT (issue #716) MOVED IT 192 / 1440 / 556 → 193 / 1454 / 559**: the
`band_panel_pool_kit_short_builders` frame, the per-card state claims, the driven tool-line fork, the
tool-count needle's positive and negative, and the covered-bill Roadwork hover claim.

**THE POINTER-CUSTODY GUARD MOVED IT 1438 → 1440 `PASS`**, frames and `assert OK` unchanged: the
pushed-event liveness claim and the *nothing foreign reached the viewport* claim, asked once at
`_finish`. The behaviour, the two adversaries it was falsified with and why a quiet machine cannot
reproduce the class by moving the mouse are in `test-harnesses.md` → "A SIMULATED GESTURE IS NOT
HERMETIC" — it is the shared windowed-harness contract rather than this harness's own, and all six
pixel harnesses take the seal. **Two of this harness's mechanisms belong to it**: `_drive_click`
pushes its press and release with no awaited frame between them, and every simulated event goes
through the one `_push_input` wrapper the guard discriminates on.

**THE CARAVAN PASS AND THE HUNT VERB'S RETIREMENT took out one frame and five `assert OK` and
added two `: PASS`** — five hunt-compose frames out and four denial-hosted layout frames in, the hunt
sheet's chart guards out (the `assert OK`s), and the work-party block re-staged for the caravan plus
the footer claim in. The two sections at the foot of this file have what each claim can tell apart.

**THE WORK PARTY's BLOCK ADDED two frames, six `assert OK` and fourteen `: PASS`** — two frames
(`band_panel_work_party` and its narrow twin), whose bounds trios account for all **six**
`assert OK`s, and **fourteen** `: PASS` — thirteen on the block itself plus
`_assert_work_party_rows_fit_the_zone`, which reports as a claim rather than as an `assert OK`.
The section at the foot of this file has the fixture and what each claim can tell apart.

**THE POOL-TOE SUB-UNIT FIX MOVED IT 183 / 1323 / 536 → 184 / 1328 / 542**, re-measured rather than
summed: **+1 frame** (`band_panel_keepers_tools_short`), **+5 `PASS`** (`_assert_pool_toe_rounding`'s
`_assert_pool_toe_sub_unit_shortfall` block is four of them, the new frame's
`_assert_work_inspector_fits` the fifth) and **+6 `assert OK`** (the frame's bounds/content-fits pair,
its width claim, and `_assert_keeping_remedy`'s three `print`-reported answers — that producer lands
in the `assert OK` tally for the same reason `_assert_keeper_warning` does).

⛔ **THE `PASS` COUNT WENT DOWN (1328 → 1323) AND NO CLAIM WAS LOST** — `docs/plan_pool_toe.md` §3
retired two kit PICKERS and every claim that could only be made about a control, while §5's pool TOE
added a new set. Retired: the queue strip's kit picker (its derivation, its `(default)` mark,
`_assert_build_kit_command_grammar`'s two command claims and the FLOW pair
`_assert_queue_settings_flow` / `_assert_queue_settings_predicate`), the work inspector's Upkeep
picker (`_assert_unstated_upkeep_kit_falls_through`'s three, and the roster/`none`/`(default)` half
of `_assert_kits_section_draws_its_control`), and the queue HEAD-vs-ROW kit AGREEMENT
(`_open_queue_row_kit_face`), which needed two readings and now has one. Added: the pool TOE's
four-card SET, `_assert_pool_toe_filled_is_not_absent`, `_assert_pool_toe_shared_item` and
`_assert_pool_toe_rounding`, and `_assert_pool_toe_inflection`'s nine. **A tally that falls is a
thing to explain, not a thing to adjust.** An earlier arc removed one frame
(`band_panel_faction_knowledge`) and added none, and measured 126 / 404 / 685 on its own; the
build-queue arc (#576) landed in `main` in between and the two sets of numbers were neither
branch's. **That is the "RE-MEASURED, never summed" rule arriving through a MERGE rather than
through a commit** — two correct tallies, both stale the moment the branches met, and adding them
gets a third wrong answer. Re-run after any merge that touches this harness, and write the answer
HERE: a second tally further down the file is a second thing to forget, which is exactly how this
one came to be read 170 / 1161 / 511 while the harness was in fact running 180 frames.


## `tools/band_panel_preview.gd` / `.tscn`

Dev-only preview harness for the **Band/City dockable panel**: instances the real `BandCityPanel` +
`HudLayer`, injects the panel into the HUD, pushes a seeded player band through
`update_band_alerts`, and dumps the panel docked left/right/top/bottom + collapsed on one dock of each
ORIENTATION (`band_panel_*.png`) so the chrome + the relocated band detail + the HUD reflow can be eyeballed
without a server: `scripts/preview.sh res://tools/band_panel_preview.tscn`.

**It isolates BOTH prefs files** — `NarrativeForkPanel.config_path_override` *and*
`BandCityPanel.config_path_override` (the dock/collapse/TAB prefs). Without the second one the
harness read whichever narrow-shell TAB the previous run left selected — so the band-zone frames
silently rendered the work or parties zone instead — and then wrote its own tab walk back over the
player's `user://band_city_dock.cfg`. Any state that judges a specific zone in the NARROW shell must
still `set_active_tab` explicitly (`DEFAULT_TAB` is `work`). Disclosure states drive the REAL click
path: `_click_disclosure` emits `meta_clicked` on the live vitals label with the very `[url]` meta
its own text carries, never a poke at Hud state.

**The same herd FIELD-PAIR guard rides here** (`_guard_frame_herd_fields`, called from `_save`):
both of this harness's managed herds — the mid-build-pen aurochs and the under-contained corral —
shipped setting only the ownership-gated `herders_needed`, so any compose sheet opened on them would
floor the INVESTMENT rungs' worker cap on an absent `herders_needed_if_managed`; they now set both
through `_set_managed_herders`, a deep scan over every herd dict the HUD holds fails the run when
`herders_needed_if_managed < herders_needed` **or when a herd with a non-zero gated count does not
carry the two EQUAL** (a gated count above zero means the herd is already managed, so the would-be
crew is the same crew — the two sim functions differ only by the gate it has passed), and the pass
line carries the scanned count so a vacuous scan is visible.

**The frame set is a STRICT BIT-IDENTITY REFERENCE — 78 frames — EXCEPT that SIX of them were caught
drifting during issue #450, and the flake is INTERMITTENT.** Measured over three consecutive pairs
on one build: two pairs came back 77/77, one differed in exactly
`band_panel_dockrow_{bottom,left,top}` · `band_panel_shell_{at,below}_threshold` ·
`band_panel_wide_ultrawide`.

**Those six are precisely the states that RE-PIN the canvas or the window**, i.e. the WM-maximize
race this row already describes; the defence against it is frame-count-sensitive, so anything
changing how many frames the run takes to reach the first `_pin_window` can shift its odds.

**Whether it predates #450 is UNESTABLISHED** — settling it needs a pre-change baseline pair, which
that change was not able to run. It is a property of the harness and not of any panel: all 194 + 168
assertions pass on every run, and the other 72 frames were stable across every pair measured.

**Do not read a diff of those six as a regression without re-running the pair.** The set is
otherwise held together by the same three-part treatment `ui_preview` carries: `Engine.time_scale =
0.0`, the canvas trio (`_pin_window` / `_stabilize_canvas`, which ASKS for `project.godot`'s
maximize and undoes it so every run takes one path / the `_capture` geometry guard, measured against
the per-state `_pinned_size` since this harness re-pins for the ultrawide, threshold and 1920×1080
dock-row states), and the two prefs files it already isolated. The RE-CHECK RULE found exactly one
live animation here: the turn orb's `0.5 - 0.5 * cos(t)` breath, which DEGENERATES to its faintest
at phase 0, so `_pulse_time` is seeded to a quarter period — it draws only while the orb has no
attention entries, which is why `band_panel_no_idle` was the only frame drifting. Both MapViews are
`visible = false` (data only) and **no `Tween` is ever created**, since nothing here drives
`TellingPanel`; if a state ever does, it must be stepped or flushed, because a Tween at `time_scale
0` never advances at all. The freeze moved 12 frames, all checked: 10 differ by a 4-px antialiasing
sliver on one glyph, `band_panel_no_idle`'s breath re-baselined to its midpoint, and
`band_panel_left` GAINED the zoom rail it had been capturing before the chrome finished laying out.

**`_assert_crew_edit_keeps_improvement`** (on `band_panel_work_policy_investment`) pins the
OPTIMISTIC OVERLAY's second axis: a crew edit on a row mid-Corral must leave `improvement` intact in
the pending entry, or the board flashes the build off for a turn and re-advertises the rung already
being built.

**`_assert_open_strip_reaches_the_map`** (on `band_panel_dockrow_ultrawide_empty`) is the harness's
one CLICK-THROUGH claim, and it exists because a PNG cannot carry it: a horizontal dock's strip is
mostly live map now, and whether `PanelRoot` eats the presses aimed at it is pixel-invisible. Real
presses through `Viewport.push_input` against this harness's own `_unhandled_input` (the
`ui_preview` event-dock idiom), with bare canvas as the precondition, both gaps beside the card as
the claim, and each ISLAND's own surface — the card's chrome RING and the chrome cluster's bare
column — as the complement.

**The harness's full-rect backdrop `ColorRect` is `MOUSE_FILTER_IGNORE` for it**: at the `Control`
default it swallowed every press that missed the panel, so the harness's own decoration would have
failed the claim whatever the panel did (the same fix `ui_preview`'s backdrop needed).

**The card's INTERIOR is deliberately not probed** — a press into the work board's blank area
reaches `_unhandled_input` even though `PanelCard` is `STOP` and covers it, and neither a `STOP`
child of the card nor a `STOP` sibling behind it closes that hole, so asserting it would pin an
engine behaviour this panel does not decide. Sabotage-verified: restoring `PanelRoot`'s `STOP` fails
it three ways, naming both gaps and the filter.

**IT REPORTS THROUGH THE FAMILY CONTRACT** (see "The exit status IS the verdict"): every failure
goes through `_fail`, which prints `band_panel_preview: FAIL — …` and counts, and `_finish()`
derives the status from that tally, so `$?` and `grep FAIL` cannot disagree. `_assert_band_panel` is
a thin front for the same sink — `PASS` on success, `_fail(label)` otherwise.

**The hang guard is the ONE reporter outside that contract**, and it must be: a `Watchdog` sibling
node (`tools/preview_watchdog.gd`) prints `band_panel_preview: FAIL watchdog — …` and quits 1
itself, because the case it exists for is the harness script being dead — there is no `_failures`
left to increment and no `_finish()` left to reach. Its token is `FAIL watchdog — `, NOT the
family's `FAIL — `, so a scanner keyed to the family separator reads a 180 s stall as a clean run.

**It does NOT share `ui_preview`'s chapter-loading defect** — it loads no chapters — but it does
share the shape underneath: the whole run is one long `await`ing `_ready()` ending in
`get_tree().quit()`, and any of the three scenes/scripts it `preload`s failing to compile takes its
own parse down with it, leaving the root scriptless and the process idling forever. `_settle`
reports progress and `_finish()` disarms the guard, and its 60 frames are byte-identical with the
guard in place.

**THE HARNESS'S TALLY IS THE ONE AT THE TOP OF THIS FILE AND IS DELIBERATELY NOT RESTATED HERE** — a
second tally is a second thing to forget, which is the rule that line states about itself. A clean run
still exits 0 with ZERO `FAIL` lines; for how many frames and claims that is, read the tally above.
**What follows is the DELTA HISTORY** — kept for what each step COST, never for the absolutes it
quotes: every figure in it is frozen at the moment that step landed, several were already behind the
harness when written, and summing them gives a wrong answer by construction. (It was 379 / 570 / 115
before the WORK TAB READ THE LEG IN FLIGHT — `band-city-panel.md`
→ "THE ROW'S PERCENTAGE IS THE LEG IN FLIGHT'S". That step is **+1 frame and +17 `: PASS`**:
`band_panel_rung_track_legs` and the four-state block below, whose remaining two `assert OK`s are
that frame's own bounds/content-fits pair. The `assert OK` delta reads +3 rather than +2 and the
`PASS` one +19 rather than +17, which is the recorded figure's own drift rather than two lost claims —
the paragraph below already records this tally as having been behind the harness twice.) (The figures
before it were RE-MEASURED into an emptied `ui_preview_out/`.) The figure recorded here before the
MATERIALS-ONLY WORK ROW read 260 / 420 / 104 and was already 119 / 150 / 11 behind the harness, so
that change's delta is not recoverable by subtracting them: it is **+2 frames, +8 `assert OK` and +6
`: PASS`** — `band_panel_work_material_forage` / `_crops`, each contributing its own shell / bounds /
content-fits / width-fits quartet and `_assert_work_row_states_its_materials`' three.

**It was 240 `assert OK` lines, 341 `: PASS` ones and ZERO `FAIL` ones, over
94 frames.** (It was 236 / 332 / 92 before the KEEPING POOL landed — `docs/plan_standing_upkeep.md`
§2.5, the arc that took maintenance off the tile. Its two frames are
`band_panel_upkeep_mode_spread` / `_priority`, worth their two bounds/content-fits `assert OK` pairs
and `_assert_upkeep_mode_control`'s four `PASS` each, with the fund-mode NEGATIVE on the reference
band as the ninth. **The under-herded A/B moved and did not grow**: its fixtures now vary the HERD's
pool share where they used to vary a per-source `maintain` crew, which is the same three claims about
a different measurement.) (It was 226 / 332 / 89 before the under-herded ⚠ was re-aimed at the KEEPING crew: the
`band_panel_keepers_short` / `_staffed` pair is two of the three frames and four of the ten
`assert OK`s — each state's bounds/content-fits pair — with `_assert_keeper_warning`'s three answers
and `band_panel_unbuilt_rung`'s three (its own pair plus `_assert_unbuilt_warning`) being the rest.
**Those three report through `print`, not `_assert_band_panel`, so they land in the `assert OK` tally
and the `: PASS` one is unmoved** — which is exactly the shape that makes a `PASS`-only count read
this arc as having added nothing.) (Five of those `PASS`es are `_assert_work_material_readouts`, the board half of closing
the inedible quarry's `+0.00`; four more are the party PACK's, on `band_panel_worst_case_party`; one
**four more are the KIT REPRICING's material arm**, inside
`_assert_kit_reprices_the_source` — the per-material rate by RATIO, its no-op twin at the reference
tier, the take through `expected_materials` at a crew below the saturating one, and the plant web's
own row on the no-retreat patch beside it. **They are named rather than numbered from an offset** —
this tally moved twice in one merge, once for arc #527 and once for the partly-equipped arc, and a
claim of the form *"the five above 310"* survives neither. It was 328 before the material arm; the
`assert OK` count is unchanged, those four riding an existing state's driven block rather than a new
frame.)


**THE WORST-CASE PARTY CARRIES THE PACK CLAUSE, and its fixture holds TWO piles of one material at
two ratings.** A batch is one pile at ONE RATING, so a fixture with one batch per material would pass
just as well against a producer that summed them — which is the retired trade scalar rebuilt out of
its own replacement. The fourth claim is therefore a NEGATIVE: the two amounts' sum must not appear.
The clause rides the `Carried:` row rather than adding a line, so the strip's measured extent
is unchanged (`band-city-panel.md` → "The parties strip's lines"), and the row is found by its
own prefix rather than by index — a producer that moved the clause elsewhere then fails instead of
passing on whichever line happened to sit there. (It was 233 / 313 / 91 before arc #527 retired the `trade_goods` yield axis. **Three
frames went with the band's Trade vitals row** — `band_panel_trade_expanded_left` /
`band_panel_trade_zero` / `band_panel_trade_short_tier` — taking their three `assert OK` PAIRS and
their `_assert_forage_trade_counted` / `_assert_trade_row_reads_zero` /
`_assert_trade_row_absent_in_short_tier` helpers with them. **The three WORK-board frames were KEPT
and RE-PURPOSED**: `band_panel_work_trade_rows` / `_inspector` / `_totals` keep their names and stage
an inedible quarry — a frame that stopped asserting anything is worse than one that fails, so the
states that could still say something real were re-pointed rather than deleted. Their wolf row read
`+0.00 /turn` for one release and now reads **`+0.22 hide`**, off the assignment's resolved
`material_yield`.)

**`_assert_work_material_readouts` makes THREE claims, and the deer beside the wolf is why they
bite**: the wolf must state its hide; the deer must be UNCHANGED (so "always print the materials"
cannot pass); and no material term may reach the deer's sentence, which is the
render-only-when-non-zero rule asked one account further out. Its subject is the RESOLVED
`material_yield` — **not** a rate the compose sheet would project, because a worked row reports what
a turn actually credited. (`labor-ui.md` → "AN INEDIBLE QUARRY QUOTES WHAT IT PAYS" has the three
fields and which surface reads which.) **The three figures are MEASURED from a run, never summed** — band fission retired one
family of frames here and added another in the same merge this arc landed in, so two arcs' deltas
added by hand is exactly how a tally stops matching its harness. (The retired **"start a life
here"** arrival verb had five frames here — `band_panel_settle_offered` / `_withheld` / `_confirm` /
`_too_small` / `_blocked_both` — and the `_assert_settle_*` family. Issue #511 replaced the verb with
**band fission**, so those went and three took their place: `band_panel_split_ready` /
`band_panel_split_too_few` / `band_panel_split_blocked_both`. **The `blocked_both` state is the one
that earns its keep**, exactly as its settle predecessor did — it is the only frame where BOTH worker
floors hold at once, so truncating `split_blocked_reason` to the first sentence fails there and
nowhere else. Its band is squeezed to 7 workers on purpose: on the shared 16-worker fixture a split
small enough to fail the new band's floor still leaves 14 at home, so one composition cannot trip
both.) (The ACTION REGISTRY's own block is worth sixteen `PASS` and the COLLAPSED RAIL thirty-three —
see "`_assert_action_registry`" and "The collapsed rail's two frames" below.) (**A new frame costs
TWO `assert OK`s, not one** — `_assert_zones_within_bounds` and `_assert_zone_content_fits`, one of
each per state; the content-fits line names its state and the zone-rect line does not, which is what
makes the second easy to miss when counting a log by eye.) (**The split sheet is opened through the
real verb**: `_open_split_sheet` closes any open verb, selects the panel band on its own hex, requires
`verb_enabled(split)` and emits the panel's REAL `action_invoked(VERB_SPLIT)` — writing the pending
verb directly would pass against a Split that no longer opens anything, which is the regression the
frames exist to catch. Every sheet assertion reads `_sheet_root()`, the selection drawer's allocation
host.) (The
only `ERROR:` lines in a clean log are Godot's own shutdown RID-leak noise, which is why the status
is the verdict and an `ERROR:` count is not.) (The two tallies are no longer equal, and that is not
a miscount: `_assert_scroll_only_where_sanctioned` and `_assert_band_columns_ignore_content` each
emit several `PASS` lines under one `assert OK` heading.) (Count the `PASS` tally as `: PASS`, not a
bare `PASS`: one `assert OK` line contains the word in its own text — `4 rung marks are hoverable
(tooltip + PASS)` — so a bare grep answers one too many. One earlier record read
**112** where a measurement of the same build reads **111** — one high by exactly that miscount.) Both
of the errors this harness used to expect are gone: the 11-frame `Zone_band` 25px overflow (issue #374
re-homed the band zone's optional rows and widened the wide shell's flanks) and, after it,
`band_panel_parties_inspector_wide`'s `Zone_parties` pair — one VBox needing 310px of a 300px box,
reported twice, once by `_assert_zones_within_bounds` and once by `_assert_zone_content_fits`. That one
is closed by tightening `PARTIES_INSPECTOR_LINE_SEPARATION` (`band-city-panel.md` → "The parties
strip's lines"), and **`band_panel_worst_case_party` is what keeps it closed**: it stages a denial
party carrying every optional detail line at once, asserts the strip really renders all of them
(`WORST_CASE_DETAIL_LINES`; a shorter strip fits, so every assertion goes green on a state that has
stopped measuring anything) and PRINTS its extent.

**`band_panel_vitals_worst_case`** is the state that pins it — one band carrying EVERY optional
vitals row at once in the height-capped TOP dock, which no fixture had ever staged, run through the
bounds assertion, `_assert_zone_content_fits` and the new **`_assert_merged_food_row_fits`** (the
SHORT tier's merged `Food … · 128.4 fodder` line measured against the column it must not wrap in —
353px of 380). Beside them **`_report_zone_content_extent` PRINTS each zone's content extent against
its box** rather than asserting: it reads 299 of a 300px box, and a near-miss and a comfortable fit
are the same green line otherwise. The rows, the merge and the flank widths are specified in
`band-city-panel.md` / `band-readouts.md`.

**Its fixture carries the band's whole FODDER LEDGER, not just the stock.** The Fodder row grew the Food
line's other three beats (`fodder_need` / `fodder_income` / `turns_of_fodder`), which makes it by some
distance the LONGEST optional vitals row a band can hold — so a worst case seeding `fodder_store`
alone stopped being a worst case the moment the row grew, and `WORST_CASE_FODDER_NEED` /
`_INCOME` / `WORST_CASE_TURNS_OF_FODDER` follow the same longest-form rule every other constant in
that block does. The need deliberately outruns the income, so the frame is the WARN state as well as
the widest one: at this tier the row is MERGED, so what it renders is ` · 128.4 fodder` in amber. **The
tally did not move** (805 `PASS`, no frame count change) — the merged clause is the same width it
was, which is the check that the widening did not reach the SHORT tier's measured column.

> ⛔ **THE PER-SOURCE CARRY AXIS IS DELETED, AND THIS BLOCK RECORDED ITS ARRIVAL** (issue #543). It
> read: *"the PER-SOURCE CARRY AXIS contributes SEVEN `PASS` to `ui_preview` … two of the seven are
> the husbandry-hint pair becoming a 2×2 (both kits against both a wild herd and a pen, since the pen
> tier is gated on the SOURCE now); the other five are `_assert_a_pen_prices_on_the_keepers_carry` …
> in `ui_preview` exactly FIVE frames move and they are precisely the corralled-herd compose sheets —
> `hunt_crew_herders` · `herd_pen_self_feeding` · `herd_pen_extending` · `herd_pen_foddered` ·
> `improvement_done_penned` — each in the Kit hint alone, now `pen 12.0 per keeper` where it read the
> stalking kit's attack and sled."* `EquipmentStat::PenCarry` is gone — **carry is carry** — so those
> five hints read the stalking kit's attack and sled again, which is where they started.
>
> **THE PRICING BLOCK SURVIVES WITH ITS EXPECTATION INVERTED; THE 2×2 DID NOT SURVIVE AT ALL.**
> `_assert_a_pen_prices_on_the_hunters_carry` still drives `DrawerComposeController`'s real seam and
> now requires a herd and its corralled twin to price identically, with a **bare-handed** party under
> the reference at both as the liveness half. **Net +2 `PASS` on `ui_preview`** (1589 → 1591): 14
> pen-axis claims removed, 16 added.
>
> ⛔ **THE 2×2's PEN COLUMN WAS A TAUTOLOGY AND IS DELETED** (1597 → 1594 `PASS`, no frame moved).
> This paragraph read *"`_assert_the_hint_reads_the_same_at_a_pen` still takes four readings and now
> requires the fence to move NOTHING while the KIT moves the line"*, and the code did not: deleting
> `EquipmentStat::PenCarry` also took `tier_hint`'s source parameter, so its two "penned" readings
> were the BYTE-IDENTICAL call the wild pair makes and the corralled twins were handed nowhere. Two
> claims that cannot fail unless the two above them already have assert that a control is PRESENT,
> not RIGHT. What is left is `_assert_the_hint_states_each_kits_own_items` — the two WILD readings,
> which are real — and **"the fence moves nothing" is now STRUCTURAL for the hint**: `tier_hint` has
> no argument a pen could arrive through, so a penned reading is not expressible and rebuilding one
> means re-adding the parameter #543 deleted. The fence still reaches real code through the PRICING
> block, which takes the herd. Rationale at the assertion, in `chapters/compose_rungs.gd`.
>
> **THE PEN AND THE VANTAGE JOINED `BandKitTiers`** and the vantage half of that still stands — a
> Scout card read 2 tiles of sight against a reveal at 1 while the client answered off the ROSTER's
> fresh tier. The pen half is deleted with the axis; `_assert_the_appended_axes_read_the_band` keeps
> its fresh/worn PAIR shape, re-aimed so a DRY crook shows as dry beside an untouched sled's own haul.

**The dock harness is untouched because its own `_band_fixture` publishes no `kit_tiers` at all** —
the whole-row absence, which is the one case the roster still answers and the reason that branch
survives. Sabotage-verified by restoring the roster read on both axes: exactly the two worn claims
fail, naming `pen 40.0 per keeper` and `2.0`, i.e. the original defect in its own words.

**The FORECAST QUERY arc's client half.** Measured AFTER it merged with the per-source carry axis,
the expanded TOE roster, the role cards' kit picker above and the appended-axis pairs: `ui_preview`
is **281 frames / 688 `PASS`, exit 0** and `band_panel_preview` **87 frames / 227 `assert OK` / 259
`: PASS`, exit 0**. The `ui_preview` tally rose from 670 in two steps and **NEITHER MOVED A FRAME IN
EITHER HARNESS**, which is what says each was byte-identical everywhere but on the case it fixed.
The hunt quantiser's collapse into one expression added seven PNG-less claims to `chapters/hunt.gd`
(`labor-ui.md` → "THE `max(1.0)` SITS ON THE CARRY ARM ALONE" and "THE CARRY CLAMP IS CHARGED PER
BODY"); the retreat's reach into the crew answers added seven more (`_retreat_crew_assertions`) and
**inverted two `band_panel_preview` claims that encoded the doctrine it reverses** — the dock
chart's hold crew and the kit sheet's cap, which now MOVE with `dispersion` rather than standing
still — so that harness's tally is unchanged while two of its lines say the opposite of what they
used to.

**Sabotage-verified on three DISJOINT mutations, each failing a different set and only one of them
moving frames**: the retired two-branch form fails the played 4.80/0.36 pair (2 claims, 0 frames);
an averaged-then-clamped kill fails the cadence pair (2 claims, **33 frames** — every herd compose
sheet whose take or holding rate sits below one body); and `engage_workers` restored to the raw
reach fails the crew-invariant trio (3 claims, 0 frames). The two arcs' own pre-merge figures are
recorded in their paragraphs above and are NOT additive with these — the merge retired the
sampled-party and estimate-honesty claims from one side while the other added the role-card and
pen-axis ones. The `ui_preview` `PASS` tally rose from 632 with the arc's REVIEW pass, which added a
PNG-less chapter (`chapters/forecast_seam.gd`, LAST, so it moves no frame) carrying eleven claims
about the seam itself — the world boundary, the two failure classes, and which end of the plateau
the party cap seeds on. All three are invisible in every frame the harness takes: a stale answer
renders as an ordinary forecast, a refusal renders as an ordinary refusal, and a cap one worker
either way renders a plausible stepper. Sabotage-verified on FOUR mutations failing DISJOINT sets —
dropping `ForecastQuery.reset()` from `HudLayer.reset_world_state` fails the three world-boundary
claims, pinning the retry predicate FALSE fails the transport retry alone, pinning it TRUE fails the
server refusal and the no-spin claim, and an off-by-one in `expedition_useful_cap` fails the plateau
claim alone. The `band_panel_preview` `PASS` tally FELL from 257, and the drop is retirements rather
than losses: `_assert_party_ladder_rounding`, `_assert_party_past_the_rungs_is_quoted`,
`_assert_denial_quoted_party_note` and `_assert_denial_party_needed_skips_horizon` all described the
SAMPLED party axis the query replaced — a raid is costed for the party on the stepper now, so there
is no rung to round to and no rung to name. The frame COUNT is unchanged in both harnesses.  **MOST OF `ui_preview`'S RAID FRAMES MOVED, AND THAT MOVE IS THE ARC.** Every expedition
readout, every trip verdict and every Send face went from the pending placeholder to real numbers —
the sheets ask now and the canned answerer answers. Nothing else moved.  **Three things a harness
has to do differently once the forecast is a QUESTION**, all learned by watching them fail:  - **A
band fixture without `band_id` asks NOTHING.** Every asker refuses to compose a question about a
band holding `HudConst.NO_BAND_ID`, so a fixture missing it renders the pending placeholder in place
of every raid readout on a HUD that is behaving correctly. `BandFx.with_band_id` stamps it (entity +
`FIXTURE_BAND_ID_OFFSET`, so the two handles differ) and every band fixture in both harnesses goes
through it — `band_panel_preview` reads the offset off `BandFx` rather than restating it. -
**Restaging one herd id with a DIFFERENT table changes the sim's answer without changing the
question**, so `ask` is never re-entered and the frame renders the state before the swap.
`band_panel_preview._set_world_herds` therefore calls `forecast_query().reset()`. A live sim cannot
reach that — a herd's numbers move, its identity does not. - **`ForecastFx._herd_for_id` picks the
source that CARRIES the asked-for table**, because the two harnesses disagree in opposite
directions: `ui_preview` selects herds it never pushes through `update_herds`, and
`band_panel_preview` pushes tables under an id whose SELECTION is stale. Either order alone renders
one of them out of the state it was staged into.  **The one state that needs NO answerer is the one
that needs it uninstalled.** `band_panel_deny_hover_two_prey` clears the sender before its hover (a
still-installed answerer would take the question and land the reply during `_settle`), renders,
asserts, and reinstalls. It is the only
frame in either harness showing the in-flight state, which is a real one — measured live at **1264
ms for the first query of a session** and 48-63 ms once warm.  **The INTERFACE-SCALE arc's additions
to this harness, and the three rules they cost.** `band_panel_preview` is **87 frames / 227 `assert
OK` / 278 `: PASS`** and `ui_preview` **279 frames / 639 `PASS`, exit 0** once the BAND-WIDE ROLE
CARDS' KIT PICKER had landed on top of the FODDER FACE (#449), the EXPANDED TOE ROSTER, the KIT
OFFER TEST and the GEAR BREAKDOWN's three new rows.

**The role-card picker contributes ONE frame, TWO `assert OK` and THIRTEEN `PASS` here and nothing
to `ui_preview`** — `band_panel_role_kits` (the LEFT dock, both cards) with its bounds/content-fits
pair, `_assert_role_card_gear`'s six, `_assert_role_cards_are_level`'s two and
`_assert_role_kit_command_carries_the_pick`'s five. The figures before it were **86 / 225 / 265**.

**The gear breakdown contributes ONE frame, ONE `assert OK` and EIGHT `PASS` to `band_panel_preview`
and nothing to `ui_preview`** — `band_panel_kit_expanded` (the dock's own Kit popover, opened on the
reference band) and `_assert_gear_breakdown_states_every_kit`, which asks each of the three new rows
both what it must say and what it must not; the `assert OK` is that state's own bounds check.
`ui_preview`'s tally is unchanged because the three rows are ADDITIONS to a popover its
`band_kit_expanded` / `band_kit_bare` frames already render — those two moved, and one existing
husbandry-hint expectation was re-pointed at the shared fixture's own handling-gear condition (the
chapter used to graft the item on, and grafting a second row for an item the fixture now ships would
shadow it). The figures before it were **85 / 224 / 257**.

**The offer test contributes THREE frames and NINE `PASS` to `ui_preview` and nothing at all to
`band_panel_preview`** — `herd_kit_offer_red_deer`, the same sheet with the picker OPEN (a closed
`OptionButton` face names the selected kit alone, so only the popup can show a withheld row and the
reason on it) and `herd_kit_offer_rabbit`, all three in `chapters/compose_rungs.gd` over a
locally-built roster, `BandFx.kit_roster_fixture()` carrying neither a trapping nor a pen-axis kit.
The dock harness is untouched because its own roster carries no mass-bounded weapon and its quarry
no pen, so every kit on every one of its sheets is offered exactly as before. Rationale in
`labor-ui.md` → "A KIT THAT CANNOT WORK ON THIS PREY IS GREYED". The fodder face contributes ONE
frame and 2 `assert OK` / 12 `PASS` here (`band_panel_work_fodder`, whose two zone assertions are
the `assert OK` pair, whose `_assert_work_fodder_readouts` + the paired negative on
`band_panel_work_trade_totals` are eight of the `PASS`es, and whose review pass added
`_assert_work_sort_tiers`' four), and ONE frame and 3 `PASS` in `ui_preview`
(`forage_fodder_standing`). It also adds FIVE `PASS` and NO STATE to `map_preview`, while MOVING
four of its frames: `_assert_yield_label_component` drives the one-slot fall-through directly, a map
badge rendering a plausible label whichever account it chose — and the four frames are the ones
built on `_snapshot_work`, which gained the hay Field that renders the label (`map_band_work` ·
`map_band_pending` · `map_band_label_overlap` · `map_worked_ready`). The EXPANDED TOE ROSTER
contributes the last two `PASS` in `ui_preview` and NO frame: its husbandry-hint pair
(`chapters/compose_rungs.gd`) drives `KitRoster.tier_hint` over a locally-built roster,
`BandFx.kit_roster_fixture()` carrying no pen-axis kit. The figures before the two of them were
**84 / 222 / 245** and **275 / 625**, reached once the interface-scale arc, the faction page and the
CRAFTABLE-KITS arc (#493) had all merged — 80 + main's 4 faction states, and main's 269 + our 5,
plus `event_dock_narrow_band` (1 frame / 3 assertions) from the review of #497.

**The craftable-kits arc contributes 16 `PASS` here and 6 in `ui_preview`, and ZERO frames to
either** — 229 + 16 and 619 + 6, which is how that merge was checked: a tally that is exactly the
sum of the two sides is evidence no claim was dropped resolving it, and a tally that is not is the
first sign one was.

## `_assert_action_registry` — one registry, three mount points, PNG-less

**Sixteen `PASS`, no frame** for the two expanded mounts (259 -> 275 here; `ui_preview` untouched at
713), plus **thirteen** for the COLLAPSED one (`_assert_collapse_re_home`, below). Nothing in it is
visible in a picture: a panel one button wider is a plausible panel, a bar that reserved a row for
nothing renders as a slightly taller card, and a glyph drawn on either row looks deliberate. The
behaviour it guards is specified in `band-city-panel.md` -> "The action registry is ONE list with TWO
mount points".

It runs **LAST and PNG-less**, for the tier probes' own reason — it registers and retires actions and
re-docks the panel, so anything rendered after it would render against a panel the run never chose. It
restores the shipped `⚒` registration and the LEFT dock on the way out.

- **EVERY MOUNT CLAIM IS A PAIRING** (`_assert_action_mount_pairing`, two `PASS` per call): the `⚒` on
  the mount the orientation calls for AND absent from the other, plus the bar measuring a row only
  where it carries them. A one-sided form — "the glyph is on the bar" — passes on a panel that lost
  the button entirely, and a glyph-only form passes on one that mounted it twice. It is asked four
  times by this block: on the LEFT dock, across the runtime **re-home** LEFT -> TOP, and across the
  re-home back — and four more by `_assert_collapse_re_home`. **It asks all THREE mounts every time**,
  so a rail that kept a copy of the expanded chrome's buttons fails rather than reading as "the glyph
  is there".
- **The WIDTH claims** (LEFT dock) register a second action through the REAL `register_action` and
  assert the subject row's minimum width and the docked card's are unmoved (302.0 / 326.0, against the
  380px the dock reserves), with the **vacuity** that the action ROW's own minimum did grow (30 ->
  71px) — otherwise both hold because nothing was added.
- **The HORIZONTAL claims** assert the body sits **flush** under the subject row with actions
  registered (gap 0.0px, against 44.0 on the vertical dock — the paired positive), and that retiring
  every action does not move the TOP dock's strip by a pixel (395 -> 395), with the vacuity that the
  glyph really did leave the row. The registry costs a horizontal dock nothing in either state.
- **The EMPTY-BAR claims** are back on the vertical dock, which is the only place the bar is ever
  live: hidden and measuring 0, the body flush under the subject row, and the band zone getting the
  bar's **44px back** (759 -> 803) — a vertical dock has no strip to grow, so the zone is what pays.

Sabotage-verified on three mutations. Pinning the mount to the BAR (the layout this replaced) fails
**three** — the LEFT -> TOP pairing pair and the horizontal flush claim; dropping `_refresh_action_mount()`
from `set_dock` fails **the same three**, the re-home being what both mutations break (the walk back
to LEFT then finds the mount already where it wants it, so that pairing stays green). Pinning the
mount to the SUBJECT ROW fails a DISJOINT **nine** — both vertical pairings, the bar vacuity, both
width claims naming the pre-change **340.0 / 364.0**, both flush claims and the return-leg pairing.

### The collapsed rail's two frames, and `_assert_collapse_re_home`

**One frame added (`band_panel_collapsed_bottom`, beside the existing `band_panel_collapsed`) and
thirty-three `PASS`** — twenty on the two frames, thirteen in `_assert_collapse_re_home`. The
behaviour is specified in `band-city-panel.md` -> "The collapsed rail runs along the dock's plentiful
axis".

**The two frames are a PAIR and neither is worth rendering alone**: the arrangement that is right on a
tall left rail (stacked) is the one that pushes the restore toggle off a 46px horizontal one, and a
rail showing its glyph looks like a rail in either thumbnail. `_assert_collapsed_rail(where,
expect_stacked)` therefore asserts the axis BOTH ways round — stacked on the orientation that wants
it, on one line on the other — over the glyph, the `⚒` and the restore toggle, and pairs it with the
justification claim (trailing-end on a horizontal rail, centred in the strip on a vertical one).

Three supporting claims, each covering a way the geometry passes while being wrong: the controls are
inside the RESERVED STRIP rather than merely inside the card (a card grown past its own reservation
puts them off-screen while still "containing" them); each is a full `ICON_BUTTON_SIZE` square, since a
button clamped to nothing is inside every rect; and the strip the panel LAYS OUT equals the size it
REPORTS reserving. That last one caught a real defect during the change — a bottom dock laid out at
128px, the LEFT rail's stacked minimum, because the anchors ran before the rail was re-pointed.

`_report_collapsed_rail_headroom` PRINTS rather than asserts, on both frames: what the rail spends of
its cross axis (where `COLLAPSED_SIZE` is a floor) and of the long axis the verbs accumulate on —
**108 of 1128px on a LEFT dock and 103 of 1550 on a BOTTOM one, i.e. ~31 and ~45 more verbs**.

**`_assert_collapse_re_home` runs at the end of `_assert_action_registry`, PNG-less**, and walks
collapse/expand on both orientations: the mount pairing on each of the four legs, a PRESS on the rail
driven through the button's own `pressed` (found by walking the RAIL, never by id off
`_action_buttons` — that dictionary holds whichever mount built it, so an id lookup would drive the
BAR's copy and report the invoke as proof the rail works), and the gate travelling with the verb — a
predicate answering false renders disabled on the expanded mount AND still disabled once the rail
rebuilds the button, with an ungated action on that same rail as the vacuity.

Sabotage-verified on four DISJOINT mutations. Pinning `_header_rail.vertical` true (the stacked rail
both ways, i.e. the reported defect) fails **exactly two** — the horizontal frame's axis pairing and
its justification claim, naming `right 974.0 vs rail 1733.0`. Making the rail not a mount fails
**ten**: four mount pairings, both rail presses (`no probe button on the collapsed rail to press`),
the LEFT frame's axis claim and its card-fits-strip claim. Returning the flat `COLLAPSED_SIZE` from
the cross-axis size fails **exactly the two** card-fits-strip claims, one per orientation, naming
`54.0 within 46.0` and `56.0 within 46.0`. And a mount that ignores the `enabled` predicate fails
**exactly the two** gate claims. **The pairing is what makes the first of those two-sided**: a
one-sided "they share a line" passes on a rail that lost the glyph.

**`_assert_band_columns` compares the RAW strip again** (360 / 335 against the two consts), which is
itself a registry claim: a horizontal dock mounts its actions on the subject row, so a bar leaking a
row there fails those two before any mount assertion runs.

**(1) COUNT FRAMES FROM A RUN INTO AN EMPTIED `ui_preview_out/`.** It is gitignored and never
cleaned, so PNGs written by an experiment that was measured, rejected and REVERTED sat on disk and
were counted by an `ls` — a figure went into this file as 81 when the run produced 79.

**(2) `_pin_window` verifies the LOGICAL VIEWPORT, not `window.size`, and FAILS rather than
warning.** Every assertion here is measured against the logical viewport, so a resize that has not
landed leaves measurements mid-flight (a `2600×928` window under a pinned `1920×1080` canvas gives a
logical 3025), and a `push_warning` in a 500-line log is invisible in a harness whose value is
bit-identity. `_release_canvas_pin()` exists for the same reason: a state that pins a CANVAS and a
later one that pins only a WINDOW must not leave the second rendering at a projection nobody chose.

**(3) AN ASSERTION PHRASED IN THE IMPLEMENTATION'S OWN TERMS IS NOT COVERAGE.** Twice in one arc a
green claim failed to see the thing it named change: the rail's alignment was asserted as "strip end
less `_bound_trailing`", which held whichever way the bound was applied, and the card's centring the
same way — sabotage put the card 419px off with all four centring claims green. Both now claim the
VIEWPORT's edge. Also from that arc: `_right_dock_content_reach()` measures each card CLIPPED to
`RightScroll`, because a card taller than its box keeps its full height and hangs out of it (1193 in
a box ending 1056), so a bare rect reports content that is never painted; and the promise walk
DERIVES its fork rather than hard-coding it, having been written as `left + right ceiling` and so
would have walked 2432 once the trailing charge was dropped — 560px clear of the real 1871, passing
while measuring nothing.

222 `assert OK` + 229 `PASS` before the CRAFTABLE-KITS arc (issue #493) added sixteen `PASS` claims
and no `assert OK` — the kit repricing's own six in `_assert_kit_reprices_the_source` (two about the
ratio's denominator, two about the retreat's own `stay_fraction` field, and the end-to-end take/cap
pair) and ten more across the compose sheets' own claims.

**Both steps moved ZERO of this harness's frames**, measured by stashing each change and
re-rendering: no rendered fixture publishes `stayFraction` and `BandFx.kit_roster_fixture()` ships
no `dispersion` at all, which is exactly why those claims are DRIVEN over a locally-built two-kit
roster rather than rendered — a fixture-driven version would compare a kit against itself. The
repricing step did move 83 of `ui_preview`'s frames, all drawer compose sheets, and that split is
itself the finding: only one frame in the whole set can see whether the repricing is live. 194
`assert OK` + 163 `PASS` before the FACTION ROLLUP PR's REVIEW PASS added five `PASS` claims and no
`assert OK` — three on the parties row's NAME LINK (that pressing it leaves the page at all, that it
lands on the party's HOME BAND rather than on the party, and that the page is restored afterwards
for the states below), one on the caret setup's own precondition (that it really reached the page
with a band selected, without which the caret claim under it is vacuous), and one on the keyless-key
scan, which until then measured nothing: `clip_text` floors a `Label`'s minimum at ONE PIXEL rather
than zero, so the `size.x <= 0.0` test it shipped with passed on a fully clipped column — verified
by sabotage, which came back `0 keyless` with the key squeezed away. 187 `assert OK` + 138 `PASS`
before the FOUR-ZONE body (issue #450, option A1) generalized `BandCityPanel` from three named zones
to a declared ordered list and gave the faction page a `knowledge` column — that step added ONE
state (`band_panel_faction_knowledge`, so 78 frames) and, net, six `assert OK`s and nineteen
`PASS`es. The `assert OK` six are the new state's bounds/content-fits pair,
`_assert_work_zone_readable` on `band_panel_faction_wide` (which the wide faction frame had never
called) and the three-line `_assert_faction_shell_threshold` block's two bracket lines plus one
readability line. The `PASS` nineteen are `_assert_faction_knowledge_zone`'s six,
`_assert_faction_zone_layout`'s two, the threshold block's derivation claim, the SETTLING
meter-filled claim — and eleven that are NOT new: `_assert_faction_type_scale`'s two came back after
being repointed at the KNOWLEDGE zone (they had gone to a single FAIL when the tracks left the work
zone, since a summary row's name is a Button and the Label-pair walk then found no rows at all), and
the remaining nine are the run's own count settling. A THIRD pass in the same PR RETIRED THE
TOP-RIGHT HUD BLOCK the faction page had replaced — `TurnBlock`, `TopBar` and all eight of their
Labels are deleted from `HudLayer.tscn` — and it moved this harness by one `assert OK` and two
`PASS`es (193/161 → 194/163).

**Both changes are on `band_panel_dockrow_top`, and both are the retirement's geometry rather than
new coverage.** That state's card had 1920 − 360 (left dock) − **419** (the readout block's LIVE
width) = 1141 and picked the NARROW shell; with the block gone the trailing bound is the right
dock's own ~344, so it has 1216, picks the WIDE shell, and `_assert_work_zone_readable` now runs
there and passes — the extra `assert OK`.

**`_assert_card_clears_hud_columns` had to learn to RE-RENDER, and that is a real repair rather than
a repoint**: it clears the bounds to check its own negative control, but the card's width is built
from a column count the CONTROLLER declares, and `_affordable_work_columns` caps that count against
the BOUNDED span — so the "unbound" card came back with the count granted under the bounds (one
column, 1190px, centred) and cleared both columns, correctly refusing to prove anything. It
re-renders on each side of the bound change now, which is what re-grants the count against the full
strip; its right-hand region is the RIGHT DOCK rather than the retired readouts, that dock being
what a top-docked card now shares a vertical band with. `ui_preview` needed four repoints of its own
for the same reason and they are recorded in its row. A follow-up pass in the same PR took the
page's rows to the `band` zone's VITALS size on Ray's eye (see `band-city-panel.md` → "THE TYPE
SCALE IS THE PAGE'S OWN VITALS ROWS", which retires the head→row-step rule this file's own
type-scale note used to record) and gave SETTLING a real head with its reading on a row — which put
the knowledge zone 36px over its 300px box. That added `PASS`es (157 → 161): the SETTLING head/row split became two claims where the folded readout was one, the row
size gained its live comparison against the vitals label, and the tier gained its two-sided pair.

**The tab-strip claim is a REPLACEMENT, not an addition** — the old one asserted `set_tab_label` had
renamed one word, the new one asserts the whole declared strip by equality. Sabotage-verified on two
DISJOINT mutations: hard-wiring `_wide_separator_span()` back to TWO gaps fails **exactly one**
assertion, the four-zone derivation, naming `1569` against `1544` — the bracket beside it is
self-consistent under that mutation and stays green, which is the demonstration that only the
equality claim can see it; and dropping the `knowledge` descriptor from `FACTION_ZONE_LAYOUT` fails
**six**, each naming what it found — the tab strip, the zone's existence, the host order
(`["Zone_band", "Zone_work", "Zone_parties"]`), the knowledge column's flank (380 against 354, the
expanding zone's width answered for a key the layout no longer carries), the derivation (1190
against 1569, correctly following the shorter list) and the type-scale zone's own declaration.

**That last one is a GUARD rather than a claim, and it earned its keep in the same run**: the
type-scale walk dereferences the zone, and an unhandled error inside this harness's one long
`await`ing `_ready()` ABORTS the whole run — so without it a subject that had stopped declaring that
zone would take every later state down with it instead of naming itself. 181 `assert OK` + 117
`PASS` before the FACTION PAGE (issue #450) added three states and eighteen assertions — the three
states' bounds/content-fits pairs account for the six new `assert OK`s, `_assert_faction_page`'s
eleven (its nine plus `_assert_faction_type_scale`'s two) and `_assert_faction_cycler`'s seven for
the rest. 179 + 114 before the FILL TARGET's retirement, which removed two
`_assert_band_panel` calls — the dock sheet's offers-a-fill-target claim and the quarry chooser's
drops-the-stale-target one — and repointed a third. 174 `assert OK` + 106 `PASS` before
`band_panel_worst_case_party` — that step gained FIVE `assert OK`s and seven `PASS`es, and only
three of the five are new assertions: the other two are `band_panel_parties_inspector_wide`'s bounds
and content-fits pair, which had been reporting `ERROR` and now report `assert OK`, so a reader
counting only the new state's own would come up two short. 175 `assert OK` + 91 `PASS` before the
recall-verb pair — that step LOST an `assert OK` and gained
fifteen `PASS`es, the old `_assert_row_recall_confirms` having printed its own raw `assert OK` line
where its replacement reports through `_assert_band_panel`, 175 + 77 before the sampled party AXIS's
three guards, 173 + 67 before the unsampled-party guard,
168 + 67 before the band zone gained the `Kit` row and the SHORT tier's Morale+Growth merge, 164
`assert OK` + 57 `PASS` before the KIT PICKER's three states, 164 + 56 before the map path's
whole-cohort claim, 164 + 53 before its Kit-row claims, 162 + 46 before the collapse verdict's five
clause shapes, 162 + 43 before the `horizon` guard, 160 `assert OK` + 38 `PASS` before the denial
sheet's short-handed refusal, 158 + 33 before the DENIAL raid's deep-party pair, 96 before issue
#460 added the work-sort claims, 132 before issue #377 added the ultrawide dock-row pair and its
four island assertions, 148 before its click-through guard (`_assert_open_strip_reaches_the_map`),
and 147 `assert OK` + 15 `PASS` before the DENIAL raid's first two compose states — **re-record BOTH
numbers in the same PR whenever you add or remove an assertion**, or the next reader cannot tell a
new assertion from a lost one, which is the only judgement this figure exists to support. The two
tallies are separate because they come from different reporters: `_assert_band_panel` prints `PASS`,
everything else `assert OK`, so a count of one alone silently misses half the suite)

## Worked-source mark states (issue #412)

**`band_panel_preview`** — `band_panel_rung_ready` / `band_panel_rung_ready_filter`, with five
assertions (the mark is SELECTIVE: two of three rows offer a rung) plus two for the forage jump naming
the LAND. Both jump assertions are mutation-tested.

## The DENIAL raid's states (`docs/plan_denial_raid.md` slice 2)

The Deny sheet carries party and kit, and its herd arrives one of two ways (`targeting.md` → "THE BAND
VERBS' TARGET IS THE LAST STEP"): PRE-SELECTED by a click on a highlighted herd while the sheet is
open, after which the sheet states every readout and its Send commits; or, with no prey, as the click
the Send arms, the verdict then riding the hover banner. The banner claims are made on
`TargetingController.banner_text()` after `HudLayer.notify_hex_hovered`, each by EQUALITY on the
banner's tail (`_assert_banner_states`) — half of every claim is what the line must NOT also say.

- **`band_panel_compose_deny`** — the sheet on the band's drawer: party, kit, a plain live send, and
  the floor surfaces ABSENT (`_assert_denial_sheet_carries_no_floor`, the heading matched
  **upper-cased** because `alloc_section_label` upper-cases what it is given).
- **`band_panel_deny_hover`** — the send armed and the boar hovered: the range verdict, leading with the
  expectation, both ends shifted by `DENIAL_OUTBOUND_TRAVEL_TURNS`, then the travel split. The leg is a
  constant derived from the fixture's OWN geometry (band (71, 18), boar (75, 18), 2 tiles a turn ⇒
  `ceil(4 / 2)` = 2) rather than asked of `outbound_travel_turns`.
- **PNG-less on the same banner**: the `repelled` verdict (the SAME boar with only its table swapped —
  `_set_world_herds` resets the forecast seam, since restaging one herd id with a different table
  changes the answer without changing the question), the zero-travel verdict on the herd standing on
  the band's own tile (no travel split), the open-high shape on its own party size (the sheet's stepper
  set and re-rendered, which re-captures it into the armed pick), and the SHORT-HANDED refusal: the
  reference band's three idle against a requirement of eleven reads
  `denial_short_handed_reason` in the banner, and the click on that herd sends nothing and leaves the
  pick armed. Each hover is followed by a `_settle`, which is where the canned answer lands.
- **`band_panel_deny_hover_two_prey`** — a warren and a wolf pack on one hex, hovered with the answerer
  uninstalled: `Rabbit Warren +1 more · <DENIAL_FORECAST_PENDING>`. The shared tile is used because the
  seam serves a cached answer for a subject it has already costed, so only a herd no hover has asked
  about can show the in-flight state. `_assert_quarry_chooser` then clicks the hex: the chooser opens
  and nothing is sent, it lists exactly the two, and choosing the wolf through the popup's REAL
  `id_pressed` sends ONE raid at the wolf.
- **THE PRE-SELECTED PREY** — `_assert_prey_highlight` holds the passive herd highlight up while the
  sheet is open (`TargetingController.is_preselect_on`, `need: "herd"` at `QUARRY_NO_REACH_BOUND`,
  never targeting) and down once it closes. **`band_panel_compose_deny_prey`**: a click on the boar
  sends nothing, leaves the band selected and the pick unarmed, and the Prey row states it with its
  `✕` (`_assert_prey_is_stated`); the party is re-pinned after the pre-selection, which SEEDS it.
  `_assert_denial_sheet_viable` then reads the sheet by EQUALITY — the range verdict, the take line
  (`kills … · brings home … food · leaves … on the range`, composed from the VOCABULARY and the
  fixture's own arithmetic, never through `denial_take_bbcode`), the estimate caveat, and a plain live
  Send — and `_assert_denial_pelt_take` pins the inedible quarry's take on the producer (pelts, no
  false `0.00 food`). `_assert_prey_send_commits`: the Send sends ONE raid at the prey, arms no pick,
  closes the sheet.
- **`band_panel_compose_deny_short_handed`** — the deep-party herd pre-selected on the reference band's
  three idle: Send DISABLED as `Not Enough Hunters`, the reason naming both numbers, the counted repelled
  refusal not printed beside it (`_assert_denial_sheet_short_handed`). The Prey row's `✕` then clears
  the prey, and the Send goes back to arming the pick.
- **`band_panel_compose_deny_two_prey`** — a pre-selecting click on the two-herd hex opens the chooser;
  choosing the wolf makes it the prey and sends nothing, and the Prey row's `⋯`
  (`HudWidgets.QUARRY_CHOICES_META`) lists both and marks the wolf (`_assert_prey_chooser`).
- **`_assert_denial_click_commits`** — one herd on the hex, so the click sends exactly one
  `send_denial_raid` with the sheet's party and kit, the pick comes down, the sheet closes, and the
  band is still the selection.
- `ui_preview` **`expedition_denial_panel`** (`chapters/band_expedition.gd`) — the launched party's
  drawer, the at-the-herd clock UNSHIFTED **plus the negative that the launch wording appears nowhere
  on it**.

`_assert_denial_turn_clause_shapes` drives `denial_turns_clause` over constructed forecasts for the
shapes no table here stages — a turn clause is a string, and a banner renders a plausible sentence
whichever draw it led with. It also pins the IN-FLIGHT span, the span being chosen once for the whole
clause.

**Two fixture rules the denial tables must follow**, both because a fixture that breaks one makes the
assertions decorative: a row's `delivered_food` is what the PACK holds and everything else killed is
`wasted_food`, and a `repelled` table's kill counts are small but **non-zero** — a repelled party is
outbred, not incapable. **A fixture that states one half of a sim-side pair must state the other from
the same split.** The requirement is the forecast reply's own `party_needed`, searched server-side,
so the client never derives it from rows.

`cargo xtask command-guard` carries the half neither preview can: it opens the Deny sheet, presses its
send (arming the pick), clicks the herd, and parses the emitted line with the REAL server parser, which
is the only thing that can assert the four-token grammar. **Each mission's send wears its OWN meta** —
the missions emit different signals with non-interchangeable payloads.

### The band zone's TIERS, and why the two tier probes run LAST

`_assert_growth_row_not_merged` runs at TALL and at COMPACT, and both probes **resize the canvas and
re-dock**. Run mid-file they flipped `band_panel_arrivals_top` out of its 300px `Zone_band` into a
265px `NarrowZoneHost` and overflowed it — a panel left in another shell silently re-renders every
state after it in the wrong one. They are therefore the last thing before `_finish()`, where there is
nothing left to perturb, and they re-push the worst-case band and select the `band` tab explicitly
(the narrow shell renders ONE zone, and the run above leaves whichever tab its last state selected).

`_report_zone_content_extent` now names the TIER beside the band zone's extent, on both hosts the zone
can render into. An extent quoted without it is a number whose content nobody can reconstruct: the
SHORT tier renders three fewer rows than the TALL one.

### The KIT PICKER's two states (`docs/plan_denial_raid.md`)

`band_panel_compose_deny_kit` / `_kit_open`, specified in `band-city-panel.md` → "The KIT row rides the
Deny and Scout sheets". What the harness half has to get right:

- **`BandFx.kit_roster_fixture()` is the ONE roster and BOTH preview harnesses plus `command_guard`
  drive it** — `band_panel_preview` and `command_guard` `preload` `tools/ui_preview/fixtures_band.gd`
  for it. It is world config the sim publishes
  once, not a per-harness prop, and two copies could quote different tiers or a different job default
  while the `kit <id>` token is asserted against one of them. Every entry states all THREE tiers with
  the BARE value on each axis its kit does not use, which is the wire's own shape and is what
  `KitRoster.unequipped_tier` reads the bare-handed tier off; `none` is authored LAST, exactly as
  `equipment.json` authors it.
- **`band_panel_preview` shares a SECOND fixture module, and every patch and herd row here goes
  through it**: `tools/ui_preview/fixtures_rung.gd` stamps each fixture's `current_rung` off its own
  flags (`test-harnesses.md` → "A fixture's STANDING RUNG is DERIVED, never typed"). The stamp rides
  the fixture FUNCTION's return — `return RUNG_FX.stamp_herds([...])` — which is one place per
  function to forget rather than one place per row, and a row re-dialled afterwards is re-stamped
  where it is re-dialled.
- **The kit frames use their own band** (`_kit_worn_band_fixture`), not the shared one:
  `DetailFormat.band_states_kit` is a bare `has()` on the spears key, so putting durabilities on
  `_band_fixture` lights the `Kit` vitals row in 13 other states and overflows `Zone_band` — the note
  `_kit_band_fixture` already carries. Its SLED is dry and its spears are not, so the picker's hint is
  assertable as the EFFECTIVE tier rather than the roster's fresh one.
- **`command_guard` composes a NON-DEFAULT kit on every path**, because `Main._kit_token` omits the
  tail at the job default and the omitted line is byte-identical to the pre-roster one. The composed
  id must be written BEFORE the send is pressed: the send captures the sheet's values into the armed
  pick, so a selection written afterwards is not the one the click carries.
- **The Rust half ASSERTS the kit, and the drives alone did not.** Each entry is
  `{kind, line, expected_kit}` (`_record` takes the expectation off the drive's own payload, and
  FAILS when a drive composed the job default — that line's assertion could never fail), and
  `xtask/src/command_guard.rs` compares it against what `parse_command_line` recovered. Without it a
  `Main._kit_token` regressed to `""` left every line parsing, every `EXPECTED_KINDS` count intact
  and the gate PASSING while no kit reached the server. It also refuses a run in which **no** line
  expects a non-default kit, the vacuity twin of the differing-handles precondition. `kit_failure` is
  pulled out of the loop and unit-tested (`cargo test -p xtask`) over lines the real parser produces,
  so the regression is reachable without launching Godot.

## A ROLE PASSES **TWO** GATES, AND THE GUARD DRIVES EVERY ONE OF THEM NOW

**A role the text grammar did not name was unstaffable for a whole slice.** Reported from live play: a
staffing `+` did nothing and the event dock read *"Not connected to the server."* The client was
connected. `sim_runtime::command_text`'s `assign_labor` role match enumerated a closed set and returned
`UnexpectedToken` for anything else — and the native bridge (`bridge/command.rs` → `send_line`)
**parses a line before it sends it**, so the command was rejected locally and never reached the
socket.

- **A role has to be admitted TWICE, by two files that do not know about each other**: the text grammar
  in `sim_runtime`, and the sim's own `handle_assign_labor`. A claim that the sim parses a role is true
  of the handler and says nothing about the grammar.
- **THIS GATE EXISTS TO CATCH EXACTLY THAT, AND IT PASSED**, because its band-wide-role drive was a
  single `scout` line. An assertion that passes because the case was never asked is the failure mode
  this repo keeps re-learning; a guard whose whole purpose is "the client's lines parse" must ask about
  **every** line the client can build, not one representative.
- **The drive is a sweep over `ASSIGN_LABOR_ROLES`** — scout · warrior · roadwork — each with a
  non-default kit. **The retired roles** (`RETIRED_ASSIGN_LABOR_ROLES`: agriculture · husbandry ·
  quarrywork · builders, spelled as strings because their `HudConst` constants retired) are asserted to
  build NO line (`docs/plan_site_crews.md`, `docs/plan_labor_at_scale.md`).
- **Every site verb is driven in every form it takes** (`docs/plan_site_crews.md` §2.4):
  `work_priority` in the patch, herd and working forms, `unqueue` in all four (patch, herd,
  working `<x> <y> <material>`, road `road <x> <y>`) — counted per form in `EXPECTED_KINDS`, and driven
  through the controls' own emitters so a payload missing `material` or `road` fails the parse check as
  the patch it would name. `work_priority`'s road form is asserted to build NO line, and
  `_assert_retired_kit_verbs` asserts `Main` has no `format_build_kit` / `format_upkeep_kit` and the HUD
  no signal for either.
- **`_assert_every_role_is_emittable` closes the other direction**: every listed role builds a real line
  from `Main.format_assign_labor`, and an unknown role builds NOTHING — so the list is a list rather
  than a builder that accepts anything.
- **What it cannot do is stated in the code rather than left implied.** Nothing in GDScript can see a
  role added to `format_assign_labor`'s `match` and *not* to this list — that arm is literals with no
  reflectable set. What it does catch is the direction this defect actually travelled: a role the
  builder and the sim both know and the text grammar does not.
- **`EXPECTED_KINDS`' count is re-derived from the role list at runtime**, because a `const` initializer
  cannot call `Array.size()` — attempting it is a hard parse error that hangs the guard SILENTLY, which
  is this family's documented failure mode arriving on the fix for another one.

Sabotage-verified by reverting a Rust role addition: the guard fails naming the form and the token —
`the server parser REJECTED 'assign_labor 0 71204 <role> 2' — UnexpectedToken("<role>")`.

## `command_guard`'s SHIPMENT drive asserts an AMOUNT, not a handle (arc #527, issue #517)

`_drive_send_trade_expedition` is the one drive whose subject is a number. Everything else in this
gate asks *"is this the right band / the right kit"*; a manifest asks *"is this an amount the band
actually holds"*, and a client that gets it wrong emits a line that parses, names the right band and
carries the right kit — every other assertion in the file green.

- **The piles are FRACTIONAL and authored in the sim's own TICKS** (`TRADE_FOOD_HELD_TICKS`
  21_050_001, `TRADE_FODDER_HELD_TICKS` 84_050_001, `TRADE_HIDE_HELD_TICKS` 4_567_891), so the
  entry's `cargo_held_ticks` and the Rust
  half's comparison are exact — a decimal here would re-round the very thing being compared. The food
  pile is adversarial TWICE: a tenth rounds it up to `21.1`, and flooring it onto the fixed-point grid
  alone still emits `21.050001`, which the parser's `f32` round-trip lands one tick ABOVE the pile.
- **THE HAY PILE IS DRIVEN BECAUSE THE TWO COMMODITIES LOOK ALIKE** (issue #590). Food and hay are
  both `is_material` false, so the only thing keeping a hay press out of the `food` token is the
  manifest row's own `id` — and the Rust half looks each emitted token's pile up BY NAME
  (`sim_runtime::commands::FODDER_CARGO_KEY`), so a `fodder` amount checked against the food pile
  fails here rather than in play. The pile also sits on hay's own coarser scale (~84 units against
  food's ~21). That pile is what once drove `CARGO_LOAD_MAX_PRESSES` to 128; since the row gained a
  `Max` button (issue #620) the hay drive is **one press**, and the constant came back down to **32**.
  **It is a TOOLING ceiling and never a statement about the step** — moving it must not be confused
  with changing `COMPOSE_CARGO_STEP`, which is a game-design lever.
- **The rows are loaded two ways, and the split is deliberate.** Hay goes through its `Max` button
  (`_load_pile_with_max`), which is the control a player reaches for at that scale and which must
  *disable* itself on the press — a `Max` that answered with nothing would leave the drive emitting a
  zero it never noticed. **Food and hide stay on the `+` loop**, because `Max` floors to a tenth and
  the whole subject of the Rust half below is an exact fractional holding (`21.049995`,
  `4.567889`) surviving the `f32` round trip. Driving every row with `Max` would leave that gate
  comparing whole tenths against whole tenths — vacuous against the case it exists for.
- **A press clamps to the row's CEILING, not to the pile.** `_set_cargo_amount` takes a
  caller-resolved `row_max` (`min(held, remaining pack space ÷ this row's weight)`), so `+` greys out
  when the *pack* is full even where the band holds more. A drive that wrote the manifest itself
  would test its own arithmetic; driving the controls tests the sheet's.
- **The Rust half quantises what the parser recovered the way the SIM does** (`scalar_ticks`, the
  `f32` multiply included — `f64` here would agree with a client the server refuses) and refuses a run
  in which no shipment was composed from a fractional pile, the vacuity twin of the kit and
  differing-handles preconditions: whole units survive any rounding.
- **The destination is the click the sheet's send arms** (`notify_targeting_click` on where the tie
  last saw the band), the same path `ui_preview`'s trade chapter drives.
- **`manifest_failures` is unit-tested** (`cargo test -p xtask`) over lines the real parser produces —
  both rounded spellings failing, the floored one passing — so the regression is reachable without
  launching Godot, the treatment `kit_failure` already has.

## `command_guard`'s OUTFITTING drive — the verb that gained a band (the per-band loadout arc)

`set_starting_loadout` used to address a faction and default to its band, so it was not in this gate
at all. Every band has an outfitting window of its own now — the spawned band's grant, and a take on
the home band for every splinter a split makes — so the band is **positional and required**, and this
is the gate that says the client sends the durable `BandId` down it rather than the ECS `entity` the
picker also holds. `xtask/src/command_guard.rs`'s `band_handle` gained the matching arm; the emitted
line is `set_starting_loadout 0 71204 kit big_game 2 material hide 5`, the band token being the
fixture's `BAND_ID` and never its deliberately different `BAND_ENTITY`.

- ⛔ **IT IS DRIVEN THROUGH A ROW'S OWN `+`, AND IT WAS THE FOOTER BUTTON.** That control closed
  nothing and sent everything; the outfitting draft is deleted, so it closes the card and sends
  nothing, and a press on it captures no line at all. The command is composed on a STEPPER press now
  (`_press_row_plus`, by the row's `HudLoadoutVocab.MATERIAL_ROW_META` and then by the stepper's `+`
  face, `HudWidgets.add_stepper_controls` stamping no meta on either button), which is the only way to
  reach the code a player reaches. Still driven LAST: pushing an open window earlier would stand a card
  up over every drive above it.
- **The window rides the BAND fixture** (`_outfitting_band_fixture`), which is what makes the emitted
  band the one this gate already asserts everything else against.
- **The order is one kit line and one material line, and the KIT line comes from a row nobody
  pressed.** The band HOLDS its default outfit — the sim applies it when it makes the band — so the
  window's own `kits` / `materials` are what the card draws, and one press on the material row sends
  the whole allocation: both tails, from one gesture. What is read off the line is the BAND TOKEN; a
  longer order would be more tokens saying the same thing. Both budgets sit above the allocation, the
  client drawing a published one as-is.
- **It carries no `expected_kit`.** `SetStartingLoadout` has no kit AXIS — its `kit <id> <n>` tail is
  an allocation rather than the selection `Main._kit_token` omits — so `kit_token` answers
  `NotKitBearing` and the kit gate correctly says nothing about it.

## The BAND VERBS block (issue #529)

Rendered on the left dock and judged mostly PNG-less:

- **`_assert_band_verbs_registered`** — the five verb ids lead the action registry in
  `HudComposeVocab.BAND_VERBS` order, ahead of `⚒` and `▲`.
- **`_assert_parties_zone_has_no_footer`** — the parties zone carries no verb button
  (`HudWidgets.VERB_BUTTON_META`) and no send.
- **`_assert_band_verbs_dispatch`** — every verb pressed through the panel's REAL `action_invoked`
  reaches `dispatch_verb`, judged by what it does: Move arms the tile pick (under a banner naming the
  band), and Scout, Deny, Trade and Split each open their sheet on the band's own hex with NO pick
  armed; each is closed through the sheet's own close.
- **`_open_verb_sheet`** opens a sheet the way a player does: the panel band selected on its own hex
  (the map's `unit_selected` hop, which this harness has no map to make), then the verb through the
  real `action_invoked`.
- **`_assert_verb_sheet_mounted`** (on `band_panel_compose_scout` and `band_panel_compose_deny`) — the
  sheet is in the drawer of the band it is for (the band is the selection), under the
  `VERB · from <band>` header, and nowhere on the panel. **`_assert_sheet_names_no_target`** — the
  sheet names no herd standing in the world and holds no chooser.
- **`_assert_send_armed`** (on `band_panel_compose_scout`, the frame of the armed sheet) — the send
  armed the verb's pick, the sheet stays open, and the send is drawn pressed.
- **`_assert_escape_cancels_the_pick_first`** — with `Main.escape_claimant` and the real HUD readers:
  armed, Esc is `ESC_TARGETING`; after the cancel the sheet is still open with its send un-armed, and
  the next Esc is `ESC_VERB_FORM`.
- **`_assert_scout_click_commits`** — `notify_targeting_click` on `SCOUT_TARGET_TILE` sends exactly one
  party, to that tile, with the sheet's party; the pick is down, the sheet closed, and the band still
  the selection (a targeting click selects nothing).
- The Deny half is "The DENIAL raid's states" above.
- **`_assert_verbs_gate_on_idle`** (on `band_panel_no_idle`) — Scout, Deny and Trade disabled on the
  bar, Move and Split live.
- **`_open_split_sheet`** opens the split sheet through `_open_verb_sheet` and writes only its stepper.

The action-registry block unregisters the verbs to test an empty bar and re-registers them through
`_register_band_verbs()`, so the states after it see the shipped registration.

## The RECALL VERB pair

Claims no frame can carry — each is about a tooltip or a dialog. The behaviour they guard is specified
in `band-city-panel.md` → "THE RECALL VERB FOLLOWS THE SIM".

**`_assert_row_recall_confirms` is a set of THREE presses, and the set is the claim.** A rule that
showed one verb everywhere satisfies any one of them alone. Each drives the REAL `_build_party_row` and
the REAL `pressed` handler — never `confirm_recall_expedition` directly, the row's own ✕ being where a
caller could drift — and each asserts the verb, the tooltip and the ceremony:

| fixture | differs by | verb | press |
|---|---|---|---|
| `_in_field_expedition_fixture` | — | Recall | dialog, no emit |
| `_in_camp_expedition_fixture` | POSITION alone | Cancel | emit, no dialog |
| `_in_camp_with_report_owed_fixture` | `pending_reveal_count` alone | Recall | dialog, no emit |

- **It pushes its own roster first and restores the previous one after.** `party_cancels_in_camp`
  resolves the home band through `player_band_by_entity`, so a camped party whose band is not in
  `_player_bands` answers "in the field" for the wrong reason — and the state after this block reads
  the roster it was left with (`update_band_alerts` keeps a losing-population diff).
- **It COUNTS the HUD's dialogs rather than looking for one.** `_dismiss_dialogs` frees with
  `queue_free`, which is deferred, so the previous press's dialog is still a child when the next
  assertion runs — measured: the camped party reported `dialog=true` on its first green run.
- Sabotage-verified in BOTH directions, each failing a disjoint set: pinning the predicate TRUE fails
  the field party's three and the report-owed party's three; pinning it FALSE fails the camped party's
  three alone.

## Build crew chips (`docs/plan_labor_at_scale.md`)

A declared build has no list: the Work tab shows it as a crew chip on the site's own row
(`band-city-panel.md` → "THE BUILD CREW CHIP"). The states are in `_render_build_crew_states` (patch,
herd and working rows, with `_crew_patches` / `_crew_chips` staging and finding them) and the road row's
in `band_panel_build_crew_road`; source-level crews come from `tools/ui_preview/fixtures_crew.gd`
(`CrewFx`), dialled once by `BandFx.staff_builders` and re-read by every source that passes through.

- **`band_panel_build_crews`** — every declared build on the board wears ONE chip
  (`WORK_ROW_STOP_CHIP_META`), asserted by COUNT (at least three: a patch, a herd, a working). The hovers
  are asserted by their sentences: `N building (up to M), done in ≈T` on a staffed build,
  `No spare hands — raise a food row's floor or stop a row` where the sim staffed nobody,
  `Short of hoes` on a build short of its own tool, and `Click to stop this build.` on every chip.
- **A REAL click on a chip sends the stop** (`_drive_click`, never `pressed.emit()`, which cannot see a
  covered or zero-size control): the emitted payload is read back through `Main.format_unqueue` and
  must begin `unqueue `. The press leaves a real withdrawal on the optimistic overlay, so the state
  clears `_pending_labor` straight after, or every state below inherits a board missing that chip.
- **`band_panel_build_crew_road`** — the same chip on a ROADWORK roster row: exactly one chip inside
  the roster block, stating no spare hands and the road's own tool shortfall.
- **The SET is the claim.** A chip that always warned passes the hover claims alone, and one that never
  warned passes the staffed one; the staffed, empty and tool-short sources ride one frame.

### The sanctioned-scroll guard

`_assert_scroll_only_where_sanctioned` asserts the work zone's scroll exists exactly where it is
sanctioned: the sections list when the zone is mounted on a band page, and the road roster's list
(`ROSTER_EXPANDED_SCROLL_NAME`) when that expansion holds the zone, never otherwise.

⛔ **EACH OF ITS CLAIMS IS MADE ONLY WHERE ITS ZONE IS MOUNTED.** The narrow shell parents only the
ACTIVE tab's zone (`_reparent_zones` detaches the rest), so a zone can be in `_zones` and nowhere the
walk could find it; asking this guard from a narrow-shell WORK-tab state would otherwise report a
parties list that had "lost" a scroll it was merely detached from.

> **A CLICK FREES THE LIST, NOT ONLY THE ROWS.** A row captured before a click that rebuilds the zone
> is a freed object, and `Control.is_ancestor_of` on a freed instance **raises** — which ends the
> assertion block with **no `FAIL` line**, leaves the state open over everything below, and still exits
> 0 with a healthy PASS count. Re-find from the live tree after every real click.

## The build states and the rollback (`docs/plan_standing_upkeep.md` §4.6a/b)

Two blocks land at the end of the run. Order is load-bearing here as everywhere: each restores the
reference band and the herd roster on its way out, and the rollback block leaves the pending overlay
empty.

**`_render_work_build_state_states` — the WORK ROW's build states, as a SET.** ONE band, FOUR forage
rows differing only in their meter and the wire's own countdown: climbing (`🌱45%`, `SIGNAL_DEEP`),
declared-with-nobody-on-it (`🌱⚠`, `WARN`), losing ground (the same stalled face, from the wire's `-3`
rather than from the staffing), and parked-with-keeping-covered (`🌱60%`, `SIGNAL_DEEP`). All four
faces and all four inks are asserted by EQUALITY, and the faces are composed from `HudWorkVocab`'s own
formats so the claim is the FORK and not the format.

- **The SET is the claim.** A row builder that marked EVERYTHING passes the two stalled claims; one
  that marked NOTHING passes the two healthy ones. Sabotage-verified in both directions, each failing
  a disjoint pair.
- **EACH ROW CARRIES ITS OWN `builders_staffed`, which is what makes four states reachable at once.**
  The declared-with-nobody-on-it row has none, and the others differ by meter and countdown, so the
  states have somewhere to differ on one board.
- **`_assert_work_row_and_badge_agree` is the two-surface claim, and no per-surface assertion can make
  it** — each is perfectly self-consistent while contradicting the other. The row's face comes off the
  RENDERED board (`HudWorkVocab.WORK_ROW_BUILD_STATE_META`, valued the face, because the three states
  differ by one glyph and a text search would only confirm the string already assumed); the badge's
  verdict is composed the way `BandOverlayRenderer` composes it. Its COUNTS —
  four build rows, exactly two stalled — are what stop a hard-wired verdict agreeing with itself.
- **`_work_row_build_faces` keys on the row's NAME label, found by `SIZE_EXPAND_FILL`.** Every other
  slot in a work row is a fixed column, so that flag is the one structural handle on the label whose
  text the assertion is trying to join against.

**`_assert_pending_assign_rollback` — PNG-LESS, and it has to be**: a card showing three scouts is a
perfectly ordinary card. It asserts the two `has_method` names `Main` probes for (a failed probe fails
SILENTLY, so a rename would simply stop rolling anything back), drives the REAL `_emit_assign_labor`
for both writes so the payload under test is the one `Main` receives, then drops ONE and requires an
unrelated pending edit on the same band to SURVIVE. **The survivor is half the claim** — a rollback
that cleared the whole overlay passes the first half and is a worse bug. Sabotage-verified: erasing
the entity's whole record fails exactly the survivor claim.

> **What it does NOT drive is `Main`'s one-line `if not _send_formatted_command(...)`.** `Main` is a
> scene node with a `_ready` this harness does not stand up, so the block drives everything up to and
> including the method that handler reaches by name. The seam it cannot reach is one comparison.

## THE LEG IN FLIGHT — the two-leg fixture existed and could not tell the defect from the fix

**`_track_climbing_patch_fixtures` had staged a two-leg entry since the destination track landed, and
no assertion in this file could have caught the reported defect on it**, because its band's forage row
carried **no `improvement` token**. The wire publishes one — `snapshot::population::resolved_build_job`
is `patch_build_verb`'s answer, which honours a declaration at or above the rung being raised, so a
`sow` on untended ground publishes `sow` — and without it the client's own `build_verb` fell through
to the Cultivate meter and the board read the leg **by accident**. `_track_band_fixture(build_job)`
states it, which is what makes `band_panel_rung_track_climbing` reproduce the played state.

The claim is that the Work tab's two readouts — the row's title (the DESTINATION, what the player
ordered) and the rung mark's leg percentage — name ONE rung, with the wanted percent composed through
the TILE CARD's own producer, so the claim is that two surfaces AGREE and a literal on each side cannot
let both be separately plausible.

**The fixtures a percentage needs, because a row quoting the wrong rung's meter renders a perfectly
plausible row:**

- the reported two-leg `sow`, on `band_panel_rung_track_climbing`;
- **the FIRST turn of banked work** (`TRACK_FIRST_TURN_WORK_DONE`, one unit of fifty ⇒ 2%) — the
  state that actually catches the bug, since at 60% a renderer that had merely swapped one meter for
  another passes;
- **a single-leg `sow` on already-tended ground, which must be UNCHANGED** — its leg IS its
  destination, and without it "read the leg" is satisfied by a renderer that always names the rung
  below the one declared;
- **a PEN RING**, which is not a leg at all: it widens the rung its herd already stands on, so the
  ladder credit is structurally zero for its whole life. Its percentage is the herd's own
  `pen_extend_progress / pen_extend_cost`, and the fixture pair (30 of 40 work ⇒ 75%) differs from the
  herd drawer's `herd_pen_extending` (42 of 70 ⇒ 60%) on purpose, so a row that reached for the wrong
  ring fails on the number instead of coinciding with it.

## THE CROP STEP AND THE SOW'S PRICE ARE ONE BASKET AT TWO WIRE PRICES (§4.15)

`_crop_patch_row(field_work_cost, committed)` is the whole fixture family for the three states
`band_panel_rung_price_cheap` / `band_panel_rung_crop` / `band_panel_rung_price_dear`, and **the two
things it varies are the two the claims are about**: what the WIRE quotes for the Field rung, and
which crop the patch is committed to.

**THE BASKET IS MIXED, and a single-plant one cannot make the claim at all.** A staple holding 70% and
paying food sits beside a cash crop holding 20% and paying **none** — which is the tile that got
committed to tobacco in play, and the shape a picker of names and shares cannot warn about. The
`0.00 food` clause is asserted by EQUALITY against `HudFloraVocab.FLORA_CROP_FOOD_CLAUSE_FORMAT`, so a
row that suppressed the zero (which every other clause on a crop row correctly does) fails.

**THE TWO CROPS CARRY THEIR OWN SOW PRICES, AND THE INEQUALITY IS ASSERTED SEPARATELY.** `38 work`
against `150 work` on one tile is the whole of what the per-crop figure buys, and a picker quoting one
price per PATCH renders a perfectly plausible step — every row carries a number, and it is the same
wrong number — so the faces are pinned by EQUALITY and the *they differ* claim is stated over the
leading work clauses on its own. Both expectations are composed from the picker's OWN clause formats
(`_want_crop_face` / `_want_work_clause`), which pins the figures rather than the wording.

**A THIRD PLANT CARRIES NO `sow_work_cost` AT ALL and must render NO ROW.** Its `can_sow` is
deliberately `true` — that flag is the SPECIES' global ceiling — so the crop step's own presence guard
is the only thing that can withhold it, and a filter written against the ceiling flag would leave it
on the list offering a job the sim refuses at a price it never quoted.

**THE PATCH'S OWN PRICE IS STILL ASSERTED, and it is now a pair with the per-crop one.**
`_assert_rung_sow_price` reads the Field row's `fieldWorkCost` back through
`RUNG_TRACK_COST_UNDATED_FORMAT` — `38 work` on uncommitted ground, `150 work` on the SAME basket
committed to the minority crop — so a Field row quoting a constant passes either state alone. The
fixture passes the COMMITTED crop's own const as the patch's `field_work_cost`, which is the invariant
the sim asserts on the encoded envelope stated from the client's side; neither figure is derived from
the other, and neither is derived from a share.

**THE FIELD ROW STATES NO REASON.** The crop-and-share sentence beneath the price is retired — the
work figure stands on its own now that each crop states one — so `_assert_rung_sow_price` makes the
price claim alone.

**`_assert_ready_mark_declares` WALKS THE STEP RATHER THAN ASSUMING EITHER SHAPE.** The declare board
carries a `sow`, a `tame` and a `corral`, and only the first asks for a crop — so after pressing the
track's target row the harness looks for `_rung_crop_rows()` and, where the card offers one, asserts
**no declaration escaped yet** and then presses the first crop. That negative is the one worth having:
*the step is open* and *the rung committed anyway* are not mutually exclusive, and only the second is
the defect. It is made on the SIGNAL (`improvement_requested`) rather than on the card, a declaration
being what actually escapes.

`_rung_crop_rows` keys on `HudWorkVocab.RUNG_CROP_ROW_META`, spelled apart from `RUNG_TRACK_ROW_META`
so a harness asking *which rung* can never be answered by a crop row that happens to be on screen;
`_rung_crop_faces` reads each row's aside by POSITION (the sibling that follows it), a price-and-payoff
face carrying no identity beyond the row it prices.

Sabotage-verified by returning the destination reading: **exactly six fail** — both readouts on the
two-leg sow, the first turn and the animal twin — printing the played `Sowing 0% · turn 64` and `▦0%`,
while the single-leg control stays green.

## The MATERIALS-ONLY work row — the assertions were fine, the fixtures never reached the state

`_assert_zone_content_width_fits` recurses correctly, scales off the LIVE host size and has ridden
every `_assert_zone_content_fits` call site since the width defect first shipped. It had never seen a
row that overflows, because **no fixture in this file gave a FORAGE assignment a `material_yield` at
all**, and the one two-material fixture (`WIDE_SENTENCE_MATERIALS`) sits on a herd that also pays
FOOD — so the row's retired one-slot fall-through took the food branch there and the material list
never reached a row. The behaviour is specified in `band-city-panel.md` → "THE ROW IS TWO LINES".

- **`band_panel_work_material_forage`** — the reported case, `+0.24 fibre · +0.34 grape` on a patch
  paying no food and no fodder, in the LEFT dock (the shipped default edge and the narrowest box the
  work zone is ever given; the same row on a bottom dock has 789px and says nothing).
- **`band_panel_work_material_crops`** — the measured worst case, a RollingHills-style tile realizing
  all four field-ceiling cash crops.
- **The DEER hunt row rides both as the control.** A row that still pays food must be UNCHANGED, or
  "shorten the rate" would be satisfied by a board that shortened every rate on it. The wolf is
  deliberately left OFF these two boards, so the forage row is the only overflowing one and a failure
  names it rather than whichever row happened to measure widest.

**`_assert_work_row_states_its_materials` claims that BOTH LINES render WHOLE, and that is the half a
width assertion structurally cannot see.** A zone whose content fits says nothing about how the row
SPENT the room: the name Label was allocated **1px** — Godot's floor, not zero — so it stayed a
perfectly valid, perfectly findable node with its `text` intact and every text-based claim about it
passed while it rendered as nothing. Each claim is *it is not elided*, measured against the label's
OWN font at its OWN size (`_label_text_width`, the faction page's keyless-key scan one surface over,
with its `KEYLESS_KEY_WIDTH_TOLERANCE`). That is stronger than the RELATION it replaced — the name
getting at least the retired 46px rate column's width — and it is what the two-line row is FOR: there
is no sibling column left to state a relation against. Two claims ride with them and the set is what
makes any of them worth anything: line two really did state EVERY material (otherwise "the lines fit"
passes on a board that stopped rendering the accounts), and it states them IN FULL rather than naming
one and counting the rest.

**THE LINE-TWO WIDTH CLAIM IS ABOUT THE ACCOUNTS, NOT THE WHOLE LINE, and the difference is the FLOOR
clause.** Line two closes with `50% left standing` (~96px), and the four-cash-crop worst case asks
**332px of a 322px line** with it — so the trim lands on the floor, and a whole-line claim would fail
on the one fixture built to be the widest. `_accounts_clause_width` measures the line less its
trailing clause (`rsplit` with a limit of ONE: the clause joiner and the accounts' own separator are
the same glyph, so splitting forward would cut after the first account instead). What the two-line row
promises is that the ACCOUNTS never have to be cut — the reading the retired 46px slot could not give
at any width — and a fourth claim beside it requires the WHOLE line, floor included, to ride the
label's own hover, because the floor has no other home once the strip's sentence is gone.

**AND THE STRIP MUST RESTATE NONE OF IT** (`_assert_work_inspector_restates_no_accounts`, on
`band_panel_work_inspector_width`). That redundancy is what paid for the second line, so a sentence
quietly growing back would put the work zone over its box on the one state with zero spare. It is the
NEGATIVE half of `_assert_work_row_line_two_states_every_account` — which replaced the identical claim
asked of the STRIP, the long line having moved to the row — and the two are asserted together: "line
two says it" is satisfied by a tab that says it twice, and "the strip does not" by a tab that lost the
accounts entirely. Sabotage-verified as a set: restoring the sentence fails the redundancy claim AND
`band_panel_pools_wide_selected`, the latter naming `short by 16`.

**`_report_work_row_name_column` PRINTS rather than asserts**, and the number is the fork it exists to
surface: the narrow shell's name column measures **146px** (96 under the retired slot) while
`Hunt Thunder Mammoths` — `fauna_config.json`'s widest, stated as `WIDEST_SHIPPED_ROW_NAME` — needs
**164**, so the longest shipped name still elides by 18px in a side dock. No fixture on this board can
say that for itself; every row here is `Forage (nn, nn)` or a short species, so the un-elided
assertions pass on a column half this wide. Whether 18px is answered by a shorter row format, a wider
flank or nothing at all is a design decision, and a failing assertion would state one of those answers
— the `_report_zone_content_extent` rule.

**The accounts Label is reached from the NAME by walking up to the row and back down by
`WORK_ROW_ACCOUNTS_META`** (`_work_row_accounts_label`). The two live on different lines of one row,
so nothing but the row's own `PanelContainer` encloses both, and a second text match over the panel is
free to land on another row.

**THE WOLF ROW SHIPS TWO MATERIALS AND THE FIXTURE CARRIED ONE.** `fauna_config.json` gives the wolf
`hide` and `bone`, and one material fitted the retired 46px slot with room to spare — so the row that
SHIPS was the untested one. `WORK_ROW_MATERIAL_ROWS` states both, which put
`band_panel_work_trade_rows` / `_inspector` / `_totals` **2px over the 356px box** under that slot:
the same defect one web across, at a twentieth of the size, predating the selective-gather work
entirely.

**Sabotage-verified** while the accounts were still a 46px slot, by reverting the elide and the cap
together (i.e. the shipped pre-fix behaviour): **42 failures**, naming the mechanism rather than a
symptom — the wolf's three states 2px over, `band_panel_work_material_forage` 9px over with `the row's
NAME is allocated at least the rate column's own width (1 of 46)`, and
`band_panel_work_material_crops` **172px** over, its column asking 528 of 356, with every `+` and `−`
on the board reported outside the box. With the accounts on their own line both states read
**354 / 356**, the name **146 of the 91 its own font needs**, and the four-crop line **322 of 218** —
i.e. the defect is not merely bounded, it is unreachable at the shipped roster's widths.


## The RANK's frames — one board carrying all three marks, with the picker open on it (§4.9 item 9b)

`_render_work_priority_states` renders three states, and the FIRST is the one this arc keeps having to
re-learn the need for.

- **`band_panel_work_priority`** — the REQUIRED combined state: a LEFT-docked Work tab whose board
  carries a **HIGH row, a NORMAL row and a LOW row at once**, with the inspector open on the low one
  and the **priority picker showing in the same frame**. Two disjoint frame families with the defect
  living in the gap has hidden three separate defects in this arc already (the strip's own reserved
  height, the queue strip vs the inspector, the materials fall-through); a marked-rows frame beside a
  picker frame is that shape again. `_assert_work_row_priority_marks` asserts both halves together —
  exactly HIGH and LOW carry a mark, the NORMAL row carries none — because "the marked rows carry a
  prefix" passes on a board that prefixes every row and "the normal one carries none" passes on a
  board that dropped the feature.
- **`band_panel_work_priority_floor_swap`** — the same row with the FLOOR picker in, reached by a real
  press on `Change policy` while the priority picker was standing. **The mutual exclusion is
  unassertable from any single-picker frame**: two pickers that are never asked for together always
  look exclusive. The claim is that the priority picker is *gone, not merely covered*
  (`_find_meta_control` answers null), and the reverse press is driven too so neither picker is the
  privileged one.
- **`band_panel_work_priority_widest`** — the four-cash-crop worst case with a HIGH mark on it. It is
  what the LEADING placement exists for, and `_assert_marked_row_accounts_still_fit` prints the split:
  **310px of a 322px line, 69 mark + 241 accounts**, with the accounts allocated 253 — so they still
  render whole and the trim still lands on the trailing floor clause.

**⛔ EVERY PRESS IS A REAL PRESS.** `_press_work_inspector_link` finds the inline link by FACE (an
inline link carries no meta, and the four faces are fixed vocabulary) and drives it through
`_drive_click`; `_assert_work_priority_click` drives each of the three picker buttons the same way and
asserts the **command LINE** `Main.format_work_priority` produces, not the payload dict — so the token
order, the source form and the level word are judged as the socket would see them. All three levels
are driven, because a picker that sent one level whatever was pressed satisfies any single-button
claim. The link is a TOGGLE, so the helper presses it only when the picker is shut, which after the
first pick is every time (committing closes it — itself part of the contract, and asserted here rather
than in a frame of its own).

**The picker's LIT test is `_assert_lit_rung`'s**, the primary stylebox's own background colour, so
the two pickers cannot be judged by two different notions of *selected*. Which button is primary is
invisible in a thumbnail — three buttons in a row look the same — and a picker that lit nothing would
read as a row with no rank at all.

**`_assert_work_inspector_worst_case_fits` now stages the PRIORITY picker**, which is the taller of
the two by its hint line. The two are mutually exclusive, so the strip's documented ceiling is a max
over the pair rather than a sum; staging the floor picker there would understate it by that line and
leave `WORK_INSPECTOR_CEILING_HEIGHT` describing a state that is no longer the worst one.

**AND THERE IS NO PICKER TO STAGE ANY MORE.** It set `_work_picker_open = WORK_PICKER_PRIORITY` for
one slice, because the ceiling was a MAX and the priority arm was the tallest of three; item 12d's
second pass draws every section unconditionally, so the worst case is a property of the MODEL alone
and the conditional children are the whole of what makes it worse than the board's own rows.

**IT IS MEASURED AGAINST THE DIALOG SINCE `docs/plan_standing_upkeep.md` §4.9 item 12d, and it is the
one state that ever tested this.** It used to add the built strip to the harness offscreen and compare
the reservation against the strip's own column — a claim about a strip no zone hosts any more. The
worst case is MOUNTED into the real `WorkInspectorDialog` now, and five things are asked of it: the
reservation covers what the strip draws, the CARD was fitted to that reservation plus its own chrome,
the card FITS the viewport it is centred in, the note really WRAPS, and the reservation is the
documented ceiling plus whole wrapped lines and nothing else. The middle pair is what turned a 106px
unreserved risk into a rect that either fits or does not.

**⛔ ITS NOTE HAD TO STOP BEING A SHORT SENTENCE, and the fixture's own comment said the words were
irrelevant.** They were, until the card's prose lines started WRAPPING: with
`"Animals are drifting off."` on it the wrap term is zero, and *"reserved >= drawn with a wrapped note
in it"* is the pre-wrap claim wearing a new label — the same vacuous shape that has bitten twice on
this branch. The fixture states the shipped shortfall sentence's shape now, and the assertion reads
`Label.get_line_count()` off the DRAWN label before it measures anything, so a copy edit that shortens
it fails loudly rather than quietly stopping testing the thing.

**AND THE CEILING EQUALITY SPLIT IN TWO** — `is_equal_approx(reserved, WORK_INSPECTOR_CEILING_HEIGHT)`
cannot survive a reservation that legitimately varies with the sentence. The structural question it
was really asking (*has a fifth conditional child appeared without moving the constant?*) is asked as:
the reservation is at or above the ceiling, and the whole excess is an integer number of
`WORK_INSPECTOR_NOTE_WRAP_LINE_HEIGHT`s. A new child of any other height lands off that grid.

**`_assert_note_renders_in_full` is the claim whose absence let the elided sentence ship.**
`_find_aside_label` matches on `Label.text`, and `text` is the WHOLE sentence even when the label draws
an `…` — elision is a render property, not a text one — so every existing claim about that note passed
over the defect. What is asked instead: the overrun behaviour is `OVERRUN_NO_TRIMMING` and autowrap is
on, every line the label has is VISIBLE (`get_visible_line_count() == get_line_count()`, and at least
`WRAPPED_NOTE_MIN_LINES` of them), and the drawn rect is at least the label's own minimum height AND at
least the column `_work_inspector_note_width` measured the wrap against — which is the one number a
label laid out narrower than it was priced would break.

**Falsified, four ways, and every one of them failed loudly.** Baseline before the slice: **761 PASS,
0 FAIL**; after: **789 PASS, 0 FAIL** (exit 0 both times — and the exit STATUS is the verdict, since a
scene that fails to parse exits 0 with no assertions run at all, which is exactly what the first draft
of this chapter did).

| Restored defect | Failures |
|---|---|
| `work_row_priority_prefix` returns `""` (no mark) | **2** — `exactly the HIGH and LOW rows are marked … (got [])`, and `the widest row carries no priority mark, so this measures nothing` |
| the priority picker renders under *any* open picker (both at once) | **7** — including `…and the priority picker is GONE with it, not merely covered` and both strips' `RESERVES what it DRAWS (96 reserved, 141 drawn)` |
| the commit always sends `normal` | **2** — the `high` and `low` clicks by name; the `normal` one correctly still passes |
| the `Priority` link set `MOUSE_FILTER_IGNORE` (a dead control) | **7** — the link claim, the picker's three buttons, the lit claim and the swap-back |

The last of those is the one worth keeping in mind: it is `pressed.emit()`'s blind spot, the shape
that shipped a completely dead drag green in #570, and it is caught only because the link is driven
through `_drive_click` rather than called.


## The `-5` producers — a fresh declaration and a stalled one must read apart (§4.9)

The wire's `-5` (`BUILD_TURNS_NOT_YET_ESTIMATED`) means *declared since the last turn resolved, so no
estimate pass has run for it*, and the defect it fixed was that a fresh declaration and a genuine stall
looked identical. **`_assert_not_yet_estimated_producers` IS PNG-LESS AND DELIBERATELY SO.** Three of
the four swept producers cannot be seen in any one frame — a pace tints a compose face, a stall verdict
marks a map badge — and the stated failure mode in this client is *two producers disagreeing about one
meter*. It asks the producers themselves (`build_turns_remaining` passes `-5` through and specifically not onto
`-1`; `build_pace` answers `BUILD_PACE_UNKNOWN` and neither stops the sheet nor paints it as climbing;
`build_is_stalled` is false for a staffed `-5` **and still true for a `-3` beside it**, so the verdict
has not simply stopped firing; `build_turns_clause` states no clause), so a consumer rendering
correctly off a producer that has quietly stopped passing the sentinel still fails here.

### `UNKNOWN_BUILD_TURNS_SENTINEL` had to be re-aimed, for the THIRD time

`tools/ui_preview/chapters/improvements.gd` keeps a value *one past the last the schema defines* and
asserts that an unrecognised negative renders as the STALLED hazard. It was `-5`. **The day the wire
spelled `-5`, that claim stopped pinning the rule and started pinning the bug** — it was asserting the
`⚠ Stalled` that this fix removes, and it is what failed the `ui_preview` run. It moves to `-6`; the
constant's own doc has said since its second re-aim that this is required maintenance, and this is the
third round. **It is the sharpest of the three**, because `-5` is the one value the client must render
as a NEUTRAL face rather than merely a different one.

### Falsifications

| Restored defect | Failure |
|---|---|
| `build_pace` loses its `-5` arm (a swept consumer left un-swept) | `` `build_pace` answers no verdict for it (got "growing") `` and `neither stops the sheet nor paints it as climbing` |
| `build_turns_remaining` collapses `-5` onto `-1` again (the root) | both producer claims |

## The material half's frames (`docs/plan_standing_upkeep.md` §4.9 item 12)

**MEASURED, BEFORE AND AFTER, ON THIS TREE**: `140 frames / 828 : PASS / 436 assert OK` →
`144 / 853 / 448`, exit 0 both times.

**FIVE FRAMES ADDED, ONE RETIRED.**

- **`band_panel_rung_pen_price` / `band_panel_rung_pen_short`** — the `⌃` track's price asides on the
  corral-ready Aurochs of the DECLARE board, whose next rung is the one rung on the shipped ladder that
  eats a material. **The pair is the claim**: a band whose shelf covers the pile gets the price and the
  hold cost and NO stall warning; a band two hurdles short of six gains the third aside in WARN ink.
  *"The stall warning renders"* passes on a card that renders it always, and *"it does not"* on a card
  that never does.
- **`band_panel_work_material_short`** — the work row's good-shortfall note beside a hands-shortfall
  one, ON ONE BOARD, with the inspector open on the good-short row. The ink is a render-site decision
  and no model claim can see it, so the frame and the drawn `Label`'s `font_color` are both asserted.
- **`band_panel_standing_bill` / `band_panel_standing_bill_expanded`** — the `Upkeep:` row and its open
  per-good popover, plus the PNG-less negative that a band owing no good draws no row AND registers no
  caret.
- **`band_panel_kit_expanded` RETIRED** with the `Gear` row's popover. `_assert_gear_breakdown_states_every_kit`
  survives whole, retargeted at `DisclosureController.kit_breakdown_lines` — every claim it made was
  about the COMPOSITION, and that producer is untouched.

**The faction page gained `_assert_faction_standing_bill` and lost the `Kit` row's presence claim.**
`_faction_roster`'s BOTH bands now owe a good, deliberately: on a roster where one band owes, a sum and
its single term are the same number and a page that had stopped summing would render identically. The
shelves are 6.0 and 0.2, so the faction figure (6.2) is distinguishable from the mean, from the worst
and from either band alone — and only the second band is inside the critical runway, which is exactly
the case the alert clause exists for.

> ### ⛔ AN EXPECTATION ASKED OF THE PRODUCER UNDER TEST PASSED ITS OWN FALSIFICATION
>
> `_assert_material_short_note` composed its expected sentence through `HudWorkVocab.material_short_note`.
> With that arm restored to `return ""` the expectation collapsed to `""`, the row's note was `""`, and
> **the claim passed over the defect it exists to catch** — three of the four sibling claims failed and
> this one did not. It composes from `WORK_ROW_MATERIAL_SHORT_FORMAT` and the fixture's own numbers now.
>
> **The hover claim had the identical shape one layer up** and was found the same way: it built a
> tooltip through `under_kept_tooltip` and then searched it for the string it had just handed in. It
> reads the row model's own `tooltip` now.
>
> This is the second instance of the rule already written down in this file for `_assert_map_path_states_kit`
> — *an expectation re-derived through the code under test asserts nothing* — and both were found by
> the falsification pass rather than by review.

**Falsification counts, each defect restored on its own:**

| Defect restored | Failures |
|---|---|
| `_build_price_asides` / `_hold_price_asides` → `[]` | 4 (the pile aside, the hold aside, the stall aside, its WARN ink) |
| `material_short_note` → `""` | 4 (the sentence, the DANGER severity, the hover, the drawn ink) |
| `band_has_material_upkeep` → `false` | 9 band-page (2 row, 2 caret-never-clicked, 5 popover) |
| `FactionRollup._upkeep_line` → `""` | 5 (the sum, the rate, the drill-down size, the per-band runway, the jump) |

## The HARVEST rename's half here (`docs/plan_standing_upkeep.md` §4.9 item 12c)

**MEASURED, BEFORE AND AFTER, ON THIS TREE**: `855 : PASS / 448 assert OK` → `858 / 448`, exit 0 both
times. **NO FRAME ADDED AND NONE RETIRED**; the board's plant rows and the pen row's shortfall
sentence read differently inside the frames they already had.

The three `: PASS` are the material note's two new claims (it does not say the source EATS the good;
it names the remedy) and `_assert_work_sort_groups_by_kind`.

> ### ⛔ CLAIM 4 OF `_assert_work_sort_stable` LOST ITS FALSIFIER TO THE RENAME
>
> *The DEFAULT sort groups by KIND* was made to bite by a managed plant row reading `Tend (…)`, which
> sorts AFTER every `Hunt …` row while its `kind` is still `forage`. Every plant row reads
> `Harvest (…)` now and **`"Harvest" < "Hunt"`**, so label order and kind order COINCIDE on every
> board the shipped vocabulary can produce and a label-only comparator satisfies the claim.
>
> **Measured rather than reasoned**: dropping the kind term from `_work_name_sorts_before` fails
> **exactly one** assertion, and it is `_assert_work_sort_groups_by_kind` — the new synthetic pair
> whose labels run opposite to their kinds — not the mixed-rung fixture that used to carry it. The
> fixture's labels are composed from `WORK_ROW_PLANT_FORMAT` so it still describes a board the game can
> draw; what it can no longer do is falsify.
>
> **THE SYNTHETIC LABELS ARE MARKED AS SYNTHETIC AND MUST STAY SO.** They are the one place in this
> harness where a work row's label is not a string the client can produce, and the reason is written
> at the constants: no shipped label can express the disagreement any more.

**`_material_short_sentence` DROPPED ITS SOURCE-NOUN ARGUMENT and kept its shape.** The ⛔ at its head
— composed from the FORMAT and the fixture's own numbers, **never** through `material_short_note` —
is unchanged and is why the rename could not launder itself through the expectation.

**A RETIRED THING NEEDS A TEST THAT IT IS GONE**, so the retired tail is spelled as a needle
(`MATERIAL_SHORT_RETIRED_EATS_NEEDLE`, `" eats."`) rather than reached through a const that no longer
exists — and it is asserted as a PAIR with the remedy's presence, or *"the sentence lost a clause"*
passes.

**Falsification counts, each defect restored on its own:**

| Defect restored | Failures |
|---|---|
| the retired `a turn this pen eats` tail | **2** here — the EATS needle and the remedy — naming the played sentence |
| `_work_name_sorts_before` drops its kind term | **1** here — the synthetic pair, `(hunt, forage)` |
| `plant_crew_label` → `""` | **0** here, **30** in `ui_preview` (the five states' four surfaces, their agreement claims, the five liveness claims and both rung-blind claims) |
| the card-side material sentence restored | **0** here, **4** in `ui_preview`'s `improvements` |


## The ring caret and the strip's head line (§4.9 item 12c, Stage B)

The six `: PASS` this stage added are `_assert_ring_caret_rides_the_standing_mark`. No frame was added
and none removed.

> ### ⛔ AND ONE GUARD SILENTLY STOPPED COVERING THE ROW THE NEW CONTROL IS ON
>
> `_assert_rung_labels_are_hoverable` walked `Label`s through `_collect_rung_labels`. The penned
> herd's rung mark is a `Button` now, so it dropped straight out: the line read **`4 rung marks are
> hoverable` → `3`** and nothing failed. Caught by diffing the `assert OK` LINES against the baseline
> rather than by the exit status or the count, which is what that diff is for.
>
> It walks `_collect_meta_controls` on the shared meta now — which is what the meta was always for —
> and it **forks the filter test with the shape**: `PASS` on a read-only mark (the only value that
> both shows a Label's tooltip and lets the row's click through), `STOP` on the ring mark, or the
> press that opens the card would also open the inspector under it. It counts the pressable one
> separately and FAILS when a board carrying a penned herd draws none, so the coverage cannot go
> quiet the same way twice.

**THE A/B IS THE CLAIM, three ways.** A caret on every mark satisfies a bare presence check, so the
PASTORAL row is asserted to carry none; the READY slot is asserted EMPTY on the penned row, which is
the mechanical reason the control is on the mark at all (`RungLadder.has_track` is false at the top of
the animal branch) and would silently stop being true if that slot ever grew an offer; and the same
penned herd with a ring already banked is asserted to lose the caret — **driven, not staged**: the
fixture is mutated and the models re-derived, so no frame's subject moves.

**`herd_graze_pen`'s `herd_pen_extending` is the inverted twin.** It asserted the badge's NUMBER on
both render paths; it asserts the badge and the `Extend pen` button are GONE, against a
**precondition** that the fixture really has a ring 60% banked (or "no badge" is free on a pen with no
ring) and a **liveness** claim that the drawer still draws its action row (`["Assign herders ▸"]`).
Net `-1` on `ui_preview`: four claims became three, then the precondition put it back.

## The inspector DIALOG's matrix, and why it is a matrix (§4.9 item 12d)

`_render_work_inspector_dialog_states`, appended last in the run order. The behaviour it covers is in
`band-city-panel.md` → "THE WORK INSPECTOR IS A DIALOG"; what belongs here is the shape of the walk and
what would have been missed without it.

**IT IS ELEVEN CONFIGURATIONS, not a frame.** The defect it exists to stop lived in the gap between
two frame families this file had built for years — *every picker-open frame was a tall dock and every
wide-dock frame had the expansion closed* — so a twelfth frame in either family would have proved
nothing. LEFT at 1080/900/768/720 and BOTTOM at 1920×1080, 1600×900, 1440×900, 1366×768, 1280×800,
1152×720, 1024×768. (It read *"ELEVEN CONFIGURATIONS × FOUR PICKER STATES … with
`none`/`floor`/`priority`/`kits` walked at each"* until item 12d's second pass retired
`_work_picker_open` outright; the card draws POLICY, PRIORITY and KITS at once, so there is nothing
left to walk and each configuration measures ONE card. `DIALOG_PROBE_PICKERS` is a ⛔ RETIRED comment
in the harness for the same reason.)

**AND THE MATRIX IS NOW THE HOME OF THE BOARD-LAYOUT CLAIM TOO** (`_probe_work_board_layout`), which
is a reuse rather than a new walk: the rows-per-column preference is decided against a dock's
AFFORDANCE, and this is already the one place that enumerates docks and viewports. Per configuration
it walks `BOARD_LAYOUT_PROBE_COUNTS` (3, 4, 6, 8, 12 — the preference exactly, the 5 + 1 board it
exists to rebalance, the two counts either side, and one past every affordable column) at two queue
depths, and asserts the page still SHOWS every source the height-derived layout would have and takes
no more pages to do it. See `band-city-panel.md` → "THE COLUMN BREAKS AT THREE ROWS BY PREFERENCE".
- **BOTH BOARD DEPTHS, because what stands above the board decides how tall a column can be.** At the
  fullest staged depth the preference has nothing to shorten and the probe would have reported "no
  change" everywhere while proving nothing, so the same dock is walked with nothing above the board as
  well. Walking both is what makes it a probe of the RULE rather than of one fixture's depth.
- **It asserts `min(page, count)`, never `page`.** Slots a band cannot fill are not worth protecting,
  and a raw-page assertion would forbid the change's whole purpose — a six-source band would have to
  keep a 10-slot 2 × 5 page with four slots empty and go on drawing 5 + 1. The two readings coincide
  wherever the page binds, which is wherever a source could actually be lost.
- **THE SWEEP DECLARES, so it hands the card back.** `_work_board_capacity` is not a pure reader — it
  calls `set_work_columns`, which resizes the card — so the probe re-renders at the end to restore the
  count the REAL band declares. Without it every configuration after the first is measured against a
  card sized for twelve synthetic sources.
- **Paired with liveness**, the rule this whole section runs on: a board that draws no rows satisfies
  every inequality above, so the live board's own row count is asserted non-zero beside them.

**THE FIXTURE IS THE FULLEST BAND IN THIS FILE, and that is the choice the matrix stands on.** It runs
on `_build_queue_band_fixture(4)` with `_build_queue_patches(4)` — a board of four declared builds
with their crews, the state the four pixels of spare were measured in. On the reference band the same walk reports 400–700px of spare on every row and asserts nothing about the
arithmetic that mattered.

**Three frames, and the third is the one item 12c never got**:
`band_panel_work_inspector_dialog_bottom` (the card centred over a wide dock's map, the board whole
underneath it), `…_left` (the SAME card on a vertical dock — the fork the slice refused to make, so
there is one code path and one frame family), and `…_kits` (the kit pair open on a WIDE dock).
`band_panel_work_inspector_dialog_over_track` is the fourth: the rung track opened from a board row
while the card is up.

**WHAT IT ASSERTS THAT A FRAME CANNOT.** Each of these is paired with a liveness half, because every
one of them is an ABSENCE claim and an absence claim passes on a surface that was never built:

| claim | its liveness half |
|---|---|
| no inspector strip is left anywhere inside the panel | the card is up, and really draws its head, its links row and its close `✕` |
| `_work_board_capacity` answers identically for all four pickers | the rendered row count does not move either, and is non-zero |
| the card's layer holds no dismiss catcher | the card is `MOUSE_FILTER_STOP` over a board that really drew rows |
| the card does not intersect `BandCityPanel.card_rect()` | its centre is the viewport's, and its rect has area |
| tabbing away from Work takes the card down | it was up on the Work tab first |

**The re-select claim captures the NODE, not the state.** *"Still open"* is satisfied by a card that
was freed and rebuilt, so `_assert_dialog_survives_a_reselect` holds the instance across the switch and
requires the SAME card holding a DIFFERENT strip — plus the same key still closing it, which is what
keeps the toggle a toggle.

**The stacking claim is structural and says so.** `_assert_rung_track_opens_over_the_dialog` asserts
the node KINDS — a `PopupPanel` (an embedded subwindow, which Godot composites above every
`CanvasLayer` of the parent viewport) over a `Control` on a `CanvasLayer` — plus the state that makes
the question live, both up at once. No comparison of two rects could establish it on a frame where
they happen not to overlap, and a price card opening behind the surface that spawned it is exactly the
kind of thing that ships.

**Sabotage-verified by restoring the retired terms** (`_work_board_capacity`'s `inspector_h`,
`WORK_ZONE_GAP_COUNT` 3): 2 failures, and the informative one is
`the board's capacity does not move for ANY picker … (rows [2, 1, 1, 1], drawn [1, 1, 1, 1])`. The
matrix's own printout shows the same thing as a measurement — LEFT 1440×720 drops from 3 board rows to
2 the moment a picker opens.

**Every strip-finding assertion in this file moved off `_panel`.** `_work_inspector_root()` is
`HudLayer.work_inspector_host()` — the LAYER rather than the dialog node, because it is never null once
the HUD is up and every recursive finder rooted at it is also used for a NEGATIVE claim, where a `null`
root would crash instead of answering *not there*.

## The three claims this branch had to INVERT rather than delete (§4.9 item 12d, second pass)

An assertion that outlives the property it was written for does not go red — it goes on passing, and
what it now guards is retired behaviour. Three of them on one branch, each inverted with its liveness
half kept:

| the assertion | asserted | asserts now |
|---|---|---|
| `_assert_kits_picker_is_exclusive_and_costs_the_max` → **`_assert_sections_are_drawn_and_cost_the_sum`** | the three pickers are mutually exclusive and the strip reserves the MAX | all three sections draw on ONE render and the card reserves the **SUM**, term for term against the producer, and that sum is more than its tallest term |
| `_assert_kits_picker_draws_both_controls` → **`_assert_kits_section_draws_both_controls`** | *"with the expansion CLOSED the strip draws neither picker"*, driven by writing `_work_picker_open` and pressing a `Kits` link | both pickers, their rosters, `none` on both and the HINT — with **no click at all** |
| the priority frame's swap pair | *"`Change policy` swapped the strip to the FLOOR picker … the priority picker is GONE with it, not merely covered"* | both grids are on the card at once, each under its own header |

Two more went with them: *"the pick CLOSES the picker"* became *"the PRIORITY section is still drawn
after the pick"* — which is what lets the three levels be pressed in sequence the way a player does —
and *"the card is centred in the VIEWPORT"* became *"…in the ROOM the dock leaves"*, which is the one
that caught a 340px card running straight through a bottom dock's panel.

**`_press_work_inspector_link` is deleted.** Its whole job was pressing a link that opened an
expansion, and there is no such link; its last caller went with the swap frame it drove.

**The matrix stopped walking four picker states per configuration**, and that is a stronger reading of
the same property rather than a weaker one: with `_work_picker_open` retired there is nothing a click
could change about the zone, so the zone figure is taken once and the interesting number moved to the
CARD's height against the room it is centred in. `band_panel_work_inspector_dialog_tight` is the frame
for the tightest room any shipped configuration leaves (a 1152×720 bottom dock: 264px against a 340px
card, so the card's own scroll carries the rest).

## The PENDING row's kit pickers — the state a live game found and this file did not cover

**MEASURED, BEFORE AND AFTER, ON THIS TREE**: `154 frames / 983 : PASS / 479 assert OK` →
`156 / 1030 / 479`, exit 0 both times. `ui_preview` is unchanged in every figure (370 frames /
1589 `PASS`, exit 0), which is what says the client half moved no HUD render.

**The `assert OK` tally does not move for a new frame here, and that is not a miscount.** These two
states call `_assert_work_inspector_fits` (which reports through `_assert_band_panel`, i.e. `PASS`)
and neither `_assert_zones_within_bounds` nor `_assert_zone_content_fits` — the card is a
viewport-centred dialog and takes no zone height, so the zone pair those two states would report is
the pair the *previous* state already reported.

**TWO FRAMES, ONE PER WEB** — `band_panel_work_kits_pending_patch` and `_pending_herd` — appended
after `_render_kits_upkeep_gate_states`, each a **BRAND-NEW** assignment on a source the band does not
work (`QUEUE_SECOND_PATCH` carries a patch and no row; `PENDING_KIT_HERD` is the roster's second
deer). That is the harder half of the case on purpose: with no confirmed row behind it the only
honest answer is the kit the command itself carried, so a repair that preserved the SETTLED row's kit
passes on an edit and fails here.

**The kit sent is deliberately the NON-DEFAULT `none`.** A picker falling back to the job default
reads `Harvesting kit` / `Stalking kit` and satisfies every presence and liveness claim, so the face
is asserted by EQUALITY against `No kit`.

**`_assert_crew_edit_keeps_the_kit` is PNG-less and driven**, because the silent re-kit is a value on
a command line and the card it happens under is byte-identical either way. It drives the REAL
`_emit_work_assign` and requires both the kit AND the count to move correctly — the count half is
what catches an emit that returned at its own guard.

> ### ⛔ THE BAND IS STAMPED LOCALLY, NEVER READ OFF `_panel_band`
>
> `Hud._after_pending_change` re-renders the SELECTED unit into the panel, and an earlier state in
> this file leaves an UNSTAMPED `_band_fixture()` in the selection — so `_panel_band` comes back
> carrying `entity` and no `band_id`, and the next `_emit_assign_labor` returns at its own guard.
> **Measured: the first emit landed and the crew edit after it silently did nothing** (`2 → 2`), with
> the kit claim beside it still green because the old record was still there. It is the same trap
> `_assert_crew_edit_keeps_improvement` records; the overlay is keyed by `entity`, which is unchanged,
> so the row the inspector opens on is still the right one.

### `_assert_kit_pickers_state_a_selection` — the claim whose absence let a dead control ship

Every kit assertion in this file asked whether a picker EXISTED and whether it carried entries. None
asked what it was SHOWING, so `select(NO_ENTRY_SELECTED)` over an empty face passed all of them. It
walks `KIT_PICKER_METAS` and makes three claims per drawn picker — a roster, a LIT index, a non-empty
FACE — paired with the liveness half that a TAKE picker was drawn at all (the loop SKIPS a picker
that is not there, the Upkeep one being conditional by design, so without it "no picker is blank" is
satisfied by a card that drew none). **The face and the index are separate claims because
`build_option_picker` takes them as separate arguments** and writes the face AFTER the select.

It is called from all three existing kit guards (`_assert_kits_section_draws_both_controls`,
`_assert_wild_source_offers_no_upkeep`, `_assert_kept_source_offers_upkeep`) and from both pending
states, i.e. every state in this file that draws one.

### Two fixture repairs, both describing a source no server can produce

- **`_band_fixture()`'s forage and hunt rows state a `kit_id`.** `LaborAssignment.kitId` is always a
  real roster id on a source row, so a fixture omitting it left the take picker resolving through its
  FALLBACK on every frame in this file. Each row states its own job's DEFAULT, so **no frame moved**;
  what it buys is that "the picker names the row's kit" is a claim about the row.
- **No fixture states a per-site keeping kit.** The resolver is `resolve_worked_sources`, a
  MEMBERSHIP SET carrying no kit at all — it answers only *does one of the viewer's bands work this
  source*, which gates a build scratch — and a site's keeping tools follow from its own rung and are
  read off the cohort's `pool_toe` (`docs/plan_pool_toe.md` §4). The wire has no
  `upkeep_kit_id` / `upkeep_kit_named`, so a fixture stating them would describe a row no server
  sends.

**`_assert_unstated_upkeep_kit_falls_through` is PNG-LESS and DRIVEN because no fixture here can
render its state.** The wire states `""` only for a source no band works, and every kept source in
this harness is one its band works — so the BUILDER is called directly with the model the wire would
produce. Three claims, the third being what stops the first two passing on a picker that merely
defaults: it is lit, its face names the derivation, and the entry it lit wears the `(default)` mark.

### The three falsifications, each failing a DISJOINT set

| Restored defect | Failures |
|---|---|
| the pending overlay drops `kit_id` | **4** — both webs' *"the take picker names the kit the PLAYER sent"* (`"Harvesting kit"` / `"Stalking kit"` against `"No kit"`) and both crew-edit vacuity guards. **The face and LIT claims stay GREEN**, which is the demonstration that the picker's fallback alone hides the blank face while leaving the silent re-kit live |
| the take picker measures its default against `source.get("default_kit_id")` | **6** — the LIT and FACE claims on `band_panel_work_kits_picker` / `_kept_patch` / `_kept_herd`, each reading `index -1 of 2` and `""`. The two PENDING states stay green (their rows carry a real kit), and so do the two WILD ones (their band is `_band_fixture()`, now stamped) |
| the upkeep picker's fall-through removed | **3** — the driven block alone, naming `index -1 of 2` and `"" want "Tillage kit"` |

## Every `⌃` opens a track with a rung on it — the run-wide claim, and its liveness half

**The defect it pins is a DEAD BUTTON, which no PNG can show.** A completed Field drew its standing
glyph *and* a `⌃` offering to build a Field; the press reached `_open_rung_track`, `has_track`
answered false over a source at the top of its branch, and the handler returned in silence — an
enabled `Button` with `MOUSE_FILTER_STOP`, so the click did not even fall through to the inspector.
The frame is a perfectly ordinary board.

`_guard_frame_rung_offers` runs from `_save`, beside `_guard_frame_herd_fields` and for the same
reason: a slot that would open nothing fails against the state it drew in. For every slot whose
`WORK_ROW_BUILD_KIND_META` is `OFFER` it re-asks `RungLadder.track` + `has_track` **through the
slot's own model** (`WORK_ROW_MODEL_META`, set beside the build-kind meta), which is the same pair of
calls `_open_rung_track` makes on that model. It presses nothing: a press would float a popup into
the next state's frame, and this file's state order is load-bearing.

**THE FIXTURE THAT WOULD HAVE PINNED THE BUG WAS NOT WRITTEN, DELIBERATELY.** The trigger was a
published meter one `f32` ULP short of the published standing, and the sim can no longer emit it — so
a hand-stamped two-marks snapshot would assert an impossible state. The claim is the *relation* the
fix rests on instead, over the fixtures the run already walks.

**The liveness half is part of the claim.** `_assert_rung_offers_open_tracks` FAILS when the count of
tracks actually opened is zero, because "no row ever contradicted it" passes on a board that never
offers anything, and it prints all four counts either way — currently `68 offer slots over 162
frames, 68 tracks opened, 0 empty`.

### The falsification

| Restored defect | Failures |
|---|---|
| `RungGates.rung_has_room` forced to `true`, so the offer test admits a rung the track has banked | **13** — twelve per-frame failures naming the row and the rung it stands on (all `corral`, `animal:pen` being the top of the animal branch, so a corralled herd is where the empty track lives) plus the run-wide verdict, `12 of 80 rendered ⌃ offer(s) open a destination track with no rung on it`. The offer count rising from 68 to 80 is itself the tell |

### RETIRED — the 99% repair's two states

`_render_repair_and_declare_states` opened by staging a patch with `is_cultivated` true beside a 90%
meter and asserting the board offered `cultivate` on it. That snapshot is one no server can send now
(`labor-ui.md` → "THE OFFER TEST AND THE TRACK TEST ASK ONE QUESTION"), so **the eroded state is gone
and the CONTROL survives**: a rung standing at its cost offers nothing, which is the reported
defect's own shape. `_assert_repair_offer(where, want_offer)` lost its flag and is
`_assert_no_repair_offer(where)`.

`_repair_patch_fixture` takes `cultivated` beside `progress` so the standing and the meter travel
together — `RUNG_FX.stamp_patch` derives `current_rung` from the flag, so the two halves of a fixture
cannot be set apart. `_assert_repair_card_states_no_countdown` asks its BUILT row at a full meter and
its unbuilt row on a patch that really is short of the rung; the claim (the fork on `built` happens
before the countdown is read) is unchanged by the correction.

## The scout launch sheet's kit picker (`chapters/band_expedition.gd`, the ranging-kit arc)

Three frames — `expedition_kit_ranging` / `_short` / `_none` — appended LAST in the chapter's frame
walk, plus eleven claims. **The three frames are the three things the gear line has to be able to
say**: fully outfitted, short of outfits, and bare-handed on both webs. One frame would pass on a
builder that printed one clause and never reached the others.

- **The picker's CONTENTS are the claim no frame can carry.** A scouting party is never told a kit
  cannot be used, so the block asserts both `expedition` kits are listed AND that NONE is greyed —
  the per-quarry withholding the hunt sheets do must not fire here, and a greyed row is invisible in a
  thumbnail with the popup shut.
- **The gear line is asserted for BOTH webs on the SAME line.** `hint.contains(GATHER_EQUIPPED)` is
  the load-bearing one: a hunt-only line renders a perfectly ordinary hint.
- **THE SIGHT CLAUSE IS ASSERTED AS A PAIR ACROSS TWO FRAMES, which is the only shape that can catch
  it.** `expedition_kit_ranging` must read the EQUIPPED radius and `expedition_kit_none` the bare one
  — and the second claim carries a NEGATIVE half (`not … contains` the equipped clause), because a
  builder that printed one radius whatever is picked satisfies either half alone. The expectation is
  composed through `_expedition_sight_clause`, off `HudComposeVocab.KIT_EXPEDITION_SIGHT_FORMAT` and
  its decimals, so a copy edit moves the claim rather than breaking it.
- **The command claim is a PAIR and PNG-less** — a tail is not a picture. The null pick must emit
  ` kit none` and the DEFAULT pick must emit no tail at all: a builder that always appended satisfies
  the first alone, one that never did satisfies the second. It is driven through the REAL path (the
  Scout sheet on the band's drawer, its send arming the pick, the click on the tile committing through
  `send_expedition_to`) so it covers the whole carry rather than a hand-built dictionary.

**Three harness traps this block walked into, all previously recorded elsewhere in this file and all
met again here:**

- ⛔ **A LAMBDA CAPTURES A LOCAL BY VALUE.** The command witness assigned to a `String` and reported
  that nothing was ever emitted. It is an `Array` box, the container every chapter witness uses.
- ⛔ **INJECTING THE PANEL DOES NOT RENDER THE SELECTION INTO IT.** The dock came up on whatever
  `panel_band()` the walk had left behind — the reference band with 13 of 16 hands assigned — so the
  party stepper capped at 3 and every claim was about the wrong band while looking ordinary. The block
  calls `render_band` explicitly.
- ⛔ **`shortfall_line` IS SILENT ON A BAND THAT STATES NO GEAR LEDGER**, and silence is also what a
  fully-covered party produces. The bare `BandFx.band_fixture()` states no `kit_item_conditions`, so
  the short state photographed a covered party; the block stages `BandFx.with_equipped_kit`, and the
  band's SCARCEST ranging item against a party of nine is what makes the shortfall sentence reachable.

**THE SHORTFALL ARITHMETIC MOVED WHEN THE KIT GAINED ITS FOURTH ITEM, and it moved for a reason that
is not a bug.** `wayfinding` joined `ranging` once the party's observation radius became kit-aware,
and `shortfall_line` counts COMPLETE OUTFITS — the `min` over the kit's whole item list. The harness
band holds 2 sets of wayfinding gear (`BandFx.KIT_SCOUT_HEADCOUNT`) against 4 baskets, so the
wayfinding gear is now the binding item: the covered party is 2 rather than 4, and the short frame
reads `2 of 9 Ranging kits available` where it read `4 of 9`.

**`BandFx.kit_roster_fixture()` gained the `ranging` entry and `none` gained the `expedition` job.**
The ranging kit lists that job ALONE and appears in no hunt, forage, scout or warrior picker
in either harness. It equips exactly ONE axis to a value the roster does not already carry —
`expedition_sight_range`, which no other entry lifts, and which is the point: a sight clause reading
the same for this kit and for `none` would present the fourth item as decoration. Every other axis it
equips is one the roster already reaches, and no reader outside the `expedition` job's own gear line
consults the sight axis — **no frame rendered before it existed changes.**

**`command_guard` drives `send_expedition` with a NON-DEFAULT kit** (`BandFx.KIT_ID_NONE` against
`KIT_DEFAULT_EXPEDITION`), and the verb joined `KIT_BEARING_KINDS`; `xtask/src/command_guard.rs`'s
`kit_token` gained the `SendExpedition` arm, that verb having left the `NotKitBearing` list. Without
both halves the drive would prove only that a line PARSES.

## The workings roster's rung TRACK (issue #650)

One frame and eight assertions appended to `_assert_the_workings_roster_names_its_workings`, plus a
rung CATALOG the whole state now needs. The behaviour is `extraction-workings.md`'s; what belongs
here is the shape of the drive.

**`band_panel_workings_track` is the deposit ladder's ONE rendered frame, and this is the only
harness that can take it**: the track is opened from the roster ROW's declaring mark, and
`ui_preview` stands up no Band panel. Its four ROW STATES are asserted over the producer in
`ui_preview`'s `workings` chapter — a frame shows one, and the branch has four.

⛔ **THE STATE PUSHES A DEPOSIT RUNG CATALOG, AND WITHOUT ONE IT IS EVIDENCE OF NOTHING.** Every
value cell names its rung out of `SubsistenceSection.ladderRungs`, so a roster with no catalog
behind it draws `forestry:felling` in each one; and the row's mark is built from
`RungLadder.has_track` over its rows, so with no catalog the mark does not draw at all and **every
claim about it passes vacuously**. It is cleared with the deposits in
`_restore_workings_roster_fixture`, a per-world constant being exactly the thing a later state
inherits.

⛔ **THE VALUE-CELL CLAIM IS ASKED WITH THE SAME CATALOG THE ROW WAS BUILT FROM.** `deposit_row_value`
names the rung out of it, so an expectation composed against `[]` compares a display name with a raw
wire key and fails for a reason that has nothing to do with the REUSE being claimed. A second claim
beside it pins the name as the catalog's word and the raw key as its negative.

⛔ **THE CARD IS A `PopupPanel`, i.e. a `Window`, SO `_find_meta_control` WALKS STRAIGHT PAST IT** —
the road ladder's own trap, and it cost a run here (`pressing it opens the SHARED rung track` failed
on a track that was up and correct). The rows are read through the same `_rung_track_states` /
`_rung_track_faces` pair the plant track's own states use, which recurse through the Window because
they walk `get_children()` rather than gating the recursion on `Control`.

**What only this frame can say:** one mark per row with somewhere left to go, each keyed to its own
`(tile, material)`; the branch's own TWO rungs and no more — the two ladders are ONE vector, so a walk
that forgot to filter on `branch` would offer a coppice here; the floor stated as a FACT rather than a
price of zero, asserted BOTH as `STATE_STANDING` and as the rendered face, since a `0 work` price is
the shape the branch's own ⛔ refuses; the quarry LEADING with its pile and its standing bill (a
`begins_with`, because *every ordered rung leads with its price, refused or not* and this walk's
faction may or may not have learned Quarrying by the time it gets here); **no `≈` estimate on it**;
and its material aside through the shared price composer.

**The per-row prohibition is unchanged and still asserted.** The existing scan walks the ROWS for the
stepper's `−`/`+` faces and the road roster's `✕`; the mark is asserted BESIDE it, which is what says
a declaring `⌃` is not one of the three controls roads.md forbids on a row.

## `band_foreign_beside_own` — the two band tiers in ONE frame (`chapters/band_expedition.gd`)

**One frame and seven `PASS`, appended LAST in the chapter's `run()`** — after the dock is released
and the reference band handed back, so no frame before it moves. `EXPECTED_CHECKPOINTS` 115 → **127**,
RE-MEASURED by raising the const to an impossible number and reading `reached 127` back: the declared
115 was already six under the chapter's real count, so a delta applied to it would have set a floor
the chapter could fall through.

**`_foreign_band_fixture` IS THE CHANGE, and the state it used to stage is one no server can send.**
It published a `⛺` and a `Nomadic band` tooltip; a foreign band's row is an allow-list of six fields
now (`band-readouts.md` → "A FOREIGN BAND'S ROW IS SIX FIELDS"), so the stage went, `activity` went,
and `name` / `band_id` came in. `band_foreign` therefore reads differently in the same frame it
always had — the shape of a fixture that stopped describing the wire.

**THE PAIR IS THE CLAIM.** `band_foreign` renders a LONE rival and is green whether the client draws
one honestly or has stopped drawing bands at all; the new state puts an owned band on the same visible
hex, so the roster carries the redacted row's neutral dot, empty mark column and silent activity slot
beside a full one. Seven claims, and three of them exist only to stop the other four passing
vacuously:

| claim | why it is there |
|---|---|
| the roster lists BOTH bands | the precondition — a drawer saying nothing about a rival is also what a roster that dropped the rival produces |
| the drawer states its `Position` | the one row a redacted cohort can honestly fill |
| no `Food` row, no `Morale` row | their producers read fields a rival's row leaves at DEFAULT; a zeroed `Food 0 (∞)` in healthy green is the client claiming a larder it cannot see |
| the allocation host is NOT visible | no band order is reachable on somebody else's people |
| our own band on that hex still reads Food and Morale, and still offers `Move` | the PAIRED POSITIVE — without it every absence above passes on a drawer that has stopped producing rows at all, the one failure that looks exactly like the fix working |

⛔ **THE ORDERS CLAIM ASKS THE HOST'S VISIBILITY, NOT THE BUTTON'S ABSENCE FROM THE TREE.**
`_render_occupant_drawer` HIDES `%AllocationPanel` for a foreign band rather than emptying it, so the
previous state's `Move` button is still parented under it and a `find_button_by_text` walk finds one
on every frame in the chapter — measured, the first cut of this state failed on exactly that.

> ### ⛔ A UNIT HOLDING A TILE THAT HOLDS THAT UNIT IS A CYCLE, and it reads as a state that did not change
>
> `Hud.show_unit_selection` takes a `duplicate(true)` of the cohort, so a fixture whose `tile_info.units`
> contains the selected band itself makes Godot print `Max recursion reached`, ABORT the copy and render
> the PREVIOUS selection — a perfectly ordinary frame, one state stale. It cost a run. The rows in the
> tile's `units` list are stripped of their own `tile_info` (`_roster_row`), which is the shape
> `MapView` already has: `tile_info` is stamped onto the payload handed out, never onto the rows inside
> it.

## The work party's block — one contrast frame, one narrow one (`docs/plan_civilization_steps.md`)

The behaviour is `band-city-panel.md` → "The work row reports its own WORK PARTY"; what belongs here
is the shape of the fixture and what each claim can tell apart. The party is a **caravan** now
(`.claude/rules/core_sim/work-party.md`), and the fixture is staged for it: one row per state the
block can be in.

**IT RUNS LAST, after `_render_empty_work_zone_states`**, so no earlier frame moves — and it hands
the world and the reference band back on the way out.

⛔ **THE LOCAL ROW IS THE FIXTURE'S SUBJECT, not its scenery.** *A row with no party renders exactly
as it did before any of this existed* is a claim about the DIFFERENCE between two rows, so the local
forage row and the three postings are ONE board: an identity asserted on a band with no party
anywhere on it passes on a client that never implemented the block at all. **Its party keys are ABSENT
rather than zeroed** — absent and `party_workers == 0` are one reading, and a fixture spelling the
zeros out would be asserting the decoder's shape rather than the row's.

⛔ **AND `realized_yield` IS THE PARTY'S `net_rate_home` ON EVERY POSTED ROW** — one forecast
publishes both, so a fixture giving them two numbers describes a row no server can send.

| posting | `walkTiles` | `walkOutRemaining` | `huntersOnTheRoad` | `nextLoadHomeIn` | the block |
|---|---|---|---|---|---|
| local forage | — | — | — | — | **no block** |
| walking out (deer) | 6 | 2 | 0 | 0 | crew line, `Walking out — reaches the herd in 2 turns` |
| running (deer) | 8 | 0 | 1 | 3 | crew `· 1 on the road`, `Next load home in 3 turns` |
| running, inedible (wolf) | 5 | 0 | 1 | 1 | crew `· 1 on the road`, `Next load home in 1 turn` |

⛔ **THE WOLF POSTING WAS THE UNSUPPLIED ONE, AND IS RE-STAGED AS AN ORDINARY FAR POSTING.** Under
the retired eat-first rule it carried `party_deficit` 1.2 and drew a DANGER `Needs … from home` line,
and the deer posting a `Party ate 2.00`; both fields are gone from the wire
(`.claude/rules/core_sim/work-party.md` → "RETIRED: an eat-first rule"), and a pelt take walks home
like any other.

**Each of the three live fields drives exactly one line and is set independently of the other two**
(`_work_party_row` takes them as three arguments), so every absence claim below is a claim about one
field rather than a coincidence of the fixture. The postings also differ in every number, so a block
that read the wrong row's party lands on a figure a claim names.

| claim | what only IT can say |
|---|---|
| a LOCAL row grows no block at all | the identity |
| the walking-out posting draws TWO lines | the liveness the identity and the absences below need |
| its crew line, by EQUALITY, with no road clause | the noun is the board's own resolver's, the tile the party's, the walk one-way `-tile walk` |
| its walk-out line names the HERD and the plural | `walkOutRemaining` is what it reads |
| …and it carries NO next-load line | nobody has reached the herd; a `0` drops it |
| the rate line contains `net_rate_home` | a far row states what ARRIVES, never `0.0 · in transit` |
| the running crew line carries `· 1 on the road` | the live road count |
| `Next load home in 3 turns`, and the block is those TWO lines | `nextLoadHomeIn` is what it reads, and nothing follows it |
| the running posting has NO walk-out line | the claim that keeps an arrived posting from re-promising its arrival |
| `in 1 turn`, not `in 1 turns` | every pack passes through one on its way home |
| the INEDIBLE posting draws the same two lines a running posting does | a pelt take is an ordinary posting |
| NO row's drawn block carries `Party ate` or `food a turn from home` (`RETIRED_PARTY_ROW_NEEDLES`) | the eat-first rule's lines did not survive it; the size claims above are its liveness |
| every party line on the three postings is `INK_DIM` | no line on the block is a warning any more — a render-site decision no model claim can see |
| (PNG-less) a forage party walking out names the PATCH, singular | the plant web's noun and the walk-out singular, through the one producer |

**`band_panel_work_party_narrow` is not a second picture of the same thing.** The work zone
`clip_contents` and the board is paged in uniform rows, so a block drawing taller than the capacity
arithmetic reserved is sliced off the bottom; `_assert_work_party_rows_fit_the_zone` measures the
DRAWN rows against the zone with a liveness guard ahead of it. Measured: **4 rows, 304 of 759 px**.

**`band_panel_work_party_spoils` / `band_panel_homeward` (#706)** run after the crew-split states, on
`_work_party_spoil_band_fixture`: the party band with its running posting's walk rotting 0.35 a turn
and its pelt posting stood down — those three hands walk home with 2.40 food, 0.80 of
it rotting, first load in 1, all home in 3. Claims: the far row's third party line equals
`WORK_ROW_PARTY_SPOILS_LEAD + ROT_RATE_FORMAT` and is drawn in `WARN`; the walking-out posting grows no spoil line;
on the band tab the partition guard holds with a `Walking home` segment of 3, `effective_idle` is 0,
and the WORKFORCE zone's `HOMEWARD_LINE_META` lines equal the three expected strings. They hand the
reference band back, and no earlier frame moves.

**`band_panel_work_row_homeward`** runs inside the same block, after `band_panel_homeward` and before
the hand-back, on `_row_homeward_band_fixture`: the spoil fixture plus the pelt row KEPT at crew 0
with 3 walking home, 2.40 food, free next turn, and the near posting carrying 2 more walking home,
no food, free in 3. Claims: the pelt row is on the board and its only party-meta line is the walker
line, in `INK_DIM`; the near row's block ends on `2 walking home — free in 3 turns`; the far row
draws no walker line.

> ⛔ **THE PIPELINE MODEL'S FIXTURE IS RETIRED WITH ITS FIELDS.** It staged `porters`,
> `travelTiles` / `transitTurns` and `partyTransitRemaining` — a near posting part-way through a walk
> the sim counted down, a settled one whose line was open, and a `First load arrives next turn`
> singular — and every one of those fields is gone from the wire. Its sabotage history (a client
> reading `transitTurns` in place of the countdown) has no field left to reach for.

**Sabotage-verified** by drawing the next-load line on `nextLoadHomeIn == 0` (`elif > 0` → `else`):
**EXIT=1, exactly 3 of this block's claims fail** — the walking-out posting's two-line count
(`Next load home in 0 turns` appears), its no-next-load absence, and the PNG-less forage walk
out — while every presence claim stays green. Restored: green.

**And by re-adding a `Needs 1.20 food a turn from home` line to every posted row's block: EXIT=1,
exactly 5 failures** — the no-ate/deficit claim (it names the line on all three postings), the three
postings' line counts, and the PNG-less forage walk's count; the local row's identity stays green, and
nothing else in the run moves. Restored: green.

## A far WORKING's party on the roster hover — PNG-less, six claims

`_assert_a_far_working_states_its_party`, called after `_assert_a_working_flags_the_crew_that_outgrew_it`,
stages the roster fixture's FAR stone working as a posted party (`party_workers` 3, a 5-tile walk, one
on the road, the next load in 2, `net_rate_home` 0.9) and reads the drawn row's hover: the crew line,
the next-load line and `Arriving home: +0.90 stone a turn` by equality through `HudWorkVocab`'s own
formats, no `food` word anywhere, and — the paired negative — a near working with no party carrying no
block. It puts the roster fixture back with `_put_the_workings_roster_back`, the put-down state's own
restore, so the CASE 2 / CASE 3 states after it run against the board they were written for.

## One pick armed at a time, and a hidden sheet does not stay live

Three PNG-less guards run straight after `_assert_denial_click_commits`, each judged on what ONE
targeting click emits and what the banner says:

- **`_assert_verb_pick_replaces_move`** — Move armed through `TargetingController.begin_move_band`
  under an open Deny sheet (the action bar's Move closes the sheet first, so only the entry point can
  put the two side by side), then the sheet's real Send: the banner reads DENY, and one click on the
  herd emits one `send_denial_raid` and no `move_band`.
- **`_assert_move_replaces_verb_pick`** — the Deny pick armed, then `begin_move_band`: the banner reads
  MOVE, the sheet's send is drawn un-armed, and one click emits one `move_band` and no raid. The
  optimistic move it records is dropped again (`drop_pending_move`).
- **`_assert_occupant_change_closes_the_verb`** — the Deny sheet open, the selection cycled to the
  band's own hex's LAND, then to a HERD on it: each time the verb is cleared, the herd highlight is
  gone, and `Main.escape_claimant` no longer answers `ESC_VERB_FORM`.

**Sabotage-verified** by making `_disarm_other_picks` and `note_selection_occupant` no-ops: EXIT=1,
exactly 11 failures, all of them these claims (both clicks emit two orders), and nothing else moves.

## The quarry-eligibility guards (the Hunt verb is retired)

There is no hunt verb (`band-city-panel.md` → "THERE IS NO HUNT VERB"), and with it went
`band_panel_compose_hunt` / `_short` / `_eradicate` / `_no_prey` / `_empty` and the hunt-sheet-only
guards (`_assert_hunt_sheet_chart`, `_assert_chart_reads_the_settled_party`, the dock's trip-bound
clause). `SEND_HUNT_CONFIRM_META` is deleted.

**The quarry-eligibility guards inverted with the rule.** `_assert_quarry_eligibility` used to require
the near herd REFUSED (inside the hunting party's reach); every herd at a known distance is a quarry now, so it
requires the near herd COMMITTED (handed to the pick's commit) and a herd at an UNKNOWN distance
refused with targeting still armed. `_assert_denial_quarry_eligibility` lost its hunt half and asserts the glow at `QUARRY_NO_REACH_BOUND`.

`_assert_dock_chart_carries_the_kit` survives PNG-less — it is arithmetic on `floor_chart_model`,
which the herd sheet still draws — and is called from the same block.

## The Trade tab's states live in `tools/band_panel_trade_tab.gd` (issue #731)

**A file of its own, run LAST** (`await TRADE_TAB_STATES.new().run(self)` just before
`_assert_pending_assign_rollback`), so the tab's fixtures do not grow this harness and no earlier
frame moves. It is a `RefCounted` handed the harness node (`h`) — the `ui_preview` chapter shape —
and it hands the panel back at the preview canvas, left-docked, on the Work tab, with the reference
band.

Its fixture is a five-camp network (one relay, one open-ground link) built off `h._band_fixture()`,
its crossings off `BandFx.transfer_crossing` — the shared decoder-shaped row both harnesses stage.
Three turns: the **busy** one (four goods, one foreign import, one export, plus a `party_home` and a
`dowry_out` the tab must not show), the **fold** one (six imports) and the **busiest** one (eight
goods, fifteen shipments), which is the longest the tab gets. The tab is ONE sanctioned scroll of three
sections now (waiting on an answer, this turn, history) and has no SHORT tier, so a row a list opens
from is scrolled into view first (`_reveal`) and the strip-height body (`trade_tab_short`) asserts the
full tier inside a scroll that overflows. The claims a frame cannot make: which causes reached the tab,
the badge rule (the dealings badge, so none with nothing waiting), the three sections in order, the
relay's first hop, the friction row, the 1190 threshold with no Trade flank, the wide section carrying
the merged sections with the treaty proposal in the Peoples section and no Dealings section under
Parties, and `_assert_scroll_only_where_sanctioned` across both shells (`SANCTIONED_SCROLLS` pairs
`TradeZoneController.TAB_SCROLL_NAME` with `ZONE_TRADE`, claimed IFF the tab is mounted). The frames are listed in
`band-city-panel.md` → "The Trade tab".

## `band_panel_workings_overstaffed` measures the GEARED cap (issue #663)

`_assert_a_working_flags_the_crew_that_outgrew_it` stages its cut-back pair with a room of `11` over
a bare `2.2` per cutter, so the bare quotient is five, and publishes the sim's geared
`useful_cutters` on the band's two `extract` rows below it (`WORKINGS_WORN_USEFUL`: wood 3, stone 2).
Five cutters on the wood sit BETWEEN the two ceilings, so a roster row still dividing the room by the
bare rate says nothing where the published cap says `overstaffed`. **That premise is asserted, not
trusted**, against a chapter-local bare quotient (`_worn_workings_bare_cap`) — a fixture where the two
ceilings agreed could not say which one the row read. The stone crew sits exactly on its own published
cap and must carry nothing, the pair's negative half.

## The Work tab's SECTIONS and its SITE CREWS (`docs/plan_site_crews.md`)

`_render_work_sections_states`, run straight after `_render_empty_work_zone_states`. One board carries
every row shape the sections introduce: a WILD patch (owes nothing), a Tended Patch its crew
keeps in full, a Tended Patch its crew keeps SHORT, and the reference hunt row. Frames: `band_panel_work_sections`, `band_panel_work_sections_tools_short`,
`band_panel_work_sections_collapsed`.

- **Every expected string is composed from the fixture's own numbers**, never through
  `site_crew_line` / `site_keeping_hint`: line two is the rung ALONE (`Tended Patch`, `Wild`), the
  short hover is `Short: tending needs 2 workers, this crew can't cover it.` over `Tending: 2 workers
  now · another needed around turn 58`, and the tools `ⓘ` hover is that tending line plus the tools
  sentence.
- **The pills sit on the row's own pill line** (`WORK_ROW_PILL_LINE_META`), never beside the rung line.
- **`_assert_site_crew_is_whole_workers` is the keeping-cap claim**: a forecast whose take is useful up
  to 2 hands, carrying the sim's site crew of 3 (`with_published_site_crew`), caps at 3 exactly — no
  keeping count added — a crew of 2 can still add a hand, and 3 cannot. The herder-floor frame's twin
  claim asserts both cap twins gate at the forecast's own ceiling, adding nothing.
- **The tools-short `ⓘ` hover carries all three tending clauses**: the kept patch is being raised to a
  Field (`improvement: sow` on its row), so it reads `Tending: 2 workers now · another needed around
  turn 58 · 3 workers once it's a Field`, and `TENDING_RUNG_PHRASES` pins the article per rung name.
- **The pill is claimed by a REAL click**: a `Priority` pill on the kept row is pressed with
  `_drive_click` and the emitted LINE is read back through `Main`'s own formatter —
  `work_priority … high`. There is no `Build` pill.
- **The collapse is a pair**: the AGRICULTURE chevron folds its section (head kept, no harvest row)
  while the HUSBANDRY row stays, and the same chevron unfolds it back to the full row count.
- **The inspector on the kept patch** re-runs `_assert_kits_section_draws_its_control` and
  `_assert_work_inspector_worst_case_fits`, both carried over from the retired POOLS worst-case state.
- `_assert_scroll_only_where_sanctioned` gained the sections list: it exists exactly when the work
  zone is mounted on a band page and the road roster's expansion does not hold the zone.

**What retired with the pools, as whole families**: the POOLS block states (`band_panel_pools_*`, the
fund-mode worst case), the pool-card mark / TOE / idle / pending sets and their fixtures, the
declare-time keeping claims, the keeper-tools remedy fork, the workings-roster head and its idle and
unseen cases, the Gathering filter state (`band_panel_rung_ready_filter`), the chips claims on the
empty work zone, and the board-capacity probes (`_probe_work_board_layout`). The workings roster claims
were re-aimed at the GROUNDWORK section: every row now carries its own crew stepper, and
`_assert_groundwork_site_crew_lines` reads each row's rung line (no head count) and its pill line. A
block's HEAD is found through `_block_head`: the Work tab's section head on the collapsed path, the
block's own first child when expanded.

**A 1920×1080 display cannot run the whole walk**: macOS leaves a 928px-tall window there, so every
state that pins a 1080-high canvas fails its pin and the dock-row walk stalls into the watchdog. The
site-crews arc was measured on a temporary copy with those pins at 900 and the dock-row and
interface-scale walks skipped — 188 frames / 1326 `: PASS` / 401 `assert OK`, exit 0 — which is not
comparable to the full-walk tally above.

## The Work tab's four sections in ONE frame, and the pool row as a site row

`band_panel_work_sections_all_five` (`_render_work_sections_all_five`) stands the sections band up
with roads it keeps and two workings, so AGRICULTURE, HUSBANDRY, ROADWORK and GROUNDWORK all draw at
once. `_assert_pool_row_is_a_site_row` measures each pool row against a DRAWN harvest row
— the `+`'s right edge and the icon's x equal to theirs, the title at `WORK_ROW_FONT_SIZE` in its
state's ink (WARN on the short road crew), and the muted second line — so the claim is that the
columns line up, not that the code passed the same constant. `band_panel_work_sections` is the
paired negative: the same band holding no road draws no ROADWORK section and no `Road crew` row.

`band_panel_workings_inspector` is the GROUNDWORK row's click: the inspector opens, its put-down is
keyed to the row's `(tile, material)`, and the press sends `abandon_working` and closes the card. The
frame waits a second `_settle` because `WorkInspectorDialog.refit` fits over two frames.

`band_panel_roadwork_inspector` is the road row's click, the same shape: no road row carries the
put-down, the click opens the inspector, its `Stop keeping this road` is keyed to the row's tile, a
real viewport click on it sends `abandon 0 road <x> <y>`, and the card closes. `command_guard`'s
`_drive_road_abandon` reaches the verb the same way — a click on the row, then the card's put-down.

## The two-slot claim moved to the BASKET row (one overstaff number per web)

`_assert_kit_short_notes`' *"…beside the STAFFING note"* claim rode the short HUNT row on its
`workers_needed`. A hunt row's overstaff note now quotes the `+`'s own ceiling (`hunt_useful_workers`,
`labor-ui.md` → "EVERY SURFACE ASKS ONE PREDICATE ON ONE NUMBER PER WEB"), which that herd's row does
not price, so the hunt row honestly carries no staffing note. The claim now rides the basket-short
FORAGE row, whose site crew (`KIT_BASKET_SHORT_NEEDED` = 2) is one short of its crew: on forage
`workers_needed` IS the ceiling, so the row is overstaffed and short of baskets in one frame. It also
asserts the kit note is non-empty, or the claim would pass on a row that lost it. Measured: exit 0,
1500 `PASS`.

## The crew bar on the Work-tab rows (`_render_work_crew_split_states`)

Three frames, run after the work-party states: `band_panel_work_crew_split`,
`band_panel_work_max_chip` and `band_panel_work_crew_split_big`. The behaviour is
`band-city-panel.md` → "STAFFED WORK ROWS".

- **One board holds every case**: a mixed crew (3 keeping it up, 1 with the kit, 1 bare, one hand holding
  the floor: `3 keep it up · 1 with basket · 1 bare-handed — 1 hold it at the floor, 1 more draw it down to
  it`), a crew all keeping (`3 keep it up`), a wild patch whose bar has no keepers, and a wood working
  (`1 keep it up · 1 with sled`). The producer's words are asked for a hunt crew (`4 with spears — 2 hold it
  at the floor, 2 more draw it down to it`) and an empty one (`Nobody here this turn`).
- **The rows carry the wire's staffed fields** (`_crew_split_row_fields`: `keepers_staffed` /
  `kitted_staffed` / `bare_staffed` summing to `workers`, `hold_crew`, a `kit_toe` line), and the item
  names come from `set_item_display_names`. A second band
  (`CREW_SPLIT_OTHER_BAND_ENTITY`) works the mid patch with its own split.
- The claims:
  - the bar counts the row's crew and its three segments are the ROW's own counts;
  - the hover, by equality, with the kit's on-screen name and the hold clause;
  - the bar takes keyboard focus and a focus floats the same sentence, and sits inside its row;
  - the row's crew is a read-only number and the board draws no `+` on a staffed row;
  - the second band, jumped to on the same patch, draws its own crew and split.
- **The Max chip** (`band_panel_work_max_chip`): the mid row capped at `CREW_SPLIT_MAX_CHIP_VALUE` states
  `Max 3` as a label on the pill line; the uncapped row beside it draws none.
- **The big crew** (48 hands, 12 keeping, 20 kitted) shrinks the squares to their floor and counts the rest,
  and the bar stays inside its row.
- **The inspector** (`_assert_work_inspector_max_and_stop`, run with `band_panel_inspector`): the card
  draws the Max stepper reading `No max`, and `Unassign` emits one stop request that
  `Main.format_stop_labor` renders as `stop_labor <faction> <band> …`, never `assign_labor`. The reservation
  term is `WORK_INSPECTOR_MAX_SECTION_HEIGHT`, summed in `_assert_sections_are_drawn_and_cost_the_sum`.
- **The rest of the board follows**: a staffed row at crew zero stays on the board,
  the pool rows' crew control (a `+`, or the Roadwork
  pool's number) sits in the site rows' column, no Groundwork row carries a stepper, and a pending
  edit's Max restates through `_emit_work_assign` (`RESTATE_STANDING_COUNT`).
- **`command_guard`** drives the six `stop_labor` forms (the band improvement's `branch=<id>` is the sixth) through the real server parser
  (`_drive_stop_labor`).

**A clean run is exit 0, "run complete — no failures".**

## The migration-mode rows on the Work tab (`_follow_row_states`, `docs/plan_roaming_bands.md`, #797)

Four frames — `band_panel_follow_moving` / `_camped` / `_behind` / `_off` — on ONE band with three hunt
rows: a migratory herd it follows (the row under test), a migratory herd it does not (toggle offered,
unpressed) and a resident herd (no toggle). Each case moves the followed herd against the band's tile.
Per frame: the toggle exists on exactly the two migratory rows, exactly one is pressed when the flag is
on, and the party-block lines (read off `WORK_ROW_PARTY_META`) contain the status line by EQUALITY
(`Moving with it · next (72, 18)`, `Camped in the herd. Kills land in camp.`, `Catching up · 5 hexes
behind`) — and none of the three on the flag-off row. A PNG-less claim presses the unflagged row's toggle
through its real `pressed` signal and requires ONE hunt line ending ` follow`. `command_guard` drives the
same line through the real parser (`_drive_assign_labor_kits`) and asserts the token is LAST, after
`kit <id>`, and absent when unticked.

## `band_panel_family_limit.gd` - the breeding ceiling's frames (issue #691)

A satellite of `band_panel_preview` in its own file (the `band_panel_trade_tab.gd` shape), appended
after the Trade tab so no earlier frame moves. It stages the world constants (`LineageWorld`) and the
faction names, then renders the FACTION page's Family limit row in each reading (`family_limit_room`,
`_near` with a fading member and the popover open, `_at` kin, `_lost_touch` with a band in its own
group, `_peoples` with another faction fading, `_lifted` row absent), one Band-tab frame
(`family_limit_band_tab`: no row, `Births stopped`, the breakdown's fourth row) and the split sheet
twice (`family_limit_split`, `family_limit_split_free`). PNG-less claims pin the split arithmetic.
**The faction page is entered once through the real cycler and re-entered only when off it** (a second
`CYCLE_PREV` leaves for the last band), and each Band-tab state selects its band on its own hex first:
a popover click re-renders the hosts off the selection.


## The need rows' frames (`_need_row_states`, issue #798)

Four frames (`band_panel_need_out`, `_waiting`, `_steady`, `_behind`) on one band whose followed
mammoth row is the only need row (a followed row with workers 0), beside a forage row and a second hunt
row that act as donors. `_need_case` asserts per frame: exactly one need-row count slot (`N out` or
`when needed`); the primary line by equality; the stripe ink; and, for the crew-out frame, the donor
counts `4 -> 3` and `3 -> 2`, the WARN line, and the Workforce `out hunting` text. The base follow
fixture's followed row used to carry `workers: 2`, a state the sim cannot send, so it is now a need row.
`_assert_follow_toggle_resends` requires the line to end ` 0 follow`; `_assert_need_toggle_off_resends`
asserts OFF sends an ordinary hunt at `min(hunt_useful_workers, idle)`. The pending overlay is cleared
before the frames so an earlier toggle press does not mark a second herd's row. `command_guard` also
round-trips the zero-count line (`ASSIGN_LABOR_GRAMMAR_DRIVES` 8, `ASSIGN_LABOR_EXPECTED` 14).

## `command_guard`'s DIPLOMACY drives (issue #233)

Seventeen commands in one `_drive_diplomacy` (run last; `EXPECTED_KINDS`: `answer_dealing` 7,
`set_accept_own_gifts` 2, `propose_trade`, `propose_treaty`, `send_envoy`, `end_treaty` 1 each,
`set_open_borders` 2, and a second `send_expedition` and `send_trade_expedition`). They replaced a throwaway
test that fed the same lines to `parse_command_line` once. Every drive presses the control a player presses
(a dealing's own `Accept` / `Refuse` / `Counter`, the sheets' commit buttons, the Peoples strip's links, the
gift checkbox, the confirm dialogs) and **states what the line must say just before it presses**
(`_expect`, merged into the emitted entry), because a line that merely parses proves nothing here.

- **Seven answers**: a refusal, a bare accept, an offer's accept with a return drawn from the larder, a
  trade counter (both legs), a treaty counter that keeps the proposal's terms and adds a bundle, and a
  REPLY taken (`Take it`, a bare `accept`) and walked home from (`Reject it`, a bare `refuse`). The
  proposals and replies are pressed on the Trade tab, the treaty proposal on the Peoples tab, and the gift
  setting on the Trade tab.
- **The Rust half checks what the parse recovered** (`faction_addressed_failures`,
  `diplomacy_field_failures`): the dealing id and the kind of answer (`accept` / `refuse` / `counter` /
  `counter_terms`), the people an end or a border names, the border's state, the gift setting, a treaty's
  road rights, open borders and bundle COUNT, an offer's flag, a scout's strangers order and pack, and the
  destination of a proposal, treaty proposal or envoy. **`BandHandle::FactionAddressed`** is keyed on the
  parsed variant (like `PlaceAddressed`), so a band-addressed command cannot be opted out by being
  relabelled, and **a drive that stated no expectation is itself a failure**: a comparison against nothing
  passes everything.
- **The destination is the exact substitution this gate exists for**: the fixture's destination band has an
  entity (`DESTINATION_ENTITY`) that differs from its `BandId` (`DESTINATION_ID`) on purpose, and a line
  carrying the entity parses perfectly and addresses a band that does not exist.
- **The border drive covers both directions and the breach**: toward a people the player had closed to (one
  press, no question) and against the treaty that holds them open (a confirm, then the line).
- **Sabotage-verified**: ending the treaty with `other + 1` and proposing to the sending band instead of the
  destination fail exactly those two lines, each naming what was sent against what was composed; unit tests
  (`a_destination_entity_in_place_of_its_band_id_is_a_failure`,
  `a_faction_addressed_line_is_compared_with_the_composed_answer`) pin both comparisons without launching
  Godot. The gate reads **70 commands** on this tree.

## The one People bar's frames (issue #878)

`band_panel_people_idle` (the reference band, 3 idle: the amber card under the People block) and
`band_panel_people_no_idle` (every hand spent: no card, no Idle chip) are a PAIR, and
`_assert_people_bar_is_one_bar` makes the shared claims on both and on the need/homeward states:
the four chips sum to `size`, the Idle chip is `HudBandLaborState.people_idle`, working is
`working_age − idle`, the card exists exactly when idle > 0, and the working chip's tooltip names every
non-zero part of `working_breakdown`. `_assert_faction_idle_card` (on `band_panel_faction`, two bands both
idle) and `_assert_faction_work_idle_labels` (on `band_panel_faction_work`: an idle label per band row,
`N working` in each summary, no bar) cover the faction page; `_assert_idle_card_reason_list` drives the
most-idle-first, capped-at-three `+N more` list over constructed bands. The orb's idle alert is asserted
equal to `people_idle` in `_assert_idle_attention_nets_the_muster`.
