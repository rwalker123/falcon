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
(`improvement_offered_gated`, re-fixtured onto a Stressed patch with Cultivation known), and
`forage_sow_locked` pins both halves at once by staging BOTH kinds of reason and asserting the
ground's refusal leads while the knowledge line appears nowhere.

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
| `tools/ui_preview/fixtures_*.gd` | Pure `static func` fixtures shared by two or more chapters — `base` (the primitives the other three build on), `band`, `herd`, `forage`, `tile`, `world` |
| `tools/ui_preview/node_query.gd`, `readouts.gd`, `compose_vocab.gd`, `input_probe.gd` | Shared `static` helpers: finding a control by identity, reading values back out of rendered text, the compose spine vocabulary, and driving real pointer input through `Viewport.push_input` (the canvas→window conversion, a hover, the two gestures, a wheel notch, a press-and-cancelled-release click) |

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

**A clean run is 296 frames / 824 `PASS`, exit 0.** (It was 827 before arc #527 retired the
`trade_goods` yield axis — three claims went with the account; **no frame was added or removed**, and
the crop-picker frames listed under `land-readouts.md` → "WHAT A CASH CROP PAYS, PER MATERIAL" moved
in place as their basket rows swapped a trade scalar for per-material clauses.) **Twelve** of those frames are the Materials &
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
