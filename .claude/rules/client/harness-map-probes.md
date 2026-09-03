---
paths:
  - "clients/godot_thin_client/tools/map_preview.gd"
  - "clients/godot_thin_client/tools/blend_probe.gd"
  - "clients/godot_thin_client/tools/map_preview.tscn"
  - "clients/godot_thin_client/tools/blend_probe.tscn"
---

<!-- Split out of .claude/rules/client/test-harnesses.md, which was itself extracted from
     clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e.
     The pseudo-table cells this file carries were re-wrapped at 100 columns; no wording changed. -->

# The `map_preview` and `blend_probe` harnesses

The two map-side render probes: marker//overlay states and the edge-blend renderer.

## `tools/map_preview.gd` / `.tscn`

Dev-only **MapView** preview harness (HUD-only ui_preview's companion): instances the real
`MapView`, feeds a canned `display_snapshot` + selects a band, and dumps PNGs (`map_*.png`) to
`ui_preview_out/`. Verifies the selected-band labor highlights (work-range ring / worked forage
tiles / hunted-herd ring+link; scouting draws no disc — it extends sight in the fog), the
terrain/blend states, and the **rivers** state (`map_rivers*.png` — hex-edge Minor/Major rivers +
the NavigableRiver terrain chain, incl. `map_rivers_join.png`: a zoomed, hex-anchored close-up of
the trunk HEAD, where two tributaries hand over at corners — the frame the `river_inflow` spurs are
judged on — `map_rivers_head_minor.png`: a second navigable head fed by a **Minor tributary only**,
the frame the HEAD TAPER is judged on; **`map_rivers_midchain.png`**: a Minor tributary handing over
at a vertex of a **MID-CHAIN** trunk hex (upstream *and* downstream channel exits) — the frame the
head-taper's **exit-count gate** is judged on: the trunk must hold **constant full width through the
junction** (any pinch-and-swell at the hex centre is the HOURGLASS the gate exists to prevent) while
the spur still reaches its vertex. The case the drainage-network rewrite created and the fixtures
never had; **`map_rivers_notch.png`**: a chain HEAD whose tributary hands over at its BOTTOM vertex
(corner 1) and whose single channel exit is the ADJACENT SW side — both flanking the same corner,
the geometry the old centre-hub routing drew a NOTCH / inverted-V on. The direct
inflow-corner→exit-midpoint routing must draw ONE smooth tapered channel with no notch (zoomed via
`NOTCH_ZOOM_IN`); **`map_rivers_lake_alongside.png`**: a one-hex `inland_sea` ringed by three
navigable hexes whose `river_channel` exits all run along their own chain / out to the sea — NONE
into the lake (the @21,61 case). The shore pass's per-edge MOUTH test must draw the lake's FULL
beach/foam ring INCLUDING the navigable-adjacent edges (the old "any navigable adjacency" exclusion
ate them); the true mouth into the eastern sea in the same frame STAYS open; and
`map_rivers_web.png`: a solid CLUMP of adjacent navigable hexes with `river_channel` winding through
it as ONE snake — the **regression guard** for the spider-web bug, since the other river fixtures
build their chain by hand and are paths by construction, which is why the harness never caught it.
Any cross-link/triangle there = the terrain-inferred arm rule is back) and the **starving-pen
distress badge** (`map_herd_starving` — a starving pen beside a fed one, **plus a third starving pen
(boar)**: every species now has bundled sprite art, so all three pens are `FaunaSprites` markers and
the frame proves the ring/badge reads over a sprite — it no longer exercises the emoji fallback at
all) and **`map_fauna_sprites`** (the SPRITE ROSTER: one herd per bundled-art ALIAS GROUP on its own
hex — the only frame where the whole art set is judged at once for swapped/clipped/fringed sprites.

**The four cervids lead the list, adjacent** (issue #439): Red Deer / Wild Elk / Wild Reindeer /
Desert Gazelle all drew `deer.png` for the life of the roster, and what hid it was that no frame
ever stood them side by side. It is **rows of eleven from origin (3, 4)** (21 entries, so two rows
with the second one short) — the roster outgrew a single spaced row across `GRID_W` (16), and `seal`
+ `catfish` were absent entirely from a frame whose whole job is coverage, as later were
`steppe_runner` + `marsh_grazer`, and later still `snow_hare` + `ibex` + `grouse`.
It STARTS on row **4, not 5**, because the band camp stands on `(BAND_X, BAND_Y) = (8, 6)` and a
roster entry landing there renders STACKED under the camp marker instead of alone at true marker
size — which is what happened to the Jungle Fowl when the origin was 5.

**Eleven columns keep the whole roster on rows 4-5, so the camp's row 6 is EMPTY** — a stronger
property than dodging one column on an occupied row, and the reason to widen rather than shift when
it next overflows. The span is columns 3-13, inside the known-surviving crop.

> ### ⛔ THE PREDICTED COLLISION HAPPENED, AND THE HARNESS DID NOT CATCH IT
>
> This section used to say *"past 20 entries the wrap reaches column 8 on row 6 and the collision
> returns, so move the origin or widen `FAUNA_ROSTER_COLUMNS` at that point rather than discovering
> it in a frame."* The roster then went to 21 in one commit and **entry 20 (Marsh Grazers) landed on
> `(8, 6)`** — `col = 4 + (20 % 8) = 8`, `row = 4 + (20 / 8) = 6` — rendering stacked under the camp
> marker in the one frame that judges the whole sprite set.
>
> **It shipped because `map_preview` asserts no marker POSITIONS.** The run completed, printed no
> `FAIL`, and exited 0, and that clean status was cited as evidence the new art rendered correctly.
> This is `test-harnesses.md`'s *"the exit status IS the verdict"* one turn further out: the status
> is the right thing to read for an assertion that exists, and says nothing about a defect no
> assertion covers. A prediction written in a rule file is not a check — nothing enforced it, so the
> arithmetic had to be redone by hand in review to find it.
>
> **The next recurrence is index 27 (roster size 28).** If a guard is wanted rather than another
> prediction, the natural one is a `PASS` line in the fauna-sprites state asserting that no roster
> hex equals `(BAND_X, BAND_Y)` — it is pure arithmetic over the constants and needs no pixels.

**This frame does NOT prove coverage** — it enumerates a hand-written CLIENT-side list, so it is
blind to a species that list has never heard of; that claim belongs to `cargo xtask
fauna-icon-guard`, which checks against the sim's `fauna_config.json`. What the frame is for is
judging art that EXISTS, at true marker size. On this state's `DEFAULT_CANVAS_SIZE` the cover-fit
crops the **columns**, not the rows — all twelve rows are on screen and roughly cols 2–14 survive —
so a third row is affordable and a wider row is not) and its food twin **`map_site_sprites`** (the
same idea for `SiteSprites`: one food site per bundled art key in one row, including a `game_trail`
site — which must draw the fauna DEER — and an unknown module, which must fall to the `default`
sprig; the riverine fish↔reeds pair is judged separately on `map_riverine_split`, since one module
drawing two icons needs two terrains, not two hexes) Also state **"pasture"** (`map_pasture.png`) —
the **graze distribution** on an earthlike-shaped fixture map under the `pasture` overlay channel
(see Overlay Channels): the frame Phase 2a exists to be judged on (is prairie really pasture? is the
alluvial fallback dominant? are glacier/lava/water distinct from merely-poor ground?). It stages a
**woodland block a live map does not have** (the palette thins forest out), sizes the window to the
grid's aspect (MapView is **cover-fit**, so a mismatch CROPS exactly the distribution you came to
see), and **saves the legend as its own frame** (`map_pasture_legend`, the picker's popover — it PRINTED the dict for as long as this harness had no surface to draw it into). Also state
**"forage"** (`map_forage.png`) — the **human-food distribution**, the SAME earthlike fixture
painted from the human-food table under the `forage` channel, so it compares tile-for-tile with
`map_pasture` and the two food webs' divergence reads directly (forest/river rich on forage / poor
on pasture; the shelf column glows on forage where it is barren on pasture) without a server:
`scripts/preview.sh res://tools/map_preview.tscn`.

Also the four **TEMPERATURE states** (issue #614), which are where the two-reading treatment is
judged. The fixture is a latitude gradient with a **cold pocket** dropped into the survivable middle,
so one frame carries a lethal cold tail, a lethal heat tail, and a closed lethal RING — a contour
that is a boundary rather than a stripe. It publishes per-tile °C on the tiles and NOTHING else: no
`channels` entry, so what is painted has to be the channel `MapView` synthesized, and the mortality
model rides in the snapshot's `overlays` as the four `survivability_*` scalars, driving the real
`_ingest_overlay_channels` adoption rather than seeding `TileSurvivability` behind the renderer.
`map_temperature` is the whole field; `map_temperature_pocket` crops to the ring; `map_temperature_legend`
opens the picker's popover on it; `map_temperature_farzoom` is the LOD half, on a grid large enough
that the COVER fit lands under the hatch's detail gate — the hatch is gone and the contour alone is
still drawing the survival lines.

**Sixteen assertions ride them, because the frames cannot carry any of it.** Two on the roster (the
channel is offered on a world with tile temperatures and NOT on one without — the negative asked
first, since a positive alone passes against a row that is simply always present); three that the
legend's swatches are what `_tile_color` paints the map's own coldest and warmest tiles and that its
rows state real degrees (the map-and-legend-share-one-ramp rule); two that its Lethal row follows the
sim's model, asked by RETUNING `TileSurvivability` and re-reading the row, which a transcribed range
cannot answer; and three PIXEL probes in a box around the pocket — hatch and contour present close
in, hatch gone and contour still present far out, with the fit radius asserted against the gate as an
explicit premise first. The pixel probes are boxed rather than whole-frame because a targeted box
makes the ABSENCE claim sharper (the hatch is asked for exactly where a lethal hex is being drawn) as
well as thousands of times cheaper in GDScript. The last six are the LETHAL SWATCH's: that the row
asks for a hatched swatch rather than a solid block, that its colour, angle and edge are the very
`MapView` constants the map's own passes draw with (a PNG shows the swatch but cannot say the lines
were struck from the same constants — a transcribed copy renders identically and drifts later), and
that the opt-in did not leak, every other row here and on a channel that predates the kind keeping
the default solid.

Sabotage-verified in four runs, each restored: removing the two draw calls fails the three pixel
claims; forcing the hatch past its gate, hardcoding the legend's degrees and making
`has_temperature_data` answer true fails the LOD, retune and negative-roster claims (and three
PRE-EXISTING roster assertions besides, which is the registry's own guards policing the new row);
painting the swatches from the forage ramp and formatting the rows as fractions fails the three
legend-parity claims; and making `has_temperature_data` answer false fails the positive-roster claim
and cascades through the rest, the channel then being unpaintable. The swatch's six took two more:
reverting the row to a solid kind with hand-written colour, angle and edge (4 fail), and leaking the
hatched kind onto a temperature ramp row AND a forage row (2 fail, one per leak guard).

Also the four **ANNOTATION states**, added by the
`AnnotationRenderer` extraction because that family had **no fixture at all** and so no refactor of
it could be pixel-checked. **Three remain; there were four.**

`map_trade_overlay` and its whole fixture (`_trade_link` / `_trade_links` / `_snapshot_trade_overlay`
/ `_tile_entity` / `_entity_tiles` and the `TRADE_*` consts) went with the trade-link overlay itself:
the sim publishes no link network, so the frame covered a draw that is handed the empty set on every
live frame (`overlay-channels.md` → "RETIRED", `docs/plan_contact_and_logistics.md`). Issue #232's
route-network overlay is what earns a frame back here, and it will need its own fixture rather than
this one restored — the retired one addressed endpoints by tile ENTITY through `tile_lookup`, which
is why it was the only flat-backdrop state publishing a `tiles` array.

**`map_crisis_annotations`** (all four shapes the draw can produce in one frame: a
multi-hop path in the `PackedInt32Array` wire form, a multi-hop path in the Array-of-`[col,row]`
form, a single-tile halo+core marker, and a single-tile marker with an unknown severity — the
`CRISIS_COLOR` fallback — and no label; the `crisis` channel is selected AFTER the snapshot, because
`display_snapshot` clears the active overlay every time); **`map_terrain_highlight`** (the Terrain
tab's highlight tool on the four-band biome map, so the MATCHED band and the three UNMATCHED ones
are both in frame); and **`map_routes`** (three multi-hop turning order paths covering
`faction_colors`' INT key, its STRING key and an unknown faction's amber default, plus a
one-waypoint order the draw must bail on). They run LAST, each clearing its own state afterwards,
and they switch the canvas back to `DEFAULT_CANVAS_SIZE` (the river states leave the pasture aspect
pinned).

**THESE FOUR PROVE "UNCHANGED", NOT "CORRECT"** — they were written AFTER the code they cover, so
they encode current behaviour including any bugs in it. That is exactly the right tool for a
decomposition safety net and the wrong one to mistake for a correctness test; the same caveat
applies to any fixture added to protect a refactor rather than to pin a decision.

**It PINS ITS CANVAS AND WAITS FOR THE WM** — the `blend_probe` treatment (`_pin_canvas` /
`_ensure_canvas` from `_settle` / the `_capture` geometry guard / `CANVAS_PIN_MAX_FRAMES`), because
`project.godot` opens MAXIMIZED and macOS applies — and RE-applies — that asynchronously, so the
bare `get_window().size = …` + two `process_frame`s it used to do in `_ready` was a RACE it mostly
LOST: measured on a clean run, **33 of 41 saved frames came out at the monitor's 3840×1050 instead
of the intended 1000×800**, and the four earliest states flipped between the two from run to run.
`_canvas_size` (not a const) tracks the per-state canvas, so the aspect-matched pasture/forage
states still switch to `PASTURE_WINDOW_SIZE` via `_set_canvas` — MapView is cover-fit, so a
mismatched aspect CROPS the very distribution those states exist to show — and, as before, never
switch back.

**`content_scale_size` / `content_scale_factor` are deliberately NOT pinned here** (blend_probe pins
both for its 1:1 canvas): `project.godot` stretches `canvas_items` with an `expand` aspect, so
pinning them would re-project EVERY frame — a mass pixel change, not a race fix. That is also why
the `_capture` guard measures the **window-sized canvas** rather than copying blend_probe's
viewport-rect test: with content scaling live the captured image matches the WINDOW (1:1 measured),
while the viewport's logical rect is the `expand` projection and matches neither (a 1000×800 window
reports a 1920×1536 rect), so a viewport-rect guard here could never be satisfied.

**It also FREEZES ANIMATION TIME** (`Engine.time_scale = 0.0` in `_ready`), which is what closes the
last 14: with the canvas pinned, the only remaining run-to-run difference was genuinely ANIMATED
content — the 11 `map_rivers*` frames (the shader's `TIME × river_flow_speed` channel scroll) plus
`map_quarry_targeting` / `map_expeditions` (the `delta`-driven targeting and awaiting-expedition
pulses).

**The frame set is consequently a STRICT BIT-IDENTITY REFERENCE — 72/72 frames byte-identical across
runs** (verified over consecutive runs, most recently while fixing the herd trail's seam handling —
which added no states, the count having read a stale 65 since some run before that, as it had read a
stale 62 before that; note the harness has 54 `_save` CALL SITES and saves 72 frames, because
several states save inside a loop, so the two numbers are not meant to match), which is the property
the decomposition passes rely on: a frame that varies cannot be pixel-diffed to prove a refactor
changed nothing.

**The cost is that every animation renders at a FIXED PHASE** rather than wherever the clock landed,
so those 14 frames moved once when it landed (a deliberate re-baseline; the other 42 are
byte-identical with or without it).

**Freezing at phase 0 erases nothing, and that was checked against the draw code before it was
taken** — both pulses use the `0.5 + 0.5 * sin(t)` idiom, so `t = 0` is the MIDPOINT, not zero
amplitude (the awaiting ring draws at 1.46× radius / 0.65 alpha, the quarry glow at 0.60× / 0.675),
and the river's phase is a UV OFFSET whose coverage alpha is a purely geometric `smoothstep`, so
channel, banks and taper are untouched.

**If a future animated element is added, re-check it the same way**: an amplitude term (`A ·
sin(t)`) WOULD vanish at phase 0, and a frame that is deterministic because its subject disappeared
is worse than one that varies.

**A THIRD determinism source was the DEVELOPER'S OWN PREFS FILE**, and it is the
`band_panel_preview` / `ui_preview` config-isolation bug wearing a different hat: `ClientSettings`
is an autoload that has already loaded the real `user://client_settings.cfg` by the time `_ready`
runs. When it was found, `MapView.zoom_step` scaled `ZOOM_BUTTON_STEP` by `zoom_speed_multiplier`,
so every state reaching its zoom through `zoom_step` (the three
`map_rivers_join`/`_head_minor`/`_midchain` close-ups and both `map_swim_*panzoom` frames) rendered
at a DIFFERENT zoom on a machine whose Options slider had been moved: measured at the slider's max
(3.0), `RIVER_JOIN_ZOOM_STEPS × 0.5 × 3.0` asked for 4.5 and was CLAMPED by `MAX_ZOOM_FACTOR`, so
those frames silently tracked the cap rather than the 3 steps their const names.

**That mechanism is GONE** — the rail is now a snapped ladder that ignores the slider entirely (see
`map-renderers.md` → Zoom rail), and no state here currently reaches a scaled input path — so the
pin is now PRECAUTIONARY rather than load-bearing for those five frames.

**It stays**, by the same "state the condition, never inherit it" rule as the fog line one row
above: the next state to use a continuous zoom path would silently re-acquire the bug. `_ready` pins
`zoom_speed_multiplier` / `pan_speed_multiplier` to their `*_DEFAULT`s by assigning the MEMBERS
DIRECTLY — never the setters, which `_save` over the player's own file.

**A FOURTH source was the OS CURSOR**, the treatment `blend_probe` already carried and this harness
did not: it renders in a REAL window, so `MapView._unhandled_input` picked up the pointer and drew a
faint HOVER hex outline into whichever frame was rendering. Measured here, `map_riverine_split` came
back with a brightened hex outline on ~1 run in 5 — **319 pixels at a max channel delta of 37**, on
a DIFFERENT hex each time, i.e. far too small to catch by eye and easily large enough to break the
byte-diff. `_ready` now calls `_map.set_process_unhandled_input(false)`; three consecutive runs
after it are 62/62 identical.

**The lesson generalises: a harness that renders in a real window must drop input, not try to park
the pointer.** Also state **`map_max_zoom`** — the OTHER END OF THE ZOOM RAIL, added with issue
#375's raise of `MAX_ZOOM_FACTOR` from 4.0 to 7.0. Every other state renders at the cover fit
(`MIN_ZOOM_FACTOR`), so nothing judged the cap; this one sits at exactly `MAX_ZOOM_FACTOR`
(referenced through `MAP_VIEW`, never a literal) with textured terrain + edge blending, the worked
band's per-source yield labels, and BOTH marker families, then centres the band hex — at the cap the
viewport holds a handful of hexes, so an unpanned frame is an arbitrary corner with none of the
subject in it.

**The GRID is the load-bearing choice**: `zoom_factor` is a multiple of the COVER FIT, so what 7×
means in pixels is decided by the grid, and `MAX_ZOOM_GRID` therefore mirrors `MapSizes`' SMALLEST
offered map (Tiny, 56×36) — the smallest map has the largest fitted radius, hence the most magnified
terrain texture the rail can reach in a real game. A bigger grid would flatter the cap; this
harness's own 16×12 grid would slander it (one hex comes out WIDER THAN THE VIEWPORT, so every label
and marker falls off-frame and the state judges nothing). A `push_warning` fires if
`last_hex_radius` drifts from `base_hex_radius × MAX_ZOOM_FACTOR`, so the state cannot silently stop
sitting at the cap the way `map_band_yield_farzoom` once stopped guarding the LOD gate. It shares
`_snapshot_work_on_grid(w, h)` with that LOD state — one fixture, the same subject at the two ends
of the rail.

**A PNG-LESS `_assert_zoom_ladder` block rides after it** (no `_save`, deliberately, so the frame
count stays 62 and the bit-identity claim is untouched): six assertions that the rail's LADDER holds
— an on-rung click moves exactly one rung in each direction, an OFF-GRID start (mid-way between two
rungs, so neither a round-up nor a round-down bug can pass by luck) SNAPS to the adjacent rung in
the direction of travel, and a click at either limit is a clean no-op. It also prints the ladder as
a player walks it (`1.0 → 1.5 → … → 7.0`). A picture could never carry these claims — every rung
renders as a plausible map — and the harness pins the speed slider, so an assertion is the ONLY
thing here that can see the rail regress **THE WORKED-BAND FIXTURES STATE A HARVEST FLOOR, AND TWO
DIFFERENT ONES.** Every yield label ends in its assignment's floor ZONE mark
(`BandOverlayRenderer._entry_floor_glyph`), so these fixtures are the only thing deciding which
marks the frame set ever renders — and they carried the retired `policy` stance strings (`sustain` /
`deplete`), which no client code reads, long after `band_panel_preview` had migrated. Every row
therefore fell through to `DEFAULT_HARVEST_FLOOR` and the frames were frozen on ONE glyph. They now
carry `WORK_PEAK_FLOOR` / `WORK_DRAWDOWN_FLOOR` (0.15, the floor
`band_panel_preview.LEGACY_STANCE_FLOORS` maps `deplete` onto), split across both the flat-grid
fixture and `_snapshot_work_on_grid`, so `map_band_work` reads `+0.48 ♻` beside `+0.27 ⚠ ⇊` and
`map_max_zoom` draws both marks large.

**A SECOND PNG-LESS BLOCK, `_assert_work_floor_marks`, is what stops it regressing to
one-glyph-everywhere**: it asks the RENDERER (`_entry_floor_glyph`) rather than the fixtures' floats
— the fixtures differing is the premise, not the claim — and makes TWO assertions, that the rows
render at least `WORK_FLOOR_MARKS_MIN` (2) DISTINCT marks and that none resolves to `""`. Both
sabotage-verified and they fail independently: pinning the glyph to the peak zone fails the first
alone, resolving an unknown zone fails both.

**Writing it exposed a live defect** — `_draw_yield_label` re-resolved its already-resolved glyph
through `FoodIcons.for_policy`, so the map had drawn NO harvest mark at all (`overlay-channels.md` →
"The floor MARK is resolved ONCE"); the before/after frames were byte-identical until it was fixed,
which is exactly how a one-glyph frame set hides a no-glyph one. `map_band_yield_farzoom` is
deliberately NOT in the changed set: it LOD-suppresses every label, so a floor it cannot draw cannot
move it.

**A THIRD PNG-less block rides beside them, `_assert_yield_label_component`** (issue #449): the
label has room for exactly ONE rate, so WHICH account it states is the whole claim, and `+0.00` and
`+0.40 fodder` are the same badge at map scale. It asks `BandOverlayRenderer._yield_label_rate_text`
directly — the choice is split out of `_draw_yield_label` for that reason, a draw call rendering to
a canvas nothing can read a glyph back off — over values rather than a fixture, and pairs every
fall-through with the case that must NOT change: food still leads wherever there is food (which is
what stops "always show fodder" passing), food still leads a source paying food AND a material,
fodder still beats a material in the wire's own order, and a source paying into no account at all
still prints its food zero. (A TRADE branch sat between food and fodder and won the slot ahead of it;
arc #527 retired that account, and its follow-up put MATERIALS at the END of the cascade instead —
`food → fodder → materials` — so an inedible quarry states `+0.22 hide` rather than falling through
to the zero.) `_entry_materials` is asked beside `_entry_fodder`, for the same reason: a
fall-through is unreachable if the entry's vector is never read, and neither has a realized fallback
to make. `_entry_fodder` is asked beside them,
since the fall-through is unreachable if the entry's feed rate is never read — and it has no
realized fallback to make, fodder being plant-only.

**A THIRD WORKED FORAGE TILE RENDERS THE LABEL, because the guard's claim is not the frame's**
(`FODDER_FIELD_*`, a sown hay Field paying feed and no provisions). The guard pins
WHICH account fills the one slot; only a frame can say whether the chosen STRING fits beside its
neighbours, and `_draw_pill_plate` sizes to the measured run rather than clipping, so a label that
spans hexes overdraws an adjacent marker with every assertion green — the class
`map_band_label_overlap` exists for. Measured on `map_band_work`: the plate is **67px against a 74px
hex-column pitch**, i.e. just inside its own hex's band, against 49px for `+0.27 ⚠ ⇊` (the 47px
`⇄+0.22 ⇊` it was also measured against went with arc #527's retired account).

**The 2.5× figure that motivated the state is wrong — it is 1.4×** — and the reason is the plate
rather than the text: padding is a fixed fraction of the font size, so it does not scale with the
run. Nothing is overdrawn on any of the four frames. It sits three hexes west of forage tile B on
the same row, deliberately NOT touching it: two adjacent hexes' labels crowd each other whatever
they say, so an adjacent pair could not separate the plate's own REACH from ordinary neighbour
crowding

### `_assert_selection_outline_wraps` — the selection outline at the SEAM, read off the pixels

A **PNG-less** block riding the `map_travel_seam` fixture (it repans the camera, so it comes after
the frame it borrows the snapshot from; the next state's `_fit_map_to_view` puts the camera back).
It pans half a map width so the low columns' WRAPPED copies sit in the middle of the frame, converts
the centre pixel to a tile the way a click does (`_point_to_offset`), and makes three assertions: the
PREMISE that the probe really is over a wrapped copy — without it the other two pass on any map at
all — then, deselected against selected, that ink appears **inside the clicked hex's own box** and
**nowhere else in the frame**.

**It reads pixels because a geometry assertion could only re-ask `_hex_center_wrapped` the question
the draw asks it**, and would stay green the moment the draw stopped calling it — which is precisely
the regression. Sabotage-verified: reverting `_draw_tile_selection_highlight` to the unwrapped
`_outline_hex` reports **0 px changed anywhere in the frame**, i.e. the reported bug exactly — a
click that draws nothing at all. The box is **two radii** around the pressed pixel, since the press
lands anywhere inside the hex and the outline reaches a full radius past it on the far side; at one
radius the ring is split and the "inks nothing else" half fails on the outline's own far edge.

**It saves no PNG and moves none** — the frame set was byte-identical across the fix, the outline
being unwrapped and wrapped to the same place on every non-wrapping fixture here.

### `_assert_herd_trail_unwraps` — the seam guard for a CONNECTED path

The second PNG-less seam block, on its own wrapping fixture (`_snapshot_herd_trail_seam`: one herd,
no band, parked one hex east of the seam). It pans half a map west so the seam sits mid-frame,
captures, seeds `herd_trails` with a trail crossing it — the map's last two columns then its first
two, head on the herd's own tile — captures again, and makes two claims about the ink: that there IS
some, and that it spans no more than `HERD_TRAIL_SEAM_MAX_SPAN_COLS` hex columns.

**THE HARNESS COULD NEVER HAVE CAUGHT THIS ON A PNG, AND THAT IS THE GENERAL LESSON HERE**: a trail
needs TWO successive snapshots to reach a second point, and every fixture in this file is ONE
snapshot, so `_draw_herd_trail` had no coverage of any kind — not a weak frame, no frame. A draw fed
by ACCUMULATED state is invisible to a single-snapshot harness whatever else it renders, and the
seeded-state probe is how it gets covered. `map_preview`'s other accumulator (`culture_layer_map`)
is in the same position.

**The bound is set from the two MEASURED spans, not from the map's width**, which is the part worth
copying. The arithmetic says the unwrapped defect draws a 15-column segment; measured, it inks
**6.1 columns (456px)**, because the line runs off-frame west and is CLIPPED. The honest drawing inks
**3.0 columns (224px)** — its four hexes are three steps apart. A bound reasoned from 15 would sit
above BOTH and pass the bug; 4.0 splits what was actually rendered.

**The liveness half is not decoration.** A span bound alone is satisfied by a trail that draws
NOTHING — the tightest span is the empty one — which is the shape of every "a dead field cannot
diverge" trap. Sabotage-verified in both directions, and they fail independently: restoring the
per-point `_hex_center` fails the span alone (`456px, within 299`), and stubbing the draw out fails
the liveness alone (`0 px changed`).

**It saves no PNG and moves none** — 72/72 byte-identical across the fix, since it clears
`herd_trails` behind it and the next state re-fits the camera, and since the unwrapping is an
identity on the non-wrapping fixtures the trail and the routes actually appear on.

### `map_overlay_picker` — the channel picker OPEN, and the two claims a picture cannot carry

`docs/plan_knowledge_screen.md` §6. The picker is mounted on the MINIMAP, so its `◐` button rides
**every frame this harness saves** — which is also why the migration moved all 65 of them, and why
they were re-read rather than re-baselined. Only this state opens the popover, which is where the
channel list, the `stub data` marker and the legend are; one fixture carries a live ramp channel, a
`placeholder` one and terrain-tag data, so all three render together.

**The popover is IN the capture only because `OverlayPicker` is a `Control` and not a `PopupPanel`.**
A `PopupPanel` is a Window and renders to its own surface — the shipped popover would have been absent
from this frame and unjudgeable. That is the reason for the `TurnOrb` catcher shape, recorded here
because this harness is the thing that would have silently lost.

A block of assertions rides beside it — **count them from a run, not from this line** — and the
load-bearing ones are the pair no frame can hold: **that a
chosen channel SURVIVES the next snapshot**, and **that a channel the picker did NOT set stands**:

- `_ingest_overlay_channels` clears `active_overlay_key` on every frame it ingests, so without the
  picker's re-apply a chosen channel is painted for one turn and reverts, which reads as a click that
  did nothing rather than as a bug.
- The mirror of it is the one that shipped: re-asserting on `overlay_legend_changed` — which fires on
  every channel change, not just an ingest — made the picker overwrite **every other caller**, and
  `map_pasture`, `map_forage`, `map_hunt_danger`, `map_threat`, `map_crisis_annotations` and the two
  pasture-selection frames all came out as bare terrain. **Every one of them is a plausible picture of
  a map with no overlay on it**, and this harness's own assertions were silent, so nothing but a pixel
  diff against a pre-change render could see it. `overlay-channels.md` → "two signals, two rules" is
  the fix; the assertion here is what pins it.

The **ROSTER's composition** is the third no-picture claim (four plausible names render identically in
any order, and the empty key leading / `terrain_tags` trailing are the two placements
`OverlayChannels` decides). The last one pairs *"a world with no tag data is not offered
`terrain_tags`"* with *"…and keeps the empty key"* on purpose: **the empty key is spelled `""`, so a
one-entry roster and a zero-entry one print identically**, and the absence claim alone is satisfied by
a merge that dropped everything on the floor.

**JUDGE THIS MIGRATION BY A PIXEL DIFF, NEVER BY THE HASH LIST.** The button moves all 65 frames, so
"which frames changed" answers nothing here — capture the PNGs before the change and diff each pair,
where a button-sized bounding box is the expected result and a whole-image one is a real regression.
That is exactly how the stomp above was caught.

**IT DRIVES REAL POINTER INPUT NOW, AND THE CONVERSION IS THE WHOLE TRICK.** `_click_canvas` presses
through `Viewport.push_input` so the GUI pass picks the top control exactly as it does for a player —
which is the only way to test the overlay picker's catcher, a full-screen `STOP` on a layer above its
own buttons. **`push_input` takes WINDOW coordinates and a control's rect is in CANVAS ones**, and
this harness pins a canvas the window does not match, so an unconverted press lands somewhere else
entirely: measured, it missed the bar on every leg and every claim failed with nothing open. The
conversion is `ui_preview`'s `InputProbe.canvas_to_window` — SHARED rather than copied, for the reason
`band_panel_preview` already shares `fixtures_band.gd`, and it made this the second cross-harness
preload in the tree. The third is `fixtures_rung.gd`, which this harness's patch and herd fixtures
derive their `current_rung` through — `_patch_rung_key` / `_herd_rung_key` were local to this file
until the whole client started reading that one field (`test-harnesses.md` → "A fixture's STANDING
RUNG is DERIVED, never typed").

**THE LEGEND IS ITS OWN FRAME NOW, AND `ui_preview` LOST THREE STATES TO THIS ONE.**
`map_overlay_legend` is the legend popover open on the channel menu's own selection, and
`map_pasture_legend` / `map_forage_legend` / `map_temperature_legend` ride the `pasture` / `forage` /
`temperature` states as `_save_overlay_legend`. Those last two were a `print` of the legend dict here and a hand-TRANSCRIBED
fixture in `ui_preview`'s `pasture_legend` / `forage_legend`, kept in step by hope; this harness owns
a real MapView, so it can open the picker on the real builder's rows. `_save_overlay_legend` CLOSES
the popover again — the picker rides a long-lived MapView, so one left open renders in every later
frame. (`ui_preview`'s five `terrain_legend_*` sort-control frames went with the `L` card itself and
have no successor: the sort header was that card's, not the legend's.)

**AND THE CHROME CLAIMS ARE STRUCTURAL, BECAUSE THIS HARNESS HAS NO HUD.** It stands up a bare
MapView, so the minimap takes its FLOATING bottom-right mount and none of the docked CanvasLayers
exist — which means the second reported defect, the popover drawing UNDER the Band/City panel,
renders here as a perfectly correct frame. Two assertions cover it without one:

- The popover's **layer** is compared against `BandCityPanel.LAYER_INDEX` / `EventDockPanel.LAYER_INDEX`
  / `Main.WORKBENCH_LAYER` / `Main.INSPECTOR_LAYER` / `Main.HUD_LAYER` — **those files' own constants,
  never a number written twice** — plus `Main.LOADING_OVERLAY_LAYER` from the other side, so raising a
  dock's layer fails this rather than silently re-burying the popover. `map_preview` therefore
  `preload`s `Main.gd` for its layer roster alone.
- The **position** clamp reserves an edge through `MapView.set_reserved_inset`, exactly as `Main` does
  for the real panel. It reserves the **RIGHT** edge, which is *not* the shipped case: the reported bug
  was a LEFT dock over an embedded minimap, and with the minimap floating bottom-right here a left-edge
  claim passes with the clamp deleted. The near edge drives the same `_play_area()` bound from the side
  a fixture can actually move, and it is **paired with a precondition** asserting the UNRESERVED popover
  really does reach into the strip — without which the probe passes whenever the two positions coincide.

Both were sabotage-verified: dropping the layer to 101 fails the first naming all five surfaces it is
under, and short-circuiting `_play_area()` to the raw viewport fails the second at `1907 <= 1425`.

## `tools/blend_probe.gd` / `.tscn`

Dev-only **edge-blend probe rendered at the GAME's on-screen hex radius** — the other harnesses
*fit* their grid to the window (r ≈ 83–178) and the blend look is radius-relative, so every
judgement made in a fitted frame was wrong. Pins a 1:1 1920×1080 canvas + a grid sized so
`_fit_map_to_view` lands on the target radius (it prints the achieved radius and warns if it
drifts).

**Two states:** (1) a **band strip** of flat biomes at r≈45 (desert · prairie · scrub · alluvial ·
tundra · salt flat — every adjacent pair is a flat↔flat seam) → `blend_bands_*.png`; (2) **ISOLATED
prairie hexes surrounded on all six sides by dark rocky soil** at **r≈75** (the user's on-screen
size) → `blend_isolated_shipped.png` + one full frame & native-res close-up per tuning variant + a
labelled contact sheet (`V6_*.png`).

**State 2 is mandatory for any blend change**: a straight band seam looks fine even when the blend
is tearing holes in hex interiors — only a surrounded hex exposes it (that is how the shredding
regression shipped).

**Two more states (V7, water↔water):** (3) an irregular **deep-ocean region embedded in continental
shelf** (plus isolated deep hexes) at r≈77 → `V7_water_W1.png` (water on the shared LAND levers —
still a soft-edged hexagon) vs `V7_water_W2.png` (the shipped `water_blend` block — the silhouette
dissolves); (4) a ragged **coast** frame with a single water id → `V7_coast_unchanged.png`, the
**bit-identical reference** any blend-eligibility change is pixel-diffed against (it must not move
the shoreline).

**Two more states:** (5, V8) the water patch rendered **FoW OFF vs FoW ON** (a mix of active +
discovered hexes, nothing unexplored) → `V8_water_fow_off.png` / `V8_water_fow_on.png` — the FoW
tint comes from a **per-hex, NEAREST-sampled vis-map**, which used to make every discovered↔active
adjacency a **hard hex-shaped tint boundary that is not a terrain seam**. Any "hard straight edges
are back" report must be checked against this pair BEFORE the blend is touched. This is also the
frame the **FoW boundary softening** is judged on (see Fog-of-war softening: the steps must be gone,
pure states unchanged); (6, V10) the shipped **shoreline profile** on the ragged coast at r≈75,
rendered against TWO land biomes → `V10_shore.png` + `V10_shore_closeup.png` (prairie) and
**`V10_shore_dark_land.png` + `V10_shore_dark_land_closeup.png`** (rocky_regolith). The close-ups
are where the "is there a hard line anywhere on land→sand→foam→water?" call is made (the downscaled
full frame hides a 1px line; see Shoreline), and **the DARK-land one is decisive** — prairie's tan
hides sand-vs-land contrast and masked an invisible-beach bug through several passes, so never judge
the beach on prairie alone. `_render_variant(overrides, name, crop…)` overrides any `terrain_config`
lever (incl. the nested `water_blend` / `shore` blocks) live, which is how the shipped values were
swept.

**One more state (8, W): the FoW hex-step BEFORE vs AFTER the boundary softening** — one camera, one
terrain, one visibility map, only `fow_softness` varying → `W_fow_off.png` (FoW off, the
terrain-only reference: the deep-ocean blob's edges are already soft, which **exonerates the
blend**), `W_fow_on.png` (softness `0` — reproduces the **unsmoothed per-hex tint**, i.e. the hard
hexagonal brightness steps), `W_fow_fixed.png` (the shipped softness — steps gone, mist preserved).
Each also dumps a `_closeup` and, decisively, a **`_same_terrain`** crop straddling hexes **(4,3)
Active / (3,3) Discovered — BOTH continental shelf**, so the only thing that can draw an edge
between them is the FoW tint. That crop answers any "hard straight edges in open water, even between
hexes of the same terrain" report.

**One more state (9, X): the DARK-WATER report on REAL game terrain** → `X_dark_water.png` +
`X_dark_water_closeup.png`, rendered from a **verbatim 14×10 window of a LIVE snapshot's id-map**
(`X_WATER_IDS`), FoW OFF, r≈75. The synthetic water states (3/5/8) never reproduced the "dark
patches of open water with hard full-hexagon edges" report because their deep-ocean region is ONE
clean ragged blob; the real ocean is **salt-and-pepper** shelf/deep, and a lone deep hex ringed by
shelf can only read as a dark HEXAGON.

**Any "dark water hexagons" report must be rendered on THIS state** — a synthetic blob will not show
it. It is the frame the water **depth field** (see Edge Blending → water) was verified against.

**One more state (10, L): the PER-WATER-TERRAIN shore profile on a SMALL INLAND SEA** →
`L1_current.png` / `L2_no_wisp.png` / `L3_half.png` / `L4_tenth.png` (+ `*_full.png`), a 7-hex
`inland_sea` lake in a field of **dark rocky_regolith** (prairie's tan camouflages both sand and
foam) at r≈75, one camera/crop across all four. `_render_lake_variant` overrides the inland_sea
entry's `shore_profile` in the live config and calls
`TerrainTextureManager.rebuild_layer_shore_map()` — the sweep for choosing a lake's coast (now in
the three-scale scheme; **L3 IS the shipped lake**, `sand 0.5 / foam 0.5 / wisp 0`, and L4 = the
whole profile scaled so its OUTERMOST reach, `wisp_center + wisp_half` = 0.68·r, lands at ~0.10·r →
0.147).

**The harness disables `MapView._unhandled_input`** — it renders in a REAL window, so the OS cursor
otherwise drew a faint HOVER hex outline into the frames, a run-to-run difference of a few thousand
pixels that silently defeats the pixel-diff the coast states exist for. With it off, consecutive
runs are **byte-identical**, so `V7_coast_unchanged.png` / `V10_shore*.png` are usable as strict
bit-identity references.

**One more state (11, H): ROLLING HILLS "cut off at the hex edge"** → `H_*.png`, a `rolling_hills`
(24) blob + **isolated** hills hexes + an **isolated alpine (26)** hex in a field that is dark
`rocky_reg` west / tan `prairie` east, at r≈75 with the **hex grid overlay OFF** (a drawn hexagon
would answer the very question under test). Frames: `H_before` (the artifact), **`H_base_only`**
(peaks skipped by pushing `peak_min_radius` above the render radius — isolates the BASE floor, and
is what proved the cut is the rugged base hexagon, **not** a weak mound overhang), `H_peaks_only`
(the amplified `before − base_only` pixel diff = the peak pass's exact footprint: it shows the
mounds DO overhang, and that the peak **cast shadow darkens the whole neighbour hex**, a second hard
hexagon), and the candidate fixes `H_fix_overhang` / **`H_fix_base`** (`blend_rugged_land`) /
`H_fix_both`. Each renders a full frame + a seam close-up + the **isolated-hex** and **alpine**
close-ups (the mandatory shred checks). `H_gate_bands_full` / `H_gate_coast` re-render the flat↔flat
strip and the coast with the rugged gate ON — they must byte-compare **identical** to
`blend_bands_full` / `V7_coast_unchanged`.

**One more state (12, R): the RUGGED-GATE SWEEP** — `blend_rugged_land` is GLOBAL, so shipping it
lets EVERY rugged biome's base floor blend, and the failure mode is SHREDDING. R renders **each
rugged biome as an ISOLATED hex** (even col + even row ⇒ never adjacent to another subject) in TWO
fields, each **gate OFF vs gate ON** so every biome is a controlled A/B: `R_flatoff_*` / `R_flat_*`
(dark `rocky_reg` west, tan `prairie` east) and `R_ruggedoff_*` / `R_rugged_*` (a field of
`canyon_badlands` — the rugged↔rugged case), plus `R_*_field_full`.

**The gate-OFF pair is not optional**: several biomes' own art (e.g. `karst_highland`'s
semi-transparent overhanging spires) *looks* like neighbour texture leaking into the hex, and only
the A/B tells art from tear.

**One more state (13, S): the PEAK CAST-SHADOW HEXAGONS** — an alpine massif + an isolated
`rolling_hills` hex in a light prairie field, grid OFF → `S_shadow.png` + `_closeup` + `_iso`, and
decisively **`S_shadow_footprint*.png`**, the amplified diff against a `shadow_strength = 0` render
(the cast shadow **in isolation** — the only frame on which "is it hex-shaped? is it still
directional?" can actually be answered, since the semi-transparent mound fringe contaminates every
other measurement).

**Two harness bugs were fixed here and must not regress:** (a) `project.godot` opens the window
**MAXIMIZED** (`window/size/mode=3`) and the WM applies that a few frames into the run — *after*
`_ready` sized it — so the viewport became the whole monitor and every state after the second
silently rendered at **r ≈ 154, not the game's 75** (and the taller states overflowed the canvas,
clipping the close-ups). `_pin_canvas` re-asserts WINDOWED + 1920×1080 on every `_refit`. (b) Lever
overrides now go through **`_override_config`/`_restore_config`**, which **ERASE** a key that was
absent instead of writing `null` back: TerrainRenderer reads levers as `bool(config.get(key,
DEFAULT))`, the default only applies when the key is **missing**, and a present-but-null key reaches
`bool(null)` — a **runtime error that aborts `TerrainRenderer.update_shader_quad` before it pushes a
single uniform**, so every later frame renders with STALE uniforms and lies.

