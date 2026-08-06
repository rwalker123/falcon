extends Node

## Dev-only preview harness for the dockable Band / City panel (slice 2 scaffold).
##
## Instances the real BandCityPanel alongside a real HudLayer, wires the panel's
## reservation onto the HUD (mirroring Main's `_apply_reservation` fan-out for the
## `hud` surface), then docks the panel to each edge (+ collapsed) and dumps one
## PNG per state so the chrome + the HUD reflow can be eyeballed without a server.
## The full MAP reflow/clip is only exercised in the running client.
##
##   godot --path . res://tools/band_panel_preview.tscn
##
## then read ui_preview_out/band_panel_*.png.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")

## The hang guard, a SIBLING node in `band_panel_preview.tscn` (`tools/preview_watchdog.gd`).
##
## **This harness does NOT have `ui_preview`'s chapter-loading defect** — it loads no chapters, so
## nothing here can leave a half-written frame set behind a broken sub-script. What it DOES share is
## the shape underneath it: the whole run is one long `await`ing `_ready()` whose last line is
## `get_tree().quit()`, so any runtime error aborts it without ever exiting, and any of the three
## scenes/scripts it `preload`s failing to compile takes THIS script's parse down with it — leaving
## the root node scriptless and the process idling forever with no FAIL and no status. The guard is
## the same node, for that shape only; this harness's own PASS/FAIL contract is untouched (it prints
## `ERROR: band_panel_preview` lines and still exits 0 on a red run — see
## `.claude/rules/client/test-harnesses.md`).
const WATCHDOG_NODE := "Watchdog"
const WATCHDOG_PROGRESS_METHOD := "note_progress"

## Scratch prefs file — never the player's `user://narrative.cfg`.
const PREVIEW_PREFS_PATH := "user://band_panel_preview_prefs.cfg"
## Scratch DOCK prefs — never the player's `user://band_city_dock.cfg`. Without this the harness both
## read the tab a previous run left selected (so the early frames rendered whichever zone that was,
## not the band zone they exist to show) and wrote its own tab walk back over the player's.
const PREVIEW_DOCK_PREFS_PATH := "user://band_panel_preview_dock.cfg"
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
## The real MapView, for the map-selection path state (see `band_panel_people_map_path`).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## **THE KIT ROSTER IS SHARED WITH `ui_preview`, and deliberately so.** It is world config the sim
## publishes once (`SubsistenceSection.kits`), not a per-harness prop: two copies could quote
## different tiers or a different job default, and the `kit <id>` command token asserted here is the
## same token that harness's frames are read against. This is the ONE cross-harness fixture preload.
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
## The hex `_band_fixture()` stands on — the tile the map-path state clicks.
const MAP_PATH_TILE := Vector2i(71, 18)
## A grid just large enough to hold MAP_PATH_TILE, and one flat terrain id to fill it with.
const MAP_PATH_GRID_W := 80
const MAP_PATH_GRID_H := 30
const MAP_PATH_TERRAIN_ID := 11
const OUT_DIR := "res://ui_preview_out"
# A left inspector strip width to prove co-edge stacking (bug 1).
const INSPECTOR_STRIP := 300.0
# The sim turn the arrival-schedule states render on, so the strip tooltips + the outlook "empty ~turn
# N" marker read as absolute turns rather than the pre-first-overlay relative form.
const ARRIVAL_PREVIEW_TURN := 40
# The paged-board states work a row of this many forage patches from this origin — far past one
# page in either shell, which is the whole point of the pager.
const MANY_SOURCE_COUNT := 34
const MANY_SOURCE_ORIGIN_X := 40
const MANY_SOURCE_ORIGIN_Y := 20
# Dependants per working-age adult in the big-band fixture, held near the base band's own shape
# (9 children + 5 elders to 16 workers) so its PEOPLE bar reads like a real band, not a scaled prop.
const MANY_SOURCE_CHILD_RATIO := 0.56
const MANY_SOURCE_ELDER_RATIO := 0.31
# Sub-pixel slack when comparing a zone's content rect against its host rect.
const ZONE_BOUNDS_TOLERANCE := 1.0
## The merged Food line's hay clause, as it reads AFTER the BBCode is stripped — the needle proving the
## SHORT tier really merged the two larders rather than dropping one. The word, not the number: the
## stock is a fixture value and this is a claim about the CLAUSE.
const MERGED_FOOD_HAY_NEEDLE := "hay"
## The standalone `Fodder:` row's key, which must be ABSENT wherever the merge fired. Matched bare —
## `DetailFormat._split_kv` drops the `": "` separator into two table cells, so the colon is never in
## the rendered text (the rule `_assert_trade_row_absent_in_short_tier` already records).
const FODDER_ROW_NEEDLE := "Fodder"
## The `RichTextLabel` theme keys the vitals width measurement reads its OWN font/size/gutter from —
## never a hardcoded face, since the measurement is only honest in the font the label actually draws.
const VITALS_FONT_THEME_KEY := "normal_font"
const VITALS_FONT_SIZE_THEME_KEY := "normal_font_size"
const VITALS_TABLE_SEPARATION_THEME_KEY := "table_h_separation"
## Offset applied to a fixture cohort's `entity` to derive its `band_id` — see `_push_bands`.
const FIXTURE_BAND_ID_OFFSET := 4000
## One Wild Boar's worth of yield in provisions (`HerdTelemetryState.foodPerAnimal`) — the quarry
## fixture's delivered food is animals × this, so the sheet's forecast quotes a real food total.
const QUARRY_FOOD_PER_ANIMAL := 4.0
## One animal's worth of TRADE GOODS (issue #337) — a hunt pays a vector, so a raid cell carries this
## payload beside its food one. Small against the food quantum: an edible quarry is meat first.
const QUARRY_TRADE_PER_ANIMAL := 0.5
## The INEDIBLE quarry on the work board (issue #337): its hunt row pays trade goods and no food.
const TRADE_ONLY_HERD_ID := "game_wolf_03"

# ---- THE COMBAT GATE's two herd terms on the quarry (`docs/plan_hunt_through_combat.md` §4.2) -----
## `defense` is whether a hit counts at all — deliberately ABOVE the roster's bare-handed `attack`
## (1.0) and far below the big-game kit's (20.0), so the gate's verdict FLIPS with the kit and the
## kit-mismatch frame's line is a discriminator rather than a decoration.
const QUARRY_DEFENSE := 2.0
## …and `durability` is how many counting hits it takes. A round number well above the effective
## attack, so the effort figure reads as a real several-hunter-turns rather than a rounding.
const QUARRY_DURABILITY := 60.0

# ---- THE DENIAL RAID's fixture (`docs/plan_denial_raid.md`) --------------------------------------
## The party the two denial frames compose, i.e. which row of the table below they render. It is the
## reference band's whole IDLE workforce, because that is the only ceiling this form has: the denial
## sheet deliberately carries NO max-useful cap (a raid has no payload to plateau), so it renders a
## party the HUNT sheet beside it would have clamped to the boar's raid plateau of 2 — which is the
## rendered difference between the two forms. Set it above `_band_fixture`'s idle count and the
## stepper clamps, leaving every assertion below answering for a row the frame never shows.
const DENIAL_PARTY := 3
## The VIABLE table's rows for parties 1..8. **More hands break the herd sooner** — the mission's only
## lever — so the counts fall monotonically; the band widens where the retreat is chanciest. Party 4
## reads `3–5`, the plan's own worked example.
const DENIAL_TURNS_ROW := [11, 8, 6, 4, 4, 3, 3, 2]
const DENIAL_LOW_ROW := [9, 6, 5, 3, 3, 2, 2, 2]
const DENIAL_HIGH_ROW := [14, 10, 8, 5, 5, 4, 4, 3]
const DENIAL_KILLS_ROW := [26, 42, 55, 66, 74, 82, 88, 94]
## The REPELLED table's kills — non-zero, and that is the claim. A repelled party is not one that
## kills nothing; it is one whose kills do not outpace the herd's regrowth, so the take readout must
## still have something to state while the verdict says the herd is never pushed past recovery. Its
## turn rows are all `0`, the wire's "not within the horizon on that end".
const DENIAL_REPELLED_KILLS_ROW := [3, 5, 7, 9, 10, 11, 12, 13]
## **THE BAND WHOSE IDLE WORKFORCE OUTRUNS `max_expedition_party_size`** (8, on `_band_fixture`). That
## field is the wire echo of the sim's estimate-table SAMPLING AXIS, not a rules cap, so the denial
## stepper's ceiling is the band's own idle workers — and this count is the only shape in which a
## ceiling read off the wrong field is visible at all. Deliberately ABOVE `DENIAL_DEEP_PARTY_NEEDED`,
## so the seed lands unclamped and the cap has somewhere further to go.
const DENIAL_DEEP_PARTY_IDLE := 12
## The party the sim quotes for that quarry (`denialPartyNeeded`): the smallest one whose kills outpace
## the herd's regrowth. **Above 8**, which is the case the whole frame exists for — a requirement one
## rung past the sampling axis, which the old stepper could not even be dialled to.
const DENIAL_DEEP_PARTY_NEEDED := 11
## …and the party the second frame steps BACK to, below that requirement, so its row is `repelled` and
## the refusal beneath it has a count to name.
const DENIAL_DEEP_PARTY_SHORT := 4
## **THE REPORTED SHAPE — a bounded expectation, a bounded good run, and a BAD run that never
## finishes.** `high == 0` is the wire's "not within the horizon on that end", and no other denial
## fixture in this file stages it: every table above bounds all three, so the frames could not show
## what the old rule did here — it dropped the expectation entirely and quoted the LUCKY end alone,
## beside a take line priced at the expectation. The spread between the two is deliberately wide,
## because a low sitting near the expectation would render a defensible-looking sentence either way.
const DENIAL_OPEN_HIGH_TURNS := 47
const DENIAL_OPEN_HIGH_LOW := 12
## The party this frame composes. Inside the reference band's idle workforce, so the stepper renders
## it unclamped and the frame is judged on the sentence rather than on a cap.
const DENIAL_OPEN_HIGH_PARTY := 2
## Whole animals ONE raider of that party kills over the raid. A repelled party is not one that kills
## nothing — it is one the herd outbreeds — so the sub-requirement rows carry a real take.
const DENIAL_DEEP_KILLS_PER_WORKER := 3
## The collapse band quoted for a party at or above the requirement. One row of the table is ever
## rendered, so a flat band states everything the frame needs and nothing it does not.
const DENIAL_DEEP_TURNS := 6
const DENIAL_DEEP_TURNS_LOW := 5
const DENIAL_DEEP_TURNS_HIGH := 8
## Food ONE raider hauls home over the whole raid — tiny beside the kill, which IS the mission. A
## fixture that hauled its whole kill would be a hunting raid wearing a denial outcome, and the waste
## readout would have nothing to state.
const DENIAL_CARRY_PER_WORKER := 2.0
## The quarry fixtures straddle the band's hunt reach: the Wild Boar is a party's job, the Roe Deer
## one tile out is a local hunt the picker must refuse.
const QUARRY_BAND_HUNT_REACH := 2
const QUARRY_FAR_HERD_ID := "game_boar_04"
const QUARRY_FAR_X := 75
const QUARRY_FAR_Y := 18
const QUARRY_NEAR_HERD_ID := "game_deer_79"
const QUARRY_NEAR_X := 72
const QUARRY_NEAR_Y := 18
## **A HERD ON THE BAND'S OWN TILE** — the extreme of "within hunt reach", and the case a DENIAL raid
## must still be allowed to name (reported from play: the warren beside camp could not be broken,
## because the quarry rule was the hunt's). It stands at the band fixture's own coordinates, so its
## outbound walk is exactly ZERO turns — which is also the only geometry that exercises the verdict's
## no-travel-split branch, a herd even one tile out costing a turn.
const QUARRY_HOME_HERD_ID := "game_rabbit_18"
const QUARRY_HOME_SPECIES := "Rabbit Warren"
const QUARRY_HOME_X := 71
const QUARRY_HOME_Y := 18
## Stated rather than re-derived, like `DENIAL_OUTBOUND_TRAVEL_TURNS`: the band and the warren share a
## tile, so the odd-r distance is 0 and `ceil(0 / move_rate)` is 0 whatever the move rate.
const QUARRY_HOME_OUTBOUND_TRAVEL_TURNS := 0
## **TWO HERDS ON ONE HEX** — the reported pair. A tile can hold more than one herd and a map click
## names only the TILE, so the pick resolves to whichever the snapshot lists first and re-clicking
## resolves to the same one; the Quarry row's chooser is the way to the other. The pair is
## deliberately a food quarry beside an INEDIBLE one: they differ in art, in name and in what the
## raid brings home, so a chooser that offered one herd twice could not pass. Same row as the band
## (71, 18) and seven columns out, i.e. far beyond `QUARRY_BAND_HUNT_REACH`.
const SHARED_TILE_X := 78
const SHARED_TILE_Y := 18
const SHARED_TILE_FOOD_HERD_ID := "game_rabbit_11"
const SHARED_TILE_FOOD_SPECIES := "Rabbit Warren"
const SHARED_TILE_PELT_HERD_ID := "game_wolf_11"
const SHARED_TILE_PELT_SPECIES := "Wolf Pack"
## The fill target staged before the chooser is driven, so the switch can be seen to DROP it. Any
## positive value does; it is a count of the OLD quarry's animals, which is the whole point.
const SHARED_TILE_STALE_FILL_TARGET := 5
## The shared hex's raid table: whole animals taken per party size 1..8, and the turns it takes. Flat
## in the turns because nothing on this frame is judged on trip LENGTH — the claim is the chooser.
const SHARED_TILE_RAID_ANIMALS_ROW := [4, 7, 9, 10, 10, 10, 10, 10]
const SHARED_TILE_RAID_TURNS := 6
## The two species' per-animal quanta. A rabbit is small and pays a little of both; a wolf pays pelts
## alone, so it carries a TRADE quantum and no food one at all.
const SHARED_TILE_FOOD_PER_ANIMAL := 1.5
const SHARED_TILE_FOOD_TRADE_PER_ANIMAL := 0.2
const SHARED_TILE_PELT_TRADE_PER_ANIMAL := 0.9
## **THE WALK OUT TO THE FAR QUARRY, stated from the fixture's own geometry.** The band stands at
## (71, 18) and the boar at (75, 18) — the same row, so the odd-r hex distance is the bare column
## delta, 4 — and `_band_fixture` moves 2 tiles a turn, so the party arrives on turn `ceil(4 / 2)` = 2.
## The denial verdict adds it to both ends of the collapse band, because the sim's table counts only
## the turns spent working the herd. Written out rather than asked of `outbound_travel_turns`: an
## expectation re-derived through the code under test asserts nothing.
const DENIAL_OUTBOUND_TRAVEL_TURNS := 2
# The two disclosure keys of `_band_fixture()` (entity 904) — the `[url]` meta payload its Food /
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for that band.
const BAND_FIXTURE_DISCLOSURE_FOOD := "food:904"
const BAND_FIXTURE_DISCLOSURE_MORALE := "morale:904"
const BAND_FIXTURE_DISCLOSURE_TRADE := "trade:904"

## The work-inspector policy-picker states work TWO Hunt rows on one band. They used to be told apart
## by the RUNG they stood on — one on `corral`, which the four-rung picker could not highlight at all.
## **Since issue #442 there is no such row**: `policy` is always a stance, so both rows light a rung
## and the picker behaves identically on each. What the pair now proves is the other half of that
## split — a row that IS building something (`improvement: "corral"`) still lights its STANCE and a
## pick still commits immediately, because a stance re-pick no longer touches the build at all.
const INVESTMENT_ROW_FLOOR := SourceForecast.FLOOR_FOOD_PEAK
const INVESTMENT_ROW_PRESET := SourceForecast.FLOOR_PRESET_PEAK
const INVESTMENT_ROW_IMPROVEMENT := "corral"
const INVESTMENT_ROW_HERD_ID := "game_aurochs_11"
## The crew that mid-build pen owes. Set through `_set_managed_herders`, so BOTH herder counts carry it.
const INVESTMENT_ROW_HERDERS_NEEDED := 3
const EXTRACTIVE_ROW_FLOOR := SourceForecast.FLOOR_FOOD_PEAK
const EXTRACTIVE_ROW_PRESET := SourceForecast.FLOOR_PRESET_PEAK
const EXTRACTIVE_ROW_HERD_ID := "game_deer_07"
## The rung both assertions PRESS. Extractive, so on the investment row it is a genuine "discard the
## pen and take at Surplus instead", and on the control row an ordinary change of take.
const PICKED_RUNG_PRESET := SourceForecast.FLOOR_PRESET_STRIP

## The under-contained managed herd (fauna neglect-escape arc): a Corralled herd that needs 4 herders
## but is staffed with only 2, so it sheds animals — the work-board ⚠ / drifting-off note case.
const UNDER_HERDED_WORK_HERD_ID := "game_aurochs_uh"
## The crew that pen owes — the SAME number as the row's `workers_needed`, which is where the shed
## comes from (staffed 2 < needed 4), so the two read from one const rather than two loose literals.
const UNDER_HERDED_WORK_HERDERS_NEEDED := 4

## THE HERDER-FLOOR ROW (`band_panel_work_herder_floor`) — a MANAGED herd whose crew requirement is
## LARGER than what its take saturates, which is the only shape that can expose the bug: the row flags
## the herd under-herded and, without the floor, disables the very `+` that would staff the 3rd herder.
## The numbers are the playtest's Wild Fowl. `ceil(0.09 take ÷ 0.05 per worker) = 2` is the take-side
## max-useful; the crew is 3; the row is staffed at 2 with idle workers free, so the `+` is gated by
## the source and by nothing else. `food_per_animal` is deliberately ABSENT — a whole-animal quantum
## would re-derive the cap through the carry model and the frame would stop testing the floor.
const HERDER_FLOOR_HERD_ID := "game_fowl_hf"
const HERDER_FLOOR_HERDERS_NEEDED := 3
const HERDER_FLOOR_STAFFED := 2
const HERDER_FLOOR_PER_WORKER := 0.05
const HERDER_FLOOR_SUSTAIN_CEILING := 0.09
## What `max_useful_workers` answers for that pair, and what the cap would be WITHOUT the floor —
## named because both cap twins are asserted against it and against the crew that must outrank it.
const HERDER_FLOOR_TAKE_USEFUL := 2

## THE SOURCE-RUNG BOARD — one row per rung of both ladders, on ONE band, so the marks are judged
## against each other rather than one frame at a time. Wild carries NO mark (that is the design), so
## it is on the board as the control: without it the frame cannot show that absence reads as wild
## rather than as a missing glyph.
##   plants:  (70,20) wild · (71,20) 🌾 Tended Patch · (72,20) ▦ Field
##   animals: `game_boar_rp` ◎ pastoral (tamed, unpenned) · `game_aurochs_rp` 🐄 penned
## The two herds are the pair `DetailFormat` alone CANNOT tell apart — `husbandry_label` and
## `corral_label` both wear 🐄 — so a pastoral row that reads 🐄 here is the exact defect the mark
## exists to prevent.
const RUNG_WILD_TILE := Vector2i(70, 20)
const RUNG_TENDED_TILE := Vector2i(71, 20)
const RUNG_FIELD_TILE := Vector2i(72, 20)
## The committed crop each prepared patch carries — it rides the rung mark's TOOLTIP, which is the
## only place the board has room to name it.
const RUNG_TENDED_CROP := "Wild Emmer"
const RUNG_FIELD_CROP := "Einkorn"
const RUNG_PASTORAL_HERD_ID := "game_boar_rp"
const RUNG_PENNED_HERD_ID := "game_aurochs_rp"
## The penned herd's crew, staffed in full — this frame is about the RUNG, so it must not also trip
## the under-herded ⚠ and leave two explanations for one amber row.
const RUNG_PENNED_HERDERS := 2
## Every Nth many-source patch carries a rung, so the paged/threshold frames show rung marks mixed
## among wild rows at real board density. Coprime with each other and with the 3 the overstaffed
## rows cycle on, so no row lands on two conditions in lockstep.
const RUNG_MANY_TENDED_STRIDE := 4
const RUNG_MANY_FIELD_STRIDE := 7

# The two hunt-party fixtures the parties-inspector states open (entities from the fixtures below).
const HUNT_DELIVERING_ENTITY := 952
const HUNT_LEAN_ENTITY := 953
# A hunt party whose target herd has DROPPED OUT of `_world_herds` (lost/replaced), projecting 0.
const HUNT_LOST_ENTITY := 954
# A 21:9 monitor — comfortably past the wide shell's content cap, which is the whole point of the state.
const ULTRAWIDE_WIDTH := 3440
const ULTRAWIDE_HEIGHT := 900
# The two shell-threshold probe windows. The panel is bottom-docked in both, so the window width IS
# `_panel_extent().x`, the value `_shell_is_wide` tests — one pixel below the derived threshold (must
# pick the NARROW tabbed shell) and exactly at it (the narrowest legitimate WIDE shell). Derived from
# the panel's own const so they can never drift from the threshold they bracket.
const SHELL_THRESHOLD_UNDERSHOOT := 1
const SHELL_THRESHOLD_HEIGHT := 900
## The canvas the DOCK-ROW states render at (issue #324). 1080p with a bottom dock is the case the
## issue is about, and the canvas — not just the window — has to be pinned: `project.godot` stretches
## `canvas_items`, so a bare window pin renders at the 1920 base width whatever the window says.
const DOCKROW_CANVAS := Vector2i(1920, 1080)
## The map the dock-row states seed their minimap from — the DEFAULT size, resolved through the same
## registry the New Game pane and the inspector's Map tab use. The rail width the reflow declares is a
## function of the minimap's grid ASPECT (`MinimapPanel.resize_to_aspect`: `embedded_height × aspect`,
## clamped into the config's `[min_width, max_width]`), so it has to come from here and never from a
## literal — otherwise the frames render a nav cluster the game never has.
const DOCKROW_MAP := MapSizes.DEFAULT_KEY
## Flat fill for that stand-in minimap raster. `MinimapController._rebuild_image` paints one pixel per
## HEX from live terrain + fog, which needs a whole MapView snapshot; this harness only needs the
## thumbnail's SIZE to be honest, so it substitutes a flat 1px-per-hex image at the real grid
## dimensions. The aspect — the only thing that drives the rail width — is therefore the real one.
const DOCKROW_MINIMAP_FILL := Color(0.16, 0.24, 0.20, 1.0)
# The window every state but the ultrawide one renders at.
const PREVIEW_SIZE := Vector2i(1500, 900)
# How many frames to keep re-asserting the window before giving up and warning. Also the bound on
# `_capture`'s geometry retry, so a WM that refuses to honour the pin fails loudly instead of hanging.
const WINDOW_PIN_MAX_FRAMES := 30
## How many CONSECUTIVE frames the window must hold `PREVIEW_SIZE` in `_stabilize_canvas` before the
## first state renders, and the bound on how long it waits for that. The maximize is applied — and
## RE-applied — asynchronously, so "it is the right size once" is not the same as "it stays".
const CANVAS_STABLE_FRAMES := 30
const CANVAS_STABLE_MAX_FRAMES := 600
## Phase to seed the turn orb's calm breath at, as a fraction of `TurnOrb.PULSE_PERIOD`. The breath is
## `0.5 - 0.5 * cos(t)`, which is ZERO — its faintest, smallest instant — at phase 0, so freezing the
## clock there would render the pulse at the bottom of its range. A quarter period puts `cos` at 0,
## i.e. the breath's MIDPOINT, which is what an unfrozen frame averaged.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.25

## The size every state re-asserts before it renders — see `_pin_window`.
var _pinned_size := PREVIEW_SIZE
## The canvas size every state re-asserts, `ZERO` = leave the project's stretch alone — see `_pin_canvas`.
var _pinned_canvas := Vector2i.ZERO
var _hud: HudLayer
var _panel: BandCityPanel
## The hang guard from the scene, or `null` if it has gone — a safety net, never a dependency.
var _watchdog: Node = null
## The last state `_save`d, so an assertion failure names the frame it fired on.
var _current_state := "<pre-render>"
## Set by `_unhandled_input` below — this harness's stand-in for `MapView`'s hex picking, which is also
## an `_unhandled_input` handler. See `_assert_open_strip_reaches_the_map`.
var _unhandled_press_seen := false

## The probe MapView's hex picking stands in for: a press that survives the GUI pass and reaches
## unhandled input is a press that would have selected the hex under the pointer.
func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		_unhandled_press_seen = true


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
const FIXTURE_COLLAPSE_FRACTION := 0.15
const FIXTURE_COLLAPSE_RATE := 0.20
const FIXTURE_RESEED_FLOOR_FRACTION := 0.02
# `per_worker_biomass_capacity` for each web, used only where the fixture's own rates cannot state the
# throughput (a source that pays no food — the exact case the wire field was added for).
const FIXTURE_PLANT_PER_WORKER_BIOMASS := 8.0
const FIXTURE_ANIMAL_PER_WORKER_BIOMASS := 40.0

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

## Seed `per_worker_biomass` + `regrowth_samples` on a fixture that predates them. Both are skipped
## when the fixture states its own, so a state authored to exercise a particular curve keeps it.
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
func _floorify_estimates(src: Dictionary) -> Dictionary:
	var estimates: Variant = src.get("hunt_trip_estimates", null)
	if not (estimates is Dictionary):
		return src
	var rekeyed := {}
	for key in (estimates as Dictionary):
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


# A floor BELOW the food peak, for the frames that need "this crew is drawing the source down" — the
# `deplete`/`surplus` stances these fixtures were written against. It is one of the sim's own raid
# samples, so a converted raid table lands on a real row rather than an interpolated one.
const DEEP_DRAW_FLOOR := 0.15

