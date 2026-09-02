---
paths:
  - "clients/godot_thin_client/tools/ui_preview.gd"
  - "clients/godot_thin_client/tools/ui_preview/**"
  - "clients/godot_thin_client/tools/ui_preview.tscn"
---

<!-- Split out of .claude/rules/client/test-harnesses.md, which was itself extracted from
     clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e.
     The pseudo-table cells this file carries were re-wrapped at 100 columns; no wording changed. -->

# The `ui_preview` harness

The HUD PNG-walk harness and its chapter decomposition.

## `tools/ui_preview.gd` / `.tscn` + `tools/ui_preview/**`

Dev-only preview harness: instances the real `HudLayer` with canned selection/targeting data,
renders each state, and saves PNGs to `ui_preview_out/` (gitignored).

**A state lives in the CHAPTER that owns its arc, not in `ui_preview.gd`** — see "The harness is a
manifest; the states are chapters" below for the layout and where a new state or fixture goes. The
one-card selection layout has its own states — `tile_panel_land` / `tile_panel_no_forage` /
`tile_panel_herd` / **`tile_panel_crowded`** (3 bands + 2 herds: every row visible, drawer capped,
dock not scrolling — the frame the cap is judged on) / **`tile_panel_land_sticky`** (the
sticky-selection ASSERTION, driven through the REAL path: it instances MapView, wires Main's two
signals, clicks the crowded hex, clicks the land row, and replays whatever
`refresh_selection_payload` answers — the payload must not be `"unit"` and the land must still be
lit. Proven to fail with MapView's `select_occupant` `"land"` branch removed) /
**`tile_panel_deselect_keeps_tile`** (the twin ASSERTION for the MAP-click deselect, same idiom — a
real MapView with `set_fow_enabled(false)`, Main's three signals, `handle_hex_click` on a lone herd
and then on an empty hex — asserting the empty hex stays `selected_tile`, the occupant selection
clears, and `refresh_selection_payload` answers `"tile"`. The first click is load-bearing: the clear
branch only arms once an occupant is selected. Proven to fail on both the tile and the payload with
MapView's `selected_tile = Vector2i(-1, -1)` restored) / **`tile_panel_occupant_cycle`** (the
ASSERTION for issue #429 and for the LAND stop that completed it — re-clicking a hex cycles through
everything the tile PANEL lists, not just its bands — same idiom again: a real MapView with
`set_fow_enabled(false)`, Main's FOUR selection signals **plus the `roster_occupant_selected`
relay**, and repeated `handle_hex_click` calls on a hex holding one band and TWO herds, the smallest
stack that can prove every half of the issue. Eight assertions: click 1 lands on the band (bands
still win the first click, the land being the ring's LAST stop), click 2 advances PAST it to the
first herd, click 3 to the second (a multi-herd hex is not stuck on `herds[0]`), the cycled herd
survives the next snapshot's `refresh_selection_payload` → `reapply_selection` (the HUD's
sticky-choice auto-pick does not steal it back), **click 4 reaches the LAND**, **the LAND likewise
survives the next snapshot** — the assertion that catches the failure the `land_selected` →
`show_land_selection` → `note_choice_tile` seam exists to prevent, since the auto-pick fires on
exactly the two-empty-occupant-dicts state the land IS — click 5 WRAPS past it to the top, and a map
re-click after a PANEL roster-row click continues from that row, the property the identity-derived
advance buys.

**The roster relay is the load-bearing half of the wiring**: it is what re-enters `select_occupant`
mid-click, so reading `cycle_index` back out of the member instead of taking it as a parameter fails
this state. Proven to fail on exactly that mutation, and the land pair proven to fail (4 assertions,
this state's three plus the toggle's) when `Hud.show_land_selection` drops to a bare
`_selection.select_land()`. A **PNG-less companion block** rides after it for the smallest ring that
has a land stop — ONE herd, no band, re-using the deselect fixture — asserting the herd ↔ land
toggle in both directions and its wrap) / `tile_panel_unseen` (remembered hex: chips + land row +
the unknown-contents note, NO occupant rows) / **`tile_panel_unexplored_own_band`** (the pair of FoW
rules colliding: an UNEXPLORED hex holding your own expedition, with the LAND row clicked through
the real handler — no terrain rows AND a non-empty roster, the two conditions that between them once
hid every child of the drawer. Four assertions, two of them preconditions, asked of
`%OccupantDetail` itself because a PNG cannot tell a blank drawer from one that rendered fine.
Sabotage-verified against the unconditional roster skip) / `tile_panel_band` (the Band/City pointer
line, not a blank gap, **plus the drawer's `Move`** — and its behavioural ASSERTION: the hex carries
three player bands, the SECOND is selected through the real list path, and pressing the REAL button
must put the HUD into move-band targeting for **that** band (302), not the faction default
`_player_band` (301). Proven to fail with `_on_move_band_pressed` resolving to `_player_band`;
`tile_panel_crowded` additionally asserts the no-panel fallback shows exactly ONE Move, proven to
fail with a second one added) (`tile_panel_feed_shown` — `R` on, both growing left-dock cards
fitting — is RETIRED with the command feed; there is one growing card in that column now, and
`predator_feed` went with it, its alert styling having moved into `HudEventVocab` and onto the
`event_dock_*` frames below). Part 2 (the compose sheet) adds **`tile_panel_compose_forage`** /
**`tile_panel_compose_herd`** (the expedition branch + raid forecast) /
**`tile_panel_compose_gated`** (a locked rung greyed AND its gate reasons rendered beside it, inside
the sheet) — all three must show the map UNDIMMED behind the sheet — and **`tile_panel_standing`**
(the CLOSED read state on a worked source: `⇊ 4 foragers · +2.74 /turn ⚠ · only 2 of 4 working`, the
⚠ and the note being the same two INDEPENDENT flags a Band-panel Current-actions row carries). Plus
four behavioural ASSERTIONS, each proven to fail before it was trusted: a snapshot
(`reapply_selection`) leaves the sheet OPEN, the same refresh CLOSES it when the subject is swapped
(the half that proves the first is not vacuous), starting move-band targeting closes it, and — with
a sheet and targeting BOTH active, the only configuration that can tell the order apart —
`Main.escape_claimant` answers `compose_sheet`.

**A state that exists to judge the picker/stepper/forecast/gate reasons must call the harness's
`_compose_forage` / `_compose_herd`** (which open the sheet); the drawer alone now shows only the
summary + button.

**FOUR ASSERTIONS WERE REPOINTED WHEN THE TOP-RIGHT HUD BLOCK WAS RETIRED** (issue #450 —
`TurnBlock`, `TopBar` and their eight Labels are deleted from `HudLayer.tscn`), and three of them
were about the same thing: the top-bar `⚒ Your people know:` strip. `world_reset` asked whether the
strip was visible before and after `reset_world_state()`; `two_meter_split` asked whether it named
Penning; `forage_fodder_known` asked whether it named the fifth track.

**All three now ask the CACHE** (`_topbar.faction_tracks` / `faction_knowledge`), which is a better
witness than the label was in every case — it is what the reset exists to clear, and what the
faction page's KNOWLEDGE zone reads — and `two_meter_split`'s is now an EQUALITY against its
fixture's own part-learned 0.45, where the strip's test was mere presence (it rendered a track at
any progress above zero). The fourth is `event_dock`'s: the readout block's authored-width claim is
**deleted** with the region it measured, and its `SIDE_TOP` clearance claim is re-aimed at the
**RIGHT DOCK**, which is what a top bar now shares a vertical band with — the `ContentRow` begins at
the top of the screen with `TopBar` gone.

**A chapter that touches a deleted node does not fail politely**: the three strip sites raised
runtime errors that ABORTED their chapters mid-way, which showed up as 40 missing `PASS` lines and
one unrelated-looking failure three chapters later, so grep the harnesses for a node's name before
deleting it from the scene.

**The frame set IS a STRICT BIT-IDENTITY REFERENCE — 274/274 frames byte-identical across
consecutive runs**, the property a HUD decomposition pass leans on: a frame that varies cannot be
pixel-diffed to prove a refactor changed nothing. Two runs of *identical* code used to differ in
**all 184**, and closing that took THREE fixes, not one. (1) **Animation time is frozen**
(`Engine.time_scale = 0.0` at the top of `_ready` — the `map_preview` / `blend_probe` treatment).
(2) **The canvas is pinned AND the maximize is taken DELIBERATELY.** `project.godot` opens MAXIMIZED
and macOS applies — and re-applies — that asynchronously: one measured run rendered **177 of its 184
frames at the monitor's 5120x1410 instead of the intended 1500x900** while the next rendered all 184
at 1500x900, so the runs disagreed on the HUD's LAYOUT (most frames were being judged at a width the
HUD never ships at), and even with the canvas pinned, *whether a run ever passed through the wide
window* left two byte-distinct clusters differing by ±1 on the antialiased edges of ~85 frames
(`window/stretch` is `canvas_items`/`expand`, so every control draws at a fractional 0.78 scale and
every edge is a coverage blend). Dodging the maximize is not available — a late one still landed
mid-run after 30 stable frames — so `_stabilize_canvas` ASKS for it, undoes it, and holds the canvas
for 30 CONSECUTIVE frames before state 1; `_ensure_canvas` (from `_settle`) and the `_capture`
geometry guard then never have to spend a frame mid-run, which is what keeps every state's
layout-pass count equal between runs. (3) **`BandCityPanel.config_path_override` is isolated**
alongside `NarrativeForkPanel`'s: the harness was writing the developer's real
`user://band_city_dock.cfg` (found holding its own `edge=2` / `tab="band"`) and READING the tab back
out of it, so `tile_panel_band` rendered the empty `work` zone in one run and `band` in the next off
nothing but that leftover file — it now asks for `ZONE_BAND` explicitly, the rule
`band_panel_preview` already carries.

