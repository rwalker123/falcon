---
paths:
  - "clients/godot_thin_client/tools/band_panel_preview.gd"
  - "clients/godot_thin_client/tools/band_panel_preview.tscn"
  # `command_guard` is gated HERE, not in `harness-headless-guards.md`, because the rationale it
  # needs is the KIT PICKER's: it `preload`s `BandFx.kit_roster_fixture()` — the only cross-harness
  # fixture preload in the tree — and the "compose a NON-DEFAULT kit on every path, and write the id
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

**A clean run exits 0 and prints 236 `assert OK` lines, 332 `: PASS` ones and ZERO `FAIL` ones, over
92 frames.** (It was 226 / 332 / 89 before the under-herded ⚠ was re-aimed at the KEEPING crew: the
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
SHORT tier's merged `Food … · 128.4 hay` line measured against the column it must not wrap in —
353px of 380). Beside them **`_report_zone_content_extent` PRINTS each zone's content extent against
its box** rather than asserting: it reads 299 of a 300px box, and a near-miss and a comfortable fit
are the same green line otherwise. The rows, the merge and the flank widths are specified in
`band-city-panel.md` / `band-readouts.md`.

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

**Their NUMBERS are unchanged**, because `BandFx.kit_roster_fixture()` carries no husbandry kit: the
roster's max on the pen axis equals its bare tier, so the ratio is 1 and the repricing
short-circuits. Measured by rendering HEAD's two files and diffing by SHA-256 in both directions — 5
of 279 differ, and restoring the change reproduces the post-change set byte-for-byte.
Sabotage-verified by reverting the axis to the job's: exactly the four discriminating claims fail,
two naming `0.3 against 0.3` (the two kits quoting one pen the same number — the cancellation the
defect hid behind) and two naming the wild hint rendered at a pen. The other three are the "must not
move" guards and correctly stay green.

**THE PEN AND THE VANTAGE JOINED `BandKitTiers`, and that contributes FOUR `PASS` to `ui_preview`,
ZERO frames and nothing at all to `band_panel_preview`.** Those two axes were the ones a per-kit
readout had to answer off the ROSTER's fresh tier, so a dry-`husbandry_gear` band's pen compose
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
locally-built roster, `BandFx.kit_roster_fixture()` carrying neither a trapping nor a husbandry kit.
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
`BandFx.kit_roster_fixture()` carrying no `husbandry` kit. The figures before the two of them were
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
  for it, the only cross-harness fixture preload in the tree. It is world config the sim publishes
  once, not a per-harness prop, and two copies could quote different tiers or a different job default
  while the `kit <id>` token is asserted against one of them. Every entry states all THREE tiers with
  the BARE value on each axis its kit does not use, which is the wire's own shape and is what
  `KitRoster.unequipped_tier` reads the bare-handed tier off; `none` is authored LAST, exactly as
  `equipment.json` authors it.
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