func _ready() -> void:
	_watchdog = _resolve_watchdog()
	# FREEZE ANIMATION TIME — the treatment `ui_preview`, `map_preview` and `blend_probe` all carry, and
	# taken for the same reason: a frame that varies run-to-run cannot be pixel-diffed to prove a panel
	# refactor changed nothing. Measured before the freeze, two runs of IDENTICAL code differed byte-wise
	# in `band_panel_no_idle` — 51 px inside the turn orb's 71×70 ring box, the calm breath.
	#
	# What survives phase 0 was CHECKED against the draw code, not assumed:
	#   • the turn orb's breath is `0.5 - 0.5 * cos(t)`, which DEGENERATES to its faintest, smallest
	#     instant at phase 0, so its phase is seeded to the midpoint below rather than left at 0. It is
	#     drawn only while the orb has no attention entries (`_draw_pulse` vs `_draw_badge`), which is
	#     why just one frame moved;
	#   • MapView's awaiting-expedition / targeting pulses are not in any frame — both MapViews this
	#     harness builds are `visible = false`, data only;
	#   • the ONE tween in the whole client is `TellingPanel`'s page turn, and this harness pushes no
	#     narrative beats, so no tween is ever created here. `ui_preview` has to flush tweens in its
	#     settle; there is deliberately nothing to flush here. RE-CHECK THAT if a state ever drives the
	#     Telling panel: a Tween at `time_scale = 0` never advances AT ALL, so it would pin at its
	#     starting frame rather than merely render at a fixed phase.
	# `Hud._process` only hides a tooltip and `MapView._process` is input-driven, so neither carries a
	# time term; `Main` / `LogsPanel` / `ScriptHostManager` are not instanced. `_settle` waits on
	# `process_frame`, which still fires at `time_scale` 0.
	Engine.time_scale = 0.0
	# PIN THE WINDOW. `project.godot` opens MAXIMIZED and macOS applies — and re-applies — that
	# asynchronously, so a bare `size =` is a race the harness does not stay winning: every frame then
	# renders at monitor size instead of PREVIEW_SIZE, silently changing what each state proves (a
	# 3440-wide "bottom dock" frame is testing the ultrawide cap, not the ordinary wide shell). Same
	# hazard `blend_probe._pin_canvas` exists for.
	await _pin_window(PREVIEW_SIZE)
	DirAccess.make_dir_absolute(OUT_DIR)

	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	# IT STANDS IN FOR THE MAP, AND THE MAP CONSUMES NOTHING. A `ColorRect` is a `Control`, so at the
	# default `STOP` this backdrop swallowed every press that was not over the panel — which made the
	# click-through claim `_assert_open_strip_reaches_the_map` exists for unaskable here (the harness's
	# own decoration would have failed it whatever the panel did). In the live client the map is a
	# `Node2D` picking hexes out of `_unhandled_input`, so `IGNORE` is what makes the backdrop honest.
	# The same fix `ui_preview`'s backdrop needed for the event dock's overlay probe.
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bg_layer.add_child(bg)

	# Isolate the narrative/HUD-panel preferences from the player's real profile before the HUD
	# reads them — otherwise a developer who has pressed `L` renders different frames than one who
	# has not. Same rule as ui_preview; see its prefs-isolation block.
	NarrativeForkPanel.config_path_override = PREVIEW_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_PREFS_PATH))

	BandCityPanel.config_path_override = PREVIEW_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_DOCK_PREFS_PATH))

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)

	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	# Fan the panel's reservation onto the HUD as Main does — INCLUDING its TOP-dock exemption and the
	# lateral bounds that go with it (issue #377), or these frames would show the HUD yielding a strip the
	# live client does not, and a card free to sit where the live one is bounded. `Main` is not instanced
	# here, so the two rules are restated; `Main._reserver_overlays_hud` /
	# `Main._update_band_panel_lateral_bounds` are the authority.
	_panel.reservation_changed.connect(func(edge: int, size: float):
		var hud_yields: bool = edge != SIDE_TOP
		if _hud.has_method("set_reserved_inset"):
			_hud.set_reserved_inset(&"band_panel", edge, size if hud_yields else 0.0)
		var columns: Vector2 = Vector2.ZERO if hud_yields else _hud.lateral_column_widths()
		_panel.set_lateral_bounds(columns.x, columns.y))

	await get_tree().process_frame
	await get_tree().process_frame
	# Hold the canvas until the WM stops fighting it — before the first state, so no LATER settle has
	# to spend a frame on it. See `_stabilize_canvas`.
	await _stabilize_canvas()

	# Seed the turn orb's calm breath at its MIDPOINT. `_pulse_time` only ever advances by `delta`,
	# which is 0 with the clock frozen, so whatever is set here is the phase every frame renders at —
	# and phase 0 is the breath's trough (alpha 0.30 / radius 44 of a 0.30..0.85 / 44..47 range), i.e.
	# a deterministic frame whose subject has faded to its faintest. Set once; nothing resets it.
	_hud.turn_orb._pulse_time = TurnOrb.PULSE_PERIOD * TURN_ORB_PULSE_MIDPOINT_FRACTION

	# Seed the top bar so the HUD reflow reads against real content.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# Slice 3: inject the panel into the HUD and push a player band through the real snapshot
	# path (update_band_alerts → _refresh_panel_band), so the FULL band detail relocates into the
	# panel — summary lines + labor allocation + the settlement stage header/cycler.
	# Push the band PLUS two detached expeditions (home_band_entity = the band's entity): the cycler
	# must read 1/1 (expeditions excluded), and the panel's "Active expeditions" section must list
	# both. Order interleaves an expedition first to prove the split (not just "first cohort = band").
	_hud.set_band_city_panel(_panel)
	# THE DOCK-ROW REFLOW WIRING (issue #324), exactly as `Main._connect_band_city_panel` does it: a
	# SECOND listener on `reservation_changed` plus a one-shot seed. This harness does not instance
	# `Main`, so without it the reflow would only ever be exercised by poking the controller — and the
	# `band_panel_dockrow_*` states below are meant to drive the real path.
	if _hud.has_method("reflow_dock_row"):
		_panel.reservation_changed.connect(Callable(_hud, "reflow_dock_row"))
		_hud.reflow_dock_row(_panel.get_dock(), _panel.current_reservation_size())
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row names the herd
	# from here and, on click, jumps to its LIVE tile — the herd has MIGRATED away from the
	# assignment's launch target (70, 17) to (68, 15), which is exactly what the row must resolve.
	_set_world_herds(_herd_fixtures())
	# The world's food modules (Main pushes snapshot["food_modules"]): the Forage row leads with the
	# module's map glyph (savanna grassland → 🌾 on (71, 18)).
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])
	# The world's KIT ROSTER (Main pushes snapshot["kits"] + the two job defaults): the compose
	# sheets' Kit picker is built from it. World setup rather than per-state, exactly as the herds and
	# food modules above are — a roster seeded per frame would give one sheet a picker and the next none.
	_hud.update_kit_roster(BandFx.kit_roster_fixture(),
		BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE)
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	print("band_panel_preview: cycler split — player_bands=%d (expect 1), player_expeditions=%d (expect 2)" % [
		_hud._band_labor._player_bands.size(), _hud._band_labor._player_expeditions.size()])

	# Dock to each edge and render.
	_panel.set_collapsed(false)
	for state in [
		{"edge": SIDE_LEFT, "name": "band_panel_left"},
		{"edge": SIDE_RIGHT, "name": "band_panel_right"},
		{"edge": SIDE_TOP, "name": "band_panel_top"},
		{"edge": SIDE_BOTTOM, "name": "band_panel_bottom"},
	]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])

	# Collapsed rail (docked left).
	_panel.set_dock(SIDE_LEFT)
	_panel.set_collapsed(true)
	await _settle()
	await _save("band_panel_collapsed")
	_panel.set_collapsed(false)


	# Bug 1 — co-edge stacking with the Inspector. Reserve a left inspector strip (as Main does)
	# and push the band panel's matching leading offset, docked left: the panel must render to the
	# RIGHT of the strip (no overlap at x=0). The strip region is left empty here (no inspector in
	# this harness) — what matters is the panel starts at INSPECTOR_STRIP, not the screen edge.
	_panel.set_dock(SIDE_LEFT)
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, INSPECTOR_STRIP)
	_panel.set_edge_offset(INSPECTOR_STRIP)
	await _settle()
	await _save("band_panel_stacked_left")
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	_panel.set_edge_offset(0.0)

	# Bug 2 — panel stays populated on a stepper edit while a FOREIGN hex is selected. Selecting a
	# tile calls `_selected_unit.clear()`; `_panel_band` must NOT alias it. Then drive a worker
	# assign on the panel band (the worker-stepper path → `_after_pending_change`): the panel must
	# stay populated (never blank) and show the optimistic "· pending".
	_hud.show_tile_selection({"x": 5, "y": 5, "terrain_label": "Prairie Steppe", "visibility_state": "active"})
	print("band_panel_preview: bug2 — _panel_band empty after foreign select? ", _hud._band_labor._panel_band.is_empty())
	_hud._emit_assign_labor(_hud._band_labor._panel_band, "forage", 6, 71, 18, "",
		SourceForecast.DEFAULT_HARVEST_FLOOR)
	await _settle()
	await _save("band_panel_stepper_foreign")

	# Food + Morale summary-line disclosures, in BOTH dock layouts (tall LEFT / wide TOP). The
	# breakdown opens in a POPOVER, never inline — so these frames prove two things at once: the
	# popover renders its rows, and the band zone behind it is UNCHANGED (WORKFORCE + both role cards
	# still whole). Driven through the REAL path: `meta_clicked` on the live vitals label, i.e. the
	# exact signal a click emits and the exact handler it runs — a debug back door could pass here
	# while the live path was broken.
	# (a) Food breakdown (Gathered/Hunted/Eaten).
	_push_bands([_band_fixture()])
	_panel.set_active_tab(&"band")   # the narrow shell shows ONE zone; these frames judge the band one
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_food_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_food_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_FOOD)
		await _settle()
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_FOOD)   # toggle shut before the next dock

	# (b) Morale breakdown (same disclosure mechanism, same popover, indented contributions).
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_morale_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_morale_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_MORALE)
		await _settle()
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_MORALE)

	# (b2) THE TRADE ROW (issue #381) — what THIS band earns per turn in the second product. The row is
	# **purely band-scoped**: it carries a rate and no stock, because the only trade-goods stock the sim
	# publishes is faction-global and every band would print the same total. So the states below pin the
	# rate's two ends plus the tier gate — there is no stock axis left to vary.
	#
	# (i) EARNING — the fixture's forage patch pays ⇄ 0.04 through the `realized == 0` fallback and its
	# deer pays ⇄ 0.04 outright, so the headline reads +0.08 over a TWO-row breakdown. Disclosure OPEN,
	# because **the Gathered row is the regression guard**: reading `realized_trade_yield` alone drops
	# the forage half, which is exactly how a cash-crop band came to read `+0.00` in playtest.
	# LEFT dock only; see (iii) for why the row is not in a T/B frame.
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_TRADE)
	await _settle()
	await _save("band_panel_trade_expanded_left")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_forage_trade_counted()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_TRADE)

	# (ii) ZERO — a band working no trade-paying source. **The row is STILL THERE**, reading `+0.00 /turn`
	# in neutral ink with no caret, and that is the whole point of the state: a row that vanished at zero
	# read in playtest as "this band cannot trade at all" rather than "it earns none right now". The caret
	# is absent because `register` declines an empty payload — an income-only breakdown has no rows when
	# there is no income — so a zero row is honestly inert rather than opening an empty popover.
	_push_bands([_no_trade_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_trade_zero")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_trade_row_reads_zero()

	# (iii) THE SHORT-TIER DROP. The T/B dock's band zone is ~300px and CLIPS what it cannot hold, so the
	# Trade row is gated off there exactly as the food-outlook chart is — measured at 26px, against a zone
	# with nothing to spare. The SAME earning band as (i), in a TOP dock, must render Food/Morale/Growth
	# and NO Trade row. **Asserted, not just eyeballed**, because an absent row and a row clipped off the
	# bottom of a `clip_contents` zone are the same picture.
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_trade_short_tier")
	_assert_zones_within_bounds()
	_assert_trade_row_absent_in_short_tier()

	# (iv) THE WORST CASE — every optional vitals row a band can carry AT ONCE, in the height-capped
	# TOP dock. Nothing in this harness had ever rendered one: each optional row had its own frame and
	# each of those fixtures was otherwise ordinary, so the zone was never asked to hold all of them
	# together — which is exactly how a band with the full set came to overflow a box that CLIPS.
	# The fixture carries a hay larder AND a pen feed bill, productivity below full, a fertility
	# reading, a trade stock and rate, and the projected arrivals the FOOD OUTLOOK chart needs, so
	# every gate in `build_band_zone` / `unit_summary_lines` is live at once.
	_push_bands([_vitals_worst_case_band_fixture()])
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _settle()   # let the deferred fit_content re-pack settle before capture
	await _save("band_panel_vitals_worst_case")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_report_zone_content_extent("band_panel_vitals_worst_case")
	_assert_merged_food_row_fits()
	# The SHORT tier's SECOND merge, and the one the `Kit` row is paid for with. Measured in the same
	# frame as the Food merge because this is the frame that carries every optional row at once — the
	# only state in which the zone is asked to hold the full set.
	_assert_merged_morale_growth_fits()


	# (c) CONCERNING food (net negative + low runway): the breakdown AUTO-shows (no click) under a red net.
	_push_bands([_concerning_food_band_fixture()])
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_food_concerning_left"},
			{"edge": SIDE_TOP, "name": "band_panel_food_concerning_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])

	# ROW STATUS GLYPHS — the vocabulary frame. One band whose Current actions carry a CONFIRMED
	# forage row (● working, overstaffed → "· only 2 of 5 working") + a CONFIRMED hunt row (● working,
	# overdrawing → ⚠), plus a PENDING forage row on a DIFFERENT tile (◌, amber) so pending and working
	# read side by side and the ⚠/overstaffing notes prove they still compose. Active expeditions cover
	# every phase glyph: outbound ➤ / hunting ● / delivering ◄ / returning ◄ / awaiting ▮▮ + words.
	_hud.show_tile_selection({})   # clear the foreign selection so the panel band is the subject
	# Drop the earlier bug-2 pending assign (it targets the same tile as the confirmed forage row and
	# would mask it) so this frame shows a CONFIRMED row and a PENDING row side by side.
	_hud._band_labor._pending_labor.clear()
	_push_bands([_band_fixture()] + _phase_expedition_fixtures())
	_hud._emit_assign_labor(_hud._band_labor._panel_band, "forage", 4, 72, 19, "", DEEP_DRAW_FLOOR)
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_status_glyphs")

	# Fit-to-content height (no clipping) — push a TALLER band: starving + full morale breakdown +
	# output row + the send-expedition section, so the summary column is much taller than the old fixed
	# T/B PANEL_HEIGHT would allow. Dock top/bottom and confirm every column's bottom row is visible and
	# the reserved strip grew to fit (map/HUD reflow is fanned onto the HUD as usual).
	_hud.show_tile_selection({})   # clear the foreign selection so the panel band is the subject again
	_push_bands([_starving_band_fixture(), _scout_expedition_fixture(), _hunt_expedition_fixture()])
	for state in [
		{"edge": SIDE_TOP, "name": "band_panel_top_tall"},
		{"edge": SIDE_BOTTOM, "name": "band_panel_bottom_tall"},
	]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _settle()   # extra frame: let the deferred fit_content re-pack + reservation settle
		await _save(state["name"])
		_report_zone_content_extent(String(state["name"]))

	# PER-SOURCE MAX-USEFUL CAP on the Current-actions rows. Push a band with idle workers to spare and
	# three staffed sources: a Forage row staffed AT its patch's max-useful (3), a Forage row BELOW its
	# patch's max-useful (1 of 5), and a Hunt row staffed AT its herd's max-useful (2). With idle still
	# available the two AT-cap rows' `+` must be DISABLED (capped per source), the below-cap row's `+`
	# ENABLED, and Scout's `+` still tracks idle. The forecast fields ride the pushed herds/patches.
	_hud.show_tile_selection({})
	_hud._band_labor._pending_labor.clear()
	_set_world_herds(_cap_demo_herd_fixtures())
	_set_forage_patches(_cap_demo_patch_fixtures())
	_push_bands([_cap_demo_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_source_cap")

	# ARRIVAL SCHEDULE — the per-source tick strip + the merged Food-outlook chart. Seed a current turn
	# so the strip's cell tooltips + the chart's "empty ~turn N" marker read as absolute turns.
	_hud.update_overlay(ARRIVAL_PREVIEW_TURN, {})
	_hud.show_tile_selection({})
	_hud._band_labor._pending_labor.clear()

	# (a) A LUMPY hunt (gaps) beside a CONTINUOUS forage (every slot positive). The hunt row must gain a
	# tick strip with visible gaps; the forage row must gain NONE (the gap rule); the merged projection
	# must sawtooth upward (hauls > flat drain).
	# `_arrivals_band_fixture` is the fixture that actually RENDERS the FOOD OUTLOOK chart (it carries
	# `arrival_schedule`s; the plain `_band_fixture` does not, so its band zone has no chart at all).
	# The TALL (L) shell shows the full chart; the height-capped T/B shells (top + bottom) land the band
	# zone in the SHORT tier, where the chart is DROPPED and the role cards go hint-less. The
	# content-fits assertion on the T/B frames is what proves that drop keeps the zone inside its box:
	# ungated (the chart rendered at full height in the SHORT tier) it overruns the ~300px T/B cap by
	# 115px, which is exactly the overflow the tier gating exists to prevent — and which the work-heavy
	# `band_panel_work_wide` / `band_panel_parties_inspector_wide` states cannot catch (their big band's
	# vitals carry no chart either).
	_push_bands([_arrivals_band_fixture()])
	_panel.set_active_tab(&"band")   # the narrow (L) shell shows ONE zone; these frames judge the band one
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_arrivals_left"},
			{"edge": SIDE_TOP, "name": "band_panel_arrivals_top"},
			{"edge": SIDE_BOTTOM, "name": "band_panel_arrivals_bottom"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _settle()   # let the deferred fit_content re-pack settle before capture
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()

	# (b) A band whose larder EMPTIES inside the horizon: sparse lumpy hauls under a heavy drain, so the
	# walk hits 0 and the chart draws the dashed DANGER "empty ~turn N" marker.
	_push_bands([_arrivals_starving_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_arrivals_empty")

	# ---- Zone content (docs/band_panel_ux_proposal.html) ----------------------
	# PEOPLE + WORKFORCE bars and the two role CARDS, in the TALL (L dock) shell where the band zone
	# gets its full height: both bars, their keys, the dependency ratio, and the hinted cards.
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"band")
	await _settle()
	await _save("band_panel_people")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# band_panel_people_map_path — THE SAME PEOPLE BLOCK, reached the OTHER way: by clicking the band
	# ON THE MAP. `band_panel_people` above drives the SNAPSHOT path (`update_band_alerts` re-resolves
	# the band from the raw `populations` floats), which is exactly the path that SELF-HEALS the marker
	# truncation bug — so it could never have caught it. The map path feeds the panel MapView's unit
	# MARKER instead (`_rebuild_unit_markers` → `refresh_selection_payload` → `show_unit_selection` →
	# `_render_band_into_panel`), and a marker that narrowed the fractional age brackets with `int()`
	# zeroes every remainder, leaving `HudFormat.apportion_people` nothing to redistribute: 9 + 16 + 4 = 29 in
	# the PEOPLE header against a band of 30. Driven through the REAL MapView, never a hand-built dict.
	var map_path_view: Node2D = MAP_VIEW_SCRIPT.new()
	map_path_view.visible = false   # data only — a visible map would render behind every later frame
	add_child(map_path_view)
	map_path_view.display_snapshot(_map_path_snapshot())
	map_path_view.unit_selected.connect(_hud.show_unit_selection)
	map_path_view.handle_hex_click(MAP_PATH_TILE.x, MAP_PATH_TILE.y, MOUSE_BUTTON_LEFT)
	# The HUD already holds its own copy of the payload, so the map goes away BEFORE the capture:
	# MapView's minimap is its own CanvasLayer and is NOT hidden by `visible = false`, so a surviving
	# instance paints a stray thumbnail into this frame and every later one (map_preview's gotcha).
	map_path_view.unit_selected.disconnect(_hud.show_unit_selection)
	map_path_view.queue_free()
	await get_tree().process_frame
	await _settle()
	_assert_people_sum_matches_size(_hud._selection._selected_unit, "band_panel_people_map_path")
	_assert_map_path_states_kit()
	await _save("band_panel_people_map_path")
	# Restore the snapshot-path band so the later states start from the same subject they always did.
	_push_bands([_band_fixture()])

	# The paged WORK BOARD at 34 sources — far past one page in the narrow (L dock) shell, so the
	# pager must appear and NOTHING may scroll. Its patches carry RUNG marks on a stride, so the
	# board is also where the marks are judged at real density — and, because the shell-threshold
	# probes below re-render this same band, where they are judged at the narrowest legal column.
	_hud.update_food_modules(_many_forage_modules())
	_set_forage_patches(_many_source_patch_fixtures())
	_push_bands([_many_sources_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_page")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	# The board renders in NAME order now (issue #460), so this is where both halves of that change are
	# judged: the sorts themselves, and the `⋯` menu saying which one is running.
	_assert_work_sort_stable()
	_assert_work_menu_marks_active_sort("band_panel_work_page")

	# The same 34 sources in the WIDE (bottom dock) shell: multi-column, column-major, hairlines.
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_work_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# A row OPEN in the inspector strip: the board loses rows to it, and still no scrollbar.
	_panel.set_dock(SIDE_LEFT)
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_source_models(_hud._band_labor._panel_band, 0)[0]["key"])
	await _settle()
	await _save("band_panel_inspector")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# The Work menu's destructive action asks first, and the confirm names what is SPARED.
	_hud._bandpanel._on_work_unassign_all_pressed(_hud._band_labor._panel_band, 34)
	await _settle()
	await _save("band_panel_clear_confirm")
	_dismiss_dialogs()

	# THE TWO PRODUCTS ON THE WORK BOARD (issue #337). The concerning-food band works three sources —
	# a forage patch (food only), a deer hunt (food AND trade, food leading) and a WOLF hunt whose food
	# fields are honestly 0. Its row must headline `⇄ +0.22` ALONE: before this arc the client read only
	# food, so the wolf row said `+0.00 /turn` and the pack looked worthless. The inspector strip is
	# opened on that row so its one-sentence readout is judged too — it states the same components the
	# row does. The Food line above is the control: it still counts FOOD only, so a trade-only hunt must
	# not move it (trade goods credit the faction stockpile, never the larder).
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_push_bands([_concerning_food_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_trade_rows")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_open_work_inspector_for_herd(TRADE_ONLY_HERD_ID)
	await _settle()
	await _save("band_panel_work_trade_inspector")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# THE AGGREGATES (issue #337, phase 2). Same board with the deer removed, so the band's ONLY hunt
	# pays trade: the head must read `2 sources +0.15 /turn ⇄ +0.22` — a SIBLING trade total, never
	# folded into the food one — and the hunt chip `🦌 1 · ⇄ 0.22`, with the food component suppressed
	# rather than printed as a `0.00` that says the wolf pack yields nothing. This is the frame the
	# fix is judged on: the previous state's header excluded the wolf's `+0.22` while its row sat
	# directly underneath, so the arithmetic visibly did not add up.
	_push_bands([_trade_only_hunt_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_trade_totals")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# THE WORK INSPECTOR'S POLICY PICKER — the one control on the board with no frame coverage at all
	# until it got these (`_work_floor_open` is otherwise never true in either harness). Two rows: one
	# BUILDING a pen beside one that is not, and the claim is that the picker cannot tell them apart.
	# The standing-investment WARN line and the discard confirm that used to ride the first row are
	# gone with issue #442 — a stance re-pick leaves the improvement alone, so there is nothing to warn
	# about discarding, and both rows take the immediate-emit path the extractive one always did.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_set_world_herds(_investment_policy_herd_fixtures())
	_push_bands([_investment_policy_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	_open_work_policy_picker_for_herd(INVESTMENT_ROW_HERD_ID)
	await _settle()
	await _save("band_panel_work_policy_investment")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	# A BUILDING row lights its STANCE like any other — the state that used to light nothing.
	_assert_lit_rung(INVESTMENT_ROW_PRESET)
	_assert_policy_pick_confirms(INVESTMENT_ROW_HERD_ID, false)
	# THE OTHER HALF OF "a stance re-pick leaves the improvement alone": the pick must also not DROP it.
	# The frame above judges what is DRAWN; this judges what the edit WRITES, which no PNG can show — a
	# board rendered from a blanked axis looks like a perfectly ordinary board.
	_assert_crew_edit_keeps_improvement(INVESTMENT_ROW_HERD_ID, INVESTMENT_ROW_IMPROVEMENT)

	# The CONTROL: the very same picker on the row that is building NOTHING. Its two assertions are
	# now identical to the pair above, which IS the claim — the improvement axis is invisible here.
	_open_work_policy_picker_for_herd(EXTRACTIVE_ROW_HERD_ID)
	await _settle()
	await _save("band_panel_work_policy_extractive")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_lit_rung(EXTRACTIVE_ROW_PRESET)
	_assert_policy_pick_confirms(EXTRACTIVE_ROW_HERD_ID, false)
	_hud._bandpanel._work_floor_open = false
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# UNDER-CONTAINED managed herd in the WORK board (fauna neglect-escape arc): a Corralled herd that
	# needs 4 herders but is staffed with only 2 sheds animals to the wild. It must read as trouble
	# WHEREVER it is listed — here, on its work row — with the established overhunt ⚠ (amber marks +
	# amber severity stripe) and the "Too few herders — animals are drifting off." note in the
	# inspector, not only in its own drawer.
	_set_world_herds(_under_herded_work_herd_fixtures())
	_push_bands([_under_herded_work_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_under_herded")
	# ASSERTED WHILE ITS BAND IS STILL STAGED. This call sat ~45 lines further down, below the
	# rung-ready block that replaces the panel band — so it looked for a Hunt row on a herd nobody
	# worked and reported "no Hunt work row for game_aurochs_uh" on every run. A guard that fails for
	# want of its own subject says nothing about the flag it was written to pin.
	_assert_under_herded_work_row(UNDER_HERDED_WORK_HERD_ID)

	# THE RUNG-READY MARK ON THE WORK BOARD (issue #412) — the panel twin of the map badge. Three rows,
	# and the CONTRAST is what the frame is for: a tended patch on willing ground offers `⌃▦`, a fully
	# tamed "pen"-ceiling herd offers `⌃🐄`, and a wild-ceiling herd offers nothing however much the
	# faction knows. A chevron on every row would prove nothing.
	#
	# Knowledge is pushed FIRST: the mark reads `RungGates` against the top bar's row, so without it
	# every source is honestly "not ready" and the board renders a frame with nothing to look at.
	_hud.update_intensification([{"faction": 0,
		"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0}])
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_set_forage_patches(_ready_patch_fixtures())
	_set_world_herds(_ready_herd_fixtures())
	_push_bands([_ready_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_rung_ready")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_ready_marks()

	# The READY FILTER chip narrows the board to exactly those rows — its own count beside the
	# attention chip, never folded into it.
	_hud._bandpanel._set_work_filter(HudWorkVocab.WORK_FILTER_READY)
	await _settle()
	await _save("band_panel_rung_ready_filter")
	_assert_ready_filter_narrows()
	_hud._bandpanel._set_work_filter(HudWorkVocab.WORK_FILTER_ALL)
	_hud.update_intensification([])

	# THE FORAGE JUMP NAMES THE LAND (issue #412, a pre-existing defect the marks made reachable-looking).
	# A hunt row always named its herd; a forage row focused the tile and left the hex's AUTO-PICK to
	# choose, so on a hex that also holds a band or a herd it opened THAT instead of the patch. The mark
	# is what makes it matter: a row that says "this patch can be sown" must land on the patch.
	#
	# Asserted, not pictured — the wrong subject and the right one render the same card shape.
	_assert_forage_jump_names_land()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# THE HERDER FLOOR — the board must not flag a problem and then disable its own remedy. A managed
	# Wild Fowl herd grew to owe 3 keepers while its take saturates at 2 workers, and the row is staffed
	# at 2 with idle workers free. The take-side max-useful alone would gate the `+` dead at 2, directly
	# under the ⚠ that says a 3rd herder is needed (the playtest report). Both cap twins now floor on
	# `SourceForecast.herd_crew_floor`, so the row's `+` reaches the crew the sim is asking for — and the
	# assertion states that as the twin invariant, which a PNG structurally cannot carry.
	_set_world_herds(_herder_floor_herd_fixtures())
	_push_bands([_herder_floor_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_herder_floor")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_herder_floor_row(HERDER_FLOOR_HERD_ID)

	# THE SOURCE-RUNG BOARD — five rows, one per rung of the two ladders, on ONE band so the marks are
	# read against each other: wild forage (NO mark, the control) · 🌾 Tended Patch · ▦ Field · ◎
	# pastoral herd · 🐄 penned herd. The mark is orthogonal to the policy glyph, which reads ♻ Sustain
	# on every row here precisely so the frame cannot be passed by the verb: before this, a Tended Patch
	# under Sustain and plain wild ground under Sustain were indistinguishable on the board. The narrow
	# (L) shell puts all five in one column at `WORK_COLUMN_MIN_WIDTH`, which is also where the label's
	# remaining width is judged.
	_hud.update_food_modules(_rung_forage_modules())
	_set_forage_patches(_rung_patch_fixtures())
	_set_world_herds(_rung_herd_fixtures())
	_push_bands([_rung_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_rungs")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_work_row_rungs()
	_assert_rung_labels_are_hoverable()

	# The same five rows in the WIDE (bottom) shell, where the rung slot competes with the multi-column
	# split for the label's width.
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_work_rungs_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# Back to the LEFT dock before moving on: the states after this one inherit the dock rather than
	# setting their own, so leaving the panel bottom-docked would silently re-render `band_panel_no_idle`
	# and `band_panel_compose_hunt` in the wide shell.
	_panel.set_dock(SIDE_LEFT)

	# Restore the reference band so later states start from their usual subject — and the paged board's
	# patch set with it, because `update_forage_patches` REPLACES the lookup: the ultrawide, dock-row
	# and shell-threshold states below re-render `_many_sources_band_fixture`, so leaving the five rung
	# patches installed would strip the marks back off exactly the frames that judge them at the
	# narrowest legal column.
	_set_world_herds(_herd_fixtures())
	_set_forage_patches(_many_source_patch_fixtures())
	_push_bands([_band_fixture()])

	# The parties COMPOSE sheet, QUARRY-FIRST. With a quarry picked the whole hunt form resolves: the
	# policy rungs carry their ascending per-policy metric, the party stepper caps at the raid's
	# max-useful plateau, the trip forecast reads, and the Send button takes its verdict.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_set_world_herds(_quarry_herd_fixtures())
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_assert_quarry_eligibility()
	_assert_denial_quarry_eligibility()
	_assert_denial_party_needed_skips_horizon()
	_assert_denial_turn_clause_shapes()
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = "hunt"
	_hud._compose.set_party_quarry(QUARRY_FAR_HERD_ID)
	# Picking a quarry fills the party to its max-useful cap (the one-shot `TargetingController._try_pick_quarry` sets);
	# seed it here too so the frame shows the shipped default (the party at the cap, not a stray 1).
	_hud._compose.arm_party_autofill()
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt")
	_report_compose_widths("band_panel_compose_hunt")
	_assert_hunt_sheet_chart(true, "band_panel_compose_hunt")

	# **THE SAME SHEET IN THE HEIGHT-CAPPED TOP DOCK** — the tier gate on the chart, and the only
	# state that renders it. The parties zone CLIPS there, and the chart is ~150px of a ~300px box, so
	# the SHORT tier keeps the presets alone exactly as the band zone's outlook chart is kept out. The
	# frame is judged on the ABSENCE plus the fit: a gate that never fired and a chart clipped off the
	# bottom of the zone are the same picture.
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_compose_hunt_short")
	# **THE FIT IS REPORTED HERE, NOT ASSERTED, and the measurement is why.** An OPEN parties compose
	# sheet does not fit a height-capped horizontal dock at all: measured at 593px of a 265px box
	# WITHOUT the chart — quarry row, presets, floor hint, party stepper, kit row, forecast and send,
	# none of which this tier drops. That is a pre-existing property of opening the sheet in a T/B
	# dock (no frame had ever rendered it there) and not the chart's doing; gating the chart is
	# necessary and nowhere near sufficient. Asserting the fit here would fail on a defect this state
	# exists to document, and skipping the assertion silently would hide it — so the extent is printed
	# with the frame beside it.
	_report_zone_content_extent("band_panel_compose_hunt_short")
	_report_compose_widths("band_panel_compose_hunt_short")
	_assert_hunt_sheet_chart(false, "band_panel_compose_hunt_short")
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	# **THE DOCK IS THE SECOND LAUNCH SITE, AND IT MUST OFFER THE SAME ORDERS** (§5.2). A lever on the
	# herd drawer's sheet and absent here is the same defect as a lever that does nothing, so both
	# halves are asserted: the fill-target control is present, and the trip's BOUND is named. The bound
	# rides its own quiet line here (this zone's forecast is the one-LINE form, already dense with five
	# facts) where the drawer folds the identical clause into its readout verdict — one table, so the
	# two surfaces cannot describe one stop differently.
	_assert_band_panel("the dock's hunt sheet offers a fill target, like the herd drawer's",
		_find_meta_control(_panel, HudWidgets.FILL_TARGET_META) != null)
	_assert_band_panel("…and names which stop ends the trip",
		_has_label_containing(_panel, SourceForecast.TRIP_BOUND_CLAUSES[
			SourceForecast.TRIP_BOUND_PACK_FULL]))
	# **ONE QUARRY ON THE HEX GETS NO CHOOSER, and this frame is the whole guarantee that the common
	# case did not grow chrome for the rare one.** The boar stands alone on (75, 18); the paired
	# positive is `band_panel_compose_deny_two_quarries`, without which a chooser rendered on every
	# sheet would satisfy every claim there.
	_assert_band_panel("a lone quarry on the hex gets NO chooser on the Quarry row",
		_find_meta_control(_panel, HudWidgets.QUARRY_CHOICES_META) == null)

	# The same sheet on ERADICATE — the frame the EXPEDITION rung's hint is judged on (issue #337). The
	# launch picker is the ONE surface that renders `SEND_HUNT_POLICY_HINTS` verbatim, and Eradicate's
	# line must describe the whole-stock haul, the currency the SPECIES pays (meat, ⇄ trade goods, or
	# both — the raid banks its trade half too now) and the permanent end state, never "delivers no food".
	_hud._bandpanel._send_hunt_floor = SourceForecast.FLOOR_MIN
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt_eradicate")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._send_hunt_floor = SourceForecast.DEFAULT_HARVEST_FLOOR

	# The same sheet with NO quarry yet: the "Choose…" row, the hint, a disabled Send — and nothing
	# below it, since policy/party/forecast are all unanswerable without a herd.
	_hud._compose.clear_party_quarry()
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt_no_quarry")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# Same sheet under Scout: scouting title, NO quarry row, NO policy picker, "Send scouting party…".
	_hud._bandpanel._party_compose_mission = "scout"
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_scout")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# **THE DENIAL FORM — the third verb** (`docs/plan_denial_raid.md` §3). Quarry → party → the
	# COLLAPSE VERDICT → the take → send. What is ABSENT is the specification: no floor picker, no
	# floor hint, no fill target, no crew preset — a herd and a party size, and nothing else the
	# `send_denial_raid` grammar (closed at four tokens) could even carry.
	_hud._bandpanel._party_compose_mission = HudComposeVocab.COMPOSE_MISSION_DENY
	_hud._compose.set_party_quarry(QUARRY_FAR_HERD_ID)
	_hud._bandpanel._send_expedition_count = DENIAL_PARTY
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_viable()

	# ---- THE KIT PICKER, on the sheet the roster was designed against ----------------------------
	# **CLOSED.** The row sits directly under the party stepper and above the verdict, because a kit
	# describes the crew and moves every figure below it. The band is re-pushed carrying real component
	# CONDITIONS, so the hint line under the picker states this band's EFFECTIVE tier — the fresh-kit
	# numbers on `KitOption` are not what a band with worn spears actually gets, and quoting them would
	# be the defect class this branch has spent four commits removing.
	_push_bands([_scout_expedition_fixture(), _kit_worn_band_fixture(), _hunt_expedition_fixture()])
	_hud._compose.set_party_kit_id(BandFx.KIT_DEFAULT_HUNT)
	_hud._bandpanel._send_expedition_count = DENIAL_PARTY
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_kit")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_kit_picker_closed()

	# **OPEN.** The roster grows toward a dozen kits and a pill row cannot hold that in a 354px column,
	# so the control is a picker button opening a CHECKED-RADIO menu — the quarry chooser's idiom. The
	# popup is an embedded subwindow, so it lands in the capture; the structural claims (which entries,
	# which one is marked, which one is tagged the default, and `none` LAST) ride the assertion, since
	# a screenshot cannot say which item carries the radio dot.
	var kit_menu := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as MenuButton
	if kit_menu != null:
		# Placed by hand under the button. `MenuButton.show_popup()` would do it, but it also grabs
		# input and can move focus mid-run; the popup is an EMBEDDED subwindow, so positioning it and
		# calling `popup()` renders it into the same viewport the capture reads.
		var below := kit_menu.get_screen_position() + Vector2(0.0, kit_menu.size.y)
		kit_menu.get_popup().position = Vector2i(below)
		kit_menu.get_popup().popup()
	await _settle()
	await _save("band_panel_compose_deny_kit_open")
	_assert_kit_picker_open(kit_menu)
	if kit_menu != null:
		kit_menu.get_popup().hide()

	# **THE KIT-MISMATCH STATE** — `none` selected against a table quoted for `big_game`. This is the
	# frame the honesty rule is judged on, and it is judged largely on what the sheet must NOT say: no
	# collapse verdict, no estimate caveat, no take line, no counted refusal. What it MUST say is the
	# combat gate — composed from wire terms, honest at any tier — plus the sentence naming the kit
	# those withheld numbers belonged to. Driven through the popup's REAL `id_pressed`, so the pick
	# path is exercised rather than the model being written.
	_pick_kit(KitRoster.NO_KIT_ID if kit_menu == null else BandFx.KIT_ID_NONE)
	await _settle()
	await _save("band_panel_compose_deny_kit_mismatch")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_kit_mismatch_suppresses_estimates()
	# Restore: the frames below are read against the DEFAULT kit and the reference band, and a
	# selection left on `none` would suppress every verdict they assert.
	_hud._compose.set_party_kit_id(BandFx.KIT_DEFAULT_HUNT)
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_hud._bandpanel._send_expedition_count = DENIAL_PARTY
	_hud._bandpanel.rerender()
	await _settle()

	# The SAME form against a herd that outbreeds the party — the `repelled` verdict, which is a claim
	# about the PARTY and not about the clock. It still LAUNCHES (a raid that cannot get there keeps
	# working the herd until recalled), so the Send warns rather than blocking. Judged as a PAIR with
	# the viable frame above: a table answering one verdict for every outcome satisfies either alone.
	_set_world_herds(_quarry_herd_fixtures(_denial_repelled_rows()))
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_repelled")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_repelled()
	_set_world_herds(_quarry_herd_fixtures())

	# **TWO HERDS ON ONE HEX** — the reported gap. The map click names a TILE, so a warren sharing a
	# hex with a wolf pack resolves to whichever the snapshot lists first and re-clicking resolves to
	# the same one; the Quarry row's `⋯` chooser is the way to the other. Rendered on the DENIAL form
	# because that is where it was reported, and the row is shared, so the hunt form gets the identical
	# control from the identical builder. The pair reads differently on purpose — a warren pays meat,
	# a wolf pays pelts alone — so the chooser is judged on two rows that could not be confused.
	_set_world_herds(_shared_tile_quarry_fixtures())
	_hud._compose.set_party_quarry(SHARED_TILE_FOOD_HERD_ID)
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_two_quarries")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_quarry_chooser()
	_set_world_herds(_quarry_herd_fixtures())

	# **THE SAME VIABLE FORM ON A QUARRY THE BAND IS CAMPED ON TOP OF** — the reported defect. Denial
	# erases a herd rather than harvesting one, so a herd inside the band's hunt reach is a legal target
	# (a HUNT of it still is not — `_assert_denial_quarry_eligibility` pins both halves). The walk out
	# is ZERO here, which is the frame's other claim: the verdict must still name its span and must not
	# append "(0 of them travel)".
	_hud._compose.set_party_quarry(QUARRY_HOME_HERD_ID)
	# **RE-PINNED, because adopting a quarry now SEEDS the party.** The chooser assertion above drives
	# the real `choose_quarry`, which arms the autofill the denial sheet consumes — so the sheet came
	# out of that block on the shared hex's requirement rather than on `DENIAL_PARTY`, and this frame's
	# verdict is asserted against that row. Stating the party is what keeps the frame's claim its own.
	_hud._bandpanel._send_expedition_count = DENIAL_PARTY
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_in_reach")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_in_reach_verdict()

	# **A BAND WITH MORE IDLE WORKERS THAN `max_expedition_party_size`, ON A QUARRY THAT NEEDS MORE
	# STILL.** That field is the wire echo of the estimate tables' sampling axis, not a rules cap, so
	# the stepper's ceiling is the band's own idle workforce — and this quarry's requirement (11) sits
	# one rung past the 8 the old cap enforced, i.e. past a party the sheet could not even be dialled
	# to. The quarry is adopted through the REAL `choose_quarry` — the one adoption both the map pick
	# and the chooser take — so the seed is exercised by the path that arms it rather than by writing
	# the count.
	_push_bands([_scout_expedition_fixture(), _deep_party_band_fixture(), _hunt_expedition_fixture()])
	var deep_herds := _quarry_herd_fixtures(_denial_needs_deep_party_rows())
	_set_world_herds(deep_herds)
	_hud._compose.clear_party_quarry()
	_hud._targeting.choose_quarry(_deep_party_band_fixture(), deep_herds[0],
		HudComposeVocab.COMPOSE_MISSION_DENY)
	await _settle()
	await _save("band_panel_compose_deny_deep_party")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_deep_party()

	# The SAME sheet stepped back BELOW the requirement: that row is `repelled`, and its reason must
	# now NAME the party the sim quotes instead of prescribing hands without a count.
	_hud._bandpanel._send_expedition_count = DENIAL_DEEP_PARTY_SHORT
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_short_party")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_counted_refusal()

	# **THE SAME QUARRY IN FRONT OF A BAND THAT CANNOT FIELD IT AT ALL** — the reference band's THREE
	# idle workers against a requirement of 11. This is the one state in which the Send DISABLES: a
	# party the player chose to under-size still launches (the frame above), but a band that cannot
	# reach the requirement however it dials the stepper has no such choice to be trusted with. Only
	# the band changes; the herds are the deep-party table still, so the pair differ in supply alone.
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_short_handed")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_short_handed()

	# **THE REPORTED VERDICT SHAPE — a bounded expectation over an UNBOUNDED bad run.** No other denial
	# table in this file leaves an end open, so no frame could show what the old rule did with one: it
	# dropped the expectation and quoted the lucky end alone, under a take line priced at the
	# expectation. Back on the reference band, so the sentence is what differs from the frames above.
	_set_world_herds(_quarry_herd_fixtures(_denial_open_high_rows()))
	_hud._bandpanel._send_expedition_count = DENIAL_OPEN_HIGH_PARTY
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_open_high")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_denial_open_high_verdict()
	_set_world_herds(_quarry_herd_fixtures())

	_hud._bandpanel._send_expedition_count = 1
	_hud._bandpanel._party_compose_open = false
	_hud._bandpanel._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

	# Zero idle workers: BOTH mission buttons (Scout / Hunt) stay VISIBLE and DISABLED, with the
	# shared reason line beneath them.
	_push_bands([_no_idle_band_fixture()])
	await _settle()
	await _save("band_panel_no_idle")

	_assert_no_scroll_containers()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# PARTIES INSPECTOR STRIP — a row click opens the full Mission/Target/Policy/Phase/Carried/
	# Next-delivery detail, mirroring the work board's row → inspector.
	_hud.show_tile_selection({})
	_hud._band_labor._pending_labor.clear()
	_set_world_herds(_herd_fixtures())

	# (a) WIDE shell (bottom dock): the strip renders in the height-capped T/B shell too → the
	# DELIVERING party's "Next delivery: ~14 food in 6 turns". Reuses the work-heavy band fixture (the
	# `band_panel_work_wide` config) so the board is populated; its band zone fits the ~300px T/B cap
	# for the same reason `_band_fixture`'s does — the SHORT tier drops the FOOD OUTLOOK chart (that
	# gating is what `band_panel_arrivals_top`/`_bottom` guard with a chart-bearing fixture). The strip
	# + a party row + footer fit because the strip replaces the bottom spacer (`_build_parties_zone_content`).
	_hud.update_food_modules(_many_forage_modules())
	_push_bands([_many_sources_band_fixture(), _hunt_expedition_fixture()])
	_panel.set_dock(SIDE_BOTTOM)
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))   # close before the next state

	# (b) NARROW shell (left dock, Parties tab): the tall L/R parties zone holds both parties + the strip
	# with room to spare. Inspect the NO-SURPLUS party → the invisible-line bug the strip fixes:
	# "Next delivery: none — the herd has no surplus to raid" must be VISIBLE, not hidden.
	_push_bands([_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_narrow")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))

	# (b2) NEXT-DELIVERY DISAMBIGUATION on a projected-0 forecast. A hunt party is bound to ONE herd
	# (its `expedition_target_herd`) that MIGRATES and is often NOT the herd on the tile the player is
	# looking at, so a projected 0 means one of two things and the party's target tells them apart:
	# still in `_world_herds` → at/below its policy floor (no surplus); absent → lost/replaced (returning
	# home). The Target row also carries the target's live position so the player can SEE which herd the
	# party is bound to. Render all three parties + assert every line. `_world_herds` = _herd_fixtures():
	# game_deer_07 (@68,15) + game_deer_79 (@64,11); the LOST party targets an absent id.
	_set_world_herds(_herd_fixtures())
	_push_bands([
		_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture(),
		_lost_hunt_expedition_fixture(),
	])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LOST_ENTITY))
	await _settle()
	await _save("band_panel_next_delivery_disambiguation")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_next_delivery_disambiguation()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LOST_ENTITY))

	# (c) DETAIL-PANEL via the MARKER path — the FIX-4 regression. The Occupants-card drawer reads
	# `BandDetailLines.expedition_summary_lines(_selected_unit)`, and `_selected_unit` is the MapView unit MARKER, not
	# a raw `_player_expeditions` dict. Drive the REAL marker path (display_snapshot →
	# _rebuild_unit_markers → handle_hex_click → show_unit_selection → _selected_unit) with a hunt party
	# projecting 14.5 food in 6t, and ASSERT the Next-delivery line reaches the panel (rounds to 15).
	_assert_detail_panel_delivery()

	# (d) The row ✕ recall must CONFIRM first (like "Recall all"), not emit immediately.
	_assert_row_recall_confirms()

	# ULTRAWIDE: past the width the three zones can USE, the wide shell CENTRES at its content cap
	# instead of stretching, leaving equal margins either side. Without it a single work row is strung
	# across the whole monitor and the band zone sits a screen away from the parties zone. The frame to
	# read is the equality of the two black margins — and that the board itself is unchanged.
	await _pin_window(Vector2i(ULTRAWIDE_WIDTH, ULTRAWIDE_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	_push_bands([_many_sources_band_fixture()])
	await _settle()
	await _save("band_panel_wide_ultrawide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	print("band_panel_preview: ultrawide — work zone %.0fpx of a %dpx panel (capped + centred)" % [
		_panel.work_zone_size().x, ULTRAWIDE_WIDTH])

	# THE SHELL THRESHOLD, bracketed. `WIDE_SHELL_MIN_WIDTH` is DERIVED from what the wide shell needs
	# (both flanks + one readable work column + the separators + the card chrome), and nothing else in
	# this harness renders anywhere near it — 1500 and 3440 are both comfortably past it, so a
	# too-low threshold was invisible here. These two frames are the before/after of the flip.
	# The bottom-bar chrome now SHARES a horizontal dock's row (issue #324), and the shell test reads
	# the panel's width MINUS the trailing chrome rail — so the probe widths must add the live rail width
	# back on, or they would bracket a threshold the panel no longer applies to the raw window width. The
	# width is canvas-independent (`max` of a fixed 260px turn cluster and a grid-aspect minimap), and the
	# panel is already bottom-docked + reflowed from the ultrawide state above, so it can be read here.
	# `_rail_span()`, not `_rail_width()`: the rail also costs a `RAIL_SEPARATOR_SPAN` gutter, and probing
	# against the bare width would bracket the threshold 25px off.
	var rail_span: float = _panel._rail_span()
	var shell_threshold_width := int(ceil(BandCityPanel.WIDE_SHELL_MIN_WIDTH + rail_span))
	print("band_panel_preview: shell threshold probes at %d / %d (threshold %.0f + rail span %.0f)" % [
		shell_threshold_width - SHELL_THRESHOLD_UNDERSHOOT, shell_threshold_width,
		BandCityPanel.WIDE_SHELL_MIN_WIDTH, rail_span])
	# One pixel BELOW: the wide shell could not give the board a readable column, so the panel must
	# choose the NARROW tabbed shell — which hands the board the panel's WHOLE interior.
	await _pin_canvas(Vector2i(shell_threshold_width - SHELL_THRESHOLD_UNDERSHOOT, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_shell_below_threshold")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_shell_is_wide(false, "band_panel_shell_below_threshold")

	# Exactly AT it: the narrowest legitimate wide shell — three columns, the work zone at exactly
	# `ZONE_WORK_MIN_WIDTH`, its rows still legible with un-clipped labels.
	await _pin_canvas(Vector2i(shell_threshold_width, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_shell_at_threshold")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_shell_is_wide(true, "band_panel_shell_at_threshold")

	await _render_dock_row_states()

	# ---- THE BAND-ZONE TIERS, LAST, AND DELIBERATELY SO ------------------------------------------
	# The SHORT tier merges Growth onto the Morale line; TALL and COMPACT must not. Both probes RESIZE
	# THE CANVAS and re-dock, and a panel left in another shell silently re-renders every state after
	# it in the wrong one (measured: run mid-file, they flipped `band_panel_arrivals_top` from its
	# 300px `Zone_band` into a 265px `NarrowZoneHost` and overflowed it). So they run after the last
	# frame, where there is nothing left to perturb.
	_push_bands([_vitals_worst_case_band_fixture()])
	# The BAND tab, explicitly: the narrow shell renders ONE zone into `NarrowZoneHost`, and the run
	# above leaves whichever tab its last state selected — so without this the probes measure the WORK
	# board and find no vitals label to read at all.
	_panel.set_active_tab(&"band")
	# …and THE SAME BAND IN THE TALL DOCK, which must NOT have merged: Morale and Growth are separate
	# rows there, with the morale cause clause intact. Without this the merge could quietly become the
	# layout everywhere and every frame above would still be green.
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_report_zone_content_extent("band_panel_vitals_worst_case_tall")
	_assert_growth_row_not_merged("band_panel_vitals_worst_case_tall")
	# …and the COMPACT tier between them, which must not have merged either. **PNG-LESS, and that is
	# the honest shape of it**: the tier is reachable only on a short canvas (the narrow shell's zone
	# box is the canvas minus ~95px, so COMPACT's 340-420px band needs a 435-515px window), and this
	# band's COMPACT content measures 528px — it overflows that box by ~143px whatever the vitals do.
	# That is a pre-existing property of the tier and not this merge's business, so the ROWS are
	# asserted and the fit deliberately is not. Without this the merge could leak into COMPACT and
	# every rendered frame would still be green, since no frame renders at that tier.
	await _pin_canvas(Vector2i(PREVIEW_SIZE.x, COMPACT_TIER_PROBE_HEIGHT))
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_report_zone_content_extent("compact_tier_probe")
	_assert_growth_row_not_merged("compact_tier_probe")
	await _pin_canvas(PREVIEW_SIZE)

	_assert_herd_field_pairs()
	_finish()

# ---- THE DOCK-ROW REFLOW (issue #324) ---------------------------------------------------------
#
# On a HORIZONTAL dock the HUD's bottom-bar chrome shares the panel's reserved row — nav cluster at
# the leading end, turn orb at the trailing one — and `BottomBar` drops out of layout so `ContentRow`
# reclaims its height. A VERTICAL dock must be bit-identical to before. Rendered at 1080p, which is
# the window the issue is about, and driven through the REAL `reservation_changed → reflow_dock_row`
# path wired at the top of `_ready` (never by poking the controller).
func _render_dock_row_states() -> void:
	await _pin_canvas(DOCKROW_CANVAS)
	_seed_embedded_minimap()
	_push_bands([_many_sources_band_fixture()])

	# BOTTOM: the chrome in ONE column at the row's TRAILING end — minimap + zoom rail directly above the
	# turn orb — nothing in the row's leading gutter (the band zone is flush to the left edge), and
	# `BottomBar` gone.
	_panel.set_collapsed(false)
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_dockrow_bottom")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(true, "band_panel_dockrow_bottom")
	_assert_parked_chrome_fits("band_panel_dockrow_bottom")
	_assert_shell_is_wide(true, "band_panel_dockrow_bottom")
	print("band_panel_preview: dockrow bottom — rail %.0fpx + %.0f gutter = %.0f span (nav %.0f, turn %.0f), stack needs %.0f of a %.0f strip, work zone %.0fpx" % [
		_panel._rail_width(), BandCityPanel.RAIL_SEPARATOR_SPAN, _panel._rail_span(),
		_hud.nav_backing.get_combined_minimum_size().x, _hud.turn_orb.get_combined_minimum_size().x,
		_hud._dockrow._required_height(), _panel.current_reservation_size(),
		_panel.work_zone_size().x])

	# TOP — THE SECOND CONTROL, and it asserts the OPPOSITE of what it used to (issue #377). The chrome
	# must stay HOME: the minimap bottom-left and the turn orb bottom-right, where they always live.
	# Relocating for a top dock was a symmetry that was never measured — `Hud.set_reserved_inset` only
	# displaces `BottomBar` when the inset and the bar share an edge, i.e. on a BOTTOM dock, so a top
	# dock had nothing to recover and dragging the chrome to the top of the screen only cost the player
	# a fixed landmark. The card still floats and centres here; it simply has the whole strip to do it in.
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_dockrow_top")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(false, "band_panel_dockrow_top")
	_assert_no_rail_width("band_panel_dockrow_top")
	_assert_chrome_home_exact("band_panel_dockrow_top")
	# The NARROW shell here, and that is arithmetic rather than a regression: a top dock keeps the HUD's
	# strip, so its card has 1920 − 360 (left dock) − 419 (readouts) = 1141px, under the 1190 the wide
	# shell needs for three zones. The alternative to tabbing is drawing the card over the readouts, which
	# is the bug this state exists to prove is gone. A top dock reaches the wide shell on a wider window —
	# `band_panel_dockrow_ultrawide` is bottom-docked, where the HUD yields and the whole row is the
	# card's.
	_assert_shell_is_wide(false, "band_panel_dockrow_top")
	_assert_card_clears_hud_columns("band_panel_dockrow_top")

	# LEFT — THE CONTROL. A vertical dock keeps today's behaviour exactly: the chrome is back in
	# `BottomBar` and the rails contribute nothing. The work-zone baseline captured here is what the
	# round-trip state below compares against.
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_dockrow_left")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(false, "band_panel_dockrow_left")
	_assert_no_rail_width("band_panel_dockrow_left")
	var vertical_work_zone := _panel.work_zone_size()

	# COLLAPSED BOTTOM — the frame that proves collapse does not slice the minimap. The reserved strip
	# is `COLLAPSED_SIZE` (46px), far under the taller cluster's minimum, so the fit gate must DECLINE
	# and the chrome must stay in `BottomBar`.
	_panel.set_dock(SIDE_BOTTOM)
	_panel.set_collapsed(true)
	await _settle()
	await _save("band_panel_dockrow_collapsed_bottom")
	_assert_chrome_parked(false, "band_panel_dockrow_collapsed_bottom")
	_panel.set_collapsed(false)

	# THE ROUND TRIP. Reparenting round-trips are where this class of change rots, so walk
	# bottom → left → bottom → left and assert the clusters came home EXACTLY: authored parent AND
	# child index, the anchors/size flags captured at construction, `BottomBar`'s authored minimum
	# height, and a work zone identical to the never-reflowed baseline above.
	for edge in [SIDE_BOTTOM, SIDE_LEFT, SIDE_BOTTOM, SIDE_LEFT]:
		_panel.set_dock(edge)
		await _settle()
	await _save("band_panel_dockrow_reflow_round_trip")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_chrome_parked(false, "band_panel_dockrow_reflow_round_trip")
	_assert_no_rail_width("band_panel_dockrow_reflow_round_trip")
	_assert_chrome_home_exact("band_panel_dockrow_reflow_round_trip")
	var round_trip_work_zone := _panel.work_zone_size()
	if not round_trip_work_zone.is_equal_approx(vertical_work_zone):
		push_error("band_panel_preview: round trip left the work zone at %s, baseline was %s" % [
			round_trip_work_zone, vertical_work_zone])
	else:
		print("band_panel_preview: assert OK — round trip restored work_zone_size() to %s" % round_trip_work_zone)

	# ULTRAWIDE BOTTOM DOCK — the frame issue #377 was reported on, and the ONLY one that reaches the
	# configuration it describes. It runs LAST because it re-pins the canvas, and the round-trip state
	# above compares against a baseline captured at `DOCKROW_CANVAS`.
	#
	# The card is sized from `_card_width()` and placed by `_position_card_and_rail`, so the question this
	# frame asks is what the panel does with a strip FAR wider than its content wants: the card must come
	# out at its declared width and sit centred in the room the chrome cluster leaves, with open map
	# either side, rather than stretching to the monitor. It is deliberately a DOCK-ROW state rather than a wider
	# `band_panel_wide_ultrawide`: the parked chrome is the subject, so the frame needs the REAL minimap
	# this block has already seeded — against an empty `MinimapContainer` the rail is the zoom rail's
	# ~80px and a mis-placed rail is nearly invisible.
	await _pin_canvas(Vector2i(ULTRAWIDE_WIDTH, DOCKROW_CANVAS.y))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_dockrow_ultrawide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(true, "band_panel_dockrow_ultrawide")
	_assert_parked_chrome_fits("band_panel_dockrow_ultrawide")
	_assert_shell_is_wide(true, "band_panel_dockrow_ultrawide")
	_assert_card_is_narrower_than_strip("band_panel_dockrow_ultrawide")
	_assert_rail_is_right_justified("band_panel_dockrow_ultrawide")
	_assert_card_is_centred("band_panel_dockrow_ultrawide")
	var busy_card := _panel._panel.get_global_rect().size.x
	var busy_columns: int = _panel._work_columns

	# THE SAME ULTRAWIDE DOCK WITH NOTHING TO SHOW — the state the whole width rework is FOR, and the
	# one the 34-source frame above structurally cannot make: a board with 34 rows wants every column it
	# can get, so a card sized to its content and a card sized to the monitor look identical there.
	# A band with NO worked sources wants ONE column, so the card must come back visibly narrower.
	_push_bands([_band_fixture()])
	await _settle()
	await _save("band_panel_dockrow_ultrawide_empty")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_card_is_narrower_than_strip("band_panel_dockrow_ultrawide_empty")
	_assert_rail_is_right_justified("band_panel_dockrow_ultrawide_empty")
	_assert_card_is_centred("band_panel_dockrow_ultrawide_empty")
	_assert_card_follows_its_content(busy_card, busy_columns, "band_panel_dockrow_ultrawide_empty")
	# The state with the MOST open map around its card, so the gaps this probes are the ones a player
	# actually loses when the strip eats their clicks.
	await _assert_open_strip_reaches_the_map("band_panel_dockrow_ultrawide_empty")

## Put a REAL embedded minimap in the HUD's `MinimapContainer` before the dock-row states render.
## Without it those frames judge the reflow against an EMPTY container — the left rail collapses to the
## zoom rail's ~80px instead of the ~290px the game actually has, so both the measured rail span and the
## frames would be honest about nothing. Driven exactly as `MinimapController._setup` drives it
## (`setup_embedded` into `Hud.get_minimap_container()`, then `set_grid_size`, which calls
## `resize_to_aspect`), with the grid resolved from `MapSizes` and the raster a documented flat stand-in
## for `_rebuild_image`'s per-hex paint — see `DOCKROW_MINIMAP_FILL`.
func _seed_embedded_minimap() -> void:
	var container: Control = _hud.get_minimap_container()
	if container == null:
		push_warning("band_panel_preview: no MinimapContainer — dock-row rail widths will be unrealistic")
		return
	var option: Dictionary = MapSizes.option_for(DOCKROW_MAP)
	var grid := Vector2i(int(option["width"]), int(option["height"]))
	var minimap := MinimapPanel.new()
	add_child(minimap)
	minimap.setup_embedded(container)
	var image := Image.create(grid.x, grid.y, false, Image.FORMAT_RGBA8)
	image.fill(DOCKROW_MINIMAP_FILL)
	minimap.set_texture(ImageTexture.create_from_image(image))
	minimap.set_grid_size(grid.x, grid.y)
	print("band_panel_preview: dockrow minimap — %s map %dx%d (aspect %.3f) → panel min %s" % [
		option["label"], grid.x, grid.y, float(grid.x) / float(grid.y),
		minimap.panel.custom_minimum_size])

## GUARD: is the bottom-bar chrome parked in the panel's rail slots, or home in `BottomBar`? Asserts
## BOTH halves of the swap — `BottomBar`'s visibility and each cluster's PARENT — because either one
## alone can be right while the other is wrong (a hidden bar with the chrome still inside it erases
## the chrome; a parked chrome under a visible bar double-books the row's height).
func _assert_chrome_parked(parked: bool, state_name: String) -> void:
	var failures: Array[String] = []
	if _hud.bottom_bar.visible == parked:
		failures.append("bottom_bar.visible is %s but the chrome should be %s" % [
			_hud.bottom_bar.visible, "parked" if parked else "home"])
	for pair in _parked_chrome_pairs():
		var cluster: Control = pair[0]
		var want: Node = pair[1] if parked else _hud.bottom_bar
		if cluster.get_parent() != want:
			failures.append("%s sits under %s, expected %s" % [
				cluster.name, cluster.get_parent().name, want.name])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s chrome %s" % [state_name, "parked in the row" if parked else "home in BottomBar"])
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## The two parked-chrome clusters paired with the rail slot each belongs in — nav on TOP, turn cluster
## BELOW. One definition, so the parent assertion and the containment assertion cannot disagree about
## which cluster goes where.
func _parked_chrome_pairs() -> Array:
	return [
		[_hud.nav_backing, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_TOP)],
		[_hud.turn_orb, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_BOTTOM)],
	]

## GUARD: the parked chrome must FIT the rail and the rail must fit the strip, and the STACK must sit
## CENTRED in the column.
## **Fit** is the same claim `_assert_zone_content_fits` makes for the zones, and for the same reason:
## the rail CLIPS, so a cluster too wide or too tall for it is silently sliced rather than visibly
## broken. It is what catches a rail whose declared width lags the minimap's (the width is DECLARED,
## never measured from the content, so nothing else would notice) — and it is why these states seed a
## REAL minimap; against an empty `MinimapContainer` the rail collapses to the zoom rail's ~80px and the
## check is vacuous. Both levels are checked: each cluster inside the rail, and the rail inside the card's
## interior strip.
## **Centred** is the other half, and fitting does not imply it: a stack pinned to the rail's mid-line and
## grown DOWNWARD still sits entirely inside a 340px column while rendering ~64px low. That is exactly
## what `set_anchors_and_offsets_preset` does to a plain `Control` (see `BandCityPanel._build_rail`'s note
## 3), so the centre-vs-centre test is the guard on that trap.
func _assert_parked_chrome_fits(state_name: String) -> void:
	var failures: Array[String] = []
	var rail: Control = _panel._rail
	var rail_rect := rail.get_global_rect()
	var stack_top := INF
	var stack_bottom := -INF
	for pair in _parked_chrome_pairs():
		var cluster: Control = pair[0]
		var rect := cluster.get_global_rect()
		stack_top = minf(stack_top, rect.position.y)
		stack_bottom = maxf(stack_bottom, rect.end.y)
		var over := _rect_overflow(rect, rail_rect)
		if over.x > ZONE_BOUNDS_TOLERANCE or over.y > ZONE_BOUNDS_TOLERANCE:
			failures.append("%s %s spills the rail %s by (%.1f, %.1f)" % [
				cluster.name, rect, rail_rect, maxf(over.x, 0.0), maxf(over.y, 0.0)])
	# The rail must stay inside the STRIP — `_root`, not the card. Since issue #377 the chrome cluster is
	# a SIBLING of the card rather than its last cell, so asking whether it fits the card would now be
	# asking the wrong container entirely (and would fail on a correct layout).
	var strip := _panel._root.get_global_rect()
	var rail_over := _rect_overflow(rail_rect, strip)
	if rail_over.x > ZONE_BOUNDS_TOLERANCE or rail_over.y > ZONE_BOUNDS_TOLERANCE:
		failures.append("the chrome rail %s spills the card %s by (%.1f, %.1f)" % [
			rail_rect, strip, maxf(rail_over.x, 0.0), maxf(rail_over.y, 0.0)])
	var drift: float = absf(0.5 * (stack_top + stack_bottom) - rail_rect.get_center().y)
	if drift > ZONE_BOUNDS_TOLERANCE:
		failures.append("the chrome stack sits %.0fpx off the rail's vertical centre (stack %.0f, rail %.0f)" % [
			drift, 0.5 * (stack_top + stack_bottom), rail_rect.get_center().y])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the chrome stack fits its rail, the rail fits the strip, and the stack is centred" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## PRECONDITION for the two assertions below: the strip really is WIDER than the card wants to be, so
## the island geometry they judge has slack to get wrong. Without it both would pass vacuously on a
## window the card fills anyway, where "centred" and "flush right" are true for free.
func _assert_card_is_narrower_than_strip(state_name: String) -> void:
	var card := _panel._panel.get_global_rect().size.x
	var strip := _panel._root.get_global_rect().size.x
	var slack: float = strip - card - _panel._rail_span()
	if slack <= ZONE_BOUNDS_TOLERANCE:
		push_error("band_panel_preview: %s — the card (%.0fpx) fills its %.0fpx strip, so the island assertions below prove nothing" % [
			state_name, card, strip])
		return
	print("band_panel_preview: assert OK — %s the card is an island (%.0fpx card + %.0fpx chrome span in a %.0fpx strip, %.0fpx of open map)" % [
		state_name, card, _panel._rail_span(), strip, slack])

## GUARD: the chrome cluster is FLUSH RIGHT against the STRIP's trailing edge (issue #377).
##
## Measured against the strip rather than the card, and that changed with the islands: the rail used to
## be the last cell of `_card_row`, so the only sensible claim was "inside its own card's trailing
## inset". It is a sibling of the card now, anchored to `_root`, so the claim is the stronger one — it
## sits at the edge of the screen, with the card floating well to its left.
func _assert_rail_is_right_justified(state_name: String) -> void:
	var rail_right := _panel._rail.get_global_rect().end.x
	var strip_right := _panel._root.get_global_rect().end.x
	var gap: float = strip_right - rail_right
	if absf(gap) > ZONE_BOUNDS_TOLERANCE:
		push_error("band_panel_preview: %s — the chrome cluster ends at %.0f but the strip ends at %.0f (%.0fpx of dead space to its right)" % [
			state_name, rail_right, strip_right, gap])
		return
	print("band_panel_preview: assert OK — %s the chrome cluster is flush to the strip's trailing edge (%.0f)" % [
		state_name, strip_right])

## GUARD: the card's width FOLLOWS ITS CONTENT — the claim the whole rework rests on (issue #377).
##
## Compared against the SAME dock at the SAME canvas with a busier band, because the absolute width
## proves nothing on its own: a card hard-wired to any constant would satisfy "narrower than the strip"
## and "centred" perfectly. What it cannot satisfy is *changing* when the band does.
##
## Both halves are asserted, and the column count is not redundant with the width — a width that moved
## for some unrelated reason (a chrome tweak, a flank retune) would pass a width-only test while the
## board stayed at four columns, which is the actual complaint: an empty work zone stretched across the
## monitor. The exact arithmetic is asserted too, so a card that merely shrank *somewhat* fails.
func _assert_card_follows_its_content(busy_width: float, busy_columns: int, state_name: String) -> void:
	var failures: Array[String] = []
	var quiet_width := _panel._panel.get_global_rect().size.x
	var quiet_columns: int = _panel._work_columns
	if quiet_columns >= busy_columns:
		failures.append("an unworked band still asks for %d board columns against the busy band's %d" % [
			quiet_columns, busy_columns])
	if quiet_width >= busy_width:
		failures.append("the card is %.0fpx with nothing to show and %.0fpx with 34 sources — it did not follow its content" % [
			quiet_width, busy_width])
	# The difference must be exactly the columns dropped: nothing else in the card may have moved.
	var expected: float = busy_width - float(busy_columns - quiet_columns) * BandCityPanel.ZONE_WORK_MIN_WIDTH
	if absf(quiet_width - expected) > ZONE_BOUNDS_TOLERANCE:
		failures.append("the card is %.0fpx but dropping %d columns from %.0fpx predicts %.0fpx" % [
			quiet_width, busy_columns - quiet_columns, busy_width, expected])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the card follows its content (%.0fpx / %d columns busy → %.0fpx / %d quiet)" % [
			state_name, busy_width, busy_columns, quiet_width, quiet_columns])
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## GUARD: a TOP-docked card is drawn over NEITHER HUD column (issue #377).
##
## The top dock is the one edge where the HUD keeps its strip — its right-hand column of readouts belongs
## BESIDE the card, not pushed under the map — so it is also the one edge where the card can be drawn
## over something. The claim is made as rect non-overlap against the live regions rather than as "the
## bound was applied", because a bound that is set and then ignored reads identically to one that works.
##
## **It takes a negative control first, on the same two live rects**: with the bounds cleared the card
## genuinely DOES overlap, so a pass cannot be satisfied by two rects that happen never to meet — which
## is what a sparse band would give for free, and exactly how the half-fix looked complete.
func _assert_card_clears_hud_columns(state_name: String) -> void:
	var card := _panel._panel.get_global_rect()
	var columns := {
		"the left dock": _hud.left_dock_region.get_global_rect(),
		"the right readouts": _hud.turn_block.get_global_rect(),
	}
	# NEGATIVE CONTROL: unbound, this band's card must actually reach at least one of them.
	_panel.set_lateral_bounds(0.0, 0.0)
	var unbound := _panel._panel.get_global_rect()
	var would_collide := false
	for rect_variant in columns.values():
		if unbound.intersects(rect_variant):
			would_collide = true
	var live: Vector2 = _hud.lateral_column_widths()
	_panel.set_lateral_bounds(live.x, live.y)
	var failures: Array[String] = []
	if not would_collide:
		failures.append("the UNBOUND card %s clears both columns anyway, so this state proves nothing — stage a busier band" % unbound)
	for name_variant in columns:
		var rect: Rect2 = columns[name_variant]
		if card.intersects(rect):
			failures.append("the card %s is drawn over %s %s" % [card, name_variant, rect])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the card clears both HUD columns (and would collide unbound)" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## GUARD: the CARD sits centred in the room the chrome cluster leaves.
##
## Fitting does not imply centring (the `_assert_parked_chrome_fits` lesson on the other axis): a card
## packed hard against the leading edge is entirely inside its strip and reads as a panel that ignores
## the right half of an ultrawide. It is the CARD being measured now, not its content column — the
## column simply fills the card since the card itself became the thing that narrows.
func _assert_card_is_centred(state_name: String) -> void:
	var card := _panel._panel.get_global_rect()
	var strip := _panel._root.get_global_rect()
	var lead_margin: float = card.position.x - strip.position.x
	var trail_margin: float = (strip.end.x - _panel._rail_span()) - card.end.x
	if absf(lead_margin - trail_margin) > ZONE_BOUNDS_TOLERANCE:
		push_error("band_panel_preview: %s — the card is not centred: %.0fpx of margin leading, %.0fpx trailing" % [
			state_name, lead_margin, trail_margin])
		return
	print("band_panel_preview: assert OK — %s the card is centred (%.0fpx of open map either side)" % [
		state_name, lead_margin])

## GUARD: THE OPEN MAP EITHER SIDE OF THE CARD IS STILL CLICKABLE (issue #377).
##
## A horizontal dock reserves the whole strip but only DRAWS two islands in it, and the map renders
## through the gaps — so the gaps must behave like map. `MapView` picks hexes out of `_unhandled_input`,
## and the Viewport marks a press handled the moment any `STOP` control under the pointer takes it, so a
## `PanelRoot` left at the `Control` default silently eats every click, drag-pan and wheel-zoom aimed at
## the ~1929px of visible map around a 3440 bottom dock. Nothing about that is visible in a PNG: the
## frame is pixel-identical either way, which is why this claim is behavioural.
##
## **Driven through the REAL dispatch** (`Viewport.push_input`) against this harness's own
## `_unhandled_input`, the `ui_preview` event-dock idiom: the GUI pass runs first, and a press it
## consumes never becomes unhandled. Inspecting `mouse_filter` alone would assert the cause and not the
## effect — the filters are read back too, but only BESIDE the behaviour, so a future regression is
## legible rather than merely detected.
##
## **All three halves are required.** The precondition (open canvas reaches the map path) is what stops a
## probe that never fires from passing everywhere — the failure the event-dock version was rewritten to
## avoid. The gaps must reach. And the two ISLANDS must not, or a probe that fires indiscriminately would
## pass just as well.
##
## **The island half is asserted on each island's OWN surface** — the card's chrome ring (its border and
## content margins, where `PanelCard` itself is what the pointer finds) and the chrome cluster's bare
## column — never on the card's INTERIOR. The interior is zone content, whose controls carry their own
## filters, and it is measured as leaky: a press into the work board's blank area (a ~200×50 canvas-px
## region of `Zone_work` here) reaches `_unhandled_input` even though `PanelCard` is `STOP` and covers it,
## and neither a `STOP` child of the card nor a `STOP` sibling BEHIND it closes the hole (both tried; only
## a sibling in FRONT of the card does, which would eat the panel's own buttons). Asserting the interior
## would therefore pin an engine behaviour this panel does not control, in a claim about `_root`.
func _assert_open_strip_reaches_the_map(state_name: String) -> void:
	var strip := _panel._root.get_global_rect()
	var card := _panel._panel.get_global_rect()
	var rail_span: float = _panel._rail_span()
	var failures: Array[String] = []
	# PRECONDITION: a press on bare canvas, far from the strip, must reach unhandled input at all.
	var canvas: Vector2 = get_viewport().get_visible_rect().size
	if not await _press_reaches_map(_canvas_to_window(canvas * PROBE_CANVAS_CENTRE_FRACTION)):
		failures.append("a press on bare canvas never reaches _unhandled_input, so this probe proves nothing")
	# THE CLAIM: both gaps — leading (strip edge → card) and trailing (card → the chrome cluster).
	var gaps := {
		"the open strip LEADING the card": Rect2(
			strip.position, Vector2(card.position.x - strip.position.x, strip.size.y)),
		"the open strip TRAILING the card": Rect2(
			Vector2(card.end.x, strip.position.y),
			Vector2(strip.end.x - rail_span - card.end.x, strip.size.y)),
	}
	for gap_name_variant in gaps:
		var gap: Rect2 = gaps[gap_name_variant]
		if gap.size.x <= 2.0 * PROBE_RECT_INSET:
			failures.append("%s is only %.0fpx wide — there is no open map to click, so stage a narrower card" % [
				gap_name_variant, gap.size.x])
			continue
		for point in _rect_probe_points(gap):
			if not await _press_reaches_map(_canvas_to_window(point)):
				failures.append("a press at %s in %s never reached the map's input path" % [point, gap_name_variant])
				break
	# THE COMPLEMENT: each ISLAND still eats the clicks that land on its own surface, or the probe is
	# simply always true. The card is probed on its chrome RING and the chrome cluster on its bare column.
	var islands := {
		"the card's own chrome ring": _rect_ring_probe_points(card),
		"the chrome cluster": _rect_ring_probe_points(Rect2(
			Vector2(strip.end.x - _panel._rail_width(), strip.position.y),
			Vector2(_panel._rail_width(), strip.size.y))),
	}
	for island_name_variant in islands:
		for point: Vector2 in islands[island_name_variant]:
			if await _press_reaches_map(_canvas_to_window(point)):
				failures.append("a press at %s on %s fell through to the map's input path" % [
					point, island_name_variant])
				break
	# The filters that make all of that true, read back beside the behaviour.
	if _panel._root.mouse_filter != Control.MOUSE_FILTER_IGNORE:
		failures.append("PanelRoot's mouse_filter is %d, not IGNORE — the strip is not transparent to the pointer" % _panel._root.mouse_filter)
	if _panel._panel.mouse_filter != Control.MOUSE_FILTER_STOP:
		failures.append("PanelCard's mouse_filter is %d, not STOP" % _panel._panel.mouse_filter)
	if _panel._rail.mouse_filter != Control.MOUSE_FILTER_STOP:
		failures.append("ChromeRail's mouse_filter is %d, not STOP" % _panel._rail.mouse_filter)
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the open map either side of the card takes clicks (%.0fpx leading, %.0fpx trailing) and the card still eats its own" % [
			state_name, card.position.x - strip.position.x, strip.end.x - rail_span - card.end.x])
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## Where the "is the probe alive at all" press lands: the middle of the canvas, which on every state
## that runs this guard is bare ground — the strip is on an edge and the HUD's own columns are not.
const PROBE_CANVAS_CENTRE_FRACTION := 0.5
## How far inside a rect a probe point sits. Two canvas px: unambiguously within the rect after the
## canvas→window scale, small enough to still land inside a thin margin.
const PROBE_RECT_INSET := 2.0

## The RING of a rect — its corners and edge midpoints, `PROBE_RECT_INSET` inside, with the centre left
## out. That is the band an island owns itself: the card's border + content margins, the chrome
## cluster's bare column. See the guard's docstring for why the interior is deliberately not asked.
func _rect_ring_probe_points(rect: Rect2) -> Array[Vector2]:
	var points := _rect_probe_points(rect)
	points.remove_at(points.size() / 2)
	return points

## Nine points across a rect — corners, edge midpoints and centre, each pulled `PROBE_RECT_INSET`
## inside. The centre alone would never do for the OPEN-STRIP half: the gap beside the card is wide, and
## a filter that leaked only at its edges would pass a single-sample probe.
func _rect_probe_points(rect: Rect2) -> Array[Vector2]:
	var lo := rect.position + Vector2(PROBE_RECT_INSET, PROBE_RECT_INSET)
	var hi := rect.end - Vector2(PROBE_RECT_INSET, PROBE_RECT_INSET)
	var mid := rect.get_center()
	return [
		Vector2(lo.x, lo.y), Vector2(mid.x, lo.y), Vector2(hi.x, lo.y),
		Vector2(lo.x, mid.y), mid, Vector2(hi.x, mid.y),
		Vector2(lo.x, hi.y), Vector2(mid.x, hi.y), Vector2(hi.x, hi.y),
	]

## Did a left-press at this WINDOW point survive the GUI pass and reach `_unhandled_input`? That is
## exactly "would MapView have picked the hex underneath".
func _press_reaches_map(window_point: Vector2) -> bool:
	_unhandled_press_seen = false
	var approach := InputEventMouseMotion.new()
	approach.position = window_point
	get_viewport().push_input(approach)
	await get_tree().process_frame
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = window_point
	get_viewport().push_input(press)
	await get_tree().process_frame
	var seen := _unhandled_press_seen
	await _release_press(window_point)
	return seen

## Finish the click `_press_reaches_map` started, and it is not optional. A press with no release
## LATCHES `gui.mouse_focus` on whatever control took it, and Godot then routes every later press to
## that control WITHOUT re-picking — so probe 2 onwards would report probe 1's answer wherever they
## landed. The MOTION comes first so a `BaseButton` holding the press sees the pointer leave and clears
## `pressing_inside`: the release then cancels the click instead of firing it, which is what keeps a
## probe over the header's dock chooser from re-docking the panel mid-assertion.

func _release_press(window_point: Vector2) -> void:
	var park := _canvas_to_window(get_viewport().get_visible_rect().size * PROBE_CANVAS_CENTRE_FRACTION)
	var motion := InputEventMouseMotion.new()
	motion.position = park
	motion.relative = park - window_point
	get_viewport().push_input(motion)
	var release := InputEventMouseButton.new()
	release.button_index = MOUSE_BUTTON_LEFT
	release.pressed = false
	release.position = park
	get_viewport().push_input(release)
	await get_tree().process_frame

## Canvas coordinates → WINDOW coordinates, which is what `push_input` takes. The states pin
## `content_scale_size` to their canvas, and the WM can refuse the matching window size, so a control's
## own rect and an input position are not guaranteed to be in the same units.
func _canvas_to_window(canvas_point: Vector2) -> Vector2:
	var canvas: Vector2 = get_viewport().get_visible_rect().size
	if canvas.x <= 0.0 or canvas.y <= 0.0:
		return canvas_point
	var window := Vector2(get_window().size)
	return Vector2(canvas_point.x / canvas.x * window.x, canvas_point.y / canvas.y * window.y)

## How far `rect` pokes outside `bounds` on each axis (negative = comfortably inside).
func _rect_overflow(rect: Rect2, bounds: Rect2) -> Vector2:
	return Vector2(
		maxf(rect.end.x - bounds.end.x, bounds.position.x - rect.position.x),
		maxf(rect.end.y - bounds.end.y, bounds.position.y - rect.position.y))

## GUARD: a VERTICAL dock must spend NOTHING on the rail — neither its column nor its separator gutter —
## whatever width the HUD last declared; the panel forces it to 0 by EDGE, so the whole strip is the
## zones'. **Both halves are asserted**: `_rail_span()` covers the 25px gutter as well as the column, and
## the separator's own `visible` is checked because a stray hairline down the middle of a left dock is
## exactly the regression the shown-with-the-rail rule exists to prevent — and a `BoxContainer` only skips
## separation around a HIDDEN child, so the visibility IS what makes the span's zero honest.
func _assert_no_rail_width(state_name: String) -> void:
	var failures: Array[String] = []
	var span := _panel._rail_span()
	if not is_zero_approx(span):
		failures.append("still spends %.0fpx on the chrome rail" % span)
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s spends nothing on the chrome rail" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## GUARD: the clusters came home to the EXACT authored parent, child index, anchors and size flags the
## controller captured before the first reflow. A preset applied on park must not leak into the
## un-reflowed layout, and an off-by-one index would silently swap the chrome with the bar's spacer.
func _assert_chrome_home_exact(state_name: String) -> void:
	var failures: Array[String] = []
	for entry_variant in _hud._dockrow._home:
		var entry: Dictionary = entry_variant
		var cluster: Control = entry["node"]
		if cluster.get_parent() != entry["parent"]:
			failures.append("%s parent is %s, authored %s" % [
				cluster.name, cluster.get_parent().name, entry["parent"].name])
		if cluster.get_index() != int(entry["index"]):
			failures.append("%s child index is %d, authored %d" % [
				cluster.name, cluster.get_index(), int(entry["index"])])
		var anchors: Array = [cluster.anchor_left, cluster.anchor_top, cluster.anchor_right, cluster.anchor_bottom]
		if anchors != entry["anchors"]:
			failures.append("%s anchors are %s, authored %s" % [cluster.name, anchors, entry["anchors"]])
		var flags: Array = [cluster.size_flags_horizontal, cluster.size_flags_vertical]
		if flags != entry["flags"]:
			failures.append("%s size flags are %s, authored %s" % [cluster.name, flags, entry["flags"]])
	var authored_min: float = _hud._dockrow._bottom_bar_min_height
	if not is_equal_approx(_hud.bottom_bar.custom_minimum_size.y, authored_min):
		failures.append("BottomBar minimum height is %.0f, authored %.0f" % [
			_hud.bottom_bar.custom_minimum_size.y, authored_min])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s chrome restored exactly (parent/index/anchors/flags/bar minimum)" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## GUARD (FIX 4): the Next-delivery line must reach the DETAIL PANEL through the MARKER, not only the
## raw `_player_expeditions` dict. Push a hunt party through a REAL MapView (display_snapshot →
## _rebuild_unit_markers), click its hex to set `_hud._selection._selected_unit`, and assert the marker-sourced
## drawer line reads "Next delivery: ~15 food in 6 turns" (14.5 → 15). Verified to FAIL before the
## marker copy carried the three fields.
func _assert_detail_panel_delivery() -> void:
	var view: Node2D = MAP_VIEW_SCRIPT.new()
	view.visible = false   # data only — a visible map paints behind later frames (minimap gotcha)
	add_child(view)
	var tile := Vector2i(64, 11)
	var terrain: Array = []
	terrain.resize(MAP_PATH_GRID_W * MAP_PATH_GRID_H)
	terrain.fill(MAP_PATH_TERRAIN_ID)
	var party := _hunt_expedition_fixture()
	party["current_x"] = tile.x
	party["current_y"] = tile.y
	party["expedition_projected_delivery"] = 14.5
	party["expedition_eta_turns"] = 6
	view.display_snapshot({
		"grid": {"width": MAP_PATH_GRID_W, "height": MAP_PATH_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _stamp_band_ids([party]),
	})
	view.unit_selected.connect(_hud.show_unit_selection)
	view.handle_hex_click(tile.x, tile.y, MOUSE_BUTTON_LEFT)
	view.unit_selected.disconnect(_hud.show_unit_selection)
	var lines: Array = _hud._banddetail.expedition_summary_lines(_hud._selection._selected_unit)
	var want := "Next delivery: ~15 food in 6 turns"
	if lines.has(want):
		print("band_panel_preview: assert OK — detail panel (marker path) renders '%s'" % want)
	else:
		push_error("band_panel_preview: detail panel MISSING '%s' — marker path dropped the field. Got: %s" % [
			want, str(lines)])
	view.queue_free()

## GUARD: a projected-0 next-delivery forecast must disambiguate on the party's TARGET herd, and the
## Target row must carry the target's live position. Requires `_world_herds` already set to
## `_herd_fixtures()`. Drives the shared `DetailFormat.expedition_next_delivery_line` /
## `BandDetailLines.expedition_summary_lines`
## helpers directly (the same ones the strip, the drawer and the row tooltip use) and prints every
## rendered line. Verified to FAIL before the target-based branch (a lost target reading "no surplus").
func _assert_next_delivery_disambiguation() -> void:
	# (1) target FOUND in telemetry, projects 0 → "no surplus", Target row shows the herd's position.
	var lean := _lean_hunt_expedition_fixture()
	var lean_delivery := DetailFormat.expedition_next_delivery_line(
		lean, _hud._band_labor.expedition_target_herd(lean))
	var lean_target := _summary_target_line(lean)
	_check_line("no-surplus delivery", lean_delivery, DetailFormat.EXPEDITION_NEXT_DELIVERY_NO_SURPLUS)
	_check_line("no-surplus target", lean_target, "Target: Red Deer (68, 15)")
	# (2) target ABSENT from telemetry, projects 0 → "target herd lost".
	var lost := _lost_hunt_expedition_fixture()
	var lost_delivery := DetailFormat.expedition_next_delivery_line(
		lost, _hud._band_labor.expedition_target_herd(lost))
	_check_line("lost delivery", lost_delivery, DetailFormat.EXPEDITION_NEXT_DELIVERY_TARGET_LOST)
	# (3) projecting party (delivery > 0) → the ETA line, Target row shows the herd's position.
	var live := _hunt_expedition_fixture()
	var live_delivery := DetailFormat.expedition_next_delivery_line(
		live, _hud._band_labor.expedition_target_herd(live))
	var live_target := _summary_target_line(live)
	_check_line("projecting delivery", live_delivery, "Next delivery: ~14 food in 6 turns")
	_check_line("projecting target", live_target, "Target: Roe Deer (64, 11)")

## The `Target: …` line `BandDetailLines.expedition_summary_lines` emits for a party ("" if none).
func _summary_target_line(party: Dictionary) -> String:
	for line in _hud._banddetail.expedition_summary_lines(party):
		if String(line).begins_with("Target:"):
			return String(line)
	return ""

## Assert a rendered line equals what we want, printing the exact string either way.
func _check_line(label: String, got: String, want: String) -> void:
	if got == want:
		print("band_panel_preview: assert OK — %s renders '%s'" % [label, got])
	else:
		push_error("band_panel_preview: %s expected '%s' but got '%s'" % [label, want, got])

## GUARD: the row ✕ (single-party recall) must route through the CONFIRM dialog, not fire the recall
## emit immediately — mirroring "Recall all". Build a real party row, press its recall Button, and
## assert a ConfirmationDialog appeared on the HUD while `recall_expedition_requested` did NOT fire.
## Verified to FAIL with the ✕ wired straight to `_on_recall_expedition_pressed`.
func _assert_row_recall_confirms() -> void:
	var fired := [false]
	var sink := func(_payload: Dictionary) -> void: fired[0] = true
	_hud.recall_expedition_requested.connect(sink)
	var row: HBoxContainer = _hud._bandpanel._build_party_row(_hunt_expedition_fixture())
	var recall: Button = row.get_child(row.get_child_count() - 1)   # ✕ is the row's last child
	recall.pressed.emit()
	var dialog_shown := false
	for child in _hud.get_children():
		if child is ConfirmationDialog:
			dialog_shown = true
	_hud.recall_expedition_requested.disconnect(sink)
	if dialog_shown and not fired[0]:
		print("band_panel_preview: assert OK — row ✕ recall confirms first (no immediate emit)")
	else:
		push_error("band_panel_preview: row ✕ recall did NOT confirm (dialog=%s, emitted=%s)" % [
			dialog_shown, fired[0]])
	_dismiss_dialogs()
	row.queue_free()

## GUARD: whenever the WIDE shell is active, the work zone must be at least one readable board column
## (`ZONE_WORK_MIN_WIDTH`) — otherwise Hud's `_work_board_capacity` clamps to a single column too
## narrow for its own row labels, and the NARROW shell would have given the board strictly MORE room.
## That is the invariant a hand-picked `WIDE_SHELL_MIN_WIDTH` violated across a whole band of widths,
## and the recursive zone-bounds assertion cannot catch it: a CLIPPED label still sits inside its rect.
func _assert_work_zone_readable() -> void:
	if not _panel._shell_is_wide():
		return
	var work_width := _panel.work_zone_size().x
	if work_width + ZONE_BOUNDS_TOLERANCE < BandCityPanel.ZONE_WORK_MIN_WIDTH:
		push_error("band_panel_preview: wide shell with a %.0fpx work zone — under ZONE_WORK_MIN_WIDTH (%.0f)" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])
	else:
		print("band_panel_preview: assert OK — wide shell work zone %.0fpx >= %.0f" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])

## GUARD: the two threshold-probe states exist to pin WHICH shell is chosen, so state it outright —
## a frame that silently rendered the other shell would still pass every other assertion here.
func _assert_shell_is_wide(expected: bool, state_name: String) -> void:
	var actual := _panel._shell_is_wide()
	if actual != expected:
		push_error("band_panel_preview: %s expected shell wide=%s but got %s" % [
			state_name, expected, actual])
	else:
		print("band_panel_preview: assert OK — %s shell wide=%s" % [state_name, actual])

## GUARD: the PEOPLE block's three brackets must account for EVERY person in the band. They arrive
## fractional (Scalar), so `HudFormat.apportion_people` distributes the remainders by largest remainder —
## which only works if the remainders survive the trip. A marker that narrowed them with `int()`
## truncates every one to zero, and the header then undercounts against the band's own size.
func _assert_people_sum_matches_size(band: Dictionary, state_name: String) -> void:
	var raw: Array[float] = [
		float(band.get("age_children", 0.0)),
		float(band.get("age_working", 0.0)),
		float(band.get("age_elders", 0.0)),
	]
	var whole := HudFormat.apportion_people(raw)
	var total := 0
	for part in whole:
		total += part
	var size := int(band.get("size", 0))
	if total != size:
		push_error("band_panel_preview: %s PEOPLE brackets sum to %d but the band holds %d (raw %s — narrowed?)" % [
			state_name, total, size, str(raw)])
	else:
		print("band_panel_preview: assert OK — %s PEOPLE brackets sum to the band's %d people" % [state_name, size])

## **THE MAP-CLICK PATH CARRIES THE KIT, and it is this harness's THIRD instance of one bug class.**
## The marker copy is a hand-listed allowlist, so a field the decoder ships and the panel reads goes
## dark on the map path alone — `hunt_mode` first, then `working_age`/`idle_workers`, now the Minimal
## TOE's six. Clicking a band's icon on the map made its `Kit` row simply vanish
## (`DetailFormat.band_states_kit` is a bare `has()` on the spears key), and took the ⚠ zero-effective-
## attack warning silently with it (`SourceForecast.hunt_gate_model` early-returns BLANK without
## `hunter_attack`) — a missing warning looking exactly like a hunt that is fine.
##
## **BOTH HALVES, because either passes alone on a broken client.** The PAYLOAD half asks the selected
## unit — the marker copy itself — since that is where the leak is and a panel that stopped rendering
## the row for its own reasons would hide it. The RENDER half asks the frame, since a marker carrying
## six keys nothing draws is not the fix either. The rendered value is read out of the vitals
## `RichTextLabel` (the row is BBCode, which a `Label` walk cannot see at all).
func _assert_map_path_states_kit() -> void:
	var band: Dictionary = _hud._selection._selected_unit
	# **THE SIX ARE NAMED FROM THE READOUTS' OWN CONSTANTS, not from a list on MapView.** Since the
	# marker became a structural copy there IS no key list there to borrow — and borrowing one would
	# have asserted that the copy copies what the copy copies. These are the keys `DetailFormat` and
	# `SourceForecast` actually read, so the claim is "what the panel asks for arrived".
	var missing: Array[String] = []
	for toe_key in [
		DetailFormat.KIT_DURABILITY_KEY_SPEARS, DetailFormat.KIT_DURABILITY_KEY_SLED,
		DetailFormat.KIT_DURABILITY_KEY_BASKETS, DetailFormat.KIT_TIER_KEY_HUNT_CARRY,
		DetailFormat.KIT_TIER_KEY_FORAGE_CARRY, SourceForecast.BAND_HUNTER_ATTACK_KEY,
	]:
		if not band.has(toe_key):
			missing.append(String(toe_key))
	_assert_band_panel("the map-click payload carries the Minimal TOE's six (missing %s)" % str(missing),
		missing.is_empty())
	# …and the payload is the WHOLE cohort, which is the invariant that stops a fourth leak: the marker
	# is `entry.duplicate()` plus declared stamps, so every key the fixture cohort carries is here.
	# `marker_field_guard` owns the exhaustive form of this against a realistic cohort; this is the
	# same claim at the END of the chain the report came from — map click → marker → selection → panel.
	var dropped: Array[String] = []
	for source_key in _kit_band_fixture():
		if not band.has(source_key):
			dropped.append(String(source_key))
	_assert_band_panel("…and the map-click payload is the WHOLE cohort (dropped %s)" % str(dropped),
		dropped.is_empty())
	# …and they arrive as the FLOATS the wire carries. Presence cannot see an `int()` narrowing, which
	# is the second bug class `marker_field_guard` exists for and which is live-visible here: the
	# marker IS the selection payload for a band clicked on the map.
	var spears := float(_kit_band_fixture().get(DetailFormat.KIT_DURABILITY_KEY_SPEARS, 0.0))
	_assert_band_panel("…un-narrowed, spears reading %s against the fixture's %s"
			% [str(band.get(DetailFormat.KIT_DURABILITY_KEY_SPEARS, 0.0)), str(spears)],
		is_equal_approx(float(band.get(DetailFormat.KIT_DURABILITY_KEY_SPEARS, 0.0)), spears))
	# The RENDER half — the row the report was actually about. The needle carries the VALUE as well as
	# the label, so it cannot be satisfied by a row that rendered the kit's name over a defaulted
	# reading; and it is composed from the FIXTURE's number rather than asked of `kit_condition_face`,
	# which would re-derive the expectation through the code under test. **`BAND_KIT_ROW_PREFIX` is NOT
	# what appears on screen** — the vitals rows are DISCLOSURES, so the row's own label is the caret's
	# (`Kit ▸`) and the prefix is consumed by that wrapping.
	var want := "%s %s" % [DetailFormat.KIT_LABEL_SPEARS,
		String.num(spears, DetailFormat.KIT_CONDITION_DECIMALS)]
	_assert_band_panel("…so the Kit row renders on the map path — \"%s\"" % want,
		_rich_text_containing(_panel, want) != "")

## GUARD: the zone model is NO-SCROLL by construction — a ScrollContainer anywhere in the panel would
## silently reintroduce the content-dependent sizing the rework removed.
func _assert_no_scroll_containers() -> void:
	var found := _find_scroll_container(_panel)
	if found != null:
		push_error("band_panel_preview: ScrollContainer in the panel at %s — the zones must not scroll" % found.get_path())
	else:
		print("band_panel_preview: assert OK — no ScrollContainer in the panel")

func _find_scroll_container(node: Node) -> Node:
	if node is ScrollContainer:
		return node
	for child in node.get_children():
		var found := _find_scroll_container(child)
		if found != null:
			return found
	return null

## GUARD: a zone's content must FIT — not merely sit inside its host's rect. The zone hosts clip, so
## content the box cannot hold still reports a rect within bounds and passes `_assert_zones_within_bounds`
## while being silently sliced off the frame (the WORKFORCE key row cut mid-glyph, the role cards gone).
## Containment is not completeness: the invariant that matters is that the zone box is at least as tall
## as the content's own combined minimum size.
func _assert_zone_content_fits() -> void:
	var failures: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		_collect_zone_content_shortfall(host, host, failures)
	if failures.is_empty():
		print("band_panel_preview: assert OK — every zone's content fits its zone box (%s)" % _current_state)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [_current_state, failure])

## Walk a zone host looking for content the BOX cannot hold. The zone content roots are plain
## `Control` wrappers (`HudWidgets.wrap_zone`) that report NO minimum size, so the measurable thing is the
## column inside them — hence the recursion past every zero-minimum wrapper. A control that DOES
## report a minimum height is measured from where it sits (its top, relative to the zone) and then
## not descended into: its own minimum already accounts for its children.
func _collect_zone_content_shortfall(node: Node, host: Control, failures: Array[String]) -> void:
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		var needed := content.get_combined_minimum_size().y
		if needed <= 0.0:
			_collect_zone_content_shortfall(content, host, failures)
			continue
		var top := content.global_position.y - host.global_position.y
		var box := host.size.y
		if top + needed > box + ZONE_BOUNDS_TOLERANCE:
			failures.append("zone %s: %s (%s) needs %.0fpx from y=%.0f but the box is only %.0fpx (short by %.0f)" % [
				host.name, content.name, content.get_class(), needed, top, box, top + needed - box])

## GUARD: nothing a zone renders may fall outside the zone rect it was given. Checked RECURSIVELY —
## the top-level content is anchored full-rect and so always "fits", while the thing that actually
## overflows is a board row off the bottom of the column. The hosts clip, so an overflow is invisible
## in the frame; this is the only thing that catches it.
## The SHORT band-zone tier must drop the Trade row (`BandPanelController._build_vitals_label` passes
## `compact`). Asserted rather than eyeballed: a dropped row and a row clipped off the bottom of a
## `clip_contents` zone are the SAME PICTURE, so only a text read can tell them apart. It reads the
## rendered vitals BBCode back out of the live label, which is also what makes it fail if the gate is
## removed — the row would be present in the text while still invisible in the PNG.
##
## **MATCH BARE KEYS, NOT `"Trade:"`.** `DetailFormat._split_kv` splits each `Key: value` line into a
## BBCode TABLE row and drops the `": "` separator, so the colon is never in the rendered text.
func _assert_trade_row_absent_in_short_tier() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: short-tier trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	# The Food row proves the vitals label is actually populated — without it, "no Trade row" would
	# pass vacuously on an empty label.
	if not text.contains("Food"):
		push_error("band_panel_preview: short-tier trade assert — vitals label has no Food row (vacuous)")
		return
	if text.contains("Trade"):
		push_error("band_panel_preview: SHORT tier still renders the Trade row — the compact gate is off")
		return
	print("band_panel_preview: assert OK — SHORT tier drops the Trade row (Food row still present)")

## The hosts the band zone can render into — its own zone box in the WIDE shell, and the single
## swapped host in the NARROW one. The tier note is appended to their extent lines alone.
const BAND_ZONE_HOST_NAMES := ["Zone_band", "NarrowZoneHost"]
const BAND_ZONE_TIER_NOTE_FORMAT := " [%s tier]"
## The three tiers by name, indexed by `HudWorkVocab.BAND_ZONE_TIER_*` (SHORT 0, COMPACT 1, TALL 2).
const BAND_ZONE_TIER_NAMES := ["SHORT", "COMPACT", "TALL"]

## The canvas height that lands the LEFT dock's band zone in the COMPACT tier. The narrow shell's zone
## box is the canvas minus ~95px of chrome, and COMPACT is `[BAND_ZONE_CHART_MIN_HEIGHT,
## BAND_ZONE_TALL_MIN_HEIGHT)` = [340, 420) — so 480 gives a 385px box, mid-band rather than on either
## edge, where a few pixels of chrome drift cannot silently move the probe into a neighbouring tier.
const COMPACT_TIER_PROBE_HEIGHT := 480

## Which content tier the band zone is rendering at RIGHT NOW — read off the controller rather than
## re-derived from the zone height, so the reported tier is the one that actually built the rows.
func _band_zone_tier_name() -> String:
	var tier: int = _hud._bandpanel._band_zone_tier
	if tier < 0 or tier >= BAND_ZONE_TIER_NAMES.size():
		return "?"
	return String(BAND_ZONE_TIER_NAMES[tier])

## MEASUREMENT (not an assertion — `_assert_zone_content_fits` is the assertion): print how tall each
## zone's content actually came out against the box it was given, so a state that PASSES still says by
## how much. A near-miss and a comfortable fit are the same green line otherwise, and the whole point
## of the worst-case state is knowing what the margin is.
## Uses the SAME walk `_collect_zone_content_shortfall` does — the deepest `top + needed` any measurable
## control reaches — so the number printed here and the number asserted on cannot come from two reads.
func _report_zone_content_extent(state_name: String) -> void:
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		var extent := _zone_content_extent(host, host)
		if extent <= 0.0:
			continue
		print("band_panel_preview: %s — zone %s content %.0fpx of a %.0fpx box (%.0f spare)%s" % [
			state_name, host.name, extent, host.size.y, host.size.y - extent,
			# The band zone's TIER, beside its extent: the SHORT tier renders two fewer rows than the
			# TALL one (Trade dropped, Fodder and Growth merged), so an extent quoted without it is a
			# number whose content nobody can reconstruct.
			BAND_ZONE_TIER_NOTE_FORMAT % _band_zone_tier_name() \
				if BAND_ZONE_HOST_NAMES.has(String(host.name)) else ""])

## The deepest point any measurable control in this zone reaches, relative to the zone's own top.
func _zone_content_extent(node: Node, host: Control) -> float:
	var deepest := 0.0
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		var needed := content.get_combined_minimum_size().y
		if needed <= 0.0:
			deepest = maxf(deepest, _zone_content_extent(content, host))
			continue
		deepest = maxf(deepest, content.global_position.y - host.global_position.y + needed)
	return deepest

## GUARD: the SHORT tier merges the hay larder onto the Food line (`BandDetailLines`'
## `BAND_FOOD_HAY_CLAUSE_FORMAT`) to save a row — and the vitals label is `AUTOWRAP_WORD`, so a merged
## line too wide for the band zone WRAPS and costs back the very row the merge bought. A wrap is also
## invisible in the frame: two lines of a rendered vitals block look exactly like two rows.
##
## Measured rather than eyeballed: the Food row's natural (unwrapped) run, in the label's OWN font at
## its OWN size, against the width the label was actually given, plus the gutter the `[table=2]`
## spends between its key and value cells — so the figure is the whole ROW rather than one cell.
##
## **THE ROW IS CUT OUT OF THE PARSED TEXT BY THE NEXT ROW'S KEY, not by a newline.** `[table]` rows
## carry NO line break into `get_parsed_text()` — every row of the vitals block comes back concatenated
## into one string (measured: the three-row worst case reads as a single 916px run) — so a per-line
## split measures the whole block and reports a wrap on a label that fits comfortably.
func _assert_merged_food_row_fits() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: merged-food-row assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains(MERGED_FOOD_HAY_NEEDLE):
		push_error("band_panel_preview: the SHORT tier's Food row carries no hay clause — the merge is off (got: %s)" % text)
		return
	if text.contains(FODDER_ROW_NEEDLE):
		push_error("band_panel_preview: the SHORT tier still renders a standalone Fodder row beside the merged Food line")
		return
	# **THE ROW IS BOUNDED BY THE ROW THAT FOLLOWS IT, AND THAT ROW IS NOW `Kit`.** This read to
	# `Morale` while Food and Morale were adjacent; the Kit row landed between them and the cut then
	# measured TWO rows as one, reporting a 624px wrap on a line that fits comfortably. A bound naming
	# the row that actually follows is the only kind that survives an insertion, so it takes whichever
	# of the candidates comes FIRST rather than one fixed name.
	var food_run := _vitals_run(text, HudDisclosureVocab.DETAIL_ROW_FOOD,
		[HudDisclosureVocab.DETAIL_ROW_KIT, HudDisclosureVocab.DETAIL_ROW_MORALE])
	if food_run == "":
		push_error("band_panel_preview: merged-food-row assert cannot find the Food row (got: %s)" % text)
		return
	_assert_vitals_run_fits("merged Food", food_run, vitals)

## GUARD: the SHORT tier's OTHER merge — Growth joined onto the Morale line to pay for the `Kit` row
## every live band states (`BandDetailLines.BAND_MORALE_GROWTH_CLAUSE_FORMAT`). Same trap and the same
## measurement as the Food row above: the label is `AUTOWRAP_WORD`, so a merged line too wide for the
## column WRAPS and costs back the very row the merge bought — a fix that measures as no fix, with
## nothing failing. **The bounds assertion cannot see this**: a wrapped line still sits inside the zone
## rect, so `_assert_zone_content_fits` passes and the frame is silently one row taller.
##
## It is also what makes the DROPPED morale cause clause load-bearing rather than cosmetic: put the
## cause back at this tier (`— harsh terrain (Karst Cavern Mouth)`) and this is the assertion that
## fails, naming the overflow.
func _assert_merged_morale_growth_fits() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: merged-morale-row assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains(HudDisclosureVocab.DETAIL_ROW_GROWTH):
		push_error("band_panel_preview: the SHORT tier's Morale line carries no Growth clause — the merge is off (got: %s)" % text)
		return
	# Nothing follows Morale in the dock's vitals block (the Position row is the drawer host's), so the
	# run reaches the end of the label — stated as an EMPTY bound list rather than left implicit.
	var morale_run := _vitals_run(text, HudDisclosureVocab.DETAIL_ROW_MORALE, [])
	if morale_run == "":
		push_error("band_panel_preview: merged-morale-row assert cannot find the Morale row (got: %s)" % text)
		return
	_assert_vitals_run_fits("merged Morale+Growth", morale_run, vitals)

## GUARD: **the merge is the SHORT tier's layout and nobody else's.** Morale and Growth stay separate
## rows at TALL and COMPACT, with the morale cause clause intact — so a merge leaking upward would
## quietly cost every tier a reading it has the room for.
##
## Structural, off the BBCode rather than the parsed text: `detail_bbcode` opens every table row with
## `[cell]`, so a standalone Growth row's clickable run is preceded by one while the merged clause's is
## preceded by the clause SEPARATOR. The parsed text strips both, which is why the visible half — the
## `of normal` anchor, which only a standalone row spends the width on — is asserted beside it rather
## than instead of it.
func _assert_growth_row_not_merged(state_name: String) -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: growth-row tier assert found no vitals label (%s)" % state_name)
		return
	var merged_needle := BandDetailLines.BAND_MORALE_GROWTH_CLAUSE_SEPARATOR \
		+ DetailFormat.DISCLOSURE_URL_OPEN
	if vitals.text.contains(merged_needle):
		push_error("band_panel_preview: %s merged Growth onto the Morale line — that is the SHORT tier's layout only" % state_name)
		return
	if not vitals.get_parsed_text().contains(DetailFormat.GROWTH_ROW_ANCHOR_SUFFIX):
		push_error("band_panel_preview: %s dropped the Growth row's `of normal` anchor — the SHORT tier's short form leaked up" % state_name)
		return
	print("band_panel_preview: assert OK — %s keeps Growth as its own row, anchor intact" % state_name)

## One vitals ROW cut out of the parsed block. **`[table]` rows carry NO line break into
## `get_parsed_text()`** — the whole block comes back concatenated into one string — so a row is cut by
## the KEY of whichever row follows it, and an empty `bounds` list means "this row runs to the end of
## the block". Returns "" when `key` is not in the text at all.
func _vitals_run(text: String, key: String, bounds: Array) -> String:
	var start := text.find(key)
	if start < 0:
		return ""
	var stop := text.length()
	for bound_variant in bounds:
		var at := text.find(String(bound_variant), start + key.length())
		if at > start:
			stop = mini(stop, at)
	return text.substr(start, stop - start)

## Measure one vitals row's NATURAL (unwrapped) run against the width the label was actually given — in
## the label's OWN font at its OWN size, plus the gutter the `[table=2]` spends between its key and
## value cells, so the figure is the whole ROW rather than one cell.
func _assert_vitals_run_fits(label: String, run: String, vitals: RichTextLabel) -> void:
	var font := vitals.get_theme_font(VITALS_FONT_THEME_KEY)
	var font_size := vitals.get_theme_font_size(VITALS_FONT_SIZE_THEME_KEY)
	var table_gap := float(vitals.get_theme_constant(VITALS_TABLE_SEPARATION_THEME_KEY))
	var needed: float = font.get_string_size(run, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x + table_gap
	var available := vitals.size.x
	print("band_panel_preview: %s row — \"%s\" measures %.0fpx of a %.0fpx column" % [
		label, run, needed, available])
	if needed > available:
		push_error("band_panel_preview: the %s line WRAPS — %.0fpx of run in a %.0fpx column" % [
			label, needed, available])
	else:
		print("band_panel_preview: assert OK — the %s line fits its column (%.0f spare)" % [
			label, available - needed])

## **THE FORAGE-TRADE REGRESSION.** A forage source ships `realized_trade_yield == 0` (the documented
## not-yet-projected sentinel) beside a real `trade_yield`, and the decoder always inserts the key — so
## a fallback spelled `has("realized_trade_yield")` silently drops every cash crop and the row reads
## `+0.00` on a band visibly selling flax. The fixture's patch pays 0.04 and its deer pays 0.04, so the
## headline must read +0.08 and the breakdown must carry BOTH categories. A PNG cannot carry this — the
## broken and the fixed frame differ by two characters — so it is asserted on both halves: the total
## proves the forage contribution landed, the Gathered row proves it landed on the right category.
func _assert_forage_trade_counted() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: forage-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("+0.08"):
		push_error("band_panel_preview: Trade must read +0.08 (forage 0.04 + hunt 0.04) — got: %s" % text)
		return
	# The band-local STOCK, read off `stores.trade_goods` the way the Food row reads the larder.
	# Matched as the VALUE cell's own run (`12.0 · +0.08`) rather than `Trade 12.0`: the KV formatter
	# splits the row into table cells and the key cell carries the disclosure caret, so the two are never
	# adjacent in the parsed text. ONE DECIMAL — the stock is a float on screen because the sim
	# accumulates sub-unit trade income; the exact rendered value is what this pins.
	if not text.contains("12.0 · +0.08"):
		push_error("band_panel_preview: Trade row does not carry the band's stock of 12 — got: %s" % text)
		return
	var rows := _disclosure_rows(BAND_FIXTURE_DISCLOSURE_TRADE)
	var joined := "\n".join(rows)
	if not joined.contains(DetailFormat.FOOD_LABEL_GATHERED):
		push_error("band_panel_preview: the Trade breakdown has no Gathered row — the forage source's trade was dropped (rows: %s)" % joined)
		return
	if not joined.contains(DetailFormat.FOOD_LABEL_HUNTED):
		push_error("band_panel_preview: the Trade breakdown has no Hunted row (rows: %s)" % joined)
		return
	print("band_panel_preview: assert OK — a forage source's trade counts (Trade +0.08, Gathered + Hunted)")

## The breakdown rows stashed for a disclosure key, read back the way the popover reads them.
func _disclosure_rows(key: String) -> Array[String]:
	var payloads: Dictionary = _hud._disclosures._breakdown_payloads
	var rows: Array[String] = []
	var stashed: Variant = payloads.get(key, [])
	if stashed is Array:
		for row in (stashed as Array):
			rows.append(String(row))
	return rows

## The zero case: the Trade row must be PRESENT and read a zero rate. Asserted because "absent" and
## "present but zero" are one glance apart in a PNG and the difference is the whole playtest report.
func _assert_trade_row_reads_zero() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: zero-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("Trade"):
		push_error("band_panel_preview: a band earning no trade dropped its Trade row — it must read zero")
		return
	# `format_yield` writes a signed magnitude, so a zero rate renders "+0.00". Matching the NUMBER
	# rather than the row keeps this from passing on an earning band that merely has a Trade row.
	if not text.contains("+0.00"):
		push_error("band_panel_preview: zero-trade band's Trade row does not read +0.00 — got: %s" % text)
		return
	print("band_panel_preview: assert OK — a band earning no trade still shows Trade, reading +0.00")

func _find_vitals_label(node: Node) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).get_parsed_text().contains("Morale"):
		return node as RichTextLabel
	for child in node.get_children():
		var found := _find_vitals_label(child)
		if found != null:
			return found
	return null

func _assert_zones_within_bounds() -> void:
	var failures: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		_collect_zone_overflow(host, host.get_global_rect(), failures)
	if failures.is_empty():
		print("band_panel_preview: assert OK — every zone renders inside its zone rect")
		return
	for failure in failures:
		push_error("band_panel_preview: %s" % failure)

func _collect_zone_overflow(node: Node, bounds: Rect2, failures: Array[String]) -> void:
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		var rect := content.get_global_rect()
		# Zero-sized spacers/separators report a degenerate rect; only real content can overflow.
		if rect.size.x > 0.0 and rect.size.y > 0.0:
			var over_x: float = rect.end.x - bounds.end.x
			var over_y: float = rect.end.y - bounds.end.y
			if over_x > ZONE_BOUNDS_TOLERANCE or over_y > ZONE_BOUNDS_TOLERANCE:
				failures.append("%s (%s) overflows its zone by (%.1f, %.1f)" % [
					content.name, content.get_class(), maxf(over_x, 0.0), maxf(over_y, 0.0)])
				continue   # one report per subtree — its children overflow by construction
		_collect_zone_overflow(content, bounds, failures)

## GUARD: the dock hunt sheet's floor CHART is gated on the zone having room — present at TALL, absent
## at SHORT, where the parties zone is height-capped and clips. **Both halves are asserted**: a gate
## that never fires and a gate stuck on are both green to the bounds assertion, since a clipped chart
## still sits inside the zone rect.
func _assert_hunt_sheet_chart(want: bool, state_name: String) -> void:
	var chart := _find_meta_control(_panel, HudWidgets.FLOOR_CHART_META)
	var tier := _band_zone_tier_name()
	if want and chart == null:
		push_error("band_panel_preview: %s (%s tier) renders NO floor chart — the tier gate is stuck off" % [
			state_name, tier])
		return
	if not want and chart != null:
		push_error("band_panel_preview: %s (%s tier) renders a floor chart — the tier gate is stuck on" % [
			state_name, tier])
		return
	print("band_panel_preview: assert OK — %s (%s tier) %s the floor chart" % [
		state_name, tier, "carries" if want else "keeps out"])

## MEASUREMENT: the compose sheet's floor PICKER and its CHART against the column they render in.
## Both are widgets the herd drawer sized in a ~400px sheet and the dock hosts in a ~354px zone, and
## both fail SILENTLY when they do not fit — the picker WRAPS onto a second row (the reason the zone
## once clamped itself to 2 columns) and the chart raises the zone's minimum width past its host,
## where it is clipped. A green bounds assertion says neither happened; only the numbers say by how
## much, which is what decides whether a shortened face was enough.
func _report_compose_widths(state_name: String) -> void:
	var picker := _find_meta_control(_panel, HudWidgets.POLICY_RUNG_META)
	# The rung's own meta rides the BUTTON; the grid that lays the three of them out is its
	# grandparent (button → cell `MarginContainer` → grid), and the GRID is what can wrap.
	var grid: Control = picker.get_parent().get_parent() as Control if picker != null else null
	if grid != null:
		print("band_panel_preview: %s — floor picker grid needs %.0fpx of a %.0fpx column (%d columns, %d rungs)" % [
			state_name, grid.get_combined_minimum_size().x, grid.size.x,
			(grid as GridContainer).columns if grid is GridContainer else -1,
			grid.get_child_count()])
	var chart := _find_meta_control(_panel, HudWidgets.FLOOR_CHART_META)
	if chart == null:
		print("band_panel_preview: %s — no floor chart in this zone" % state_name)
		return
	print("band_panel_preview: %s — floor chart needs %.0f x %.0fpx, drawn at %.0f x %.0fpx" % [
		state_name, chart.get_combined_minimum_size().x, chart.get_combined_minimum_size().y,
		chart.size.x, chart.size.y])

## The panel's fixed-size zone hosts (BandCityPanel names them `Zone_<key>` / `NarrowZoneHost`).
func _find_zone_hosts(node: Node) -> Array:
	var hosts: Array = []
	if String(node.name).begins_with("Zone_") or node.name == "NarrowZoneHost":
		hosts.append(node)
	for child in node.get_children():
		hosts.append_array(_find_zone_hosts(child))
	return hosts

## Two Hunt rows on one band, told apart by the rung they STAND on: a part-built pen (an INVESTMENT
## rung, which the work inspector's four-extractive-rung picker cannot highlight) and an ordinary
## Sustain take (the control). Same band, same zone, so the two frames differ in exactly the rung.
## The forage jump must leave the LAND as the lit subject, even on a hex whose roster also holds a
## band (the auto-pick's preference, and what it used to hand back instead).
func _assert_forage_jump_names_land() -> void:
	var subjects: Array = []
	_hud._bandpanel.roster_occupant_selected.connect(
		func(kind: String, _id: Variant) -> void: subjects.append(kind), CONNECT_ONE_SHOT)
	_hud._bandpanel.focus_labor_source(71, 18)
	_assert_band_panel("forage jump — the row names the LAND, not the hex's auto-picked occupant",
		subjects == [HudSelectionState.SUBJECT_LAND])
	_assert_band_panel("forage jump — the land is the lit subject afterwards",
		_hud._selection.subject() == HudSelectionState.SUBJECT_LAND)

## Pass/fail reporting for the rung-ready assertions, in this harness's `push_error` idiom so a
## regression fails loudly in the run log rather than waiting to be noticed in a thumbnail.
## A control carrying `meta`, found by IDENTITY rather than by face — the rule this harness already
## follows for policy rungs (`HudWidgets.POLICY_RUNG_META`). The fill-target control is a checkbox
## whose own text FLIPS between its two states, so a text match would find it in one state and pass
## vacuously in the other.
func _find_meta_control(node: Node, meta: String) -> Control:
	if node is Control and (node as Control).has_meta(meta):
		return node as Control
	for child in node.get_children():
		var found := _find_meta_control(child, meta)
		if found != null:
			return found
	return null

## Does any Label under `node` carry `text`? For the bound clause, which is a plain
## `HudWidgets.alloc_hint_label` sentence — the ONE case where "this text appears somewhere" IS the
## claim, and it is paired above with a positive identity check so neither can pass alone.
func _has_label_containing(node: Node, text: String) -> bool:
	if node is Label and (node as Label).text.contains(text):
		return true
	for child in node.get_children():
		if _has_label_containing(child, text):
			return true
	return false

func _assert_band_panel(label: String, ok: bool) -> void:
	if ok:
		print("band_panel_preview: PASS — ", label)
	else:
		push_error("band_panel_preview: FAIL — %s" % label)

## THE BOARD MUST NOT RE-ORDER UNDER THE PLAYER'S OWN EDIT (issue #460), and both comparators must be
## TOTAL ORDERS. Neither claim is visible in a PNG — a re-sorted board is a perfectly plausible board —
## so the sorts are driven directly, over models shaped like `_work_source_models`' output.
##
## Four claims, and the second is what stops the first being satisfied by a comparator that ignores
## `rate` altogether:
##   1. under the DEFAULT sort a worker step (a `rate` change) leaves the key order identical;
##   2. under `WORK_SORT_YIELD` the SAME step DOES reorder — the opt-in sort still ranks live;
##   3. both sorts answer the same key sequence from two different starting permutations, which is the
##      only thing that can see a missing `key` tiebreak (`sort_custom` is not stable in Godot);
##   4. the DEFAULT sort groups by KIND — every `forage` row above every `hunt` row — which the label
##      order alone does NOT give, since a managed plant row reads "Tend (…)" and sorts after "Hunt".
##      Asserted on `kind`, never on the label: testing the label would re-enact the assumption that
##      the prefix identifies the kind, which is exactly what is false.
func _assert_work_sort_stable() -> void:
	var controller = _hud._bandpanel
	# THE FIRST CLAIM IS ABOUT THE LIVE DEFAULT, so it does NOT set the sort — nothing in this harness
	# has picked one, so `_work_sort` is exactly what a fresh session boots with. Pinning it to
	# `WORK_SORT_NAME` here would assert that the name sort is stable and say nothing about which sort
	# the board actually uses, which is the whole of issue #460.
	var restore_sort: StringName = controller._work_sort
	var models := _work_sort_fixture_models()
	var name_before := _sorted_work_keys(controller, models)
	_bump_work_sort_fixture_rate(models)
	var name_after := _sorted_work_keys(controller, models)
	_assert_band_panel("work sort — a worker step leaves the DEFAULT (`%s`) order untouched (%s)"
		% [String(restore_sort), ", ".join(name_after)], name_after == name_before)
	# 4 — still on the live default: the kind blocks the filter chips name must be the board's blocks.
	var kinds := _sorted_work_kinds(controller, _work_sort_fixture_models())
	var last_forage := kinds.rfind(SourceForecast.LABOR_KIND_FORAGE)
	var first_hunt := kinds.find(SourceForecast.LABOR_KIND_HUNT)
	_assert_band_panel("work sort — the DEFAULT (`%s`) puts every forage row above every hunt row (%s)"
		% [String(restore_sort), ", ".join(kinds)], last_forage < first_hunt)
	# 2 — the counter-check: the opt-in yield sort must genuinely track the same edit.
	controller._work_sort = HudWorkVocab.WORK_SORT_YIELD
	var yield_models := _work_sort_fixture_models()
	var yield_before := _sorted_work_keys(controller, yield_models)
	_bump_work_sort_fixture_rate(yield_models)
	var yield_after := _sorted_work_keys(controller, yield_models)
	_assert_band_panel("work sort — the same worker step DOES re-rank `Sort by yield` (%s → %s)"
		% [", ".join(yield_before), ", ".join(yield_after)], yield_after != yield_before)
	# 3 — total order, both modes, from two different starting permutations.
	for sort in HudWorkVocab.WORK_SORTS:
		controller._work_sort = sort
		var forward := _sorted_work_keys(controller, _work_sort_fixture_models())
		var reversed_models := _work_sort_fixture_models()
		reversed_models.reverse()
		var backward := _sorted_work_keys(controller, reversed_models)
		_assert_band_panel("work sort — `%s` is a total order (same keys from a reversed input: %s)"
			% [String(sort), ", ".join(forward)], forward == backward)
	controller._work_sort = restore_sort

## The sort fixture, carrying BOTH reachable ties: two herds sharing a label (`WORK_ROW_HUNT_FORMAT`
## renders one string per species, so two Wild Boar herds collide) and two sources sharing a rate.
## Only the keys the two comparators read are populated — this exercises the sort, not the board.
##
## The TEND row is what makes claim 4 bite: its label is built from `WORK_ROW_TEND_FORMAT`, so it
## sorts alphabetically AFTER every "Hunt …" row while its `kind` is still `forage`. Composing the
## label from the format const rather than a literal means renaming the format cannot silently leave
## this case uncovered.
func _work_sort_fixture_models() -> Array:
	return [
		{"key": "hunt:boar_b", "label": "Hunt Wild Boar", "kind": "hunt",
			"rate": 0.40, "trade_rate": 0.10},
		{"key": WORK_SORT_STEPPED_KEY, "label": "Hunt Wild Boar", "kind": "hunt",
			"rate": WORK_SORT_TIED_RATE, "trade_rate": 0.10},
		{"key": "forage:12,7", "label": "Forage (12, 7)", "kind": "forage",
			"rate": WORK_SORT_TIED_RATE, "trade_rate": 0.0},
		{"key": "forage:3,9", "label": "Forage (3, 9)", "kind": "forage",
			"rate": 0.60, "trade_rate": 0.0},
		{"key": "forage:8,4", "kind": "forage",
			"label": HudWorkVocab.WORK_ROW_TEND_FORMAT % [WORK_SORT_TEND_TILE.x, WORK_SORT_TEND_TILE.y],
			"rate": 0.30, "trade_rate": 0.0},
		{"key": "hunt:wolf", "label": "Hunt Grey Wolf", "kind": "hunt",
			"rate": 0.0, "trade_rate": 0.22},
	]

## The tile the fixture's managed plant row sits on — only its label is read, so any coordinate does.
const WORK_SORT_TEND_TILE := Vector2i(8, 4)

## The source whose crew the assertion "steps", and the rate two sources start tied on. The stepped
## source is one of the tied pair, so the step both breaks a tie and moves the row to the TOP of the
## yield order — an edit the name sort must ignore and the yield sort must not.
const WORK_SORT_STEPPED_KEY := "hunt:boar_a"
const WORK_SORT_TIED_RATE := 0.25
## Where the stepped source lands after its "+" press — above every other row's rate.
const WORK_SORT_STEPPED_RATE := 0.90

func _bump_work_sort_fixture_rate(models: Array) -> void:
	for m in models:
		if String((m as Dictionary).get("key", "")) == WORK_SORT_STEPPED_KEY:
			(m as Dictionary)["rate"] = WORK_SORT_STEPPED_RATE

## Sort a COPY through the controller's own comparator and report the resulting key order.
func _sorted_work_keys(controller, models: Array) -> Array:
	var copy := models.duplicate()
	controller._sort_work_models(copy)
	var keys: Array = []
	for m in copy:
		keys.append(String((m as Dictionary).get("key", "")))
	return keys

## The same, reporting each row's `kind` instead of its key — the field the filter chips select on.
func _sorted_work_kinds(controller, models: Array) -> Array:
	var copy := models.duplicate()
	controller._sort_work_models(copy)
	var kinds: Array = []
	for m in copy:
		kinds.append(String((m as Dictionary).get("kind", "")))
	return kinds

## The `⋯` menu must SAY which sort is active — without the mark the board's order is unexplained, the
## menu offering two sorts and stating neither. Asserted on the popup rather than in a frame: the popup
## is a Window and never renders into the capture.
func _assert_work_menu_marks_active_sort(state_name: String) -> void:
	var popup := _find_work_menu_popup(_panel)
	if popup == null:
		_assert_band_panel("%s — the work zone's `⋯` menu was not found" % state_name, false)
		return
	var checked: Array = []
	for i in range(popup.item_count):
		if popup.is_item_checked(i):
			checked.append(popup.get_item_text(i))
	var want := HudWorkVocab.WORK_MENU_SORT_NAME if _hud._bandpanel._work_sort == HudWorkVocab.WORK_SORT_NAME \
		else HudWorkVocab.WORK_MENU_SORT_YIELD
	_assert_band_panel("%s — the work menu marks exactly the active sort (checked: %s, active: %s)"
		% [state_name, str(checked), want], checked == [want])

## The work zone's section menu, found by the SORT ENTRY its popup carries — the parties zone builds a
## `⋯` menu too, and both are plain `MenuButton`s, so the node type alone cannot tell them apart.
func _find_work_menu_popup(node: Node) -> PopupMenu:
	if node is MenuButton:
		var popup: PopupMenu = (node as MenuButton).get_popup()
		for i in range(popup.item_count):
			if popup.get_item_text(i) == HudWorkVocab.WORK_MENU_SORT_NAME:
				return popup
	for child in node.get_children():
		var found := _find_work_menu_popup(child)
		if found != null:
			return found
	return null

## The rung-ready board fixture: three sources, exactly one of each answer the mark can give.
func _ready_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 940
	band["id"] = "Band 12"
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "workers_needed": 3, "floor": 0.5,
			"target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"fauna_id": "ready_tamed", "target_x": 70, "target_y": 17,
			"actual_yield": 0.30, "sustainable_yield": 0.30},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"fauna_id": "ready_never", "target_x": 69, "target_y": 19,
			"actual_yield": 0.20, "sustainable_yield": 0.20},
	]
	return band

## A TENDED patch on willing ground → its next rung is Sow.
func _ready_patch_fixtures() -> Array:
	return [{
		"x": 71, "y": 18, "ecology_phase": "thriving",
		"is_cultivated": true, "is_field": false, "sow_site_refusal": "",
		"composition": [{"species": "wild_wheat", "display_name": "Wild Wheat",
			"share": 1.0, "can_cultivate": true, "can_sow": true}],
	}]

## One fully tamed "pen"-ceiling herd (→ Corral) and one "wild"-ceiling herd that can never climb —
## the control that proves the mark is selective rather than decorative.
func _ready_herd_fixtures() -> Array:
	return [
		{"id": "ready_tamed", "species": "Aurochs", "x": 70, "y": 17,
			"population": 210, "ecology_phase": "thriving", "huntable": true,
			"domestication": 1.0, "husbandry_ceiling": "pen", "per_worker_yield": 0.15,
			"hunt_policy_ceilings": {"sustain": 0.30, "surplus": 0.90, "deplete": 1.40,
				"eradicate": 2.00, "corral": 0.70}},
		{"id": "ready_never", "species": "Roe Deer", "x": 69, "y": 19,
			"population": 90, "ecology_phase": "thriving", "huntable": true,
			"domestication": 0.0, "husbandry_ceiling": "wild", "per_worker_yield": 0.10,
			"hunt_policy_ceilings": {"sustain": 0.20, "surplus": 0.60, "deplete": 0.90,
				"eradicate": 1.40}},
	]

## The mark is SELECTIVE — two of the three rows offer a rung, the wild-ceiling herd none. Asserted
## rather than eyeballed: three chevrons and one chevron look similar in a thumbnail, and "the mark
## renders" is a much weaker claim than "the mark renders where it should and nowhere else".
func _assert_ready_marks() -> void:
	var models: Array = _hud._bandpanel._work_source_models(_hud._band_labor.panel_band(), 0)
	var ready: Array = models.filter(func(m): return String(m["ready_policy"]) != "")
	_assert_band_panel("ready — exactly two of the three worked sources offer a rung", ready.size() == 2)
	var by_policy: Array = ready.map(func(m): return String(m["ready_policy"]))
	by_policy.sort()
	_assert_band_panel("ready — the tended patch offers Sow and the tamed herd Corral",
		by_policy == ["corral", "sow"])
	_assert_band_panel("ready — the wild-ceiling herd offers nothing",
		models.filter(func(m): return String(m["herd_id"]) == "ready_never" \
			and String(m["ready_policy"]) == "").size() == 1)

## The ready chip narrows the board to the offering rows and nothing else.
func _assert_ready_filter_narrows() -> void:
	var models: Array = _hud._bandpanel._work_source_models(_hud._band_labor.panel_band(), 0)
	var shown: Array = _hud._bandpanel._filter_work_models(models)
	_assert_band_panel("ready filter — the board narrows to the two offering rows", shown.size() == 2)
	_assert_band_panel("ready filter — every shown row actually offers a rung",
		shown.filter(func(m): return String(m["ready_policy"]) == "").is_empty())

func _investment_policy_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 912
	band["id"] = "Band 9"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 3, "workers_needed": 3, "floor": INVESTMENT_ROW_FLOOR,
			"improvement": INVESTMENT_ROW_IMPROVEMENT,
			"fauna_id": INVESTMENT_ROW_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": 0.75, "sustainable_yield": 0.75},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "floor": EXTRACTIVE_ROW_FLOOR,
			"fauna_id": EXTRACTIVE_ROW_HERD_ID, "target_x": 69, "target_y": 19,
			"actual_yield": 0.20, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 1},
	]
	return band

## The two herds those rows work. The pen is mid-build (`corral_progress`), which is exactly the
## ~25-turn investment a pick in the work inspector would throw away.
func _investment_policy_herd_fixtures() -> Array:
	var penned := {
		"id": INVESTMENT_ROW_HERD_ID, "species": "Aurochs", "x": 70, "y": 17,
		"population": 210, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corral_progress": 0.4,
		"per_worker_yield": 0.25,
		"hunt_policy_ceilings": {
			"sustain": 0.40, "surplus": 1.10, "deplete": 1.60, "eradicate": 2.40,
		},
		# The build dips are FRACTIONS of the held stance now, not rows of the list above (#442).
		"tame_build_fraction": 0.50, "corral_build_fraction": 0.50,
	}
	_set_managed_herders(penned, INVESTMENT_ROW_HERDERS_NEEDED)
	return [
		penned,
		{
			"id": EXTRACTIVE_ROW_HERD_ID, "species": "Red Deer", "x": 69, "y": 19,
			"population": 90, "ecology_phase": "thriving", "huntable": true,
			"per_worker_yield": 0.10,
			"hunt_policy_ceilings": {
				"sustain": 0.20, "surplus": 0.60, "deplete": 0.90, "eradicate": 1.40,
			},
		},
	]

## A band keeping an UNDER-CONTAINED pen: one keeper works the Corralled herd, but it needs 4 herders.
## The work board must flag its Hunt row (fauna neglect-escape arc). `herded_fraction` is left STALE at
## 1.0 to prove the flag derives from the ACTUAL staffed count (2 < needed 4), not the lagging fraction.
func _under_herded_work_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 918
	band["id"] = "Band 18"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 2, "workers_needed": UNDER_HERDED_WORK_HERDERS_NEEDED,
			"floor": 0.5,
			"improvement": "corral",
			"fauna_id": UNDER_HERDED_WORK_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": 5.40, "sustainable_yield": 5.40, "overdraws": false},
		{"kind": "scout", "workers": 1},
	]
	return band

## The Corralled herd that row works: needs 4 herders, `herded_fraction` a stale 1.0 (the OLD code
## would have read it "fully herded"), so only the actual staffed count exposes the shed.
func _under_herded_work_herd_fixtures() -> Array:
	var penned := {
		"id": UNDER_HERDED_WORK_HERD_ID, "species": "Aurochs", "x": 70, "y": 17,
		"population": 210, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": true, "herded_fraction": 1.0,
		"per_worker_yield": 5.40,
		"hunt_policy_ceilings": {
			"sustain": 5.40, "surplus": 6.0, "deplete": 7.0, "eradicate": 8.0,
			"tame": 5.40, "corral": 5.40,
		},
	}
	_set_managed_herders(penned, UNDER_HERDED_WORK_HERDERS_NEEDED)
	return [penned]

## The band working that Wild Fowl: 2 herders on it (below the crew of 3) and idle workers free, on an
## EXTRACTIVE rung so `herd_crew_floor` reads the ownership-gated `herders_needed` — the field the row's
## own under-herded ⚠ gates on, which is the whole point of the frame.
func _herder_floor_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 919
	band["id"] = "Band 19"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": HERDER_FLOOR_STAFFED,
			"workers_needed": HERDER_FLOOR_HERDERS_NEEDED, "floor": 0.5,
			"fauna_id": HERDER_FLOOR_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": HERDER_FLOOR_SUSTAIN_CEILING,
			"sustainable_yield": HERDER_FLOOR_SUSTAIN_CEILING, "overdraws": false},
	]
	return band

## The herd itself — TAMED but unpenned (the ◎ pastoral rung), so it is owned and really does owe the
## keepers its `herders_needed` names, while its take stays small enough that the take-side max-useful
## (2) lands BELOW that crew (3).
func _herder_floor_herd_fixtures() -> Array:
	var fowl := {
		"id": HERDER_FLOOR_HERD_ID, "species": "Wild Fowl", "x": 70, "y": 17,
		"population": 60, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": false,
		"per_worker_yield": HERDER_FLOOR_PER_WORKER,
		"hunt_policy_ceilings": {
			"sustain": HERDER_FLOOR_SUSTAIN_CEILING, "surplus": 0.14, "deplete": 0.20,
			"eradicate": 0.30, "tame": 0.05, "corral": 0.05,
		},
	}
	_set_managed_herders(fowl, HERDER_FLOOR_HERDERS_NEEDED)
	return [fowl]

## THE INVARIANT AS A TEST: one row cannot flag a problem and disable its own remedy, and the two cap
## twins cannot gate differently.
##
## Three claims, and the middle one is what makes the other two non-vacuous:
##   1. the row still carries the under-herded ⚠ — the board KNOWS the herd is short a keeper;
##   2. its `+` is ENABLED at the staffed 2, so the remedy the ⚠ demands is reachable;
##   3. `source_worker_cap_state` (the worked row) and `_forecast_worker_cap` (the compose stepper)
##      answer with the SAME ceiling — the crew of 3, not the take-side 2 — which is the promise the
##      two twins make by sitting beside each other.
func _assert_herder_floor_row(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var idle := _hud._band_labor.effective_idle(band)
	if idle <= 0:
		push_error("band_panel_preview: herder-floor frame needs idle workers to gate on the source")
		return
	var found := false
	for model in _hud._bandpanel._work_source_models(band, idle):
		var m: Dictionary = model
		if String(m.get("herd_id", "")) != herd_id:
			continue
		found = true
		if not bool(m.get("under_herded", false)):
			push_error("band_panel_preview: expected under_herded on the Hunt row for %s" % herd_id)
		elif not bool(m.get("can_add", false)):
			push_error(("band_panel_preview: the under-herded row for %s disables its own `+` at %d "
				+ "workers with %d idle — the board flags the shed and refuses the fix")
				% [herd_id, int(m.get("workers", 0)), idle])
		else:
			print("band_panel_preview: assert OK — the under-herded row keeps its `+` live (crew %d > take-useful %d)"
				% [HERDER_FLOOR_HERDERS_NEEDED, HERDER_FLOOR_TAKE_USEFUL])
	if not found:
		push_error("band_panel_preview: no Hunt work row for %s" % herd_id)
		return
	# The twins, asked the same question about the same herd+policy. `_forecast_worker_cap` is given an
	# assignable count above both candidate ceilings so its answer IS the usefulness ceiling and not a
	# labor bound; `source_worker_cap_state` is probed on either side of that ceiling.
	var herd := _hud._band_labor.find_world_herd(herd_id)
	var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	# `herd_crew_floor` keys on the IMPROVEMENT axis since #442 (it picks the ownership-gated
	# `herders_needed` or the would-be `herders_needed_if_managed`), so the probe reads the ROW's own
	# improvement rather than asserting one — that is what keeps the twin comparison honest.
	var floor_workers := SourceForecast.herd_crew_floor(herd,
		_hud._band_labor.improvement_for_hunt(band, herd_id) != SourceForecast.IMPROVEMENT_NONE)
	var compose_cap := int(_hud._drawercompose._forecast_worker_cap(
		forecast, HERDER_FLOOR_HERDERS_NEEDED + 1, floor_workers)["cap"])
	var row_below: bool = bool(SourceForecast.source_worker_cap_state(
		forecast, HERDER_FLOOR_HERDERS_NEEDED - 1, 1, floor_workers)["can_add"])
	var row_at: bool = bool(SourceForecast.source_worker_cap_state(
		forecast, HERDER_FLOOR_HERDERS_NEEDED, 1, floor_workers)["can_add"])
	if compose_cap != HERDER_FLOOR_HERDERS_NEEDED:
		push_error("band_panel_preview: the compose stepper caps at %d, not the crew of %d"
			% [compose_cap, HERDER_FLOOR_HERDERS_NEEDED])
	elif not (row_below and not row_at):
		push_error(("band_panel_preview: the worked row does not gate at the crew of %d "
			+ "(can_add below=%s, at=%s)") % [HERDER_FLOOR_HERDERS_NEEDED, row_below, row_at])
	else:
		print("band_panel_preview: assert OK — both cap twins gate at the crew of %d, above the take-useful %d"
			% [HERDER_FLOOR_HERDERS_NEEDED, HERDER_FLOOR_TAKE_USEFUL])

## The under-contained Hunt row must carry the shed flag: the ⚠ mark, the drifting-off note, and the
## `under_herded` model flag the row + inspector tint from.
func _assert_under_herded_work_row(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var found := false
	for model in _hud._bandpanel._work_source_models(band, 0):
		var m: Dictionary = model
		if String(m.get("herd_id", "")) != herd_id:
			continue
		found = true
		if not bool(m.get("under_herded", false)):
			push_error("band_panel_preview: expected under_herded on the Hunt row for %s" % herd_id)
		elif not String(m.get("marks", "")).contains(HudComposeVocab.OVERHUNT_FLAG):
			push_error("band_panel_preview: expected the ⚠ mark on the under-herded row for %s" % herd_id)
		elif not String(m.get("note", "")).contains("drifting off"):
			push_error("band_panel_preview: expected the drifting-off note on the under-herded row for %s" % herd_id)
		else:
			print("band_panel_preview: assert OK — under-herded Hunt row flags the shed (⚠ + note)")
	if not found:
		push_error("band_panel_preview: no Hunt work row for %s" % herd_id)

# ---- THE SOURCE-RUNG BOARD ------------------------------------------------------------------------
#
# `update_forage_patches` was called EXACTLY ONCE in this whole harness (the per-source-cap state), so
# `forage_patch_lookup()` was empty for every Work-tab frame and no rung could ever have rendered here.
# These fixtures close that: the rung frame below, and rung-marked patches under the paged board so the
# marks are also seen at real density and in the narrow-shell threshold frames.

## A band working one source per rung — three forage rows (wild / Tended / Field) and two hunt rows
## (pastoral / penned). Every row is staffed and unremarkable otherwise, so the ONLY thing that differs
## down the board is the rung mark.
func _rung_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 922
	band["id"] = "Band 22"
	band["idle_workers"] = 6
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"target_x": RUNG_WILD_TILE.x, "target_y": RUNG_WILD_TILE.y,
			"actual_yield": 0.61, "sustainable_yield": 0.61},
		{"kind": "forage", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"target_x": RUNG_TENDED_TILE.x, "target_y": RUNG_TENDED_TILE.y,
			"actual_yield": 0.97, "sustainable_yield": 0.97},
		{"kind": "forage", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"target_x": RUNG_FIELD_TILE.x, "target_y": RUNG_FIELD_TILE.y,
			"actual_yield": 1.94, "sustainable_yield": 1.94},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "floor": 0.5,
			"fauna_id": RUNG_PASTORAL_HERD_ID, "target_x": 70, "target_y": 19,
			"actual_yield": 1.20, "sustainable_yield": 1.20},
		{"kind": "hunt", "workers": RUNG_PENNED_HERDERS, "workers_needed": RUNG_PENNED_HERDERS,
			"floor": 0.5,
			"fauna_id": RUNG_PENNED_HERD_ID, "target_x": 69, "target_y": 20,
			"actual_yield": 5.40, "sustainable_yield": 5.40},
	]
	return band

## The three patches those forage rows work. Deliberately RUNG FIELDS ONLY — no `per_worker_yield` /
## `ceiling_*` — so `SourceForecast.max_useful_workers` stays UNBOUNDED and the steppers gate exactly as
## they did before patches were pushed here at all. This frame is about the mark, not the cap.
func _rung_patch_fixtures() -> Array:
	return [
		{"x": RUNG_WILD_TILE.x, "y": RUNG_WILD_TILE.y, "is_cultivated": false, "is_field": false},
		{"x": RUNG_TENDED_TILE.x, "y": RUNG_TENDED_TILE.y, "is_cultivated": true, "is_field": false,
			"committed_display_name": RUNG_TENDED_CROP},
		# A Field is ALSO cultivated — that is why the row builder tests `is_field` FIRST, and why this
		# fixture sets both rather than the field flag alone.
		{"x": RUNG_FIELD_TILE.x, "y": RUNG_FIELD_TILE.y, "is_cultivated": true, "is_field": true,
			"committed_display_name": RUNG_FIELD_CROP},
	]

## The two herds those hunt rows work: one TAMED but unpenned (pastoral), one CORRALLED. The penned one
## is fully staffed so the frame carries no ⚠ competing with the rung mark for the eye.
func _rung_herd_fixtures() -> Array:
	var penned := {
		"id": RUNG_PENNED_HERD_ID, "species": "Aurochs", "x": 69, "y": 20,
		"population": 180, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": true,
		"hunt_policy_ceilings": {"sustain": 5.40},
	}
	_set_managed_herders(penned, RUNG_PENNED_HERDERS)
	return [
		{
			"id": RUNG_PASTORAL_HERD_ID, "species": "Wild Boar", "x": 70, "y": 19,
			"population": 140, "ecology_phase": "thriving", "huntable": true,
			# Tamed but NOT corralled — the rung the animal ladder had no glyph of its own for.
			"domestication": 1.0, "corralled": false,
			"hunt_policy_ceilings": {"sustain": 1.20},
		},
		penned,
	]

## Forage modules for the rung tiles, so each Forage row still resolves its map glyph and the rung mark
## is read BESIDE a source glyph rather than in isolation.
func _rung_forage_modules() -> Array:
	var modules: Array = []
	for tile in [RUNG_WILD_TILE, RUNG_TENDED_TILE, RUNG_FIELD_TILE]:
		modules.append({"x": tile.x, "y": tile.y, "module": "savanna_grassland", "kind": "gather"})
	return modules

## Patches for the PAGED board, so the rung marks are also seen at real board density and in the
## narrow-shell threshold frames. Carries `_cap_demo_patch_fixtures()` forward because
## `update_forage_patches` CLEARS the lookup: dropping (71,18) would re-enable a `+` the
## `band_panel_work_trade_*` frames render disabled, moving a frame this change has nothing to do with.
## Rung fields only, for the same cap-neutrality reason as `_rung_patch_fixtures`.
func _many_source_patch_fixtures() -> Array:
	var patches := _cap_demo_patch_fixtures()
	for i in range(MANY_SOURCE_COUNT):
		var patch := {"x": MANY_SOURCE_ORIGIN_X + i, "y": MANY_SOURCE_ORIGIN_Y}
		if i % RUNG_MANY_FIELD_STRIDE == 3:
			patch["is_cultivated"] = true
			patch["is_field"] = true
			patch["committed_display_name"] = RUNG_FIELD_CROP
		elif i % RUNG_MANY_TENDED_STRIDE == 1:
			patch["is_cultivated"] = true
			patch["committed_display_name"] = RUNG_TENDED_CROP
		patches.append(patch)
	return patches

## Every row on the rung board must carry the mark its rung wears — and, decisively, the WILD row must
## carry NONE. Asserting only the marked rows would pass a build that stamped a glyph on everything.
func _assert_work_row_rungs() -> void:
	var expected := {
		"forage:%d,%d" % [RUNG_WILD_TILE.x, RUNG_WILD_TILE.y]: "",
		"forage:%d,%d" % [RUNG_TENDED_TILE.x, RUNG_TENDED_TILE.y]: DetailFormat.CULTIVATION_GLYPH,
		"forage:%d,%d" % [RUNG_FIELD_TILE.x, RUNG_FIELD_TILE.y]: DetailFormat.field_glyph(),
		"hunt:%s" % RUNG_PASTORAL_HERD_ID: DetailFormat.pastoral_glyph(),
		"hunt:%s" % RUNG_PENNED_HERD_ID: DetailFormat.CORRAL_GLYPH,
	}
	# **THE ROW'S VERB FOLLOWS THE SAME RUNG, and it is a SECOND axis off the same patch dict** — a crew
	# on a Tended Patch or a Field is TENDING, not foraging (`labor-ui.md` → "The plant web's crew noun
	# follows the standing rung"). Asserted beside the rung MARK rather than instead of it: the mark
	# says what the source IS and the label says what is being DONE there, so one passing cannot stand
	# in for the other. The hunt rows keep their own `WORK_ROW_HUNT_FORMAT` and are not in this table.
	var expected_labels := {
		"forage:%d,%d" % [RUNG_WILD_TILE.x, RUNG_WILD_TILE.y]:
			HudWorkVocab.WORK_ROW_FORAGE_FORMAT % [RUNG_WILD_TILE.x, RUNG_WILD_TILE.y],
		"forage:%d,%d" % [RUNG_TENDED_TILE.x, RUNG_TENDED_TILE.y]:
			HudWorkVocab.WORK_ROW_TEND_FORMAT % [RUNG_TENDED_TILE.x, RUNG_TENDED_TILE.y],
		"forage:%d,%d" % [RUNG_FIELD_TILE.x, RUNG_FIELD_TILE.y]:
			HudWorkVocab.WORK_ROW_TEND_FORMAT % [RUNG_FIELD_TILE.x, RUNG_FIELD_TILE.y],
	}
	var labels_seen := 0
	var seen := {}
	for model in _hud._bandpanel._work_source_models(_hud._band_labor._panel_band, 0):
		var m: Dictionary = model
		var key := String(m.get("key", ""))
		if not expected.has(key):
			continue
		seen[key] = true
		if expected_labels.has(key):
			var label := String(m.get("label", ""))
			if label != String(expected_labels[key]):
				push_error("band_panel_preview: %s expected row label '%s' but got '%s'" % [
					key, expected_labels[key], label])
			else:
				labels_seen += 1
		var glyph := String(m.get("rung_glyph", ""))
		if glyph != String(expected[key]):
			push_error("band_panel_preview: %s expected rung glyph '%s' but got '%s'" % [
				key, expected[key], glyph])
		elif glyph != "" and String(m.get("rung_tooltip", "")) == "":
			push_error("band_panel_preview: %s wears a rung glyph with no tooltip naming the rung" % key)
	for key in expected:
		if not seen.has(key):
			push_error("band_panel_preview: no work row for %s on the rung board" % key)
	if seen.size() == expected.size():
		print("band_panel_preview: assert OK — %d work rows wear their standing rung (wild bare)" % seen.size())
	if labels_seen == expected_labels.size():
		print("band_panel_preview: assert OK — %d plant rows name the verb their rung runs (Forage/Tend)"
			% labels_seen)

## The rung mark's TOOLTIP has to actually be reachable, and its slot must not eat the row's click —
## two SILENT failures a rendered frame cannot show. A `Label` defaults to `MOUSE_FILTER_IGNORE`, which
## makes `tooltip_text` a no-op (this HUD has shipped six such tooltips nobody ever saw), while the
## obvious fix, `HudWidgets.set_label_tooltip`, sets `STOP` — which would swallow the press that opens
## the inspector strip. Only `PASS` satisfies both, so that is what is asserted.
##
## The marks are found by `HudWorkVocab.WORK_ROW_RUNG_META`, NEVER by their glyph: `savanna_grassland`'s
## SITE icon is also 🌾, so a text match walks straight into the row's source-icon Label — which this
## assertion did, and failed on, before the meta existed.
func _assert_rung_labels_are_hoverable() -> void:
	var labels: Array = []
	_collect_rung_labels(_panel, labels)
	var marked := 0
	for label_variant in labels:
		var label: Label = label_variant
		if String(label.get_meta(HudWorkVocab.WORK_ROW_RUNG_META)) == "":
			continue   # a WILD row's reserved-but-empty slot — nothing to hover
		marked += 1
		if label.tooltip_text == "":
			push_error("band_panel_preview: rung mark '%s' carries no tooltip" % label.text)
			return
		if label.mouse_filter != Control.MOUSE_FILTER_PASS:
			push_error("band_panel_preview: rung mark '%s' has mouse_filter %d — PASS is the only value that both shows the tooltip and lets the row's click through" % [
				label.text, label.mouse_filter])
			return
	if marked == 0:
		push_error("band_panel_preview: no rung mark rendered in the panel (%d slots) — the mark is missing" % labels.size())
	else:
		print("band_panel_preview: assert OK — %d rung marks are hoverable (tooltip + PASS), %d wild slots bare" % [
			marked, labels.size() - marked])

func _collect_rung_labels(node: Node, out: Array) -> void:
	if node is Label and (node as Label).has_meta(HudWorkVocab.WORK_ROW_RUNG_META):
		out.append(node)
	for child in node.get_children():
		_collect_rung_labels(child, out)

## Open the work inspector on the row standing on `policy`, with its policy picker EXPANDED, and
## repage so the picker actually renders. `_work_floor_open` is otherwise never true in either
## harness, which is why this control had zero frame coverage.
## Open the work inspector on the row working a NAMED herd — the trade-row frames need a specific
## source (the wolf), not "the first row", which is the forage patch.
func _open_work_inspector_for_herd(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	for model_variant in _hud._bandpanel._work_source_models(band, 0):
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) != herd_id:
			continue
		_hud._bandpanel._toggle_work_inspector(String(model.get("key", "")))
		return
	push_error("band_panel_preview: no work row hunting '%s' — fixture drifted?" % herd_id)

## **Keyed on the HERD, not on the rung.** Both rows stand on the same stance now (issue #442 — the
## build verb moved to its own field), so a rung is no longer an identity; the source is.
func _open_work_policy_picker_for_herd(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	for model_variant in _hud._bandpanel._work_source_models(band, 0):
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) != herd_id:
			continue
		_hud._bandpanel._work_open_key = String(model.get("key", ""))
		_hud._bandpanel._work_floor_open = true
		_hud._bandpanel._repage_work_zone()
		return
	push_error("band_panel_preview: no work row hunting '%s' — fixture drifted?" % herd_id)

## The open inspector strip: the work zone host's PanelContainer (the board and chips are boxes).
func _work_inspector_strip() -> PanelContainer:
	var host: VBoxContainer = _hud._bandpanel._work_zone_host
	if host == null or not is_instance_valid(host):
		return null
	for child in host.get_children():
		if child is PanelContainer:
			return child
	return null

## The inspector picker's rung buttons, keyed by policy — found by the `HudWidgets.POLICY_RUNG_META`
## the picker stamps on each one, NEVER by matching its face. The face is presentation and has already
## changed twice (glyph + metric → glyph + name over metric → that pair as child Labels at two sizes,
## which left the Button's own `text` empty), and each time a text match here would have quietly
## returned nothing and passed every assertion vacuously. It also has to RECURSE now: a rung is a cell
## (a MarginContainer holding the button and the label stack), so the grid's children are no longer the
## buttons themselves.
func _picker_rung_buttons() -> Dictionary:
	var buttons := {}
	var strip := _work_inspector_strip()
	if strip == null:
		return buttons
	var grid := _find_first_grid(strip)
	if grid == null:
		return buttons
	_collect_rung_buttons(grid, buttons)
	return buttons

func _collect_rung_buttons(node: Node, out: Dictionary) -> void:
	if node is Button and (node as Button).has_meta(HudWidgets.POLICY_RUNG_META):
		out[String((node as Button).get_meta(HudWidgets.POLICY_RUNG_META))] = node
	for child in node.get_children():
		_collect_rung_buttons(child, out)

func _find_first_grid(node: Node) -> GridContainer:
	if node is GridContainer:
		return node
	for child in node.get_children():
		var found := _find_first_grid(child)
		if found != null:
			return found
	return null

## `_assert_standing_investment_line` went with the WARN line it read, and `_find_label_with_text`
## was its only caller (issue #442): a work row can no longer stand on a rung the picker cannot
## show, so there is no such line to look for.

## Press a real rung button and watch what happens: the emit must land IMMEDIATELY, with no dialog,
## on BOTH rows. `want_confirm` survives as a parameter so the assertion still states which outcome it
## expects rather than asserting a bare "nothing happened" — but no caller passes `true` any more.
## The confirm it once guarded existed because a stance pick DISCARDED a running build; since issue
## #442 `assign_labor` does not touch the improvement axis at all, so there is nothing to lose and
## nothing to ask about. A row that IS building takes the same path as one that is not, which is the
## whole point of the pair.
func _assert_policy_pick_confirms(standing: String, want_confirm: bool) -> void:
	var buttons := _picker_rung_buttons()
	if not buttons.has(PICKED_RUNG_PRESET):
		push_error("band_panel_preview: no '%s' rung in the work inspector's picker" % PICKED_RUNG_PRESET)
		return
	var fired := [false]
	var sink := func(_payload: Dictionary) -> void: fired[0] = true
	_hud.assign_labor_requested.connect(sink)
	(buttons[PICKED_RUNG_PRESET] as Button).pressed.emit()
	var dialog_shown := false
	for child in _hud.get_children():
		if child is ConfirmationDialog:
			dialog_shown = true
	_hud.assign_labor_requested.disconnect(sink)
	if dialog_shown == want_confirm and fired[0] == (not want_confirm):
		print("band_panel_preview: assert OK — a '%s' row's pick %s" % [
			standing, "confirms before discarding" if want_confirm else "emits immediately"])
	else:
		push_error("band_panel_preview: '%s' row pick expected (confirm=%s, emit=%s) but got (confirm=%s, emit=%s)" % [
			standing, want_confirm, not want_confirm, dialog_shown, fired[0]])
	_dismiss_dialogs()

## CONTROL (ii): on an EXTRACTIVE row exactly ONE rung wears the `primary` variant. There is no other
## marker of "this is the standing rung" than the button's own resting fill, so read it back.
func _assert_lit_rung(standing: String) -> void:
	var lit: Array[String] = []
	var buttons := _picker_rung_buttons()
	for policy in buttons:
		var box := (buttons[policy] as Button).get_theme_stylebox("normal")
		if box is StyleBoxFlat and (box as StyleBoxFlat).bg_color.is_equal_approx(HudStyle.BUTTON_PRIMARY_BG):
			lit.append(String(policy))
	if lit.size() == 1 and lit[0] == standing:
		print("band_panel_preview: assert OK — exactly one rung lit, and it is '%s'" % standing)
	else:
		push_error("band_panel_preview: expected only '%s' lit in the picker but got %s" % [standing, str(lit)])

## Drop every optimistic pending assign through the REAL path — a snapshot whose turn is NEWER than the
## edit is what confirms it — so an assertion that issues one leaves the board as it found it, and the
## next one starts from the CONFIRMED assignments rather than from its neighbour's leftovers.
func _clear_pending_labor() -> void:
	_hud._band_labor.reconcile_pending(_hud._band_labor.current_turn() + 1)

## **THE IMPROVEMENT MUST SURVIVE A CREW EDIT** (issue #442). `assign_labor` deliberately does not carry
## the second axis, so between the click and the next snapshot the OPTIMISTIC PENDING overlay is the ONLY
## thing holding it — and an emit that omits the argument writes `IMPROVEMENT_NONE` over a running build,
## which `effective_worker_map` then reads back for the rest of the turn. Every work-board crew edit funnels
## through `_emit_work_assign` (the row `−/+`, the inspector's Unassign link, a stance pick), so driving it
## once covers all three.
##
## Two claims, and the FIRST is what stops the second being vacuous — a row that never carried the
## improvement would "keep" it trivially:
##   1. the confirmed row really is mid-build: it carries the improvement AND renders the BUILDING badge;
##   2. after the edit the row is PENDING and still carries both — it has not flipped back to advertising
##      the very rung already under way (`next_rung_ready` excludes the verb in flight, so a blanked axis
##      re-offers it), and `herd_crew_floor` still keys on the would-be crew rather than the gated one.
func _assert_crew_edit_keeps_improvement(herd_id: String, improvement: String) -> void:
	_clear_pending_labor()
	# The band is staged LOCALLY rather than read off `_panel_band`, and that is deliberate: an emit
	# re-renders the SELECTED player band into the panel (`Hud._after_pending_change` →
	# `_render_selection_panel`), so the picker assertion above has already swung `_panel_band` to
	# whichever band an earlier state selected. Both calls under test take the band as a PARAMETER, and
	# the only shared state either touches is the pending overlay keyed by this band's entity — cleared
	# on the way out — so this leaves every following frame exactly as it found it.
	var band: Dictionary = _stamp_band_ids([_investment_policy_band_fixture()])[0]
	var before := _find_work_model_for_herd(band, herd_id)
	if before.is_empty():
		push_error("band_panel_preview: no Hunt work row for '%s' — fixture drifted?" % herd_id)
		return
	if String(before.get("improvement", "")) != improvement or String(before.get("building_glyph", "")) == "":
		push_error(("band_panel_preview: the '%s' row is not mid-build before the edit "
			+ "(improvement '%s', building glyph '%s') — the crew-edit assertion would be vacuous")
			% [herd_id, String(before.get("improvement", "")), String(before.get("building_glyph", ""))])
		return
	# The REAL row-stepper path, at one worker more than it stands on — the `+` a player presses.
	_hud._bandpanel._emit_work_assign(band, before, int(before.get("workers", 0)) + 1)
	var after := _find_work_model_for_herd(band, herd_id)
	if not bool(after.get("pending", false)):
		push_error("band_panel_preview: the crew edit on '%s' recorded no pending assign to judge" % herd_id)
	elif String(after.get("improvement", "")) != improvement:
		push_error(("band_panel_preview: a crew edit on '%s' dropped the improvement — the row now reads "
			+ "'%s' instead of '%s', so its build badge vanishes and the rung it is already climbing is "
			+ "re-offered for the rest of the turn")
			% [herd_id, String(after.get("improvement", "")), improvement])
	elif String(after.get("building_glyph", "")) == "":
		push_error(("band_panel_preview: a crew edit on '%s' kept the improvement but lost the BUILDING "
			+ "badge — the row stopped showing the verb under way") % herd_id)
	else:
		print("band_panel_preview: assert OK — a pending crew edit keeps the '%s' build on the '%s' row"
			% [improvement, herd_id])
	_clear_pending_labor()

## The work-board model for the row hunting `herd_id`, or {} — the models are rebuilt per call, so a row
## has to be re-found after every edit rather than held across one.
func _find_work_model_for_herd(band: Dictionary, herd_id: String) -> Dictionary:
	for model_variant in _hud._bandpanel._work_source_models(band, _hud._band_labor.effective_idle(band)):
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) == herd_id:
			return model
	return {}

## Close any modal the preview opened, so the next state renders unobstructed.
func _dismiss_dialogs() -> void:
	for child in _hud.get_children():
		if child is AcceptDialog:
			(child as AcceptDialog).hide()
			child.queue_free()

## 34 gather modules on a row of tiles, so every Forage row resolves a real map glyph.
func _many_forage_modules() -> Array:
	var modules: Array = []
	for i in range(MANY_SOURCE_COUNT):
		modules.append({"x": MANY_SOURCE_ORIGIN_X + i, "y": MANY_SOURCE_ORIGIN_Y,
			"module": "savanna_grassland", "kind": "gather"})
	return modules

## A band working MANY_SOURCE_COUNT forage patches — the case the paged board exists for (34 rows
## would be ~950px of unbroken list in the old stack).
func _many_sources_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["working_age"] = MANY_SOURCE_COUNT * 2
	band["idle_workers"] = MANY_SOURCE_COUNT
	# Keep the age split in step with the enlarged workforce — `age_working` IS `working_age`, and the
	# three sum to `size` (see `_band_fixture`). Derived, not retyped, so raising MANY_SOURCE_COUNT
	# cannot silently desync the PEOPLE bar from the WORKFORCE bar beneath it.
	var workers: int = int(band["working_age"])
	band["age_working"] = workers
	band["age_children"] = int(round(workers * MANY_SOURCE_CHILD_RATIO))
	band["age_elders"] = int(round(workers * MANY_SOURCE_ELDER_RATIO))
	band["size"] = workers + int(band["age_children"]) + int(band["age_elders"])
	var assignments: Array = []
	for i in range(MANY_SOURCE_COUNT):
		assignments.append({
			"kind": "forage", "workers": 1,
			# Every third patch is overstaffed, so the ⚠ attention chip + the WARN stripe have content.
			"workers_needed": 1 if i % 3 != 0 else 0,
			"floor": 0.5,
			"target_x": MANY_SOURCE_ORIGIN_X + i, "target_y": MANY_SOURCE_ORIGIN_Y,
			"actual_yield": 0.10 + 0.01 * float(i), "sustainable_yield": 0.10 + 0.01 * float(i),
		})
	band["labor_assignments"] = assignments
	return band

## **A BAND WHOSE IDLE WORKFORCE OUTRUNS `max_expedition_party_size`** (left at the reference band's 8).
## The denial stepper's ceiling is supply — idle workers — and that field is the estimate tables'
## SAMPLING AXIS rather than a rules cap, so this is the only band shape in which a stepper reading the
## wrong one is visible at all. Same entity 904, so the expeditions still attach and the cycler reads 1/1.
func _deep_party_band_fixture() -> Dictionary:
	var band := _band_fixture()
	# **THE WORKFORCE IS WHAT IS RAISED, NOT `idle_workers`.** `HudBandLaborState.effective_idle`
	# derives idle as `working_age − assigned`, so writing the idle count alone would leave every
	# surface — the stepper's cap included — still reading the reference band's 3.
	var assigned := 0
	for assignment_variant in (band["labor_assignments"] as Array):
		assigned += int((assignment_variant as Dictionary).get("workers", 0))
	var workers := assigned + DENIAL_DEEP_PARTY_IDLE
	# Keep the age split in step with the enlarged workforce, `_many_sources_band_fixture`'s rule:
	# `age_working` IS `working_age` and the three sum to `size`, or the PEOPLE bar renders as a bug on
	# the very frame the parties zone is being judged on. SCALED off the reference band's own brackets
	# rather than retyped, so the dependency ratio the bar is tinted by does not move either.
	var scale := float(workers) / float(band["age_working"])
	band["working_age"] = workers
	band["idle_workers"] = DENIAL_DEEP_PARTY_IDLE
	band["age_working"] = float(workers)
	band["age_children"] = float(band["age_children"]) * scale
	band["age_elders"] = float(band["age_elders"]) * scale
	band["size"] = int(round(
		float(workers) + float(band["age_children"]) + float(band["age_elders"])))
	return band

## Every worker committed: the parties footer must still SHOW its button, disabled, with the reason.
func _no_idle_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["idle_workers"] = 0
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 16, "workers_needed": 16, "floor": 0.5,
			"target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
	]
	return band

## Pin the CANVAS (`content_scale_size`) as well as the window, and keep the two equal so the stretch
## factor is exactly 1 and the panel's canvas-space width IS `size.x`.
##
## Needed because `project.godot` stretches `canvas_items` with an `expand` aspect: the canvas is
## never SMALLER than the project's base resolution on either axis, so `get_visible_rect().size.x`
## floors at 1920 however narrow the window is — a plain `_pin_window(1055, 900)` still renders a
## 1920-wide panel and silently proves nothing about a sub-1920 threshold.
func _pin_canvas(size: Vector2i) -> void:
	_pinned_canvas = size
	await _pin_window(size)

## Force the window WINDOWED at `size` and wait for the WM to actually honour it, so a maximize
## cannot land between two states and render them at different resolutions.
func _pin_window(size: Vector2i) -> void:
	_pinned_size = size
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = size
	if _pinned_canvas != Vector2i.ZERO:
		window.content_scale_size = _pinned_canvas
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		if window.size == size and window.mode == Window.MODE_WINDOWED:
			break
		window.mode = Window.MODE_WINDOWED
		window.size = size
		await get_tree().process_frame
	if window.size != size:
		push_warning("band_panel_preview: window pinned to %s but reports %s" % [size, window.size])

## Settle the window ONCE, in `_ready`, before any state renders — and take the maximize DELIBERATELY
## on the way, which is what closes the last of the drift.
##
## `project.godot` opens the window MAXIMIZED and macOS applies that asynchronously, so whether a run
## ever passed through the monitor-sized window was a COIN FLIP — and it is a coin flip the pixels
## remember: `window/stretch` is `canvas_items` with an `expand` aspect, so the stretch scale swings
## across a maximize and the rasterized-glyph coverage state does not come back bit-identical. It is
## also a LAYOUT flip, not merely a pixel one — a run that loses the race renders the "bottom dock"
## states at the monitor's width, i.e. against the ultrawide content cap rather than the wide shell
## the state exists to judge (one measured run drew `band_panel_left` at 5120×1410). Dodging the
## maximize is not available — `ui_preview` measured a late one landing mid-run after 30 stable frames
## — so ASK for it, then undo it: every run then takes the same path.
func _stabilize_canvas() -> void:
	get_window().mode = Window.MODE_MAXIMIZED
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size != PREVIEW_SIZE:
			break
		await get_tree().process_frame
	# Restore and HOLD: the maximize is re-applied asynchronously, so "the right size once" is not the
	# same as "it stays" — wait for CANVAS_STABLE_FRAMES consecutive good frames. After this every
	# `_pin_window` at the same size returns without awaiting, so each state gets the same number of
	# layout passes in every run.
	var stable := 0
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size == PREVIEW_SIZE and get_window().mode == Window.MODE_WINDOWED:
			stable += 1
			if stable >= CANVAS_STABLE_FRAMES:
				return
		else:
			stable = 0
			await _pin_window(PREVIEW_SIZE)
		await get_tree().process_frame
	push_error("band_panel_preview: the window never held the pinned %s canvas — frames will drift" % PREVIEW_SIZE)

## The viewport image, GUARANTEED to be at the size this state pinned (or an integer HiDPI multiple of
## it). The WM's deferred maximize can resize the render target between a settle and a capture, so
## re-pin and re-draw until the geometry is the pinned one, then give up loudly rather than save a
## frame that silently renders the panel at a width the state never asked for.
func _capture(name: String) -> Image:
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			# No image to read back — the dummy renderer (i.e. someone ran this with `--headless`,
			# which selects it on Godot 4.5+). Capture is impossible, but the compile/scene gate and
			# every assertion still ran. Run WITHOUT `--headless` for PNGs.
			push_warning("band_panel_preview: null image (dummy renderer?) — skipping %s.png; run without --headless" % name)
			return null
		var w := image.get_width()
		var h := image.get_height()
		if w % _pinned_size.x == 0 and h % _pinned_size.y == 0 \
				and w / _pinned_size.x == h / _pinned_size.y:
			return image
		await _pin_window(_pinned_size)
		await get_tree().process_frame
		RenderingServer.force_draw()
		await get_tree().process_frame
	push_error("band_panel_preview: viewport never came back to the pinned %s canvas for %s" % [_pinned_size, name])
	return null

## The hang guard from the scene, or `null` if the node has gone. Checked for its method rather than
## assumed: calling a missing method on an untyped `Node` is a runtime error, and one raised here
## would abort `_ready` exactly the way the guard exists to survive.
func _resolve_watchdog() -> Node:
	var node := get_node_or_null(WATCHDOG_NODE)
	if node != null and node.has_method(WATCHDOG_PROGRESS_METHOD):
		return node
	push_warning(("band_panel_preview: no %s node in the scene — the run has NO hang guard. Restore "
		+ "it from tools/band_panel_preview.tscn (see preview_watchdog.gd).") % WATCHDOG_NODE)
	return null

## A sign of life for the hang guard, from the one call every state makes.
func _note_progress() -> void:
	if _watchdog != null:
		_watchdog.note_progress()

## Stand the guard down on the way out, so a slow shutdown cannot be reported as a stall.
func _finish() -> void:
	if _watchdog != null:
		_watchdog.disarm()
	get_tree().quit()

func _settle() -> void:
	_note_progress()
	# Re-assert the window EVERY state: the WM's maximize lands asynchronously and can arrive between
	# two states, rendering them at different resolutions (blend_probe hit the same thing).
	await _pin_window(_pinned_size)
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	_current_state = name
	# Check the herd fixtures RENDERING IN THIS FRAME, so a half-set field pair fails against the state
	# it silently mis-renders rather than against nothing at all.
	_guard_frame_herd_fields(name)
	var image: Image = await _capture(name)
	if image == null:
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("band_panel_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("band_panel_preview: saved ", name, ".png")

## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live vitals
## RichTextLabel with the very `[url]` meta its own text carries, so the bound handler + anchor run
## exactly as they do in the game. A debug back door (poking Hud state directly) would pass even with
## the click path broken, which is the whole reason this goes through the signal.
func _click_disclosure(key: String) -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label := _find_meta_label(_panel, meta)
	if label == null:
		push_warning("band_panel_preview: no vitals label offering '%s' — disclosure not rendered?" % meta)
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


# ---- the herd herders_needed FIELD-PAIR guard ---------------------------------------------------
# The sim exports TWO herder counts per herd and the client reads DIFFERENT ones by rung, so a fixture
# that sets only one is a silent lie rather than an error:
#   • `herders_needed` — OWNERSHIP-GATED (`fauna::herd_herders_needed`): 0 unless the herd is
#     corralled or owned. The extractive rungs' field, and what the drawer's "Herders A / N" row reads.
#   • `herders_needed_if_managed` — ownership-INDEPENDENT (`fauna::would_be_herders_needed`): the crew
#     the herd WOULD owe, 0 only for a species that can never be tamed. `DrawerComposeController`'s
#     `_forecast_worker_cap` floor reads THIS one for the INVESTMENT rungs (Tame / Corral).
# Both this harness's managed herds set only the first, so any state that opened a compose sheet on
# them would floor the investment cap at 0 — no error, just a wrong number on a frame whose whole job
# is to be read. Half-setting the pair is not catchable by eye, so it is caught here.
#
# THE INVARIANT, from the sim, not from guesswork: `would_be_herders_needed` is identical to
# `herd_herders_needed` except its gate, so the two agree on every herd EXCEPT a not-yet-owned tameable
# one (gated 0, would-be crew real). A herd whose gated count is `> 0` is by definition managed
# (corralled or owned) and therefore tameable, so the ungated field takes the same branch:
#     herders_needed > 0  ⇒  herders_needed_if_managed == herders_needed
# and, in general, `herders_needed_if_managed >= herders_needed`.
const HERDERS_NEEDED_KEY := "herders_needed"
const HERDERS_NEEDED_IF_MANAGED_KEY := "herders_needed_if_managed"
## Deep-scan bound. Fixtures are trees, but a bound turns a future self-referencing one into a stop
## rather than an infinite walk.
const HERD_SCAN_MAX_DEPTH := 8

var _herd_pair_scans := 0
var _herd_pair_violations := 0

## Set BOTH herder counts on a MANAGED herd fixture. The sim exports them EQUAL there (see the
## invariant above), and setting them one at a time is precisely the mistake the guard exists to
## catch — so managed fixtures set them together, through this. A still-WILD but tameable herd is the
## one case where they differ; this harness has none, and one added later writes them by hand.
func _set_managed_herders(fixture: Dictionary, needed: int) -> void:
	fixture[HERDERS_NEEDED_KEY] = needed
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = needed

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
			push_error(("band_panel_preview: %s — herd \"%s\" declares %s %d but %s %d. The would-be "
				+ "crew can never be SMALLER than the ownership-gated one, and on a herd with herders "
				+ "(i.e. a managed one) the sim exports them EQUAL — the investment rungs' worker cap "
				+ "floors on the second field, so half-setting the pair silently caps the crew at the "
				+ "take-side count. Set both through _set_managed_herders.") % [where,
				String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed])
		elif needed > 0 and if_managed != needed:
			# The OTHER half of the invariant, and the one a `>=` test lets through. The gate is the
			# ONLY difference between the two sim functions, so a NON-ZERO gated count already says the
			# herd passed the gate — it is corralled or owned — and the would-be crew is then computed
			# from the same species and headcount by the same arithmetic. A bigger would-be crew is not
			# a conservative fixture, it is an impossible herd: it claims managing this herd would cost
			# MORE than managing it already does.
			_herd_pair_violations += 1
			push_error(("band_panel_preview: %s — herd \"%s\" declares %s %d and %s %d. Once %s is "
				+ "above zero the herd IS managed, and the would-be crew is the SAME crew — the sim's "
				+ "two functions differ only by the ownership gate this herd has already passed, so "
				+ "they must be EQUAL here. Set both through _set_managed_herders; only a still-WILD "
				+ "tameable herd may carry a larger would-be crew, and its gated count is 0.")
				% [where, String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed, HERDERS_NEEDED_KEY])

	for value in dict.values():
		_guard_herd_fields(value, where, depth + 1)

## Every herd dictionary the HUD is holding as this frame renders — the world list, the panel's band
## and the roster around it, plus the selection state (whose `tile_info` carries herds too).
func _guard_frame_herd_fields(state: String) -> void:
	_guard_herd_fields(_hud._band_labor._world_herds, state)
	_guard_herd_fields(_hud._band_labor._player_band, state)
	_guard_herd_fields(_hud._band_labor._player_bands, state)
	_guard_herd_fields(_hud._band_labor._panel_band, state)
	_guard_herd_fields(_hud._selection._selected_herd, state)
	_guard_herd_fields(_hud._selection._roster_herds, state)
	_guard_herd_fields(_hud._selection._selected_tile_info, state)

## The field-pair guard's verdict, ONE line for the whole run (each violation has already been
## push_error'd against the frame it rendered in). The scanned count is part of the claim: a guard that
## walked nothing would pass vacuously, and "0 herd dicts scanned" says so out loud.
func _assert_herd_field_pairs() -> void:
	if _herd_pair_violations > 0:
		push_error("band_panel_preview: %d herd dict(s) of %d scanned half-set the herders_needed pair"
			% [_herd_pair_violations, _herd_pair_scans])
		return
	print("band_panel_preview: assert OK — every herd fixture keeps the herders_needed pair consistent (%d herd dicts scanned)"
		% _herd_pair_scans)

## The snapshot's herd list (shape `Hud.update_herds` / `MapView._rebuild_herd_markers` consume).
## The hunted herd sits at (68, 15) — NOT the (70, 17) its hunt assignment was launched at — so the
## Hunt row's jump proves it resolves the herd's current position, not the stale target.
func _herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120, "ecology_phase": "stressed"},
		{"id": "game_deer_79", "species": "Roe Deer", "x": 64, "y": 11, "population": 90, "ecology_phase": "thriving"},
	]

## The QUARRY herd for the party compose sheet: a Wild Boar carrying BOTH sim-exported tables — the
## band FLOW ceilings and, decisively, the forward-simulated `hunt_trip_estimates` the sheet's policy
## metrics / max-useful party cap / trip forecast are all pure lookups into. Without the trip table the
## sheet renders bare rungs and no forecast, i.e. exactly the state the quarry-first flow exists to fix.
## It sits 4 tiles from the band at (71,18), so the round-trip travel term is exercised too.
## The two quarry herds the parties compose sheet is judged on. **`denial_rows` swaps the FAR herd's
## denial table and nothing else** (`docs/plan_denial_raid.md`) — the viable and the repelled frames
## must differ only in the sim's answer, or a "the verdict changed" assertion would be satisfied by
## two different herds rather than by two different forecasts.
func _quarry_herd_fixtures(denial_rows: Array = []) -> Array:
	var herd := {
		"id": QUARRY_FAR_HERD_ID, "species": "Wild Boar", "x": QUARRY_FAR_X, "y": QUARRY_FAR_Y,
		"population": 140, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8, "food_per_animal": QUARRY_FOOD_PER_ANIMAL,
		"hunt_policy_ceilings": {
			"sustain": 0.30, "surplus": 1.20, "deplete": 0.60, "eradicate": 0.0,
		},
		# The TRADE half of the vector (issue #337) — a boar's hide sells beside its meat.
		"per_worker_trade": 0.12, "trade_per_animal": QUARRY_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.05, "surplus": 0.18, "deplete": 0.09, "eradicate": 0.0,
		},
	}
	# The server's measured boar raid: 1 hunter → 5 animals / 7 turns, 2 → 8 / 8, 3+ → 8 / 4. Delivered
	# food plateaus at party 2, so the sheet's stepper must cap there with its "max 2 useful" note.
	var turns_row := [7, 8, 4, 4, 4, 4, 4, 4]
	var animals_row := [5, 8, 8, 8, 8, 8, 8, 8]
	var table := {}
	for i in animals_row.size():
		var w := i + 1
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		# A CLEAN raid — the party hauls its whole kill home, so delivered = animals × fpa, waste 0.
		# The deeper policies raid to a lower floor and so take MORE (Surplus < Deplete), which is the
		# ASCENDING per-policy metric the picker buttons must read.
		# EVERY rung DELIVERS, Eradicate included. `delivers_food` was REDEFINED by issue #337 — it now
		# says the QUARRY IS EDIBLE, not "this rung is a denial mission" — and an Eradicate raid banks
		# the whole-stock windfall. (This fixture used to assert the opposite, which was correct before
		# that arc.) Each cell carries the trade payload too: a hunt pays a vector, not a food scalar.
		for entry in [["sustain", 0], ["surplus", 2], ["deplete", 3], ["eradicate", 5]]:
			var animals: int = base + int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * QUARRY_FOOD_PER_ANIMAL,
				"delivered_trade": float(animals) * QUARRY_TRADE_PER_ANIMAL,
				"wasted_food": 0.0,
				# **WHICH STOP ENDS THIS SAMPLED TRIP** (`docs/plan_hunt_through_combat.md` §5.2).
				# The sim writes it on every row, so a fixture without it is a herd no live server can
				# produce — and the dock sheet's bound line would then be absent for the honest
				# "not stated" reason, leaving its ONE render site unexercised. A clean raid that
				# hauls its whole kill is stopped by the PACK.
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_PACK_FULL}
	herd["hunt_trip_estimates"] = table
	var denial_table := denial_rows if not denial_rows.is_empty() else _denial_viable_rows()
	herd["denial_estimates"] = denial_table
	herd["denial_party_needed"] = _denial_party_needed_for(denial_table)
	# **WHICH KIT BOTH TABLES ARE QUOTED FOR.** The sim writes the hunt job's default on every herd,
	# always, so a fixture leaving them blank would exercise only the client's fall-back reading and
	# the STATED path — the one live data takes — would go untested. Stamped on all three herds below
	# through `_stamp_estimate_kits`.
	_stamp_estimate_kits(herd)
	# The COMBAT GATE's two herd terms (`docs/plan_hunt_through_combat.md` §4.2). They exist here for
	# the kit-mismatch frame, which suppresses the estimate tables and renders the gate in their place:
	# without them the gate answers `stated == false` and the frame would show a sheet that says
	# nothing at all. Chosen so the gate DISCRIMINATES between the kits — at the big-game tier (20) the
	# effective attack is 18 and the line states the effort, bare-handed (1) it is 0 and the line
	# refuses outright, which is exactly the `none` party's honest verdict.
	herd["defense"] = QUARRY_DEFENSE
	herd["durability"] = QUARRY_DURABILITY
	# A second huntable herd INSIDE the band's hunt reach. It is not a party's job (the band can work
	# it from home), so the picker must refuse it — the near half of the eligibility assertion.
	var near := {
		"id": QUARRY_NEAR_HERD_ID, "species": "Roe Deer", "x": QUARRY_NEAR_X, "y": QUARRY_NEAR_Y,
		"population": 90, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {"sustain": 0.20, "surplus": 0.80, "deplete": 0.40, "eradicate": 0.0},
		"per_worker_trade": 0.12, "trade_per_animal": QUARRY_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {"sustain": 0.03, "surplus": 0.12, "deplete": 0.06, "eradicate": 0.0},
		"hunt_trip_estimates": table.duplicate(true),
	}
	# A third huntable herd standing ON THE BAND'S TILE. A hunting party must still refuse it — there is
	# no expedition to make of game you are camped on — but a DENIAL raid must take it, because denial
	# erases a herd rather than harvesting one. It carries the same viable denial table as the boar, so
	# the two frames differ only in the WALK, which is the term under test.
	var home := {
		"id": QUARRY_HOME_HERD_ID, "species": QUARRY_HOME_SPECIES,
		"x": QUARRY_HOME_X, "y": QUARRY_HOME_Y,
		"population": 260, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.6, "food_per_animal": QUARRY_FOOD_PER_ANIMAL,
		"hunt_policy_ceilings": {"sustain": 0.25, "surplus": 1.00, "deplete": 0.50, "eradicate": 0.0},
		"per_worker_trade": 0.05, "trade_per_animal": QUARRY_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {"sustain": 0.02, "surplus": 0.08, "deplete": 0.04, "eradicate": 0.0},
		"denial_estimates": denial_table,
		"denial_party_needed": _denial_party_needed_for(denial_table),
	}
	_stamp_estimate_kits(near)
	_stamp_estimate_kits(home)
	return [herd, near, home]

## Stamp a herd with the kit id its two pre-launch estimate tables are quoted for — the HUNT job's
## default, which is what the sim writes on every herd whether or not it publishes either table.
## Mutates in place and returns nothing: every caller is building the dict it hands over.
static func _stamp_estimate_kits(herd: Dictionary) -> void:
	herd[KitRoster.HERD_TRIP_ESTIMATES_KIT_KEY] = BandFx.KIT_DEFAULT_HUNT
	herd[KitRoster.HERD_DENIAL_ESTIMATES_KIT_KEY] = BandFx.KIT_DEFAULT_HUNT

## **TWO ELIGIBLE QUARRIES ON ONE HEX** — the reported pair, both beyond the band's hunt reach so the
## picker accepts either. Their ORDER is the fixture's claim as much as their contents: the compose
## sheet is staged on the FIRST (the warren, what a tile click would resolve to), and reaching the
## second is exactly what the chooser exists for.
##
## The wolf is INEDIBLE, which is why it is the second herd rather than a second rabbit: it pays
## pelts and no meat, so the two rows read differently at every register a live server would produce
## them at — and a denial raid on it hauls trade goods and leaves no food on the range.
func _shared_tile_quarry_fixtures() -> Array:
	var food_herd := {
		"id": SHARED_TILE_FOOD_HERD_ID, "species": SHARED_TILE_FOOD_SPECIES,
		"x": SHARED_TILE_X, "y": SHARED_TILE_Y,
		"population": 320, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.9, "food_per_animal": SHARED_TILE_FOOD_PER_ANIMAL,
		"hunt_policy_ceilings": {"sustain": 0.40, "surplus": 1.40, "deplete": 0.70, "eradicate": 0.0},
		"per_worker_trade": 0.04, "trade_per_animal": SHARED_TILE_FOOD_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.02, "surplus": 0.07, "deplete": 0.04, "eradicate": 0.0,
		},
		"hunt_trip_estimates": _shared_tile_raid_table(
			SHARED_TILE_FOOD_PER_ANIMAL, SHARED_TILE_FOOD_TRADE_PER_ANIMAL),
		"denial_estimates": _denial_viable_rows(),
		"denial_party_needed": _denial_party_needed_for(_denial_viable_rows()),
	}
	var pelt_herd := {
		"id": SHARED_TILE_PELT_HERD_ID, "species": SHARED_TILE_PELT_SPECIES,
		"x": SHARED_TILE_X, "y": SHARED_TILE_Y,
		"population": 40, "ecology_phase": "thriving", "huntable": true,
		# No food account at all — an inedible quarry's provisions rate is a structural zero, not a
		# reading, so the whole food half is absent rather than set to 0.0.
		"per_worker_trade": 0.20, "trade_per_animal": SHARED_TILE_PELT_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.10, "surplus": 0.35, "deplete": 0.18, "eradicate": 0.0,
		},
		"hunt_trip_estimates": _shared_tile_raid_table(0.0, SHARED_TILE_PELT_TRADE_PER_ANIMAL),
		"denial_estimates": _denial_trade_only_rows(),
		"denial_party_needed": _denial_party_needed_for(_denial_trade_only_rows()),
	}
	return [food_herd, pelt_herd]

## A compact raid table for the shared-hex pair: one row per (floor sample × party size), with the
## payload derived from the species' own quanta. `food_per_animal == 0` is the INEDIBLE case — the
## quarry delivers no food at any party size, which is what `delivers_food` states.
func _shared_tile_raid_table(food_per_animal: float, trade_per_animal: float) -> Dictionary:
	var table := {}
	for i in SHARED_TILE_RAID_ANIMALS_ROW.size():
		var party := i + 1
		var animals := int(SHARED_TILE_RAID_ANIMALS_ROW[i])
		for floor_key in ["sustain", "surplus", "deplete", "eradicate"]:
			table["%s:%d" % [floor_key, party]] = {
				"turns_to_fill": SHARED_TILE_RAID_TURNS,
				"delivers_food": food_per_animal > 0.0,
				"delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * food_per_animal,
				"delivered_trade": float(animals) * trade_per_animal,
				"wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_PACK_FULL,
			}
	return table

## The viable denial table with its FOOD accounts struck out — the inedible quarry's version. A raid
## on a wolf pack kills the same animals and hauls the same pelts; there is no meat to bring home and
## none to leave rotting on the range, so both food halves are zero rather than the boar's numbers.
func _denial_trade_only_rows() -> Array:
	var rows: Array = []
	for row_variant in _denial_viable_rows():
		var row: Dictionary = (row_variant as Dictionary).duplicate(true)
		row["delivered_food"] = 0.0
		row["wasted_food"] = 0.0
		rows.append(row)
	return rows

## The DENIAL raid's pre-launch table — an ARRAY with ONE row per party size and no other axis, which
## is the whole shape difference from `hunt_trip_estimates` above: denial carries no floor and no fill
## target, so party size is the only thing there is to sample and a row's `party_workers` is its id.
##
## `outcome` is on every row because the client renders nothing numeric without it, and a `0` turn
## count means "not within the horizon on that end" rather than "immediately".
func _denial_rows(outcome: String, turns_row: Array, low_row: Array, high_row: Array,
		kills_row: Array) -> Array:
	var rows: Array = []
	for i in kills_row.size():
		var party := i + 1
		var killed := int(kills_row[i])
		var killed_food := float(killed) * QUARRY_FOOD_PER_ANIMAL
		# What the pack holds, never what it killed: the raid banks a rounding error on the way home.
		var hauled := minf(killed_food, float(party) * DENIAL_CARRY_PER_WORKER)
		var hauled_share := (hauled / killed_food) if killed_food > 0.0 else 0.0
		rows.append({
			"party_workers": party,
			"turns_to_collapse": int(turns_row[i]),
			"turns_to_collapse_low": int(low_row[i]),
			"turns_to_collapse_high": int(high_row[i]),
			"outcome": outcome,
			"animals_killed": killed,
			"delivered_food": hauled,
			"wasted_food": killed_food - hauled,
			"delivered_trade": float(killed) * QUARRY_TRADE_PER_ANIMAL * hauled_share,
		})
	return rows

## A raid that gets there: `past_recovery`, with a real turn band.
func _denial_viable_rows() -> Array:
	return _denial_rows(SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY,
		DENIAL_TURNS_ROW, DENIAL_LOW_ROW, DENIAL_HIGH_ROW, DENIAL_KILLS_ROW)

## A raid that never gets there: `repelled`, every turn row `0` (not within the horizon on either
## end) and a small but NON-ZERO kill count — the party is outbred, not incapable.
func _denial_repelled_rows() -> Array:
	var zeroes := [0, 0, 0, 0, 0, 0, 0, 0]
	return _denial_rows(SourceForecast.DENIAL_OUTCOME_REPELLED,
		zeroes, zeroes, zeroes, DENIAL_REPELLED_KILLS_ROW)

## **THE OPEN-HIGH TABLE — every row bounded on the expectation and the good run, unbounded on the
## bad one.** `high == 0` is the wire's own "not within the horizon on that end"; the sim really does
## publish this shape (a raid whose unlucky draws run past the 60-turn projection), and it is the shape
## the verdict copy shipped wrong. Flat across party sizes, since the claim is the SENTENCE and a
## descending table would only invite an assertion about which row was read.
func _denial_open_high_rows() -> Array:
	var kills: Array = []
	var turns: Array = []
	var low: Array = []
	var zeroes: Array = []
	for i in DENIAL_KILLS_ROW.size():
		kills.append(int(DENIAL_KILLS_ROW[i]))
		turns.append(DENIAL_OPEN_HIGH_TURNS)
		low.append(DENIAL_OPEN_HIGH_LOW)
		zeroes.append(0)
	return _denial_rows(SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY, turns, low, zeroes, kills)

## **A TABLE WITH THE REQUIREMENT INSIDE IT** — every party below `DENIAL_DEEP_PARTY_NEEDED` is
## `repelled`, that party and up are `past_recovery`. This is the shape the sim publishes for a herd
## whose requirement outruns `maxExpeditionPartySize`: the party axis runs to whichever of that
## ceiling and `denialPartyNeeded` is larger (`snapshot.fbs`), so the table STOPS at the requirement
## rather than at 8 — which is also why a stepper dialled past it quotes no verdict at all.
func _denial_needs_deep_party_rows() -> Array:
	var kills: Array = []
	var turns: Array = []
	var low: Array = []
	var high: Array = []
	var zeroes: Array = []
	for i in DENIAL_DEEP_PARTY_NEEDED:
		kills.append((i + 1) * DENIAL_DEEP_KILLS_PER_WORKER)
		turns.append(DENIAL_DEEP_TURNS)
		low.append(DENIAL_DEEP_TURNS_LOW)
		high.append(DENIAL_DEEP_TURNS_HIGH)
		zeroes.append(0)
	# Composed through `_denial_rows` twice rather than by hand, so both halves carry the payload
	# arithmetic (what the pack holds, what is left on the range) the rest of this fixture set uses.
	var repelled := _denial_rows(SourceForecast.DENIAL_OUTCOME_REPELLED, zeroes, zeroes, zeroes, kills)
	var viable := _denial_rows(SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY, turns, low, high, kills)
	var rows: Array = []
	for i in kills.size():
		rows.append(viable[i] if i + 1 >= DENIAL_DEEP_PARTY_NEEDED else repelled[i])
	return rows

## **`denialPartyNeeded`, DERIVED FROM THE TABLE RATHER THAN STATED BESIDE IT.** The field IS "the
## smallest party in `denialEstimates` whose raid SUCCEEDED", so a fixture that spelled it out
## separately could quote a party its own rows contradict — and every one of these tables would then
## have to be kept in step by hand. `DENIAL_PARTY_NEEDED_NONE` when no row succeeds, which is exactly
## what the sim publishes for a herd no quoted party drives down.
##
## **THE TEST IS `denial_outcome_succeeds`, NOT "is not `repelled`"**, and the difference is a shipped
## defect: `horizon` is neither, so the looser test quoted a row whose projection merely RAN OUT as the
## party that breaks the herd — a Wild Aurochs sheet opened at 5 under the verdict *"still standing when
## the forecast runs out"*. The success set lives in `SourceForecast` for the reason every other outcome
## key does: the client renders verdicts off these keys, and a second copy of "which outcomes count as
## success" is what let the two diverge.
## **THE `horizon` GUARD — the one shape no fixture in this file stages, and the one that shipped
## wrong.** Every denial table here is built from `past_recovery` and `repelled` alone, so "the first
## row that is not `repelled`" and "the first row that SUCCEEDED" agree on all four of them and the
## defect is invisible to every frame. This drives `_denial_party_needed_for` over a table whose first
## non-repelled row is a `horizon` — the projection ran its whole length with the herd still standing —
## and requires the derivation to walk PAST it to the row that actually breaks the herd.
##
## PNG-less on purpose: a table's quoted party is a number, not a picture, and the sheet renders the
## same plausible stepper whichever row it came from. Asserted directly over constructed rows, since
## the answer is a pure function and a fixture would only re-state it.
func _assert_denial_party_needed_skips_horizon() -> void:
	var outcomes := [
		SourceForecast.DENIAL_OUTCOME_REPELLED,
		SourceForecast.DENIAL_OUTCOME_HORIZON,
		SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY,
	]
	var rows: Array = []
	for i in outcomes.size():
		rows.append({
			SourceForecast.DENIAL_ESTIMATE_PARTY_KEY: i + 1,
			"outcome": String(outcomes[i]),
		})
	var horizon_party := 2
	var success_party := 3
	_assert_band_panel("a `horizon` row is NOT the party that breaks the herd (derived %d, wanted %d)"
			% [_denial_party_needed_for(rows), success_party],
		_denial_party_needed_for(rows) == success_party)
	# …and the negative half, or the claim above also passes on a derivation that simply took the LAST
	# row: a table whose only non-repelled row is a horizon quotes NO party at all.
	var no_success := rows.slice(0, horizon_party)
	_assert_band_panel("…and a table that never succeeds quotes no party at all (derived %d)"
			% _denial_party_needed_for(no_success),
		_denial_party_needed_for(no_success) == SourceForecast.DENIAL_PARTY_NEEDED_NONE)
	# **THE SUCCESS SET AND THE VERDICT TABLE'S `VERDICT_OK` SEVERITIES ARE ONE ANSWER**, stated twice
	# in `SourceForecast` twenty lines apart. `denial_outcome_succeeds` decides which party the sheet
	# opens on and the severity decides whether the Send wears the primary face; the two disagreeing
	# would offer a party under a warning face, or warn about one it had just recommended.
	var agree := true
	for outcome_variant in SourceForecast.DENIAL_VERDICTS:
		var outcome := String(outcome_variant)
		var severity_ok := String(SourceForecast.DENIAL_VERDICTS[outcome]["severity"]) \
			== SourceForecast.VERDICT_OK
		if SourceForecast.denial_outcome_succeeds(outcome) != severity_ok:
			agree = false
	_assert_band_panel("…and the success set is exactly the verdict table's VERDICT_OK entries", agree)

func _denial_party_needed_for(rows: Array) -> int:
	for row_variant in rows:
		var row: Dictionary = row_variant as Dictionary
		if SourceForecast.denial_outcome_succeeds(String(row.get("outcome", ""))):
			return int(row.get(SourceForecast.DENIAL_ESTIMATE_PARTY_KEY, 0))
	return SourceForecast.DENIAL_PARTY_NEEDED_NONE

## The PARSED text of the first `RichTextLabel` under `node` containing `text`, or `""`. The verdict
## and take lines are BBCode (`HudWidgets.forecast_label`), which `_has_label_containing` — a `Label`
## walk — cannot see at all; and it returns the WHOLE line rather than a bool because the claims below
## are about what the line does NOT also say, which a `contains` can never carry.
func _rich_text_containing(node: Node, text: String) -> String:
	if node is RichTextLabel:
		var parsed := (node as RichTextLabel).get_parsed_text()
		if parsed.contains(text):
			return parsed
	for child in node.get_children():
		var found := _rich_text_containing(child, text)
		if found != "":
			return found
	return ""

# ---- THE KIT PICKER (`docs/plan_denial_raid.md`) -------------------------------------------------

## Every rendered text line under `node`, in tree order — a `Label`'s `text` and a `RichTextLabel`'s
## PARSED text (BBCode stripped), skipping hidden nodes and blanks. It exists for the kit-mismatch
## claim, which is partly about what the sheet must NOT say: a `contains` search can only ever testify
## that something IS there, so the absence half needs the WHOLE list to compare against.
func _text_lines(node: Node) -> Array[String]:
	var lines: Array[String] = []
	if node is Control and not (node as Control).visible:
		return lines
	if node is RichTextLabel:
		var parsed := (node as RichTextLabel).get_parsed_text().strip_edges()
		if parsed != "":
			lines.append(parsed)
	elif node is Label:
		var text := (node as Label).text.strip_edges()
		if text != "":
			lines.append(text)
	for child in node.get_children():
		lines.append_array(_text_lines(child))
	return lines

## Drive the kit picker's popup through its REAL `id_pressed` dispatch, choosing the entry whose label
## begins with this kit's display name. By the POPUP, never by writing `ComposeState` — the pick path
## (menu → callback → `set_party_kit_id` → rerender) is half of what the frames claim.
func _pick_kit(kit_id: String) -> void:
	var menu := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as MenuButton
	if menu == null:
		_assert_band_panel("picking a kit needs the picker to exist", false)
		return
	var want := KitRoster.display_name_for_id(_hud._band_labor.kits(), kit_id)
	var popup := menu.get_popup()
	for i in popup.item_count:
		if popup.get_item_text(i).begins_with(want):
			popup.id_pressed.emit(popup.get_item_id(i))
			return
	_assert_band_panel("picking a kit needs an entry named %s (found %d entries)"
		% [want, popup.item_count], false)

## The picker CLOSED: it exists, its face names the selected kit, and the hint beneath it states this
## band's EFFECTIVE tier.
##
## **THE HINT IS THE CLAIM, and it is composed from the fixture's own numbers rather than through
## `KitRoster.tier_hint`** — an expectation re-derived through the function under test asserts
## nothing. The carry is the BARE tier while the roster publishes 40 for this kit, so a hint quoting
## the fresh number fails here and nowhere else; the attack is the EQUIPPED one on the same line,
## which is what stops "quote the bare tier for everything" passing instead.
func _assert_kit_picker_closed() -> void:
	var menu := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as MenuButton
	_assert_band_panel("the denial sheet carries a Kit picker", menu != null)
	if menu == null:
		return
	var face := HudComposeVocab.KIT_PICKER_FACE_FORMAT % [
		String(HudComposeVocab.KIT_JOB_GLYPHS[KitRoster.JOB_HUNT]), "Big-game kit"]
	_assert_band_panel("…whose face names the selected kit (\"%s\")" % menu.text,
		menu.text == face)
	var hint := HudComposeVocab.KIT_HINT_SEPARATOR.join([
		HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(BandFx.KIT_ATTACK_EQUIPPED,
			HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(BandFx.KIT_HUNT_CARRY_BARE,
			HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [HudComposeVocab.KIT_COMPONENT_SPEARS,
			int(KIT_FRAME_SPEARS_CONDITION)],
		HudComposeVocab.KIT_HINT_DRY_FORMAT % HudComposeVocab.KIT_COMPONENT_SLED,
	])
	var rendered := _find_meta_control(_panel, KitRoster.KIT_HINT_META) as Label
	_assert_band_panel("…over a hint stating the EFFECTIVE tier, not the fresh one — \"%s\"" % hint,
		rendered != null and rendered.text == hint)

## The picker OPEN. A screenshot cannot say which entry carries the radio dot, so the structure rides
## here: the roster's hunt kits and only those, the composed one marked, the job default TAGGED, and
## `none` LAST — which it is because the ROSTER authors it last and this client sorts nothing.
func _assert_kit_picker_open(menu: MenuButton) -> void:
	_assert_band_panel("the Kit picker opens a menu", menu != null)
	if menu == null:
		return
	var popup := menu.get_popup()
	var labels: Array[String] = []
	for i in popup.item_count:
		labels.append(popup.get_item_text(i))
	# The GATHERING kit lists `forage` alone, so its absence is the filter working rather than a
	# roster that happens to hold two entries.
	var want_labels: Array[String] = [
		"Big-game kit" + HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX, "No kit"]
	_assert_band_panel("…listing exactly this verb's kits, the default tagged, `none` last — %s"
			% str(labels),
		labels == want_labels)
	var checked: Array[String] = []
	for i in popup.item_count:
		if popup.is_item_checked(i):
			checked.append(popup.get_item_text(i))
	_assert_band_panel("…marking exactly the composed kit (%s)" % str(checked),
		checked.size() == 1 and String(checked[0]).begins_with("Big-game kit"))

## **THE KIT-MISMATCH STATE, ASSERTED BY EQUALITY** — `none` composed against tables quoted for
## `big_game`.
##
## The claim is half an ABSENCE (no collapse verdict, no estimate caveat, no take line, no counted
## refusal — every one of them a figure computed for a raid the player is not sending) and a
## `contains` search can only testify that something IS present. So the sheet's lines BELOW the kit
## hint are compared to the exact expected list: the combat gate, which is composed from wire terms
## and is honest at any tier, then the sentence naming the kit whose numbers were withheld. A verdict
## put back fails this, and so does a gate line dropped.
func _assert_kit_mismatch_suppresses_estimates() -> void:
	var hint := _find_meta_control(_panel, KitRoster.KIT_HINT_META) as Label
	_assert_band_panel("the kit-mismatch sheet still states the picked kit's tier", hint != null)
	if hint == null:
		return
	var lines := _text_lines(_panel)
	var at := lines.find(hint.text)
	_assert_band_panel("…and that hint is on the sheet the assertion walks", at >= 0)
	if at < 0:
		return
	var tail := lines.slice(at + 1)
	# The gate at the BARE-handed tier against this quarry's defense: the effective attack is 0, so it
	# refuses outright — the honest verdict for a party carrying nothing, and the one thing this sheet
	# can still say. Composed from the vocabulary, never through `hunt_gate_model_at`.
	var gate := SourceForecast.HUNT_GATE_BLOCKED_FORMAT % [
		SourceForecast.HUNT_FORECAST_WARN_GLYPH, "Wild Boar",
		String.num(BandFx.KIT_ATTACK_BARE, SourceForecast.HUNT_GATE_SCALAR_DECIMALS),
		String.num(QUARRY_DEFENSE, SourceForecast.HUNT_GATE_SCALAR_DECIMALS)]
	var note := HudComposeVocab.KIT_DENIAL_ESTIMATES_QUOTED_FORMAT % ["Big-game kit", "No kit"]
	var want: Array[String] = [gate, note]
	_assert_band_panel(("…and below it says EXACTLY the gate and the quoted-kit note — "
			+ "no verdict, no caveat, no take, no refusal. Got %s") % str(tail),
		tail == want)
	# The send stays LIVE: the raid is perfectly launchable, we simply cannot quote its length. A
	# disabled button here would read as the kit being illegal, which it is not.
	var confirm := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…while the Send stays live — the raid launches, only its length is unquotable",
		confirm != null and not confirm.disabled)

## The VIABLE denial form. The ABSENCES are half the claims — what this mission does not carry IS its
## specification, so a form that grew a floor picker would be as wrong as one that quoted no verdict.
func _assert_denial_viable() -> void:
	var quarry := "Wild Boar"
	# Composed from the VOCABULARY, never from `denial_verdict_text` — an expectation re-derived
	# through the formatter under test asserts nothing.
	var want := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["line"]) % quarry
	# **THE RANGE IS FROM LAUNCH, SO BOTH ENDS CARRY THE WALK OUT.** The sim's table counts raiding
	# turns; the party has to get there first, and the HUNT form on this same sheet has always
	# headlined a round-trip total — so an unqualified collapse count read as the same span and was
	# short by the outbound leg. The expectation is stated from the harness's side (the constant
	# below, derived from this fixture's own geometry) so the two arrive at one string from opposite
	# ends; re-deriving it through `outbound_travel_turns` would assert nothing.
	# **THE EXPECTATION LEADS AND THE SPREAD FOLLOWS IT**, because the take line under this sentence is
	# priced at the expectation: a verdict leading with the lucky end describes a different raid from
	# the kill count two rows down.
	want += SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
		SourceForecast.DENIAL_TURNS_ONE_FORMAT % (
			DENIAL_TURNS_ROW[DENIAL_PARTY - 1] + DENIAL_OUTBOUND_TRAVEL_TURNS),
		SourceForecast.DENIAL_SPAN_FROM_LAUNCH]
	want += SourceForecast.DENIAL_SPREAD_RANGE_FORMAT % [
		DENIAL_LOW_ROW[DENIAL_PARTY - 1] + DENIAL_OUTBOUND_TRAVEL_TURNS,
		DENIAL_HIGH_ROW[DENIAL_PARTY - 1] + DENIAL_OUTBOUND_TRAVEL_TURNS]
	want += SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT % DENIAL_OUTBOUND_TRAVEL_TURNS
	_assert_band_panel("the denial form leads with the EXPECTATION and states the spread — \"%s\"" % want,
		_rich_text_containing(_panel, want) == want)
	# **THE WASTE IS STATED AND IS NOT DRESSED AS A WARNING.** On a hunt an unhauled kill wears
	# `HUNT_FORECAST_WARN_GLYPH`; on a raid it IS the mission, so the line is quiet and factual. Both
	# halves on ONE line — the take is there, and it carries no alarm — since a form that lost the
	# line entirely would satisfy the negative on its own.
	var killed: int = DENIAL_KILLS_ROW[DENIAL_PARTY - 1]
	var killed_food := float(killed) * QUARRY_FOOD_PER_ANIMAL
	var left := killed_food - minf(killed_food, float(DENIAL_PARTY) * DENIAL_CARRY_PER_WORKER)
	var take_line := _rich_text_containing(_panel,
		SourceForecast.DENIAL_TAKE_KILLS_FORMAT % [killed, quarry])
	_assert_band_panel("…and states the take PLAINLY — kills %d, leaves %s on the range, no alarm"
			% [killed, SourceForecast.format_magnitude(left)],
		take_line.contains(SourceForecast.DENIAL_TAKE_LEFT_FORMAT
				% SourceForecast.format_magnitude(left))
			and not take_line.contains(SourceForecast.HUNT_FORECAST_WARN_GLYPH))
	# NO FLOOR ANYWHERE — not a picker, not a fill target, not even the row heading. Three surfaces,
	# one claim. **The heading is matched UPPER-CASED because `alloc_section_label` upper-cases what it
	# is given**, so the vocabulary const as written matches nothing and that clause would be vacuous
	# — which is exactly how it first shipped, passing with a Policy row put back on the form.
	_assert_band_panel("…and offers NO floor picker, NO fill target and no Policy row",
		_find_meta_control(_panel, HudWidgets.POLICY_RUNG_META) == null
			and _find_meta_control(_panel, HudWidgets.FILL_TARGET_META) == null
			and not _has_label_containing(_panel, HudComposeVocab.COMPOSE_FIELD_POLICY.to_upper()))
	# **THE BAND IS AN ESTIMATE, NOT A PROMISE, AND THE PANEL SAYS SO** — `turns_to_collapse` is an
	# integral over many stochastic retreat draws, so a lucky run really can finish sooner than the
	# reported low. The caveat rides under every verdict that quotes a number (and, per the repelled
	# frame, under none that does not).
	_assert_band_panel("…and words the band as an estimate rather than a promise",
		_has_label_containing(_panel, SourceForecast.DENIAL_ESTIMATE_CAVEAT))
	# The Send is the plain primary one and is ENABLED — this raid works.
	var send := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…and its Send is the plain primary one, enabled",
		send != null and not send.disabled
			and send.text == String(SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["button"]))