**The freeze moved exactly 30 frames and every one was checked, not assumed**: 27 changed only in
the turn orb's calm breath — `0.5 - 0.5 * cos(t)` is at its FAINTEST, smallest instant at phase 0,
so `_pulse_time` is seeded to a quarter period (the breath's midpoint) rather than left to
degenerate; `tile_panel_band` and `herd_corral_under_herded` moved because the OLD frames were WRONG
(an empty work tab; a STALE capture whose rendered cap contradicted its own passing assertion); and
`telling_live_oral_arrival` now shows both pages mid-crossfade instead of only the outgoing one.

**A TWEEN IS THE HAZARD A SHADER `TIME` TERM IS NOT** — `TIME` still evaluates at t = 0, but a
`Tween` at `time_scale = 0` never advances AT ALL, so a page turn pins at `progress = 0`, i.e. the
page BEFORE the turn. `_settle` therefore drives every live tween to its END with one oversized
`custom_step` (`_flush_tweens`), and the ONE state that must capture a turn in flight calls
`_settle(false)` after stepping it a fixed 40% of its own duration — a chosen phase instead of a
raced one. TellingPanel's page turn is the only `create_tween` in the whole client.

**RE-CHECK RULE for anything animated added later** (the same one `map_preview` and `blend_probe`
carry): an AMPLITUDE term (`A * sin(t)`) — or any tween — vanishes or degenerates at phase 0, and a
frame that is deterministic because its subject disappeared is worse than one that varies, whereas
an offset or a midpoint idiom (`0.5 + 0.5 * sin(t)`) survives; classify the new term, and drive any
new tween, before trusting the freeze.

**A FIELD-PAIR GUARD rides on every frame:** `_guard_frame_herd_fields` deep-scans every herd
Dictionary the HUD holds as a state renders (plus every herd handed to `_compose_herd`) and FAILS
the run when `herders_needed_if_managed < herders_needed`, or when a herd carrying herders does not
carry the two EQUAL — the sim's `would_be_herders_needed` is `herd_herders_needed` minus its
ownership gate, so they diverge only on a not-yet-owned tameable herd
(`_tame_worker_cap_herd_fixture`: 0 and 10). It exists because `_under_herded_corral_fixture` set
only `herders_needed` while `DrawerComposeController._forecast_worker_cap` floors the INVESTMENT
rungs on `herders_needed_if_managed`: the floor read 0, the cap collapsed to 1, and the frame
rendered the exact opposite of the cap it documents with NOTHING logged. Managed fixtures now set
both through `HerdFx.set_managed_herders` (the guard caught two more half-set ones the hour it was
added), and it is proven non-vacuous by dropping the second field again — which fails the run.

**The forage-vector states (#426)** — `forage_three_accounts` / `forage_three_accounts_overdraw` /
`forage_dead_season`, built on `_hay_meadow_tile_fixture` / `_dead_season_tile_fixture`. They are
the first forage fixtures to pay anything but provisions, and both write their per-policy ROWS out
by hand rather than taking `BaseFx.seed_forage_rows`' derivation, which seeds the non-food accounts
to 0 by design — the "genuinely non-derivable row" case that helper's own docstring names.

**The hay meadow's two accounts bind DIFFERENTLY on purpose** (food is slow to gather off ground
that carries plenty, hay comes in fast off a meadow that regrows little), which is what makes a
per-account `min(w × per_worker, ceiling)` and a per-account overdraw verdict observably different
from ones applied to a total. **Its third account was TRADE**, retired with the axis by arc #527, so
these fixtures carry food and fodder alone. (Its column was once deliberately non-monotone — `Deplete`
carried a ×4 market markup that put its cell above Eradicate's — and the harvest-floor arc had already
retired that markup before the account went, since a deeper floor earns more only by taking more
BIOMASS.) Eight assertions ride them, each sabotage-verified to fail.

**The ZERO-CREW pairs** — `forage_unstaffed` / `forage_unassign` and their hunt twins
**`herd_hunt_unstaffed` / `herd_hunt_unassign`** — are judged as pairs, never singly: a crew of 0 is
an unassign on a source this band works and a no-op on one it does not, and only the pair can show
that the button, the rename and the improvement control all key off the same test (the rule is in
`labor-ui.md` → "A CREW OF 0 IS THREE STATES"). The hunt unassign state opens the sheet at its
STANDING crew first and asserts the improvement control is there, so its absence at 0 is a change
rather than a herd that is never offered a rung.

**The CORRAL-GATE pair `herd_corral_gated` / `herd_corral_ungated`** (they replaced
`herd_corral_locked_both` / `herd_corral_locked`) is an A/B on ONE fully-tamed herd with only the
faction's Penning moving between them — and the FIXTURE is the load-bearing part: the improvement
control offers one rung, so a gated Corral needs Tame retired, and the part-tamed herd those frames
used to stage renders an offered **Tame** and no gate at all. `two_meter_split` stages the same
shape for the same reason.

**Since the compose sheet stopped rendering a KNOWLEDGE-only gate at all, the A/B reads "no control"
→ "a live box" rather than "a Label" → "a live box"** (the claim is unchanged and sharper — the
animal is identical across the two, so Penning alone produces the offer), and the gated half is now
one of the two frames the SUPPRESSION rule is judged on, `forage_cultivate_locked` being the plant
one: each asserts the ABSENCE of any improvement control, no crop list, and — in the same frame — a
LIVE teaching line naming the craft being earned, which is what makes the removal a progression
rather than a hole. The gated control's SHAPE moved to a SOURCE gate that survives
(`improvement_offered_gated`, re-fixtured onto a TENDED patch on ground that will never take seed,
with Seed Selection KNOWN — `Sow`'s site refusal is the only source gate this sheet can still render,
`Cultivate` gating on knowledge alone and `Corral`'s ownership half being unreachable where only the
next rung is offered), and `forage_sow_locked` pins both halves at once by staging BOTH kinds of
reason and asserting the ground's refusal leads while the knowledge line appears nowhere.

**`forage_cultivate_stressed` IS THE POSITIVE HALF OF THAT MOVE, and it was asserting the bug.** No
rung on either web carries a health gate (`labor-ui.md` → the gate reshuffle's callout), so a Stressed
patch with Cultivation known OFFERS Cultivate; the frame's four claims are the non-Thriving
PRECONDITION, the live checkbox, the crop list beside it, and the ABSENCE of the retired ecology
refusal anywhere on the sheet. The needle for that absence is a chapter LITERAL
(`RETIRED_PHASE_GATE_NEEDLE`), the vocabulary const having gone with the gate. Sabotage-verified by
restoring the phase term: exactly those three claims fail — the precondition rightly does not — while
the gated frame beside them stays green.

**Re-fixture, never re-describe**: a frame whose stated subject stops occurring must get a fixture
that brings the subject back, since rewriting the comment lowers the bar to whatever the code
happens to do.

**The two-axis guards (#442 review).** `herd_compose_crew_noun_after_pen` pins that the sheet's
EYEBROW cannot be resolved against a stale improvement: tick Corral on a pen-ready herd, then open a
WILD herd's sheet — the header must read `ASSIGN HUNTERS`, not `ASSIGN HERDERS`, because
`ComposeState` does not clear `_hunt_improvement` on a source change and the eyebrow is built BEFORE
the re-seed. Its third assertion (the stepper's own axis) deliberately stays PASS under the sabotage
that fails the other two — the stepper was always right, and the header disagreeing with it IS the
defect. A PNG-less guard beside `quick_hunt_note` covers the same axis on
`Hud.quick_assign_hunters`. Both exist because a reviewer found what neither the harness nor a
screenshot showed.

**The CHECKBOX-VISIBILITY pair rides on `herd_corral_ungated`** (#445): the offered box's
`unchecked` art composited over `HudStyle.PANEL_SOLID` must clear `CHECKBOX_INDICATOR_MIN_CONTRAST`,
and the `checked` art's colour with brightness divided out must sit within
`CHECKBOX_TICK_COLOUR_TOLERANCE` of `SIGNAL`.

**Neither is phrased as "an override is set", deliberately** — the first cut of that fix set
`icon_normal_color`, which a `CheckBox` ignores entirely, so an override-shaped assertion would have
passed on a control that rendered nothing; and the ticked half asks about HUE rather than contrast
because the stock tick chip is light and clears a contrast bar unchanged. Both sabotage-verified
against removing the `apply_checkbox` call.

**`turn_orb_unworked_rung`** is the neglect-clock frame: a 6-patch control set plus an under-crewed
herd, asserting that an improved source with NO crew raises a row, that a WORKED one and a RIVAL's
do not, that the countdown reads off the wire (`0` = biting now) and that a source with
`hasNeglectGrace == false` renders no countdown at all — the collision that bool exists to prevent.

**Writing it exposed a live defect**: the attention producers run BEFORE `ingest_snapshot_bands`
(load-bearing for the decline diff) but were asking the labor model for crew counts, i.e. LAST
turn's roster — so every improved patch alarmed as unworked on the first snapshot after a load. The
roster is threaded in now.

**The EVENT-DOCK block (issue #272)** runs last before the icon probe and instances a real
`EventDockPanel` — its own CanvasLayer, like the `tile_panel_band` band panel — then frees it, so it
cannot leak a reserved strip into the other ~239 frames. `Main` owns the reservation fan-out and is
never instanced here, so the harness pushes `reservation_changed` into `Hud.set_reserved_inset` by
hand; that is what makes the frames show the HUD reflowing off the strip. Five states:

**`event_dock_bottom`** (the shipped default — bottom edge, 2 rows, the `notable` floor,
opened-and-closed first so the alerts are READ and the bar is the plain newest-first one),
**`event_dock_top_expanded`** (the other edge with the log open — the bar must be a one-line TITLE,
asserted at exactly one row, and not a second printing of the log's own newest turn-group),
**`event_dock_everything_expanded`** (the `routine` floor, i.e. everything the retired feed carried,
with the log scrolling internally), **`event_dock_alerts_only`** (one row, alerts only — the
`status=feral` row must survive as a `cultivate` kind PROMOTED by its detail token, and every
receipt must be gone) and **`event_dock_pinned_alert`** (4 rows over a FRESH ingest, so the alerts
are unread again and the OLDEST event — a raid five turns back — holds the leading slot above three
newer Notable rows).

**Eleven PNG-less assertions ride the same block**, each saying something no picture can. The `seq`
family: the fixture's **two byte-identical turn-47 raids survive as TWO events** (the de-duplication
fix — under the old `tick\|kind\|label\|detail` signature they collapsed into one, so two wolf packs
raiding one band in one turn were reported as a single raid), a row with no usable `seq` ingested
twice **lands once** (the signature fallback still de-dupes), and two rows both carrying `seq: 0`
**do not collide** (`0` is the sentinel for "never pushed", not a key).

**The ROLLBACK trio** is the one to keep green: `CommandEventLog` is checkpoint state, so a restore
replays events REUSING sequence numbers the client has seen, and the harness drives exactly that — a
batch, a `reset()` (what a full frame does), then rows reusing those `seq` values with different
labels, asserting the new rows are held and the replaced ones are gone.

**The BAND-LABEL trio**: the sim's positional `Band 3` becomes the roster's own name, an id the
roster does not know is left alone, and the substitution **stops at a digit boundary** — a fixture
that CONSTRUCTS `"Four left Band 3 for Band 30"`, since the sim names only one band per label today
and no live event reaches the trap, which would make the assertion decorative.

**The IGNORED-KIND set** (eight) asks the STORED pool, never the rendered rows — "ignored" is a
stronger claim than "not shown", and a rendered-scope assertion narrows silently to whatever is on
screen: both inlets (`ingest_events` and `note_system`, the second being the one every live
`command_echo` actually arrives through, so a filter covering only the first would look right and do
nothing), the `seq` slot the dropped row must not burn, and the `Everything` floor with both
channels on — each beside a positive companion in the same batch, a genuine `system` fault and a
world event that must survive, without which they would all pass on a dock that ignored everything.
Sabotage-verified twice, and the two mutations fail DIFFERENT subsets: moving the filter into
`_visible_events()` fails the four pool-scoped assertions while the two visible-scoped ones stay
green (the render-scope blindness, demonstrated rather than asserted), and moving the ingest test
after the de-duplication fails the `seq`-slot one alone.

**The PERPENDICULAR-INSET set** (the live bug: a top bar drawn over a left-docked band panel)
instances a real `BandCityPanel` docked LEFT — a literal reservation width would prove nothing about
two rects actually clearing each other — and renders `event_dock_inset_left_panel` /
`event_dock_inset_bottom_panel`, asserting the bar's `offset_left` equals that panel's reserved
width, that the two global rects do not intersect on EITHER edge, and that the reservation the bar
publishes is unchanged. It takes a **negative control first, on the same two live nodes**: at zero
inset the rects genuinely DO overlap, so the claim is not satisfiable by two panels that happen
never to meet.

**The CO-EDGE set rides beside it** — `event_dock_co_edge_top` / `_bottom` / `_collapsed` /
`_control` / **`_expanded`** (the last being the only one with the log OPEN, i.e. the TALLEST the
displaced strip gets; it adds `_assert_strip_within_viewport`, which asks whether `_edge_offset +
_cross_axis_size()` still lands inside the viewport and PRINTS the slack, since the two clamps
governing those heights sum to 1.1 of the window and only their absolute caps keep the pair under
the 1080-px canvas floor — see `event-dock.md`), the bar and a `BandCityPanel` on the SAME
horizontal edge (the reported bug: the bar drawn over the panel), asserted as rect non-overlap
behind its own zero-offset negative control, with the collapsed and other-edge cases as the two
claims a latched or edge-blind offset would fail. `Main` is not instanced here either, so the
chapter restates the offset sum from the live panel's `get_dock()` / `current_reservation_size()`;
`_assert_bar_clears_co_edge` guards vacuity on the HORIZONTAL band, the opposite axis to
`_assert_bar_clears`, because two things on one edge share a vertical band for free. Spec in
`event-dock.md`. The **HUD-furniture** half rides beside it — the bar must clear the left dock, the
right dock and the top-bar readout block even with nothing docked at all, which is the case the
first inset fix got wrong — plus three assertions that each of those regions renders no wider than
the AUTHORED width the bar is bounded by, so a scene edit that outgrows a column fails here instead
of overlapping in play.

**`_assert_bar_clears` is what keeps those honest**: it refuses to pass on a bar/region pair that
shares no vertical band. The HUD's regions sit in different bands — a bottom bar is in the
`BottomBar`'s, a top bar in the `TopBar`'s, and only a bar tall enough to reach the `ContentRow` can
touch the docks — so "these rects do not overlap" is true for free of most pairs, and the first
version of this block passed with the fix reverted. Each claim is therefore made on the edge where
it bites: nav backing + turn orb on `SIDE_BOTTOM`, the readout block on `SIDE_TOP`, the two docks
with the log EXPANDED.

**The PROSE-RENDERING set** states its guarantee as a general property rather than a spot check: no
Label the dock renders may contain a `=`, guarded by two preconditions so it cannot pass vacuously
(the scan saw labels at all, and the pool really does hold raw `key=value` details for one to have
leaked from). Beside it, `detail_phrase` is driven directly for the cases a rendered row would pass
while silently DROPPING the fragment — an unknown value, an unknown numeric key, the reported
`category=settle_site at (64,36)`, a value containing a space (`species=Grey Wolf`, which a naive
split loses), and the hidden keys. Beside it the same discipline covers the wire's `{:.3}` casualty
numbers: no rendered detail may show a trailing-zero decimal, scanned over the rendered labels AND
`detail_phrase` across the whole retained pool — **the pool half is not belt-and-braces**, it is
what stopped the claim going vacuous when the one row carrying a padded number drifted outside the
log's five-turn window and the on-screen scan passed with the trim reverted. Driven cases pin the
rest: a fractional `1.750` survives un-rounded (the assertion that stops someone "simplifying" the
trim into an `int()`), a whole `2.000` loses its padding, a bare `100` is left alone (`rstrip("0")`
would answer `1`), and the label's own `killed=` is not repeated beside it.

**The casualty fixtures carry the sim's REAL wire shape** — `killed=`/`wounded=` at `{:.3}`, never
an invented `losses=` key — because the tidier fixture is exactly what made the scan vacuous the
first time.

**The WIDTH-CAP pair** asserts both halves — the strip equals the available band at the normal
canvas and equals `MAX_STRIP_WIDTH`, centred, at `ULTRAWIDE_WINDOW_SIZE` — because a cap hard-wired
on fails the first and one hard-wired off fails the second; that one state renders outside
`_ensure_canvas`'s pinned-canvas guard (which exists to keep every OTHER frame comparable) and
re-pins after, and it is the only frame in the set that reaches the configuration the complaint came
from.

**The OVERLAY pair** covers what the dock owes for floating over live map: real presses driven
through `Viewport.push_input` against the harness's own `_unhandled_input` (which stands in for
`MapView`'s hex picking), **sampled over nine points across the bar's rect** — the centre alone
lands on a row and is consumed whatever the root and card do, so it passed with both of their
filters set to `IGNORE` — with a precondition that a press on open canvas DOES reach
`_unhandled_input`, so a probe that never fires fails instead of passing everywhere.

**That precondition earned its keep twice**: the first attempt read `gui_get_hovered_control()`,
which answers "nothing" in this harness even over a `PanelContainer` that certainly consumes, and
the second found the harness's own full-rect backdrop `ColorRect` swallowing every press (it is
`MOUSE_FILTER_IGNORE` now — it stands in for a `Node2D` map that consumes nothing). Beside it,
`event_dock_over_bright_terrain` renders the strip against a pale field, since every other frame in
the set puts it on near-black and its opacity was never under any pressure. **"Nothing moves down"**
is asserted the same way — on `SIDE_TOP`, against both `LayoutRoot` offsets (taken on the bottom
edge it was true for free), with a negative control pushing the same size under a different id to
prove the reserved-inset path still works and it is the event dock that is exempt. That mirror
consults **`Main.MAP_ONLY_RESERVERS` itself** rather than restating its answer, so emptying that
table makes the harness fan out exactly as the live client would. A birth passing the DEFAULT floor
is asserted beside them — `born` shipped Routine, i.e. invisible unless the player chose
"Everything", and a rung table is one dict entry from that regression. Plus the expanded bar being
one row, and **the strip yields to the map** — measured as a PAIR, the widest bar with the log
closed AND the log open (which collapses the bar to one title line), because those are alternatives
rather than addends and neither is the worst case by inspection. Thirteen of them are
sabotage-verified, run in isolation so each says what it catches — rendering the raw detail leaks
`at=18,31` into the `=` assertion, gutting the generic fallback fails both unknown-token claims
while the table-driven ones correctly still pass (they test a different layer), removing the width
cap fails the ultrawide half, and hard-wiring it on fails the narrow half AND three clearance
claims, a 1280 strip in a 1216 band overhanging both columns — emptying `MAP_ONLY_RESERVERS` fails
"nothing moves down" (and turns every clearance claim VACUOUS, since a pushed HUD no longer shares a
band with the bar: a loud failure either way, never a silent pass), and zeroing the two column
widths fails all five clearance claims non-vacuously plus the three authored-width ones. Plus:
making `reset()` keep `_seen_seq` fails the rollback one, treating `seq: 0` as a key fails two, a
plain `String.replace` in the band substitution fails the boundary one, and hard-zeroing the dock's
`offset_left`/`offset_right` fails all three inset assertions while correctly leaving the zero
control and the reservation-unchanged claim green. It isolates a THIRD prefs file,
`EventDockPanel.config_path_override`, for the same two reasons as the second: the dock persists its
edge / row count / detail floor / channels and these states walk all four.

**The TWO-LINE FACE block (`chapters/button_faces.gd`, issue #383)** runs late in `CHAPTERS`, after
the event dock, and adds NO frame — it renders into an offscreen `SubViewport`, so the frame count
and the bit-identity claim are untouched. A face needing two font sizes is child `Label`s beside an
empty-`text` Button, and `font_color` — plus Godot's own disabled fade — reaches a Button's `text`
and nothing else, so a state coloured through the theme alone moves the BOX and leaves both lines
bright: an unavailable control that reads as available, visible only in the disabled/variant case
and so invisible in an ordinary screenshot. Two halves, covering different seams. The CALLER half
drives the real `build_crew_targets` with a SELECTED and a resting pill in one row — the pair IS the
claim, since a face hard-coding either colour fails on the other — and asserts each pill's lead line
is `HudStyle.button_font_color`'s answer for its variant and its second line that SAME answer at
`POLICY_PICKER_METRIC_ALPHA`. The STATE half renders each hand-built face builder
(`_policy_rung_cell`, `_crew_target_pill`) enabled and DISABLED and compares the PEAK LUMINANCE
inside each line's own rect — the state no live caller reaches today, hence the half nothing else
can see.

**It is measured on pixels, not on "an override is set"**, the `_checkbox_indicator_contrast`
lesson: an override-shaped assertion passes on a control whose override reaches nothing the widget
actually draws with, so what is asserted is that the tint reaches the RENDERED GLYPHS. The peak is
read rather than a contrast-against-the-panel, because the disabled stylebox fades the box UNDER the
text and a contrast measure would conflate that with the fade being asserted.

**PIXELS CANNOT SAY HOW A LINE GOT DIM, which is why the `modulate` claim sits BESIDE them rather
than inside them** — a face dimmed by `modulate` (the double-dim `_policy_rung_cell`'s note rejects,
since it multiplies the box the disabled stylebox has already faded) reads to a luminance measure
exactly like a properly tinted one and passes every reading in the block.
`_face_modulate_is_identity` is what refuses that shape, asked of both states; the first cut of this
guard had the rationale backwards and would have shipped the forbidden implementation fully green.
Measured: `INK` 0.93 → `INK_FAINT` 0.50 on line 1, 0.70 → 0.39 on line 2, against a
`TWO_LINE_FACE_MIN_DIM` of 0.10, with an ink-floor precondition so the dim claims cannot pass on a
face that drew no glyphs. Sabotage-verified FOUR ways, each failing a DISJOINT subset: giving
`_crew_target_pill`'s second line a literal colour fails both caller-half second-line claims and the
pill's line-2 pixel claim (`0.93 → 0.93` — the line staying bright beside a faded box, the defect
itself); making `_policy_rung_cell` ignore its `tint` fails both rung pixel claims while the caller
half correctly stays green (a different layer); dropping `selected` from the crew-target caller
fails the two `primary` claims alone; and **repainting `_policy_rung_cell` at full ink with a dim
`cell.modulate` fails the modulate claim ALONE — all four of that rung's luminance readings stay
green (`0.93 → 0.49`, `0.70 → 0.38`)**, which is the demonstration that the two claims cover
different failures and neither is redundant. Iterate on HUD styling without a server:
`scripts/preview.sh res://tools/ui_preview.tscn`

## The harness is a manifest; the states are chapters

`tools/ui_preview.gd` reached **11,847 lines**, of which a single `_ready()` was **5,945** — and it
took ~104 commits a quarter, a typical one landing 3-9 hunks scattered across it. Two worktrees
adding unrelated states therefore conflicted as a matter of course, and one merge had to reconcile
1,700 changed lines in this file alone. It is now ~730 lines plus a package:

| Path | Holds |
|---|---|
| `tools/ui_preview.gd` | The harness `Node`: `_settle` / `_save` / `_capture` / `_assert_hud`, the canvas + prefs + tween plumbing, the prologue that stands the HUD up, the icon-probe epilogue, `CHAPTERS` + `_instantiate_chapters`, and the one exit `_finish()` |
| `tools/ui_preview/chapters/*.gd` | One `RefCounted` per arc (`hunt`, `forage_crop`, `herd_graze_pen`, `event_dock`, `button_faces`, `forecast_seam`, …), each `run(harness)` plus the fixtures only it uses. A chapter need not render a frame — `button_faces` and `forecast_seam` are both PNG-less, and that is a normal shape |
| `tools/ui_preview/fixtures_*.gd` | Pure `static func` fixtures shared by two or more chapters — `base` (the primitives the other three build on), `band`, `herd`, `forage`, `tile`, `world`, and `rung`, which derives every patch and herd row's `current_rung` from the flags the row already carries and is shared with `map_preview` / `band_panel_preview` / `snapshot_alias_guard` (`test-harnesses.md` → "A fixture's STANDING RUNG is DERIVED, never typed") |
| `tools/ui_preview/node_query.gd`, `readouts.gd`, `compose_vocab.gd`, `input_probe.gd` | Shared `static` helpers: finding a control by identity, reading values back out of rendered text, the compose spine vocabulary, and driving real pointer input through `Viewport.push_input` (the canvas→window conversion, a hover, the two gestures, a wheel notch, a press-and-cancelled-release click, and the `press_left` / `release_left` pair a caller drives apart when the press itself opens a popup) |

**Where a new thing goes.** A state → the chapter that owns its arc. A fixture used by one chapter
→ a method on that chapter. A fixture used by two → a `fixtures_*.gd` `static func`. **`ui_preview.gd`
itself takes a new line only when a whole chapter is added**, which is the property the split exists
for.

**`CHAPTERS` order is load-bearing.** Every state renders into ONE long-lived `HudLayer`, so a
chapter moved is a set of frames changed — `dock_fresh_profile_default` is first precisely so no
later state can leak a preference into it. Append within a chapter rather than reordering.

**The module preloads must stay a DAG.** GDScript rejects a cyclic `preload`, and the land, forage
and herd fixtures genuinely interlock (`food_tile_fixture` seeds forage rows; a herd fixture builds
on a food tile). `fixtures_base.gd` is the layer that breaks it: it holds exactly the primitives
reached from more than one of the three, and depends on none of them.

**A chapter reaches the harness through `h`, never the reverse.** A chapter is a `RefCounted`, so
`get_tree()` / `add_child()` and every `_hud` / `_settle` / `_save` call goes through `h`. `h` is
deliberately untyped: typing it needs either a `preload` of the harness (a cycle) or a `class_name`
on it, and the locals that lose inference as a result are annotated from the harness's own declared
return types where that is possible and left untyped where it is not — which costs a compile-time
check and nothing at runtime.

**The split was verified as a pure refactor, and that is the only way to attempt one here**: all 247
frames byte-identical by SHA-256, and the assertion list — 469 lines at the time — unchanged in
content and order, against a baseline captured first and confirmed reproducible across two runs. The
frame set's bit-identity (below) is what makes that check possible at all. **Do the same for any
later move**: capture the hashes and the sorted `ui_preview: PASS` lines BEFORE touching anything,
and take a control render when something else in the merge could move a frame on its own — that is
how the `button_faces` relocation was separated from the five band-panel frames its merge brought in.

**A BROKEN CHAPTER FAILS THE RUN; IT DOES NOT HANG IT — and `CHAPTERS` therefore holds PATHS, not
`preload`s.** The split's first cut `preload`ed each chapter, which makes a chapter's parse error a
parse error in `ui_preview.gd` itself: the engine answers `Could not preload resource script` and
then `Failed to load script "res://tools/ui_preview.gd"`, the scene's root node comes up with **no
script at all**, and `_ready` — where the whole walk and its closing `get_tree().quit()` live — never
runs. **The process then idles forever**: no PNG written, no `FAIL` printed, no exit status, and the
previous run's frames still sitting on disk looking like a completed set. One such process was found
alive after 59 minutes, and it is invisible to every check we have. The three parts of the fix:

- **`_instantiate_chapters` loads and instantiates the WHOLE roster up front**, before the prologue
  renders anything, and reports a bad one through `_fail` — the harness's `FAIL` token, naming the
  chapter — instead of dying of it. Discovering it mid-walk would leave a half-written frame set,
  which is the same lie one frame at a time.
- **The test is `GDScript.can_instantiate()`, NOT a null check.** `load` on a chapter that does not
  compile answers a **non-null, non-functional** `GDScript`; calling `new()` on it raises
  `Nonexistent function 'new'`, which ABORTS `_instantiate_chapters` — and GDScript answers an
  aborted (non-coroutine) call with the return type's **default**, so the caller sails on. Measured:
  the first cut of this fix rendered ONE frame, printed no `FAIL` and exited 0. `_ready` therefore
  also checks the roster **by count** against `CHAPTERS`, so any future abort in that function is a
  failure rather than a short walk.
- **`_finish()` is the only way out**, and the exit status is the run's own failure tally: a clean
  run exits `0`, a run with any `FAIL` in it exits non-zero, so `grep FAIL` and `$?` cannot disagree.
  **Nothing consumes the status** — no `xtask` task runs `ui_preview`, no CI job does, and the agent
  instructions read the output — so it is free for a caller to start relying on. All five sibling
  render harnesses derive their status the same way; the shared contract is `test-harnesses.md` →
  "The exit status IS the verdict".

**ONE HANG SHAPE THE WATCHDOG STRUCTURALLY CANNOT CATCH: a stall BEFORE the scene loads.** It arms
from its own `_ready`, so a run that never reaches the scene never arms it — and the symptom is
indistinguishable from a slow run: no `FAIL watchdog`, no exit, no PNGs. Observed once for 26 minutes
after an editor `godot --import` pass, where the next WINDOWED launch stalled during window creation;
the log stopped at the OpenGL line, with neither the `[TerrainDefinitions]` load nor `watchdog armed`
after it. **Those two lines are the diagnostic**: a log carrying them has a live scene and a live
guard, so the run is merely slow; a log without them is stuck outside both and will sit there
forever. `--headless` sidesteps it — the compile gate and every non-capture assertion still run
(measured: 163 of the windowed 194 `assert OK`, the rest belonging to states that need a viewport,
plus one `the window never held the pinned canvas` **warning**, which is the dummy renderer being
reported as the driver fact it is rather than counted against the run) — which makes it the fast way
to answer "does this still compile?" without waiting on a display. **It exits 0 on a clean tree**,
so `test-harnesses.md` → "The exit status IS the verdict" reads a headless run the same way it
reads a windowed one.

**Every other hang shape is caught by `preview_watchdog.gd`**, a sibling node in both preview scenes
— see its section in `test-harnesses.md`.

**A textually clean merge of this harness is not a working one.** Merging `main` into the split
auto-resolved `ui_preview.gd` with no conflict and produced a harness that did not parse: the
incoming block called `_find_crew_target`, a helper the split had moved to `Q.find_crew_target`. The
run rendered **zero** frames. Git cannot see that seam, so any merge touching this harness must be
rendered before it is trusted — an empty conflict list proves nothing here.

## Worked-source mark states (issue #412)

**`ui_preview`** — nine behavioural assertions on `RungGates.next_rung_ready`, asserted DIRECTLY over
constructed sources because the predicate is pure and a PNG cannot tell an absent mark from a mark the
renderer skipped. One pair per condition, so a regression names which one broke. The ordering
assertion is the one to be careful with — see the `RungGates.gd` row in `labor-ui.md` for why it needs
a WILD sowable patch rather than the obvious tended one.

## Band fission — the arrival drawer, and the dock's prose branch (issue #511)

**THE ARRIVAL DRAWER HAS TWO ACTIONS AND ASSERTS NEITHER'S PRESENCE ANY MORE.** `expedition_panel` /
`expedition_returning` carried a PAIR of claims about a third arrival action — a settle button
offered to an arrived party and withheld from one under orders — and a band splits where it stands
now, so the affordance is GONE rather than renamed and both claims went with it. The surviving
assertion on the returning frame is that `Move` is still there, which is what tells a phase branch
that built nothing from one that built the right thing. **The two frames still render**; only the
callout's wording moved (`HudExpeditionVocab.EXPEDITION_AWAITING_CALLOUT` names the two actions the
drawer builds, and only those).

**A chapter that outlives a deleted `const` fails at LOAD, not at the call**, and that is the shape
to remember here: those assertions referenced `HudComposeVocab.PARTY_SETTLE_ACTION` after the
vocabulary block was retired, so `chapters/band_expedition.gd` — `CHAPTERS[0]` — would not compile
and the whole harness reported a bad chapter before writing a frame.

**`event_dock_band_founded`** (`chapters/event_dock.gd`, appended last) renders a SPLIT and a split
REFUSED at the ALERTS-ONLY floor, which is what proves the rung rather than showing two rows the
`notable` floor would have admitted anyway. Its two details are the pair the dock's prose branch
needs — one PROSE (the sim's refusal sentence, which no other fixture in that chapter stages, and
which carries TWO sentences because `SplitRefusals::explanation` reports every applicable refusal)
and one TOKENS (`status=split band=… parent=… x=… y=… workers=… share=… provisions=…`) — and the
token half is what proves the walk still runs for details that really are the machine contract. The
rule is in `event-dock.md` → "…but a detail the sim wrote as a SENTENCE".

**Both details are spelled out as chapter constants** rather than recomposed through
`SourceForecast`/`HudEventVocab`, the `_assert_horizon_floor_is_the_whole_trip` rule: an expectation
built from the code under test can only agree with itself. **They are the SIM's own shapes**, copied
from `server.rs handle_split_band` — a fixture in the shape of a retired handler asserts against a
payload no server can produce, which is what these two were when `handle_settle_expedition` went.

### The compose sheet's reset on close, and the unstaffed build

Two frames in `chapters/improvements.gd`, appended last. The behaviour is `labor-ui.md`'s and
`selection-card.md`'s; what belongs here is the shape of the drive.

**RETIRED — `compose_pool_take_full` / `compose_pool_take_freed`, with the shared pool they staged**
(`docs/plan_standing_upkeep.md` §2.5). They were an A/B on ONE band at `idle_workers == 0` proving
that the take and build steppers drew on ONE pool; there is one stepper on a sheet now, so the claim
has no second control to make. `Readout.build_crew_value` / `build_crew_can_add` / `build_crew_plus`
went with them, and **`Readout.stepper_count` replaced the set**: the surviving claim is the ABSENCE
of any build control, asserted by COUNT rather than by a retired meta, since a re-added row would
carry no tag and a tag search would pass vacuously.

**Its one lesson outlived it, and applies to any press-driven state here.** A press rebuilds the
controls and `queue_free`s the old row, **which stays in the tree until the frame ends** — so a settle
between presses is load-bearing (without it the second press lands on the freed row and nothing
moves), and any node COUNT taken in that window double-counts. `Readout.stepper_count` skips a control
that `is_queued_for_deletion()` for exactly that reason; it read 3 steppers on a one-stepper sheet
before it did.
- **`compose_reopen_reseeds` drives the close through the real path** (`close_compose_sheet` → the
  sheet's `closed` → `_on_compose_sheet_closed`), because the reset rides that signal; poking
  `ComposeState` would assert the harness's own write. The PAIR is the claim — the edit must be
  visible while the sheet is open, or "it shows the live crew on reopen" passes on a sheet that never
  took the edit.
- **`tile_build_unstaffed`'s map and herd claims are DRIVEN** — a badge is drawn to a canvas and no
  assertion reads a glyph back off one, and `herd_summary_lines` is pure. Each group is a pair or a
  triple, "always warn" passing any lone positive.

**One existing state was re-pointed**: `forage_crop_picker_sow` dialled its rung before the first
open, which `_show_tile`'s close now re-seeds away — the plant twin of the trap `_compose_herd`'s
docstring records. Open, dial, re-open.

**And ONE fixture had to grow hands**: `herd_kit_swap_over_geared` staged six hunters AND six keepers
on a band of ten idle, which the shared pool correctly refuses (the take clamped to four and the state
stopped being about an over-geared BUILD). Staged rather than worked around — that frame's claim is
about six armed keepers, and a band that cannot field six beside its hunters is not the band the claim
is about.

**A clean run is 332 frames / 1075 `PASS`, exit 0. RE-MEASURED, never summed** — this figure moved
three times in one arc and once across a merge, and a running total kept by addition would be wrong
by now. (The measurement above came back FIVE higher than the 895 recorded before it while the arc
#527 review added exactly ONE claim — the `Carrying:` mass one. Four `PASS`es had accumulated
un-recorded, which is the whole reason this line says re-measure. It read 989 here and MEASURED 994
immediately before the keeping-crew fix, which added three and no frame: the third under-kept claim,
plus two bare `assert`s in `chapters/herd_graze_pen.gd` converted to `_assert_hud` — a bare `assert`
prints no `PASS` and HALTS the run rather than reporting, which is why the conversion counts as two
found rather than two added.)

**THE KEEPING POOL (`docs/plan_standing_upkeep.md` §2.5) ADDED ONE FRAME AND MOVED CLAIMS RATHER THAN
GAINING THEM** — the measurement above came back two frames and one `PASS` off the recorded 320 / 997
while this arc added one frame and net-zero claims, which is this line's own instruction being earned
again. `herd_keeping_mid_build` is the new one (a herd mid-Tame; its claim has since inverted with the
`Keeping:` row's retirement — a build that IS being paid states nothing at all) and
`forage_no_food_basket`'s neighbour `forage_reopened_crews` kept its name while its subject shrank
from three crews to two — its keeping-stepper claims went with the stepper, replaced by ONE
structural claim that the sheet mounts exactly one untagged stepper. The under-kept pair's third
claim likewise moved from *"hunters do not hold a herd"* to *"a herd mid-build is not asking the pool
for keepers"*, since the first is now true by construction: no crew on a source can reach the
keeping.

**The compose sheet's own turn estimate is worth FIVE frames and TWELVE `PASS`.** Three are one
A/B plus a drag: `improvement_turns_lone_crew` / `improvement_turns_full_crew` (one patch, one floor,
**BUILD** crews 1 and 4 — `≈20 turns` against `≈5 turns`) and `improvement_turns_learning_floor` (the
same crew mid-DRAG at the Learning preset), all three in `chapters/improvements.gd`.
**WHAT THE A/B MOVES IS THE BAND'S `builders` POOL** (`docs/plan_standing_upkeep.md` §2.5) — the take
crew has not priced a build since §2.2, and the build's own stepper is retired with the per-source
crew, so the harness staffs the ROLE through `BandFx.staff_builders`. **That helper covers
`player_band()` as well as `player_bands()`**: several chapters set the single-band member alone, and
a helper walking only the list was a silent no-op on every one of them.
**The drag frame's claim INVERTED with the floor's retirement**: it used to assert a deeper floor
quoted a FASTER build (`learn_multiplier` scaled the accrual); a build crew is not pulling on the
source, so the same builders now read the SAME estimate at both floors, and the non-vacuity companion
is the take beneath it, which does still follow the dial. **A frame set
that renders one crew proves nothing here**: the defect was a sheet quoting the sim's committed-crew
answer, which renders a perfectly plausible number and simply never moves — so the A/B, not either
half, is the claim, and the negative beside it names the frozen value. The drag is driven through
`floor_changed(value, committed = false)`, the chart's live half, since only the live-refresh
registry can make the box follow a gesture that must not rebuild the sheet.

**`forage_reopened_crews` IS THE FRAME THAT PROVES ALL THREE CREWS ARE READABLE.** A band with
`idle_workers == 0` whose every hand is on ONE patch — 4 gatherers, 3 builders, 2 keepers, three
distinct counts so a seed that read the wrong field lands on a number the assertion names — reopened
on that patch's sheet. It was unreachable while `improvementWorkers` / `maintainWorkers` were
write-only: the steppers could only clamp at `idle`, so they opened at nobody with a maximum of
nobody. Six `PASS`: the regime itself (`idle == 0`, without which every claim below is about an
ordinary band), one per stepper opening on its own crew, the keeping stepper reaching PAST its seed
at zero idle, the keeping row stating `you have 2 of 3` off the wire, and the composed crop surviving
the reopen over ground with no `committed_species` at all — the case where the assignment's `species`
is the only record of what the player chose. In `chapters/forage_accounts.gd`.

**The GEAR half is a KIT SWAP, and it needs its own frames** — `herd_kit_swap_bare_build` /
`herd_kit_swap_geared_build` in `chapters/compose_rungs.gd`, one warren at one crew at one floor with
only the kit picker moving (`≈17 turns` against `≈11`). It lives in that chapter because both gear
terms ride the kit row and only that chapter stages a roster carrying the handling kit; no plant item
declares the build stat yet, so the crew A/B's frames exercise the ungeared arm alone. Its
saturation claims are DRIVEN beside the frames — a crew above the kit's own saturating crew cannot be
staffed on a frame without putting the assertion at the mercy of the stepper's cap.

**`herd_kit_swap_over_geared` is the BOUNDARY of that same form** — the same warren and kit over a
band holding a party's worth of hurdles, six armed keepers taking 51 work off a 50-unit Tame, reading
`50 work, ≈1 turn` rather than the bare `50 work` a withheld estimate leaves. It is the shipped
start-stock case (`_pen_axis_band` takes the gear's saturating crew as a parameter for it), and the
frame doubles as the only RENDERED singular clause in the corpus — no other fixture lands a one-turn
job. Three `PASS`: a precondition that the stepper really staffed the over-geared crew (a clamp below
it would leave the claims describing an ordinary build), the one-turn clause, and the negative naming
the bare price. Sabotage-verified by restoring the `BUILD_TURNS_NO_ESTIMATE` return — exactly the
one-turn claim and its negative fail, the precondition rightly staying green.

Sabotage-verified five ways, each failing a DISJOINT subset: reverting the running face to
`build_turns_remaining` fails the three crew-A/B claims; building the control outside the live
registry fails the two drag claims; resolving a FIXED kit instead of the offered one fails the geared
frame, the two-kits negative and the saturation claim; and dropping the `min` on the head count fails
the geared frame and both saturation claims while leaving the bare frame green (a kit that arms
nobody is unaffected by an uncapped head count).

**The estimate's own WORK PREDICATE is a third pair, and one of them is a NEGATIVE frame** —
`improvement_no_room_plant` (was `improvement_paused_plant`) and `improvement_stressed_advances`, one
Stressed patch with only the FLOOR moving: above its 22 / 100 stock nothing stands above the floor and
the face quotes NOTHING; beneath it the same patch reads `≈167 turns`. **The pair is the claim** — a
lone negative passes on a sheet that stopped quoting turns at all, and a lone positive on one with no
predicate. The animal half is `herd_tame_stalled`, re-fixtured onto a Stressed herd composed at
`FLOOR_MAX`, which is the reported case at its sharpest: ×2.00 is the largest multiplier on the axis,
so an omitted predicate quotes the FASTEST estimate in the game for a build going nowhere. Its
absence needle is the `≈` both count forms open with, never a specific count, which is what an
absence claim needs. Two more `PASS` ride the crew A/B for the singular/plural fork
(`DetailFormat.build_turns_clause`), driven rather than staged — no fixture lands a one-turn job.

Sabotage-verified two ways, DISJOINT: dropping the predicate from `build_turns_at` fails exactly the
two no-estimate claims (plant and animal) and nothing else; restoring the retired
`_improvement_paused_note` fails exactly the three pause-line absences (both plant frames and the
herd). **Those three absences are what the retargeting bought** — `improvement_paused_plant` asserted
the presence of that line, so the contradiction the review found was captured in this harness as a
pass.

**The work-costed build readout added SEVEN `PASS` and NO frame** (`docs/plan_unit_costed_work.md`
§11): five on the plant A/B in `chapters/improvements.gd` and two on `herd_corral` in
`chapters/herd_graze_pen.gd`. It **moved frames all over the corpus instead**, which is the shape to
expect from a readout arc — every meter row now states its job's size, so `food_tile`,
`forage_cultivate`, `herd_corral` and their siblings changed their answer without changing their
name. The claims and their three disjoint sabotages are in `selection-card.md` → "The build meter
says WORK".

**Its fixtures DERIVE `work_done` from the fraction they already state** (`BaseFx.price_plant_build`
/ `HerdFx.price_animal_build`), and every site that re-dials a meter calls one of them — a fixture
whose percentage and absolute disagree would render the exact confusion the readout exists to remove.

**THE 32 FLORA ICONS MOVED 100 FRAMES, AND THAT IS THE ARC LANDING RATHER THAN A REGRESSION.** Every
moved frame is one whose card carries a flora basket or the crop picker — plus the states rendered
after them, the tile card being long-lived in the HUD, which is why `turn_orb_*`, `terrain_legend_*`,
`narrative_fork_*` and `reserved_dock_*` are in the list. (The `terrain_legend_*` frames have since
been retired with the right dock's `L` card — the list is left as it was MEASURED rather than edited
to match a later tree, which is what makes it evidence.) **Five assertions failed on the way**, all
five for the right reason, and the re-aiming is the part worth knowing:

- **The liveness precondition fired exactly as designed.** `a real species key answers NO PATH —
  coverage is zero today` was written to fail the day art landed, and it did. It is now the POSITIVE
  half (`wild_emmer` must resolve *in the shipped directory*), with the degradation asserted beside
  it on **`hay_grass`** — the one roster member that will never have art, `icon_prompts.txt` recording
  the absence as deliberate ("32 prompts, 33 species"). A key that is merely un-drawn *yet* would put
  the claim in a race with the next batch of PNGs.
- **`_assert_food_layer_rows`' art claim is re-aimed, not relaxed.** It read `3 of 3` from
  `CropRoleSprites.SPRITE_DIR`; it now counts the tiers SEPARATELY and demands **2 species + 1 role**
  on one tile. A single "every row carries bundled art" count would pass with the whole species tier
  reverted, every row falling back to a role mark that is also bundled art.
- **The UNSTATED-role fixture had to change WHICH plant is untagged**, from `cotton` to `hay_grass`.
  The species tier outranks the role, so it also outranks the role's ABSENCE: once `cotton.png`
  existed, that row led with species art and the blank-slot path — the whole point of the fixture —
  became unreachable through it. Only a row with no species art AND no role can render the spacer.

Sabotage-verified by making `path_for` answer `""`: exactly the SIX species-tier claims fail (the
tile splits `0 species + 3 role`, i.e. the pre-art state restored) while all four role-tier claims
stay green — the demonstration that the two tiers are independently asserted rather than one claim
wearing two hats.

**THAT SABOTAGE REACHES `path_for` ALONE, AND FOR A WHILE THAT WAS THE WHOLE COVERAGE.** Every
`FloraSprites` claim in this harness went through the `RichTextLabel` host, so the OTHER accessor —
`texture_for`, whose one call site is `DrawerComposeController._build_crop_picker` — had no claim
anywhere: deleting the row's `if crop_art != null: btn.icon = …` block left the run at **exit 0 with
the full `PASS` tally** and nothing naming it. A frame diff is not the missing signal either, "A
harness renders the IMPORT CACHE" being the reason a picture cannot answer whether art resolved.
**Two `PASS` and no frame** now close it, in `chapters/forage_crop.gd` after
`forage_crop_picker_fodder` — that fixture being the one basket holding a species WITH art beside
the one that permanently has none. Asserted as a **pair** (a lone positive passes on a picker that
icons every row; a lone negative on one that resolves nothing), and reached by SPECIES KEY through
`HudWidgets.FLORA_CROP_ROW_SPECIES_META` / `ForageFx.find_crop_row_by_species`, never by face: a
row's label is a different axis from the id its art is composed from, and these fixtures pair
`wild_emmer` with "Wild Grain" on purpose. Sabotage-verified by disabling that `btn.icon` block —
exactly the POSITIVE claim fails (909 `PASS`, exit 1) and nothing else in the run does, the negative
correctly staying green.

**The flora-species precedence block (issue #339) added SEVEN `PASS` and NO frame**, in
`chapters/land_readouts.gd` beside `_assert_food_layer_rows`. It was written while `FloraSprites`
coverage was zero, when the species tier was unreachable on a shipped card and no frame could move —
the block points
`FloraSprites.sprite_dir_override` at `CropRoleSprites.SPRITE_DIR`, which does ship PNGs, drives
`DetailFormat.flora_composition_lines` through it and clears it again. **The row's ROLE is chosen so
its own art is a DIFFERENT file from the species-resolved one**, or "the species tier won" and "the
role tier coincided" would be the same green line. **The charset guard's claim is the one that had to
be re-aimed**: asked at the shipped directory it passes with `_is_valid_key` deleted (every key
answers `""` for free), so the live half is asked UNDER the override with two keys whose composed
paths really do load — `../crops/staple` and a capitalised `Staple` — while the four-shape contract
claim beside it stays deliberately vacuous. Sabotage-verified four ways, each failing a DISJOINT set:
dropping the species step from the chain fails the two row claims; ignoring the override fails those
two plus the precondition; deleting the charset guard fails the under-override claim ALONE (the
contract claim correctly stays green, which is the vacuity demonstrated rather than asserted); and
dropping `path_for`'s loaded check fails the two degradation claims **and three of
`_assert_food_layer_rows`' own** — every species then resolving to a nonexistent flora path that
displaces the role marks, which is exactly what that older group is there to catch.

### ⛔ A PRECONDITION ASSERTED OVER THE FIXTURE'S OWN CONSTANTS IS NOT A PRECONDITION

A claim whose job is *"the sim effect this whole block depends on is still applying"* has to reach the
**sim's** number. `land_readouts.gd` asserted a Field's boosted ceiling exceeded the ground under it
as `FIELD_GROUND_CAPACITY * FIELD_CAPACITY_GAIN > FIELD_GROUND_CAPACITY`, where the gain was a
harness-local `2.53` — that is `x * 2.53 > x`, arithmetic over two numbers the fixture wrote itself,
and it stays green with `labor_config.json`'s `field_capacity_gain` set to `1.0`, which is precisely
the day the block goes vacuous. The gain is now read out of `core_sim/src/data/labor_config.json`
(`forage.cultivation.field_capacity_gain`) as the sim-side twin
`climbing_to_field_does_not_compound_the_capacity_gain` already did, and **the read itself is a second
claim** — a config the harness failed to parse must fail loudly rather than fall back to a literal and
restore the tautology. The general shape is "a dead field cannot diverge": a fixture that supplies
both sides of its own comparison is measuring the harness.

### A flora fixture's `species` must be a real `flora_config.json` id

**The KEY is an asset lookup now, and a wrong one fails SILENTLY.** `FloraSprites` composes
`<species>.png` from the wire key, so a fixture keyed on a species the sim does not ship resolves no
art and renders the crop-ROLE mark — which is a legitimate state, indistinguishable from "this plant
has no art yet". The frame stays green while the thing it is evidence of has stopped happening on it,
the "a dead field cannot diverge" shape. The precedence block above does not cover this: it drives its
own composition with its own key, so it proves the mechanism and says nothing about the fixtures.

**Before `FloraSprites` the key was pure opaque payload** — nothing branched on the string, and the
one reader (`committed_species`) was supplied by the same fixture, so a fixture was self-consistent by
construction and could invent whatever it liked. That is why five invented keys accumulated unnoticed,
and **two of them were the roster's DISPLAY NAMES snake_cased** (`flax_fields` for `flax`,
`wild_grapevine` for `grapevine`) — a key derived from a display name rather than taken from the id,
which is the same defect class as `FaunaSprites`' table in issue #439. The other three were wholly
invented (`wild_grain`, `ground_nut`, `wild_wheat`).

**The LABELS are deliberately NOT aligned, and that is a separate axis.** A row reads its
`display_name`, never its key, so realigning keys alone moves **no frame** (measured: 309/309
byte-identical, 907 `PASS`) while renaming labels moves every frame carrying one. Worse, the two
`GATED_CROP_NEEDLE` claims are NEGATIVES matched on `"Wild Grain"` — a needle that no longer names
anything passes for free, so a label rename would turn both silently vacuous. So a fixture pairs a
roster ID with its own label on purpose (`wild_emmer` / "Wild Grain"), and each such site carries a
comment saying so; do not "tidy" the mismatch away.

**Two keys are deliberately left alone.** `marsh_reed` is not a rename — its nearest roster analogue
`reed_and_root` is a `staple` and that fixture needs a `fodder` row, so it is an invented plant
awaiting a decision, not a wrong id. And `_overlong_basket_tile_fixture`'s second "Ground Nut" row is
`wild_pulses` rather than the `wild_tubers` its twins took, because that basket already holds
`wild_tubers`: **one species key twice in one basket renders two names under one icon** the moment art
exists. `wild_wheat` (`map_preview`, `band_panel_preview`) and `sedge` (`snapshot_alias_guard`) are
other harnesses' and untouched.

**`forage_no_food_basket` is the newest frame** (`chapters/forage_accounts.gd`, appended last) and
carries **fourteen** `PASS`, counting the two compose-sheet fit claims every state in that chapter
takes. It is the reported tile — a wild basket of Tobacco 56% + Hay Grass 44%, which pays no calories
at all — and it stands at the junction of the two defects arc #527's axis alias left behind: the
worker cap read `max 1 worker useful here` beneath the sheet's own `13 clear it now` / `2 hold it
after`, and the PER TURN box named the fodder and never the tobacco.

**Its cap claim is a PAIR, and neither half is worth anything alone**: this patch clears
`MAX_USEFUL_BARREN` while `_dead_season_tile_fixture` — asked in the same frame's assertions — still
caps at 1, because "not barren" is trivially satisfied by a cap that stopped answering. The reach
claim is read off the RENDERED pills and finished through a real press of *clear it now*, since the
defect was the panel disagreeing with itself and the clamp lives in the press handler. The material
claims are composed at a crew deliberately BELOW the saturating one: at the clearing crew the take's
two arms are equal by construction, so a readout that never read the per-worker rate prints the same
number. Sabotage-verified — stubbing `SourceForecast.off_axis_useful_workers` to `NO_CREW_ANSWER`
fails exactly the three cap claims and leaves every material and fodder claim green, the two defects
being independent.

**`forage_cash_crop_field` is that tile ONE RUNG UP, and it carries the 2026-08-22 report** — a
completed 100%-tobacco Field with two tenders committed, which read `TENDERS 0` and `max 0 workers
useful here`. It is cut from `_no_food_basket_tile_fixture` so the only things that move are the
Field's own flags, the commitment, and the basket narrowing to the one crop a sown Field is
(`#433`). **Six `PASS` plus the chapter's two fit claims**, and each answers a different link of the
chain: the fixture really is a built Field paying no food and one material (without it every claim
below is about a wild patch); its material CEILING is composed rather than structurally empty (the
mechanism — a cap claim alone passes on a client that floors the cap and still has no ceiling); the
cap clears `MAX_USEFUL_BARREN`; and ⛔ **the STAGED count reaches the committed crew**, which is the
dangerous half and the one thing the frame cannot show, a `TENDERS 0` stepper being a perfectly
ordinary control. That claim is read off `ComposeState.forage_count()` — what the COMMIT sends —
rather than off the arithmetic above it, `clamp_forage_count` sitting between the two. Its scope is
`SourceForecast.pays_any_account`, asserted on this Field and paired against the dead-season patch,
which must still cap at one worker however many hands stand on it.

**`forage_cash_crop_gather` carries five `PASS`**: the crew composes at all,
each of the tile's two materials is quoted, **each has a ROW OF ITS OWN**, and the FOOD row still
reads. That last one is not padding — "quote the materials" is satisfied by a sheet that stopped
quoting the food, and the frame exists because a cash-crop tile's sheet quoted neither. The
two-rows claim is deliberately STRUCTURAL (`Readout.yields_account_number` per account) rather than a
needle for the sum's digits: that needle collided with the food row's own `after` reading the first
time it was written, which is a coincidence any numeric negative on this sheet is one tuning away
from.

The partly-equipped arc (issue #520) is worth
**three frames and fourteen `PASS`** of that, in three groups:

- **`band_kit_short` carries seven** — the row's fraction on the spears' own face, the popover's
  sentence on the SPEARS' line and not the SLED's, a short kit never called bare hands, the UNSTAFFED
  job's two (it states no shortfall and keeps the sound ▲, asserted on the same frame as a live
  shortfall because the two zeros are one glance apart), and the driven faction-rollup pair.
- **`band_kit_forage_short` carries three** — the FOUR-JOB denominator (`workersOnQuotedJob`): two
  baskets among four gatherers, asserted beside the claim that the perfectly-equipped SPEARS on the
  same band say nothing, which is what pins that the two rows were divided by different numbers.
- **`herd_hunt_gate_split` carries four**, counting the two negatives that each cover a different way
  the split line can be wrong: the UNIFORM control appended to `herd_hunt_gate_effort` (without which
  the claim passes on a sheet that annotates every band) and the PNG-less re-compose at a party that
  fits inside the armed run.

**THREE existing frames moved, and none is a regression**: `band_kit` / `band_kit_expanded` /
`band_kit_bare` now sit over a STAFFED forage job, so their dry baskets read `(0/4)` and the popover
says *"— bare hands · none of your 4 workers carry one"*. Nothing else in either harness moved —
**that arc left `band_panel_preview` untouched**, which is the check that the shared
`fixtures_band.gd` additions are inert there (its own kit fixtures publish neither worker field,
which is the whole-row-absent case). *Arc #527 has since moved that harness for its own reasons; its
current tally lives in `harness-band-panel.md`, stated once.*

**Arc #527 (the `trade_goods` retirement) changed no frame COUNT at all, and that is the thing to
know about it**: three claims went with the account; the follow-up that gave a herd
`material_per_biomass` / `per_worker_material` added four to `herd_hunt_pelts_only`; and the
expedition half added three to `herd_hunt_pelts_raid`. **Every one moved a frame IN PLACE** — the
crop-picker frames swapped a trade scalar for per-material clauses, the wolf's compose sheet went
from quoting nothing to `0.11 HIDE`, and its raid went from reading as a denial mission to
`≈5 GREY WOLF` over `2.75 HIDE`. A frame that changes its answer without changing its name is what
makes that arc's history readable in one `git log -p`.

**Twelve** of those frames are the Materials &
Crafting chapter's: the ledger's own frame, its reserved-edge / event-bar / band-dock / co-edge /
collapsed variants, the two-tier and folded group heads, and the two stopped benches — the crew that
walked off and the store that cannot cover the next draw. **The figure is
MEASURED, never summed** — the count moved with the band-fission merge as well as with this arc, and
two arcs' deltas added by hand is how a tally stops matching its harness.

**The running totals are recorded as deltas, not as a chain**, because band fission both retired the
settle chapter's `PASS`es and added its own, so the old absolutes no longer add up to the current
one: the map-gesture state below is worth **nineteen** `PASS` and **no frame**; the crafting chapter
twelve frames and one hundred and four `PASS`es (counted on the `: PASS` delimiter, one of its own
assertions carrying the bare word in its prose); the band-compose arc three frames and twelve `PASS`es (the
drawer pair's three, the dock frame's three, and the two-band compose pair below).

**The crafting STOCK-ROW readout is worth six `PASS`es and NO frame** — measured, `1575 → 1581`, exit
0 both, **370 frames unchanged**. Two of the six ride the existing `crafting_panel` save and four are
PNG-less. **It moves about ten frames without adding one**, which is the accounting that matters here:
giving the fixture band a `cordage` material and two batches of it puts a fourth group in the rail of
every state built on `_crafting_band()` / `_two_tier_band()` — `crafting_panel`,
`crafting_panel_reserved_edges`, `crafting_panel_event_bar`, the three band-dock states, the
re-render state, both two-tier states and `crafting_bench_priority`.

**And the `1575` is a RUN, not a subtraction.** It was first reported as `1581 − 6`, which is exactly
the "never sum a delta onto a recorded figure" trap below wearing the other face — the arithmetic
happened to come out right, and it was still not evidence. The baseline was re-taken by restoring the
three changed client files to the PR's merge-base in place and running the harness.

**The band FODDER LEDGER arc is worth four frames and six `PASS`es** — measured, `1381 → 1387` and
`490 → 494` on one windowed run. Five of the six are `band_expedition`'s
(`band_hay_short` / `band_hay_covered` / `band_hay_empty_store` / `band_hay_and_pen`, whose block
makes every claim over FOUR line-sets at once) and one is `herd_graze_pen`'s (the pen's hay need,
appended to the existing self-feeding/foddered pair's block). `EXPECTED_CHECKPOINTS` moved 70 → 79
and 71 → 72 respectively.

**The PEN's `Fed:` ROW is worth one frame and five `PASS`** — measured, `1387 -> 1392` on one windowed
run. The frame is `herd_pen_no_fodder`, appended LAST in `chapters/herd_graze_pen.gd` so no frame
before it moves; the `PASS`es are that chapter's pen block growing from four assertions to nine, over
FOUR line-sets in ONE block. The four states differ only in which optional terms are present -
`all pasture`, a fodder share, `no fodder`, a shortfall clause with and without its `more` - so each
is the others' control, and checking one in a frame of its own is exactly the gap this arc has lost
three defects in. Two of the nine make claims no needle on a single state can: the fodder share pinned
as `fed - pasture` on a fixture where a division answers 12% to the subtraction's 7%, and the CORRAL
row pinned as the plain badge on both starving pens, which is the regression risk of collapsing
`corral_built_label` down to it. `_lines_any_contain_ci` is the chapter's case-insensitive twin, for a
RETIRED word that must not return in ANY spelling. The chapter's `EXPECTED_CHECKPOINTS` moved
72 -> 78.

**Its band frames are three states of ONE row and are judged together, not in three places.** Every
claim in the block is a CONTRAST — draining against covered, rendered against absent, in the
pull-down against on the row — and a contrast checked one half at a time is not checked: the runway
alone passes on a row that always reads a number, the gate alone on a row that renders for every band
in the game. So the fourth line-set is a plain forager band that must render NO Fodder row, and the
assertions compare the four against each other rather than each against itself.

**RESHAPING THAT ROW INTO THE FOOD ROW'S SHAPE cost one frame and six `PASS`** — measured,
`1392 -> 1398`, `EXPECTED_CHECKPOINTS` 79 -> 86. The frame is `band_hay_breakdown`, the Fodder
pull-down OPEN, appended after the block's dock frame is released so nothing before it moves; the
block itself went from five assertions to nine and gained two outside it. **The open pull-down is a
new STATE, so the four states stayed in the one block rather than splitting into a second family** —
what an inline append produces is a perfectly plausible popover beside a row that ALSO carries the
rows, which only a claim holding both halves at once can catch. So the block asserts the flows are in
`fodder_breakdown_lines` AND that none of them is in the produced lines, and the frame then asks the
opened POPOVER what it actually holds (a disclosure registered with an empty payload draws a card
with nothing in it and looks fine in a thumbnail).

**The WRAP is the thing that reshape removed, and it is MEASURED, not looked at**
(`_assert_fodder_row_fits`): two lines of a rendered vitals block look exactly like two rows, and no
`contains` can see the difference. The row's natural unwrapped run is measured in the drawer label's
OWN font at its OWN size plus the `[table=2]` gutter — **216px in a 354px column** — with the run cut
out of the parsed text by the row that FOLLOWS it, since `[table]` rows carry no line break into
`get_parsed_text()`. It is asserted on `band_hay_short`, the state the long row wrapped in.

**A pre-existing engine quirk visible in these frames:** the SECOND `[url]` row of a vitals table
draws no meta underline. It was Morale's row before the Fodder row landed between them and it is the
Fodder row's now; the BBCode is byte-identical for all of them (`_key_cell` builds every one), so it
is Godot's table-cell underline pass and not a client difference. Do not "fix" it in the formatter.

**MAKING THAT ROW UNCONDITIONAL cost one frame and fourteen `PASS`** — measured, `1504 -> 1518` on
one windowed run, `EXPECTED_CHECKPOINTS` 86 -> 101. The frame is **`band_fodder_dormant`**, appended
after the pull-down state so nothing before it moves, and it is the first in this chapter to hold a
LIVE fodder row and a DORMANT one in ONE render — the dim treatment is a claim about a DIFFERENCE,
and a difference photographed one half at a time is not photographed.

**The setup ORDER is what makes two band-detail surfaces possible at all.** A selected player band is
the DOCK's subject wherever a dock exists and the Occupants drawer then renders a one-line pointer at
it, so a docked HUD has exactly ONE. The state selects the forager band with **no panel injected**
(the drawer's own fallback path, which every hay state above renders on) and injects the dock
afterwards; neither `set_band_city_panel` nor `render_band` re-renders the drawer. That precondition
is itself asserted — a drawer that had flipped to the pointer would carry one fodder row instead of
two and photograph perfectly tidily.

**The block also moves the faction's Foddering and must put it back.** `_ingest_intensification`
REPLACES a faction's whole row, and `band_expedition` is the FIRST chapter — every chapter after it
inherits whatever this one leaves. The two dormant sentences need the track part-learned and then
learned, so the block pushes both and restores the untouched zeros on the way out, the same restore
`band_panel_preview` makes around its own five-track fixture.

**A `[url=` SEARCH OVER PRODUCED LINES IS A VACUOUS CARET TEST.** A line producer emits plain
`Key: value` strings and `detail_bbcode` is what draws the clickable run, so the needle can never
fire — measured: a dormant branch wrongly registering a full disclosure left that assertion green,
and only the rendered-surface half caught it. The honest question is `DisclosureController.state()`,
read BETWEEN the two productions (`unit_summary_lines` clears its rows on entry), with the LIVE
band's registration beside it so "registers nothing" cannot pass on a build that registers nothing
anywhere.

**Falsified four ways from this chapter**, each failing a disjoint set: dropping the dim treatment
(2), dropping the hover (5), re-gating the row (9 — including the frame's own both-surfaces
precondition), and registering a disclosure on the dormant row (2).

**`band_hay_and_pen` is the frame that carries a BAND and a PEN at once**, which the drawer cannot do
on its own: a player band's detail moves into the Band/City dock when one is present, so the dock
states the band's `Fodder` ledger while the tile drawer states the starving pen's own
`Fed: ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn` share of it — **one word across both**,
the pen row saying `fodder` as the band's store row always has. The dock is `SIDE_RIGHT` on purpose — a vertical dock is the TALL
tier, which keeps the Fodder row STANDALONE rather than merging it onto Food, the SHORT tier's merge
being `band_panel_preview`'s to hold. The chapter releases the panel and hands the reference band
back afterwards, the raid block's own restore idiom, so a stranded reserved edge cannot move frames
in later chapters.

**The bench's rate / finish / tint / clear block is worth sixteen of that hundred and four and one
frame** — measured on the `: PASS` delimiter over the contiguous run from the crew-of-zero frame's
first claim to the ✕-verb pair, the running bench's own five claims sitting up in state 1's block
instead. The frame is the pair's second half: a bench short of material publishes a REAL rate and is
stopped
anyway, which is the only shape that can say whether the estimate is withheld by the rate or by the
refusal. **It is also the pair the TINT is judged on** — the crewless bench's reason reads in the
quiet ink and the short bench's in `DANGER`, because whether a refusal is a fault or a prompt is the
sim's `blockedSeverity` and not the panel's reading of the wording; a third, PNG-less fixture stamps
`neutral` on a shortfall SENTENCE, which no sim resolves and which only a panel re-deriving the tint
from the words gets wrong. Restoring the always-`DANGER` tint fails the crewless claim and the
mismatch claim and leaves the shortage claim green. Sabotage-verified six further ways, each failing
a DISJOINT subset — dropping the `blockedReason`
half of the gate fails the short bench's two line claims ALONE, the crew-of-zero frame staying green
because its rate is zero; re-deriving the rate off the crew fails the five rate/estimate claims and
nothing else; a floor in place of the ceiling fails only the non-dividing remainder; drawing the ✕
unconditionally fails only the idle bench's absence claim; wiring the ✕ to `make_requested` fails
both halves of the verb pair (`[]` against `[{…recipe_id: "baskets"}]`); and composing the tooltip
out of the recipe's inputs — the forbidden implementation — fails both tooltip halves at *"20 fibre
already cut"* against the 5 fibre · 1 hide the store really lost.

### A scroll over the card must not also drive the map

The last block of `chapters/crafting_bench.gd`, PNG-less. It guards `MapView`'s pointer-navigation
routing (`map-renderers.md` → "A POINTER-DRIVEN NAVIGATION INPUT IS DECLINED WHERE A CONTROL CLAIMS
THE PIXEL"), on the crafting card because that is the surface it was reported on — but the rule is
every floating and docked card's, and the state stands a real `MapView` up rather than owning one.

**Every claim is a PAIRING, because a one-sided one passes on a map that has stopped answering
gestures at all**: over the card the map must hold still, over open map the same event must still
move it. It runs all four probes for each of the three event kinds (pan gesture, pinch, wheel):

| probe | what only IT can say |
|---|---|
| the card's own CHROME (top-left corner, inside the card and outside the scroll) | the map's guard, with no `ScrollContainer` in the picture |
| the LEDGER parked at its top | the event reached the RIGHT surface — the scroll offset moved (and, for a pinch, rightly did not) |
| the LEDGER parked at its FLOOR | the reported case: a container with nothing left to give stops accepting a pan gesture |
| open map | the vacuity guard for all three above |

**THE LEDGER-AT-TOP PROBE IS THE WEAK ONE AND IT IS NOT THERE FOR THE MAP CLAIM.** A
`ScrollContainer` with room left accepts a scroll and a wheel itself, so over a scrolling ledger the
map holds still whatever `MapView` does — measured: the first cut of this state probed only the
ledger centre, and with the guard removed **only the PINCH failed**, no scroll container taking one.
That is the whole reason the chrome and floor probes exist.

**The open-map point is SEARCHED, and searched with a LEFT press.** A press is the one pointer input
the GUI pass really does stop, so "a press here reached `_unhandled_input`" is a reading of "this
pixel is open map" that is independent of the hover the fix under test is built on. A lattice walk
rather than a literal point, this frame carrying a left dock, a bottom bar and a centred card; the
state PRINTS the point it found and fails loudly when the frame offers none.

The `MapView` is `visible = false`, data only, and freed again — the `band_panel_preview` idiom, and
for its reason: a surviving instance paints a stray minimap thumbnail into every later frame, that
being its own `CanvasLayer` which `visible = false` does not hide. It is never handed a HUD
reference, so the minimap is never built at all.

**Sabotage-verified: with the guard removed the run fails exactly SEVEN** — the three chrome claims,
the three floor claims and the pinch's ledger claim — while all three open-map claims, both
ledger-took-it claims and the pinch's ledger-ignored-it claim stay green. **Zero frames moved**, in
this harness (295/295 byte-identical against a run with the block disabled) or in
`band_panel_preview` (93/93 against a run on the pre-change `MapView`, which also read the identical
235 `assert OK` / 342 `: PASS`).

### The compose sheet composes for the PANEL band, not the first one

**`compose_panel_band_hunt` / `compose_panel_band_forage`** (`chapters/hunt.gd`, appended last) guard
`Hud._resolve_assign_band` once founding makes a second band reachable. **BOTH sheets, because they
are two separate injections of that resolver** — one passing says nothing about the other, and the
playtest report named both.

**The ROSTER is the assertion.** Every other compose fixture in this harness is single-band, which is
exactly why nothing here caught the defect: with one band all three rungs of the resolver agree, so a
state that does not stage a SECOND band as the panel's subject passes for free. The pair stages the
panel band as the roster's second entry and gives the three candidate answers three deliberately
unlike idle counts — a parent with NONE (its crew left with the expedition), the colony's live 2, and
a stale render-time panel copy at 9 — so each wrong rung fails as its own number rather than hiding
inside another's.

Three claims per sheet, because they fail apart: the picker's rendered FACE (what the player read),
the composed band ENTITY (what a commit would name), and the crew stepper's CAP (what stopped the
player staffing the hunt). Sabotage-verified by restoring the bare `player_band()` fallback — exactly
those six fail, at `Band 1` / entity 841 / a stepper capped at 0, and nothing else in the run does.

**A leaked panel band is a real hazard in this harness**, and the shape to know: `tile_panel_band`
sets one through the real `render_band` path and never clears it, so every chapter after it runs with
a panel band the resolver now prefers. A resolver variant that returned the STORED panel dict when the
entity is absent from the roster failed four unrelated assertions in `tile_panel` and `compose_rungs`
for exactly that reason. The shipped one falls through to `player_band()` instead — which is also the
correct live behaviour, an unresolvable panel entity being a band that has left the world.

### …and on a WORKED source it opens on the working band, and a switch re-seeds

**`compose_working_band_forage` / `compose_band_switch_forage` and their hunt twins**
(`chapters/hunt.gd`, appended after the pair above) — four frames and forty `PASS`. The behaviour is
`labor-ui.md`'s; what belongs here is the shape of the fixture and of the drive.

**The roster is the assertion, again, and one band more than the pair above needs.** Three bands: the
ladder's own answer works NEITHER source, and the other two work BOTH. With the ladder's band among
the workers the tie goes to the ladder and the state passes with the rung removed; with only two bands
there is nowhere for a band SWITCH to go that is also a working band.

**The two standing crews are 2 and 3, and neither may be `HudConst.WORKER_STEP`** — a stepper reading
1 cannot tell a re-seed from the no-standing-assignment fallback. Both sit under either source's
max-useful ceiling, so what is rendered is the seed rather than a clamp of it.

**Four claims per state, because they fail apart**: the picker's FACE (what the player reads), the
STEPPER (what a commit would send), the commit VERB, and the improvement control's PRESENCE. A
vacuity guard rides between the two switches — the crew dialed to 0 on a band that really does work
the source, asserting the sheet really does say `Unassign` and really does drop the control, without
which "not `Unassign`" passes on a sheet that can no longer say it.

**The `Band:` picker is driven with REAL POINTER INPUT** (`_pick_actor_band`: the face, then the popup
row, through `InputProbe`), the three gotchas `chapters/trade.gd`'s destination pick records —
`canvas_to_window` for the embedded subwindow, the popup's own `index_pressed` as the witness for a
derived point, and an `is_instance_valid` guard on the teardown, the pick having freed the popup by
rebuilding the sheet.

Sabotage-verified on two DISJOINT mutations: restoring the bare `set_*_band` write fails **eight** —
the re-seed claims on both webs, reading `got Unassign` and a missing improvement control, i.e. the
played defect — while the default claims stay green; returning `_resolve_assign_band()` unchanged from
`_band_working_source` fails exactly **four**, the default claims, at `Band 2, got Band 1`.

**Thirty-six existing frames moved with the standing-crew line's removal**, every one a compose sheet
on a source with a standing crew, plus `herd_band_picker_b`, whose switched band now re-seeds to the
`WORKER_STEP` floor instead of being hand-clamped by the chapter.

## The UNBOUNDED-RAID floor: one frame, three equalities, and a driven denial pair

`ui_preview` **`herd_hunt_horizon_travel`** (`chapters/hunt.gd`, appended last in the hunt chapter) is
the only state that can tell the fix from the bug. Its pairing half `herd_hunt_forecast_horizon`
structurally cannot: that band carries no `band_move_tiles_per_turn`, so its trip is all hunting and
`horizon` and `horizon + travel` are the SAME number — a client quoting the bare horizon renders it
identically. The travel frame raids the same never-completing Steppe Bison from the 8-tiles-out band,
so the two answers differ by the whole walk.

`_assert_horizon_floor_is_the_whole_trip` asserts all three surfaces by **EQUALITY** — the trip
verdict, the Send button's face and the one-line form's head through the travel split — against
sentences spelled out in the chapter rather than re-composed through `SourceForecast`'s own formats,
since a copy claim that borrows the copy under test can only agree with itself. A `contains` would not
do either: the two candidate lines share every word and differ only in a number. Each message names
BOTH the wanted and the found string, so a failure reads as `68` against `60` rather than as a bare
mismatch. Sabotage-verified by returning the bare horizon as `turns_floor` — exactly those three fail
and nothing else in the run does.

**Every band fixture in both preview harnesses now states `expedition_forecast_horizon_turns`**, from
the one named `BandFx.FORECAST_HORIZON_TURNS`; a fixture without it takes the `*_NO_HORIZON_*` fallback
and renders the old hedge, which would pass a weakly-worded assertion. `marker_field_guard` carries the
field too, the in-flight denial readout reading it off the MARKER.

The DENIAL half is PNG-less and driven (`chapters/band_expedition.gd`): the `horizon` verdict's two
spans by equality — *"…after 60 turns of raiding"* with no band, *"…after 67 turns from launch"* with an
outbound leg — plus the no-lever fallback to the bare hedge. **The pair is the claim**: a builder that
ignored `travel` satisfies the first alone and one that always shifted satisfies the second alone. It
is driven rather than rendered because no denial fixture in either harness stages a `horizon` row, and
a sentence is a string — a frame shows a plausible verdict whichever clock it quotes.

## `chapters/trade.gd` — the cargo picker and a shipment in flight (arc #527, issue #517)

**Appended LAST in `CHAPTERS`**, after `crafting_bench`. Seven frames and thirty `PASS` — plus
one more in `chapters/event_dock.gd`, where the shipment's `destination=` label swap belongs
beside the band-label trio it extends rather than in a chapter that instantiates no dock. It
injects a real `BandCityPanel` docked RIGHT on the PARTIES tab, drives the whole compose act through
the panel's own controls, and releases the panel and hands the reference band back before it ends —
so a chapter appended after it starts where every other one does.

**Every control is driven, not set.** The footer's mission button is pressed (by
`HudWidgets.MISSION_LAUNCH_META`, never by face), the destination is chosen with REAL POINTER INPUT
(below), the party is raised through its stepper's `+` reading `PARTY_STEPPER_COUNT_META`
back on each press, and each cargo row is loaded by repeated presses of its OWN `+` — which is what
exercises the clamp-to-the-pile and the per-press rebuild rather than the members behind them.

**THE DESTINATION PICK IS TWO REAL PRESSES, AND IT USED TO BE A FAKED SIGNAL THAT COULD NOT FAIL.**
`picker.emit_signal("item_selected", 0)` calls the connected lambda by hand, so every step between a
click and `on_pick` — the popup opening, the entry being reachable, and the engine deciding whether a
pick is a CHANGE at all — went untested, and the chapter stayed green through a picker that was dead
in play (`labor-ui.md` → "A PICKER STATES ITS OWN SELECTION"). It now presses the picker's face
(`InputProbe.press_left` / `release_left`: an `OptionButton` runs at `ACTION_MODE_BUTTON_PRESS`, so the
popup is up before the release exists and the two halves must be driven apart) and then presses the
entry, both through `Viewport.push_input`. Four claims ride it, and the sabotage that reverts the fix
fails three of them plus the whole downstream chain — **twelve in all, and NOT the fourth**: the press
really does land on entry 0 either way, and what the bug swallows is the pick, which is exactly the
decomposition those two claims are separated to show.

**Three things about driving a popup that are not obvious, all measured here:**
- **The popup is an EMBEDDED subwindow, and `push_input` un-stretches an event into canvas space
  before forwarding to one** — so the press goes through `InputProbe.canvas_to_window` like every other
  probe. A raw canvas point misses it entirely.
- **Hover feedback is not available**: `PopupMenu.get_focused_item()` answers `-1` for every pushed
  motion (the accessor works — `set_focused_item` round-trips), so the `_find_open_map_point` style of
  hover-search cannot find a row here. The entry's point is derived from the popup's own rect and item
  count, and `index_pressed` is LISTENED to so the derivation is CHECKED rather than trusted.
- **The popup is FREED under the probe** — the pick runs `on_pick` → `rerender()` → `queue_free` on the
  row the picker hangs off — so its teardown is `is_instance_valid`-guarded and the answer is read off a
  MEMBER. An unguarded `disconnect` raises, which aborts the call, and an aborted GDScript call answers
  with its return type's default: `0` is a legal entry index, so the "landed on entry 0" claim passed
  for a helper that never finished. That is `_instantiate_chapters`' own lesson met a second time.
- **A lambda captures a local by VALUE**, so a witness assigning to a `var` outside it reports nothing
  ever happened. It cost a run.

**The claims that only a driven run can make:**

| claim | why nothing else says it |
|---|---|
| the picker lists BOTH ties, the parked one disabled with its reason in its own label | a picker that filtered parked ties renders a shorter list that looks perfectly correct |
| the destination's position is worded as REMEMBERED, and the walk wears `≈` | the arc's keystone; a live-position render is indistinguishable in a screenshot |
| a material row names the pile's RATING | the fixture holds TWO `hide` piles at different ratings, which is the only shape that can fail |
| mass and cap composed from the FIXTURE's side | the harness and the sheet arrive at one number from opposite ends |
| an over-cap manifest disables the send | the client's courtesy, reached by shrinking the party rather than by growing the cargo, so the cap moves and the manifest does not |
| the shipment's materials are **not** summed | asserted as an ABSENCE — a row that added hide to bone still renders two plausible numbers and every other assertion passes |
| the in-flight `Carrying:` row weighs the WHOLE PACK against the cap | composed from the fixture's own terms (`12 food + 2.0 × (4.0 + 1.2)` = 22.4 of 40), so the row and the compose sheet's meter arrive at one number from opposite ends. It read `12.0 / 40.0` — the cargo's FOOD over the MASS cap — and every other claim on that row passed |
| the destination `BandId` never appears on screen | the id is distinctive (`BandFx.FIXTURE_BAND_ID_OFFSET + entity`), so a leak has something to find |
| the `Bound for` row names the band anyway | the fixture publishes `expeditionDestinationName` as `""` — the LIVE shape, bands having no names — so the row can only read `Band 2` by joining the roster on the id beside it |

**`trade_footer` exists for the GLYPH, and that is not decoration.** A mark missing from this
client's fallback font renders as an INVISIBLE GAP — no tofu box, nothing an assertion can see — and
that is exactly what 🤝 did on the Food breakdown's transfer rows before it was replaced. The frame
is the only thing that catches it, and it caught the fifth footer button being clipped off the edge
of a 354px column in the same pass.

**The party fixture carries `expedition_trade_material_carry_weight`**, which the native decoder
echoes onto every cohort. Without it the `Carrying:` row prices the pack at its food and the mass
claim above goes green on the defect it exists for.

**The Food-ledger half opens the disclosure, and it is now the ONLY thing that can see the two
terms**: the headline states the steady rate and deliberately says nothing about a transfer
(`band-readouts.md` → "The Food line's TRANSFERS are breakdown rows"). **Its label search starts at the HARNESS ROOT, not the
HUD**: a player band's detail renders into the Band/City panel, which is a sibling `CanvasLayer`
rather than a child of the HUD, so a HUD-rooted walk finds nothing and the click silently never
happens.

**A PNG-LESS THIRD OPENING RIDES AFTER IT — the COMMAND-REFRESHED frame** (issue #517): the same
band with `transfer_received` / `transfer_sent` zeroed and the per-turn pair intact, i.e. the frame
the sim rebuilds from live components after any dispatched command, asserting both `⇄` rows are still
itemized. **No picture can make that claim** — the two readings differ only in which field the rows
came from, and the wrong one renders no rows rather than wrong ones. It is judged as a PAIR with the
turn frame above, and the accumulator is ZEROED rather than left behind, because a client rendering
whichever term is non-zero would otherwise pass both. Sabotage-verified: pointing the rows back at
the accumulating pair fails exactly these two and nothing else in the run.


## The one-row rung readout, and the never-finishes repro (issue #545)

Two frames and a re-pointing pass across `chapters/improvements.gd`,
`chapters/herd_graze_pen.gd` and `chapters/compose_rungs.gd`. The behaviour is `selection-card.md`'s
and `labor-ui.md`'s; what belongs here is the shape of the drive and the fixture arithmetic it moved.

- **`improvement_never_finishes_unstarted` was the reported repro, and §4.6a INVERTED it** — it is
  `improvement_unstarted_standing_price` now. The frame is unchanged (a wild patch, Cultivate
  declared, one builder) and every claim on it flipped: the rate is not a build term any more, so the
  honest answer is the `≈50 turns` the frame used to name as the defect, and the `∞` it used to assert
  is what must now appear nowhere. See "The rate is not a tax on building" below.
- **`tile_two_meters_live` is the both-rows frame**, and its third claim is the SILENCE — a patch whose
  keeping is paid carries no mark on either row. Its Field meter and turn count are deliberately
  unlike every other build reading in the chapter, so a card rendering one rung's numbers on both rows
  cannot pass.
- **The five hazard states are asked of the PRODUCER as one conjunction** (`_hazard_states_all_marked`),
  because two of them render in states no frame stages and the claim is about the SET.
- **`_meter_value_markup` became `_rung_value_markup`**, and the needle is now the whole rendered value
  rather than a verb: the card states `≈11 turns (96%)` where it stated `Preparing 48 / 50 work (96%)`,
  so a needle built from a build verb would be asserting a readout that no longer exists.

**THE FIXTURES GREW THE RUNGS' OWN RATES, AND TWO STATES HAD TO BE RE-STAFFED FOR IT.**
`BaseFx.price_plant_build` now sets `patch_cultivation_upkeep_demand` / `patch_field_upkeep_demand`
and `HerdFx.price_animal_build` takes the animal rate as a PARAMETER — the animal rungs both declare
`1.0 × source_load`, so a warren's rate is not the reference herd's. **While** the rate was a real
term of the pace — it was briefly `crew − rate`, and is not now —
`improvement_stressed_advances` staffed THREE builders where it staffed one and the kit-swap counts
moved 17/11/9/4 → 25/17/11/6.

**§4.6a UNDID BOTH OF THOSE, WHICH IS WHY THIS BLOCK IS WORTH READING TWICE.** The rate stopped being
a build term, so the fixtures' rates stopped pacing anything and both re-staffings reverted: the
stressed frame is back to ONE builder at `≈50 turns`, and the kit-swap counts back to 17/11/9/4. The
rates themselves stayed — they are the offered face's STANDING PRICE now, quoted rather than
subtracted — so `price_plant_build` / `price_animal_build` still set them, and the kit-swap clause is
asserted WITH the standing half (`_kit_swap_held_price`).

**A clean run is 331 frames / 1051 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says. The
recorded figure before the DECLARED improvement state was **328 / 1040**, and the frame count moved by
three against ONE frame added — which is this line's own instruction being earned again: two frames
had accumulated un-recorded, exactly as four `PASS`es had the time before.

## The improvement control's DECLARED state, and the build line's three inks

One frame (`compose_offer_no_hands`, appended last in `chapters/improvements.gd`) and eleven claims.
The behaviour is `labor-ui.md`'s — "A DECLARATION IS NOT A BUILD" and "THE BUILD LINE'S STATE IS ITS
COLOUR"; what belongs here is the shape of the drive and the two re-pointings it forced.

- **`tile_build_unstaffed` gained the three claims that say the door opens both ways** — the control
  is a `CheckBox` in `IMPROVEMENT_STATE_DECLARED`, it is TICKED, and it is LIVE *on a band whose
  `effective_idle` is 0*. Three claims rather than one because they are one claim each about the three
  ways it used to fail: the wrong node type (a `Label` has no toggle), an unticked box (which would
  read as no declaration at all), and a disabled one (which cannot be undone).
- **`compose_offer_no_hands` KEPT ITS NAME AND ITS FIXTURE AND ASSERTS THE INVERSE** (§2.5). It staged
  a band whose every hand is on tile A and opened a sheet over an UNWORKED tile B, so the build pool
  was empty and the box greyed out with its reason. **Declaring costs no hands now** — ticking appends
  a queue entry — so the state it stages has no refusal left in it, and what it asserts is that the box
  is OFFERED, LIVE, and mounts no builders control of any kind. The precondition
  (`effective_idle == 0`) is what keeps that a claim rather than a tautology: a fixture that had
  stopped being hand-starved would pass it for the wrong reason.
- **The three INKS are asserted as a set across three frames**, read as the RESOLVED font colour
  through `ForageFx.improvement_face_color` — a `Color` reader, because the pace has three states and
  the two `∞` ones read alike through the warned/not-warned bool it replaced (which survives, written
  in terms of it). `improvement_turns_lone_crew` is red-losing and `improvement_turns_full_crew`
  green-growing, asserted as a PAIR on one frame's claim so a face pinned to either ink fails the
  other; `improvement_rung_slipped` is the other red one, reached through the SLIDING state rather
  than through an arithmetic sign.
- **The retired threshold is now asserted as an outright ABSENCE, on both sides of the A/B**
  (§4.6a). It was a RE-HOMING for one slice — the rate reachable as the BUILDERS label's tooltip — and
  the mechanism under it is gone: the keeping pool owes the rate at every fullness, so no rung declares
  a build-crew bar. `ForageFx.build_work_floor` / `build_work_floor_tooltip` and
  `BUILD_WORK_FLOOR_ABSENT` are deleted with it, since a scanner for a meta nothing stamps answers
  `absent` on every sheet in the game — an assertion that cannot fail. It was paired with *the row
  still mounts* (`ForageFx.build_crew_row`), or "states no threshold" passed on a sheet with no
  BUILDERS row at all. **§2.5 retired the row itself**, so that pairing is gone with it and the
  surviving structural claim is the opposite one — `Readout.stepper_count` pinning the sheet at ONE
  stepper, which is what catches a build control re-added under any meta, any label or none.
- **TWO EXISTING CLAIMS WERE RE-POINTED, and both were asserting the bug.**
  `improvement_turns_*`'s *"amber while the crew is under it, and quiet once it is cleared"* was about
  the deleted note. And `herd_compose_reopen_fresh`'s precondition asserted that a WILD herd with Tame
  declared and nobody on it quotes a `Taming — 0%` METER — which is the one-way door in miniature. Its
  claim was never the meter: it is the stale-vs-fresh dict, and the baseline only has to establish
  that the sheet is not ALREADY quoting the taming herd's 4%. It asserts the DECLARED state and the
  absence of that meter instead.

## The rotting build sentinel, and the hand-off window (PR #557 review)

One frame and eight `PASS` across two chapters, for the two client-side defects the sim's
`BUILD_METER_ROTS` split and the crew-hand-off producer left behind. The behaviour is `labor-ui.md`'s
and `turn-orb.md`'s; what belongs here is the shape of the drive and the re-aiming it forced.

- **`tile_meter_rotting` is the fourth answer on the tile card**, appended straight after
  `tile_meter_never`, and it is judged **against** it rather than alone: the two are one step apart on
  the same patch at the same meter, and the whole claim is that they read DIFFERENTLY — same `∞`, same
  hazard mark, different words and different INK. Two claims, and the second is the one that would
  have caught the bug: the positive alone passes on a card rendering BOTH rows, so the holding row's
  exact value must be ABSENT from this frame.
- **The claim is word-AND-tint markup, and the tint is the half that was missing.** Both states lead
  with `RUNG_HAZARD_GLYPH`, so a claim about the mark passes on a rotting build painted amber.
- **`ForageFx.improvement_face_is_warned` became `improvement_face_stops`** and now answers for BOTH
  stopping inks. Hard-wired to `WARN` it FAILED on the redder, worse half of the claim it exists to
  make — which is exactly what happened when the sentinel was honoured, because the lone-builder A/B
  frames quote a NEGATIVE net (one hand against `plant:tended`'s 2 work) and had only ever read amber
  because the client flattened the two answers. Where a frame means one specific ink it compares
  `improvement_face_color` directly.
- **TWO EXISTING CLAIMS WERE ASSERTING THE FLATTENING.** `improvement_turns_lone_crew`'s pace claim
  read *"a HOLDING build line is amber"* for a crew the chapter's own constant doc already described
  as a negative net, and the unstarted repro's ink claim was the same one rung over (that frame has
  since inverted outright — see below).
  Both name the LOSING red now, and the holding ink's own frame is `tile_meter_never`, on the card,
  where a crew EXACTLY at the rate is staged.
- **`UNKNOWN_BUILD_TURNS_SENTINEL` moved `-3` → `-4`.** It stood for *whatever the wire grows next*
  and the wire grew it, so the harness was holding the client's failure to follow in place, green. **A
  sentinel-is-unknown claim has to be re-aimed the day the schema spells that value**; it is one past
  the last one defined, and it moves again the next time the schema grows.

**The hand-off block is PNG-LESS and DRIVEN** (`chapters/turn_orb.gd`, appended last), because both
failures render a perfectly ordinary popover — correctly shaped, worded and inked rows, just too many
of them, and only counting says so. The producer is asked directly
(`AttentionController.build_band_attention([], [])`, counting `crew_handoff` rows) over an events
array in each of the two shapes the wire really delivers:

| fixture | the failure only IT reaches |
|---|---|
| the full snapshot's whole retained RING — this turn's two hand-offs plus older ones | a producer with no `tick` filter re-dates the retention window to now |
| the mid-tick RECAPTURE, re-shipping this turn's rows at their own `seq`s | a producer with no `seq` set announces each one twice |
| the same rows re-stamped for the NEXT turn | the vacuity guard — both claims above pass on a producer that has stopped ingesting at all |

**The old rows are hand-offs in every other respect** — same action token, same status tokens — so a
filter keyed on anything but the tick lets them through; and they are chosen to push the count PAST
`ATTENTION_HANDOFF_MAX_ROWS`, a flood that stayed under the cap being counted correctly and still
wrong. **The details are spelled as chapter constants**, copied from `systems::labor`'s completion
pass, the `_assert_horizon_floor_is_the_whole_trip` rule: an expectation composed through
`AttentionController`'s own tokens can only agree with itself.

**The restore is load-bearing** — the window is cleared on a turn CHANGE, so an empty array ingested
against the turn already held leaves it exactly as it was and leaks three rows into every state after.

**Sabotage-verified, disjointly.** Restoring the old flattening in `build_turns_remaining` fails
exactly the rotting frame's DANGER-ink claim and nothing else in the run (the hazard-mark conjunction
correctly stays green — a flattened `-3` still reaches the STALLED hazard, which carries the mark,
which is why that claim cannot stand in for this one). Removing the tick/seq filter fails exactly the
ring and recapture claims, the vacuity guard correctly staying green.

**A clean run is 333 frames / 1077 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says.

## The kit swap's A/B moved to the BUILDERS' row (the builders-kits arc)

No frame added or removed, no assertion added or removed, and **both of `chapters/compose_rungs.gd`'s
kit-swap frames now stage a different input** — the sheet no longer prices a build at its own picker's
kit, which was the defect (`labor-ui.md` → "A BUILD IS PRICED AT THE **BUILDERS'** KIT"). The hunt
picker is held FIXED across `herd_kit_swap_bare_build` / `_geared_build` and what moves is the band's
`builders` row: unnamed on the bare frame (the sheet derives the roster's animal kit, for which that
band publishes no resolved row, so the crew builds bare at `≈17 turns`) and `handling` on the geared
one (a kit the band DOES publish a row for, so `≈11`). The counts are unchanged, which is the point —
the same two readings, off the row that actually decides them.

- **`BandFx.staff_builders` / `builders_role_row` take the row's `kit_id`**, defaulting to `""`, the
  honest fixture for a pool the sim has resolved no kit for.
- **The chapter's own synthetic handling entry (`HANDLING_KIT_ID`) and its `kit_tiers` row state
  `build_work_branch: animal`.** A
  worth without a branch serves NO web, so the geared frame would have read the bare answer for a
  reason that has nothing to do with the row under test.
- **`BandFx.kit_roster_fixture()` gained `hurdling` and `tillage`** — both list `builders` ALONE, so
  every hunt and forage picker in both harnesses is byte-identical and only the Builders role card can
  see them — and `hoes` joined the condition rows beside `hurdles`, the wire's list being the CONFIG's
  item table rather than the band's holdings (without it the Builders card read `Hoes dry` on a band
  nobody had asked about hoes).

**A clean run is 332 frames / 1075 `PASS`, exit 0 — RE-MEASURED.** The recorded figure above was one
frame and two `PASS` higher before this arc, which touched neither: `git diff` over
`tools/ui_preview/**` adds and removes no `_save` and no assertion, so the previous record had already
drifted — which is this line's own instruction being earned again.

## The rate is not a tax on building (`docs/plan_standing_upkeep.md` §4.6a)

No frame added, no frame removed, and **one frame renamed because its claim inverted**. The behaviour
is `labor-ui.md`'s; what belongs here is the fixture arithmetic, which moved in three places.

- **`improvement_never_finishes_unstarted` → `improvement_unstarted_standing_price`.** A wild patch
  with one builder now reads `≈50 turns` in green: nothing is banked, so nothing can rot, so the whole
  of that hand's output is progress. Its old positive (`∞`) is its new negative, and its old negative
  (`≈50 turns`) is its new positive — which is exactly why it was renamed rather than re-worded. A
  third PRECONDITION rides beside the two it already had: `meter_rot_per_turn` really is zero here. It
  gains a PNG-less step at the end — the same rung unstaffed back to nobody, so the OFFERED face can
  be read: **a RUNNING face carries no price at all**, so the standing-price claim can only be made
  where the rung is still an offer.
- **The turn A/B is staged on a SHORT-KEPT patch** (`_short_kept_food_tile` — a stated
  `meter_rot_per_turn` of 2.0 with the `patch_upkeep_shortfall` that explains it). The reference patch
  rots at nothing and every staffed builder on it climbs, so without the re-staging the lone-crew
  frame would quote `≈20 turns` in green and the two `∞` claims would have been asserted away. **The
  subtracted 2.0 is unchanged and its IDENTITY is not** — the counts (10 at four hands, `∞` at one)
  are the same numbers about a different mechanism, which the fixture's own doc says out loud.
- **The rot the fixture states is above what any shipped plant rung can bleed** (`plant:tended` 0.5,
  `plant:field` 0.75, and the animal web zero by construction), so neither `∞` is reachable at a
  staffed build crew on shipped config. The client must still render both, so the state is staged;
  whether it is reachable in play is the sim's question.
- **`HerdFx.ANIMAL_METER_ROT` is a CONSTANT, not a parameter** — no animal rung declares a
  `meter_decay`, so an animal meter never goes backwards. It is stated rather than omitted because the
  closed form nets it: an absent field and a stated nothing are the same arithmetic, and only one of
  them says the nothing is a fact about the web.
- **`ForageFx.build_work_floor` / `build_work_floor_tooltip` / `BUILD_WORK_FLOOR_ABSENT` are deleted**
  with the meta they scanned for; the claim they served is an ABSENCE now, paired with *the BUILDERS
  row still mounts*.

### …and the second landing: a PARKED build is not a failure

The sim's "no answer" boundary moved from *is anyone staffed* to *is there work banked*, so zero
builders became a reported state rather than a silence. Frame-count neutral again; **one frame renamed
for the same reason as before, its claim having inverted.**

- **`tile_meter_reverting` → `tile_meter_held`.** Same patch, same 96%, same nobody-on-it — and it
  reads `Held at 96%` in NEUTRAL ink with **no hazard mark**, where it read `⚠ Reverting 96%` in
  amber. The negative is asserted beside it (the mark is nowhere on that rung's value), because the
  whole point is an absence and a positive claim alone cannot see one.
- **The hazard SET shrank, and the shrink is asserted.** `_hazard_states_all_marked` carried *work
  banked and nobody on it* as state (2); it now carries the same state as a **`not held.contains(mark)`
  negative** inside the same conjunction, which is where the set is decided rather than sampled.
- **The turn A/B is staged at `plant:tended`'s OWN shipped rot (0.5), not a fixture figure.** Both
  crews outrun it — `≈40 turns` at one hand, `≈6` at four, both GREEN — which is the shipped plant web
  being honest: no plant rung bleeds faster than one worker banks, so a lone builder is slow rather
  than doomed. **The two `∞` states are reached at ZERO builders**, PNG-less on the same sheet: red
  `∞` on the short-kept patch, and the HELD pace asked of the producer for the half that is not a
  failure.
- **`build_pace` takes the CREW now**, because one wire value covers two states. The pair is asserted
  at the producer (`HOLDS` + no crew ⇒ `BUILD_PACE_HELD`, stops nothing; `HOLDS` + a crew ⇒
  `BUILD_PACE_HOLDING`, stops), since a frame can stage only one of them at a time.
- **`rung_row_value`'s `building` became `staffed`**, and the harness spells it `STAFFED` /
  `UNSTAFFED` rather than a bare bool: at a call site `false` says nothing about which of the two
  `BUILD_METER_HOLDS` readings the row is being asked for.
- **`band_panel_preview._assert_unbuilt_warning` asserts the MERGED note.** Its row still warns; what
  changed is that it wears the keeping note rather than a BUILDERS one, and the `unbuilt` model flag it
  read is gone.
- **`tile_two_meters_live` gained a PNG-less second reading with the keeping SHORT** — the one shape on
  which the at-risk mark has a choice of rows, and the shape a mark on the wrong row is invisible in.
  It asserts the routing at the producer (`at_risk_rung` answers Sow; `rung_is_under_kept` is true for
  Sow and false for Cultivate) **and** as rendered (the tended row keeps its badge unmarked), plus the
  third claim that makes the withheld mark safe: the card still carries its `At risk:` row. The
  negative rides `_hazard_states_all_marked`'s conjunction as state (6), so the SET decides whether a
  mark means anything rather than one frame sampling it.
- **`tile_meter_stalled` is the two-producer EQUALITY made visible** — a half-built Cultivate on a
  patch drawn below its own floor, nobody on it. The card renders the sim's `⚠ Stalled 96%` and the
  sheet answers `BUILD_TURNS_NO_ESTIMATE` for the same crew of zero, with the negative naming the
  reading a crew-gated work predicate produced (`held`). The precondition — the floor really does
  stand above this patch's stock — rides beside it, or the frame is about a patch with room to work in.
- **`_hazard_states_all_marked` grew a SEVENTH state: the routing on the UNBUILT arm.** `built` forks
  before the routing does, so the built-row negative said nothing about a Cultivate abandoned under a
  declared Sow; that state is asserted to carry its own mark **and no `≈`**, which is the reviewer's
  defect denied by name. `tile_two_meters_live` renders the same walk PNG-less, with the precondition
  that `build_verb` answers SOW while `at_risk_rung` answers CULTIVATE — the shape in which the two
  per-source numbers name different rungs.
- **The `held` FACE is rendered, not only asserted at the producer.** The A/B's own sheet is recomposed
  over the kept reference tile — **the same coordinates**, so the composition survives the swap and the
  only thing that moves is the shortfall — and the face reads `— held` where the short-kept one reads
  `— ∞ turns` at the identical crew of zero.

## The build-queue arc's changes here (`docs/plan_standing_upkeep.md` §4.6b)

**A clean run is 332 frames / 1076 `PASS`, exit 0 — RE-MEASURED.** No frame was added or removed; the
one `PASS` is `improvements.gd`'s claim that the land card's `At risk:` block names the ROLE that pays
(`selection-card.md` → "…BUT IT NOW CARRIES A REMEDY SUB-ROW"), sabotage-verified by dropping the
append.

**ONE FRAME MOVED, AND IT MOVED BECAUSE A LATENT FIT RACE WAS FIXED RATHER THAN BECAUSE THE COPY
GREW.** `forage_fodder_locked`'s compose card rendered **782px against a panel demanding 789, with the
internal scroll DISABLED** — precisely the clamped-short state
`_assert_compose_sheet_card_holds_its_content` exists to catch, and the only state in the corpus
within one line of that boundary.

The cause is in `ComposeSheet.refit`, not in the fixture: it waits ONE frame, then `_fit_width`s the
card and reads `_body.get_combined_minimum_size().y` **in the same pass**, so the height it fits to is
the previous layout's wrapping. Lengthening the offered face's standing-price clause by three words
was enough to make the two readings differ by 19px — one line. **`refit` now waits a second frame
after the width fit**, and the state renders 801 (its correct fitted height) with every other compose
sheet's card, panel minimum and body minimum **unchanged to the pixel** — measured across all eleven
states that assertion rides.

**The measurement that identified it is worth keeping**: print `card.size`, the panel's combined
minimum and the BODY's minimum together for every state the assertion visits. `card − body_min` is the
chrome `refit` used, and where that differs from the same subtraction on the panel, the two readings
were taken at different widths.

## `chapters/selective_gather.gd` — the species chips, and what they cost (the selective gather)

**Appended LAST in `CHAPTERS`**, after `trade`, so no existing frame moves. Twelve frames and
fifty-two `PASS`. It ends by closing the sheet, resetting the compose source and handing the
reference band back, so a chapter appended after it starts where every other one does.

**THERE ARE TWO CHIP STATES AND THEY ARE ASSERTED OFF `HudWidgets.SPECIES_CHIP_STATE_META`, never off
ink** — a filled pill is a thing an assertion cannot measure, and the meta is what lets a claim be
about the CONTROL. The retired third state (*default-included*) is why the meta existed at all; it is
still the right instrument, because the frame now carries the distinction as a pill's PRESENCE and a
scan cannot read that either.

**THE PRESSED CHIP IS THE ONLY CHIP THAT MOVES, and that is the regression this chapter guards.**
Measured in play on a basket of two — Tobacco Fields 57% beside Wild Grapevine 43%, both drawn as
selected — pressing Tobacco turned Grapevine off. `forage_take_only_pressed_moves` presses one chip on
a two-plant basket and asserts the OTHER chip is exactly where it was; **a basket of exactly TWO is
what the claim needs**, since on three plants a set-writing toggle and a subtracting one both leave a
plausible-looking row. The hay meadow is staged for it rather than a fourth fixture built, its
arithmetic already closing.

**AND NARROWING BY CLICK IS A SUBTRACTION NOW**, so `forage_take_chip_priced` reaches the scarce plant
by unticking the other two — which makes it the one state in this chapter that presses a chip TWICE,
and so the only one that can see a second press land on the first press's own answer.

**The other subject is the PRICE, and no picture can judge it.** The defect this chapter is written
against is a sheet SITTING STILL when a chip is ticked, which renders a perfectly ordinary readout —
so every price claim is a RELATION between two rendered readings of the same tile, never a magnitude
compared against a constant the fixture also states.

| frame | what only IT can say |
|---|---|
| `forage_take_default` | with nothing ticked every chip reads SELECTED, the line says the whole basket comes home, and the whole basket's take + useful count are recorded as the baseline everything below is a relation against |
| `forage_take_chip_priced` | two real presses on real chips move BOTH the quoted food and the useful-worker count — `0.96 → 0.15 FOOD` at 3 hands becomes `0.05 → 0.01` at 1 |
| `forage_take_narrowed` | the sheet OPENS on the selection the band's own row carries, and prices it IDENTICALLY to the ticked one |
| `forage_take_zero_food` | a plant paying `0.0` food is PRICED — a live `1.22 → 0.18 FODDER`, no food row, no *not priced* aside |
| `forage_take_unquoted` | a selection the WIRE priced no rate for quotes nothing, in words |
| `forage_take_absent` | **the OTHER silence, and it says NOTHING** — a selection naming plants this tile no longer grows quotes no take and prints no sentence, in particular not the frame above's. The composer's own reason is asserted directly (`SELECTION_REASON_ABSENT`), that two-state split being what keeps the surviving sentence off this state; judged with the frame above and with a third claim neither may swallow: a crop paying `0.0` food is FULLY QUOTED |
| `forage_take_cultivate` | single-pick: exactly one chip lit, and the line NAMES the crop the game would settle on |
| `forage_take_cultivate_picked` | picking a crop MOVES the lit pill — it picks the basket's OTHER `can_cultivate` member, since picking the crop the resolver had already settled on would light the pill that was already lit — and the line says cultivating weeds the rest out |
| `forage_take_cash_narrowed` | **the case the feature was argued on** — tick cotton, see `3.55 FIBRE · 1.52 TOBACCO` where the *not priced* apology used to be, and no FOOD row |
| `forage_take_cash_merged` | flax + cotton, both fibre payers, composing into ONE fibre row |
| `forage_take_cash_grain` | the same basket narrowed to the grain: NO material row at all, and the food still quoted |
| `forage_take_only_pressed_moves` | **the reported bug** — a basket of two, one chip pressed, the other exactly where it was |
| `forage_take_last_plant_refused` | the last remaining plant cannot be unticked, and the row SAYS why in the consequence line's own slot |

**THE TICK IS DRIVEN AS A POINTER GESTURE** (`_press_chip`, through `InputProbe`), because the tick
is what broke: a state that wrote the selection into `ComposeState` and re-rendered would assert the
harness's own write rather than the control's effect. A chip is a plain `Button` under a
mouse-transparent face, so a press at its rect centre reaches it exactly as a player's does — and the
press FREES that button (the compose block rebuilds), so the helper settles before anything is counted
and the caller must not touch it after.

**`_press_chip`'s RETURN IS NOT THE REFUSAL'S WITNESS**, and reading it as one is the trap that state
walked into first: it answers whether a button was found and a pointer pushed at it, which is TRUE of
the refused press too. What is declined is the MODEL's edit, so the verdict is read off the rendered
chip (still selected) and the rendered sentence — plus the NEGATIVE that the line it replaced is
absent, without which "states the reason" passes on a row printing both sentences at once. A fourth
claim presses another chip and requires the sentence to be GONE, which is what pins the flag as a
one-transaction memory rather than something the row keeps saying.

**THE SCARCE PLANT IS CHOSEN TO MOVE BOTH ARMS OF THE TAKE'S `min`** — the tile's smallest share AND
its poorest converter — so a sheet that re-clamped the crew without re-pricing, or re-priced without
re-capping, fails one of the pair.

**THE PER-SPECIES RATES ARE DERIVED, NOT AUTHORED** (`_price_basket`). The wire's contract is an
identity — `Σ(share × rate) == provisionsPerBiomass` — so three rates written freehand would describe
a patch no server can publish, and the sheet's composition would then be checked against arithmetic
that does not close. The fixture states WEIGHTS and normalises them against the patch's own
basket-averaged rate, so the identity holds whatever that rate is; the hay meadow does the same one
member at a time (`0.6 × (rate / 0.6) + 0.4 × 0 == rate`).

**THE MERGE CLAIM IS ARITHMETIC ON THE COMPOSITION, NOT A READING OFF THE TAKE.** Flax and cotton pay
fibre at rates 3.5x apart, so a composition that took the LAST one rather than merging by material id
lands on `0.12` or `0.42` where the weighted mean is `0.24` — and a take is that mean through two more
clamps, so a frame showing a plausible number cannot separate the two answers. The chapter therefore
calls `SourceForecast.selection_rates` directly for the merge, with a **single-species control** beside
it (cotton alone must compose its own rate, unmerged) so the `0.24` is demonstrably neither plant's
number by accident. The rendered frame's claim is the weaker, complementary one: the sheet draws ONE
fibre row.

**THE THIRD PLANT IS A CONTROL, AND THE FIT ASSERTION RIDES THE MERGED FRAME.** `wild_emmer` pays the
whole cash tile's food and NO material, so `forage_take_cash_grain` can claim "no row" is
distinguishable from a zero on the same basket. `forage_take_cash_merged` carries
`_assert_compose_sheet_fits` / `_assert_compose_sheet_card_holds_its_content` because a narrowed take
states every account the selection pays and this is the widest readout the sheet can produce — the nine
standing fit states are all whole-basket sheets and cannot see it (measured: 300 demanded of 306
usable).

**THE CASH TILE'S PATCH VECTORS ARE COMPOSED FROM ITS ENTRIES', never authored beside them** — the
`Σ(share × rate)` identity the schema pins, on every account — for `_price_basket`'s reason one
account out: a fixture stating them freehand describes a patch no server can publish, and the
whole-basket half of every claim is then checked against arithmetic that does not close.

**THE ZERO-FOOD STATE IS A PAIR ON ONE BASKET.** Ticking the hay must state no food row; ticking the
grain in the same meadow must still state one — without the second half, "no food row" passes on a
readout that stopped stating the account. Its `Foddering` track is dialled complete in this chapter's
`update_intensification`, or the fodder account is locked and the `0.0` this state exists for has no
live sibling beside it.

**THE BRACKET'S CLAIM IS A DRIVEN PROBE, NOT THE FIXTURE'S NUMBERS.** Both surfaces read the wire's
`standing_biomass` now, so the fixture's figures MATCH the tile card's split on purpose — a fixture
that made them disagree would put two bracket sets for one basket on one screen and read as a defect.
`_assert_bracket_follows_the_wire` instead moves ONE entry's published biomass to a value no share of
this tile can produce, recomposes, asserts the chip followed, and restores it — PNG-less, so the
frames after it are the frames before it.

**`forage_take_narrowed` PAYS FOR A SECOND `_settle` EXPLICITLY.** `ComposeSheet.refit` waits TWO
frames — width first, then the body's height at that width — and every other state here composes
twice and pays that frame incidentally; this one composes ONCE on purpose (the selection must come
off the band's row and nothing else), so without it the card renders at the previous fit's height.

**Three existing negatives were re-pointed when the chips landed, and they were asserting a rule this
arc narrows.** `ForageFx.GATED_CROP_NEEDLE` was `"Wild Grain"` — a plant name asserted absent from
the whole compose sheet, to prove the retired crop LIST was gone. The chip row puts one chip per
named plant back on that sheet, so all three states now name Wild Grain legitimately. What those
states still claim is true and worth asserting — a sheet composing a plain gather offers no crop
CHOICE — so the needle is `GATHER_SHEET_CROP_KEY_NEEDLE`, the chip row's SINGLE-PICK key (`Crop`),
which appears only where a rung is in flight. A plant name is no longer evidence of a crop picker.

**RETIRED — `forage_take_overstaffed`.** It staged a crew above the standing row's `workersNeeded`
and asserted a chip-row idle warning that no longer exists: the crew stepper's own
`max N useful here` note moves with the chips now, so a second sentence would be a second producer of
one verdict. Its claim survives, sharper, as `forage_take_chip_priced`'s useful-count half.

**A clean run is 346 frames / 1177 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says. The
toggle fix added the two frames above and eleven claims net (four retired with the third chip state and
the crop-picker distinction it carried, fifteen added across the subtraction, the regression and the
refusal).

## The ⚠'s one producer, and the biomass quantiser (this arc)

Two PNG-less blocks appended to `chapters/hunt.gd`, one 2x2 re-pointed in
`chapters/forage_accounts.gd`, and one claim retired from `chapters/herd_improve.gd`. The behaviour is
`labor-ui.md`'s; what belongs here is the shape of the drive and what the sabotages fire.

- **`_overdraw_is_the_wires_answer` swaps the acting band for a COPY carrying a standing row**, asserts
  the three surfaces (`source_yield_readout`, `BandOverlayRenderer.yield_label_overdraw`, the compose
  model) **and their AGREEMENT**, and hands the band back. The agreement is asserted separately from
  the three readings, because three claims that each happen to coincide is not the same statement as
  *these surfaces cannot disagree*. Its PRECONDITION is that the retired `actual > sustainable`
  comparison answers the OPPOSITE of the wire on both halves of the A/B — an `actual` below its
  `sustainable` where the wire says true (the first harvest the schema names), a kill turn's spike
  where it says false — so neither claim can pass on a client that is still deriving.
- **`_wolf_material_take_assertions` is the boar pair's claim on the inedible web**, with the same
  vacuity guard: the retired crew-throughput line must really move across the crew range the reach arm
  pins, or *"flat"* is a claim about a fixture rather than about the fix. The oracle is
  `HerdFx.hunt_take_oracle` handed BIOMASS terms — that function names no account, so it is the same
  cross-check the deer's food terms get rather than a second oracle.
- **`_edible_take_is_unchanged_assertions` exists because nothing else could see the failure.**
  Measured: pointing `SourceForecast.body_quantum` at the published `bodyMass` instead of the food pair
  moved **forty-odd frames and failed not one assertion**. Every claim on those sheets is a relation, a
  presence or a word, so a take quoting the wrong number renders a perfectly plausible readout. It
  asserts the rendered FOOD account against the RETIRED food-keyed expression on the reference herd,
  over a precondition that that fixture's two pairings genuinely disagree.
- **The fixtures that grew terms did so rather than being worked around.** The wolf states
  `body_mass` / `per_worker_biomass` / `engage_rate`; `_quantisation_boar_herd` and `_cadence_herd`
  state `per_worker_biomass`, because two of the three blocks that read them do NOT floorify and a
  fixture whose take depends on which caller reached it first can disagree with itself.
- **`herd_improve.gd`'s *"the overdraw gate answers the same, build or no build"* is RETIRED**, not
  restated: it was `X == X` over a helper that no longer exists.

**`herd_overdraw_agrees` is the one FRAME this arc adds**, appended LAST in the hunt chapter so no
earlier state moves: the herd drawer's standing summary (`💀 3 hunters · +0.63 /turn ⚠`) beside the
compose sheet's readout (`1.23 → 0.00 FOOD · ⚠ OVERDRAWS THE HERD`) — the reported pair, agreeing, on
one source, with the sheet's own take deliberately ABOVE the row's steady rate so the mark is visibly
not a comparison of the two. **It is EVIDENCE, not the claim**: the third surface is painted into
MapView's canvas and only the driven block can ask it.

**TWELVE frames moved and every one is accounted for.** Nine lost a ⚠ the sheet used to invent (all
nine are UNWORKED fixtures — see `labor-ui.md` → "THE CONSEQUENCE IS THAT THE SHEET REPORTS RATHER THAN
PREVIEWS"), and `herd_hunt_pelts_only` / `herd_hunt_pelts_raid` are the wolf gaining a body: `0.40 HIDE`
where it read `0.22`, an animal-counted floor flag, and a party cap the engagement crew now floors.
**`herd_hunt_both_products` and `herd_hunt_material_take` moved for a tenth reason worth knowing**:
each is identical to the pixel down to its commit button and its CARD is 13px / 3px taller — the
documented `refit` fit sensitivity, tripped by a neighbouring sheet changing height. Every fit
assertion stays green.

**Sabotage-verified three ways, each failing a DISJOINT set.** A floor-keyed derivation restored in
`_source_overdraws` fails **seven** (the forage 2x2, both build-crew claims, and the hunt block's
compose-sheet and agreement halves). The crew-throughput line restored beside the delivered figure
fails **thirteen** — every material claim on both quarries, each naming the crew-scaled number. And the
quantum pointed at the published `bodyMass` fails **one**, the edible magnitude claim, at
`0.1365 against 0.2093`.

**A clean run is 352 frames / 1302 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says. The
recorded figure before this arc was 346 / 1177, the run MEASURED 347 / 1200 before a line of it was
touched, and 348 / 1227 was recorded mid-arc — un-recorded drift accumulating every time, which is this
line's own instruction being earned again.

> #### ⛔ AND THE COUNT IS MACHINE-CHECKED NOW, BECAUSE `EXIT=0` WAS NOT ENOUGH
>
> **A GDScript `assert` that fails inside a chapter aborts that chapter and the harness still exited
> `0`.** It was caught only by comparing the `PASS` count against a run from earlier the same session:
> a stale-closure regression killed `compose_rungs` partway, **67 claims never ran** — the forage-drawer
> restate, the herd restate and the whole kit/equipment-tier block — and every other gate was green
> (build, clippy, `decode-guard`, 1799 Rust tests, and this harness's own exit status).
>
> **Godot cannot surface the abort**: nothing in the process can read its own stderr, and an aborted
> coroutine returns exactly as a finished one does. So the harness asserts **its own work** instead —
> each chapter declares `const EXPECTED_CHECKPOINTS`, and `ui_preview.gd` samples a counter around
> `chapter.run(self)` and fails the run if the chapter falls short.
>
> - **Checkpoints, not assertions** — `_assert_hud` *and* `_save` both count, because `docks_legend`
>   makes **zero** assertions and renders frames only; an assertion-only floor would be `0` there and
>   leave the one chapter an abort truncates entirely unguarded.
> - **EVERY assertion sink counts, and one of them did not.** `_assert_turn_orb` is a second sink
>   beside `_assert_hud` — the turn-orb chapter's own — and it printed its PASS without touching the
>   counter, so fifteen claims escaped the tally `_assert_hud`'s docstring says no claim can escape.
>   An abort could have dropped the whole turn-orb guard set and still cleared that chapter's floor.
>   A new sink must increment `_checkpoint_count`, or it is a hole in exactly the guard it looks like
>   it is part of.
> - **A floor, not an equality** — adding claims must never fail the run; losing them is the failure.
> - **A chapter that declares nothing FAILS**, which is what makes it un-bypassable: were a missing
>   const merely unguarded, deleting it would be the silent bypass and every new chapter would start
>   unguarded.
> - **The number lives on the chapter**, never in a roster in `ui_preview.gd` — a table of 22 counts in
>   the harness is exactly the shared-edit surface the chapter split exists to remove. Read through
>   `get_script_constant_map()`; a `var` is deliberately not accepted, since the run could move it.
>
> Falsified twice, disjointly: the real regression restored, and a planted `assert(false)` at the top of
> `crafting_bench.run()`. Both `EXIT=1`, each naming the chapter and the count it reached.
>
> **So `$?` now covers a chapter dying mid-run.** It still does not cover a claim that is *present and
> wrong* — that is what the sabotage discipline is for.

## The knowledge arc's two chapters — the screen (slice B) and its orb rows (slice C)

`chapters/turn_orb.gd` grew the slice-C block; the screen's own chapter is below. Two things about
the turn-orb one are the arc's, not the orb's, and both are about SHARED WALK STATE:

- **It clears the faction's tracks for its band states and puts back what it inherited.** The
  knowledge producer is faction-wide and rides the band rows' own registry, and earlier chapters push
  tracks without always advancing the turn — so the screen's diff has not rolled since whichever of
  them last did, and the first turn tick here rolls everything they taught in between onto one orb.
  The ALL-CLEAR states stop being clear and the under-kept block's negative-control COUNT gains a row.
  Restoring is the other half: `compose_rungs` runs three chapters later and gates its hunt-compose
  frames on that knowledge, so leaving the tracks cleared moved four of its frames into judging a crew
  stepper under knowledge nobody meant to change.
- **It empties the band half at the CACHE (`set_band_attention([])`), never by ingesting a calm
  band.** `update_band_alerts` is not inert — `ingest_snapshot_bands` overwrites the walk's
  `player_band` / `player_bands` / `prev_band_sizes`, which later chapters render against. That was
  the first cut, and it moved the same four frames a second way.
- **It hands back the TURN as well**, which `docks_legend`'s `reserved_dock` draws on the orb face.
  The tracks go back at two different turns, because the knowledge diff only rolls when the turn
  moves and a single push would announce whatever the inherited tracks hold that the block's own
  staging did not.

**All three were found by PIXEL-DIFFING every frame against a run at `main`, not by the exit
status** — the run was green through all of it. The tell was a frame moving outside the orb's own
corner.

> #### ⛔ A GITIGNORED BUILD ARTIFACT MOVES EVERY FRAME, AND IT IS NOT YOUR CHANGE
>
> `clients/godot_thin_client/build_stamp.txt` is written by the build and read by `ClientBuild` into
> the bottom-centre `build cli … · srv ?` overlay, which **every frame draws**. It is absent on a
> fresh worktree (the overlay reads `dev-unknown`) and appears the moment something stamps it — so a
> baseline captured before that and a run captured after differ on **400+ frames**, in a 10px strip
> nobody looks at. Move it aside before capturing either side, or the diff drowns the real movers.

### `chapters/knowledge_panel.gd` — the knowledge screen (slice B)

**Appended LAST in `CHAPTERS`**, so no existing frame moves. Seven frames and 89 assertions (its
`EXPECTED_CHECKPOINTS` floor is the measured **96** — frames count too, so the three launcher-face
frames the cairn arc appended moved it 75 → 85, and the loaded-world block moved it again to 96;
the floor is RE-MEASURED, never the old number plus the claims anyone remembers adding, and the
surplus over that arithmetic is the drift that says why. **Two branches each raising this number is
a merge conflict whose answer is neither side** — take the measurement of the merged file), and **most
of it is PNG-less on purpose**: every claim this screen makes renders as a plausible picture whatever
it says — a pill reading `2`, a greyed row, the clause *"nothing is using it"*, a `3` on the launcher's
pip — so the derivation is asked of `KnowledgeRoster` directly, with models staged in the chapter, and
the frames are for the LAYOUT alone. The behaviour is `knowledge-panel.md`'s; what belongs here is the
shape of the drive.

| what only IT can say | how |
|---|---|
| a faction that knows NOTHING renders every ladder track, all `not begun` | asked of an EMPTY tracks dict, which is what the wire really sends on turn one |
| a domain with no nodes draws NO column | the craft fan is the only one that can be empty, the two ladder columns' nodes being DECLARED |
| the tracks that unlock nothing are exactly `UNLOCKLESS_TRACKS` | the derived set against the declared one — the ONLY thing that tells a deliberate omission from a forgotten `RUNG_KNOWLEDGE_TRACKS` entry, which reads identically |
| at-or-ABOVE, not the done FLAGS | a patch carrying `is_field` and NO `is_cultivated` must read Cultivation as in use — the reading a per-verb-flag test gets backwards |
| the two webs' pools do not cross | a plant patch must not satisfy an animal knowledge |
| a craft is in use four ways and NOT by a recipe existing | the negative runs FIRST with the whole recipe book present and nothing held, since "a recipe of this craft exists" is true of every craft on every turn |
| the cross-craft negative | holding an awl must not make Tanning in use — without it every craft goes in use the moment the band holds anything |
| a rival's patch, and a herd nobody works | asked of the CONTROLLER, where the two resolutions live, and staged the way the shipped scans read them (a different faction on the patch; the herd off the band's assignment list) |
| the five filter counts, by EQUALITY | over a model whose five filters all answer DIFFERENTLY — a fixture where two coincide cannot tell those two apart |
| a KNOWN track is never `close` | behind the precondition that there ARE known tracks for one to have leaked from |
| the FIRST observation reports nothing new, the next TURN does, a second snapshot in the same turn keeps it | three claims, and the middle one is what stops the first passing on a diff that never fires |
| "new this turn" implies KNOWN | an empty tracks row pushed with the diff still holding the key — the shape that rendered `New this turn 1` over a faction that knew nothing — with the diff's SURVIVAL asserted beside it, or the claim passes because the set was cleared |
| the pip's count, and that it survives a dock change | on a REAL `BandCityPanel`; the retention is invisible in any frame |
| a knowledge-only delta moves the pip | pushed with NOTHING else, which is what makes it a claim about the SECTION rather than about the frame |
| the pip clears when a tended patch appears | the honest trigger — opening the screen deliberately does not clear it |
| **a LOADED world does not announce what it arrived knowing** | the roster and the progress row pushed on ONE turn in `Main`'s order, then a tick — the shape a save/load actually produces, and the reason it is the ladder twin of the late-catalogue block. Four legs: the roster really carries the three as KNOWN (**first**, or the rest is vacuous), no `learned_this_turn`, no `new_this_turn`, no row out of `AttentionController.knowledge_attention` — the orb asked in its OWN terms, since the orb is the surface that shouted — and a track completing AFTER the load that must still be announced, without which a fix that merely went quiet passes |

**THE FIXTURES DERIVE THEIR STANDING RUNG** (`fixtures_rung.gd`), this tree's rule.
⛔ **AND A HERD FIXTURE IS KEYED `id`, NOT `herd_id`** — `HudBandLaborState.find_world_herd` matches on
`id`, so the other spelling is invisible to the assignment walk and every animal claim reads "nothing
is using it" for a reason that has nothing to do with the code under test. It cost a run.

⛔ **THE SECTIONS ARE PUSHED IN `Main`'s OWN ORDER** — knowledge, catalogues, patches, then
populations. This chapter's first cut pushed `update_band_alerts` first, so the pip was computed
against the previous block's tracks and read `2` where the controller read `1`: a fixture in any other
order is staging a snapshot no server sends. **That mismatch is also what found the live defect** —
the pip was pushed from `update_band_alerts` alone.

**The row and the filter pill are DRIVEN with real pointer input**, never `pressed.emit()`: a node row
is a `PanelContainer` with a `gui_input` handler and has no signal of its own to fake, and the harness
contract's reason applies either way — an emitted signal passes on a control that is covered,
zero-size or filtered out of the hit test, which is exactly the shape that row shipped in first (a
`Button` whose face was never laid out).

**Its FAILURE MESSAGES name what was FOUND, and the first cut named what was WANTED** — it printed
`not wanted`, so a failure read `in use (got true)` and said nothing about the verdict. An ABSENT node
is reported distinguishably too: a roster that dropped a track answers `false` to every question asked
of it, and "the node is missing" is not the same failure as "the node says in use".

**Frames:** `knowledge_panel` · `knowledge_panel_untouched` (**the frame this arc is about**) ·
`knowledge_panel_detail` · `knowledge_panel_filtered` · `knowledge_launcher_mark` / `_rail` / `_bar`
(the launcher's bundled cairn at each of the three action mounts, captured inside the pip block
because that block is the only one standing a REAL `BandCityPanel` up).

**A clean run is 384 frames / 1710 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says. The
figure recorded when this chapter landed was 353 / 1405, and the loaded-world block added eleven
claims and no frame; everything else between the two is drift accumulated un-recorded, exactly as it
had been the three times before. Measure; do not sum.

**NINE SABOTAGES, each failing a DISJOINT subset and each naming what it caught:**

| the forbidden implementation | fails, and only |
|---|---|
| the `progress <= 0.0` skip restored | the two greyed-track claims, plus the `all` pill (8 → 7) |
| the per-verb done FLAG instead of the rung comparison | the two at-or-ABOVE claims (`a FIELD stands above Tended`, `a pen stands above pastoral`) |
| a recipe of this craft merely EXISTING counts as in use | the three craft negatives |
| `close` as a bare progress threshold | the `close` pill (2 → 5) and `a KNOWN track is never close (3 leaked)` |
| `new this turn` no longer implying KNOWN | the coherence claim alone |
| the pip pushed from `update_band_alerts` alone | the knowledge-only-delta claim alone — **the live defect, in its own words** |
| the pip count living only on the button | the mount-rebuild claim and the inside-the-button claim |
| patch ownership ignored | the RIVAL's-patch claim alone |
| every herd on the wire counted as the faction's | the unworked-pen claim alone |
| the same-turn baseline fold narrowed back to `first_seen` | **5 of the loaded-world legs** — two tracks not-learned, their two roster nodes, and the orb's row count (`2 rows`) — with the three non-vacuous legs still PASSING, which is what shows the block was really staging known tracks. **The live defect, in its own words.** Only two of the three tracks fail, because this long-lived HUD had already folded `cultivation` in an earlier block; the sabotage is still decisive |

### TWO FRAMES MOVED FOR A REASON THAT WAS NOT THIS ARC, and proving that took a revert run

`band_morale_expanded` / `band_growth_expanded` came back different from a baseline captured earlier
the same session — the disclosure popover OPEN where the baseline showed it closed, which is what those
frames' own names and comments say they stage. **Neither carries an assertion**, so nothing in the run
said so either way.

It is not this arc: with the whole client reverted to the pre-change commit **in the same environment**
the two frames still differed from the baseline and matched the changed tree, byte for byte. What sits
between the two is a **`godot --import` pass**, run to register the new `class_name`s — the documented
"a harness renders the IMPORT CACHE, not the art on disk" hazard, reaching a popover rather than a
sprite. The new rendering is the correct one.

**The method is the part worth keeping**: swap the changed files to the earlier ref with `git show`,
re-run, compare, swap back, and confirm `git status` is clean. An unexplained frame move is not
evidence of anything until it has been attributed, and "my change is the only thing that moved" is an
assumption a single run can falsify.

> **`compose_band_switch_forage` FLAKED ONCE DURING THIS PASS AND PASSED CLEAN ON RE-RUN** — five
> failures cascading from one press that landed on the dismiss catcher, the documented synthetic-pointer
> race (`labor-ui.md` → "THE SHEET DISMISSES ON PRESS **AND** RELEASE"). A run that fails only that
> state's block is that race, not a regression; re-run before believing it.


## `event_dock_crew_cut` also pins the two rungs' MARKS, sampled off the render

The frame already staged all four `status=` rows plus a plain receipt in one fixture, which is what
made the glyph claim possible without a new frame: **a trimmed line and a lapsed line are in one
render**, and the whole defect was that they drew identically. A frame with only one of them is green
with the fix and green with the defect restored.

`_preview_dock_row_glyph` / `_preview_dock_row_glyph_color` find the row by the text of its LABEL
column and answer its **glyph column's** text and ink — `_make_event_row` builds each row as an
`HBoxContainer` whose first child is the glyph `Label`, so this reads what the player is looking at.
The expectation is the vocab's named const; composing both sides through `HudEventVocab` would assert
only that the table agrees with itself.

**The set is five row-level claims plus two that a new status can actually break:**

- the cut wears `STATUS_REDUCED_GLYPH`, the destroyed row wears `STATUS_SHED_GLYPH`, **the two are not
  equal**, the narrowed take wears the cut's mark, and both keep `HudStyle.WARN`;
- the **iff over the whole `DETAIL_STATUS_STYLE` table** — every token's glyph tracks its rung, in
  both directions — because every claim above names a token the chapter already stages and a token
  added later would pass all of them;
- **and that the two consts differ**, which is the precondition without which the iff is vacuous.

### Falsifications

`ui_preview` before the change **1344 PASS / 0 FAIL**, after **1350 / 0**, exit 0 both.

| Restored defect | Failures |
|---|---|
| `STATUS_REDUCED_GLYPH := "⚠"` (both marks the same const value) | **2** — `READ APART … ("⚠" vs "⚠")` and `the two marks BEING DIFFERENT` |
| the `status=trimmed` ROW re-pointed at `STATUS_SHED_GLYPH` (the shipped defect, literally) | **4** — the cut's mark, `READ APART`, the narrowed take's mark, and the table iff naming the row: `["status=trimmed(⚠ wants ▾)"]` |

**The first row is the honest finding.** With the two consts equal, the row-level equality claims and
the table iff **all pass vacuously** — the const moved under them — and only the two inequality claims
fail. That is exactly why the `BEING DIFFERENT` claim is in the set: it is not a restatement of the
iff, it is the thing that keeps the iff from being satisfiable by a table that marks everything alike.


### …and the SIXTH row: `status=stalled`, which is what that iff was for

The crafting bench's third shed token was added to the same fixture rather than to a frame of its own,
so all three rungs are in ONE render: five `▾` rows (both bench lines, the pruned take, the two
trimmed crews) and one `⚠` on the lapsed row, which is also the only amber-labelled one.

**The iff caught it the moment it was wrong, both ways** — under a `RUNG_ALERT` rung it named
`["status=stalled(▾ wants ⚠)"]`, and under a `STATUS_SHED_GLYPH` mark `["status=stalled(⚠ wants ▾)"]`.
That is the invariant behaving exactly as it was written for: a token added later passes every claim
that names the tokens the chapter already stages, and only a rule over the whole table sees it.

**Two staged bench rows, not one.** `announce_shed_bench` reuses `status=trimmed` with `kind=bench`
where hands remain, so `SHED_BENCH_TRIMMED_*` is the negative that stops the new token quietly
becoming *every* bench line. Confirmed rather than assumed: it reads Notable with the same `▾`.

**The rung claim is really about `RUNG_BY_KIND`.** `craft` is absent from it, so the line takes
`DEFAULT_RUNG` (`RUNG_ROUTINE`) — under the dock's own floor — and `…reaches the DEFAULT floor, which
`craft` alone would not have` is the claim that a missing `DETAIL_STATUS_STYLE` row would break.

> ⛔ **THE FRAME HAD TO OPEN THE LOG, AND THE BAR'S CAP IS WHY.** `RECENT_COUNT_MAX` is **4** and the
> fixture now stages **six** status rows, so collapsed the two oldest are pushed off the bar — which
> silently turned `trimmed` and the linked hunt row into rows nobody could sample, and showed up as
> `a CUT crew is drawn with the reduction mark "▾" (got "")` plus two link claims. Expanded,
> `_render_bar` draws a single title line and `_log_body` draws every visible event, so there is no
> double-count and every floor claim is unaffected — they read `_visible_events()`, not the drawn bar.
> **A fixture that grows past the bar's cap must open the log or it stops being one frame.**

### Falsifications

`ui_preview` before **1351 PASS / 0 FAIL**, after **1356 / 0**, exit 0 both. `band_panel_preview` is
untouched at **805 / 0**.

## The bench's RANK — one frame, twenty claims, three disjoint sabotages

`docs/plan_standing_upkeep.md` §4.9 item 9b's crafting half. The behaviour is `crafting-panel.md`'s;
what belongs here is the shape of the drive and the ONE measurement that moved the design.

**`crafting_bench_priority` is appended LAST in `chapters/crafting_bench.gd`**, after the map-gesture
block and before the chapter hands the HUD back, so no earlier frame moves. It is the whole new
state in one picture: an IDLE bench (no job, hence no ✕), a HIGH mark leading its line two, the
picker OPEN beneath it and its `High` rung lit against two unlit ones.

**A MARKED BENCH AND AN UNMARKED ONE CANNOT SHARE A PNG** — the card renders one band's bench — so
the unmarked half is a claim rather than a picture, made on the reference band in the same block and
**PAIRED** with the marked one. Neither is worth anything alone: *prints a prefix* passes on a panel
that always prints one, *prints nothing* on a panel that lost the mark entirely.

**⛔ THE LINK COSTS THE WELL NO ROW, AND ONE ASSERTION IS WHY.** Built as a fourth line under the
teach line, it added ~24px to every bench well — and `crafting_panel_band_dock_collapsed`'s ledger
clears its 1072px room by **18px**, so the state whose `expect_overflow` is `false` began scrolling
and the run failed there. The link rides the trailing edge of LINE TWO instead (the sub label expands
into the slack), which is a placement measurement made by an existing assertion rather than by
inspection.

**Every press is REAL POINTER INPUT** through `InputProbe` — the link's and each rung's —
`pressed.emit()` being unable to see a covered, disabled, zero-size or IGNORE-filtered control. The
command is asserted on the LINE through `MAIN_SCRIPT.format_bench_priority`, the work board's own
idiom, for **all three** levels: a commit hard-wired to `normal` satisfies any single-level claim.
Beside each, what did NOT go out — a mis-wired rung emitting `bench_crew` or `clear_bench` satisfies
a bare *something was emitted*.

**The LIT test is the SAME one `band_panel_preview._assert_work_priority_picker_lit` makes** — the
primary variant's own background — so the one control cannot be judged by two notions of "selected"
on the two surfaces that mount it.

### Falsifications

`ui_preview` before **1356 PASS / 0 FAIL / 349 frames**, after **1376 / 0 / 350**, exit 0 both.
`band_panel_preview` untouched at **805 / 0**, exit 0.

| Restored defect | Failures |
|---|---|
| the commit emits `WORK_PRIORITY_NORMAL` whatever rung was pressed | **2** — the `high` and `low` line claims, each naming `got "bench_priority 0 4971 normal"`. The `normal` claim correctly stays green |
| the link and the picker gated on `recipe_id != ""` (hidden on an idle bench) | **3** — `the bench well carries a \`Priority\` link to press`, `the picker renders on an IDLE bench`, `…opened on HIGH rather than on the default ()`. Both MARK claims stay green |
| the line-two prefix never rendered | **3** — the HIGH mark, its ink, and the LOW mark's face-and-ink pair. `a NORMAL bench prints no rank prefix` correctly stays green |

**None passed vacuously**, and each sabotage's green half is the pairing doing its job: the mark and
the control are independent, so a defect in one must leave the other's claims standing.

| Restored defect | Failures |
|---|---|
| `status=stalled` at `RUNG_ALERT` | **3** — `is Notable … (got alert)`; `NOT the Alert a lapsed row earns`; the iff naming `["status=stalled(▾ wants ⚠)"]` |
| `status=stalled` wearing `STATUS_SHED_GLYPH` | **2** — `wears the reduction mark beside the cut … (got "⚠")`; the iff naming `["status=stalled(⚠ wants ▾)"]` |
| its `DETAIL_STATUS_WORK_LINK` row removed | **2** — `exactly the 4 rows that NAME a band offer the jump (got 3 links)`; `asked [3, 5, 7], want [3, 5, 5, 7]` |

None passed vacuously: each restoration was named by at least one assertion that reads the RENDER and
one that reads the table.

## The event dock's long detail, and the compose layer (`chapters/event_dock.gd`)

Four frames and twenty-five `PASS`, appended to the event-dock chapter before it frees its panel —
so no earlier state moves and the chapter still hands the HUD back where it found it. RE-MEASURED off
the run rather than hand-summed, as this file's own rule says: a first draft of this line read "three
frames and fifteen", because `_assert_card_within_root` makes two claims per call and is called three
times. The behaviour is `event-dock.md`'s and `panel-framework.md`'s; what belongs here is the shape
of the drive.

- **`_report_event_row_columns` PRINTS the derivation's own terms** — the long detail's natural width,
  the cap, the widest row's minimum against the budget, and the log head / foot / card minimums
  against the floor. `DETAIL_MAX_WIDTH` is derived from measurements, and a derivation nobody
  re-measures goes stale silently; a red line there asks for a decision rather than a failing run.
  **It reports the WIDEST row, never the last one**: `GLYPH_COLUMN_WIDTH` is a floor rather than a
  clip, so an emoji kind glyph draws past it and two rows of different kinds do not cost the same —
  a figure taken off whichever row rendered last is 9px optimistic, which is most of the slack.
  It also splits a row's FURNITURE by whether the row carries a `Work tab` link (79 against 154 here,
  the link itself 66), which is the split `ROW_FURNITURE_WIDTH` is derived across. Furniture is taken
  as row minimum LESS the detail's, never as a sum of the controls the harness knows about — the
  failure being guarded is a control added to the line and left out of the budget, and a hand-summed
  figure would leave the new one out a second time.
- **`_assert_card_within_root` asserts the DRAWN rect and the combined MINIMUM alike.** The rect says
  what this window did; the minimum says what the card would demand of the narrowest strip the dock
  can ever be handed, and only the second survives a change of viewport. Made on both surfaces, since
  the expanded log is where a row's minimum propagates hardest (`_log_scroll` disables horizontal
  scrolling, which passes a child's minimum width straight through).
- **THREE DETAIL LENGTHS ON ONE FRAME, because the bound has three behaviours and no single row can
  show more than one.** A SHORT one (untouched — asserted as `not clip_text`, the flags rather than a
  width), a MEDIUM one past the bound that the row can still pay for in full, and the reported LONG
  one that no strip can. Without the medium row, "it grows" is unprovable; without the short one, the
  bound could be a fixed column trimming every row in the game with every other claim still green.
  - ⛔ **The short one's key must not be one `HudEventVocab.DETAIL_KEY_HIDDEN` swallows** — the first
    draft used `count=1`, which `detail_phrase` renders as `""`, so the row drew no detail label at
    all and the claim was about a control that was not there.
  - ⛔ **ITS CLAIM IS BOTH THE FLAGS AND A WIDTH, and the width half was lost once to a probe that
    reported the wrong units.** A `Label` built detached shapes at the DEFAULT theme's font SIZE
    rather than at its own override (33px against 27 for `Cold`), so `drawn >= probe natural` was
    failing a row that is in fact whole and the claim was weakened to `not clip_text` alone. The
    probe goes through `EventDockPanel.natural_label_width` now — which asks the font instead of the
    label's theme cache — so it reports drawn pixels and the width claim is back beside the flags.
    **A probe in the wrong units does not merely fail; it also silently weakens whatever assertion is
    rewritten around it.**
- **`event_dock_long_detail_floored` is where the growth half has to prove it did not undo the fix.**
  Squeezed to `MIN_STRIP_WIDTH` there is no slack to hand out, so the phrase must fall back on
  `DETAIL_MAX_WIDTH` exactly — asserted as trimmed AND as not below the bound, with
  `_assert_card_within_root` beside it. A growth flag that let the label demand its natural width
  again would put the card straight back outside its strip.
- **The reported row's own LABEL is asserted beside its phrase.** The two share the shortfall in
  proportion to what each wants, so the label keeps most of its text on the same row that grew the
  phrase — the failure a constant stretch ratio produces in one direction or the other.
- ⛔ **A FOURTH ROW MOUNTS A `Work tab` LINK, AND WITHOUT IT THE WHOLE BLOCK PROVED NOTHING ABOUT THE
  ROW THE BUG CAME FROM.** `_make_event_row` appends that `Button` only for a `status=trimmed|lapsed|
  pruned` detail carrying a `band=` id; the three rows above are a `died` and two `hunt`s, so no
  fixture here mounted one and `ROW_FURNITURE_WIDTH` was measured — and passed — on rows with no link
  column at all. Staged now as the reported shed line, with two preconditions that keep it honest: a
  count of the links actually mounted, and `ROW_FURNITURE_WIDTH` asserted to cover the widest
  furniture measured on the bar. **The second is the derivation itself as an assertion** — without it
  a row that grows a control fails on a card rect three claims later, naming the symptom rather than
  the stale term. A third claim pins the link drawn in full at the floored strip, which is what the
  budget now pays for.
- **`compose_sheet_over_event_dock` needs BOTH the frame and the index claim.** Stacking order is a
  property of the CanvasLayer indices rather than of any rect, so no pixel comparison can state it;
  and an index claim alone passes on a sheet that never opened. It also asserts the sheet's parent and
  that `_sync_to_viewport` still resolves to the whole viewport under the new parent — the identity
  transform of a sibling `CanvasLayer`, checked rather than assumed.
- ⛔ **THAT FRAME PUSHES RAW ZERO INSETS FOR ITS ONE STATE, AND WITHOUT THEM IT IS A PICTURE OF
  NOTHING.** The sheet floats beside the selection card, i.e. over the HUD's LEFT COLUMN — which is
  exactly the band `_preview_push_event_dock_insets` keeps the bar out of — so at the ordinary insets
  the two never share a pixel and the eye cannot judge which is on top. Zero insets are a real
  configuration (a dock with no HUD column beside it); the state restores the ordinary ones after, and
  a rect-intersection claim rides beside the frame as its non-vacuity guard.

**A clean run is 370 frames / 1589 `PASS`, exit 0 — RE-MEASURED.** (It was 370 / 1581 before
`docs/plan_standing_upkeep.md` §4.9 item 12c collapsed the plant web's crew noun. **NO FRAME MOVED IN
COUNT and forty-odd moved in CONTENT** — every plant compose sheet, every plant work row and every
`Harvesting kit` face — which is the shape of a rename: the states are the same states, reading
differently. The eight `PASS`es are five liveness claims on `_assert_plant_crew_noun` and the two
`_assert_plant_crew_noun_is_rung_blind` claims that carry the collapse itself, less one net from
`improvements`' inverted card-side block, which lost the *"names the good"* pair and gained a
containment claim, a figures claim and the liveness one.) The
figure this replaced (`365 / 1535`) had itself gone stale by five frames and forty `PASS`es before the
crafting stock-row change above touched it, which is the second time in a row that has happened —
**read this number as a date stamp, not as a live total**, and re-measure rather than trusting it. The
figure BEFORE that one (`357 / 1432`) had gone stale in the tree well before the arc below touched it,
which is exactly the trap the rule is written for: **never sum a delta onto a recorded figure — take a
baseline run first.**

### The frame diff that attributed this arc, and the one frame it could not explain

Nineteen frames differ from the pre-arc tree, and **the diff was taken against a build of the pre-arc
CLIENT rather than against the previous run of this one** — the `git show`/swap/re-run method above.
That mattered: an intermediate build of this arc was itself trimming rows, so a diff against it read
the fix as the regression.

Sixteen of the nineteen differ by **2 to 20 pixels** — sub-pixel antialiasing where a bounded label's
box rounds a pixel differently; the glyphs are complete and in the same place. Three are real and two
of those are the arc landing: `event_dock_band_founded` (a two-sentence prose refusal, 918px drawn, now
sharing its shortfall with its label instead of overflowing the card) and `event_dock_narrow_band`
(the deliberately squeezed 496px band, where the bound is meant to bite).

⛔ **THE THIRD IS `forage_take_default`, AND IT IS NOT A DETAIL-BOUND FRAME AT ALL** — its compose
sheet's card renders ~56px taller than its content. Attributed by isolation and then by measurement,
and the first attribution was WRONG: with the sheet parented back onto the HUD and the compose
`CanvasLayer` still created the frame differs identically, which pointed at frame timing and so at
`ComposeSheet.refit`'s discarding `_fit_pending` guard. That guard was a real defect and is fixed — it
coalesces now (`labor-ui.md`) — but the frame measures **identically before and after** it (card 766,
content 683, chrome 83): both of that state's two opens land before the first fit resumes from its
await, so the one fit that runs already measures the final body. The actual cause is `refit`'s own
`chrome` term over-counting the header row, which is left as a separate decision. **The lesson is the
method**: a plausible mechanism that explains the symptom is not the cause until the fix for it moves
the number.


## The material half's frames and the five it retired (`docs/plan_standing_upkeep.md` §4.9 item 12)

**MEASURED, BEFORE AND AFTER, ON THIS TREE**: `366 / 1518` → `365 / 1535`, exit 0 both times. Net one
frame FEWER and seventeen more `PASS`, and the arithmetic is worth writing down because the two move in
opposite directions.

**FIVE FRAMES RETIRED WITH THE BAND'S `Gear` ROW** — `band_kit`, `band_kit_expanded`, `band_kit_bare`,
`band_kit_short`, `band_kit_forage_short` — because the row they photographed no longer exists and the
`BAND_DISCLOSURE_KIT` caret they were driven through is no longer registered. **Their popover claims
survive whole**: `chapters/band_expedition.gd`'s kit block is DRIVEN and PNG-LESS now, reading
`DisclosureController.kit_breakdown_lines` directly, which is what `chapters/compose_rungs.gd`'s own
gear claims already did. Four ROW claims went with the frames (the row states all three kits; a spent
kit reads as a WORD in danger ink; the row states the spears' shortfall on their own face; the baskets
state theirs against the forage head count) plus the two `_assert_faction_kit_counts_the_short_band`
claims — the faction `Kit` row being retired too.

**FOUR FRAMES ADDED**: `band_standing_bill` and `band_standing_bill_expanded` (the `Upkeep:` row and
its open drill-down, `chapters/band_expedition.gd`), `tile_meter_blocked_materials` (the stuck reason
that NAMES a good, `chapters/improvements.gd`) and `event_dock_material` (both `kit_life` seams beside
the `material_shortfall` Alert, `chapters/event_dock.gd`).

⛔ **THE PER-CHAPTER `EXPECTED_CHECKPOINTS` FLOORS MOVED AND WERE RE-MEASURED, NOT ADJUSTED BY THE
DELTA.** `band_expedition` 101 → **102**, `improvements` 174 → **186**, `event_dock` 165 → **186**. The
first of those is the one that shows why: it lost eleven checkpoints and gained ten, and its recorded
floor was already under its real count, so a delta applied to the old number would have set a floor the
chapter could fall THROUGH silently. Raise the const to an impossible number, run once, read the
reported count, set it exactly.

**`band_panel_queue_expanded_autoscroll.png` IS NON-DETERMINISTIC BETWEEN RUNS** and is not attributable
to any change — measured directly: two consecutive runs of the unmodified tree agree with each other
and differ from a third. Anything comparing frame sets across runs has to expect it.

## The material-only row's two sentences (`chapters/crafting_bench.gd`)

Six `PASS` and NO frame, all six riding the existing `crafting_panel` save. The behaviour and the
claims' pairing are `crafting-panel.md`'s; what belongs here is the tally and the fixture correction.

**MEASURED, BEFORE AND AFTER, ON THIS TREE**: `370 / 1591` → `370 / 1597`, exit 0 both. The
before-figure was taken as a RUN on the merge base, not by subtracting six from the after — the
"never sum a delta onto a recorded figure" rule above — and it happened to agree with the recorded
`370 / 1589` to within the two `PASS`es that had drifted in un-recorded since, which is the usual
outcome and still not evidence.

**`crafting_bench`'s `EXPECTED_CHECKPOINTS` moved 143 → 149, RE-MEASURED rather than adjusted by the
delta** — the const was raised to an impossible number, the run read back `reached 149 checkpoints`,
and 149 was set exactly. Six claims added to a floor of 143 happens to be 149 here; that arithmetic
is a coincidence to check, never a substitute for the read.

**NO FRAME MOVED IN COUNT AND ELEVEN MOVED IN CONTENT** — every state built on `_crafting_band()` /
`_two_tier_band()` renders the ledger's one stock row, whose second line and whose refusal both
changed their wording. That is the shape of a copy fix: the same states, reading differently.

⛔ **THE FIXTURE'S OWN CORDAGE REFUSAL WAS STAGING A PAYLOAD NO SIM CAN RESOLVE.** It read
`Reed .70 → strong` — a `→ grade` tail on a material-only recipe — which `RecipeDef::grade_for` has
stopped producing, so the chapter was asserting against a shape the server cannot send. It is
`Reed, no loom` now, and the `baskets` KIT offer's existing `Reed, no loom → fair` became the
vacuity guard beside it: same bench material, same absent loom, one word of difference.

## The hint's pen column was a tautology (`chapters/compose_rungs.gd`, PR #589 review)

Three `PASS` REMOVED and no frame moved: `370 / 1597` → `370 / 1594`, exit 0 both, each figure a run
on this tree rather than a delta. The claims deleted were the two "penned" hint readings and the
wild/penned precondition beside them; the two WILD readings, which are real, are kept, and the
assertion is `_assert_the_hint_states_each_kits_own_items` — the name it now earns.

⛔ **THE DELETED CLAIMS COULD NOT FAIL.** They evaluated `KitRoster.tier_hint(kits, kit, band,
JOB_HUNT)` — the BYTE-IDENTICAL expression the wild pair uses, `tier_hint` having lost its source
parameter when issue #543 deleted `EquipmentStat::PenCarry` — while the corralled twins they were
named for were built and handed nowhere. A block whose stated purpose was *"the fence moves NOTHING"*
would therefore have stayed green under exactly the regression it named: a source parameter re-added
to `tier_hint`, or a `QUARRY_CORRALLED_KEY` read off the band dict.

**FOR THE HINT, THE INVARIANT IS NOW STRUCTURAL RATHER THAN TESTED**, and that is the trade this
records: restoring a real penned reading requires `tier_hint` to take a source again, which is the
change the assertion was about. `tier_hint` has no argument a pen can arrive through, so a penned
reading is not expressible; **a future source parameter on it is the signal to rebuild the
wild/penned equality**, and the assertion's own docstring says so at the point of edit. The fence
still crosses real code in `_assert_a_pen_prices_on_the_hunters_carry`, which drives
`DrawerComposeController._hunt_priced_herd` over `_corral_twin`'s pair — that helper survives, with
one caller instead of two.

**`compose_rungs`'s `EXPECTED_CHECKPOINTS` moved 90 → 104, RE-MEASURED**: the const was raised to an
impossible number, the run read back `reached 104 checkpoints`, and 104 was set. The declared 90 was
a STALE floor — the chapter had been reaching 107 before this change, so subtracting the three
removed claims from 90 would have left the guard 14 checkpoints slacker than the file it audits.

## The plant rung's material price, authored (the material half of upkeep)

Five `PASS` and NO frame, appended LAST in `chapters/improvements.gd` — it renders nothing and mounts
nothing, so no capture after it moves. `EXPECTED_CHECKPOINTS` 204 → **210**, RE-MEASURED rather than adjusted by the delta: the declared
204 was already one under the chapter's real count.

**It exists because `patch_crossref_guard` going green cannot tell a working wiring from a dead one.**
That guard proves `cultivation_upkeep_material_demand` / `field_upkeep_material_demand` reach
`tile_info` (`harness-headless-guards.md`), and on the shipped ladder they arrive carrying `[]` — no
plant rung declares a material — so a key that arrives empty renders exactly what a key that never
arrived renders. The state authors a plant material upkeep instead, which is this repo's own
convention for a path the shipped config does not reach (`equipment.json`'s bronze tier and
`materials.json`'s `varieties` are covered the same way).

**THE CLAIM IS WHAT THE ROW SAYS, by EQUALITY against a clause composed through the shipped formats**
— never that an aside exists, and never a `contains` a clause missing its material half would still
satisfy. `RungLadder.track` is prefix-parametric, so it is asked at `patch_`, which is the spelling
this arc's cross-ref put those keys in.

- **The two rungs carry four DIFFERENT figures** (2.0 / 0.35 work·stone against 4.0 / 0.6), so a
  producer reading one rung's key for the other row fails rather than passing on a coincidence. Each
  clears `MATERIAL_FLOW_MIN` / `UPKEEP_WORK_MIN`, so nothing is suppressed as noise.
- **The negative IS the shipped config's own state**: the same rung with an empty quote states its
  work term and no material clause at all — an empty list is *this rung eats nothing*, never a
  `0 stone`. Without it, a client appending the good's word to every rung passes above.
- **The rendered half is asserted on a DETACHED column.** The `⌃` track is a `PopupPanel` the BAND
  PANEL floats, and mounting one into this harness's long-lived `HudLayer` would change every frame
  after it; `RungLadder.build_track` into a freed `VBoxContainer` proves the clause reaches a real
  `Label` — the half a producer claim alone cannot make — and touches no scene.

**The other three of the arc's seven were already covered** and are worth not re-covering:
`improvements.gd`'s existing block authors `patch_upkeep_material_demand` /
`patch_upkeep_material_supplied` and asserts the tended row wears `⚠ slipping`, and
`tile_meter_blocked_materials` authors `patch_build_material_cost` and asserts the refusal NAMES the
good in DANGER ink. Those renderers were correct all along — only the cross-ref feeding them was
missing, which is why the fix moved no frame: a clean run is **1614 `PASS`, exit 0**, five more than
before with the same frame set.

## `chapters/supply_network.gd` — which link the goods crossed, as a UX PROTOTYPE (issue #548)

**Appended LAST in `CHAPTERS`**, after `knowledge_panel`, so no existing frame moves. Four frames and
seventeen assertions (`EXPECTED_CHECKPOINTS` **21** — frames count too; RE-MEASURED off the run). It
ends by handing the reference band back, so a chapter appended after it starts where every other one
does. The behaviour is `band-readouts.md`'s; what belongs here is the shape of the fixtures and of the
drive.

**IT IS A PROTOTYPE AND ITS PAYLOAD IS NOT ON THE WIRE** — see `band-readouts.md` → "The fallback, and
what is on the wire". The states exist to be LOOKED AT before the sim half is written; the assertions
are what stop them being four plausible pictures.

| frame | what only IT can say |
|---|---|
| `supply_quiet` | **the negative case** — a camp where nothing crossed either link carries its ordinary flows and NOT ONE transfer row, on EITHER account. Claimed on the `⇄` glyph rather than the two labels, so a row naming some third link kind cannot slip past |
| `supply_food_links` | both kinds as their own rows among `Gathered` / `Hunted` / `Consumed`, and the breakdown is rows ONLY — no sentence, no footer, no radius |
| `supply_food_route_both_ways` | both directions on one link netted into **ONE** `⇄ Trade route +1.00` row, with BOTH gross figures asserted absent — plus, PNG-less, the consequence of netting: a kind whose arrivals and departures cancel exactly renders no row |
| `supply_fodder_links` | the Fodder popover keeps `Grown` / `Pens` and gains the identical pair at its own one-decimal resolution — held beside `supply_food_links`, which is the consistency proof |

**THE CONSISTENCY CLAIM IS MECHANICAL, not two frames side by side.** `_transfer_phrases` strips both
ledgers' rows to the `⇄` phrase, dedupes and sorts, and compares the ARRAYS: if one account ever
re-words the link the other states, that fails. A pair of pictures cannot say it, and two accounts
wording one event two ways is a defect this readout has already been rejected for once.

⛔ **AND `_prose_lines` IS THE GUARD AGAINST THE OTHER REJECTION.** Every breakdown row carries the
shared indent, so any produced line that does NOT is a sentence, a footer or a paragraph. Two earlier
cuts explained themselves in prose and were rejected for it; prose is invisible to a `contains` test
for row text, so the claim is made over the line SHAPE instead.

**THE FIXTURE MOVES HAY DIFFERENTLY FROM GRAIN, DELIBERATELY** — same turn, same two links, different
amounts and one different direction. A ledger reading the other account's figures then fails here
instead of looking plausible on every frame.

**AND THE LEGACY PATH IS ASSERTED, PNG-LESS.** The fallback that keeps today's wire rendering today's
two generic rows is ONE line in `_link_transfer_lines`, and is exactly the kind of line a later
simplification deletes — so the chapter strips the four food keys off its own fixture and claims both
halves: the generic pair renders, and no link kind it was not told appears.

**No existing frame changed and no frame count moved but this chapter's own.** The generic pair is
preserved verbatim, so `chapters/trade.gd`'s four assertions on
`DetailFormat.FOOD_LABEL_TRANSFER_RECEIVED` / `_SENT` still hold unaltered.

**A clean run is 396 frames / 1763 `PASS`, exit 0 — RE-MEASURED**, as this file's own rule says. The
last figure recorded above was `370 / 1594`; the gap is drift accumulated un-recorded, exactly as it
has been every previous time. Measure; do not sum.
