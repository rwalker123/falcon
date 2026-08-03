extends Node

## Dev-only UI preview harness.
##
## Instances the real HudLayer with canned selection data, renders each state,
## and saves a PNG to `ui_preview_out/` in the project. Lets us iterate on HUD /
## selection-panel / targeting styling without a running server or manual
## screenshots. Not part of the game — run explicitly:
##
##   godot --path . res://tools/ui_preview.tscn
##
## then read ui_preview_out/*.png.

# A floor BELOW the food peak, for the frames that need "this crew is drawing the source down" — the
# `deplete`/`surplus` stances these fixtures were written against. It is one of the sim's own raid
# samples, so a converted raid table lands on a real row rather than an interpolated one.
const DEEP_DRAW_FLOOR := 0.15

# The would-be herder crew on `herd_tame_worker_cap` — see `_tame_worker_cap_herd_fixture` for why it
# has to clear the Tame rung's own take-useful (~27 since the build dip moved onto the crew).
const TAME_CAP_WOULD_BE_HERDERS := 30

# "leave the floor alone" for `_compose_herd`'s optional argument — a sentinel OUTSIDE the legal
# `0..1` range, since every real floor including `0` is a value a frame may want to dial.
const COMPOSE_FLOOR_UNSET := -1.0

# The per-biomass rate a DEAD-SEASON patch keeps. A rate says what grows here, which a season does
# not change; the season empties the stock and the crew's throughput. Any positive value serves —
# the patch's stock is pinned AT the food peak, so every ceiling is 0 whatever this is.
const BARREN_PATCH_PER_BIOMASS := 0.01

# **THE HAY MEADOW'S TWO NON-FOOD RATES.** Sized so the meadow's two accounts BIND DIFFERENTLY, which
# is the fixture's whole job: at the seeded stock (room 40 above the food peak) the fodder ceiling is
# 0.20/turn against a crew that gathers 0.13 fodder/worker, so the CEILING binds on fodder; food
# gathers at 0.08/worker against a 0.60 ceiling, so LABOR binds there. A crew can therefore sit
# comfortably inside the patch's food regrowth while stripping its hay — which is only expressible
# because `min(w x per_worker, ceiling)` and the overdraw verdict are both applied PER ACCOUNT.
const HAY_MEADOW_FODDER_PER_BIOMASS := 0.005
const HAY_MEADOW_TRADE_PER_BIOMASS := 0.00025

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
## Scratch prefs file for this harness — NEVER the player's `user://narrative.cfg`. See the
## prefs-isolation block in `_ready()` for the incident that made this non-negotiable.
const PREVIEW_PREFS_PATH := "user://ui_preview_prefs.cfg"
## Scratch DOCK prefs for the `BandCityPanel` this harness injects — NEVER the player's
## `user://band_city_dock.cfg`. A second file because the panel keeps its own (edge / collapsed / tab).
const PREVIEW_DOCK_PREFS_PATH := "user://ui_preview_dock_prefs.cfg"
## Scratch prefs for the `EventDockPanel` — a THIRD scratch file, for the same reason as the second:
## the dock reads its edge / row count / detail floor on construction and writes them back as the
## harness walks them, so without this a frame would depend on what the last run left behind AND the
## walk would land in the developer's real `user://narrative.cfg`.
const PREVIEW_EVENT_PREFS_PATH := "user://ui_preview_event_prefs.cfg"
# Force-compile MapView here so the harness also acts as a full-context compile
# check for it (autoloads are registered when the harness runs as a scene, which
# --check-only cannot do).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## Preloaded for its STATIC `escape_claimant` alone (the ESC precedence chain, extracted so the order
## can be asserted without standing up the whole app scene) — Main is never instanced here.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## Injected for ONE state (`tile_panel_band`) and released again: a selected player band's detail
## renders into this panel, so it is the only way to render the drawer's "it went over there"
## pointer line rather than the no-panel legacy fallback.
const BAND_CITY_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
## Injected for the `event_dock_*` block at the end of the run and freed again. Like the band panel
## it is its OWN CanvasLayer (not part of `HudLayer.tscn`), so it exists only for the states that
## judge it and cannot leak a reserved strip into the other 200-odd frames.
const EVENT_DOCK_SCENE := preload("res://src/ui/EventDockPanel.tscn")
const OUT_DIR := "res://ui_preview_out"
# The canvas EVERY frame renders at. Pinned rather than set once, because `project.godot` opens the
# window MAXIMIZED and the WM applies — and RE-applies — that asynchronously; see `_ensure_canvas`.
const PREVIEW_CANVAS_SIZE := Vector2i(1500, 900)
## The CANVAS every frame is composed in, which is NOT the window: `project.godot` stretches
## `canvas_items` from a 1920-wide base with an `expand` aspect, so a control's own geometry is in
## these units while the PNG is in `PREVIEW_CANVAS_SIZE`. The event dock's width cap is a canvas
## number, so the narrow-case assertion has to compare against this one.
const PREVIEW_CANVAS_SIZE_BASE := Vector2i(1920, 1152)
## A deliberately ULTRAWIDE window for the one state that exercises the event dock's width cap — the
## configuration the "way too wide for its content" report came from, and one no other frame reaches.
## Rendered outside `_ensure_canvas`'s pinned-canvas guard (which exists to keep every OTHER frame
## comparable), then the canvas is re-pinned.
const ULTRAWIDE_WINDOW_SIZE := Vector2i(2560, 900)

# How many frames `_ensure_canvas` / `_capture` keep re-asserting the pinned canvas while waiting for
# the WM to honour it. Bounded so a WM that refuses to shrink the window fails loudly, never hangs.
const CANVAS_PIN_MAX_FRAMES := 60
# How many CONSECUTIVE frames the window must hold the pinned canvas in `_stabilize_canvas` before the
# first state renders, and the bound on how long it waits for that. The maximize is applied — and
# RE-applied — asynchronously, so "it is the right size once" is not the same as "it stays".
const CANVAS_STABLE_FRAMES := 30
const CANVAS_STABLE_MAX_FRAMES := 600
# One manual step longer than any tween in the client, so `_settle`'s flush always reaches the end
# state (and fires the finished-callback) in a single `custom_step`.
const TWEEN_FLUSH_SECONDS := 3600.0
# Phase to seed the turn orb's calm breath at, as a fraction of `TurnOrb.PULSE_PERIOD`. The breath is
# `0.5 - 0.5 * cos(t)`, which is ZERO — its faintest, smallest instant — at phase 0, so freezing the
# clock there would render the pulse at the bottom of its range in all ~180 frames. A quarter period
# puts `cos` at 0, i.e. the breath's MIDPOINT, which is what an unfrozen frame averaged.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.25
# How far into the oral page-turn the live-arrival state drives its REAL tween before capturing, as a
# fraction of the panel's own duration — a chosen mid-motion phase in place of "however many frames
# the clock happened to give us". Must stay strictly inside (0, 1) to keep the tween RUNNING.
const TELLING_LIVE_TURN_FRACTION := 0.4
# The SECOND player band on the crowded hex (`_crowded_bands_fixture()[1]`, "Band Ash"). The Move
# assertion selects it deliberately: the faction default is the FIRST band, so a Move wired to
# anything but the list selection answers 301 instead.
const TILE_PANEL_MOVE_BAND_ENTITY := 302
# The Move button's face, in both hosts (the drawer's §18 button and the Band/City Orders block).
const MOVE_BUTTON_TEXT := "Move"
# Slice 1 reserved-dock probe: left-edge reservation width used to verify the HUD insets.
const RESERVED_PROBE_WIDTH := 300.0
# The crowded hex the sticky-land-selection state clicks, and a grid just large enough to contain it
# (the crowded fixtures all sit at 58, 24). Prairie steppe, matching that fixture's biome.
const STICKY_TILE := Vector2i(58, 24)
const STICKY_GRID_W := 64
const STICKY_GRID_H := 32
const STICKY_TERRAIN_ID := 11
# The deselect-keeps-the-tile state's two hexes, on the same grid: a hex carrying the lone herd, and
# an EMPTY land hex a few columns away (far enough that no marker or occupant can bleed into it, so a
# click there resolves as bare land).
const DESELECT_HERD_TILE := Vector2i(30, 16)
const DESELECT_LAND_TILE := Vector2i(34, 16)
# The lone herd on `DESELECT_HERD_TILE`. Named because the land-toggle assertions re-use that same
# one-occupant fixture and have to name the herd the cycle keeps coming back to.
const DESELECT_HERD_ID := "game_deer_405"
# The occupant-cycle state's hex, on the same grid and clear of the other two fixtures' hexes. ONE
# band and TWO herds share it, which is the smallest stack that can prove BOTH halves of issue #429:
# a herd under a band is reachable at all (a band-only prefix used to end the cycle), and a
# multi-herd hex is not stuck on `herds[0]`. The expected cycle order is bands-then-herds, so:
# the band, herd A, herd B, and back to the band.
const CYCLE_TILE := Vector2i(12, 8)
const CYCLE_BAND_ENTITY := 401
const CYCLE_HERD_FIRST_ID := "game_aurochs_429a"
const CYCLE_HERD_SECOND_ID := "game_boar_429b"
# Park the OS cursor over empty canvas before rendering. The HUD drops its hovered-hex record (and
# with it the targeting banner's hunt forecast) whenever the pointer sits over an interactive HUD
# control — see Hud._suppress_tooltip_over_ui. Wherever the cursor happened to be when the harness
# launched would otherwise decide whether the hover states render, making them non-deterministic.
const MOUSE_PARK_POSITION := Vector2(750, 640)
# The armed hunt party for the pre-launch forecast states (4 workers, matching the spec's worked
# example: a 4-worker party fills in ~6 turns on a mammoth but ~54 on red deer).
const HUNT_FORECAST_PARTY := 4
# The dialed-in hunter count for the LOCAL hunt preview states — deliberately dialed PAST every
# ceiling in them, so the stepper clamps it back to the sheet's own whole-animal carry cap exactly as
# it would for the player (`LOCAL_HUNT_CAPPED_CREW`; these frames render 3 hunters, not 6). The point
# survives the clamp: even the clamped crew out-carries every policy ceiling here, so the HERD (not
# the hunters) is still the binding constraint — which is exactly the case where the per-turn yield
# preview earns its keep.
const LOCAL_HUNT_HUNTERS := 6
## What the stepper actually renders once `LOCAL_HUNT_HUNTERS` is dialed in: the sheet's own
## whole-animal carry cap ("max 3 workers useful here"), Red Deer food_per_animal 2.0 ÷ the band's
## 0.8 per-worker carry = 3 carriers to haul one body. The dial is clamped to it exactly as it is
## for the player, so this — not 6 — is what a guard on those frames can assert.
const LOCAL_HUNT_CAPPED_CREW := 3
## The crews the two BOUND frames are composed at, and the numbers their steppers must SHOW: one
## bound by the band's idle labor, one by the maximum party size. Named because each is asserted
## against the rendered value, so the dial and the expectation are one number rather than two.
const LABOR_BOUND_CREW := 3
const PARTY_SIZE_BOUND_CREW := 2
## The crew the TWO-PRODUCT frames (issue #337) are composed with — the wolf's pelts-only pair and the
## oracle deer's food+trade control. Two hunters is the oracle's own no-waste point (food_per_animal
## 1.23 ÷ the band's 0.8 per-worker carry ⇒ 2 carriers haul one whole body), so the frame the trade
## components are read on carries no waste term to argue with; the wolf rides the same crew so the
## inedible quarry and the both-products control are compared at ONE party size.
const PELT_FRAME_HUNTERS := 2
# ---- THE FLOOR CHART's five cases (docs/plan_harvest_floor.md §7.3) -----------------------------
# A floor ABOVE a nearly-full patch's stock, so nothing stands above the line and the flag has to flip
# below it — the two things `floor_chart_full` is judged on.
const FLOOR_CHART_ABOVE_STOCK := 0.95
## A SECOND live-drag floor for the teaching line, and it has to sit BELOW this state's standing
## stock. The drag before it parks the floor ABOVE the stock, where the aside correctly reads
## "Teaching nothing: nothing is being taken" — and any other floor still above the stock reads the
## SAME sentence, so the assertion would compare a string with itself and pass on a line that never
## re-read. This value crosses the sim's work predicate, so the drag moves the aside from that end of
## the non-degeneracy rule to a live rate.
const FLOOR_CHART_TEACHING_DRAG_FLOOR := 0.10
## The faction's Cultivation while the chart block renders — part-learned, so its WILD patches still
## have a lesson to teach and the aside's teaching line exists to be dragged and compared at all. The
## frames above this block complete every track, and a source teaches nothing once its lesson is
## known; `forage_lesson_known` flips it back to 1.0 and asserts exactly that.
const FLOOR_CHART_CULTIVATION_LEARNING := 0.55
# A stock already drawn well below the food peak but comfortably above a plant's reseed floor: low
# enough that the projection's descent to the floor is legible, high enough that the curve has room to
# flatten rather than bottoming out in the first turn.
const FLOOR_CHART_DRAWN_STOCK_FRACTION := 0.35
# The floor that patch is worked to — under its stock, clear of the plot's baseline, so the curve's
# descent and the FLAT it holds afterwards are both legible.
const FLOOR_CHART_HELD_FLOOR := 0.20
# **BELOW `ecology.collapse_fraction` (0.15).** The herd is past its Allee threshold, so the sampled
# curve is NEGATIVE here and the projection must show a decline the crew did not cause.
const FLOOR_CHART_ALLEE_STOCK_FRACTION := 0.08
# A crew big enough to bite, small enough that "clear it now" and "hold it after" stay different
# numbers — a frame where the two targets coincide cannot show that there are two of them.
const FLOOR_CHART_CREW := 3
# `_crew_target_count`'s answer when the target is not rendered at all. NOT 0, which is a real reading
# ("nothing to clear"), and the distinction is the dead-season assertion's whole subject.
const CREW_TARGET_ABSENT := -1
## The two INVESTMENT-rung payoff terms the Wild Boar frame is judged on (issue #397), spelled out as
## literal strings rather than rebuilt from `SourceForecast.picker_products` — an assertion that
## re-derives the terms through the very formatter under test asserts nothing. Food leads, and each
## half appears because the boar pays both; the pre-fix face was the food clause alone.
##
## They moved off the PICKER's rung face onto the IMPROVEMENT control's own (issue #442), which is why
## the payoff ARROW is gone from them: the control's face already reads
## `◎ Tame this herd · then <terms>`, so a second arrow inside the terms said "then → 1.48" twice.
const BOAR_TAME_PAYOFF_FACE := "1.48 food · 0.37 trade"
const BOAR_CORRAL_PAYOFF_FACE := "2.95 food · 0.74 trade"
## The three forage-rung faces `_hay_meadow_tile_fixture` / `_dead_season_tile_fixture` are judged on
## (issue #426), spelled out as literals for the same reason as the boar pair above. The first two are
## the THREE-account line the plant web grew a column for, in wire order (provisions · trade goods ·
## fodder) and ascending on food and fodder between them. The third is the one surviving zero: a rung
## whose ceiling EXISTS and is empty says so, which is the whole difference between "pays nothing this
## season" and "the wire never described this patch".
# The preset metric as the TOOLTIP spells it (`SourceForecast.extractive_take_pair`'s `full`), which
# is where it lives now that the button face states the intent alone. The face's compact spelling
# (`0.60 food · 0.01 trade · 0.20 fodder`) has no surface left to be read from.
const HAY_PEAK_TOOLTIP := "up to +0.60/turn · ⇄ +0.01 trade goods/turn · +0.20 fodder/turn"
const HAY_STRIP_TOOLTIP := "up to +1.35/turn · ⇄ +0.02 trade goods/turn · +0.45 fodder/turn"
const DEAD_SEASON_TOOLTIP := "up to +0.00/turn"
## The crew the hay meadow's overdraw frame is composed at — the smallest that puts the FODDER take
## past its Sustain ceiling (3 × 0.13 = 0.39 against 0.20) while the FOOD take (0.24) is still inside
## the patch's 0.60. One forager overdraws nothing at all, so a smaller crew would pass that state's
## claim vacuously.
const HAY_OVERDRAW_FORAGERS := 8
## Which line of a rung's two-line face carries the metric: line 0 is the rung NAME
## (`HudFormat.policy_face`), line 1 the products (`HudWidgets._policy_rung_cell` builds them in that
## order). A rung with no metric wears line 0 alone.
const POLICY_RUNG_METRIC_LINE := 1
## "no count dialed in" for `_compose_herd` — a real dial can be 0 (an unstaffed compose), so the
## sentinel has to sit outside the valid range rather than reuse 0.
const COMPOSE_COUNT_UNSET := -1
# The crowded hex's staffed-wildlife-row state: the SAME herd worked both ways at once. Two distinct
# counts so the row's meta can only read right if it SUMS them (4 + 6 = `10 🏹`) — a single shared
# number would pass even if one source were dropped.
const OCCUPANTS_HUNT_LOCAL_WORKERS := 4
const OCCUPANTS_HUNT_PARTY_WORKERS := 6
# The sim's forward-SIMULATED turns-to-fill for the 4-worker party in these states (it exports the
# answer; the client never divides). Sustain is a small renewable flow → slow; Surplus/Deplete strip the
# herd's stock headroom first → fast. The deer's Sustain trip (54) blows past the 20-turn viability
# threshold; its Surplus trip (6) does not — same herd, same party, opposite verdicts.
const MAMMOTH_SUSTAIN_TRIP_TURNS := 6
const DEER_SUSTAIN_TRIP_TURNS := 54
const DEER_SURPLUS_TRIP_TURNS := 6
const MAMMOTH_SURPLUS_TRIP_TURNS := 3
# The whole animals the 4-worker RAID delivers (HuntTripEstimate.animalsTaken) — the payload the readout
# headlines. A viable/slow raid lands a positive count; a herd at/below its policy floor lands 0 (the
# no-surplus state). Surplus/Deplete raid deeper than Sustain, so a deeper policy lands MORE animals.
const MAMMOTH_SUSTAIN_ANIMALS := 8
const DEER_SUSTAIN_ANIMALS := 6
const DEER_SURPLUS_ANIMALS := 12
const NO_SURPLUS_ANIMALS := 0
# The server's measured Wild Boar raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter → 5 animals /
# 7 turns, 2 → 8 / 8, 3 → 8 / 4. animalsTaken PLATEAUS at 8 (party 2), so max-useful = 2 hunters — the
# frame the "delivers ≈5 boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged on.
const BOAR_RAID_ANIMALS := [5, 8, 8, 8, 8, 8, 8, 8]
const BOAR_RAID_TURNS := [7, 8, 4, 3, 3, 3, 3, 3]
const BOAR_FOOD_PER_ANIMAL := 4.0
# The Thunder Mammoth's food quantum — big enough that no fieldable party can carry a whole one, which
# is what makes `_partial_waste_mammoth` the WASTE fixture: a party of `w` hauls ~`w` of the 16 and
# rots the rest. Named here rather than left a local because the waste assertion computes the same
# percentage the readout prints, and two spellings of one quantum would drift.
const MAMMOTH_FOOD_PER_ANIMAL := 16.0
# One animal's worth of TRADE GOODS on an EDIBLE quarry (issue #337) — a hunt pays a vector, so every
# raid cell carries a trade payload beside its food one and the readout names both. Deliberately much
# smaller than the food quantum: a deer/boar is meat first, hide second (the INEDIBLE case, where trade
# is the whole payload, is the wolf fixture below).
const RAID_TRADE_PER_ANIMAL := 0.5
# The DISTANCE frames' raid (`_hunt_distance_herd`, the reference Red Deer at 2.0 food/animal): a party
# of `i+1` lands `DISTANCE_RAID_ANIMALS[i]` animals in `DISTANCE_RAID_TURNS[i]` HUNTING turns. Those
# frames open at the seeded party of 1, so the first cell is the one they render; the plateau at 3
# animals-taken keeps the party stepper's max-useful cap meaningful rather than unbounded. The turns sit
# well inside a band's `expedition_viability_warn_turns`, so the trip verdict reads OK there and the
# slow/long raids stay the business of the fixtures built for them.
const DISTANCE_RAID_ANIMALS := [3, 5, 6, 6, 6, 6, 6, 6]
const DISTANCE_RAID_TURNS := [9, 7, 6, 6, 6, 6, 6, 6]
# The `herd_hunt_raid_travel` frame's two halves, named so the split assertion states the arithmetic
# rather than a pair of literals: `_raid_travel_band` sits 8 tiles from the (66,10) boar and moves 2
# tiles a turn, so the round trip is ceil(2 × 8 / 2), and the boar's own 2-party cell fills in 8
# hunting turns (`BOAR_RAID_TURNS[1]`). 16 total, inside the band's 20-turn warn line.
const RAID_TRAVEL_TURNS := 8
const RAID_TRAVEL_HUNT_TURNS := 8
# 0 = the raid ran the whole forecast horizon still delivering (a long raid), used by the no-surplus /
# collapsed fixtures where the raid also lands 0 animals.
const NEVER_FILLS_TRIP_TURNS := 0
# The Telling fixture's two authored voice registers. Named here ONLY so the harness can pin the
# preference deterministically — nothing in the client hardcodes a register (VoiceLine.register is
# free-form by design; the panel builds its toggle from what the fork actually carries).
const FORK_REGISTER_MYTHIC := "mythic"
const FORK_REGISTER_WARM := "warm"
# The Telling panel's medium rungs. Named here only so the states read; the client keys its styling
# off a table with an `oral` fallback, never off these three being exhaustive.
const TELLING_MEDIUM_ORAL := "oral"
const TELLING_MEDIUM_PAINTED := "painted"
const TELLING_MEDIUM_WRITTEN := "written"
# The pen-keeping band's entity id — its own, so its Food disclosure key (`food:<entity>`) doesn't
# collide with the reference band's.
const PEN_KEEPER_BAND_ENTITY := 906
# The reference band (`_band_fixture()`, entity 904) disclosure keys — the `[url]` meta its Food /
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for it.
const BAND_DISCLOSURE_FOOD := "food:904"
const BAND_DISCLOSURE_MORALE := "morale:904"
const BAND_DISCLOSURE_GROWTH := "growth:904"
# The collapsed-growth band is `_concerning_food_band_fixture`'s entity (905), not 904.
const BAND_DISCLOSURE_GROWTH_COLLAPSED := "growth:905"
# The Red Deer pen at its settled escapement point (design doc §7, MEASURED from a sim run): the
# feed the herd demands per turn, and the share of it a broke keeper managed to pay in the starving
# state. `pen_fed_fraction` < 1 ⇒ the herd is shrinking.
const PEN_UPKEEP_RED_DEER := 1.74
const PEN_FED_STARVING := 0.40
# The herder-deficit state's staffing PAIR (`herd_corral_under_herded`). The growing corral needs 2
# herders every turn to hold its tameness while only 1 is staffed, and the two numbers must DISAGREE
# for the deficit to render at all — so the fixture's `herders_needed`, the band's dialed-down hunt
# assignment and the auto-max assertion all read them from here rather than repeating bare 1s and 2s.
const UNDER_HERDED_CORRAL_HERDERS_NEEDED := 2
const UNDER_HERDED_CORRAL_HERDERS_STAFFED := 1
# The species name every orb row that names a herd must quote. `Hud._herd_label_for_id` resolves
# `game_deer_07` through the roster, the current selection and the world-herd list in that order, and
# every fixture carrying that id declares the same `species` — so the alert text is asserted against the
# ONE string all three lookups answer, never against a hand-typed copy of it.
const RED_DEER_LABEL := "Red Deer"
# The unworked-rung / under-crewed state's wire numbers (`turn_orb_unworked_rung`).
# `neglectGraceRemaining` ships as `(grace + 1) - neglect`, so every one of these is a COUNTDOWN to the
# penalty, never a count of neglected turns:
#   • NEGLECT_GRACE_SOON — the tended patch has 2 turns left. Deliberately not 1: the countdown
#     interpolates `ATTENTION_TURN_PLURAL_SUFFIX`, and at 1 the suffix is empty, so a row that dropped
#     the plural entirely would still match.
#   • NEGLECT_GRACE_NOW — the wire's `0`, which is "the ground is reverting THIS turn", the most urgent
#     reading there is. It must never render as a `0`-turn countdown.
#   • NEGLECT_GRACE_FULL — what a source that IS being kept reads (the rung's whole window). The worked
#     control carries it, so its silence is the WORKED test and not an incidentally absent countdown.
#   • NEGLECT_GRACE_HERD — the animal web's twin, on the under-crewed herd; plural for the same reason.
# The third patch of the set has NO number here at all: it carries `has_neglect_grace == false`
# (nothing at risk), which is the one reading the pair of fields exists to keep distinct from the zero.
const NEGLECT_GRACE_SOON := 2
const NEGLECT_GRACE_NOW := 0
const NEGLECT_GRACE_FULL := 4
const NEGLECT_GRACE_HERD := 3
# The under-crewed herd's staffing PAIR — 2 keepers standing where the sim demands 4. Like the
# corral-deficit pair above they must DISAGREE for the alert to fire at all, and both halves are read
# back off the RENDERED row (`2 of 4 keepers — sheds in 3 turns`).
const UNDER_CREWED_HERD_STAFFED := 2
const UNDER_CREWED_HERD_NEEDED := 4
# What the orb's registry must hold in that state: THREE unworked-rung rows out of six staged patches
# (the wild one, the rival's and the worked one raise nothing) plus the ONE under-crewed herd row.
# Counted rather than searched, because a producer that alarmed on every source would satisfy every
# positive assertion in the block without this one.
const UNWORKED_EXPECTED_ROWS := 4
# Somebody else's faction — the owner of the "not ours" control patch. Derived from the player's id so
# the two can never be written equal, which would silently turn that negative control into a positive.
const RIVAL_FACTION_ID := HudConst.PLAYER_FACTION_ID + 1
# The stale-DATA reopen guard's herd (`herd_compose_reopen_fresh`), staged on ONE id across two turns.
# Turn N it is still WILD — not owned, so the ownership-gated crew is 0 and only the would-be crew is
# real. Turn N+1 the sim has taken ownership: a real crew of 4 (both halves equal, as the sim exports
# them on a managed herd) and a meter that has just left zero. Every number the sheet quotes moves
# between the two, and none of them is in the drawer's shape signature — which is the whole test.
const REOPEN_HERD_ID := "game_deer_reopen_01"
const REOPEN_WILD_WOULD_BE_HERDERS := 3
const REOPEN_TAMING_HERDERS := 4
const REOPEN_TAMING_DOMESTICATION := 0.04
# Idle workers comfortably above BOTH max-useful caps (3 wild / 4 taming), so the stepper is bound by
# USEFULNESS and renders the "max N workers useful here" note rather than the labor-bound one.
const REOPEN_IDLE_WORKERS := 12
const REOPEN_WORKING_AGE := 24
# The CREW-NOUN guard's pen-ready herd (`herd_compose_crew_noun_after_pen`) — the one the player ticks
# Corral on FIRST, so it is the source of the improvement the NEXT herd's header must not inherit. Its
# id is deliberately distinct from `_herd_fixture`'s, since a same-id re-open is not a source change and
# would not stage the bug.
const CREW_NOUN_PEN_HERD_ID := "game_aurochs_crewnoun"
## The crew the WILD herd of that pair would owe if it were ever tamed — its ownership-gated count is 0.
const CREW_NOUN_WILD_WOULD_BE_HERDERS := 3
# The quick-hunt axis guard's herd, and idle workers for the shortcut to have something to send (the
# `quick_hunt_note` state beside it deliberately runs at 0, which is the no-op case).
const QUICK_HUNT_HERD_ID := "game_aurochs_quickhunt"
const QUICK_HUNT_IDLE_WORKERS := 3
# `_band_fixture` carries no `band_id` (nothing else here emits a command), and `Hud._emit_assign_labor`
# REFUSES a band without one — so the shortcut would no-op silently and the guard would pass on nothing.
const QUICK_HUNT_BAND_ID := 9041
# The three fog-of-war states MapView tags onto tile_info (mirrors Hud.VISIBILITY_*).
const VIS_ACTIVE := "active"
const VIS_DISCOVERED := "discovered"
const VIS_UNEXPLORED := "unexplored"

# Hex-edge river fixtures. The wire mask is 12 bits, 2 bits per odd-r direction, in the SIM's
# direction order (clockwise from E: 0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE) — built here with the
# same RiverEdges vocabulary the UI decodes with, so the fixture can't drift from the contract.
const RIVER_MASK_NONE := 0
# Minor on E + SE — one class, so one row.
const RIVER_MASK_SINGLE_CLASS := (
	(RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 0))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 1))
)
# Major on NE + NW, Minor on SW — the two-class case: "Major River: NE, NW" then "Minor River: SW".
const RIVER_MASK_TWO_CLASS := (
	(RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 5))
	| (RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 4))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 2))
)

var _hud: HudLayer

## Every compose spine captured this run, keyed by the sheet it came from (see `_record_compose_spine`).
## A DICT rather than two fields because the parity assertion is about the RELATION between them, and a
## missing capture must fail loudly rather than compare an empty array against another empty one.
var _compose_spines := {}


# ---- LEGACY FIXTURE ADAPTER: the four stances -> the escapement floor ---------------------------
# Every fixture in this file states a source's take as the retired per-STANCE ceiling table, because
# that is what the wire carried when they were written. The wire carries the per-biomass yield VECTOR
# now (`docs/plan_harvest_floor.md` §5) and the client composes `max(0, B - floor*K) x rate` at any
# floor, so the tables are converted HERE, in one place, rather than by rewriting ~50 literals.
#
# **THE CONVERSION PINS THE OLD `sustain` ROW TO THE FOOD PEAK**, which is the honest mapping: Sustain
# took the herd's renewable yield and the food peak is the floor that pays the most forever. So every
# frame's headline number at the DEFAULT floor is the number these fixtures were tuned to show, and
# what changes is that the other two presets now read off one curve instead of four authored rows.
#
# `B` and `K` come from the fixture when it carries a usable pair; otherwise they are seeded, because
# a fixture written before the floor existed had no reason to state a stock the client would divide
# by. The seeded pair leaves a real spread across the presets (strip 2.25x the peak, learn 0.25x).
const FIXTURE_CAPACITY := 100.0
const FIXTURE_STOCK_FRACTION := 0.9

# ---- THE GROWTH TERMS THE FIXTURES PREDATE (slice 4b) -------------------------------------------
# `perWorkerBiomass` and `regrowthSamples` are wire fields no fixture written before them can carry,
# and the chart needs BOTH — without a curve it renders nothing at all, which would silently drop the
# instrument out of ~50 frames. So the adapter seeds them, in the SAME one place it converts the
# stances, and it is careful about which of the two webs it is standing in for.
#
# **THE HARNESS IS STANDING IN FOR THE SIM HERE, and that is the one place a growth model may be
# written in GDScript.** These constants are the shipped config's (`labor_config.forage.ecology` /
# `fauna_config.ecology`) and the shapes are the two the sim publishes: a patch is logistic lifted to
# its reseed floor and therefore NEVER negative, a herd declines at `collapse_rate` below its Allee
# threshold and therefore IS. A fixture that flattened that asymmetry would let the chart clamp a
# herd's crash to zero and still look right.
const FIXTURE_REGROWTH_SAMPLES := 11
const FIXTURE_PLANT_REGROWTH_RATE := 0.25
const FIXTURE_ANIMAL_REGROWTH_RATE := 0.05
# **THE PHASE BANDS, WHICH ARE ALSO THE ANIMAL CURVE'S ALLEE POINT.** `collapse_fraction` is one
# number in the sim doing two jobs on the animal web — the boundary `classify_ecology_phase` calls a
# herd Collapsing at, and the stock `net_biomass_delta` turns negative below — so the seeded curve and
# the seeded zone read it from ONE constant here too. Splitting them would let a fixture draw a chart
# whose red band and whose crash begin at different heights, which is precisely the disagreement
# `floor_chart_herd_allee` exists to catch. `labor_config.forage.ecology` and `fauna_config.ecology`
# state the same pair today (0.15 / 0.40); the plant web simply has no Allee term behind its cut.
const FIXTURE_COLLAPSE_FRACTION := 0.15
const FIXTURE_STRESSED_FRACTION := 0.40
const FIXTURE_COLLAPSE_RATE := 0.20
const FIXTURE_RESEED_FLOOR_FRACTION := 0.02
# `per_worker_biomass_capacity` for each web, used only where the fixture's own rates cannot state the
# throughput (a source that pays no food — the exact case the wire field was added for).
const FIXTURE_PLANT_PER_WORKER_BIOMASS := 8.0
const FIXTURE_ANIMAL_PER_WORKER_BIOMASS := 40.0

# ---- THE STALE-VERB PATCH: the played tile, at the SHIPPED numbers ------------------------------
# Reported from play, and the reason `SourceForecast.live_improvement` exists. Every constant here is
# a shipped config value rather than a fixture convenience, because the defect is only visible at the
# proportions a live patch has: `crew_to_hold` divides a regrowth the LAND owns by a carry the CREW
# owns, so the 4× a stale build dip puts on the crew shows up as a crew target 3× too large, and a
# fixture whose regrowth is small next to its carry rounds the whole error away.
const STALE_VERB_CAPACITY := 195.0
# Just above the floor it is worked at, so the crew is REGROWTH-bound rather than room-bound — the
# steady state in which "how many hands hold this patch" is the question the sheet is answering.
const STALE_VERB_STOCK := 112.0
const STALE_VERB_FLOOR := 0.57
const STALE_VERB_CREW := 2
# `labor_config.forage.per_worker_biomass_capacity` (8.0) × the tile's seasonal weight. Worldgen sets
# every food module's weight to `INITIAL_SEASONAL_WEIGHT` (1.0) and no system ever moves it, so this
# IS a live patch's published throughput — the season is not what dips a forager today.
const STALE_VERB_PER_WORKER_BIOMASS := 8.0
# The basket's share-weighted food rate: wild tubers 0.35 × 0.065 + wild rice 0.15 × 0.070. Cotton and
# flax pay no food at all, which is why the patch converts at well under `provisions_per_biomass`.
const STALE_VERB_FOOD_PER_BIOMASS := 0.03325
# …and the same basket's trade rate, which the two cash crops carry: 0.35 × 0.005 + 0.30 × 0.200 +
# 0.20 × 0.150 + 0.15 × 0.005.
const STALE_VERB_TRADE_PER_BIOMASS := 0.0925
# The plant rungs' `yield_fraction_while_building` (`intensification_ladder.json`) — the factor that
# must NOT ride a crew whose build has already landed.
const STALE_VERB_BUILD_FRACTION := 0.25
# Two throughputs are "the same" when they agree to within the resolution the panel states a rate at.
const STALE_VERB_THROUGHPUT_EPSILON := 0.01

# ---- THE BUILDING PATCH: the regime where the REGROWTH beats the ROOM ---------------------------
# Reported from play, and the frame three separate defects appear in AT ONCE — none of them visible
# on any other fixture, because all three need the same narrow regime: a crew whose whole-turn carry
# is a shade UNDER the patch's own regrowth. There the standing room is a puddle, the regrowth is a
# river, and the sheet's four numbers stop agreeing with one another:
#
#   • `clear it now` was `room ÷ carry` = 5 — a crew that provably clears nothing, since the patch
#     regrows more each turn than those five hands can lift, printed two lines above a verdict saying
#     seven are needed. It is now floored on the reaching crew.
#   • `⚠ OVERDRAWS THE PATCH` fired beside a verdict reading *it settles at 54% and holds there*: the
#     take-vs-food-peak test is `take > 0` on a patch standing at the peak, i.e. a fact about the
#     FLOOR. Gated on the projection now.
#   • Nothing said the crew was at QUARTER throughput, which is where every "impossible" number here
#     comes from. The crew row says it now.
#
# **THE ARITHMETIC WAS NEVER WRONG — the numbers only disagreed with each other**, so every constant
# is the shipped one (the stale-verb patch's basket and carry, `intensification_ladder.json`'s 0.25)
# and the assertions below are RELATIONS between the rendered numbers, not literals: a fixture that
# drifts must fail rather than quietly re-baseline.
const BUILD_DIP_CAPACITY := 195.0
# Just under the food peak (97.5), so the room above the 45% floor is ~9 biomass while the patch
# regrows ~12 — the inversion the whole frame rests on. Also makes the food-peak ceiling ZERO, which
# is what let the overdraw test degenerate into "the floor is below the peak".
const BUILD_DIP_STOCK := 97.0
const BUILD_DIP_FLOOR := 0.45
# Six foragers × (8.0 × 0.25) = 12.0 biomass/turn — a shade under the ~12.19 the patch regrows at its
# peak, so the stock RISES under this crew and settles above the floor. One more hand reverses it,
# which is what makes the pair an A/B on the overdraw gate rather than one frame's say-so.
const BUILD_DIP_CREW := 6
const BUILD_DIP_DECLINE_CREW := 7
# The rung's own build crew (`<rung>CrewNeeded`) and the band's hands. The band must be able to REACH
# the reaching crew, or the cap — not the fix — is what the frame would be measuring.
const BUILD_DIP_CREW_NEEDED := 2
const BUILD_DIP_IDLE_WORKERS := 9

# ---- THE BUILDING HERD: the regime where the dip drops the crew BELOW ONE BODY ------------------
# **THE ANIMAL WEB'S HALF OF THE SAME DEFECT**, and it fails DIFFERENTLY from the plant one, which is
# why it needs its own frame. A hunt take is quantised to whole animals AFTER the crew's collection is
# dipped (`hunt_take` → `quantise_animal_take`), and that rule's `max(1, carryable)` means a crew the
# build drops below one body does NOT simply take a fraction less: it still kills one animal and
# WASTES what it cannot haul. So the dip moves the waste line, not merely the take, and a fixture whose
# crew stays above one body the whole way through cannot see it.
#
# Every constant is a SHIPPED one. The species is the roster's heaviest TAMEABLE animal — a Steppe
# Runner (`fauna_config` `body_mass` 120, `husbandry_ceiling` "pastoral") — because the regime needs a
# body heavier than a few hands can carry AND a rung the sheet will actually offer: the heavier
# mammoth is `wild`-ceilinged and can never be tamed, so composing a Tame on one would stage a build
# the sim refuses.
const HERD_DIP_BODY_MASS := 120.0
# `fauna_config` `hunt.provisions_per_biomass` / `trade_goods_per_biomass`, and `labor_config`'s
# `hunt.per_worker_biomass_capacity` — the three rates that turn biomass into this sheet's numbers.
const HERD_DIP_PROVISIONS_PER_BIOMASS := 0.02
const HERD_DIP_TRADE_PER_BIOMASS := 0.005
const HERD_DIP_PER_WORKER_BIOMASS := 40.0
# A migratory herd's own scale (`fauna_config` mammoth/steppe-runner `biomass` 4000–12000). The stock
# sits a WHISKER above half its capacity, which is what puts the food-peak ceiling (0.4 food/turn) far
# below the take and leaves the ⚠ free to fire — while the herd's regrowth there (~112 biomass/turn)
# sits BETWEEN the dipped crew's carry (80) and the undipped one's (160), so the same crew draws this
# herd down while hunting it and does not while gentling it. That is the whole A/B.
const HERD_DIP_CAPACITY := 9000.0
const HERD_DIP_STOCK := 4520.0
# Below the food peak, so the take is judged against a ceiling it can exceed — the only way the
# overdraw flag can testify about anything on this frame.
const HERD_DIP_FLOOR := 0.30
# Four hunters carry 3.2 food undipped — one whole 2.4-food body, with change — and 1.6 under the
# build, which is two thirds of a body: the forced-partial branch, and a third of the kill left behind.
const HERD_DIP_CREW := 4
# `intensification_ladder.json`, animal:pastoral `yield_fraction_while_building`. The animal rungs dip
# to a HALF where the plant rungs dip to a quarter — do not carry the plant number across.
const HERD_DIP_BUILD_FRACTION := 0.5
# The crew this herd WOULD owe once managed (`herders_needed_if_managed`); its ownership-gated twin is
# 0, because a herd mid-Tame is not owned yet. Under the composed crew, so the investment rung's cap
# floor is not what the frame ends up measuring.
const HERD_DIP_WOULD_BE_HERDERS := 3
const HERD_DIP_IDLE_WORKERS := 12

## Rewrite one source dict IN PLACE. `prefix` is "" for a raw herd / wire patch, `patch_` for the
## tile_info cross-ref. Returns the same dict, so call sites read `_floorify(fixture)`.
func _floorify(src: Dictionary, prefix: String = "") -> Dictionary:
	if src.is_empty():
		return src
	_floorify_ceilings(src, prefix)
	_seed_growth_terms(src, prefix)
	return src

## Is this dict a HERD? A herd carries `species`; a forage patch carries `committed_species` and never
## a bare one, and the `patch_` prefix settles the tile_info case outright. It decides which growth
## SHAPE the seeded curve takes, so guessing wrong would hand a patch a herd's crash.
func _fixture_is_herd(src: Dictionary, prefix: String) -> bool:
	return prefix == "" and src.has("species")

## Seed `per_worker_biomass` + `regrowth_samples` + the two phase-band cuts on a fixture that predates
## them. Each is skipped when the fixture states its own, so a state authored to exercise a particular
## curve — or a particular boundary — keeps it.
func _seed_growth_terms(src: Dictionary, prefix: String) -> void:
	var is_herd := _fixture_is_herd(src, prefix)
	if not src.has(prefix + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY):
		# Recover it from the fixture's own numbers where they can state it — that is EXACT and keeps
		# every existing frame's expected-yield line unchanged — and fall back to the config's
		# throughput on a source that pays no food, where the recovery is `0/0`.
		var rate := float(src.get(prefix + "provisions_per_biomass", 0.0))
		var per_worker := float(src.get(prefix + "per_worker_yield", 0.0))
		var carry := (per_worker / rate) if rate > 0.0 and per_worker > 0.0 \
			else (FIXTURE_ANIMAL_PER_WORKER_BIOMASS if is_herd else FIXTURE_PLANT_PER_WORKER_BIOMASS)
		src[prefix + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY] = carry
	var capacity := float(src.get(prefix + "carrying_capacity", 0.0))
	if capacity > 0.0 and not src.has(prefix + SourceForecast.FORECAST_REGROWTH_SAMPLES_KEY):
		var samples := PackedFloat32Array()
		for i in range(FIXTURE_REGROWTH_SAMPLES):
			var fraction := float(i) / float(FIXTURE_REGROWTH_SAMPLES - 1)
			samples.push_back(_fixture_regrowth_delta(fraction, capacity, is_herd))
		src[prefix + SourceForecast.FORECAST_REGROWTH_SAMPLES_KEY] = samples
	# THE PHASE BANDS the chart draws as zones. Seeded on BOTH webs (the cut points are ecology config,
	# which every source has) and skipped when a fixture states its own, so a state authored to put a
	# particular boundary under the floor line keeps it.
	if not src.has(prefix + SourceForecast.FORECAST_COLLAPSE_FRACTION_KEY):
		src[prefix + SourceForecast.FORECAST_COLLAPSE_FRACTION_KEY] = FIXTURE_COLLAPSE_FRACTION
	if not src.has(prefix + SourceForecast.FORECAST_STRESSED_FRACTION_KEY):
		src[prefix + SourceForecast.FORECAST_STRESSED_FRACTION_KEY] = FIXTURE_STRESSED_FRACTION
	if not is_herd:
		return
	# **THE WHOLE-ANIMAL QUANTUM, IN BIOMASS.** `crew_to_hold` rounds up to one body on this web
	# (mirroring the sim's `hunt_haul_workers`), and `body_mass` is the term it rounds to — in the same
	# units as the curve, unlike `food_per_animal`, which is that body already converted to provisions.
	# Derived from the fixture's own pair on whichever account the species pays, so it cannot disagree
	# with the rates beside it; a species that pays neither leaves it absent and the rounding is simply
	# not applied.
	if src.has(prefix + SourceForecast.FORECAST_BODY_MASS_KEY):
		return
	for pair in [["food_per_animal", "provisions_per_biomass"], ["trade_per_animal", "trade_per_biomass"]]:
		var per_animal := float(src.get(prefix + String(pair[0]), 0.0))
		var rate := float(src.get(prefix + String(pair[1]), 0.0))
		if per_animal > 0.0 and rate > 0.0:
			src[prefix + SourceForecast.FORECAST_BODY_MASS_KEY] = per_animal / rate
			return

## One sample of the seeded curve: the source's one-turn biomass delta at `fraction` of K.
func _fixture_regrowth_delta(fraction: float, capacity: float, is_herd: bool) -> float:
	var stock := fraction * capacity
	if is_herd:
		# **THE ANIMAL CURVE GOES NEGATIVE BELOW THE ALLEE POINT.** Past that threshold the herd
		# declines whether or not it is hunted, which is why floor 0 ENDS a herd on this web.
		if fraction < FIXTURE_COLLAPSE_FRACTION:
			return -FIXTURE_COLLAPSE_RATE * stock
		return FIXTURE_ANIMAL_REGROWTH_RATE * stock * (1.0 - fraction)
	# **THE PLANT CURVE NEVER DOES.** A stripped stand is lifted to its reseed floor before it
	# regrows, so the delta at 0 is the lift itself — positive, and the reason a patch comes back.
	var lift := maxf(stock, FIXTURE_RESEED_FLOOR_FRACTION * capacity)
	var grown := minf(capacity, lift + FIXTURE_PLANT_REGROWTH_RATE * lift * (1.0 - lift / capacity))
	return grown - stock

func _floorify_ceilings(src: Dictionary, prefix: String) -> void:
	var legacy := "hunt_policy_ceilings" if prefix == "" and src.has("hunt_policy_ceilings") \
		else "forage_policy_ceilings"
	var rows: Variant = src.get(prefix + legacy, null)
	if not (rows is Dictionary):
		_floorify_estimates(src)
		return
	var peak_food := float((rows as Dictionary).get("sustain", 0.0))
	var peak_trade := _legacy_peak(src, prefix, legacy + "_trade" if legacy.begins_with("forage") \
		else "hunt_policy_trade_ceilings")
	var peak_fodder := _legacy_peak(src, prefix, "forage_policy_fodder_ceilings")
	# The stock the ceiling is composed from. Reuse the fixture's own pair when it leaves real room
	# above the peak; otherwise seed one, since dividing by a zero room would fabricate an infinity.
	# **A SOURCE WITH A POSITIVE FOOD-PEAK CEILING IS BY DEFINITION ABOVE THE PEAK**, and several
	# fixtures predate that being expressible: they author a healthy Sustain take on a herd standing
	# BELOW `K/2`, which the four-row model let them get away with and the one-curve model cannot. The
	# capacity is kept (the drawer's "Biomass: B / K" pair is a readout of its own) and the stock is
	# raised to `FIXTURE_STOCK_FRACTION` of it, which is what the authored ceiling was always claiming.
	var capacity := float(src.get(prefix + "carrying_capacity", 0.0))
	var biomass := float(src.get(prefix + "biomass", 0.0))
	# **A SOURCE WITH NO CAPACITY HAS NO FLOOR AXIS AT ALL** — `max(0, B - floor*K)` is `B` at every
	# floor when `K` is 0, so every preset would quote one number and the picker would silently claim
	# the dial does nothing. Several fixtures state a stock without one (nothing read it before), so a
	# capacity is derived from the stock rather than the other way round, which leaves the drawer's
	# "Biomass" reading untouched.
	if capacity <= 0.0:
		capacity = (biomass / FIXTURE_STOCK_FRACTION) if biomass > 0.0 else FIXTURE_CAPACITY
		src[prefix + "carrying_capacity"] = capacity
	var room := biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
	if room <= 0.0:
		biomass = FIXTURE_STOCK_FRACTION * capacity
		room = biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
		src[prefix + "biomass"] = biomass
	src[prefix + "provisions_per_biomass"] = peak_food / room
	src[prefix + "trade_per_biomass"] = peak_trade / room
	src[prefix + "fodder_per_biomass"] = peak_fodder / room
	for key in ["hunt_policy_ceilings", "hunt_policy_trade_ceilings", "forage_policy_ceilings",
			"forage_policy_trade_ceilings", "forage_policy_fodder_ceilings",
			"forage_policy_per_worker", "forage_policy_per_worker_trade",
			"forage_policy_per_worker_fodder"]:
		src.erase(prefix + key)
	_floorify_estimates(src)

func _legacy_peak(src: Dictionary, prefix: String, key: String) -> float:
	var rows: Variant = src.get(prefix + key, null)
	return float((rows as Dictionary).get("sustain", 0.0)) if rows is Dictionary else 0.0

## The FLOOR each retired stance stood for, so a converted raid table lands on the sim's own sampled
## floors (`snapshot::RAID_FORECAST_FLOOR_SAMPLES` = 0.0, 0.15, 0.30, 0.50, 0.80). Sustain is the food
## peak; the other three are the successively deeper draws they named.
const LEGACY_STANCE_FLOORS := {
	"sustain": 0.5, "surplus": 0.3, "deplete": 0.15, "eradicate": 0.0,
}

## Re-key a legacy `"<stance>:<party>"` raid table onto `"<floor>:<party>"`, and put the two fields
## the client SCANS on each row (`floor` / `party_workers`) — it no longer rebuilds the key, since the
## real key renders the floor with Rust's float Display.
##
## **IT MUST BE IDEMPOTENT, AND IT WAS NOT.** A converted row's key is `"0.5:4"`, whose leading token
## is not a stance, so a SECOND pass over the same dict skipped every row and left an EMPTY table
## behind — and `_floorify_ceilings` reaches here even on its early return, so any state that calls
## `_show_herd(h)` and then `_compose_herd(h)` with the SAME dict silently lost its whole raid table.
## Every expedition frame in the `_hunt_assign_forecast_states` block and the boar-raid set did exactly
## that: `hunt_trip_forecast` answered `available: false`, the sheet rendered no forecast at all, and
## the states went on passing because nothing asserted on a readout those frames no longer had. A row
## already carrying the floor field is therefore kept verbatim rather than dropped.
func _floorify_estimates(src: Dictionary) -> Dictionary:
	var estimates: Variant = src.get("hunt_trip_estimates", null)
	if not (estimates is Dictionary):
		return src
	var rekeyed := {}
	for key in (estimates as Dictionary):
		var converted: Variant = (estimates as Dictionary)[key]
		if converted is Dictionary \
				and (converted as Dictionary).has(SourceForecast.HUNT_ESTIMATE_FLOOR_KEY):
			rekeyed[key] = converted
			continue
		var parts := String(key).split(":")
		if parts.size() != 2:
			continue
		var stance := String(parts[0])
		if not LEGACY_STANCE_FLOORS.has(stance):
			continue
		var floor_value := float(LEGACY_STANCE_FLOORS[stance])
		var party := int(parts[1])
		var row: Dictionary = (estimates as Dictionary)[key].duplicate()
		row["floor"] = floor_value
		row["party_workers"] = party
		rekeyed["%s:%d" % [str(floor_value), party]] = row
	src["hunt_trip_estimates"] = rekeyed
	return src


## The harness's ONE gate into the HUD for a source fixture: everything goes through `_floorify`
## first, so no state can accidentally hand the panel a retired per-stance table (which would render
## as a silent zero rather than as a failure).
func _show_herd(herd: Dictionary) -> void:
	_hud.show_herd_selection(_floorify(herd))

func _show_tile(tile: Dictionary) -> void:
	_hud.show_tile_selection(_floorify(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX))

func _set_world_herds(herds: Array) -> void:
	for h in herds:
		if h is Dictionary:
			_floorify(h)
	_hud.update_herds(herds)

func _set_forage_patches(patches: Array) -> void:
	for p in patches:
		if p is Dictionary:
			_floorify(p)
	_hud.update_forage_patches(patches)


func _ready() -> void:
	# FREEZE ANIMATION TIME — the same treatment `map_preview` and `blend_probe` carry, and taken for
	# the same reason: a frame that varies run-to-run cannot be pixel-diffed to prove a HUD refactor
	# changed nothing. Measured before the freeze, two runs of IDENTICAL code differed byte-wise in
	# 184 of 184 frames (the turn orb's calm breath animates in every frame there is).
	#
	# What survives phase 0 was CHECKED against the draw code, not assumed:
	#   • the awaiting-expedition / targeting pulses (`0.5 + 0.5 * sin(t)` and `base + amp * sin(t)`)
	#     are MapView's, and this harness's MapView is `visible = false` — data only;
	#   • the turn orb's breath is `0.5 - 0.5 * cos(t)`, which DEGENERATES to its minimum at phase 0,
	#     so its phase is seeded to the midpoint below rather than left at 0;
	#   • the ONE tween in the whole client (TellingPanel's page turn) never advances at time_scale 0,
	#     so `_settle` drives live tweens to their END state — see `_flush_tweens`.
	# `_settle` waits on `process_frame`, which still fires at time_scale 0.
	Engine.time_scale = 0.0
	_pin_canvas(get_window())
	DirAccess.make_dir_absolute(OUT_DIR)

	# A mid-tone terrain-ish backdrop so the translucent card reads correctly.
	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg_layer.add_child(bg)

	# ---- prefs isolation — FIRST, before anything can read OR write a preference ----------------
	# THE HARNESS MUST NEVER TOUCH THE PLAYER'S PROFILE. It once did, and it cost a real debugging
	# session: a state called the persisting `toggle_victory()` while the legend was open for a
	# frame, `_save_hud_panel_prefs` wrote BOTH keys, and `legend_suppressed=false` landed in the
	# developer's `user://narrative.cfg` — so their next real game came up with the Terrain Types
	# panel visible and the shipped default looked broken. Redirect every read/write to a scratch
	# file and DELETE it, which is both the isolation and a genuine fresh profile.
	NarrativeForkPanel.config_path_override = PREVIEW_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_PREFS_PATH))
	# THE SECOND prefs file, for the same two reasons `band_panel_preview` isolates it. `tile_panel_band`
	# injects a real `BandCityPanel`, which READS its dock/collapse/TAB prefs on construction and WRITES
	# them back when the harness docks it — so without this the harness was editing the developer's
	# `user://band_city_dock.cfg` (found holding this harness's `edge=2` / `tab="band"`), AND the frame it
	# renders depended on whatever tab the previous run left behind: the panel's zone was measured
	# rendering `work` in one run and `band` in the next, off nothing but that leftover file.
	BandCityPanel.config_path_override = PREVIEW_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_DOCK_PREFS_PATH))
	# THE THIRD prefs file, same two reasons: the event dock persists its edge, row count, detail
	# floor and channels, and the `event_dock_*` states walk all four.
	EventDockPanel.config_path_override = PREVIEW_EVENT_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_EVENT_PREFS_PATH))
	# The Telling panel restores its collapsed state in its constructor, so pin it expanded BEFORE
	# the HUD instantiates (into the scratch file, now that the override is set).
	TellingPanel.save_collapsed(false)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	await get_tree().process_frame
	await get_tree().process_frame
	# Hold the canvas until the WM stops fighting it — before the first state, so no LATER settle has
	# to spend a frame on it. See `_stabilize_canvas`.
	await _stabilize_canvas()
	Input.warp_mouse(MOUSE_PARK_POSITION)

	# Seed the turn orb's calm breath at its MIDPOINT. `_pulse_time` only ever advances by `delta`,
	# which is 0 with the clock frozen, so whatever is set here is the phase every frame renders at —
	# and phase 0 is the breath's trough (alpha 0.30 / radius 44 of a 0.30..0.85 / 44..47 range), i.e.
	# a deterministic frame whose subject has faded to its faintest. Set once; nothing resets it.
	_hud.turn_orb._pulse_time = TurnOrb.PULSE_PERIOD * TURN_ORB_PULSE_MIDPOINT_FRACTION

	# The Tile-card Climate band is driven by the sim's PUBLISHED cut points, which the live
	# client adopts from the snapshot's overlays (MapSection.climateBands) via MapView. This
	# harness has no MapView, so seed TileClimate with the shipped values (polar ≤0 / boreal ≤3
	# / temperate ≤18 °C) exactly as a first snapshot would — otherwise every tile card would
	# skip the Climate row (has_bands() == false, the honest pre-publish blank).
	TileClimate.set_cut_points(0.0, 3.0, 18.0)

	# Top-bar Sedentarization meter (faction 0, soft band) — visible across all frames.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])

	# Top-bar demographics readout (faction 0 age structure + dependency ratio).
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# Top-bar intensification-knowledge meters (faction 0): Cultivation still learning
	# (block-glyph bar + "learning"), Herding fully mastered ("✔ known"). Visible across frames.
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# Top-bar Wondrous-Sites discoveries readout (faction 0). The strip keys on `site_id`, so this
	# fixture is built to prove the two cases the glyph could not distinguish:
	#   • GLYPH COLLISION — `great_peak` and `sky_arch` both ship ⛰, and must stay TWO entries:
	#     great_peak's bundled sprite, then sky_arch's ⛰ emoji (it has no art).
	#   • REPEAT INSTANCE — the second `great_peak` is a different tile, so it lifts the count to 4
	#     while adding no strip entry: the number counts instances, the strip counts kinds.
	# `verdant_basin` is the other bundled sprite. Reads `◈ Discoveries 4` + 3 marks.
	_hud.update_discoveries([{
		"faction": 0,
		"sites": [
			{"x": 12, "y": 8, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 20, "y": 14, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
			{"x": 26, "y": 9, "site_id": "sky_arch", "category": "landmark", "display_name": "Sky Arch", "glyph": "⛰"},
			{"x": 31, "y": 17, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
		],
	}])

	# The labor-allocation UI (Early-Game Labor slice 3b) targets the single player band;
	# seed it so the herd/tile "assign" controls resolve a band to staff.
	_hud._band_labor._player_band = _band_fixture()
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row reads the herd's
	# species from here and, when clicked, jumps to its LIVE tile (it has migrated away from the hunt
	# assignment's launch target).
	_set_world_herds(_world_herds_fixture())
	# The world's food modules (Main pushes snapshot["food_modules"]): each Forage row leads with the
	# module's map glyph, so the panel row and the map marker read as the same resource.
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])

	# State 0-fresh-profile — THE SHIPPED DEFAULT DOCK LAYOUT, rendered on the path a real player
	# travels and nothing else: prefs section erased above, HUD freshly instantiated, and the first
	# real terrain legend arriving from MapView exactly as `Main._on_overlay_legend_changed` pushes
	# it. NOTHING may call `set_suppressed` / `toggle_legend` / `toggle_victory` before this point —
	# that is the whole value of the state. The right dock must be EMPTY of both reference cards:
	# no Terrain Types, no Victory. This state is FIRST on purpose, so no later state can leak into
	# it, and it is the regression guard for "the legend is visible by default in the real game".
	_hud.update_overlay_legend(_terrain_legend_fixture())
	_hud.update_victory_state(_victory_state_fixture())
	await _settle()
	await _save("dock_fresh_profile_default")
	_assert_hud("fresh profile: Terrain Types legend is hidden",
		not _hud.terrain_legend_panel.visible)
	_assert_hud("fresh profile: Victory panel is hidden",
		not _hud.victory_panel.visible)

	# State 1 — a single band selected (GOOD state): the Occupants roster + the labor allocation panel.
	# Food + Morale are healthy, so BOTH summary rows read collapsed with a ▸ disclosure caret
	# (`Food ▸ …` / `Morale 82% ▸`) — click-to-expand, nothing auto-shown.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band")

	# State 1-foreign — a NON-player band selected. The drawer is the same `unit_summary_lines` host,
	# but almost none of it applies: morale/output/breakdowns are player-only (someone else's band is
	# not ours to read), there is no allocation panel, and the identity rows (name, size) now live in
	# the roster row above. So the check this state exists for: does the drawer collapse to an empty
	# card once `Unit`/`Size` are gone? (It keeps the bare larder Food line + Position.)
	_hud.show_unit_selection(_foreign_band_fixture())
	await _settle()
	await _save("band_foreign")

	# State 1-forage-policy — the forage allocation row carries a policy tag like Hunt does. This band
	# forages on Deplete policy, which the sim gathers past the patch's regrowth: the sim-answered
	# `overdraws` flag is true, so the row reads `Forage (71, 18) [deplete] +0.62 /turn ⚠` (amber
	# over-forage flag). The default `band` state above shows the [sustain] tag with overdraws=false.
	var forage_policy_band := _band_fixture()
	forage_policy_band["labor_assignments"] = [
		{"kind": "forage", "workers": 6, "target_x": 71, "target_y": 18, "floor": 0.15, "actual_yield": 0.62, "sustainable_yield": 0.40, "overdraws": true},
		{"kind": "scout", "workers": 2},
	]
	_hud.show_unit_selection(forage_policy_band)
	await _settle()
	await _save("forage_policy")

	# State 1-food-a — GOOD food, breakdown OPEN. The breakdown renders in a POPOVER, never inline
	# (growing the row in place is what clipped the Band panel's fixed-height band zone), so the frame
	# shows the indented `Gathered · Hunted · Eaten` rows in a small card under the row. Driven through
	# the REAL path — `meta_clicked` on the live drawer label, the exact signal a click emits.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await _settle()
	await _save("band_food_expanded")
	_click_disclosure(BAND_DISCLOSURE_FOOD)

	# State 1-morale-a — GOOD morale, breakdown OPEN (same disclosure, same popover): the morale
	# contribution rows.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_MORALE)
	await _settle()
	await _save("band_morale_expanded")
	_click_disclosure(BAND_DISCLOSURE_MORALE)

	# State 1-growth-a — GOOD growth, breakdown OPEN. The band out-breeds its base rate (188% of
	# normal), so the row reads neutral ink and its disclosure names what is HELPING: `▲ ×1.50 larder
	# reserve` / `▲ ×1.25 larder growing`. `hunger` is neutral (the band ate) so its row is omitted
	# rather than listed as a no-op — and the multipliers read down to the headline: 1.50 × 1.25.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH)
	await _settle()
	await _save("band_growth_expanded")
	_click_disclosure(BAND_DISCLOSURE_GROWTH)

	# State 1-growth-b — COLLAPSED growth on the concerning band (23% of normal → red row, WARN
	# caret), breakdown OPEN. All three factors are off neutral, so this is the frame that proves the
	# rows multiply out to the headline: 0.60 × 1.50 × 0.25 = 0.23. It is the whole point of the
	# export — the player already had the larder and the Food line, not the attribution.
	_hud.show_unit_selection(_collapsed_growth_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)
	await _settle()
	await _save("band_growth_collapsed")
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)

	# State 1-growth-c — a REHYDRATED band: the sim publishes no fertility reading (the factors are
	# derived, not persisted), so there is NO Growth row and no caret at all. The regression this
	# guards is the tempting one — defaulting the factors to 0 and rendering "Growth: 0% of normal",
	# i.e. reading missing data as a total collapse of births.
	_hud.show_unit_selection(_unprojected_growth_band_fixture())
	await _settle()
	await _save("band_growth_unprojected")

	# State 1-food-b — CONCERNING food (net negative + low runway): the Food line net reads red and
	# its caret wears WARN rather than SIGNAL — the breakdown no longer opens itself (a popover that
	# popped on a snapshot would be worse than the clipping it replaced), so the invitation to read it
	# has to be visible on the row.
	_hud.show_unit_selection(_concerning_food_band_fixture())
	await _settle()
	await _save("band_food_concerning")

	# State 1-food-c — a band KEEPING A PEN (docs/plan_corral_managed_population.md). Its ledger has
	# THREE terms, not two: the corral grosses 5.40, the people eat 1.15, and the penned animals eat
	# 1.74 off the same larder (`pen_feed_upkeep`, the sim's own figure — the client never sums the
	# herds' upkeep itself). Net = 5.88 − 1.15 − 1.74 = +2.99, NOT the +4.73 the old two-term ledger
	# would have advertised. Breakdown popover open to show all four rows at once.
	_hud.show_unit_selection(_pen_keeper_band_fixture())
	await _settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await _settle()
	await _save("band_pen_feed")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1-food-d — the same pen, STARVING: the band could pay only 0.70 of the 1.74 the herd
	# demands, so the pen feed row shrinks to what was actually paid while the herd wastes away (the
	# herd drawer carries the alarm — see `herd_corral_starving`). Income has fallen with the herd,
	# and the net has gone red.
	_hud.show_unit_selection(_starving_pen_band_fixture())
	await _settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await _settle()
	await _save("band_pen_starving")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1b — an all-idle band: no assignments, every worker idle. The allocation panel
	# shows just the Scout + Warrior rows (both at 0) under the Working/Idle header.
	var idle_band := _band_fixture()
	idle_band["activity"] = "idle"
	idle_band["idle_workers"] = 16
	idle_band["labor_assignments"] = []
	_hud.show_unit_selection(idle_band)
	await _settle()
	await _save("band_idle")

	# State 1p — optimistic pending feedback: a fresh forage assignment (6 workers to a new
	# tile) is in flight before the snapshot confirms. The panel shows an amber "· pending"
	# Forage row and the Idle count reflects it immediately (16 − [5+4+2+2+6=19] clamps to 0).
	# (Seeds the HUD-local pending map directly to mimic a just-issued assign_labor.)
	_hud._band_labor._pending_labor = {
		904: {
			"turn": 0,
			"assign": {"forage:64,20": {"kind": "forage", "workers": 6, "x": 64, "y": 20, "herd_id": "", "floor": 0.5}},
		}
	}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_pending")
	_hud._band_labor._pending_labor = {}

	# State 1e — a scouting expedition selected in its awaiting-orders phase: the drawer shows the
	# dedicated expedition readout (Mission / Phase "Awaiting orders" / Party / Provisions) and the
	# Recall + Move panel with the amber awaiting callout, instead of the labor-allocation UI.
	_hud.show_unit_selection(_expedition_fixture())
	await _settle()
	await _save("expedition_panel")

	# State 1f — the same expedition after Recall, now in its returning phase: the panel's button
	# reads "Returning" (disabled) instead of a grayed-out "Recall", and the awaiting callout is
	# gone. The drawer Phase row reads "Returning".
	var returning_expedition := _expedition_fixture()
	returning_expedition["expedition_phase"] = "returning"
	_hud.show_unit_selection(returning_expedition)
	await _settle()
	await _save("expedition_returning")

	# State 1g — outfit party cap: a resident band with 16 idle workers but a server party cap of 8.
	# The "Send scouting expedition" Party stepper maxes at min(idle 16, cap 8) = 8 — dialed to 8, the
	# + is disabled, confirming the stepper clamps to the CAP, not to idle.
	var cap_band := _band_fixture()
	cap_band["idle_workers"] = 16
	cap_band["max_expedition_party_size"] = 8
	cap_band["labor_assignments"] = []   # all 16 working-age workers read idle
	_hud._bandpanel._send_expedition_count = 8
	_hud.show_unit_selection(cap_band)
	await _settle()
	await _save("expedition_outfit_cap")
	_hud._bandpanel._send_expedition_count = 1   # reset so later states render a fresh party stepper

	# State 1h — a hunting expedition (PR 2, §2b) selected in its Hunting phase: the panel shows the
	# hunt readout (Mission "Hunting expedition", Target herd, Policy, Carried 8 / 16, Party) +
	# Recall/Move.
	_hud.show_unit_selection(_hunt_expedition_fixture())
	await _settle()
	await _save("expedition_hunt_panel")

	# State 1i — a FULL hunt party (carried at the carry ceiling): the Carried row reads "16 / 16 …
	# · FULL" and the Phase is Delivering (it heads home when full).
	var full_hunt := _hunt_expedition_fixture()
	full_hunt["expedition_phase"] = "delivering"
	full_hunt["stores"] = {"provisions": 16.0}
	full_hunt["turns_of_food"] = 8.0
	_hud.show_unit_selection(full_hunt)
	await _settle()
	await _save("expedition_hunt_full")

	# State 1j — a recalled hunt party in its Returning phase: the Phase reads "Returning" and the
	# panel's Recall button flips to a disabled "Returning" (same treatment as the scout panel).
	var returning_hunt := _hunt_expedition_fixture()
	returning_hunt["expedition_phase"] = "returning"
	returning_hunt["stores"] = {"provisions": 12.0}
	returning_hunt["turns_of_food"] = 6.0
	_hud.show_unit_selection(returning_hunt)
	await _settle()
	await _save("expedition_hunt_returning")

	# State 1j2 — a DEPLETE hunt party in flight: Deplete relaunches for repeated trips, so its
	# "Next delivery" line wears the recurring ↻ marker. That ↻ must read distinct from the Deplete
	# policy glyph (⇊) elsewhere in the panel — the whole point of the marker choice.
	var deplete_hunt := _hunt_expedition_fixture()
	deplete_hunt["expedition_hunt_policy"] = "deplete"
	deplete_hunt["expedition_eta_turns"] = 9
	deplete_hunt["expedition_projected_delivery"] = 16.0
	deplete_hunt["expedition_recurring"] = true
	_hud.show_unit_selection(deplete_hunt)
	await _settle()
	await _save("expedition_hunt_recurring")

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (DEPLETE selected) with its EXPEDITION hint. The expedition hints must never promise
	# HUSBANDRY — the Hunting arm accrues none, though since #337 it does bank the trade half of the
	# kill — so Deplete's line frames the rung by the PRESSURE it applies (relaunching trip after trip)
	# rather than by a craft the party cannot teach. The outfit block sits below the left dock's fold,
	# so scroll to see the hint.
	var launch_band := _band_fixture()
	launch_band["idle_workers"] = 12
	launch_band["labor_assignments"] = []
	var left_scroll: ScrollContainer = _hud.left_stack.get_parent() as ScrollContainer
	_hud._bandpanel._send_hunt_floor = DEEP_DRAW_FLOOR
	_hud.show_unit_selection(launch_band)
	await _settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await _settle()
	await _save("expedition_launch_policy")
	left_scroll.scroll_vertical = 0

	# State 1k-sustain — the SUSTAIN launch hint, which had to be rewritten when Sustain became the
	# maximum-sustainable-yield FLOW (it used to promise "one conservative harvest", a model that no
	# longer exists). It also must NOT mention domestication: only a RESIDENT band's Sustain hunt
	# builds husbandry — an expedition accrues none (the one payoff half still missing from a raid,
	# now that #337 banks its trade goods).
	_hud._bandpanel._send_hunt_floor = SourceForecast.FLOOR_FOOD_PEAK
	_hud.show_unit_selection(launch_band)
	await _settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await _settle()
	await _save("expedition_launch_policy_sustain")
	left_scroll.scroll_vertical = 0

	# State 1a — a well-fed but demoralized band: healthy food (∞) yet morale 0.22
	# (< critical), so the drawer's Morale line reads a red 22%. Discontent drags
	# Output to 56% (red) and the itemized morale breakdown + recovery guidance show.
	_hud.show_unit_selection(_low_morale_band_fixture())
	await _settle()
	await _save("band_low_morale")

	# State 1b — band alerts: seed previous sizes, then a snapshot that raises all
	# three alert kinds (starving red / losing-population amber / idle quiet).
	_hud.update_band_alerts(_band_alert_baseline())
	_hud.update_band_alerts(_band_alert_fixture())
	await _settle()
	await _save("band_alerts")

	# State 1c — Wondrous Sites: the top-bar `◈ Discoveries` readout. The `site_discovered` event is
	# pushed alongside it because a real snapshot carries both; the HUD's own consumer of that array
	# is the Telling now (the event dock is `Main`'s panel — see the `event_dock_*` block).
	_hud.ingest_command_events([
		{"tick": 42, "kind": "site_discovered", "label": "Discovered Verdant Basin", "detail": "A settle-site revealed at (20, 14)."},
	])
	_hud.clear_selection()
	await _settle()
	await _save("discoveries")

	# (State 1d — `predator_feed` — is RETIRED with the left-dock command feed it rendered. The
	# threat/casualty alert styling it judged moved into `HudEventVocab.KIND_STYLE` and is judged on
	# `event_dock_bottom` / `event_dock_pinned_alert` at the end of this run.)

	# State 1e — Predators Phase 3 band readout: the Warrior-card "⚠ Predator nearby — N on guard"
	# crimson alert AND the "⚔ Lost to raids −1.20" ledger row, both lit at once. A threatening predator
	# is placed within raid range in the world-herd list so the client-derived proximity check fires; the
	# food breakdown popover is opened to show the forfeit row. The shared herd list is restored after.
	_set_world_herds([_raiding_predator_herd_fixture()])
	var raided_band := _raided_band_fixture()
	_hud._band_labor._player_band = raided_band
	_hud.show_unit_selection(raided_band)
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await _settle()
	await _save("predator_band_raided")
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	_set_world_herds(_world_herds_fixture())   # restore the shared world-herd list

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# State 2 — a food tile selected, band WITHIN forage range: the Tile card's "Assign foragers"
	# controls (a "Band:" dropdown naming the actor band + a Foragers −/+ count + an enabled **Forage**
	# button). With one player band the dropdown is a single item ("Band 1").
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_tile")
	_assert_compose_sheet_fits("food_tile")

	# State 2-crop — the SAME tile once a band has committed it under Cultivate/Sow, WITH THE BUILD
	# STILL RUNNING (flora roster S1 + issue #433). A `Crop: Wild Grain` row appears ABOVE the basket
	# and the basket is UNCHANGED (45 / 30 / 25, identical to `food_tile.png`), because the species is
	# recorded on the first worked turn — ~25 turns before any weeding happens. Wild Grain's 🌿 row is
	# marked in SIGNAL, which is what joins the two rows by eye. THREE FRAMES ARE THE TEST, in order:
	# `food_tile` (wild) -> here (committed, nothing grown yet) -> `food_tile_crop_tended` (weeded).
	# A "committed" frame alone would pass while the client still collapsed the basket on commit.
	_show_tile(_committed_crop_tile_fixture())
	_compose_forage(_committed_crop_tile_fixture())
	await _settle()
	await _save("food_tile_crop")

	# State 2-crop-tended — the third frame: the SAME commitment once the Tended Patch lands and the
	# basket finally REWEIGHTS (Wild Grain 45% -> 68%, Oak Mast 25% -> 2%, Ground Nut untouched, the
	# increase coming off the least abundant member first). The Cultivation row reads "🌾 Tended Patch"
	# beside it, so the frame states the cause and the effect together.
	_show_tile(_weeded_crop_tile_fixture())
	_compose_forage(_weeded_crop_tile_fixture())
	await _settle()
	await _save("food_tile_crop_tended")

	# State 2-growing — the "What grows here" SECTION on the bare tile card (no compose sheet): a header
	# then one 🌿 row per realized plant, name + share%, in wire order (share DESC). The pair is TWO
	# "Alluvial Plain" tiles with DIFFERENT realized baskets (Wild Emmer 70% + Flax 30% vs Cotton 55% +
	# Flax 45%), so read side by side they are the visible proof that same-biome tiles no longer carry a
	# uniform per-biome roster — the per-tile realization the compose picker already shows, now on the
	# card a player gets by just inspecting a tile. Compose source reset so only the card renders.
	_hud._compose.reset_forage_source()
	_show_tile(_cash_basket_tile_fixture())
	await _settle()
	await _save("tile_growing_here")
	_show_tile(_cash_variant_basket_tile_fixture())
	await _settle()
	await _save("tile_growing_here_variant")

	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()

	# State 2-forecast — the same food tile with the Foragers stepper parked AT the forecast cap
	# (3 = the Sustain ceiling's max-useful workers, below the band's 10 idle): the `+` button is
	# DISABLED, the "max 3 workers useful here — more would be idle" note explains why, and the
	# "Expected yield" row reads the ceiling itself (+0.96 /turn = min(3 × 0.32, 0.96)).
	_hud._compose.set_forage_count(3)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_forecast_cap")

	# State 2-labor — the SAME food tile, but the actor band has only 2 idle workers, BELOW Sustain's
	# max-useful of 3: the Foragers stepper caps at 2 (LABOR, not usefulness) and the note names the
	# reason — "2 of 3 useful — free up idle workers to send more" — so a `+` gone dead at idle reads as
	# fixable by reassigning labor, not as a silent bug. The usefulness ceiling (3) is unchanged; only
	# the note differs from the usefulness-bound `forage_forecast_cap` above.
	var forage_labor_band: Dictionary = _forage_range_bands()[0].duplicate(true)
	forage_labor_band["idle_workers"] = 2
	_hud._band_labor._player_band = forage_labor_band
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(2)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_labor_bound")
	# Restore the 10-idle range band + count for the states that follow.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(3)

	# State 2-tended — a fully-cultivated forage patch: the Tile card's cultivation row reads
	# "🌾 Tended Patch" (SIGNAL tint) with an "Ecology: Thriving" row above it. A tended
	# patch's ceilings all equal its per-worker yield, so the forecast caps the stepper at 1 worker.
	_show_tile(_tended_tile_fixture())
	_compose_forage(_tended_tile_fixture())
	await _settle()
	await _save("tended_tile")

	# State 2-stressed — an over-drawn (uncultivated) forage patch: the Ecology row reads a WARN-amber
	# "⚠ Stressed" right under "Forage biomass", exactly like a stressed herd's Ecology row. Proves the
	# row is NOT gated on cultivation.
	_hud._compose.set_forage_count(1)
	_show_tile(_stressed_tile_fixture())
	await _settle()
	await _save("food_tile_stressed")

	# ---- Climate band: rendered off the sim's PUBLISHED cut points (Climate Authority) -----------
	# The Climate row is classified by the sim's cut points (polar ≤0 / boreal ≤3 / temperate ≤18 °C),
	# NOT a client threshold. Drive the same tile card at four temperatures spanning the ladder and
	# confirm the label tracks the sim's inclusive-upper-bound bands. A cold highland reads Polar/Boreal,
	# a warm lowland reads Temperate/Tropical — and "Polar" now appears ONLY where the sim says so, which
	# is the whole point of retiring the client's own cool_min.
	_show_tile(_climate_tile_fixture(-6.0, "Frost Highland"))
	await _settle()
	await _save("climate_polar")
	_show_tile(_climate_tile_fixture(2.0, "Boreal Upland"))
	await _settle()
	await _save("climate_boreal")
	_show_tile(_climate_tile_fixture(12.0, "Temperate Vale"))
	await _settle()
	await _save("climate_temperate")
	_show_tile(_climate_tile_fixture(27.0, "Tropical Lowland"))
	await _settle()
	await _save("climate_tropical")

	# ---- The tile card's TWO FOOD-WEB ROWS ------------------------------------------------------
	# `Foraging` (people) directly above `Grazing` (animals), each carrying its stock and its ecology
	# phase inline, with the human layer's basket indented beneath its row. The pair replaced four
	# interleaved rows under names that inverted each other (`Pasture` bare beside `Forage biomass`
	# qualified; `Pasture ecology` qualified beside `Ecology` bare), which a playtest reader mistook
	# one for the other three times.
	#
	# State food_layers — the reference frame: all THREE crop roles on one patch, so every role icon is
	# in one picture and the card states outright that 62% of what grows on this ground is not food.
	_hud._compose.set_forage_count(1)
	_show_tile(_three_role_tile_fixture())
	await _settle()
	await _save("tile_food_layers")

	# State food_layers_unstated — the SAME tile with the cash crop's role missing from the wire. `""`
	# means UNSTATED, not "staple", so that row must render NO icon while its two neighbours keep
	# theirs; a defaulted icon here would invent a fact about the plant.
	_show_tile(_unstated_role_tile_fixture())
	await _settle()
	await _save("tile_food_layers_unstated")

	# The three claims a PICTURE cannot carry, asserted over the REAL producer's lines (the harness
	# pokes `_drawer` directly, the `tile_panel_*` idiom). Each is sabotage-verified.
	_assert_food_layer_rows()

	# State 2-pasture-stressed — the graze drawn down into the stressed band: "Grazing 61 / 240 ·
	# ⚠ Stressed", the phase inline and WARN-amber, identical in label and tint to a stressed herd or
	# patch. (The healthy pair — `Foraging` above `Grazing`, both Thriving — is on `food_tile`.)
	_show_tile(_overgrazed_tile_fixture())
	await _settle()
	await _save("tile_pasture_stressed")

	# State 2-pasture-none — a GLACIER: the biome carries no pasture at all, so the sim holds no patch
	# and the card prints NOTHING about pasture. "0 / 0" would be a lie of a different kind — a starved
	# pasture rather than an absent one — and this frame is the guard against it.
	_show_tile(_no_pasture_tile_fixture())
	await _settle()
	await _save("tile_pasture_none")

	# State 2-pasture-legend — the map legend for the `pasture` overlay channel (rows produced by
	# MapView._build_pasture_legend; see map_preview's "pasture" state for the map itself). The barren
	# tones sit OFF the straw→grass ramp: dead ground and water are their own rows, so "no pasture at
	# all" can never be read as "poor pasture".
	# The legend card ships SUPPRESSED (the player opens it with `L`), so every legend state opens it
	# and CLOSES IT AGAIN around its own frames — see `_open_legend` / `_close_legend`.
	_open_legend()
	_hud.update_overlay_legend(_pasture_legend_fixture())
	await _settle()
	await _save("pasture_legend")
	_close_legend()
	_hud.clear_selection()

	# State 2-forage-legend — the map legend for the `forage` overlay channel (rows produced by
	# MapView._build_forage_legend; see map_preview's "forage" state for the map). The twin of the
	# pasture legend, but honest about the OPPOSITE meaning of absence: NO water row (shelves carry
	# forage and ride the ramp), a single "No forage" barren row (deep ocean/glacier/lava only), and a
	# "Gathering sites: N" sub-count so the ramp reads as POTENTIAL without calling the rest dead.
	_open_legend()
	_hud.update_overlay_legend(_forage_legend_fixture())
	await _settle()
	await _save("forage_legend")
	_close_legend()
	_hud.clear_selection()

	# ---- Hex-edge rivers on the Tile card (ui/RiverEdges.gd, the shared text formatter) -----------
	# State 2-river-both — the interesting case: a tile whose sides carry BOTH classes. The card must
	# read "Major River: NE, NW" then "Minor River: SW" — Major first (the bigger river reads first),
	# directions in compass order from NE clockwise, NOT the sim's bit order (which starts at E).
	_show_tile(_river_tile_fixture(RIVER_MASK_TWO_CLASS))
	await _settle()
	await _save("river_tile_both")

	# State 2-river-minor — a single-class tile: one "Minor River: E, SE" row, no Major row.
	_show_tile(_river_tile_fixture(RIVER_MASK_SINGLE_CLASS))
	await _settle()
	await _save("river_tile_minor")

	# State 2-river-none — mask 0: NO river row at all (not an empty "River:" label).
	_show_tile(_river_tile_fixture(RIVER_MASK_NONE))
	await _settle()
	await _save("river_tile_none")

	# ---- Cultivate: the forage INVESTMENT rung (gated, then unlocked) ----------------------------
	# State 2-cultivate-locked — **THE KNOWLEDGE-SUPPRESSION RULE'S OWN FRAME, on the plant web.** The
	# faction has NOT finished learning Cultivation (the top-bar meter reads "Cultivation ▰▰▰…
	# learning") and the patch is Thriving and wild, so KNOWLEDGE is the only thing blocking the rung —
	# and this sheet renders no improvement control at all for that.
	#
	# **THE FRAME'S SUBJECT MOVED WITH THE RULE, and it is a progression rather than a hole.** It used
	# to be the gated control's reason line ("🌱 Your people know Cultivation 55% — ♻ forage a wild
	# patch to learn it"). That sentence was both redundant and vacuous HERE: the aside two rows up
	# states the same lesson live and quantified, and its remedy — forage a wild patch — names the very
	# work this sheet is composing, so it told the player to do what they were in the middle of doing.
	# What the frame shows now is the pair that has to hold TOGETHER: nothing is offered that the sim
	# would refuse, and the aside is still naming the lesson being earned. A SOURCE gate is untouched
	# and still leads a control — `improvement_offered_gated` and `forage_sow_locked` are those frames.
	_hud._compose.set_forage_count(1)
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_cultivate_locked")
	# **ASKED OF THE WHOLE CONTROL FAMILY, not of the Cultivate rung.** `_find_improvement_control`
	# answers null for a rung merely spelled differently, so a per-rung form of this passes on a sheet
	# that renders some OTHER rung's control; `IMPROVEMENT_CONTROL_META` rides all four states the
	# widget can be in, so this says "no improvement control, of any rung, in any state".
	_assert_hud("a rung blocked ONLY on knowledge renders NO improvement control on this sheet",
		_find_meta_node(_hud._drawercompose._compose_sheet,
			HudWidgets.IMPROVEMENT_CONTROL_META) == null)
	# The visible symptom of getting this wrong, and why it is asserted separately: dropping the reason
	# WITHOUT suppressing the control leaves an unchecked, live box over a live crop list — the sheet
	# inviting a commitment the sim rejects, which is strictly worse than the line that was cut.
	_assert_hud("…and no crop list beneath it, the sheet offering nothing it cannot commit",
		_find_crop_row(_hud._drawercompose._compose_sheet, GATED_CROP_NEEDLE) == null)
	# **THIS IS WHAT MAKES THE REMOVAL A PROGRESSION.** The rung is not merely hidden: the aside names
	# the very craft whose absence suppressed the control, live, in the same frame. Read BY META — the
	# aside's siblings move with the floor too, so a whole-aside search says nothing about this line.
	_assert_hud("…while the aside still names the lesson being earned, so the rung is not silent",
		_teaching_line(_hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_FORAGE][
				SourceForecast.IMPROVEMENT_NONE])))

	# Learning Cultivation crosses 0.55 → 1.0 between snapshots: the one-shot command-feed nudge fires
	# ("Cultivation learned — The Cultivate policy is now available on Thriving patches."), visible in
	# the left-dock Command Feed card in every frame from here on.
	_hud.update_intensification([{"faction": 0, "cultivation": 1.0, "herding": 1.0}])

	# State 2-cultivate — knowledge known + a Thriving patch: 🌱 Cultivate is ENABLED and selected. The
	# forecast states the DEAL instead of a single number — "Preparing: +0.24 /turn → then +1.20 /turn"
	# (ceiling_cultivate → tended_yield) — and the stepper caps at 1 worker (a managed source needs one).
	_show_tile(_food_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_cultivate")
	# THE FORAGE HALF of the compose-order invariant (see `_compose_spine`): capture this sheet's control
	# spine, to be compared against the local-hunt sheet's when that renders further down.
	#
	# **CAPTURED HERE AND NOT ON `food_tile`, WHERE IT USED TO BE — the spine must be taken where the
	# sheet carries every control it can carry.** `food_tile` renders at Cultivation 55%, i.e. a rung
	# blocked on KNOWLEDGE ALONE, and this sheet now builds no improvement control for that; comparing
	# that three-control spine against the local hunt's four would fail an ORDER assertion for a reason
	# that has nothing to do with order. This state is the same sheet one snapshot later, with the
	# knowledge complete and the rung composed — so both spines are full, and the equality is a real
	# claim about sequence again.
	_record_compose_spine(COMPOSE_SPINE_KEY_FORAGE)

	# State 2-crop-picker — THE CROP PICKER (flora roster S1), on the longest basket the sim produces
	# (5 named plants). Under 🌱 Cultivate the selection must land on the HIGHEST-SHARE LEGAL row —
	# Wild Emmer 34%, which is also the sim's own default — while River Fish and Oak Mast stay VISIBLE
	# and greyed (they climb no rung), and Ground Nut 14% stays fully pressable: a small share is a bad
	# choice, not an illegal one. Judge legibility + fit here, not on the 3-entry reference tile.
	_show_tile(_long_basket_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker")

	# THE COMMIT BUTTON MUST STAY ON SCREEN. A picker that pushes `Forage` below the sheet's fold is
	# worse than the problem the picker solved, so the picker's list scrolls WITHIN itself
	# (FLORA_CROP_LIST_MAX_HEIGHT) rather than growing the sheet. Asserted on the LONGEST basket, the
	# only case that can trip it: the sheet's own ScrollContainer must have nothing left to scroll.
	var sheet_scroll: ScrollContainer = _hud._drawercompose._compose_sheet._scroll
	var sheet_overflow: float = sheet_scroll.get_v_scroll_bar().max_value - sheet_scroll.size.y
	print("ui_preview: compose sheet overflow = %.1f (card %.1f)" % [
		sheet_overflow, _hud._drawercompose._compose_sheet._card.size.y])
	_assert_hud("a 5-plant crop picker leaves the Forage button on screen (sheet does not scroll)",
		sheet_overflow <= 1.0)
	# The height that bought those rows used to come from COLLAPSING the other rung's gate reasons; the
	# improvement control bought it outright (issue #442) by offering ONE rung instead of six, so no
	# other rung's prerequisites are on the card to collapse in the first place. That is the claim now.
	#
	# ASKED OF THE CONTROLS, not of the sheet's text. This searched the whole sheet for the words "Seed
	# Selection" — which passes if the sheet failed to open at all, and would go on passing if a second
	# rung rendered wearing any other reason. The claim is about how many improvement CONTROLS there
	# are, so it counts them: the composed Cultivate, and nothing for the rung above it.
	_assert_hud("only ONE improvement is offered, so no second rung's prerequisites crowd the card",
		_find_improvement_control(_hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CULTIVATE) != null
		and _find_improvement_control(_hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_SOW) == null)

	# State 2-crop-then-a / -b — THE PICKER ACTUALLY MOVES THE PAYOFF. The "· then" term used to quote
	# a species-BLIND patch number, so committing to Ground Nut showed Wild Emmer's payoff and the picker
	# appeared to change nothing above it. These two frames are the SAME tile with a DIFFERENT crop
	# selected; the assertion is that the payoff differs between them, which is the only thing
	# that proves the substitution is wired to the selection rather than rendered once.
	#
	# **READ OFF THE RUNNING CONTROL'S OWN FACE, by meta.** The payoff used to ride a separate deal line
	# beneath the box and now rides the face itself, in the offered box's `· then` grammar — so the one
	# Callable feeding both states is asserted where the player actually reads it.
	_hud._compose.set_forage_count(1)
	_hud._compose.set_forage_species("wild_emmer")
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_then_emmer")
	var then_emmer := _improvement_face(
		_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)

	_hud._compose.set_forage_species("ground_nut")
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_then_groundnut")
	var then_groundnut := _improvement_face(
		_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	print("ui_preview: then-term  emmer=%s  ground_nut=%s" % [then_emmer, then_groundnut])
	_assert_hud("the running rung's 'then' payoff tracks the SELECTED crop",
		then_emmer.contains(IMPROVEMENT_PAYOFF_NEEDLE)
			and then_groundnut.contains(IMPROVEMENT_PAYOFF_NEEDLE)
			and then_emmer != then_groundnut)
	_hud._compose.set_forage_species("")

	# State 2-crop-marginal — the ALL-MARGINAL tile (RollingHills' real ratios). Every legal crop is
	# below 1.0×, so the whole list is warn-inked and the hint says why — and every row stays PRESSABLE.
	# The ratio is here to stop a bad idea being invisible, never to forbid it.
	_show_tile(_marginal_basket_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_marginal_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_marginal")

	# State 2-crop-overlong — THE SCROLL'S OWN FRAME. A SYNTHETIC 8-plant basket, longer than any tile
	# the sim can produce, so the picker's internal list actually scrolls: the visible-row cap is set so
	# every SHIPPED basket fits whole, which would otherwise leave this path rendered by nothing. The
	# `Forage` button must still be on screen — that is what the cap protects, at any basket length.
	_show_tile(_overlong_basket_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_overlong_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_overlong")
	var overlong_scroll: ScrollContainer = _hud._drawercompose._compose_sheet._scroll
	print("ui_preview: overlong-basket sheet overflow = %.1f (card %.1f)" % [
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y,
		_hud._drawercompose._compose_sheet._card.size.y])
	_assert_hud("an 8-plant crop picker still leaves the Forage button on screen",
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y <= 1.0)

	# ---- THE TWO ZERO-WORKER SUBMITS (playtest defect) -------------------------------------------
	# `workers == 0` means two different things depending on whether this band already works the tile,
	# and the button + the forecast line have to agree in BOTH. These frames are judged as a PAIR.
	#
	# State 2-unstaffed (A) — 0 foragers on a tile this band does NOT work. Pressing Forage would send a
	# command that changes nothing, so the button is DISABLED and still reads `Forage`. The payoff
	# NUMBER stays on the running box's face — it is how the player decides the tile is worth staffing
	# at all — and there is no longer a SEQUENCE beside it to be wrong about at zero crew: the deal
	# line's today/dip terms are what a zero crew made unreachable, and only the payoff survived it.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_show_tile(_food_tile_fixture())
	# The FIRST compose settles the source key; the policy and count must be set after it, because a
	# source change re-seeds both from the band's standing assignment and would overwrite them.
	_compose_forage(_food_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_count(0)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_unstaffed")
	# By META — the commit verb follows the patch's rung now, and a bare "Forage" literal here would
	# be a second, silent spelling of that rule.
	var unstaffed_btn := _compose_commit_button(_hud._drawercompose._compose_sheet)
	_assert_hud("0 workers on an unassigned tile disables the submit (it would be a no-op)",
		unstaffed_btn != null and unstaffed_btn.disabled)
	# **THE DELETED DEAL LINE, ASSERTED AS A PAIR.** Absence alone is vacuous — deleting the payoff too
	# would satisfy it — so the same frame asserts the payoff is ON the running control's face, in the
	# offered box's own `· then` grammar.
	_assert_hud("the improvement deal LINE is gone from the sheet",
		not _has_label_containing(_hud._drawercompose._compose_sheet, IMPROVEMENT_DEAL_MIDDLE_NEEDLE))
	_assert_hud("…while what the tile would pay once prepared rides the running box's own face",
		_improvement_face(_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
			.contains(IMPROVEMENT_PAYOFF_NEEDLE))
	# **A CREW OF ZERO IS BUILDING NOTHING, AND THE ASIDE MAY NOT SAY OTHERWISE.** `learn_multiplier`
	# is a function of the FLOOR alone, so at the food peak it reads ×1.00 no matter who is assigned —
	# and this frame has a composed Cultivate with NOBODY on it. The build half is gated on the same
	# work predicate the lesson is, which is a fact about the sim rather than a display nicety: build
	# accrual and knowledge accrual share one multiplier and one `crew_is_working_the_source` gate.
	# Asserted on this frame because it is the only one that pairs a live build with an empty crew.
	_assert_hud("an unstaffed build claims no build rate — nobody is building it",
		not _teaching_line(_hud._drawercompose._compose_sheet).to_lower().contains("building at"))

	# State 2-unassign (B) — the SAME 0 workers on a tile this band DOES work: that is the sim's
	# unassign, not a no-op. The button stays live and is RENAMED, and the "assign to begin" line is
	# gone — it would contradict the button. What abandoning costs is already on the card in the
	# Cultivate policy hint ("It must stay staffed or it goes feral").
	_hud._band_labor._player_band = _cultivating_forage_band_fixture()
	_hud._compose.reset_forage_source()
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_count(0)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_unassign")
	var unassign_btn := _find_button_by_text(_hud._drawercompose._compose_sheet, "Unassign")
	_assert_hud("0 workers on a tile this band works stays live, renamed Unassign",
		unassign_btn != null and not unassign_btn.disabled)
	# …and the improvement control is SUPPRESSED here, which is the other half of the same judgement:
	# offering to START a build in the act of abandoning the source says two opposite things at once.
	# Asked of the whole control family, so a rung merely spelled differently cannot satisfy it.
	_assert_hud("…and offers no rung to start while it is handing the source back",
		_find_meta_node(_hud._drawercompose._compose_sheet,
			HudWidgets.IMPROVEMENT_CONTROL_META) == null)

	# Restore the unassigned near band for the frames that follow.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_count(1)

	# State 2-crop-committed — the patch has already committed. The commitment is one-way until it
	# lapses, so the picker becomes a LOCKED READOUT — but a readout OF THE WHOLE BASKET, with the
	# committed row marked, not a lone crop name: a bare name beside a tile card listing three plants
	# had the two panels of one tile disagreeing about what grows there, and read as "this tile is Wild
	# Grain now" (issue #433 deleted exactly that belief). The double-open is the same one the two states above spell out — the
	# `reset_forage_source()` just above makes the first open a SOURCE CHANGE, which re-seeds the policy
	# from the band's standing assignment (Sustain) and threw the `cultivate` away. This state had been
	# silently rendering the Sustain sheet, which carries no crop block at all, so the readout it exists
	# to judge was not in the frame.
	#
	# IT IS ALSO THE SIZING CASE FOR BOTH SURFACES, which is why it runs on the 4-species basket and on
	# a band that WORKS the tile: `realized_species_max` is 4, and the 3-species tile is one row short
	# of the caps the committed block used to blow — the compose sheet's fixed 560px ceiling and the
	# drawer's cap against the dock. A committed patch's block went from ONE line to FOUR rows, so both
	# surfaces gained ~66px at once, and neither fixture in the shipped set could reach them.
	_hud._band_labor._player_band = _cultivating_forage_band_fixture()
	# **THE STOCKPILE PUSH THAT USED TO SIT HERE IS GONE, AND WITH IT A LAYOUT TERM** (issue #381).
	# The left-dock Stockpiles card sat below the tile card and was hidden until a faction carried
	# stock, so seeding stock here was what put a reserved sibling into `DockScrollFit`'s measurement.
	# That card is retired and the band dock's Trade row that replaced it is band-scoped (a rate, no
	# faction stock), so nothing in the HUD reads `faction_inventory` and `HudLayer.update_stockpiles`
	# no longer exists. The drawer's cap is now measured against a left dock holding the tile card and
	# the default-hidden command feed — which IS the layout the player has, and a slightly roomier one
	# than this state was originally tuned against.
	# WHAT THE COMMITTED BLOCK COSTS THE DRAWER, measured rather than reasoned about: the SAME tile
	# with the commitment stripped, so the printed pair is the before/after of one change on one
	# layout. Both surfaces grew at once, and a sizing claim about either is worth only its number.
	var uncommitted_twin := _four_species_committed_tile_fixture()
	uncommitted_twin.erase("patch_committed_species")
	uncommitted_twin.erase("patch_committed_display_name")
	_show_tile(uncommitted_twin)
	await _settle()
	print("ui_preview: uncommitted drawer body=%.1f" % _hud.subject_body.get_combined_minimum_size().y)
	# …and what it cost against the OLD render, which showed the `Crop:` row INSTEAD of the basket. A
	# basket-less committed patch cannot occur (the sim only commits to a member of the basket), so
	# this is a measurement fixture and never a saved frame — it exists to put a number on the growth.
	var old_render_twin := _four_species_committed_tile_fixture()
	old_render_twin.erase("patch_composition")
	_show_tile(old_render_twin)
	await _settle()
	print("ui_preview: pre-change (crop row, no basket) drawer body=%.1f"
		% _hud.subject_body.get_combined_minimum_size().y)
	_show_tile(_four_species_committed_tile_fixture())
	_compose_forage(_four_species_committed_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_compose_forage(_four_species_committed_tile_fixture())
	await _settle()
	await _save("forage_crop_committed")
	# `alloc_section_label` upper-cases its text, so the header is matched in the case it RENDERS in.
	_assert_hud("a committed patch's picker is a locked readout under the committed-crop header",
		_has_label_containing(_hud._drawercompose._compose_sheet,
				HudFloraVocab.FLORA_CROP_COMMITTED_HEADER.to_upper())
			and _has_label_containing(_hud._drawercompose._compose_sheet,
				HudFloraVocab.FLORA_CROP_COMMITTED_HINT))
	# THE BUG THIS STATE NOW GUARDS: the readout lists the WHOLE basket, in the tile card's own order.
	# Asserting the committed name alone passed while the other two plants were being suppressed.
	var committed_rows: Array[Button] = []
	for basket_crop in ["Wild Emmer", "Flax Fields", "Hay Grass", "Wild Grapevine"]:
		committed_rows.append(_find_crop_row(_hud._drawercompose._compose_sheet, basket_crop))
	_assert_hud("…listing every plant in the basket, not just the committed one",
		not committed_rows.has(null))
	var committed_all_locked := true
	for row in committed_rows:
		committed_all_locked = committed_all_locked and row != null and row.disabled
	_assert_hud("…with every row locked (the commitment is one-way until it lapses)", committed_all_locked)
	# `_rung_is_selected` reads the `normal` stylebox's fill, which `apply_button` writes from the
	# VARIANT — the one mark of selection that survives the disabled treatment, which is the whole
	# reason `selected_when_disabled` is passed here. Written for policy rungs, true of any
	# `apply_button`-styled button, and the only reading that can tell marked-and-locked from locked.
	_assert_hud("…and the committed crop marked as the standing choice",
		committed_rows[0] != null and _rung_is_selected(committed_rows[0]))
	_assert_hud("…while the rest of the basket is not",
		committed_rows[1] != null and not _rung_is_selected(committed_rows[1]))
	# ---- BOTH SURFACES MUST FIT THE 4-ROW BLOCK -------------------------------------------------
	# Printed as well as asserted: when one of these fails, the numbers say WHICH ceiling bit (the
	# sheet's own cap, the viewport, or the dock's remaining room), which a bare false cannot.
	var committed_sheet: ComposeSheet = _hud._drawercompose._compose_sheet
	var committed_sheet_scroll: ScrollContainer = committed_sheet._scroll
	var committed_sheet_overflow: float = committed_sheet_scroll.get_v_scroll_bar().max_value \
		- committed_sheet_scroll.size.y
	print("ui_preview: committed sheet card=%.1f body=%.1f overflow=%.1f viewport=%.1f" % [
		committed_sheet._card.size.y, committed_sheet._body.get_combined_minimum_size().y,
		committed_sheet_overflow, get_viewport().get_visible_rect().size.y])
	_assert_hud("a 4-species committed block does not make the compose sheet scroll internally",
		committed_sheet_overflow <= 1.0)
	# Clipping is the SYMPTOM the player reported (the `Now 1` line off the top, the Forage button
	# sliced), and a scroll-extent check alone would not see a control sitting outside the card, so
	# the two ends of the sheet are measured against the card's own rect.
	var committed_now_line := _label_node_containing(committed_sheet, HudComposeVocab.COMPOSE_NOW_STAFFED_FORMAT % [1, ""])
	_assert_hud("…and the staffing line the sheet opens with is inside the card",
		_rect_contains(committed_sheet._card.get_global_rect(), committed_now_line))
	_assert_hud("…and so is the Forage button it ends with",
		_rect_contains(committed_sheet._card.get_global_rect(),
			_compose_commit_button(committed_sheet)))
	# The TILE CARD's drawer is the other surface the same 4 rows pushed past its cap. Internal
	# scrolling is BY DESIGN here (a crowded hex must scroll inside the drawer rather than drag the
	# dock), so the assertion is not "never scrolls" — it is that THIS content, which fits the room
	# the dock has left, is not being capped short of it.
	var drawer_scroll: ScrollContainer = _hud.subject_scroll
	var drawer_overflow: float = drawer_scroll.get_v_scroll_bar().max_value - drawer_scroll.size.y
	# The same three terms `DockScrollFit.fit_height` caps against, printed so a failure says WHICH
	# ran out — the dock's height, the rows above the drawer, or the cards reserved below it.
	var drawer_top_in_dock: float = drawer_scroll.global_position.y - _hud.left_dock_scroll.global_position.y
	var drawer_reserved_below := _dock_height_reserved_below(_hud.tile_panel)
	print("ui_preview: committed drawer cap=%.1f body=%.1f overflow=%.1f dock=%.1f top=%.1f reserved=%.1f available=%.1f" % [
		drawer_scroll.custom_minimum_size.y, _hud.subject_body.get_combined_minimum_size().y,
		drawer_overflow, _hud.left_dock_scroll.size.y, drawer_top_in_dock, drawer_reserved_below,
		_hud.left_dock_scroll.size.y - drawer_top_in_dock - drawer_reserved_below])
	_assert_hud("…nor does it make the tile card's drawer scroll, with dock room to spare",
		drawer_overflow <= 1.0)
	# Restore the unstaffed near band + the 3-species tile for the frames that follow.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_count(1)

	_hud._compose.set_forage_improvement("cultivate")
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()

	# State 2-cultivate-stressed — knowledge known, but the patch is ⚠ Stressed: Cultivate stays visible
	# and greyed with the OTHER reason — "Patch is Stressed — ease workers off and let it regrow to
	# Thriving" (the ecology gate, not the knowledge one). The remedy is deliberately NOT "Sustain it":
	# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever.
	_show_tile(_stressed_tile_fixture())
	_compose_forage(_stressed_tile_fixture())
	await _settle()
	await _save("forage_cultivate_stressed")

	# ---- Sow + the Field: plant RUNG 3 (slice 6b) -------------------------------------------------
	# State 6b-sow-locked — Seed Selection is only 12% learned AND this ordinary prairie refuses seed,
	# so BOTH kinds of reason are live at once: one fixed by PRACTICE (work a Tended Patch), one only
	# by MOVING somewhere else. No other rung on either ladder has the latter.
	#
	# **THAT PAIR IS WHY THIS FRAME PINS THE SUPPRESSION RULE, not merely the survival of it** — and it
	# is the frame that was asserting the DEFECT. The sheet used to delete the knowledge reason
	# unconditionally and render the source one alone, on the premise that the aside states that lesson
	# live two rows up. Reported from play: a lone reason reads as THE reason, so a tended patch at
	# Seed Selection 77% on dry ground claimed the knowledge was in hand and the water was all that
	# stood in the way — the message for a player who HAS Seed Selection. The premise is conditional
	# too: the aside names the lesson only while the crew is actually working the source, and on that
	# frame it read "Teaching nothing".
	#
	# The knowledge reason is now dropped ONLY when it is the sole one. Here BOTH render: the knowledge
	# reason leads (the near-term one a player can move), the ground's refusal keeps the note slot
	# beneath. They are different decisions — *you do not know how yet* means wait, *this ground will
	# never take seed* means move on.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0,
		"seed_selection": SOW_LOCKED_SEED_SELECTION, "penning": 0.0,
	}])
	# **THE TILE HAS TO BE A TENDED ONE NOW** (issue #442). Only ONE improvement is ever offered — the
	# source's next rung — so on a WILD patch with Cultivation known, Cultivate is what the control
	# offers and Sow is not reached at all. A tended patch has its rung-2 built, which makes Sow the
	# next rung and puts this frame's subject back on screen. That is the change working, not a loss:
	# the old picker showed all six rungs at once and had to grey four of them to say so.
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.set_forage_improvement("")
	_hud._compose.reset_forage_source()
	_show_tile(_tended_tile_fixture())
	_compose_forage(_tended_tile_fixture())
	await _settle()
	await _save("forage_sow_locked")
	# A rung blocked on the SOURCE still TEACHES IN FULL — the reason is the control's own text, which
	# is the whole point of showing a gated improvement rather than hiding it.
	var sow_box := _find_improvement_control(_hud._drawercompose._compose_sheet, "sow")
	_assert_hud("a SOURCE-gated improvement is SHOWN, never hidden — the rung stays discoverable",
		sow_box != null and not (sow_box is CheckBox))
	var sow_knowledge_reason := HudFloraVocab.GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT % [
		HudFormat.progress_percent(SOW_LOCKED_SEED_SELECTION),
		FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)]
	_assert_hud("…with the KNOWLEDGE prerequisite leading as the control's OWN text — the one a player can move",
		_improvement_face(_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_SOW)
			== HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_SOW), sow_knowledge_reason])
	# **AND THE SOURCE GATE SURVIVES BENEATH IT — asserted as the PAIR, because either alone is the
	# bug.** Only the lead line would mean the ground's permanent refusal had been swallowed; only the
	# presence of the knowledge reason somewhere would be satisfied by the old lead-with-the-source
	# rendering. Asked of the whole sheet, since a reason "renders" wherever it lands.
	_assert_hud("…and the ground's own refusal still renders beneath it, not swallowed by the lead",
		_has_label_containing(_hud._drawercompose._compose_sheet,
			String(HudFloraVocab.SOW_REFUSAL_REASONS[SOW_LOCKED_REFUSAL_KEY])))
	# …and the rung BELOW it reads as the state it left behind, not as a second greyed option.
	_assert_hud("…above a DONE label for the rung already built",
		_find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate") is Label)

	# Seed Selection completes → the one-shot feed nudge fires ("Seed Selection learned — The Sow
	# policy is now available — but only on rich, well-watered ground.").
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])

	# State 6b-sow-too-dry — knowledge KNOWN, and still refused: this prairie is rich but dry. THE
	# WHOLE POINT of the sim shipping a reason rather than a bool — only ~46 of 4160 tiles (1.1%) will
	# take seed, so "why can't I sow here?" is *the* question rung 3 provokes, and the client cannot
	# re-derive the answer (it has neither the biome capacity table nor the hydrology). The line must
	# name the fault (dry), not just refuse, and point at the rung that lifts it.
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_sow_too_dry")

	# State 6b-sow-too-poor — the OTHER refusal, and the reason this pair is rendered together: thin
	# upland ground that IS watered. A different fault must produce a different sentence and a
	# different remedy — if these two frames read the same, the reason field is being wasted.
	_show_tile(_sow_too_poor_tile_fixture())
	_compose_forage(_sow_too_poor_tile_fixture())
	await _settle()
	await _save("forage_sow_too_poor")

	# State 6b-sow — QUALIFYING ground at last (alluvial plain beside fresh water — one of the 46).
	# ▦ Sow is ENABLED and selected, with NO refusal line. The forecast states a deal that is
	# deliberately shaped unlike Cultivate's: "Preparing: +0.02 /turn → then +2.40 /turn" — near-zero
	# while the crop is in the ground (pure investment; there is no standing stand to take a fraction
	# of), then 2× a tended patch. That asymmetry IS rung 3's bargain.
	_show_tile(_sowable_tile_fixture())
	_hud._compose.set_forage_improvement("sow")
	_compose_forage(_sowable_tile_fixture())
	await _settle()
	await _save("forage_sow")

	# State 6b-crop-picker-sow — THE SAME long basket as `forage_crop_picker`, one rung up, on ground
	# that will take seed. `can_sow` is a DIFFERENT flag from `can_cultivate`, so only Wild Emmer stays
	# legal here and Hazel/Ground Nut join the greyed rows: the two frames side by side are what prove
	# the gate reads the composed rung's own flag rather than one "can be farmed" bit.
	_show_tile(_sowable_long_basket_tile_fixture())
	_hud._compose.set_forage_improvement("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_sowable_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_sow")

	# State F3 fodder crop — a basket with a HAY crop under Sow. Hay Grass pays fodder, not provisions,
	# so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the picker
	# instead shows "Hay Grass 30% · 1.8 hay". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio — proof a normal crop's row is untouched.
	_show_tile(_fodder_basket_tile_fixture())
	_hud._compose.set_forage_improvement("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_fodder_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_fodder")

	# State F4 cash crop — a basket with a CASH crop under Sow. Flax pays trade, not provisions or
	# fodder, so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the
	# picker instead shows "Flax 30% · 2.4 trade". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio — proof a normal crop's row is untouched (twin of the fodder frame).
	_show_tile(_cash_basket_tile_fixture())
	_compose_forage(_cash_basket_tile_fixture())   # settle the source key first (it changed)
	_hud._compose.set_forage_improvement("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_cash_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_cash")

	# State F4 per-tile flora realization — the SECOND Alluvial Plain tile. Same biome as the frame
	# above, but a DIFFERENT realized basket (Cotton 55% + Flax 45% vs Wild Emmer 70% + Flax 30%): two
	# tiles of one biome now carry a seeded per-tile subset, not the uniform per-biome roster. Rendered
	# beside `forage_crop_picker_cash`, the pair is the visible proof of the whole slice — read both.
	_show_tile(_cash_variant_basket_tile_fixture())
	_compose_forage(_cash_variant_basket_tile_fixture())   # settle the source key first (it changed)
	_hud._compose.set_forage_improvement("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_cash_variant_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_cash_variant")

	# Issue #419 — THE SAME CASH BASKET ONE RUNG DOWN, which had no frame at all before this. Two
	# defects were invisible without it:
	#   1. Every row printed as trade-only (`Wild Emmer 70% · 0.4 trade`), because "cash crop" was
	#      detected from `trade_payoff > 0` and EVERY staple carries the flat 0.005 trade token.
	#   2. The row quoted `sow_*` — a FIELD payoff — on the Cultivate rung, so flax advertised the
	#      2.4 trade a sown field pays instead of the 0.95 a tended patch does.
	# It must now read `Wild Emmer 70% · 2.7× · 0.11 trade` and `Flax 30% · 0.3× · 0.95 trade`: the
	# ratio the rung exists to compare is back on every row, each row states BOTH accounts it pays,
	# and the numbers are the tended rung's own. Flax's food ratio is a warn-inked LOSS and that is
	# correct — rung 2 weeds rather than replaces, so committing to flax really does surrender
	# calories, which is the cost its trade clause is the benefit of.
	_show_tile(_cash_basket_tile_fixture())
	_compose_forage(_cash_basket_tile_fixture())   # settle the source key first (it changed)
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_cash_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_cash_cultivate")

	# State 6b-sowing — the rung-3 BUILD meter: the Field row reads "Sowing 45%", following the pen's
	# "Building 40%" / the fence's "Fencing 60%" convention. It sits BESIDE the "Cultivation 🌾 Tended
	# Patch" row: the patch carries TWO independent meters, and both are the SOURCE's own.
	_show_tile(_sowing_tile_fixture())
	await _settle()
	await _save("forage_field_building")

	# State 6b-field — the COMPLETED Field, top of the plant ladder. The row must read "▦ Field" in
	# SIGNAL cyan — a visibly DIFFERENT THING from "🌾 Tended Patch" (different word, different glyph),
	# not a bigger percentage. That is the whole test of rung 3's readout.
	_show_tile(_field_tile_fixture())
	await _settle()
	await _save("forage_field")

	# State 6b-cultivate-done — a COMPLETED Tended Patch with a standing Cultivate selection: the build is
	# DONE, so Cultivate is a dead-end no-op. 🌱 Cultivate greys with "Already a Tended Patch — ♻
	# Sustain-forage it to harvest", the composed policy falls back to Sustain, and the "Preparing → then"
	# prep line is GONE (the forecast now reads the Sustain harvest, +/turn). This is the fix for the panel
	# lying: Cultivate used to stay enabled and keep paying the low prep dip on a finished patch.
	_show_tile(_tended_tile_fixture())
	_hud._compose.set_forage_improvement("cultivate")
	_compose_forage(_tended_tile_fixture())
	await _settle()
	await _save("forage_cultivate_done")

	# State forage_stale_verb — **THE TWO PUBLISHED NUMBERS MUST IMPLY ONE THROUGHPUT.** The state above
	# proved the finished patch stops OFFERING Cultivate; this one proves it stops being PRICED as one.
	# Reported from play: a tended patch reading `Forage biomass 111 / 195` with `2 foragers · +0.41
	# /turn` on the card, and a sheet beside it asking for **6 hold it after** — a crew that can only be
	# right if a forager carries ~2 biomass, while the sim's own rate for the crew already working it
	# says ~6. Nothing on screen could explain the gap: the improvement control read `🌾 Tended Patch`,
	# a DONE label, so no build was visibly in flight. The stale `Cultivate` in the compose state was.
	#
	# `seed_forage` only runs when the SOURCE changes, so a composition outlives the build it named —
	# and the sim clears the assignment's `improvement` the turn the rung completes, which is precisely
	# when the two halves of the panel start dividing by different throughputs. Staged the way play
	# reaches it: open the sheet (seeding crew + floor off the standing assignment, improvement ""),
	# then dial in the verb the finished build left behind and re-open.
	var stale_tile := _floorify(_stale_verb_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_carry := SourceForecast.per_worker_biomass(stale_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_samples := SourceForecast.regrowth_samples(stale_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_growth := SourceForecast.regrowth_at(stale_samples, STALE_VERB_FLOOR)
	# **THE CARD'S NUMBER, COMPOSED THE WAY THE SIM COMPOSES IT** — regrow, then take what stands above
	# the floor, capped by what the crew can carry (`forage_take`'s `min(worker_cap, ceiling)`). Derived
	# from the tile's own wire terms rather than written as a literal, so the standing rate and the crew
	# targets are answering about the SAME patch by construction and this assertion cannot be satisfied
	# by a fixture that drifted.
	var stale_standing_rate := minf(float(STALE_VERB_CREW) * stale_carry, stale_growth) \
		* STALE_VERB_FOOD_PER_BIOMASS
	# Captured rather than restored to a named fixture: the band in force here is whatever the state
	# before this one left, and re-seeding it from a guess is how a later state's crew quietly moves.
	var prior_player_band := _hud._band_labor.player_band()
	var prior_player_bands := _hud._band_labor._player_bands
	_hud._band_labor._player_band = _stale_verb_band_fixture(stale_standing_rate)
	_hud._band_labor._player_bands = [_hud._band_labor.player_band()]
	_show_tile(stale_tile)
	_compose_forage(stale_tile)
	_hud._compose.set_forage_improvement("cultivate")
	_compose_forage(stale_tile)
	await _settle()
	await _save("forage_stale_verb")
	var stale_sheet := _hud._drawercompose._compose_sheet
	var stale_hold := _crew_target_count(stale_sheet, HudWidgets.CREW_TARGET_HOLD)
	# (1) THE CREW TARGETS DIVIDE BY THE THROUGHPUT THE WIRE PUBLISHED. Compared against the crew terms
	# recomposed here from the source's own fields at NO dip — the answer a patch with nothing left to
	# build must give. With the stale verb pricing the crew this reads 6 against 2.
	_assert_hud("a finished rung's verb dips no crew — HOLD divides by the wire's own throughput (%d)"
		% stale_hold,
		stale_hold == SourceForecast.crew_to_hold(stale_samples, STALE_VERB_FLOOR, stale_carry, 0.0))
	_assert_hud("…and so does CLEAR, the other half of the same division",
		_crew_target_count(stale_sheet, HudWidgets.CREW_TARGET_CLEAR)
			== SourceForecast.crew_to_clear(SourceForecast.escapement_room(stale_tile,
				HudComposeVocab.FORAGE_FORECAST_PREFIX, STALE_VERB_FLOOR), stale_carry,
				SourceForecast.crew_that_reaches(stale_samples, STALE_VERB_STOCK,
					STALE_VERB_CAPACITY, STALE_VERB_FLOOR, stale_carry)))
	# (2) **THE INVARIANT THAT BROKE** — the sheet's crew target and the card's rate must imply the SAME
	# biomass per forager. The card's is a LOWER bound (its take may be bound by the room rather than by
	# the crew), so a crew target may never price a forager BELOW it: that is exactly the contradiction
	# played — 12.3 biomass moved by 2 foragers, beside a target saying a forager carries 2.
	var stale_from_card := (stale_standing_rate / STALE_VERB_FOOD_PER_BIOMASS) / float(STALE_VERB_CREW)
	var stale_from_hold := stale_growth / float(maxi(stale_hold, 1))
	_assert_hud("the card's rate and the sheet's crew imply ONE throughput (%.2f vs %.2f biomass/forager)"
		% [stale_from_card, stale_from_hold],
		stale_hold > 0 and stale_from_hold >= stale_from_card - STALE_VERB_THROUGHPUT_EPSILON)
	# (3) …and the frame really is a FINISHED patch rather than a build in flight, which is what makes
	# the two assertions above claims about a STALE verb rather than about a legitimate dip. A RUNNING
	# Cultivate is a live CheckBox; this one is the DONE state's static Label, naming the rung the
	# patch is standing on.
	var stale_control := _find_improvement_control(stale_sheet, "cultivate")
	_assert_hud("the finished rung reads as a DONE label, so no build in flight can explain a dip",
		stale_control is Label and not (stale_control is CheckBox)
			and _improvement_face(stale_sheet, "cultivate").contains(
				String(HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"])))
	_hud._band_labor._player_band = prior_player_band
	_hud._band_labor._player_bands = prior_player_bands
	_hud._compose.reset_forage_source()   # the states after this one open on their own patch

	# ---- THE BUILDING PATCH: WHEN THE REGROWTH BEATS THE ROOM ------------------------------------
	# **THE FRAME THREE DEFECTS SHARE, and no other fixture reaches it.** Reported from play: a patch
	# at `K 195` with ~9 biomass standing above its floor and ~12 growing back every turn, worked by
	# six foragers at a live Cultivate's quarter carry. It rendered `5 clear it now` · `6 hold it
	# after` · `⚠ OVERDRAWS THE PATCH` over a verdict reading *this crew can't draw it that low. It
	# settles at 54% and holds there — 7 foragers would reach the floor.* Four numbers, no two of which
	# agree, and every one of them individually correct arithmetic.
	# THE ARITHMETIC WAS NOT THE DEFECT — the numbers contradicting each other was.
	var building_tile := _floorify(_building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var build_samples := SourceForecast.regrowth_samples(building_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	# The crew term the sheet divides by, recomposed HERE from the tile's own wire fields — the carry
	# and the rung's dip, exactly as `floor_chart_model` composes it. Every relation below is stated
	# against it rather than against a literal, so a fixture that drifts fails instead of re-baselining.
	var build_carry := SourceForecast.per_worker_biomass(building_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX) \
		* SourceForecast.build_dip(building_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX, "cultivate")
	var build_reaching := SourceForecast.crew_that_reaches(build_samples, BUILD_DIP_STOCK,
		BUILD_DIP_CAPACITY, BUILD_DIP_FLOOR, build_carry)
	# THE CARD'S STANDING RATE, composed the way the sim composes it (`forage_take`'s `min(crew carry,
	# ceiling)` through the patch's food rate) — derived from the tile's own wire terms rather than
	# written down, so the card and the sheet cannot drift apart by fixture edit.
	var build_standing_rate := minf(float(BUILD_DIP_CREW) * build_carry,
		SourceForecast.escapement_room(building_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			BUILD_DIP_FLOOR)) * STALE_VERB_FOOD_PER_BIOMASS
	var prior_build_band := _hud._band_labor.player_band()
	var prior_build_bands := _hud._band_labor._player_bands
	_hud._band_labor._player_band = _building_patch_band_fixture(build_standing_rate)
	_hud._band_labor._player_bands = [_hud._band_labor.player_band()]
	_show_tile(building_tile)
	_compose_forage(building_tile)
	_hud._compose.set_forage_floor(BUILD_DIP_FLOOR)
	_hud._compose.set_forage_improvement("cultivate")
	_hud._compose.set_forage_count(BUILD_DIP_CREW)
	_compose_forage(building_tile)
	await _settle()
	await _save("forage_build_dip")
	_assert_compose_sheet_fits("forage_build_dip")
	var build_sheet := _hud._drawercompose._compose_sheet
	var build_clear := _crew_target_count(build_sheet, HudWidgets.CREW_TARGET_CLEAR)
	# (0) THE FRAME REALLY IS THE REGIME. Without this every assertion below is about an ordinary
	# patch: the whole point is a crew that CANNOT out-take the regrowth, so the crew that can must be
	# strictly larger than the one-turn quotient the target used to state.
	_assert_hud("the fixture reaches the regime — the reaching crew (%d) exceeds the one-turn quotient (%d)"
		% [build_reaching, SourceForecast.crew_to_clear(SourceForecast.escapement_room(
			building_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX, BUILD_DIP_FLOOR), build_carry, 0)],
		build_reaching > SourceForecast.crew_to_clear(SourceForecast.escapement_room(building_tile,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, BUILD_DIP_FLOOR), build_carry, 0))
	# (1) **THE INVARIANT, stated as a RELATION between the two rendered numbers** rather than as the
	# pair of literals it happens to produce: a target offering to *clear it now* may never name fewer
	# hands than the verdict beside it names as merely REACHING the floor. Those five foragers cleared
	# nothing in any number of turns.
	_assert_hud("clear-it-now (%d) is never below the crew the verdict names as reaching the floor (%d)"
		% [build_clear, build_reaching],
		build_clear >= build_reaching and build_reaching > 0)
	# (2) …AND THE STEPPER CAN REACH IT (§7.6). Flooring the target without flooring the cap trades one
	# contradiction for another — a pill naming a crew the `+` refuses. Driven through the REAL button,
	# because the clamp lives in the press handler and not in the arithmetic.
	_find_crew_target(build_sheet, HudWidgets.CREW_TARGET_CLEAR).pressed.emit()
	_assert_hud("…and the stepper reaches that crew rather than clamping it to a smaller cap",
		_hud._compose.forage_count() == build_clear)
	_hud._compose.set_forage_count(BUILD_DIP_CREW)
	_compose_forage(building_tile)
	# (3) **THE ⚠ AND THE VERDICT NOW READ THE SAME PROJECTION.** The take is well past the food-peak
	# ceiling (which is zero on a patch standing at the peak), so the per-account test still fires and
	# the gate is the only thing suppressing it — and what the gate reads is the stock CLIMBING.
	var build_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK, BUILD_DIP_CAPACITY,
		BUILD_DIP_FLOOR, float(BUILD_DIP_CREW) * build_carry)
	_assert_hud("the projection this crew produces RISES — there is nothing being overdrawn (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(build_walk["settled_fraction"])],
		float(build_walk["settled_fraction"]) > BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	_assert_hud("…so no overdraw flag fires beside a verdict saying the patch grows",
		not _hud._drawercompose._local_forage_preview_bbcode(_hud._band_labor.player_band(),
			building_tile, BUILD_DIP_FLOOR, BUILD_DIP_CREW, "cultivate").contains(HudStyle.WARN_HEX))
	# (4) THE DIP, STATED ON THE CREW ROW. Every impossible-looking number above follows from it.
	_assert_hud("a live build states its quarter carry on the crew row",
		_crew_row_dip_note(build_sheet).contains(
			str(HudFormat.progress_percent(STALE_VERB_BUILD_FRACTION))))

	# State forage_build_dip_decline — **THE OTHER HALF OF THE GATE, one hand apart.** Seven foragers
	# out-carry the patch's fastest regrowth, so the same patch at the same floor now genuinely falls
	# to the line — and the ⚠ must come back. Without this frame the assertion above passes vacuously
	# on a gate that suppressed the flag everywhere.
	_hud._compose.set_forage_count(BUILD_DIP_DECLINE_CREW)
	_compose_forage(building_tile)
	await _settle()
	await _save("forage_build_dip_decline")
	var decline_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK,
		BUILD_DIP_CAPACITY, BUILD_DIP_FLOOR, float(BUILD_DIP_DECLINE_CREW) * build_carry)
	_assert_hud("one more hand out-carries the regrowth, and the projection FALLS (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(decline_walk["settled_fraction"])],
		float(decline_walk["settled_fraction"]) < BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	_assert_hud("…and the overdraw flag fires there, so the gate subtracts rather than silences",
		_hud._drawercompose._local_forage_preview_bbcode(_hud._band_labor.player_band(),
			building_tile, BUILD_DIP_FLOOR, BUILD_DIP_DECLINE_CREW, "cultivate")
			.contains(HudStyle.WARN_HEX))
	_assert_hud("…and the verdict agrees with it — this crew reaches the floor",
		_verdict_severity(_hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)

	# State forage_build_dip_none — THE SAME PATCH WITH NO BUILD IN FLIGHT, which is the only way to
	# read the dip note as a CLAIM: a line that renders on every sheet says nothing. The crew row must
	# be bare here, and the whole sheet re-prices at the full 8.0 carry (the cap collapses to a pair of
	# hands, which is itself the dip's absence made visible).
	_hud._compose.set_forage_improvement(SourceForecast.IMPROVEMENT_NONE)
	_hud._compose.set_forage_count(BUILD_DIP_CREW)
	_compose_forage(building_tile)
	await _settle()
	await _save("forage_build_dip_none")
	_assert_hud("no build in flight, no dip claimed on the crew row",
		_crew_row_dip_note(_hud._drawercompose._compose_sheet) == "")
	_hud._band_labor._player_band = prior_build_band
	_hud._band_labor._player_bands = prior_build_bands
	_hud._compose.reset_forage_source()   # the states after this one open on their own patch

	# State 6b-sow-done — a COMPLETED Field with a standing Sow selection: ▦ Sow greys with "Already a
	# Field — ♻ Sustain-forage it to harvest", mirroring the finished-patch case one rung up (Cultivate is
	# greyed here too — the ground is both tended AND a Field).
	_show_tile(_field_tile_fixture())
	_hud._compose.set_forage_improvement("sow")
	_compose_forage(_field_tile_fixture())
	await _settle()
	await _save("forage_sow_done")

	# State forage_field_from_wild — **A FIELD SOWN STRAIGHT FROM WILD GROUND**, which the frame above
	# cannot be: its fixture climbs rung by rung, so a Field is also cultivated there and the retire
	# test passes for the wrong reason. `Sow` needs no prior patch, so `cultivation_progress` is 0 and
	# stays 0 — and the client asked "is Cultivate built?" by reading `is_cultivated`, got a truthful
	# false, and OFFERED the lower rung on a finished Field. Reported from play. The sim has never
	# agreed: `forage_rung_already_built` matches `Cultivate => patch.is_managed()`, so the box was
	# live for a build the server treats as already built.
	var wild_sown := _wild_sown_field_tile_fixture()
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_improvement("")
	_show_tile(wild_sown)
	_compose_forage(wild_sown)
	await _settle()
	await _save("forage_field_from_wild")
	_assert_hud("the fixture really is the state at issue — rung 3 built on an UNcultivated patch",
		SourceForecast.improvement_is_done(wild_sown, HudComposeVocab.FORAGE_FORECAST_PREFIX,
				SourceForecast.IMPROVEMENT_SOW)
			and not bool(wild_sown["patch_is_cultivated"]))
	_assert_hud("…so a completed Field retires Cultivate, as the sim's own rung test does",
		SourceForecast.improvement_is_done(wild_sown, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE))
	_assert_hud("…and the sheet offers no Cultivate box on it",
		not (_find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate") is CheckBox))
	# **THE PAIR THAT STOPS THIS BECOMING "CULTIVATE IS NEVER OFFERED".** A retire test that answered
	# true unconditionally would satisfy every line above; a wild patch with the knowledge in hand must
	# still offer the rung.
	_assert_hud("…while a WILD patch still offers Cultivate — the rung is retired, not deleted",
		not SourceForecast.improvement_is_done(_food_tile_fixture(),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE))
	_hud._compose.reset_forage_source()

	# ---- THE PLANT WEB'S CREW NOUN FOLLOWS THE STANDING RUNG -------------------------------------
	# Reported from play: every surface for a sown Field still said *forage* / *Foragers*. The ladder
	# config is the authority — `wild` declares the harvest primitive `worker_take`, `tended` and
	# `field` both declare `worker_tend` — so a managed source's crew are TENDERS and only a wild
	# stand's are FORAGERS. `HudFormat.plant_crew_label` is the one resolver; these four states drive
	# the four surfaces it feeds (sheet eyebrow, crew-row label, commit button, drawer open button)
	# and, on every frame, assert the eyebrow and the stepper AGREE — the disagreement being the
	# failure the single resolver exists to make unexpressible.
	await _assert_plant_crew_noun("plant_crew_wild", _food_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL)
	await _assert_plant_crew_noun("plant_crew_tended", _tended_tile_fixture(),
		HudComposeVocab.TEND_CREW_LABEL)
	# **BOTH UPPER RUNGS, NOT ONE.** A Tended Patch answers through `patch_is_cultivated` and a Field
	# sown from wild ground through `patch_is_field` + `FORECAST_RETIRED_BY_HIGHER_RUNG` — two
	# different flags reaching one noun, so a resolver that read only the first would pass above and
	# fail here (`_wild_sown_field_tile_fixture` is the Field that was never cultivated).
	await _assert_plant_crew_noun("plant_crew_field", _wild_sown_field_tile_fixture(),
		HudComposeVocab.TEND_CREW_LABEL)
	# **THE CASE A NAIVE "IS AN IMPROVEMENT COMPOSED?" TEST GETS WRONG.** These people are foraging the
	# wild stand AND clearing ground — which is exactly what the build dip charges them for — so the
	# noun must not move until the rung COMPLETES. `_building_patch_tile_fixture` is wild ground with
	# `cultivation_progress` part-way and `is_cultivated` false, and the compose carries the verb.
	await _assert_plant_crew_noun("plant_crew_wild_building", _building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL, SourceForecast.IMPROVEMENT_CULTIVATE)
	# …and its Sow twin, on the same wild ground: `Sow` needs no prior patch, so a Sow in flight is the
	# other half of "a build is running here" and must read identically.
	await _assert_plant_crew_noun("plant_crew_wild_sowing", _building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL, SourceForecast.IMPROVEMENT_SOW)
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_improvement("")

	# ---- ALL THREE ACCOUNTS ON A FORAGE FACE (issue #426, face treatment A) -----------------------
	# State forage_three_accounts — THE FRAME THIS PASS IS JUDGED ON. Every other forage fixture pays
	# provisions alone, so the picker's three-account face had no frame at all and a hay meadow was
	# indistinguishable from barren prairie. The extractive rungs must now read
	# `0.24 food · 0.01 trade · 0.40 fodder` and ascend on food and fodder — while TRADE does not,
	# `Deplete` alone carrying the ×4 market markup, which is the sim's ladder and not a fixture typo.
	# **THE PICKER STAYS THREE ABREAST, and this frame is why that is a measurement and not a guess.**
	# A wide-face column ceiling of 2 was built for exactly this face and then refuted here: at three
	# columns the sheet comes out 555px — against the deer hunt picker's long-standing 546 — nothing
	# clips, and 3 + 3 reads better than the 2 + 2 + 2 the ceiling produced. The frame is what a future
	# change to that ceiling has to argue with.
	var hay_meadow := _hay_meadow_tile_fixture()
	_show_tile(hay_meadow)
	_compose_forage(hay_meadow)   # settle the source key first (it changed)
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.set_forage_species("")
	_compose_forage(hay_meadow)
	await _settle()
	await _save("forage_three_accounts")
	_assert_compose_sheet_fits("forage_three_accounts")
	# **THE NUMBERS MOVED TO THE TOOLTIP AND THESE ASSERTIONS FOLLOWED THEM.** The claim is unchanged
	# — a three-account patch states all three, in wire order, and every one rises as the floor drops
	# because they are one stock through three fixed rates. What changed is where a player reads it:
	# the face carries the intent alone now (a preset metric is the ROOM above that floor, a one-off,
	# and it stood in food units directly over a biomass chart), so a face assertion would testify to
	# the wrong surface. The pair below is what proves the move rather than a deletion.
	_assert_hud("a forage rung names all three accounts, in wire order",
		_policy_rung_tooltip(_hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(HAY_PEAK_TOOLTIP))
	_assert_hud("every account rises together as the floor drops — one stock, three fixed rates",
		_policy_rung_tooltip(_hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_STRIP).contains(HAY_STRIP_TOOLTIP))
	_assert_hud("…and the FACE states no number at all, on any preset",
		_policy_rung_metric(_hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK) == ""
			and _policy_rung_metric(_hud._drawercompose._compose_sheet,
				SourceForecast.FLOOR_PRESET_STRIP) == "")
	# **THE NEGATIVE HALF OF THE `now → after` READING.** A crew that never reaches the floor never
	# enters the holding state, so promising it a held rate is the same class of lie as the burst
	# wearing `/TURN`.
	#
	# **ASKED AT A FLOOR BELOW THE GROWTH PEAK, AND THAT IS THE WHOLE ASSERTION.** Written first
	# against this frame's own `FLOOR_FOOD_PEAK` sheet, it was VACUOUS: at the peak the floor SITS ON
	# the fastest regrowth, so any crew that can out-carry the regrowth there can also reach it, and
	# `now == after` suppresses the arrow whether or not the gate exists — deleting the gate changed
	# no pixel. Below the peak the crew must cross faster regrowth than it will meet at the floor, so
	# settling short and having a different held rate are finally possible at once. The
	# `ungated != gated` line is what proves this crew WOULD have been shown a second number; the line
	# after it proves it was not; and `reach_crew` above them proves it is the settling crew we mean.
	var rows_key := _hud._drawercompose.YIELD_MODEL_ROWS
	var settles_crew := 1
	var gated: Dictionary = _hud._drawercompose._forage_yield_model(_hud._band_labor.player_band(),
		hay_meadow, FLOOR_CHART_HELD_FLOOR, settles_crew, SourceForecast.IMPROVEMENT_NONE, false)
	var ungated: Dictionary = _hud._drawercompose._forage_yield_model(_hud._band_labor.player_band(),
		hay_meadow, FLOOR_CHART_HELD_FLOOR, settles_crew, SourceForecast.IMPROVEMENT_NONE, true)
	_assert_hud("this crew genuinely settles SHORT of the floor it is being priced against",
		settles_crew < SourceForecast.reach_crew(hay_meadow, SourceForecast.SOURCE_KIND_FORAGE,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, FLOOR_CHART_HELD_FLOOR,
			SourceForecast.IMPROVEMENT_NONE))
	_assert_hud("…and genuinely HAS a different held rate, so the gate is what hides it",
		str(ungated.get(rows_key, [])) != str(gated.get(rows_key, [])))
	_assert_hud("…so a crew that settles short is promised NO held rate",
		not str(gated.get(rows_key, [])).contains(SourceForecast.YIELD_ROW_AFTER))
	_assert_hud("…and a row with no transition is given a header with no arrow to key",
		_yields_header(_hud._drawercompose._compose_sheet).contains("PER TURN")
			and not _yields_header(_hud._drawercompose._compose_sheet).contains("→"))
	# **THE PEAK ZONE CONTRIBUTES NOTHING, ANYWHERE.** Its line — "the most food this source can pay,
	# turn after turn, forever" — restated the definition of the preset the player had just clicked and
	# named no consequence they could act on, so it is struck from `FLOOR_ZONE_HINTS` itself rather
	# than suppressed per surface: an empty entry silences it on all five consumers, which is the
	# intent for copy worth nothing on any of them. Both halves are asserted, the TABLE's and the
	# ASIDE's, because a suppression at either level would satisfy only one.
	#
	# **PAIRED WITH THE STRIP ZONE, which must still warn.** A lone negative is satisfied by emptying
	# the whole table, and `strip`'s line is the one that may never go: it is the only place the sheet
	# says floor 0 is irreversible on the animal web, and the reaching verdict drops its own "then
	# holds it" clause there on the understanding that this line carries the consequence.
	_assert_hud("the hint TABLE carries nothing for the peak zone — the sentence said nothing",
		HudFormat.floor_hint(SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.LABOR_KIND_FORAGE) == "")
	_assert_hud("…so the readout's aside states no peak hint either",
		not _readout_aside_text(_hud._drawercompose._compose_sheet).contains("turn after turn"))
	_assert_hud("…and the STRIP zone still warns, on the web whose floor 0 is permanent",
		HudFormat.floor_hint(SourceForecast.FLOOR_MIN, SourceForecast.LABOR_KIND_HUNT)
			.contains("gone for good"))

	# State forage_three_accounts_overdraw — THE SAME meadow at floor 0 with a crew big enough to bite.
	#
	# **THE PER-ACCOUNT DIVERGENCE THIS FRAME WAS BUILT ON IS GONE, and its absence is a fact about the
	# model rather than a lost capability.** It used to author a fast fodder throughput beside a slow
	# food one, so a crew could sit inside the patch's food regrowth while stripping its hay — and the
	# verdict had to be ANY-account. The plant take is one BIOMASS quantity through three fixed rates
	# now (`forage::forage_take`'s own note: "both operands are the same biomass through the same
	# rates, so the two components agree on which side binds"), so every account overdraws or none
	# does. The `or` in the verdict is therefore inert on the plant web — kept because it costs
	# nothing and the animal web's quantised take is not obliged to stay that way.
	#
	# What the frame still pins is that the verdict tracks the FLOOR: the same crew reads amber below
	# the food peak and green at it. The crew size is load-bearing and deliberately not the auto-max —
	# below ~7 foragers LABOR binds under every ceiling and the honest verdict is renewable at every
	# floor, so a small-crew frame would pass this state's claim vacuously.
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_MIN)
	_hud._compose.set_forage_count(HAY_OVERDRAW_FORAGERS)
	_compose_forage(hay_meadow)
	await _settle()
	await _save("forage_three_accounts_overdraw")
	_assert_hud("a crew past the food peak's room overdraws — the verdict tracks the floor",
		_hud._drawercompose._local_forage_preview_bbcode(
			_hud._band_labor.player_band(), hay_meadow, SourceForecast.FLOOR_MIN, HAY_OVERDRAW_FORAGERS)
			.contains(HudStyle.WARN_HEX))
	_assert_hud("the same crew on the rung that protects the patch reads renewable",
		not _hud._drawercompose._local_forage_preview_bbcode(
			_hud._band_labor.player_band(), hay_meadow, SourceForecast.FLOOR_FOOD_PEAK, HAY_OVERDRAW_FORAGERS)
			.contains(HudStyle.WARN_HEX))

	# State forage_dead_season — THE STATE THE ISSUE IS NAMED FOR. A patch the wire fully DESCRIBES
	# and whose every cell is zero: deep winter on the same meadow. It must not be confused with
	# `tile_panel_no_forage` (no food module at all, hence no patch and correctly no compose block) —
	# here the sim has answered, and the answer is "nothing this season". So the sheet stays LOUD: the
	# rungs render, they state their zeros as `0.00 food` (the one surviving zero — an empty ceiling
	# that EXISTS is a fact worth reading), and the worker cap stays live rather than switching off.
	var dead_season := _dead_season_tile_fixture()
	_show_tile(dead_season)
	_compose_forage(dead_season)   # settle the source key first (it changed)
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_compose_forage(dead_season)
	await _settle()
	await _save("forage_dead_season")
	_assert_compose_sheet_fits("forage_dead_season")
	# THE "GOES SILENT" HALF OF THE ISSUE, and it needs the PREVIEW line rather than the rungs: a rung
	# renders whether or not it has a metric (name-only is a legal face), so asserting the picker
	# exists passes even with the bug restored. The preview line is what actually disappeared — it
	# returns "" on an unknown forecast — so it is the only witness that can testify here.
	_assert_hud("a fully-zero forecast still states its take rather than going silent",
		_hud._drawercompose._local_forage_preview_bbcode(
			_hud._band_labor.player_band(), dead_season, SourceForecast.FLOOR_FOOD_PEAK, 1) != "")
	_assert_hud("a zero rung states its zero rather than going blank",
		_policy_rung_tooltip(_hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(DEAD_SEASON_TOOLTIP))
	# The cap is the half a PNG cannot testify to: `known` is a PRESENCE test, so a described-but-empty
	# patch is capped at `MAX_USEFUL_BARREN` (1) — NOT left UNBOUNDED, which is what an undescribed one
	# gets and what the old rate-based `known` wrongly handed this state.
	_assert_hud("a described-but-empty patch caps workers rather than going unbounded",
		SourceForecast.max_useful_workers(SourceForecast.forecast_inputs(
			dead_season, SourceForecast.SOURCE_KIND_FORAGE,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK))
			== SourceForecast.MAX_USEFUL_BARREN)

	# `forage_dead_season` is ALSO the CHART's dead-season case (below), so it carries that pair of
	# assertions rather than a second identical PNG: `perWorkerBiomass` is honestly 0 in deep winter,
	# so the two crew targets have no denominator and must be ABSENT rather than rendered as a zero
	# saying "nobody is needed" — while the chart still draws, the patch's stock, its floor and its
	# growth curve all being real facts about the ground.
	_assert_hud("a dead-season patch prices no crew target rather than dividing by a zero throughput",
		_crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR)
			== CREW_TARGET_ABSENT)
	_assert_hud("…and still draws its chart, the stock and the curve being facts about the ground",
		_find_meta_node(_hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) != null)

	# ---- THE CHART, THE TARGETS AND THE VERDICT (docs/plan_harvest_floor.md §7.1/§7.3/§7.6) --------
	# Five fixtures, each breaking the instrument a DIFFERENT way — a chart is exactly the kind of
	# thing that compiles, runs, exits 0 and is visibly wrong, so each is rendered AND looked at.
	# Three are here (the two patches and the dead season above); the herd pair rides beside the wolf.
	#
	# **THE FACTION IS PUT BACK TO STILL-LEARNING CULTIVATION FOR THIS BLOCK, and that is a fixture
	# repair rather than a convenience.** These patches are WILD, so the lesson they teach is
	# Cultivation — and a source teaches nothing once the faction knows its lesson, so at the
	# all-complete dial the frames above leave behind, the aside's teaching line is correctly ABSENT
	# and the live-drag assertion below (that the line RE-READS on a drag) would be asserting nothing.
	# The pair at the end of the block flips the dial back and asserts the absence deliberately.
	_hud.update_intensification([{
		"faction": 0, "cultivation": FLOOR_CHART_CULTIVATION_LEARNING, "herding": 1.0,
		"seed_selection": 1.0, "penning": 0.0,
	}])

	# State floor_chart_full — A FULL PATCH WITH THE FLOOR ABOVE ITS STOCK. Nothing stands above the
	# line, so there is nothing to clear (that target reads 0, not a crew) and the verdict reports
	# exactly that. **The CAP does not collapse with it, and this frame is the limit case that proves
	# why** (§7.2): the room is 0, but the patch still grows a little every turn, so the crew that TAKES
	# that growth is 1 — and `max_useful_workers` floors on it rather than telling the player to drop a
	# gatherer they need on the very next turn. The chart's own subject is the
	# GEOMETRY: a nearly-full stock band under a floor line at the very top of the plot, with the
	# floor's flag FLIPPED BELOW its line, the case that would otherwise draw off the plot's edge.
	# (The *at-or-below-the-floor* verdict is stated with a real crew by `forage_dead_season` and
	# `floor_chart_herd_allee`, whose caps leave one; it cannot also be shown here, because a source
	# with no room admits no useful workers at all.)
	var full_patch := _floorify(_hay_meadow_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	_show_tile(full_patch)
	_compose_forage(full_patch)
	_hud._compose.set_forage_floor(FLOOR_CHART_ABOVE_STOCK)
	_hud._compose.set_forage_count(FLOOR_CHART_CREW)
	_compose_forage(full_patch)
	await _settle()
	await _save("floor_chart_full")
	_assert_compose_sheet_fits("floor_chart_full")
	_assert_hud("a floor above the stock is BLOCKED — the source binds, not the crew",
		_verdict_severity(_hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_BLOCKED)
	_assert_hud("…and there is nothing to clear, so that target reads zero rather than a crew",
		_crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR) == 0)
	# THE HALF A PNG CANNOT SHOW: the chart, the targets and the verdict are read against the SAME
	# crew the stepper renders. They were composed before the cap clamped it once, so the panel stated
	# a verdict for a crew it then refused to staff; this is what pins the order that fixed it.
	var full_hold := _crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
	_assert_hud("a source with no room still admits the crew that HOLDS it — the cap floors on the hold number",
		full_hold > 0)
	_assert_hud("the verdict reads the crew the stepper shows, not one the cap is about to clamp away",
		_stepper_value(_hud._drawercompose._compose_sheet) == mini(FLOOR_CHART_CREW, full_hold))

	# State floor_chart_drawn_down — THE SAME PATCH ALREADY DRAWN DOWN, worked below the food peak.
	# The stock band is amber (the patch reports Stressed), the floor sits under it, and the projection
	# must fall to the line and then run FLAT along it: a plant curve never goes negative, so a patch
	# held at a low floor is held, not lost. That is the frame the herd pair below is read against.
	var drawn_patch := _floorify(_hay_meadow_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	drawn_patch["x"] = 67
	drawn_patch["patch_ecology_phase"] = "stressed"
	drawn_patch["patch_biomass"] = FLOOR_CHART_DRAWN_STOCK_FRACTION \
		* float(drawn_patch["patch_carrying_capacity"])
	_show_tile(drawn_patch)
	_compose_forage(drawn_patch)
	# **A FLOOR BELOW THE STOCK BUT ABOVE THE BASELINE**, deliberately not `strip`: at floor 0 the
	# projection lands on the plot's own bottom edge and the "descends, then RUNS FLAT along the line"
	# reading — the whole contrast with the herd frame below — is indistinguishable from the axis.
	_hud._compose.set_forage_floor(FLOOR_CHART_HELD_FLOOR)
	_hud._compose.set_forage_count(FLOOR_CHART_CREW)
	_compose_forage(drawn_patch)
	await _settle()
	await _save("floor_chart_drawn_down")
	_assert_hud("a patch drawn toward a reachable floor states a HOLD crew, not just a clearing one",
		_crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
			!= CREW_TARGET_ABSENT)
	# **THE BURST AND THE STEADY RATE, ON THE SAME READING.** The headline take is capped by the ROOM
	# above the floor, so a crew big enough to clear that room in a turn or two had its one-off burst
	# labelled `/TURN` — the misreading this pair exists to end. Asserted as `now → after` per account
	# rather than as a second row: the three accounts are one biomass flow through a fixed vector, so
	# a second row would carry one new fact three times.
	var burst_text := _yields_text(_hud._drawercompose._compose_sheet)
	_assert_hud("a crew that reaches the floor states what it takes NOW and what it holds AFTER",
		burst_text.contains("0.22 → 0.06") and burst_text.contains("0.07 → 0.02"))
	# The `after` must be strictly SMALLER, or the reading would be claiming a drawdown pays less than
	# it does — and the two numbers coming from one function with two ceilings is exactly what could
	# silently swap them. Both parsed off the rendered face, never recomputed here.
	_assert_hud("…and the held rate is the LOWER of the two, on every account it states",
		_yield_now_after(burst_text, "FOOD")[1] < _yield_now_after(burst_text, "FOOD")[0]
			and _yield_now_after(burst_text, "FODDER")[1] < _yield_now_after(burst_text, "FODDER")[0])
	# **THE ASIDE NO LONGER NARRATES WHAT THE NUMBERS ABOVE IT ALREADY SAY.** Two lines went:
	#   • the idle-crew note (`2 of your 3 foragers go idle once it is holding — only 1 can carry what
	#     grows back`) was arithmetic over the stepper's count and the `hold it after` pill, both a
	#     centimetre above it — and that pill is a BUTTON that sets the count, so the remedy was never
	#     a sentence away either. THIS frame is the one that carried it (3 foragers, hold crew 1), so
	#     it is the frame that can testify it is gone.
	#   • the PEAK zone's hint, asserted below where a peak-floor sheet is on screen.
	# The idle needle is the whole rendered clause, not the bare count: `1` appears in the crew targets
	# and in the stepper, so a digit search would pass with the line restored.
	_assert_hud("the aside does not narrate the idle count the crew row already states twice",
		_crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD) == 1
			and not _readout_aside_text(_hud._drawercompose._compose_sheet).contains("go idle"))
	# **THE UNIT IS SAID ONCE, IN THE HEADER, AND THE HEADER KEYS THE ARROW.** Three `/TURN`s were the
	# widest thing on the row and it could not afford them once each account stated two numbers; the
	# header also stops `→` being a glyph the player has to guess. `NOW → AFTER` is the crew buttons'
	# own two words, which sit directly above it.
	var burst_header := _yields_header(_hud._drawercompose._compose_sheet)
	_assert_hud("the row states its unit once in a header, not per account",
		burst_header.contains("PER TURN") and not burst_text.contains("/TURN"))
	_assert_hud("…and the header keys the arrow while there is one to key",
		burst_header.contains("NOW → AFTER"))
	# **THE DRAG CONTRACT, which no frame can show.** A LIVE floor change must refill the readings that
	# follow the floor WITHOUT rebuilding the controls — because the rebuild `queue_free`s the chart,
	# and Godot routes motion to the node that took the press, so a rebuilt chart ends the drag on the
	# first pixel of movement. Driving the signal directly is the only way to test it headlessly: the
	# chart must SURVIVE, and the verdict must have re-read against the new floor.
	var live_chart := _find_meta_node(_hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META)
	# **AND SO MUST THE YIELDS — the reading the drag is AIMED at.** Reported from play: the verdict
	# followed the drag while the food/trade numbers sat frozen, catching up only on release when the
	# rebuild lands. Captured BEFORE the emit, because the only assertion that can see that bug is a
	# CHANGE: the stale row is a perfectly valid, perfectly findable node, so "the yields row is still
	# there" passes with the defect fully restored.
	var yields_before := _yields_text(_hud._drawercompose._compose_sheet)
	live_chart.emit_signal("floor_changed", FLOOR_CHART_ABOVE_STOCK, false)
	# **THE FRAME IS LOAD-BEARING.** `queue_free` is DEFERRED, so a rebuild leaves the old chart both
	# valid and findable for the rest of the frame it happened on — every same-frame form of this
	# assertion passes with the bug restored (measured, twice). Settling first is what makes the free
	# land, and `is_instance_valid` then answers the question actually being asked: is the node that
	# took the press still there to receive the motion?
	await _settle()
	_assert_hud("a LIVE drag leaves the chart alive — a rebuilt one would end the drag it is serving",
		is_instance_valid(live_chart))
	_assert_hud("…and the verdict has re-read against the dragged floor, without that rebuild",
		_verdict_severity(_hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_BLOCKED)
	var yields_after := _yields_text(_hud._drawercompose._compose_sheet)
	_assert_hud("…and so have the YIELDS, which are what the player is dragging TOWARD (%s → %s)"
		% [yields_before, yields_after],
		yields_before != "" and yields_after != "" and yields_after != yields_before)
	# **THE DRAG'S ONLY AFFORDANCE, which no frame can show either.** The whole plot is the drag
	# target — grabbing a 1px line would be unusable — so nothing about the chart's SHAPE says it can
	# be dragged, and a screenshot cannot carry a cursor. Reported from play: the pointer stayed an
	# arrow over the chart where the prototype showed the up/down resize cursor across the whole chart
	# area. Asserted on the control for the same reason as the pair above.
	_assert_hud("the chart wears the vertical-resize cursor, so the drag has an affordance at all",
		live_chart.mouse_default_cursor_shape == Control.CURSOR_VSIZE)
	# **THE TEACHING RATE FOLLOWS THE DRAG TOO.** `learn_multiplier` is `floor / the food peak`, so
	# the aside's cyan line is a function of the floor exactly as the yields and the crew targets are
	# — and it is the line that tells the player what the top half of the dial is FOR, so a stale one
	# is the worst of the three to leave behind. Compared before/after rather than against a literal:
	# the fixture's floor is free to move without silently retargeting this at a number.
	var teaching_before := _teaching_line(_hud._drawercompose._compose_sheet)
	live_chart.emit_signal("floor_changed", FLOOR_CHART_TEACHING_DRAG_FLOOR, false)
	await _settle()
	_assert_hud("the teaching rate re-reads on a LIVE drag, like the numbers it sits under",
		_teaching_line(_hud._drawercompose._compose_sheet) != teaching_before
			and _teaching_line(_hud._drawercompose._compose_sheet) != "")
	# Put the sheet back where the frame above left it (a live change deliberately does not re-render).
	_hud._compose.set_forage_floor(FLOOR_CHART_HELD_FLOOR)
	_compose_forage(drawn_patch)

	# State forage_lesson_known — **A LESSON THE FACTION HAS ALREADY LEARNED IS NOT TAUGHT AGAIN**, and
	# the claim is only meaningful as an A/B: this is the SAME patch, the same crew and the same floor
	# as the frame above, with the faction's Cultivation as the only thing that moves. `rung_lesson`
	# keys off the SOURCE's standing rung alone, so a wild patch went on reading `Teaching cultivation
	# at ×1.00` for the rest of the game (reported from play) — and asserting only the empty half would
	# pass on a line blanked unconditionally, which is why the learning half is captured first.
	var teaching_learning := _teaching_line(_hud._drawercompose._compose_sheet)
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])
	_compose_forage(drawn_patch)
	await _settle()
	await _save("forage_lesson_known")
	_assert_hud("a lesson still being earned IS named, so the pair is not vacuous",
		teaching_learning.contains(TEACHING_LESSON_NEEDLE)
			and teaching_learning.contains(String(SourceForecast.RUNG_LESSONS[
				SourceForecast.SOURCE_KIND_FORAGE][SourceForecast.IMPROVEMENT_NONE])))
	# NO LINE AT ALL rather than an empty one: with no build in flight there is no second half to keep.
	_assert_hud("…and the same patch teaches nothing once the faction knows it — no line, not a blank",
		_teaching_line(_hud._drawercompose._compose_sheet) == "")

	# Reset so the states after this render their usual staple patch + Sustain rung.
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.set_forage_species("")

	# ---- THE IMPROVEMENT CONTROL: three states, one axis (issue #442 §3) ------------------------
	# State 442-cultivate-running — the RUNNING state. A patch with a standing Cultivate improvement
	# renders a CHECKED box carrying the build meter, with the stance row above it untouched. The stance
	# is Sustain here; the frame below it says Deplete, and the two are equally legal.
	_hud._band_labor._player_band = _cultivating_forage_band_fixture()
	_hud._compose.reset_forage_source()
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	# CEILING-BOUND ON PURPOSE, and it is what makes the frame beneath this one legible. The readout's
	# take is `min(crew x per_worker, ceiling)`, so at a small crew LABOR binds and every floor quotes
	# the same number — the two frames would then differ only in which rung is lit. Saturating the
	# patch puts the CEILING in charge, which is the term the floor actually moves.
	_hud._compose.set_forage_count(IMPROVEMENT_STANCE_FRAME_FORAGERS)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("improvement_running_plant")
	var sustain_yields := _yields_text(_hud._drawercompose._compose_sheet)
	var running_box := _find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate")
	_assert_hud("a running Cultivate renders a CHECKED improvement box",
		running_box is CheckBox and (running_box as CheckBox).button_pressed)
	_assert_hud("…carrying the build meter the sim reports (60%)",
		_improvement_face(_hud._drawercompose._compose_sheet, "cultivate").contains("60%"))
	# READ OFF THE RENDERED PICKER, not off the compose model: the model is the input this frame sets,
	# so asserting it back proves only that the harness can write a field. What the frame claims is that
	# the stance ROW still shows Sustain lit beside a running build.
	_assert_hud("…and the stance row is untouched, still on the band's own Sustain",
		_rung_is_selected(_find_policy_rung(_hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK)))
	# **THE DEAL LINE IS GONE AND ITS PAYOFF IS ON THE FACE — asserted as a PAIR**, because "gone" alone
	# is satisfied by having deleted the payoff with it. The line's middle term restated the readout's
	# own PER TURN headline verbatim and its first term the price of building, which the crew row states
	# as a factor; only the payoff was unique to it, so only the payoff travelled — into the very
	# `· then` grammar the OFFERED box already used, so the control reads alike in both states.
	_assert_hud("…and the deal LINE beneath the box is gone",
		not _has_label_containing(_hud._drawercompose._compose_sheet,
			IMPROVEMENT_DEAL_MIDDLE_NEEDLE))
	_assert_hud("…with its payoff moved onto the running box's face, in the offer's own grammar",
		_improvement_face(_hud._drawercompose._compose_sheet, "cultivate")
			.contains(IMPROVEMENT_PAYOFF_NEEDLE))
	# **KNOWN LESSON + A BUILD IN FLIGHT — the teaching line keeps the half that is still true.**
	# Cultivation completed several frames above, so `Teaching cultivation at ×1.00` would be teaching a
	# craft this faction finished learning; one multiplier paces the lesson and the build meter alike,
	# so what survives is the BUILDING half. Both halves asserted: the word must be gone AND the
	# building sentence present, or blanking the line entirely would pass.
	_assert_hud("a lesson the faction already knows is not taught again beside a running build",
		not _teaching_line(_hud._drawercompose._compose_sheet).contains(TEACHING_LESSON_NEEDLE))
	_assert_hud("…while the BUILD half, which one multiplier still paces, keeps its line",
		_teaching_line(_hud._drawercompose._compose_sheet).contains(TEACHING_BUILD_NEEDLE))
	# **THE RUNNING BOX IS LIVE, AND IS NEVER GATED.** Unchecking abandons the build, and the abandon
	# path asks for nothing — no knowledge, no ceiling, no site, no Thriving — because abandoning a
	# STALLED build is the case it exists for. A disabled box here would be the regression the split
	# introduced by accident (under the old model, picking another policy always walked a build away).
	_assert_hud("a running improvement's box is LIVE — unchecking is always allowed",
		running_box is CheckBox and not (running_box as CheckBox).disabled)

	# State 442-deplete-beside-cultivate — **THE FRAME THE WHOLE TWO-AXIS MODEL EXISTS TO MAKE SAYABLE.**
	# The same running Cultivate at a DEEP FLOOR: legal, un-gated, and self-defeating through the
	# ecology rather than through a rule. The deeper floor frees a larger ceiling, so the crew's take
	# TODAY is bigger than the food-peak frame's — which is exactly the trap: you take more now and
	# drive the patch out of Thriving, stalling your own meter.
	_hud._compose.set_forage_floor(DEEP_DRAW_FLOOR)
	_hud._compose.set_forage_count(IMPROVEMENT_STANCE_FRAME_FORAGERS)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("improvement_deplete_while_building")
	# THE READING THAT MOVES IS THE READOUT'S, and it is the one the deal's middle term used to restate
	# — which is why deleting that term lost no information. Compared against the food-peak frame above:
	# a deeper floor frees a larger ceiling, so the same crew on the same patch must quote a different
	# take. The PAYOFF deliberately does not move (it is a property of the finished rung, not of the
	# floor), so asserting the face here would assert a constant.
	var deplete_yields := _yields_text(_hud._drawercompose._compose_sheet)
	print("ui_preview: take  peak=%s  deep=%s" % [sustain_yields, deplete_yields])
	_assert_hud("…and the RENDERED take moves with the floor, not just the model behind it",
		sustain_yields != "" and deplete_yields != "" and sustain_yields != deplete_yields)
	# BOTH AXES, READ OFF THEIR OWN CONTROLS. This asserted the two compose-model fields the frame had
	# just written, which is true whatever the sheet rendered — and "no gate, no repaint" is precisely a
	# claim about the rendering: the Deplete rung must be lit AND live, with the Cultivate box still
	# checked beside it.
	var deplete_rung := _find_policy_rung(
		_hud._drawercompose._compose_sheet, SourceForecast.FLOOR_PRESET_STRIP)
	var building_box := _find_improvement_control(_hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CULTIVATE)
	# A deep floor is NOT one of the three presets, so no preset lights — which is the honest reading
	# and the thing a picker of shortcuts must be able to say. What the frame claims is that the
	# improvement box is untouched by the floor beside it: no gate, no repaint.
	_assert_hud("a deep floor stands beside a running Cultivate — no gate, no repaint",
		deplete_rung != null and not deplete_rung.disabled and not _rung_is_selected(deplete_rung)
		and building_box is CheckBox and (building_box as CheckBox).button_pressed)
	# **THE DIP MOVED ONTO THE CREW** (`docs/plan_harvest_floor.md` §3.1), and this is the assertion
	# that pins it. The old claim here was that a deeper draw's "while building" term is BIGGER,
	# because the dip multiplied the ceiling — which is exactly the bug the move fixed: a fraction of a
	# bigger standing stock still filled the crew's baskets, so a deep floor built for free. The dip is
	# a factor on THROUGHPUT now, so the build term is floor-INDEPENDENT wherever the crew is the
	# binding side, and the two floors' build terms are EQUAL there. The crew is deliberately small
	# enough to bind under both ceilings; the take-today assertion beside it is what stops this from
	# passing vacuously on a forecast that ignores the floor altogether.
	var band := _hud._band_labor.player_band()
	var deep_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		DEEP_DRAW_FLOOR, SourceForecast.IMPROVEMENT_CULTIVATE)
	var peak_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.IMPROVEMENT_CULTIVATE)
	var deep_building := SourceForecast.expected_yield(
		deep_deal["build_forecast"], IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
	var peak_building := SourceForecast.expected_yield(
		peak_deal["build_forecast"], IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
	_assert_hud("the build term is floor-INDEPENDENT on a labour-bound crew — a deep floor builds no faster",
		is_equal_approx(deep_building, peak_building))
	_assert_hud("…while the take TODAY still rises with the deeper draw, so the frame is not vacuous",
		SourceForecast.expected_yield(deep_deal["base_forecast"], IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
		>= SourceForecast.expected_yield(peak_deal["base_forecast"], IMPROVEMENT_STANCE_FRAME_FORAGERS, band))

	# State 442-build-crew — **THE SHEET AND THE SIM, ON ONE NUMBER.** `forecast_inputs` used to take a
	# STANCE ONLY, so while a build ran the sheet read the UNDIPPED ceiling and three surfaces went wrong
	# together: the stepper let the player dial workers the sim reports idle, the green line quoted a take
	# the sim does not pay, and the overdraw verdict compared an undipped take against the Sustain bar.
	# The two cap paths are documented as twins that "can never gate differently" — and they could not,
	# because they were wrong in the SAME way, agreeing with each other while contradicting the sim.
	# So the control here is the SIM's answer: `workers_needed`, read back off the very assignment the
	# sheet is composed over.
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.set_forage_count(BUILD_CREW_DIALED_FORAGERS)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("improvement_build_crew")
	var sim_workers_needed := int((HudBandLaborState.labor_assignments_of(
		_hud._band_labor.player_band())[0] as Dictionary)["workers_needed"])
	var rendered_cap := _stepper_value(_hud._drawercompose._compose_sheet)
	print("ui_preview: build crew  sim workers_needed=%d  rendered cap=%d" % [
		sim_workers_needed, rendered_cap])
	# ONE equality, and the DIP is what carries it. Undipped the cap is ceil(0.96/0.32) = 3 — a quarter
	# of what the sim asks — and the rung's crew floor (2) sits below either, so only the dipped
	# inversion lands on the sim's 12.
	_assert_hud("the compose stepper caps at the crew the SIM asks for (%d), not at an undipped ceiling"
		% sim_workers_needed, rendered_cap == sim_workers_needed)
	# THE WORKED-ROW TWIN, on the SAME forecast. `source_worker_cap_state` is the Band panel's gate, and
	# the two are only genuinely one ceiling if it goes dead at exactly that count — asserted on either
	# side of it so "always false" cannot pass.
	var build_forecast := SourceForecast.forecast_inputs(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.IMPROVEMENT_CULTIVATE)
	var build_floor := SourceForecast.plant_crew_floor(_seeded_food_tile(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE)
	var row_below: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed - 1, BUILD_CREW_IDLE_ON_HAND, build_floor)["can_add"])
	var row_at: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed, BUILD_CREW_IDLE_ON_HAND, build_floor)["can_add"])
	_assert_hud("…and the WORK BOARD's `+` gates at the same count — live below it, dead at it",
		row_below and not row_at)
	# THE READOUT'S TAKE, read off the RENDERED sheet: it must be the sim's own
	# `min(w × per_worker × dip, ceiling)`, not the undipped labour take.
	#
	# **THE SECOND HALF OF THIS PAIR WAS THE DEAL'S "while building" TERM, and it is gone WITH the deal
	# line rather than merely untested.** The two were asserted to carry the same figure, and they did —
	# byte for byte, being the same crew through the same dipped forecast — which is precisely the
	# duplication that retired the line. What remains is the one producer.
	var build_green := _yields_text(_hud._drawercompose._compose_sheet)
	print("ui_preview: build crew  take=%s" % build_green)
	_assert_hud("the green forecast line quotes the DIPPED take the sim pays (%s)"
		% BUILD_CREW_DIPPED_TAKE, build_green.contains(BUILD_CREW_DIPPED_TAKE)
		and build_green.contains(SourceForecast.YIELD_RENEWABLE_NOTE.to_upper()))

	# THE ABANDON, plant side — driven here rather than on the frame above because committing CLOSES
	# the sheet and writes a pending assign, which the Deplete frame beside it reads.
	await _assert_abandon_emits(SourceForecast.LABOR_KIND_FORAGE, "cultivate",
		"abandon_improvement %d forage %d %d" % [HudConst.PLAYER_FACTION_ID,
			int(_food_tile_fixture()["x"]), int(_food_tile_fixture()["y"])])

	# State 442-cultivate-paused — the PAUSED build. The sim deliberately leaves this alone: a patch that
	# drops out of Thriving mid-build KEEPS its improvement and merely pauses accrual
	# (`.claude/rules/core_sim/cultivation.md` — "neither lost nor silently switched"). The control has to
	# say the same thing: the box stays CHECKED and a WARN line states the pause, its cause and the
	# ease-off remedy. This is the `_tame_stalled_hint` treatment, now on the plant web too.
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.reset_forage_source()
	_show_tile(_stressed_tile_fixture())
	_compose_forage(_stressed_tile_fixture())
	await _settle()
	await _save("improvement_paused_plant")
	var paused_box := _find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate")
	_assert_hud("a paused build keeps its box CHECKED — progress is not lost",
		paused_box is CheckBox and (paused_box as CheckBox).button_pressed)
	_assert_hud("…and the WARN line names the pause, its cause and the ease-off remedy",
		_has_label_containing(_hud._drawercompose._compose_sheet, IMPROVEMENT_PAUSED_NEEDLE))
	# **THE SHARPEST CASE FOR THE UNGATED RULE.** A STALLED build is exactly when a player reaches for
	# the abandon, so a paused box must stay LIVE — and this is the one frame where greying it would
	# look defensible (the source has left Thriving, which is what gates the build's own START). The
	# notes here are a loud WARN line, so this also pins that `notes` do not disable a RUNNING control
	# the way they disable an OFFERED one.
	_assert_hud("a PAUSED build's box is still live — abandoning a stalled build is the whole point",
		paused_box is CheckBox and not (paused_box as CheckBox).disabled)

	# State 442-cultivate-done — the DONE state. A finished patch's rung becomes a static LABEL (no box
	# to uncheck, nothing to clear), and the NEXT rung's checkbox renders beneath it. This is the state
	# that retired #420 outright: a label cannot be selected-and-gated.
	# SOWABLE ground, not the reference patch. The claim below is that the ladder CONTINUES beneath a
	# done label — which needs a next rung that is genuinely on offer. `_tended_tile_fixture` is built
	# on the reference tile, whose `sow_site_refusal` is "too_dry", so Sow there can only ever be
	# gated: the assertion would be testing the gated shape while claiming to test the offered one.
	var tended_tile := _sowable_tile_fixture()
	# `patch_`-PREFIXED, like every other key on a tile_info dict — the unprefixed spellings are the
	# RAW wire patch's, and setting those here leaves `improvement_is_done` reading nothing at all.
	tended_tile["patch_cultivation_progress"] = 1.0
	tended_tile["patch_is_cultivated"] = true
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	_hud._band_labor._player_band = _cultivating_forage_band_fixture(
		int(tended_tile["x"]), int(tended_tile["y"]))
	_hud._compose.reset_forage_source()
	_show_tile(tended_tile)
	_compose_forage(tended_tile)
	await _settle()
	await _save("improvement_done_plant")
	var done_label := _find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate")
	_assert_hud("a finished Cultivate is a static LABEL, not a checkbox",
		done_label is Label and not (done_label is CheckBox))
	_assert_hud("…naming the state the build left the patch in",
		_improvement_face(_hud._drawercompose._compose_sheet, "cultivate").contains(
			String(HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"])))
	# The ladder CONTINUES: an offerable next rung is a live checkbox, which is also what separates the
	# done state from a dead end. A gated next rung would be a Label — see `forage_sow_locked` — so
	# this assertion only means something on ground that will take seed.
	var next_rung := _find_improvement_control(_hud._drawercompose._compose_sheet, "sow")
	_assert_hud("…and the NEXT rung's LIVE checkbox sits beneath it",
		next_rung is CheckBox and not (next_rung as CheckBox).disabled)

	# State 442-offered-gated — the OFFERED state with an unmet prerequisite. A SOURCE-gated improvement
	# is SHOWN, UNCHECKED and EXPLAINED: discovering the rung exists and what it costs to unlock must
	# not require already having unlocked it.
	#
	# **THE FIXTURE MOVED FROM THE KNOWLEDGE GATE TO A SOURCE GATE, and that is the rule change rather
	# than a weakening.** It staged a wild Thriving patch with Cultivation 35% known, i.e. a rung gated
	# on KNOWLEDGE ALONE — and the compose sheet now renders NO control there at all (the aside two
	# rows up says the same lesson live and quantified, and the reason's remedy named the very work the
	# sheet was composing). A Stressed patch with Cultivation fully known keeps this frame's actual
	# subject — the gated control's SHAPE — on a gate that survives. The suppressed case is not lost
	# either: `forage_cultivate_locked` already staged exactly this fixture and is now the frame the
	# suppression rule is judged on.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.0,
	}])
	_show_tile(_stressed_tile_fixture())
	_compose_forage(_stressed_tile_fixture())
	await _settle()
	await _save("improvement_offered_gated")
	var gated_box := _find_improvement_control(_hud._drawercompose._compose_sheet, "cultivate")
	# **A GATED RUNG IS A LABEL, NOT A DISABLED CHECKBOX** — the control's SHAPE says whether this is a
	# choice or a fact, and an unmet prerequisite is a fact. The greyed-checkbox form this replaced put
	# an offer the player cannot accept ("Cultivate this patch · then 0.04 food …") directly above the
	# sentence explaining that they cannot accept it.
	_assert_hud("a gated improvement is SHOWN, never hidden — the rung stays discoverable",
		gated_box != null)
	_assert_hud("…as a LABEL rather than a checkbox, because it is a state and not a choice",
		not (gated_box is CheckBox))
	# Matched WHOLE, not by needle: this reason is the one the ecology raises, and a `contains` on a
	# fragment would still pass if the remedy clause (the half that says what to DO) went missing.
	_assert_hud("…whose own text is the REASON, so nothing offers what cannot be taken",
		_improvement_face(_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
			== HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE),
				HudFloraVocab.GATE_REASON_PATCH_THRIVING_FORMAT % String(
					_stressed_tile_fixture()["patch_ecology_phase"]).capitalize()])
	_assert_hud("…and the offer wording is gone entirely, not merely greyed",
		not _has_label_containing(_hud._drawercompose._compose_sheet, GATED_OFFER_NEEDLE))
	# THE CROP LIST IS PART OF COMMITTING, so a refused commitment offers none. Shipped once with the
	# picker rendered under the disabled box: four live, clickable crop rows beneath a checkbox whose
	# own note read "Your people know Cultivation 0%" — the card refusing the act and inviting the
	# player to configure it in the same breath. The gate NOTE stays (it answers "why not?"); the
	# CONFIGURATION goes. Found in play, not by the harness, which is why the assertion exists now.
	_assert_hud("…and offers no crop to commit to, committing being what is refused",
		_find_crop_row(_hud._drawercompose._compose_sheet, GATED_CROP_NEEDLE) == null)

	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])

	# ---- THE THIRD METER STATE: BUILDING vs REVERTING (issue #442) ------------------------------
	# **"Preparing 99%" WAS THE MOST MISLEADING LINE ON THE CARD.** A meter that is bleeding back toward
	# wild wore the build's own word in the build's own neutral ink, so gaining and losing read
	# identically — the two differ only in which DIRECTION the number is moving, which a percentage
	# cannot show. Judged as an A/B on ONE patch at ONE meter value, because the claim is precisely that
	# the same number reads differently depending on whether a crew is on it: the only thing that moves
	# between these two frames is who the player band is working.
	var meter_tile := _food_tile_fixture()
	meter_tile["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	#   (a) BUILDING — the band's own Cultivate assignment is on this tile. Neutral ink, build verb.
	_hud._band_labor._player_band = _cultivating_forage_band_fixture()
	_hud._band_labor._player_bands = [_hud._band_labor.player_band()]
	_hud.clear_selection()
	_show_tile(meter_tile)
	await _settle()
	await _save("tile_meter_building")
	var building_row := _hud.tile_detail.text
	# **WORD AND TINT IN ONE NEEDLE.** `detail_bbcode` renders a row's value as
	# `[color=#HEX]<value>[/color]`, so asserting the whole tinted cell pins both halves at once — and it
	# has to, because the old row was not merely mis-WORDED, it was mis-COLOURED: a bleeding meter wore
	# the neutral ink of a build one turn from done. A bare hex search would match any INK row on the
	# card; this one can only be satisfied by the cultivation value itself.
	_assert_hud("a meter a crew IS building reads as a BUILD, in neutral ink",
		building_row.contains(_meter_value_markup(
			HudFloraVocab.CULTIVATION_PREPARING_LABEL, HudStyle.INK_HEX)))
	#   (b) REVERTING — the SAME patch at the SAME percentage with nobody building it. The band is
	#   working a different tile, so the patch is improved, unworked and bleeding.
	_hud._band_labor._player_band = _cultivating_forage_band_fixture(
		METER_AWAY_TILE_X, int(meter_tile["y"]))
	_hud._band_labor._player_bands = [_hud._band_labor.player_band()]
	_hud.clear_selection()
	_show_tile(meter_tile)
	await _settle()
	await _save("tile_meter_reverting")
	var reverting_row := _hud.tile_detail.text
	print("ui_preview: meter rows  building=%s  reverting=%s" % [
		_detail_excerpt(building_row, CULTIVATION_ROW_KEY),
		_detail_excerpt(reverting_row, CULTIVATION_ROW_KEY)])
	_assert_hud("the SAME meter with nobody on it reads as a LOSS, in WARN ink — not a build",
		reverting_row.contains(_meter_value_markup(
			HudFloraVocab.RUNG_REVERTING_LABEL, HudStyle.WARN_HEX)))
	# THE NEGATIVE, with the positive above it as its companion (a whole-text search alone would also
	# pass on a card that rendered no cultivation row at all): the build's word must be REPLACED, not
	# merely joined — a row reading both would be the same ambiguity in longer form.
	_assert_hud("…and the build's own word is GONE from the row, not merely joined by another",
		not reverting_row.contains(HudFloraVocab.CULTIVATION_PREPARING_LABEL))

	# Restore the unassigned near band + a plain Sustain compose for the range states below.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_count(1)
	_hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# States 2-fog-a/b/c — the three SIGHT states. The player must always be able to tell "there is
	# nothing here" apart from "I can't see what's here", so the Tile card leads with a `Sight:` row and
	# an unseen hex REPLACES its Occupants roster with a statement instead of rendering an empty one.
	#   2-fog-a  Active      — `Sight: In sight` (cyan), full live card (the food_tile above).
	#   2-fog-b  Discovered  — a remembered hex that DOES carry a herd: the herd must NOT be listed and
	#                          the Occupants card must read "out of sight · …bands and herds move".
	#                          (MapView fog-gates herds out of tile_info at source; the HUD re-reads the
	#                          same visibility_state flag, so it's honest even fed a leaky dict — which
	#                          is exactly what this fixture is.)
	#   2-fog-c  Unexplored  — never seen: `Sight: Unexplored` + "Nobody has been here."
	_show_tile(_sight_tile_fixture(VIS_ACTIVE))
	await _settle()
	await _save("tile_sight_active")

	_hud.clear_selection()
	_show_tile(_sight_tile_fixture(VIS_DISCOVERED))
	await _settle()
	await _save("tile_sight_remembered")

	_hud.clear_selection()
	_show_tile(_sight_tile_fixture(VIS_UNEXPLORED))
	await _settle()
	await _save("tile_sight_unexplored")
	_hud.clear_selection()

	# States 2-fog-d/e/f — the UNIT half of the fog rule:
	#     hidden == tile not visible AND unit is not ours.
	#   2-fog-d  YOUR OWN expedition on an UNEXPLORED hex → STILL listed and selectable. This is the
	#            regression guard for the load-bearing exception: the sim excludes expeditions from fog
	#            reveal (discovery is comm-range gated), so your own party ROUTINELY stands on an
	#            Unexplored tile — a plain visibility gate would delete it from the map/roster exactly
	#            while you're using it. The roster also warns that you still can't see anything ELSE there.
	#   2-fog-e  A FOREIGN band on a fogged (Remembered) hex → NOT listed; Occupants reads out-of-sight.
	#   2-fog-f  The same foreign band on a VISIBLE hex → listed normally (neutral dot, no allocation).
	_show_tile(_own_expedition_unexplored_tile())
	await _settle()
	await _save("tile_sight_own_expedition")

	_hud.clear_selection()
	_show_tile(_foreign_band_tile(VIS_DISCOVERED))
	await _settle()
	await _save("tile_sight_foreign_hidden")

	_hud.clear_selection()
	_show_tile(_foreign_band_tile(VIS_ACTIVE))
	await _settle()
	await _save("tile_sight_foreign_visible")
	_hud.clear_selection()

	# State 2b — the same food tile, single FAR band (~21 tiles away, beyond work_range 2): foraging is
	# stationary gathering with NO expedition fallback, so the Forage button is DISABLED and an
	# out-of-range hint shows ("(66,10) is 21 tiles away — beyond this band's forage range (2)").
	_hud._band_labor._player_band = _forage_range_bands()[1]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_out_of_range")

	# State 2c — TWO bands at DIFFERENT distances from ONE food tile, NEAR band selected (821, 1 tile
	# away ≤ range 2): enabled **Forage**. The band-picker selection — not the tile — drives it.
	_hud._band_labor._player_bands = _forage_range_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_near")

	# State 2d — same two bands, FAR band selected via the picker (822, ~21 tiles away): the SAME tile
	# now DISABLES Forage + shows the out-of-range hint, proving WHICH band is selected drives the
	# enabled-vs-disabled state (the case single-band playtest can't cover).
	_hud._compose.set_forage_band(int(_forage_range_bands()[1]["entity"]))
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_far")
	# Reset so later states resolve their usual band.
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# band_alerts (above) overwrote _player_band with alert-fixture bands (which carry no hunt_reach);
	# re-seed the reference band so the herd assign controls resolve a proper band with a hunt reach.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)

	# State 3 — a huntable herd selected on a food tile, WITHIN the band's hunt reach: the "Assign
	# hunters" controls (a "Band:" dropdown naming the actor band, a Hunters −/+ count, the
	# sustain/surplus/deplete/eradicate policy picker, and the local "Hunt Here" button). A
	# Thriving herd shows a neutral ecology readout in the drawer.
	# Push both fixtures as the known-herd roster so the open-ended Attack/Defense bars have a
	# reference to normalize against (Elevation-style) — the mammoth holds the roster max.
	_set_world_herds([_herd_fixture(), _deadly_herd_fixture()])
	_show_herd(_herd_fixture())
	_compose_herd(_herd_fixture())
	await _settle()
	await _save("herd_verbs")
	# THE PAIR. Without a wild herd asserted here, "commits with the herders' verb" is satisfied by a
	# button hard-coded the OTHER way — the same bug with the sides swapped.
	_assert_hud("a WILD herd is still staffed by HUNTERS and still commits `Hunt Here`",
		_crew_row_label(_hud._drawercompose._compose_sheet)
				== HudComposeVocab.HUNT_CREW_LABEL.to_upper()
			and _compose_commit_button(_hud._drawercompose._compose_sheet) != null
			and _compose_commit_button(_hud._drawercompose._compose_sheet).text
				== HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON)
	# ASSERT the HARMLESS case: the base Red Deer carries no combat components (all default 0), so its
	# component rows all read empty — and crucially NO "Harmless"/"Deadly" verdict word appears (words
	# don't survive the roster). The rows are the raw components, Elevation-style.
	var deer_lines := DetailFormat.herd_summary_lines(_herd_fixture(), _hud._band_labor.world_herds())
	assert(_danger_component_rows_present(deer_lines))
	assert(not _danger_verdict_word_present(deer_lines))
	assert(_danger_row_value(deer_lines, "Fights back").ends_with("0%"))

	# State 3b-danger — a DEADLY-TO-HUNT herd (a mammoth: attack 8, ferocity 0.9, aggression 0). Its
	# component rows read high Attack + high Fights back but EMPTY Aggressive — the "deadly to hunt, no
	# camp threat" story at a glance. Still no verdict word.
	_show_herd(_deadly_herd_fixture())
	await _settle()
	await _save("herd_danger")
	var mammoth_lines := DetailFormat.herd_summary_lines(_deadly_herd_fixture(), _hud._band_labor.world_herds())
	assert(_danger_component_rows_present(mammoth_lines))
	assert(not _danger_verdict_word_present(mammoth_lines))
	# Fights back 90%, Aggressive 0% — the split that proves strength ≠ danger.
	assert(_danger_row_value(mammoth_lines, "Fights back").ends_with("90%"))
	assert(_danger_row_value(mammoth_lines, "Aggressive").ends_with("0%"))
	# **THE ANSWER LEADS AND ITS THREE FACTORS INDENT UNDER IT.** `Hunt` is `attack × ferocity` and
	# `Threat` is `attack × aggression`, so exactly three of the four components compose the derived
	# row — Defense is in neither and stays FLAT, above it, with the other facts about what this herd
	# is. Asserting the indent per row rather than the block's order alone: the claim is "Defense does
	# not contribute", and a reordering that indented all four would satisfy any pure ordering test.
	var mammoth_text := "\n".join(mammoth_lines)
	var indent := DetailFormat.DANGER_COMPONENT_INDENT
	_assert_hud("Danger's three factors are indented under it",
		mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_ATTACK_ROW])
			and mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_FEROCITY_ROW])
			and mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_AGGRESSION_ROW]))
	_assert_hud("…while Defense stays flat, being in neither product",
		mammoth_text.contains("\n%s: " % DetailFormat.DANGER_DEFENSE_ROW)
			and not mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_DEFENSE_ROW]))
	_assert_hud("…and the derived row LEADS them rather than trailing four equal-weight inputs",
		mammoth_text.find("%s: " % DetailFormat.DANGER_DERIVED_ROW)
			< mammoth_text.find("%s%s: " % [indent, DetailFormat.DANGER_ATTACK_ROW]))
	# **THE INDENT MUST NOT COLLIDE WITH THE FULL-WIDTH SUB-LINE PREFIX.** `detail_bbcode` routes any
	# line beginning with `MORALE_BREAKDOWN_INDENT` out of the KV table and into a full-width branch,
	# which would leave these bars starting at three different x positions — and a bar that shares no
	# column measures nothing. Both halves asserted: the prefixes cannot collide, AND the row really
	# did render as a table cell, which is the fact the first half exists to protect.
	_assert_hud("the danger indent cannot be swallowed by the full-width sub-line branch",
		not indent.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT))
	_assert_hud("…so an indented factor still renders as a KV table cell, bars in one column",
		DetailFormat.detail_bbcode(mammoth_lines, DetailFormat.Context.new()).contains(
			"[cell][color=#%s]%s%s" % [HudStyle.INK_DIM_HEX, indent, DetailFormat.DANGER_ATTACK_ROW]))

	# State 3b-predator (Predators Phase 1a) — a carnivore (Grey Wolf Pack, prey_sense_radius 4): a
	# predator is a HUNTER, not quarry, so the Size row reads "Big predator" (not "Big game") and the
	# wild-ceiling hint reads "Wild predator — hunt only" (not "Wild game — hunt only").
	_show_herd(_predator_herd_fixture())
	await _settle()
	await _save("herd_predator")
	var wolf_lines := DetailFormat.herd_summary_lines(_predator_herd_fixture(), _hud._band_labor.world_herds())
	var wolf_text := "\n".join(wolf_lines)
	assert(wolf_text.contains("Big predator"))
	assert(wolf_text.contains("Wild predator — hunt only"))
	assert(not wolf_text.contains("Big game"))
	assert(not wolf_text.contains("Wild game"))
	# A HERBIVORE (the deer, prey_sense_radius absent/0) is byte-for-byte unchanged — still "game".
	var deer_size_lines := DetailFormat.herd_summary_lines(_herd_fixture(), _hud._band_labor.world_herds())
	assert("\n".join(deer_size_lines).contains("game"))

	# State 3b — an overhunted herd: the ecology readout warns "⚠ Collapsing" in red.
	_show_herd(_collapsing_herd_fixture())
	await _settle()
	await _save("herd_collapsing")

	# State 3b-graze — the ecological carrying-capacity readout (Grazing Phase 2b-iii). A HEALTHY herd:
	# the drawer shows the merged "Herd: 15 / 22 · Thriving" pair (animals standing vs the ceiling the
	# land sets, its ecology phase riding the row) + a separate "Range: 7 tiles" row — with NO
	# overgrazing warning (biomass ≤ K).
	_show_herd(_grazing_healthy_herd_fixture())
	await _settle()
	await _save("herd_grazing_healthy")
	# ASSERT THE UNIT, not just that a pair rendered. 1480 biomass ÷ 100 body_mass = 15 animals against
	# 2150 ÷ 100 = 22 — so a row that silently kept counting biomass would read "1480 / 2150" and fail
	# BOTH halves. The phase clause is asserted on the same line because folding the standalone
	# `Ecology` row into this one is the other half of the change; if it split back out, the row would
	# read a bare "15 / 22" and this fails while a plain contains("15 / 22") would not.
	#
	# **`_assert_hud`, NOT a bare `assert`, and deliberately** — the bare form the danger rows above
	# use halts this harness on failure instead of reporting one: a headless run breaks into the
	# debugger and hangs until it is killed, so the failing line is only findable in a stack trace on
	# stderr. Measured while sabotage-checking this very assertion.
	var graze_lines := DetailFormat.herd_summary_lines(
		_grazing_healthy_herd_fixture(), _hud._band_labor.world_herds())
	var graze_text := "\n".join(graze_lines)
	_assert_hud("the herd's stock row counts ANIMALS against its ceiling, phase riding the row",
		graze_text.contains("Herd: 15 / 22 · Thriving"))
	_assert_hud("…so neither the biomass number nor its label survives anywhere on the card",
		not graze_text.contains("1480") and not graze_text.contains("Biomass"))
	_assert_hud("…and no standalone Ecology row does either — the phase is stated once, on the stock",
		not graze_text.contains("Ecology:"))

	# State 3b-overgraze — the same rows, but biomass (2100) > K (1352): the pair reads "Herd: 21 / 14"
	# (current > max) and the WARN-amber "⚠ Overgrazing — range can't sustain this herd" row appears
	# beneath. It shows ONLY when biomass exceeds K — the honest sim-number comparison, not a
	# re-derived ecology model.
	_show_herd(_overgrazing_herd_fixture())
	await _settle()
	await _save("herd_overgrazing")
	# ASSERT THE OVERSHOOT SURVIVES THE UNIT CHANGE. A `current > max` pair is the whole reason this is
	# a pair and not a fill percentage, and dividing both sides by a body could have been written to
	# clamp. 2100 ÷ 100 = 21 against 1352 ÷ 100 = 14 (13.52, rounded) — still the wrong way round.
	var overgraze_text := "\n".join(DetailFormat.herd_summary_lines(
		_overgrazing_herd_fixture(), _hud._band_labor.world_herds()))
	_assert_hud("an overgrazed herd still reads current ABOVE max, in animals",
		overgraze_text.contains("Herd: 21 / 14"))
	_assert_hud("…with the warning that says what the inverted pair costs",
		overgraze_text.contains(DetailFormat.OVERGRAZING_WARNING))

	# State 3b-smallgame — a radius-0 herd (small game grazes only its own tile): "Range: 1 tile"
	# (singular), and the map draws a single-hex highlight rather than a ring.
	_show_herd(_small_game_herd_fixture())
	await _settle()
	await _save("herd_grazing_small_game")

	# State 3c — a domesticated + corralled herd: the drawer shows "Husbandry 🐄 Domesticated"
	# AND "Corral 🐄 Corralled" (SIGNAL tint), the herd end of the intensification ladder — plus the
	# amber "Pen feed -1.74 /turn" row, the running cost a penned (non-grazing) herd costs its keeper.
	_show_herd(_domesticated_herd_fixture())
	await _settle()
	await _save("herd_domesticated")

	# State 3c-starving — the same pen, UNDERFED (`pen_fed_fraction` 0.40): the herd is shrinking
	# every turn and the drawer says so in red — "Corral ⚠ Starving — 40% fed" replaces the penned
	# badge, and the Pen feed row names the shortfall ("only 40% paid"). Biomass is visibly down.
	_show_herd(_starving_pen_herd_fixture())
	await _settle()
	await _save("herd_corral_starving")

	# Staffing readout (fauna neglect-escape arc) — the fix for the stale "N of M working" count. The
	# reference band (`_band_fixture`) staffs 4 herders on game_deer_07, and the count now comes from
	# that ACTUAL assignment, never from last turn's resolved `herded_fraction`.
	# FULLY STAFFED: the herd needs 4 and 4 are on it → a calm "Herders: 4 / 4" (neutral ink), no
	# consequence line. `herded_fraction` is a stale 0.4, so the OLD reconstruction would have read a
	# self-contradictory "2 / 4 — under-herded" — proving the fix.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_show_herd(_fully_herded_herd_fixture())
	await _settle()
	await _save("herd_fully_herded")
	# ASSERT the corrected count: the actual staffed 4 shows, not the stale reconstruction 2.
	var fully_lines := DetailFormat.herd_summary_lines(
		_fully_herded_herd_fixture(), _hud._band_labor.world_herds(),
		_hud._band_labor.assigned_herders_for(_fully_herded_herd_fixture()["id"]))
	assert(_lines_contain(fully_lines, "Herders: 4 / 4"))
	assert(not _lines_any_contain(fully_lines, "under-herded"))

	# UNDER-HERDED: the herd now needs 6 herders but only 4 are staffed → an amber "Herders: 4 / 6 —
	# under-herded" (the ACTUAL count) plus the shed line "Under-herded — animals are drifting off.
	# Staff all 6 herders to hold the herd." — NOT the retired "tameness slipping" copy. `herded_fraction`
	# is a stale 1.0, so the OLD reconstruction would have read a calm "6 / 6" with no warning.
	_show_herd(_under_herded_herd_fixture())
	await _settle()
	await _save("herd_under_herded")
	var under_lines := DetailFormat.herd_summary_lines(
		_under_herded_herd_fixture(), _hud._band_labor.world_herds(),
		_hud._band_labor.assigned_herders_for(_under_herded_herd_fixture()["id"]))
	assert(_lines_contain(under_lines, "Herders: 4 / 6 — under-herded"))
	assert(_lines_any_contain(under_lines, "animals are drifting off"))
	assert(not _lines_any_contain(under_lines, "slipping"))

	# State 2d-γ self-feeding pen — a radius-2 pen (19 fenced tiles) on lush land: the fenced footprint
	# grazes the WHOLE feed, so the feed-split reads "Fed by pasture 100% · larder 0.0 food/turn" and the
	# amber Pen-feed debit row is gone. With no ring in flight, `_build_herd_assign_controls` shows the
	# "Extend pen" button (issues extend_pen at the pen anchor). Also carries the "Pen: radius 2 · 19
	# tiles" footprint row.
	_hud._compose.reset_hunt_source()
	_show_herd(_self_feeding_pen_herd_fixture())
	_compose_herd(_self_feeding_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_self_feeding")

	# State 2d-γ extending pen — the SAME pen mid-extension (`pen_extend_progress` 0.6): the keeper is
	# fencing the next ring, so the "Extend pen" button is replaced by a WARN-amber "Fencing 60%" badge
	# (the pen twin of the corral-build "Building N%" meter). Partial pasture → "Fed by pasture 60% ·
	# larder 0.7 food/turn".
	_hud._compose.reset_hunt_source()
	_show_herd(_extending_pen_herd_fixture())
	_compose_herd(_extending_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_extending")

	# State F3 foddered pen — the honest THREE-way feed split. The pen drew hay, so its GROSS demand
	# (`pen_upkeep` 2.0) partitions into pasture 40% (0.80 free) · hay 0.9 (`pen_hay_food`) · larder 0.3
	# (`pen_larder_bill`, the NET bread bill) — 0.80 + 0.90 + 0.30 == 2.0, the sim-pinned invariant. It
	# reads "Fed by pasture 40% · hay 0.9 · larder 0.3 food/turn"; the two-term states above
	# (`herd_domesticated` 0% · larder 1.7, `herd_pen_self_feeding` 100% · larder 0.0) show NO hay term,
	# so the two forms are provably different — and the larder term is now the true net, not the gross.
	_hud._compose.reset_hunt_source()
	_show_herd(_foddered_pen_herd_fixture())
	_compose_herd(_foddered_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_foddered")

	# State 2d-δ wild ceiling — a hunt-only species. NO husbandry track in the drawer (no
	# domestication / corral / pen rows), just the dim "Wild game — hunt only" hint, and the hunt policy
	# picker offers the extractive four with NO Corral rung.
	_hud._compose.reset_hunt_source()
	_show_herd(_wild_herd_fixture())
	_compose_herd(_wild_herd_fixture())
	await _settle()
	await _save("herd_ceiling_wild")

	# State 2d-δ pastoral ceiling — tameable + roams, never pennable. The drawer KEEPS the "Husbandry
	# Domesticating 60%" row but shows "Herdable, not pennable" where the Corral rows would sit; the hunt
	# policy picker again drops the Corral rung.
	_hud._compose.reset_hunt_source()
	_show_herd(_pastoral_herd_fixture())
	_compose_herd(_pastoral_herd_fixture())
	await _settle()
	await _save("herd_ceiling_pastoral")

	# ---- Corral: the hunt INVESTMENT rung, gated then ungated -------------------------------------
	# THE PAIR IS ONE A/B: the same FULLY TAMED herd, and the only thing that moves between the two
	# frames is the faction's Penning. That is the whole claim — Corral is gated on PENNING and on
	# nothing else — and it is why Herding is fully known in both.
	#
	# **WHAT MOVES BETWEEN THE HALVES IS NOW THE CONTROL'S EXISTENCE, not its shape.** A gated Corral
	# can only ever be gated on KNOWLEDGE here (the SOURCE half is unreachable — see below), and this
	# sheet renders no control for a knowledge-only gate, so the A/B reads "no control" → "a live box"
	# rather than "a Label" → "a live box". The claim it exists to make is unchanged and, if anything,
	# sharper: the ANIMAL is identical across the two, so Penning alone is what produces the offer.
	#
	# **THE FIXTURE HAD TO CHANGE, not the description** (issue #442). These two frames used to stage a
	# 40%-tamed herd and document a gated Corral wearing its "This herd is 40% tamed" SOURCE reason; on
	# a herd that is not yet tamed the control now offers 🐾 Tame — the next rung — so no Corral gate
	# rendered in either frame and both described a subject they no longer showed. A gated Corral needs
	# Corral to BE the next rung, which needs Tame retired, which needs the herd fully tamed. That is
	# also why the SOURCE half of the gate is no longer reachable in this control at all: the moment it
	# would apply, Tame is what is offered instead, and the remedy is a checkbox rather than a sentence.
	#
	# State 3c-corral-gated — **THE SUPPRESSION RULE'S OWN FRAME, on the animal web**, and the herd is
	# what makes it worth having beside the plant one: the animal is READY and the people are not, so
	# nothing about the source explains the missing control. 🐄 Corral renders NOT AT ALL — the reason
	# it would carry ("Your people know Penning 35% — ♻ hunt a tamed herd to learn it") is the lesson
	# the aside is already stating live, and its remedy names the very hunt this sheet is composing.
	# What DOES render is the ◎ Pastoral DONE label for the rung this herd has climbed, which is what
	# keeps the absence specific rather than the whole control family having vanished.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": CORRAL_GATE_PENNING,
	}])
	_hud._compose.reset_hunt_source()
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	await _save("herd_corral_gated")
	var corral_gated := _find_improvement_control(_hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CORRAL)
	# **THE FAILURE THIS CATCHES IS AN OFFER, not a hidden Label.** Suppressing the reason without
	# suppressing the control leaves an unchecked, live `Pen this herd · then 1.50 food` box on a
	# faction 35% of the way through Penning — a commitment the sim rejects — so the assertion is
	# ABSENCE, and the DONE label below is what proves the sheet did not simply fail to build.
	_assert_hud("a Corral blocked ONLY on knowledge renders NO improvement control on this sheet",
		corral_gated == null)
	# The whole reason string, so this is safe to ask of the SHEET AT LARGE where the bare word
	# "Penning" is not (it also appears in the top-bar strip and in a hint's craft clause — exactly how
	# the `two_meter_split` assertion below once passed for the wrong reason). Suppressed must mean it
	# appears NOWHERE, including in the note slot beneath a control.
	_assert_hud("…and the knowledge reason it would have carried appears nowhere on the sheet",
		not _has_label_containing(_hud._drawercompose._compose_sheet,
			HudFloraVocab.GATE_REASON_PENNING_KNOWLEDGE_FORMAT % [
				HudFormat.progress_percent(CORRAL_GATE_PENNING),
				FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)]))
	# …and the removal is a progression rather than a hole, on this web too: the ASIDE is naming the
	# craft this herd's standing rung teaches — penning — in the same frame, live.
	_assert_hud("…while the aside still names the lesson being earned, so the rung is not silent",
		_teaching_line(_hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_HERD][
				SourceForecast.IMPROVEMENT_TAME])))
	# …and the rung it has already climbed reads as the STATE it is, above the one it cannot start.
	_assert_hud("…beneath the DONE label for the rung this herd has climbed",
		_improvement_face(_hud._drawercompose._compose_sheet, HudConst.LABOR_POLICY_TAME).contains(
			String(HudComposeVocab.IMPROVEMENT_DONE_LABELS[HudConst.LABOR_POLICY_TAME])))

	# State 3c-corral-ungated — the SAME herd once Penning is fully known. Nothing about the ANIMAL
	# changed, so if the box does not go live the gate is keyed to something it should not be.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 1.0,
	}])
	_hud._compose.reset_hunt_source()
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	await _save("herd_corral_ungated")
	var corral_ungated := _find_improvement_control(_hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CORRAL)
	_assert_hud("Penning alone unlocks Corral — the same herd now offers it as a live choice",
		corral_ungated is CheckBox and not (corral_ungated as CheckBox).disabled
		and not (corral_ungated as CheckBox).button_pressed)
	# **AND THE ASIDE STOPS TEACHING IT, in the same breath.** This is the animal half of the A/B the
	# plant web runs on `forage_lesson_known`: nothing about the herd moved between the two frames, so
	# the gated one above naming `penning` and this one naming nothing is the whole claim that the line
	# reads the FACTION and not just the rung. No build is composed here, so no half of the sentence
	# survives — a line, not a blank.
	_assert_hud("…and the aside stops teaching a craft the faction has finished learning",
		_teaching_line(_hud._drawercompose._compose_sheet) == "")
	# **AN UNTICKED BOX HAS TO BE THERE TO BE TICKED.** Godot's stock `unchecked` art is a FILLED
	# near-black square drawn for a LIGHT surface, so on this console it reserved its width and painted
	# nothing: an offer that read as a line of prose with no control on it. Measure the thing that was
	# actually wrong — CONTRAST against the panel — rather than the presence of an override: the first
	# cut of the fix set `icon_normal_color`, which a CheckBox ignores entirely, and an override-shaped
	# assertion would have passed on it.
	_assert_hud("an offered rung's box is VISIBLE against the panel, not black on black",
		_checkbox_indicator_contrast(corral_ungated, "unchecked")
		>= CHECKBOX_INDICATOR_MIN_CONTRAST)
	# The ticked half needs a DIFFERENT question asked of it: the stock `checked` art is a light chip and
	# already clears the contrast bar, so re-using that measure here would pass with the fix removed —
	# a vacuous assertion. What the designer asked for is that a running build be unmistakable, so pin
	# the HUE: ticked reads in `SIGNAL`, the colour this HUD uses for nothing but live state.
	_assert_hud("…and the ticked art reads in SIGNAL, so a running build is unmistakably running",
		_checkbox_tick_colour_gap(corral_ungated) <= CHECKBOX_TICK_COLOUR_TOLERANCE)

	# State 3d-corral — a fully-domesticated, not-yet-penned herd with the pen 40% built: 🐄 Corral is
	# ENABLED and selected, the forecast states the deal ("Preparing: +0.23 /turn → then +1.50 /turn
	# − 0.34 feed", the `corral` ceiling row → corral_yield minus the projected pen_upkeep, stepper capped at the
	# 1 keeper a managed source needs), and the drawer carries the "Corral: Building 40%" row — the
	# herd twin of the tile's "Cultivation N%". The picker's 🐄 Corral button wears the `→ +1.50/turn`
	# PAYOFF (corral_yield), above ◎ Tame's `→ +1.20/turn` and Sustain's `up to +0.90/turn`.
	#
	# `pen_upkeep` is the feed this pen WOULD demand once built — the sim projects it at the herd's
	# current biomass (on the same basis as `corral_yield`), so the pre-commit row subtracts the real
	# running cost rather than saying "before feed".
	_hud._compose.reset_hunt_source()
	_show_herd(_corral_ready_herd_fixture())
	_compose_herd(_corral_ready_herd_fixture(), COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, "corral")
	await _settle()
	await _save("herd_corral")
	# **THE COMMIT VERB FOLLOWS THE CREW NOUN ON THIS WEB TOO, and it did not.** `_herd_crew_noun` has
	# always resolved Hunters/Herders off the standing rung, and the eyebrow, the stepper and the
	# drawer's open button all followed — but the commit button was HARD-CODED, so an `ASSIGN HERDERS`
	# sheet over a `Herders` stepper committed with `Hunt Here`. Reported from play. Asserted with the
	# stepper beside it, because the claim is that the two agree, not merely that the button changed.
	_assert_hud("a managed herd's sheet is staffed by HERDERS",
		_crew_row_label(_hud._drawercompose._compose_sheet)
			== HudComposeVocab.HERD_CREW_LABEL.to_upper())
	_assert_hud("…and commits with their own verb, not the hunt one",
		_compose_commit_button(_hud._drawercompose._compose_sheet) != null
			and _compose_commit_button(_hud._drawercompose._compose_sheet).text
				== HudComposeVocab.ASSIGN_LOCAL_HERD_BUTTON)

	# State 3d-corral-under-herded — the HERDER-DEFICIT cap fix. A composing-Corral herd needs 2 herders
	# every turn to hold its tameness, but the Corral rung's take/prepare max-useful is 1. The compose
	# stepper's cap must be max(1, herders_needed 2) = 2, so the `+` reaches 2 and the maintenance crew is
	# staffable (an under-herded corral is otherwise an unwinnable trap). The drawer's Herders row reads
	# "1 / 2 — under-herded" and the shed consequence line ("animals are drifting off. Staff all 2 herders
	# to hold the herd." — NEVER the retired "tameness slipping" copy) names 2 — the SAME herders_needed
	# the cap uses.
	# Auto-max (a policy click arms the compose hunt autofill) fills the crew to the corrected cap of 2.
	#
	# THE STAFFING IS DIALED DOWN FOR THIS STATE ONLY. The drawer's "Herders A / N" row reads the band's
	# ACTUAL hunt assignment on this herd (`assigned_herders_for`), and the reference band staffs 4 — which
	# renders a FULLY-herded corral and hides the very shortfall the cap floor exists to fix. The row and
	# the cap floor have to DISAGREE for the deficit to be visible, so this band staffs 1 against the herd's
	# requirement of 2; `_band_fixture()` is restored immediately after the save, since `herd_corral_depleted`
	# and every state downstream document the reference band's 4.
	var under_herded_band := _band_fixture().duplicate(true)
	for entry in under_herded_band["labor_assignments"]:
		if entry is Dictionary and String((entry as Dictionary).get("kind", "")) == "hunt":
			(entry as Dictionary)["workers"] = UNDER_HERDED_CORRAL_HERDERS_STAFFED
	_hud._band_labor._player_band = under_herded_band
	_hud._compose.reset_hunt_source()
	_show_herd(_under_herded_corral_fixture())
	# The three-line auto-max idiom (`herd_hunt_automax`): open once so the rung is composed, arm the
	# one-shot, then re-open so it is consumed against the COMPOSED Corral. Arming before the first open
	# spends the one-shot on the re-seeded rung instead, and dialing an explicit count would overwrite
	# whatever auto-max produced — the frame must show auto-max REACHING the cap, not advertising it.
	_compose_herd(_under_herded_corral_fixture(), COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, "corral")
	_hud._compose.arm_hunt_autofill()
	_compose_herd(_under_herded_corral_fixture())
	await _settle()
	await _save("herd_corral_under_herded")
	# The claim is REACHABILITY, and it is the herder FLOOR that guarantees it: auto-max fills to the
	# cap, and the cap is never below the crew the sim demands. It is no longer EQUAL to that crew,
	# because the cap is now the SELECTED STANCE's take-useful raised to the floor (issue #442) rather
	# than a build verb's 1-worker prep count raised to it — a crew building a pen still hunts, and the
	# stance says how hard. Asserting equality would pin the old overload's arithmetic.
	_assert_hud("auto-max fills the corral crew to at least the herder deficit, proving it is reachable",
		_hud._compose.hunt_count() >= UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	# Restore the reference band (4 herders on game_deer_07) for everything downstream.
	_hud._band_labor._player_band = _band_fixture()

	# State 3d-corral-depleted — the SAME rung on a herd BELOW the pen's escapement point (K/2). The
	# managed harvest takes only the biomass standing above that point, so the payoff is honestly
	# +0.00 /turn while the feed is still 0.14 — a pure loss. The face must SHOW both zeros and carry
	# the WARN "⚠ Too depleted to pen" note, never suppress the zero as if it were missing data.
	_hud._compose.reset_hunt_source()
	_show_herd(_depleted_corral_herd_fixture())
	_compose_herd(_depleted_corral_herd_fixture(), COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, "corral")
	await _settle()
	await _save("herd_corral_depleted")
	# **THE WARNING SURVIVED THE DEAL LINE IT WAS WRITTEN UNDER.** It rides the improvement control's
	# own note slot now (the slot the paused-build line uses), so this frame — the only one that
	# produces it — is where a silent loss would show. The zero it explains is asserted beside it: a
	# note over a suppressed payoff would be a warning about a number the player cannot see.
	_assert_hud("a pen that would pay nothing says so, in the note slot under its own box",
		_has_label_containing(_hud._drawercompose._compose_sheet,
			HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE))
	_assert_hud("…above a face that still states the zero payoff and the feed it would still eat",
		_improvement_face(_hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CORRAL)
			.contains(SourceForecast.PICKER_FOOD_PRODUCT_FORMAT
				% SourceForecast.format_magnitude(0.0)))

	# ---- THE INTENSIFICATION LADDER, slice 6b -----------------------------------------------------
	# THE TWO-METER SPLIT (docs/plan_intensification_ladder.md §4.1) — the headline of this slice, and
	# the frame it is judged on. Two meters advance from one action and they are DIFFERENT KINDS of
	# thing; this state puts both on screen at once so the distinction can actually be seen:
	#   • FACTION KNOWLEDGE — the top-bar strip, prefixed "⚒ Your people know:". Herding ✔ known,
	#     Penning still learning at 45%. This is your PEOPLE's craft: faction-wide, permanent, earned
	#     by practice. It appears NOWHERE else — never in the drawer below.
	#   • PER-SOURCE PROGRESS — this herd's own "Husbandry: 🐄 Domesticated" row, down in its drawer.
	#     Local to THIS animal, and it decays if abandoned.
	# The bridge between them is the readout's ASIDE — the teaching line, which names the craft this
	# herd's standing rung is earning, live, on the sheet where the work is composed.
	#
	# **THAT BRIDGE USED TO BE THE GATED 🐄 CORRAL'S REASON LINE, and it is gone from this sheet.** A
	# knowledge-only gate renders no control here at all, so the sheet no longer carries a knowledge
	# PERCENT anywhere — that number lives in the top-bar strip alone, which is the first assertion
	# below. The reason string itself is unchanged and still rendered by every other surface that shows
	# a gate (`RungGates.hunt_gates` is untouched); what is pinned here now is the aside.
	#
	# **THE HERD IS FULLY TAMED, and it has to be** (issue #442). The frame staged a 40%-tamed herd
	# while the bridge line was a rung of the policy picker; the improvement control offers the NEXT
	# rung, so on a part-tamed herd it offers 🐾 Tame and no gate reason renders at all — the frame kept
	# its subject in its comment and lost it on screen. Retiring Tame is what makes Corral the rung on
	# offer, and a Corral gated on Penning is the only shape this bridge has. It also sharpens the
	# contrast the frame exists for: the ANIMAL is ready and the PEOPLE are not, so the two meters are
	# unmistakably about different things.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.12,
		"penning": TWO_METER_PENNING,
	}])
	_hud._compose.reset_hunt_source()
	_show_herd(_fully_tamed_herd_fixture())
	_compose_herd(_fully_tamed_herd_fixture())
	await _settle()
	await _save("two_meter_split")
	# THE TWO-METER SPLIT'S OWN INVARIANT, asked of the three SPECIFIC controls that carry it. The pair
	# that stood here searched the WHOLE SHEET for the word "Penning" and was matching the Sustain
	# HINT's craft clause, not the gate reason it claimed to test — the shape this sweep exists to
	# remove. Each half below names its own surface, so a regression says which one moved.
	_assert_hud("the FACTION's craft lives in the top-bar strip",
		_hud.intensification_label.visible
		and _hud.intensification_label.text.contains(
			String(TopBarReadouts.KNOWLEDGE_TRACK_LABELS[HudFloraVocab.KNOWLEDGE_TRACK_PENNING])))
	_assert_hud("…and THIS HERD's own progress lives in its own drawer's Husbandry row",
		_has_label_containing(_hud.occupant_detail,
			DetailFormat.husbandry_label(DetailFormat.HUSBANDRY_PROGRESS_COMPLETE)))
	_assert_hud("…and no knowledge percent leaks into the drawer, where it would read as a stat of the animal",
		not _has_label_containing(_hud.occupant_detail,
			String(TopBarReadouts.KNOWLEDGE_TRACK_LABELS[HudFloraVocab.KNOWLEDGE_TRACK_PENNING])))
	# **THE GATED-CORRAL BRIDGE ASSERTION IS REMOVED, NOT WEAKENED.** It read the gated Corral control's
	# own face — "Your people know Penning 45% — ♻ hunt a tamed herd to learn it" — and the compose
	# sheet renders no control at all for a knowledge-only gate, so its subject no longer occurs here.
	# The suppression itself is pinned on `herd_corral_gated` (absence + the reason nowhere on the
	# sheet); the reason STRING is still `RungGates.hunt_gates`' and still rendered wherever a gate
	# reason is shown outside this sheet.
	#
	# THE BRIDGE, as it now stands — the aside's teaching line, read BY META so neither the hint text
	# nor the top-bar strip can satisfy it. It names the CRAFT and not the percent, which is the honest
	# claim: the sheet is where the lesson is being earned, the strip is where its progress is read.
	_assert_hud("…and the aside's teaching line is the bridge: the craft this herd's rung teaches",
		_teaching_line(_hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_HERD][
				SourceForecast.IMPROVEMENT_TAME])))

	# State 6b-tame — the ◎ Tame affordance itself: a 6th option in the LOCAL hunt picker, beside
	# Sustain/Surplus/Deplete/Eradicate/Corral, ENABLED (Herding is known) and selected on a
	# pen-ceiling herd that is only 40% tamed. Now that the sim exports `pastoralYield`, Tame renders
	# the SAME dip→payoff pair as its three siblings: "Preparing: +<dip> → then +1.20 /turn" (dip from
	# `hunt_policy_ceilings["tame"]`, payoff = pastoral_yield, no feed term — Tame has no running cost).
	# Its picker button wears the `→ +1.20/turn` payoff, above Sustain's `up to +0.90/turn`.
	await _save("herd_tame")

	# State 6b-tame-stalled — the "why isn't my Tame progressing?" hint. Taming accrues ONLY while the
	# herd is Thriving, but is deliberately NOT gated on it (a herd's phase swings as you hunt it), so
	# the sim just PAUSES the meter. Silence here would recreate exactly the hidden-rule problem this
	# arc exists to kill, so the drawer says it: what stopped, why, that progress is NOT lost, and the
	# remedy (ease off — the opposite of "work harder").
	_hud._compose.reset_hunt_source()
	_show_herd(_taming_stalled_herd_fixture())
	_compose_herd(_taming_stalled_herd_fixture(), COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, "tame")
	await _settle()
	await _save("herd_tame_stalled")

	# TAMING-STARTUP-LAG GUARD — composing an INVESTMENT rung (Tame) on a still-WILD herd must offer the
	# ownership-INDEPENDENT would-be herder crew, not the 1-worker Tame-prep count. A wild herd's
	# `herders_needed` is ownership-gated to 0, so the take/prepare max-useful (1) used to pin the cap at 1;
	# the player could staff only 1, the herd became owned next turn needing 3, and read under-herded. The
	# fix floors the LOCAL-hunt cap on `herders_needed_if_managed` (3) for investment rungs only.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	# A band with idle workers comfortably above both caps (Tame 30, Sustain 7), so the stepper is bound by
	# USEFULNESS (the "max N useful here" note), not by the idle-labor ceiling (a different note entirely).
	var tame_cap_band := _band_fixture()
	tame_cap_band["idle_workers"] = TAME_CAP_WOULD_BE_HERDERS * 2
	tame_cap_band["working_age"] = TAME_CAP_WOULD_BE_HERDERS * 3
	_hud._band_labor._player_band = tame_cap_band
	_hud._band_labor._player_bands = [tame_cap_band]
	_hud._compose.reset_hunt_source()
	_show_herd(_tame_worker_cap_herd_fixture())
	# Tame is DIALED IN through `_compose_herd`, which survives the source-change re-seed — see its doc.
	_compose_herd(_tame_worker_cap_herd_fixture(), COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, "tame")
	await _settle()
	await _save("herd_tame_worker_cap")
	# Tame floors the cap on the would-be crew (10), NOT the Tame-prep useful (1): the sheet's max-useful
	# note reads "max 10 workers useful here". Pre-fix it read "max 1 worker useful here" (floored on the
	# ownership-gated herders_needed 0).
	_assert_hud("Tame offers the full would-be herder crew (max %d), not the 1-worker prep count"
		% TAME_CAP_WOULD_BE_HERDERS,
		_has_label_containing(_hud._drawercompose._compose_sheet,
			"max %d workers useful" % TAME_CAP_WOULD_BE_HERDERS))
	_assert_hud("…and not the pre-fix 1-worker cap",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "max 1 worker useful"))
	# COMPANION — the EXTRACTIVE Sustain rung manages nothing, so it needs no herders: its cap is
	# take-useful only (Sustain 1.50 ÷ 0.30 = 5), and the would-be crew (3) must NOT leak into it.
	_hud._compose.reset_hunt_source()
	_show_herd(_tame_worker_cap_herd_fixture())
	_compose_herd(_tame_worker_cap_herd_fixture(), COMPOSE_COUNT_UNSET, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_tame_worker_cap_sustain")
	_assert_hud("Sustain caps on its own take-useful (max 7), floored at 0",
		_has_label_containing(_hud._drawercompose._compose_sheet, "max 7 workers useful"))
	_assert_hud("…the would-be herder crew (%d) does not leak into an extractive rung"
		% TAME_CAP_WOULD_BE_HERDERS,
		not _has_label_containing(_hud._drawercompose._compose_sheet,
			"max %d workers useful" % TAME_CAP_WOULD_BE_HERDERS))

	# State 442-tame-running — THE ANIMAL WEB's running improvement, the exact twin of
	# `improvement_running_plant`. Same control, same three states, same forecast: the two ladders are
	# one grammar (spec §4), and rendering them together is what proves it.
	_hud._band_labor._player_band = _tame_standing_band_fixture()
	_hud._band_labor._player_bands = [_tame_standing_band_fixture()]
	_hud._compose.reset_hunt_source()
	_show_herd(_taming_herd_fixture())
	_compose_herd(_taming_herd_fixture())
	await _settle()
	await _save("improvement_running_animal")
	var tame_box := _find_improvement_control(_hud._drawercompose._compose_sheet, "tame")
	_assert_hud("a running Tame renders a CHECKED improvement box, as Cultivate does",
		tame_box is CheckBox and (tame_box as CheckBox).button_pressed)
	# **THE SAME PAIR ITS PLANT TWIN CARRIES, on the web that shares the control.** The deal LINE is
	# gone from both sheets and the payoff rides both faces; asserting only the absence would pass on a
	# sheet that had lost the payoff too, which is why the second half names the face.
	_assert_hud("…with no deal LINE beneath it, exactly as the plant web has none",
		not _has_label_containing(_hud._drawercompose._compose_sheet,
			IMPROVEMENT_DEAL_MIDDLE_NEEDLE))
	_assert_hud("…and the payoff on the running box's face, in the offer's own grammar",
		_improvement_face(_hud._drawercompose._compose_sheet, "tame")
			.contains(IMPROVEMENT_PAYOFF_NEEDLE))
	# KNOWN LESSON + A BUILD IN FLIGHT, on the animal web: Herding is complete for this faction, so the
	# aside drops the craft and keeps the build the same multiplier paces. Both halves, for the reason
	# the plant twin states.
	_assert_hud("a known lesson is not taught again on the hunt sheet either",
		not _teaching_line(_hud._drawercompose._compose_sheet).contains(TEACHING_LESSON_NEEDLE))
	_assert_hud("…while its BUILD half still reads, as it does on the plant sheet",
		_teaching_line(_hud._drawercompose._compose_sheet).contains(TEACHING_BUILD_NEEDLE))
	_assert_hud("a running Tame's box is LIVE too — the abandon path is ungated on both webs",
		tame_box is CheckBox and not (tame_box as CheckBox).disabled)
	# **THE HERD FORM, which is the one a shared branch gets wrong.** `abandon_improvement` targets by
	# WEB (`hunt` → herd id) while the SET verbs target by VERB — and `corral` is a herd's rung
	# addressed by a TILE, so a formatter that reused the set-verb rule would send coordinates here.
	await _assert_abandon_emits(SourceForecast.LABOR_KIND_HUNT, HudConst.LABOR_POLICY_TAME,
		"abandon_improvement %d hunt %s" % [HudConst.PLAYER_FACTION_ID,
			String(_taming_herd_fixture()["id"])])

	# State 442-tame-done — the animal DONE state, and **THE ONE ASYMMETRY THAT SURVIVES** (spec §4):
	# a fully tamed herd's ◎ Pastoral label carries NO upkeep, because a pastoral herd still grazes.
	# Its Corral twin below does, because a penned one cannot. The next rung's box (🐄 Corral) sits
	# beneath the label, which is what the done state is for.
	var tamed_herd := _fully_tamed_herd_fixture()
	_hud._compose.reset_hunt_source()
	_show_herd(tamed_herd)
	_compose_herd(tamed_herd)
	await _settle()
	await _save("improvement_done_animal")
	var pastoral_label := _find_improvement_control(_hud._drawercompose._compose_sheet, "tame")
	_assert_hud("a finished Tame is a static LABEL, not a checkbox",
		pastoral_label is Label and not (pastoral_label is CheckBox))
	_assert_hud("…and carries NO upkeep — a pastoral herd still grazes (the asymmetry, held)",
		pastoral_label != null
		and not _improvement_face(_hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).contains(UPKEEP_NEEDLE))
	_assert_hud("…with the next rung, Corral, offered beneath it",
		_find_improvement_control(_hud._drawercompose._compose_sheet, "corral") is CheckBox)

	# State 442-corral-done — the OTHER half of that asymmetry: a PENNED herd's 🐄 label DOES carry the
	# pen's per-turn fodder upkeep, because a penned herd cannot graze and someone feeds it every turn.
	# A standing obligation belongs with the standing state. The two frames must NOT be made to match.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = [_band_fixture()]
	_hud._compose.reset_hunt_source()
	_show_herd(_domesticated_herd_fixture())
	_compose_herd(_domesticated_herd_fixture())
	await _settle()
	await _save("improvement_done_penned")
	var penned_label := _find_improvement_control(_hud._drawercompose._compose_sheet, "corral")
	_assert_hud("a finished Corral is a static LABEL",
		penned_label is Label and not (penned_label is CheckBox))
	_assert_hud("…and DOES carry the pen's upkeep — the one asymmetry between the two webs",
		_improvement_face(_hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL).contains(UPKEEP_NEEDLE))

	# ---- THE BUILDING HERD: A DIPPED CREW THAT CANNOT CARRY ONE BODY ------------------------------
	# **THE ANIMAL WEB'S HALF OF THE DEFECT THE PLANT SHEET HAS ALREADY LOST.** `herd_axis_rates`
	# composed its forecast at the DEFAULT improvement, so every number the local-hunt preview quoted —
	# the take, the waste split, the animals-per-turn line — was priced as though nobody were building
	# anything, while the sim pays a gentling crew `workers × per_worker × build_dip`. The worker cap,
	# the chart and both crew targets beside them already carried the verb, so the sheet disagreed with
	# the sim AND with itself.
	#
	# It is staged at the ONE regime where the dip is not a scaling: the crew crosses BELOW one body
	# mass, so `quantise_animal_take`'s `max(1, carryable)` turns the shortfall into a kill it cannot
	# haul. The two frames are judged as a PAIR — this one and the same herd with no build in flight —
	# because "every number got smaller" is exactly what a wrong fix produces too.
	var dip_herd := _floorify(_building_herd_fixture())
	var prior_dip_band := _hud._band_labor.player_band()
	var prior_dip_bands := _hud._band_labor._player_bands
	_hud._band_labor._player_band = _building_herd_band_fixture()
	_hud._band_labor._player_bands = [_hud._band_labor.player_band()]
	# THE SIM'S OWN COMPOSITION, recomposed from the herd's wire terms rather than written down, so a
	# fixture that drifts fails these assertions instead of quietly re-baselining them.
	var dip_fraction := SourceForecast.build_dip(dip_herd, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME)
	var dip_fpa := float(dip_herd["food_per_animal"])
	var dip_ceiling := SourceForecast.escapement_room(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR) \
		* float(dip_herd["provisions_per_biomass"])
	var bare_collection := float(HERD_DIP_CREW) * float(dip_herd["per_worker_yield"])
	var built_collection := bare_collection * dip_fraction
	var bare_take := _hunt_take_oracle(bare_collection, dip_ceiling, dip_fpa)
	var built_take := _hunt_take_oracle(built_collection, dip_ceiling, dip_fpa)
	var bare_face: String = HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT \
		% _hud._drawercompose._format_animal_rate(float(bare_take["delivered"]) / dip_fpa)
	var built_face: String = HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT \
		% _hud._drawercompose._format_animal_rate(float(built_take["delivered"]) / dip_fpa)
	var built_killed: float = float(built_take["delivered"]) + float(built_take["wasted"])
	var built_waste_pct := int(round(float(built_take["wasted"]) / built_killed * 100.0))
	_hud._compose.reset_hunt_source()
	_show_herd(dip_herd)
	_compose_herd(dip_herd, HERD_DIP_CREW, HERD_DIP_FLOOR, SourceForecast.IMPROVEMENT_TAME)
	await _settle()
	await _save("herd_build_dip")
	var dip_sheet := _hud._drawercompose._compose_sheet
	# (0) THE FRAME REALLY IS THE REGIME, and without this every assertion below is about an ordinary
	# hunt: the crew must carry a whole body undipped and less than one under the build, which is the
	# only place `max(1, carryable)` bites and therefore the only place the waste line can move.
	_assert_hud(("the fixture reaches the regime — %d hunters carry a whole %.2f-food body (%.2f) and "
		+ "the same crew gentling the herd carries %.2f, less than one")
		% [HERD_DIP_CREW, dip_fpa, bare_collection, built_collection],
		dip_fraction < SourceForecast.NO_BUILD_DIP and bare_collection >= dip_fpa
			and built_collection < dip_fpa)
	# (1) …AND A BUILD REALLY IS IN FLIGHT. A dip with no visible build is the stale-verb defect, a
	# different bug wearing the same numbers, so the frame states which one it is: a LIVE ticked box.
	var dip_box := _find_improvement_control(dip_sheet, SourceForecast.IMPROVEMENT_TAME)
	_assert_hud("…and the sheet is visibly BUILDING — a live, ticked Tame, not a stale verb",
		dip_box is CheckBox and (dip_box as CheckBox).button_pressed)
	_assert_hud("…staffed by the composed crew (%d), so the cap is not what the frame measures"
		% HERD_DIP_CREW, _stepper_value(dip_sheet) == HERD_DIP_CREW)
	# (2) **THE TAKE IS THE SIM'S DIPPED ONE.** Stated as the sim's own composition of the herd's wire
	# terms and as a RELATION to the undipped take — never as a literal — so a config retune moves the
	# fixture rather than the claim. Undipped this crew lands a whole animal a turn; it must not say so
	# while it is gentling the herd instead.
	_assert_hud("the take is the sim's DIPPED one (%s/turn), not the undipped %s/turn"
		% [built_face, bare_face],
		_yields_text(dip_sheet).contains(built_face)
			and not _yields_text(dip_sheet).contains(bare_face))
	_assert_hud("…and it is strictly under the take the same crew would land hunting (%.2f < %.2f food/turn)"
		% [float(built_take["delivered"]), float(bare_take["delivered"])],
		float(built_take["delivered"]) < float(bare_take["delivered"]))
	# (3) **AND THE WASTE IS WHAT MOVED**, which is the half a scaled-down take cannot produce: the crew
	# still kills one animal and leaves the part it cannot haul. A build that merely shrank the take
	# would render no waste note at all.
	_assert_hud("…because the dipped crew kills a body it cannot carry — %d%% wasted" % built_waste_pct,
		built_waste_pct > 0
			# The readout's small print is UPPERCASED by `HudWidgets._readout_unit_label`, so every
			# needle aimed at the note/waste labels is raised here. The NUMBER labels beside them are
			# not, which is why the rate needles above are compared as written.
			and _yields_text(dip_sheet).contains(
				(SourceForecast.HUNT_WASTE_NOTE_FORMAT % built_waste_pct).to_upper()))
	# (4) **THE OVERDRAW GATE WALKS THE CREW THE TAKE IS PRICED FOR.** It was asked at
	# `IMPROVEMENT_NONE` to match takes that were themselves undipped; with the takes fixed, an undipped
	# projection walks a crew four times the one being quoted. This herd's regrowth sits BETWEEN the two
	# carries, so the two answers genuinely differ here and the argument is load-bearing rather than
	# decorative.
	_assert_hud("the overdraw gate walks the DIPPED crew — this herd grows under it, though it falls under the undipped one",
		not SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
				HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW,
				SourceForecast.IMPROVEMENT_TAME)
			and SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
				HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW,
				SourceForecast.IMPROVEMENT_NONE))
	_assert_hud("…so the row reads renewable rather than flagging a drawdown this crew is not committing",
		_yields_text(dip_sheet).contains(SourceForecast.YIELD_RENEWABLE_NOTE.to_upper())
			and not _yields_text(dip_sheet).contains(
				HudComposeVocab.LOCAL_HUNT_OVERDRAW_NOTE.to_upper()))
	# (5) THE CREW ROW SAYS IT. Every number above follows from a half carry, and the sheet has to say
	# so somewhere — the plant web's rule, on the animal sheet's own dip.
	_assert_hud("a live build states its half carry on the crew row",
		_crew_row_dip_note(dip_sheet).contains(
			str(HudFormat.progress_percent(HERD_DIP_BUILD_FRACTION))))
	# (6) **THE CREW TARGETS AND THE WORKER CAP WERE ALREADY RIGHT — MEASURED, NOT ASSUMED.** They read
	# `forecast_inputs` through the chart model, which the builder has always handed the composed verb,
	# so they divide by the DIPPED carry. Pinned both ways: the rendered target is the dipped answer AND
	# it differs from the undipped one, without which the claim would pass on a sheet that ignored the
	# dip entirely.
	var dip_carry := SourceForecast.per_worker_biomass(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX) * dip_fraction
	var dip_samples := SourceForecast.regrowth_samples(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	var dip_hold := SourceForecast.crew_to_hold(dip_samples, HERD_DIP_FLOOR, dip_carry,
		HERD_DIP_BODY_MASS)
	var bare_hold := SourceForecast.crew_to_hold(dip_samples, HERD_DIP_FLOOR,
		dip_carry / dip_fraction, HERD_DIP_BODY_MASS)
	_assert_hud("the *hold it after* target divides by the DIPPED carry (%d, against %d undipped)"
		% [dip_hold, bare_hold],
		_crew_target_count(dip_sheet, HudWidgets.CREW_TARGET_HOLD) == dip_hold
			and dip_hold != bare_hold)

	# State herd_build_dip_none — THE SAME HERD WITH NO BUILD IN FLIGHT, and the half that proves the
	# first is not simply a sheet scaled down. Nothing about the animal moves: the crew lands a whole
	# body again, wastes nothing, and the ⚠ comes back — because four hunters really do out-carry this
	# herd's regrowth when they are hunting it rather than gentling it.
	_hud._compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
	_hud._compose.set_hunt_count(HERD_DIP_CREW)
	_hud._drawercompose.open_herd_compose(dip_herd)
	await _settle()
	await _save("herd_build_dip_none")
	var bare_sheet := _hud._drawercompose._compose_sheet
	_assert_hud("no build in flight, no dip claimed on the crew row",
		_crew_row_dip_note(bare_sheet) == "")
	_assert_hud("…the same crew lands the whole body again (%s/turn)" % bare_face,
		_yields_text(bare_sheet).contains(bare_face)
			and not _yields_text(bare_sheet).contains(built_face))
	_assert_hud("…wasting nothing, so the waste note is a claim about the BUILD and not about the herd",
		float(bare_take["wasted"]) == 0.0
			and not _yields_text(bare_sheet).contains(HUNT_WASTE_NEEDLE.to_upper()))
	_assert_hud("…and the ⚠ returns: hunting, this crew really does draw the herd down",
		_yields_text(bare_sheet).contains(HudComposeVocab.LOCAL_HUNT_OVERDRAW_NOTE.to_upper()))
	_hud._band_labor._player_band = prior_dip_band
	_hud._band_labor._player_bands = prior_dip_bands
	_hud._compose.reset_hunt_source()   # the states after this one open on their own herd

	# ---- THE TWO ZERO-CREW SUBMITS, HUNT SIDE ----------------------------------------------------
	# The forage pair above (`forage_unstaffed` / `forage_unassign`) is one half of a rule that belongs
	# to BOTH sheets: `workers == 0` means two different things depending on whether this band already
	# works the source, and the sim skips validation entirely at 0 — so the unassign is always legal.
	# The hunt sheet had ONE state for both, a live button that sent a command changing nothing, and no
	# rename on the source it does work. These two frames are judged as a pair, exactly as the forage
	# ones are.
	#
	# State hunt-unstaffed (A) — 0 hunters on a herd this band does NOT hunt. Pressing would send a
	# no-op, so the button is DEAD and still wears the verb.
	_hud._compose.reset_hunt_source()
	_show_herd(_investment_pair_boar_herd())
	_compose_herd(_investment_pair_boar_herd(), ZERO_CREW)
	await _settle()
	await _save("herd_hunt_unstaffed")
	var hunt_noop_btn := _find_button_by_text(_hud._drawercompose._compose_sheet,
		HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON)
	_assert_hud("0 hunters on a herd this band does not hunt disables the submit (it would be a no-op)",
		hunt_noop_btn != null and hunt_noop_btn.disabled)

	# State hunt-unassign (B) — the SAME 0 on a herd this band DOES hunt (4 standing hunters on
	# `game_deer_07`): that is the sim's unassign, not a no-op. The button stays live and is RENAMED,
	# and the improvement control is GONE — what abandoning costs is already on the card in the rung's
	# own hint, so offering to START a build in the act of abandoning the source says two opposite
	# things at once. The positive-crew open below it is what makes that absence a CHANGE and not a
	# sheet that simply never offers this herd a rung.
	_hud._compose.reset_hunt_source()
	_show_herd(_taming_herd_fixture())
	_compose_herd(_taming_herd_fixture())
	await _settle()
	_assert_hud("precondition: at its standing crew the same herd IS offered its next rung",
		_find_improvement_control(_hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME) != null)
	_compose_herd(_taming_herd_fixture(), ZERO_CREW)
	await _settle()
	await _save("herd_hunt_unassign")
	var hunt_unassign_btn := _find_button_by_text(_hud._drawercompose._compose_sheet,
		HudComposeVocab.UNASSIGN_BUTTON)
	_assert_hud("0 hunters on a herd this band hunts stays live, renamed Unassign",
		hunt_unassign_btn != null and not hunt_unassign_btn.disabled)
	_assert_hud("…and offers no improvement to start in the act of abandoning the source",
		_find_improvement_control(_hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME) == null
		and _find_improvement_control(_hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL) == null)

	# Back to a plain Sustain compose for the band-picker / distance states below.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud._compose.set_hunt_count(1)
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.set_hunt_improvement("")
	_hud._compose.reset_hunt_source()

	# State 3f — TWO player bands: the "Assign hunters" controls' "Band:" dropdown lists both
	# (positional "Band 1" / "Band 2"). Default selection is the resolved band (Band 1, 12 idle).
	# The Hunters count is dialed to 8 and CLAMPS to 7 with `+` disabled, because the binding cap here
	# is USEFULNESS ("max 7 workers useful here"), not the band's 12 idle — the frame shows the stepper
	# answering to the sheet's own ceiling while the picker's default selection resolves.
	_hud._band_labor._player_bands = _two_player_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()   # force a fresh seed so the default selection = resolved band
	_show_herd(_herd_fixture())
	_compose_herd(_herd_fixture(), 8)
	await _settle()
	await _save("herd_band_picker")

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the picker path
	# re-caps the Hunters count to the newly-selected band's assignable workers (8 → 2, + now
	# disabled), demonstrating selection → actor band → stepper re-cap.
	var second_band: Dictionary = _two_player_bands()[1]
	_hud._compose.set_hunt_band(int(second_band["entity"]))
	_hud._compose.set_hunt_count(clampi(
		_hud._compose.hunt_count(), 0, _hud._band_labor.assignable_hunt_workers(second_band, _herd_fixture()["id"])))
	_compose_herd(_herd_fixture())
	await _settle()
	await _save("herd_band_picker_b")
	# Reset so later states render their usual single-band dropdown.
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()

	# State 3h — distance-aware herd-hunt, SINGLE far band: a lone band ~27 tiles from the herd (beyond
	# its hunt_reach 7). The affordance fully replaces the local option — the button reads "Send
	# Expedition", a distance hint shows, the stepper reads "Party", and Assign emits
	# send_hunt_expedition (party = the stepper), NOT assign_labor.
	_hud._band_labor._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_expedition")
	_assert_compose_sheet_fits("herd_hunt_expedition")
	# The EXPEDITION branch reads in the same grammar as the two local sheets as far as it goes — it
	# builds no improvement control (a detached party builds nothing), so only the shared HEAD is
	# claimed here, which is exactly what `_record_compose_spine` asserts.
	_record_compose_spine(COMPOSE_SPINE_KEY_EXPEDITION)
	# **THE TRIP READOUT — the expedition's answer in the SAME box the local sheet uses.** The branch
	# used to render one wrapped sentence carrying five facts ("delivers ≈3 Red Deer over ≈9 turns ·
	# ~6 food · ⇄ ~2 trade goods"), beside a local sheet that laid the same kinds of fact out in a
	# bounded well — two sheets on one panel reading nothing alike. What must NOT carry over is the
	# per-turn framing, and the header is where that shows: a trip has no steady state, so
	# `THIS TRIP` and not `PER TURN`, and no `now → after` arrow to key.
	_assert_hud("the expedition sheet's readout is headed for a TRIP, not for a rate",
		_yields_header(_hud._drawercompose._compose_sheet)
			== SourceForecast.EXPEDITION_TRIP_ROW_HEADER.to_upper())
	_assert_hud("…so it states no PER TURN header and no now → after arrow",
		not _yields_header(_hud._drawercompose._compose_sheet).contains("PER TURN")
			and not _yields_header(_hud._drawercompose._compose_sheet).contains("→"))
	# THE PAYLOAD, ALL THREE TERMS. The animal count leads in the local hunt row's own idiom (the `≈`
	# face, the quarry as the unit, no account), then the accounts those bodies pay. Every term is
	# named, because matching one survives losing either of the others — and this quarry pays BOTH
	# accounts, which is the positive half of the render-only-where-the-vector-pays pair asserted on
	# the zero-trade mammoth below.
	var trip_yields := _yields_text(_hud._drawercompose._compose_sheet)
	_assert_hud("the ANIMAL count leads the row, in the quarry's own name",
		trip_yields.contains("≈%d" % DISTANCE_RAID_ANIMALS[0]) and trip_yields.contains("RED DEER"))
	_assert_hud("…with the trip's FOOD beside it",
		trip_yields.contains(SourceForecast.format_magnitude(DISTANCE_RAID_ANIMALS[0] * 2.0))
			and trip_yields.contains("FOOD"))
	_assert_hud("…and its TRADE, since this quarry pays both",
		trip_yields.contains(SourceForecast.format_magnitude(
			DISTANCE_RAID_ANIMALS[0] * RAID_TRADE_PER_ANIMAL)) and trip_yields.contains("TRADE"))
	_assert_hud("a raid that hauls its whole kill states NO waste note",
		not trip_yields.contains("wasted".to_upper()))
	# THE VERDICT states the trip's length. This band carries no move rate, so travel is 0 and there is
	# no split to spell out — the pair that DOES is `herd_hunt_raid_travel` below.
	_assert_hud("the verdict states how long the party is away",
		_verdict_text(_hud._drawercompose._compose_sheet).contains(str(DISTANCE_RAID_TURNS[0])))
	_assert_hud("…and a brisk raid reads OK",
		_verdict_severity(_hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)
	# **THE BOX IS NOT A CHART, AND A PARTY IS NOT A CREW.** Without this pair, "made the expedition
	# look like the local sheet" could quietly come to mean "gave it a chart and crew targets" — both
	# of which are deliberately absent, a raid being a forward-simulated trip rather than a per-turn
	# drawdown by a resident crew, with no floor curve to walk and no holding crew to price.
	_assert_hud("…and the branch is still an EXPEDITION sheet — no chart, no crew targets",
		_find_meta_node(_hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) == null
			and _find_crew_target(_hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_CLEAR) == null
			and _find_crew_target(_hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_HOLD) == null)
	# The peak zone's half of change A, on the surface that has the least redundancy: this sheet sits
	# at `FLOOR_FOOD_PEAK`, the zone now says nothing, and the aside renders NOT AT ALL rather than a
	# dashed rule over empty space. Paired with the strip-zone assertion on `forage_three_accounts`,
	# which is what keeps "empty the whole table" from passing.
	_assert_hud("the peak zone contributes no aside to the trip readout either",
		_readout_aside_text(_hud._drawercompose._compose_sheet) == "")

	# State 3i — TWO bands at DIFFERENT distances from ONE herd, NEAR band selected: band 811 sits ON
	# the herd (distance 0 ≤ reach 7) → "Hunt Here" + assign_labor. The band-picker selection —
	# not the herd — drives it (the resolved/default band is the near one here).
	_hud._band_labor._player_bands = _hunt_distance_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_near")

	# State 3j — same two bands, FAR band selected via the picker (entity 812, ~27 tiles away): the SAME
	# herd now offers "Send Expedition" (party cap = min(idle 6, max party 8) = 6), proving that
	# WHICH band is selected flips the label + command + band-entity target, not the herd.
	_hud._compose.set_hunt_band(int(_hunt_distance_bands()[1]["entity"]))   # FAR band
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_far")
	# Reset so later states render their usual single-band dropdown + default band.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)

	# States 3k–3o — the HERD-PANEL hunt forecast, EXPEDITION branch. This is the second entry point
	# into a hunting expedition (herd-first): the herd is beyond the band's hunt_reach, so the panel
	# composes party + policy and sends immediately — no targeting step, so the banner's forecast never
	# appears. The forecast therefore renders LIVE above the button (the block re-renders on every
	# stepper tick / policy click) from the SAME helpers the banner uses: a PURE LOOKUP into the herd's
	# `hunt_trip_estimates` cell for (policy, party size). The client does no arithmetic here — the sim
	# forward-simulated each trip and exported the turns. Party 4:
	#   3k viable      — Sustain on a Thunder Mammoth: the sim's cell says 6 turns → cyan line, normal
	#                    primary "Send Expedition" button.
	#   3l not viable  — Sustain on Red Deer: 54 turns > warn 20 → amber line + the button itself goes
	#                    "armed" and names the cost: "Send Anyway (≈54 turns)".
	#   3m surplus     — the SAME Red Deer on Surplus: a Surplus party strips the herd's stock headroom
	#                    rather than living off its renewable flow, so the sim's cell says ~6 turns —
	#                    VIABLE. (The old bug re-derived the trip from the band's flow ceiling and scared
	#                    the player off a perfectly good trip; only the sim's own row knows.)
	#   3n never fills — a collapsing Wild Fowl flock: every cell is `turns_to_fill = 0` → red line +
	#                    the DISABLED "Herd too lean to raid" button, exactly as 3r below (the HERD has
	#                    nothing left to give, and no party size can fix a herd with no surplus).
	#   3o eradicate   — a healthy Red Deer on Eradicate: it DELIVERS like every other rung (#337 pays each
	#                    rung the species' yield vector), and its cell ran the whole horizon still
	#                    delivering → amber LONG-RAID line + "Send Anyway (long raid)". NOT a denial:
	#                    denial is now a property of the QUARRY (pays neither product), not of the rung.
	# WARNED, not BLOCKED — and never a confirm dialog: a slow raid and a long one are real tradeoffs, so
	# they read as a price tag and stay ENABLED. The ONE blocked case is 3n's, a herd with no surplus
	# left: it would return empty at every party size, so there is no price to pay.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	for state: Dictionary in _hunt_assign_forecast_states():
		var far_herd: Dictionary = state["herd"]
		_hud._compose.reset_hunt_source()    # force a fresh seed (band = resolved, policy = the herd's current)
		_hud._compose.set_hunt_band(-1)
		_show_herd(far_herd)
		# The policy-picker click, without the click.
		_compose_herd(far_herd, HUNT_FORECAST_PARTY, float(state["floor"]))
		await _settle()
		await _save(String(state["name"]))
		_assert_trip_readout(String(state["name"]))

	# AUTO-MAX on a policy click (expedition branch): picking a policy fills the Party to that policy's
	# max-useful cap. The mammoth's Sustain payload keeps rising to the fieldable ceiling, so a Sustain
	# click sets the party to 6 (min(plateau, idle 6)) — the "give me everything, zero idle hunters"
	# default. The compose hunt autofill is the one-shot a policy CLICK arms; the rebuild consumes it.
	var automax_herd := _partial_waste_mammoth()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(automax_herd)
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.arm_hunt_autofill()
	_compose_herd(automax_herd)
	await _settle()
	await _save("herd_hunt_expedition_automax")

	# States 3p–3s — the RAID readout (delivered payload + waste) + the party stepper capped at max-useful.
	# A hunting expedition is a greedy raid: it grabs the herd's standing surplus in a burst and comes home,
	# so the headline is the delivered PAYLOAD, and `deliveredFood` PLATEAUS with party size once the surplus
	# (not the pack) binds — that plateau IS max-useful. The clean Wild Boar carries the server's measured
	# raid (hauls its whole kill, no waste). The picker buttons read each policy's MAX food/turn, ascending.
	#   3p boar raid   — a 1-hunter raid: "delivers ≈5 Wild Boar over ≈7 turns · ~20 food" (no waste), cyan +
	#                    primary "Send Expedition"; picker "up to +10.67 / +13.33 / +14.67 /turn".
	#   3q max useful  — 2 hunters: "delivers ≈8 Wild Boar over ≈8 turns · ~32 food"; a 3rd delivers NO more
	#                    food (the surplus binds), so the stepper caps at 2 and the `+` note reads
	#                    "max 2 workers useful here — more would be idle". The silent-idle-hunter gap, closed.
	#   3r no surplus  — a herd stripped to its floor: deliveredFood = 0 at EVERY size → the raid returns
	#                    empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button (party
	#                    size can't fix it — surplus is a property of the herd, not the party).
	#   3s eradicate   — the boar on Eradicate: the whole-stock windfall comes home (#337), so the raid line
	#                    quotes its payload in BOTH products and the Send button is the ordinary one. What
	#                    Eradicate costs is the herd itself, permanently — never the payload.
	var boar := _raid_boar_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(boar)
	_compose_herd(boar)   # source_changed seeds party = 1
	await _settle()
	await _save("herd_hunt_boar_raid")

	_hud._compose.set_hunt_count(2)               # key unchanged → no re-seed; caps at the plateau (2)
	_compose_herd(boar)
	await _settle()
	await _save("herd_hunt_max_useful")

	# State 3q-travel — the SAME boar raid, staffed by a band the herd is 8 tiles away from (beyond
	# hunt_reach 7 → expedition) and carrying a move rate. `turnsToFill` is HUNTING turns only, so the
	# client adds the round-trip TRAVEL the band-agnostic estimate table can't (ceil(2 × 8 / 2) = 8): at
	# party 2 the readout reads "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food",
	# and the stepper still caps at the animalsTaken plateau (2). `band_move_tiles_per_turn` now ships on the
	# wire (schema slot 124) and is decoded onto the band; this fixture carries it exactly as the decoder does.
	_hud._band_labor._player_bands = [_raid_travel_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(boar)
	_compose_herd(boar, 2)
	await _settle()
	await _save("herd_hunt_raid_travel")
	# **THE SPLIT — the half of the trip verdict `herd_hunt_expedition` structurally cannot show.**
	# That band carries no move rate, so its trip is all hunting and the verdict states one number;
	# this one walks 8 tiles each way at 2 tiles a turn, so the total is 8 hunting + 8 travel and the
	# verdict has to spell out where those turns go. Asserted as a PAIR with the total, because a
	# verdict quoting the split alone would leave the player adding it up themselves.
	var travel_verdict := _verdict_text(_hud._drawercompose._compose_sheet)
	_assert_hud("a raid with travel states the TOTAL and the split it is made of",
		travel_verdict.contains(str(RAID_TRAVEL_HUNT_TURNS + RAID_TRAVEL_TURNS))
			and travel_verdict.contains("%d hunting, %d travel" % [
				RAID_TRAVEL_HUNT_TURNS, RAID_TRAVEL_TURNS]))
	_assert_hud("…and a trip inside the band's warn line still reads OK",
		_verdict_severity(_hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)
	# Restore the far band (no move rate) for the remaining raid states.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]

	var lean := _no_surplus_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(lean)
	_compose_herd(lean, HUNT_FORECAST_PARTY)
	await _settle()
	await _save("herd_hunt_no_surplus")

	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(boar)
	_compose_herd(boar, 2, SourceForecast.FLOOR_MIN)
	await _settle()
	await _save("herd_hunt_eradicate")
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# States 3t–3v — the LABOR-BOUND note. When the herd's max-useful party exceeds the hunters you can
	# field, the `+` caps at LABOR (not usefulness), and the note names the reason AND the ceiling you're
	# working toward — "N of M useful — free up idle workers to send more". The Steppe Bison's plateau
	# DIFFERS BY POLICY (Sustain 4, Deplete 7), which is how the "of M" is shown to track the policy.
	var bison := _labor_bound_raid_herd()
	var bound_band: Dictionary = _hunt_preview_far_band().duplicate(true)
	bound_band["idle_workers"] = 3           # below Sustain's plateau of 4 AND Deplete's of 7 → labor-bound
	_hud._band_labor._player_bands = [bound_band]
	_hud._band_labor._player_band = bound_band
	#   3t Sustain — idle 3 < plateau 4 → "3 of 4 useful — free up idle workers to send more", + dead at 3.
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(bison)
	_compose_herd(bison, LABOR_BOUND_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_labor_bound")
	_assert_hud("the labor-bound frame renders the 3-hunter crew idle labor caps it at",
		_stepper_value(_hud._drawercompose._compose_sheet) == LABOR_BOUND_CREW)
	#   3u Deplete — SAME herd + band, policy flipped: the plateau rises to 7 → "3 of 7 useful", proving the
	#              ceiling tracks the selected policy. Key unchanged so the policy override sticks.
	_hud._compose.set_hunt_floor(DEEP_DRAW_FLOOR)
	_compose_herd(bison)
	await _settle()
	await _save("herd_hunt_labor_bound_deplete")
	#   3v Party-size-bound — the SUB-CASE where freeing idle workers would NOT help: idle 6 >= max party 2,
	#              so the party-SIZE cap binds, not idle. The note reads "2 of 4 useful — at the max party
	#              size" instead of the free-up-workers advice.
	var party_capped: Dictionary = _hunt_preview_far_band().duplicate(true)
	party_capped["idle_workers"] = 6
	party_capped["max_expedition_party_size"] = 2
	_hud._band_labor._player_bands = [party_capped]
	_hud._band_labor._player_band = party_capped
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(bison)
	_compose_herd(bison, PARTY_SIZE_BOUND_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_party_size_bound")
	_assert_hud("the party-size-bound frame renders the 2-hunter crew the max party size caps it at",
		_stepper_value(_hud._drawercompose._compose_sheet) == PARTY_SIZE_BOUND_CREW)
	# Restore the far band + sustain for the states that follow.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# States 3n–3o — the same panel's LOCAL branch (herd within hunt_reach). The preview line reads the
	# crew's HONEST carry-aware delivered take in ANIMALS (delivered ÷ food_per_animal), not the
	# unquantized food rate. Red Deer fpa 2.0, band per-worker 0.8, output 0.9; Sustain ceiling 0.30,
	# Deplete 0.60. `LOCAL_HUNT_HUNTERS` is dialed in, but the stepper clamps the crew to 3 carriers
	# (`LOCAL_HUNT_CAPPED_CREW`) — and the clamp is immaterial to what these two frames show: a 3-hunter
	# crew collects 2.16 food/turn and a 6-hunter one 4.32, both far above the ceilings below, so the
	# HERD's flow ceiling is what binds and the quantized take is the same either way:
	#   3n Sustain — delivered = min(0.30×0.9, …) = 0.27 → ≈0.14 Red Deer/turn · renewable (green).
	#   3o Deplete  — delivered 0.54 > Sustain 0.27 → WARN-amber "⚠ ≈0.27 Red Deer/turn — overdraws the
	#                herd" (the same ⚠ the allocation rows use). No waste (a whole deer is carryable).
	# (The herd's `hunt_trip_estimates` ride along but are IGNORED here — a trip table answers an
	# EXPEDITION's question; a local hunt is carry arithmetic over the band's flow ceilings. Band = flow
	# arithmetic; expedition = lookup.)
	var local_herd := _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
		DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS)
	_hud._band_labor._player_bands = [_hunt_preview_local_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(local_herd)
	_compose_herd(local_herd, LOCAL_HUNT_HUNTERS)
	await _settle()
	await _save("herd_hunt_local_sustain")
	_assert_compose_sheet_fits("herd_hunt_local_sustain")
	# THE HUNT HALF, and the parity check itself: both sheets must ask WHICH STANCE before HOW MANY
	# PEOPLE, and in the same order throughout. The hunt sheet staffed first until the consistency pass;
	# a frame cannot hold that claim, which is why it is asserted rather than eyeballed.
	_record_compose_spine(COMPOSE_SPINE_KEY_HUNT)
	_assert_compose_order_parity(COMPOSE_SPINE_KEY_FORAGE, COMPOSE_SPINE_KEY_HUNT)
	_assert_hud("the local-hunt frames render the dialed-in crew (capped), not the re-seeded 1",
		_stepper_value(_hud._drawercompose._compose_sheet) == LOCAL_HUNT_CAPPED_CREW)

	# Flip the policy picker to Deplete — the same click path the player takes; the preview line
	# re-computes live off the new ceiling.
	_hud._compose.set_hunt_floor(DEEP_DRAW_FLOOR)
	_compose_herd(local_herd)
	await _settle()
	await _save("herd_hunt_local_overdraw")

	# The SAME local picker flipped to Eradicate — the frame the rung's HINT is judged on (issue #337).
	# Its text must describe the whole-stock windfall + the permanent end state, and must NOT claim the
	# rung yields nothing: the sim pays every rung its species' yield vector, Eradicate included.
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_MIN)
	_compose_herd(local_herd)
	await _settle()
	await _save("herd_hunt_local_eradicate")

	# States 3p–3q — the WHOLE-ANIMAL carry cap. A big-game aurochs drops as one 80-biomass body via the
	# kill-credit bank; food_per_animal 1.6 outweighs one hunter's carry (per_worker 0.80), so the cap is
	# the CARRIERS needed to haul the peak-turn drop, not ceil(smoothed-rate / per_worker). Sustain
	# (ceiling 0.74) used to read "max 1 useful" (the bug: ceil(0.74/0.80)=1) — it must now read "max 2".
	_hud._band_labor._player_bands = [_hunt_preview_local_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	var aurochs := _aurochs_big_game_fixture()
	_show_herd(aurochs)
	_compose_herd(aurochs, 1, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_whole_animal_cap")

	# Flip to Deplete — two bodies drop on the peak turn, so the cap climbs to 4: it tracks the selected
	# policy's ceiling, exactly as the smoothed-rate cap did.
	_hud._compose.set_hunt_floor(DEEP_DRAW_FLOOR)
	_compose_herd(aurochs)
	await _settle()
	await _save("herd_hunt_whole_animal_cap_deplete")

	# States 3s–3v — the CARRY-AWARE ANIMALS-FIRST local-hunt preview (spec oracle: deer fpa 1.23, band
	# per-worker 0.8, output 1.0, Sustain ceiling 2.33). The preview line reads the crew's HONEST
	# delivered take in animals, not the unquantized food rate the crew could never carry; the policy
	# buttons read "up to X/turn" (the herd's cap, worker-independent).
	_hud._band_labor._player_bands = [_delivered_oracle_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]

	# 3s — 2 hunters land exactly one whole 1.23 deer/turn, no waste → "≈1 Red Deer/turn · renewable",
	# and the four ascending "up to +2.33 / +3.50 / +5.00 / +7.00 /turn" cap buttons.
	var oracle_clean := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(oracle_clean)
	_compose_herd(oracle_clean, 2, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_delivered_clean")

	# 3t — 1 hunter can't carry even one whole deer (0.80 < 1.23), so 35% of the kill rots →
	# "≈0.65 Red Deer/turn · ⚠ 35% wasted" (green line, amber waste suffix).
	var oracle_waste := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(oracle_waste)
	_compose_herd(oracle_waste, 1, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_delivered_waste")

	# 3u — AUTO-MAX on policy select: simulate the picker click path (autofill flag + policy set) starting
	# from a count of 1; the rebuild fills the crew to the Sustain max-useful cap (4 carriers), so the
	# stepper sits at 4 and the line reads the full ≈1.89 deer/turn with zero waste.
	var oracle_automax := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(oracle_automax)
	_compose_herd(oracle_automax, 1, SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.arm_hunt_autofill()
	_compose_herd(oracle_automax)
	await _settle()
	await _save("herd_hunt_automax")

	# 3v — big game (mammoth fpa 16, Sustain ceiling 2.4): auto-max staffs the 20 carriers, delivered
	# 2.4 → ≈0.15 mammoth/turn, and the averaging-WINDOW hint appears: "≈1 Woolly Mammoth every ~7
	# turns — the rate above is averaged over that span."
	var window_herd := _big_game_window_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(window_herd)
	_compose_herd(window_herd, 1, SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.arm_hunt_autofill()
	_compose_herd(window_herd)
	await _settle()
	await _save("herd_hunt_big_game_window")
	_assert_compose_sheet_fits("herd_hunt_big_game_window")

	# 3w — THE INEDIBLE QUARRY (issue #337). A wolf pays PELTS AND NO MEAT: `provisions == 0` on every
	# rung, a real trade ceiling on all four. This is the frame the whole arc is judged on. The picker's
	# four buttons must read FOUR ASCENDING TRADE numbers on their second line — `0.90 / 1.35 / 1.95 /
	# 2.70 trade` — with NO food term and NO zeros anywhere; before the fix the client read only food, so all four read `+0.00`
	# and the pack rendered as a source worth nothing. The preview line below the picker must still show a
	# per-turn ANIMAL rate (the ratio is unit-free — it divides by the TRADE quantum, since the food one
	# is honestly 0), and the averaging-window disclaimer must still appear.
	var wolf := _pelt_only_wolf_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(wolf)
	# Crew + rung go through `_compose_herd`, which dials them in AFTER the source-change re-seed.
	_compose_herd(wolf, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_pelts_only")
	_assert_compose_sheet_fits("herd_hunt_pelts_only")
	# **THE CHART ON AN INEDIBLE QUARRY** (the wolf half of the five chart cases). The readout above it
	# carries no food line at all, and the chart must not care: a floor is a fraction of BIOMASS, and
	# the crew targets divide by `perWorkerBiomass`, which is positive on a wolf where both the food
	# rate and `perWorkerYield` are honestly `0`. That is precisely why the field exists — the old
	# `perWorkerYield / provisionsPerBiomass` recovery is `0/0` on this animal.
	_assert_hud("a wolf's chart draws — a floor is biomass, and biomass is what this species has",
		_find_meta_node(_hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) != null)
	_assert_hud("…and its crew targets are priced off the biomass throughput, not the absent food one",
		_crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR)
			> CREW_TARGET_ABSENT)
	# **A CLICKABLE TARGET THE STEPPER BESIDE IT CANNOT REACH IS THE PANEL ARGUING WITH ITSELF** (§7.2),
	# and the wolf is where that was found: `5 hold it after` sat under `max 4 workers useful here`,
	# because the cap answered "hands that clear what stands THIS turn" and the target answered "hands
	# that take the regrowth EVERY turn" — and the cap was the one that was wrong (a source AT its floor
	# has no room, so it capped at 0 while a positive crew was needed next turn). `max_useful_workers`
	# now floors on the hold crew, so the press below lands the stepper on exactly the number the button
	# offered. Driven through the REAL button, since the clamp that used to swallow it lives in the press
	# handler rather than in the arithmetic.
	var wolf_hold := _crew_target_count(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
	_assert_hud("the wolf states a hold-it-after crew to click at all", wolf_hold > 0)
	_find_crew_target(_hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD).pressed.emit()
	_assert_hud("…and the stepper reaches that crew instead of clamping it to a smaller cap",
		_hud._compose.hunt_count() == wolf_hold)

	# State floor_chart_herd_allee — **THE HERD BELOW ITS ALLEE POINT, and the frame the whole sampled
	# curve exists for.** Under `collapse_fraction` a herd's regrowth samples are NEGATIVE: it declines
	# every turn whether or not anyone hunts it. The projection must therefore fall AWAY from the floor
	# toward extinction. Clamping those samples to zero is the instinctive thing to do with a chart and
	# it would draw this herd sitting still — the exact asymmetry that makes floor 0 end a herd and
	# only set a patch back (compare `floor_chart_drawn_down`, whose curve flattens onto its floor).
	var allee_herd := _floorify(_collapsing_herd_fixture())
	allee_herd["biomass"] = FLOOR_CHART_ALLEE_STOCK_FRACTION * float(allee_herd["carrying_capacity"])
	# The band is the wolf state's, deliberately — this frame is about the HERD's curve, and swapping
	# the actor would put a second variable in a comparison the reader is meant to make against it.
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(allee_herd)
	_compose_herd(allee_herd, FLOOR_CHART_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("floor_chart_herd_allee")
	# The PNG shows the decline; this is the half it cannot testify to — that the samples themselves
	# are negative down there, which is what the projection reads and what a clamp would erase.
	_assert_hud("the herd's curve is NEGATIVE below its Allee point — decline, not stillness",
		SourceForecast.regrowth_at(SourceForecast.regrowth_samples(allee_herd,
			HudComposeVocab.BARE_FORECAST_PREFIX), FLOOR_CHART_ALLEE_STOCK_FRACTION) < 0.0)
	_assert_hud("…while the plant curve never is, at the same fraction of its own capacity",
		SourceForecast.regrowth_at(SourceForecast.regrowth_samples(drawn_patch,
			HudComposeVocab.FORAGE_FORECAST_PREFIX), FLOOR_CHART_ALLEE_STOCK_FRACTION) >= 0.0)

	# **A VERDICT MAY NOT PROMISE AN AFTERMATH THE SOURCE HAS NO WAY TO REACH.** Reported from play: a
	# Rabbit Warren at `Take everything` read `0 hold it after` beside "Reaches the floor in 2 turns,
	# then holds it — taking only what grows back". The herd is GONE at floor 0; there is nothing to
	# hold and nothing that grows back, and the panel was contradicting its own crew target.
	#
	# **The discriminator is the REGROWTH at that floor, not the web and not floor 0**, and this pair
	# is what pins that: the same floor on a PATCH keeps the full sentence, because a stripped patch
	# reseeds from bare ground and genuinely does hold at 0 paying what grows back. A fix that branched
	# on "fauna" or on "floor == 0" would pass the herd line below and fail the patch line under it.
	var strip_crew := 64
	var stripped_herd := SourceForecast.floor_chart_model(allee_herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_MIN, strip_crew,
		SourceForecast.IMPROVEMENT_NONE, "hunters", LESSON_NOT_YET_LEARNED)
	var stripped_patch := SourceForecast.floor_chart_model(drawn_patch,
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_MIN, strip_crew, SourceForecast.IMPROVEMENT_NONE, "foragers",
		LESSON_NOT_YET_LEARNED)
	var stripped_herd_text := String((stripped_herd.get("verdict", {}) as Dictionary).get("text", ""))
	var stripped_patch_text := String((stripped_patch.get("verdict", {}) as Dictionary).get("text", ""))
	_assert_hud("both stripped sources REACH their floor, so both are stating the reaching verdict",
		stripped_herd_text.contains("Reaches the floor")
			and stripped_patch_text.contains("Reaches the floor"))
	_assert_hud("a herd taken to nothing is not promised that it holds what grows back",
		not stripped_herd_text.contains("grows back"))
	_assert_hud("…while a patch at the same floor still is — it reseeds, so the clause is TRUE there",
		stripped_patch_text.contains("grows back"))
	# **THE LINE THAT RULES OUT THE PLAUSIBLE WRONG FIX.** Branching on `kind != SOURCE_KIND_HERD`
	# passes both assertions above — the two fixtures there make "is a herd" and "cannot regrow"
	# coincide, so the sabotage changed no output and the pair testified to nothing. A HEALTHY herd
	# above its floor regrows at that floor like anything else and must KEEP the clause; that is the
	# case a web branch gets wrong, and the only one of the three that can see the difference.
	var held_herd := SourceForecast.floor_chart_model(
		_floorify(_grazing_healthy_herd_fixture()), SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, strip_crew,
		SourceForecast.IMPROVEMENT_NONE, "hunters", LESSON_NOT_YET_LEARNED)
	var held_herd_text := String((held_herd.get("verdict", {}) as Dictionary).get("text", ""))
	_assert_hud("a HERD that still regrows at its floor keeps the clause — it is the growth, not the web",
		held_herd_text.contains("Reaches the floor") and held_herd_text.contains("grows back"))

	# **THE FLOOR FLAG'S UNIT AND ITS ORDER**, which no PNG can testify to at 10px. Asserted against
	# hand-built models rather than the live sheet so both webs are reachable from one place and the
	# expected strings are computable by eye: 1075 ÷ 100 = 10.75 → 11 animals at floor 0.50.
	#
	# The ORDER is the assertion that matters, and it is now the SAME on both. An animal count over a K
	# of ~21 has ~21 states where biomass had one per FLOOR_STEP, so an animal-FIRST flag would sit
	# unmoved across a tenth of the drag and read as a stuck control; the percent leads to keep the flag
	# responsive, and once it must lead on fauna the patch follows it so one control cannot swap its
	# terms mid-session. `==` (not `contains`) is what pins the order — a `contains` passes on either.
	var flag_probe := HarvestFloorChart.new()
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK,
		"capacity": 2150.0, "body_mass": 100.0, "quarry": "Red Deer"})
	_assert_hud("a HERD's floor flag counts animals, after the percent",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer")
	# THE OTHER WEB: a patch has no body, so its quantity stays biomass — no `≈`, no species — while
	# the ORDER around it is identical. Without this the suite could not tell "fauna converted" from
	# "everything converted", and could not see the patch's percent silently moving back to the tail.
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK, "capacity": 195.0})
	_assert_hud("…and a PATCH's states biomass, in the same order and with no animal count",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 97.5) == "leave 50% · 98")
	flag_probe.free()
	# The conversion itself, on literals. `animal_count` is the ONE place biomass becomes a head count
	# (the drawer row and the flag both read it), so its two edges are worth stating outright: a
	# species with no `body_mass` on the wire yields no count at all, and a herd holding a FIFTH of a
	# body counts ONE, never the rounded zero — it is alive on the map and the sim's kill step floors
	# at one body too.
	_assert_hud("body mass turns biomass into animals",
		SourceForecast.animal_count(820.0, 100.0) == 8)
	_assert_hud("…a herd under one body still counts one, never zero",
		SourceForecast.animal_count(19.0, 100.0) == 1)
	_assert_hud("…and a species with no body mass has no count to state",
		SourceForecast.animal_count(820.0, 0.0) == SourceForecast.ANIMAL_COUNT_NONE)
	# **THE FLAG AND THE VERDICT NAME ONE THRESHOLD, so they must name it in one unit.** Caught in a
	# frame, not in review: this sheet read `leave 50% · ≈11 Red Deer` over "grows past 1075". Both now
	# render the quantity through `stock_face`, and this is the assertion that says so — the verdict's
	# sentence must CONTAIN what the flag flies, on the same model.
	var at_floor := SourceForecast.harvest_verdict({"reached_turn": SourceForecast.PROJECTION_REACHED_NONE,
		"settled_fraction": 0.0, "series": []}, FLOOR_CHART_CREW, 96.0, 2150.0,
		SourceForecast.FLOOR_FOOD_PEAK, 0, "hunters", 100.0, "Red Deer")
	_assert_hud("the at-floor verdict quotes the threshold in the SAME unit the flag flies",
		String(at_floor.get("text", "")).contains("≈11 Red Deer")
			and not String(at_floor.get("text", "")).contains("1075"))

	# 3x — the same wolf as an EXPEDITION target (band 27 tiles off). `delivers_food = false` on every
	# cell now means THE QUARRY IS INEDIBLE, not "a denial mission", so the raid line must read a real
	# delivery whose payload is trade goods — `delivers ≈5 Grey Wolf over ≈9 turns · ⇄ ~7 trade goods` —
	# and the Send button must be the ordinary primary send, NOT "brings nothing home".
	var wolf_raid := _pelt_only_wolf_raid_herd()
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(wolf_raid)
	_compose_herd(wolf_raid, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_pelts_raid")

	# 3y — THE BOTH-PRODUCTS CONTROL: the same oracle deer, whose hide sells beside its meat. Each picker
	# button's product line must carry BOTH components with FOOD LEADING (`2.33 food · 0.34 trade`), which
	# is the half of the rule the wolf frame cannot prove. Rendered right after the wolf so the two are
	# compared directly. Both frames also judge the TWO-LINE FACE itself: the rung's name over its
	# products, so `which rung` and `what it pays` stop competing in one line of glyphs.
	_hud._band_labor._player_bands = [_delivered_oracle_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	var oracle_pair := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(oracle_pair)
	_compose_herd(oracle_pair, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("herd_hunt_both_products")

	# 3z — THE INVESTMENT-RUNG TWIN of 3y (issue #397). The extractive rungs above have paid a pair since
	# #337, but Tame and Corral rendered a FOOD-ONLY payoff face — a Wild Boar read `→ 1.48 food` beside
	# its own extractive rungs' `0.74 food · 0.18 trade`, silently dropping a trade half the sim exports
	# (`pastoralTrade` / `corralTrade`). A prepared herd pays the same two products a hunted one does, so
	# the payoff obeys the same render-only-when-non-zero rule: both faces must name FOOD THEN TRADE.
	# Domestication is mid-ladder on purpose — Tame retires from the picker once the herd is fully tamed,
	# and Corral is knowledge-gated below that, so a frame carrying BOTH rungs necessarily has one greyed.
	# A gated rung still wears its payoff (that is the point of showing it), which this frame also proves.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	var payoff_boar := _investment_pair_boar_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(payoff_boar)
	# TAME RUNNING: its own payoff rides the checked box's own face, exactly as the offered box's does
	# below — which is what makes the pair of assertions here a comparison of two STATES of one control
	# rather than of two different widgets.
	_compose_herd(payoff_boar, PELT_FRAME_HUNTERS, COMPOSE_FLOOR_UNSET, "tame")
	await _settle()
	await _save("herd_investment_both_products")
	_assert_hud("Tame's payoff names BOTH products, food leading",
		_improvement_face(_hud._drawercompose._compose_sheet, HudConst.LABOR_POLICY_TAME)
			.ends_with(BOAR_TAME_PAYOFF_FACE))
	# CORRAL OFFERED: the boar is fully tamed here, so Tame is DONE and Corral is the rung on offer —
	# its payoff quoted on the checkbox's own face, which is where a not-yet-started rung states terms.
	var penned_boar := _investment_pair_boar_herd()
	penned_boar["domestication"] = 1.0
	_hud._compose.reset_hunt_source()
	_show_herd(penned_boar)
	_compose_herd(penned_boar, PELT_FRAME_HUNTERS)
	await _settle()
	await _save("herd_investment_corral_offer")
	var corral_offer := _find_improvement_control(_hud._drawercompose._compose_sheet, "corral")
	_assert_hud("Corral's offered face names BOTH products, food leading",
		corral_offer is CheckBox
		and _improvement_face(_hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL).ends_with(BOAR_CORRAL_PAYOFF_FACE))

	# Reset so later states render their usual single-band dropdown + default band/policy.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# State 3d — a populated hex: the Tile card + the Occupants roster split. Three
	# player bands (turns_of_food 15 / 7 / 2 → green / amber / red vitality dots, with
	# harvest / scout / idle activity glyphs) under Bands (3), and one stressed herd
	# (amber ecology dot) under Wildlife (1). Auto-selects the first band, so the
	# drawer shows its Rations and the Scout verb.
	_show_tile(_occupied_tile_fixture())
	await _settle()
	await _save("occupants_band")

	# State 3e — the same hex with the wildlife row selected: the drawer swaps to the
	# herd's Species / Biomass and the Hunt / Follow + policy verbs.
	_show_herd(_occupied_herd_fixture())
	await _settle()
	await _save("occupants_herd")

	# State 3e-staffed — the SAME hex, with the bison actually being hunted BOTH ways at once: a
	# standing local hunt (4 workers assigned by Band Fen) and a detached hunting party of 6
	# committed to the same herd. The wildlife row's meta must read the SUM, `10 🏹`, right-aligned
	# exactly like the land row's `N 🌾` — one herd, two mechanisms, one staffing number. The drawer
	# leads with `Size: Big game`, the class that used to ride the row.
	var hunted_bands: Array = _occupied_units_fixture()
	hunted_bands[0]["labor_assignments"] = [
		{"kind": "hunt", "workers": OCCUPANTS_HUNT_LOCAL_WORKERS, "fauna_id": "game_bison_02",
			"floor": 0.5, "target_x": 58, "target_y": 24},
	]
	_hud._band_labor._player_bands = hunted_bands
	_hud._band_labor._player_band = hunted_bands[0]
	_hud._band_labor._player_expeditions = [
		{"id": "Party Fen", "entity": 401, "home_band_entity": 301,
			"size": OCCUPANTS_HUNT_PARTY_WORKERS, "expedition_mission": "hunt",
			"expedition_target_herd": "game_bison_02", "expedition_phase": "outbound",
			"current_x": 59, "current_y": 24},
	]
	_show_herd(_occupied_herd_fixture())
	await _settle()
	await _save("occupants_herd_staffed")
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_expeditions = []

	# ---- ONE CARD, ONE LIST, ONE DRAWER (docs/plan_tile_panel_layout.md) ------------------------
	# The hex is now a single card: a pinned chip strip, one selectable list with the LAND as its
	# first row, and one height-capped drawer that whichever row is lit fills. These six states are
	# the layout's own frames — every other tile/herd/forage state above exercises the same builders
	# through it, which is why their framing changed with this arc.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(3)

	# tile_panel_land — the LAND row lit: chips pinned above (In sight · Hospitable · Temperate ·
	# Fertile · Verdant Basin), the land row leading the list with the tile's forage glyph + biome
	# name, and the terrain rows + "Assign foragers" compose block in the drawer beneath.
	_show_tile(_food_tile_fixture())
	await _settle()
	await _save("tile_panel_land")

	# tile_panel_no_forage — the same layout on ground that offers nothing: the land row's meta
	# reads "No forage" and the drawer carries terrain rows with NO compose block.
	_show_tile(_barren_tile_fixture())
	await _settle()
	await _save("tile_panel_no_forage")

	# tile_panel_herd — a herd row lit: the land row is STILL in the list above it (the land never
	# leaves), and the hunt compose block fills the one drawer.
	_hud._band_labor._player_band = _hunt_preview_local_band()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(_occupied_herd_fixture())
	await _settle()
	await _save("tile_panel_herd")

	# tile_panel_crowded — THE state this arc exists for: 3 bands + 2 herds. Every row must be
	# visible, the drawer must CAP (scrolling internally on the selected band's allocation block),
	# and the whole card must fit the dock without the dock itself scrolling.
	# The player faction really IS these three bands here, and the first of them forages this very
	# hex — so the land row must report the hex's STAFFING (`5 🌾`), not restate the module name the
	# drawer and the sheet header already carry (§20). Leaving `_player_bands` empty made the row
	# fall back to the module label and ellipsise it, which is the defect, not the fixture's intent.
	_hud._band_labor._player_bands = _crowded_bands_fixture()
	_show_tile(_crowded_tile_fixture())
	await _settle()
	await _save("tile_panel_crowded")
	# NO Band/City panel is injected here, so this is the legacy fallback path — it renders
	# `%AllocationPanel`, whose Orders block already carries a Move. The drawer's §18 button must NOT
	# be added on top of it, or the player would see the same order offered twice.
	_assert_hud("the no-panel fallback shows exactly ONE Move button",
		_count_buttons_by_text(_hud.allocation_panel, MOVE_BUTTON_TEXT) == 1)

	# tile_panel_no_flash — THE FLASH-MECHANISM GUARD (docs/plan_hud_decomposition.md §2a). The
	# tile-inspector "flash" on every turn-advance was `_render_selection_panel` UNCONDITIONALLY
	# tearing down + recreating the card's chips / roster rows / drawer actions even on a same-tile
	# restate where only numbers moved. That teardown is a transient the static PNG harness cannot
	# capture, so this proves the MECHANISM instead: a same-tile restate with CHANGED NUMBERS, fed
	# through the REAL per-snapshot `reapply_selection("tile", …)` path, must PATCH the existing nodes
	# in place — SAME instances, values updated — never free + rebuild them; while a genuine identity
	# change (a band entering the hex) still DOES rebuild, so the diff cannot mask a real update.
	# Proven to FAIL against the pre-fix teardown code.
	_hud.clear_selection()
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _no_flash_band_fixture(3, 0.90)
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_show_tile(_no_flash_tile_fixture(0.01, 84.0))
	await _settle()
	var flash_chip_ids := _child_instance_ids(_hud.tile_chips)
	var flash_row_ids := _child_instance_ids(_hud.subject_list)
	var flash_action_ids := _child_instance_ids(_hud.forage_assign_controls)
	var flash_chip_before := _chip_text(_hud.tile_chips, 1)          # the habitability chip
	var flash_summary_before := _forage_summary_text()
	# A SECOND snapshot of the SAME tile with different numbers (habitability rating, patch biomass,
	# forage worker count + rate), replayed the way Main replays MapView's payload every turn.
	_hud._band_labor._player_band = _no_flash_band_fixture(5, 1.40)
	_hud.reapply_selection("tile", _no_flash_tile_fixture(0.06, 61.0))
	await _settle()
	await _save("tile_panel_no_flash")
	_assert_hud("same-tile restate REUSES the chip nodes (no teardown)",
		_child_instance_ids(_hud.tile_chips) == flash_chip_ids and not flash_chip_ids.is_empty())
	_assert_hud("same-tile restate REUSES the roster-row nodes (no teardown)",
		_child_instance_ids(_hud.subject_list) == flash_row_ids and not flash_row_ids.is_empty())
	_assert_hud("same-tile restate REUSES the forage drawer-action nodes (no teardown)",
		_child_instance_ids(_hud.forage_assign_controls) == flash_action_ids and not flash_action_ids.is_empty())
	_assert_hud("…and the reused chip's value updated to the new number (Hospitable → Harsh)",
		_chip_text(_hud.tile_chips, 1) != flash_chip_before and _chip_text(_hud.tile_chips, 1) != "")
	_assert_hud("…and the reused drawer summary updated to the new worker count/rate",
		_forage_summary_text() != flash_summary_before and _forage_summary_text() != "")
	# The identity-change half: a band ENTERING the hex must rebuild the roster, so a masked-update
	# bug in the diff cannot survive here.
	var flash_tile_with_band := _no_flash_tile_fixture(0.06, 61.0)
	flash_tile_with_band["units"] = [_no_flash_band_fixture(5, 1.40)]
	_hud.reapply_selection("tile", flash_tile_with_band)
	await _settle()
	_assert_hud("a band entering the hex REBUILDS the roster (membership changed)",
		_child_instance_ids(_hud.subject_list) != flash_row_ids)
	_hud.clear_selection()
	_hud._band_labor._player_band = {}

	# ---- PART 2: THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§17) ----------------------
	# The two ~270px compose blocks left the drawer for a floating sheet. The states above are now the
	# READ state (a standing summary + `Assign … ▸`, and the drawer is visibly shorter for it); these
	# are the WRITE state.

	# tile_panel_compose_forage — the sheet open over the LAND: the full policy grid + band picker +
	# stepper + forecast + button, floating beside the selection card. The MAP MUST STILL BE VISIBLE
	# behind it — an assignment is composed AGAINST the map (work-range ring, hunt reach), so unlike
	# NarrativeForkPanel this sheet draws NO scrim.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_show_tile(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	_assert_hud("the Assign button opens the compose sheet", _hud.is_compose_sheet_open())
	await _save("tile_panel_compose_forage")

	# tile_panel_compose_herd — the herd sheet on the EXPEDITION branch (the band is beyond hunt
	# reach): the raid forecast + "Send Expedition" must survive the move to the sheet intact.
	_hud._band_labor._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("tile_panel_compose_herd")

	# tile_panel_compose_gated — a LOCKED rung inside the sheet: 🐄 Corral greyed AND its gate reasons
	# rendered right beside it. The reasons explain the greyed button, so they had to travel WITH the
	# picker; a reason left behind in the drawer would explain a button that is no longer there.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.35,
	}])
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	await _save("tile_panel_compose_gated")

	# ---- BEHAVIOURAL ASSERTIONS (§17) -----------------------------------------------------------
	# (2) A SNAPSHOT MUST NOT CLOSE THE SHEET. `reapply_selection` runs every turn; closing on it
	# would make the sheet unusable under autoplay. Driven through the real per-snapshot path — the
	# same `reapply_selection("herd", …)` Main replays from MapView's payload — with the sheet open.
	_assert_hud("precondition: the herd sheet is open before the snapshot",
		_hud.is_compose_sheet_open())
	_hud.reapply_selection("herd", _corral_locked_herd_fixture())
	await _settle()
	_assert_hud("a snapshot re-render leaves the compose sheet OPEN",
		_hud.is_compose_sheet_open())
	# …and the SAME refresh DOES close it when the subject it is composing is gone. This half is what
	# proves the half above is not vacuous: the refresh really ran and chose to keep the sheet.
	_hud.reapply_selection("herd", _raid_boar_herd())   # a DIFFERENT herd id
	await _settle()
	_assert_hud("a snapshot that swaps the subject closes the sheet",
		not _hud.is_compose_sheet_open())
	# Re-open on the herd the targeting assertion below needs.
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()

	# (3) STARTING A TARGETING FLOW CLOSES THE SHEET — a floating sheet over the map while the player
	# is being asked to click a hex is a trap. Driven through the real Move-band entry point.
	_hud._targeting.begin_move_band()
	await _settle()
	_assert_hud("starting move-band targeting closes the compose sheet",
		not _hud.is_compose_sheet_open())

	# (1) ESC PRECEDENCE. The chain lives in `Main.escape_claimant`, driven here with the REAL HUD's
	# own `is_compose_sheet_open()` / `is_targeting_active()` rather than hardcoded booleans. It is
	# asserted with BOTH TRUE AT ONCE — targeting is still armed above and the player then opens the
	# sheet on top of it (the drawer stays clickable during targeting, so this is a state the client
	# really reaches). Both-true is the only configuration that can tell the ORDER apart: with the
	# sheet open alone, any ordering answers "compose_sheet".
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	_hud._targeting.begin_move_band()
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	_assert_hud("precondition: a sheet and targeting are BOTH active",
		_hud.is_compose_sheet_open() and _hud.is_targeting_active())
	_assert_hud("ESC claims the sheet AHEAD of targeting (and never the pause menu)",
		MAIN_SCRIPT.escape_claimant(false, _hud.is_compose_sheet_open(), _hud.is_targeting_active())
			== MAIN_SCRIPT.ESC_COMPOSE_SHEET)
	_hud.close_compose_sheet()
	await _settle()
	_assert_hud("…and with the sheet closed, ESC falls back through to targeting-cancel",
		MAIN_SCRIPT.escape_claimant(false, _hud.is_compose_sheet_open(), _hud.is_targeting_active())
			== MAIN_SCRIPT.ESC_TARGETING)
	_hud.cancel_active_targeting()
	await _settle()

	# (4) A WHEEL TICK OVER THE CATCHER MUST NOT DISMISS THE SHEET. The catcher is MOUSE_FILTER_STOP
	# across the whole viewport, so an idle scroll anywhere over the map lands on it — and this sheet
	# has NO SCRIM precisely because the player is still reading that map while composing. Dismissing
	# on a wheel tick would throw the composition away mid-read. Driven through the REAL handler by
	# emitting the catcher's own `gui_input`, and paired with the left-click half, which is what proves
	# the wheel half is not vacuous (i.e. that click-outside dismissal still works at all).
	_show_herd(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	_assert_hud("precondition: the sheet is open before the wheel tick",
		_hud.is_compose_sheet_open())
	for wheel_button in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		_hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(wheel_button))
	await _settle()
	_assert_hud("a wheel tick on the catcher leaves the compose sheet OPEN",
		_hud.is_compose_sheet_open())
	_hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(MOUSE_BUTTON_LEFT))
	await _settle()
	_assert_hud("a left-click on the catcher still CLOSES the compose sheet",
		not _hud.is_compose_sheet_open())

	# tile_panel_standing — §14's own frame: the drawer's CLOSED read state on a source the player
	# already works. The summary reuses `SourceForecast.source_yield_readout` verbatim, so it wears the same three
	# parts a Band-panel Current-actions row does — the policy glyph + crew + rate, the ⚠ overdraw
	# flag (ecological) and the "· only N of M working" overstaff note (labor). This fixture crosses
	# the two deliberately: a Deplete patch that DOES overdraw, staffed 4 where only 2 are needed.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _standing_forage_band_fixture()
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_show_tile(_food_tile_fixture())
	await _settle()
	await _save("tile_panel_standing")

	# tile_panel_land_sticky — THE BEHAVIOURAL ASSERTION for the sticky land selection, driven
	# through the REAL client path, because the bug does not live where a hand-picked
	# `reapply_selection("tile", …)` would put it. MapView holds its OWN occupant selection, and
	# `refresh_selection_payload` answers `kind: "unit"` for as long as `selected_unit_id >= 0` — so on
	# an OCCUPIED hex the tile branch is never even reached. Hence: instance the real MapView, wire the
	# two signals Main wires, click the hex, click the LAND row, then ASK MAPVIEW what the next
	# snapshot's payload is and feed whatever it says into `reapply_selection`. Hardcoding "tile" here
	# would assert a path the bug cannot reach.
	var sticky_map: Node2D = MAP_VIEW_SCRIPT.new()
	# Data only — a visible map would render behind the HUD in this and every later frame.
	sticky_map.visible = false
	add_child(sticky_map)
	# FoW OFF, stated explicitly — this assertion DIES SILENTLY without it. `_fow_enabled` defaults
	# to `true` (it fails closed for the live client), which fog-gates every band and herd out of
	# `_tile_info_at` / `_units_on_tile` at source: the crowded hex reads "Unexplored / Unknown" and
	# both asserts below pass VACUOUSLY, with no occupant left to fail to stick to. The guard must
	# see the occupants it was written to guard.
	sticky_map.set_fow_enabled(false)
	sticky_map.display_snapshot(_sticky_map_snapshot())
	# Main's wiring, verbatim (Main._on_map_tile_selected / _on_map_unit_selected /
	# _on_hud_roster_occupant_selected).
	sticky_map.tile_selected.connect(_hud.show_tile_selection)
	sticky_map.unit_selected.connect(_hud.show_unit_selection)
	_hud.roster_occupant_selected.connect(sticky_map.select_occupant)
	sticky_map.handle_hex_click(STICKY_TILE.x, STICKY_TILE.y, MOUSE_BUTTON_LEFT)  # lands on a band
	_hud._selectioncard._on_land_row_selected()                                   # the player picks LAND
	# The next snapshot: Main asks MapView what is selected and replays it into the HUD.
	var sticky_payload: Dictionary = sticky_map.refresh_selection_payload()
	_hud.reapply_selection(String(sticky_payload.get("kind", "none")), sticky_payload.get("data", {}))
	await _settle()
	_assert_hud("land row clears MapView's occupant selection (payload is not \"unit\")",
		String(sticky_payload.get("kind", "")) != "unit")
	_assert_hud("land selection survives the next snapshot on a crowded hex",
		_hud._selection._selected_subject == "land" and _hud._selection._selected_unit.is_empty() and _hud._selection._selected_herd.is_empty())
	await _save("tile_panel_land_sticky")
	sticky_map.tile_selected.disconnect(_hud.show_tile_selection)
	sticky_map.unit_selected.disconnect(_hud.show_unit_selection)
	_hud.roster_occupant_selected.disconnect(sticky_map.select_occupant)
	sticky_map.queue_free()
	await get_tree().process_frame

	# tile_panel_deselect_keeps_tile — THE BEHAVIOURAL ASSERTION for issue #405: clicking an EMPTY hex
	# while a herd is selected must leave that hex SELECTED, not selectionless. A PNG cannot carry this
	# claim — "no selection outline" and "an outline drawn under an overlay" look identical — so it is
	# asserted on state, through the real click path, the `tile_panel_land_sticky` idiom above: a real
	# MapView, Main's signal wiring, real `handle_hex_click` calls. `_handle_entity_selection`'s clear
	# branch only arms once an occupant is selected, so the first click (the herd) is what makes the
	# second click able to fail; asserting on the empty click alone would pass vacuously.
	var deselect_map: Node2D = MAP_VIEW_SCRIPT.new()
	deselect_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	add_child(deselect_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true`, and a
	# fog-gated herd would never be selected by the first click, leaving nothing to deselect.
	deselect_map.set_fow_enabled(false)
	deselect_map.display_snapshot(_deselect_map_snapshot())
	# Main's wiring for this path, verbatim (Main._on_map_tile_selected / _on_map_herd_selected /
	# _on_map_selection_cleared).
	deselect_map.tile_selected.connect(_hud.show_tile_selection)
	deselect_map.herd_selected.connect(_hud.show_herd_selection)
	deselect_map.selection_cleared.connect(_hud.clear_selection)
	deselect_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("clicking a herd selects the herd AND its tile",
		deselect_map.selected_herd_id != "" and deselect_map.selected_tile == DESELECT_HERD_TILE)
	deselect_map.handle_hex_click(DESELECT_LAND_TILE.x, DESELECT_LAND_TILE.y, MOUSE_BUTTON_LEFT)
	var deselect_payload: Dictionary = deselect_map.refresh_selection_payload()
	await _settle()
	_assert_hud("deselecting a herd on an empty hex KEEPS that hex selected (#405)",
		deselect_map.selected_tile == DESELECT_LAND_TILE)
	_assert_hud("deselecting a herd clears the OCCUPANT selection",
		deselect_map.selected_herd_id == "" and deselect_map.selected_unit_id == -1)
	_assert_hud("the deselected hex falls back to its land card",
		String(deselect_payload.get("kind", "")) == "tile")
	await _save("tile_panel_deselect_keeps_tile")
	deselect_map.tile_selected.disconnect(_hud.show_tile_selection)
	deselect_map.herd_selected.disconnect(_hud.show_herd_selection)
	deselect_map.selection_cleared.disconnect(_hud.clear_selection)
	deselect_map.queue_free()
	await get_tree().process_frame

	# tile_panel_occupant_cycle — THE BEHAVIOURAL ASSERTION for issue #429: re-clicking a hex cycles
	# through ALL of its occupants, not just its bands. `_handle_entity_selection` used to take
	# `herds_here[0]` and only when the hex held no units at all, so a multi-herd hex always opened on
	# the same herd and a herd sharing a hex with ANY band was unreachable from the map at any number of
	# clicks. A PNG cannot carry that claim — the frames differ only in which name the card is showing —
	# so it is asserted on state through the real click path, the `tile_panel_land_sticky` idiom: a real
	# MapView, Main's signal wiring, real `handle_hex_click` calls.
	var cycle_map: Node2D = MAP_VIEW_SCRIPT.new()
	cycle_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	add_child(cycle_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true` and
	# `_herds_on_tile` gates on `_is_tile_visible`, so a fogged hex presents a ZERO-occupant stack and
	# every assertion below would pass vacuously on a cycle with nothing in it.
	cycle_map.set_fow_enabled(false)
	cycle_map.display_snapshot(_cycle_map_snapshot())
	# Main's wiring, verbatim — INCLUDING the roster relay, because the HUD's fresh-hex auto-pick
	# re-enters `select_occupant` through it mid-click (tile_selected → show_tile_selection → render →
	# the auto-pick → roster_occupant_selected → here), rewriting `cycle_index` to the FIRST occupant.
	# Without this connection the harness would not exercise the re-entrancy the cycle has to survive.
	cycle_map.tile_selected.connect(_hud.show_tile_selection)
	cycle_map.unit_selected.connect(_hud.show_unit_selection)
	cycle_map.herd_selected.connect(_hud.show_herd_selection)
	# The FOURTH map→HUD selection signal, and the one the land stop lives or dies on: without it the
	# land click reaches the HUD as nothing at all, the auto-pick sees two empty occupant dicts on a
	# hex it has no recorded choice for, and the first band is selected straight back.
	cycle_map.land_selected.connect(_hud.show_land_selection)
	_hud.roster_occupant_selected.connect(cycle_map.select_occupant)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("click 1 of the occupant cycle lands on the band (bands still win the first click)",
		cycle_map.selected_unit_id == CYCLE_BAND_ENTITY and cycle_map.selected_herd_id == "")
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("click 2 advances PAST the band to the first herd (#429: unreachable before)",
		cycle_map.selected_herd_id == CYCLE_HERD_FIRST_ID and cycle_map.selected_unit_id == -1)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("click 3 advances to the SECOND herd (a multi-herd hex is not stuck on herds[0])",
		cycle_map.selected_herd_id == CYCLE_HERD_SECOND_ID)
	# The cycled herd has to survive the next snapshot, which is where the HUD's sticky-choice guard
	# could undo it: Main asks MapView what is selected and replays whatever it answers.
	var cycle_payload: Dictionary = cycle_map.refresh_selection_payload()
	_hud.reapply_selection(String(cycle_payload.get("kind", "none")), cycle_payload.get("data", {}))
	await _settle()
	_assert_hud("the cycled herd survives the next snapshot (the HUD auto-pick does not steal it back)",
		String(_hud._selection._selected_herd.get("id", "")) == CYCLE_HERD_SECOND_ID)
	# Click 4 reaches the LAND — the cycle is everything the tile PANEL lists, not just the occupants,
	# and the land is its LAST stop so the first click on a fresh hex still opens on the top occupant.
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	await _settle()
	_assert_hud("click 4 advances past the last herd to the LAND (the cycle lists what the panel lists)",
		cycle_map.selected_unit_id == -1 and cycle_map.selected_herd_id == "" \
			and _hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND)
	# THE STICKY HALF. `_resolve_auto_selected_subject` auto-picks the first band whenever BOTH
	# occupant dicts are empty — which IS the land state — so a map-driven land pick that did not
	# record the choice tile would be undone by the very next snapshot, silently and invisibly. This
	# is the inverse of the herd case above, where a non-empty occupant dict suppresses the auto-pick
	# on its own. Same idiom: ask MapView what the next frame carries and replay whatever it answers.
	var cycle_land_payload: Dictionary = cycle_map.refresh_selection_payload()
	_hud.reapply_selection(String(cycle_land_payload.get("kind", "none")), cycle_land_payload.get("data", {}))
	await _settle()
	_assert_hud("the cycled LAND survives the next snapshot (the HUD auto-pick does not steal the band back)",
		_hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND \
			and _hud._selection._selected_unit.is_empty() and _hud._selection._selected_herd.is_empty())
	await _save("tile_panel_occupant_cycle")
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("click 5 WRAPS past the land to the top of the stack",
		cycle_map.selected_unit_id == CYCLE_BAND_ENTITY and cycle_map.selected_herd_id == "")
	# A PANEL roster-row click re-anchors the cycle: the next map click continues from THAT row, which
	# is what deriving the advance from the selected occupant's IDENTITY (rather than from the stored
	# index) buys. Picking the first herd from the list must make the next map click give the second.
	_hud._selectioncard._on_roster_row_selected("herd", CYCLE_HERD_FIRST_ID)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("a map re-click continues from the herd picked in the PANEL, not the stored index",
		cycle_map.selected_herd_id == CYCLE_HERD_SECOND_ID)
	cycle_map.tile_selected.disconnect(_hud.show_tile_selection)
	cycle_map.unit_selected.disconnect(_hud.show_unit_selection)
	cycle_map.herd_selected.disconnect(_hud.show_herd_selection)
	cycle_map.land_selected.disconnect(_hud.show_land_selection)
	_hud.roster_occupant_selected.disconnect(cycle_map.select_occupant)
	cycle_map.queue_free()
	await get_tree().process_frame

	# The SMALLEST cycle with a land stop, and the one the change was asked for on: a hex with exactly
	# ONE animal and no band, where re-clicking has to TOGGLE herd ↔ land. It re-uses the deselect
	# fixture (one herd, no bands) because that is already that shape. No PNG — the frames it would
	# produce are the herd card and the land card, both already captured elsewhere; what is unproven
	# is the two-member cycle, which only state can carry. The roster relay is wired for the same
	# reason it is above: with no band on the hex the auto-pick reaches for the first HERD, so a land
	# stop that failed to record its choice tile would be pulled straight back to the animal.
	var toggle_map: Node2D = MAP_VIEW_SCRIPT.new()
	toggle_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	add_child(toggle_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true`, and a
	# fog-gated herd leaves a ZERO-occupant hex whose cycle has no land stop to reach.
	toggle_map.set_fow_enabled(false)
	toggle_map.display_snapshot(_deselect_map_snapshot())
	toggle_map.tile_selected.connect(_hud.show_tile_selection)
	toggle_map.herd_selected.connect(_hud.show_herd_selection)
	toggle_map.land_selected.connect(_hud.show_land_selection)
	_hud.roster_occupant_selected.connect(toggle_map.select_occupant)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("a lone herd still wins the FIRST click on its hex (land is the cycle's LAST stop)",
		toggle_map.selected_herd_id == DESELECT_HERD_ID)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	await _settle()
	_assert_hud("re-clicking a ONE-animal hex toggles to the land",
		toggle_map.selected_herd_id == "" and toggle_map.selected_unit_id == -1 \
			and _hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("a third click toggles back to the animal (a two-member cycle wraps)",
		toggle_map.selected_herd_id == DESELECT_HERD_ID)
	toggle_map.tile_selected.disconnect(_hud.show_tile_selection)
	toggle_map.herd_selected.disconnect(_hud.show_herd_selection)
	toggle_map.land_selected.disconnect(_hud.show_land_selection)
	_hud.roster_occupant_selected.disconnect(toggle_map.select_occupant)
	toggle_map.queue_free()
	await get_tree().process_frame

	# tile_panel_unseen — a REMEMBERED hex. Chips + the land row render (geography is remembered
	# knowledge), the herd this fixture deliberately carries does NOT, and the drawer states that
	# the contents are unknown. An empty list would be a claim of emptiness we cannot back up.
	_hud.clear_selection()
	_show_tile(_sight_tile_fixture(VIS_DISCOVERED))
	await _settle()
	await _save("tile_panel_unseen")

	# tile_panel_band — a PLAYER band lit while the dockable Band/City panel exists: its detail
	# renders there, so the drawer would otherwise be a blank gap. It must point at where the
	# detail went instead. (The panel is injected only for this frame and released after, so the
	# reserved edge does not follow the states below.)
	var tile_panel_band_panel: BandCityPanel = BAND_CITY_PANEL_SCENE.instantiate()
	add_child(tile_panel_band_panel)
	# Fan the panel's reservation onto the HUD as Main does, and dock it RIGHT — docked left it
	# reserves the very edge the selection card lives on and covers the frame under test.
	tile_panel_band_panel.reservation_changed.connect(func(edge: int, size: float):
		_hud.set_reserved_inset(&"band_panel", edge, size))
	tile_panel_band_panel.set_dock(SIDE_RIGHT)
	# The panel's narrow shell shows ONE zone, and its prefs are a fresh profile (see the isolation
	# block in `_ready`), so it opens on `DEFAULT_TAB` = work. This frame is about where the band
	# DETAIL went, so ask for the band zone — the same rule `band_panel_preview` carries. It used to
	# come up on `band` only because a previous run had written that tab into the PLAYER's prefs file.
	tile_panel_band_panel.set_active_tab(BandCityPanel.ZONE_BAND)
	_hud.set_band_city_panel(tile_panel_band_panel)
	# THREE player bands on this hex, and the faction default is the FIRST one — so "the band the
	# list has selected" and "the faction's default band" are DIFFERENT answers, which is the only
	# configuration in which the Move assertion below can fail (§18).
	var tile_panel_band_roster: Array = _crowded_bands_fixture()
	_hud._band_labor._player_bands = tile_panel_band_roster
	_hud._band_labor._player_band = tile_panel_band_roster[0]
	var tile_panel_band_subject: Dictionary = tile_panel_band_roster[0]
	tile_panel_band_subject["tile_info"] = _crowded_tile_fixture()
	_hud.show_unit_selection(tile_panel_band_subject)
	# The player then picks the SECOND band, through the real subject-list selection path.
	_hud._selectioncard.select_roster_occupant("unit", TILE_PANEL_MOVE_BAND_ENTITY)
	await _settle()
	await _save("tile_panel_band")

	# THE MOVE ASSERTION (§18). Driven through the drawer's REAL button — calling
	# `_targeting.begin_move_band` directly would assert the resolver, not the wiring — and the pending
	# move must name the band SELECTED IN THE LIST (302), never the faction default
	# (`_player_band`, 301), which is what a naive wiring resolves to on a crowded hex.
	var tile_panel_move_btn: Button = _find_button_by_text(_hud.allocation_panel, MOVE_BUTTON_TEXT)
	_assert_hud("the player-band drawer offers Move", tile_panel_move_btn != null)
	if tile_panel_move_btn != null:
		tile_panel_move_btn.emit_signal("pressed")
	await _settle()
	_assert_hud("Move enters move-band targeting", _hud.is_targeting_active())
	_assert_hud("…targeting the band SELECTED IN THE LIST, not the faction default",
		int(_hud._targeting._pending_move_band.get("entity", -1)) == TILE_PANEL_MOVE_BAND_ENTITY)
	_hud.cancel_active_targeting()
	await _settle()
	_hud.set_band_city_panel(null)
	_hud.set_reserved_inset(&"band_panel", SIDE_RIGHT, 0.0)
	tile_panel_band_panel.queue_free()
	await get_tree().process_frame

	# (`tile_panel_feed_shown` is RETIRED with the command feed. It existed to prove the left dock's
	# TWO growing cards could share one height budget; there is one growing card there now.)

	# Restore the single-band compose context the states below assume.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# State 4 — targeting active: pressing "Move" on the band allocation panel enters
	# tile-targeting, raising the top-centre banner ("MOVE … click a destination tile").
	_hud.show_unit_selection(_band_fixture())
	_hud._targeting.begin_move_band()
	await _settle()
	await _save("targeting_banner")
	_hud.cancel_active_targeting()

	# The old states 4a–4c — the pre-launch raid forecast hanging off the TARGETING BANNER — are
	# gone with the mechanism. They existed because the herd was only known at the targeting step; the
	# band-panel launch flow now picks the quarry FIRST, inside the compose sheet, so the forecast
	# lives in the form with the real party size and policy (band_panel_preview `band_panel_compose_hunt`).

	# State 5 — quick-hunt convenience (map double-click a herd): with idle workers it
	# assigns them to hunt; with none it posts a command-feed note instead of silently
	# no-opping. Seed a fully-staffed band (0 idle) so the note renders in the Command Feed.
	var staffed_band := _band_fixture()
	staffed_band["idle_workers"] = 0
	_hud._band_labor._player_band = staffed_band
	_show_tile(_food_tile_fixture())
	_hud.quick_assign_hunters("game_bison_02")
	await _settle()
	await _save("quick_hunt_note")

	# State 5a — PNG-LESS companion: **the shortcut must not blank the improvement axis** (issue #442).
	# `assign_labor` deliberately does not carry the second axis, so between the double-click and the
	# next snapshot the OPTIMISTIC PENDING overlay is the only thing holding it — and an emit that lets
	# it default to `IMPROVEMENT_NONE` flashes a running pen off the work board (and drops the herding
	# crew floor from the would-be count to the ownership-gated one) for the whole turn. No frame:
	# a board rendered from a blanked axis looks like a perfectly ordinary board, so only the overlay
	# can testify. The band hunts ONE herd and is already building its pen; the precondition assertion
	# is what stops the second one passing on a band that had nothing to keep.
	# `Hud._resolve_assign_band` prefers the SELECTED player unit over `player_band()`, and an earlier
	# state left one selected — so clear it, or the shortcut resolves to a band that is building nothing
	# and the assertion below judges the wrong band. (The next state clears it too; this is not restored.)
	_hud.clear_selection()
	var quick_hunt_band := _band_fixture()
	quick_hunt_band["band_id"] = QUICK_HUNT_BAND_ID
	quick_hunt_band["idle_workers"] = QUICK_HUNT_IDLE_WORKERS
	quick_hunt_band["labor_assignments"] = [{
		"kind": "hunt", "workers": 2, "floor": 0.5,
		"improvement": SourceForecast.IMPROVEMENT_CORRAL,
		"fauna_id": QUICK_HUNT_HERD_ID, "target_x": 66, "target_y": 10,
	}]
	_hud._band_labor._player_band = quick_hunt_band
	_assert_hud("precondition: the quick-hunt band really is building a pen on that herd",
		_hud._band_labor.improvement_for_hunt(quick_hunt_band, QUICK_HUNT_HERD_ID)
			== SourceForecast.IMPROVEMENT_CORRAL)
	_hud.quick_assign_hunters(QUICK_HUNT_HERD_ID)
	var quick_hunt_pending: Dictionary = _hud._band_labor.pending_assigns_for(
		int(quick_hunt_band.get("entity", -1))).get(
			_hud._band_labor.pending_key(SourceForecast.LABOR_KIND_HUNT, -1, -1, QUICK_HUNT_HERD_ID), {})
	_assert_hud("a quick-hunt keeps the pen the band is already building on that herd",
		String(quick_hunt_pending.get("improvement", "")) == SourceForecast.IMPROVEMENT_CORRAL)
	# Leave the overlay as it was found — a snapshot with a NEWER turn is what confirms a pending edit.
	_hud._band_labor.reconcile_pending(_hud._band_labor.current_turn() + 1)
	_hud._band_labor._player_band = _band_fixture()

	# State 6 — turn orb, ALL-CLEAR: a player band with zero idle workers → empty
	# attention registry → the orb calm-pulses (dashed cyan arc), the caption reads
	# "Turn 42 · ▸ all clear", and no badge shows.
	_hud.clear_selection()
	_hud.update_overlay(42, {})
	_hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await _settle()
	await _save("turn_orb_clear")

	# State 6a-fit — THE TURN NUMBER IS ON THE FACE, and its type size is MEASURED, not tabled
	# (`TurnOrb._turn_font_size`: step down from `TURN_FONT_SIZE_MAX` until the string fits
	# `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at `TURN_FONT_SIZE_MIN`). Walk 1 → 47 → 999 →
	# 1200 and assert, for each, that the rendered string is the number, that the chosen size is inside
	# the declared band, and — the point of the fit — that it actually FITS the usable chord. A 4-digit
	# turn is the case that would otherwise overflow the circle; `turn_orb_turn_4digit` is its frame.
	for probe_turn in [1, 47, 999, TURN_ORB_FOUR_DIGIT_TURN]:
		_hud.update_overlay(probe_turn, {})
		await _settle()
		_assert_turn_face_fits(probe_turn)
	# The curved `TURN` word above the number rides the same face. Its geometry is number-independent, so
	# one arithmetic check covers every probe; the 4-digit frame saved above is where the CLEARANCE between
	# the word and the widest number is judged by eye, at true size.
	await _assert_turn_word_clears()
	await _save("turn_orb_turn_4digit")
	# Back to the state the following orb states describe.
	_hud.update_overlay(42, {})
	await _settle()

	# State 6b — turn orb, EMPTY registry, orb-face CLICK: advancing must always be possible
	# from the orb, so with nothing to triage the click ADVANCES the turn directly and opens NO
	# popover (the old bug opened a tall blank box whose Advance affordance was pushed off-screen,
	# trapping the player). Assert the emitted advance signal (the harness can't run a real turn)
	# and that no popover opened. THE CLICK NOW ALSO RAISES THE RESOLVING GATE, so the saved frame
	# shows the gate at t=0: a dimmed face, the number just beginning to break apart, and the ring's
	# rotating sweep arc where the calm pulse was.
	var advance_hits := [0]
	var advance_cb := func() -> void: advance_hits[0] += 1
	_hud.turn_orb.advance_requested.connect(advance_cb)
	_hud.turn_orb._on_face_pressed()
	await _settle()
	_assert_turn_orb("empty click advances", advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	# THE BUG THE GATE EXISTS FOR, and the one thing a PNG can never show: mashing the face used to
	# queue N advances while the server was still resolving turn 1. A second press must emit NOTHING.
	_hud.turn_orb._on_face_pressed()
	await _settle()
	_assert_turn_orb("a second click while resolving emits no advance",
		advance_hits[0] == 1 and _hud.turn_orb.is_resolving())
	# The footer is the SECOND way to advance, so it wears the second block reason: `_advance_block_label`
	# returns "Resolving…" here where a fork would make it "Answer first to advance". Opened
	# programmatically (the face click is gated) and closed again, so the frame below is orb-only.
	_hud.turn_orb.open_popover()
	await _settle()
	var resolving_footer := _turn_orb_advance_button()
	_assert_turn_orb("the popover's Advance wears the resolving reason and is disabled",
		resolving_footer != null and resolving_footer.disabled
		and resolving_footer.text == TurnOrb.ADVANCE_RESOLVING_LABEL)
	_hud.turn_orb.toggle_popover()
	await _settle()
	await _save("turn_orb_clear_click_advances")

	# State 6b-resolving — THE IN-PROGRESS FRAME, mid-orbit on the very gate State 6b just raised:
	# the old number has broken apart into evenly-spaced glyphs riding a ring inside the face, the
	# ring itself carries a rotating sweep arc in the accent (NOT the calm pulse, which would say
	# "nothing needs you" mid-turn), the face is dimmed and the `TURN` word is gone with the number.
	# The clock is frozen, so the phase is STEPPED to a chosen point rather than raced.
	_step_turn_orb_anim(TurnOrb.RESOLVE_SCATTER_SEC
		+ TurnOrb.RESOLVE_ORBIT_PERIOD * TURN_ORB_ORBIT_CAPTURE_FRACTION)
	await _settle()
	await _save("turn_orb_resolving")

	# Answer the turn the way the server does and let the re-form finish, so the gate is DOWN for the
	# states below (6c clicks the face again and expects the popover) — then restore turn 42.
	await _settle_turn_orb_resolve(43)
	_assert_turn_orb("the gate lifts once the re-formed number lands",
		not _hud.turn_orb.is_resolving() and _hud.turn_orb._face.text == "43")
	_hud.update_overlay(42, {})
	await _settle()

	# State 6c — turn orb, NON-EMPTY registry: the click opens the reasons popover, and the
	# popover's `Advance ▸` footer button emits advance_requested (unchanged behavior). Seed one
	# attention entry, open via the face click, then fire the footer button and assert the emit.
	advance_hits[0] = 0
	_hud.update_band_alerts([
		{"faction": 0, "entity": 511, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 5},
	])
	_hud.turn_orb._on_face_pressed()
	await _settle()
	var opened := _hud.turn_orb._popover_open
	var footer_btn := _turn_orb_advance_button()
	var had_footer := footer_btn != null
	if had_footer:
		footer_btn.pressed.emit()   # frees the popover (advance closes it)
	await _settle()
	_assert_turn_orb("non-empty popover + footer advances",
		opened and had_footer and advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	# The footer is the OTHER advance emitter, so it raises the same gate — lower it before anything
	# below renders the orb, and put the turn back where the following states describe it.
	await _settle_turn_orb_resolve(43)
	_hud.update_overlay(42, {})
	await _settle()
	_hud.turn_orb.advance_requested.disconnect(advance_cb)

	# State 6d — THE HOVER HINT, both halves of it. The turn NUMBER never leaves the face, so the
	# affordance is a small glyph BELOW it that appears on hover and names what the click will do —
	# and the two clicks are different, so the two glyphs are different. Here the registry is still
	# State 6c's (one idle-workers row), so hovering must show the up-caret `▴`: the reasons popover
	# opens ABOVE the orb, and promising `‣‣` would promise an advance this click does not perform.
	_hud.turn_orb._set_face_hovered(true)
	await _settle()
	_assert_turn_orb("hovering a non-empty orb hints review, not advance",
		_hud.turn_orb._hint_glyph == TurnOrb.HINT_GLYPH_REVIEW and _hud.turn_orb._face.text == "42")
	await _save("turn_orb_hint_review")

	# ...and with an EMPTY registry the same hover shows `‣‣`, because THAT click does advance. The
	# number stays on the face in both frames — that is the whole change.
	_hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await _settle()
	_assert_turn_orb("hovering an all-clear orb hints advance",
		_hud.turn_orb._hint_glyph == TurnOrb.HINT_GLYPH_ADVANCE and _hud.turn_orb._face.text == "42")
	await _save("turn_orb_hint_advance")

	# ...and the same hover at the WIDEST number, which is the tight case for the stack: the face is
	# 74px and now carries the curved `TURN`, a number and this hint. Turn 1200 steps the number down
	# to 23px, so it is the frame where the hint has the MOST room below it; `turn_orb_hint_advance`
	# above (turn 42, a 30px number) is the least. Both clearances are judged here, at true size.
	_hud.update_overlay(TURN_ORB_FOUR_DIGIT_TURN, {})
	await _settle()
	await _save("turn_orb_hint_4digit")
	_hud.update_overlay(42, {})
	_hud.turn_orb._set_face_hovered(false)
	await _settle()

	# State 7 — turn orb, ALL THREE ATTENTION KINDS (the folded-in Alerts panel): a first
	# snapshot seeds prior band sizes so "losing population" has a baseline, then the live
	# snapshot fires one of each producer — Band 1 starving (days 3 < critical → critical/red),
	# Band 2 shrank 90→78 with emigrants (losing population → warn/amber), Band 3 has idle
	# workers (warn/amber). The badge reads "3", the pulse stops, and the popover (opened here)
	# lists all three with the starving/critical row sorted to the TOP, each with a Jump row.
	# A starving EXPEDITION is interleaved between the bands to verify the bands-only numbering:
	# it produces NO attention entry (never "Band N starving") and does not shift Band 2/Band 3's
	# positional numbers — the idle-workers row still reads "Band 3", matching the picker/header.
	_hud.update_band_alerts([
		{"faction": 0, "entity": 601, "size": 120, "turns_of_food": 12.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		{"faction": 0, "entity": 602, "size": 90, "turns_of_food": 999.0, "activity": "hunt",
			"current_x": 31, "current_y": 21},
		{"faction": 0, "entity": 603, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9},
	])
	_hud.update_band_alerts([
		# Band 1 — starving (3 turns of food, below critical).
		{"faction": 0, "entity": 601, "size": 120, "turns_of_food": 3.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		# A detached hunt expedition, also starving — must NOT emit a "Band N starving" entry and
		# must NOT consume a band number (Band 2/Band 3 below stay 2 and 3).
		{"faction": 0, "entity": 650, "size": 6, "turns_of_food": 2.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "hunting", "home_band_entity": 601,
			"current_x": 25, "current_y": 18},
		# Band 2 — losing population: 90 → 78, well-fed but 12 emigrated last turn → "people leaving".
		{"faction": 0, "entity": 602, "size": 78, "turns_of_food": 999.0, "morale": 0.30,
			"morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 31, "current_y": 21},
		# Band 3 — idle labor: 4 working-age workers unassigned.
		{"faction": 0, "entity": 603, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_attention")

	# State 7b — turn orb, AWAITING-ORDERS producer: an expedition parked at its objective is a
	# demand on the player (it burns provisions doing nothing), structurally the same class as idle
	# workers — so it produces its OWN attention row per party. Here: one band with idle workers
	# (the two producers must coexist) + FOUR awaiting parties (a scout and a hunt party name their
	# objective; the 4th trips the ATTENTION_AWAITING_MAX_ROWS cap → an aggregate "+1 more awaiting
	# orders" row). A non-awaiting (outbound) expedition proves only `awaiting` produces a row. The
	# popover must still fit above the orb with its `Advance ▸` footer on-screen.
	_hud.turn_orb.set_attention([])   # drop State 7's registry so this frame is only these rows
	_hud.update_band_alerts([
		{"faction": 0, "entity": 701, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
		{"faction": 0, "entity": 751, "size": 6, "turns_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 39, "current_y": 26},
		# The hunt party names its OBJECTIVE by species (game_deer_07 → "Red Deer" via the world-herd
		# list pushed above), not the raw fauna id — the row has to be actionable at a glance.
		{"faction": 0, "entity": 752, "size": 5, "turns_of_food": 7.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "awaiting", "home_band_entity": 701,
			"expedition_target_herd": "game_deer_07", "current_x": 64, "current_y": 11},
		{"faction": 0, "entity": 753, "size": 4, "turns_of_food": 6.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 18, "current_y": 44},
		{"faction": 0, "entity": 754, "size": 4, "turns_of_food": 5.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 51, "current_y": 8},
		{"faction": 0, "entity": 755, "size": 6, "turns_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "outbound", "home_band_entity": 701,
			"current_x": 33, "current_y": 30},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_awaiting_orders")

	# State 7c — turn orb, STARVING-PEN producer: the band that keeps the pen could not pay its feed,
	# so the penned herd is shrinking every turn and 25 turns of investment are draining away. Two
	# rows here ON PURPOSE, and they are NOT the same alert twice: the empty larder is one cause with
	# two different losses — the PEOPLE are starving (critical, jumps to the band) and the HERD is
	# starving (warn, jumps to the herd, where the fed fraction + feed cost are). Only one shouts.
	_hud.turn_orb.set_attention([])
	_set_world_herds([_starving_pen_herd_fixture()])
	_hud.update_band_alerts([
		{"faction": 0, "entity": 801, "size": 46, "turns_of_food": 1.0, "activity": "hunt",
			"current_x": 64, "current_y": 11, "idle_workers": 0,
			"labor_assignments": [
				# BOTH PRODUCTS (issue #337): the hide sells beside the meat, so the drawer's standing
				# summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade shown only because it
				# is non-zero. Same `SourceForecast.source_yield_readout` the Band panel's rows use.
				{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5,
					"improvement": "corral",
					"target_x": 66, "target_y": 10, "actual_yield": 0.84, "sustainable_yield": 0.84,
					"trade_yield": 0.12, "realized_trade_yield": 0.12},
			]},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_starving_pen")
	# **THIS PRODUCER WAS DEAD, AND THE FRAME COULD NOT SAY SO** (issue #442). It found its pens by
	# `policy == "corral"`, and the axis split made `policy` always one of the four STANCES — the build
	# verb moved to `improvement` — so the test could never again be true and no starving pen had been
	# reported since. A PNG of an orb with one row in it looks entirely reasonable, which is why the
	# assertion is the only thing that could have caught it. Read off the RENDERED rows.
	var pen_rows := _orb_rows()
	_assert_hud("the starving-pen producer still fires after the stance/improvement split",
		_orb_row_with(pen_rows, HudAttentionVocab.ATTENTION_PEN_LABEL_FORMAT % RED_DEER_LABEL) != null)
	_set_world_herds(_world_herds_fixture())   # restore the shared world-herd list

	_hud.turn_orb.toggle_popover()   # close, so later states render without it

	# State 7d — turn orb, THE UNWORKED-RUNG + UNDER-CREWED producers (issue #442). A built rung nobody
	# is working is the one loss the WORK BOARD structurally cannot report: that board lists
	# ASSIGNMENTS, and an unworked patch has none, so it is ABSENT from the board rather than flagged on
	# it. The orb is the generic "something needs you" hub, so this is where it has to live — and the
	# URGENCY rides the row's own words, not a standing counter the player would learn to watch.
	#
	# The fixture is built as a set of CONTROLS, because every claim here is about which sources produce
	# a row and which do not:
	#   (70,20) tended, owned, unworked, grace 2   → a row, counting down
	#   (71,20) FIELD,  owned, unworked, grace 0   → a row, the penalty biting NOW
	#   (72,20) tended, owned, unworked, NO grace  → a row with NO countdown at all (the bool's whole job)
	#   (73,20) WILD,   owned, unworked            → NO row: nothing has been built here to lose
	#   (74,20) tended, NOT ours                   → NO row: a rival's ground is not our alarm
	#   (66,10) tended, owned, WORKED by the band  → NO row: it is being kept
	_hud.turn_orb.toggle_popover()
	_hud.turn_orb.set_attention([])
	_set_forage_patches(_neglect_patches_fixture())
	_set_world_herds([_under_crewed_herd_fixture()])
	_hud.update_band_alerts([
		{"faction": 0, "entity": 811, "size": 40, "turns_of_food": 99.0, "activity": "forage",
			"current_x": 66, "current_y": 10, "idle_workers": 0,
			"labor_assignments": [
				# The WORKED control — the same rung on the same kind of ground, kept.
				{"kind": "forage", "workers": 2, "target_x": 66, "target_y": 10, "floor": 0.5,
					"improvement": "", "actual_yield": 1.20, "sustainable_yield": 1.20},
				# The UNDER-CREWED herd: 2 keepers where the sim asks 4.
				{"kind": "hunt", "workers": UNDER_CREWED_HERD_STAFFED, "fauna_id": "game_deer_07",
					"floor": 0.5, "improvement": "",
					"target_x": 68, "target_y": 15, "actual_yield": 0.60, "sustainable_yield": 0.60},
			]},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_unworked_rung")
	var neglect_rows := _orb_rows()
	for row in neglect_rows:
		print("ui_preview: orb row  %s | %s" % [String(row["label"]), String(row["detail"])])
	var lapsing_soon: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 70, 20])
	var lapsing_now: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["sow"], 71, 20])
	var no_grace: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 72, 20])
	_assert_hud("an unworked Tended Patch raises a row naming the rung and the hex",
		lapsing_soon != null)
	# **THE COUNTDOWN, at N > 0.** The number is the wire's own `(grace + 1) - neglect`; the client does
	# no subtraction, so a row quoting anything else means someone re-derived it.
	_assert_hud("…whose urgency is IN THE TEXT — `%s`"
		% (HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT % [
			NEGLECT_GRACE_SOON, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]),
		lapsing_soon != null and String(lapsing_soon["detail"]) == (
			HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT % [
				NEGLECT_GRACE_SOON, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]))
	# **AND AT ZERO, which is NOT "nothing at risk".** `0` is the wire's "the penalty is biting NOW" —
	# the most urgent reading there is — so it must never render as a `0`-turn countdown.
	_assert_hud("a rung at grace 0 says the ground is reverting NOW, never `in 0 turns`",
		lapsing_now != null and String(lapsing_now["detail"]) == HudAttentionVocab.ATTENTION_LAPSE_NOW)
	# **AND THE BOOL, which is the whole reason the pair is two fields.** `has_neglect_grace == false`
	# means nothing is at risk; rendered as a countdown it would collide with the biting-now zero and
	# read as the loudest row on the card. Asserted by DIGITS, so no phrasing of a number can pass.
	_assert_hud("a source with NO neglect grace renders no countdown at all — not even a zero",
		no_grace != null and not _contains_digit(String(no_grace["detail"])))
	# THE THREE NEGATIVE CONTROLS, counted rather than searched: a producer that alarmed on everything
	# would satisfy every positive assertion above.
	_assert_hud("a wild patch, a rival's ground and a WORKED rung raise nothing (%d rows, not %d)"
		% [neglect_rows.size(), _neglect_patches_fixture().size()],
		neglect_rows.size() == UNWORKED_EXPECTED_ROWS)
	# THE ANIMAL HALF — under-crewed rather than unworked, because a herd carries no owner on the wire
	# and only the band's own assignment can attribute it.
	var herd_row: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_CREWED_LABEL_FORMAT % RED_DEER_LABEL)
	_assert_hud("a managed herd below its keeper count raises a row naming both counts",
		herd_row != null and String(herd_row["detail"]) == (
			HudAttentionVocab.ATTENTION_UNDER_CREWED_DETAIL_FORMAT % [
				UNDER_CREWED_HERD_STAFFED, UNDER_CREWED_HERD_NEEDED,
				HudAttentionVocab.ATTENTION_SHED_SOON_FORMAT % [
					NEGLECT_GRACE_HERD, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]]))
	# MOST URGENT FIRST — the rows are sorted on the wire's countdown, so the ground reverting NOW sits
	# above the one with turns left. `ATTENTION_UNWORKED_MAX_ROWS` caps the list, and a cap that kept an
	# arbitrary three would be worse than none.
	# BOTH ROWS PINNED PRESENT FIRST. `find()` answers -1 for a missing row, and -1 is less than every
	# real index — so the bare comparison PASSES when the biting-now row is absent, which is the one
	# failure this assertion exists to catch. Presence is carried by the earlier row assertions, but an
	# assertion that reads as an ordering claim must not be satisfiable by an absence.
	var now_at := neglect_rows.find(lapsing_now)
	var soon_at := neglect_rows.find(lapsing_soon)
	_assert_hud("…and the biting-now row sorts above the one still counting down",
		now_at >= 0 and soon_at >= 0 and now_at < soon_at)
	_hud.turn_orb.toggle_popover()
	_set_forage_patches([])              # restore: no patches for the states below
	_set_world_herds(_world_herds_fixture())   # restore the shared world-herd list
	_hud.turn_orb.set_attention([])

	# State 8 — reserved-space docking (Slice 1 refactor): a left-edge reservation of
	# RESERVED_PROBE_WIDTH px insets the whole HUD (LayoutRoot.offset_left), so the top/bottom
	# bars start that much further right — mirroring how the docked Inspector shrinks the play
	# space. Save the inset frame, then release it (size 0) and save the restored frame.
	_hud.clear_selection()
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, RESERVED_PROBE_WIDTH)
	await _settle()
	await _save("reserved_dock")
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	await _settle()
	await _save("reserved_dock_cleared")

	# Terrain-legend sort control (base terrain legend, key == "terrain"). Several
	# biomes of varying tile counts so the default count-desc order + the Name/Count
	# sort toggles + sort persistence across a regen push are all visible. Rendered
	# before the full-screen icon probe below so the right-dock legend isn't covered.
	# Opened here and closed at the end of THIS block (not hundreds of lines later).
	_open_legend()
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("terrain_legend_count_desc")  # default: Count, high→low

	# Click "Name" → alphabetical A→Z.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await _settle()
	await _save("terrain_legend_name_asc")

	# Click "Name" again → Z→A.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await _settle()
	await _save("terrain_legend_name_desc")

	# Click "Count" → back to count, and again → low→high.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	await _settle()
	await _save("terrain_legend_count_asc")

	# Simulate a map regen (fresh terrain-legend push): the chosen sort (count asc)
	# must persist, not snap back to the default.
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("terrain_legend_persist")
	_close_legend()

	# ---- The Telling (docs/plan_the_telling.md) -----------------------------------------------
	# The narrative fork decision surface + the client-side end-turn gate. The fixture is the REAL
	# authored copy from core_sim/src/data/beat_definitions.json (`sedentarization.soft_drift`, the
	# `soft_drift.long_chase` wardrobe entry, nouns resolved as the sim resolves them at post time),
	# so the frame shows prose at real length rather than lorem that flatters the layout.
	_hud.clear_selection()
	_hud.update_overlay(41, {})
	# Pin the register so the run is deterministic (the preference persists in user://).
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)

	# State F1 — the panel, auto-opened the first time the fork appears: the narration as the hero
	# element, three choices in catalog order (the defer choice styled `ghost`, and ALWAYS enabled —
	# it is the out the gate depends on), the gloss collapsed, the voice toggle in the footer.
	_hud.update_pending_forks(_pending_forks_fixture())
	_hud.update_stance_axes(_stance_axes_fixture())
	await _settle()
	await _save("narrative_fork_panel")

	# State F2 — the SAME fork in the other register. Verifies the toggle and that the noticeably
	# shorter/looser `warm` copy lays out as well as the long `mythic` one. The registers come from
	# the fork itself, never a hardcoded list.
	_hud._turnorb._fork_panel._on_register_picked(FORK_REGISTER_WARM)
	await _settle()
	await _save("narrative_fork_panel_warm")

	# State F3 — THE GATE, and the single most important assertion in this file. With a blocking
	# fork seeded, an orb-face click must NOT advance the turn (it opens the reasons popover
	# instead), and the popover's Advance button must be DISABLED and wear the reason. This is the
	# exact inverse of `turn_orb_clear_click_advances`.
	_hud._turnorb._fork_panel.close()
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)
	var fork_advance_hits := [0]
	var fork_advance_cb := func() -> void: fork_advance_hits[0] += 1
	_hud.turn_orb.advance_requested.connect(fork_advance_cb)
	_hud.turn_orb._on_face_pressed()
	await _settle()
	var fork_footer := _turn_orb_advance_button()
	_assert_turn_orb("blocking fork: face click does not advance",
		fork_advance_hits[0] == 0 and _hud.turn_orb._popover_open)
	_assert_turn_orb("blocking fork: Advance is disabled",
		fork_footer != null and fork_footer.disabled)
	await _save("turn_orb_fork_blocks")
	_hud.turn_orb.advance_requested.disconnect(fork_advance_cb)
	_hud.turn_orb.toggle_popover()

	# (The old State F4 `narrative_feed` — narrative prose styled INSIDE the command feed — was
	# retired with PR-C. The feed no longer renders narrative kinds at all, so the state could only
	# ever have shown their absence; `telling_and_feed` below is its replacement and tests the
	# thing that now matters: that the receipts survive alongside real narrative volume.)

	# ---- The Telling panel (PR-C) ------------------------------------------------------------
	# States G1–G3. The dock is cleared first so the two narrative cards are judged on their own
	# chrome rather than on whatever the previous state left selected.
	# `clear_selection()` deliberately KEEPS the tile card (deselecting an occupant should not
	# forget the hex), so the tile info has to go first or the Tile card fills the dock and both
	# narrative cards get squeezed out of the frame entirely.
	_hud._selection._selected_tile_info.clear()
	_hud.clear_selection()
	_hud._telling.reset()

	# G1 — ORAL: the current utterance only. No page furniture, no leaf controls, no page number — oral
	# memory does not keep the previous telling, so the visible page is pinned to the NEWEST beat (the
	# fork at tick 22). Ingest the real authored copy (incl. the catalog's longest line, so a page's
	# wrap is genuinely exercised).
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_fixture_events())
	await _settle()
	await _save("telling_panel_oral")

	# G2 — PAINTED: the accumulating wall. The SAME entries, now retained as pages you can walk FORWARD
	# through (a marks + position cue, no back control). Parked mid-way (page 3/6) so the retained
	# earlier pages and the forward-only affordance read at once. `debug_jump_to` is the NON-animating
	# park — these SETTLED end-state frames must not catch a page-turn tween mid-flight (that's what the
	# `telling_turn_*_mid` states capture on purpose).
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	_hud._telling.debug_jump_to(2)
	await _settle()
	await _save("telling_panel_painted")

	# G3 — WRITTEN: the full book. Page number + ‹ › leaf controls, parked on a NON-LAST page (3/6) so
	# backward leafing is visibly available (both ‹ and › active). Nothing about the copy changes
	# between the rungs (per-medium copy is a deliberate non-goal) — only the title, accent and
	# CAPABILITIES age, which is the whole point.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(2)
	await _settle()
	await _save("telling_panel_written")

	# G3b — UNREAD: the yields-to-reader rule. The reader is held on an OLD page (1/6) while newer pages
	# exist; the page never turns on its own, so a subtle "a new telling waits" cue appears instead of
	# yanking them forward. (Advancing the turn — reveal_newest() — is what catches them up.)
	_hud._telling.debug_jump_to(0)
	await _settle()
	await _save("telling_panel_unread")

	# G4 — THE FRAME THAT PROVES THE SPLIT WORKED. The Telling panel holds its fixed page while a batch
	# of ordinary command receipts arrives: before the split, two beats filled the narrative card
	# outright and pushed every receipt off screen. The Telling must claim exactly its own kinds here
	# and nothing else — the receipts belong to the event dock. (Oral restored.)
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_command_receipts())
	await _settle()
	await _save("telling_and_feed")

	# G5 — THE DEFAULT DOCK LAYOUT. The right dock holds the Telling panel ALONE: Victory and
	# Terrain Types both ship suppressed, so the narrative surface gets the full right-dock height
	# instead of the squeezed share it had while it lived under the left dock's selection cards.
	# The left dock is the selection card's alone now — the command feed that used to sit under it is
	# retired — which is the layout this frame exists to show.
	_hud.update_victory_state(_victory_state_fixture())
	await _settle()
	await _save("dock_default_layout")
	# The Telling panel is registered with `right_dock.add(..., 10)`, and `PanelDock._reorder`
	# reparents. Screenshotting the dock only shows it LOOKS right; assert WHERE it lives, so a
	# dropped/reordered registration (or a scene edit that re-authors it under the left dock)
	# fails here instead of silently reverting the narrative surface to the left column.
	_assert_hud("default layout: Telling panel lives in the right dock stack",
		_hud.telling_panel.get_parent() == _hud.right_stack)

	# G6 — the same frame with BOTH reference cards toggled back on (the `V` / `L` path), so the
	# right dock's stacking order — Telling, then Victory, then Terrain Types — is visible and the
	# Telling panel is seen to yield height rather than overlap.
	# Victory goes through the REAL `toggle_victory` (the `V` path, prefs write included — the harness
	# cleared the section at startup, and this toggles back below); the legend uses the harness helper.
	_hud.toggle_victory()
	_open_legend()
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("dock_panels_revealed")
	_assert_hud("toggled on: Terrain Types legend is visible", _hud.terrain_legend_panel.visible)
	_assert_hud("toggled on: Victory panel is visible", _hud.victory_panel.visible)
	# Restore the shipped default so any later state renders the real layout.
	_hud.toggle_victory()
	_close_legend()

	# TWO-BEAT ORAL — a single speaking turn firing TWO beats (both sharing one tick, so they are ONE
	# page). The page must GROW to fit both beats + gloss with NO scrollbar — the playtest fix (the
	# strictly-fixed height scrolled the second beat out of view). Assert the inner scroll is not engaged.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_two_beat_oral_fixture())
	await _settle()
	_assert_hud("two-beat oral page grows to fit both beats with no scrollbar",
		not _hud._telling.debug_page_scrolls())
	await _save("telling_panel_oral_two_beats")

	# SCROLL YIELDS-TO-READER — a beyond-cap (scrolling) page must NOT yank a mid-page reader to the top on
	# an IDEMPOTENT static repaint (a retaining-medium beat arrival that leaves the visible page unmoved),
	# but MUST start at the top on a real page turn. Two tall written pages that both overflow the cap.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud.ingest_command_events(_telling_tall_pages_fixture())
	_hud._telling.debug_jump_to(0)
	await _settle()
	var telling_scroll: ScrollContainer = _hud._telling._scroll
	telling_scroll.scroll_vertical = 40   # the reader has scrolled down the tall page
	await _settle()
	_assert_hud("tall page overflows so the reader's scroll offset holds", telling_scroll.scroll_vertical == 40)
	# Idempotent repaint: a new beat arrives on a NEW tick, but written stays on page 0 (index clamped, the
	# visible page's text is unchanged) — the yields case. Must PRESERVE the reader's scroll position.
	_hud.ingest_command_events([{"tick": 2, "kind": "narrative_beat", "label": "A far-off new telling waits.", "detail": "later"}])
	_assert_hud("idempotent repaint of the same page preserves the reader's scroll position",
		telling_scroll.scroll_vertical == 40)
	# A real page turn resets the inner scroll to the top of the new page.
	_hud._telling.leaf(1)
	_assert_hud("a real page turn resets the inner scroll to the top", telling_scroll.scroll_vertical == 0)
	_hud._telling.debug_end_turn()

	# LIVE-PATH ORAL ARRIVAL — the REAL trigger, no debug hook. Drive the actual per-snapshot Hud entry
	# points (`update_voice_medium` THEN `ingest_command_events`, plus the `_refit_right_dock` a real
	# snapshot fires) with a genuinely new beat, and PROVE a running tween is created AND survives to paint
	# frames (an idempotent re-render / refit in the same cycle must not `_kill_tween` it). This is the gap
	# the mid-transition freeze states could not cover: they show the tween CAN render, not that the live
	# beat-arrival path TRIGGERS one.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events([{"tick": 0, "kind": "narrative_beat",
		"label": "The scouts came back thinner and louder than they left, all of them saying one word: Salt Pillar Reach.",
		"detail": "sites.discovered_this_turn = 1"}])
	await _settle()   # initial population — no animation by design
	# A new snapshot: medium re-pushed unchanged (must NOT clobber), then a genuinely new beat arrives.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events([{"tick": 5, "kind": "narrative_beat",
		"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
		"detail": "provisions.total falling for 3 turns"}])
	_hud._refit_right_dock()   # a refit in the same cycle must not kill the in-flight turn tween
	_assert_hud("live oral beat-arrival creates a running page-turn tween",
		_hud._telling.debug_turn_active())
	# Advance the REAL tween to a CHOSEN mid-motion phase. Animation time is frozen (see `_ready`), so
	# awaiting frames here would advance it by exactly nothing and the state would capture the page
	# BEFORE the turn — the one frame in the run whose subject the freeze could have erased. One
	# `custom_step` of 40% of the oral dissolve keeps it genuinely in flight AND makes the phase a
	# decision instead of whatever the clock handed us (which is what made this frame drift).
	_step_tweens(TellingPanel.PAGE_TURN_DURATION_ORAL * TELLING_LIVE_TURN_FRACTION)
	_assert_hud("live oral tween survives an in-cycle refit and is still running mid-motion",
		_hud._telling.debug_turn_active())
	# The one `_settle` that must NOT flush tweens: this frame IS the mid-turn render.
	await _settle(false)
	await _save("telling_live_oral_arrival")
	_hud._telling.debug_end_turn()   # settle deterministically before the next state

	# ---- Page-turn animation: motion matures with the medium (mid-transition capture) --------------
	# The harness dumps single frames, so each state DRIVES a page turn, then FREEZES the tween at its
	# midpoint (`debug_freeze_turn_at`) so the outgoing and incoming pages COEXIST in the captured PNG —
	# proof the motion is real. Setup jumps (`debug_jump_to`) are non-animating so the measured turn
	# starts from a clean resting page. The block ends with a clean static render, so the frozen overlay
	# never leaks into a later frame.
	_hud._telling.reset()
	_hud.ingest_command_events(_telling_fixture_events())

	# WRITTEN — a horizontal SLIDE, forward: the outgoing page exits left as the incoming enters from the
	# right. Frozen mid-slide, both pages are onscreen offset horizontally, with the ‹ › book furniture.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(1)
	await _settle()
	_hud._telling.leaf(1)
	_hud._telling.debug_freeze_turn_at(0.5)
	await _settle()
	await _save("telling_turn_written_mid")

	# PAINTED — the incoming page RISES from just below with a fade (new marks drifting onto the wall).
	# Frozen partway up, the incoming page sits low + faint over the fading outgoing one.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	_hud._telling.debug_jump_to(1)
	await _settle()
	_hud._telling.leaf(1)
	_hud._telling.debug_freeze_turn_at(0.45)
	await _settle()
	await _save("telling_turn_painted_mid")

	# ORAL — a CROSSFADE in place: a new recitation replacing the last (oral keeps no prior page). Frozen
	# at the crossover, both pages read at partial alpha in the same spot, with NO furniture.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud._telling.debug_jump_to(3)
	await _settle()
	_hud._telling.reveal_newest()
	_hud._telling.debug_freeze_turn_at(0.5)
	await _settle()
	await _save("telling_turn_oral_mid")

	# INTERRUPTION — a rapid second turn must KILL the running tween and settle to the CORRECT final page,
	# with no leftover overlay/offset. Turn 0→1, immediately 1→2, then force the settle a completed tween
	# would reach, and assert the visible page is 2 with the overlay gone.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(0)
	await _settle()
	_hud._telling.leaf(1)          # 0 → 1 (tween begins)
	_hud._telling.leaf(1)          # 1 → 2 immediately (must kill + restart)
	_hud._telling.debug_end_turn() # force the settle
	await _settle()
	_assert_hud("interrupted page-turn settles to the final page with no leftover overlay",
		_hud._telling.debug_visible_index() == 2 and not _hud._telling.debug_overlay_visible())
	await _save("telling_turn_interrupted")

	# Clean static state (newest oral page, no frozen overlay) before the downstream frames.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud._telling.reveal_newest()
	await _settle()

	# ---- Hunt/husbandry render-honesty pass (intensification ladder client UX) ----------------------
	# Fix #1 + #5 — CURRENT ACTIONS rows: a summary row headlines the honest per-turn FOOD rate
	# (sustainable, not the 0.00 pulse) + the policy/status glyphs, with NO `≈… /turn` animals-per-turn
	# cadence (that lives on the compose-preview line). Both rows must read `Hunt <species> +X /turn ♻ ●`;
	# the big-game (under-crewed) row also keeps its muted "· 1.9 wasted" note (yld.muted_note, not cadence).
	_set_world_herds(_hunt_rhythm_herds_fixture())
	_hud.show_unit_selection(_hunt_actions_band_fixture())
	await _settle()
	await _save("hunt_actions_rhythm")
	_set_world_herds(_world_herds_fixture())
	# **THE WASTED NOTE IS THE ANIMAL WEB'S, AND THE SAME NUMBER MEANS THE OPPOSITE ON A PATCH.** One
	# wire field, two facts: on a herd `wasted_yield` is `killed − carried`, meat that really rotted;
	# on a patch it is `room − take`, stock the crew did not reach, which the sim's own note says
	# "stays in the stock and regrows". Reported from play as `0.75 wasted` sitting permanently on a
	# well-run Alluvial Plain — and permanent is the word, because `room > take` is the state the
	# compose sheet RECOMMENDS (its `hold it after` target is far below its `clear it now` one).
	#
	# **Asserted as a PAIR against ONE readout call**, not on a rendered frame: no forage fixture
	# carries a non-zero `wasted_yield`, so a frame assertion would pass with the bug fully present.
	# The hunt half is what stops the fix from being "silence the note everywhere", and the tooltip is
	# checked beside the note because the wasted text was appended to both.
	var wasted_model := {"has_yield": true, "workers": 2, "workers_needed": 0,
		"actual_yield": 0.30, "sustainable_yield": 0.30, "wasted_yield": 0.75, "overdraws": false}
	var wasted_forage := SourceForecast.source_yield_readout(
		wasted_model, SourceForecast.LABOR_KIND_FORAGE)
	var wasted_hunt := SourceForecast.source_yield_readout(
		wasted_model, SourceForecast.LABOR_KIND_HUNT)
	_assert_hud("a PATCH states no waste — the stock it did not reach is still standing",
		String(wasted_forage.get("muted_note", "")) == ""
			and not String(wasted_forage.get("tooltip", "")).contains("wasted"))
	_assert_hud("…while a HERD still does, where the meat really rotted",
		String(wasted_hunt.get("muted_note", "")).contains("wasted")
			and String(wasted_hunt.get("tooltip", "")) != "")

	# Fix #2 + #1(forecast) + #6 — the LOCAL hunt compose view: the policy picker shows each rung's
	# per-turn take so Sustain < Surplus < Deplete < Eradicate reads as ASCENDING, and the live preview
	# pairs its rate with the kill-rhythm. (The stepper on a WILD herd reads "Hunters".)
	# A compact NON-food tile so the herd drawer (not a full forage tile card) lands in-frame.
	var picker_herd := _herd_fixture()
	picker_herd["tile_info"] = _compact_herd_tile_fixture()
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_show_herd(picker_herd)
	_compose_herd(picker_herd, 3, SourceForecast.FLOOR_FOOD_PEAK)
	await _settle()
	await _save("hunt_picker_ascending")

	# Fix #6 — a MANAGED (corralled) herd's local crew are HERDERS, not a hunt party: the stepper reads
	# "Herders" so a pen whose workersNeeded scales with the herd doesn't look like a hunt-party bug.
	_hud._compose.reset_hunt_source()
	_show_herd(_domesticated_herd_fixture())
	_compose_herd(_domesticated_herd_fixture())
	await _settle()
	await _save("hunt_crew_herders")

	# Fix #4 — LEARNING knowledge visibility: Penning at 34% (0 < value < 1) must climb WITH its % in
	# the top-bar strip, not be absent-until-100. Seed Selection mid-climb too; Cultivation/Herding ✔.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "seed_selection": 0.6, "herding": 1.0, "penning": 0.34}])
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("knowledge_penning_climbing")
	# Restore the default strip for any later frame.
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# STALE-CLOSURE GUARD (herd) — the drawer diff-cache patches a same-SHAPE restate in place and
	# DELIBERATELY keeps the compose-open button's `pressed` closure intact. Before the fix
	# `_herd_actions_shape` omitted the herd id, so switching to a DIFFERENT herd of identical structure
	# took the PATCH path and left "Assign hunters ▸" opening the PREVIOUS herd's compose (playtest: the
	# rabbit's button opened the boar's Tame sheet). Two wild huntable herds share the "assign-button, no
	# summary" shape, so the buggy patch path fires; pressing the button must open herd B's compose, not A's.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()
	var stale_herd_a := _wild_herd_fixture()
	var stale_herd_b := _wild_herd_fixture()
	stale_herd_b["id"] = "game_deer_stale_99"
	stale_herd_b["species"] = "Roe Deer"
	stale_herd_b["label"] = "Roe Deer (game_deer_stale_99)"
	# Drive the REAL drawer-actions path (`refresh_drawer_actions` calls these), settling a frame between
	# each so the diff-cache's deferred `queue_free` completes: without the settles, stale buttons linger
	# in-tree, the child-count patch test misreads, and `_find_button_by_text` grabs the wrong node.
	_hud._drawercompose._clear_herd_drawer()   # drop any prior-state button so A gets a FRESH closure
	await _settle()
	_hud._drawercompose.build_herd_drawer_actions(stale_herd_a)   # full rebuild → button opens A
	await _settle()
	_hud._drawercompose.build_herd_drawer_actions(stale_herd_b)   # same shape → the patch path under test
	await _settle()
	var stale_herd_btn := _find_button_by_text(
		_hud.herd_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower())
	assert(stale_herd_btn != null)
	stale_herd_btn.pressed.emit()
	await _settle()
	await _save("herd_assign_button_targets_selected_herd")
	# The opened compose must be herd B (the herd now shown), never the herd A it was first wired against.
	assert(_hud._compose.kind() == ComposeState.KIND_HERD)
	assert(_hud._compose.subject() == String(stale_herd_b["id"]))
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_hunt_source()

	# STALE-CLOSURE GUARD (forage) — the identical diff-cache pattern on the forage drawer. Before the fix
	# the forage-actions shape omitted the tile subject key, so switching between two food tiles of the same
	# shape kept "Assign foragers ▸" opening the PREVIOUS tile's forage compose. Same drive, other drawer.
	_hud._compose.reset_forage_source()
	var stale_tile_a := _food_tile_fixture()
	var stale_tile_b := _food_tile_fixture()
	stale_tile_b["x"] = 70
	stale_tile_b["y"] = 20
	_hud._drawercompose._clear_forage_drawer()   # drop any prior-state button so tile A gets a FRESH closure
	await _settle()
	_hud._drawercompose.build_forage_drawer_actions(stale_tile_a)   # full rebuild → button opens tile A
	await _settle()
	_hud._drawercompose.build_forage_drawer_actions(stale_tile_b)   # same shape → the patch path under test
	await _settle()
	# STRUCTURALLY, not by face: the open button's noun follows the patch's rung, and the bare `assert`
	# this replaces BREAKS THE HEADLESS RUN INTO THE DEBUGGER rather than reporting — measured, it hung
	# the suite the first time the noun moved under it.
	var stale_forage_btn := _forage_open_button()
	_assert_hud("the forage drawer's open button survives a same-shape restate", stale_forage_btn != null)
	if stale_forage_btn != null:
		stale_forage_btn.pressed.emit()
	await _settle()
	await _save("forage_assign_button_targets_selected_tile")
	# The opened compose must be tile B (subject key "70,20"), never tile A ("66,10") it was first wired to.
	_assert_hud("the forage drawer's button opens a FORAGE compose",
		_hud._compose.kind() == ComposeState.KIND_FORAGE)
	_assert_hud("…on the tile now SHOWN (70,20), not the one it was first wired against",
		_hud._compose.subject() == "70,20")
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_forage_source()

	# STALE-DATA GUARD (herd) — herd_compose_reopen_fresh. The two guards above prove the retained
	# compose-open closure re-targets when the SUBJECT changes; this one proves it re-reads when only
	# the herd's STATE moves, which the shape signature deliberately cannot see. `_herd_actions_shape`
	# carries the herd's IDENTITY and none of its live numbers (folding `domestication` /
	# `herders_needed` in would rebuild the drawer every tick and bring back the reflow flash the patch
	# path exists to remove), so the turn taming starts the drawer PATCHES in place and the button keeps
	# its connection. A closure that CAPTURED the herd dict then feeds the sheet a pre-tame herd — the
	# playtest report where the drawer read "Domesticating 4% · Herders 3 / 4" while the sheet beside it
	# still said "This herd is 0% tamed" and "max 3 workers useful here", one whole turn behind.
	# `_live_herd` re-resolves through the selection model at PRESS time, so both surfaces read one dict.
	# A PNG alone cannot carry this claim: a fresh sheet and a stale one differ only in their numbers.
	# Knowledge is complete on both rungs, so Tame is ungated (a gated Tame would be reset to Sustain
	# and the frame would test nothing) and Corral's ONLY gate reason is the herd's own tameness — the
	# number under test, rendered as the compact one-liner rather than a bulleted list.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	var reopen_band := _band_fixture()
	reopen_band["idle_workers"] = REOPEN_IDLE_WORKERS
	reopen_band["working_age"] = REOPEN_WORKING_AGE
	_hud._band_labor._player_band = reopen_band
	_hud._band_labor._player_bands = [reopen_band]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_floor(SourceForecast.DEFAULT_HARVEST_FLOOR)
	var reopen_wild := _reopen_wild_herd_fixture()
	var reopen_taming := _reopen_taming_herd_fixture()
	# TURN N — select the wild herd through the real path, which fully rebuilds the drawer and wires a
	# FRESH closure onto the compose-open button.
	_show_herd(reopen_wild)
	await _settle()
	var reopen_btn := _find_button_by_text(
		_hud.herd_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower())
	assert(reopen_btn != null)
	# Open the sheet by PRESSING the real button, then dial Tame in and press again — the second open
	# finds `hunt_key` unchanged, so the rung survives the source-changed re-seed (`_compose_herd`'s
	# double-open, done here through the button because the button's closure is what is under test).
	reopen_btn.pressed.emit()
	await _settle()
	_hud._compose.set_hunt_improvement(HudConst.LABOR_POLICY_TAME)
	reopen_btn.pressed.emit()
	await _settle()
	# BASELINE — the pre-tame numbers, so the assertions below are proven to be a CHANGE and not a
	# coincidence. Both sentences are built from the shipped formats, so they read what the player reads.
	# **THE RUNNING IMPROVEMENT'S METER IS THE WITNESS** (issue #442). It was the gated Corral rung's
	# "This herd is N% tamed" reason line, which only existed while a build verb was a picker rung; the
	# meter on the checked Tame box states the SAME number, is the thing the player actually reads, and
	# is unambiguously per-herd — so a stale captured dict shows through it just as plainly.
	# **BUILT FROM THE METER FORMAT AND MATCHED AS A PREFIX**, because the face now carries the rung's
	# payoff after the percent (`🐾 Taming — 4% · then 1.20 food`) and the payoff is not what this pair
	# is about. The percent is followed by `%` in the format, so one meter's face can never be a prefix
	# of the other's — `— 0%` does not lead `— 34%` — and the claim stays as exact as the `==` was.
	var stale_meter := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
		FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME),
		String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[HudConst.LABOR_POLICY_TAME]), 0]
	var fresh_meter := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
		FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME),
		String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[HudConst.LABOR_POLICY_TAME]),
		HudFormat.progress_percent(REOPEN_TAMING_DOMESTICATION)]
	_assert_hud("precondition: the WILD herd's sheet quotes its own untamed meter",
		_improvement_face(_hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).begins_with(stale_meter))
	# The player closes the sheet and ends the turn. Closing matters: with the sheet OPEN the snapshot's
	# `refresh_compose_sheet` rebuilds it against `_selection.herd()` and self-heals, which is exactly
	# why the bug reads as "one turn behind" rather than as a permanent lie.
	_hud._drawercompose.close_compose_sheet()
	await _settle()
	var reopen_btn_id := reopen_btn.get_instance_id()
	# TURN N+1 — the SAME herd id restated with taming under way, through the real per-snapshot path.
	_hud.reapply_selection("herd", reopen_taming)
	await _settle()
	_assert_hud("the same-herd restate PATCHES the drawer in place (the button node survives)",
		_hud.herd_assign_controls.get_child_count() == 1
		and _hud.herd_assign_controls.get_child(0).get_instance_id() == reopen_btn_id)
	# The crew NOUN is the second half of the report: the sim now demands keepers (`herders_needed` 4),
	# so `SourceForecast.is_managed_hunt_source` reads managed and the button — patched in place, not
	# rebuilt — flips to "Assign herders ▸", agreeing with the drawer's own "Herders: A / 4" row.
	_assert_hud("…and its noun flips to herders, the sim having asked for keepers",
		reopen_btn.text == HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HERD_CREW_LABEL.to_lower())
	reopen_btn.pressed.emit()
	await _settle()
	await _save("herd_compose_reopen_fresh")
	_assert_hud("the reopened sheet quotes the FRESH meter (4% tamed), not the captured 0%",
		_improvement_face(_hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).begins_with(fresh_meter))
	# The HERDERS row is the second witness, and a different field entirely (`herders_needed` 0 -> 4),
	# so the two cannot both pass off one stale-or-fresh dict by coincidence.
	#
	# **IT HAS TO BE THE ROW, NOT THE NUMBER.** This searched the drawer OR the sheet for the digit "4"
	# — which the fresh meter beside it ("Tame — 4%") already contains, so it passed off the very
	# witness it was meant to be independent of, and would have passed off a coordinate or a yield just
	# as happily. The Herders row's whole rendered value is the only text that can testify: it names the
	# demand, and it names it in the row the claim is about. Assigned is READ rather than assumed — this
	# band hunts a different herd, so the deficit form is what renders, and hardcoding it would pin the
	# fixture's staffing instead of the herd's demand.
	_assert_hud("…and the drawer's herder demand is the live one (4), not the pre-tame 0",
		_has_label_containing(_hud.occupant_detail, DetailFormat.herders_label(
			_hud._band_labor.assigned_herders_for(REOPEN_HERD_ID), REOPEN_TAMING_HERDERS)))
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_floor(SourceForecast.DEFAULT_HARVEST_FLOOR)
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# ---- THE CREW NOUN AND THE PREVIOUS HERD'S IMPROVEMENT ---------------------------------------
	# `ComposeState._hunt_improvement` is ONE slot shared by every herd, and neither `begin_hunt_source`
	# nor `reset_hunt_source` clears it — so a noun resolved from it names the crew after whichever herd
	# was composed LAST. Tick Corral on a pen-ready herd, then select a WILD one: `is_managed_hunt_source`
	# read true against the leftover, the header said `ASSIGN HERDERS`, and the stepper built by the very
	# same render — from the improvement `_build_herd_assign_controls` had just RE-SEEDED — said `Hunters`.
	# That is the disagreement `_herd_crew_noun` was written to remove, with the sides swapped.
	#
	# The two herds must differ in ID (a same-id re-open is not a source change and stages nothing) and
	# the second must be genuinely UNMANAGED — `_herd_fixture` is 40% tamed, unpenned, owing no keepers,
	# so `is_managed_hunt_source` is false on its own axis and can only read true off the leftover.
	# A PNG carries the header; the assertions carry the stepper AGREEING with it, since a header alone
	# cannot show a disagreement.
	var crew_noun_pen := _corral_ready_herd_fixture()
	crew_noun_pen["id"] = CREW_NOUN_PEN_HERD_ID
	crew_noun_pen["label"] = "Aurochs (%s)" % CREW_NOUN_PEN_HERD_ID
	crew_noun_pen["species"] = "Aurochs"
	var crew_noun_wild := _herd_fixture()
	# It DECLARES the unmanaged half of the herder pair — owed no keepers (the ownership-gated 0) while
	# naming the crew it WOULD owe if tamed. That is the still-wild tameable shape, the one case the
	# field-pair guard admits an unequal pair in, and it is what makes the claim precise: this herd's own
	# axis says "hunters", so a header reading HERDERS can only have got it from the previous herd.
	crew_noun_wild[HERDERS_NEEDED_KEY] = 0
	crew_noun_wild[HERDERS_NEEDED_IF_MANAGED_KEY] = CREW_NOUN_WILD_WOULD_BE_HERDERS
	_set_world_herds([crew_noun_pen, crew_noun_wild])
	_show_herd(crew_noun_pen)
	await _settle()
	# Tick Corral on the pen-ready herd — `_compose_herd` opens, sets the axis (what the checkbox's
	# `on_toggle` writes) and re-opens, so the sheet really is composing a pen when we leave it.
	_compose_herd(crew_noun_pen, COMPOSE_COUNT_UNSET, COMPOSE_FLOOR_UNSET, SourceForecast.IMPROVEMENT_CORRAL)
	await _settle()
	_assert_hud("precondition: the pen-ready herd's sheet really is composing a Corral",
		_hud._compose.hunt_improvement() == SourceForecast.IMPROVEMENT_CORRAL)
	_hud._drawercompose.close_compose_sheet()
	# Now the WILD herd, selected through the real path so its drawer actions rebuild.
	_show_herd(crew_noun_wild)
	await _settle()
	_assert_hud("the wild herd's drawer button asks for hunters, not the penned herd's herders",
		_find_button_by_text(_hud.herd_assign_controls,
			HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower()) != null)
	_hud._drawercompose.open_herd_compose(crew_noun_wild)
	await _settle()
	await _save("herd_compose_crew_noun_after_pen")
	_assert_hud("…and the sheet's eyebrow reads ASSIGN HUNTERS, not the previous herd's HERDERS",
		_hud._drawercompose._compose_sheet._header.text.contains(
			(HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower()).to_upper()))
	# The independent half: the STEPPER names itself from the axis the sheet re-seeded, so reading that
	# axis back proves the header agrees with the stepper rather than the two being wrong together.
	_assert_hud("…and the stepper it agrees with is built on THIS herd's own (empty) improvement axis",
		_hud._compose.hunt_improvement() == SourceForecast.IMPROVEMENT_NONE
		and _crew_row_label(_hud._drawercompose._compose_sheet)
			== HudComposeVocab.HUNT_CREW_LABEL.to_upper())
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
	# RESTORE the roster this block replaced rather than clearing it: `_guard_frame_herd_fields` scans
	# every herd the HUD holds as each later frame renders, so emptying it here would quietly retire
	# those scans from the last states of the run.
	_set_world_herds(_world_herds_fixture())

	# WORLD-BOUNDARY GUARD — `Hud.reset_world_state()`, the HUD half of the stale-world fix.
	# A freshly generated world sends `intensification_knowledge: []`, which MERGES to nothing, so the
	# previous game's "⚒ Your people know" strip survived into the new one; the Telling panel is
	# deliberately never reset per snapshot, so its old beats stayed in the book. Seed BOTH the way a
	# played-out world leaves them, then reset and assert they are gone. A PNG alone would not prove
	# this — a hidden strip and a strip that was never seeded look identical — so the frame is captured
	# for the eye and the two assertions carry the claim.
	_hud._selection._selected_tile_info.clear()
	_hud.clear_selection()
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])
	_hud._telling.reset()
	_hud.ingest_command_events(_telling_fixture_events())
	await _settle()
	_assert_hud("world-boundary guard seeds a knowledge strip to clear",
		_hud._topbar.intensification_label.visible)
	_assert_hud("world-boundary guard seeds a telling to clear",
		not _hud._telling._entries.is_empty())
	_hud.reset_world_state()
	await _settle()
	_assert_hud("world reset hides the knowledge strip (a new world knows nothing)",
		not _hud._topbar.intensification_label.visible)
	_assert_hud("world reset empties the Telling (a new world is a different story)",
		_hud._telling._entries.is_empty())
	await _save("world_reset")

	# ---- the RUNG-READY predicate (issue #412) ---------------------------------------------------
	# `RungGates.next_rung_ready` is what decides whether a worked source wears the ⌃ mark on the map
	# and on the work board, and it is pure — no node, no snapshot — so it is asserted DIRECTLY over
	# constructed sources rather than inferred from a picture. A PNG could not carry these claims: an
	# absent mark and a mark the renderer happened to skip look identical.
	#
	# Each pair below pins ONE of the three conditions (§3 of the design doc) by flipping exactly one
	# input, so a regression names which condition broke.
	var rr_knows_all := {"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0}
	var rr_knows_none := {"cultivation": 0.4, "seed_selection": 0.0, "herding": 0.3, "penning": 0.0}
	var rr_wild_patch := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"sow_site_refusal": "too_dry",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	var rr_tended_sowable := {
		"ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
		"sow_site_refusal": "",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	# THE ORDERING FIXTURE, and it has to be a WILD patch on sowable ground. `is_cultivated` retires
	# Cultivate outright, so on a TENDED patch the two rungs are mutually exclusive and the answer is
	# Sow whichever order they are tested in — an ordering assertion there passes for the wrong reason
	# (measured: swapping the branches left it green). Sow needs NO prior patch, so this is the one
	# shape that clears BOTH gates at once and can tell the orders apart.
	var rr_wild_sowable := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"sow_site_refusal": "",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	var rr_wild_herd := {"domestication": 0.0, "husbandry_ceiling": "pen"}
	var rr_tamed_herd := {"domestication": 1.0, "husbandry_ceiling": "pen"}
	var rr_forever_wild := {"domestication": 0.0, "husbandry_ceiling": "wild"}
	# UNGATED: knowledge is the difference, nothing else.
	_assert_hud("ready — a Thriving wild patch offers Cultivate once Cultivation is known",
		String(RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "cultivate")
	_assert_hud("ready — the same patch offers NOTHING while Cultivation is unlearned",
		RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_none).is_empty())
	# HIGHEST RUNG FIRST — the claim, on the only shape that can carry it (see rr_wild_sowable).
	_assert_hud("ready — a wild patch clearing BOTH gates answers the HIGHER rung (Sow, not Cultivate)",
		String(RungGates.next_rung_ready("forage", rr_wild_sowable, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "sow")
	# And the retire rule that makes the tended case mutually exclusive in the first place.
	_assert_hud("ready — a tended patch offers Sow, its Cultivate rung being retired as finished",
		String(RungGates.next_rung_ready("forage", rr_tended_sowable, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "sow")
	# OFFERED, the LAND half: the wild patch above is refused for Sow by the ground itself.
	_assert_hud("ready — dry ground withholds Sow even with Seed Selection known",
		String(RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) != "sow")
	# NOT ALREADY RUNNING: the verb in flight is progress, not an opportunity.
	_assert_hud("ready — a patch already being cultivated offers nothing (the verb is in flight)",
		RungGates.next_rung_ready("forage", rr_wild_patch, "cultivate", rr_knows_all).is_empty())
	# The animal web: same three conditions, one knowledge per transition.
	_assert_hud("ready — a wild herd offers Tame once Herding is known",
		String(RungGates.next_rung_ready("hunt", rr_wild_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "tame")
	_assert_hud("ready — a fully tamed herd advances to Corral, not back to Tame",
		String(RungGates.next_rung_ready("hunt", rr_tamed_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "corral")
	# OFFERED, the SPECIES half: a "wild"-ceiling animal never climbs, however much we know.
	_assert_hud("ready — a wild-ceiling species offers nothing at any knowledge level",
		RungGates.next_rung_ready("hunt", rr_forever_wild, RUNG_BUILDING_NOTHING, rr_knows_all).is_empty())
	# The mark names the rung with the SAME glyph the policy picker uses, never a private one.
	# THE RUNG UNDER WAY — the state that used to render nothing. `next_rung_ready` excludes the verb
	# in flight (a patch mid-Cultivate is progress, not an opportunity), which was right and left the
	# in-flight case unmarked, so an actively-cultivated patch looked emptier than an untouched one.
	# **THE GATE'S THIRD ARGUMENT IS THE IMPROVEMENT AXIS** (issue #442), never the harvest stance.
	var rr_building_patch := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"cultivation_progress": 0.42, "sow_site_refusal": "too_dry",
		"composition": [{"can_cultivate": true, "can_sow": false}],
	}
	_assert_hud("building — a patch under Cultivate reports that verb and its meter",
		RungGates.rung_in_progress("forage", rr_building_patch, "cultivate") == \
			{"policy": "cultivate", "glyph": FoodIcons.for_policy("cultivate"), "progress": 0.42})
	# Keyed on the IMPROVEMENT, not on a non-zero meter: a half-built patch nobody is working is a
	# standing rung, which is what the rung glyph is for. The stance says nothing about it either way —
	# passing a stance here answers `{}`, which is the whole point of the split (issue #442).
	_assert_hud("building — the same patch building NOTHING is not building (a meter is not work)",
		RungGates.rung_in_progress("forage", rr_building_patch, RUNG_BUILDING_NOTHING).is_empty())
	_assert_hud("building — a STANCE in the improvement slot answers nothing, never a meter",
		RungGates.rung_in_progress("forage", rr_building_patch, "sustain").is_empty())
	# Each verb names its OWN meter — reading the wrong one would report a confident wrong number.
	_assert_hud("building — Sow reads field_progress, not the cultivation meter beside it",
		float(RungGates.rung_in_progress("forage", {"field_progress": 0.7, "cultivation_progress": 0.2},
			"sow").get("progress", -1.0)) == 0.7)
	_assert_hud("building — Corral reads corral_progress on a herd",
		float(RungGates.rung_in_progress("hunt", {"corral_progress": 0.25, "domestication": 1.0},
			"corral").get("progress", -1.0)) == 0.25)
	# The two answers are mutually exclusive, which is what lets one badge slot carry both states.
	_assert_hud("building and ready are mutually exclusive on one source",
		RungGates.next_rung_ready("forage", rr_building_patch, "cultivate", rr_knows_all).is_empty() \
			and not RungGates.rung_in_progress("forage", rr_building_patch, "cultivate").is_empty())
	_assert_hud("ready — the answer carries the policy's own glyph",
		String(RungGates.next_rung_ready("hunt", rr_tamed_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("glyph", "")) == FoodIcons.for_policy("corral"))
	# THE OFFER TWIN (issue #442) — `next_rung_offered` shares `next_rung_ready`'s ordering and differs
	# on the gate alone, which is the difference between a MARK (promises the verb is available) and the
	# compose CONTROL (teaches that the verb exists and what it costs). A gated rung must therefore be
	# absent from one and present, with its reasons, in the other.
	_assert_hud("offered — an unlearned Cultivate is still OFFERED, so the control can teach it",
		String(RungGates.next_rung_offered("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).get("policy", "")) == "cultivate")
	_assert_hud("…carrying the reason that explains the lock",
		not RungGates.next_rung_offered("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).get("reasons", []).is_empty())
	_assert_hud("offered — the SAME rung is withheld from the READY answer a mark reads",
		RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).is_empty())
	# Highest-first is SHARED, so the sowable-wild-ground answer must agree across both entry points.
	_assert_hud("offered — highest rung first, exactly as the mark orders it",
		String(RungGates.next_rung_offered("forage", rr_wild_sowable, RUNG_BUILDING_NOTHING,
			rr_knows_all).get("policy", "")) == "sow")
	# A species that can never climb is withheld from BOTH — an unreachable prerequisite must never be
	# offered gated, because greying it would imply the lock could be lifted.
	_assert_hud("offered — a wild-ceiling species is offered nothing, gated or otherwise",
		RungGates.next_rung_offered("hunt", rr_forever_wild, RUNG_BUILDING_NOTHING,
			rr_knows_none).is_empty())


	# ---- THE EVENT DOCK (issue #272) ---------------------------------------------------------
	# Its own CanvasLayer, injected here and freed below, so it exists only for the frames that judge
	# it. The reservation is pushed into the HUD by hand — `Main` owns that fan-out and `Main` is
	# never instanced here — so the frames show the HUD reflowing off the reserved strip exactly as
	# it does live.
	_hud.clear_selection()
	_hud._selection._selected_tile_info.clear()
	await _settle()
	var event_dock: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	add_child(event_dock)
	event_dock.reservation_changed.connect(_on_preview_event_dock_reservation)
	await get_tree().process_frame
	_on_preview_event_dock_reservation(event_dock.get_dock(), event_dock.current_reservation_size())
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)

	# THE ROLLBACK REGRESSION. `CommandEventLog` is checkpoint state, so a rollback restores it
	# INCLUDING its `next_seq` counter and the replayed events REUSE sequence numbers the client has
	# already seen. A rollback publishes a FULL frame, which is why `Main` clears the dock on every
	# full snapshot BEFORE applying its events — without that clear the dock suppresses every replayed
	# row as a duplicate `seq` and goes on showing a plausible but stale log, silently. Drive exactly
	# that: a batch, then `reset()` (what the full frame does), then rows REUSING those `seq` values
	# with different labels. The new labels must be what the dock holds.
	event_dock.reset()
	event_dock.ingest_events(_event_dock_rollback_before())
	_assert_hud("rollback: the pre-rollback batch is held",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_BEFORE_LABEL) == 1)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_rollback_after())
	_assert_hud("rollback: a replayed event REUSING a seen `seq` is shown, not swallowed as a duplicate",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_AFTER_LABEL) == 1)
	_assert_hud("rollback: …and the pre-rollback row it replaced is gone, not stacked beside it",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_BEFORE_LABEL) == 0)

	# A `seq` of ZERO is a SENTINEL, not a key: it is the FlatBuffers default and means the row never
	# went through `CommandEventLog::push`. Keyed on, every such row would collide onto one. Two rows
	# that differ only in label must therefore both survive.
	event_dock.reset()
	event_dock.ingest_events(_event_dock_zero_seq_fixture())
	_assert_hud("seq 0 is a sentinel, not a key: two unsequenced rows do not collide",
		event_dock._events.size() == EVENT_DOCK_ZERO_SEQ_ROWS)

	# THE BAND NAME IS THE CLIENT'S. The sim writes a positional `Band <BandId>` because the snapshot
	# carries no band name; the HUD's roster says that band is `Band 1`, and the dock must say so too
	# — bounded at a digit boundary, so a `Band 3` fixture cannot rewrite the `Band 30` beside it.
	event_dock.reset()
	event_dock.set_band_labels(EVENT_DOCK_BAND_LABELS)
	event_dock.ingest_events(_event_dock_band_label_fixture())
	_assert_hud("band label: the sim's positional `Band 3` is re-labelled to the roster's own name",
		_preview_event_label_count(event_dock, EVENT_DOCK_RELABELLED, true) == 1)
	_assert_hud("band label: an id the roster does not know keeps the sim's own label untouched",
		_preview_event_label_count(event_dock, EVENT_DOCK_UNKNOWN_BAND_LABEL, true) == 1)
	_assert_hud("band label: the substitution stops at a DIGIT boundary (`Band 3` ≠ `Band 30`)",
		_preview_event_label_count(event_dock, EVENT_DOCK_DIGIT_BOUNDARY_LABEL, true) == 1)
	event_dock.set_band_labels({})

	# THE PREFS FILE THAT EXISTS BUT HAS NO `[events]` SECTION — i.e. every player upgrading into
	# this build, whose `narrative.cfg` already carries the voice register and `[hud_panels]`. This
	# escaped the first pass because the harness pointed the override at a path that did not exist
	# at all, so `ConfigFile.load` failed and `_load_prefs` returned before it ever read a key.
	# `channels` is the ONLY key whose absence cannot be expressed as a plain default: absent means
	# "every channel on", a stored EMPTY array means the player turned them all off, and collapsing
	# those two is what a naive `[]` default would do. Both branches are walked here.
	_write_event_prefs_without_section()
	event_dock._load_prefs()
	_assert_hud("prefs: an existing file with no [events] section leaves every channel ON",
		_preview_event_channels_all_on(event_dock))
	_write_event_prefs_with_channels([])
	event_dock._load_prefs()
	_assert_hud("prefs: a STORED empty channel list is all-off, not mistaken for an absent key",
		not _preview_event_channels_all_on(event_dock))
	_write_event_prefs_without_section()
	event_dock._load_prefs()

	event_dock.reset()
	event_dock.ingest_events(_event_dock_fixture())
	_assert_hud("seq de-dup: two identical same-turn raids are TWO events, not one",
		_preview_event_kind_count(event_dock, "predator_raid") == EVENT_DOCK_DUPLICATE_RAIDS)
	# And the SIGNATURE fallback still de-dupes a row with no usable `seq`, so a mixed frame cannot
	# duplicate every row every turn. It is a degrade path, not a second mechanism — it carries the
	# old collapse-two-identical-rows bug for exactly the rows that give it no better key.
	var seqless := [{"tick": 47, "kind": "forage", "label": "A row with no seq", "detail": ""}]
	event_dock.ingest_events(seqless)
	event_dock.ingest_events(seqless)
	_assert_hud("seq de-dup: a row carrying no usable seq still falls back to the signature",
		_preview_event_label_count(event_dock, "A row with no seq") == 1)

	# The client's own System-channel note — the Inspector's console chatter routed onto the dock,
	# in the shape `Inspector._append_command_log` emits it (the LINE is the label; there is no
	# separate detail, or the only words that matter end up at the far end of the bar).
	event_dock.note_system("Command socket lost — reconnecting", "", true)

	# event_dock_bottom — THE SHIPPED DEFAULT: bottom edge, 2 rows, the `notable` floor. Opened and
	# closed first, which is what a player does and what marks the alerts read, so this frame shows
	# the plain newest-first bar rather than the pinned one (that is its own state below).
	event_dock.set_expanded(true)
	event_dock.set_expanded(false)
	await _settle()
	await _save("event_dock_bottom")
	# A BIRTH IS VISIBLE AT THE DEFAULT FLOOR. `born` shipped Routine, which is BELOW
	# `DEFAULT_DETAIL_LEVEL`, so it never appeared unless the player chose "Everything" — reported
	# live as a population counter ticking up while the bar said nothing. A rung table is one dict
	# entry away from that regression at any time, so it is asserted rather than eyeballed.
	_assert_hud("a birth passes the DEFAULT detail floor (`born` is Notable, not Routine)",
		HudEventVocab.DETAIL_FLOOR[HudEventVocab.DEFAULT_DETAIL_LEVEL].has(
			String(HudEventVocab.RUNG_BY_KIND["born"])))

	# **NOTHING DOCKED LEFT OR RIGHT, AND THE BAR STILL CLEARS THE HUD'S OWN FURNITURE.** This is the
	# case the first inset fix got wrong: it bounded the bar against edge RESERVERS, and the left
	# dock, the right dock and the top-bar readout block are not reservers. Reported live as a bar
	# sitting over `Turn N` / `Units` / `Pop`.
	#
	# **EACH CLAIM IS MADE WHERE IT IS NON-VACUOUS, and `_assert_bar_clears` enforces that.** The
	# HUD's regions occupy different vertical bands, so most bar/region pairs never share any y at
	# all and "they do not overlap" is true of them for free: a BOTTOM bar sits in the BottomBar's
	# band (nav backing + turn orb), a TOP bar in the TopBar's (the readout block), and only a bar
	# tall enough to reach the ContentRow can touch the two docks. Asserting the wrong pair passes
	# with the fix reverted — which is exactly what the first version of this block did.
	_assert_bar_clears(event_dock, _hud.nav_backing, "the bottom-left nav backing (minimap + zoom rail)")
	_assert_bar_clears(event_dock, _hud.turn_orb, "the bottom-right turn orb")
	# THE COLUMNS ARE AUTHORED, AND THE BAR IS BOUNDED BY THE AUTHORED NUMBER. If a card or a metrics
	# string ever outgrows its column, the live rect passes the authored width and the bar's bound is
	# a lie — so it fails HERE rather than by overlapping in play.
	_assert_hud("the LEFT column renders no wider than the authored width the bar is bounded by (%.0f)"
			% _hud.left_column_width(),
		_hud.left_dock_region.get_global_rect().size.x <= _hud.left_column_width())
	_assert_hud("the RIGHT dock renders no wider than the authored column (%.0f)" % _hud.right_column_width(),
		_hud.right_dock_region.get_global_rect().size.x <= _hud.right_column_width())
	_assert_hud("…and so does the readout block, which has no authored width of its own by default",
		_hud.turn_block.get_global_rect().size.x <= _hud.right_column_width())

	# THE REPORTED CASE ITSELF: a TOP bar shares the TopBar's vertical band with the readout block,
	# which is where `Turn N` / `Units` / `Sedentarization` / `Pop` live. Nothing is docked left or
	# right, so a bound that only knew about reservers puts the bar straight over them.
	event_dock.set_dock(SIDE_TOP)
	await _settle()
	_assert_bar_clears(event_dock, _hud.turn_block, "the top-bar readout block (Turn / Units / Pop)")

	# **NOTHING MOVES DOWN**, and this is asserted HERE — with the dock actually on `SIDE_TOP` —
	# because `offset_top` is the offset a TOP strip would move and the dock spends most of this
	# block on the bottom edge. Made against the wrong edge the claim is true for free, which is the
	# same vacuity `_assert_bar_clears` guards against. A picture cannot carry it either: a frame
	# with the readouts 60px lower still looks like a HUD. `Main` keeps the dock out of the HUD's
	# registry entirely (`MAP_ONLY_RESERVERS`), so `LayoutRoot` keeps its full height on every edge.
	_assert_hud("the dock's SIDE_TOP reservation does NOT push the HUD down (LayoutRoot offsets stay 0)",
		is_zero_approx(_hud.layout_root.offset_top) and is_zero_approx(_hud.layout_root.offset_bottom))
	# THE NEGATIVE CONTROL: the mechanism works, it is the event dock that is exempt from it. The same
	# size pushed under a DIFFERENT id does move the HUD — so the claim above is about the exemption
	# and not about a reserved-inset path that quietly does nothing.
	_hud.set_reserved_inset(&"preview_probe", SIDE_TOP, event_dock.current_reservation_size())
	await _settle()
	_assert_hud("…and that is an EXEMPTION, not a broken path: another reserver's SIDE_TOP strip does move it",
		_hud.layout_root.offset_top > 0.0)
	_hud.set_reserved_inset(&"preview_probe", SIDE_TOP, 0.0)
	await _settle()

	# A bar tall enough to reach the ContentRow is the only one that can touch the two DOCKS, so the
	# expanded log on the bottom edge is where that pair is asserted.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_expanded(true)
	await _settle()
	_assert_bar_clears(event_dock, _hud.left_dock_region, "the HUD's LEFT dock")
	_assert_bar_clears(event_dock, _hud.right_dock_region, "the HUD's RIGHT dock")
	event_dock.set_expanded(false)
	await _settle()

	# event_dock_top_expanded — the OTHER edge, log open. Two claims: the bar reads as a one-line
	# title and NOT as a second copy of the log's newest turn-group (the failure the prototype made
	# unmissable), and the log opens INWARD from the top edge with the bar still hugging it.
	event_dock.set_dock(SIDE_TOP)
	event_dock.set_expanded(true)
	await _settle()
	await _save("event_dock_top_expanded")
	_assert_hud("expanded: the bar is ONE row, not a reprint of the log's newest turn-group",
		event_dock._rows.get_child_count() == 1)

	# event_dock_everything_expanded — the `routine` floor, i.e. every receipt the retired feed used
	# to carry, with the log open. This is the state the strip could eat the map in, so it is where
	# the yield cap is asserted.
	event_dock.set_detail_level(HudEventVocab.RUNG_ROUTINE)
	await _settle()
	await _save("event_dock_everything_expanded")

	# event_dock_alerts_only — the quietest setting on the narrowest bar: one row, alerts only. The
	# `status=feral` row must be here (a `cultivate` kind PROMOTED to Alert by its detail token) and
	# every routine receipt must be gone.
	event_dock.set_expanded(false)
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_recent_count(1)
	event_dock.set_detail_level(HudEventVocab.RUNG_ALERT)
	await _settle()
	await _save("event_dock_alerts_only")

	# event_dock_pinned_alert — 4 rows at the `notable` floor over a FRESH ingest, so the alerts are
	# unread again. Turn 47's raid is inside the window; the pin is judged on the deeper one, so the
	# fixture's alerts sit far enough back that the newest four rows cannot contain them.
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_detail_level(HudEventVocab.RUNG_NOTABLE)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_pin_fixture())
	await _settle()
	await _save("event_dock_pinned_alert")
	_assert_hud("pinned alert: the unread raid holds the LEADING slot",
		event_dock._pinned_order >= 0)

	# ---- THE BAR LIVES BETWEEN THE VERTICAL DOCKS -------------------------------------------
	# Reported live: a `SIDE_TOP` bar spanning the full window, drawn at layer 104 over the
	# `SIDE_LEFT` band panel at 103, covering its tab bar. `RESERVER_PRIORITY` cannot fix that —
	# it orders reservers stacked ALONG one edge, and TOP and LEFT are not co-edge — so the bar's
	# own EXTENT is pulled in by the live left/right reservation totals instead.
	#
	# A REAL `BandCityPanel` supplies the number. A literal would prove nothing about the two
	# rects actually clearing each other, which is the whole claim.
	event_dock.set_expanded(false)
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_dock(SIDE_TOP)
	var inset_panel: BandCityPanel = BAND_CITY_PANEL_SCENE.instantiate()
	add_child(inset_panel)
	await get_tree().process_frame
	inset_panel.set_dock(SIDE_LEFT)
	var left_reserved: float = inset_panel.current_reservation_size()
	_hud.set_reserved_inset(&"band_panel", SIDE_LEFT, left_reserved)
	await _settle()
	# The band panel DOES inset the HUD, so its left dock now sits inside the reserved strip — which
	# is why the two terms ADD rather than compete.
	var expected_left: float = left_reserved + _hud.left_column_width()

	# THE NEGATIVE CONTROL, taken FIRST and against the same two live nodes: with the insets at zero
	# the rects really do overlap. So the assertion below is not satisfiable by two panels that
	# happen never to meet, and the state it describes is reachable rather than hypothetical.
	event_dock.set_perpendicular_insets(0.0, 0.0)
	await _settle()
	_assert_hud("inset control: at zero inset the bar genuinely DOES overlap a left-docked panel",
		event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect()))

	_preview_push_event_dock_insets(event_dock, left_reserved, 0.0)
	await _settle()
	await _save("event_dock_inset_left_panel")
	_assert_hud("inset: the top bar starts past the left-docked panel AND the HUD's own left dock (%.0f + %.0f)"
			% [left_reserved, _hud.left_column_width()],
		is_equal_approx(event_dock._root.offset_left, expected_left))
	_assert_hud("inset: …and it overlaps neither the docked panel nor the HUD's left dock",
		not event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect())
			and not event_dock._root.get_global_rect().intersects(_hud.left_dock_region.get_global_rect()))
	_assert_hud("inset: …and it still clears the readout block on the far side",
		not event_dock._root.get_global_rect().intersects(_hud.turn_block.get_global_rect()))
	_assert_hud("inset: the reservation the bar PUBLISHES is unchanged — this moves where it is drawn, not what it claims",
		is_equal_approx(event_dock.current_reservation_size(), event_dock._bar_height()))

	# The BOTTOM edge takes the same inset — the bug was about the horizontal axis, so both edges
	# must be fixed and a fix that only reached `SIDE_TOP` has to fail here.
	event_dock.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("event_dock_inset_bottom_panel")
	_assert_hud("inset: the BOTTOM bar takes the same bound",
		is_equal_approx(event_dock._root.offset_left, expected_left)
			and not event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect()))

	_hud.set_reserved_inset(&"band_panel", SIDE_LEFT, 0.0)
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)
	inset_panel.queue_free()
	await get_tree().process_frame
	await _settle()

	# ---- NO RAW WIRE TOKEN EVER REACHES A ROW -------------------------------------------------
	# The defect: rows printed the sim's detail verbatim, so one read `category=settle_site at
	# (64,36)`. Stated as the GENERAL property rather than spot-checking three strings — every Label
	# the dock renders, bar and log, must be free of `=`. The two guards under it are what stop that
	# being vacuous: the walk must have seen labels at all, and the pool must actually CONTAIN a raw
	# `=` for one to have been able to leak.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_detail_level(HudEventVocab.RUNG_ROUTINE)
	event_dock.set_expanded(true)
	await _settle()
	var raw_tokens := 0
	for event in event_dock._events:
		if String(event["detail"]).contains("="):
			raw_tokens += 1
	_assert_hud("precondition: the pool really does hold raw `key=value` details (%d of them)" % raw_tokens,
		raw_tokens > 0)
	var scanned := 0
	var leaked := ""
	for label in _preview_dock_labels(event_dock):
		scanned += 1
		if label.contains("=") and leaked == "":
			leaked = label
	_assert_hud("precondition: the scan actually walked the rendered rows (%d labels)" % scanned,
		scanned > 0)
	_assert_hud("no rendered row carries a raw wire token — %d labels scanned, worst offender %s"
			% [scanned, "none" if leaked == "" else "\"%s\"" % leaked],
		leaked == "")

	# **NO RENDERED DETAIL CARRIES A TRAILING-ZERO DECIMAL.** The sim writes casualties with `{:.3}`,
	# which is honest on the wire (a `Scalar` really can be fractional) and DEBUG OUTPUT on a
	# notification bar — `Killed 2.000` is a float where the player is owed a count. Stated as the
	# general property, like the `=` one, and guarded the same way: the pool must actually hold a
	# `.000` for one to have reached the screen.
	# Re-seeded so the casualty rows are on the NEWEST turns and therefore inside the log's window.
	# THIS MATTERS: the pin fixture that ran before this put its raid seven turns back, outside the
	# five the log shows, so the scan walked rows that never had a padded number in them and passed
	# with the trim reverted. The precondition below counts the POOL, so it cannot catch that on its
	# own — the scan has to cover the whole pool too.
	event_dock.reset()
	event_dock.ingest_events(_event_dock_fixture())
	await _settle()
	var padded_wire := 0
	for event in event_dock._events:
		if String(event["detail"]).contains(".000"):
			padded_wire += 1
	_assert_hud("precondition: the pool really does hold `{:.3}` wire numbers (%d of them)" % padded_wire,
		padded_wire > 0)
	# TWO scans, and the second is what makes the first honest. The rendered labels are what the
	# player actually sees; `detail_phrase` over EVERY retained event is the complete property, and it
	# cannot go vacuous by an event drifting out of the log's five-turn window.
	var padded := ""
	for label in _preview_dock_labels(event_dock):
		if _has_padded_decimal(label) and padded == "":
			padded = label
	for event in event_dock._events:
		var phrase := EventDockPanel.detail_phrase(String(event["detail"]))
		if _has_padded_decimal(phrase) and padded == "":
			padded = phrase
	_assert_hud("no detail renders with a trailing-zero decimal, on screen or in the pool — worst offender %s"
			% ("none" if padded == "" else "\"%s\"" % padded),
		padded == "")
	# **THE TRIM IS NOT A ROUND**, and this is the assertion that stops someone "simplifying" it into
	# an `int()`. A casualty count reading `2` when the sim said `1.5` is a lie the player cannot
	# detect, so a genuinely fractional value has to survive intact.
	_assert_hud("a fractional wire number survives UN-ROUNDED (`wounded=1.750` -> `%s`)"
			% EventDockPanel.detail_phrase("wounded=1.750"),
		EventDockPanel.detail_phrase("wounded=1.750") == "Wounded 1.75")
	_assert_hud("…while a whole one loses its padding (`wounded=2.000` -> `%s`)"
			% EventDockPanel.detail_phrase("wounded=2.000"),
		EventDockPanel.detail_phrase("wounded=2.000") == "Wounded 2")
	# A bare integer must not be touched — `rstrip("0")` on `100` would answer `1`, which the trim
	# avoids only by returning early when there is no decimal point at all.
	_assert_hud("…and a whole number with trailing zeros is left ALONE (`warriors=100` -> `%s`)"
			% EventDockPanel.detail_phrase("warriors=100"),
		EventDockPanel.detail_phrase("warriors=100") == "Warriors 100")
	_assert_hud("the LABEL's own casualty count is not repeated beside it (`killed=3.000 wounded=1.000` -> `%s`)"
			% EventDockPanel.detail_phrase("killed=3.000 wounded=1.000"),
		EventDockPanel.detail_phrase("killed=3.000 wounded=1.000") == "Wounded 1")

	# AN UNKNOWN KEY AND AN UNKNOWN VALUE STILL RENDER AS ENGLISH. The sim adds kinds and tokens with
	# no schema change, so a token with no table row is the COMMON case over time — the generic
	# fallback is what makes a raw identifier on screen impossible by construction rather than by
	# anyone remembering to add a row. Asserted on `detail_phrase` directly: a rendered row would also
	# pass while silently dropping the fragment, which is the other way to get this wrong.
	_assert_hud("unknown VALUE renders as English (`quarry_state=half_eaten` -> `%s`)"
			% EventDockPanel.detail_phrase("quarry_state=half_eaten"),
		EventDockPanel.detail_phrase("quarry_state=half_eaten") == "Half eaten")
	_assert_hud("unknown NUMERIC key keeps its key (`spoiled_units=7` -> `%s`)"
			% EventDockPanel.detail_phrase("spoiled_units=7"),
		EventDockPanel.detail_phrase("spoiled_units=7") == "Spoiled units 7")
	_assert_hud("the reported row renders as prose (`category=settle_site at (64,36)` -> `%s`)"
			% EventDockPanel.detail_phrase("category=settle_site at (64,36)"),
		EventDockPanel.detail_phrase("category=settle_site at (64,36)") == "Settle site · (64, 36)")
	_assert_hud("a value containing a SPACE survives the token walk (`species=Grey Wolf`)",
		EventDockPanel.detail_phrase("killed=2.000 species=Grey Wolf").ends_with("Grey Wolf"))
	_assert_hud("keys the LABEL already carries are dropped (`band=3 count=4 direction=out` -> `%s`)"
			% EventDockPanel.detail_phrase("band=3 count=4 direction=out"),
		EventDockPanel.detail_phrase("band=3 count=4 direction=out") == "departed")
	event_dock.set_expanded(false)
	event_dock.set_detail_level(HudEventVocab.RUNG_NOTABLE)
	await _settle()

	# ---- THE ULTRAWIDE CAP --------------------------------------------------------------------
	# The configuration the complaint came from, and one nothing else in this set reaches: the bar
	# spanned the whole band, so a row's label sat at one end of two feet of screen and its detail at
	# the other. BOTH halves are asserted, because a cap hard-wired on would fail the narrow case and
	# one hard-wired off would fail the wide one.
	var band_now: float = float(PREVIEW_CANVAS_SIZE_BASE.x) - event_dock._inset_left - event_dock._inset_right
	_assert_hud("below the cap the strip fills the band exactly as before (%.0f of %.0f available)"
			% [event_dock._root.size.x, band_now],
		is_equal_approx(event_dock._root.size.x, band_now) and band_now < EventDockPanel.MAX_STRIP_WIDTH)

	get_window().size = ULTRAWIDE_WINDOW_SIZE
	await get_tree().process_frame
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame
	var wide_band: float = event_dock._viewport_size().x - event_dock._inset_left - event_dock._inset_right
	_assert_hud("precondition: the ultrawide band (%.0f) is genuinely wider than the cap (%.0f)"
			% [wide_band, EventDockPanel.MAX_STRIP_WIDTH],
		wide_band > EventDockPanel.MAX_STRIP_WIDTH)
	_assert_hud("at ultrawide the strip stops at the cap (%.0f) instead of spanning the band (%.0f)"
			% [event_dock._root.size.x, wide_band],
		is_equal_approx(event_dock._root.size.x, EventDockPanel.MAX_STRIP_WIDTH))
	var lead_gap: float = event_dock._root.offset_left - event_dock._inset_left
	var trail_gap: float = event_dock._viewport_size().x - event_dock._inset_right - event_dock._root.offset_right
	_assert_hud("…and it is CENTRED in the band, not pinned to an edge (%.0f leading / %.0f trailing)"
			% [lead_gap, trail_gap],
		is_equal_approx(lead_gap, trail_gap))
	var wide_image := get_viewport().get_texture().get_image()
	if wide_image != null:
		wide_image.save_png("%s/event_dock_ultrawide.png" % OUT_DIR)
		print("ui_preview: saved event_dock_ultrawide.png")
	_pin_canvas(get_window())
	await _settle()

	# THE STRIP YIELDS TO THE MAP. Both ways the dock can grow — the widest BAR (`RECENT_COUNT_MAX`
	# rows, log closed) and the LOG open (which collapses the bar to one title line) — must leave the
	# reserved strip inside `MAX_STRIP_HEIGHT_FRACTION` of the canvas, which is what leaves a usable
	# viewport. Measured as a PAIR because the bar and the log are alternatives, not addends, so
	# neither one alone is the worst case by inspection. A picture cannot carry this claim at all: a
	# strip that had eaten 90% of the screen would still render as a plausible bar.
	var widest_bar := event_dock.current_reservation_size()
	event_dock.set_expanded(true)
	await _settle()
	var open_log := event_dock.current_reservation_size()
	var strip_cap := float(PREVIEW_CANVAS_SIZE.y) * EventDockPanel.MAX_STRIP_HEIGHT_FRACTION
	_assert_hud("the strip yields to the map: %d rows = %.0f px, log open = %.0f px, cap %.0f of a %d px canvas"
			% [EVENT_DOCK_MAX_ROWS, widest_bar, open_log, strip_cap, PREVIEW_CANVAS_SIZE.y],
		maxf(widest_bar, open_log) <= strip_cap)

	event_dock.reservation_changed.disconnect(_on_preview_event_dock_reservation)
	_hud.set_reserved_inset(&"event_dock", SIDE_BOTTOM, 0.0)
	event_dock.queue_free()
	await get_tree().process_frame
	await _settle()

	# Icon probe last, on a top layer with its own backdrop (rendering is warm by
	# now), so every food glyph is captured via the map's draw path.
	var probe_layer := CanvasLayer.new()
	probe_layer.layer = 100
	add_child(probe_layer)
	var probe_bg := ColorRect.new()
	probe_bg.color = Color(0.06, 0.09, 0.10)
	probe_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	probe_layer.add_child(probe_bg)
	var probe := preload("res://tools/icon_probe.gd").new()
	probe_layer.add_child(probe)
	await _settle()
	await _save("food_icons")

	# The herd field-pair guard's verdict, ONE line for the whole run (each violation has already been
	# push_error'd against the frame it rendered in). The scanned count is part of the claim: a guard
	# that walked nothing would pass vacuously, and "0 herd dicts scanned" says so out loud.
	_assert_hud("every herd fixture keeps the herders_needed pair consistent (%d herd dicts carrying it)"
		% _herd_pair_scans, _herd_pair_violations == 0)

	get_tree().quit()

## Victory progress shaped as `Hud._refresh_victory_status` consumes it: no winner declared yet and
## a few modes at differing progress, so the card has real height when it is toggled on and the
## progress sort (highest first) is visible.
func _victory_state_fixture() -> Dictionary:
	return {
		"winner": {},
		"modes": [
			{"id": "cultural_ascendancy", "progress_pct": 0.42, "achieved": false},
			{"id": "great_works", "progress_pct": 0.18, "achieved": false},
			{"id": "hegemony", "progress_pct": 0.06, "achieved": false},
		],
	}

## Open / close the Terrain Types legend around a block of legend states.
##
## The card ships SUPPRESSED, so a legend state must open it — and every legend state MUST close it
## again at the end of its own block. An earlier cut opened it once and restored it ~700 lines later,
## which meant a dozen intervening states silently rendered with a non-default right dock and NO
## state anywhere exercised the shipped default. That is precisely how a default-visibility bug
## hides, so scope stays tight and local.
##
## Set through the controller rather than `Hud.toggle_legend`, which would PERSIST the choice to the
## prefs file this harness clears at startup — a harness must not write the preference it is testing.
func _open_legend() -> void:
	_hud._legend.set_suppressed(false)

func _close_legend() -> void:
	_hud._legend.set_suppressed(true)

## Six narrative beats in the `mythic` register, transcribed VERBATIM from the authored copy in
## `core_sim/src/data/beat_definitions.json` with their nouns filled in as the sim would fill them.
## Real copy, not lorem: the panel's whole job is prose, and placeholder text of the wrong length
## would make both the wrapping and the density read wrong.
##
## The first entry is `cold_open.bone_ground` — the LONGEST line in the catalog (225 chars) — so
## the multi-line wrap case is exercised in every telling frame rather than by luck.
func _telling_fixture_events() -> Array:
	return [
		{"tick": 0, "kind": "narrative_beat",
			"label": "We are 24. The ground behind us is bone, and we will not go back to it. Ahead lies a country with no names — not the hills, not the waters, not the years to come. Naming it is your work now. Walk well, and be remembered.",
			"detail": "turn.index = 0 · band.count = 24"},
		{"tick": 3, "kind": "narrative_beat",
			"label": "The scouts came back thinner and louder than they left. Salt Pillar Reach, they said, over and over, until we all knew the word.",
			"detail": "sites.discovered_this_turn = 1"},
		{"tick": 9, "kind": "narrative_beat",
			"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
			"detail": "provisions.total falling for 3 turns"},
		{"tick": 14, "kind": "narrative_beat",
			"label": "A woman pressed seed into the mud to see what it would do. The mud answered. We know a new thing.",
			"detail": "knowledge.cultivation = 1.00"},
		{"tick": 18, "kind": "narrative_beat",
			"label": "The chase is longer every season and ends in less. The aurochs were the road we walked; the road is going quiet under us.",
			"detail": "herd.ecology_phase = collapsing"},
		{"tick": 22, "kind": "narrative_fork",
			"label": "There are paths here now, worn by our own feet, going to places only we go. That is how a country becomes a home, or a trap.",
			"detail": "sedentarization.score = 41"},
	]

## TWO beats sharing ONE tick — a single speaking turn that said two things, so they form ONE page.
## Reproduces the playtest bug (the fixed-height page scrolled the second beat off instead of growing).
func _telling_two_beat_oral_fixture() -> Array:
	return [
		{"tick": 6, "kind": "narrative_beat",
			"label": "We have stopped catching rabbits and started keeping them. A fence, a little grass, and they breed under our own eyes.",
			"detail": "husbandry.penning = 0.34"},
		{"tick": 6, "kind": "narrative_beat",
			"label": "We are more now than we were when we left the bone ground. The children born on this road have never slept anywhere else.",
			"detail": "band.count = 31"},
	]

## TWO tall pages (ticks 0 and 1, seven distinct long beats each) that BOTH overflow `PAGE_MAX_HEIGHT`, so
## the inner ScrollContainer actually scrolls — the fixture the yields-to-reader scroll test needs (a page
## that fits the cap can hold no non-zero scroll offset to preserve).
func _telling_tall_pages_fixture() -> Array:
	var out: Array = []
	var long := "The chase is longer every season and ends in less; the aurochs were the road we walked, and the road is going quiet under our own feet."
	for tick in [0, 1]:
		for i in range(7):
			out.append({"tick": tick, "kind": "narrative_beat",
				"label": "%d. %s" % [i, long], "detail": "beat %d of tick %d" % [i, tick]})
	return out

## Ordinary command receipts for the split frame — the transactional acknowledgements that used to
## be pushed off the feed by two beats. Deliberately MORE than one, so "the feed is legible again"
## is something the frame can actually show rather than imply.
func _telling_command_receipts() -> Array:
	return [
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "6 foragers → (27, 26)"},
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "3 hunters → Aurochs Herd"},
		{"tick": 23, "kind": "command", "label": "Move band", "detail": "Band 1 → (28, 25)"},
		{"tick": 23, "kind": "site_discovered", "label": "Salt Pillar Reach", "detail": "Wondrous site at (31, 22)"},
	]

## Settle the HUD for a capture. `finish_tweens = false` is for the ONE state that must capture a page
## turn IN MOTION; it steps the tween itself so the phase is chosen rather than raced.
func _settle(finish_tweens: bool = true) -> void:
	await _ensure_canvas()
	if finish_tweens:
		_flush_tweens()
	await get_tree().process_frame
	# Force a synchronous frame rather than awaiting `RenderingServer.frame_post_draw`.
	# Under the dummy rendering backend (which `--headless` selects on Godot 4.5) no
	# real draw ever posts, so that await never returns and the harness hangs. force_draw
	# just no-ops there, so a stray headless run fails fast in `_save` instead of hanging.
	RenderingServer.force_draw()
	await get_tree().process_frame

## Drive every live tween to its END state. THIS IS WHAT MAKES THE TIME FREEZE SAFE: a shader's `TIME`
## still evaluates at phase 0, but a Tween at `time_scale = 0` never advances AT ALL, so a page turn
## would be pinned at `progress = 0` — the OUTGOING page fully opaque and the incoming one at alpha 0,
## i.e. a frame that renders the page BEFORE the turn it exists to show. Stepping past the duration
## also fires the finished-callback, so the panel's own `_end_turn` settle runs exactly as it does live.
func _flush_tweens() -> void:
	for tween in get_tree().get_processed_tweens():
		if tween.is_valid() and tween.is_running():
			tween.custom_step(TWEEN_FLUSH_SECONDS)

## Advance every live tween by a FIXED slice — a deliberately chosen mid-motion phase, for the state
## that captures a page turn in flight. Deterministic because the clock contributes nothing.
func _step_tweens(seconds: float) -> void:
	for tween in get_tree().get_processed_tweens():
		if tween.is_valid() and tween.is_running():
			tween.custom_step(seconds)

## Hold the window at the pinned canvas. Deliberately does NOT touch `content_scale_size` /
## `content_scale_factor` (the same call `map_preview` makes): `project.godot` stretches `canvas_items`
## with an `expand` aspect, so pinning those would re-project every frame — a mass pixel change, not a
## race fix. The race is a window mode/size problem.
func _pin_canvas(win: Window) -> void:
	win.mode = Window.MODE_WINDOWED
	win.size = PREVIEW_CANVAS_SIZE

## Hold the window at the pinned canvas and WAIT for the WM to honour it, before anything is captured.
## `project.godot` opens MAXIMIZED and macOS applies — and RE-applies — that asynchronously, many
## frames in, so the bare `get_window().size = …` this harness used to do in `_ready` was a RACE that
## did not stay won. Measured on two clean runs of identical code: one came back with 177 of its 184
## frames at the monitor's 5120x1410 instead of the intended 1500x900 while the other rendered all 184
## at 1500x900 — so the two runs disagreed on the HUD's LAYOUT, not merely its pixels, and most frames
## were being judged at a width the HUD never ships at.
func _ensure_canvas() -> void:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		if get_window().size == PREVIEW_CANVAS_SIZE and get_window().mode == Window.MODE_WINDOWED:
			return
		_pin_canvas(get_window())
		await get_tree().process_frame

## Settle the window ONCE, in `_ready`, before any state renders — and take the maximize DELIBERATELY
## on the way, which is what closes the last of the drift.
##
## `project.godot` opens the window MAXIMIZED and macOS applies that asynchronously, so whether a run
## ever passed through the monitor-sized window was a COIN FLIP — and it is a coin flip the pixels
## remember. Measured over four runs with the clock already frozen and the canvas pinned: runs that
## never maximized and runs that did formed two byte-DISTINCT clusters, differing by ±1 on the
## antialiased edges of ~85 frames (`window/stretch` is `canvas_items` with an `expand` aspect, so the
## stretch scale swings 0.78 → 2.67 → 0.78 across a maximize and the rasterized-glyph/coverage state
## does not come back bit-identical). Dodging the maximize is not available — a late one still landed
## mid-run after 30 stable frames — so ASK for it, then undo it: every run now takes the same path.
## Four consecutive runs came back 184/184 byte-identical, with zero mid-run re-pins.
func _stabilize_canvas() -> void:
	get_window().mode = Window.MODE_MAXIMIZED
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size != PREVIEW_CANVAS_SIZE:
			break
		await get_tree().process_frame
	# Restore and HOLD: the maximize is re-applied asynchronously, so "the right size once" is not the
	# same as "it stays" — wait for CANVAS_STABLE_FRAMES consecutive good frames. After this every
	# `_ensure_canvas` returns without awaiting, so each state gets the same number of layout passes.
	var stable := 0
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size == PREVIEW_CANVAS_SIZE and get_window().mode == Window.MODE_WINDOWED:
			stable += 1
			if stable >= CANVAS_STABLE_FRAMES:
				return
		else:
			stable = 0
			_pin_canvas(get_window())
		await get_tree().process_frame
	push_error("ui_preview: the window never held the pinned %s canvas — frames will drift" % PREVIEW_CANVAS_SIZE)

## The viewport image, GUARANTEED to be the pinned canvas (or an integer HiDPI multiple of it). The
## WM's deferred maximize can resize the render target between a settle and a capture, so re-pin and
## re-draw until the geometry is the canvas's, then give up loudly rather than save a bad frame. With
## `content_scale_*` deliberately unpinned the captured image matches the WINDOW, not the viewport's
## logical `expand` rect — so the guard measures against the window-sized canvas.
func _capture(name: String) -> Image:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			# No image to read back — the dummy renderer (i.e. someone ran this with
			# `--headless`, which selects it on Godot 4.5). Capture is impossible, but
			# the compile/scene gate still passed. Run WITHOUT `--headless` for PNGs.
			push_warning("ui_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
			return null
		var w := image.get_width()
		var h := image.get_height()
		if w % PREVIEW_CANVAS_SIZE.x == 0 and h % PREVIEW_CANVAS_SIZE.y == 0 \
				and w / PREVIEW_CANVAS_SIZE.x == h / PREVIEW_CANVAS_SIZE.y:
			return image
		_pin_canvas(get_window())
		await get_tree().process_frame
		RenderingServer.force_draw()
		await get_tree().process_frame
	push_error("ui_preview: viewport never came back to the pinned %s canvas for %s" % [PREVIEW_CANVAS_SIZE, name])
	return null

func _save(name: String) -> void:
	# Check the herd fixtures RENDERING IN THIS FRAME, so a half-set field pair fails against the state
	# it silently mis-renders rather than against nothing at all.
	_guard_frame_herd_fields(name)
	var image: Image = await _capture(name)
	if image == null:
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("ui_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("ui_preview: saved ", name, ".png")

## Walk the open reasons popover to its `Advance ▸` footer button (last body row's child).
func _turn_orb_advance_button() -> Button:
	var pop := _hud.turn_orb._popover
	if pop == null or pop.get_child_count() == 0:
		return null
	var body := pop.get_child(0)
	if body.get_child_count() == 0:
		return null
	var footer := body.get_child(body.get_child_count() - 1)
	if footer.get_child_count() == 0:
		return null
	var btn := footer.get_child(0)
	return btn as Button

## **THE RENDERED reason rows of the open popover**, in the order they are drawn, each as
## `{label, detail}` read off the two Labels themselves — never off `TurnOrb._entries`. A registry read
## would pass on a row the popover never drew, and it would also skip the sort `set_attention` applies,
## so a claim about which row sits ABOVE which could not be made against it. The popover body is a
## header, one Button per entry, and a footer whose Advance button is nested one level deeper — so the
## body's DIRECT Button children are exactly the reason rows.
func _orb_rows() -> Array:
	var rows: Array = []
	var pop := _hud.turn_orb._popover
	if pop == null or pop.get_child_count() == 0:
		return rows
	for row_node in pop.get_child(0).get_children():
		if not (row_node is Button) or row_node.get_child_count() == 0:
			continue
		# The row is stripe · icon · text stack · jump, and the text stack is the only VBox in it, so
		# the label/detail pair is reached structurally rather than by counting siblings.
		for cell in row_node.get_child(0).get_children():
			if not (cell is VBoxContainer) or cell.get_child_count() < 2:
				continue
			rows.append({
				"label": String((cell.get_child(0) as Label).text),
				"detail": String((cell.get_child(1) as Label).text),
			})
			break
	return rows

## The rendered row whose label is EXACTLY `label`, or `null`. Rows are found by the words the player
## reads, so a producer that fired with different text is a miss rather than a silent match.
func _orb_row_with(rows: Array, label: String) -> Variant:
	for row_variant in rows:
		var row: Dictionary = row_variant
		if String(row["label"]) == label:
			return row
	return null

const DIGIT_CHARACTERS := "0123456789"

## Does this rendered string carry ANY digit? The "renders no countdown at all" claim is asserted on
## DIGITS rather than on an absent phrase, so no rewording of a number — `0`, `in 0 turns`, `0 left` —
## can satisfy it.
func _contains_digit(text: String) -> bool:
	for i in text.length():
		if DIGIT_CHARACTERS.contains(text[i]):
			return true
	return false

## The Telling: a pending fork on the wire, in the per-faction shape the native decoder produces
## (`[{faction, forks: [...]}]`). Copy is verbatim from beat_definitions.json —
## `sedentarization.soft_drift` / `soft_drift.long_chase` — with `{beast.plural}` resolved the way
## the sim resolves nouns at post time, so the frame judges REAL prose at REAL length.
func _pending_forks_fixture() -> Array:
	return [{
		"faction": 0,
		"forks": [{
			"beat_id": "sedentarization.soft_drift",
			"wardrobe_id": "soft_drift.long_chase",
			"posted_tick": 41,
			"narration": [
				{"register": FORK_REGISTER_MYTHIC, "text": "Three seasons, and each one we chased the mammoths and left the seed-ground unturned. The children do not remember a walled night. At the fires, they have begun to call us the People of the Long Chase. Is that who we are?"},
				{"register": FORK_REGISTER_WARM, "text": "Three seasons now, all of them spent following the mammoths, and nobody's turned the seed-ground once. The children have never slept behind a wall. People have started calling us the People of the Long Chase. Is that us?"},
			],
			"choices": [
				{"choice_id": "yes_trail", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We are the trail"},
					{"register": FORK_REGISTER_WARM, "text": "Yes — we're trail people"},
				]},
				{"choice_id": "no_root", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We were meant to root"},
					{"register": FORK_REGISTER_WARM, "text": "No — we were meant to settle"},
				]},
				# Exactly one choice carries is_defer, and the SERVER computes it — the client reads
				# the flag and never re-derives which choice writes nothing.
				{"choice_id": "defer", "is_defer": true, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "Say nothing"},
					{"register": FORK_REGISTER_WARM, "text": "Let it lie for now"},
				]},
			],
			"gloss": [
				{"signal": "sedentarization.score", "value": 41.0},
				{"signal": "stance.roam_settle", "value": -0.18},
			],
		}],
	}]

func _stance_axes_fixture() -> Array:
	return [{"faction": 0, "axes": [{"axis": "roam_settle", "value": -0.18}]}]

## Open the COMPOSE SHEET on a source and render its compose block there.
##
## Part 2 of docs/plan_tile_panel_layout.md moved `%ForageAssignControls` / `%HerdAssignControls` out
## of the drawer into a floating sheet, so a state that exists to judge the picker/stepper/forecast/
## gate-reasons has to OPEN it — the drawer now shows only the standing summary + `Assign … ▸`.
## These two calls replace the direct `_hud._build_*_assign_controls(...)` the states used before;
## the builders still run, just against the sheet's content container.
##
## **IT GOES THROUGH `_floorify`, LIKE ITS HERD TWIN.** Most states pass a FRESH fixture here rather
## than the object `_show_tile` already converted, so the sheet was being built from a dict the
## adapter had never seen. That was invisible while the adapter only rewrote ceilings — the fixture
## builders seed those themselves — and stopped being invisible the moment the adapter also had to
## seed the growth terms: every compose sheet opened this way lost its chart.
func _compose_forage(tile_info: Dictionary) -> void:
	_hud._drawercompose.open_forage_compose(
		_floorify(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX))

## Open the herd compose sheet, optionally DIALING IN a count and/or policy.
##
## `count` / `policy` are applied AFTER the first open, and that ordering is the whole point:
## opening the compose on a DIFFERENT herd re-seeds both off the band's standing staffing
## (`DrawerComposeController._build_herd_assign_controls`'s `source_changed` branch → `seed_hunt`),
## so a `set_hunt_count`/`set_hunt_policy` made BEFORE the first open is silently thrown away —
## the bug that rendered the documented 6-hunter local-hunt frames with 1 hunter (#357). The
## second open re-renders with `hunt_key` unchanged, so `source_changed` is false and the dialed
## values survive into the render (still subject to the stepper's own cap clamp, as in the game).
## Omit both to render whatever the re-seed produces — which is what the raid states deliberately want.
## `improvement` is the SECOND AXIS (issue #442) — a build verb is no longer a value of `policy`, so a
## frame that used to dial `policy: "tame"` dials this instead, and may dial a STANCE beside it. The
## re-open contract is unchanged: dial after the first open, then re-open so the render sees it.
func _compose_herd(herd: Dictionary, count: int = COMPOSE_COUNT_UNSET,
		floor: float = COMPOSE_FLOOR_UNSET, improvement: String = "") -> void:
	# The compose sheet is where the pair actually BITES (`_forecast_worker_cap`'s floor), and a herd
	# can be composed without ever being the selected subject — so check the argument here too, not
	# only through the per-frame scan in `_save`.
	_guard_herd_fields(herd, "compose_herd")
	_floorify(herd)
	_hud._drawercompose.open_herd_compose(herd)
	if count == COMPOSE_COUNT_UNSET and floor == COMPOSE_FLOOR_UNSET and improvement == "":
		return
	if count != COMPOSE_COUNT_UNSET:
		_hud._compose.set_hunt_count(count)
	if floor != COMPOSE_FLOOR_UNSET:
		_hud._compose.set_hunt_floor(floor)
	if improvement != "":
		_hud._compose.set_hunt_improvement(improvement)
	_hud._drawercompose.open_herd_compose(herd)

## A synthetic PRESSED mouse-button event, for driving a Control's real `gui_input` handler. The
## harness has no OS input, so this is how a click/wheel gesture is put through the shipped code path
## rather than calling the handler's effect directly.
func _mouse_button_event(button_index: int) -> InputEventMouseButton:
	var event := InputEventMouseButton.new()
	event.button_index = button_index
	event.pressed = true
	return event

## Find a Button by its face anywhere under `root` — the harness presses the REAL control the player
## presses, so an assertion covers the wiring and not just the handler it would have called.
## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live drawer label
## with the very `[url]` meta its own text carries, so the bound handler + anchor run exactly as they
## do in the game. Toggling: a second call on the same key dismisses the popover.
func _click_disclosure(key: String) -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label := _find_meta_label(_hud, meta)
	if label == null:
		push_warning("ui_preview: no detail label offering '%s' — disclosure not rendered?" % meta)
		return
	label.meta_clicked.emit(meta)

func _find_meta_label(node: Node, meta: String) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).text.contains("[url=%s]" % meta):
		return node
	for child in node.get_children():
		var found := _find_meta_label(child, meta)
		if found != null:
			return found
	return null

## Does any Label under `root` contain this text? The gate-reason assertions' instrument: a reason that
## has been COLLAPSED into a tooltip is no longer any label's text, so this tells a spelled-out
## prerequisite from a one-line "locked (N requirements unmet)" summary.
## The text of the first Label under `root` containing `needle` — "" when there is none. Lets a frame
## assert on a value that must CHANGE (a rung's payoff face) rather than merely be present.
## Slack allowed when asserting a control sits INSIDE its card (`_rect_contains`): a control laid out
## flush against the card's inner edge can land a sub-pixel over it and is not what "clipped" means.
const CLIP_TOLERANCE_PX := 1.0
## The two remedies a STANDING-but-gated Cultivate must still spell out (issue #420). Each is the tail
## of its `HudFloraVocab` reason, so the assertion reads the sentence the player reads and not just the
## rung's presence: the paused build's ease-off advice, and the finished patch's harvest advice.
## **THE PHRASE ONLY THE DELETED DEAL LINE COULD PRODUCE.** The improvement forecast line read
## `A → B while building → C`; its middle term restated the readout's own PER TURN headline and its
## first term the price of building (the crew row's dip note), so only the payoff was unique to it and
## the payoff moved onto the running control's FACE. Nothing else on either sheet says "while
## building" — `CREW_BUILD_DIP_NOTE_FORMAT` deliberately does not — so this needle now asserts the
## line's ABSENCE. **Absence alone is a vacuous claim** (deleting the payoff too would satisfy it), so
## every frame asserting it also asserts the payoff ON the face, by meta.
const IMPROVEMENT_DEAL_MIDDLE_NEEDLE := "while building"
## The `· then` grammar the OFFERED box and the RUNNING one now share — the whole point of moving the
## payoff onto the face is that the two states of one control read alike, so one needle serves both.
const IMPROVEMENT_PAYOFF_NEEDLE := "· then "
## `floor_chart_model`'s `lesson_known` for a probe reading the VERDICT rather than the aside: the
## faction has NOT learned this source's lesson, so the teaching line is the one it always carried.
const LESSON_NOT_YET_LEARNED := false
## The teaching line's two halves, as the needles that tell them apart: a lesson still being earned
## leads with the verb, and a lesson already known states only the BUILD the same multiplier paces.
const TEACHING_LESSON_NEEDLE := "Teaching"
const TEACHING_BUILD_NEEDLE := "Building at ×"

const IMPROVEMENT_PAUSED_NEEDLE := "ease off and it resumes"
## A crop `_food_tile_fixture`'s basket really carries, used to prove the crop list is ABSENT under a
## gated offer. Naming a real crop matters: a needle no basket contains would make the assertion pass
## whether the list rendered or not.
const GATED_CROP_NEEDLE := "Wild Grain"
## The offer wording that must NOT appear while the rung is gated — the imperative the gated state
## exists to remove. Kept as a literal so a reworded offer cannot silently pass this assertion.
const GATED_OFFER_NEEDLE := "Cultivate this patch"
## The Corral done-label's upkeep clause — asserted PRESENT on the penned frame and ABSENT on the
## pastoral one, which is the only way to pin an asymmetry rather than merely one side of it.
const UPKEEP_NEEDLE := "fodder/turn upkeep"
## The invariant TAIL of `SourceForecast.HUNT_WASTE_NOTE_FORMAT` (`⚠ %d%% wasted`) — the only part of
## that note a percentage-free ABSENCE test can name. The present-case assertion uses the whole
## formatted note instead, so the pair cannot both be satisfied by a note that lost its number.
const HUNT_WASTE_NEEDLE := "wasted"
## `RungGates`' third argument is the IMPROVEMENT axis, and these probes pass "this crew is building
## nothing". Named rather than a bare "" so a reader cannot mistake it for an omitted stance.
const RUNG_BUILDING_NOTHING := ""
## The faction's PENNING on the two corral-gate frames and on `two_meter_split`. Named because each is
## asserted against the rendered gate reason's percent, so the fixture value and the expected string
## are one number rather than two that can drift. Deliberately DIFFERENT between the two states, so a
## frame quoting the other one's percent fails rather than passing off a shared constant.
const CORRAL_GATE_PENNING := 0.35
const TWO_METER_PENNING := 0.45
## `forage_sow_locked`'s two gate inputs, named for the same reason: the frame asserts the rendered
## SOURCE reason against `SOW_REFUSAL_REASONS[…]` and the ABSENCE of the knowledge reason at this
## exact percent, so the fixture and the expected strings are one value each rather than two that can
## drift apart. The refusal key is `_food_tile_fixture`'s own (`_tended_tile_fixture` inherits it).
const SOW_LOCKED_REFUSAL_KEY := "too_dry"
const SOW_LOCKED_SEED_SELECTION := 0.12
## The crew the two zero-crew submits are composed at. Named because 0 is the WHOLE subject of those
## frames — it is the sim's unassign on a worked source and a no-op on an unworked one — and a bare 0
## beside `COMPOSE_COUNT_UNSET` reads like an omission.
const ZERO_CREW := 0
## The crew the two stance-beside-a-build frames are DIALED at — past every stance's cap, so the sheet's
## own clamp decides the crew and the deal's terms are the CEILING rather than the number typed here.
## It used to be described as "enough to saturate the patch on EVERY stance (Eradicate 4.80 / 0.32 =
## 15)", which stopped being true when the cap learned about the dip (#442): a BUILDING crew is capped
## on `stance × 0.25`, so Sustain clamps to 2 (the build crew) and Deplete to 3. Both frames still show
## the ceiling binding — that is what the clamp guarantees — and the pair still differs only by stance.
# **LABOUR-BOUND UNDER BOTH FLOORS, deliberately.** The build term is floor-independent only where
# the crew is the binding side, so this sits under the food-peak ceiling's dipped crew count
# (0.96 / (0.32 x 0.25) = 12). Fifteen was chosen when the dip rode the CEILING and the frame's claim
# was the opposite one; at 15 the peak's ceiling binds and the two floors' build terms differ.
const IMPROVEMENT_STANCE_FRAME_FORAGERS := 8
## **THE SIM'S OWN `workers_needed` FOR A CULTIVATING CREW ON THE REFERENCE PATCH.** Its derivation from
## the ladder's and the fixture's numbers is on `_cultivating_forage_band_fixture`, which ships it on the
## wire; `improvement_build_crew` asserts the compose cap equals what the sheet READS BACK off that
## assignment, so the control is the sim's published answer rather than a number the harness chose twice.
const CULTIVATE_SIM_WORKERS_NEEDED := 12
## Dialed past every plausible cap on that frame, so what the stepper renders IS the cap.
const BUILD_CREW_DIALED_FORAGERS := 14
## Idle workers handed to the WORKED-ROW cap twin, so IDLE never becomes the binding term and the two
## probes differ only by the count under test. Any number above the cap does; this one is not the band's.
const BUILD_CREW_IDLE_ON_HAND := 14
## **THE METER VALUE THE BUILDING/REVERTING PAIR IS JUDGED AT — deliberately NEAR COMPLETE.** "Preparing
## 96%" beside "Reverting 96%" is the exact ambiguity the third state exists to remove: at a high
## percentage the two states are most alike and the stakes are highest, since what is nearly finished is
## also what there is most to lose. Both frames render this ONE number, so the word and the tint are the
## only things that can differ between them.
const REVERTING_METER_PROGRESS := 0.96
const REVERTING_METER_PERCENT := 96
## The tile the band works INSTEAD in the reverting frame — any tile that is not the one being judged.
## The patch under test is then improved, owned and unworked, which is the whole condition.
const METER_AWAY_TILE_X := 64
## The tile card's cultivation ROW key, for the run-log excerpt. Not an assertion input: the assertions
## match the rendered VALUE markup, which no other row can produce.
const CULTIVATION_ROW_KEY := "Cultivation"
## How much of a rendered detail card to echo around a row key when reporting. Enough for the value
## cell and its colour tag, short enough to stay one log line.
const DETAIL_EXCERPT_CHARS := 96
const DETAIL_EXCERPT_ABSENT := "<row absent>"
## The take that crew is paid — `min(2 × 0.32, 0.96 × 0.25)` = the DIPPED ceiling, 0.24 food/turn. It is
## the number the green forecast line, the deal's middle term and the sim's own `actual_yield` must all
## carry; before the dip reached the forecast the green line quoted 0.64 (the undipped labour take) while
## the deal beside it said 0.24 — the same patch, the same crew, two different answers on one sheet.
# The take the sheet quotes on `improvement_build_crew`: the crew clamps to the sim's own
# `workers_needed` (12), and 12 x 0.32 x 0.25 = 0.96 — exactly the food-peak ceiling, i.e. the
# saturation point where the dip costs nothing at all. That coincidence IS the frame's subject.
const BUILD_CREW_DIPPED_TAKE := "0.96"
## **The three "already built" remedy needles went with the gate reasons they pinned** (issue #442):
## a completed rung is a static DONE LABEL now, not a greyed picker button, so there is no dead end to
## explain and nothing for a needle to find. `IMPROVEMENT_DONE_LABELS` is what those frames assert on.
## The herder crew on the fully-tamed herd: the herd's `herders_needed` pair AND the workers the
## standing Tame assignment staffs, so the two cannot disagree about how many hands are on it.
const TAMED_HERD_CREW := 4

## The Label NODE carrying `needle`, for the assertions that measure WHERE a row sits rather than
## what it says. `_label_text_containing` answers the text; a clipping check needs the rect.
func _label_node_containing(root: Node, needle: String) -> Label:
	if root == null:
		return null
	if root is Label and (root as Label).text.contains(needle):
		return root as Label
	for child in root.get_children():
		var found := _label_node_containing(child, needle)
		if found != null:
			return found
	return null

## What the VISIBLE cards stacked below `card` reserve in its dock — the harness's read-only echo of
## `DockScrollFit._height_reserved_below`, so a sizing failure can be attributed to a term rather than
## guessed at. Read-only on purpose: it must not become a second implementation the real one drifts
## from, which is why it is only ever printed, never asserted on.
func _dock_height_reserved_below(card: Control) -> float:
	var stack := card.get_parent() as VBoxContainer
	if stack == null:
		return 0.0
	var separation := float(stack.get_theme_constant("separation"))
	var reserved := 0.0
	var below := false
	for child in stack.get_children():
		if child == card:
			below = true
			continue
		var sibling := child as Control
		if not below or sibling == null or not sibling.visible:
			continue
		reserved += sibling.get_combined_minimum_size().y + separation
	return reserved

## Is `control` fully inside `rect` (both global)? Null control = false — an assertion about where a
## row sits must FAIL when the row is missing entirely, never pass vacuously. A one-pixel tolerance,
## because a control flush against the card's inner edge is not clipped.
func _rect_contains(rect: Rect2, control: Control) -> bool:
	if control == null:
		return false
	var inner := control.get_global_rect()
	return inner.position.y >= rect.position.y - CLIP_TOLERANCE_PX \
		and inner.end.y <= rect.end.y + CLIP_TOLERANCE_PX

func _label_text_containing(root: Node, needle: String) -> String:
	if root == null:
		return ""
	if root is Label and (root as Label).text.contains(needle):
		return (root as Label).text
	# RichTextLabels carry the BBCode SOURCE in `text`, which is what a needle spanning a `[color]`
	# span (the improvement deal's WARN-amber middle term) has to be matched against — the improvement
	# deal and the local yield previews are all `HudWidgets.forecast_label`s.
	if root is RichTextLabel and (root as RichTextLabel).text.contains(needle):
		return (root as RichTextLabel).text
	for child in root.get_children():
		var found := _label_text_containing(child, needle)
		if found != "":
			return found
	return ""

## One rung-meter row's rendered VALUE CELL — `[color=#HEX]<verb> 96%[/color]`, exactly as
## `DetailFormat.detail_bbcode` emits it. Word and tint in ONE needle, because the decaying state was a
## failure of BOTH and an assertion that pinned only one of them would pass on half a fix.
func _meter_value_markup(verb: String, hex: String) -> String:
	return "[color=#%s]%s %d%%[/color]" % [hex, verb, REVERTING_METER_PERCENT]

## A readable slice of a rendered detail card's BBCode around one row key — for the run log, so a
## failing meter assertion shows what the card actually SAID rather than only that it disagreed.
func _detail_excerpt(bbcode: String, key: String) -> String:
	var at := bbcode.find(key)
	if at < 0:
		return DETAIL_EXCERPT_ABSENT
	return bbcode.substr(at, DETAIL_EXCERPT_CHARS)

func _has_label_containing(root: Node, text: String) -> bool:
	if root == null:
		return false
	if root is Label and (root as Label).text.contains(text):
		return true
	if root is RichTextLabel and (root as RichTextLabel).text.contains(text):
		return true
	for child in root.get_children():
		if _has_label_containing(child, text):
			return true
	return false

func _find_button_by_text(root: Node, text: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text == text:
		return root as Button
	for child in root.get_children():
		var found := _find_button_by_text(child, text)
		if found != null:
			return found
	return null

## A CROP-PICKER ROW by the plant it names. A row's face is `<name> <share>% · <payoff>×`, whose share
## and payoff digits are the fixture's business and change whenever a basket is retuned, so the row is
## found by its NAME PREFIX — never by full text, which would make every crop assertion a duplicate of
## the fixture. Returns null when the basket carries no such plant.
func _find_crop_row(root: Node, crop_name: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text.begins_with(crop_name + " "):
		return root as Button
	for child in root.get_children():
		var found := _find_crop_row(child, crop_name)
		if found != null:
			return found
	return null

## The METRIC line of a policy rung's two-line face — `→ 1.48 food · 0.37 trade`, the products line
## the payoff/cap assertions read. The rung is found by `HudWidgets.POLICY_RUNG_META`, its identity,
## and NEVER by button text: the face lives on a two-Label stack beside an empty-`text` Button, so
## `_find_button_by_text` finds nothing at all here. "" when the rung is absent from the picker or
## wears its name alone (no metric).
## A preset button's TOOLTIP — where the floor's metric lives now that the face carries only the
## intent. Reached by the rung's meta like everything else here, never by its face.
func _policy_rung_tooltip(root: Node, policy: String) -> String:
	var btn := _find_policy_rung(root, policy)
	return btn.tooltip_text if btn != null else ""

func _policy_rung_metric(root: Node, policy: String) -> String:
	var btn := _find_policy_rung(root, policy)
	if btn == null:
		return ""
	# The face's Labels are siblings of the Button under the rung's CELL, not children of it.
	var lines := _face_lines(btn.get_parent())
	return lines[POLICY_RUNG_METRIC_LINE] if lines.size() > POLICY_RUNG_METRIC_LINE else ""

## Is this rung the SELECTED one? Read off the `normal` stylebox's fill, which `HudStyle.apply_button`
## writes from the variant — `BUTTON_PRIMARY_BG` is the one marker of "this is the chosen rung". It is
## read here rather than the `disabled` box because a rung can now be selected AND gated at once
## (issue #420): Godot then DRAWS the disabled box, but the variant the button was styled with is
## still recorded on `normal`, so this answers "which rung is lit?" in both states.
func _rung_is_selected(btn: Button) -> bool:
	if btn == null:
		return false
	var box := btn.get_theme_stylebox("normal")
	return box is StyleBoxFlat \
		and (box as StyleBoxFlat).bg_color.is_equal_approx(HudStyle.BUTTON_PRIMARY_BG)

## How many rungs the picker under `root` puts abreast — its `GridContainer.columns`. Reached from a
## rung rather than by searching for a GridContainer: the sheet holds several grids, and the one that
## matters is by definition the one a rung is in. The rung's own parent is its CELL (a
## `MarginContainer`), so the grid is one further up. 0 when there is no picker to measure, which
## fails an equality assertion rather than passing it vacuously.
func _policy_picker_columns(root: Node) -> int:
	var btn := _find_policy_rung(root, SourceForecast.FLOOR_PRESET_PEAK)
	if btn == null or btn.get_parent() == null:
		return 0
	var grid := btn.get_parent().get_parent()
	return (grid as GridContainer).columns if grid is GridContainer else 0

## Find the IMPROVEMENT control anywhere under `root`, by `HudWidgets.IMPROVEMENT_CONTROL_META` — its
## identity, never its face (which carries a live meter and a payoff and so changes every frame). The
## NODE TYPE is half the assertion: a `CheckBox` is offered or running (`button_pressed` tells those
## apart) and a plain `Label` is done, which is exactly the three-state contract. Returns the Control,
## so a caller can type-test it.
## The luminance gap an improvement checkbox's indicator must clear against the panel it sits on.
## 0.25 of the full range is far below what the fix delivers (~0.5 for both states) and far above what
## the defect scored: Godot's stock `unchecked` art is `#191919` at half alpha, which composites over
## `PANEL_SOLID` to a gap of ~0.001 — invisible, which is exactly the bug.
const CHECKBOX_INDICATOR_MIN_CONTRAST := 0.25

## How visible one of a CheckBox's indicator states is: composite every pixel of the art it would draw
## over `HudStyle.PANEL_SOLID` at that pixel's own alpha, and keep the largest luminance distance from
## the panel. **Measured off the art, not off a theme override**, because "the box is invisible" is a
## claim about pixels — an assertion phrased as "an override is set" would have passed on a
## `icon_normal_color` override, which a `CheckBox` ignores.
func _checkbox_indicator_contrast(control: Control, icon: String) -> float:
	var box := control as CheckBox
	if box == null:
		return 0.0
	var tex := box.get_theme_icon(icon)
	if tex == null:
		return 0.0
	var img := tex.get_image()
	if img == null:
		return 0.0
	var panel := HudStyle.PANEL_SOLID
	var panel_luminance := panel.get_luminance()
	var best := 0.0
	for y in img.get_height():
		for x in img.get_width():
			var px := img.get_pixel(x, y)
			var over := panel.lerp(Color(px.r, px.g, px.b), px.a)
			best = maxf(best, absf(over.get_luminance() - panel_luminance))
	return best

## How far the TICKED art's colour may sit from `HudStyle.SIGNAL` once brightness is divided out.
## Godot's stock tick chip is neutral grey, which scores ~0.65 on this measure, so 0.1 separates
## "recoloured to the live token" from "whatever the theme shipped" with room to spare.
const CHECKBOX_TICK_COLOUR_TOLERANCE := 0.1

## The ticked indicator's own colour, compared to `SIGNAL` with BRIGHTNESS DIVIDED OUT: take the art's
## most prominent pixel, scale it and `SIGNAL` so each has a largest channel of 1, and measure the
## distance. Brightness is normalised away because the chip's absolute level comes from the stock art
## being recoloured rather than redrawn; what is being pinned is that a running build wears the
## console's live colour instead of the stock theme's grey.
func _checkbox_tick_colour_gap(control: Control) -> float:
	var box := control as CheckBox
	if box == null:
		return 1.0
	var tex := box.get_theme_icon("checked")
	if tex == null:
		return 1.0
	var img := tex.get_image()
	if img == null:
		return 1.0
	var best := Color(0, 0, 0, 0)
	var best_weight := 0.0
	for y in img.get_height():
		for x in img.get_width():
			var px := img.get_pixel(x, y)
			var weight := px.get_luminance() * px.a
			if weight > best_weight:
				best_weight = weight
				best = px
	return _unit_channel(best).distance_to(_unit_channel(HudStyle.SIGNAL))

## A colour as a hue-only vector: its RGB scaled so the largest channel is 1. `Vector3.ONE` (neutral
## white/grey) for a black input, which reads as "no colour of its own" — the right answer for the
## degenerate case and the same answer a grey chip gives.
func _unit_channel(c: Color) -> Vector3:
	var peak: float = maxf(maxf(c.r, c.g), c.b)
	if peak <= 0.0:
		return Vector3.ONE
	return Vector3(c.r, c.g, c.b) / peak

## **UNCHECK THE RUNNING BOX AND CHECK WHAT THE CLIENT WOULD TRANSMIT** — driven through the REAL
## control and the REAL formatter, not through the handler each would have called.
##
## The chain is the player's: press the live `CheckBox` (which flips it and fires `toggled`), press
## the sheet's own commit button, capture the payload off `HudLayer.improvement_requested`, and run it
## through `Main.format_abandon_improvement` — the pure static `Main._on_hud_improvement` dispatches
## to. Asserting the LINE rather than the payload is what makes this test the shipped representation:
## the payload could carry a perfectly good herd id and still be formatted into the tile-targeted
## grammar, which is exactly the mistake the two webs' differing targeting rules invite.
##
## Restores the composed improvement afterwards, so the frame that just rendered is not disturbed for
## whatever asserts against it next.
## The compose sheet's EYEBROW, as rendered — the header is one BBCode `RichTextLabel` holding
## `<EYEBROW>  <subject>`, so `get_parsed_text` is what a player actually reads off it. "" when no
## sheet is open, which fails a `begins_with` rather than satisfying it.
func _compose_sheet_eyebrow() -> String:
	var sheet: Control = _hud._drawercompose._compose_sheet
	if sheet == null or sheet._header == null:
		return ""
	return (sheet._header as RichTextLabel).get_parsed_text().strip_edges()

## A compose sheet's COMMIT button by its own meta, never by face: the face is the thing every crew-noun
## assertion is ABOUT (`Forage` / `Tend` / `Hunt Here` / `Unassign`), so finding it by text could only
## ever confirm the string the caller already assumed.
func _compose_commit_button(root: Node) -> Button:
	var node := _find_meta_node(root, HudWidgets.COMPOSE_COMMIT_META)
	return node as Button if node is Button else null

## The LAND drawer's `Assign … ▸` button. Found STRUCTURALLY — `%ForageAssignControls` holds at most a
## standing-summary `HFlowContainer` and this one Button (`build_forage_drawer_actions`) — for the same
## reason as above: its face carries the crew noun under test.
func _forage_open_button() -> Button:
	for child in _hud.forage_assign_controls.get_children():
		if child is Button:
			return child as Button
	return null

## **THE PLANT WEB'S CREW NOUN, ON ALL FOUR SURFACES OF ONE FRAME.** Stages a patch, builds the drawer's
## read state and opens its sheet, then asserts the sheet's eyebrow, the crew-row label, the commit
## button and the drawer's open button all name `want_label` — plus, independently of `want_label`,
## that the eyebrow and the stepper AGREE. That last one is the point: the reported failure mode is a
## header saying one noun over a stepper saying another, and it is expressible whenever the two resolve
## separately, so it is asserted as a RELATION between two rendered strings rather than against a
## constant either could drift from.
##
## `improvement` composes a build IN FLIGHT (`""` for none). It must not move the noun — a crew clearing
## ground is still foraging the stand — which is exactly what the `plant_crew_wild_building` /
## `plant_crew_wild_sowing` states pass it for.
func _assert_plant_crew_noun(state_name: String, tile: Dictionary, want_label: String,
		improvement: String = SourceForecast.IMPROVEMENT_NONE) -> void:
	_hud._compose.reset_forage_source()
	_show_tile(tile)
	# Drop the previous tile's button so this state gets a FRESH drawer build rather than the
	# same-shape patch path — the noun must be right on both, and the patch path is covered by
	# `forage_assign_button_targets_selected_tile`.
	_hud._drawercompose._clear_forage_drawer()
	await _settle()
	_hud._drawercompose.build_forage_drawer_actions(
		_floorify(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX))
	_compose_forage(tile)
	if improvement != SourceForecast.IMPROVEMENT_NONE:
		# **DIAL THE VERB AFTER THE FIRST OPEN, THEN RE-OPEN — the herd sheet's contract, and the
		# forage sheet keeps it too.** Opening on a DIFFERENT source re-seeds the composition off the
		# band's own standing build (`_build_forage_assign_controls`' `source_changed` branch →
		# `seed_forage`), so a verb set BEFORE the first open is silently thrown away: measured, the
		# Cultivate and Sow frames came back BYTE-IDENTICAL, both rendering whatever the re-seed
		# produced rather than the build under test.
		_hud._compose.set_forage_improvement(improvement)
		_compose_forage(tile)
	await _settle()
	if improvement != SourceForecast.IMPROVEMENT_NONE:
		# The fixture must actually REACH the state being claimed — a build the sheet quietly dropped
		# would leave this whole state asserting the no-build case twice under two names.
		_assert_hud("%s: the sheet really is composing a live `%s`" % [state_name, improvement],
			_hud._compose.forage_improvement() == improvement)
	await _save(state_name)
	var sheet: Control = _hud._drawercompose._compose_sheet
	var eyebrow := _compose_sheet_eyebrow()
	var stepper_label := _crew_row_label(sheet)
	var commit := _compose_commit_button(sheet)
	var open_btn := _forage_open_button()
	var want_eyebrow := (HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % want_label.to_lower()).to_upper()
	_assert_hud("%s: the sheet's eyebrow reads `%s`" % [state_name, want_eyebrow],
		eyebrow.begins_with(want_eyebrow))
	_assert_hud("%s: the crew row is labelled `%s`" % [state_name, want_label.to_upper()],
		stepper_label == want_label.to_upper())
	_assert_hud("%s: the commit button reads `%s`" % [state_name,
			String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(want_label, ""))],
		commit != null and commit.text == String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(want_label, "")))
	_assert_hud("%s: the drawer opens with `%s`" % [state_name,
			HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % want_label.to_lower()],
		open_btn != null
			and open_btn.text == HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % want_label.to_lower())
	# THE CONSISTENCY CLAIM, stated without naming the noun — a header and a stepper that resolve
	# through one function cannot disagree, and a frame where they do is the defect itself.
	_assert_hud("%s: the eyebrow and the stepper name the SAME crew on one frame" % state_name,
		stepper_label != "" and eyebrow.begins_with("%s %s" % [
			(HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % "").strip_edges().to_upper(),
			stepper_label]))

func _assert_abandon_emits(kind: String, improvement: String, want_line: String) -> void:
	var sheet := _hud._drawercompose._compose_sheet
	var box := _find_improvement_control(sheet, improvement)
	if not (box is CheckBox):
		_assert_hud("abandon (%s): a running improvement box to uncheck" % kind, false)
		return
	var captured: Array[Dictionary] = []
	var sink := func(payload: Dictionary) -> void: captured.append(payload)
	_hud.improvement_requested.connect(sink)
	(box as CheckBox).button_pressed = false
	# **SETTLE BEFORE LOOKING FOR THE COMMIT BUTTON.** Unchecking rebuilds the whole control block,
	# and the builder clears its host with `queue_free()` — a DEFERRED removal, so until the frame
	# ends the stale nodes are still children and a synchronous search finds the OLD button first.
	# Pressing that one runs a closure built against the pre-uncheck composition, which emits nothing
	# (`composed == standing`) and reads exactly like "the abandon path is not wired up".
	await _settle()
	# By META, not by face: the forage commit's verb now follows the patch's rung (`Forage` on wild
	# ground, `Tend` on a managed one), so a text match here would encode an assumption about the
	# fixture's rung that has nothing to do with what this probe is testing.
	var commit := _compose_commit_button(sheet)
	if commit == null:
		_hud.improvement_requested.disconnect(sink)
		_assert_hud("abandon (%s): the sheet's commit button" % kind, false)
		return
	commit.pressed.emit()
	_hud.improvement_requested.disconnect(sink)
	if captured.is_empty():
		_assert_hud("abandon (%s): unchecking a running build emits a command" % kind, false)
		return
	var line := String(MAIN_SCRIPT.format_abandon_improvement(captured[0]).get("line", ""))
	print("ui_preview: abandon %s -> %s" % [kind, line])
	_assert_hud("unchecking a running %s build transmits `%s`" % [kind, want_line],
		line == want_line)
	# Committing also fired `assign_labor`, whose OPTIMISTIC pending entry would tint every later
	# frame's rows amber. Drop it — the probe is about the command, not about the overlay.
	_hud._band_labor._pending_labor.clear()

## The improvement control's FACE text, whichever of its three node shapes it is in — the handle the
## meter assertions read. "" when the control is absent.
func _improvement_face(root: Node, improvement: String) -> String:
	var control := _find_improvement_control(root, improvement)
	if control is CheckBox:
		return (control as CheckBox).text
	if control is Label:
		return (control as Label).text
	return ""

func _find_improvement_control(root: Node, improvement: String) -> Control:
	if root == null:
		return null
	if root is Control and (root as Control).get_meta(
			HudWidgets.IMPROVEMENT_CONTROL_META, "") == improvement:
		return root as Control
	for child in root.get_children():
		var found := _find_improvement_control(child, improvement)
		if found != null:
			return found
	return null

func _find_policy_rung(root: Node, policy: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).get_meta(HudWidgets.POLICY_RUNG_META, "") == policy:
		return root as Button
	for child in root.get_children():
		var found := _find_policy_rung(child, policy)
		if found != null:
			return found
	return null

## The first node under `root` carrying `meta` — the identity finder for the three 4b controls, which
## carry no text at all (the chart) or a face made of live numbers (the targets, the verdict). A text
## match on any of them would find nothing and pass, which is the failure this idiom exists to avoid.
func _find_meta_node(root: Node, meta: String) -> Node:
	if root == null:
		return null
	if root is Control and (root as Control).has_meta(meta):
		return root
	for child in root.get_children():
		var found := _find_meta_node(child, meta)
		if found != null:
			return found
	return null

## The COUNT a crew target is offering, read off the face it renders — or `CREW_TARGET_ABSENT` when
## that target is not rendered at all. The two answers are different claims: `0` says "nothing needs
## clearing", absent says "this source's crew cannot be priced".
func _crew_target_count(root: Node, key: String) -> int:
	var button := _find_crew_target(root, key)
	if button == null:
		return CREW_TARGET_ABSENT
	# **READ OFF THE META, NEVER THE FACE.** The pill's face is a two-Label stack over an
	# empty-`text` Button (a count and its label at one size are one undifferentiated phrase), so the
	# old `button.text.split(" ")[0]` finds an empty string here — and `int("")` is 0, which is a REAL
	# reading of this control ("nothing needs clearing"). It would have passed silently.
	return int(button.get_meta(HudWidgets.CREW_TARGET_COUNT_META, CREW_TARGET_ABSENT))

## The READOUT's yields row as one string — every Label in it, joined. The row is found by
## `HudWidgets.YIELDS_ROW_META`, its identity: its face is a flow of Labels at three sizes (the
## number, the unit + its route, the take's qualifier), so there is no single `text` to match and a
## needle search across the sheet would find whichever Label happened to hold it. "" when no readout
## rendered, which fails a `contains` assertion rather than satisfying it.
func _yields_text(root: Node) -> String:
	var row := _find_meta_node(root, HudWidgets.YIELDS_ROW_META)
	return " ".join(_face_lines(row)) if row != null else ""

## The readout's HEADER — the caption over the yields row, carrying the unit and (when the readings
## state one) the key to their arrow. It is the row's SIBLING, not a Label inside it, which is what
## keeps `_yields_text` reading only the numbers: asserting "the unit is not repeated per account"
## against a string that included the header would pass on a row that repeated nothing and a header
## that said everything. "" when no readout rendered.
func _yields_header(root: Node) -> String:
	var row := _find_meta_node(root, HudWidgets.YIELDS_ROW_META)
	if row == null or row.get_parent() == null:
		return ""
	var index := row.get_index()
	if index <= 0:
		return ""
	var caption := row.get_parent().get_child(index - 1)
	return (caption as Label).text if caption is Label else ""

## One account's `now → after` pair, parsed off the RENDERED face — `[now, after]`, or `[0, 0]` when
## that account states no transition. Parsed rather than recomputed: a helper that asked
## `expected_yield_account` twice would agree with the widget by construction and testify to nothing.
func _yield_now_after(yields_text: String, account: String) -> Array:
	# The face reads `<now> → <after> <ACCOUNT>`, so the pair is the three tokens before the account.
	var upto := yields_text.split(account)[0].strip_edges().split(" ", false)
	if upto.size() < 3 or upto[upto.size() - 2] != "→":
		return [0.0, 0.0]
	return [float(upto[upto.size() - 3]), float(upto[upto.size() - 1])]

## The CREW ROW's label — `HUNTERS` / `HERDERS` / `FORAGERS`, the crew noun the sheet resolved off the
## composed improvement axis. By meta rather than by text, because the sheet's EYEBROW two rows above
## carries the same noun in the same case (`ASSIGN HUNTERS`), so a search would match it and pass
## without ever reaching the crew row. "" when there is no crew row.
## The READOUT's ASIDE as one string — its lines joined. Found by `HudWidgets.READOUT_ASIDE_META`,
## its identity: every line is a plain Label at one size, so there is no distinguishing face, and the
## teaching line's own text carries a live multiplier that a needle would have to be re-tuned against
## every time a fixture's floor moved. "" when no aside rendered.
func _readout_aside_text(root: Node) -> String:
	var block := _find_meta_node(root, HudWidgets.READOUT_ASIDE_META)
	return " ".join(_face_lines(block)) if block != null else ""

## The teaching line ALONE, by its own meta. Its aside siblings move with the floor too, so a
## whole-aside comparison is satisfied by them and cannot testify about this sentence — proven, by
## blanking the note and watching the aside-wide form still pass.
func _teaching_line(root: Node) -> String:
	var node := _find_meta_node(root, HudWidgets.READOUT_TEACHING_META)
	return (node as Label).text if node is Label else ""

func _crew_row_label(root: Node) -> String:
	var node := _find_meta_node(root, HudWidgets.CREW_ROW_LABEL_META)
	return (node as Label).text if node is Label else ""

## The crew row's BUILD-DIP note, by its own meta — `""` when none rendered, which is a real reading
## and half of what the note is asserted on: a line that appears on every sheet claims nothing. Not
## found by text, and not by scanning the row: the row LABEL sits beside it and renders either way.
func _crew_row_dip_note(root: Node) -> String:
	var node := _find_meta_node(root, HudWidgets.CREW_ROW_DIP_META)
	return (node as Label).text if node is Label else ""

func _find_crew_target(root: Node, key: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).get_meta(HudWidgets.CREW_TARGET_META, "") == key:
		return root as Button
	for child in root.get_children():
		var found := _find_crew_target(child, key)
		if found != null:
			return found
	return null

## The verdict's SEVERITY (`SourceForecast.VERDICT_*`), which is its assertable half — the sentence
## carries turn counts and percentages that move with the fixture. "" when no verdict rendered.
func _verdict_severity(root: Node) -> String:
	var node := _find_meta_node(root, HudWidgets.VERDICT_META)
	return String((node as Control).get_meta(HudWidgets.VERDICT_META, "")) if node != null else ""

## The verdict's SENTENCE — the row's Labels joined (the severity dot is a Label of the row too, so it
## leads). Found by the same meta as the severity above, because the row's two halves are one claim and
## a needle search across the sheet would match whichever line happened to carry the same number. "" when
## no verdict rendered, which fails a `contains` assertion rather than satisfying it.
func _verdict_text(root: Node) -> String:
	var node := _find_meta_node(root, HudWidgets.VERDICT_META)
	return " ".join(_face_lines(node)) if node != null else ""

## The TRIP-READOUT claims that live on the `_hunt_assign_forecast_states` frames, dispatched by state
## name so each fixture is asserted on the ONE thing it was built to show. They ride here rather than
## after the loop because the loop is where each state is actually staged, and re-staging one to assert
## it would risk asserting a sheet the frame never rendered.
##
## **EACH IS ONE HALF OF A PAIR**, the other half being `herd_hunt_expedition`'s block (a clean raid
## paying BOTH accounts, no waste, a brisk OK verdict): a lone "the waste note is here" passes on a
## readout that always prints one, and a lone "there is no trade row" passes on a readout that can no
## longer print any account at all.
func _assert_trip_readout(state_name: String) -> void:
	var sheet: Control = _hud._drawercompose._compose_sheet
	match state_name:
		"herd_hunt_forecast_viable":
			# A party of 4 kills a 16-food mammoth and hauls 4 of it — the WASTE half, and the ZERO
			# ACCOUNT half in one fixture: this cell carries no `delivers_trade` at all, so the trade
			# row must not render. The trade-paying deer in `herd_hunt_expedition` is its twin.
			var wasted := _yields_text(sheet)
			var waste_pct := int(round((MAMMOTH_FOOD_PER_ANIMAL - HUNT_FORECAST_PARTY)
				/ MAMMOTH_FOOD_PER_ANIMAL * 100.0))
			_assert_hud("a partial kill states its WASTE on the trip's yields row",
				wasted.contains((SourceForecast.HUNT_WASTE_NOTE_FORMAT % waste_pct).to_upper()))
			_assert_hud("…and an account the quarry does not pay renders NO row",
				wasted.contains("FOOD") and not wasted.contains("TRADE"))
		"herd_hunt_forecast_slow":
			# 54 turns past the band's 20-turn warn line — the verdict carries the severity the Send
			# button and the one-line form already carry, so the box cannot disagree with either.
			_assert_hud("a raid past the band's warn line reads SLOW in the trip verdict",
				_verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and _verdict_text(sheet).contains(str(DEER_SUSTAIN_TRIP_TURNS)))
		"herd_hunt_forecast_eradicate":
			# `turns_to_fill == 0` — the raid ran the whole forecast horizon still delivering, so there
			# is no total to quote and the verdict says so instead of printing a bare 0.
			_assert_hud("an unbounded raid states no total, and still reads SLOW",
				_verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and _verdict_text(sheet).contains(
						SourceForecast.EXPEDITION_TRIP_LONG_VERDICT))
		"herd_hunt_forecast_no_surplus":
			# **A REFUSED RAID RENDERS NO BOX AT ALL.** It has no payload to lay out in rows, and an
			# empty well would read as a raid delivering nothing measurable rather than one the panel
			# is declining — so the branch keeps the one-line refusal it always had.
			_assert_hud("a raid with no surplus renders the refusal, never an empty readout box",
				_find_meta_node(sheet, HudWidgets.YIELDS_ROW_META) == null
					and _has_label_containing(sheet, "too lean to raid"))

## Every Label text under `root`, in tree order — the rung face's lines as they are stacked.
func _face_lines(root: Node) -> Array[String]:
	var lines: Array[String] = []
	if root == null:
		return lines
	if root is Label:
		lines.append((root as Label).text)
	for child in root.get_children():
		lines.append_array(_face_lines(child))
	return lines

# ---- the compose sheets' SHARED VERTICAL GRAMMAR ------------------------------------------------
# The forage sheet and the hunt sheet ask the same two questions in the same act — WHICH STANCE, and
# WITH HOW MANY PEOPLE — and they must ask them in the same order, because a player moving between the
# two is reading one control layout, not two. The hunt sheet used to put its crew stepper directly
# under the band picker, i.e. staff first and decide after; both now read
#   band picker → stance picker → (hint) → crew stepper → … → improvement.
#
# A FRAME CANNOT HOLD THAT CLAIM. Two PNGs side by side show the order to a human who thinks to look,
# and nothing fails when one of them moves — which is exactly how they drifted apart. So the invariant
# is asserted as a SPINE: the ordered structural controls of the open sheet, with the prose between
# them (hints, cap notes, forecasts, gate reasons, the plant web's crop rows) deliberately EXCLUDED.
# The two webs legitimately say different things in different places; what must match is the order in
# which the controls come.
const COMPOSE_SPINE_BAND := "band"
const COMPOSE_SPINE_POLICY := "policy"
const COMPOSE_SPINE_STEPPER := "stepper"
const COMPOSE_SPINE_IMPROVEMENT := "improvement"
## What EVERY compose sheet must open with — both webs, and the hunt sheet's local and expedition
## branches alike. The expedition branch builds no improvement control (a detached party builds
## nothing), so the shared claim is the HEAD; the two LOCAL sheets are additionally compared in full.
const COMPOSE_SPINE_HEAD: Array[String] = [
	COMPOSE_SPINE_BAND, COMPOSE_SPINE_POLICY, COMPOSE_SPINE_STEPPER,
]
## The three sheets whose spines this run captures, as `_compose_spines` keys. Named consts because the
## capture sites and the parity check sit ~1,600 lines apart, and a typo in either would silently
## compare a spine against nothing.
const COMPOSE_SPINE_KEY_FORAGE := "forage"
const COMPOSE_SPINE_KEY_HUNT := "local hunt"
const COMPOSE_SPINE_KEY_EXPEDITION := "hunt expedition"
## The `−` face `HudWidgets.add_stepper_controls` gives every stepper's decrement button (U+2212, not a
## hyphen). It is the one structural handle on a stepper row — unlike a rung or an improvement box, a
## stepper carries no meta — so the walk below finds it by that face.
const COMPOSE_STEPPER_MINUS_FACE := "−"

## The open compose sheet's spine, in tree order. Each recognized control is tagged and NOT descended
## into: a rung's cell holds Labels, an improvement control holds its own rows, and neither is a spine
## control in its own right. A policy PICKER emits one tag however many rungs it holds.
func _compose_spine(root: Node) -> Array[String]:
	var spine: Array[String] = []
	_collect_compose_spine(root, spine)
	return spine

func _collect_compose_spine(node: Node, spine: Array[String]) -> void:
	if node == null:
		return
	if node is Control and (node as Control).has_meta(HudWidgets.IMPROVEMENT_CONTROL_META):
		spine.append(COMPOSE_SPINE_IMPROVEMENT)
		return
	if node is OptionButton:
		spine.append(COMPOSE_SPINE_BAND)
		return
	if node is Button and (node as Button).has_meta(HudWidgets.POLICY_RUNG_META):
		if spine.is_empty() or spine[spine.size() - 1] != COMPOSE_SPINE_POLICY:
			spine.append(COMPOSE_SPINE_POLICY)
		return
	if node is Button and (node as Button).text == COMPOSE_STEPPER_MINUS_FACE:
		spine.append(COMPOSE_SPINE_STEPPER)
		return
	for child in node.get_children():
		_collect_compose_spine(child, spine)

## Capture the open sheet's spine under `key`, and assert the shared HEAD on the spot so a failure
## names the sheet that broke rather than only the pair. An EMPTY spine fails too — a sheet that never
## opened would otherwise make the parity comparison vacuously true.
func _record_compose_spine(key: String) -> void:
	var spine := _compose_spine(_hud._drawercompose._compose_sheet)
	_compose_spines[key] = spine
	_assert_hud("the %s compose sheet opens band → stance → crew (spine %s)" % [key, str(spine)],
		spine.slice(0, COMPOSE_SPINE_HEAD.size()) == COMPOSE_SPINE_HEAD)

## THE PARITY ASSERTION: the two LOCAL compose sheets must read in the same control order, start to
## finish. Both keys must have been recorded — comparing two missing spines would pass while proving
## nothing, which is the failure mode a frame-only check already has.
func _assert_compose_order_parity(forage_key: String, hunt_key: String) -> void:
	var have_both := _compose_spines.has(forage_key) and _compose_spines.has(hunt_key)
	_assert_hud("both compose spines were captured before the parity check (%s, %s)"
		% [forage_key, hunt_key], have_both)
	if not have_both:
		return
	var forage_spine: Array = _compose_spines[forage_key]
	var hunt_spine: Array = _compose_spines[hunt_key]
	_assert_hud(("the forage and local-hunt sheets read in the SAME control order — forage %s, hunt %s"
		% [str(forage_spine), str(hunt_spine)]), forage_spine == hunt_spine)

## The crew a stepper is SHOWING — the value Label `HudWidgets.add_stepper_controls` lays between the
## `−` and the `+`. Structural, like the spine walk: a stepper carries no meta, so it is found by that
## `−` face and the value is its next sibling.
##
## It exists because "the frame renders the N-worker crew" was being asserted against
## `ComposeState.hunt_count()` — the model the harness itself had just dialed. That is a real test of
## the CLAMP (the render writes the clamped count back), but it is not a test of the readout, and a
## stepper drawing any other number would pass it. `STEPPER_VALUE_ABSENT` on a missing stepper, so a
## sheet that never opened fails an equality claim rather than satisfying it.
const STEPPER_VALUE_ABSENT := -1
func _stepper_value(root: Node) -> int:
	var minus := _find_button_by_text(root, COMPOSE_STEPPER_MINUS_FACE)
	if minus == null or minus.get_parent() == null:
		return STEPPER_VALUE_ABSENT
	var siblings := minus.get_parent().get_children()
	var index := siblings.find(minus)
	if index < 0 or index + 1 >= siblings.size():
		return STEPPER_VALUE_ABSENT
	var value: Node = siblings[index + 1]
	return int((value as Label).text) if value is Label else STEPPER_VALUE_ABSENT

## How many Buttons under `root` wear this face — the "is the same order offered twice?" test.
func _count_buttons_by_text(root: Node, text: String) -> int:
	if root == null:
		return 0
	var total := 1 if (root is Button and (root as Button).text == text) else 0
	for child in root.get_children():
		total += _count_buttons_by_text(child, text)
	return total

## ---- THE TILE CARD'S TWO FOOD-WEB ROWS — the three claims a frame cannot carry ----------------
##
## A picture shows that the card LOOKS right; none of these can be read off one. They run against the
## REAL line producer (`SubjectDrawerController._tile_terrain_lines`), never against a re-derivation
## here, so a regression in the producer is what fails them.
##
## 1. THE BASKET DECOMPOSES THE STOCK. The indented rows' biomasses must sum to the `Foraging` row's
##    own ceiling — the whole reason each row states an absolute beside its share. Independent
##    rounding does NOT sum (78 + 64 + 64 = 206 against this fixture's 205), so this is a real test of
##    the remainder fold and not of arithmetic that could not fail.
## 2. AN UNSTATED ROLE RENDERS NO ICON. `""` means the roster does not know this species, not
##    "staple", so the row must carry none of the three role marks while its neighbours carry theirs.
## 3. THE TWO ROWS ARE ADJACENT, FORAGING FIRST. Adjacency is what stops the two webs being confused,
##    and it is invisible to any assertion that merely finds both rows present.
func _assert_food_layer_rows() -> void:
	var lines := _hud._drawer._tile_terrain_lines(_floorify(
		_three_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	var forage_index := _detail_row_index(lines, HudFloraVocab.FORAGING_KEY)
	var graze_index := _detail_row_index(lines, HudFloraVocab.GRAZING_KEY)
	_assert_hud("the tile card states a Foraging row and a Grazing row",
		forage_index >= 0 and graze_index >= 0)
	# The basket rows sit BETWEEN them, so "adjacent" means the animal row follows the human block —
	# the human layer is never split by it, which is exactly what used to happen.
	var basket := _flora_basket_rows(lines)
	_assert_hud("…Foraging leads, and Grazing follows its basket with nothing else between",
		forage_index >= 0 and graze_index == forage_index + basket.size() + 1)
	var basket_total := 0
	for row in basket:
		basket_total += _flora_row_biomass(row)
	# **AGAINST THE STANDING STOCK, never the ceiling.** These rows say what the `150 / 205` above
	# them is MADE OF; summing to 205 would decompose a full patch nobody is looking at, and the card
	# would then hold two numbers disagreeing about which stand is under discussion. The fixture is
	# drawn down precisely so this assertion can tell the two apart.
	_assert_hud("…and the basket's biomasses sum to the STANDING Foraging stock (%d of %d)" % [
			basket_total, int(THREE_ROLE_STOCK)],
		basket.size() == 3 and basket_total == int(THREE_ROLE_STOCK))
	var unstated := _flora_basket_rows(_hud._drawer._tile_terrain_lines(_floorify(
		_unstated_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)))
	var icon_rows := 0
	var cotton_has_icon := true
	for row in unstated:
		var has_icon := _flora_row_has_role_icon(row)
		if has_icon:
			icon_rows += 1
		if row.contains("Cotton"):
			cotton_has_icon = has_icon
	_assert_hud("a species whose role the wire leaves UNSTATED renders no role icon",
		unstated.size() == 3 and not cotton_has_icon)
	_assert_hud("…while the two roles the wire DOES state still wear theirs", icon_rows == 2)

## The index of the `Key: value` row with this key, or -1. Matches the key EXACTLY (up to the
## `DetailFormat` separator) so `Foraging` cannot be found by a row that merely mentions it.
func _detail_row_index(lines: Array[String], key: String) -> int:
	var prefix := key + DetailFormat.DETAIL_KV_SEPARATOR
	for index in lines.size():
		if lines[index].begins_with(prefix):
			return index
	return -1

## The indented basket rows, in order. They are the only indented rows the LAND drawer emits.
func _flora_basket_rows(lines: Array[String]) -> Array[String]:
	var rows: Array[String] = []
	for line in lines:
		if line.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT):
			rows.append(line)
	return rows

## The `(78)` a basket row closes with — parsed back out of the RENDERED row, so this reads what the
## player reads rather than recomputing what it should have been.
func _flora_row_biomass(row: String) -> int:
	var open_paren := row.rfind("(")
	var close_paren := row.rfind(")")
	if open_paren < 0 or close_paren <= open_paren:
		return 0
	return int(row.substr(open_paren + 1, close_paren - open_paren - 1))

func _flora_row_has_role_icon(row: String) -> bool:
	for role in FoodIcons.CROP_ROLE_ICONS:
		if row.contains(String(FoodIcons.CROP_ROLE_ICONS[role])):
			return true
	return false

## Same shape as `_assert_turn_orb`, for dock-card visibility. A PNG shows what a frame looks like;
## these say what it MUST be, so a default regression fails loudly in the run log instead of waiting
## for someone to notice a card that should not be there.
# ---- the event dock's fixtures + probes ---------------------------------------------------------

## The largest bar the dock offers, referenced rather than written as a 4 so the state and the
## panel's own `RECENT_COUNT_MAX` cannot drift.
const EVENT_DOCK_MAX_ROWS := EventDockPanel.RECENT_COUNT_MAX
## How many `predator_raid` rows the fixture carries on turn 47 — TWO, deliberately identical apart
## from `seq`. This is the number the old signature de-duplication answered 1 to.
const EVENT_DOCK_DUPLICATE_RAIDS := 2

## Mirror what `Main` does with the dock's reservation — **by asking `Main`'s own table**, not by
## restating its answer. `MAP_ONLY_RESERVERS` is a `const` on the Main script (preloaded here
## already, for `escape_claimant`), so dropping `&"event_dock"` from it makes this harness fan the
## reservation out to the HUD exactly as the live client would, and the "nothing moves down"
## assertion below fails. A hard-coded `pass` here would have pinned the harness, not the client.
func _on_preview_event_dock_reservation(edge: int, size: float) -> void:
	if MAIN_SCRIPT.MAP_ONLY_RESERVERS.has(&"event_dock"):
		return
	_hud.set_reserved_inset(&"event_dock", edge, size)

## The harness's stand-in for `Main._update_event_dock_insets`: the vertical reservation total on each
## side PLUS the HUD's own authored side column. `Main` is never instanced here, so the sum is
## restated — but every term is read live off the same nodes `Main` reads, so a change to either
## column's authored width lands here without an edit.
func _preview_push_event_dock_insets(dock: EventDockPanel, reserved_left: float, reserved_right: float) -> void:
	dock.set_perpendicular_insets(
		reserved_left + _hud.left_column_width(), reserved_right + _hud.right_column_width())

## How many retained events of one kind the dock is holding — read off its own accumulator, since the
## claim is about DE-DUPLICATION and a rendered row count would also be filtered by the detail floor.
func _preview_event_kind_count(dock: EventDockPanel, kind: String) -> int:
	var count := 0
	for event in dock._events:
		if String(event["kind"]) == kind:
			count += 1
	return count

func _preview_event_channels_all_on(dock: EventDockPanel) -> bool:
	for channel in HudEventVocab.CHANNEL_ORDER:
		if not bool(dock._channels.get(String(channel), false)):
			return false
	return true

## A scratch `narrative.cfg` that EXISTS and carries another panel's section, but no `[events]` —
## the shape every upgrading player's file has on first launch into this build.
func _write_event_prefs_without_section() -> void:
	var cfg := ConfigFile.new()
	cfg.set_value("hud_panels", "legend_suppressed", true)
	cfg.save(EventDockPanel.config_path())

func _write_event_prefs_with_channels(channels: Array) -> void:
	var cfg := ConfigFile.new()
	cfg.set_value("hud_panels", "legend_suppressed", true)
	cfg.set_value("events", "channels", channels)
	cfg.save(EventDockPanel.config_path())

## `rendered` reads the label the dock would DRAW (`_row_label`, i.e. after the band substitution)
## rather than the raw one it stored. The band-label assertions have to ask the rendered one — the
## substitution is deliberately a render-time resolution, so a raw read would pass on a dock that
## never re-labels anything.
func _preview_event_label_count(dock: EventDockPanel, label: String, rendered: bool = false) -> int:
	var count := 0
	for event in dock._events:
		var found: String = dock._row_label(event) if rendered else String(event["label"])
		if found == label:
			count += 1
	return count

## The rollback fixture pair: two batches REUSING the same `seq` values with different labels, which
## is exactly what a restored `CommandEventLog` replays (its `next_seq` counter is checkpoint state).
const EVENT_DOCK_ROLLBACK_SEQ := 501
const EVENT_DOCK_ROLLBACK_BEFORE_LABEL := "Hunters brought back red deer"
const EVENT_DOCK_ROLLBACK_AFTER_LABEL := "The hunt came home empty"

func _event_dock_rollback_before() -> Array:
	return [{"tick": 60, "kind": "hunt", "faction": 0,
		"label": EVENT_DOCK_ROLLBACK_BEFORE_LABEL, "detail": "", "seq": EVENT_DOCK_ROLLBACK_SEQ}]

func _event_dock_rollback_after() -> Array:
	return [{"tick": 60, "kind": "hunt", "faction": 0,
		"label": EVENT_DOCK_ROLLBACK_AFTER_LABEL, "detail": "", "seq": EVENT_DOCK_ROLLBACK_SEQ}]

## Two rows carrying the SENTINEL `seq` of 0 and differing only in label. Keyed on `seq` they would
## collide onto one; routed to the signature fallback they are two.
const EVENT_DOCK_ZERO_SEQ_ROWS := 2

func _event_dock_zero_seq_fixture() -> Array:
	return [
		{"tick": 61, "kind": "forage", "faction": 0, "label": "An unsequenced row", "detail": "", "seq": 0},
		{"tick": 61, "kind": "forage", "faction": 0, "label": "A second unsequenced row", "detail": "", "seq": 0},
	]

## The band-relabel fixture. The roster knows `band=3` as `Band 1` (its ROSTER POSITION, not its id)
## and `band=30` as `Band 2`, and knows nothing of `band=9`.
##
## **The third row is the DIGIT-BOUNDARY trap, and it is CONSTRUCTED rather than quoted.** The sim
## names exactly one band per label today (`systems::population::push_migration_events` writes
## `"4 left Band 3"`), so no live event reaches the trap — but a plain `String.replace` of `Band 3`
## finds the `Band 3` inside `Band 30` first and corrupts the label to `Band 10`, which is a bug
## waiting for the first label that names two bands (a split or a merge is the obvious next one).
## A fixture that cannot reach the state it claims makes the assertion decorative, so this one
## reaches it. Note the honest limitation it also pins: only the band the `band=` token NAMES is
## substituted — the second band keeps whatever the sim called it.
const EVENT_DOCK_BAND_LABELS := {"3": "Band 1", "30": "Band 2"}
const EVENT_DOCK_RELABELLED := "A child came of age in Band 1"
const EVENT_DOCK_UNKNOWN_BAND_LABEL := "A child came of age in Band 9"
const EVENT_DOCK_DIGIT_BOUNDARY_LABEL := "Four left Band 1 for Band 30"

func _event_dock_band_label_fixture() -> Array:
	return [
		{"tick": 62, "kind": "came_of_age", "faction": 0,
			"label": "A child came of age in Band 3", "detail": "band=3 count=1", "seq": 601},
		{"tick": 62, "kind": "came_of_age", "faction": 0,
			"label": "A child came of age in Band 9", "detail": "band=9 count=1", "seq": 602},
		{"tick": 62, "kind": "migrated", "faction": 0,
			"label": "Four left Band 3 for Band 30", "detail": "band=3 count=4 direction=out", "seq": 603},
	]

## The dock's main fixture — the proposal's own prototype vocabulary, carried on the real wire shape
## (`{tick, kind, faction, label, detail, seq}`). It spans six turns so the log has turn-groups to
## walk, covers all three rungs, both channels' worth of styling, and the three ways a row's accent
## is decided: the kind's own threat style (`predator_raid` ⚔ crimson, `hunt_danger` ⚠ amber), a
## `status=` detail token PROMOTING a routine kind to Alert (`cultivate status=feral`), and the
## plain rung defaults.
##
## The casualty rows carry the sim's REAL wire shape — `killed=` / `wounded=` written with `{:.3}`,
## never a `losses=` key the sim does not have. That fidelity is what gives the trailing-zero scan
## something to catch; a tidier invented fixture made the claim vacuous, and the precondition beside
## it said so out loud.
##
## `seq` is monotonic across the whole array, oldest first, exactly as the sim appends it.
func _event_dock_fixture() -> Array:
	return [
		{"tick": 42, "kind": "forage", "faction": 0, "label": "Foragers returned with 9 provisions", "detail": "", "seq": 1},
		{"tick": 42, "kind": "tame", "faction": 0, "label": "The aurochs herd has grown tame", "detail": "", "seq": 2},
		{"tick": 43, "kind": "born", "faction": 0, "label": "A child was born in Windhollow", "detail": "count=1", "seq": 3},
		{"tick": 43, "kind": "found_settlement", "faction": 0, "label": "Windhollow was settled", "detail": "", "seq": 4},
		{"tick": 44, "kind": "scout", "faction": 0, "label": "Two workers sent to scout the northern ridge", "detail": "", "seq": 5},
		{"tick": 44, "kind": "came_of_age", "faction": 0, "label": "A child came of age in Windhollow", "detail": "count=1", "seq": 6},
		{"tick": 44, "kind": "campaign_milestone", "faction": 0, "label": "Ashfoot has become a hamlet", "detail": "", "seq": 7},
		{"tick": 45, "kind": "corral", "faction": 0, "label": "Corral raised at Ashfoot", "detail": "", "seq": 8},
		{"tick": 45, "kind": "cultivate", "faction": 0, "label": "The upper patch has gone feral", "detail": "status=feral", "seq": 9},
		{"tick": 45, "kind": "expedition_arrived", "faction": 0, "label": "Expedition reached 24,9 — awaiting orders", "detail": "", "seq": 10},
		{"tick": 45, "kind": "died", "faction": 0, "label": "An elder died of cold in Windhollow", "detail": "cause=cold bracket=elders", "seq": 11},
		{"tick": 46, "kind": "hunt", "faction": 0, "label": "Hunters brought back red deer", "detail": "", "seq": 12},
		{"tick": 46, "kind": "born", "faction": 0, "label": "A child was born in Ashfoot", "detail": "count=1", "seq": 13},
		{"tick": 46, "kind": "site_discovered", "faction": 0, "label": "The Weeping Arch", "detail": "category=landmark at=18,31", "seq": 14},
		{"tick": 46, "kind": "hunt_danger", "faction": 0, "label": "The aurochs hunt cost the party three lives", "detail": "killed=3.000 wounded=1.000 species=Aurochs", "seq": 15},
		{"tick": 47, "kind": "sow", "faction": 0, "label": "Barley sown on the river terrace", "detail": "", "seq": 16},
		{"tick": 47, "kind": "forage", "faction": 0, "label": "Foragers returned with 12 provisions", "detail": "", "seq": 17},
		{"tick": 47, "kind": "migrated", "faction": 0, "label": "Four left Ashfoot for Windhollow", "detail": "count=4 direction=out", "seq": 18},
		{"tick": 47, "kind": "came_of_age", "faction": 0, "label": "Two children came of age in Ashfoot", "detail": "count=2", "seq": 19},
		# THE DE-DUPLICATION PAIR — byte-identical apart from `seq`. Two packs, one turn, one band.
		{"tick": 47, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 20},
		{"tick": 47, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 21},
	]

## The PIN fixture: one Alert, deliberately OLD, under enough newer Notable rows that a 4-row bar
## cannot reach it on chronology alone. That is the whole test — the raid must claim the leading slot
## rather than being pushed off by the receipts that followed it.
func _event_dock_pin_fixture() -> Array:
	return [
		{"tick": 40, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 101},
		{"tick": 41, "kind": "came_of_age", "faction": 0, "label": "A child came of age in Ashfoot", "detail": "count=1", "seq": 102},
		{"tick": 42, "kind": "site_discovered", "faction": 0, "label": "The Weeping Arch", "detail": "at=18,31", "seq": 103},
		{"tick": 43, "kind": "died", "faction": 0, "label": "An elder died of cold in Windhollow", "detail": "cause=cold", "seq": 104},
		{"tick": 44, "kind": "migrated", "faction": 0, "label": "Four left Ashfoot for Windhollow", "detail": "count=4 direction=out", "seq": 105},
		{"tick": 45, "kind": "expedition_arrived", "faction": 0, "label": "Expedition reached 24,9 — awaiting orders", "detail": "", "seq": 106},
		{"tick": 46, "kind": "tame", "faction": 0, "label": "The aurochs herd has grown tame", "detail": "", "seq": 107},
	]

## Assert the event bar clears one HUD region — **and that the claim is not vacuous**.
##
## The HUD's regions occupy different vertical bands, so most bar/region pairs share no `y` at all
## and "these two rects do not intersect" is trivially true of them: a BOTTOM bar cannot reach the
## top-bar readouts however wrong its horizontal bound is. A block of such claims passes with the fix
## reverted, which is the failure this guard exists to prevent — so the overlap on the PERPENDICULAR
## axis is required first, and a pair that does not share one fails as VACUOUS rather than passing.
## Every string the dock currently RENDERS — bar rows, log rows, chips, the foot — as flat text.
## The raw-token guard walks this rather than the event records, because the records are supposed to
## hold `key=value`: the claim is about what reaches the screen.
## Does this rendered string carry a trailing-zero decimal — `2.000`, `1.50`? The wire's `{:.3}`
## casualty format produces them and a rendered row must not. Stated as a PROPERTY of any numeric
## word rather than a list of known strings, so a new `{:.N}` field on a future kind is covered
## without an edit here. `is_valid_float` is the precision that matters: without it an endpoint like
## `127.0.0.1:41000` in a system note would read as a padded decimal and fail for nothing.
func _has_padded_decimal(text: String) -> bool:
	for word in text.split(" ", false):
		if word.contains(".") and word.ends_with("0") and word.is_valid_float():
			return true
	return false

func _preview_dock_labels(dock: EventDockPanel) -> Array[String]:
	var found: Array[String] = []
	var stack: Array[Node] = [dock._rows, dock._log_body]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node == null:
			continue
		for child in node.get_children():
			stack.append(child)
		if node is Label:
			found.append((node as Label).text)
	return found

func _assert_bar_clears(dock: EventDockPanel, region: Control, what: String) -> void:
	var bar := dock._root.get_global_rect()
	var box := region.get_global_rect()
	if bar.position.y >= box.end.y or box.position.y >= bar.end.y:
		_assert_hud("VACUOUS — the bar and %s share no vertical band, so 'they do not overlap' claims nothing" % what, false)
		return
	_assert_hud("the bar clears %s (they share a vertical band, so this is a real claim)" % what,
		not bar.intersects(box))

func _assert_hud(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS hud — ", label)
	else:
		push_error("ui_preview: FAIL hud — %s" % label)

# ---- the herd herders_needed FIELD-PAIR guard ---------------------------------------------------
# The sim exports TWO herder counts per herd and the client reads DIFFERENT ones by rung, so a fixture
# that sets only one is a silent lie rather than an error:
#   • `herders_needed` — OWNERSHIP-GATED (`fauna::herd_herders_needed`): 0 unless the herd is
#     corralled or owned. The extractive rungs' field, and what the drawer's "Herders A / N" row reads.
#   • `herders_needed_if_managed` — ownership-INDEPENDENT (`fauna::would_be_herders_needed`): the crew
#     the herd WOULD owe, 0 only for a species that can never be tamed. `DrawerComposeController`'s
#     `_forecast_worker_cap` floor reads THIS one for the INVESTMENT rungs (Tame / Corral).
# `_under_herded_corral_fixture` set only the first; the investment floor therefore read 0, the crew
# cap collapsed to 1, and the frame rendered the exact opposite of the cap it documents — with nothing
# logged anywhere, because both keys are optional and `get(…, 0)` is a legal answer.
#
# THE INVARIANT, from the sim, not from guesswork: `would_be_herders_needed` is identical to
# `herd_herders_needed` except its gate, so the two agree on every herd EXCEPT a not-yet-owned tameable
# one (where the gated field is 0 and the would-be crew is real — that is `_tame_worker_cap_herd_fixture`,
# 0 and 10, deliberately). A herd whose gated count is `> 0` is by definition managed (corralled or
# owned) and therefore tameable, so the ungated field takes the same branch:
#     herders_needed > 0  ⇒  herders_needed_if_managed == herders_needed
# and, in general, `herders_needed_if_managed >= herders_needed`. Pinned sim-side by
# `core_sim/src/snapshot/mod.rs`'s would-be-crew export test (its three cases are exactly these).
const HERDERS_NEEDED_KEY := "herders_needed"
const HERDERS_NEEDED_IF_MANAGED_KEY := "herders_needed_if_managed"
## Deep-scan bound. Fixtures are trees, but a bound turns a future self-referencing one into a stop
## rather than an infinite walk.
const HERD_SCAN_MAX_DEPTH := 8

var _herd_pair_scans := 0
var _herd_pair_violations := 0

## Walk everything reachable from `subject` and check the pair on every dict that carries either half.
## Deliberately a SCAN and not a per-fixture assertion: a guard you have to remember to call for each
## new fixture is the same failure mode as remembering to set the second field.
func _guard_herd_fields(subject: Variant, where: String, depth: int = 0) -> void:
	if depth > HERD_SCAN_MAX_DEPTH:
		return
	if subject is Array:
		for item in (subject as Array):
			_guard_herd_fields(item, where, depth + 1)
		return
	if not (subject is Dictionary):
		return
	var dict: Dictionary = subject
	if dict.has(HERDERS_NEEDED_KEY) or dict.has(HERDERS_NEEDED_IF_MANAGED_KEY):
		_herd_pair_scans += 1
		var needed := int(dict.get(HERDERS_NEEDED_KEY, 0))
		var if_managed := int(dict.get(HERDERS_NEEDED_IF_MANAGED_KEY, 0))
		if if_managed < needed:
			_herd_pair_violations += 1
			push_error(("ui_preview: FAIL herd fields — %s herd \"%s\" declares %s %d but %s %d. "
				+ "The would-be crew can never be SMALLER than the ownership-gated one, and on a herd "
				+ "with herders (i.e. a managed one) the sim exports them EQUAL — the investment rungs' "
				+ "worker cap floors on the second field, so half-setting the pair silently caps the "
				+ "crew at the take-side count.") % [where, String(dict.get("id", "?")),
				HERDERS_NEEDED_KEY, needed, HERDERS_NEEDED_IF_MANAGED_KEY, if_managed])
		elif needed > 0 and if_managed != needed:
			# The OTHER half of the invariant, and the one a `>=` test lets through. The gate is the
			# ONLY difference between the two sim functions, so a NON-ZERO gated count already says the
			# herd passed the gate — it is corralled or owned — and the would-be crew is then computed
			# from the same species and headcount by the same arithmetic. A bigger would-be crew is not
			# a conservative fixture, it is an impossible herd: it claims managing this herd would cost
			# MORE than managing it already does.
			_herd_pair_violations += 1
			push_error(("ui_preview: FAIL herd fields — %s herd \"%s\" declares %s %d and %s %d. Once "
				+ "%s is above zero the herd IS managed, and the would-be crew is the SAME crew — the "
				+ "sim's two functions differ only by the ownership gate this herd has already passed, "
				+ "so they must be EQUAL here. Set both through _set_managed_herders; only a still-WILD "
				+ "tameable herd may carry a larger would-be crew, and its gated count is 0.")
				% [where, String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed, HERDERS_NEEDED_KEY])
	for value in dict.values():
		_guard_herd_fields(value, where, depth + 1)

## Every herd dictionary the HUD is holding as this frame renders — the world list, the selected
## subject and roster, the tile's occupants, and the bands (whose `tile_info` carries herds too).
func _guard_frame_herd_fields(state: String) -> void:
	_guard_herd_fields(_hud._band_labor._world_herds, state)
	_guard_herd_fields(_hud._band_labor._player_band, state)
	_guard_herd_fields(_hud._band_labor._player_bands, state)
	_guard_herd_fields(_hud._band_labor._panel_band, state)
	_guard_herd_fields(_hud._selection._selected_herd, state)
	_guard_herd_fields(_hud._selection._roster_herds, state)
	_guard_herd_fields(_hud._selection._selected_tile_info, state)

## A 4-digit turn — the widest the face has to hold, and the case a fixed font size would clip.
const TURN_ORB_FOUR_DIGIT_TURN := 1200
## The slice the frozen clock is stepped by when driving the orb's resolve animation. It IS the orb's
## own per-frame clamp, and taking it from there is load-bearing rather than tidy: the orb caps how
## much of the animation ONE call may advance, so a harness stepping in bigger slices would silently
## advance less than it asked for and capture the wrong phase.
const TURN_ORB_ANIM_STEP_SEC := TurnOrb.RESOLVE_MAX_STEP_SEC
## Enough steps for the WORST path — the fail-open timeout, then a full scatter and re-form — plus a
## margin, so the cap only trips on an animation that genuinely cannot terminate.
const TURN_ORB_RESOLVE_MAX_STEPS := int((TurnOrb.RESOLVE_TIMEOUT_SEC + TurnOrb.RESOLVE_SCATTER_SEC \
	+ TurnOrb.RESOLVE_REFORM_SEC) / TURN_ORB_ANIM_STEP_SEC) * 2
## Where in a revolution `turn_orb_resolving` is captured: far enough past the scatter that the digits
## are unmistakably OFF their resting places and the sweep arc is unmistakably rotated.
const TURN_ORB_ORBIT_CAPTURE_FRACTION := 0.15

## GUARD: the turn number on the orb face must BE the turn, be sized inside the declared band, and fit
## the face's usable chord. Measured against the button's own font, exactly as `_turn_font_size` does —
## the alternative (eyeballing turn 1200) is how a clipped number ships.
func _assert_turn_face_fits(expected_turn: int) -> void:
	var orb := _hud.turn_orb
	var face: Button = orb._face
	var text := face.text
	var size := face.get_theme_font_size("font_size")
	var font := face.get_theme_font("font")
	var budget: float = TurnOrb.FACE_DIAMETER * TurnOrb.TURN_TEXT_WIDTH_FRACTION
	var width: float = font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, size).x
	var ok := text == str(expected_turn) \
		and size >= TurnOrb.TURN_FONT_SIZE_MIN and size <= TurnOrb.TURN_FONT_SIZE_MAX \
		and width <= budget + 1.0
	_assert_turn_orb("turn %d on the face reads '%s' at %dpx, %.0f of %.0f wide" % [
		expected_turn, text, size, width, budget], ok)

## GUARD: the curved `TURN` word inside the face must not wrap around the circle, must not cross the
## face's edge, and must draw exactly when there IS a number for it to label. Drawn pixels cannot be
## asserted, so assert the ARITHMETIC — via the SAME `turn_word_metrics()` the draw reads, so this
## cannot pass while the renderer computes something else.
func _assert_turn_word_clears() -> void:
	var orb := _hud.turn_orb
	var metrics: Dictionary = orb.turn_word_metrics()
	var arc_angle: float = metrics["arc_angle"]
	var outer_reach: float = float(metrics["radius"]) + float(metrics["glyph_height"])
	var face_radius: float = TurnOrb.FACE_DIAMETER * 0.5
	_assert_turn_orb("curved '%s' spans %.0f° (ceiling %.0f°)" % [
			TurnOrb.TURN_WORD, rad_to_deg(arc_angle), rad_to_deg(TurnOrb.TURN_WORD_MAX_ARC_ANGLE)],
		arc_angle > 0.0 and arc_angle < TurnOrb.TURN_WORD_MAX_ARC_ANGLE)
	_assert_turn_orb("curved '%s' reaches %.1f of the face's %.1f radius" % [
			TurnOrb.TURN_WORD, outer_reach, face_radius], outer_reach <= face_radius)
	# THE VISIBILITY RULE CHANGED, so these two state the new one. Hovering used to swap the number out
	# for the advance glyph and take the word with it; the number now NEVER leaves the face (the hint
	# glyph carries the affordance instead), so hover must not hide the word...
	var was_hovered: bool = orb._face_hovered
	orb._set_face_hovered(true)
	var shown_on_hover := orb._show_turn_word()
	orb._set_face_hovered(was_hovered)
	_assert_turn_orb("curved '%s' stays while the face is hovered" % TurnOrb.TURN_WORD, shown_on_hover)
	# ...and the ONE case where there is no number to label is the resolve animation, which scatters it
	# onto the orbit ring. Driven through the REAL gate (a face click), then settled back.
	var restore_turn: int = orb._turn
	orb._on_face_pressed()
	var hidden_while_scattered := not orb._show_turn_word()
	_assert_turn_orb("curved '%s' hides while the number is scattered" % TurnOrb.TURN_WORD,
		hidden_while_scattered)
	await _settle_turn_orb_resolve(restore_turn + 1)
	_hud.update_overlay(restore_turn, {})
	await _settle()

## Advance the orb's resolve animation by `seconds` of frozen-clock time, in slices the orb will
## actually honour. One big call would be clamped to `RESOLVE_MAX_STEP_SEC` and quietly under-advance.
func _step_turn_orb_anim(seconds: float) -> void:
	var remaining: float = seconds
	while remaining > 0.0:
		var slice: float = minf(remaining, TURN_ORB_ANIM_STEP_SEC)
		_hud.turn_orb._advance_resolve_animation(slice)
		remaining -= slice

## Drive the turn orb out of its resolving gate the way a server answer does — a `set_turn` with a
## DIFFERENT value — and prove the animation actually terminates.
##
## THE CLOCK IS FROZEN HERE (`Engine.time_scale = 0`), so the orb's `_process` sees `delta == 0` and
## the re-form would never finish on its own: the same hazard `_flush_tweens` handles for the one Tween
## in the client. Step the phase machine by a fixed slice instead — deterministic, and it is the REAL
## `_advance_resolve_animation`, so a phase that cannot terminate fails right here instead of hanging
## the orb in the game.
func _settle_turn_orb_resolve(answer_turn: int) -> void:
	var orb := _hud.turn_orb
	_hud.update_overlay(answer_turn, {})
	for _i in range(TURN_ORB_RESOLVE_MAX_STEPS):
		if not orb.is_resolving():
			await _settle()
			return
		orb._advance_resolve_animation(TURN_ORB_ANIM_STEP_SEC)
		await get_tree().process_frame
	push_error("ui_preview: FAIL turn-orb — the resolve gate never lifted in %d steps of %.2fs" % [
		TURN_ORB_RESOLVE_MAX_STEPS, TURN_ORB_ANIM_STEP_SEC])

func _assert_turn_orb(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS turn-orb — ", label)
	else:
		push_error("ui_preview: FAIL turn-orb — %s" % label)

## The instance ids of a container's direct children, so an assertion can prove a restate REUSED the
## same nodes (in-place patch) rather than freeing + recreating them (teardown).
func _child_instance_ids(node: Node) -> Array:
	var ids: Array = []
	if node != null:
		for child in node.get_children():
			ids.append(child.get_instance_id())
	return ids

## The face text of the chip at `index` in the pinned chip strip (each chip is a PanelContainer whose
## first child is its Label).
func _chip_text(strip: Node, index: int) -> String:
	if strip == null or index < 0 or index >= strip.get_child_count():
		return ""
	var chip := strip.get_child(index)
	if chip.get_child_count() == 0:
		return ""
	var label := chip.get_child(0) as Label
	return label.text if label != null else ""

## The forage drawer's standing-summary text (the first child of `%ForageAssignControls` is the
## summary HFlowContainer; its first child is the main status Label).
func _forage_summary_text() -> String:
	var controls := _hud.forage_assign_controls
	if controls == null or controls.get_child_count() == 0:
		return ""
	var flow := controls.get_child(0)
	if flow.get_child_count() == 0:
		return ""
	var label := flow.get_child(0) as Label
	return label.text if label != null else ""

## A NON-player band (faction 1): what a rival's cohort actually looks like on the wire — an identity,
## a size, a position, and nothing of ours to read (no morale/output/labor/flow fields). Backs the
## `band_foreign` state, which exists to prove the drawer doesn't collapse to an empty card now that
## the identity rows moved into the roster row.
func _foreign_band_fixture() -> Dictionary:
	return {
		"id": "Ashen Kin",
		"size": 96,
		"entity": 977,
		"faction": 1,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		"activity": "forage",
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Prairie Steppe",
			"visibility_state": "active",
		},
	}

func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"size": 148,
		"entity": 904,
		"faction": 0,
		"pos": [71, 18],
		# Good food state: a long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the
		# Food line reads "… · +0.26 /turn" and the category breakdown is collapsed (clickable open).
		"turns_of_food": 22.0,
		# Good morale (≥ warn, not falling) → the Morale row is collapsed with a ▸ caret. The signed
		# Layer-1 contributions (above the breakdown epsilon) give the disclosure real content on expand.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		# Thriving growth (docs/plan_population_growth_model.md): fed (hunger 1.0, so that factor is
		# neutral and its row is omitted), a saturated larder (reserve 1.5) and net-positive food
		# (trend 1.25) → 1.0 × 1.5 × 1.25 = 188% of normal. Reads neutral ink — normal growth is
		# normal, not a "good" — and its disclosure shows what is HELPING, which is the good-state
		# case the row must still be openable in.
		"fertility_hunger": 1.0,
		"fertility_reserve": 1.5,
		"fertility_trend": 1.25,
		"stores": {"provisions": 84.0},
		# Early-Game Labor (slice 3b): 16 working-age workers, 3 idle, split across a
		# Forage tile, a Hunt herd, and the Scout + Warrior band-wide roles.
		"working_age": 16,
		"idle_workers": 3,
		# Server's hard party-size cap (expedition config, default 8) — the outfit stepper maxes at
		# min(idle, this).
		"max_expedition_party_size": 8,
		# Global config levers echoed on every cohort. They are DISPLAY levers — neither computes
		# a trip length. The targeting banner's turns-to-fill is a PURE LOOKUP into the target herd's
		# `hunt_trip_estimates` (the sim forward-simulates the trip and exports the answer); the client
		# does ZERO arithmetic for an expedition and never divides a carry cap by a rate.
		#   expedition_viability_warn_turns — the viable/not-viable threshold applied to turns_to_fill.
		#   hunt_per_worker_provisions      — one hunter's throughput, used ONLY by the resident-band
		#     LOCAL hunt preview, which IS arithmetic: min(workers × 0.8, band_ceiling) × output_mult.
		# Band = flow arithmetic; expedition = lookup.
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0): the forecast shows the HAUL a filled pack delivers as
		# party × this (blessed party×lever arithmetic, NOT the turns-to-fill lookup).
		"expedition_per_worker_carry": 4.0,
		"work_range": 2,
		# Hunt reach (work_range + hunt leash) — large enough here that BOTH the reference herd_fixture
		# (9 tiles from this band's pos) and the occupied-hex herd (16 tiles) stay WITHIN reach, so those
		# herd states render the LOCAL "Hunt Here" controls (the far-herd expedition path has its
		# own dedicated fixtures, _hunt_distance_bands).
		"hunt_reach": 16,
		"scout_reveal_radius": 2,
		"activity": "forage",
		# Band food flow (Food summary line): total income across the worked sources vs the cohort's
		# consumption. Net = 0.94 − 0.68 = +0.26 (positive → larder growing), shown green on the Food
		# line. Per-source actual/sustainable yields live on the assignments below.
		# The Gathered/Hunted breakdown sums the assignment actual_yields (0.48 / 0.46) by kind.
		"food_income": 0.94,
		"food_consumption": 0.68,
		# `workers_needed` is the overstaffing axis, INDEPENDENT of the overdraw (⚠) axis — the two
		# rows below deliberately cross them so one frame proves both, AND proves the ⚠ now keys off the
		# sim-answered `overdraws` bool, not the client-derived `actual > sustainable`:
		#   • forage: 5 assigned but only 1 needed (the patch's ceiling caps the take) → the amber
		#     "· only 1 of 5 working" note, and NO ⚠ (Sustain patch, overdraws=false).
		#   • hunt: 4 assigned, 4 needed → no overstaff note. `actual_yield 0.46 > sustainable_yield 0.20`
		#     (a banked whole animal cashed on this KILL turn), yet `overdraws=false` under Sustain → the
		#     row reads CLEAN, NO ⚠. Under the old client test this row false-tripped the flag — the fix.
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "workers_needed": 4, "overdraws": false},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Freshwater Marsh",
			"tags_text": "Freshwater, Wetland",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A band that KEEPS A CORRAL: the third term of the food ledger. Its one keeper works the penned
## Red Deer herd (the sim pays the pen's GROSS managed yield, 5.40), and the herd eats 1.74/turn off
## the band's larder — `pen_feed_upkeep`, exported by the sim (`PopulationCohortState.penFeedUpkeep`)
## precisely so the client never has to sum it. Numbers are the design doc's measured Red Deer pen at
## its escapement operating point (B* = K/2): gross 5.40, feed 1.74, net 3.66.
func _pen_keeper_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = PEN_KEEPER_BAND_ENTITY
	band["id"] = "Band 4"
	band["turns_of_food"] = 22.0
	band["food_income"] = 5.88          # forage 0.48 + the pen's gross 5.40
	band["food_consumption"] = 1.15     # the PEOPLE's meals
	band["pen_feed_upkeep"] = 1.74      # the ANIMALS' feed — a debit in neither row above
	band["fodder_store"] = 12.4         # the band's HAY larder (Flora roster F3) — feeds the pen
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		# A managed source: one keeper, take == sustainable (escapement); Corral is managed, so the
		# sim-answered `overdraws` is false → no ⚠ and no overstaff note.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 5.40, "sustainable_yield": 5.40, "workers_needed": 1, "overdraws": false},
		{"kind": "scout", "workers": 2},
	]
	return band

## The SAME pen, underfed: the band's income has collapsed (a shrinking herd yields less — gross
## 1.90) and it could hand over only 0.70 of the 1.74 the herd demanded. `pen_feed_upkeep` is what
## was actually PAID (the sim's `LocalStore::take` partial-payment primitive), so the ledger still
## balances against the larder; the herd carries the shortfall as `pen_fed_fraction` 0.40.
## Net = 1.32 − 1.15 − 0.70 = −0.53 — the death spiral the readout exists to make visible: the herd
## shrinks, so it yields less, so there is less to feed it with.
func _starving_pen_band_fixture() -> Dictionary:
	var band := _pen_keeper_band_fixture()
	band["turns_of_food"] = 3.0
	band["food_income"] = 1.32          # forage 0.48 + the shrunken pen's 0.84
	band["pen_feed_upkeep"] = 0.70      # PAID, not demanded — the herd starves for the difference
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
		# BOTH PRODUCTS on the drawer's standing summary (issue #337): the hide sells beside the meat,
		# so the one-line summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade only because
		# it is non-zero. It comes from the SAME `SourceForecast.source_yield_readout` the Band panel's
		# rows use, so the two surfaces cannot state different products for one assignment.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 0.84, "sustainable_yield": 0.84, "workers_needed": 1, "overdraws": false, "trade_yield": 0.12, "realized_trade_yield": 0.12},
		{"kind": "scout", "workers": 2},
	]
	return band

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) and a
## low larder runway (4 days). Both trip `DetailFormat.food_is_concerning`, so the category breakdown auto-shows
## under a red net figure without any click.
## The band the growth model exists for: its income has collapsed and it is now eating short off a
## nearly-empty larder. All three factors are off neutral at once, which is what makes this the frame
## that proves the breakdown MULTIPLIES out to its headline (0.60 × 1.05 × 0.25 = 0.16 → "16% of
## normal", below `fertility.critical` → a RED row under a WARN caret).
##
## It is derived from the concerning-food band rather than being that band: a band four turns from
## empty is still eating FULL today, so a `hunger` below 1.0 there would be an incoherent fixture.
func _collapsed_growth_band_fixture() -> Dictionary:
	var band := _concerning_food_band_fixture()
	band["turns_of_food"] = 1.0
	band["stores"] = {"provisions": 0.6}
	band["fertility_hunger"] = 0.60    # ate 60% of what it wanted
	band["fertility_reserve"] = 1.05   # almost nothing banked
	band["fertility_trend"] = 0.25     # income gone — the shipped deficit floor
	return band

## A band whose fertility has NOT been projected — a rehydrated cohort, before the next tick. The
## sim publishes the all-zero not-projected sentinel (a computed `reserve` is ≥ 1 by construction, so
## a zero reserve cannot be a real reading), and the drawer must answer with NO Growth row rather
## than a fabricated 0%.
func _unprojected_growth_band_fixture() -> Dictionary:
	var fixture := _band_fixture()
	fixture["fertility_hunger"] = 0.0
	fixture["fertility_reserve"] = 0.0
	fixture["fertility_trend"] = 0.0
	return fixture

func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 905
	band["id"] = "Band 3"
	band["turns_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15, "overdraws": false},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20, "overdraws": false},
		{"kind": "scout", "workers": 2},
	]
	return band

## A scouting expedition (docs/plan_exploration_and_sites.md §2) in its awaiting-orders phase:
## a detached party (is_expedition) carrying a mission/phase + party size + provisions. The drawer
## renders the dedicated expedition readout + Recall/Move panel, not the labor-allocation UI.
func _expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"size": 6,
		"entity": 7001,
		"faction": 0,
		"pos": [80, 30],
		"turns_of_food": 9.0,
		"stores": {"provisions": 48.0},
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "awaiting",
		"tile_info": {
			"x": 80, "y": 30,
			"terrain_label": "Highland Tundra",
			"tags_text": "Cold, Exposed",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party following a
## migratory herd. mission "hunt" + a target herd + carried food (its own kills). The drawer renders
## the hunt readout (target herd + carried food + phase) + Recall/Move.
func _hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"size": 5,
		"entity": 7101,
		"faction": 0,
		"pos": [64, 22],
		"turns_of_food": 4.0,
		# Carried 8 of a 16 carry cap → "Carried 8 / 16".
		"stores": {"provisions": 8.0},
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_hunt_policy": "surplus",
		"expedition_carry_cap": 16.0,
		# In-flight next-delivery forecast: 12 food arrives in 6 turns. Surplus is one-shot, so the
		# party folds home after delivering → not recurring (no ↻).
		"expedition_eta_turns": 6,
		"expedition_projected_delivery": 12.0,
		"expedition_recurring": false,
		"tile_info": {
			"x": 64, "y": 22,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A well-fed band whose morale has collapsed on a harsh tile: food is not limited
## (∞) but morale 0.22 sits below the critical threshold, so the Morale row reads red.
func _low_morale_band_fixture() -> Dictionary:
	var fixture := _band_fixture()
	fixture["id"] = "Band 5"
	fixture["entity"] = 905
	fixture["turns_of_food"] = 999.0
	fixture["stores"] = {"provisions": 260.0}
	fixture["morale"] = 0.22
	# Falling morale driven by the harsh cavern terrain: the drawer shows
	# "Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)".
	fixture["morale_delta"] = -0.010
	fixture["morale_cause"] = 1  # Terrain
	# Civilization Wellbeing (docs/plan_civ_wellbeing.md): discontent drags Output to 56%
	# (< critical → red), and the four signed Layer-1 contributions (sum = morale_delta)
	# drive the itemized breakdown. People are relocating (last_emigrated > 0).
	fixture["output_multiplier"] = 0.56
	fixture["discontent_fraction"] = 0.44
	fixture["last_emigrated"] = 6
	fixture["morale_settling"] = 0.010   # +1.0%  settling (positive base growth)
	fixture["morale_terrain"] = -0.012   # −1.2%  harsh terrain
	fixture["morale_climate"] = -0.008   # −0.8%  harsh climate
	fixture["morale_unrest"] = 0.0       # below epsilon → row omitted
	# Its GROWTH, by contrast, is fine — fed, well-stocked, income covering the drain (1.0 × 1.50 ×
	# 1.0 = 150% of normal, so only the reserve row lists). That contrast is the point of having it
	# here: births are morale-INDEPENDENT in this model, so a miserable band on harsh ground must not
	# read as a band that has stopped breeding.
	fixture["fertility_hunger"] = 1.0
	fixture["fertility_reserve"] = 1.50
	fixture["fertility_trend"] = 1.0
	fixture["tile_info"] = {
		"x": 44, "y": 61,
		"terrain_label": "Karst Cavern Mouth",
		"tags_text": "Subsurface, Harsh",
		"visibility_state": "active",
		# Cavern habitability (~0.0825) lands in the Harsh band → amber Tile-card row.
		"habitability": 0.0825,
		# High-latitude cold ~-2° → "Polar" climate band (neutral Tile-card row).
		"temperature": -2.0,
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## Prior-snapshot band sizes so the "losing population" alert has a baseline to
## compare against (Band Ash drops 90 → 78 in the live fixture below).
func _band_alert_baseline() -> Array:
	return [
		{"faction": 0, "entity": 101, "size": 60, "turns_of_food": 12.0, "activity": "harvest", "current_x": 71, "current_y": 18},
		{"faction": 0, "entity": 102, "size": 90, "turns_of_food": 999.0, "activity": "hunt", "current_x": 40, "current_y": 22},
		{"faction": 0, "entity": 103, "size": 45, "turns_of_food": 999.0, "activity": "harvest", "current_x": 12, "current_y": 9},
	]

func _band_alert_fixture() -> Array:
	return [
		# Starving: 3 turns of food (< critical) → red alert.
		{"faction": 0, "entity": 101, "size": 60, "turns_of_food": 3.0, "activity": "harvest", "current_x": 71, "current_y": 18,
			"harvest": {"band_label": "Band Fen"}},
		# Losing population to relocation: size 90 → 78, well-fed (∞) but discontented and
		# 12 people emigrated last turn → amber alert "losing population — people leaving".
		{"faction": 0, "entity": 102, "size": 78, "turns_of_food": 999.0, "morale": 0.30, "morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 40, "current_y": 22,
			"harvest": {"band_label": "Band Ash"}},
		# Idle labor: quiet low-priority alert.
		{"faction": 0, "entity": 103, "size": 45, "turns_of_food": 999.0, "activity": "idle", "current_x": 12, "current_y": 9},
	]

## Two player bands (multi-band split is deferred, but the assign controls' band-picker must
## handle N). Different idle_workers so switching the dropdown visibly re-caps the worker
## stepper; neither hunts the deer herd, so the cap for a fresh source == idle_workers.
func _two_player_bands() -> Array:
	# hunt_reach 6 keeps both bands WITHIN local reach of the (66,10) herd (distances 0 and 3), so the
	# band-picker states test the LOCAL-hunt re-cap (the distance-aware expedition path is exercised by
	# _hunt_distance_bands below).
	return [
		{"entity": 801, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 12, "hunt_reach": 6, "activity": "forage", "labor_assignments": []},
		{"entity": 802, "faction": 0, "size": 40, "current_x": 68, "current_y": 12,
			"working_age": 6, "idle_workers": 2, "hunt_reach": 6, "activity": "hunt", "labor_assignments": []},
	]

## Distance-aware herd-hunt (docs/plan_exploration_and_sites.md §2b): two player bands at DIFFERENT
## distances from ONE herd — a NEAR band ON the herd tile (within hunt_reach → LOCAL hunt) and a FAR
## band ~27 tiles away (beyond reach → hunting EXPEDITION). Proves the SELECTED band (band-picker)
## drives the local-vs-expedition label + command + band-entity target — the case single-band
## playtest can't surface. Both carry idle workers + a party cap so either verb is dialable.
func _hunt_distance_bands() -> Array:
	return [
		{"entity": 811, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "forage", "labor_assignments": []},
		{"entity": 812, "faction": 0, "size": 80, "current_x": 86, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "hunt", "labor_assignments": []},
	]

## Range-aware forage: two player bands at DIFFERENT distances from the (66,10) food tile — a NEAR band
## 1 tile away (within work_range 2 → forage ENABLED) and a FAR band ~21 tiles away (beyond range →
## forage DISABLED + out-of-range hint). Foraging is stationary gathering, so out-of-range has NO
## expedition fallback — just a disabled button. Proves the SELECTED band (band-picker) drives the
## enabled-vs-disabled state — the case single-band playtest can't surface.
func _forage_range_bands() -> Array:
	return [
		{"entity": 821, "faction": 0, "size": 120, "current_x": 67, "current_y": 10,
			# **THE IDLE COUNT HAS TO CLEAR THE DIPPED BUILD CREW.** `improvement_build_crew` asserts the
			# stepper reaches the sim's own `workers_needed` (12 since the dip moved onto the crew), and
			# the stepper caps at `idle + already staffed` — so 10 idle pinned it one short and the frame
			# would have failed on the labour bound rather than on the thing it is testing.
			"working_age": 20, "idle_workers": 16, "work_range": 2, "activity": "forage", "labor_assignments": []},
		{"entity": 822, "faction": 0, "size": 80, "current_x": 80, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "work_range": 2, "activity": "forage", "labor_assignments": []},
	]

## The near band of `_forage_range_bands`, ALREADY WORKING the (66,10) food tile — the fixture behind
## the drawer's standing-assignment summary (§14). The assignment deliberately crosses the two
## INDEPENDENT flags the summary shares with a Band-panel Current-actions row: `overdraws` true (a
## Deplete patch drawing past regrowth — the ecological ⚠) AND 4 workers where 2 are needed (the labor
## "· only 2 of 4 working" note). `realized_yield` is the steady average the summary headlines.
## The near band of `_forage_range_bands`, ALREADY WORKING the (66,10) food tile at a MODEST staffing —
## the fixture behind the compose sheet's UNASSIGN state. Deliberately separate from
## `_standing_forage_band_fixture`, whose assignment is tuned to trip the drawer summary's overdraw and
## overstaff flags; this one is a plain, healthy Cultivate crew, so the unassign frame is judged on the
## button/forecast pair and nothing else.
##
## It is also the fixture behind the two STANDING-BUT-GATED frames (issue #420), which is why the tile
## is a PARAMETER: a standing assignment is matched by TILE, so a frame selecting a patch other than
## the (66,10) reference — the finished Tended Patch at (67,11) — would read as UNSTAFFED there, i.e.
## exactly the "not standing" case those frames must not render. Both defaults keep every existing
## caller on the reference tile.
## **`workers_needed` IS THE SIM'S OWN ANSWER, AND IT IS WHAT THE COMPOSE CAP IS JUDGED AGAINST.**
## Derived here by the sim's rule rather than picked, so the assertion on `improvement_build_crew` has a
## control it did not write itself. For this patch under Sustain + Cultivate
## (`_food_tile_fixture`: per-worker 0.32, Sustain ceiling 0.96, cultivate fraction 0.25, crew 2):
##   take        = min(w × 0.32 × 0.25, 0.96)       (`forage::forage_take` — **THE DIP RIDES THE CREW**)
##   take crew   = ceil(0.96 / (0.32 × 0.25)) = 12  (`systems::labor::workers_needed_for_take`)
##   workers_needed = max(build crew 2, take crew 12) = 12  (`systems::labor::source_crew_needed`)
## **THE NUMBER QUADRUPLED when the dip moved off the ceiling** (`docs/plan_harvest_floor.md` §3.1),
## and that is its whole player-visible consequence: a crew big enough to saturate the source's stock
## pays no dip at all, so the remedy for a slow build is HANDS — at a 25% carry, four times as many.
## It read `2` under the dipped ceiling and `1` before either half of that existed.
func _cultivating_forage_band_fixture(x: int = 66, y: int = 10) -> Dictionary:
	var band: Dictionary = _forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 1, "target_x": x, "target_y": y, "floor": 0.5,
		"improvement": "cultivate",
		"actual_yield": 0.08, "sustainable_yield": 0.96, "realized_yield": 0.08,
		"workers_needed": CULTIVATE_SIM_WORKERS_NEEDED, "overdraws": false,
	}]
	return band

func _standing_forage_band_fixture() -> Dictionary:
	var band: Dictionary = _forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 4, "target_x": 66, "target_y": 10, "floor": 0.15,
		"actual_yield": 2.74, "sustainable_yield": 0.96, "realized_yield": 2.74,
		"workers_needed": 2, "overdraws": true,
	}]
	return band

## The herd the distance-aware states select — the same (66,10) herd but a NON-food tile_info, so the
## Tile card drops its "Assign foragers" block and the hunt button + distance hint sit in-frame.
##
## **IT CARRIES A RAID TABLE, and without one the expedition frames judge nothing about the trip.**
## `_herd_fixture` publishes the BAND's flow ceilings and no `hunt_trip_estimates`, so every expedition
## sheet opened on it answered `available: false` and rendered no forecast at all — a state a live herd
## cannot be in (the sim exports an estimate row for every huntable herd) and the one state in which
## every claim about the trip readout would pass vacuously. The counts are the reference deer's own
## `food_per_animal` 2.0 through `_raid_estimate_table`, so the payload is `animals × 2` food beside
## `animals × RAID_TRADE_PER_ANIMAL` trade — both accounts positive, which is what makes the
## zero-account frame beside it (`_partial_waste_mammoth`, no trade at all) a real contrast.
func _hunt_distance_herd() -> Dictionary:
	var herd := _herd_fixture()
	herd["tile_info"] = _plain_herd_tile_info()
	herd["hunt_trip_estimates"] = _raid_estimate_table(
		DISTANCE_RAID_TURNS, DISTANCE_RAID_ANIMALS, float(herd["food_per_animal"]))
	return herd

## A Wild Boar carrying the server's MEASURED raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter →
## 5 animals / 7 turns, 2 → 8 / 8, 3 → 8 / 4. `animalsTaken` plateaus at 8 (party 2), so max-useful = 2.
## The frame the "delivers ≈5 Wild Boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged
## on. `food_per_animal` = 4 so the readout appends the food total (~20 at 5 animals, ~32 at 8).
func _raid_boar_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_boar_04", "Wild Boar", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = BOAR_FOOD_PER_ANIMAL
	herd["hunt_trip_estimates"] = _raid_estimate_table(
		BOAR_RAID_TURNS, BOAR_RAID_ANIMALS, BOAR_FOOD_PER_ANIMAL)
	return herd

## A raid estimate TABLE from a per-party Sustain (turns, animals) pair (index i = a party of i+1). The
## deeper policies raid to a lower floor, so they take MORE animals (Surplus < Deplete < Eradicate) — the
## per-policy ASCENDING the picker buttons read. **Eradicate DELIVERS** — it takes the most animals and
## banks the whole-stock windfall (issue #337 redefined `delivers_food`: it means the QUARRY IS EDIBLE,
## not "this rung is a denial mission", and a boar is edible on every rung). Every cell also carries the
## trade twin, since a hunt pays a VECTOR: `delivers_trade` + `delivered_trade = animals × tpa`.
## The per-policy bumps are illustrative fixture data; the live sim exports the real per-floor counts.
func _raid_estimate_table(turns_row: Array, animals_row: Array, fpa: float,
		tpa: float = RAID_TRADE_PER_ANIMAL) -> Dictionary:
	var table := {}
	for i in animals_row.size():
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × fpa, waste 0.
		# delivered_food is the PRIMARY payload the client headlines + the field the max-useful scan and
		# "too lean" test read — every cell must carry it.
		for entry in [["sustain", 0], ["surplus", 2], ["deplete", 3], ["eradicate", 5]]:
			var animals: int = base + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * tpa, "wasted_food": 0.0,
			}
	return table

## A raid herd whose max-useful party DIFFERS BY POLICY, to prove the labor-bound note's "of M" tracks
## the selected policy: Sustain's animalsTaken keeps rising through a party of 4 (then plateaus), Deplete's
## through a party of 7. A band that can field only 3 hunters is labor-bound under BOTH — so the note reads
## "3 of 4 useful" on Sustain and "3 of 7 useful" on Deplete, the same herd, only the policy changed.
func _labor_bound_raid_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_bison_09", "Steppe Bison", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = 4.0
	var sustain_animals := [3, 5, 7, 9, 9, 9, 9, 9]     # plateau at party 4
	var surplus_animals := [4, 6, 8, 10, 12, 12, 12, 12] # plateau at party 5
	var deplete_animals := [5, 7, 9, 11, 13, 15, 17, 17]  # plateau at party 7
	var fpa := 4.0    # matches food_per_animal above; clean raid → delivered = animals × fpa, waste 0
	var table := {}
	for i in sustain_animals.size():
		var w := i + 1
		# Every rung DELIVERS, Eradicate included (issue #337 — `delivers_food` is about the species,
		# and a bison is edible on every rung). Both products on every cell.
		for entry in [["sustain", sustain_animals[i], 8], ["surplus", surplus_animals[i], 6],
				["deplete", deplete_animals[i], 5], ["eradicate", int(deplete_animals[i]) + 2, 4]]:
			var animals: int = int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": int(entry[2]), "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals, "delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0}
	herd["hunt_trip_estimates"] = table
	return herd

## A herd stripped to its policy floor: EVERY (policy, party) cell delivers 0 animals, so the raid comes
## home empty at any size — the one non-viable case (surplus is a property of the HERD, not the party, so
## no party size fixes it). The button must be DISABLED with the "too lean to raid" reason.
func _no_surplus_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_rabbit_02", "Rabbit Warren", "thriving", 0.05, 0, 0)
	herd["size_class"] = "small"
	# The herd is at its floor: no surplus at ANY party size → delivered_food 0 everywhere, so the raid
	# comes home empty and the button DISABLES ("too lean — no surplus above this policy's floor").
	var table := {}
	for w in range(1, 9):
		# The species is edible and its pelts sell — it is the HERD that has nothing left, so both
		# `delivers_*` flags are true on every rung and BOTH payloads are 0. That is what makes this
		# the "too lean" case rather than the "denial mission" one (issue #337).
		for policy in ["sustain", "surplus", "deplete", "eradicate"]:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": 0, "delivered_food": 0.0, "delivered_trade": 0.0,
				"wasted_food": 0.0,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## A hex in a given SIGHT state, deliberately carrying a herd in ALL THREE — including the unseen
## ones, where MapView would never have put one (it fog-gates `_herds_on_tile` at source). Feeding the
## HUD a "leaky" dict on purpose proves the HUD's own gate: on a Discovered/Unexplored hex it must
## refuse to list the herd and must say the contents are unknown, rather than showing an empty roster
## (which would read as "nothing here" — the exact lie this slice exists to kill).
func _sight_tile_fixture(visibility_state: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["herds"] = [_herd_fixture()]
	tile["herd_count"] = 1
	return tile

## YOUR OWN scouting expedition standing on an UNEXPLORED hex — the case the fog rule must NOT break.
## The tile carries the party AND a herd; the herd is redacted (nobody can see it), but the party stays.
func _own_expedition_unexplored_tile() -> Dictionary:
	var tile := _sight_tile_fixture(VIS_UNEXPLORED)
	tile["units"] = [_expedition_fixture()]
	tile["unit_count"] = 1
	return tile

## A FOREIGN band (faction 1) on a hex in the given sight state. On an unseen hex it must vanish from
## the roster (it is not ours); on a visible hex it lists normally with a neutral dot.
func _foreign_band_tile(visibility_state: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["units"] = [{
		"id": "Rival Band",
		"entity": 6001,
		"faction": 1,
		"size": 63,
		"pos": [66, 10],
		"activity": "forage",
	}]
	tile["unit_count"] = 1
	return tile

## A NON-food hex under the herd, so the Tile card drops its "Assign foragers" block and the herd's
## assign controls (stepper + policy + forecast + button) sit fully in-frame.
func _plain_herd_tile_info() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## The herd-panel EXPEDITION forecast states (herd beyond hunt_reach), each also naming the composed
## POLICY — because the policy is half the key (`"<policy>:<party_workers>"`) the forecast looks up in
## the herd's `hunt_trip_estimates`. Re-deriving a Surplus trip from the BAND's flow ceiling instead of
## reading the sim's row was the bug these cover.
func _hunt_assign_forecast_states() -> Array:
	return [
		{
			# THE PARTIAL-WITH-WASTE case: a Thunder Mammoth is big game (16 food/animal), and a party of
			# 4 can't carry a whole one — it kills the 1-animal surplus and hauls only 4 food, wasting 12.
			# So the line reads a brisk-but-lossy "delivers ≈1 Thunder Mammoth over ≈6 turns · ~4 food ·
			# ⚠ 75% wasted" (cyan headline + amber waste), and the button STAYS ENABLED (a partial is a
			# real delivery, the waste % is just informative). This is the case the whole pass exists for.
			"name": "herd_hunt_forecast_viable",
			"floor": 0.5,
			"herd": _partial_waste_mammoth(),
		},
		{
			# A SLOW raid: Sustain on a Red Deer still delivers ≈6 animals, but over 54 turns — past the
			# band's warn threshold (20) → amber "⚠ … — a slow raid" + "Send Anyway (≈54 turns)".
			"name": "herd_hunt_forecast_slow",
			"floor": 0.5,
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# The SAME Red Deer on Surplus: a Surplus raid strips deeper (≈12 animals) and comes home in
			# ~6 turns — a brisk, richer raid. Reading the sim's row, never re-deriving it.
			"name": "herd_hunt_forecast_surplus",
			"floor": 0.3,
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# No surplus: a collapsing Wild Fowl flock is at/below its floor → animalsTaken = 0, the raid
			# returns empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button.
			"name": "herd_hunt_forecast_no_surplus",
			"floor": 0.5,
			"herd": _assign_preview_herd("game_fowl_03", "Wild Fowl", "collapsing", 0.0,
				NEVER_FILLS_TRIP_TURNS, NEVER_FILLS_TRIP_TURNS,
				NO_SURPLUS_ANIMALS, NO_SURPLUS_ANIMALS),
		},
		{
			# Eradicate DELIVERS (#337): every rung is paid the species' yield vector, so this row carries
			# a real payload and the client must NOT read a denial off the policy string. A denial is a
			# quarry that pays neither product — see `_pelt_only_wolf_raid_herd` for the inedible case.
			"name": "herd_hunt_forecast_eradicate",
			"floor": 0.0,
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
	]

## The partial-with-waste raid herd: a Thunder Mammoth (16 food/animal) whose standing surplus is ONE
## animal. Any fieldable party kills that 1 animal but cannot carry a whole mammoth — a party of `w` hauls
## ~`w` food and wastes the rest — so `delivered_food` rises with party size while `animals_taken` stays 1.
## At the composed party of 4: delivered 4, wasted 12 → 75% wasted, button ENABLED. The per-policy turns
## descend Sustain(6) > Surplus(4) > Deplete(3) > Eradicate(2) so the picker's max-food/turn caps read
## ASCENDING. This is
## exactly the case the old `animals_taken`-based "too lean" test and plateau scan got wrong (a leading 1).
func _partial_waste_mammoth() -> Dictionary:
	var herd := _assign_preview_herd("game_mammoth_11", "Thunder Mammoth", "thriving", 2.7,
		MAMMOTH_SUSTAIN_TRIP_TURNS, MAMMOTH_SURPLUS_TRIP_TURNS,
		MAMMOTH_SUSTAIN_ANIMALS, MAMMOTH_SUSTAIN_ANIMALS)
	var fpa := MAMMOTH_FOOD_PER_ANIMAL
	herd["food_per_animal"] = fpa
	# Eradicate rides the SAME loop as the other three (#337): it is paid the species' yield vector like
	# every rung, so a mammoth is edible on Eradicate too. It merely raids fastest (2 turns), which keeps
	# the picker's max-food/turn caps ascending. It used to carry a hand-built `delivers_food = false`
	# cell — a denial state the sim can no longer produce for an edible quarry.
	var policy_turns := {"sustain": 6, "surplus": 4, "deplete": 3, "eradicate": 2}
	var table := {}
	for w in range(1, 9):
		var delivered := minf(float(w), fpa)     # each hunter hauls ~1 food of the 16-food kill
		for policy in policy_turns:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": int(policy_turns[policy]), "delivers_food": true,
				"animals_taken": 1, "delivered_food": delivered, "wasted_food": fpa - delivered,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## A forecast herd (carrying BOTH sim-exported per-policy ceiling tables) as a SELECTED herd — i.e. on
## a plain tile, the way `show_herd_selection` receives it — rather than as a hovered hex.
func _assign_preview_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int, surplus_trip_turns: int,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	var herd := _forecast_herd(id, species, phase, sustain_ceiling, trip_turns, surplus_trip_turns,
		sustain_animals, surplus_animals)
	herd["huntable"] = true
	herd["tile_info"] = _plain_herd_tile_info()
	return herd

## The band the herd-panel EXPEDITION preview states staff: it carries the forecast levers (the global
## config values echoed on every cohort) and sits at (86,24) — ~27 tiles from the (66,10) herd, beyond
## its hunt_reach 7, so every herd resolves to the expedition branch.
func _hunt_preview_far_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 831, "faction": 0, "size": 80,
		"current_x": 86, "current_y": 24, "pos": [86, 24],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0) → the forecast's HAUL = party × this.
		"expedition_per_worker_carry": 4.0,
		"activity": "forage", "labor_assignments": [],
	}

## A band 8 tiles from the (66,10) herd (beyond hunt_reach 7 → expedition) carrying a MOVE RATE, so the
## raid forecast's round-trip travel is exercised: ceil(2 × 8 / 2) = 8 travel turns added to the hunting
## turns. `band_move_tiles_per_turn` now ships on the wire (schema slot 124) and is decoded onto the band;
## this carries the same value the decoder surfaces.
func _raid_travel_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 833, "faction": 0, "size": 80,
		"current_x": 66, "current_y": 18, "pos": [66, 18],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		"expedition_per_worker_carry": 4.0,
		"band_move_tiles_per_turn": 2,
		"activity": "forage", "labor_assignments": [],
	}

## The band the herd-panel LOCAL preview states staff: it sits ON the (66,10) herd (distance 0 ≤ reach
## 7 → local branch) and runs at a REDUCED `output_multiplier` (0.9), so the yield preview visibly
## applies the band's morale/discontent productivity modifier — the one term that makes a resident
## hunt's take differ from an expedition's.
func _hunt_preview_local_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 832, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 14, "idle_workers": 10,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 0.9,
		"activity": "hunt", "labor_assignments": [],
	}

## The oracle band for the carry-aware delivered/waste preview: per-worker 0.8, output 1.0 (so the
## rendered numbers match the spec oracle EXACTLY — no morale modifier muddying them), sitting ON the
## herd (local branch), with plenty of idle workers so the big-game auto-max (20 carriers) isn't
## labor-bound.
func _delivered_oracle_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 840, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": 26,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	}

## THE BUILDING HERD — a Steppe Runner mid-TAME, at the shipped rates (see the `HERD_DIP_*` block).
## It is the ONLY fixture on either web where the build dip changes the SHAPE of the take rather than
## its size: four hunters carry one whole body, the same four gentling the herd carry two thirds of
## one, and `quantise_animal_take`'s `max(1, carryable)` turns that shortfall into a kill they cannot
## haul home.
##
## It states its terms in the MODERN wire vocabulary (stock, capacity, the per-biomass vector) rather
## than as a legacy per-stance table, so `_floorify_ceilings` leaves every number exactly as authored —
## which is what lets the assertions recompose the sim's own take from them.
func _building_herd_fixture() -> Dictionary:
	return {
		"id": "game_runner_09", "label": "Steppe Runners (game_runner_09)",
		"species": "Steppe Runners", "size_class": "migratory",
		"huntable": true, "ecology_phase": "thriving",
		# Pastoral ceiling + a part-filled meter: Tame is the rung on offer and it is RUNNING, which is
		# the only shape that can dip a crew at all.
		"husbandry_ceiling": "pastoral",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": HERD_DIP_STOCK,
		"carrying_capacity": HERD_DIP_CAPACITY,
		"graze_range_radius": 2,
		"provisions_per_biomass": HERD_DIP_PROVISIONS_PER_BIOMASS,
		"trade_per_biomass": HERD_DIP_TRADE_PER_BIOMASS,
		"per_worker_biomass": HERD_DIP_PER_WORKER_BIOMASS,
		"per_worker_yield": HERD_DIP_PER_WORKER_BIOMASS * HERD_DIP_PROVISIONS_PER_BIOMASS,
		"per_worker_trade": HERD_DIP_PER_WORKER_BIOMASS * HERD_DIP_TRADE_PER_BIOMASS,
		# One body, in each account and in biomass — the three statements of the same animal, so the
		# whole-animal quantum the sheet divides by cannot disagree with the curve beside it.
		"food_per_animal": HERD_DIP_BODY_MASS * HERD_DIP_PROVISIONS_PER_BIOMASS,
		"trade_per_animal": HERD_DIP_BODY_MASS * HERD_DIP_TRADE_PER_BIOMASS,
		"body_mass": HERD_DIP_BODY_MASS,
		"tame_build_fraction": HERD_DIP_BUILD_FRACTION,
		# The pastoral rung's payoff (its own MSY: the pastoral r over this K, through the hunt rate),
		# so the improvement control states a real deal rather than a zero.
		"pastoral_yield": 3.6,
		"herders_needed": 0,
		"herders_needed_if_managed": HERD_DIP_WOULD_BE_HERDERS,
		"tile_info": _plain_herd_tile_info(),
	}

## The band gentling it: standing ON the herd (distance 0 ≤ reach → the LOCAL branch, the only one
## that carries an improvement), `output_multiplier` 1.0 so the rendered numbers ARE the model's, and
## idle hands well clear of the composed crew so the frame measures the dip and not the labor ceiling.
func _building_herd_band_fixture() -> Dictionary:
	return {
		"id": "Band 1", "entity": 846, "faction": 0, "size": 90,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": HERD_DIP_IDLE_WORKERS,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	}

## **THE SIM'S `fauna::quantise_animal_take`, RESTATED IN FOOD** — the harness's oracle for what a
## hunting crew is actually paid, so the assertions compare the sheet against the SIM's composition
## rather than against itself (both halves of the sheet dipped together would satisfy any test the
## sheet makes of its own numbers).
##
## Food and biomass differ only by the species' constant provisions rate, which divides out of every
## comparison the sim makes — `collection / body_mass` is `collection_food / food_per_animal` — so this
## is the same arithmetic in cheaper units. `max(1.0, carryable)` is the load-bearing line: a crew that
## cannot carry one whole animal still kills one and wastes the difference.
func _hunt_take_oracle(collection: float, ceiling: float, food_per_animal: float) -> Dictionary:
	var affordable := floorf(ceiling / food_per_animal)
	if affordable < 1.0:
		return {"delivered": 0.0, "wasted": 0.0}
	var killed := minf(affordable, maxf(1.0, floorf(collection / food_per_animal)))
	var killed_food := killed * food_per_animal
	var carried := minf(killed_food, collection)
	return {"delivered": carried, "wasted": killed_food - carried}

## The spec oracle deer: food_per_animal 1.23, Sustain flow ceiling 2.33, per-worker 0.8, output 1.0.
##   1 worker  → can't carry one whole 1.23 deer → delivered 0.80, ≈0.65 deer/turn · ⚠ 35% wasted
##   2 workers → lands exactly one whole deer/turn, no waste → ≈1 deer/turn · renewable
##   4 workers → the Sustain-max cap, delivered 2.33 → ≈1.89 deer/turn, no waste
## Ascending `hunt_policy_ceilings` so the "up to X/turn" cap buttons read Sustain < Surplus < Deplete <
## Eradicate; husbandry ceiling "wild" keeps the picker to the four extractive rungs.
func _delivered_oracle_herd() -> Dictionary:
	return {
		"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "species": "Red Deer",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 820.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": 1.23,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {
			"sustain": 2.33, "surplus": 3.5, "deplete": 5.0, "eradicate": 7.0,
		},
		# THE SECOND PRODUCT (issue #337). A deer is edible AND its hide sells, so it pays BOTH: the
		# picker's four rungs must read food-then-trade (food leading), never food alone. The trade
		# ceilings are the food ones times the species' hide-to-meat ratio, so they ascend together.
		"trade_per_animal": 0.18,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.34, "surplus": 0.51, "deplete": 0.73, "eradicate": 1.02,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## THE INEDIBLE QUARRY (issue #337) — a wolf pack: `provisions == 0` on every rung, a real TRADE yield
## on all four. It is the frame the whole arc is judged on. Before the fix the client read only food, so
## this herd rendered `+0.00` on every picker button and a source "worth nothing"; it must now read four
## ASCENDING trade numbers, NO food line anywhere, and no zeros. Every food-denominated field is
## deliberately 0/absent — `food_per_animal` too — so anything that still divides by a food quantum
## divides by zero and shows up in the frame rather than hiding.
func _pelt_only_wolf_herd() -> Dictionary:
	return {
		"id": "game_wolf_03", "label": "Grey Wolf (game_wolf_03)", "species": "Grey Wolf",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 240.0,
		"husbandry_ceiling": "wild",
		"prey_sense_radius": 4,
		"food_per_animal": 0.0,
		"per_worker_yield": 0.0,
		"hunt_policy_ceilings": {
			"sustain": 0.0, "surplus": 0.0, "deplete": 0.0, "eradicate": 0.0,
		},
		"trade_per_animal": 1.40,
		"per_worker_trade": 0.45,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.90, "surplus": 1.35, "deplete": 1.95, "eradicate": 2.70,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## The wolf's RAID table: `delivers_food = false` (an INEDIBLE quarry, NOT a denial policy) beside
## `delivers_trade = true` on every rung, so the expedition line must read a real delivery in trade
## goods rather than the "denial mission" the old `delivers_food`-only branch would have called it.
func _pelt_only_wolf_raid_herd() -> Dictionary:
	var herd := _pelt_only_wolf_herd()
	var table := {}
	var animals_row := [3, 5, 6, 6, 6, 6, 6, 6]
	for i in animals_row.size():
		for entry in [["sustain", 0, 9], ["surplus", 1, 7], ["deplete", 2, 6], ["eradicate", 4, 5]]:
			var animals: int = int(animals_row[i]) + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": int(entry[2]),
				"delivers_food": false, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": 0.0, "wasted_food": 0.0,
				"delivered_trade": float(animals) * 1.40,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## A big-game herd for the averaging-WINDOW hint: food_per_animal 16, Sustain flow ceiling 2.4 → one whole
## mammoth lands only every ceil(16/2.4)=7 turns, so the delivered ≈0.15/turn rate carries the "≈1 … every
## ~7 turns" span line. The whole-animal cap needs 20 carriers to haul one 16-food body, and auto-max
## staffs them (band idle 26).
func _big_game_window_herd() -> Dictionary:
	return {
		"id": "game_mammoth_01", "label": "Woolly Mammoth (game_mammoth_01)",
		"species": "Woolly Mammoth",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 3200.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": 16.0,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {
			"sustain": 2.4, "surplus": 3.6, "deplete": 5.0, "eradicate": 7.0,
		},
		# Ivory sells (issue #337) — a live herd carries the trade half of its vector too.
		"trade_per_animal": 2.4,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.36, "surplus": 0.54, "deplete": 0.75, "eradicate": 1.05,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## THE FLASH GUARD's tile (docs/plan_hud_decomposition.md §2a): an active, foraged prairie hex with
## the full chip set (sight · habitability · climate · tags · site) and a standing forage patch, so a
## restate with a different `habitability` (Hospitable → Harsh) and `patch_biomass` proves the chips +
## land row + drawer patch in place instead of tearing down. Same coords across restates — the same
## HEX, only its numbers move.
func _no_flash_tile_fixture(habitability: float, biomass: float) -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"habitability": habitability,
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"site_name": "Verdant Basin",
		"patch_ecology_phase": "thriving",
		"patch_biomass": biomass,
		"patch_carrying_capacity": 120.0,
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
	}

## THE FLASH GUARD's band: a player band foraging the no-flash hex with `workers` on it at `yield_val`
## food/turn, so the drawer renders a standing summary (`♻ N foragers · +X /turn`) and the land row a
## `N 🌾` staffing meta — both of which must UPDATE in place (not rebuild) when the numbers change.
## Sustain + `overdraws:false` and no `workers_needed` keep the summary's SHAPE stable across restates
## (no warn/overstaff labels appear/disappear), so only values move.
func _no_flash_band_fixture(workers: int, yield_val: float) -> Dictionary:
	return {
		"id": "Band Steady",
		"size": 30,
		"entity": 909,
		"faction": 0,
		"pos": [66, 10],
		"current_x": 66, "current_y": 10,
		"activity": "forage",
		"working_age": 16,
		"idle_workers": maxi(0, 16 - workers),
		"work_range": 3,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": workers, "target_x": 66, "target_y": 10,
				"floor": 0.5, "actual_yield": yield_val, "sustainable_yield": yield_val, "overdraws": false},
		],
	}

## **Seed the per-policy forage ROWS from the flat scalars this fixture already states** (#426).
##
## The wire now carries the tile's whole yield vector as one row per rung — six dicts keyed by policy,
## both the ceiling and the per-worker term, in all three accounts — and the six flat `patch_ceiling_*`
## scalars are deprecated slots. `SourceForecast.forecast_is_known` reads the ROW's PRESENCE as its
## "does the wire describe this source" witness, so a fixture that seeds only the scalars now correctly
## reads as *undescribed* and renders no forecast at all.
##
## Deriving the rows here rather than hand-writing them at ~30 fixture sites keeps each fixture naming
## its numbers ONCE, in the readable scalar form its comments explain, and makes the two
## representations unable to disagree. A state that wants a genuinely NON-derivable row (a trade-paying
## or fodder-paying tile) overwrites the relevant dict after calling this.
##
## Trade and fodder default to 0 — the render-only-when-non-zero rule means every existing frame is
## then byte-identical, which is exactly what a reseeding pass must not disturb.
##
## **THE `patch_ceiling_*` KEYS IT READS ARE A FIXTURE-AUTHORING SHORTHAND, NOT WIRE KEYS, AND THIS
## ERASES THEM** (#426). The six flat scalars they are named after are retired `(deprecated)` wire
## slots that `MapView` no longer cross-refs and `SourceForecast` no longer reads — so a tile dict
## left carrying them would be a wire-shaped key with no wire behind it, and the next fixture author
## to reach for one would get silence rather than an error. Consuming them here keeps ~30 fixtures
## naming their numbers once, in the readable form their comments explain, while guaranteeing no
## fixture can hand the HUD a representation the sim stopped sending.
func _seed_forage_rows(tile: Dictionary) -> Dictionary:
	var per_worker := float(tile.get("patch_per_worker_yield", 0.0))
	# **A RE-SEED FALLS BACK TO WHAT IS ALREADY THERE**, and that is what makes the layered fixtures
	# work: most of them are `_food_tile_fixture()` (already seeded) plus a few overrides plus a second
	# `_seed_forage_rows`. Reading only the scalars would silently zero every account the second caller
	# did NOT restate.
	var peak_food := float(tile.get("patch_ceiling_sustain", 0.0))
	var peak_trade := 0.0
	var peak_fodder := 0.0
	if tile.has("patch_provisions_per_biomass"):
		var prior_room := float(tile.get("patch_biomass", 0.0)) \
			- SourceForecast.FLOOR_FOOD_PEAK * float(tile.get("patch_carrying_capacity", 0.0))
		if peak_food <= 0.0:
			peak_food = float(tile["patch_provisions_per_biomass"]) * prior_room
		peak_trade = float(tile.get("patch_trade_per_biomass", 0.0)) * prior_room
		peak_fodder = float(tile.get("patch_fodder_per_biomass", 0.0)) * prior_room
	# **THE STOCK THE CEILING IS COMPOSED FROM.** A fixture states a ceiling; the wire states the terms
	# a ceiling is built out of, so this reverses the arithmetic the client now does — pinning each
	# fixture's authored `sustain` number to the FOOD PEAK, which is the honest mapping (Sustain took
	# the renewable yield; the peak is the floor that pays the most forever). At the seeded stock the
	# other two presets fall out at 2.25x and 0.25x of it.
	var capacity := FIXTURE_CAPACITY
	var biomass := FIXTURE_STOCK_FRACTION * FIXTURE_CAPACITY
	var room := biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
	tile["patch_carrying_capacity"] = capacity
	tile["patch_biomass"] = biomass
	# **A BARREN PATCH KEEPS ITS RATES AND LOSES ITS STOCK** — the dead-season case, and the whole of
	# what issue #426 turns on. Its per-biomass vector is a property of what GROWS there and stays
	# positive; what a dead season zeroes is the crew's throughput and the standing crop. Zeroing the
	# RATE instead would make the patch read as one the wire never described, which is the opposite of
	# the state.
	if peak_food <= 0.0 and peak_trade <= 0.0 and peak_fodder <= 0.0:
		tile["patch_biomass"] = SourceForecast.FLOOR_FOOD_PEAK * capacity
		tile["patch_provisions_per_biomass"] = BARREN_PATCH_PER_BIOMASS
		tile["patch_trade_per_biomass"] = 0.0
		tile["patch_fodder_per_biomass"] = 0.0
	else:
		tile["patch_provisions_per_biomass"] = peak_food / room
		tile["patch_trade_per_biomass"] = peak_trade / room
		tile["patch_fodder_per_biomass"] = peak_fodder / room
	# **THE TWO BUILD DIPS ARE FRACTIONS** (issue #442). `patch_ceiling_cultivate` /
	# `patch_ceiling_sow` remain the fixture-authoring shorthand — a fixture states the dip as the
	# absolute rate its comments explain — and this converts each to the wire's fraction form by
	# dividing by the food-peak ceiling, which is exactly what the old row was. A fixture that states a
	# fraction outright wins; a barren patch leaves it 0, i.e. "no build described here".
	for rung in SourceForecast.FORAGE_IMPROVEMENTS:
		var key := "patch_%s_build_fraction" % rung
		if not tile.has(key):
			var dip := float(tile.get("patch_ceiling_%s" % rung, 0.0))
			tile[key] = (dip / peak_food) if peak_food > 0.0 else 0.0
		tile.erase("patch_ceiling_%s" % rung)
	for policy in LEGACY_STANCE_FLOORS:
		tile.erase("patch_ceiling_%s" % policy)
	return tile

## The staple tile as the COMPOSE SHEET sees it — `_food_tile_fixture` already runs through
## `_seed_forage_rows`, so this is simply the named handle the dip-comparison assertion reads its
## forecast from. Naming it keeps that assertion from re-stating which fixture it is judging.
func _seeded_food_tile() -> Dictionary:
	return _food_tile_fixture()

func _food_tile_fixture() -> Dictionary:
	var tile := {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		# Fertile steppe: low drain → "Hospitable" (green Tile-card row).
		"habitability": 0.01,
		# Mid-latitude ~18° → "Temperate" climate band (neutral Tile-card row).
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		# A discovered Wondrous Site on this tile → the Tile card shows a "Site: …" line.
		"site_name": "Verdant Basin",
		# Forage patch being worked toward cultivation → the Tile card's "Cultivation 60%" row.
		"patch_cultivation_progress": 0.6,
		"patch_is_cultivated": false,
		"patch_has_owner": true,
		"patch_owner": 0,
		"patch_ecology_phase": "thriving",
		# Standing forage stock vs the patch ceiling (sim default capacity 120) → the Tile card's
		# "Forage biomass 84 / 120" row, the patch counterpart to a herd's Biomass row.
		"patch_biomass": 84.0,
		"patch_carrying_capacity": 120.0,
		# Pre-commit yield forecast (food/turn at THIS biomass, exported at output_multiplier 1.0).
		# Sustain's ceiling admits ceil(0.96 / 0.32) = 3 useful foragers — below band 821's 10 idle
		# workers, so the Foragers stepper caps at 3 and shows the "max 3 workers useful here" note.
		# The higher-policy ceilings admit 6 / 9 / 15, so switching policy visibly moves the cap.
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
		# The Cultivate INVESTMENT rung: while the patch is being prepared it pays only a fraction of
		# its Sustain ceiling (the dip the player is buying with), then flips to the tended yield.
		# Both are food/turn at output_multiplier 1.0, like the ceilings above.
		"patch_ceiling_cultivate": 0.24,
		"patch_tended_yield": 1.20,
		# THE BUILD CREWS (#442) — `intensification_ladder.json`'s own `crew_needed` for the two plant
		# rungs (tended 2, field 3), which is what the compose stepper FLOORS its cap on. Not decoration:
		# the dip shrinks the ceiling the cap divides, so without a crew a Cultivate composed here caps
		# at ONE forager while the sim asks for two — the exact disagreement the pair of them fixes.
		"patch_cultivate_crew_needed": 2,
		"patch_sow_crew_needed": 3,
		# THE NEGLECT GRACE (#442) — the countdown to this rung reverting. The reference patch IS being
		# worked (a crew is cultivating it), so it reads the plant:tended rung's full `grace + 1` = 3:
		# "walk away and you have this long". `has_neglect_grace` is what makes the number readable at
		# all — a wild patch would ship `false`, not a zero.
		"patch_has_neglect_grace": true,
		"patch_neglect_grace_remaining": 3,
		# Plant RUNG 3 — the Field + the Sow verb. This reference tile is ordinary prairie steppe:
		# rich enough to forage, but it will NOT take seed (rung 3 moves seed, it cannot fertilize or
		# irrigate), so the sim's `sow_site_refusal` verdict rides here and the Sow option is gated
		# with the reason. Only ~1% of a real map is sowable, so REFUSED is the common case and is
		# deliberately the default fixture; `_sowable_tile_fixture` is the exception.
		"patch_field_progress": 0.0,
		"patch_is_field": false,
		"patch_ceiling_sow": 0.0,
		"patch_field_yield": 0.0,
		"patch_sow_site_refusal": "too_dry",
		# WHAT GROWS HERE (flora roster F1) — the named plants this tile's forage capacity decomposes
		# into. Wire order (share DESC, then species key ASC) is preserved verbatim by the card.
		# The shares are chosen so NAIVE rounding totals 101% (46 + 30 + 25): the card must absorb the
		# remainder into the largest share and render 45 / 30 / 25 — this fixture IS the rounding test.
		# `can_cultivate` / `can_sow` are SPECIES-GLOBAL rung legality (flora roster S1), deliberately
		# mixed here so the crop picker has a greyed row in every frame: Oak Mast climbs nothing (a wild
		# harvest forever) and Ground Nut tends but never sows. `*_yield_ratio` is what committing PAYS
		# relative to gathering wild, on the CORRECTED scale (the sim's ratio omitted
		# `tended_regrowth_gain`, understating every Cultivate figure by exactly 2×) — so above 1.0 is
		# now the NORM and these read: a strong crop (2.40×), an honest middle one (1.70×) and the 0
		# sentinel on the greyed rows. `*_payoff` is the same rung's provisions/turn committed to THAT
		# species, and it is what the compose sheet's "→ then" term quotes once a crop is picked: the two
		# rows differ (1.20 vs 0.85), which is what makes the selection visibly move the forecast.
		"patch_composition": [
			{"species": "wild_grain", "role": "staple", "display_name": "Wild Grain", "share": 0.455,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.295,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.85, "sow_payoff": 0.0},
			{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.25,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
		# The GRAZE (pasture) layer — the ANIMAL-edible twin of the forage patch above (Grazing Phase
		# 2a). Prairie steppe is the reference pasture: capacity 240, standing full, hence Thriving.
		# Rendered as the `Pasture` / `Pasture ecology` rows right under `Forage biomass`, so the card
		# states the two facts side by side: what HUMANS can eat here, and what ANIMALS can eat here.
		"graze_biomass": 240.0,
		"graze_capacity": 240.0,
		"graze_ecology_phase": "thriving",
	}
	return _seed_forage_rows(tile)

## An OVERGRAZED pasture: the standing graze has been drawn deep into the stressed band, so the
## `Pasture ecology` row reads a WARN-amber "⚠ Stressed" — the SAME label + tint a stressed herd or a
## stressed forage patch gets (one ecology vocabulary, one styling path). Nothing eats graze until
## Phase 2b, so this state cannot occur in a live 2a map; it renders the path the tint will take.
## A tile whose Climate row is under test: same card as `_food_tile_fixture`, only the
## `temperature` (and a label) vary, so the ONLY thing moving between the four climate_* frames
## is the band the sim's cut points classify that temperature into.
func _climate_tile_fixture(temperature: float, terrain_label: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["temperature"] = temperature
	tile["terrain_label"] = terrain_label
	return tile


## STAGE 2 of the commitment — a band has COMMITTED this patch to Wild Grain and the build is STILL
## RUNNING (`_food_tile_fixture` carries `cultivation_progress` 0.6, `is_cultivated` false). The
## commitment is recorded on the FIRST worked turn, so the basket underneath it is the wild one,
## UNCHANGED — 45 / 30 / 25, byte-for-byte what `food_tile` shows. The card must therefore render the
## `Crop: Wild Grain` row AND the whole basket, with Wild Grain marked in SIGNAL; collapsing to the
## crop row alone claimed a mixed tile was already pure the instant the order was given (issue #433).
## The committed species is a MEMBER of the basket on purpose — the mark has nothing to land on
## otherwise, and the sim can only ever commit to a plant the tile actually realizes.
func _committed_crop_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_committed_species"] = "wild_grain"
	tile["patch_committed_display_name"] = "Wild Grain"
	return tile

## STAGE 3 — the same commitment once the Tended Patch COMPLETES, which is when the basket finally
## moves. Weeding lifts the favored share to `min(1, share x tended_weeding_gain)` (0.455 x 1.5 =
## 0.6825) and takes the increase off the LEAST abundant members first, so Oak Mast (0.25) absorbs all
## 0.2275 of it and Ground Nut is untouched: 68 / 30 / 2. Read against `food_tile_crop` — same tile,
## same crop, one build later — this pair is the whole point of showing the basket beside the Crop
## row: you can watch Oak Mast fall 25% -> 2% as the work lands.
func _weeded_crop_tile_fixture() -> Dictionary:
	var tile := _committed_crop_tile_fixture()
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	var basket: Array = []
	for entry_variant in tile["patch_composition"]:
		var entry: Dictionary = (entry_variant as Dictionary).duplicate(true)
		match String(entry["species"]):
			"wild_grain": entry["share"] = 0.6825
			"oak_mast": entry["share"] = 0.0225
		basket.append(entry)
	tile["patch_composition"] = basket
	# A tended patch reports every policy ceiling == per_worker_yield (see `_tended_tile_fixture`), so
	# the stepper caps at 1 worker and the frame does not also change the forecast under test.
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return _seed_forage_rows(tile)

## THE SIZING CASE FOR A COMMITTED PATCH — `realized_species_max` is 4, so a 4-plant basket is the
## WORST CASE both surfaces must fit, not an outlier, and the 3-plant reference tile is one row short
## of reaching either cap. Taken from the playtest hex that broke them: Wild Emmer 47 / Flax Fields 21
## / Hay Grass 21 / Wild Grapevine 11, committed to the emmer with the build barely started, worked by
## a band (so the sheet also carries its `Now 1` line — the row that was clipped off the top).
## The mixed accounts are deliberate: a provisions crop, two cash crops and a fodder crop put all
## three row formats in one frame at the length where the height is tightest.
func _four_species_committed_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_committed_species"] = "wild_emmer"
	tile["patch_committed_display_name"] = "Wild Emmer"
	tile["patch_cultivation_progress"] = 0.04
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.47,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.39, "sow_payoff": 2.40},
		{"species": "flax_fields", "role": "cash", "display_name": "Flax Fields", "share": 0.21,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0, "sow_trade_payoff": 11.7},
		{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.21,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0, "sow_fodder_payoff": 15.6},
		{"species": "wild_grapevine", "role": "cash", "display_name": "Wild Grapevine", "share": 0.11,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0, "sow_trade_payoff": 12.5},
	]
	return tile

## The LONGEST basket the sim can produce — a navigable hex blends the valley's basket with the
## channel's fishery, so five named plants can share one tile (RollingHills carries four). The crop
## picker must fit and stay legible at that length, which is why the sizing case gets its own fixture
## rather than being judged on the 3-entry reference tile.
func _long_basket_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.34,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10,
			"cultivate_trade_payoff": 0.11, "sow_trade_payoff": 0.16},
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.24,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.34, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.67, "sow_payoff": 0.0,
			"cultivate_trade_payoff": 0.06, "sow_trade_payoff": 0.0},
		{"species": "river_fish", "role": "staple", "display_name": "River Fish", "share": 0.18,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0,
			"cultivate_trade_payoff": 0.04, "sow_trade_payoff": 0.0},
		{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.10,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
	]
	return tile

## THE ALL-MARGINAL TILE — RollingHills' real numbers: every crop it can grow yields LESS than simply
## gathering the tile wild (Hazel 0.67, Wild Emmer 0.60, Wild Tubers 0.49, Berry Scrub 0.35). Nothing
## is illegal and nothing is disabled — the whole list is warn-inked and every row is still pressable,
## because "this land is not worth farming" is a verdict the player must be able to read AND overrule.
## SYNTHETIC — NOT A REAL TILE. Eight named plants, longer than any basket the sim can produce today
## (the longest real one is the 5-plant navigable-hex blend). Its ONLY job is to keep the crop picker's
## internal scroll RENDERED: the visible-row cap is set so every SHIPPED basket fits without scrolling,
## which would otherwise leave that path unexercised by any frame until F5 lengthens the roster and
## someone discovers it rotted. Do not treat these species or shares as a balance reference.
func _overlong_basket_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.22,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10},
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.17,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 2.20, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 1.10, "sow_payoff": 0.0},
		{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.85, "sow_payoff": 0.0},
		{"species": "river_fish", "role": "staple", "display_name": "River Fish", "share": 0.13,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.11,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.44, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.72, "sow_payoff": 0.0},
		{"species": "berry_scrub", "role": "staple", "display_name": "Berry Scrub", "share": 0.09,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0},
		{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.08,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "marsh_reed", "role": "fodder", "display_name": "Marsh Reed", "share": 0.06,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.35, "sow_payoff": 0.0},
	]
	return tile


func _marginal_basket_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.34,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.94, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.47, "sow_payoff": 0.0},
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.28,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.84, "sow_yield_ratio": 1.26,
			"cultivate_payoff": 0.42, "sow_payoff": 0.63},
		{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.22,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.68, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.34, "sow_payoff": 0.0},
		{"species": "berry_scrub", "role": "staple", "display_name": "Berry Scrub", "share": 0.16,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.49, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.25, "sow_payoff": 0.0},
	]
	return tile

## The same long basket on ground that will actually take seed — `Sow` is gated on the site, so the
## crop picker's rung-3 frame needs the sowable tile, not the reference one (a refused Sow falls back
## to Sustain and the picker would not render at all).
func _sowable_long_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = _long_basket_tile_fixture()["patch_composition"]
	return tile

## A basket with a FODDER crop (Flora roster F3): Hay Grass is fodder-dominant, so a `N.N×` row alone
## would call it worthless. Under Sow the picker reads `Hay Grass 30% · 1.80 hay` beside the staple's
## `Wild Emmer 70% · 3.2× · 0.16 trade` — each row stating every account it pays. On sowable ground so
## both rows are legal and pressable: a fodder crop is a legal, valuable choice.
##
## **Hay `can_cultivate` too (issue #419)** — its `cultivation_ceiling` is `field`, so the Cultivate rung
## reaches it, and its rung-2 hay is its own number (0.72), not the Field's 1.8. This fixture greyed it
## and shipped only the Field figure, so a Cultivate row here quoted a sown field's hay.
func _fodder_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_trade_payoff": 0.11, "sow_trade_payoff": 0.16},
		{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.30,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.25, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.12, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.72, "sow_fodder_payoff": 1.8,
			"cultivate_trade_payoff": 0.0, "sow_trade_payoff": 0.0},
	]
	return tile

## A basket with a CASH crop (Flora roster F4): Flax is trade-dominant, so its provisions payoff is a
## fraction of the staple's and the `N.N×` row alone would call it worthless. Both rows state every
## account they pay — `Wild Emmer 70% · 3.2× · 0.16 trade` beside `Flax 30% · 2.40 trade` under Sow.
##
## **BOTH RUNGS ARE POPULATED, and flax `can_cultivate` (issue #419).** This fixture had
## `can_cultivate: false` on the cash crop and no `cultivate_*_payoff` at all, which is a fiction: every
## cash crop's `cultivation_ceiling` is `field`, so `allows_cultivate()` passes and the row is fully
## pressable on the Cultivate rung. Greying it here meant the Cultivate rung of a cash basket had **no
## frame in the harness**, which is how the picker came to print a *sown Field's* trade on the Cultivate
## row unseen. The rung-2 numbers are the shape the sim actually ships (measured: cotton at rung 2 pays
## ~1/3 of its Field trade, and still pays the volunteers' calories at a rate BELOW gathering wild —
## #433 weeds rather than replaces, so the food ratio is a real, warn-inked loss and not a 0).
func _cash_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_trade_payoff": 0.11, "sow_trade_payoff": 0.16},
		{"species": "flax", "role": "cash", "display_name": "Flax", "share": 0.30,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.30, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.15, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_trade_payoff": 0.95, "sow_trade_payoff": 2.4},
	]
	return tile

## PER-TILE FLORA REALIZATION (Flora roster F4) — the SECOND Alluvial Plain tile. Same biome as
## `_cash_basket_tile_fixture` (both "Alluvial Plain"), but a DIFFERENT realized basket: two tiles of
## one biome no longer carry the uniform per-biome roster, they carry a seeded per-tile SUBSET. This
## one is cash-DOMINANT — Cotton 55% + Flax 45%, both cash crops paying trade — where its twin was
## grain-dominant (Wild Emmer 70% + Flax 30%). Rendered beside it, the pair is the visible proof that
## same-biome tiles realize different species/shares. A different coord so it reads as its own tile.
func _cash_variant_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["x"] = 68
	tile["y"] = 12
	tile["patch_composition"] = [
		{"species": "cotton", "role": "cash", "display_name": "Cotton", "share": 0.55,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.28, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.14, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_trade_payoff": 1.42, "sow_trade_payoff": 3.6},
		{"species": "flax", "role": "cash", "display_name": "Flax", "share": 0.45,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.30, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.15, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_trade_payoff": 0.95, "sow_trade_payoff": 2.4},
	]
	return tile


## **THE TILE THE FOOD-LAYER ROWS ARE JUDGED ON — all three crop ROLES on one patch.** A river-delta
## stand carrying a staple, a cash crop and a fodder crop, so the card's basket shows one of every
## role icon and states outright that most of what grows on this ground is not food: 38% staple
## against 62% cash + fodder. Every other basket fixture is staple-dominant, so until this one existed
## the role icons had no frame that could tell them apart.
##
## **IT STATES ITS OWN STOCK AND CAPACITY, so it deliberately does NOT go through `_seed_forage_rows`**
## (the `_stale_verb_tile_fixture` precedent), which pins every fixture it touches to one
## `FIXTURE_CAPACITY`. The capacity is what each basket row's absolute biomass is a share OF, so it has
## to be a number the three rows can be checked against by eye — and 205 is chosen so the naive
## rounding of `38 / 31 / 31` percent MISSES it by one (78 + 64 + 64 = 206), making this frame the
## biomass-remainder test exactly as `_food_tile_fixture`'s 46/30/25 is the percentage one.
##
## Standing at full capacity, so `Foraging 205 / 205` and the three rows sum to both numbers at once —
## the clearest possible reading of "these decompose the row above".
const THREE_ROLE_CAPACITY := 205.0
## **DELIBERATELY BELOW THE CEILING.** The basket decomposes what is STANDING, and a full patch
## cannot tell that apart from one decomposing the capacity — the two coincide there, so the
## assertion below would pass either way and prove nothing. 150 of 205 makes the claim testable.
const THREE_ROLE_STOCK := 150.0

const THREE_ROLE_GRAZE_CAPACITY := 130.0

func _three_role_tile_fixture() -> Dictionary:
	return {
		"x": 64, "y": 8,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.02,
		"temperature": 19.0,
		"height_display": "5 ▬▭▭▭▭▭▭▭▭▭",
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"food_kind": "river_garden",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": THREE_ROLE_STOCK,
		"patch_carrying_capacity": THREE_ROLE_CAPACITY,
		"patch_provisions_per_biomass": 0.012,
		"patch_trade_per_biomass": 0.021,
		"patch_fodder_per_biomass": 0.017,
		"patch_per_worker_biomass": 26.0,
		"patch_per_worker_yield": 0.31,
		"patch_is_cultivated": false,
		"patch_cultivation_progress": 0.0,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.38,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "cotton", "role": "cash", "display_name": "Cotton Fields", "share": 0.31,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 0.28, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.14, "sow_payoff": 0.0,
				"cultivate_trade_payoff": 1.42, "sow_trade_payoff": 3.6},
			{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.31,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 0.25, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.12, "sow_payoff": 0.0,
				"cultivate_fodder_payoff": 0.72, "sow_fodder_payoff": 1.8},
		],
		"graze_biomass": THREE_ROLE_GRAZE_CAPACITY,
		"graze_capacity": THREE_ROLE_GRAZE_CAPACITY,
		"graze_ecology_phase": "thriving",
	}

## **THE SAME TILE WITH ONE PLANT'S ROLE UNSTATED** — the `""` case, which the wire says means "this
## server's roster no longer knows this species", NOT "staple". The row must render its share and its
## biomass with NO icon at all rather than defaulting into a real category, and the two tagged rows
## beside it are what make that visible. The key is OMITTED rather than set to `""` so the fixture also
## covers the shape the decoder produces when the wire carries no role (it only inserts the key when
## the string is there).
func _unstated_role_tile_fixture() -> Dictionary:
	var tile := _three_role_tile_fixture()
	tile["x"] = 65
	var basket: Array = []
	for entry_variant in tile["patch_composition"]:
		var entry: Dictionary = (entry_variant as Dictionary).duplicate(true)
		if String(entry["species"]) == "cotton":
			entry.erase("role")
		basket.append(entry)
	tile["patch_composition"] = basket
	return tile

func _overgrazed_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 68
	tile["graze_biomass"] = 61.0
	tile["graze_ecology_phase"] = "stressed"
	return tile

## Ground that carries NO pasture at all (a glacier — the biome's graze capacity is a stated 0, so the
## sim holds no patch there and the tile carries no graze fields). The card must print NOTHING about
## pasture here — never "0 / 0", which would read as a starved pasture rather than an absent one.
func _no_pasture_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 3,
		"terrain_label": "Glacier",
		"tags_text": "Polar",
		"visibility_state": "active",
		"habitability": 0.09,
		"temperature": -14.0,
	}

## A plain (no forage patch) tile carrying hex-EDGE rivers on some of its sides. Deliberately
## bare of food-module keys so the Tile card is just the terrain-intrinsic rows and the river
## row(s) read unobstructed.
func _river_tile_fixture(river_mask: int) -> Dictionary:
	return {
		"x": 9, "y": 36,
		"terrain_label": "Sinkhole Field",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.03,
		"temperature": 15.0,
		"river_edges": river_mask,
	}

## A herd carrying the two DIFFERENT things the sim exports for the two DIFFERENT actors:
##   `hunt_policy_ceilings` — the BAND's renewable FLOW ceiling {policy → provisions/turn}. The local
##       hunt preview is pure arithmetic over it (Sustain's entry IS the herd's sustainable yield).
##   `hunt_trip_estimates` — the sim's forward-SIMULATED expedition trip answers, keyed
##       `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, delivers_trade, …}`. An
##       expedition's trip is NOT a rate division (on Surplus/Deplete the ceiling is a *stock* the party
##       strips in a turn or two, then it crawls at the regrowth trickle), so the client looks the answer
##       up and does no math. `turns_to_fill == 0` → won't fill within the horizon; `delivers_food ==
##       false` says the QUARRY IS INEDIBLE (#337), and only `delivers_food AND delivers_trade` both
##       false is a denial mission — the raid banks whichever half the species pays.
## `trip_turns` is the simulated turns-to-fill for the 4-worker party these states dial in.
func _forecast_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int = 0, surplus_trip_turns: int = 0,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × food_per_animal
	# and nothing rots. `delivered_food` is now the PRIMARY payload the client headlines (and the field
	# the "too lean" test / max-useful scan read), so every fixture cell must carry it; a partial-with-
	# waste cell is built explicitly (see `_partial_waste_mammoth`).
	var fpa := 2.0
	var sustain_delivered := float(sustain_animals) * fpa
	var surplus_delivered := float(surplus_animals) * fpa
	return {
		"id": id,
		"label": "%s (%s)" % [species, id],
		"species": species,
		"size_class": "big",
		"huntable": true,
		"ecology_phase": phase,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# One animal's worth of FOOD (provisions), `HerdTelemetryState.foodPerAnimal` — drives the
		# kill-rhythm on the local-hunt preview (food ÷ food). Matches `fpa` above (the clean delivered).
		"food_per_animal": fpa,
		# A LIVE herd carries BOTH forecast field sets, so this fixture must too (they were split
		# across two disjoint fixtures once, which hid every interaction between them):
		#   • `per_worker_yield` + the `hunt_policy_ceilings` table, which drive the shared
		#     `SourceForecast.forecast_inputs` → cap + "Expected yield" / "Preparing → then" row, and
		#   • `hunt_trip_estimates` below (the sim's forward-simulated EXPEDITION trip answers).
		# Per-worker matches the band's `hunt_per_worker_provisions` (0.8) and the ceilings ARE the
		# band ceilings, because the sim exports one hunt model — the two paths must agree.
		"per_worker_yield": 0.8,
		# Eradicate's ceiling was `0.0` — the retired "denial yields nothing" premise written as a number,
		# which rendered the rung's picker face as `💀 +0.00` and its local preview as a zero take. #337
		# pays every rung the species' vector, and Eradicate empties the standing stock, so it is the
		# DEEPEST floor and frees the most: 8× the Sustain flow here.
		"hunt_policy_ceilings": {
			"sustain": sustain_ceiling,
			"surplus": sustain_ceiling * 4.0,
			"deplete": sustain_ceiling * 2.0,
			"eradicate": sustain_ceiling * 8.0,
		},
		# The trade half of each rung's ceiling (issue #337), a fixed fraction of the food one.
		"trade_per_animal": fpa * 0.15,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": sustain_ceiling * 0.15,
			"surplus": sustain_ceiling * 4.0 * 0.15,
			"deplete": sustain_ceiling * 2.0 * 0.15,
			"eradicate": sustain_ceiling * 8.0 * 0.15,
		},
		"hunt_trip_estimates": {
			"sustain:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": sustain_animals,
				"delivered_food": sustain_delivered,
				"delivered_trade": float(sustain_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			"surplus:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			"deplete:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			# Eradicate DELIVERS (issue #337): `delivers_food` says the quarry is EDIBLE, not that the
			# rung is a denial mission, and an Eradicate raid banks the whole-stock windfall.
			"eradicate:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
		},
	}

## An over-drawn, UNCULTIVATED forage patch: the Tile card's "Ecology" row must still render
## (the phase gates cultivation, so it always shows on a patch) as a WARN-amber "⚠ Stressed".
## Biomass is well below capacity, mirroring a patch foraged past its regrowth.
func _stressed_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_cultivation_progress"] = 0.0
	tile["patch_is_cultivated"] = false
	tile["patch_ecology_phase"] = "stressed"
	tile["patch_biomass"] = 22.0
	return tile

## A fully-tended forage patch: the Tile card shows the "🌾 Tended Patch" badge (SIGNAL tint)
## plus an "Ecology" row, instead of the in-progress "Cultivation N%".
func _tended_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 67
	tile["y"] = 11
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	tile["patch_ecology_phase"] = "thriving"
	# A TENDED patch reports every policy ceiling == per_worker_yield, so max-useful collapses to 1
	# worker regardless of policy — the stepper caps at 1 ("max 1 workers useful here").
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return _seed_forage_rows(tile)

## **THE PLAYED TILE — a FINISHED Tended Patch whose crew a stale `Cultivate` was still dipping.**
##
## Every term is the SHIPPED one, because the whole point is that this is the arithmetic a LIVE patch
## produces and the preview fixtures could not: `per_worker_biomass_capacity` 8.0 × the tile's seasonal
## weight, which worldgen fixes at `INITIAL_SEASONAL_WEIGHT` 1.0 and nothing ever moves; the plant
## rungs' `yield_fraction_while_building` 0.25; and a basket of Wild Tubers 35% · Cotton 30% · Flax 20%
## · Wild Rice 15%, of which only the two staples pay food — 0.35 × 0.065 + 0.15 × 0.070 — so the patch
## converts at `STALE_VERB_FOOD_PER_BIOMASS` and the two cash crops carry the trade rate beside it.
##
## **It states its own stock and capacity, so it deliberately does NOT go through `_seed_forage_rows`**,
## which pins every fixture it touches to one `FIXTURE_CAPACITY`/`FIXTURE_STOCK_FRACTION` pair. This
## frame is about a particular `B / K` — a patch standing just above the floor it is worked at, where
## the crew is bound by the REGROWTH rather than by the room — and the per-biomass vector states the
## ceiling directly anyway. `_floorify` still seeds the growth curve and the phase cuts from it.
func _stale_verb_tile_fixture() -> Dictionary:
	return {
		"x": 68, "y": 12,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.72,
		"temperature": 18.0,
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": STALE_VERB_STOCK,
		"patch_carrying_capacity": STALE_VERB_CAPACITY,
		# THE RUNG THE VERB NAMES IS BUILT. `is_cultivated` is what the improvement control reads to
		# render its DONE label instead of a running meter — and, since this fix, what tells the crew
		# terms that the Cultivate still sitting in the compose state is a stale verb.
		"patch_is_cultivated": true,
		"patch_cultivation_progress": 1.0,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		"patch_provisions_per_biomass": STALE_VERB_FOOD_PER_BIOMASS,
		"patch_trade_per_biomass": STALE_VERB_TRADE_PER_BIOMASS,
		"patch_fodder_per_biomass": 0.0,
		"patch_per_worker_biomass": STALE_VERB_PER_WORKER_BIOMASS,
		"patch_per_worker_yield": STALE_VERB_PER_WORKER_BIOMASS * STALE_VERB_FOOD_PER_BIOMASS,
		# The two plant dips, as the wire carries them: `BuildDips::for_branch` publishes BOTH rungs'
		# fractions whatever the patch has already climbed, which is exactly why the fraction alone
		# cannot say "nothing left to build here" and the done flag above has to.
		"patch_cultivate_build_fraction": STALE_VERB_BUILD_FRACTION,
		"patch_sow_build_fraction": STALE_VERB_BUILD_FRACTION,
		"patch_cultivate_crew_needed": 2,
		"patch_sow_crew_needed": 3,
		# The ground is rich but away from fresh water, so the next rung is offered and REFUSED — the
		# sheet's improvement row is a done label over a site gate, with no running build anywhere on it.
		"patch_sow_site_refusal": "too_dry",
		"patch_tended_yield": 1.20,
		"patch_field_yield": 2.40,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.35,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "cotton", "role": "cash", "display_name": "Cotton Fields", "share": 0.30,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 1.40, "sow_yield_ratio": 2.60,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
			{"species": "flax", "role": "cash", "display_name": "Flax Fields", "share": 0.20,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.30, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
			{"species": "wild_rice", "role": "staple", "display_name": "Wild Rice", "share": 0.15,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
	}

## The band working that patch — 2 foragers, NO improvement (the sim cleared the assignment's verb the
## turn the Cultivate completed), no idle hands, and its rate filled in by the caller from the tile's
## own wire terms so the drawer's standing summary and the sheet's crew targets cannot state two
## different throughputs by fixture drift.
func _stale_verb_band_fixture(rate: float) -> Dictionary:
	return {
		"id": "Band 1",
		"size": 30,
		"entity": 821,
		"faction": 0,
		"pos": [67, 11],
		"current_x": 67, "current_y": 11,
		"activity": "forage",
		"working_age": 16,
		"idle_workers": 0,
		"work_range": 3,
		"turns_of_food": 12.0,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": STALE_VERB_CREW,
				"target_x": 68, "target_y": 12, "floor": STALE_VERB_FLOOR,
				"improvement": "",
				"actual_yield": rate, "sustainable_yield": rate, "realized_yield": rate,
				"overdraws": false},
		],
	}

## **THE PATCH BEING CULTIVATED — a WILD stand with the rung's build genuinely in flight.**
##
## The stale-verb patch one screen up is its exact opposite and the pair is the point: there the
## `Cultivate` was a leftover verb that must dip NOTHING, here it is a real build that must dip
## everything, and the same fields (`is_cultivated` / `cultivation_progress`) decide which. So this
## one is UNCULTIVATED with a part-filled meter — `_build_improvement_control`'s RUNNING branch, a
## live 25% carry, and no knowledge gate anywhere near it (the running branch is chosen before the
## offer is looked up).
##
## Its stock and capacity are its own (no `_seed_forage_rows`), for the reason the stale-verb fixture
## gives: this frame is about a particular `B / K` — a hair under the food peak — and a shared
## capacity/stock pair would round the whole regime away.
func _building_patch_tile_fixture() -> Dictionary:
	return {
		"x": 68, "y": 12,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.72,
		"temperature": 18.0,
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": BUILD_DIP_STOCK,
		"patch_carrying_capacity": BUILD_DIP_CAPACITY,
		# WILD ground with the rung under construction — the two fields `improvement_is_done` reads,
		# stated the opposite way round from the stale-verb patch.
		"patch_is_cultivated": false,
		"patch_cultivation_progress": 0.35,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		# The stale-verb patch's basket, verbatim: only the two staples pay food, so the patch converts
		# at well under a pure-staple rate and the ⚠ has a real take to fire on.
		"patch_provisions_per_biomass": STALE_VERB_FOOD_PER_BIOMASS,
		"patch_trade_per_biomass": STALE_VERB_TRADE_PER_BIOMASS,
		"patch_fodder_per_biomass": 0.0,
		"patch_per_worker_biomass": STALE_VERB_PER_WORKER_BIOMASS,
		"patch_per_worker_yield": STALE_VERB_PER_WORKER_BIOMASS * STALE_VERB_FOOD_PER_BIOMASS,
		"patch_cultivate_build_fraction": STALE_VERB_BUILD_FRACTION,
		"patch_sow_build_fraction": STALE_VERB_BUILD_FRACTION,
		"patch_cultivate_crew_needed": BUILD_DIP_CREW_NEEDED,
		"patch_sow_crew_needed": 3,
		"patch_sow_site_refusal": "too_dry",
		"patch_tended_yield": 1.20,
		"patch_field_yield": 2.40,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.65,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "flax", "role": "cash", "display_name": "Flax Fields", "share": 0.35,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.30, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
	}

## The band cultivating it — enough idle hands that the STEPPER, not the roster, is what bounds the
## crew. The reaching crew is the number the *clear it now* target now names, and a band that cannot
## staff it would make every assertion about that target a claim about labor scarcity instead.
##
## **IT CARRIES THE STANDING ASSIGNMENT, and that is what makes the build LIVE rather than LAPSED.** A
## part-filled cultivation meter with nobody on the tile is a patch REVERTING, which is what the tile
## card would say — a different state from the one this frame is about, rendered beside a sheet
## composing the opposite. `rate` is filled in by the caller from the tile's own wire terms, the
## stale-verb band's rule: the card's standing rate and the sheet's crew targets must be answering
## about one patch by construction.
func _building_patch_band_fixture(rate: float) -> Dictionary:
	return {
		"id": "Band 1",
		"size": 34,
		"entity": 823,
		"faction": 0,
		"pos": [67, 11],
		"current_x": 67, "current_y": 11,
		"activity": "forage",
		"working_age": 20,
		"idle_workers": BUILD_DIP_IDLE_WORKERS,
		"work_range": 3,
		"turns_of_food": 12.0,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": BUILD_DIP_CREW,
				"target_x": 68, "target_y": 12, "floor": BUILD_DIP_FLOOR,
				"improvement": "cultivate",
				"actual_yield": rate, "sustainable_yield": rate, "realized_yield": rate,
				# The stock RISES under this crew, so the sim's own flag is false here — the fact the
				# sheet's ⚠ was contradicting.
				"overdraws": false},
		],
	}

## QUALIFYING GROUND for `Sow` — an alluvial plain beside fresh water, i.e. one of the ~46 tiles of
## 4160 (1.1%) on the standard map that will actually take seed. `patch_sow_site_refusal` is "" (the
## sim's verdict: no fault), so the ▦ Sow option ENABLES once Seed Selection is known. The Sow
## forecast pair is deliberately asymmetric with Cultivate's: `ceiling_sow` is ~0 because a sown
## patch has no standing crop to take a fraction of (a bare-ground sow is PURE investment), and
## `field_yield` is 2× the tended yield — the payoff that makes the ladder's top plant rung worth it.
func _sowable_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	# Kept WITHIN the reference band's forage range (it sits on 66,10 with work_range 2) so the Forage
	# button ENABLES: this state exists to judge the Sow affordance, and an out-of-range tile disables
	# the button for an unrelated reason and hides exactly what the frame is for.
	tile["x"] = 67
	tile["y"] = 11
	tile["terrain_label"] = "Alluvial Plain"
	tile["tags_text"] = "Fertile, Fresh Water"
	tile["food_module"] = "riverine_delta"
	tile["food_module_label"] = "Riverine Delta"
	tile["site_name"] = ""
	# The ground answers the site requirement: rich enough AND watered. No refusal.
	tile["patch_sow_site_refusal"] = ""
	tile["patch_ceiling_sow"] = 0.02
	tile["patch_field_yield"] = 2.40
	return _seed_forage_rows(tile)

## The OTHER refusal. `_food_tile_fixture` is "too_dry" (rich prairie away from water); this is thin
## upland ground — watered, but too poor to take a crop without fertilizing. The two messages must
## differ, name different faults, and each point at the rung that lifts it.
func _sow_too_poor_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	# In range of the reference band, like `_sowable_tile_fixture` — the refusal must be the ONLY
	# reason Sow is unavailable in this frame.
	tile["x"] = 65
	tile["y"] = 11
	tile["terrain_label"] = "Montane Highland"
	tile["tags_text"] = "Thin Soil, Fresh Water"
	tile["food_module"] = "montane_highland"
	tile["food_module_label"] = "Montane Highland"
	tile["site_name"] = ""
	tile["patch_sow_site_refusal"] = "too_poor"
	return tile

## A patch mid-SOW: the rung-3 build meter is running, so the Field row reads "Sowing 45%". It sits
## BESIDE the Cultivation row (this ground was tended first) — the two meters are independent and
## both are the SOURCE's own, which is the per-source half of the two-meter split.
func _sowing_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	tile["patch_field_progress"] = 0.45
	tile["patch_is_field"] = false
	return tile

## A COMPLETED Field — the top of the plant ladder. The row must read "▦ Field" (SIGNAL), a visibly
## DIFFERENT THING from "🌾 Tended Patch", not a bigger percentage.
## **A FIELD SOWN STRAIGHT FROM WILD GROUND — the state `_field_tile_fixture` cannot reach.** That one
## climbs the ladder rung by rung (`_sowing_tile_fixture` sets `patch_is_cultivated`), so on it a
## Field is also cultivated and the retire test passes for the wrong reason. `Sow` needs no prior
## patch, so this is the shipped shape too: rung 3 built, rung 2's meter at ZERO and staying there.
## It is the frame the "a completed Field offers Cultivate" defect lived in.
func _wild_sown_field_tile_fixture() -> Dictionary:
	var tile := _field_tile_fixture()
	tile["patch_cultivation_progress"] = 0.0
	tile["patch_is_cultivated"] = false
	return tile

func _field_tile_fixture() -> Dictionary:
	var tile := _sowing_tile_fixture()
	tile["patch_field_progress"] = 1.0
	tile["patch_is_field"] = true
	# A completed Field reports every ceiling == per_worker_yield (a managed source needs one worker),
	# exactly as a tended patch does — so the stepper caps at 1.
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return _seed_forage_rows(tile)

## **A TILE THAT PAYS ALL THREE ACCOUNTS — the frame face treatment A is judged on (#426).** A hay
## meadow: thin human food, a token of trade, and real FODDER, which is the account the whole plant
## web grew a third column for. Every other forage fixture pays provisions alone, so until this one
## existed the picker's three-account face and the column ceiling it triggers had NO frame at all.
##
## **The rows are written out rather than derived.** `_seed_forage_rows` seeds trade and fodder to 0
## by design (so a reseeding pass leaves every existing frame byte-identical), which is exactly the
## thing under test here — so this fixture overwrites the three account dicts afterwards, the
## "genuinely non-derivable row" case that helper's docstring names.
##
## **EVERY ACCOUNT DESCENDS WITH THE FLOOR NOW, and that is a real simplification.** The trade column
## used to be non-monotone: `Deplete` alone carried `market.trade_goods_multiplier` (x4), a POLICY
## markup on stripping the patch for sale, so its cell sat above Eradicate's. The harvest-floor arc
## retired that markup — a deeper floor earns more trade only because it takes more BIOMASS — so all
## three accounts are one stock through three fixed rates and no column can invert.
func _hay_meadow_tile_fixture() -> Dictionary:
	var tile := _fodder_basket_tile_fixture()
	tile["x"] = 65
	tile["y"] = 9
	tile["terrain_label"] = "Prairie Steppe"
	tile["food_module"] = "savanna_grassland"
	tile["food_module_label"] = "Savanna Grassland"
	tile["site_name"] = ""
	# **THE TWO ACCOUNTS BIND DIFFERENTLY, and that is the fixture's real job** — see
	# `HAY_MEADOW_FODDER_PER_BIOMASS` for the sizing. Food is slow to GATHER off ground that carries
	# plenty of it, so LABOR binds on provisions; hay comes in fast off a meadow that regrows little of
	# it, so the CEILING binds on fodder.
	tile["patch_per_worker_yield"] = 0.08
	tile["patch_ceiling_sustain"] = 0.60
	tile["patch_ceiling_surplus"] = 0.90
	tile["patch_ceiling_deplete"] = 1.35
	tile["patch_ceiling_eradicate"] = 2.10
	tile["patch_ceiling_cultivate"] = 0.06
	tile["patch_ceiling_sow"] = 0.02
	# The species-BLIND patch payoffs. A crop the player picks substitutes its own three (Hay Grass
	# pays 0.72 fodder at rung 2, 1.80 at rung 3), so these are what a COMMITTED patch quotes.
	tile["patch_tended_yield"] = 0.30
	tile["patch_tended_trade"] = 0.02
	tile["patch_tended_fodder"] = 0.72
	tile["patch_field_yield"] = 0.60
	tile["patch_field_trade"] = 0.04
	tile["patch_field_fodder"] = 1.80
	# **THE NON-FOOD ACCOUNTS ARE THE PATCH'S OWN RATES, stated directly.** `_seed_forage_rows` derives
	# each account's per-biomass rate from the food-peak ceiling the fixture names, which is the right
	# reversal for a food account; the two non-food ones are independent facts about what GROWS here,
	# so they are authored as the rates the wire actually carries and the seeder is told the peak
	# ceilings they stand for. A patch's per-worker term for these two is NOT on the wire at all — the
	# client recovers it from `per_worker_yield / provisions_per_biomass`, one biomass throughput
	# serving all three accounts — so there is nothing per-account left to author here.
	tile["patch_trade_per_biomass"] = HAY_MEADOW_TRADE_PER_BIOMASS
	tile["patch_fodder_per_biomass"] = HAY_MEADOW_FODDER_PER_BIOMASS
	tile = _seed_forage_rows(tile)
	return tile

## **A DESCRIBED PATCH THAT PAYS NOTHING — the state issue #426 is named after.** Deep winter on the
## same meadow: the wire carries a full per-policy row for every rung and every cell in it is zero.
##
## This is NOT `_barren_tile_fixture`, and the difference is the whole issue: that tile has no food
## module, so there is no patch to forecast and the sheet correctly shows no compose block at all.
## Here there IS a patch, the sim HAS answered, and the answer is "nothing this season". The forecast
## must therefore read as KNOWN — the sheet stays loud, states the zeros, and keeps the worker cap
## live at `MAX_USEFUL_BARREN` — rather than falling through the "the wire said nothing" branch, which
## went silent and switched the cap off entirely.
func _dead_season_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["x"] = 66
	tile["y"] = 9
	tile["patch_ecology_phase"] = "collapsing"
	tile["patch_biomass"] = 0.0
	# Nothing grows, so nothing is worth committing to either — the investment rungs' payoffs go with
	# the harvest. The basket stays: which plants LIVE here is not a seasonal fact.
	tile["patch_tended_yield"] = 0.0
	tile["patch_tended_trade"] = 0.0
	tile["patch_tended_fodder"] = 0.0
	tile["patch_field_yield"] = 0.0
	tile["patch_field_trade"] = 0.0
	tile["patch_field_fodder"] = 0.0
	tile["patch_per_worker_yield"] = 0.0
	# **THE CREW THROUGHPUT IS HONESTLY ZERO, AND IT IS STATED RATHER THAN SEEDED.** The wire's
	# `perWorkerBiomass` folds in the tile's seasonal weight, so a dead season really does move no
	# biomass per gatherer — and this is the one fixture that must say so, because it is the case the
	# panel's crew arithmetic must not divide by. `_seed_growth_terms` would otherwise fall back to the
	# config's throughput here, since a zero food rate makes its exact recovery unavailable.
	tile["patch_per_worker_biomass"] = 0.0
	for policy in ["sustain", "surplus", "deplete", "eradicate", "cultivate", "sow"]:
		tile["patch_ceiling_%s" % policy] = 0.0
	tile = _seed_forage_rows(tile)
	return tile

## A herd mid-TAME on a pen-ceiling species: the 🐾 Tame rung is available and selected, and the herd's
## OWN meter reads 40% (`domestication`). It is the base of the taming family below; the TWO-METER
## SPLIT is staged on its fully-tamed variant, since only a retired Tame lets the gated Corral — the
## bridge between the two meters — render at all (see `two_meter_split`).
func _taming_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "pen"
	fixture["domestication"] = 0.4
	fixture["ecology_phase"] = "thriving"
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## THE BOTH-PRODUCTS INVESTMENT PAYOFF (issue #397) — a Wild Boar, edible AND worth its hide/bristles,
## on a pen-ceiling species so BOTH investment rungs are offered. Its four extractive rungs already pay
## a pair; the numbers here are the OTHER pair, the one the payoff faces render:
##   Tame   → `pastoral_yield` 1.48 food + `pastoral_trade` 0.37 trade  ⇒ `→ 1.48 food · 0.37 trade`
##   Corral → `corral_yield`   2.95 food + `corral_trade`   0.74 trade  ⇒ `→ 2.95 food · 0.74 trade`
## The food halves are the boar figures from the issue's own report (where the faces read `→ 1.48 food`
## and `→ 2.95 food` and the trade halves were dropped); each trade half is a quarter of its food half,
## the boar's hide-to-meat ratio, so the pair ascends together up the ladder exactly as the extractive
## caps do. Ordering is the ladder's: Sustain (0.90) < Tame (1.48) < Corral (2.95).
## `domestication` stays mid-ladder (0.4) because Tame RETIRES from the picker at full domestication
## while Corral is gated below it — the only way both rungs appear at once, with Corral greyed and
## still wearing its payoff.
func _investment_pair_boar_herd() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["id"] = "game_boar_11"
	fixture["label"] = "Wild Boar (game_boar_11)"
	fixture["species"] = "Wild Boar"
	fixture["size_class"] = "medium"
	fixture["pastoral_yield"] = 1.48
	fixture["pastoral_trade"] = 0.37
	fixture["corral_yield"] = 2.95
	fixture["corral_trade"] = 0.74
	return fixture

## The same herd, STRESSED — the "why isn't my Tame progressing?" case. Taming accrues only while the
## herd is Thriving, but the verb is NOT gated on it (a herd's phase swings as you hunt it): the sim
## just PAUSES the meter. Nothing else in the HUD would tell the player, so the drawer must.
func _taming_stalled_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["ecology_phase"] = "stressed"
	return fixture

## A still-WILD but tameable herd (pen ceiling) for the taming-startup-lag guard. It is NOT yet managed,
## so its OWNERSHIP-GATED `herders_needed` is 0 — but its ownership-INDEPENDENT would-be herder crew
## (`herders_needed_if_managed`, from biomass) is 10, set DELIBERATELY ABOVE this herd's Sustain
## take-useful (7, driven by the carry model) so the "no leak" companion is meaningful: composing Tame
## floors the cap UP to the 10-crew, while composing the extractive Sustain must stay at its own 7 — a
## crew-floor leak into Sustain would instead bump it to 10, which the companion asserts does NOT happen.
## A herd whose TAMING IS FINISHED — `domestication` at the sim's completion threshold, which RETIRES
## ◎ Tame (its per-source meter is full, so the improvement control shows it as the DONE state) and
## makes 🐄 Corral the rung on offer. It is managed at that point, so it carries a real herder crew
## through `_set_managed_herders` — the field pair every herd fixture owes the frame guard.
##
## **It is also the only shape on which a Corral GATE can render**, which is why `two_meter_split`
## stages it: a gate reason needs the rung to be the one on offer, and Corral only ever is once Tame
## has retired.
func _fully_tamed_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["domestication"] = SourceForecast.DOMESTICATION_COMPLETE
	_set_managed_herders(fixture, TAMED_HERD_CREW)
	return fixture

## The band STANDING on Tame on that herd — the fixture the re-admission frame turns on. Everything
## else about `_band_fixture` is kept; only the assignment list is replaced, by the single hunt
## assignment whose `fauna_id` matches `_fully_tamed_herd_fixture`'s and whose policy is the rung the
## ceiling pass has since hidden.
func _tame_standing_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["labor_assignments"] = [{
		"kind": "hunt", "workers": TAMED_HERD_CREW, "fauna_id": _taming_herd_fixture()["id"],
		"floor": 0.5, "improvement": "tame", "target_x": 70, "target_y": 17,
		"actual_yield": 0.45, "sustainable_yield": 0.45,
		"workers_needed": TAMED_HERD_CREW, "overdraws": false,
	}]
	return band

func _tame_worker_cap_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["herders_needed"] = 0
	# **THE WOULD-BE CREW HAS TO OUT-RANK THE TAKE-USEFUL for this frame to test the floor at all**,
	# and the number the take side answers QUADRUPLED when the build dip moved onto the crew
	# (`docs/plan_harvest_floor.md` §3.1): a Tame builder now needs ~27 hands to haul the same peak
	# whole-animal drop it needed 7 for. At the old 10 the floor no longer binds and the frame would
	# have been testing the take side under the floor's name.
	fixture["herders_needed_if_managed"] = TAME_CAP_WOULD_BE_HERDERS
	return fixture

## THE STALE-DATA REOPEN PAIR (`herd_compose_reopen_fresh`) — ONE herd id on two consecutive turns.
##
## Turn N, still WILD: the meter has not moved (`domestication` 0) and nothing is owned, so the
## ownership-gated `herders_needed` is 0 while the would-be crew is a real 3. Turn N+1, TAMING has
## started: the sim has taken the herd, so the crew is real (4, both halves equal — the sim exports
## them that way on a managed herd) and the meter has just left zero.
##
## EVERYTHING ELSE IS DELIBERATELY IDENTICAL — same id, same species, same ceilings, same tile — so
## `_herd_actions_shape` cannot tell the two apart and the restate takes the same-shape PATCH path.
## What the sheet then quotes is decided entirely by whether the retained compose-open closure kept a
## captured dict or re-resolves the live one. With the base fixture's Tame ceiling (0.23) over its
## per-worker yield (0.30) the take-side max-useful is 1, so the Tame rung's cap is the herder floor
## outright: 3 on the wild herd, 4 on the taming one.
func _reopen_wild_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["id"] = REOPEN_HERD_ID
	fixture["label"] = "Red Deer (%s)" % REOPEN_HERD_ID
	fixture["domestication"] = 0.0
	fixture[HERDERS_NEEDED_KEY] = 0
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = REOPEN_WILD_WOULD_BE_HERDERS
	return fixture

## The same herd one turn later, taming under way and owned — see `_reopen_wild_herd_fixture`.
func _reopen_taming_herd_fixture() -> Dictionary:
	var fixture := _reopen_wild_herd_fixture()
	fixture["domestication"] = REOPEN_TAMING_DOMESTICATION
	_set_managed_herders(fixture, REOPEN_TAMING_HERDERS)
	return fixture

## A nearly-tamed herd, FULLY STAFFED — the calm control for the staffing readout, AND the fix for the
## stale-count bug (fauna neglect-escape arc). `_band_fixture` has 4 herders (a Hunt assignment) on
## game_deer_07, and this herd needs 4, so the drawer reads a neutral "Herders: 4 / 4" — from the
## ACTUAL assigned count, not `herded_fraction`. `herded_fraction` is deliberately left STALE at 0.4
## (last turn's resolved value): the OLD code reconstructed `round(0.4 · 4) = 2` and wrongly read a
## self-contradictory "2 / 4 — under-herded", which this frame proves is gone.
func _fully_herded_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["domestication"] = 0.9
	_set_managed_herders(fixture, 4)
	fixture["herded_fraction"] = 0.4
	return fixture

## The SAME herd, UNDER-HERDED — animals are drifting off (fauna neglect-escape arc; neglect no longer
## decays tameness, it sheds whole animals to the wild). The herd now needs 6 herders but `_band_fixture`
## only staffs 4, so the drawer reads the amber "Herders: 4 / 6 — under-herded" (the ACTUAL staffed
## count) plus the muted "Under-herded — animals are drifting off. Staff all 6 herders to hold the herd."
## line — NEVER the retired "tameness slipping" copy. `herded_fraction` is left STALE at 1.0: the OLD
## code reconstructed `round(1.0 · 6) = 6` and read a calm "6 / 6" with NO warning at all — the exact
## stale-reading this fix removes.
func _under_herded_herd_fixture() -> Dictionary:
	var fixture := _fully_herded_herd_fixture()
	fixture["domestication"] = 0.98
	_set_managed_herders(fixture, 6)
	fixture["herded_fraction"] = 1.0
	return fixture

## Set BOTH herder counts on a MANAGED herd fixture. The sim exports them EQUAL there (see the
## field-pair guard above `_guard_herd_fields`), and setting them one at a time is precisely the
## mistake the guard exists to catch — so managed fixtures set them together, through this.
## A still-WILD but tameable herd is the one case where they differ and writes them by hand
## (`_tame_worker_cap_herd_fixture`: gated 0, would-be 10).
func _set_managed_herders(fixture: Dictionary, needed: int) -> void:
	fixture[HERDERS_NEEDED_KEY] = needed
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = needed

## The world's herd list (Main pushes snapshot["herds"]). Named because the turn-orb starving-pen
## state swaps in its own list and must restore this one.
func _world_herds_fixture() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120, "ecology_phase": "stressed", "food_per_animal": 2.0},
	]

## Two herds a band works at once — a FAST animal (several a turn) and a BIG one (one every several
## turns) — so the Current-actions rows can show both kill-RHYTHMs. `food_per_animal` is in PROVISIONS
## (`HerdTelemetryState.foodPerAnimal`, the decoded key), matched to the assignment's food rate:
## mammoth 16 food/animal ÷ 2.4 food/turn ≈ 7 turns; fowl 2.0 ÷ 2.6 ≈ 1.3/turn.
func _hunt_rhythm_herds_fixture() -> Array:
	return [
		{"id": "game_fowl_01", "species": "Marsh Fowl", "x": 71, "y": 18, "food_per_animal": 2.0},
		{"id": "game_mammoth_01", "species": "Woolly Mammoth", "x": 70, "y": 17, "food_per_animal": 16.0},
	]

## A band worked on TWO hunt sources — the render-honesty frame for the summary row's honest per-turn
## FOOD rate (fix #1) and the under-crewed `wastedYield` note (fix #5). Row 1 is a FAST animal; row 2 a
## BIG animal whose `actualYield` is 0.00 THIS turn — the "+0.00 /turn" lie the row used to headline —
## and which is under-crewed, so the muted "· N wasted" note shows. Neither row shows a `≈… /turn`
## animals-per-turn cadence: on a summary row the sustainable food rate is enough.
func _hunt_actions_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["labor_assignments"] = [
		# Fast: honest rate 2.60/turn. A Sustain animal → the sim-answered `overdraws` is false (no ⚠).
		{"kind": "hunt", "workers": 3, "fauna_id": "game_fowl_01", "floor": 0.5,
			"target_x": 71, "target_y": 18, "actual_yield": 2.60, "sustainable_yield": 2.60,
			"workers_needed": 3, "overdraws": false},
		# Big: honest rate 2.40/turn (the sim's measured Mammoth Sustain). actual_yield 0.00 = a wait turn
		# of the kill pulse (the old lie the row used to headline). Under-crewed → the muted "· 1.9 wasted".
		# Sustain → overdraws false, so no ⚠.
		{"kind": "hunt", "workers": 2, "fauna_id": "game_mammoth_01", "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 0.00, "sustainable_yield": 2.40,
			"workers_needed": 5, "wasted_yield": 1.9, "overdraws": false},
	]
	return band

func _herd_fixture() -> Dictionary:
	return {
		"id": "game_deer_07",
		"label": "Red Deer (game_deer_07)",
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# Ecological carrying capacity + grazing range (Grazing Phase 2b-iii): the numbers that explain
		# the herd's size. Big game roams a radius-1 range (7 tiles); on good steppe it caps ~2150, well
		# above this herd's 820 biomass, so the drawer reads the healthy "Herd: 8 / 22" pair with no
		# overgrazing warning. The dedicated grazing states below dial in overgrazed / small-game.
		"carrying_capacity": 2150.0,
		# ONE animal's biomass — what turns both numbers above into the ANIMAL counts the drawer and the
		# floor flag state. **Pinned to the fixture's own `food_per_animal`**, not chosen freely: the
		# sim's identity is `food_per_animal = body_mass × provisions_per_biomass`, so at the deer's
		# 0.02 this must be 2.0 / 0.02 = 100 or the fixture asserts against a herd that could not exist.
		"body_mass": 100.0,
		"graze_range_radius": 1,
		"route_length": 3,
		# One animal's worth of FOOD (provisions) — `HerdTelemetryState.foodPerAnimal`, the exact key the
		# decoder now emits. The kill-rhythm divides it by the food rate (both provisions): 2.0
		# food/animal vs a 0.90/turn Sustain take reads "≈1 Red Deer / 3 turns".
		"food_per_animal": 2.0,
		# Pre-commit yield forecast (food/turn at this herd's biomass, at output_multiplier 1.0).
		# Sustain admits ceil(0.90 / 0.30) = 3 useful hunters, below the reference band's 7 assignable
		# (3 idle + the 4 it already has on this herd), so the Hunters stepper caps at 3 with the
		# "max 3 workers useful here" note.
		"per_worker_yield": 0.30,
		# The two INVESTMENT rungs' PAYOFFS — the food/turn each rung pays ONCE prepared (the pastoral
		# MSY after taming, the pen's sustained rate once built), NOT the during-build dip. Ordered
		# Sustain (0.90) < Tame (1.20) < Corral (1.50) so the picker's `→ +Y/turn` payoff buttons read
		# as an ascending ladder, both clearly above Sustain's `up to +0.90/turn` cap.
		"corral_yield": 1.50,
		"pastoral_yield": 1.20,
		"corral_progress": 0.0,
		# EVERY ceiling — the four extractive rungs plus the Tame/Corral DIPS — rides this ONE list;
		# the herd has no flat `ceiling*` scalars on the wire any more (deprecated schema slots). The
		# sim exports a row for every one of the six `FollowPolicy::HUNT_POLICIES`, so this is the
		# shape the decoder produces and where `SourceForecast.forecast_inputs` reads every herd ceiling.
		"hunt_policy_ceilings": {
			"sustain": 0.90,
			"surplus": 1.80,
			"deplete": 2.70,
			"eradicate": 4.50,
		},
		# **THE TWO BUILD DIPS, AS FRACTIONS** (issue #442) — they were `tame` / `corral` ROWS of the
		# list above, each the 0.23 a builder took because Sustain (0.90) was the only stance a builder
		# could hold. 0.23 / 0.90 is that same dip stated as the factor it always was, and it now
		# multiplies WHICHEVER stance the crew holds: a Deplete builder takes 2.70 x 0.256 = 0.69.
		"tame_build_fraction": 0.23 / 0.90,
		"corral_build_fraction": 0.23 / 0.90,
		# The TRADE half of the same list (issue #337) — the decoder fills both dicts in one pass over
		# the one wire list, so a fixture that carries the food rows must carry these or the picker
		# under-reports what the rung pays.
		"trade_per_animal": 0.30,
		"per_worker_trade": 0.05,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.14,
			"surplus": 0.27,
			"deplete": 0.41,
			"eradicate": 0.68,
		},
		"tile_info": _food_tile_fixture(),
	}

## A DEADLY-TO-HUNT herd (Predators Phase 0): a woolly mammoth — high attack (8) and high ferocity
## (0.9, it fights back), but aggression 0 (a grazer never attacks unprovoked). Its drawer shows high
## Attack + Fights back bars and an EMPTY Aggressive bar — the split that proves strength ≠ danger.
## Compact tile so the component/husbandry rows land in-frame.
func _deadly_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_mammoth_02"
	fixture["label"] = "Woolly Mammoth (game_mammoth_02)"
	fixture["species"] = "Woolly Mammoth"
	fixture["husbandry_ceiling"] = "wild"
	fixture["attack"] = 8.0
	fixture["defense"] = 12.0
	fixture["ferocity"] = 0.9
	fixture["aggression"] = 0.0
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A PREDATOR (Predators Phase 1a): a Grey Wolf Pack — big, wild-ceiling, carnivore. `prey_sense_radius`
## 4 (`> 0`) is BOTH the "this is a predator" signal AND the map ring radius, so the drawer must read
## "Size: Big predator" (not "Big game") and "Wild predator — hunt only" (not "Wild game — hunt only").
func _predator_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "predator_wolf_01"
	fixture["label"] = "Grey Wolf Pack (predator_wolf_01)"
	fixture["species"] = "Grey Wolf Pack"
	fixture["size_class"] = "big"
	fixture["husbandry_ceiling"] = "wild"
	fixture["prey_sense_radius"] = 4
	fixture["attack"] = 5.0
	fixture["defense"] = 3.0
	fixture["ferocity"] = 0.8
	fixture["aggression"] = 0.7
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## Predators Phase 3 — a band UNDER an active raid, both legibility surfaces lit at once:
##   • `raid_radius` 3 (the sim's echoed `predators.raid_radius`) + a VISIBLE camp-menacing predator
##     placed one tile off the band's [71,18] in the world-herd list (`_raiding_predator_herd_fixture`)
##     → the Warrior card's live crimson "⚠ Predator nearby — N on guard" alert.
##   • `raid_forfeit` 1.20 (`PopulationCohortState.raidForfeit`, food lost to raids THIS turn) → the
##     "⚔ Lost to raids −1.20" food-ledger row and a net dragged negative the turn the raid landed.
## Reuses entity 904, so `BAND_DISCLOSURE_FOOD` opens its ledger popover.
func _raided_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["raid_radius"] = 3
	band["raid_forfeit"] = 1.20
	return band

## The VISIBLE predator the raided band can see: one tile off its [71,18] (hex distance 1, well inside
## `raid_radius` 3). `prey_sense_radius > 0` marks it a predator and `attack × aggression > 0` marks it a
## camp menace — the exact THREAT product `_band_predator_threat_present` (and the map overlay) key off.
func _raiding_predator_herd_fixture() -> Dictionary:
	return {
		"id": "predator_wolf_02",
		"species": "Grey Wolf Pack",
		"x": 70, "y": 18,
		"prey_sense_radius": 4,
		"attack": 5.0,
		"aggression": 0.7,
		"food_per_animal": 0.0,
	}

## Assertion helpers for the Predators component rows. `_danger_component_rows_present` = all four
## component keys emitted; `_danger_verdict_word_present` = the OLD danger vocabulary must be gone (a
## word can't survive the roster); `_danger_row_value` extracts a row's value cell for the bar/% check.
func _danger_component_rows_present(lines: Array) -> bool:
	for key in ["Attack", "Defense", "Fights back", "Aggressive"]:
		if _danger_row_value(lines, key) == "":
			return false
	return true

## True when SOME produced line is EXACTLY `text` (used to pin a specific "Herders: N / M" readout).
func _lines_contain(lines: Array, text: String) -> bool:
	for line in lines:
		if String(line) == text:
			return true
	return false

## True when SOME produced line CONTAINS `text` (used to pin the shed copy / prove the old copy is gone).
func _lines_any_contain(lines: Array, text: String) -> bool:
	for line in lines:
		if String(line).contains(text):
			return true
	return false

func _danger_verdict_word_present(lines: Array) -> bool:
	for line in lines:
		var text := String(line)
		for word in ["Harmless", "Minor", "Dangerous", "Deadly"]:
			if text.contains(word):
				return true
	return false

func _danger_row_value(lines: Array, key: String) -> String:
	# The three components `Danger` is made of are INDENTED under it, so a row is matched on its key
	# with the indent stripped rather than on a bare `begins_with`. `Defense` is not one of them and
	# stands flat, which the empty prefix still finds.
	for prefix in ["%s%s: " % [DetailFormat.DANGER_COMPONENT_INDENT, key], "%s: " % key]:
		for line in lines:
			var text := String(line)
			if text.begins_with(prefix):
				return text.substr(prefix.length())
	return ""

## A WILD-ceiling herd (Grazing 2d-δ): hunt-only. The drawer shows NO husbandry track (no
## domestication / corral / pen rows) — just the "Wild game — hunt only" hint — and the hunt policy
## picker drops the Corral rung.
func _wild_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "wild"
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A BIG-GAME wild herd whose WHOLE-ANIMAL body outweighs one hunter's carry — the frame the peak-turn
## carry cap is judged on. An aurochs is one 80-biomass body dropped whole by the kill-credit bank;
## `food_per_animal` 1.6 is that body in food, and one hunter carries only `per_worker_yield` 0.80. So a
## lone hunter carrying an aurochs WASTES half — the panel must say TWO hunters are useful, not one.
##   Sustain ceiling 0.74: old cap = ceil(0.74 / 0.80) = 1 (the bug); new cap =
##     ceil((floor(0.74 / 1.6) + 1) × 1.6 / 0.80) = ceil(1.6 / 0.80) = 2 → "max 2 workers useful".
##   Deplete ceiling 1.86: two bodies drop on the peak turn → ceil((floor(1.86/1.6)+1) × 1.6 / 0.80) =
##     ceil(3.2 / 0.80) = 4 → the cap tracks the selected policy's ceiling upward.
func _aurochs_big_game_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_aurochs_04"
	fixture["label"] = "Wild Aurochs (game_aurochs_04)"
	fixture["species"] = "Wild Aurochs"
	fixture["husbandry_ceiling"] = "wild"
	fixture["food_per_animal"] = 1.6
	fixture["per_worker_yield"] = 0.80
	fixture["hunt_policy_ceilings"] = {
		"sustain": 0.74, "surplus": 1.20, "deplete": 1.86, "eradicate": 2.60,
	}
	fixture["trade_per_animal"] = 0.24
	fixture["per_worker_trade"] = 0.12
	fixture["hunt_policy_trade_ceilings"] = {
		"sustain": 0.11, "surplus": 0.18, "deplete": 0.28, "eradicate": 0.39,
	}
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A compact NON-food tile_info (like the domesticated/hunt-distance herds) so the tile card stays
## short and the herd drawer's husbandry rows land in-frame rather than below the dock scroll fold.
func _compact_herd_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A PASTORAL-ceiling herd (Grazing 2d-δ): tameable + roams, but never pennable. The drawer keeps the
## domestication (Husbandry) row but shows "Herdable, not pennable" where the Corral rows would sit, and
## the hunt policy picker drops the Corral rung.
func _pastoral_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "pastoral"
	fixture["domestication"] = 0.6
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## Ground that offers NOTHING to gather: no food module, no patch. The land row's meta must read
## "No forage" (not a blank), and the drawer must carry terrain rows with no compose block.
func _barren_tile_fixture() -> Dictionary:
	return {
		"x": 71, "y": 4,
		"terrain_label": "Rocky Regolith",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.07,
		"temperature": 2.0,
		"food_module": "",
		"food_module_label": "",
		"height_display": "62 ▮▮▮▮▮▯▯▯",
	}

## THE CROWDED HEX — 3 bands + 2 herds, i.e. six subject rows once the land is counted. The state
## the height cap is judged on: every row visible, the drawer capped, the dock not scrolling.
func _crowded_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 58
	tile["y"] = 24
	tile["units"] = _crowded_bands_fixture()
	tile["herds"] = _crowded_herds_fixture()
	return tile

## Three player bands on the crowded hex, spanning the food tiers (green / amber / red dots) and
## carrying real labor so the auto-selected band's drawer renders a full allocation block — which is
## what makes the cap do any work at all.
func _crowded_bands_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 62, "idle_workers": 9,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 15.0, "morale": 0.72,
			"activity": "forage", "stores": {"provisions": 180.0},
			"food_income": 3.2, "food_consumption": 2.4,
			"labor_assignments": [
				{"kind": "forage", "workers": 5, "target_x": 58, "target_y": 24, "floor": 0.5,
					"actual_yield": 0.96, "sustainable_yield": 0.96, "realized_yield": 0.96,
					"workers_needed": 5, "overdraws": false},
			]},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 44, "idle_workers": 4,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 7.0, "morale": 0.51,
			"activity": "scout", "stores": {"provisions": 40.0}, "labor_assignments": []},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 27, "idle_workers": 0,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 2.0, "morale": 0.30,
			"activity": "idle", "stores": {"provisions": 8.0}, "labor_assignments": []},
	]

## Two herds sharing the crowded hex — a stressed bison (amber dot) and a thriving boar (green), so
## the Wildlife group is genuinely plural and the ecology dots differ down the list.
func _crowded_herds_fixture() -> Array:
	return [
		_occupied_herd_only(),
		{
			"id": "game_boar_04",
			"label": "Wild Boar (game_boar_04)",
			"species": "Wild Boar",
			"size_class": "medium",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1010.0,
			"carrying_capacity": 1433.0,
			"graze_range_radius": 1,
			"x": 58, "y": 24,
		},
	]

## The MapView snapshot behind `tile_panel_land_sticky` — the crowded hex's OWN bands and herds on a
## grid just big enough to hold it, so MapView's `_tile_info_at` / `_units_on_tile` see exactly what
## the HUD fixture describes. Nothing is redacted because the caller turns FoW OFF explicitly — a
## fresh MapView now defaults to fog ON, and this fixture carries no visibility raster, so every
## occupant would be gated out and the assertion would pass on an empty hex.
func _sticky_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _crowded_bands_fixture(),
		"herds": _crowded_herds_fixture(),
	}

## The MapView snapshot behind `tile_panel_deselect_keeps_tile` — ONE herd and no bands, so the first
## click resolves as a herd rather than a band (a band would exercise `selected_unit_id`, the other
## half of the same clear branch, but not the herd case the issue was reported on) and `DESELECT_LAND_TILE`
## is genuinely bare. Same grid as the sticky fixture; fog is turned off by the caller.
func _deselect_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [],
		"herds": [{
			"id": DESELECT_HERD_ID,
			"label": "Red Deer (%s)" % DESELECT_HERD_ID,
			"species": "Red Deer",
			"size_class": "big",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1480.0,
			"carrying_capacity": 2150.0,
			"graze_range_radius": 1,
			"x": DESELECT_HERD_TILE.x, "y": DESELECT_HERD_TILE.y,
		}],
	}

## The MapView snapshot behind `tile_panel_occupant_cycle` — ONE band and TWO herds on a single hex,
## the smallest stack that exercises both kinds and a plural one of the second kind. Same grid as the
## sticky fixture; fog is turned off by the caller. The herds carry neither `herders_needed` half, so
## the field-pair guard skips them (they are wild, and nothing here opens a compose sheet on them).
func _cycle_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [
			{"id": "Band Wold", "entity": CYCLE_BAND_ENTITY, "faction": 0, "size": 94,
				"pos": [CYCLE_TILE.x, CYCLE_TILE.y],
				"current_x": CYCLE_TILE.x, "current_y": CYCLE_TILE.y,
				"working_age": 48, "idle_workers": 6, "work_range": 2, "hunt_reach": 4,
				"turns_of_food": 11.0, "morale": 0.64, "activity": "idle",
				"stores": {"provisions": 120.0}, "labor_assignments": []},
		],
		"herds": [
			{
				"id": CYCLE_HERD_FIRST_ID,
				"label": "Aurochs (%s)" % CYCLE_HERD_FIRST_ID,
				"species": "Aurochs",
				"size_class": "big",
				"huntable": true,
				"ecology_phase": "thriving",
				"domestication": 0.0,
				"biomass": 1620.0,
				"carrying_capacity": 2400.0,
				"graze_range_radius": 1,
				"x": CYCLE_TILE.x, "y": CYCLE_TILE.y,
			},
			{
				"id": CYCLE_HERD_SECOND_ID,
				"label": "Wild Boar (%s)" % CYCLE_HERD_SECOND_ID,
				"species": "Wild Boar",
				"size_class": "medium",
				"huntable": true,
				"ecology_phase": "stressed",
				"domestication": 0.0,
				"biomass": 780.0,
				"carrying_capacity": 1360.0,
				"graze_range_radius": 1,
				"x": CYCLE_TILE.x, "y": CYCLE_TILE.y,
			},
		],
	}

## A hex with an occupant stack: 3 player bands + 1 herd, for the Occupants roster.
func _occupied_tile_fixture() -> Dictionary:
	return {
		"x": 58, "y": 24,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		"units": _occupied_units_fixture(),
		"herds": [_occupied_herd_only()],
	}

## Three player bands sharing the hex, spanning the food-status tiers (green /
## amber / red) and distinct activities (harvest / scout / idle glyphs).
func _occupied_units_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"turns_of_food": 15.0, "activity": "harvest", "stores": {"provisions": 180.0}},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"turns_of_food": 7.0, "activity": "scout", "stores": {"provisions": 40.0}},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"turns_of_food": 2.0, "activity": "idle", "stores": {"provisions": 8.0}},
	]

## The stressed herd sharing the occupied hex (amber ecology dot).
func _occupied_herd_only() -> Dictionary:
	return {
		"id": "game_bison_02",
		"label": "Steppe Bison (game_bison_02)",
		"species": "Steppe Bison",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "stressed",
		"domestication": 0.0,
		"biomass": 240.0,
		"x": 58, "y": 24,
	}

## The occupied hex's herd carrying its tile_info, so show_herd_selection renders
## the full roster with the wildlife row selected.
func _occupied_herd_fixture() -> Dictionary:
	var herd := _occupied_herd_only()
	herd["tile_info"] = _occupied_tile_fixture()
	return herd

func _collapsing_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["biomass"] = 96.0
	fixture["ecology_phase"] = "collapsing"
	fixture["domestication"] = 0.0
	return fixture

## A compact NON-food tile_info (like the corral fixtures) so the Tile card stays short and the herd
## drawer's Biomass (current/max) / Range (+ overgrazing) rows land in-frame rather than below the fold.
func _compact_herd_tile() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A HEALTHY grazing herd (Grazing Phase 2b-iii): big game (radius-1 range → "Range: 7 tiles") whose
## biomass sits below the K its range supports, so the merged "Biomass: 1480 / 2150" current/max pair
## reads current < max with NO overgrazing warning. domestication 0 keeps the frame focused on the rows.
func _grazing_healthy_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 1480.0
	fixture["carrying_capacity"] = 2150.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## An OVERGRAZING herd: biomass (2100) exceeds the K (1352) its range can sustainably feed, so the
## merged pair reads "Biomass: 2100 / 1352" (current ABOVE max) and the drawer adds the WARN-amber
## "⚠ Overgrazing — range can't sustain this herd" row. The herd is drawing its range down and will
## shrink — the honest biomass > K comparison, both numbers sim-provided.
func _overgrazing_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 2100.0
	fixture["carrying_capacity"] = 1352.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## A SMALL-GAME herd (radius-0 range): it grazes only its own tile, so the drawer reads "Range: 1 tile"
## (singular) and the map draws a single-hex highlight. Biomass below its small K → no overgrazing.
func _small_game_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_rabbit_03"
	fixture["label"] = "Rabbit Warren (game_rabbit_03)"
	fixture["species"] = "Rabbit Warren"
	fixture["size_class"] = "small"
	fixture["domestication"] = 0.0
	fixture["biomass"] = 140.0
	fixture["carrying_capacity"] = 190.0
	fixture["graze_range_radius"] = 0
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## A FULLY TAMED, not-yet-penned herd with no pen started, on the same compact tile as the corral-ready
## one — the ONE shape that can put a GATED 🐄 Corral on screen (issue #442).
##
## It was a still-wild herd (domestication 0.4) while a gated rung was a greyed button of the policy
## picker, and every rung showed at once. The improvement control offers the source's NEXT rung, so a
## part-tamed herd is offered 🐾 Tame and Corral is not rendered at all — which quietly emptied both
## corral-gate frames. Retiring Tame (a full meter) is what makes Corral the rung on offer; the only
## thing left that can gate it is the faction's PENNING, which is exactly the knowledge bridge the two
## frames document. The SOURCE half of `RungGates.hunt_gates`' Corral reason is consequently
## unreachable in this control now — the moment it would apply, Tame is offered instead, and a
## checkbox is a better remedy than a sentence.
func _corral_locked_herd_fixture() -> Dictionary:
	var fixture := _corral_ready_herd_fixture()
	fixture["corral_progress"] = 0.0
	return fixture

## A fully-domesticated herd whose pen is HALF-BUILT (not yet corralled): the Corral investment rung
## is available (knowledge + domestication both satisfied) and under way, so the hunt picker offers
## 🐄 Corral and the drawer reads "Corral: Building 40%". Compact non-food tile_info (like the
## domesticated fixture) so the Tile card stays short and the drawer rows land in-frame.
func _corral_ready_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 1.0
	fixture["corralled"] = false
	fixture["corral_progress"] = 0.4
	# `pen_upkeep` is the feed this pen WOULD demand once built (the sim projects it at the herd's
	# current biomass, on the same basis as `corral_yield`) — so the pre-commit row can quote the
	# real running cost at the moment the player decides, rather than saying "before feed".
	fixture["pen_upkeep"] = 0.34
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## A composing-Corral herd that needs MORE than one keeper (Grazing 2d-δ herder deficit): the take/prepare
## max-useful for the Corral rung is 1 ("one worker suffices to prepare"), but this growing herd needs 2
## herders EVERY turn to hold its tameness — and it is currently UNDER-herded, because the STATE dials the
## band's own hunt assignment on this herd down to 1 (the base fixture staffs 4, which would hide the very
## deficit this frame exists to show). The Herders row reads "1 / 2 — under-herded" off that ACTUAL
## assignment via `HudBandLaborState.assigned_herders_for`, and the shed consequence line names 2.
## `herded_fraction` is deliberately left STALE at 0.5 and is read NOWHERE in the render path — the same
## convention `_fully_herded_fixture` / `_under_herded_fixture` carry, and the reason the old
## reconstruct-from-the-fraction reading was retired. The compose
## stepper's cap must be max(take-useful 1, herders_needed 2) = 2, so the `+` reaches 2 and the player can
## staff the maintenance crew — otherwise the corral is lost, an unwinnable trap. A wild herd carries
## `herders_needed 0`, so this floor is a no-op there.
func _under_herded_corral_fixture() -> Dictionary:
	var fixture := _corral_ready_herd_fixture()
	# Corral is an INVESTMENT rung, and `_forecast_worker_cap`'s floor reads the ownership-INDEPENDENT
	# `herders_needed_if_managed` for those (`herders_needed` is the extractive rungs' field). The sim
	# exports the two EQUAL on an owned herd, which this one is — so a fixture setting only the first
	# floors the cap at 0 and the frame silently renders "max 1 worker useful", the very cap it exists
	# to disprove. It went unnoticed because the state used to compose Sustain by accident (#357), and
	# it is now caught by the field-pair guard rather than by a reader noticing the wrong number.
	_set_managed_herders(fixture, UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	fixture["herded_fraction"] = 0.5
	return fixture

func _domesticated_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 1.0
	# A fully-domesticated herd is penned: the drawer adds a "🐄 Corralled" row.
	fixture["corralled"] = true
	# A PENNED herd is a managed population — it eats from its keeper's larder every turn. Fully fed
	# here (`pen_fed_fraction` 1.0), so the drawer reads the healthy "🐄 Corralled" badge plus the
	# amber "Pen feed: -1.74 /turn" standing debit.
	fixture["pen_upkeep"] = PEN_UPKEEP_RED_DEER
	fixture["pen_fed_fraction"] = 1.0
	# Grazing 2d-γ — a radius-1 pen on POOR footprint: its fenced land covers NONE of the feed
	# (`pen_pasture_fraction` 0.0), so the whole GROSS demand falls on the FOOD larder as the net bill
	# (`pen_larder_bill` == gross, no hay). Feed-split reads "Fed by pasture 0% · larder 1.7 food/turn".
	# Invariant: gross × pasture(0) + hay(0) + larder(1.74) == gross(1.74).
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.0
	fixture["pen_larder_bill"] = PEN_UPKEEP_RED_DEER
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	# Compact NON-food tile_info (like the hunt-distance herd) so the tile card stays short and
	# the drawer's Husbandry + Corral rows land in-frame rather than below the dock scroll fold.
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## A DOMESTICATED but DEPLETED herd (biomass below the pen's escapement point, K/2): the pen's
## harvest takes only the biomass standing ABOVE K/2, so `corral_yield` is honestly **0.00** — penning
## this herd would eat 0.14 food/turn and pay nothing until it rebuilds. The zero is the whole point
## of the frame: it must render in full (never blanked or em-dashed) and be EMPHASIZED, because a
## player who pens this herd on a hidden zero has been misled by the UI.
func _depleted_corral_herd_fixture() -> Dictionary:
	var fixture := _corral_ready_herd_fixture()
	fixture["biomass"] = 260.0
	fixture["ecology_phase"] = "stressed"
	fixture["corral_progress"] = 0.0
	# Everything scales off the shrunken herd — including the dip, which is a share of its MSY.
	fixture["per_worker_yield"] = 0.10
	# Override the inherited ceiling table's two rows this frame reads — Sustain (the extractive
	# baseline) and the Corral DIP. The herd's ceilings live only in `hunt_policy_ceilings` now, so
	# a depleted variant must restate them here rather than shadowing them with flat scalars.
	var depleted_ceilings: Dictionary = (fixture["hunt_policy_ceilings"] as Dictionary).duplicate()
	depleted_ceilings["sustain"] = 0.10
	fixture["hunt_policy_ceilings"] = depleted_ceilings
	# The dip is a SHARE of whatever stance is held, so a shrunken herd needs no dip override at all —
	# the 0.05-against-0.10 this used to restate is exactly the inherited 0.256 fraction of the new
	# Sustain ceiling. That the override became unnecessary is the fraction form paying for itself.
	fixture["corral_yield"] = 0.0     # below K/2 → the escapement harvest takes NOTHING
	fixture["pen_upkeep"] = 0.14      # …and it would still have to be fed
	return fixture

## The SAME penned herd, STARVING: its keeper paid only 40% of the 1.74/turn feed, so the herd is
## shrinking (`pen.starve_shrink_rate × (1 − fed) × biomass`) every turn and its yield with it. The
## drawer must say so loudly — the Corral row drops its badge for a red "⚠ Starving — 40% fed", and
## the Pen feed row names the shortfall. Biomass is down from the fed fixture's 820 to show the herd
## has actually lost ground.
func _starving_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["biomass"] = 310.0
	fixture["pen_fed_fraction"] = PEN_FED_STARVING
	return fixture

## The SAME penned herd, UNDER-CREWED (`turn_orb_unworked_rung`): 2 keepers standing where the sim
## demands 4, so the shed clock has started and `neglect_grace_remaining` is counting down.
##
## **IT IS FULLY FED, ON PURPOSE.** A starving pen fires `_starving_pen_attention`'s own row off the very
## same herd, and the row COUNT is the negative control for the whole block — two producers on one herd
## would make it unreadable and would let an over-eager unworked scan hide inside the total. Its tile is
## the world-herd list's `(68, 15)` (matching the band's hunt assignment) and deliberately not the
## worked patch's `(66, 10)`, so the two webs' jump targets stay distinguishable.
func _under_crewed_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["x"] = 68
	fixture["y"] = 15
	_set_managed_herders(fixture, UNDER_CREWED_HERD_NEEDED)
	fixture["has_neglect_grace"] = true
	fixture["neglect_grace_remaining"] = NEGLECT_GRACE_HERD
	return fixture

## **THE UNWORKED-RUNG CONTROL SET** (`turn_orb_unworked_rung`) — six patches in the wire shape
## `forage_patches_to_array` produces, of which only THREE may raise a row. Every field here is one the
## producer actually reads, and each patch differs from the one above it in exactly ONE of them, so a
## failure names the condition that broke rather than "the fixture changed":
##   (70,20) tended · ours · unworked · grace 2      → a row, counting down
##   (71,20) FIELD  · ours · unworked · grace 0      → a row, the penalty biting NOW
##   (72,20) tended · ours · unworked · NO grace     → a row with no countdown at all
##   (73,20) WILD   · ours · unworked                → silent: nothing has been built here to lose
##   (74,20) tended · a RIVAL's · unworked           → silent: a rival's ground is not our alarm
##   (66,10) tended · ours · WORKED by the band      → silent: it is being kept
## The worked control carries the FULL grace window rather than omitting the pair, so its silence can
## only come from the crew on it — an absent countdown would have silenced it for the wrong reason.
func _neglect_patches_fixture() -> Array:
	return [
		{"x": 70, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_SOON},
		{"x": 71, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": true,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 72, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		{"x": 73, "y": 20, "ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		{"x": 74, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": RIVAL_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 66, "y": 10, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_FULL},
	]

## A SELF-FEEDING pen on lush land (Grazing 2d-γ): a radius-2 fenced footprint (19 tiles) whose grazing
## covers the herd's entire feed, so `pen_pasture_fraction` 1.0 and the NET larder bill `pen_larder_bill`
## is 0 (the GROSS `pen_upkeep` stays 1.74). The feed-split row reads "Fed by pasture 100% · larder 0.0
## food/turn" and the amber Pen-feed
## debit row disappears (nothing left to haul). This is the state the Extend-pen affordance renders on —
## a built pen, no ring in flight (`pen_extend_progress` 0), so `_build_herd_assign_controls` shows the
## "Extend pen" button.
func _self_feeding_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 2
	fixture["pen_footprint_tiles"] = 19
	fixture["pen_pasture_fraction"] = 1.0
	# `pen_upkeep` stays the realistic GROSS (inherited 1.74); the FOOTPRINT grazes the whole demand, so
	# the net FOOD-larder bill is 0 → "100% · larder 0.0" and the Pen-feed debit row disappears.
	# Invariant: gross × pasture(1.0) + hay(0) + larder(0) == gross(1.74).
	fixture["pen_larder_bill"] = 0.0
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	return fixture

## The SAME pen mid-EXTENSION (Grazing 2d-γ): the keeper is fencing the next ring, so
## `pen_extend_progress` is 0.6 and `_build_herd_assign_controls` replaces the "Extend pen" button with
## a WARN-amber "Fencing 60%" badge. Partial pasture (60%) so the feed-split reads "60% · larder N.N".
func _extending_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.6
	# `pen_upkeep` stays the realistic GROSS (inherited 1.74); the footprint grazes 60%, so the net
	# FOOD-larder bill is gross × (1 − 0.6) = 0.696 → "60% · larder 0.7", no hay.
	# Invariant: gross(1.74) × pasture(0.6) + hay(0) + larder(0.696) == gross(1.74).
	fixture["pen_larder_bill"] = 0.696
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.6
	return fixture

## A FODDERED pen (Flora roster F3): the pen knows Foddering and drew hay, so its feed is a THREE-way
## split, all food units. GROSS demand `pen_upkeep` = 2.0 partitions into: pasture 40%
## (`pen_pasture_fraction` 0.40 → 0.80 food grazed free), hay 0.90 (`pen_hay_food`, the food it
## displaced from the larder), and the NET bread bill 0.30 (`pen_larder_bill`). The frame PROVES the
## sim-pinned invariant, not a hand-picked answer: 0.80 + 0.90 + 0.30 == 2.0 (gross). Feed-split reads
## "Fed by pasture 40% · hay 0.9 · larder 0.3 food/turn"; the Pen-feed row states the same 0.3 net bill.
func _foddered_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_upkeep"] = 2.0        # realistic GROSS (upkeep_per_biomass × biomass scale)
	fixture["pen_pasture_fraction"] = 0.40
	fixture["pen_hay_food"] = 0.90
	fixture["pen_larder_bill"] = 0.30  # 2.0 − (2.0 × 0.40) − 0.90 == 0.30
	fixture["pen_extend_progress"] = 0.0
	return fixture

## A base terrain legend (key == "terrain") shaped exactly like
## MapView._build_terrain_legend's output: rows carry color/label/value_text plus
## the numeric `count` the sort control keys off. Counts are deliberately varied
## and out of both name/count order so the sorting is obvious.
## MapView._build_pasture_legend's output, transcribed from the map_preview "pasture" state (it prints
## the legend dict) so the two harnesses cannot disagree. The swatch colors are read off MapView's own
## constants rather than restated, so a ramp retune moves the legend with the map.
func _pasture_legend_fixture() -> Dictionary:
	var poor: Color = MAP_VIEW_SCRIPT.PASTURE_POOR_COLOR
	var rich: Color = MAP_VIEW_SCRIPT.PASTURE_RICH_COLOR
	return {
		"key": "pasture",
		"title": "Pasture (Graze Capacity)",
		"description": "Graze capacity — the ANIMAL-edible stock (grass and browse; humans cannot digest it).\nStanding stock 100% of capacity across 346 pasture tiles.",
		"rows": [
			{"color": poor.lerp(rich, 8.0 / 240.0), "label": "Poorest pasture", "value_text": "8 graze"},
			{"color": poor.lerp(rich, 138.0 / 240.0), "label": "Average pasture", "value_text": "138 graze"},
			{"color": rich, "label": "Richest pasture", "value_text": "240 graze"},
			{"color": MAP_VIEW_SCRIPT.PASTURE_DEAD_COLOR, "label": "Barren ground", "value_text": "50 tiles"},
			{"color": MAP_VIEW_SCRIPT.PASTURE_WATER_COLOR, "label": "Water", "value_text": "72 tiles"},
		],
		"stats": {"min": 8.0, "avg": 138.0, "max": 240.0},
	}

func _forage_legend_fixture() -> Dictionary:
	# The HUMAN-food twin of the pasture legend. NOTE the differences that are the whole point: there is
	# NO water row (coastal shelves carry forage and ride the ramp), the barren row is the honest
	# "No forage" (deep ocean/glacier/lava only), and the description carries the gathering-sites
	# sub-count — the tiles actually forageable today, a subset of the potential the ramp paints.
	var poor: Color = MAP_VIEW_SCRIPT.FORAGE_POOR_COLOR
	var rich: Color = MAP_VIEW_SCRIPT.FORAGE_RICH_COLOR
	return {
		"key": "forage",
		"title": "Forage (Human Food Capacity)",
		"description": "The HUMAN-edible potential of this land — seeds, nuts, tubers, fruit, and fish.\nGathering sites: 18 tiles.",
		"rows": [
			{"color": poor.lerp(rich, 5.0 / 195.0), "label": "Poorest forage", "value_text": "5 food"},
			{"color": poor.lerp(rich, 92.0 / 195.0), "label": "Average forage", "value_text": "92 food"},
			{"color": rich, "label": "Richest forage", "value_text": "195 food"},
			{"color": MAP_VIEW_SCRIPT.FORAGE_BARREN_COLOR, "label": "No forage", "value_text": "63 tiles"},
		],
		"stats": {"min": 5.0, "avg": 92.0, "max": 195.0},
	}

func _terrain_legend_fixture() -> Dictionary:
	return {
		"key": "terrain",
		"title": "Terrain Types",
		"description": "Biomes present on this map (5).",
		"rows": [
			{"color": Color("3a6f3a"), "label": "Prairie", "value_text": "412 tiles", "count": 412},
			{"color": Color("2a4a7a"), "label": "Deep Ocean", "value_text": "980 tiles", "count": 980},
			{"color": Color("c8b26a"), "label": "Desert", "value_text": "137 tiles", "count": 137},
			{"color": Color("2f5f2f"), "label": "Mixed Woodland", "value_text": "268 tiles", "count": 268},
			{"color": Color("8a8a8a"), "label": "Alpine", "value_text": "54 tiles", "count": 54},
		],
		"stats": {},
	}

# ---- the compose sheet's WIDTH invariant ------------------------------------

## **NO ROW OF AN OPEN COMPOSE SHEET MAY DEMAND MORE THAN THE CARD'S USABLE WIDTH.** The card is an
## `AutoSizingPanel`, i.e. a plain `Control`: a child's minimum width never reaches it, so a row wider
## than the card does not push the card open — it renders past the card's own rect, and whichever
## clip or edge it meets first cuts it off. That is what a screenshot showed and an assertion did not:
## the local-hunt sheet's three intent presets demanded 384px inside a card declared 340, and the
## widest rows (the 3-up preset grid, the full-width commit button) were the ones sliced.
##
## The check is a MEASUREMENT against the card's real width, not against the nominal `CARD_WIDTH` —
## the fix is that the card GROWS, so pinning the constant would fail every sheet that legitimately
## did. Usable = the card's fitted width less the panel stylebox's left+right margins and the scroll
## gutter, i.e. exactly the room `_fit_width` promised the rows.
##
## The HEADER row is measured with the body's: it sits outside the scroll and carries the subject's
## name, so a long species is content the card must be wide enough for too.
func _assert_compose_sheet_fits(state: String) -> void:
	var sheet: ComposeSheet = _hud._drawercompose._compose_sheet
	if sheet == null or not sheet.visible:
		_assert_hud("%s has an open compose sheet to measure" % state, false)
		return
	var style := HudStyle.card_stylebox()
	var gutter := sheet._scroll.get_v_scroll_bar().get_combined_minimum_size().x
	var usable: float = sheet._card.size.x - style.content_margin_left - style.content_margin_right \
		- gutter
	var rows: Array[Control] = [sheet._header_row]
	for child in sheet._body.get_children():
		if child is Control:
			rows.append(child as Control)
	var worst: float = 0.0
	var worst_row := ""
	for row in rows:
		var demand := row.get_combined_minimum_size().x
		if demand > worst:
			worst = demand
			worst_row = "%s %s" % [row.get_class(), _widest_control_face(row)]
	_assert_hud("%s: the widest compose row fits the card (%.0f demanded, %.0f usable of a %.0f card) — %s"
		% [state, worst, usable, sheet._card.size.x, worst_row],
		worst <= usable + COMPOSE_FIT_SLACK)

## A pixel of slack, so a row that lands exactly on the card's inner edge is not a failure. Anything
## that actually clips overruns by whole glyphs, never by a rounding remainder.
const COMPOSE_FIT_SLACK := 1.0

## The deepest descendant setting a row's minimum width, named by its face — so a failure says WHICH
## control is too wide rather than only that the row is.
func _widest_control_face(root: Control) -> String:
	var best: Control = root
	var stack: Array[Node] = [root]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		for child in node.get_children():
			stack.append(child)
			if child is Control and (child as Control).get_combined_minimum_size().x \
					> best.get_combined_minimum_size().x:
				best = child as Control
	var face := ""
	if best is Button:
		face = (best as Button).text
	elif best is Label:
		face = (best as Label).text
	elif best is RichTextLabel:
		face = (best as RichTextLabel).get_parsed_text()
	return "%s(%.0f) %s" % [best.get_class(), best.get_combined_minimum_size().x, face.substr(0, 40)]