## The IN-REACH form — the same viable verdict on a quarry the band is camped on top of. **Its claim
## is the TRAVEL TERM**: the walk out is genuinely zero, so both ends of the collapse band are the
## sim's own numbers unshifted, the sentence still names its span ("from launch", never bare), and the
## breakdown clause is ABSENT rather than reading "(0 of them travel)" — a term for nothing.
func _assert_denial_in_reach_verdict() -> void:
	var want := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["line"]) % QUARRY_HOME_SPECIES
	want += SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
		SourceForecast.DENIAL_TURNS_ONE_FORMAT % (
			DENIAL_TURNS_ROW[DENIAL_PARTY - 1] + QUARRY_HOME_OUTBOUND_TRAVEL_TURNS),
		SourceForecast.DENIAL_SPAN_FROM_LAUNCH]
	want += SourceForecast.DENIAL_SPREAD_RANGE_FORMAT % [
		DENIAL_LOW_ROW[DENIAL_PARTY - 1] + QUARRY_HOME_OUTBOUND_TRAVEL_TURNS,
		DENIAL_HIGH_ROW[DENIAL_PARTY - 1] + QUARRY_HOME_OUTBOUND_TRAVEL_TURNS]
	# EQUALITY, so the absence rides in the same claim: a line that also appended a travel clause is a
	# different string and fails here rather than passing a `contains`.
	_assert_band_panel("a quarry inside hunt reach is raidable, and reads sensibly at zero travel — \"%s\"" % want,
		_rich_text_containing(_panel, want) == want)
	# …stated again on its own, because the equality above would also be satisfied by a form that lost
	# the verdict entirely, and this is the clause the zero-travel case exists to keep out.
	_assert_band_panel("…and appends no travel split, there being no travel to split off",
		_rich_text_containing(_panel, SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT
			% QUARRY_HOME_OUTBOUND_TRAVEL_TURNS) == "")