**One more state (14, G): the REAL NEIGHBOURHOOD from the user's screenshot** — the "hills are STILL
cut off, with the rugged gate ON" report → `G_*.png`. State H could not see why: its hills blob sits
in FLAT fields only, so every peak edge in it is a peak↔non-peak one (which the overhang feathers).
G rebuilds the screenshot — a `rolling_hills` blob against `canyon_badlands` (rugged, **no** peak
asset), **`alpine_mountain` (which HAS one → the peak↔PEAK case)**, `high_plateau` (a peak at ~the
SAME elevation as the hills → the near-zero-Δ case), `alluvial_plain`, `rocky_reg` and an
`inland_sea` lake hex — at r ≈ 75, grid OFF. It is the **only** probe state that ships a real
**elevation raster** (`G_ELEVATION_BY_ID` + `elevation_sea_level`): every other snapshot omits the
channel, so MapView falls back to `PEAK_ELEV_FALLBACK` for EVERY hex and **no elevation asymmetry
can be judged in them**. Frames: `G_before` (shipped), **`G_no_peaks`** (peak pass skipped — it
renders the same seam as a soft ecotone, which **exonerated the base blend** and convicted the peak
overlay), `G_no_shadow` (cast shadow off, peaks on — attributes a residual line to the shadow vs the
art), `G_peaks_only` (the amplified diff = the peak pass's exact footprint), each with native-res
crops `_peakpeak` (hills↔alpine, big Δelev), `_sameelev` (hills↔plateau, Δ≈0 → must stay a soft
symmetric cross-fade), `_canyon` (peak↔non-peak — the control), `_lake` (the shoreline — hard BY
DESIGN), `_iso` + `_iso_alpine` (the mandatory isolated-hex shred checks; both sit on the LEFT of
the frame because MapView's minimap CanvasLayer is NOT hidden and a bottom-right crop captures IT).

**A `--only=` state filter** (`scripts/preview.sh res://tools/blend_probe.tscn -- --only=G`, or
`--only=1,4,G`; keys are `<number>/<letter>`, no filter = every state) renders one state instead of
all 14 — a diagnosis loop re-renders one state many times.

**A third harness bug was fixed here and must not regress:** `project.godot` opens the window
**MAXIMIZED** and macOS applies — and **RE-applies** — that asynchronously, many frames in, so a
fixed pair of `process_frame`s is a RACE that does not stay won. A filtered run puts a
radius-critical state FIRST and it fitted at **r ≈ 154, not the game's 75**; a re-maximize BETWEEN
two frames of one state rendered them at different resolutions (the pixel-diff then dies on a size
mismatch); and one DURING a crop sequence made the captured image the monitor's while the viewport
still reported the pinned size (`content_scale_size` pins the viewport, so **only
`get_window().size` can see the maximize**) — the crop then landed off-frame as a 686×1 sliver.
`_ensure_canvas` (called from `_settle`) re-pins and WAITS on the window; `_capture` re-draws until
the captured geometry is the canvas's (or an integer HiDPI multiple) instead of silently saving a
bad frame.

**A FOURTH determinism fix closes that arc, and it is what makes the set a STRICT BIT-IDENTITY
REFERENCE: ANIMATION TIME IS FROZEN** (`Engine.time_scale = 0.0` at the top of `_ready` — the same
treatment `map_preview` got, and taken for the same reason). With the canvas pinned, animated
content was the ONLY remaining run-to-run difference, and it left the set at **205 stable frames and
25 that drifted**: the `BANK_*` state is the only one here carrying a navigable river, hence the
only consumer of the shader's `TIME * river_flow_speed` channel scroll. Frozen, the whole set is
**230/230 byte-identical across runs** (verified over three consecutive runs) — the property every
MapView decomposition pass leans on, since a frame that varies cannot be pixel-diffed to prove a
refactor changed nothing, and the reference any NEW fixture gets judged against.

**The cost is that animation renders at a FIXED PHASE**; it moved exactly the 25 `BANK_*` frames
once, a deliberate re-baseline, and **the other 205 were byte-identical with and without it**
(measured — the prediction was made first and held: every bit-identity reference,
`V7_coast_unchanged` / `V10_shore*` / `H_gate_*` / `blend_bands_*` / `blend_isolated_shipped`, moved
0 bytes, as none contains a river).

**Freezing at phase 0 erases nothing, and that was checked against the shader before it was taken**:
`terrain_blend.gdshader` reads `TIME` in exactly TWO places (the edge-class river pass and the
navigable-channel pass) and both enter identically as a **UV OFFSET** into the `river_tex` sample,
while every term deciding whether water DRAWS is purely geometric — the channel `alpha` and the bank
`bank_alpha` are both `smoothstep(-river_softness, river_softness, <signed coverage>)` and
`class_mix` comes from coverage differences — so channel, banks, taper and corridor blend are
untouched and only which texels of the water art land where is pinned (confirmed visually on
`BANK_shipped` + `BANK_shipped_iso_dark`). This harness has no time-dependent GDScript of its own
(no `Time.` reads, no tween, no pulse), and `_settle` waits on `process_frame`, which still fires at
`time_scale` 0.

**RE-CHECK RULE for anything animated added later** (the same one `map_preview` carries): an
AMPLITUDE term (`A * sin(t)`) VANISHES at phase 0 and a frame that is deterministic because its
subject disappeared is worse than one that varies, whereas an offset or a midpoint idiom (`0.5 + 0.5
* sin(t)` → 0.5 at t = 0) survives — classify the new term before trusting the freeze.

**One more state (15, D): the THREE-SCALE shore profile — CLIFF vs BEACH vs LAKE, and the MIXED
coast** → `D*.png`, the ragged coast against **dark `rocky_reg`** (prairie's tan camouflages both
sand and foam) at r≈75, **grid overlay OFF**, one camera/crop per comparison set.
`_snapshot_coast(shore_id, water_id)` now takes the SEA's id, which is what selects the
`shore_profile` under test. Frames:

**`D1_cliff`** (`deep_ocean` meeting land — NO sand anywhere, big surf, and the full-strength surf
peak must still conceal the base's own step at the waterline, since there is no sand out there to
hide it); **`D2_shelf_C1/C2/C3`** (the shelf's muting ladder, `foam_scale` 0.85/0.75/0.65 ×
`wisp_scale` 0.5 — the surf's measured footprint falls 18.0k → 15.8k → 13.9k → 12.2k px against the
cliff's; **C2 ships**); **`D3_mixed_coast`** — THE DECISIVE FRAME: a `deep_ocean` hex and a
`continental_shelf` hex **adjacent along ONE coastline**, both touching the same land
(`_snapshot_mixed_coast` swaps the sea by row), where a nearest-water PICK would jump the profile at
their bisector and make the sand appear along a **hard line**; the weighted-mean profile field must
instead **fade the beach in** along the shore (measured: the land-pixel difference vs `D1_cliff`
ramps from 0.00 over ~220px ≈ 3 hex radii — not a step); and **`D4_lake_unchanged`** (the lake,
shipped config — the two-lever → three-scale migration must be a no-op).

**One more state (16, SURF): THE BRIGHT WHITE SHORELINE OUTLINE** → `W_*.png`, the state the
**waterline base cross-fade** + **`foam_opacity`** were built and chosen on (r≈75, grid OFF; the
archipelago frames also render at **r≈30 — map scale**, which is the zoom the complaint was made
at). The report was that the surf reads as "an obvious bright white outline on most land". Every
frame uses the **MIXED coast** (`_snapshot_mixed_coast`: deep_ocean CLIFF in the north rows,
continental_shelf BEACH in the south, both against **dark rocky_reg**) so each rung is cropped on
**both coast types at once** (`_cliff` / `_beach`) — they fail differently. Frames: `W_base` (the
shipped near-white ring — the complaint, and it is unmistakable); **`W_optA_1/2/3`** (option A, the
**recolour-only** ladder: still an OPAQUE ring, just greyer — rendered so the "just make it grey"
idea can be *seen* to be insufficient); **`W_optB_1/2/3`** (option B's `foam_opacity` ladder
0.35/0.55/0.75 on the cross-fade + muted colour; **0.55 ships**); and **THE MAKE-OR-BREAK PAIR —
`W_step_control` vs `W_optB_step_check`**, the CLIFF coast with the **foam disabled entirely**
(`foam_opacity 0` kills surf *and* wisp): the control (cross-fade also off) shows the **raw base
step — a razor-straight hex-edge cut**, which is what the opaque foam was hiding all along, and the
step check must show it GONE.

**Any change to the surf must re-render that pair** — a translucent surf over a live base step is
exactly the bug that broke this shoreline four times. `W_step_wl_1/2/3` is the `waterline_width`
sweep it was chosen on (0.08 dissolves the step, **0.14** reads as a wet-rock rim, 0.20 ghosts land
pebbles out to sea).

**Judge the step check at 4× magnification** — at 1:1 the cross-fade and the razor step look nearly
identical, and the first (too-narrow) cut was wrongly passed by eye before the magnified strip
caught it. `W_base_wide` / `W_optB_wide` (+ `_farzoom`) are the **archipelago**
(`_snapshot_archipelago` — islands on a lattice, alternating shelf-ringed BEACH coasts and
deep-touching CLIFF coasts, so both types are in one frame; deterministic and grid-size independent,
so the same map renders at r≈75 and at map scale):

**`W_base_farzoom` vs `W_optB_farzoom` is the frame that actually answers the complaint.** **One
more state (17, BANK): the NAVIGABLE-RIVER BANK CORRIDOR reading as a CHAIN OF HEXAGONS** →
`BANK_*.png`, the state the per-terrain **`blend_profile`** (see Edge Blending) was diagnosed and
chosen on. A navigable hex is a silty **bank** whose `blend_class` is `flat`, so the flat↔flat
interlock IS eligible on its land edges — and a shader probe (tint the mix factor `t` on id 37)
confirmed it **FIRES**: this was never a gate/eligibility bug, and no amount of re-checking
`blend_class` or the water gates will find one. It is a LOOK failure — the global ecotone is
~`0.35·r` wide and near-straight, which is invisible between two tan grasslands and glaring between
grey gravel and orange grass. The frame renders the corridor (a real `river_channel` chain, so the
water draws) at the game's **r ≈ 75** crossing a field that is **floodplain (9, luma 58) in its west
half and prairie (11, luma 112) in its east** — **both ends of the brightness range a river corridor
actually touches, in ONE frame**, because the bank is *darker* than prairie but *brighter* than
floodplain and a fix tuned against only one of them fails on the other. Plus an **ISOLATED bank hex
in each field** (the mandatory shred crops — a corridor seam cannot show a torn interior; they sit
in the TOP rows because a bottom-right crop captures MapView's minimap). `_render_bank_variant`
sweeps the profile live via `_set_blend_profile` +
`TerrainTextureManager.rebuild_layer_blend_map()`:

**`BANK_off` is the NEUTRAL profile — i.e. the BEFORE**, the shipped global levers, in the same
camera, and it reproduces the report exactly. `BANK_v1/v2/v3` are the ladder (**v2 = 2.6/2.2/2.6
SHIPS**; v1 still traces the hexagon, v3 dissolves the bank) and `BANK_shipped` is config's.
`scripts/preview.sh res://tools/blend_probe.tscn` (or `-- --only=SURF` / `-- --only=BANK`)

## Worked-source mark states (issue #412)

**`map_preview`** — `map_worked_ready` (the ⌃ CONTRAST: a tended patch offers Sow, a tamed
"pen"-ceiling deer offers Corral, a "wild"-ceiling wolf offers nothing; a chevron on every marker would
prove nothing) · **`map_worked_unstaffed`** (its A/B twin: the SAME three sources with the working
band's **`builders` ROLE row** taken off, so the one plate that can differ goes from
`🌱42%` in the deep signal ink to `🌱⚠` in WARN — see `overlay-channels.md`. **The pair is the
claim**, a plate that always warned passing either frame alone, and `_snapshot_work_ready` had to
GAIN that role row for it: without it the ready frame stages the warned case under a comment
describing work in flight. It was a per-source `improvement_workers` count until §2.5 moved the hands
onto the band) · `map_hunt_expedition_quarry` (an outbound party's quarry marked beside a resident
band's local hunt — two routes to a worked source, one grammar) · `map_overflow_worked` (three wonders
take every visible slot, so both worked sources roll into the chip as `+2 ⌃`). **Both new states push
`set_faction_knowledge` explicitly**: `map_preview` has no HUD, so without it every source reads "not
ready" — the correct degradation, but an unreadable frame. `map_band_work`'s fixture gained a food site
on each worked tile, and that is load-bearing: the first cut of the ring rendered nothing at all
because the fixture had none, and the mark correctly degraded to the bare tile outline.

### `map_ready_for_improvement` — the AGGREGATE ⌃, and why the frame is a contrast rather than a glow

`docs/plan_knowledge_screen.md` §7. The `ready_for_improvement` channel painted over the ⌃-mark fixture, plus
`map_ready_for_improvement_legend`, its `facts` card. It **extends `_snapshot_work_ready` rather than
replacing it**, so the badges and the channel are asked about the SAME sources. The two do not answer
identically: a lit hex is a strict SUBSET of the hexes wearing a ⌃, because the channel also asks
whether the source has been IMPROVED at all and whether it is the player's
(`.claude/rules/client/overlay-channels.md` → `ready_for_improvement`).

What the extension adds is a source per outcome, **each staged so every OTHER term passes** — a dark
control proves only its own term if nothing else is refusing it too.

| source | staged as | outcome |
|---|---|---|
| `FORAGE_A` `(7, 6)` | worked, tended, sowable | **LIT** — the ordinary case |
| `READY_FIRST_RUNG` | **wild**, worked by band 2 | **LIT** — worked, not improved |
| `READY_FIRST_RUNG_HERD` | **wild** herd, hunted by band 1 | **LIT** — the reported defect |
| `READY_UNWORKED_NEAR` | tended, sowable, **nobody on it** | **LIT** — improved, not worked |
| `READY_UNWORKED_HERD` | tamed, penable, **nobody hunting it** | **LIT** — its herd twin |
| `READY_MID_FIELD` | tended, Field meter part-filled, **nothing declared** | **LIT** — no in-progress test |
| the deer `(13, 6)` | tamed, worked, ceiling `pen` | **LIT** |
| `READY_FOREIGN` | worked, tended, sowable — **faction 1's** | dark — ownership |
| `READY_BARREN_LADDER` | worked, tended, **no crop may climb** | dark — the ladder |
| `(9, 8)` | worked, **crew DECLARES `cultivate`** | dark — `next_rung_ready` declines a declared verb |
| `READY_HALF_BUILT` | wild, half-cultivated, **nobody on it** | dark — neither half of the union |
| the wolf `(11, 4)` | worked, **`husbandry_ceiling: wild`** | dark — the ceiling |

> #### ⛔ BOTH HALVES OF THE CANDIDATE UNION ARE ASSERTED POSITIVELY AND SEPARATELY, and that is the
> whole lesson of this state
>
> The channel's candidate set was wrong three times — every tile on the map, then improved-only, then
> worked-only — and **each wrong version shipped with a fixture built around its own set**, which
> confirmed it instead of catching it. A count assertion cannot tell those apart: it moves for any
> reason and reads plausibly at every wrong value.
>
> So `READY_FIRST_RUNG` / `READY_FIRST_RUNG_HERD` (worked, not improved) and `READY_UNWORKED_NEAR` /
> `READY_UNWORKED_HERD` (improved, not worked) are asserted BY NAME as things that must LIGHT.
> Sabotage-verified in both directions: dropping either half of `_is_candidate` fails its own pair and
> leaves the other passing.
>
> **The same trap ate a control twice on the dark side.** `READY_MID_FIELD` was written to isolate an
> "already being built" test, having watched the two mid-Cultivate patches stop isolating it when a
> condition moved in FRONT of them; then that test was removed entirely and the control became a LIT
> case instead. **When a term is added or removed anywhere in the chain, re-run the sabotage before
> believing any control still guards what its name says.**

**A LIT MAP IS A PLAUSIBLE PICTURE OF A CHANNEL THAT LIGHTS EVERYTHING**, which is why the assertions
ask for TILES rather than a count. `_lit_ready_tiles` reads them back off the **raster the map
paints**, never off the model's own counters, so the claim cannot be the assertion agreeing with
itself.

**THE WOLF'S `husbandry_ceiling` IS STATED, NOT INHERITED.** `SourceForecast.husbandry_ceiling`
normalizes an ABSENT field to `"pen"` — the FULL ladder, so an untagged herd behaves as it did before
the field existed — which made `_pelt_only_wolf_herd` offer `Tame` and wear a `⌃` of its own, in
`map_worked_ready` as well as here. `_snapshot_work_ready` now says `"wild"` outright. Stated there
rather than on `_pelt_only_wolf_herd` so only the two frames that push knowledge move; the frames that
push none had no chevron on anything either way.

The block beside it drives four things no picture can carry:

- **THE KNOWLEDGE PUSH THAT ARRIVES LATE.** The snapshot goes in with the knowledge row EMPTY and the
  row is pushed after it — the shipped order, since `Main._apply_snapshot` renders the map first and
  fans the HUD out behind it. A channel derived only at ingest stays empty here, which is the state it
  would be in for the whole turn a discovery lands on. The empty case is asserted first (the channel
  is offered, states its empty sentence, lights nothing), so the push is a real A/B.
- **THE COUNTS SPLIT BY WEB.** One web answering for both produces a perfectly plausible total.
- **THE SECOND BAND, and it exists for one claim**: *nearest* is measured from the SELECTED band. With
  one band the anchor and its fallback are the same tile and a hardcoded first-band read passes. The
  leg changes the selection and **nothing else** — no snapshot, no re-derive — which also pins that
  the facts are answered off the CACHED model rather than stamped into it at ingest.
- **A WORLD WITH NO SOURCES OFFERS NO CHANNEL**, paired with the empty key surviving, for the reason
  the `terrain_tags` claim beside it is paired: `""` prints identically to nothing at all.

**`_assert_ready_for_improvement_scale` IS A REPORT, NOT A THRESHOLD.** §7 says to measure the all-sources
pass before assuming it is cheap, so the probe builds a **full-size 256×192 world with a patch on
every tile — every one of them tended and player-owned, so every source QUALIFIES**; a probe whose
patches were refused by the improved test would measure the cost of rejecting them early rather than
the cost of the full walk, and the ceiling would stop being a ceiling. That world is the ceiling on
what the derivation can ever be handed, since the sim seeds a patch on every food-module tile that
carries capacity and caps none of them, and the probe prints the microseconds it took. A
timing ASSERTION on a shared machine fails for reasons that have nothing to do with the code under
test, and a harness that cries wolf stops being read; what is asserted instead is the thing a number
cannot drift on, that the probe really walked a full-size world. It is the last thing the state does,
because it leaves `_map` on that world.

### `map_band_lethal_mark*` — the ⚠ a band wears on killing ground (issue #614)

Three frames on the temperature field with **two bands on it**, one in the cold pocket and one on the
temperate middle. The pair is the test: a mark asserted on the lethal band alone passes on a renderer
that draws it for every band, which is the one way this could be wrong and still look right.
`map_band_lethal_mark` is the close read (⚠ up-left, food dot up-right, banner below, no collision);
`map_band_lethal_mark_gate` is the mark at the SMALLEST size it is ever drawn at; and
`map_band_lethal_mark_farzoom` is one notch below the gate, where it is gone.

> #### ⛔ TWO PROBES THAT PASSED WITH THE FEATURE DELETED, AND WHAT REPLACED THEM
>
> **The absence was asserted where nothing was visible either way.** The far-zoom state first sat at
> radius 12.7, well past `BAND_LETHAL_MARK_MIN_RADIUS`, and the "the mark is gone" claim **passed with
> the LOD gate deleted** — the glyph is drawn there and blends into the terrain completely. An absence
> is only worth asserting where a PRESENCE would have shown, so the two LOD grids now fit just either
> side of the gate (measured ~29.0 and ~26.7) and each states its measured radius in a premise
> assertion. They had to be re-measured rather than computed: the harness canvas carries a ~1.9×
> content scale the arithmetic missed, and they moved AGAIN when the mark took a gate of its own.
>
> **A colour-distance probe cannot see an antialiased glyph.** `draw_string` blends, so a small ⚠
> never reaches its own ink — measured, the closest pixel to `HudStyle.DANGER` inside the marked
> token's box was 0.192 away and bare khaki terrain was 0.235, i.e. no threshold separates them and
> the loose tolerance that finally "passed" was matching terrain. `_frame_marks_warning_near_hex` asks
> for REDNESS instead (`r − max(g, b)`, ~100/255 on the glyph against ~18/255 on the terrain), and
> `_frame_paints_near_hex` went back to the EXACT match that is right for the flat-filled hatch and
> contour lines it was written for.
>
> The "legible at the smallest zoom it is drawn at" assertion is what then caught the real defect: at
> the banner's gate with a 9 px floor the mark was unreadable, which is why the mark now has its own
> higher gate and no size floor at all (`map-markers.md`).

Sabotage-verified in two runs: dropping the LOD gate and the `is_lethal` test together (2 fail — the
survivable band's absence and the far-zoom absence, the latter only biting once the grid moved beside
the gate); and skipping the draw entirely (2 fail — the close mark and the smallest-size mark).
