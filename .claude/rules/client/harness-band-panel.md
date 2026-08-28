---
paths:
  - "clients/godot_thin_client/tools/band_panel_preview.gd"
  - "clients/godot_thin_client/tools/band_panel_preview.tscn"
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

**One more frame and thirteen `: PASS`** — measured `139 / 815` → `140 / 828`, `assert OK` still 436.
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
- **`_assert_faction_knowledge_tier` → `_assert_faction_band_zone_tier`**, the same pair on the same
  reasoning: the wide dock must DROP Discoveries and KEEP Settling, and the second half is what stops
  it passing on a zone that rendered nothing.
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

**`FACTION_BAND_FULL_MIN_HEIGHT` was set from `_report_zone_content_extent`, not guessed** — see
`knowledge-panel.md` for the numbers and for what the guess got wrong in both directions. That printed
extent is what a re-measure reads; this page has now been at the edge of its box three times.

**A clean run is 134 frames / 421 `assert OK` / 759 `: PASS`, exit 0 — RE-MEASURED ON THE MERGED
TREE.** This arc removed one frame (`band_panel_faction_knowledge`) and added none, and measured
126 / 404 / 685 on its own; the build-queue arc (#576) landed in `main` in between and the two sets
of numbers are neither branch's. **That is the "RE-MEASURED, never summed" rule arriving through a
MERGE rather than through a commit** — two correct tallies, both stale the moment the branches met,
and adding them gets a third wrong answer. Re-run after any merge that touches this harness.


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

**A clean run exits 0 and prints 382 `assert OK` lines, 589 `: PASS` ones and ZERO `FAIL` ones, over
116 frames.** (It was 379 / 570 / 115 before the WORK TAB READ THE LEG IN FLIGHT — `band-city-panel.md`
→ "THE PERCENTAGE IS THE LEG IN FLIGHT'S". That step is **+1 frame and +17 `: PASS`**:
`band_panel_queue_leg_animal` and the four-state block below, whose remaining two `assert OK`s are
that frame's own bounds/content-fits pair. The `assert OK` delta reads +3 rather than +2 and the
`PASS` one +19 rather than +17, which is the recorded figure's own drift rather than two lost claims —
the paragraph below already records this tally as having been behind the harness twice.) (The figures
before it were RE-MEASURED into an emptied `ui_preview_out/`.) The figure recorded here before the
MATERIALS-ONLY WORK ROW read 260 / 420 / 104 and was already 119 / 150 / 11 behind the harness, so
that change's delta is not recoverable by subtracting them: it is **+2 frames, +8 `assert OK` and +6
`: PASS`** — `band_panel_work_material_forage` / `_crops`, each contributing its own shell / bounds /
content-fits / width-fits quartet and `_assert_work_row_states_its_materials`' three. The figure
recorded before the Builders card lost its kit picker read 254 / 377 / 101 and was already three
frames and forty-three `PASS` behind the harness, so that delta is not recoverable either: it is
**+2 `: PASS`, no `assert OK` and no frame** — eight of `_assert_builders_kit_picker`'s claims retired
with the control, ten gear-line/absence claims and three stepper claims in their place.

(It was 246 / 356 / 97 before the BUILD QUEUE BLOCK — `docs/plan_standing_upkeep.md` §4.6b, specified
in `band-city-panel.md` → "THE BUILD QUEUE BLOCK". Its four states —
`band_panel_build_queue` / `_blocked` / `_none` / `_wide` — account for the eight `assert OK`s (each
state's own bounds/content-fits pair) and eighteen of the twenty-one `PASS`es; the other three are
`_assert_builders_card_kit_faces`, PNG-less, which fixed the Builders card's roster-order
fall-through. (It has since grown to six, that helper's picker-face claim having become a gear-line +
picker-absence pair per state — see the BUILDERS KIT PAIR paragraph below.)
**`band_panel_build_queue_none` is the PAIRED NEGATIVE and is what makes the other three worth
anything** — a block drawn unconditionally passes every positive claim above it and fails only there,
which is exactly what sabotage produced (28 failures: that one claim plus 27 zones the block's own
20px head then overran). The wide state REPORTS its extent rather than only asserting the fit: the
work zone comes out **300px of a 300px box, 0 spare**, with the board still paging two rows.)

(It was 242 / 348 / 95 before the BUILDERS KIT PAIR — `band_panel_builders_kit_plant` /
`band_panel_builders_kit_animal`, the frames on which the Builders card's gear line is judged. **The
PAIR is the claim, and the fixture is what makes it one**: ONE band, whose forage row and hunt row
already name the two sources, with the head moving by re-ordering **the band's own `build_queue`** —
so the two frames differ in the queue head's WEB and in nothing else.

> **It used to move by dialling those SOURCES' own `buildQueuePosition`, and that stopped being the
> head** (`docs/plan_standing_upkeep.md` §4.9 item 9a). Membership and order are the band's own
> `buildQueue` now, so both frames change the BAND fixture rather than the sources — which also means
> the pair is no longer byte-identical outside the queue, and the third case added with the reorder
> arrows (a plant head under another band's animal head) is what pins that the derivation reads the
> acting band.

**They asserted a PICKER's face, its greyed entry and the reason on it until slice 6b retired that
control** (`band-city-panel.md` → "THE BUILDERS CARD MOUNTS NO PICKER EITHER"). Re-aimed rather than
deleted, since the states themselves still matter: each now asserts the read-only gear line's WHOLE
TEXT by equality — the kit's name, what its tool takes off a build, and the condition of the item
behind it, composed from the vocabulary and the fixture's own numbers rather than through
`KitRoster.role_gear_line` — beside the ABSENCE of a picker on that card, which is asked of the
CONTROL (`KitRoster.KIT_PICKER_META`) and never of the label. `_assert_builders_card_kit_faces` runs
the same helper over three states PNG-LESS, the EMPTY queue included, a resolver stuck on one web
satisfying any one of them alone.

**`_assert_builders_stepper_sends_no_kit` is the claim that made the retirement worth making**, and
it is a PAIR: driving the Builders `+` must emit a line carrying no `kit` token at all — read off
`Main.format_assign_labor`, so the sim keeps deriving per entry — while a Scout stepper driven onto
its NON-default kit first still carries `kit none`. Without the Scout half the claim passes on a
client that dropped the tail everywhere, `Main._kit_token` omitting a selection equal to the job
default. Sabotage-verified on two DISJOINT mutations: collapsing `_commanded_role_kit_id`'s builders
fork fails **exactly one**, naming `assign_labor 0 4904 builders 1 kit none`; restoring `builders` to
`KIT_PICKER_ROLES` fails **ten** — the five picker-absence claims and the five gear lines that then
have no label to read — and leaves the stepper pair green.)

(It was 240 / 341 / 94 before **`band_panel_builders_segment`** — the BUILDERS segment
state, whose two `assert OK`s are its own bounds/content-fits pair and whose seven `PASS`es are
`_assert_people_matches_workforce`'s four plus three of its own. **Its fixture IS the claim**: the
reference band's four assignments spend 13 of 16 workers, so putting THREE builders on its forage row
takes it to exactly `working_age` — before the fix the zone read `3 idle of 16` over segments summing
to 13, and after it `0 idle of 16` over segments summing to 16. **A band with slack could not say
this**: the idle count would merely be wrong by three rather than wrong about whether the band has any
hands at all. The partition guard is what does the work — it sums the RENDERED chips against
`working_age`, so a bar missing the build segment fails by exactly its count — with the segment's own
count and the NEGATIVE on the reference band (a band with no build in flight grows no such segment,
the bar's render-only-when-non-zero rule) beside it. The rationale is `labor-ui.md` →
"`effective_idle` SUMS `staffed_total`". The paragraph below records the figures as they stood before
it.)

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
more is `_assert_denial_pelt_take`; **four more are the KIT REPRICING's material arm**, inside
`_assert_kit_reprices_the_source` — the per-material rate by RATIO, its no-op twin at the reference
tier, the take through `expected_materials` at a crew below the saturating one, and the plant web's
own row on the no-retreat patch beside it. **They are named rather than numbered from an offset** —
this tally moved twice in one merge, once for arc #527 and once for the partly-equipped arc, and a
claim of the form *"the five above 310"* survives neither. It was 328 before the material arm; the
`assert OK` count is unchanged, those four riding an existing state's driven block rather than a new
frame.)

**`_assert_denial_pelt_take` IS PNG-LESS, AND THAT IS FORCED RATHER THAN CHOSEN.** An inedible
quarry's denial take line is what the claim is about, but re-targeting the deny sheet to the
shared-tile wolf needs that herd in the world list at THAT point in the walk — and this harness's
state order is load-bearing, so moving a roster push to suit one claim re-points every frame after
it. The chain asserted is the real one (`denial_forecast` → `denial_take_bbcode`) against the same
fixture table the chooser state renders, and **`band_panel_compose_deny`'s edible boar directly above
is the live control**: its line must still read food and waste, or "state the materials" would be
satisfied by a producer that replaced the food clause instead of joining it.

**THE WORST-CASE PARTY CARRIES THE PACK CLAUSE, and its fixture holds TWO piles of one material at
two ratings.** A batch is one pile at ONE RATING, so a fixture with one batch per material would pass
just as well against a producer that summed them — which is the retired trade scalar rebuilt out of
its own replacement. The fourth claim is therefore a NEGATIVE: the two amounts' sum must not appear.
The clause rides the `Carried:` row rather than adding an eighth line, so the strip's measured extent
is unchanged (`band-city-panel.md` → "The parties strip's SEVEN lines"), and the row is found by its
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
makes the second easy to miss when counting a log by eye.) (**The split sheet is opened by PRESSING
the real `⌂ Split` footer button**, found by `HudWidgets.MISSION_LAUNCH_META` — `_open_split_sheet`
closes any sheet already open first, because an open sheet REPLACES the footer and the second and
third states would otherwise have nothing to press. Writing `_party_compose_open` directly would pass
against a mission button that no longer opens anything, which is the regression the frames exist to
catch.) (The
only `ERROR:` lines in a clean log are Godot's own shutdown RID-leak noise, which is why the status
is the verdict and an `ERROR:` count is not.) (The two tallies are no longer equal, and that is not
a miscount: `_assert_scroll_only_where_sanctioned` and `_assert_band_columns_ignore_content` each
emit several `PASS` lines under one `assert OK` heading.) (Count the `PASS` tally as `: PASS`, not a
bare `PASS`: one `assert OK` line contains the word in its own text — `4 rung marks are hoverable
(tooltip + PASS)` — so a bare grep answers one too many. The figure recorded before
`_assert_chart_reads_the_settled_party` added its two was **112**, and a measurement of that same
build reads **111**, so the previous record was one high by exactly that miscount.) Both of the
errors this harness used to expect are gone: the 11-frame `Zone_band` 25px overflow (issue #374
re-homed the band zone's optional rows and widened the wide shell's flanks) and, after it,
`band_panel_parties_inspector_wide`'s `Zone_parties` pair — one VBox needing 310px of a 300px box,
reported twice, once by `_assert_zones_within_bounds` and once by `_assert_zone_content_fits`. That
one is closed by tightening `PARTIES_INSPECTOR_LINE_SEPARATION` and merging the strip's two ORDERS
lines (`band-city-panel.md` → "The parties strip's SEVEN lines"), and
**`band_panel_worst_case_party` is what keeps it closed**: the fixture that state replaced was not
the worst case — a hunt party carrying every optional detail line at once needed 328px where that
one needed 310 — so the state stages one, asserts the strip really renders all SEVEN lines (a
shorter strip fits, so every assertion goes green on a state that has stopped measuring anything)
and PRINTS its extent, which reads **294 of the 300px box**.

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

**The PER-SOURCE CARRY AXIS contributes SEVEN `PASS` to `ui_preview`, ZERO frames and nothing at all
to `band_panel_preview`.** Two of the seven are the husbandry-hint pair becoming a 2×2 (both kits
against both a wild herd and a pen, since the pen tier is gated on the SOURCE now); the other five
are `_assert_a_pen_prices_on_the_keepers_carry`, driven through `DrawerComposeController`'s real
seam over the chapter's locally-built pen-axis roster.

**The dock harness is untouched because no quarry in it is corralled.** In `ui_preview` exactly FIVE
frames move and they are precisely the corralled-herd compose sheets — `hunt_crew_herders` ·
`herd_pen_self_feeding` · `herd_pen_extending` · `herd_pen_foddered` · `improvement_done_penned` —
each in the Kit hint alone, now `pen 12.0 per keeper` where it read the stalking kit's attack and
sled.

**Their NUMBERS are unchanged**, because `BandFx.kit_roster_fixture()` carries no pen-axis kit: the
roster's max on the pen axis equals its bare tier, so the ratio is 1 and the repricing
short-circuits. Measured by rendering HEAD's two files and diffing by SHA-256 in both directions — 5
of 279 differ, and restoring the change reproduces the post-change set byte-for-byte.
Sabotage-verified by reverting the axis to the job's: exactly the four discriminating claims fail,
two naming `0.3 against 0.3` (the two kits quoting one pen the same number — the cancellation the
defect hid behind) and two naming the wild hint rendered at a pen. The other three are the "must not
move" guards and correctly stay green.

**THE PEN AND THE VANTAGE JOINED `BandKitTiers`, and that contributes FOUR `PASS` to `ui_preview`,
ZERO frames and nothing at all to `band_panel_preview`.** Those two axes were the ones a per-kit
readout had to answer off the ROSTER's fresh tier, so a dry-`hurdles` band's pen compose
sheet read `pen 40.0 per keeper` against a sim collecting 12 and a Scout card read 2 tiles of sight
against a reveal at 1. `BandFx.kit_tiers_rows` states all five axes now (it stated three, and a row
that omits an axis exercises the absence path rather than the real one), and
`chapters/compose_rungs.gd::_assert_the_appended_axes_read_the_band` drives the pair each axis
needs: the fresh tier AND the worn one, since a client stuck on the roster passes the first alone
and one that had stopped resolving passes the second alone.

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
is no rung to round to and no rung to name. The frame COUNT is unchanged in both harnesses; one
frame was RENAMED (`band_panel_compose_deny_kit_mismatch` -> `band_panel_compose_deny_pending`, its
subject having moved from "these numbers were priced for another kit" to "the answer has not landed
yet").  **MOST OF `ui_preview`'S RAID FRAMES MOVED, AND THAT MOVE IS THE ARC.** Every expedition
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
that needs it uninstalled.** `band_panel_compose_deny_pending` clears the sender before the kit pick
(a pick re-renders through the real handler, so a still-installed answerer would take that render's
question and land the reply during `_settle`), renders, asserts, and reinstalls. It is the only
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
`labor-ui.md` → "A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED". The fodder face contributes ONE
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
while measuring nothing.  **The COMPOSE-SHEET FLOAT's claims ride `band_panel_compose_hunt_short`,
and they are a SET because no one of them is a fix on its own.** `_assert_zone_content_fits` passes
TRIVIALLY once the sheet leaves the zone — an empty box fits anything — so a float that moved the
641px overflow somewhere unmeasured would look exactly like a repair.

**`_assert_compose_float`** therefore asserts the sheet is really gone from the parties zone AND
whole in the float (both by the Send button's own `HudWidgets.SEND_HUNT_CONFIRM_META`, never by a
face), that the zone holds what is left, that the float fits the VIEWPORT, that its card holds its
own content (the `AutoSizingPanel` lie `panel-framework.md` records — a card fitted too short still
DRAWS at its content's size), and that it clears the panel card. The last is the `event_dock` inset
idiom, negative control included: the vacuity guard fires on the axis the two are NOT stacked along,
and a live control first shows the very same `intersects` test firing on these very rects with the
float moved onto the card.

**`_assert_compose_in_zone` on `band_panel_compose_hunt` is the paired negative** — a trigger stuck
ON satisfies every claim above (a whole sheet, in a float, clear of the card) in a dock with ample
room, so the tall side dock is where the sheet must NOT float.

**`_assert_float_leaves_the_map_clickable`** carries the overlay half: `BandComposeFloat` is the
card and nothing more — no full-screen catcher, because the dock's sheet stays open through a map
pick — and that is driven through `Viewport.push_input` against this harness's own
`_unhandled_input`, the `_assert_open_strip_reaches_the_map` idiom, with the open band beside the
float as the precondition, the float's own RING as the claim and three samples 3px outboard of its
edge as the complement. Reading the node's `mouse_filter` back would only say what it was configured
as, not what the Viewport does with it.

**`_compose_surface()` is what keeps the rest honest**: `_assert_hunt_sheet_chart` and
`_report_compose_widths` search the FLOAT when the sheet is floated, since pointed at `_panel` they
would go vacuous the moment the float works — any new assertion about a floated sheet must go
through it. Sabotage-verified on two DISJOINT mutations: the trigger forced always-ON fails the
paired negative first and 23 further deny-state claims that search `_panel` for a sheet that should
never have left it; forced always-OFF fails exactly three — the zone-content-fits assertion naming
`needs 641px … the box is only 265px (short by 376)`, and both float assertions refusing to prove
anything. Exactly ONE frame moved for the whole change (`band_panel_compose_hunt_short`); the other
71 are byte-identical to the pre-change baseline (222 `assert OK` + 229 `PASS` before the
CRAFTABLE-KITS arc (issue #493) added sixteen `PASS` claims and no `assert OK` — the kit repricing's
own six in `_assert_kit_reprices_the_source` (two about the ratio's denominator, two about the
retreat's own `stay_fraction` field, and the end-to-end take/cap pair), the four in
`_assert_dock_chart_carries_the_kit` (the chart-known precondition, the two drawdown answers that
must move under `dispersion`, and the sim-mirror hold crew that must not), and six more across the
compose sheet's own claims.

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
the knowledge zone 36px over its 300px box and bought it a HEIGHT TIER. That added four `PASS`es
(157 → 161): the SETTLING head/row split became two claims where the folded readout was one, the row
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
the rest. 179 + 111 before `_assert_chart_reads_the_settled_party` added its vacuity guard and its
crew claim to `band_panel_compose_hunt`; that step moved ZERO frames, the defect it catches being
byte-invisible. 179 + 114 before the FILL TARGET's retirement, which removed two
`_assert_band_panel` calls — the dock sheet's offers-a-fill-target claim and the quarry chooser's
drops-the-stale-target one — and repointed a third. 174 `assert OK` + 106 `PASS` before
`band_panel_worst_case_party` — that step gained FIVE `assert OK`s and seven `PASS`es, and only
three of the five are new assertions: the other two are `band_panel_parties_inspector_wide`'s bounds
and content-fits pair, which had been reporting `ERROR` and now report `assert OK`, so a reader
counting only the new state's own would come up two short. 175 `assert OK` + 91 `PASS` before the
recall-verb pair and the compose float's two latch guards — that step LOST an `assert OK` and gained
fifteen `PASS`es, the old `_assert_row_recall_confirms` having printed its own raw `assert OK` line
where its replacement reports through `_assert_band_panel`, 175 + 77 before the sampled party AXIS's
three guards, 175 + 69 before the compose sheet's FLOAT, 173 + 67 before the unsampled-party guard,
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

## The DENIAL raid's frames (`docs/plan_denial_raid.md` slice 2)

Three states across two harnesses, and the launch pair must be judged AS a pair: the verdict table
answers one outcome per key, so a table that answered the same one for all four would satisfy either
frame alone.

- `band_panel_preview` **`band_panel_compose_deny`** — the viable raid. The range verdict, the estimate
  caveat, the quiet take line, the primary Send, and the three floor surfaces that must be ABSENT.
- `band_panel_preview` **`band_panel_compose_deny_repelled`** — the SAME quarry with only
  `denial_estimates` swapped, so the frames differ in the sim's answer and in nothing else. Its verdict
  is asserted by EQUALITY rather than `contains`, because the claim is what the line does NOT also say:
  a repelled party quotes no turn count, and a `contains` would pass on a line that quoted one.
- `ui_preview` **`expedition_denial_panel`** (`chapters/band_expedition.gd`) — the launched party's
  drawer, plus three PNG-less claims about the verdict's structure that no frame can carry.
- `band_panel_preview` **`band_panel_compose_deny_deep_party`** / **`band_panel_compose_deny_short_party`**
  — a band whose idle workforce (12) outruns `max_expedition_party_size` (8), raiding a quarry whose
  `denialPartyNeeded` (11) outruns it too. The first opens the sheet through the REAL `choose_quarry`
  (the path that arms the seed) and reads `Party 11 · of 12 idle`; the second steps back to 4, a
  `repelled` row, so the refusal has a count to name. **The band shape is the whole fixture** — no
  other band in the set has more idle workers than the sampling axis, so a stepper reading the wrong
  field is invisible everywhere else. `_denial_needs_deep_party_rows` puts the requirement INSIDE the
  table (repelled below it, `past_recovery` at and above) and the table stops there, which is the shape
  `snapshot.fbs` describes; `_denial_party_needed_for` DERIVES the field from those rows for every
  fixture in the file rather than stating it beside them, so no table can quote a party its own rows
  contradict. **It derives on `SourceForecast.denial_outcome_succeeds`, never on "is not `repelled`"** —
  `horizon` is neither, and the looser test quoted a row whose projection merely ran out as the party
  that breaks the herd. **No fixture in this file stages a `horizon` row at all**, so every table here
  derives the same number under either rule and the defect is invisible to every frame; the PNG-less
  `_assert_denial_party_needed_skips_horizon` is what covers it — a constructed
  `repelled → horizon → past_recovery` table that must derive the THIRD party, its own negative (a
  table that never succeeds quotes no party), and a cross-check that the success set is exactly the
  verdict table's `VERDICT_OK` entries, those being one answer stated twice. **The counted and numberless refusals are a pair**: `band_panel_compose_deny_repelled`'s
  table is repelled at every size, so the sim quotes nothing and the verbatim sentence is what must
  render there.
- **Adopting a quarry now SEEDS the party**, so `band_panel_compose_deny_in_reach` re-pins
  `_send_expedition_count` before rendering — the chooser assertion above it drives the real
  `choose_quarry`, which arms the one-shot the next denial render consumes.
- `band_panel_preview` **`band_panel_compose_deny_short_handed`** — the SAME deep-party quarry in front
  of the reference band's THREE idle workers, i.e. the one state in which this sheet's Send DISABLES.
  Only the BAND changes between it and `band_panel_compose_deny_short_party`, so the pair differ in
  supply alone, and they are asserted as a pair: the short-party frame's Send must stay LIVE (a party
  the player under-sized still launches) or the disable rule would pass by disabling everything. Its
  own four claims are the precondition (`idle < needed`, without which the rest are vacuous), the
  disabled `Not Enough Hunters` face, the reason naming BOTH numbers, and the counted repelled refusal
  being SUPERSEDED rather than printed beside it.
- `band_panel_preview` **`band_panel_compose_deny_open_high`** — the REPORTED verdict shape: a bounded
  expectation over an unbounded bad run (`turns_to_collapse_high == 0`). **No other denial table in
  either harness leaves an end open**, so no frame could show what the old phrasing did with one — it
  dropped the expectation and quoted the LUCKY end alone, beneath a take line priced at the
  expectation. Asserted by EQUALITY, since half the claim is what the line must NOT say, plus that the
  caveat still rides under it (the caveat is gated on `denial_turns_phrase`, which the rewrite
  re-pointed at the lead figure). The other three shapes ride the PNG-less
  `_assert_denial_turn_clause_shapes`, which drives `denial_turns_clause` over constructed forecasts —
  a turn clause is a string, and the sheet renders a plausible sentence whichever draw it led with.
  It also pins the IN-FLIGHT span there rather than leaving it to the drawer's own frame, the span
  being chosen once for the whole clause. Sabotage-verified three ways, each failing a DISJOINT set:
  leading with `low` fails all four range verdicts, dropping the unbounded-high clause fails the
  open-high frame alone, and collapsing the only-on-a-good-run branch fails that shape alone.
- **`ui_preview`'s `herd_hunt_party_size_bound` is DELETED**, with the cap it staged: `idle 6 >= max
  party 2` no longer binds anything, so the frame could only have rendered the max-useful note under a
  party-size name. `herd_hunt_labor_bound` is the surviving half of that pair. **No surviving fixture in
  either harness gives an expedition or hunt sheet more idle workers than `max_expedition_party_size`**,
  which is why dropping that clamp moves zero frames — the deleted state was the only one that made it
  bind.

**`_rich_text_containing` exists because the verdict and take lines are BBCode.** They are built by
`HudWidgets.forecast_label`, a `RichTextLabel`, and `_has_label_containing` walks `Label`s only — so a
`Label`-scoped assertion on either would find nothing and pass vacuously. It returns the whole PARSED
line rather than a bool for the equality claim above.

**Two fixture rules the denial tables must follow**, both because a fixture that breaks one makes the
assertions decorative: a row's `delivered_food` is what the PACK holds and everything else killed is
`wasted_food` (a raid that hauled its whole kill is a hunting raid wearing a denial outcome, and the
waste readout would have nothing to state); and a `repelled` table's kill counts are small but
**non-zero** — a repelled party is outbred, not incapable.

**A THIRD rule went with arc #527's retired account, and the reasoning is worth keeping.** Both
products came off ONE conversion of that same split — `delivered_trade` rode the carried share and
`wasted_trade` the rest, because the sim runs `hunt_yield.apply(take.wasted)` beside
`hunt_yield.apply(take.carried)` — so a table stating a zero `wasted_trade` beside a large
`wasted_food` was a herd no live server could produce. **The general rule survives the account: a
fixture that states one half of a sim-side pair must state the other from the same split.**

**The INEDIBLE table was the exception to that rule and states the exception's own reason.**
`_denial_pelt_only_rows` hauls the whole kill and wastes nothing, because `carry_room_biomass` answers
`NO_CARRY_BOUND` for a species paying no provisions — the pack is measured in provisions, so a quarry
that pays none never fills it. **With the trade account retired that table has no product left to
quote**, so its rows carry an all-zero food account and the frame's claim is the NEGATIVE one: no
false `0.00 FOOD`, judged against the edible boar beside it. `band_panel_compose_deny`'s EDIBLE boar,
where the pack binds hard, is where the waste clause itself is proved.

**`_assert_denial_viable`'s take claim is an EQUALITY over the whole line**, not a `contains`: half
the claim is what the sentence must not also say, and a `contains` passes on a line carrying an extra
clause. (It was written against a waste stated food-only, which satisfied every containment test while
silently dropping the hides the retired trade half accounted; the equality form is what survives the
retirement, and the expectation is composed from the VOCABULARY and the fixture's own arithmetic
rather than re-derived through `denial_take_bbcode`.)

**The TWO SPANS are asserted on different harnesses, and each names its own.** `band_panel_compose_deny`
expects the launch clock — both ends of the band plus `DENIAL_OUTBOUND_TRAVEL_TURNS`, then the travel
split — with the leg stated as a constant derived from the fixture's OWN geometry (band (71, 18), boar
(75, 18), 2 tiles a turn ⇒ `ceil(4 / 2)` = 2) rather than asked of `outbound_travel_turns`, which would
re-derive the expectation through the code under test. `expedition_denial_panel` expects the at-the-herd
clock UNSHIFTED **plus the negative that the launch wording appears nowhere on it** — a clause builder
emitting neither form would satisfy the positive alone only by accident. Sabotage-verified on disjoint
mutations: zeroing the outbound leg fails the launch claim and nothing in `ui_preview`; forcing the
from-launch wording on both surfaces fails exactly the in-flight pair.

The floor-absence claim matches its heading **upper-cased**, because `HudWidgets.alloc_section_label`
upper-cases what it is given; the vocabulary const as written matches nothing, which is how that
clause first shipped passing with a Policy row put back on the form.

`cargo xtask command-guard` carries the half neither preview can: it drives the denial confirm through
`HudWidgets.SEND_DENIAL_CONFIRM_META` and parses the emitted line with the REAL server parser, which
is the only thing that can assert the four-token grammar. **Each mission's confirm wears its OWN
meta** — a search for "the send button" on a parties compose sheet cannot tell which mission it just
launched, and the two emit different signals with non-interchangeable payloads.

### The party-axis guards, and why one of them is an INVERSION

`_assert_party_past_the_rungs_is_quoted` is the former `_assert_unsampled_party_has_no_forecast`, kept
at its own call site and turned around: the sim's sampled party LADDER made an exact match strictly
worse than the contiguous axis it replaced, so a party past the sampled sizes is now QUOTED at the top
rung with a note naming it, rather than blanking the sheet. It is inverted rather than deleted because
its subject did not change — what a party past the rungs gets is still the one thing this state can
see — and it carries the paired NEGATIVE beside it (a party ON a rung renders no note), without which
every claim passes on a sheet that annotates every raid.

**It reads the LIVE herd, not the fixture builder.** `_set_world_herds` runs every fixture through
`_floorify_estimates`, and that is what puts `floor` / `party_workers` **on the rows** — a raw
`_quarry_herd_fixtures()` table encodes the party in its KEY alone. A guard that scanned the builder's
output for the sampled party axis therefore found `0` everywhere and its claim collapsed into a
tautology (it passed on `0` vs `1`). It reads `_hud._band_labor.find_world_herd(...)` instead.

**Every estimate fixture in that file still samples CONTIGUOUSLY (1..N), so no frame can reach a party
BETWEEN two rungs** — which is the case the ladder made common and the case a blanked sheet was
reported on. `_assert_party_ladder_rounding` is what covers it, PNG-less over CONSTRUCTED tables
(`_ladder_hunt_estimates` / `_denial_ladder_rows`, the shipped `LADDER_PARTY_SIZES` plus the denial
table's `DENIAL_REQUIREMENT_ROWS` run), by the `_assert_denial_party_needed_skips_horizon` rule: which
rung a party rounds to is a number, not a picture, and the sheet renders the same plausible readout
whichever row it came from. It asserts the tie (6, between 4 and 8) rounding DOWN, an untied gap (13)
rounding UP — without which the tie rule is satisfied by a lookup that simply floors — a party past
the last rung resolving to 64, and the note's exact text by EQUALITY against the vocabulary.

**`_assert_denial_quoted_party_note` is the rendered half**, staged on a LADDER denial table so the
sentence is seen to reach the sheet. It is PNG-less deliberately: staging it as a frame would move the
state order this file's whole walk depends on. Three things make it non-vacuous — the DEEP-PARTY band
(the stepper's ceiling is the band's idle workforce, and the reference band's three would clamp a
party of 6 back onto a rung), an explicit precondition that the stepper really is sitting on that
party, and the companion claim that the verdict is still THERE, a sheet that lost its figures and kept
the note being strictly worse than the blank it replaced. **It puts the reference band back afterward**
— `update_band_alerts` keeps a losing-population diff against the last roster pushed, and the next
state is `band_panel_no_idle`, whose turn orb draws its calm breath only while there are no attention
entries; without the restore that one frame moves.

Sabotage-verified on two mutations failing DISJOINT sets: restoring the exact party match fails the
between-rungs claims naming the blank (`delivers=false`, `quoted for 0`, `got ""`) plus the rendered
sheet's fallback line, while dropping the note fails exactly the four note assertions and leaves every
resolve claim green.

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

### The KIT PICKER's three states (`docs/plan_denial_raid.md`)

`band_panel_compose_deny_kit` / `_kit_open` / `_kit_mismatch`, specified in `band-city-panel.md` →
"The KIT row rides both dock sheets". Three things about the harness half:

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
- **The quarry carries `defense` + `durability`** (`QUARRY_DEFENSE` / `QUARRY_DURABILITY`), chosen so
  the combat gate's verdict FLIPS with the kit — effort at the big-game tier, a flat refusal
  bare-handed. Without them the gate answers `stated == false` and the mismatch frame would show a
  sheet saying nothing at all.
- **`command_guard` composes a NON-DEFAULT kit on every path**, because `Main._kit_token` omits the
  tail at the job default and the omitted line is byte-identical to the pre-roster one. The composed
  id must be written BEFORE the sheet is opened: the commit button's payload is captured in a
  `pressed` closure built during the render, so a selection written afterwards is not the one the
  button carries and the line comes out untailed, asserting nothing.
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

**`builders` did not parse, and the pool was unstaffable for a whole slice.** Reported from live play:
the Builders `+` did nothing and the event dock read *"Not connected to the server."* The client was
connected. `sim_runtime::command_text`'s `assign_labor` role match enumerated
`scout | warrior | agriculture | husbandry` and returned `UnexpectedToken` for anything else — and the
native bridge (`bridge/command.rs` → `send_line`) **parses a line before it sends it**, so the command
was rejected locally and never reached the socket. No build queue could progress, because nothing could
ever stand on the pool that funds it.

- **A role has to be admitted TWICE, by two files that do not know about each other**: the text grammar
  in `sim_runtime`, and the sim's own `handle_assign_labor`. `band-city-panel.md` asserted *"the sim has
  parsed `builders` since §2.5"* — true of the handler, false of the grammar, and the two were
  conflated. The sim half was right the whole time.
- **THIS GATE EXISTS TO CATCH EXACTLY THAT, AND IT PASSED**, because its band-wide-role drive was a
  single `scout` line. An assertion that passes because the case was never asked is the failure mode
  this repo keeps re-learning; a guard whose whole purpose is "the client's lines parse" must ask about
  **every** line the client can build, not one representative.
- **The drive is a sweep over `ASSIGN_LABOR_ROLES`** — scout · warrior · agriculture · husbandry ·
  builders — each with a non-default kit, **plus the bare `assign_labor … builders 2`**, which is the
  exact form the pool's `+` emits for a player who never opened a kit picker. The kit-bearing form
  alone would have missed it.
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

Sabotage-verified by reverting the Rust role addition: the guard fails naming both forms and the token —
`the server parser REJECTED 'assign_labor 0 71204 builders 2' — UnexpectedToken("builders")`.

## `command_guard`'s SHIPMENT drive asserts an AMOUNT, not a handle (arc #527, issue #517)

`_drive_send_trade_expedition` is the one drive whose subject is a number. Everything else in this
gate asks *"is this the right band / the right kit"*; a manifest asks *"is this an amount the band
actually holds"*, and a client that gets it wrong emits a line that parses, names the right band and
carries the right kit — every other assertion in the file green.

- **The piles are FRACTIONAL and authored in the sim's own TICKS** (`TRADE_FOOD_HELD_TICKS`
  21_050_001, `TRADE_HIDE_HELD_TICKS` 4_567_891), so the entry's `cargo_held_ticks` and the Rust
  half's comparison are exact — a decimal here would re-round the very thing being compared. The food
  pile is adversarial TWICE: a tenth rounds it up to `21.1`, and flooring it onto the fixed-point grid
  alone still emits `21.050001`, which the parser's `f32` round-trip lands one tick ABOVE the pile.
- **The cargo is loaded through the rows' own `+`, to the end of each pile.** `_set_cargo_amount`
  clamps a press to what the band holds, so the last press leaves the exact held amount on the row —
  the path the sheet documents. A drive that wrote the manifest itself would test its own arithmetic.
- **The Rust half quantises what the parser recovered the way the SIM does** (`scalar_ticks`, the
  `f32` multiply included — `f64` here would agree with a client the server refuses) and refuses a run
  in which no shipment was composed from a fractional pile, the vacuity twin of the kit and
  differing-handles preconditions: whole units survive any rounding.
- **The destination is seated directly, not picked through the popup.** An `OptionButton`'s popup is
  an embedded subwindow and this half runs `--headless`; WHICH tie is chosen is `ui_preview`'s
  `trade_picker_destination`, where the pick is a real pointer gesture.
- **`manifest_failures` is unit-tested** (`cargo test -p xtask`) over lines the real parser produces —
  both rounded spellings failing, the floored one passing — so the regression is reachable without
  launching Godot, the treatment `kit_failure` already has.

## The RECALL VERB pair, and the compose float's two LATCH guards

Four claims added to `band_panel_preview`, none of which a frame can carry: three of them are about a
tooltip or a dialog, and the fourth is about a number nobody renders. The behaviour they guard is
specified in `band-city-panel.md` → "THE RECALL VERB FOLLOWS THE SIM" and the two latch bullets under
"A COMPOSE SHEET THE ZONE CANNOT HOLD LEAVES THE ZONE".

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

**`_assert_unknown_zone_box_does_not_float`** rides `band_panel_compose_hunt_short`, where the mark is
latched at the short dock's genuine 641px — the only configuration in which the two answers differ,
which is why the block leads with that precondition and refuses to claim anything under it. It makes
the box unknown the way the live client does (a collapsed panel) and drives the REAL
`_party_compose_floats`. Sabotage: restoring the `_parties_zone_box()` fallback fails exactly that one
assertion (`mark 641px, which WOULD float against the 360px fallback`).

**`_assert_mark_dropped_on_dock_change`** reads the outcome of the real `set_dock(SIDE_LEFT)` + render
that already sat in that block. The mark it judges is STAGED (`_stage_impossible_compose_mark`, four
viewport heights) because **no fixture here naturally produces a mark that overflows the tall dock** —
that dock holds this sheet comfortably, which is what `_assert_compose_in_zone` asserts one state
earlier — so a real mark leaves the two answers identical. The mark is the INPUT to the rule; the rule
is `_note_parties_zone_box`. Sabotage: dropping the reset fails the two claims and not the
preconditions (`the mark from the SHORT dock did not survive the move (now 4608px)`).

**Zero frames moved for all four**, in either harness: 72/72 `band_panel_*` and 344/344 `ui_preview`
byte-identical to the pre-change baseline. (`ui_preview`'s `telling_panel_unread.png` is flaky
run-to-run on its own, unrelated to this change and to the frame set's bit-identity claim elsewhere in
this file — it was observed differing between two runs of IDENTICAL code.)

### …AND NEITHER OF THOSE TWO GUARDS COVERED THE LIVE PATH, WHICH IS WHY THE SHEET FLOATED AGAIN

The empty hunt sheet was reported floating out of a tall LEFT dock a second time, with both guards
above in place and every assertion in this harness green. Two things account for the gap and both are
about **which question the harness was in a position to ask**:

- **No state staged an EMPTY compose form as a composing act of its own.** Every compose fixture writes
  `_party_compose_open` directly and picks a quarry first, so the smallest the sheet ever is — the form
  a player sees the instant they press `🏹 Hunt`, on a band with no parties — was never rendered from
  that entry point. `band_panel_compose_hunt_no_quarry` looks like it covers this and does not: it
  reaches the empty form by CLEARING a quarry mid-act, so it inherits the full form's mark and never
  arms a fresh measurement.
- **Every render in this harness happens from a coroutine resumed at `process_frame`**, i.e. the most
  favourable point in the frame for the deferred container sort to have been flushed by the time
  `_measure_party_compose` resumes one frame later. The phantom reading therefore never reached the
  mark HERE even with the guard fully broken — measured: reverting `_party_compose_measurable` to its
  column-width-only form leaves `_party_compose_needed` at the correct 207. **No rendered state can see
  that**, in either direction, which is why the new guard drives the PREDICATE in the window rather
  than judging a frame.

**`_assert_empty_compose_opens_in_the_zone`** (on `band_panel_compose_hunt_empty`) presses the REAL
footer launcher — reached by `HudWidgets.MISSION_LAUNCH_META`, valued on the mission, since all three
buttons come from one builder and their faces carry the mission glyph — and then asks the predicate
twice: **unmeasurable** in the pre-layout window and **measurable** after `_settle`, with the mark that
survives asserted to equal the laid-out reading. Its vacuity guard is the whole point: the pre-layout
column must read HIGH enough to have floated the sheet (it reads **1278px against a 1055px box**, where
the laid-out answer is **207**), or refusing to record it proves nothing.

**`_assert_zone_holds_its_compose_sheet`** states the invariant directly and against the MEASURED
NUMBERS rather than the dock edge: a zone with room for the sheet keeps it. Its precondition is that
room, so a state where the sheet genuinely does not fit refuses to claim anything instead of passing as
"correctly floated", and it locates the sheet by NODE IDENTITY (the controller's own
`_party_compose_sheet`, walked up to whichever surface owns it) — the empty form's Send is disabled and
carries no confirm meta, being a reason rather than a confirm. **It is called at the STATE, not inside
the block above**, so a trigger stuck ON — which takes the phantom reading out of the parties column
and trips that block's own precondition — still has this claim asked of it.

Sabotage-verified three ways, each failing a DISJOINT set: reverting the guard to the column-width test
fails the pre-layout claim alone (and nothing else in the run, which is the demonstration that no other
assertion here could see it); pinning the predicate FALSE fails the paired positive and the mark claim;
and forcing `_party_compose_floats` true fails the zone-holds-its-sheet claim, with the pre-layout block
loudly refusing its own precondition rather than passing.

**One frame added, none moved**: 73 pre-existing `band_panel_*` PNGs byte-identical to the pre-change
baseline (captured by stashing the change and re-rendering), plus the new
`band_panel_compose_hunt_empty`.

## The QUEUE's controls, and the two traps they walked into (`docs/plan_standing_upkeep.md` §4.7b)

`_render_queue_control_states` runs inside `_render_build_queue_states`, on the SAME three-entry
fixture the states above render, because each of its four claims is about the block as drawn rather
than about a fixture of its own — and the exclusion claim is about two lists at once, which no fixture
can stage alone.

> ### ⛔ A CLICK ON A QUEUE ROW FREES EVERY ROW, so a row captured before one is a freed object
>
> `_toggle_queue_settings` → `_repage_work_zone` rebuilds the block, and `_click_control` on a freed
> `Control` **raises**: the assertion block ends with no `FAIL` line, the strip is left open over every
> state that follows, and the run reports failures several states later that have nothing to do with
> the cause. `_find_queue_row(animal)` re-finds a row from the LIVE tree by its verb face, and every
> click in these blocks goes through it. The closer at the end of `_assert_queue_row_settings` already
> documented this for its own click; §4.7a ②'s kit made the ANIMAL row expandable, which reached the
> same trap one click earlier.

> ### ⛔ A `✕` GRAMMAR PROBE IS NOW A STATE CHANGE, and it is inherited by every state below
>
> The withdrawal is OPTIMISTIC and keyed on the TURN, which does not advance in a harness — so
> `_assert_unqueue_command_grammar` and `_assert_pending_queue_row`, which press the real button to
> read the line it emits, leave real withdrawals on the overlay. Both restore what they withdrew
> (`_pending_labor.clear()` / `drop_pending_unqueue`) so they stay grammar probes. Without that, the
> pending-queue states one block down lose their rows and fail for a reason nothing in them names.

**The FLOW is asserted where it is DECIDED, not where it is drawn.** No shipped dock is wide enough
for the settings pair on one line (342px of strip on the tall LEFT dock, 368 on the 1920 BOTTOM one,
against 444 — 408 of pickers plus the withdrawal's 32 and its gap, since the `✕` rides that line
now), so a rendered one-line frame is unreachable — `_assert_queue_settings_predicate` asserts
`HudWorkVocab.queue_settings_one_line` on both sides of its threshold and that the reserved height
follows the wrap, and `_assert_queue_settings_flow` REPORTS the width at every dock it renders. That
is the payoff of the wrap being a predicate both the reservation and the builder read: it is checkable
without a layout.

**The DRAG is driven TWICE — through the controller's callables AND as a real mouse gesture — because
the two pin different halves and neither implies the other.** `_render_queue_drag_state` calls
`_queue_drag_data` / `_queue_can_drop` / `_queue_drop` directly: that is the ARITHMETIC — the payload,
the edge the pointer sits on, the index the drop computes — it needs no window, and it is what proved
the per-band ordering fix. `_assert_queue_reorder_by_real_gesture` pushes
`InputEventMouseButton` / `InputEventMouseMotion` into `get_viewport()` and lets Godot's own GUI drag
machinery run: that is the WIRING — whether `mouse_focus` resolves to the handle, whether the walk up
the parent chain finds a `get_drag_data`, whether a drag ever begins at all.

⛔ **EVERY FIXTURE ABOVE HAS A MODEL FOR EVERY QUEUE ENTRY, AND THAT IS WHAT MADE THEM ALL BLIND TO
ONE CLASS OF DEFECT.** A band keeps its labor row on a source it has taken to zero take crew while
that source is QUEUED, and the work board admits a row on its take crew — so the wire queue can carry
an entry the block cannot draw, turn after turn. On a self-consistent fixture a position counted in
the DRAWN list and one counted in the WIRE queue are the same number, so no claim could tell them
apart. `_build_hidden_queue_band_fixture` is the one that can: three wire entries, the first held at
zero crew. `_assert_queue_positions_are_the_wires` then makes the claims the others cannot — the
ranks are `[1, 2]`, the top drawn row's `▲` is ENABLED and only the bottom's `▼` is disabled, neither
row wears the `▸`, and all **three** senders of `build_order` (`▲`, `▼`, and the drag through
`_assert_queue_drag_sends`) send the wire's index. Three separate arithmetics over one list: a fix to
any one of them proves nothing about the other two. Reverting the rank to the drawn index fails eight
of them. The frame is `band_panel_queue_hidden_entry`.

⛔ **AND EVERY BUTTON THE QUEUE GREW WITH THE ARROWS IS DRIVEN THE SAME WAY.** `_drive_click` — a real
`push_input` press and release on a window point — is what `_assert_queue_arrow_click` and
`_render_queue_withdrawal_state` use, because `pressed.emit()` cannot see a control that is covered,
zero-size, `IGNORE`-filtered or **disabled**, and the reorder pair shares 32px with 2px between while
the `✕` now rides a strip whose height is reserved rather than measured. The disabled case is the one
that matters most here: the arrows are deliberately dead at the ends of the queue, so a harness that
emitted the signal would read a live command off a control the player cannot press. The frame is still taken
**while the drag is live**, and the block is still asserted NOT to have rebuilt under a `rerender()`,
which is the state no other frame reaches.

> #### ⛔ "A HARNESS CANNOT DRIVE THE GESTURE" WAS FALSE, AND IT LET A DEAD GESTURE SHIP GREEN
>
> What used to stand here was *"Godot exposes no public getter for what `set_drag_forwarding` installs
> (`_get_drag_data` is a virtual the Viewport calls), so what a harness CAN read is what the player can
> see."* The getter is beside the point: a harness does not need to READ the callables, it needs to
> make the ENGINE call them — and it can, through `Viewport.push_input`.
>
> The reorder was unusable in the real client for a whole release. Pressing the handle merely opened
> the row's settings; no drag, no preview, no drop. Every direct-callable claim above passed the whole
> time, because the row's `gui_input` toggle fired on the **press** and `_repage_work_zone` freed the
> marker before the pointer had travelled far enough for Godot to ask for drag data. **A claim whose
> subject is the ENGINE's routing has to push events.**

Three things the real gesture needs, each measured rather than assumed:

- **The motion must carry `relative` AND `button_mask`.** Godot accumulates `relative` as a VECTOR SUM
  and compares its length to a threshold, so a single teleporting motion — or one with no button held —
  is not a drag at all. `QUEUE_GESTURE_KICK` deliberately overshoots on the first motion, which is what
  keeps the gesture's start independent of how far apart two `WORK_ROW_HEIGHT` rows happen to sit.
- ⛔ **The PHYSICAL cursor decides the drop's LOCAL position.** Godot picks the drag-over CONTROL from
  the pushed event, but localizes the point it hands `can_drop_data` / `drop_data` from
  `Viewport.get_mouse_position()`, which on a root window reads `DisplayServer.mouse_get_position()`.
  Pushed events alone therefore route to the right ROW and then ask *"which EDGE?"* about wherever the
  human's mouse is sitting — measured here as a drop that landed BELOW a row the pointer was in the top
  quarter of, i.e. an `above` flag that is pure noise. `Input.warp_mouse` is the only lever that moves
  what Godot reads: `_drive_drag` warps with every motion and puts the pointer back where it found it.
- **It skips under `_is_headless()`.** There is no viewport worth pushing into on the dummy driver.

**The handle's affordances are still read off the node** — the move cursor, the tooltip, the
`MOUSE_FILTER_PASS`. What changed is that the pair they exist for is now BEHAVIOURAL: a real click on
those same pixels must still open the settings strip, and a completed drag must NOT (the Viewport
consumes the button-up for the drop and never forwards it to `gui_input`). `_click_control` sends both
halves of a click for the same reason — the queue row's toggle lives on the release now.

**The withdrawal state re-pushes the same fixture on the same turn**, which is the command's own
recapture: the server broadcasts after every command, so a row that only survives until the next
snapshot fails here. It doubles as
the re-seat this layer needs — an optimistic write runs `_after_pending_change`, which re-renders the
SELECTED unit, and in this harness that is a stale reference band rather than the queue fixture.

> ⛔ **THE REASON NARROWED, AND THE OLD ONE IS NO LONGER TRUE.** It was *"that capture still carries
> the stale turn-written `buildQueuePosition`"*. `PopulationCohortState.buildQueue` is captured LIVE
> off the allocation, so `unqueue` drops the entry on the command's own recapture and the block would
> lose the row with no overlay at all. What the tombstone still covers is the **press→reply round
> trip** — the frames between the `✕` and the server's answer, which no wire field can reach — and
> the rollback for a send that never went. `band-city-panel.md` → "④ THE WITHDRAWAL…" carries the
> same correction; the two files must not drift back apart.

## The EXPANSION's frames, and the three traps they paid for (§4.9 item 9c)

`_render_queue_expanded_states` runs inside `_render_queue_control_states`, after the wire-rank block
and before the withdrawal, on a fixture **longer than any other in this file** —
`_build_long_queue_band_fixture` builds ON the two-entry reference band and appends `QUEUE_LONG_TAIL`
plant entries, so the head marker, the builders pool and both webs are the ones every other queue
state already asserts against.

⛔ **THE PAIRED NEGATIVE RUNS FIRST, ON THE SAME BAND.** `band_panel_queue_collapsed_long` asserts the
3-row block and its `+11 more` and that **no expanded list exists**. Without it every claim below
passes on a mode that is always on, and the collapsed block is the state the game spends nearly all of
its time in.

**THE ENTRY COUNT IS CHOSEN AGAINST THE TIGHTEST BOX, and it is reported rather than assumed.** The
auto-scroll frame is meaningless unless the list OVERFLOWS its viewport, and only the 1920 BOTTOM dock
is short enough to overflow at any reasonable length: `_report_queue_expanded_geometry` prints the
box, the declared viewport, the rows it affords and the scrollbar's width at every dock these states
render, so *which docks scroll* is a measurement. Fourteen entries fill **9.3 rows of viewport on the
bottom dock and 22.3 on the tall LEFT one** — the LEFT dock does not scroll, and the frame says so.

### The nine states, and what each one alone cannot tell

- **`band_panel_queue_collapsed_long`** (tall LEFT) — the mode is OFF by default and the block is
  unchanged: 3 rows plus `+11 more`, no expanded list.
- **`band_panel_queue_expanded_doors`** (tall LEFT) — both doors, both directions, all real clicks;
  the work inspector cleared on entry; press-and-slide-off toggles nothing.
- **`band_panel_queue_expanded`** (tall LEFT) — one row per entry; board / chips / pager / inspector /
  `+N more` all ABSENT; head and POOLS PRESENT; the sanctioned scroll, once, under `ZONE_WORK`, by name.
- **`band_panel_queue_expanded_arrows`** (tall LEFT) — a real `▲` and a real `▼` on **row 4**, past
  the 3-row cap, so a row only the expansion draws.
- **`band_panel_queue_expanded_settings`** (**1920 BOTTOM**) — ⛔ the expansion open AND a row's strip
  open, in the tightest box this panel ships.
- **`band_panel_queue_expanded_autoscroll`** (1920 BOTTOM) — the pump, the physical-pointer read and
  the hover re-resolve.
- **`band_panel_queue_expanded_scrolled`** (1920 BOTTOM) — the list keeps the player's place across
  the rebuild its own click causes.
- **the empty-queue survival block** (1920 BOTTOM, PNG-less) — the mode is the player's and outlives a
  band with nothing queued.
- **`band_panel_queue_expanded_hidden_entry`** (tall LEFT) — the expansion's own row loop counts the
  WIRE queue, not the list it drew.

⛔ **THE STRIP FRAME IS REQUIRED, NOT THE EXPANSION ALONE.** A frame with the expansion open and a
frame with a strip open are **two disjoint frame families with the defect living in the gap** — the
exact shape that hid a 64px overflow in §4.7 and an inspector-height defect before it. It runs on the
1920 BOTTOM dock because that is the shortest box the panel ships, and the strip is opened on **row
5**, an entry that had no way to be configured at all before this mode.

> #### ⛔ A REQUIRED FRAME'S HEADLESS SKIP WRAPS THE INPUT, NEVER THE FRAME
>
> `_assert_queue_expanded_settings` carried its `_is_headless()` return ABOVE its own claims, so under
> the dummy driver it returned after the geometry print and the row-count guard: no `_save`, no
> `_assert_zones_within_bounds`, no `_assert_zone_content_fits`, no `_assert_scroll_only_where_sanctioned`,
> no extent report. The gap the frame exists to cover was **unasserted in exactly the run that is meant
> to be the cheap full-coverage one**, and it exited 0 while doing it. None of those claims needs a
> click; the CLICK needs a viewport. The skip now wraps the input alone and the headless path opens the
> same strip through `_toggle_queue_settings`, so the **combined state** is asserted in both modes and
> only the mechanism differs (the claim's own text says which). Measured: the block reports **10 claims
> (7 `PASS` + 3 `assert OK`) headless and the same 10 windowed**, against **0 headless** before.
> `_assert_queue_expanded_scroll_persists` takes the same shape for the same reason.

### `_drive_drag` grew a HOLD, and the hold is the auto-scroll's whole test

The hold awaits frames at the destination **with nothing pushed at all** before the release, which is
the gesture a player makes when they park the pointer at the edge — and a scroll that only advanced on
motion looks identical to a working one until you stop moving. `hold_probe` is called on each held
frame and is **awaited**, so a probe may itself capture: the auto-scrolled, mid-drag list exists on no
other frame, since the drop ends the gesture and `_repage_work_zone` rebuilds the block at scroll 0.

The frame's claims are the three mechanisms, apart: `scroll_vertical` starts at 0 and has advanced by
the travel the claims need after the hold; a row that was outside the viewport is inside it now;
**the drop mark moved to a row that was not visible at the start, with the pointer stationary**; the
release sends the target's **WIRE** index, read back through `Main.format_build_order` the way
`_assert_queue_arrow_click` does; and the block did not rebuild under the gesture. It skips under
`_is_headless()` like `_assert_queue_reorder_by_real_gesture`.

> #### ⛔ THE HOLD IS A CONDITION UNDER A WALL-CLOCK BUDGET, NOT A FRAME COUNT
>
> The pump is on the unscaled wall clock, so `QUEUE_AUTOSCROLL_HOLD_FRAMES` (45) bought
> `45 × whatever a frame costs here` — measured **36px**, against the **17px** the arrival claim needs,
> with a large share of it coming from the awaited PNG `_save` mid-hold. On a faster machine or a
> faster save three claims fail at once, and it was the one outcome in this harness that depended on
> machine speed. `hold_done` makes the hold end on **`QUEUE_AUTOSCROLL_TARGET_ROWS` (2) of travel**
> instead, under `QUEUE_AUTOSCROLL_TRAVEL_BUDGET_SECONDS` (3.0, ~9× the 0.33s the pump honestly needs),
> with `QUEUE_AUTOSCROLL_HOLD_FRAMES_MAX` as a ceiling so a DEAD pump cannot hold the run open. The
> deadline is inside the callable so the gesture ENDS on it and the travel claim reports what was
> actually reached — a blown budget fails loudly rather than skipping. Measured windowed: **0 → 56px
> over 77 frames** (i.e. ~230fps, which is why 45 frames was only ever worth ~33px). **Every tick is
> still the engine's**: nothing calls `_queue_autoscroll_tick`, the hold pushes no input, and deleting
> the pump's step still fails the same **4** claims — what changed is only when the hold STOPS. The PNG
> is now captured on the frame the travel lands, so it always shows a scrolled list mid-gesture.
>
> #### ⛔ THE LAST SAMPLE IS TAKEN AT THE RELEASE, NOT A FRAME BEFORE IT
>
> The hold's loop probes and THEN awaits a frame, and the release is pushed after that frame — so the
> pump ticked once more between the final sample and the drop, and `want_position`, computed from that
> sample's `mark`/`above`, could name the row under the pointer one step BEFORE the row Godot drops
> onto. `_drive_drag` probes once more immediately before pushing the button-up, with no
> `process_frame` between, so the expectation is read from the state the drop sees. Measured, the
> window it closes is real but narrow here: **1px of travel** between the penultimate sample and the
> release across three runs, which crosses a row boundary only sometimes — an intermittent failure
> about nothing under test.

> #### ⛔ `Engine.time_scale` IS 0 IN EVERY RENDER HARNESS, SO A `_process` DELTA IS ZERO
>
> `band_panel_preview`, `ui_preview` and `blend_probe` all pin `Engine.time_scale = 0.0` for
> determinism, and `preview_watchdog` documents that every `delta` in the process is therefore zero.
> A per-frame pump driven by the frame delta advances by **exactly nothing** here — measured as
> `0 → 0px over 45 frames`, indistinguishable from having no pump at all. The auto-scroll reads
> `Time.get_ticks_usec` instead, which is unscaled. **Any future harness claim about a rate must ask
> what clock the code under test is on**; this one was written, run, and read as a bug in the fixture
> before the frozen clock was found.

### The sanctioned-scroll guard was NARROWED, not extended

`SANCTIONED_SCROLLS` gained `[BUILD_QUEUE_EXPANDED_SCROLL_NAME, ZONE_WORK]`, and its existence claim
is the **first conditional one**: the list must be found exactly when `_queue_expanded` **and** the
work zone is mounted, and never otherwise.

⛔ **AND EACH OF THE THREE CLAIMS IS NOW MADE ONLY WHERE ITS ZONE IS MOUNTED.** The narrow shell
parents only the ACTIVE tab's zone (`_reparent_zones` detaches the rest), so a zone can be in `_zones`
and nowhere the walk could find it. The band zone's claim already guarded for that; the parties one
did not, so asking this guard from a narrow-shell WORK-tab state reported a parties list that had
"lost" a scroll it was merely detached from. Every pre-existing call site is on a wide shell where all
three are parented, so no claim was weakened.

> #### ⛔ A CLICK FREES THE LIST, NOT ONLY THE ROWS
>
> `harness-band-panel.md` already records that a queue row captured before a click is a freed object.
> The expansion reaches the same trap one level up: `_toggle_queue_settings` → `_repage_work_zone`
> frees the `ScrollContainer` too, and `scroll.is_ancestor_of(strip)` on a freed instance **raises** —
> which ends the assertion block with **no `FAIL` line**, leaves the strip open over every state
> below, and still exits 0 with a healthy PASS count. It happened once here and was caught only by
> reading the log for the assertions that should have been there. `_expanded_queue_scroll()` re-finds
> it from the live tree after every real click.

### The falsifications, and the one that failed nothing

Each defect was restored, the run counted, and the fix put back.

- **The auto-scroll pump deleted → 4 failed.** *the pointer held STILL … and the list scrolled
  anyway* · *carrying N row(s) that were outside the viewport … into it* · *the drop mark MOVED under
  a stationary pointer* · *it names a row that was NOT on screen*.
- **The hover re-resolve dropped → 2 failed** — the two drop-mark claims above. The wire-index claim
  did NOT fail, and correctly so: the arithmetic is right either way, and what the re-resolve fixes is
  WHICH row the mark and the drop name.
- **The expansion's loop ranks its DRAWN list → 7 failed**, every one of them on
  `band_panel_queue_expanded_hidden_entry`: *the two drawn rows wear the WIRE's ranks* · *NEITHER
  drawn row wears the `▸`* · *the TOP drawn row's `▲` is ENABLED* · both `▲`/`▼` meta-value claims ·
  *a REAL click … emitted no build_order at all* · the `▼` command line.
- **The `_work_open_key` clear removed → 3 failed.** *entering the mode CLEARED the open work
  inspector* · *the board back and no inspector springing back onto it* (the board came back at 8 rows
  where it had 9) · *no work inspector is open beside it*, which is the required strip frame's own.
- **The header toggle fired on the PRESS → 1 failed**, and only because the claim was written for it.
  See below.
- **A stub board left drawn in the mode → 7 failed** — the zone-fit and zone-bounds guards at **both**
  docks (168px over on each) plus the board and chips absence claims.
- **The scroll restore deleted → 2 failed.** *the rebuild the click caused KEPT the player's place* ·
  *with that row still inside the viewport*.
- **The restore written into the builder instead of deferred a frame → 1 failed**, and only because
  the frame scrolls past 100px: a fresh `VScrollBar` is a `Range` and a `Range` ships `max = 100`, so
  at 84px (three rows) the un-deferred form PASSES and at 112 (four) it clamps to 100 and fails. That
  is what fixes `QUEUE_SCROLL_PERSIST_ROWS` at 4 — a falsification that does not bite is a constant
  chosen wrong, not a fix confirmed.
- **The empty-queue prune put back → 2 failed.** *the MODE survived it* · *reselecting the band that
  HAS a queue comes back EXPANDED* (it came back with 1 drawn row, i.e. collapsed).

⛔ **THE PRESS-TIME HEADER TOGGLE FAILED NOTHING, WHICH MEANT A CLAIM WAS MISSING.** The head sits in
the same place in both modes, so a plain click looks identical whichever edge fires it, and the header
is not a drag source — the drag starts on a ROW's marker — so the press-time rebuild kills no gesture.
What a press-time toggle really breaks is **press, slide off, release**, which must change nothing.
That is now driven with `_drive_drag` from the header onto the last queue row, and it is the only
claim that can tell the two edges apart. Its twin asserts the row the release landed on did not open
either — `mouse_focus` latched on the header.

> **A FALSIFICATION THAT FAILS NOTHING IS A RESULT, not a formality.** Two of the six here changed the
> harness rather than confirming it, and both were places where the obvious claim was about the fix's
> *mechanism* instead of about a consequence the player could see.

## The build states, the rollback and the pending queue row (`docs/plan_standing_upkeep.md` §4.6a/b)

Three blocks land at the end of the run, after `_render_build_queue_states`. Order is load-bearing
here as everywhere: each restores the reference band and the herd roster on its way out, and the
rollback block leaves the pending overlay empty.

**`_render_work_build_state_states` — the WORK ROW's build states, as a SET.** ONE band, FOUR forage
rows differing only in their meter and the wire's own countdown: climbing (`🌱45%`, `SIGNAL_DEEP`),
declared-with-nobody-on-it (`🌱⚠`, `WARN`), losing ground (the same stalled face, from the wire's `-3`
rather than from the staffing), and parked-with-keeping-covered (`🌱60%`, `SIGNAL_DEEP`). All four
faces and all four inks are asserted by EQUALITY, and the faces are composed from `HudWorkVocab`'s own
formats so the claim is the FORK and not the format.

- **The SET is the claim.** A row builder that marked EVERYTHING passes the two stalled claims; one
  that marked NOTHING passes the two healthy ones. Sabotage-verified in both directions, each failing
  a disjoint pair.
- **THE BAND HAS NO BUILDERS, and that is what makes four states reachable at once.** The pool is
  BAND-level, so every row on one board is asked against one crew count — with hands on it no row
  could be `UNSTARTED` and the states would have nowhere to differ. It is an ordinary live state
  besides: another band's pool can be funding the same rung, which is why the map's badge sums the
  count across bands.
- **`_assert_work_row_and_badge_agree` is the two-surface claim, and no per-surface assertion can make
  it** — each is perfectly self-consistent while contradicting the other. The row's face comes off the
  RENDERED board (`HudWorkVocab.WORK_ROW_BUILD_STATE_META`, valued the face, because the three states
  differ by one glyph and a text search would only confirm the string already assumed); the badge's
  verdict is composed the way `BandOverlayRenderer._queue_source_badge` composes it. Its COUNTS —
  four build rows, exactly two stalled — are what stop a hard-wired verdict agreeing with itself.
- **`_work_row_build_faces` keys on the row's NAME label, found by `SIZE_EXPAND_FILL`.** Every other
  slot in a work row is a fixed column, so that flag is the one structural handle on the label whose
  text the assertion is trying to join against.

**`_assert_pending_assign_rollback` — PNG-LESS, and it has to be**: a card showing three builders is a
perfectly ordinary card. It asserts the two `has_method` names `Main` probes for (a failed probe fails
SILENTLY, so a rename would simply stop rolling anything back), drives the REAL `_emit_assign_labor`
for both writes so the payload under test is the one `Main` receives, then drops ONE and requires an
unrelated pending edit on the same band to SURVIVE. **The survivor is half the claim** — a rollback
that cleared the whole overlay passes the first half and is a worse bug. Sabotage-verified: erasing
the entity's whole record fails exactly the survivor claim.

> **What it does NOT drive is `Main`'s one-line `if not _send_formatted_command(...)`.** `Main` is a
> scene node with a `_ready` this harness does not stand up, so the block drives everything up to and
> including the method that handler reaches by name. The seam it cannot reach is one comparison.

**`_render_pending_queue_states` — the pending queue row, NEGATIVE FIRST.** One confirmed entry and
nothing declared (assert no row below the head position, no `○` anywhere), then the same band with a
build declared through the real `record_pending_assign`, then the WIDE dock for the height budget,
then a turn advance that reconciles the overlay away and leaves the row CONFIRMED. Four claims on the
pending row — it sorts LAST, it wears `○` and states no date, it has no `▸`, and its `✕` still emits
`unqueue 0 72 18` — asserted together, since any one alone is satisfied by a row that got the other
three wrong. Sabotage-verified by admitting every un-positioned row: the negative fails in two states
naming the phantom rows it found.

- **`_declare_pending_build` calls `_hud._bandpanel.rerender()`, NOT `Hud._after_pending_change`, and
  that is a HARNESS fact rather than a client one.** That method also re-renders the SELECTION card,
  whose occupant this harness stages separately — a `_band_fixture()`-shaped Band 2 pushed long before
  this block — so the re-render replaces the panel's subject with the OTHER fixture of the same band
  and two of the queued rows vanish. A live client cannot reach that: both dicts come from one
  snapshot.
- **The wide dock reads `Zone_work` at 252px of a 300px box.** `Zone_band` at 749/300 and
  `Zone_parties` at 300/300 are the same numbers `band_panel_build_queue_wide` prints and are
  structural (the board is `EXPAND_FILL` and pages).

## THE LEG IN FLIGHT — the two-leg fixture existed and could not tell the defect from the fix

**`_track_climbing_patch_fixtures` had staged a two-leg entry since the destination track landed, and
no assertion in this file could have caught the reported defect on it**, because its band's forage row
carried **no `improvement` token**. The wire publishes one — `snapshot::population::resolved_build_job`
is `patch_build_verb`'s answer, which honours a declaration at or above the rung being raised, so a
`sow` on untended ground publishes `sow` — and without it the client's own `build_verb` fell through
to the Cultivate meter and the board read the leg **by accident**. `_track_band_fixture(build_job)`
states it, which is what makes `band_panel_rung_track_climbing` reproduce the played state.

Four claims per state, from `_assert_queue_row_states`, and the SET is what makes any of them worth
anything — a fix that repointed the whole row passes the last two alone:

| claim | why it is there |
|---|---|
| the row is still titled for its DESTINATION | that is what the player ordered, and it is why moving the percentage costs nothing |
| the date column, by EQUALITY | one string carries the verb, the leg's percent AND the whole climb's turn, so asserting them apart would let a row state a leg's date beside a leg's percentage and pass |
| the source row's rung chip | the two Work-tab readouts must name ONE rung — the shape of the reported defect |
| the wanted percent is composed through the TILE CARD's own producer | the claim is that two surfaces AGREE; a literal on each side lets both be separately plausible |

**Four states, and three of them are PNG-less** — a percentage is a number and a row quoting the wrong
rung's meter renders a perfectly plausible row:

- the reported two-leg `sow`, on `band_panel_rung_track_climbing`;
- **the FIRST turn of banked work** (`TRACK_FIRST_TURN_WORK_DONE`, one unit of fifty ⇒ 2%) — the
  assertion that actually catches the bug, since at 60% a renderer that had merely swapped one meter
  for another passes;
- **a single-leg `sow` on already-tended ground, which must be UNCHANGED** — its leg IS its
  destination, and without it "read the leg" is satisfied by a renderer that always names the rung
  below the one declared;
- **the animal twin** (`band_panel_queue_leg_animal`), a `corral` on an untamed herd: the two webs
  share no fixture and no rung table, so a fix reaching only the plant one passes every claim above.

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

**A FIFTH STATE SITS BESIDE THEM AND IS NOT ONE OF THE FOUR: `band_panel_queue_ring`**, the pen
extension the leg re-pointing cannot reach at all. A ring widens the rung its herd already stands on,
so it has no leg and the ladder credit is structurally zero for its whole life — the row read
`🐄 Corral Wild Fowl   turn 151 (0%)` in play. Its percentage is the herd's own
`pen_extend_progress / pen_extend_cost`, and the state carries a PNG because a queue row for a
completed rung is a shape the four above never render. Three claims, the first a PRECONDITION without
which the other two pass for free: that the pen rung is full with no rung in flight, that the face
still names the widened rung's verb, and the date column by EQUALITY. Its fixture pair (30 of 40 work
⇒ 75%) differs from the herd drawer's `herd_pen_extending` (42 of 70 ⇒ 60%) on purpose, so a row that
reached for the wrong ring fails on the number instead of coinciding with it.

**`_report_queue_row_columns` PRINTS the row's two columns and both worst cases**, the
`_report_work_row_name_column` rule one block over: the verb made the date column longer, and what a
red line there asks for is a design decision. It reads **queue NAME 126px** (the widest shipped face,
`🐄 Corral Thunder Mammoths`, needs 189 — it was already ellipsised before this change) and **DATE
168px** (its own widest, `Cultivating 100% · turn 999`, needs exactly 168), beside the BOARD row's
line one at **109px**. Those numbers are what bought the queue row's SOURCE ICON its retirement — the
arithmetic is in `band-city-panel.md`.

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


## The `-5` frame — a fresh entry and a stalled one have to be in ONE frame (§4.9)

`band_panel_build_queue_not_yet_estimated` renders a three-entry BUILD QUEUE whose rows read three
different faces at once: `Queued 0%` (the `-5` head), `⚠ Stalled 0%` (the `-1` herd behind it) and
`⚠ ∞ turns, losing ground (0%)` (the `-3` patch behind that).

**ONE FRAME, BECAUSE THE DEFECT IS THAT TWO OF THEM LOOKED IDENTICAL.** A frame staging only the fresh
entry proves nothing — it is green with the fix and green with the defect restored, on a board that
has never drawn a genuine stall beside it. `_assert_not_yet_estimated_reads_apart` therefore asserts
the pair AND their inequality: the head's exact composed face, that it carries no
`RUNG_HAZARD_GLYPH`, that the entry behind it still carries one, that the two strings differ, and both
inks (`INK` for the head, `WARN` for the stalled one) — because a neutral word in amber still says
*warning*.

**`_assert_not_yet_estimated_producers` IS PNG-LESS AND DELIBERATELY SO.** Three of the four swept
producers cannot be seen in any one frame — a pace tints a compose face, a stall verdict marks a map
badge — and the stated failure mode in this client is *two producers disagreeing about one meter*. It
asks the producers themselves (`build_turns_remaining` passes `-5` through and specifically not onto
`-1`; `build_pace` answers `BUILD_PACE_UNKNOWN` and neither stops the sheet nor paints it as climbing;
`build_is_stalled` is false for a staffed `-5` **and still true for a `-3` beside it**, so the verdict
has not simply stopped firing; `build_turns_clause` states no clause), so a consumer rendering
correctly off a producer that has quietly stopped passing the sentinel still fails here.

**Nothing on this frame is pressable**, so nothing is driven: it is a readout defect end to end.

### `UNKNOWN_BUILD_TURNS_SENTINEL` had to be re-aimed, for the THIRD time

`tools/ui_preview/chapters/improvements.gd` keeps a value *one past the last the schema defines* and
asserts that an unrecognised negative renders as the STALLED hazard. It was `-5`. **The day the wire
spelled `-5`, that claim stopped pinning the rule and started pinning the bug** — it was asserting the
`⚠ Stalled` that this fix removes, and it is what failed the `ui_preview` run. It moves to `-6`; the
constant's own doc has said since its second re-aim that this is required maintenance, and this is the
third round. **It is the sharpest of the three**, because `-5` is the one value the client must render
as a NEUTRAL face rather than merely a different one.

### Falsifications

Baseline before the slice: **789 PASS / 0 FAIL**; after: **805 PASS / 0 FAIL** (`band_panel_preview`,
exit 0 both). `ui_preview` is **1344 PASS / 0 FAIL** before and after, the re-aim included.

| Restored defect | Failures |
|---|---|
| `build_sentinel_value` folds `-5` onto the stalled face (the shipped pre-fix render) | **4** — the head's face, `carries NO hazard mark`, `the two READ APART … (["⚠ Stalled 0%", "⚠ Stalled 0%", …])`, and the head's ink |
| …the same branch deleted outright instead | **1** — the head's face only, and it renders `Cultivating 0% · turn 37`: with `-5` still passed through, `build_completion_value` adds a NEGATIVE count to the turn and dates the job five turns in the PAST |
| `build_pace` loses its `-5` arm (a swept consumer left un-swept) | **2** — `` `build_pace` answers no verdict for it (got "growing") `` and `neither stops the sheet nor paints it as climbing` |
| `build_turns_remaining` collapses `-5` onto `-1` again (the root) | **6** — all four render claims plus both producer claims |

**One of them is weaker than the others and it is worth saying so.** Deleting the branch outright
(row 2) trips only ONE assertion, because the resulting face is neither the queued one nor the stalled
one — so `carries NO hazard mark`, `READ APART` and the ink claim all pass on it. They are not
vacuous (rows 1 and 4 fail all four), but the set's floor against *some other wrong face* is the exact
string comparison alone.



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