## The REPELLED form. **The verdict is about the PARTY, and the herd-side sentence must be absent** —
## this arc has already shipped a refusal that blamed the herd for the party's problem twice, and the
## negative half is what makes the positive one mean something.
func _assert_denial_repelled() -> void:
	var quarry := "Wild Boar"
	var party_line := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_REPELLED]["line"]) % quarry
	var horizon_line := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_HORIZON]["line"]) % quarry
	# **AND THE WHOLE LINE IS THE OUTCOME, WITH NO TURN CLAUSE APPENDED.** Equality, not `contains`:
	# the outcome LEADS the sentence and the number is a clause on it, so "never a blank turn count
	# without its outcome" is only true if a forecast the sim bounded on neither end renders the
	# outcome ALONE. A `contains` would pass on a line that also quoted a number.
	_assert_band_panel("a repelled raid is refused in the PARTY's name, with no turn count — \"%s\""
			% party_line,
		_rich_text_containing(_panel, party_line) == party_line
			and _rich_text_containing(_panel, horizon_line) == "")
	# **AND NO CAVEAT, because there is no number to caveat.** `DENIAL_ESTIMATE_CAVEAT` qualifies a
	# turn band; printed under a verdict that quotes none it reads as an estimate the player cannot
	# see. Asserted as a PAIR with the viable frame, which requires it.
	_assert_band_panel("…and prints no estimate caveat, having quoted no estimate",
		not _has_label_containing(_panel, SourceForecast.DENIAL_ESTIMATE_CAVEAT))
	# It STILL LAUNCHES: a raid that cannot get there keeps working the herd until it is recalled, so
	# the launch verdict warns and the player is trusted — exactly as a slow hunting raid is.
	var send := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…and the Send warns rather than blocking",
		send != null and not send.disabled
			and send.text == String(SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_REPELLED]["button"]))
	# …and it says what to do about it, in the party's terms. **The NUMBERLESS form is the right one
	# HERE**: every row of this table is repelled, so the sim quotes no party at all
	# (`DENIAL_PARTY_NEEDED_NONE`) and there is nothing honest to name — the counted twin rides
	# `band_panel_compose_deny_short_party`, and the pair is what makes either mean anything.
	_assert_band_panel("…and the reason beside it sends the player to the PARTY",
		_has_label_containing(_panel, String(SourceForecast.DENIAL_VERDICTS[
			SourceForecast.DENIAL_OUTCOME_REPELLED]["reason"]) % quarry))

## **THE REPORTED VERDICT — a bounded expectation over an unbounded bad run.** Two claims in one
## EQUALITY, which is why it is an equality and not a `contains`: the sentence must LEAD with the
## expectation (the figure the take line beneath it is priced at), and it must SAY the bad run may not
## finish rather than dropping that end. A `contains` on the expectation alone would pass on a line
## that also quoted the lucky end as the answer, which is the defect.
func _assert_denial_open_high_verdict() -> void:
	var quarry := "Wild Boar"
	var want := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["line"]) % quarry
	want += SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
		SourceForecast.DENIAL_TURNS_ONE_FORMAT % (
			DENIAL_OPEN_HIGH_TURNS + DENIAL_OUTBOUND_TRAVEL_TURNS),
		SourceForecast.DENIAL_SPAN_FROM_LAUNCH]
	want += SourceForecast.DENIAL_SPREAD_OPEN_HIGH_FORMAT % (
		DENIAL_OPEN_HIGH_LOW + DENIAL_OUTBOUND_TRAVEL_TURNS)
	want += SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT % DENIAL_OUTBOUND_TRAVEL_TURNS
	_assert_band_panel("an unbounded bad run still leads with the expectation — \"%s\"" % want,
		_rich_text_containing(_panel, want) == want)
	# **AND THE CAVEAT STILL RIDES UNDER IT**, this verdict quoting numbers. The caveat is gated on
	# `denial_turns_phrase`, which the rewrite re-pointed at the lead figure — a gate that answered
	# `""` here would silently drop the caveat from exactly the shape that most needs qualifying.
	_assert_band_panel("…and the estimate caveat rides under it, a number having been quoted",
		_has_label_containing(_panel, SourceForecast.DENIAL_ESTIMATE_CAVEAT))

## **THE FIVE CLAUSE SHAPES, DRIVEN DIRECTLY.** Only two of them are reachable from a rendered frame
## (the ordinary range and the open high), and the other three are exactly the ends where a lone
## optimistic number could reappear. PNG-less for the reason the horizon guard is: a turn clause is a
## string, and the sheet renders a plausible-looking sentence whichever draw it led with.
func _assert_denial_turn_clause_shapes() -> void:
	var travel := 1
	var of_raiding := SourceForecast.DENIAL_TRAVEL_UNKNOWN
	# 1 — all three bounded: the expectation leads, the spread follows, the split closes.
	var ordinary := SourceForecast.denial_turns_clause({
		"turns": 20, "low": 12, "high": 31, SourceForecast.DENIAL_TRAVEL_KEY: travel})
	var want_ordinary := SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
			SourceForecast.DENIAL_TURNS_ONE_FORMAT % 20, SourceForecast.DENIAL_SPAN_FROM_LAUNCH] \
		+ SourceForecast.DENIAL_SPREAD_RANGE_FORMAT % [12, 31] \
		+ SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT % travel
	_assert_band_panel("a bounded band leads with the expectation — \"%s\"" % ordinary,
		ordinary == want_ordinary)
	# 3 — the EXPECTATION itself is unbounded, so only luck gets there. This is the one shape whose
	# lead is the good run, and it must SAY the raid is not expected to finish.
	var lucky := SourceForecast.denial_turns_clause({
		"turns": 0, "low": 12, "high": 0, SourceForecast.DENIAL_TRAVEL_KEY: travel})
	var want_lucky := SourceForecast.DENIAL_ONLY_GOOD_RUN_LEAD_FORMAT % [
			SourceForecast.DENIAL_TURNS_ONE_FORMAT % 12, SourceForecast.DENIAL_SPAN_FROM_LAUNCH] \
		+ SourceForecast.DENIAL_SPREAD_NOT_EXPECTED \
		+ SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT % travel
	_assert_band_panel("an unbounded expectation says the raid is not expected to finish — \"%s\"" % lucky,
		lucky == want_lucky)
	# 4 — `low == high`: the distribution is degenerate, so the lead IS the whole answer and no spread
	# renders. "between 8 and 8 depending on the run" is a spread for nothing.
	var degenerate := SourceForecast.denial_turns_clause({
		"turns": 8, "low": 8, "high": 8, SourceForecast.DENIAL_TRAVEL_KEY: 0})
	_assert_band_panel("a degenerate band renders no spread — \"%s\"" % degenerate,
		degenerate == SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
			SourceForecast.DENIAL_TURNS_ONE_FORMAT % 8, SourceForecast.DENIAL_SPAN_FROM_LAUNCH])
	# 5 — nothing bounded: no clause at all, so the outcome word stands alone. The structural half of
	# "never a blank turn count without its outcome".
	_assert_band_panel("a forecast bounded on no end renders no clause",
		SourceForecast.denial_turns_clause({
			"turns": 0, "low": 0, "high": 0, SourceForecast.DENIAL_TRAVEL_KEY: travel}) == "")
	# **THE IN-FLIGHT SPAN IS THE OTHER HALF OF EVERY SHAPE**, and it is asserted here rather than left
	# to the drawer's own frame: the span is chosen once for the whole clause, so a rewrite that named
	# it per branch would leave the launch sheet right and the drawer quietly telling a party already
	# out that its band starts when it leaves.
	var in_flight := SourceForecast.denial_turns_clause({
		"turns": 20, "low": 12, "high": 31, SourceForecast.DENIAL_TRAVEL_KEY: of_raiding})
	_assert_band_panel("…and a bandless forecast names the RAIDING span, never the launch one",
		in_flight.contains(SourceForecast.DENIAL_SPAN_OF_RAIDING)
			and not in_flight.contains(SourceForecast.DENIAL_SPAN_FROM_LAUNCH)
			and not in_flight.contains(SourceForecast.DENIAL_TRAVEL_SPLIT_FORMAT % 0))

## **THE DEEP PARTY** — a band whose idle workforce outnumbers `max_expedition_party_size`, on a quarry
## whose requirement outruns it too. Two claims, and neither is legible in the frame alone: the sheet
## OPENS on the party the sim quotes, and the stepper's ceiling is the band's own idle workers rather
## than the estimate tables' sampling axis.
func _assert_denial_deep_party() -> void:
	_assert_band_panel("the denial stepper opens on the party the sim quotes (%d, wanted %d)"
			% [_hud._bandpanel._send_expedition_count, DENIAL_DEEP_PARTY_NEEDED],
		_hud._bandpanel._send_expedition_count == DENIAL_DEEP_PARTY_NEEDED)
	# …and it is a party the OLD cap could not even be dialled to, which is what makes the seed a
	# change in what the form can express rather than a different default.
	_assert_band_panel("…a party past `max_expedition_party_size` (%d)"
			% int(_deep_party_band_fixture().get("max_expedition_party_size", 0)),
		DENIAL_DEEP_PARTY_NEEDED > int(_deep_party_band_fixture().get("max_expedition_party_size", 0)))
	# **THE CEILING IS THE BAND'S IDLE WORKFORCE**, driven through the render's OWN clamp rather than
	# read off the stepper's face: under the retired cap a count of 12 came back as 8. This leaves the
	# panel on a party the table quotes no row for — a real state, and the next frame re-renders anyway.
	_hud._bandpanel._send_expedition_count = DENIAL_DEEP_PARTY_IDLE
	_hud._bandpanel.rerender()
	_assert_band_panel("…and the party may be dialled to the band's whole idle workforce (%d of %d)"
			% [_hud._bandpanel._send_expedition_count, DENIAL_DEEP_PARTY_IDLE],
		_hud._bandpanel._send_expedition_count == DENIAL_DEEP_PARTY_IDLE)

## **A REPELLED RAID NAMES THE PARTY IT WOULD TAKE, WHENEVER THE SIM QUOTES ONE.** "Send more hunters"
## is correct on the merits and useless in hand — it prescribes hands without saying how many — while
## `denialPartyNeeded` has been on the wire all along. Composed from the VOCABULARY, never from
## `denial_refusal_reason`: an expectation re-derived through the code under test asserts nothing.
func _assert_denial_counted_refusal() -> void:
	var quarry := "Wild Boar"
	var want := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_REPELLED]["reason_counted"]) % [quarry, DENIAL_DEEP_PARTY_NEEDED]
	_assert_band_panel("a repelled raid's reason NAMES the party it takes — \"%s\"" % want,
		_has_label_containing(_panel, want))
	# …and the numberless sentence is GONE rather than printed beside it: with a figure in hand it is
	# the sentence this replaces, and a sheet carrying both states the remedy twice.
	var bare := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_REPELLED]["reason"]) % quarry
	_assert_band_panel("…and not the numberless sentence beside it",
		not _has_label_containing(_panel, bare))
	# **AND THE SEND IS STILL LIVE — the companion half of the disable rule.** This party is under-sized
	# BY CHOICE (the band can field 12 and the player dialled 4), which is the warn-and-trust case:
	# a raid that cannot break the herd keeps working it until recalled. Without this claim the
	# short-handed assertion below would pass on a sheet that disabled the Send for every repelled row.
	var send := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…and a party the PLAYER under-sized still launches",
		send != null and not send.disabled
			and send.text == String(SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_REPELLED]["button"]))

## **THE ONE STATE IN WHICH THIS SHEET REFUSES.** The band cannot field the party the herd requires at
## all — there is no stepper setting that reaches it — so the Send goes visible-and-disabled with its
## reason, the sheet's no-quarry convention. Composed from the VOCABULARY, and read as a PAIR with
## `_assert_denial_counted_refusal`'s live Send: a rule that disabled every repelled raid would pass
## the disable claim alone.
func _assert_denial_short_handed() -> void:
	var quarry := "Wild Boar"
	var idle := _hud._band_labor.effective_idle(_hud._band_labor.panel_band())
	_assert_band_panel("the band can field %d of the %d hunters this herd needs — the precondition"
			% [idle, DENIAL_DEEP_PARTY_NEEDED],
		idle < DENIAL_DEEP_PARTY_NEEDED)
	var send := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…so the Send is DISABLED and says which shortfall it is",
		send != null and send.disabled
			and send.text == SourceForecast.DENIAL_SHORT_HANDED_BUTTON)
	var want := SourceForecast.DENIAL_SHORT_HANDED_REASON_FORMAT % [
		quarry, DENIAL_DEEP_PARTY_NEEDED, idle]
	_assert_band_panel("…and the reason beneath it names BOTH numbers — \"%s\"" % want,
		_has_label_containing(_panel, want))
	# …and it SUPERSEDES the repelled refusal rather than printing beside it: both name the party the
	# sim quotes, so a sheet carrying the pair states the requirement twice.
	var counted := String(SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_REPELLED]["reason_counted"]) % [quarry, DENIAL_DEEP_PARTY_NEEDED]
	_assert_band_panel("…and the counted refusal is not printed beside it",
		not _has_label_containing(_panel, counted))

## **THE CHOOSER APPEARS ONLY WHERE THERE IS A CHOICE, AND CHOOSING RE-TARGETS.** Both halves are
## behavioural: a PNG can show that a `⋯` is on the Quarry row, but not what its menu holds, not which
## herd it marks as current, and not what a pick does. The frame under it is the picture; this is the
## claim.
##
## The ABSENCE half rides `band_panel_compose_hunt` (one eligible quarry on the boar's hex, so no
## chooser) — the pair is what makes either mean something, since a control rendered unconditionally
## satisfies every assertion here on its own.
func _assert_quarry_chooser() -> void:
	var menu := _find_meta_control(_panel, HudWidgets.QUARRY_CHOICES_META) as MenuButton
	_assert_band_panel("two herds on one hex put a chooser on the Quarry row", menu != null)
	if menu == null:
		return
	var popup := menu.get_popup()
	_assert_band_panel("…offering exactly the hex's two eligible quarries (found %d)"
			% popup.item_count,
		popup.item_count == 2)
	# **EXACTLY ONE ITEM IS MARKED, and it is the composed one.** A menu of plain items could not say
	# which herd the sheet is aimed at, which is the whole reason the entries are radio-check items;
	# "some item is checked" would pass on a menu that marked both.
	var checked: Array = []
	for i in popup.item_count:
		if popup.is_item_checked(i):
			checked.append(popup.get_item_text(i))
	_assert_band_panel("…marking exactly the composed quarry (%s)" % str(checked),
		checked.size() == 1 and String(checked[0]).contains(SHARED_TILE_FOOD_SPECIES))
	# **CHOOSING THE OTHER ONE RE-TARGETS**, driven through the REAL `id_pressed` wiring rather than by
	# calling the entry's callback — the popup's own dispatch is part of what is being asserted. The
	# stale fill target is staged first: it counts the WARREN's animals, so the switch must drop it
	# rather than hand it to the wolf, where a target at or above capacity is a lever that does nothing.
	_hud._compose.set_party_fill_target(SHARED_TILE_STALE_FILL_TARGET)
	var other := -1
	for i in popup.item_count:
		if not popup.is_item_checked(i):
			other = i
	popup.id_pressed.emit(popup.get_item_id(other))
	_assert_band_panel("…and choosing the other one re-targets the sheet (%s)"
			% _hud._compose.party_quarry_id(),
		_hud._compose.party_quarry_id() == SHARED_TILE_PELT_HERD_ID)
	_assert_band_panel("…dropping the fill target, which counted the OTHER herd's animals (%d)"
			% _hud._compose.party_fill_target(),
		_hud._compose.party_fill_target() == SourceForecast.NO_FILL_TARGET)
	# …and the sheet REBUILT against the new quarry: the chooser is a fresh node now, and it must mark
	# the wolf. Reading the model back alone would pass on a switch that never re-rendered.
	var after := _find_meta_control(_panel, HudWidgets.QUARRY_CHOICES_META) as MenuButton
	var after_checked := ""
	if after != null:
		var after_popup := after.get_popup()
		for i in after_popup.item_count:
			if after_popup.is_item_checked(i):
				after_checked = after_popup.get_item_text(i)
	_assert_band_panel("…and the re-rendered row marks the herd now composed (%s)" % after_checked,
		after_checked.contains(SHARED_TILE_PELT_SPECIES))

## The tile_info a map click on a herd's hex delivers (`TargetingController._huntable_herd_on_tile` reads `herds`).
func _quarry_tile_info(herd: Dictionary) -> Dictionary:
	return {"x": int(herd["x"]), "y": int(herd["y"]), "herds": [herd]}

## A hunting PARTY is for game the band cannot work from home, so the quarry picker must refuse a herd
## inside the band's `hunt_reach` (`TargetingController.is_expedition_quarry`) — the near herd is a LOCAL hunt. This
## is behavioural, not pictorial: the refusal happens at the click, which no frame can show. Verified
## to FAIL (the near herd is accepted, `_compose.party_quarry_id()` = the near id) with the eligibility test
## removed from `TargetingController._try_pick_quarry`.
func _assert_quarry_eligibility() -> void:
	var herds := _quarry_herd_fixtures()
	var far: Dictionary = herds[0]
	var near: Dictionary = herds[1]
	_set_world_herds(herds)
	# NEAR — inside hunt reach: refused, and targeting stays armed so the player can pick again.
	_hud._compose.clear_party_quarry()
	_hud._targeting._pending_pick_quarry = {"band": _band_fixture()}
	_hud._targeting._try_pick_quarry(_quarry_tile_info(near))
	assert(_hud._compose.party_quarry_id() == "",
		"band_panel_preview: a herd INSIDE hunt reach was accepted as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	assert(not _hud._targeting._pending_pick_quarry.is_empty(),
		"band_panel_preview: the refused pick dropped out of targeting instead of staying armed")
	# FAR — beyond hunt reach: accepted, and the pick ends targeting.
	_hud._targeting._try_pick_quarry(_quarry_tile_info(far))
	assert(_hud._compose.party_quarry_id() == QUARRY_FAR_HERD_ID,
		"band_panel_preview: a herd BEYOND hunt reach was refused as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	_hud._targeting._pending_pick_quarry = {}
	_hud._compose.clear_party_quarry()
	print("band_panel_preview: assert OK — quarry picker takes the far herd, refuses the near one")

## **THE BEYOND-REACH RULE BELONGS TO THE HUNT, NOT TO THE EXPEDITION** (reported from play: deer and
## rabbit a few tiles from camp were not offered as denial targets while herds further out were). A
## denial raid is not a way of GETTING food, it is a way of ERASING a herd, so a quarry the band could
## work from home is a coherent order — one hunting it at floor 0 cannot express, being carry-bounded
## and stopping at the pack. Both halves are driven against the SAME herd, because the claim is a
## DIFFERENCE between the missions: an assertion that only took the denial pick would be satisfied by
## dropping the rule from the hunt as well, which is the regression this pins against.
##
## Behavioural, not pictorial — the accept and the refusal both happen at the click. The GLOW is
## asserted here too (`min_distance`, the number MapView filters on): the halo must never promise a
## target the pick refuses nor hide one it would take, and a mission-blind glow beside a mission-aware
## pick is exactly that disagreement.
func _assert_denial_quarry_eligibility() -> void:
	var herds := _quarry_herd_fixtures()
	var home: Dictionary = herds[2]
	_set_world_herds(herds)
	# DENY, on a herd standing on the band's own tile — the extreme of "in reach". Taken, and the pick
	# ends targeting like any other.
	_hud._compose.clear_party_quarry()
	_hud._targeting._pending_pick_quarry = _pending_quarry_pick(HudComposeVocab.COMPOSE_MISSION_DENY)
	_hud._targeting._try_pick_quarry(_quarry_tile_info(home))
	assert(_hud._compose.party_quarry_id() == QUARRY_HOME_HERD_ID,
		"band_panel_preview: a DENIAL raid refused a herd inside hunt reach (%s)" \
		% _hud._compose.party_quarry_id())
	assert(_hud._targeting._pending_pick_quarry.is_empty(),
		"band_panel_preview: the accepted denial pick stayed armed instead of resolving")
	# …and the SAME herd under HUNT: still refused, still armed. This is the pin that says the fix did
	# not weaken the hunt's rule.
	_hud._compose.clear_party_quarry()
	_hud._targeting._pending_pick_quarry = _pending_quarry_pick(HudComposeVocab.COMPOSE_MISSION_HUNT)
	_hud._targeting._try_pick_quarry(_quarry_tile_info(home))
	assert(_hud._compose.party_quarry_id() == "",
		"band_panel_preview: a HUNT expedition accepted a herd on the band's own tile (%s)" \
		% _hud._compose.party_quarry_id())
	assert(not _hud._targeting._pending_pick_quarry.is_empty(),
		"band_panel_preview: the refused hunt pick dropped out of targeting instead of staying armed")
	# The glow's own filter, read off the targeting descriptor MapView is handed.
	var hunt_min := int(_hud._targeting._current_targeting_info().get("min_distance", -99))
	assert(hunt_min == QUARRY_BAND_HUNT_REACH,
		"band_panel_preview: a hunt pick glows at min_distance %d, not the band's hunt_reach %d" \
		% [hunt_min, QUARRY_BAND_HUNT_REACH])
	_hud._targeting._pending_pick_quarry = _pending_quarry_pick(HudComposeVocab.COMPOSE_MISSION_DENY)
	var deny_min := int(_hud._targeting._current_targeting_info().get("min_distance", -99))
	assert(deny_min == TargetingController.QUARRY_NO_REACH_BOUND,
		"band_panel_preview: a denial pick glows at min_distance %d, not %d (every visible herd)" \
		% [deny_min, TargetingController.QUARRY_NO_REACH_BOUND])
	_hud._targeting._pending_pick_quarry = {}
	_hud._compose.clear_party_quarry()
	print("band_panel_preview: assert OK — denial takes the herd on the band's own tile, the hunt still refuses it, and both glows agree")

## An armed quarry pick for `mission`, in the shape `TargetingController.begin_pick_quarry` builds.
func _pending_quarry_pick(mission: String) -> Dictionary:
	return {
		"band": _band_fixture(),
		TargetingController.PICK_QUARRY_MISSION_KEY: mission,
	}

## Herds for the per-source-cap verify state: game_deer_07 carries the pre-commit forecast fields the
## Current-actions Hunt row reads via `HudBandLaborState.find_world_herd` + `SourceForecast.forecast_inputs` — `per_worker_yield`
## plus the herd's ONLY ceiling representation, the `hunt_policy_ceilings` table (a herd has no flat
## `ceiling_*` scalars; the forage patches below still do).
## max-useful = ceil(0.20 / 0.10) = 2, so a Hunt row staffed at 2 is AT its cap.
func _cap_demo_herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120,
			"ecology_phase": "thriving", "per_worker_yield": 0.10,
			"hunt_policy_ceilings": {"sustain": 0.20}},
	]

## Give a RAW wire patch the per-policy ROWS the decoder now builds — the six policy-keyed dicts that
## are a patch's only ceiling representation (#426). Every rung gets the same ceiling and per-worker
## term, which is all these cap fixtures need; the two non-food accounts stay absent, so the
## render-only-when-non-zero rule leaves every frame unchanged. The ui_preview twin is
## `BaseFx.seed_forage_rows`, which derives its numbers from `patch_`-prefixed tile_info keys instead.
func _wire_patch_rows(patch: Dictionary, ceiling: float) -> Dictionary:
	var ceilings := {}
	var per_worker := {}
	for policy in ["sustain", "surplus", "deplete", "eradicate", "cultivate", "sow"]:
		ceilings[policy] = ceiling
		per_worker[policy] = float(patch.get("per_worker_yield", 0.0))
	patch["forage_policy_ceilings"] = ceilings
	patch["forage_policy_per_worker"] = per_worker
	return patch

## Forage patches for the per-source-cap verify state (shape `update_forage_patches` consumes — the RAW
## wire dict with BARE forecast keys). (71,18): max-useful = ceil(0.30 / 0.10) = 3. (60,20): max-useful
## = ceil(0.50 / 0.10) = 5.
func _cap_demo_patch_fixtures() -> Array:
	return [
		# The per-policy ROW, not the retired flat `ceiling_sustain` scalar (#426): these are RAW wire
		# patches (bare keys, no `patch_` prefix), and the row is the only ceiling representation the
		# wire carries now — a flat scalar here would leave the work rows' `+` uncapped.
		_wire_patch_rows({"x": 71, "y": 18, "per_worker_yield": 0.10}, 0.30),
		_wire_patch_rows({"x": 60, "y": 20, "per_worker_yield": 0.10}, 0.50),
	]

## The per-source-cap verify band: idle workers to spare (4), one Forage row AT its patch max-useful
## (3 at (71,18)), one Forage row BELOW its patch max-useful (1 of 5 at (60,20)), one Hunt row AT its
## herd max-useful (2 on game_deer_07), plus a Scout role. The two AT-cap `+`s must go dead with idle
## still available; the below-cap Forage `+` and the band-wide Scout `+` must stay enabled.
func _cap_demo_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 910
	band["id"] = "Band 8"
	band["idle_workers"] = 4
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "floor": 0.5, "target_x": 71, "target_y": 18, "actual_yield": 0.30, "sustainable_yield": 0.30},
		{"kind": "forage", "workers": 1, "floor": 0.5, "target_x": 60, "target_y": 20, "actual_yield": 0.10, "sustainable_yield": 0.10},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 68, "target_y": 15, "actual_yield": 0.20, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 1},
	]
	return band

## The MapView snapshot behind `band_panel_people_map_path` — the SAME `_band_fixture()` cohort the
## snapshot-path state uses, on a flat grid just big enough to hold its hex, so the marker MapView
## builds carries exactly the age structure the panel is judged on. **Fog cannot redact it, and not
## because fog is off** — a fresh MapView now defaults to fog ON. `_rebuild_unit_markers` builds the
## marker list unfiltered (the fog gate is `_unit_hidden_by_fog` at DRAW time, and it exempts your
## OWN bands), and this fixture's band is faction 0. So this state reads the marker, never a
## fog-gated `tile_info` — unlike `ui_preview`'s `tile_panel_land_sticky`, which must disable FoW
## explicitly. Verified by A/B: flipping the default moves no frame here.
func _map_path_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(MAP_PATH_GRID_W * MAP_PATH_GRID_H)
	terrain.fill(MAP_PATH_TERRAIN_ID)
	return {
		"grid": {"width": MAP_PATH_GRID_W, "height": MAP_PATH_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _stamp_band_ids([_kit_band_fixture()]),
	}

## **THE REFERENCE BAND WITH THE MINIMAL TOE'S SIX ON IT** — the six the decoder puts on EVERY cohort,
## so this rather than `_band_fixture` is the shape a live server actually produces. It is a SEPARATE
## fixture, and that is a finding rather than a preference: the `Kit` row costs 26px, the band zone
## reads **299 of its 300px box** in a height-capped T/B dock (`band_panel_vitals_worst_case` prints
## it). **The six fields now ride the SHARED fixture** — every live cohort states its kit, so a
## harness measuring a band without one was measuring a zone a whole row short of what it renders
## against a real server. The 25px that cost `Zone_band` in 13 states is paid for by the SHORT tier
## merging Growth onto the Morale line (`BandDetailLines`' `BAND_MORALE_GROWTH_CLAUSE_FORMAT`), the
## same trade the Fodder row already makes onto Food.
##
## Used by the MAP-PATH state, which renders in the TALL left dock where the row fits. Spears
## deliberately WEARING rather than round, so the row prints a real number and an `int()` narrowing is
## visible; none dry, so the DANGER tint keeps its meaning; `hunter_attack` above a Wild Boar's
## defense, so the ⚠ effective-attack gate stays quiet and its own coverage stays where it is.
func _kit_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["hunting_kit_durability"] = 74.5
	band["sled_kit_durability"] = 58.0
	band["basket_kit_durability"] = 91.0
	band["hunter_attack"] = 2.0
	band["hunt_carry_per_worker_biomass"] = 2.5
	band["forage_carry_per_worker_biomass"] = 1.75
	return band

# ---- THE SHARED FIXTURE's kit condition (`docs/plan_hunt_through_combat.md` §4.8) ----------------
## The three components' remaining condition on `_band_fixture`, on `equipment.json`'s 0-100 scale.
## **THREE DIFFERENT NUMBERS, deliberately** — a fixture giving two components one value would pass
## every assertion with their accessors swapped, which is the defect class this arc keeps reproducing.
## Spears WEARING rather than round, so the row prints a real number and an `int()` narrowing shows.
const KIT_SHARED_SPEARS_CONDITION := 74.5
const KIT_SHARED_SLED_CONDITION := 58.0
const KIT_SHARED_BASKETS_CONDITION := 91.0

# ---- THE KIT PICKER's band (`docs/plan_denial_raid.md`) ------------------------------------------
## Condition on the two hunt components, and the whole point of the pair is that they DISAGREE.
## Spears are worn but live; the SLED has run dry, so the big-game kit's carry has stepped down to the
## bare-handed tier while its attack has not. That is what makes the picker's hint line assertable as
## the EFFECTIVE tier rather than the roster's fresh one: `KitOption` publishes carry 40 for this kit
## and the band gets 12, and a hint quoting 40 to this band would be a lie of exactly the class this
## branch exists to remove.
const KIT_FRAME_SPEARS_CONDITION := 74.5
const KIT_FRAME_SLED_DRY := 0.0
## The baskets are irrelevant to a hunt sheet and are left healthy, so nothing on these frames can
## pass by reading the forage component on the hunt's row — the defect the three-kit split corrected.
const KIT_FRAME_BASKETS_CONDITION := 91.0

## The band the three Kit frames render against: the reference band plus a real, UNEVEN set of
## component conditions.
##
## **A SEPARATE FIXTURE, and the separation is load-bearing.** `_band_fixture` states no kit at all,
## and `DetailFormat.band_states_kit` is a bare `has()` on the spears key — so folding these onto it
## would light the `Kit` vitals row in 13 other states and overflow `Zone_band` by 25px, which is what
## `_kit_band_fixture`'s own note records. This one is that fixture's twin with the SLED run dry, kept
## apart from it because the map-path state asserts a live `Sled 58` row.
func _kit_worn_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["hunting_kit_durability"] = KIT_FRAME_SPEARS_CONDITION
	band["sled_kit_durability"] = KIT_FRAME_SLED_DRY
	band["basket_kit_durability"] = KIT_FRAME_BASKETS_CONDITION
	# The band's OWN resolved tiers, i.e. what it gets under the JOB DEFAULT. They are the cohort's
	# statement and the `Kit` row reads them; the picker does NOT — it resolves the SELECTED kit's
	# tiers off the roster — so they are set consistently with the conditions above rather than being
	# what the picker's assertions read.
	band["hunter_attack"] = BandFx.KIT_ATTACK_EQUIPPED
	band["hunt_carry_per_worker_biomass"] = BandFx.KIT_HUNT_CARRY_BARE
	band["forage_carry_per_worker_biomass"] = BandFx.KIT_FORAGE_CARRY_EQUIPPED
	return band

## Stamp a fixture cohort with the `band_id` the real wire carries, DELIBERATELY DIFFERENT from its
## `entity`. `band_id` is the durable handle every band-addressed command names
## (`HudConst.NO_BAND_ID`); `entity` is client-local ECS allocation state. Both are plain ints, so a
## fixture where the two agree cannot tell a correct emit from one that sent the entity — which is
## exactly how that defect shipped. The offset keeps ids readable (band 904 -> 4904) while
## guaranteeing they differ. Stamped at PUSH time, not at construction, because several fixtures
## override `entity` after the builder returns.
static func _stamp_band_ids(cohorts: Array) -> Array:
	var stamped: Array = []
	for cohort_variant in cohorts:
		var cohort: Dictionary = (cohort_variant as Dictionary).duplicate(true)
		cohort["band_id"] = int(cohort.get("entity", 0)) + FIXTURE_BAND_ID_OFFSET
		stamped.append(cohort)
	return stamped

## Push a cohort roster through the real snapshot path (`update_band_alerts`), band ids stamped.
func _push_bands(cohorts: Array) -> void:
	_hud.update_band_alerts(_stamp_band_ids(cohorts))

## A player-faction Camp-stage band (population-snapshot shape update_band_alerts consumes):
## working-age labor with idle workers + a couple of active assignments + the settlement stage
## header fields, so the relocated panel shows a full detail + allocation report.
func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"entity": 904,
		"faction": 0,
		"size": 30,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		# Good food state: long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the Food
		# line reads "… · +0.26 /turn" (green) with the category breakdown collapsed (clickable open).
		"turns_of_food": 22.0,
		# Good morale (collapsed ▸ disclosure); the signed Layer-1 contributions give the morale
		# breakdown real content when expanded.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		# Thriving growth: fed (neutral hunger, row omitted), saturated larder, net-positive food →
		# 1.0 × 1.5 × 1.25 = 188% of normal, neutral ink, collapsed ▸ disclosure.
		"fertility_hunger": 1.0,
		"fertility_reserve": 1.5,
		"fertility_trend": 1.25,
		# Trade goods are the THIRD key on the band's own `stores` since issue #381 — the sim moved them
		# off the faction stockpile, so this is what the Trade row's total reads.
		"stores": {"provisions": 84.0, "trade_goods": 12.0},
		"working_age": 16,
		"idle_workers": 3,
		# Age structure (PopulationCohortState children/working/elders) — the band zone's PEOPLE bar.
		# **`age_working` MUST equal `working_age`, and the three MUST sum to `size`.** They are one
		# band counted two ways, and the sim keeps them in step; a fixture that disagrees renders a
		# PEOPLE bar of 99 working-age adults above a WORKFORCE bar of 16 workers, which reads as a
		# bug in the very frame the two-bar design is judged on. These are the live game's own
		# numbers (`Pop 30 👶9 🛠16 🧓5`), so dep = round((9 + 5) / 16 * 100) = 88 per 100 workers.
		# FRACTIONAL, as the wire actually carries them (Scalar) — the panel apportions them to whole
		# people. Rounding each on its own gives 9 + 17 + 5 = 31 for a band of 30, which is the
		# off-by-one this fixture now guards: the frame must read 9 · 16 · 5.
		"age_children": 9.2925,
		"age_working": 16.5375,
		"age_elders": 4.6425,
		"max_expedition_party_size": 8,
		# **THE BAND'S KIT, ON THE SHARED FIXTURE BECAUSE EVERY LIVE COHORT CARRIES IT**
		# (`docs/plan_hunt_through_combat.md` §4.8). `DetailFormat.band_states_kit` is a bare `has()`
		# on the spears key, so a fixture that omits these renders no `Kit` vitals row — and the band
		# zone was then being measured a whole row short of what it renders against a real server.
		# Three DIFFERENT conditions on the 0-100 scale, so an assertion cannot pass with two
		# accessors swapped; none dry, so the row's DANGER tint keeps its meaning and the frames that
		# judge a spent kit stay the ones that state one.
		"hunting_kit_durability": KIT_SHARED_SPEARS_CONDITION,
		"sled_kit_durability": KIT_SHARED_SLED_CONDITION,
		"basket_kit_durability": KIT_SHARED_BASKETS_CONDITION,
		# The RESOLVED tiers the sim publishes beside them. Equipped throughout, matching the
		# conditions above — `hunter_attack` well clear of `QUARRY_DEFENSE`, so no compose sheet on
		# this band reads the combat gate's refusal and the frames that judge that refusal stay the
		# ones that compose a bare-handed kit.
		"hunter_attack": BandFx.KIT_ATTACK_EQUIPPED,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_EQUIPPED,
		# The raid-forecast levers the sim echoes on every cohort: the slow-raid warn line and the
		# move rate the client adds round-trip travel from. Without them the compose sheet's forecast
		# degrades to hunting turns only and can never read "slow" — i.e. it would prove less.
		"expedition_viability_warn_turns": 20,
		"band_move_tiles_per_turn": 2.0,
		"work_range": 2,
		# Deliberately SHORT: the quarry fixtures straddle it (Wild Boar 4 tiles out = a party's job,
		# Roe Deer 1 tile out = a local hunt), which is what the quarry-eligibility assertion below
		# tests. Only the herd drawer and `TargetingController.is_expedition_quarry` read it, so no other state moves.
		"hunt_reach": QUARRY_BAND_HUNT_REACH,
		# `settlement_stage_id` is the panel header's SPRITE key (the icon is only the emoji
		# fallback for a stage with no bundled art) — see `StageSprites`.
		"settlement_stage_id": "camp",
		"settlement_stage_icon": "🛖",
		"settlement_stage_label": "Camp",
		"activity": "forage",
		# Band food flow on the Food summary line: net income vs consumption + the Gathered/Hunted
		# breakdown (summed from the assignment actual_yields by kind).
		"food_income": 0.94,
		"food_consumption": 0.68,
		# The hunt overdraws (actual 0.46 > sustainable 0.20) so the ⚠ overhunting flag renders on its
		# allocation row; the forage is renewable (actual == sustainable) so it never flags. The forage
		# is also OVERSTAFFED (5 assigned, 2 needed) → the "· only 2 of 5 working" note, and carries a
		# `policy` so its row shows the ♻ policy glyph — both must survive beside the ● status glyph.
		"labor_assignments": [
			# **THE LIVE FORAGE SHAPE, AND IT IS THE REGRESSION THIS FIXTURE EXISTS FOR.** A cash crop
			# really does sell (`labor.rs`), so `trade_yield` is non-zero — but its `realized_trade_yield`
			# is the documented `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` **0.0**, and the decoder inserts
			# that key UNCONDITIONALLY. Both keys present, one of them zero, is exactly what the wire sends
			# and exactly what a `has("realized_trade_yield")` test reads as "projected: nothing".
			# The pressure axis is a FLOOR, not a stance — `policy` went with `FollowPolicy`.
			{"kind": "forage", "workers": 5, "workers_needed": 2, "floor": 0.5, "target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48, "trade_yield": 0.04, "realized_trade_yield": 0.0},
			# BOTH PRODUCTS on the worked row (issue #337): a deer pays meat AND hide, so the row
			# headline must read `+0.20 /turn · ⇄ +0.04` — food leading, trade only because it is
			# non-zero. `trade_yield` is NOT food income: the Food line's Gathered/Hunted breakdown
			# still sums `actual_yield` alone, which is what keeps the larder identity closed.
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "trade_yield": 0.04, "realized_trade_yield": 0.04},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
	}

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) + a low
## larder runway (4 days). Both trip the concerning gate, so the category breakdown auto-shows under
## a red net figure. Reuses band 904's chrome fields but a distinct entity so the cycler stays 1/1.
func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 906
	band["id"] = "Band 4"
	band["turns_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20},
		# THE TRADE-ONLY ROW (issue #337): a wolf pack pays pelts and NO meat, so every food field on
		# this assignment is honestly 0. The row must headline `⇄ +0.22` ALONE — no "+0.00 /turn",
		# which is the false reading that said the hunt was worth nothing — and it must NOT appear in
		# the Food line's Hunted total, because trade goods never enter the larder.
		{"kind": "hunt", "workers": 2, "fauna_id": TRADE_ONLY_HERD_ID, "floor": 0.15, "target_x": 72, "target_y": 19, "actual_yield": 0.0, "sustainable_yield": 0.0, "trade_yield": 0.22, "realized_trade_yield": 0.22},
		{"kind": "scout", "workers": 2},
	]
	return band

## `_band_fixture` with every TRADE component stripped off its assignments — the band that earns no
## trade at all, which is what the zero-rate Trade row is judged on. Strips rather than hand-writing a
## fixture so it cannot drift from `_band_fixture`'s chrome (and so the ONLY difference between this
## band and the earning one is the thing under test).
func _no_trade_band_fixture() -> Dictionary:
	var band := _band_fixture()
	var stripped: Array = []
	for a in (band["labor_assignments"] as Array):
		var d := (a as Dictionary).duplicate(true)
		d.erase("trade_yield")
		d.erase("realized_trade_yield")
		stripped.append(d)
	band["labor_assignments"] = stripped
	return band

## The trade-only-HUNT variant of the band above: the deer is unassigned, so every hunt this band works
## pays trade and no food. It exists to exercise the AGGREGATE suppression path — the per-kind hunt chip
## has no food component to state at all — which the mixed board cannot reach, since one food-paying
## hunt there keeps the chip's food term alive.
func _trade_only_hunt_band_fixture() -> Dictionary:
	var band := _concerning_food_band_fixture()
	band["labor_assignments"] = (band["labor_assignments"] as Array).filter(
		func(a): return String((a as Dictionary).get("fauna_id", "")) != EXTRACTIVE_ROW_HERD_ID)
	return band

## A TALLER band variant (same entity 904, so the expeditions still attach): starving + declining
## morale with the full itemized breakdown + an Output row + the send-expedition section, so the
## summary column runs well past the old fixed T/B PANEL_HEIGHT — the case that used to clip.
func _starving_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["turns_of_food"] = 1.5
	band["morale"] = 0.22
	band["morale_delta"] = -0.055
	band["morale_cause"] = 1   # Terrain
	band["morale_settling"] = 0.010
	band["morale_terrain"] = -0.030
	band["morale_climate"] = -0.020
	band["morale_unrest"] = -0.015
	band["output_multiplier"] = 0.62
	band["last_emigrated"] = 4
	# ...and its growth has collapsed with its larder: eating short off a draining store with income
	# gone → 0.55 × 1.05 × 0.25 = 14% of normal, a red Growth row above a WARN caret. It is the extra
	# row + disclosure this variant exists to push past the old fixed panel height.
	band["fertility_hunger"] = 0.55
	band["fertility_reserve"] = 1.05
	band["fertility_trend"] = 0.25
	return band

## A detached SCOUT expedition outfitted by band 904 (home_band_entity), outbound to (39,26).
func _scout_expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"entity": 951,
		"faction": 0,
		"size": 4,
		"current_x": 39,
		"current_y": 26,
		"turns_of_food": 9.0,
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "outbound",
		"home_band_entity": 904,
	}

## One expedition per PHASE, all homed on band 904 — the fixture set behind `band_panel_status_glyphs`:
## the Active-expeditions rows must render a distinct, legible glyph for each (➤ outbound / ● hunting /
## ◄ delivering / ◄ returning) and spell `awaiting` out in WARN amber (▮▮ Awaiting orders), since a
## parked party is a demand on the player, not a status.
func _phase_expedition_fixtures() -> Array:
	var scout_outbound := _scout_expedition_fixture()
	var scout_awaiting := _scout_expedition_fixture()
	scout_awaiting["entity"] = 953
	scout_awaiting["id"] = "Scouts 2"
	scout_awaiting["expedition_phase"] = "awaiting"
	var scout_returning := _scout_expedition_fixture()
	scout_returning["entity"] = 954
	scout_returning["id"] = "Scouts 3"
	scout_returning["expedition_phase"] = "returning"
	var hunt_hunting := _hunt_expedition_fixture()
	var hunt_delivering := _hunt_expedition_fixture()
	hunt_delivering["entity"] = 955
	hunt_delivering["id"] = "Hunters 2"
	hunt_delivering["expedition_phase"] = "delivering"
	return [scout_outbound, scout_awaiting, scout_returning, hunt_hunting, hunt_delivering]

## A LUMPY big-game hunt schedule: ~6-food hauls on scattered turns, zeros between them (the cadence a
## whole-animal hunt actually delivers). Length = arrivals_horizon_turns (20). Realized ≈ 2.7/turn.
func _lumpy_hunt_schedule() -> Array:
	var haul_turns := {1: true, 3: true, 4: true, 6: true, 9: true, 11: true, 14: true, 16: true, 19: true}
	var schedule: Array = []
	for i in range(20):
		schedule.append(6.0 if haul_turns.has(i) else 0.0)
	return schedule

## A CONTINUOUS forage schedule at `rate` every turn — no gap, so its row draws NO tick strip (the gap
## rule). Length 20; `rate` matches the fixture's shown realized yield so the merged chart is honest.
func _continuous_forage_schedule(rate: float = 0.9) -> Array:
	var schedule: Array = []
	for i in range(20):
		schedule.append(rate)
	return schedule

## A SPARSE hunt schedule (two hauls, deep gaps) for the emptying-larder state: the drain outpaces the
## trickle and the second haul lands too late, so the larder walk hits 0 mid-horizon.
func _sparse_hunt_schedule() -> Array:
	var haul_turns := {2: true, 9: true}
	var schedule: Array = []
	for i in range(20):
		schedule.append(5.0 if haul_turns.has(i) else 0.0)
	return schedule

## A player band whose sources carry projected arrivals: a LUMPY hunt (gaps → strip) beside a
## CONTINUOUS forage (no gap → no strip). Positive net (hauls + trickle > flat drain), so the merged
## Food-outlook chart sawtooths UPWARD.
func _arrivals_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 920
	band["id"] = "Band 9"
	# NET-POSITIVE (income 3.6 vs drain 2.0), so the runway is the not-food-limited sentinel and the
	# Food line reads ∞ — the sim reports 999 whenever net drain <= 0. A finite countdown here would
	# contradict the upward-sawtoothing chart directly beneath it.
	band["turns_of_food"] = BandFoodStatus.UNLIMITED_TURNS
	band["stores"] = {"provisions": 30.0}
	band["food_income"] = 3.6
	band["food_consumption"] = 2.0
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 2.7, "sustainable_yield": 2.7,
			"realized_yield": 2.7, "arrival_schedule": _lumpy_hunt_schedule()},
		{"kind": "forage", "workers": 3, "floor": 0.5, "target_x": 71, "target_y": 18,
			"actual_yield": 0.9, "sustainable_yield": 0.9, "realized_yield": 0.9,
			"arrival_schedule": _continuous_forage_schedule()},
		{"kind": "scout", "workers": 2},
	]
	return band

## Every quantity the WORST-CASE vitals fixture states, named because each one exists to keep ONE
## optional row alive — and because the merged Food line's width is measured against them, so a
## fixture tuned to short numbers would measure a line no player ever sees.
##
## The larder is DELIBERATELY LARGE with a LONG runway and a NEGATIVE net rate, which is a real
## combination (a big store draining slowly) and the widest the Food row can render: three digits of
## provisions, three digits of turns, a signed rate, and a three-digit hay stock beside them.
const WORST_CASE_PROVISIONS := 248.0
## `WORST_CASE_PROVISIONS` walked down at the net drain below (3.60 income − 4.60 eaten − 0.41 pen
## feed = −1.41/turn), so the runway the row prints is the one the larder actually implies.
const WORST_CASE_TURNS_OF_FOOD := 176.0
## The hay larder (Flora roster F3) and the pen bill it offsets — either one alone lights the fodder
## readout, and this fixture carries BOTH so neither gate can be the thing keeping it on.
const WORST_CASE_FODDER_STORE := 128.4
const WORST_CASE_PEN_FEED_UPKEEP := 0.41
## The band's trade stock, so the Trade row (dropped in this tier) has real content in the taller one.
const WORST_CASE_TRADE_STOCK := 46.5
## Discontent below full, so the WORK head renders its Output item.
const WORST_CASE_OUTPUT_MULTIPLIER := 0.62
## Chosen against the two worked rows' realized income (3.60) and the pen bill so the net comes out
## NEGATIVE — a signed rate is a character wider than an unsigned one, and a draining larder beside a
## long runway is the shape a big-store band really shows.
const WORST_CASE_FOOD_CONSUMPTION := 4.60

## THE WORST CASE: a band carrying EVERY optional vitals row it can simultaneously have. Built on the
## arrivals fixture, so it also carries the per-source `arrival_schedule`s the FOOD OUTLOOK chart
## needs — the block `build_band_zone` gates on height — and its two worked rows are given trade
## components so the Trade row has a rate as well as a stock.
func _vitals_worst_case_band_fixture() -> Dictionary:
	var band := _arrivals_band_fixture()
	band["entity"] = 922
	band["id"] = "Band 11"
	band["turns_of_food"] = WORST_CASE_TURNS_OF_FOOD
	band["stores"] = {"provisions": WORST_CASE_PROVISIONS, "trade_goods": WORST_CASE_TRADE_STOCK}
	band["fodder_store"] = WORST_CASE_FODDER_STORE
	band["pen_feed_upkeep"] = WORST_CASE_PEN_FEED_UPKEEP
	band["output_multiplier"] = WORST_CASE_OUTPUT_MULTIPLIER
	band["food_consumption"] = WORST_CASE_FOOD_CONSUMPTION
	# Falling morale with a named cause, so the Morale row renders its longest form beside the rest.
	band["morale"] = 0.31
	band["morale_delta"] = -0.040
	band["morale_cause"] = 1   # Terrain
	band["morale_settling"] = 0.010
	band["morale_terrain"] = -0.030
	band["morale_climate"] = -0.020
	# `_arrivals_band_fixture` restates the assignments, so the trade components have to be re-added:
	# they are what gives the (taller-tier) Trade row a live rate rather than a bare stock.
	for entry in (band["labor_assignments"] as Array):
		var assignment: Dictionary = entry
		if String(assignment.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
			assignment["trade_yield"] = 0.06
			assignment["realized_trade_yield"] = 0.06
		elif String(assignment.get("kind", "")) == SourceForecast.LABOR_KIND_FORAGE:
			# The live forage shape: a real `trade_yield` beside the not-yet-projected `0.0`.
			assignment["trade_yield"] = 0.04
			assignment["realized_trade_yield"] = 0.0
	return band

## A player band whose larder EMPTIES inside the horizon: a heavy drain over a sparse hunt + a thin
## forage trickle, so the Food-outlook walk reaches 0 and the chart draws the dashed "empty ~turn N".
func _arrivals_starving_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 921
	band["id"] = "Band 10"
	# The runway is the HONEST one — larder walked with income counted (12 food, net drain ~1.6/turn),
	# so it lands on the same turn the chart's dashed "empty ~turn N" marker does. The old
	# larder/consumption reading would have said 4 here and visibly contradicted the chart below it.
	band["turns_of_food"] = 9.0
	band["stores"] = {"provisions": 12.0}
	band["food_income"] = 0.9
	band["food_consumption"] = 2.5
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 3, "fauna_id": "game_deer_07", "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 0.5, "sustainable_yield": 0.5,
			"realized_yield": 0.5, "arrival_schedule": _sparse_hunt_schedule()},
		{"kind": "forage", "workers": 2, "floor": 0.5, "target_x": 71, "target_y": 18,
			"actual_yield": 0.4, "sustainable_yield": 0.4, "realized_yield": 0.4,
			"arrival_schedule": _continuous_forage_schedule(0.4)},
		{"kind": "scout", "workers": 1},
	]
	return band

## A detached HUNT expedition outfitted by band 904, following game_deer_79 under a Surplus policy.
func _hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"entity": 952,
		"faction": 0,
		"size": 6,
		"current_x": 66,
		"current_y": 12,
		"turns_of_food": 5.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_79",
		"expedition_floor": 0.3,
		"home_band_entity": 904,
		# In-flight next delivery → the parties inspector's "Next delivery: ~14 food in 6 turns" line.
		"expedition_eta_turns": 6,
		"expedition_projected_delivery": 14.0,
		"expedition_recurring": false,
	}

## A hunt party whose forecast projects ZERO delivery — the herd is at/below its policy floor, so the
## raid returns empty. The field is PRESENT and 0 (a real no-surplus answer), which the parties
## inspector must render as "Next delivery: none — the herd has no surplus to raid", never hide.
func _lean_hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 2",
		"entity": 953,
		"faction": 0,
		"size": 4,
		"current_x": 64,
		"current_y": 11,
		"turns_of_food": 4.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_floor": 0.5,
		"home_band_entity": 904,
		"expedition_eta_turns": 0,
		"expedition_projected_delivery": 0.0,
		"expedition_recurring": false,
	}

## A hunt party whose target herd is GONE from `_world_herds` (lost/replaced) — a projected-0 forecast
## that is NOT "no surplus": `find_world_herd` returns {} for the target id, so the delivery line must
## read "target herd lost — the party is returning home", distinct from the at-floor no-surplus case.
func _lost_hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 3",
		"entity": HUNT_LOST_ENTITY,
		"faction": 0,
		"size": 5,
		"current_x": 62,
		"current_y": 9,
		"turns_of_food": 6.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "returning",
		# NOT in `_herd_fixtures()` — the target the party launched at is no longer in the telemetry.
		"expedition_target_herd": "game_deer_gone",
		"expedition_floor": 0.5,
		"home_band_entity": 904,
		"expedition_eta_turns": 0,
		"expedition_projected_delivery": 0.0,
		"expedition_recurring": false,
	}
