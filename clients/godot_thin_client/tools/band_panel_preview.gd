extends Node

## Dev-only preview harness for the dockable Band / City panel (slice 2 scaffold).
##
## Instances the real BandCityPanel alongside a real HudLayer, wires the panel's
## reservation onto the HUD (mirroring Main's `_apply_reservation` fan-out for the
## `hud` surface), then docks the panel to each edge (+ collapsed) and dumps one
## PNG per state so the chrome + the HUD reflow can be eyeballed without a server.
## The full MAP reflow/clip is only exercised in the running client. FROM THE REPO ROOT:
##
##   scripts/preview.sh res://tools/band_panel_preview.tscn
##
## then read ui_preview_out/band_panel_*.png.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")

## The hang guard, a SIBLING node in `band_panel_preview.tscn` (`tools/preview_watchdog.gd`).
##
## **This harness does NOT have `ui_preview`'s chapter-loading defect** — it loads no chapters, so
## nothing here can leave a half-written frame set behind a broken sub-script. What it DOES share is
## the shape underneath it: the whole run is one long `await`ing `_ready()` whose last line is
## `_finish()`, so any runtime error aborts it without ever exiting, and any of the three
## scenes/scripts it `preload`s failing to compile takes THIS script's parse down with it — leaving
## the root node scriptless and the process idling forever with no FAIL and no status. The guard is
## the same node, for that shape only. A run that DOES reach `_finish()` derives its own status from
## `_failures` — see `_fail` and `EXIT_OK` / `EXIT_FAILED`.
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
## **`Main`, for its RULE and for nothing else.** The harness never instances it — it fans the panel's
## reservation out by hand — but "does the HUD yield this strip?" has exactly one home
## (`Main.band_dock_overlays_hud`, `static` so it can be asked without a node), and a harness that
## restated the rule instead would keep passing after the rule moved. That is not hypothetical: this
## file carried `edge != SIDE_TOP` until the BOTTOM edge stopped being unconditional.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## **THE KIT ROSTER IS SHARED WITH `ui_preview`, and deliberately so.** It is world config the sim
## publishes once (`SubsistenceSection.kits`), not a per-harness prop: two copies could quote
## different tiers or a different job default, and the `kit <id>` command token asserted here is the
## same token that harness's frames are read against. This is the ONE cross-harness fixture preload.
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
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
## Offset applied to a fixture cohort's `entity` to derive its `band_id` — see `_push_bands`. Read off
## `BandFx` rather than restated: `ui_preview`'s band fixtures stamp themselves with the same rule, and
## two harnesses deriving one handle two ways is how they come to address different bands.
const FIXTURE_BAND_ID_OFFSET := BandFx.FIXTURE_BAND_ID_OFFSET
## One Wild Boar's worth of yield in provisions (`HerdTelemetryState.foodPerAnimal`) — the quarry
## fixture's delivered food is animals × this, so the sheet's forecast quotes a real food total.
const QUARRY_FOOD_PER_ANIMAL := 4.0
## One animal's worth of TRADE GOODS (issue #337) — a hunt pays a vector, so a raid cell carries this
## payload beside its food one. Small against the food quantum: an edible quarry is meat first.
const QUARRY_TRADE_PER_ANIMAL := 0.5
## The INEDIBLE quarry on the work board (issue #337): its hunt row pays trade goods and no food.
const TRADE_ONLY_HERD_ID := "game_wolf_03"

## ---- THE FODDER FACE (issue #449) -------------------------------------------------------------
## The sown hay FIELD on the work board: a forage source paying feed and NEITHER provisions nor trade,
## which is the shipped case the whole change exists for (`flora_config.json`'s hay grass). Its tile is
## the same one the trade states push a food module for, so the row resolves its icon like any other.
const FODDER_FIELD_X := 71
const FODDER_FIELD_Y := 18
## The feed it pays per turn. It has to be big enough to read at two decimals and unequal to every other
## rate on this band, so an assertion matching its face cannot be satisfied by a neighbouring row.
const FODDER_FIELD_RATE := 0.40
## That rate as the one-slot ROW and the header TOTAL both spell it — the word, never a glyph, because
## fodder has none. Written out rather than composed through `SourceForecast`, since a needle built by
## the code under test agrees with whatever that code emits.
const FODDER_ROW_RATE_FACE := "+0.40 fodder"
## The same account as the INSPECTOR sentence spells it. `yield_components` renders a fodder magnitude
## unsigned (the #426 picker rule), so this is deliberately NOT the row's face with the sign stripped.
const FODDER_INSPECTOR_CLAUSE := "0.40 fodder"
## The CONTROL row's steady food rate — an ordinary deer hunt on the same band, so "a hunt is unchanged"
## is asserted against a row that is genuinely there rather than against an empty board.
const FODDER_CONTROL_HUNT_RATE := 0.20
const FODDER_CONTROL_HUNT_FACE := "+0.20"

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
## **THE SHIPPED PARTY LADDER** — `expedition_config.estimate_party_sizes`, the SAMPLED party axis of
## both estimate tables. It is restated here rather than read off a fixture because these assertions
## are about the SHAPE the sim ships: dense at the low end where one hunter is a large proportional
## change, sparse at the top where it is not. Every other estimate fixture in this file samples
# The `LADDER_*` / `DENIAL_LADDER_*` constants went with the assertions they fed. They described the
# SAMPLED party axis `expedition_config.estimate_party_sizes` published, which the forecast query
# retired: a raid is costed for the party that was composed, so there is no rung to round to.

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
## …and its Kit row's, the gear popover this harness opens. Same shape, `HudDisclosureVocab`'s
## `BREAKDOWN_KIND_KIT` over the same entity.
const BAND_FIXTURE_DISCLOSURE_KIT := "kit:904"

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

## The pen feed the faction roster's herd-keeping band pays — the conditional Food row that makes the
## faction band zone's worst case its worst case. The figure is the shipped pen upkeep the per-band
## fixtures already quote, so the row measures what a live one does.
const FACTION_PEN_FEED_UPKEEP := 1.74

## The faction roster's SECOND band — deliberately smaller and unhappier than the first, so a
## population-weighted mean and a plain one give different answers. See `_faction_roster`.
const FACTION_SECOND_BAND_SIZE := 12

const FACTION_SECOND_BAND_MORALE := 0.30

## …and its age brackets scaled to match that size, since they are the same band counted twice.
const FACTION_SECOND_BAND_SCALE := 0.4

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
# A party still standing in its home band's camp with no map report owed — the one shape a recall
# CANCELS on the spot rather than walking home (`HudBandLaborState.party_cancels_in_camp`).
const HUNT_IN_CAMP_ENTITY := 955
# **THE TALLEST PARTY THE INSPECTOR STRIP CAN BE ASKED TO HOLD** — every optional line of
# `BandDetailLines.expedition_summary_lines` live at once. See `_worst_case_party_fixture`.
const HUNT_WORST_CASE_ENTITY := 956
# **THE ONLY PHASE "start a life here" IS OFFERED IN** (issue #510) — a scout that has arrived and is
# waiting to be told what to do next. Every other party in this file is under orders, which is what
# makes the settle pair's negative half real rather than staged.
const SCOUT_AWAITING_ENTITY := 957
# Its pack number: the carried figure EQUALS the cap so the `Carried:` row takes its longest form —
# `N / cap` plus the `· FULL` badge — rather than the bare count a capless party gets.
# The floor it was launched with, deliberately NOT the default — the Orders row is asserted against
# it, so a fixture at the default would match a row the producer had stopped composing from the party.
const WORST_CASE_FLOOR := 0.3
# Its quarry, one of `_herd_fixtures()`. Named so the fixture and the assertion's needles resolve the
# SAME herd — the assertion reads the herd's live position and species back off `_world_herds` rather
# than restating them.
const WORST_CASE_TARGET_HERD_ID := "game_deer_79"
const WORST_CASE_CARRY_CAP := 18
# How many detail lines that party's strip must render. Stated here rather than counted from the render
# so the state FAILS on a producer that quietly stops emitting one — a shorter strip fits its box, so
# the extent report would go green on a fixture that had stopped being the worst case.
const WORST_CASE_DETAIL_LINES := 7
# A 21:9 monitor — comfortably past the wide shell's content cap, which is the whole point of the state.
const ULTRAWIDE_WIDTH := 3440
const ULTRAWIDE_HEIGHT := 900
# The two shell-threshold probe windows. The panel is bottom-docked in both, so the window width IS
# `_panel_extent().x`, the value `_shell_is_wide` tests — one pixel below the derived threshold (must
# pick the NARROW tabbed shell) and exactly at it (the narrowest legitimate WIDE shell). Derived from
# the panel's OWN LIVE answer (`wide_shell_min_width()`, a sum over the declared zones) so they can
# never drift from the threshold they bracket.
const SHELL_THRESHOLD_UNDERSHOOT := 1
const SHELL_THRESHOLD_HEIGHT := 900
## THE FACTION PAGE's whole tab strip, in declared order. Spelled out here rather than read back off
## `FACTION_ZONE_LAYOUT` — quoting the layout under test would only assert that it equals itself, and
## the claim is that the page reads `Faction · Work · Know · Parties` to a player.
const FACTION_TAB_LABELS: Array[String] = ["Faction", "Work", "Know", "Parties"]
## The top bar's sedentarization seed, named because the faction page's SETTLING row renders the SAME
## figure — the page reads `FactionReadouts`' retained entry, which is the ONE place the wire array is
## filtered to the player faction, and the assertion has to know which number it is asserting.
## (It was seeded for the top bar's own Sedentarization meter; that meter is retired with the
## top-right block, and the seed stays because the faction page now renders off the same cache.)
const TOPBAR_SEDENTARIZATION_SCORE := 62.0
const TOPBAR_SEDENTARIZATION_STAGE := "soft"
## Slack allowed between a stat-row key's laid-out width and the width its own font measures for its
## own string (`_faction_keyless_rows`). It absorbs the sub-pixel disagreement between the container's
## rounded layout and the text server's float measurement, and nothing wider: a CLIPPED key comes back
## one pixel wide against a key that needs tens, so the two cases are nowhere near this margin.
const KEYLESS_KEY_WIDTH_TOLERANCE := 1.0
## `HudFormat.meter_bar`'s filled cell. Spelled here rather than read off the formatter — the claim is
## that a meter at 62% draws SOMETHING, and asking the formatter what it draws would agree with itself.
const METER_FILLED_CELL := "▰"
## The FOUR-zone shell threshold, restated from the panel's named widths rather than asked of the
## method that computes it: `ZONE_BAND_WIDTH + work + ZONE_KNOWLEDGE_WIDTH + ZONE_PARTY_WIDTH`, three
## gutters and the card chrome. It is the number the whole generalization turns on — one gutter per
## GAP rather than the two a three-zone body has — so it is stated once, here, and compared.
const FACTION_SHELL_MIN_WIDTH := BandCityPanel.ZONE_BAND_WIDTH + BandCityPanel.ZONE_WORK_MIN_WIDTH \
	+ BandCityPanel.ZONE_KNOWLEDGE_WIDTH + BandCityPanel.ZONE_PARTY_WIDTH \
	+ 3.0 * BandCityPanel.RAIL_SEPARATOR_SPAN + BandCityPanel.PANEL_CHROME_H
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
## What `DisplayServer.get_name()` answers under `--headless` (measured, Godot 4.7 — it reads `macOS`
## in a real window). That driver opens no window and offers only the `dummy` rendering driver, so
## every window geometry this harness pins is a stub: see `_is_headless`.
const HEADLESS_DISPLAY_DRIVER := "headless"
## Phase to seed the turn orb's calm breath at, as a fraction of `TurnOrb.PULSE_PERIOD`. The breath is
## `0.5 - 0.5 * cos(t)`, which is ZERO — its faintest, smallest instant — at phase 0, so freezing the
## clock there would render the pulse at the bottom of its range. A quarter period puts `cos` at 0,
## i.e. the breath's MIDPOINT, which is what an unfrozen frame averaged.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.25

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — stdout used to be the only signal this harness gave, and a red
## run was indistinguishable from a green one to anything that only checked the status.
const EXIT_OK := 0
const EXIT_FAILED := 1

## The size every state re-asserts before it renders — see `_pin_window`.
var _pinned_size := PREVIEW_SIZE
## The canvas size every state re-asserts, `ZERO` = leave the project's stretch alone — see `_pin_canvas`.
var _pinned_canvas := Vector2i.ZERO
var _hud: HudLayer
var _panel: BandCityPanel
## `Main._apply_reservation`'s fan-out, restated (see `_ready`). Held so a probe that needs the panel
## UNBOUND can take it off `reservation_changed` for the length of the measurement.
var _reservation_listener: Callable
## The hang guard from the scene, or `null` if it has gone — a safety net, never a dependency.
var _watchdog: Node = null
## The last state `_save`d, so an assertion failure names the frame it fired on.
var _current_state := "<pre-render>"
## How many assertions have failed, tallied by `_fail` and turned into the exit status by `_finish`.
var _failures := 0
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
## **RESTAGING THE HERD ROSTER DROPS EVERY FORECAST ANSWER, and that is a harness fact rather than a
## client one.** A forecast is asked under a key built from the band, the herd id, the kit and the
## party — deliberately NOT from the herd's contents — so a question already answered is never re-asked.
## This file repeatedly restages ONE quarry id with a DIFFERENT table (the viable raid, then the
## repelled one, then the deep-party one), which changes the sim's answer without changing the
## question: the canned answerer is never consulted again and the frame renders the state before the
## swap. A live sim cannot reach that — a herd's own numbers move, its identity does not.
func _set_world_herds(herds: Array) -> void:
	for h in herds:
		if h is Dictionary:
			_floorify(h)
	_hud.update_herds(herds)
	_hud.forecast_query().reset()

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
	# PIN THE WINDOW. macOS applies — and re-applies — a window mode/size change
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

	# PIN THE INTERFACE SCALE, out of that same real `user://client_settings.cfg` and by the same rule
	# `map_preview` states for the speed sliders: `ClientSettings` is an autoload that has already read
	# the developer's file, and `UiScaler` has already pushed whatever it found onto the window's
	# `content_scale_factor` — which shrinks the LOGICAL viewport, so every canvas pin, every dock
	# threshold probe and every frame here would be measured at a width this panel never ships at.
	# Assign the MEMBER, never `set_ui_scale` (the setter `_save`s over that file); re-emit `changed`
	# so `UiScaler` applies the pin through its own real path.
	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)

	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	# Fan the panel's reservation onto the HUD as Main does — INCLUDING the edges where the HUD does NOT
	# yield its strip and the lateral bounds that go with them (issue #377), or these frames would show
	# the HUD yielding a strip the live client keeps, and a card free to sit where the live one is bounded.
	# **THE RULE IS CALLED, NEVER RESTATED.** It used to be spelled out here as `edge != SIDE_TOP`, and a
	# restatement is exactly how a harness ends up green while testing the predicate `Main` used to have —
	# so `Main.band_dock_overlays_hud` is asked directly (it is `static` and node-free for this). `Main`
	# itself is still not instanced; only its rule is borrowed.
	# HELD IN A MEMBER, not connected anonymously: this listener PUSHES THE LATERAL BOUNDS BACK, so any
	# probe that wants to see the panel WITHOUT them has to take it off the wire for the duration —
	# see `_assert_card_clears_hud_columns`. (Before the panel republished its reservation on a
	# bounds-driven shell flip, clearing the bounds emitted nothing and the probe's mutation simply
	# stuck; it does not any more, which is `Main`'s live behaviour and not a harness artifact.)
	_reservation_listener = func(edge: int, size: float):
		var hud_overlaid: bool = MAIN_SCRIPT.band_dock_overlays_hud(edge, size, _hud, _panel)
		if _hud.has_method("set_reserved_inset"):
			_hud.set_reserved_inset(&"band_panel", edge, 0.0 if hud_overlaid else size)
		# The RIGHT column's own clearance, `Main._update_right_column_bottom_clearance`'s half: where
		# the HUD keeps a BOTTOM strip, the parked chrome owns that strip's trailing corner and the
		# right dock's cards must stop above it. Fanned out here for the same reason the inset is —
		# `Main` is not instanced, so its rule is borrowed rather than restated.
		var keeps_bottom_strip: bool = hud_overlaid and edge == SIDE_BOTTOM
		if _hud.has_method("set_right_column_bottom_clearance"):
			_hud.set_right_column_bottom_clearance(size if keeps_bottom_strip else 0.0)
		var columns: Vector2 = _hud.lateral_column_widths() if hud_overlaid else Vector2.ZERO
		_panel.set_lateral_bounds(columns.x, columns.y)
	_panel.reservation_changed.connect(_reservation_listener)

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
	_hud.update_sedentarization([{"faction": 0,
		"score": TOPBAR_SEDENTARIZATION_SCORE, "stage": TOPBAR_SEDENTARIZATION_STAGE}])

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
		BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	# **THE CANNED FORECAST ANSWERER — prologue, because a raid sheet without it renders no numbers
	# at all.** The pre-launch forecasts are a request/response on the command socket now, and there is
	# no server here; `ForecastQuery` would sit pending forever and every raid readout would be its
	# placeholder. `fixtures_forecast` answers out of the herd fixtures' own raid tables, deferred, so
	# the frames judged below are the ones with numbers on them. Prologue rather than chapter state,
	# exactly as the kit roster above is: an answerer installed per arc would give one chapter's sheets
	# a forecast and the next chapter's none.
	ForecastFx.install(_hud)
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


	# (b3) THE GEAR BREAKDOWN — the Kit row's popover, opened on the reference band, which carries one
	# condition row per item the shipped config has. It is the ONLY surface that states what each item
	# DOES for the band, and until the expanded roster's three tiers reached the wire it could say
	# nothing at all about handling gear, wayfinding gear or clubs: a player was handed a scout kit
	# and a warrior kit whose effects were invisible, and whose running dry was invisible with them.
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"band")
	await _settle()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_KIT)
	await _settle()
	await _save("band_panel_kit_expanded")
	_assert_zones_within_bounds()
	_assert_gear_breakdown_states_every_kit()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_KIT)   # toggle shut before the next state

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
	# zone in the SHORT tier, where the chart is drawn COMPACT.
	#
	# **THE CHART IS PRESENT AT EVERY TIER, AND THAT IS WHAT THESE THREE FRAMES NOW PIN.** The SHORT
	# tier used to build no chart at all, so a band with a food history simply had none in a T/B dock —
	# and `_assert_zone_content_fits` was the thing that "proved" the drop kept the zone in its box,
	# i.e. it was reading a deletion as a fit. The zone SCROLLS now, so that assertion has nothing left
	# to say about this zone and `_assert_band_flank_charts` carries the claim instead: a chart-bearing
	# band renders its chart in all three docks, the short ones included.
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
		_assert_band_flank_charts(state["name"], true)
		_report_zone_content_extent(state["name"])

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

	# THE BAND-WIDE ROLE CARDS' KIT PICKER + GEAR LINE. Until this, a player was handed a wayfinding
	# kit and a warrior kit silently: the WORKFORCE cards named neither, and the picker was mounted
	# only on the four hunt/forage compose sheets, so naming a kit on those two roles was a
	# command-line act. The frame is the look; the two assertions under it are the claims a picture
	# cannot make — WHICH item each row derives, and whether the command carries the pick.
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"band")
	await _settle()
	await _save("band_panel_role_kits")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_role_card_gear()
	_assert_role_cards_are_level()
	_assert_role_kit_command_carries_the_pick()
	# Put the shared band back exactly as the later states expect it: the pick above is real zone
	# state and its emit is a real pending assign, and either one left behind would render a `No kit`
	# Scout card with an amber title in every band frame from here down.
	_hud._bandpanel._role_kit_ids.clear()
	_hud._band_labor._pending_labor.clear()
	_push_bands([_band_fixture()])
	await _settle()

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
	_assert_work_sort_tiers()
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
	# The paired NEGATIVE for the fodder state below: this board pays food and trade and NO feed, so
	# its head must render no fodder sibling at all. Asserted here rather than beside the positive
	# because a head that rendered the total unconditionally passes every claim made on a band that
	# actually grows hay.
	_assert_no_work_fodder_total()

	# THE FODDER FACE (issue #449). Same board shape one account further out: a sown hay Field pays
	# feed and NEITHER provisions nor trade, so before this its row headlined `+0.00 /turn` and read as
	# a dead tile while it fed the band's pens every turn. The head therefore carries all THREE
	# siblings at once — food and trade off the deer, fodder off the Field — and fodder is a SIBLING
	# for the trade total's own reason: it credits the band's FODDER store and never the larder.
	_push_bands([_fodder_field_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_fodder")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_work_fodder_readouts()

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
	_hud.update_intensification([_standing_knowledge_row()])
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
	_assert_denial_turn_clause_shapes()
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = "hunt"
	_hud._compose.set_party_quarry(QUARRY_FAR_HERD_ID)
	# Picking a quarry fills the party to its max-useful cap (the one-shot `TargetingController._try_pick_quarry` sets);
	# seed it here too so the frame shows the shipped default (the party at the cap, not a stray 1).
	# **THE COUNT IS PUT BACK ON ITS FLOOR FIRST, and that is what makes the ordering claim below
	# testable rather than lucky.** Autofill only moves the party if the party is not already at the
	# cap, so a state-order change that left a big count behind would silently turn
	# `_assert_chart_reads_the_settled_party` into a tautology. It costs the frame nothing: the very
	# next line arms the fill, so what renders is the cap either way.
	_hud._bandpanel._send_expedition_count = COMPOSE_HUNT_SEED_PARTY
	_hud._compose.arm_party_autofill()
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt")
	_report_compose_widths("band_panel_compose_hunt")
	_assert_hunt_sheet_chart(true, "band_panel_compose_hunt")
	_assert_chart_reads_the_settled_party("band_panel_compose_hunt", COMPOSE_HUNT_SEED_PARTY)
	# The tall side dock is where the sheet must NOT leave the zone — the other half of the fork the
	# height-capped state below asserts. See `_assert_compose_in_zone`.
	_assert_compose_in_zone("band_panel_compose_hunt")
	_assert_dock_chart_carries_the_kit()

	# **THE SAME SHEET IN THE HEIGHT-CAPPED TOP DOCK** — the tier gate on the chart, and the only
	# state that renders it. The parties zone CLIPS there, and the chart is ~150px of a ~300px box, so
	# the SHORT tier keeps the presets alone exactly as the band zone's outlook chart is kept out. The
	# frame is judged on the ABSENCE plus the fit: a gate that never fired and a chart clipped off the
	# bottom of the zone are the same picture.
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_compose_hunt_short")
	# **THE FIT IS ASSERTED HERE NOW, AND IT IS ASSERTED IN THREE PLACES AT ONCE.** An open parties
	# compose sheet does not fit a height-capped horizontal dock at all — measured at 641px of a 265px
	# box WITHOUT the chart (quarry row, presets, floor hint, party stepper, kit row, forecast and
	# send, none of which this tier drops), which is why this state used to REPORT its extent instead
	# of asserting it. The sheet renders in `BandComposeFloat` there now, so the claim can be made —
	# but only as a set: `_assert_zone_content_fits` alone passes TRIVIALLY once the sheet leaves the
	# zone, and a float is only a fix if the overflow landed somewhere that is itself measured.
	_report_zone_content_extent("band_panel_compose_hunt_short")
	_report_compose_widths("band_panel_compose_hunt_short")
	_assert_hunt_sheet_chart(false, "band_panel_compose_hunt_short")
	_assert_zone_content_fits()
	_assert_compose_float("band_panel_compose_hunt_short")
	await _assert_float_leaves_the_map_clickable("band_panel_compose_hunt_short")
	# **AN UNKNOWN ZONE BOX MUST NOT FLOAT.** Taken HERE, with the mark latched at the short dock's
	# genuine 641px, because that is the only configuration in which the two possible answers differ.
	_assert_unknown_zone_box_does_not_float("band_panel_compose_hunt_short")
	# **AND A MARK LATCHED IN THE SHORT DOCK MUST NOT SURVIVE THE MOVE TO THE TALL ONE.** Staged here,
	# judged after the real `set_dock` → render below.
	var staged_mark := _stage_impossible_compose_mark()
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_assert_mark_dropped_on_dock_change("band_panel_compose_hunt", staged_mark)
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	# **THE DOCK IS THE SECOND LAUNCH SITE, AND IT MUST OFFER THE SAME ORDERS** (§5.2). A lever on the
	# herd drawer's sheet and absent here is the same defect as a lever that does nothing. The FLOOR is
	# now the whole of what a raid is ordered with (the fill target is retired, issue #491), and what
	# this sheet must still state is the trip's BOUND: it rides its own quiet line here (this zone's
	# forecast is the one-LINE form, already dense with five facts) where the drawer folds the identical
	# clause into its readout verdict — one table, so the two surfaces cannot describe one stop
	# differently.
	_assert_band_panel("the dock's hunt sheet names which stop ends the trip",
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

	# **THE EMPTY FORM OPENED THE WAY A PLAYER OPENS IT, IN THE TALL DOCK — the state that was missing
	# when this defect was reported the second time.** Every compose fixture above stages its sheet by
	# writing `_party_compose_open` and picking a quarry first, so the harness never once rendered the
	# SMALLEST the sheet ever is: the form the player sees the instant they press `🏹 Hunt`, on a band
	# with no parties out. That is the exact picture that came back from play, floating out of a dock
	# with hundreds of px to spare. The whole composing act is restarted here — closed, then reopened
	# through the REAL footer button — because the phantom this exists to catch is taken on the render
	# that the press arms, and a sheet already open has already been measured.
	_hud._bandpanel._close_party_compose()
	_push_bands([_band_fixture()])
	_panel.set_active_tab(&"parties")
	await _settle()
	await _assert_empty_compose_opens_in_the_zone("band_panel_compose_hunt_empty")
	# Asked at the STATE rather than inside the block above, so it is still asked when that block
	# refuses its own precondition — a trigger stuck ON floats the sheet, which takes the phantom
	# reading out of the parties column and would otherwise let this claim go unasked.
	await _settle()
	_assert_zone_holds_its_compose_sheet("band_panel_compose_hunt_empty")
	await _save("band_panel_compose_hunt_empty")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	# Restore the roster the states below read: `update_band_alerts` keeps a losing-population diff
	# against the last roster pushed, and the parties rows are what the scout/deny frames render above
	# their sheets.
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_hud._bandpanel.rerender()
	await _settle()

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
	_assert_kit_reprices_the_source()

	# **OPEN.** The roster grows toward a dozen kits and a pill row cannot hold that in a 354px column,
	# so the control is an `OptionButton` — a native selector, which also MARKS the current entry
	# itself. The popup is an embedded subwindow, so it lands in the capture; the structural claims
	# (which entries, which one is marked, which one is tagged the default, and `none` LAST) ride the
	# assertion, since a screenshot cannot say which item carries the radio dot.
	var kit_picker := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as OptionButton
	if kit_picker != null:
		# Placed by hand under the button. `show_popup()` would do it, but it also grabs input and can
		# move focus mid-run; the popup is an EMBEDDED subwindow, so positioning it and calling
		# `popup()` renders it into the same viewport the capture reads.
		var below := kit_picker.get_screen_position() + Vector2(0.0, kit_picker.size.y)
		kit_picker.get_popup().position = Vector2i(below)
		kit_picker.get_popup().popup()
	await _settle()
	await _save("band_panel_compose_deny_kit_open")
	_assert_kit_picker_open(kit_picker)
	if kit_picker != null:
		kit_picker.get_popup().hide()

	# **THE UNANSWERED STATE** — the sheet on its first open, before the reply lands. It is judged
	# largely on what it must NOT say: no collapse verdict, no estimate caveat, no take line, no counted
	# refusal, every one of them a figure the sheet has no answer for. What it MUST say is the combat
	# gate — composed from wire terms, honest at any tier and with no reply at all — and the line saying
	# the raid is still being costed. The kit is switched to `none` first, through the popup's REAL
	# `id_pressed`, so the gate has a bare-handed tier to refuse on and the pick path is exercised.
	#
	# **THE ANSWERER IS UNINSTALLED FOR THIS ONE FRAME, and that is the state rather than a dodge.**
	# `ForecastFx` answers every question in the prologue, so a sheet is never left waiting here — which
	# is right for the other 84 frames and makes the one state this assertion is about unreachable. A
	# `ForecastQuery` with no sender asks nothing and reports PENDING, which is exactly the client's
	# position between opening a sheet and the socket replying.
	# The sender goes BEFORE the pick, not after it: `_pick_kit` re-renders through the real handler, so
	# a still-installed answerer would take that render's question and land the reply during `_settle`.
	_hud.forecast_query().set_sender(Callable())
	_pick_kit(KitRoster.NO_KIT_ID if kit_picker == null else BandFx.KIT_ID_NONE)
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_deny_pending")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_forecastless_sheet_suppresses_estimates()
	# Restore: the frames below are read against the DEFAULT kit, the reference band and a sim that
	# answers — a selection left on `none` or a seam left mute would suppress every verdict they assert.
	ForecastFx.install(_hud)
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

	# The between-rungs denial block that stood here went with the sampled party axis: it staged a
	# LADDER table and a party of 6 to prove the sheet named the rung its figures came from. A raid is
	# costed for the party on the stepper now, so the state it drove into is unreachable.
	_push_bands([_scout_expedition_fixture(), _deep_party_band_fixture(), _hunt_expedition_fixture()])
	_set_world_herds(_quarry_herd_fixtures())
	_hud._bandpanel.rerender()
	await _settle()
	# **PUT THE REFERENCE BAND BACK, and that is not tidiness.** `update_band_alerts` keeps a
	# losing-population diff against the LAST roster pushed, so leaving the deep-party band standing
	# changes what the next state's alert set says — and the next state is `band_panel_no_idle`, whose
	# turn orb draws its calm breath only while there are no attention entries. A PNG-less block must
	# leave the walk exactly where it found it.
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_set_world_herds(_quarry_herd_fixtures())
	await _settle()

	_hud._bandpanel._send_expedition_count = 1
	_hud._bandpanel._party_compose_open = false
	_hud._bandpanel._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

	# Zero idle workers: BOTH mission buttons (Scout / Hunt) stay VISIBLE and DISABLED, with the
	# shared reason line beneath them.
	_push_bands([_no_idle_band_fixture()])
	await _settle()
	await _save("band_panel_no_idle")

	_assert_scroll_only_where_sanctioned()
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
	_report_zone_content_extent("band_panel_parties_inspector_wide")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))   # close before the next state

	# (a2) THE WORST-CASE PARTY — the same height-capped bottom dock, the same open strip, and a party
	# carrying EVERY optional line of `BandDetailLines.expedition_summary_lines` at once (see
	# `_worst_case_party_fixture` for the seven and their gates). The state above is NOT the worst case:
	# its party carries no fill target, no carry cap and no trip bound, and it still overran its box —
	# which is the `band_panel_vitals_worst_case` lesson exactly. Every fixture carried SOME of the
	# optional lines and none carried them all, so the assertions were green on a strip nobody had ever
	# asked to hold the whole set.
	#
	# ONE party, not two: a second row costs the zone another 48px for a structural reason that has
	# nothing to do with the strip's own height, and mixing the two would leave the reported number
	# unattributable.
	#
	# It REPORTS its extent as well as asserting the fit — a near-miss and a comfortable fit are the
	# same green line otherwise, and this zone has now been at the edge twice.
	_push_bands([_many_sources_band_fixture(), _worst_case_party_fixture()])
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_WORST_CASE_ENTITY))
	await _settle()
	await _save("band_panel_worst_case_party")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_worst_case_party_lines()
	# **THE OVERFLOWING HALF of the scroll pair.** This is the strip that used to pin
	# `PANEL_HEIGHT_WIDE` at 294px of a 300px box; it now scrolls instead, which is what let the
	# two-column budget come down. Judged with `band_panel_band_columns_two`'s empty list below —
	# either claim alone passes on a list that always scrolls, or on one that never does.
	_assert_scroll_only_where_sanctioned()
	_assert_parties_list_scrolls_iff_it_overflows("band_panel_worst_case_party")
	_report_zone_content_extent("band_panel_worst_case_party")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_WORST_CASE_ENTITY))
	# PUT THE PREVIOUS ROSTER BACK. `update_band_alerts` keeps a losing-population diff against the LAST
	# roster pushed, so a state inserted here must leave the walk exactly where it found it or every
	# following state diffs against a roster it never saw.
	_push_bands([_many_sources_band_fixture(), _hunt_expedition_fixture()])
	await _settle()

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
	# The POPULATED non-overflowing case: two party rows and an open strip in a tall side dock with
	# room to spare, so the bar must stay hidden. `band_panel_band_columns_two`'s half is an EMPTY
	# list, which cannot tell "no bar because it fits" from "no bar because there is nothing in it".
	_assert_scroll_only_where_sanctioned()
	_assert_parties_list_scrolls_iff_it_overflows("band_panel_parties_inspector_narrow")
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

	# (e) "START A LIFE HERE" — the THIRD arrival action (issue #510), rendered as a PAIR because a
	# one-sided frame passes against a control that renders unconditionally. ONE roster carrying an
	# ARRIVED scout and a party that is still hunting, so both halves are in every frame and the two
	# states differ only in which party's inspector strip is open.
	_set_world_herds(_herd_fixtures())
	_push_bands([_band_fixture(), _awaiting_scout_expedition_fixture(), _hunt_expedition_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._toggle_parties_inspector(str(SCOUT_AWAITING_ENTITY))
	await _settle()
	await _save("band_panel_settle_offered")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_settle_affordance("band_panel_settle_offered", true)
	_hud._bandpanel._toggle_parties_inspector(str(SCOUT_AWAITING_ENTITY))

	# The SAME roster with the HUNTING party's strip open: its links are Jump + Recall and nothing
	# else, while the arrived scout's ROW keeps its `Settle` control one line above — which is what
	# makes this the other half of the pair rather than a frame with the feature switched off.
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))
	await _settle()
	await _save("band_panel_settle_withheld")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_settle_affordance("band_panel_settle_withheld", false)
	_assert_settle_confirms_before_emitting()
	# The founding prompt itself, left standing by the assertion above. An embedded subwindow lands in
	# the capture (the kit picker's popup precedent), so this is where the SHARED confirm chrome —
	# `HudStyle.apply_dialog`, worn by all four of this panel's prompts — is judged by eye.
	await _settle()
	await _save("band_panel_settle_confirm")
	_dismiss_dialogs()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))
	# PUT THE PREVIOUS ROSTER BACK — `update_band_alerts` diffs against the LAST roster pushed.
	_push_bands([
		_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture(),
		_lost_hunt_expedition_fixture(),
	])
	await _settle()

	# ---- THE FACTION PAGE (issue #450) -----------------------------------------------------------
	#
	# The all-band rollup, pinned as the cycler's FIRST entry. Its frames need a roster of MORE THAN ONE
	# band, which nothing else in this file stages for the panel: on a single band every total this page
	# prints is that band's own, so a page that had silently stopped summing would render identically.
	#
	# Reached through the REAL cycler (`Hud.cycle_panel_band`, the `◀` the player presses), never by
	# calling `render_faction` — the pinned entry IS the routing, and poking the renderer would prove
	# the page draws while saying nothing about whether it can be got to.
	# The base roster PLUS the corralled aurochs the second band keeps — appended, not substituted: the
	# party's own quarry lives in the base set, and a herd list that dropped it would leave the parties
	# row naming a raw `game_deer_79` id instead of the species.
	_set_world_herds(_herd_fixtures() + _under_herded_work_herd_fixtures())
	# **THE KNOWLEDGE ZONE'S THREE BLOCKS ARE STAGED AT THEIR WORST CASE, and that is the whole reason
	# the seeding is here rather than left to whatever the run happened to leave up.** Each block on
	# this page OMITS ITSELF when it has nothing to say and the zone CLIPS, so an unseeded fixture
	# measures a zone with two of its three blocks missing and calls the fit green. All FIVE craft
	# tracks (the ladder's ceiling, one finished so the `known` word renders beside four meters) and
	# MORE discovered kinds than the list will show, so the `+N more` row is in the measurement too.
	# SETTLING needs no push — `_ready`'s top-bar seed is a real score with a stage, and pushing a
	# second one here would only change the top bar under every frame below.
	_hud.update_intensification([_faction_knowledge_fixture()])
	_hud.update_discoveries([_faction_discoveries_fixture()])
	_push_bands(_faction_roster())
	_hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"band")
	await _settle()
	await _save("band_panel_faction")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_report_zone_content_extent("band_panel_faction")
	_assert_faction_page()

	# The WORK tab — the workforce bar and the per-band roster. A separate frame because the narrow
	# shell renders exactly ONE zone, so the state above cannot show it at all.
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_faction_work")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()

	# The KNOW tab — the fourth zone (issue #450's option A1), which exists on this subject and on no
	# other. Its own frame for the work tab's reason, and the tab is reachable at all only because the
	# faction subject DECLARED it: a band's layout has three zones and no `knowledge` key.
	_panel.set_active_tab(BandCityPanel.ZONE_KNOWLEDGE)
	await _settle()
	await _save("band_panel_faction_knowledge")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	# The FULL block's own extent, PRINTED rather than asserted: this is the tall dock, so the fit is
	# never in doubt here. What the number is for is the height tier's threshold, which has to sit
	# above it and below the ~300px a horizontal dock offers — and a threshold justified by an
	# ESTIMATE is exactly the kind of number this file keeps having to re-measure.
	_report_zone_content_extent("band_panel_faction_knowledge")
	_assert_faction_knowledge_zone()

	# WIDE: all FOUR zones abreast, which is the only layout in which the page can be read as a whole.
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_faction_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_report_zone_content_extent("band_panel_faction_wide")
	_assert_faction_zone_layout()
	# **THE KNOWLEDGE ZONE'S HEIGHT TIER, asserted here as the NEGATIVE half of a pair.** This dock's
	# zone is ~300px and cannot hold all three blocks at the page's row size, so DISCOVERIES must be
	# gone — while `band_panel_faction_knowledge`'s 1057px side dock, which asserts it PRESENT, is the
	# positive. Either claim alone is satisfied by a gate stuck in one position, and `content-fits`
	# cannot see it in either direction: a dropped block leaves a box that fits trivially, and a
	# clipped one still reports a rect inside its host.
	_assert_faction_knowledge_tier()
	await _assert_faction_shell_threshold()

	# The ROUTING claims, none of which a PNG can carry — and the last of them leaves the panel back on
	# a band, without which every state below would re-render as the rollup on its next snapshot.
	_assert_faction_cycler()

	# **PUT THE STANDING KNOWLEDGE ROW BACK.** `_ingest_intensification` REPLACES a faction's row, so
	# the five-track faction fixture would otherwise stand for the rest of the run — and the WORK
	# BOARD's ⌃ rung-ready marks are derived from that row (`RungGates.next_rung_ready`), so a state
	# below this one would mark a different set of rows as climbable.
	#
	# **The reason USED to be the top bar**, whose knowledge and discoveries strips were captured in
	# every frame below this; that block is retired (issue #450), so the restore now serves the board
	# alone. Restored to the standing row rather than cleared, because an EMPTY intensification array
	# is a no-op merge and would leave the five-track row standing.
	_hud.update_intensification([_standing_knowledge_row()])
	_hud.update_discoveries([])

	# ULTRAWIDE: past the width the three zones can USE, the wide shell CENTRES at its content cap
	# instead of stretching, leaving equal margins either side. Without it a single work row is strung
	# across the whole monitor and the band zone sits a screen away from the parties zone. The frame to
	# read is the equality of the two black margins — and that the board itself is unchanged.
	# This state pins a WINDOW and claims nothing about the canvas — see `_release_canvas_pin`, which is
	# what stops it inheriting the four-zone faction threshold's 1710px canvas from the block above.
	_release_canvas_pin()
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

	# THE SHELL THRESHOLD, bracketed. `wide_shell_min_width()` is DERIVED from what the wide shell needs
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
	# **THE THRESHOLD IS THE PANEL'S OWN, ASKED OF THE LIVE LAYOUT** — it is a sum over the declared
	# zones now, not a `const`, so a band subject answers 1190 and the four-zone faction page 1569. The
	# panel is on a BAND here (`_many_sources_band_fixture` above), so this brackets the three-zone
	# derivation; the four-zone one is bracketed by `_assert_faction_shell_threshold`.
	var shell_threshold: float = _panel.wide_shell_min_width()
	var shell_threshold_width := int(ceil(shell_threshold + rail_span))
	print("band_panel_preview: shell threshold probes at %d / %d (threshold %.0f + rail span %.0f)" % [
		shell_threshold_width - SHELL_THRESHOLD_UNDERSHOOT, shell_threshold_width,
		shell_threshold, rail_span])
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

	await _render_interface_scale_states()

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
	_assert_parked_chrome_margin("band_panel_dockrow_bottom", 1)
	_assert_shell_is_wide(true, "band_panel_dockrow_bottom")
	# **1920 IS PAST THE FORK NOW, AND THAT IS THE POINT OF THIS STATE.** It used to be the status-quo
	# side — the HUD yielded here, because with BOTH authored columns applied the card had 895 against
	# the 1190 the three zones need. The trailing column no longer reaches a bottom dock's strip, so the
	# card pays only the leading 360 and has 1239: enough. The fork fell from a logical 2432 to 1871, and
	# what 1920 buys is the defect this whole rule exists for — the tile column runs the window's full
	# height. What it COSTS is measured and deliberate: the band flank drops from two columns to one, so
	# the strip is 360 rather than 335 and the band zone renders at the SHORT tier.
	_assert_hud_yields_the_strip(false, "band_panel_dockrow_bottom")
	_assert_lateral_columns_reach_the_bottom(true, "band_panel_dockrow_bottom")
	# 1920 is the commonest window this panel ever draws in, and it is now a bounded strip — so the two
	# island claims and the column clearance are asked here as well as at the wide canvas below.
	_assert_card_clears_lateral_columns("band_panel_dockrow_bottom")
	_assert_rail_is_right_justified("band_panel_dockrow_bottom")
	_assert_card_is_centred("band_panel_dockrow_bottom")
	# …and the corner claim, staged, HERE as well as at the wide canvas — 1920 is the narrowest strip the
	# rule keeps, so it is where the card comes closest to the right dock's x-range (65px into it) and
	# where the clearance is doing the most work.
	await _assert_right_dock_clears_the_parked_chrome("band_panel_dockrow_bottom")
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
	# **THE WIDE SHELL HERE, AND IT FLIPPED WHEN THE READOUTS WENT** (issue #450). It was NARROW, and
	# that was arithmetic too: a top dock keeps the HUD's strip, and the trailing bound was the
	# top-bar readout block's LIVE 419px, so the card had 1920 − 360 (left dock) − 419 = 1141, under
	# the 1190 three zones need. With the block retired the trailing bound is the right dock's own
	# ~344, which leaves 1216 — over the threshold, and `_assert_work_zone_readable` above confirms
	# the board really gets its readable column rather than a squeezed one. The card gaining a shell
	# is the retirement's one geometric dividend, and this state is where it is stated.
	_assert_shell_is_wide(true, "band_panel_dockrow_top")
	await _assert_card_clears_hud_columns("band_panel_dockrow_top")

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
		_fail("round trip left the work zone at %s, baseline was %s" % [
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

	await _render_bottom_yield_states()

# ---- A BOTTOM DOCK THE HUD DOES NOT YIELD TO ---------------------------------------------------
#
# The reported defect: with the panel docked BOTTOM, the HUD's left column — the TILE/selection card —
# is cut off mid-content, because `Hud.set_reserved_inset` insets `LayoutRoot` on all four sides and a
# bottom reservation therefore shortens `ContentRow` across the WHOLE window, including the ~21% of it
# at the leading edge the band card never reaches.
#
# The trade, and it IS a trade: not yielding costs the card the two HUD columns (704px of authored
# width), so it is taken only where the card can pay them and stay in the wide shell
# (`Main.band_dock_overlays_hud`). These two states are the two sides of that fork, and the 1920 side
# is asserted up in `band_panel_dockrow_bottom` where it always was.

## The canvas that AFFORDS the yield, and it is DERIVED. The predicate compares
## `width − rail span (321 on the seeded Standard minimap) − 360 (the LEADING ceiling)` against
## `wide_shell_min_width()` — a BOTTOM dock charges no trailing bound — so on a band's three zones
## (1190) the fork sits at **1871**, confirmed by the promise walk below. 2560 clears it by 689px,
## which is what lets the LIVE right column render wider than its authored minimum without dragging
## the card out of the wide shell it is being asserted in. **The FOUR-zone faction page moves the fork
## with it**, the threshold being a sum over the live zone list — 1569 + the same rail and leading
## terms, i.e. ~2250. That one is DERIVED and not walked: the promise walk below runs on a band, and
## the page's own threshold is bracketed separately by `_assert_faction_shell_threshold`.
const BOTTOM_YIELD_CANVAS := Vector2i(2560, 1080)
## How far the lateral columns may stop short of the window's bottom edge and still count as reaching
## it. One pixel, `ZONE_BOUNDS_TOLERANCE`'s reason: these are float rects off a scaled canvas.
const COLUMN_BOTTOM_TOLERANCE := 1.0
## How many settles the yield rule is allowed after a viewport change before it must have stopped
## moving. Four, matching `BAND_COLUMNS_SETTLE_PASSES` — the two loops run through the same fan-out.
const BOTTOM_YIELD_SETTLE_PASSES := 4

func _render_bottom_yield_states() -> void:
	# The BUSY band: the overlap claim below needs a card wide enough to actually reach a column when
	# it is unbound, which a band with nothing to show never does (the same reason
	# `_assert_card_clears_hud_columns` states for the top dock).
	await _pin_canvas(BOTTOM_YIELD_CANVAS)
	_push_bands([_many_sources_band_fixture()])
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_dockrow_bottom_yield")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(true, "band_panel_dockrow_bottom_yield")
	_assert_parked_chrome_fits("band_panel_dockrow_bottom_yield")
	# The three halves of the claim, and none of them implies the others: the HUD kept its strip, the
	# tile column therefore reaches the window's bottom edge, and the card is STILL in the wide shell —
	# the thing a naive exemption loses.
	_assert_hud_yields_the_strip(false, "band_panel_dockrow_bottom_yield")
	_assert_lateral_columns_reach_the_bottom(true, "band_panel_dockrow_bottom_yield")
	_assert_shell_is_wide(true, "band_panel_dockrow_bottom_yield")
	# THE NEGATIVE. "The column got taller" passes trivially by letting it grow straight THROUGH the
	# card, so the card clearing both columns is asserted as its own rect claim, behind its own control.
	_assert_card_clears_lateral_columns("band_panel_dockrow_bottom_yield")
	_assert_rail_is_right_justified("band_panel_dockrow_bottom_yield")
	_assert_card_is_centred("band_panel_dockrow_bottom_yield")
	_report_bottom_yield_geometry("band_panel_dockrow_bottom_yield")
	await _assert_right_dock_clears_the_parked_chrome("band_panel_dockrow_bottom_yield")
	await _assert_bottom_yield_converges("band_panel_dockrow_bottom_yield")
	await _report_bottom_yield_at_high_scale()

	# Hand the block back exactly what it was given: the ultrawide canvas, a bottom dock and the quiet
	# band — `_render_interface_scale_states` re-pins and re-pushes, but the tier probes at the end of
	# the run read whatever roster they are left with.
	await _pin_canvas(Vector2i(ULTRAWIDE_WIDTH, DOCKROW_CANVAS.y))
	_push_bands([_band_fixture()])
	await _settle()

## GUARD: **WHOSE STRIP IS IT?** — asserted as the rule's answer AND as the inset the HUD is actually
## drawing with, because either can be right while the other is wrong: a predicate nobody applied moves
## nothing, and an inset that does not follow the predicate is the fan-out having gone stale.
##
## `expected_yield` is the STATUS-QUO direction — `true` means the HUD gave the strip up, i.e. the band
## dock does NOT overlay it.
func _assert_hud_yields_the_strip(expected_yield: bool, state_name: String) -> void:
	var size: float = _panel.current_reservation_size()
	var overlays: bool = MAIN_SCRIPT.band_dock_overlays_hud(_panel.get_dock(), size, _hud, _panel)
	# `LayoutRoot.offset_bottom` is the inset the HUD actually laid out with (negative, inward).
	var inset: float = -_hud.layout_root.offset_bottom
	var want_inset: float = size if expected_yield else 0.0
	var failures: Array[String] = []
	if overlays == expected_yield:
		failures.append("the rule says the HUD %s its strip; this state expects it to %s" % [
			"KEEPS" if overlays else "yields", "yield" if expected_yield else "KEEP it"])
	if absf(inset - want_inset) > ZONE_BOUNDS_TOLERANCE:
		failures.append("LayoutRoot is inset %.0fpx from the bottom, expected %.0f (reservation %.0f)" % [
			inset, want_inset, size])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the HUD %s its bottom strip (inset %.0f of a %.0f reservation)" % [
			state_name, "yields" if expected_yield else "KEEPS", inset, size])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

## GUARD: the reported symptom itself — **does the HUD's left column run to the bottom of the window?**
##
## `LeftDock` is `SIZE_EXPAND_FILL` inside `ContentRow`, so its rect's bottom edge IS how far down the
## tile card may draw; a bottom inset stops it a whole reservation short across the entire window. Both
## columns are measured, because the inset shortens the row and not one column of it.
##
## Asserted in BOTH directions from one call site, so the 1920 state states the clipping is still there
## (the status quo it must preserve) with the same words the wide state uses to say it is gone.
##
## **IT MEASURES REGIONS, AND FOR THE RIGHT COLUMN THE REGION IS NO LONGER THE DRAWN EXTENT.** Both
## regions still run to the window's bottom edge when the HUD keeps its strip — nothing insets
## `ContentRow` — but the right dock now holds its CARDS clear of the strip through its own margin
## (`Hud.set_right_column_bottom_clearance`), so its drawn content stops above what this measures. That
## claim is `_assert_right_dock_clears_the_parked_chrome`'s, and the two are not interchangeable: this
## one says the row was not shortened, that one says nothing is painted in the chrome's corner.
func _assert_lateral_columns_reach_the_bottom(expected: bool, state_name: String) -> void:
	var window_bottom: float = get_viewport().get_visible_rect().size.y
	var failures: Array[String] = []
	var reached: Array[String] = []
	for pair in [["the left dock", _hud.left_dock_region], ["the right dock", _hud.right_dock_region]]:
		var label: String = pair[0]
		var region: Control = pair[1]
		var bottom: float = region.get_global_rect().end.y
		var short_by: float = window_bottom - bottom
		reached.append("%s ends %.0f of %.0f" % [label, bottom, window_bottom])
		var touches: bool = short_by <= COLUMN_BOTTOM_TOLERANCE
		if touches != expected:
			failures.append("%s ends at %.0f of a %.0f window (%.0fpx short) — expected it to %s" % [
				label, bottom, window_bottom, short_by,
				"reach the bottom" if expected else "stop short at the strip"])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the HUD's columns %s the window's bottom edge (%s)" % [
			state_name, "reach" if expected else "correctly stop short of", ", ".join(reached)])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

## GUARD: **THE NEGATIVE — a full-height column and the card do not overlap.**
##
## `_assert_lateral_columns_reach_the_bottom` is satisfied by a column that grew straight through the
## card, which is the failure that would look best in a PNG and be worst in play (the tile card drawn
## under the band card). This is the top dock's `_assert_card_clears_hud_columns` on the edge where the
## columns are the DOCK REGIONS rather than the top-bar readouts — on a bottom dock the readout block
## is at the other end of the screen and shares no vertical band with the card, so a claim made against
## it would be true for free.
##
## **It takes the same negative control**: with the LEADING bound cleared the card must genuinely reach
## the left column, or the pass is a card too small to have collided with anything. The harness's own
## reservation listener has to come off the wire for that (it pushes the bounds straight back — `Main`'s
## behaviour, not an artifact).
##
## **THE TWO COLUMNS ARE MEASURED DIFFERENTLY, AND THAT IS THE POINT.** The left dock is bounded as a
## REGION, because it really is full-height and its cards may draw anywhere in it. The right dock is
## bounded as its DRAWN CONTENT (`_right_dock_content_reach`, i.e. clipped to `RightScroll`), because its
## region still spans the whole row while its cards stop above the strip — so a region-shaped claim there
## would forbid the card room the HUD is not using, which is exactly the reserve the trailing bound was
## and why it was dropped. The card is free to run under the right column's empty lower reach; what it
## may not do is touch anything painted there.
func _assert_card_clears_lateral_columns(state_name: String) -> void:
	var left := _hud.left_dock_region.get_global_rect()
	var card := _panel._panel.get_global_rect()
	_panel.reservation_changed.disconnect(_reservation_listener)
	_panel.set_lateral_bounds(0.0, 0.0)
	var unbound := _panel._panel.get_global_rect()
	var would_collide: bool = unbound.intersects(left)
	var live: Vector2 = _hud.lateral_column_widths()
	_panel.set_lateral_bounds(live.x, live.y)
	_panel.reservation_changed.connect(_reservation_listener)
	var reach: Dictionary = _right_dock_content_reach()
	var failures: Array[String] = []
	if not would_collide:
		failures.append("the UNBOUND card %s clears the left column %s anyway, so this state proves nothing — stage a busier band or a narrower canvas" % [unbound, left])
	if card.intersects(left):
		failures.append("the card %s is drawn over the left dock %s" % [card, left])
	if int(reach["cards"]) > 0 and card.intersects(Rect2(reach["painted"])):
		failures.append("the card %s is drawn over the right dock's painted content %s" % [
			card, str(reach["painted"])])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the card clears the full-height left column (and would collide unbound) and the right dock's drawn content, which stops at %.0f" % [
			state_name, float(reach["bottom"])])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

## GUARD: the yield rule is a FIXED POINT, and reaches it inside a bound.
##
## The argument is that the predicate reads only the viewport, the two AUTHORED column widths and the
## rail width the HUD declares — none of which the inset can move — so it cannot chase its own output.
## **This is the measurement rather than the argument.** The loop it would otherwise close is real and
## has three legs: the rule sets the inset, the inset sets the columns' HEIGHT, and
## `lateral_column_widths()` is a `max(authored, live)` over rects whose height just changed; a live
## term that dominated would let the bounds move the shell, the shell move the published reservation,
## and the reservation re-enter the rule. An oscillation would show as a panel flickering between two
## layouts on a resize, with every other assertion green on whichever frame was captured.
##
## It samples ACROSS viewport changes, because a rule already settled at one width cannot demonstrate
## that it settles — and it samples on BOTH SIDES OF THE FORK, which is the load-bearing part. A
## self-referential predicate is stable wherever the two branches agree; the only place it thrashes is a
## canvas where the bounds decide the answer, i.e. `DOCKROW_CANVAS`, where the row is wide enough
## unbounded (1599) and too narrow bounded (895). Measured: a predicate reading the panel's own live
## bounds instead of the authored widths leaves that canvas drawing a 395px strip while `Main` holds 360.
func _assert_bottom_yield_converges(state_name: String) -> void:
	for canvas in [BOTTOM_YIELD_CANVAS + Vector2i(BOTTOM_YIELD_NUDGE, 0), DOCKROW_CANVAS]:
		await _pin_canvas(canvas)
		var verdicts: Array[bool] = []
		var sizes: Array[float] = []
		var insets: Array[float] = []
		for _pass in range(BOTTOM_YIELD_SETTLE_PASSES):
			await _settle()
			verdicts.append(MAIN_SCRIPT.band_dock_overlays_hud(_panel.get_dock(),
				_panel.current_reservation_size(), _hud, _panel))
			sizes.append(_panel.current_reservation_size())
			insets.append(-_hud.layout_root.offset_bottom)
		var stable := true
		for i in range(1, verdicts.size()):
			if verdicts[i] != verdicts[0] or not is_equal_approx(sizes[i], sizes[0]) \
					or not is_equal_approx(insets[i], insets[0]):
				stable = false
		# The DRAWN strip against the PUBLISHED one, in the same breath: a loop that has stopped moving
		# between two settles can still have stopped half-way, with the panel drawing a size `Main` never
		# heard about (`band-city-panel.md` → "A size the panel DRAWS but never PUBLISHES").
		var drawn: float = _panel._root.get_global_rect().size.y
		if not is_equal_approx(drawn, sizes[sizes.size() - 1]):
			stable = false
		_assert_band_panel("%s: the yield rule is a fixed point at %dpx over %d settles after a resize (overlays %s, published %s, inset %s, drawn %.0f)"
			% [state_name, canvas.x, BOTTOM_YIELD_SETTLE_PASSES, str(verdicts), str(sizes), str(insets),
				drawn], stable)
	await _assert_bottom_yield_keeps_its_promise(state_name)
	await _pin_canvas(BOTTOM_YIELD_CANVAS)
	await _settle()

## How much wider the convergence probe re-pins the canvas. Any width past the fork works; 40px keeps
## the probe on the same side of it, so what is under test is the settling and not the verdict flipping.
const BOTTOM_YIELD_NUDGE := 40

## How far either side of the fork the promise probe walks, and in what step. The band it has to cover
## is the daylight between the bounds the rule reads and the LIVE ones the card is placed against —
## 75px when the rule read the columns' authored reservations (344 against a live 419 on the trailing
## one) — so the reach is comfortably past that and the step lands several probes inside it.
const YIELD_PROMISE_PROBE_STEP := 30
const YIELD_PROMISE_PROBE_REACH := 150

## GUARD: **THE RULE'S PROMISE — if the HUD KEEPS its strip, the card really IS in the wide shell.**
##
## `Main.band_dock_overlays_hud` is documented as "the HUD yields iff the card could NOT afford the wide
## shell with the bounds applied". The converse is the half nothing was asserting, and it is the half
## that broke: the rule asked the columns' AUTHORED reservations (which it must not read LIVE — that
## term is moved by the rule's own output) while the card is laid out against `lateral_column_widths()`'s
## `max(authored, live)`. Every pixel by which a live column exceeded its reservation was therefore a
## window width where the HUD kept the strip AND the card, paying the larger bound, collapsed to the
## narrow tabbed shell — precisely the trade `348e5c09` measured, rejected and wrote this rule to refuse.
## Measured before the fix: 344 authored against a live 419 left a 75px band (logical widths 2215-2289)
## in which every frame rendered the tabbed shell over a HUD that had kept its columns, publishing a
## 395px strip. The rule reads `Hud.left_column_ceiling` / `right_column_ceiling` now, neither of which
## live content can exceed. (The trailing one is passed and DISCARDED on a bottom dock — see
## `Hud.RIGHT_COLUMN_CEILING` — so what the fork actually turns on is the LEADING ceiling.)
##
## **THE SECOND CLAIM IS WHAT KEEPS THE CEILINGS HONEST.** The promise walk can only see content the
## harness happens to render, so it also asserts the ceilings still cover what the columns actually
## occupy right now — the invariant the whole fix rests on, and the one a new dock card would break.
##
## **IT IS A WALK, NOT A SPOT CHECK, AND THE WALK IS DERIVED.** The band sits immediately above the fork,
## so probing the two canvases the fixed-point check already visits (1920, 2600) would miss it in both
## directions. The fork is computed from the rule's own terms — the wide shell's minimum plus the rail's
## span plus the two column ceilings — and the probe walks from below it to well past it, so the covered
## range follows a retune of any of them instead of going quietly stale at a hardcoded width.
##
## The claim is one-directional on purpose: a width where the HUD YIELDS says nothing (the card then has
## the whole row and any shell is legitimate). That makes it satisfiable by a rule that never keeps the
## strip at all, so the walk also asserts it saw the HUD keep it at least once.
func _assert_bottom_yield_keeps_its_promise(state_name: String) -> void:
	var edge: int = _panel.get_dock()
	var size: float = _panel.current_reservation_size()
	var rail_span: float = _panel._rail_span_of(_hud.bottom_chrome_rail_width(edge, size))
	# **THE TRAILING TERM GOES THROUGH `_trailing_bound_for`, NOT STRAIGHT IN.** The rule charges a
	# BOTTOM dock's card for the leading column only, so a walk that restated `left + right` would derive
	# 2432 while the fork sat at 1871 and probe a band 560px clear of it — passing, and measuring
	# nothing. Reading the panel's own definition is what keeps the walk pointed at the fork when the
	# rule's terms move, which is exactly what this docstring claims for it.
	var fork: float = _panel.wide_shell_min_width() + rail_span \
		+ _hud.left_column_ceiling() \
		+ _panel._trailing_bound_for(edge, _hud.right_column_ceiling())
	_assert_ceilings_cover_the_columns(state_name, "this state's readouts")
	await _assert_ceilings_cover_the_widest_right_column(state_name)
	var broken: Array[String] = []
	var kept := 0
	var width := int(ceilf(fork)) - YIELD_PROMISE_PROBE_STEP
	while width <= int(ceilf(fork)) + YIELD_PROMISE_PROBE_REACH:
		await _pin_canvas(Vector2i(width, DOCKROW_CANVAS.y))
		await _settle()
		if MAIN_SCRIPT.band_dock_overlays_hud(_panel.get_dock(),
				_panel.current_reservation_size(), _hud, _panel):
			kept += 1
			if not _panel._shell_is_wide():
				broken.append("%d (bounds %.0f/%.0f, published %.0f)" % [width,
					_panel._bound_leading, _panel._bound_trailing,
					_panel.current_reservation_size()])
		width += YIELD_PROMISE_PROBE_STEP
	_assert_band_panel("%s: the HUD kept its bottom strip at %d width(s) across the fork (%.0f) and the card was in the WIDE shell at every one of them%s"
		% [state_name, kept, fork,
			"" if broken.is_empty() else " — NARROW at " + ", ".join(broken)],
		kept > 0 and broken.is_empty())

## The invariant the promise rests on, asked of whatever the HUD is rendering right now: a ceiling that
## does not cover what its column OCCUPIES is a rule promising the card room it will then be denied.
func _assert_ceilings_cover_the_columns(state_name: String, under: String) -> void:
	var occupied: Vector2 = _hud.lateral_column_widths()
	_assert_band_panel("%s: the column ceilings (%.0f / %.0f) cover what the columns occupy under %s (%.0f / %.0f)"
		% [state_name, _hud.left_column_ceiling(), _hud.right_column_ceiling(), under,
			occupied.x, occupied.y],
		_hud.left_column_ceiling() >= occupied.x and _hud.right_column_ceiling() >= occupied.y)

## …and the same invariant asked under the WIDEST CONTENT THE RIGHT COLUMN CAN HOLD, which is the only
## form of it that actually guards `Hud.RIGHT_COLUMN_CEILING`.
##
## The claim above is true of every state in this file and says almost nothing: the right dock is empty
## in nearly all of them, so it reads its authored minimum and a ceiling set anywhere at or above that
## passes without being tested.
##
## **WHAT IT STAGES CHANGED WITH THE COLUMN, and the change is the point (issue #450).** The ceiling was
## measured against the TOP-BAR READOUTS — the knowledge strip's first row, at 561px — and that whole
## block is deleted from `HudLayer.tscn`. The right-hand column is the RIGHT DOCK alone now, so the
## widest thing it can hold is the widest CARD it can hold, and the staging is the one
## `_assert_right_dock_clears_the_parked_chrome` already uses for the vertical question: the Victory
## card plus a legend long enough to reach `LegendController.LEGEND_MAX_HEIGHT`. Staging the retired
## readouts instead would call `update_demographics`, which no longer exists — and a missing method
## does not fail politely here, it ABORTS this coroutine and takes every assertion under it with it.
##
## **It restores the dock EXACTLY, and that is checked by the frame set rather than trusted** — the HUD
## is long-lived, so a legend or a Victory card left showing re-renders in every later frame.
func _assert_ceilings_cover_the_widest_right_column(state_name: String) -> void:
	var legend_rows: Array = []
	for i in range(_legend_worst_case_rows()):
		legend_rows.append({"label": "Terrain %d" % i, "value_text": "%d tiles" % i,
			"color": LEGEND_WORST_CASE_SWATCH})
	_hud.toggle_victory()
	_hud.toggle_legend()
	_hud.update_overlay_legend({"key": "terrain", "title": "Terrain Types", "rows": legend_rows})
	await _settle()
	_assert_ceilings_cover_the_columns(state_name, "the widest content the right dock can hold")
	# Hand the right dock back exactly as it was found: legend emptied AND re-suppressed, Victory hidden.
	_hud.update_overlay_legend({})
	_hud.toggle_legend()
	_hud.toggle_victory()
	await _settle()

## REPORT (never an assertion): **where the yield's RESPONSIVE FALLBACK sits.** `content_scale_factor`
## shrinks the LOGICAL viewport by exactly the scale, and every term of the rule is a logical constant,
## so a wide monitor at a high interface scale falls back under the fork and the strip is yielded again —
## the clipping returns, by design, exactly as the wide→narrow shell fallback behaves one layer down.
## It is PRINTED rather than asserted because the boundary is a consequence of two independent constants
## and pinning it would make every future retune of either one fail here for no defect.
##
## **THE SCALE IS RESTORED BEFORE THIS RETURNS, and asserted to be** — `content_scale_factor` is WINDOW
## state, not scene state, so a leak silently re-projects every later frame with nothing failing.
func _report_bottom_yield_at_high_scale() -> void:
	for scale_variant in [ClientSettings.UI_SCALE_DEFAULT, SCALE_STATE_UI_SCALE]:
		var scale: float = float(scale_variant)
		_apply_ui_scale(scale)
		await _settle()
		var logical: Vector2 = get_viewport().get_visible_rect().size
		# Same reading of the trailing term as the promise walk's fork, for the same reason: a printed
		# span that charged a column the rule does not would report the fallback at the wrong scale.
		var span: float = _panel._panel_width_extent() - _panel._rail_span() \
			- _hud.left_column_ceiling() \
			- _panel._trailing_bound_for(_panel.get_dock(), _hud.right_column_ceiling())
		print("band_panel_preview: bottom-dock yield at ui_scale %.2f on a %dpx canvas — logical viewport %.0f, span with the column ceilings %.0f of the %.0f the wide shell needs → the HUD %s its strip" % [
			scale, BOTTOM_YIELD_CANVAS.x, logical.x, span, _panel.wide_shell_min_width(),
			"KEEPS" if MAIN_SCRIPT.band_dock_overlays_hud(_panel.get_dock(),
				_panel.current_reservation_size(), _hud, _panel) else "yields"])
	_apply_ui_scale(ClientSettings.UI_SCALE_DEFAULT)
	await _settle()
	_assert_band_panel("bottom-dock yield: the interface scale is restored, so no later state inherits it",
		is_equal_approx(get_window().content_scale_factor, float(ClientSettings.UI_SCALE_DEFAULT)))

## REPORT (never an assertion): the numbers the yield trade is made of, and what the BAND FLANK pays for
## it. The flank's count reads `_available_card_span()`, so taking the two bounds off the span can drop
## a wide monitor's second band column — the trade has to be legible, not discovered later. The
## "unbound" figures are what the panel WOULD answer with the bounds cleared, i.e. the behaviour before
## this rule existed, computed from the panel's own arithmetic rather than measured in a second pass.
func _report_bottom_yield_geometry(state_name: String) -> void:
	var strip: float = _panel._panel_width_extent()
	var rail: float = _panel._rail_span()
	var bounded: float = _panel._available_card_span()
	var unbounded: float = maxf(strip - rail, 0.0)
	# The panel's own `zone_columns()` arithmetic on a HYPOTHETICAL span, which is why it is restated
	# here at all: the live call answers about the bounded span and this line's whole point is the other
	# one. It reads `wide_shell_min_width()` — the live sum over the declared zones — rather than a
	# hand-listed pair of flanks, so it follows a four-zone subject instead of quietly reporting a
	# band's answer for a faction page.
	var flank_room := func(span: float) -> int:
		var room: float = span - (_panel.wide_shell_min_width() - BandCityPanel.ZONE_BAND_WIDTH)
		return clampi(int(room / BandCityPanel.ZONE_BAND_WIDTH), 1, BandCityPanel.BAND_ZONE_MAX_COLUMNS)
	print("band_panel_preview: %s — strip %.0f, rail span %.0f, HUD columns %.0f/%.0f (ceilings %.0f/%.0f) → card span %.0f bounded / %.0f unbound (wide shell needs %.0f); band flank %d column(s) bounded / %d unbound; work board %d column(s)" % [
		state_name, strip, rail, _panel._bound_leading, _panel._bound_trailing,
		_hud.left_column_ceiling(), _hud.right_column_ceiling(), bounded, unbounded,
		_panel.wide_shell_min_width(), _panel.band_zone_columns(), flank_room.call(unbounded),
		_panel._work_columns])

## A swatch colour for the staged legend's rows. Any colour renders the same box; it is named so the
## fixture carries no bare literal.
const LEGEND_WORST_CASE_SWATCH := Color(0.3, 0.5, 0.2)

## How many terrain rows the staged legend carries: enough to drive its inner scroll to
## `LegendController.LEGEND_MAX_HEIGHT`, i.e. the tallest that card can ever be. DERIVED from the
## controller's own row arithmetic (`LEGEND_MIN_ROW_HEIGHT + LEGEND_ROW_PADDING` is its `_row_height()`),
## so a retune of either moves the fixture with it instead of leaving it quietly short.
func _legend_worst_case_rows() -> int:
	var row_height: float = LegendController.LEGEND_MIN_ROW_HEIGHT + LegendController.LEGEND_ROW_PADDING
	return int(ceilf(LegendController.LEGEND_MAX_HEIGHT / row_height))

## GUARD: **THE RIGHT DOCK'S DRAWN CONTENT STAYS OUT OF THE STRIP THE PARKED CHROME IS IN.**
##
## The other half of the flush-right rail, and the reason the rail may BE flush. When the HUD keeps a
## bottom dock's strip, `DockRowController` has parked the minimap, the zoom rail and the turn orb into
## that strip's trailing end, hard against the screen — the same corner the right dock's cards occupy.
## Measured on this canvas before the clearance existed: the Telling card at its page cap, the Victory
## card and an 11-row Terrain Types legend put the right dock's content at y 170→1151 against a strip
## whose top edge is 720, so the legend card alone lay 334px inside the parked chrome.
##
## **It is asserted against BOTH rects, and neither implies the other.** Clearing the strip's whole
## band is the general claim — it is what a future right-dock card has to keep satisfying — while
## clearing the RAIL's own rect is the specific pair sharing that corner, and a strip that grew or a
## rail that widened would break them at different moments.
##
## **THE NEGATIVE CONTROL IS THE WHOLE VALUE OF IT** (`_assert_card_clears_lateral_columns`' rule): the
## right dock is empty in every other state in this file, and an empty column clears anything. So the
## dock is STAGED at the tallest content it can hold — the Victory card plus a legend long enough to
## reach `LEGEND_MAX_HEIGHT` — and the clearance is then RELEASED to check that this content really
## does reach the chrome without it, before it is put back and the claim is made.
##
## **The staging is restored exactly and the restore is not incidental**: the HUD is long-lived, so a
## legend or a Victory card left showing re-renders in every later frame. No narrative beat is pushed
## for the same reason `TellingPanel` is untouched everywhere else here — the page turn is the client's
## one `Tween`, and a `Tween` at `Engine.time_scale = 0` never advances at all.
func _assert_right_dock_clears_the_parked_chrome(state_name: String) -> void:
	if not _hud.has_method("set_right_column_bottom_clearance"):
		_fail("%s — the HUD has no right-column clearance to assert" % state_name)
		return
	var legend_rows: Array = []
	for i in range(_legend_worst_case_rows()):
		legend_rows.append({"label": "Terrain %d" % i, "value_text": "%d tiles" % i,
			"color": LEGEND_WORST_CASE_SWATCH})
	_hud.toggle_victory()
	_hud.toggle_legend()
	_hud.update_overlay_legend({"key": "terrain", "title": "Terrain Types", "rows": legend_rows})
	await _settle()

	# THE CONTROL, first: with the clearance released this content must genuinely reach the chrome, or
	# the claim below is a right dock that was never tall enough to collide with anything.
	var clearance: float = float(_hud.call("right_column_bottom_clearance"))
	_hud.set_right_column_bottom_clearance(0.0)
	await _settle()
	var unheld: Dictionary = _right_dock_content_reach()
	_hud.set_right_column_bottom_clearance(clearance)
	await _settle()
	var held: Dictionary = _right_dock_content_reach()

	var strip_top: float = _panel._root.get_global_rect().position.y
	_assert_band_panel("%s: WITHOUT the clearance the right dock's cards really do reach the parked chrome (content ends %.0f against a strip starting %.0f; %d of them over the rail)"
		% [state_name, float(unheld["bottom"]), strip_top, int(unheld["over_rail"])],
		float(unheld["bottom"]) > strip_top and int(unheld["over_rail"]) > 0)
	_assert_band_panel("%s: the right dock's %d drawn card(s) stop above the strip (content ends %.0f of a strip starting %.0f, %.0fpx clear) under a %.0fpx clearance"
		% [state_name, int(held["cards"]), float(held["bottom"]), strip_top,
			strip_top - float(held["bottom"]), clearance],
		int(held["cards"]) > 0 and float(held["bottom"]) <= strip_top + ZONE_BOUNDS_TOLERANCE)
	# The MECHANISM behind that claim, stated separately because the clip box is what a future card is
	# bounded by: a card can only overflow the strip if the box it is clipped to reaches into it.
	_assert_band_panel("%s: the right dock's clip box %s ends above the strip (%.0f of %.0f)"
		% [state_name, str(held["clip"]), Rect2(held["clip"]).end.y, strip_top],
		Rect2(held["clip"]).end.y <= strip_top + ZONE_BOUNDS_TOLERANCE)
	_assert_band_panel("%s: no right-dock card is drawn over the parked chrome %s (%d overlapping)"
		% [state_name, str(_panel._rail.get_global_rect()), int(held["over_rail"])],
		int(held["over_rail"]) == 0)
	# **THE CARD IS THE SECOND ISLAND IN THAT CORNER, AND IT ONLY BECAME ONE WHEN THE TRAILING BOUND WAS
	# DROPPED.** While the card held the right column clear it could not reach the right dock's x-range
	# at all; it now runs to one gutter short of the rail, which overlaps that range by ~65px at 1920 —
	# so the only thing keeping the two apart is this clearance, and it has to be asserted about the card
	# as well as the chrome. It is asked HERE rather than beside the card's own column claim because the
	# right dock has to be STAGED tall for it to mean anything: at the fixtures' near-empty right dock
	# the content stops ~400px above the strip whatever the clearance does.
	#
	# **IT CURRENTLY PASSES ON THE HORIZONTAL AXIS TOO, BY 1.5px AT 1920 — WHICH IS WHY THE GAP IS
	# PRINTED.** `_card_width()` is quantised to whole zone columns, so the card stops a little short of
	# the span it is allowed; the span itself reaches `extent - rail_span`, which is 23px INSIDE the
	# right dock's painted x-range, so a card whose column arithmetic ever gave it those 23px would be
	# relying on the vertical clearance alone. A future reader must be able to see how thin that is
	# rather than read a green line.
	var card := _panel._panel.get_global_rect()
	var painted := Rect2(held["painted"])
	_assert_band_panel("%s: the band card %s is not drawn over the right dock's painted content %s (%.0fpx of horizontal daylight, %.0f vertical)"
		% [state_name, str(card), str(painted), painted.position.x - card.end.x,
			painted.position.y - card.end.y],
		int(held["cards"]) > 0 and not card.intersects(painted))

	# Hand the right dock back exactly as it was found: legend emptied AND re-suppressed, Victory hidden.
	_hud.update_overlay_legend({})
	_hud.toggle_legend()
	_hud.toggle_victory()
	await _settle()

## How far down the window the right dock actually DRAWS, and how much of that lands over the parked
## chrome.
##
## **Never `right_dock_region`, whose rect spans the whole row whether or not anything is painted in
## it** — that is exactly the distinction `348e5c09` got the wrong side of, bounding the rail against a
## reserved REGION rather than against drawn content.
##
## **And never a card's bare rect either.** `RightStack` is a `VBoxContainer` inside `RightScroll`, so a
## card taller than the box keeps its full height and simply hangs out of the bottom of it — measured
## here at 1193 in a box ending at 1056. What the player sees is the card CLIPPED to that scroll, which
## is why every rect is intersected with it: the clip box is the right dock's real drawn extent, and it
## is the thing the clearance moves.
func _right_dock_content_reach() -> Dictionary:
	var rail := _panel._rail.get_global_rect()
	var clip := _hud.right_dock_scroll.get_global_rect()
	var bottom := -INF
	var cards := 0
	var over_rail := 0
	var painted := Rect2()
	for child in _hud.right_stack.get_children():
		var card := child as Control
		if card == null or not card.visible:
			continue
		var drawn := card.get_global_rect().intersection(clip)
		if drawn.size.y <= 0.0 or drawn.size.x <= 0.0:
			continue
		painted = drawn if cards == 0 else painted.merge(drawn)
		cards += 1
		bottom = maxf(bottom, drawn.end.y)
		if drawn.intersects(rail):
			over_rail += 1
	# `painted` is the union of the DRAWN card rects — what another island in this corner may not touch.
	# An empty right dock leaves it a zero rect, which intersects nothing, and the `cards` count beside
	# it is what stops that reading as a pass.
	return {"bottom": bottom, "cards": cards, "over_rail": over_rail, "clip": clip, "painted": painted}

# ---- THE PANEL AT A HIGH INTERFACE SCALE ------------------------------------------------------
#
# `Window.content_scale_factor` shrinks the LOGICAL viewport by exactly the scale, so a 1920x1080
# window at `ui_scale` 1.35 lays the HUD out in 1422x800 — and every one of this panel's breakpoints
# is a LOGICAL constant, correctly so. What that reaches is the NARROW shell on a HORIZONTAL dock:
# measured, a bottom dock's `_available_card_span()` falls to 1101 against the 1190 the three zones
# need. That combination had no frame and no assertion in this file, and the box it hands its one
# zone was 35px SHORT of the box every tier threshold is tuned against — sliced silently, since the
# zone hosts clip.
#
# **It is not a scale feature, it is a WINDOW-SIZE one.** The identical shell is reached at
# `ui_scale` 1.0 in a window under ~1511px wide; the scale is simply the way a player gets there on
# a 1920 monitor, and the way the defect was reported.
#
# THE SCALE IS RESTORED BEFORE THIS RETURNS, and asserted to be: `content_scale_factor` is WINDOW
# state, not scene state, so a leak silently re-projects every later frame with nothing failing —
# the discipline `tools/ui_preview/chapters/interface_scale.gd` carries for the same reason.

## The scale these states render at — the one the two docked-panel defects were reported at. On
## `DOCKROW_CANVAS` it yields a 1422x800 logical viewport, which is what puts a BOTTOM dock's card
## span (1101) under `wide_shell_min_width()` and so into the narrow shell.
const SCALE_STATE_UI_SCALE := 1.35
## How far a rect may sit outside the viewport and still count as contained. A pixel, the
## `ZONE_BOUNDS_TOLERANCE` reason: these are float rects off a scaled canvas, not integers.
const SCALE_BOUNDS_TOLERANCE := 1.0
## A badge to push at the reservation-independence guard. Its VALUE is irrelevant — what is under
## test is that pushing one cannot move the strip's cross-axis size.
const SCALE_BADGE_TEXT := "3"

func _render_interface_scale_states() -> void:
	await _pin_canvas(DOCKROW_CANVAS)
	_push_bands([_band_fixture()])
	_apply_ui_scale(SCALE_STATE_UI_SCALE)

	# BOTTOM — the reported dock. The strip is horizontal and the card span has fallen under the
	# threshold, so this is the narrow shell in a height-capped strip: the one configuration in which
	# the shell's own tab bar used to be paid for out of the zone's box.
	_panel.set_dock(SIDE_BOTTOM)
	# The BAND tab explicitly: the narrow shell renders ONE zone, the run above leaves whichever tab it
	# last selected, and the band zone is the tall one — the role cards at the end of it are what the
	# report saw cut off.
	_panel.set_active_tab(&"band")
	await _settle()
	await _save("band_panel_scale_bottom")
	_assert_shell_is_wide(false, "band_panel_scale_bottom")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_panel_within_window("band_panel_scale_bottom")
	await _assert_badge_cannot_move_the_reservation("band_panel_scale_bottom")
	_report_zone_content_extent("band_panel_scale_bottom")

	# LEFT — the other dock in the report. A vertical strip is `PANEL_WIDTH` at every scale, so the
	# claim here is the containment one: the fixed 380px column and its full-height card must still
	# lie inside a viewport that is now 800px tall.
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_scale_left")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_panel_within_window("band_panel_scale_left")
	_report_zone_content_extent("band_panel_scale_left")

	_apply_ui_scale(ClientSettings.UI_SCALE_DEFAULT)
	_assert_band_panel("the interface scale is restored, so no later state inherits it",
		is_equal_approx(get_window().content_scale_factor, float(ClientSettings.UI_SCALE_DEFAULT)))

	await _assert_declared_input_republishes()

	await _render_band_column_states()
	# Hand the tier probes back the canvas the dock-row block left them on — they re-dock and re-push
	# their own band, but they take whatever canvas they are given.
	await _pin_canvas(Vector2i(ULTRAWIDE_WIDTH, DOCKROW_CANVAS.y))
	await _settle()

# ---- THE BAND FLANK'S COLUMN COUNT --------------------------------------------------------------
#
# On a horizontal dock the band zone lays its blocks out across `BandCityPanel.band_zone_columns()`
# columns — "vertical docking favours height, horizontal favours width". The count is PURELY
# GEOMETRIC, and that is the invariant this block exists to hold: it is what keeps the strip's height
# (and therefore `MapView`'s inset, and therefore its cache) off the snapshot's critical path.

## The canvas that affords the band flank TWO columns on a bottom dock, and the one that affords ONE.
##
## **DERIVED, and the derivation now has a branch in it — which is the whole content of the fork.** The
## flank's room is the strip less the chrome rail's span (321 on the seeded Standard minimap), the card
## chrome (26), the parties flank (354), one work column (380), the separators (50) **and whatever
## lateral bound the card is paying**. Below the fork (1871) the HUD yields and there is no bound, so the
## room is `width - 1131` and two columns (760) would arrive at 1891; above it the HUD keeps its strip
## and the card pays the leading column, so the room is `width - 1491` and two columns arrive at
## **2251**. The first of those is unreachable — 1891 is already past the fork — so the two-column band
## starts at 2251, and 2560 is the first real monitor width past it (QHD), clearing it by 309.
##
## It used to read 1920, which is now a ONE-column canvas: dropping the trailing bound moved the fork
## below 1920, so that width went from "yields, unbounded, two columns" to "keeps, bounded, one".
##
## 1600 is unchanged and still lands squarely in the one-column band of a WIDE shell (the shell itself
## needs a 1190 span, i.e. 1511), where a narrower canvas would be testing the narrow shell instead.
const BAND_COLUMNS_TWO_CANVAS := Vector2i(2560, 1080)
const BAND_COLUMNS_ONE_CANVAS := Vector2i(1600, 1080)
## How many settles a geometry change is allowed before the layout must have stopped moving. The
## count and the reservation feed each other through `Main`'s fan-out, so "it converges" is a claim
## with a bound, not a hope — see `_assert_band_columns_converge`.
const BAND_COLUMNS_SETTLE_PASSES := 4

func _render_band_column_states() -> void:
	# TWO columns — the wide-monitor case the rule is for.
	await _pin_canvas(BAND_COLUMNS_TWO_CANVAS)
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_band_columns_two")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_band_columns("band_panel_band_columns_two", 2)
	_assert_band_tier_rises("band_panel_band_columns_two")
	_assert_band_flank_is_full("band_panel_band_columns_two")
	# THE TWO-COLUMN BUDGET'S PARK GATE, which lives wherever two columns do. It rode
	# `band_panel_dockrow_bottom` until the fork moved below 1920 and took that canvas back to one
	# column; 335 is the tighter of the two budgets and is the one that can decline the gate.
	_assert_parked_chrome_margin("band_panel_band_columns_two", 2)
	# THE PRECONDITION FOR THE SPLIT PAIR: this state must really be the chartless one, or "the
	# chartless split balances" is a claim about the other layout.
	_assert_band_flank_charts("band_panel_band_columns_two", false)
	# **THE NON-OVERFLOWING half of the scroll pair.** This band fields no parties, so the list is one
	# hint line and the bar must stay hidden — without it, "scrolls iff it overflows" would pass on a
	# list that scrolls unconditionally, which is a visible scrollbar on every band in the game.
	_assert_scroll_only_where_sanctioned()
	_assert_parties_list_scrolls_iff_it_overflows("band_panel_band_columns_two")
	_report_zone_content_extent("band_panel_band_columns_two")
	var two_column_strip: float = _panel.current_reservation_size()
	await _assert_band_columns_converge("band_panel_band_columns_two")
	await _assert_band_columns_ignore_content("band_panel_band_columns_two")

	# THE CHARTED VARIANT, at the same span. **Both authored splits need their own state**: the one
	# above is the CHARTLESS band — a fresh band has no food history to chart, so it is turn one and
	# the first flank a new player ever sees — and without this second frame the charted pairing would
	# regress silently, since no other two-column state in this file carries a chart.
	_push_bands([_arrivals_band_fixture()])
	await _settle()
	await _save("band_panel_band_columns_two_charted")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_band_columns("band_panel_band_columns_two_charted", 2)
	_assert_band_tier_rises("band_panel_band_columns_two_charted")
	_assert_band_flank_is_full("band_panel_band_columns_two_charted")
	_assert_band_flank_charts("band_panel_band_columns_two_charted", true)
	_report_zone_content_extent("band_panel_band_columns_two_charted")
	_push_bands([_band_fixture()])
	await _settle()

	# ONE column — the regression bar. Everything here must read exactly as it did before the flank
	# could widen at all.
	await _pin_canvas(BAND_COLUMNS_ONE_CANVAS)
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_band_columns_one")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_band_columns("band_panel_band_columns_one", 1)
	# THE REGRESSION BAR, stated as its own claim: one column must still pick the tier it always did.
	# The tier budget is now the box TIMES the count, so a bug in that arithmetic would show up first
	# as a one-column flank quietly promoted into a taller tier it has no room for.
	_assert_band_panel("band_panel_band_columns_one: one column still picks the SHORT tier (%s)"
		% _band_zone_tier_name(),
		_hud._bandpanel._band_zone_tier == HudWorkVocab.BAND_ZONE_TIER_SHORT)
	_report_zone_content_extent("band_panel_band_columns_one")
	var one_column_strip: float = _panel.current_reservation_size()

	# **THE TWO-COLUMN STRIP IS SHORTER, AND THE ONE-COLUMN STRIP IS UNTOUCHED.** Two claims, not one,
	# and the second is the regression bar: a one-column flank still stacks 299px into its 300px box,
	# so a budget cut applied flat would slice it — and every TOP dock is one column (the lateral
	# bounds cost it 704px of span), which is why no top-dock frame may move.
	#
	# Asserted as EQUALITIES against the panel's own two consts rather than as an inequality: "shorter"
	# is satisfied by any cut, including one that drops the strip under the parked chrome's
	# requirement, which is the failure `_assert_parked_chrome_margin` exists for.
	_assert_band_panel("band columns: ONE column keeps the full body budget (%.0f, want %.0f) — its flank still stacks 299 of a 300px box"
		% [one_column_strip, BandCityPanel.PANEL_HEIGHT_WIDE],
		is_equal_approx(one_column_strip, BandCityPanel.PANEL_HEIGHT_WIDE))
	_assert_band_panel("band columns: TWO columns shorten the strip to the two-column budget (%.0f, want %.0f) — %.0fpx of map handed back"
		% [two_column_strip, BandCityPanel.PANEL_HEIGHT_WIDE_TWO_COLUMN,
			one_column_strip - two_column_strip],
		is_equal_approx(two_column_strip, BandCityPanel.PANEL_HEIGHT_WIDE_TWO_COLUMN))

	await _pin_canvas(Vector2i(ULTRAWIDE_WIDTH, DOCKROW_CANVAS.y))
	await _settle()

## The least of the room its columns offer that a widened flank may fill before it counts as empty.
##
## **MEASURED OVER THE WHOLE FLANK — content summed across the columns, against `columns × box` — and
## NOT over the deepest one.** The deepest-column form was the first thing written here and it was a
## guard that could not fail for the reason it existed: the chartless flank sat at 130 against 263 and
## passed at 88%, because 88% was the tall column's number and the short one was invisible to it. The
## flank's OWN emptiness is a total, so the total is what is measured.
##
## 0.60 clears the chartless flank's arithmetic CEILING with room to spare — its three blocks total
## 393px against the 600px two columns of a 300px box offer, i.e. 66%, and no split can beat that,
## because the total is the total however it is dealt out — while a SHORT-tier two-column flank fails
## it. It is deliberately not tighter:
## the number it must catch is a tier that failed to rise, not a block that gained a row.
const BAND_FLANK_FILL_FLOOR := 0.60
## How short the LESSER column may be against the taller before the flank reads as lopsided rather
## than laid out. 0.75 passes both authored splits at their MEASURED worst (**246/326 = 0.75 charted,
## 200/256 = 0.78 chartless**) and fails the charted split applied to a chartless band (130/263 =
## 0.49), which is exactly the case the second authored split exists for.
##
## **THE CHARTED FLANK NOW SITS EXACTLY ON IT — 1.5px of slack** (`246 >= 326 × 0.75`). It was 0.94
## before the role cards grew their kit pickers, and this number is now the tightest constraint on the
## band flank: the next row to land in the WORKFORCE column trips it. Re-author the split and
## re-measure — do not lower the floor to fit.
const BAND_FLANK_BALANCE_FLOOR := 0.75

## GUARD: **the widened flank SPENDS the height it recovered.** Two columns halve what each carries,
## so the tier must rise to put the recovered room back into content — the food-outlook chart and the
## role cards' descriptions, both of which the SHORT tier drops.
##
## Asserted on the tier the render actually BUILT with (`_band_zone_tier`), not re-derived from the
## box, so it fails if the tier-height arithmetic and the build ever disagree.
func _assert_band_tier_rises(state_name: String) -> void:
	var tier: int = _hud._bandpanel._band_zone_tier
	_assert_band_panel("%s: the widened flank rises out of the SHORT tier (%s) — the chart and the role-card hints come back"
		% [state_name, _band_zone_tier_name()],
		tier != HudWorkVocab.BAND_ZONE_TIER_SHORT)

## GUARD: …and the room it recovered is actually OCCUPIED, in BOTH columns.
##
## Two claims, because a flank can fail this in two independent ways and one number cannot see both:
## it can be uniformly empty (a tier that did not rise), or it can be lopsided (one column full beside
## a third-full one — the charted split applied to a chartless band). The FILL is the whole flank's
## content against the whole flank's room; the BALANCE is the shorter column against the taller.
func _assert_band_flank_is_full(state_name: String) -> void:
	var columns := _band_flank_column_extents()
	if columns.is_empty():
		_fail("%s — no band flank columns to measure" % state_name)
		return
	var box: float = _band_flank_box_height()
	var used := 0.0
	var tallest := 0.0
	var shortest := INF
	for extent_variant in columns:
		var extent: float = extent_variant
		used += extent
		tallest = maxf(tallest, extent)
		shortest = minf(shortest, extent)
	var room: float = box * float(columns.size())
	_assert_band_panel("%s: the flank fills the room its columns offer (%.0f of %.0f = %d%%, floor %d%%)"
		% [state_name, used, room, int(round(100.0 * used / room)),
			int(round(100.0 * BAND_FLANK_FILL_FLOOR))],
		room > 0.0 and used >= room * BAND_FLANK_FILL_FLOOR)
	# Only a SPLIT flank can be lopsided; one column is the whole flank and balances with itself.
	if columns.size() < 2:
		return
	_assert_band_panel("%s: …and its columns are level (%s of a %.0fpx box, shorter/taller = %d%%, floor %d%%)"
		% [state_name, str(columns.map(func(e: float) -> String: return "%.0f" % e)), box,
			int(round(100.0 * shortest / tallest)), int(round(100.0 * BAND_FLANK_BALANCE_FLOOR))],
		tallest > 0.0 and shortest >= tallest * BAND_FLANK_BALANCE_FLOOR)

## GUARD: which of the two authored splits this state is actually exercising. Named as the CHART's
## presence because that is the boolean `build_band_zone` selects on — and stated as its own assertion
## because both split claims are otherwise satisfiable by whichever layout happened to render.
func _assert_band_flank_charts(state_name: String, want: bool) -> void:
	var host := _band_flank_host()
	# By TYPE, not by node name: the chart is added without an explicit name, so a name test would be
	# asserting against Godot's default-naming rules rather than against the chart being there.
	var charted: bool = host != null and _find_chart(host) != null
	_assert_band_panel("%s: this band %s a food-outlook chart, so it is the %s split" % [
		state_name, "has" if want else "has no",
		"larder | people" if want else "vitals + PEOPLE | WORKFORCE"], charted == want)

## The first `FoodOutlookChart` under a node, or null.
func _find_chart(node: Node) -> FoodOutlookChart:
	if node is FoodOutlookChart:
		return node
	for child in node.get_children():
		var found := _find_chart(child)
		if found != null:
			return found
	return null

## Each band-flank column's content extent, deepest-first-child walk, in layout order. A ONE-column
## flank has no split row, so its single column is the zone host itself — which is what makes the fill
## claim above apply unchanged to both layouts.
func _band_flank_column_extents() -> Array[float]:
	var extents: Array[float] = []
	var host := _band_flank_host()
	if host == null:
		return extents
	var row := _find_named(host, "BandZoneColumns")
	if row == null:
		extents.append(_zone_content_extent(host, host))
		return extents
	for child in row.get_children():
		if child is Control:
			extents.append(_zone_content_extent(child, child))
	return extents

## The band flank's box height — the room ONE of its columns has.
func _band_flank_box_height() -> float:
	var host := _band_flank_host()
	return 0.0 if host == null else (host as Control).size.y

## The live band-zone host, whichever shell is up.
##
## **IT PREFERS A HOST THAT HAS CONTENT IN IT, and the narrow shell is why.** Both names in
## `BAND_ZONE_HOST_NAMES` are present in the tree at once there — the wide shell's `Zone_band` sits
## empty beside the single `NarrowZoneHost` the active tab's zone was reparented into — so a
## first-name-match answered the EMPTY one, and any claim made through it (the chart's presence, the
## flank's fill) reported an absence that is a fact about the harness rather than about the panel.
func _band_flank_host() -> Control:
	var fallback: Control = null
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		if not BAND_ZONE_HOST_NAMES.has(String(host.name)):
			continue
		if fallback == null:
			fallback = host
		for child in host.get_children():
			if child is Control and (child as Control).visible:
				return host
	return fallback

## GUARD: the flank laid out across the number of columns the panel affords — asserted on the RENDERED
## tree, not just on the panel's own answer, since a count nothing consumed is a count that did nothing.
func _assert_band_columns(state_name: String, want: int) -> void:
	var afforded: int = _panel.band_zone_columns()
	_assert_band_panel("%s: the span affords %d band column(s) (panel says %d)"
		% [state_name, want, afforded], afforded == want)
	var row := _find_named(_panel, "BandZoneColumns")
	var built: int = 0 if row == null else row.get_child_count()
	# One column is the FLAT build and must stay so — the split container is the thing that must not
	# appear there, which is why this is an equality on the built count and not a `>= 1`.
	_assert_band_panel("%s: …and the zone was BUILT with %d (%s)"
		% [state_name, want, "flat, no split row" if row == null else "%d split columns" % built],
		built == want if want > 1 else row == null)

## GUARD: **THE COUNT IS GEOMETRIC — content cannot move it, and cannot move the reservation.**
## The flicker invariant, and the one this whole change could have broken: a count that grew with the
## roster would make the strip's height a function of the snapshot, re-emitting `reservation_changed`
## into `MapView.set_reserved_inset` on every turn. Drives real content changes at a FIXED span — a
## band with a different roster and a different optional-vitals set — and requires both numbers to sit
## still.
## **AND THE RESERVATION CLAIM IS MADE ON THE PUBLISHED SIZE, by CONSUMING `reservation_changed`.**
## `Main` does not poll this panel — it stores what the signal carried and fans that to
## `MapView.set_reserved_inset` — so the published number is the one that invalidates the map cache,
## and a claim phrased as `current_reservation_size()` re-derives the very number that was never
## published and passes with the defect in (the rule `_assert_reservation_matches_drawn` records).
##
## It matters most since the band zone learned to SCROLL: the stack inside it can now be any height at
## all, so "the strip does not move when the content does" stopped being true for free.
func _assert_band_columns_ignore_content(state_name: String) -> void:
	var published: Array[float] = []
	var probe := func(_edge: int, size: float) -> void: published.append(size)
	_panel.reservation_changed.connect(probe)
	var before_columns: int = _panel.band_zone_columns()
	var before_reservation: float = _panel.current_reservation_size()
	# CONTENT-HEAVY, twice over and in both zones: the band carrying every optional vitals row (the
	# tallest band flank this harness can stage) and then the 34-source band (the busiest work board).
	_push_bands([_vitals_worst_case_band_fixture()])
	await _settle()
	_push_bands([_many_sources_band_fixture()])
	await _settle()
	var after_columns: int = _panel.band_zone_columns()
	var after_reservation: float = _panel.current_reservation_size()
	var content_publications: int = published.size()
	# THE NEGATIVE CONTROL, on the same live wire: a GEOMETRY change must publish, or "nothing was
	# published" is a claim about a signal nobody was listening to. Collapse is the cheapest one that
	# cannot be confused with content — it moves the strip to `COLLAPSED_SIZE` whatever the band holds.
	published.clear()
	_panel.set_collapsed(true)
	await _settle()
	var geometry_publications: int = published.size()
	_panel.set_collapsed(false)
	await _settle()
	_panel.reservation_changed.disconnect(probe)
	# The quiet reference band back, uncollapsed, before anything below reads either.
	_push_bands([_band_fixture()])
	await _settle()
	_assert_band_panel("%s: a content change does not move the column count (%d → %d)"
		% [state_name, before_columns, after_columns], before_columns == after_columns)
	_assert_band_panel("%s: …nor the reservation (%.0f → %.0f) — the flicker invariant"
		% [state_name, before_reservation, after_reservation],
		is_equal_approx(before_reservation, after_reservation))
	_assert_band_panel("%s: …and the panel PUBLISHED nothing across the swap (%d emissions) — the size Main fans to MapView never moved"
		% [state_name, content_publications], content_publications == 0)
	_assert_band_panel("%s: …while a real geometry change still publishes (%d emissions on collapse) — the probe is live"
		% [state_name, geometry_publications], geometry_publications > 0)

## GUARD: the layout reaches a FIXED POINT, and does so inside a bound.
##
## The hazard is a loop with three legs: the afforded count reads `_available_card_span()`, which reads
## the lateral bounds and the rail's span; the strip's height is published and fans out through `Main`;
## and on a bottom dock the HUD is inset by that height. `Hud.lateral_column_widths()` is
## `max(authored, live)` and the authored floor is what should dominate — **but that is an argument,
## and this is the measurement.** An oscillation would otherwise show up as a panel that flickers
## between two layouts on a resize, with every other assertion green on whichever frame was captured.
func _assert_band_columns_converge(state_name: String) -> void:
	var counts: Array[int] = []
	var sizes: Array[float] = []
	for _pass in range(BAND_COLUMNS_SETTLE_PASSES):
		await _settle()
		counts.append(_panel.band_zone_columns())
		sizes.append(_panel.current_reservation_size())
	var stable := true
	for i in range(1, counts.size()):
		if counts[i] != counts[0] or not is_equal_approx(sizes[i], sizes[0]):
			stable = false
	_assert_band_panel("%s: the layout is a fixed point over %d settles (columns %s, reservation %s)"
		% [state_name, BAND_COLUMNS_SETTLE_PASSES, str(counts), str(sizes)], stable)

## First descendant with this exact name, or null. `_find_zone_hosts`' sibling for a single node.
func _find_named(node: Node, want: String) -> Node:
	if String(node.name) == want:
		return node
	for child in node.get_children():
		var found := _find_named(child, want)
		if found != null:
			return found
	return null

## The canvas the republish claim is made on, at `ui_scale` 1.0. **DERIVED**: a TOP dock's card span is
## the canvas less the HUD's two authored columns (360 + 344 = 704), and the narrow shell starts below
## `wide_shell_min_width()` (1190) — so a canvas under 1894 flips when `Main` pushes those bounds and
## stands in the wide shell without them. 1400 sits well clear of both edges, where a few pixels of
## chrome drift cannot move the probe into the shell it is not testing.
const REPUBLISH_PROBE_WIDTH := 1400

## GUARD: **WHAT THE PANEL DRAWS IS WHAT THE PANEL PUBLISHED**, after a DECLARED input has moved the
## shell.
##
## The reserved size is the whole of where the event dock's bar starts (`Main._reservations` →
## `_update_event_dock_edge_offset`), and `Main` does not poll it — it stores what
## `reservation_changed` carried. `set_lateral_bounds` and `set_rail_width` relayout WITHOUT emitting,
## and both feed `_available_card_span()` → the shell → (since the strip's cross axis carries the
## active shell's chrome) the reserved size itself. So a stale publication is a bar drawn through the
## card, and neither is visible in a frame.
##
## **IT IS ASSERTED AT `ui_scale` 1.0, ON A PINNED CANVAS**, because that is where the mechanism lives:
## the flip is a WINDOW-SIZE property and the scale is only one way a player reaches it. `ui_preview`
## cannot make this claim — it pins the window but not `content_scale_size`, so its canvas floors at
## the 1920 base and the wide shell holds — which is why the consequence (the bar's own rect) is
## asserted there at 1.35 and the cause is asserted here.
##
## **The invariant is stated as published-vs-drawn rather than as a call count**, deliberately: the
## HUD's own reflow listener re-pushes the bounds when the reservation moves, so the emission ORDER is
## a settling loop and any assertion about who emitted what, when, would be pinning that loop's shape
## rather than the property that matters.
func _assert_declared_input_republishes() -> void:
	await _pin_canvas(Vector2i(REPUBLISH_PROBE_WIDTH, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_TOP)
	# The per-snapshot push (`Main._update_band_panel_lateral_bounds`). The harness's own reservation
	# listener re-pushes these too, so what settles is the loop's fixed point — which is the whole
	# point: the invariant has to hold there, not at some instant inside it.
	var columns: Vector2 = _hud.lateral_column_widths()
	_panel.set_lateral_bounds(columns.x, columns.y)
	await _settle()
	var drawn: float = _panel._root.get_global_rect().size.y

	# PRECONDITION, stated as a COMPUTATION rather than by holding the panel unbound for a frame: the
	# bounds cannot be cleared any more without the listener above pushing them straight back (that IS
	# `Main`'s behaviour, not a harness artifact), so the claim is that the bounds are what put this
	# panel in the narrow shell — the strip WITHOUT them clears the threshold and the span WITH them
	# does not. On a canvas where the shell never moves, every claim below holds for free, which is the
	# state ~every other canvas in this harness is in and precisely why this defect had none.
	var unbound_span: float = _panel._panel_width_extent() - _panel._rail_span()
	var bounded_span: float = _panel._available_card_span()
	_assert_band_panel("republish: the bounds are what put this panel in the narrow shell (span %.0f unbound / %.0f bound, threshold %.0f)"
		% [unbound_span, bounded_span, _panel.wide_shell_min_width()],
		unbound_span >= _panel.wide_shell_min_width()
			and bounded_span < _panel.wide_shell_min_width())
	_assert_band_panel("republish: the size the panel PUBLISHED is the size it draws (%.0f published, %.0f drawn)"
		% [_panel._published_reservation, drawn],
		is_equal_approx(_panel._published_reservation, drawn))
	_assert_band_panel("republish: …and it is what `current_reservation_size()` answers, so Main and the panel agree (%.0f)"
		% _panel.current_reservation_size(),
		is_equal_approx(_panel._published_reservation, _panel.current_reservation_size()))

## Push a scale the way the Options slider does — through `ClientSettings.changed`, so `UiScaler`
## applies it on its own real subscription. **The MEMBER is assigned, never `set_ui_scale`**: the
## setter `_save()`s, and this harness has no `ClientSettings` path override, so it would write the
## developer's own `user://client_settings.cfg` (the `_ready` prologue's rule, from `map_preview`).
func _apply_ui_scale(value: float) -> void:
	ClientSettings.ui_scale = value
	ClientSettings.changed.emit()

## GUARD: the panel never exceeds its edge's share of the window — its strip AND its card both lie
## inside the viewport.
##
## **BOTH RECTS, because they are set by different mechanisms and only one of them is anchored.**
## `_root` is anchored to the edge, so it is contained by construction; the CARD is a
## `PanelContainer`, i.e. a real Container, and a `Control` clamps its own size UP to its combined
## minimum — so a card whose content demanded more than the strip would draw past it while every
## anchor stayed correct (the `panel-framework.md` "a card fitted too short does not fail, it lies"
## shape). A frame cannot tell the two apart: the overflow is off-canvas.
func _assert_panel_within_window(state_name: String) -> void:
	var window := Rect2(Vector2.ZERO, get_viewport().get_visible_rect().size)
	var failures: Array[String] = []
	for pair in [["strip", _panel._root], ["card", _panel._panel]]:
		var name_part: String = pair[0]
		var control: Control = pair[1]
		var rect: Rect2 = control.get_global_rect()
		if rect.position.x < -SCALE_BOUNDS_TOLERANCE or rect.position.y < -SCALE_BOUNDS_TOLERANCE \
				or rect.end.x > window.end.x + SCALE_BOUNDS_TOLERANCE \
				or rect.end.y > window.end.y + SCALE_BOUNDS_TOLERANCE:
			failures.append("the %s is %s, outside the %s window" % [
				name_part, str(rect), str(window.size)])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s strip %s and card %s inside the %s window" % [
			state_name, str(_panel._root.get_global_rect().size),
			str(_panel._panel.get_global_rect().size), str(window.size)])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

## GUARD: the strip's cross-axis size is CONTENT-INDEPENDENT, still — the invariant that keeps
## `current_reservation_size()` (and therefore MapView's inset and its cache) constant while the
## player edits the band.
##
## It is asserted HERE because the narrow shell's strip now includes its own tab bar's measured
## height, and a tab bar carries BADGES: a badge tall enough to grow the bar would make the
## reservation a function of the snapshot, re-emitting `reservation_changed` every turn — which is
## precisely the map flicker `set_zones` exists to remove, re-entered through a new door.
func _assert_badge_cannot_move_the_reservation(state_name: String) -> void:
	var before: float = _panel.current_reservation_size()
	_panel.set_tab_badge(BandCityPanel.ZONE_WORK, SCALE_BADGE_TEXT, true)
	await _settle()
	var with_badge: float = _panel.current_reservation_size()
	_panel.set_tab_badge(BandCityPanel.ZONE_WORK, "", false)
	await _settle()
	_assert_band_panel("%s: a tab badge does not move the reservation (%.1f → %.1f)" % [
		state_name, before, with_badge], is_equal_approx(before, with_badge))

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
		# **A MISSING FIXTURE HOST IS A FAILURE, NOT AN ADVISORY.** Without the container there is no minimap, the chrome column
		# collapses to the zoom rail's ~80px, and every dock-row assertion below — the rail's span, the
		# parked cluster's fit, the card's centring — is measured against a rail the game does not have.
		# The frames render fine and prove nothing, which is the failure this harness exists to refuse.
		_fail("no MinimapContainer in the HUD — the dock-row states would measure a rail the game never has")
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
		_fail("%s — %s" % [state_name, failure])

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
## GUARD: **THE TWO-COLUMN STRIP STILL CLEARS THE PARKED CHROME STACK, AND BY HOW MUCH.**
##
## `DockRowController.parks_for` is `reserved >= _required_height()`, and the two-column budget
## (`BandCityPanel.PANEL_HEIGHT_WIDE_TWO_COLUMN`) is the first thing in this panel's history to move the
## left-hand side of that comparison DOWN. Below it the gate declines, `BottomBar` keeps the minimap and
## the turn orb, and issue #324's whole dock-row reflow silently un-does itself — the exact way an
## earlier attempt at this budget (230) failed.
##
## **AND IT WOULD NOT MERELY UN-DO — IT WOULD OSCILLATE.** A declined park restores the HUD's lateral
## bounds, which costs `_available_card_span()` 704px, which drops the flank to ONE column, which
## restores the 360 budget, which parks again. `_assert_band_columns_converge` is what would catch the
## loop; this is what stops it being reachable.
##
## Three claims, and the third is what makes the first two mean something: the strip clears the
## requirement, the chrome is REALLY parked (a margin computed on a state where the gate never applied
## is arithmetic about nothing), and the flank really has `expected_columns` (the budget is picked off
## that count, so a margin quoted without it is a number whose budget nobody can reconstruct). The
## margin is PRINTED as well as asserted — a comfortable clearance and a one-pixel squeak are the same
## green line otherwise.
##
## **BOTH BUDGETS ARE ASSERTED, at their own canvases, and that pairing is what the fork move forced.**
## The two-column budget (335) is the tighter one and used to ride the 1920 state; dropping the trailing
## bound on a bottom dock moved the fork to 1871, so 1920 now keeps the HUD's strip, pays the leading
## column and comes back to ONE column and the 360 budget. The 335 claim therefore moved to
## `band_panel_band_columns_two`'s canvas, which is where two columns now live, and 1920 keeps the claim
## at the budget it actually has. Asserting only one of them would leave the other budget's park gate
## unmeasured.
func _assert_parked_chrome_margin(state_name: String, expected_columns: int) -> void:
	var reserved: float = _panel.current_reservation_size()
	var required: float = _hud._dockrow._required_height()
	var columns: int = _panel.band_zone_columns()
	_assert_band_panel("%s: the flank really has %d column(s), so this margin is that budget's (%d)"
		% [state_name, expected_columns, columns], columns == expected_columns)
	_assert_band_panel("%s: …and the chrome is really parked, so the gate this margin is about actually applied"
		% state_name, not _hud.bottom_bar.visible)
	_assert_band_panel("%s: the %d-column strip clears the parked chrome stack — %.0f of %.0f needed, margin %.0fpx"
		% [state_name, expected_columns, reserved, required, reserved - required], reserved >= required)

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
		_fail("%s — %s" % [state_name, failure])

## PRECONDITION for the two assertions below: the strip really is WIDER than the card wants to be, so
## the island geometry they judge has slack to get wrong. Without it both would pass vacuously on a
## window the card fills anyway, where "centred" and "flush right" are true for free.
func _assert_card_is_narrower_than_strip(state_name: String) -> void:
	var card := _panel._panel.get_global_rect().size.x
	var strip := _panel._root.get_global_rect().size.x
	var slack: float = strip - card - _panel._rail_span()
	if slack <= ZONE_BOUNDS_TOLERANCE:
		_fail("%s — the card (%.0fpx) fills its %.0fpx strip, so the island assertions below prove nothing" % [
			state_name, card, strip])
		return
	print("band_panel_preview: assert OK — %s the card is an island (%.0fpx card + %.0fpx chrome span in a %.0fpx strip, %.0fpx of open map)" % [
		state_name, card, _panel._rail_span(), strip, slack])

## GUARD: the chrome cluster is FLUSH RIGHT against the trailing edge of the row the panel may use
## (issue #377).
##
## Measured against the strip rather than the card, and that changed with the islands: the rail used to
## be the last cell of `_card_row`, so the only sensible claim was "inside its own card's trailing
## inset". It is a sibling of the card now, anchored to `_root`, so the claim is the stronger one — it
## sits at the edge of the screen, with the card floating well to its left.
##
## **THE CLAIM IS THE WINDOW'S OWN RIGHT EDGE, NOT "the row's end less the HUD column".** It was the
## latter for one commit — the rail was pinned at `-(_bound_trailing + rail_width)` to hold it off the
## right-hand HUD column — and that inset the parked minimap and turn orb by the column's whole width,
## leaving a visible band of dead map between the chrome and the screen on every bottom dock past the
## fork. The clearance runs the other way now (`Hud.set_right_column_bottom_clearance`), so the rail is
## flush again and this asserts it against the VIEWPORT rather than against a bound that would move
## with it — a claim phrased in the panel's own terms goes green whichever way the bound is applied,
## which is exactly how the inset shipped unnoticed.
##
## It reports `_bound_trailing` beside the verdict, so a frame's numbers still say which HUD column was
## live when it was taken.
func _assert_rail_is_right_justified(state_name: String) -> void:
	var rail_right := _panel._rail.get_global_rect().end.x
	var window_right: float = get_viewport().get_visible_rect().end.x
	var gap: float = window_right - rail_right
	if absf(gap) > ZONE_BOUNDS_TOLERANCE:
		_fail("%s — the chrome cluster ends at %.0f but the window ends at %.0f — %.0fpx short (strip end %.0f, HUD column %.0f)" % [
			state_name, rail_right, window_right, gap,
			_panel._root.get_global_rect().end.x, _panel._bound_trailing])
		return
	print("band_panel_preview: assert OK — %s the chrome cluster is flush to the window's right edge (%.0f, with a %.0fpx HUD column live)" % [
		state_name, window_right, _panel._bound_trailing])

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
		_fail("%s — %s" % [state_name, failure])

## GUARD: a TOP-docked card is drawn over NEITHER HUD column (issue #377).
##
## The top dock is the one edge where the HUD keeps its strip — its right-hand column belongs BESIDE
## the card, not pushed under the map — so it is also the one edge where the card can be drawn over
## something. That column was the top-bar readouts AND the dock beneath them; since issue #450 retired
## the readouts it is the dock alone, which now begins at the top of the screen. The claim is made as rect non-overlap against the live regions rather than as "the
## bound was applied", because a bound that is set and then ignored reads identically to one that works.
##
## **It takes a negative control first, on the same two live rects**: with the bounds cleared the card
## genuinely DOES overlap, so a pass cannot be satisfied by two rects that happen never to meet — which
## is what a sparse band would give for free, and exactly how the half-fix looked complete.
func _assert_card_clears_hud_columns(state_name: String) -> void:  # coroutine: it re-renders twice
	var card := _panel._panel.get_global_rect()
	var columns := {
		"the left dock": _hud.left_dock_region.get_global_rect(),
		# The RIGHT DOCK, not the retired top-bar readout block (issue #450): with the top bar gone the
		# dock starts at y = 0, so it is the region a TOP-docked card now shares a vertical band with —
		# and the one whose live rect this bound is computed from.
		"the right dock": _hud.right_dock_region.get_global_rect(),
	}
	# **NEGATIVE CONTROL: unbound, this band's card must actually reach at least one of them — and it
	# has to RE-RENDER to find out.** The card's width is built up from a column count the CONTROLLER
	# declares (`set_work_columns`), and `_affordable_work_columns` caps that count against the bounded
	# span — so clearing the bounds and re-reading the rect answers with the count granted UNDER the
	# bounds and the card barely moves. Measured after the top-bar readouts were retired (issue #450):
	# the 34-source band was granted ONE column inside its 1216px bound, so the "unbound" card was the
	# same content-sized 1190 and cleared both columns, and the control correctly refused to prove
	# anything. Re-rendering re-grants the count against the full 1920, which is what the bound is
	# actually holding back.
	#
	# **THE HARNESS'S OWN RESERVATION LISTENER HAS TO COME OFF THE WIRE FOR IT TOO.** Clearing the
	# bounds can flip the shell, which moves the reserved size, which is published — and this harness
	# restates `Main._update_band_panel_lateral_bounds` on that signal, so the bounds are pushed
	# straight back and the "unbound" rect measured below is the BOUNDED one (measured: a 1141px card,
	# which is exactly the bounded span, silently failing the control instead of the claim).
	_panel.reservation_changed.disconnect(_reservation_listener)
	_panel.set_lateral_bounds(0.0, 0.0)
	_hud._bandpanel.rerender()
	await _settle()
	var unbound := _panel._panel.get_global_rect()
	var would_collide := false
	for rect_variant in columns.values():
		if unbound.intersects(rect_variant):
			would_collide = true
	var live: Vector2 = _hud.lateral_column_widths()
	_panel.reservation_changed.connect(_reservation_listener)
	_hud._bandpanel.rerender()
	await _settle()
	# The columns are re-read after the restore: the card moved twice and these rects are what the
	# CLAIM below is made against, not the ones the control was taken with.
	columns["the left dock"] = _hud.left_dock_region.get_global_rect()
	columns["the right dock"] = _hud.right_dock_region.get_global_rect()
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
		_fail("%s — %s" % [state_name, failure])

## GUARD: the CARD sits centred in the room the chrome cluster and the HUD columns leave.
##
## Fitting does not imply centring (the `_assert_parked_chrome_fits` lesson on the other axis): a card
## packed hard against the leading edge is entirely inside its strip and reads as a panel that ignores
## the right half of an ultrawide. It is the CARD being measured now, not its content column — the
## column simply fills the card since the card itself became the thing that narrows.
##
## **CENTRED IN THE GAP, not on the screen** — `_position_card_and_rail`'s own words. The leading bound
## comes off before the margins are compared, because a card centred on the screen with a full-height
## HUD column beside it would be sitting under one of them.
##
## **THE TRAILING EDGE OF THAT GAP IS MEASURED OFF WHAT ACTUALLY STANDS THERE, NEVER OFF
## `_bound_trailing`.** It was the bound for one commit, and phrasing the claim in the panel's own terms
## is what let a card sitting 210px off centre pass: the assertion subtracted the very bound that had
## displaced it, so both sides moved together and the margins matched. On a BOTTOM dock the thing at the
## trailing end is the PARKED CHROME — flush to the screen, one gutter clear of the card — and the
## right-hand HUD column is deliberately NOT in the sum, because it no longer reaches that strip
## (`Hud.set_right_column_bottom_clearance`). On a TOP dock there is no rail and the readout block really
## is the trailing occupant, so the bound is right there and is what is used.
func _assert_card_is_centred(state_name: String) -> void:
	var card := _panel._panel.get_global_rect()
	var strip := _panel._root.get_global_rect()
	var trail_edge: float
	var trail_what: String
	if _panel._rail_width() > 0.0:
		trail_edge = _panel._rail.get_global_rect().position.x - BandCityPanel.RAIL_SEPARATOR_SPAN
		trail_what = "the parked chrome"
	else:
		trail_edge = strip.end.x - _panel._bound_trailing
		trail_what = "a %.0fpx HUD column" % _panel._bound_trailing
	var lead_margin: float = card.position.x - (strip.position.x + _panel._bound_leading)
	var trail_margin: float = trail_edge - card.end.x
	if absf(lead_margin - trail_margin) > ZONE_BOUNDS_TOLERANCE:
		_fail("%s — the card is not centred in its gap: %.0fpx of margin leading (past a %.0fpx HUD column) and %.0fpx trailing (up to %s at %.0f)" % [
			state_name, lead_margin, _panel._bound_leading, trail_margin, trail_what, trail_edge])
		return
	print("band_panel_preview: assert OK — %s the card is centred in its gap (%.0fpx either side, between a %.0fpx HUD column and %s)" % [
		state_name, lead_margin, _panel._bound_leading, trail_what])

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
	# THE CLAIM: both gaps — leading (the row's start → the card) and trailing (the card → the chrome
	# cluster). **The row, not the raw strip**: where the HUD keeps its columns over this strip
	# (`Main.band_dock_overlays_hud` on a wide BOTTOM dock), the outer `_bound_leading` /
	# `_bound_trailing` bands are the HUD's own furniture, not open map, and what this guard is about is
	# whether the PANEL eats clicks aimed past it. Both bounds are 0 wherever the HUD yielded, which is
	# every state that ran this before.
	var row_start: float = strip.position.x + _panel._bound_leading
	var row_end: float = strip.end.x - _panel._bound_trailing
	var gaps := {
		"the open strip LEADING the card": Rect2(
			Vector2(row_start, strip.position.y), Vector2(card.position.x - row_start, strip.size.y)),
		"the open strip TRAILING the card": Rect2(
			Vector2(card.end.x, strip.position.y),
			Vector2(row_end - rail_span - card.end.x, strip.size.y)),
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
	# The chrome cluster is probed on its OWN rect, never re-derived from the strip's trailing edge: the
	# rail sits inboard of `_bound_trailing` when the HUD keeps its right-hand column, so a re-derived
	# rect would ring a band of open map and "the island eats its clicks" would fail on the harness's
	# arithmetic rather than on the panel.
	var islands := {
		"the card's own chrome ring": _rect_ring_probe_points(card),
		"the chrome cluster": _rect_ring_probe_points(_panel._rail.get_global_rect()),
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
			state_name, card.position.x - row_start, row_end - rail_span - card.end.x])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

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
		_fail("%s — %s" % [state_name, failure])

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
		_fail("%s — %s" % [state_name, failure])

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
		_fail("detail panel MISSING '%s' — marker path dropped the field. Got: %s" % [
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
		_fail("%s expected '%s' but got '%s'" % [label, want, got])

## The parties zone's host in the WIDE shell — the one the worst-case party state renders into. (The
## NARROW shell swaps in `NarrowZoneHost`, which is why the band zone's own extent report carries a
## two-name list; this state is bottom-docked, so it is always this one.)
const PARTIES_ZONE_HOST_NAME := "Zone_parties"

## GUARD: the worst-case party's strip must really be rendering EVERY optional line. The extent this
## state reports is only a worst case while it is — a strip that quietly stopped emitting a line is
## SHORTER, so it fits its box and both the bounds assertion and `_assert_zone_content_fits` go green
## on a state that has stopped measuring what it exists to measure. The same trap
## `band_panel_vitals_worst_case` carries one zone over.
##
## Asserted on the RENDER, not on `expedition_summary_lines`' return value: the producer answering
## seven lines says nothing about the strip building seven Labels out of them.
func _assert_worst_case_party_lines() -> void:
	var lines := _parties_inspector_lines()
	_assert_band_panel("worst-case party — the strip renders all %d detail lines (got %d: %s)" % [
		WORST_CASE_DETAIL_LINES, lines.size(), ", ".join(lines)],
		lines.size() == WORST_CASE_DETAIL_LINES)
	# …and that each optional line is the one it is supposed to be, by the LONGEST form of its own
	# gate — a strip rendering seven lines of which two are the wrong ones would pass a bare count.
	# Each needle is composed from the fixture's own numbers or from the producer's own vocabulary, so
	# a re-tune of either cannot leave the claim quietly matching nothing.
	# The TARGET is read back out of the live herd list rather than restated, so the needles and the
	# fixture cannot drift: a herd that migrated, was renamed or dropped out fails here instead of
	# quietly matching nothing.
	var target: Dictionary = _hud._band_labor.find_world_herd(WORST_CASE_TARGET_HERD_ID)
	_assert_band_panel("worst-case party — its target herd is in the telemetry (else the Target row" \
		+ " carries no position and this is not the worst case)", not target.is_empty())
	var joined := "\n".join(lines)
	for needle in [
		# The Target row's LIVE position, which needs the herd to still be in `_world_herds`.
		"(%d, %d)" % [int(target.get("x", -1)), int(target.get("y", -1))],
		# The Orders row, at the floor this party was launched with rather than at the default.
		HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(WORST_CASE_FLOOR),
		# The Carried row at its ceiling, hence the FULL badge.
		BandDetailLines.HUNT_FULL_BADGE,
		# The recurring delivery's own suffix.
		DetailFormat.EXPEDITION_RECURRING_GLYPH,
		# The sim's answer for which stop ends the trip.
		SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_PACK_FULL],
	]:
		_assert_band_panel("worst-case party — the strip states `%s`" % needle, joined.contains(needle))

## The detail-line texts the parties inspector strip is rendering right now, in order.
## `_build_parties_inspector` gives the strip ONE `PanelContainer` holding a column of: a head HBox, one
## Label per detail line, and a links HBox — so the column's direct Label children ARE the detail lines.
## Scoped to the parties zone rather than to the panel, since the WORK inspector is a `PanelContainer`
## of the same shape.
func _parties_inspector_lines() -> Array[String]:
	var lines: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		if String(host.name) != PARTIES_ZONE_HOST_NAME:
			continue
		var strip := _find_first_of_type(host, "PanelContainer")
		if strip == null:
			return lines
		for column in strip.get_children():
			for child in column.get_children():
				if child is Label:
					lines.append((child as Label).text)
	return lines

## The first descendant of `node` whose class is `type_name`, depth-first, or `null`.
func _find_first_of_type(node: Node, type_name: String) -> Node:
	for child in node.get_children():
		if child.is_class(type_name):
			return child
		var found := _find_first_of_type(child, type_name)
		if found != null:
			return found
	return null

## **THE VERB AND THE CEREMONY FOLLOW THE SIM, AND THE CLAIM IS THE PAIR.** Recalling a party still
## standing in its home band's camp with no map report owed CANCELS it on the spot
## (`core_sim::cancel_party_standing_in_camp`); one in the field walks home over turns. So the single
## recall path is two different orders and the control must say which: `Cancel`, acting straight off
## the press, against `Recall` behind the confirm that names the trip being abandoned.
##
## **A rule that showed one verb everywhere would satisfy either half alone**, which is why both are
## driven here, through the REAL row builder and the REAL `pressed` handler rather than by calling
## `confirm_recall_expedition` directly — the row's own ✕ is where a caller could drift.
##
## The two fixtures differ in POSITION and nothing else, so what is being judged is the predicate.
## Sabotage-verified in both directions: forcing the predicate true makes the FIELD party offer Cancel
## and skip the dialog; forcing it false makes the camped one raise one.
func _assert_row_recall_confirms() -> void:
	# The bands must be the roster the predicate resolves the home band out of — `party_cancels_in_camp`
	# reads `player_band_by_entity(home_band_entity)`, so a camped party whose band is not listed would
	# answer "in the field" for the wrong reason.
	_push_bands([_band_fixture(), _in_field_expedition_fixture(), _in_camp_expedition_fixture()])
	_assert_recall_press("field party", _in_field_expedition_fixture(),
		HudComposeVocab.PARTY_RECALL_VERB, HudComposeVocab.PARTY_RECALL_TOOLTIP, true)
	_assert_recall_press("camped party", _in_camp_expedition_fixture(),
		HudComposeVocab.PARTY_CANCEL_VERB, HudComposeVocab.PARTY_CANCEL_TOOLTIP, false)
	# THE TERM THAT IS NOT ABOUT POSITION: a party standing in camp that still owes a map report is
	# walked home by the sim, so the client must say `Recall` and ask. Without it, every claim above is
	# satisfied by a client testing "is it on the band's tile".
	_push_bands([_band_fixture(), _in_camp_with_report_owed_fixture()])
	_assert_recall_press("camped party owing a report", _in_camp_with_report_owed_fixture(),
		HudComposeVocab.PARTY_RECALL_VERB, HudComposeVocab.PARTY_RECALL_TOOLTIP, true)
	_push_bands([_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture(),
		_lost_hunt_expedition_fixture()])

## One half of the pair above: check the verb `BandPanelController.recall_verb` hands the parties
## inspector link and the Occupants drawer's button, build `exp`'s real party row and check its ✕'s
## tooltip, then PRESS that ✕ and require the ceremony `wants_confirm` names — a dialog and no emit,
## or an emit and no dialog.
func _assert_recall_press(label: String, exp: Dictionary, want_verb: String, want_tooltip: String,
		wants_confirm: bool) -> void:
	_assert_band_panel("recall verb — %s reads '%s'" % [label, want_verb],
		_hud._bandpanel.recall_verb(exp) == want_verb)
	var row: HBoxContainer = _hud._bandpanel._build_party_row(exp)
	var recall: Button = row.get_child(row.get_child_count() - 1)   # ✕ is the row's last child
	_assert_band_panel("recall tooltip — %s reads '%s' (got '%s')" % [label, want_tooltip, recall.tooltip_text],
		recall.tooltip_text == want_tooltip)
	var fired := [false]
	var sink := func(_payload: Dictionary) -> void: fired[0] = true
	_hud.recall_expedition_requested.connect(sink)
	# **COUNT THE DIALOGS, DO NOT LOOK FOR ONE.** `_dismiss_dialogs` frees with `queue_free`, which is
	# deferred, so the PREVIOUS half of this pair is still a child of the HUD when this half runs — a
	# presence test therefore reports the camped party as having confirmed. (Measured: it did.)
	var before := _hud_dialog_count()
	recall.pressed.emit()
	var dialog_shown := _hud_dialog_count() > before
	_hud.recall_expedition_requested.disconnect(sink)
	if wants_confirm:
		_assert_band_panel("recall ceremony — %s confirms first, no immediate emit (dialog=%s, emitted=%s)" % [
			label, dialog_shown, fired[0]], dialog_shown and not fired[0])
	else:
		_assert_band_panel("recall ceremony — %s acts on the press, no dialog (dialog=%s, emitted=%s)" % [
			label, dialog_shown, fired[0]], fired[0] and not dialog_shown)
	_dismiss_dialogs()
	row.queue_free()

## **"START A LIFE HERE" IS OFFERED ON A PHASE, AND THE CLAIM IS THE PAIR** (issue #510). Both frames
## render the SAME roster — one arrived scout, one party still hunting — so what moves between them is
## which inspector strip is open, and a control that rendered unconditionally fails the second.
##
## `strip_offers` says whether the OPEN strip is the arrived party's. The ROW control is asserted in
## BOTH states and must be found exactly once either way: it belongs to the scout's row, which is on
## screen in both, and a row control keyed on the open strip rather than on the party would vanish.
func _assert_settle_affordance(state_name: String, strip_offers: bool) -> void:
	var rows := _settle_faces_in_panel(HudComposeVocab.PARTY_SETTLE_VERB)
	_assert_band_panel("%s — exactly one party row offers `%s` (found %d)" % [
		state_name, HudComposeVocab.PARTY_SETTLE_VERB, rows], rows == 1)
	var links := _settle_faces_in_panel(HudComposeVocab.PARTY_SETTLE_ACTION)
	var want := 1 if strip_offers else 0
	_assert_band_panel("%s — the open strip offers `%s` %d time(s), found %d" % [
		state_name, HudComposeVocab.PARTY_SETTLE_ACTION, want, links], links == want)

## Controls in the PANEL whose face is exactly `face`. Both the row's button and the strip's inline
## link are `Button`s, so one walk answers for both — and matching the face is right HERE because the
## face is what the pair above is about.
##
## **SCOPED TO THE PANEL, NOT TO `Zone_parties`**, and that is not laziness: the NARROW shell (which
## these two states use, a side dock being where a player reads the parties list) reparents the
## active zone into `NarrowZoneHost`, so a `Zone_parties`-scoped walk finds nothing at all there and
## every claim above passes as `0 == 0`. Measured — the first cut of this helper did exactly that.
func _settle_faces_in_panel(face: String) -> int:
	return _count_buttons_with_text(_panel, face)

func _count_buttons_with_text(node: Node, face: String) -> int:
	var found := 0
	if node is Button and (node as Button).text == face:
		found += 1
	for child in node.get_children():
		found += _count_buttons_with_text(child, face)
	return found

## **A FOUNDING ALWAYS ASKS FIRST — there is no press-through branch**, unlike a recall, which acts
## straight off the press for a party still standing in camp. Driven through the REAL row builder and
## the REAL `pressed` handler, and asserted as a PAIR of readings (a dialog appeared, nothing was
## emitted), for the deferred-`queue_free` reason `_assert_recall_press` records.
##
## **IT LEAVES THE DIALOG UP.** The caller renders `band_panel_settle_confirm` off it — the founding
## prompt is the one confirm in this file whose COPY is a claim rather than chrome — and dismisses.
func _assert_settle_confirms_before_emitting() -> void:
	var exp := _awaiting_scout_expedition_fixture()
	var row: HBoxContainer = _hud._bandpanel._build_party_row(exp)
	var settle: Button = null
	for child in row.get_children():
		if child is Button and (child as Button).text == HudComposeVocab.PARTY_SETTLE_VERB:
			settle = child
	if settle == null:
		_fail("settle press — the arrived party's row built no `%s` control" % HudComposeVocab.PARTY_SETTLE_VERB)
		row.queue_free()
		return
	var emitted: Array[Dictionary] = []
	var sink := func(payload: Dictionary) -> void: emitted.append(payload)
	_hud.settle_expedition_requested.connect(sink)
	var before := _hud_dialog_count()
	settle.pressed.emit()
	var dialog_shown := _hud_dialog_count() > before
	_assert_band_panel("settle ceremony — confirms first, no immediate emit (dialog=%s, emitted=%d)" % [
		dialog_shown, emitted.size()], dialog_shown and emitted.is_empty())
	# **THE COPY IS ASSERTED BY EQUALITY, and that is the whole point of asserting it at all.** The
	# prompt shipped as two paragraphs naming the party and its tile and explaining the reachability
	# gate; a `contains` on any phrase of the surviving sentence passes on that version too, so only
	# equality can keep it from coming back.
	var prompt := _hud_confirm_dialog()
	var prompt_text := prompt.dialog_text if prompt != null else ""
	_assert_band_panel("settle prompt — one line, no coordinates, no party name (got \"%s\")" % prompt_text,
		prompt_text == HudComposeVocab.PARTY_SETTLE_CONFIRM)
	_hud.settle_expedition_requested.disconnect(sink)
	row.queue_free()

## The LIVE confirm dialog on the HUD — the last still-visible one, since `_dismiss_dialogs` frees
## with the deferred `queue_free` and a spent dialog stays a child for the rest of the frame.
func _hud_confirm_dialog() -> ConfirmationDialog:
	var found: ConfirmationDialog = null
	for child in _hud.get_children():
		if child is ConfirmationDialog and (child as ConfirmationDialog).visible:
			found = child
	return found

## Confirmation dialogs parented on the HUD right now, freed-but-not-yet-collected ones included —
## which is exactly why `_assert_recall_press` compares two readings rather than testing presence.
func _hud_dialog_count() -> int:
	var count := 0
	for child in _hud.get_children():
		if child is ConfirmationDialog:
			count += 1
	return count

## GUARD: whenever the WIDE shell is active, the work zone must be at least one readable board column
## (`ZONE_WORK_MIN_WIDTH`) — otherwise Hud's `_work_board_capacity` clamps to a single column too
## narrow for its own row labels, and the NARROW shell would have given the board strictly MORE room.
## That is the invariant a hand-picked `wide_shell_min_width()` violated across a whole band of widths,
## and the recursive zone-bounds assertion cannot catch it: a CLIPPED label still sits inside its rect.
func _assert_work_zone_readable() -> void:
	if not _panel._shell_is_wide():
		return
	var work_width := _panel.work_zone_size().x
	if work_width + ZONE_BOUNDS_TOLERANCE < BandCityPanel.ZONE_WORK_MIN_WIDTH:
		_fail("wide shell with a %.0fpx work zone — under ZONE_WORK_MIN_WIDTH (%.0f)" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])
	else:
		print("band_panel_preview: assert OK — wide shell work zone %.0fpx >= %.0f" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])

## GUARD: the two threshold-probe states exist to pin WHICH shell is chosen, so state it outright —
## a frame that silently rendered the other shell would still pass every other assertion here.
func _assert_shell_is_wide(expected: bool, state_name: String) -> void:
	var actual := _panel._shell_is_wide()
	if actual != expected:
		_fail("%s expected shell wide=%s but got %s" % [
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
		_fail("%s PEOPLE brackets sum to %d but the band holds %d (raw %s — narrowed?)" % [
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
		DetailFormat.KIT_ITEM_CONDITIONS_KEY, DetailFormat.KIT_TIER_KEY_HUNT_CARRY,
		DetailFormat.KIT_TIER_KEY_FORAGE_CARRY, SourceForecast.BAND_HUNTER_ATTACK_KEY,
	]:
		if not band.has(toe_key):
			missing.append(String(toe_key))
	# **The condition list must arrive with ROWS, not merely with a key.** An empty array is what a
	# dropped copy looks like, and `DetailFormat.band_states_kit` reads exactly that emptiness as
	# "this band states no kit" — so the Kit row would vanish rather than render wrong, which is the
	# failure a `has()` check cannot see.
	var conditions: Array = band.get(DetailFormat.KIT_ITEM_CONDITIONS_KEY, [])
	if conditions.is_empty():
		missing.append("%s (present but EMPTY)" % DetailFormat.KIT_ITEM_CONDITIONS_KEY)
	_assert_band_panel("the map-click payload carries the TOE the panel reads (missing %s)" % str(missing),
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
	# The condition of ONE named item, pulled out of the list the wire now carries. The fixture's own
	# number is the expectation, so the assertion cannot be satisfied by re-deriving it through the
	# code under test.
	var spears := DetailFormat.kit_condition(_kit_band_fixture(),
		DetailFormat.KIT_DURABILITY_KEY_SPEARS)
	var copied := DetailFormat.kit_condition(band, DetailFormat.KIT_DURABILITY_KEY_SPEARS)
	_assert_band_panel("…un-narrowed, spears reading %s against the fixture's %s"
			% [str(copied), str(spears)],
		is_equal_approx(copied, spears))
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

## **THE GEAR POPOVER STATES EVERY ITEM THE BAND CARRIES, EACH BESIDE THE TIER IT SETS** — the three
## the expanded roster added included, which is what this assertion was written for: their tiers
## reached the wire only just now, so before it a scout kit and a warrior kit had no readout at all.
##
## **PAIRING IS THE WHOLE CLAIM, so every row is asked BOTH what it must say and what it must not.**
## `kit_id` names the HUNT job's default and answers for the hunt tiers alone, so the two rows most
## likely to be mis-wired are the ones whose tiers resolve through a DIFFERENT job's default: a
## wayfinding row quoting the hunt kit and a clubs row quoting `hunter_attack` are both perfectly
## plausible-looking rows carrying another kit's number. The fixture's tiers are all distinct, so a
## swap cannot pass — the pen collects at 12.0 where the sled hauls 40.0, and the camp is defended at
## 6 where the hunt attacks at 20.
##
## Read off the popover's own RENDERED text, per line, like `ui_preview`'s kit assertions: the rows
## share a shape, so a whole-popover `contains` would be satisfied by the WRONG row.
func _assert_gear_breakdown_states_every_kit() -> void:
	var popover := _kit_popover_text()
	_assert_band_panel("the gear popover opened at all (%d chars)" % popover.length(),
		popover.contains(DetailFormat.KIT_BREAKDOWN_CLIFF_NOTE))
	var pen_role := DetailFormat.KIT_ROLE_PEN_CARRY_FORMAT % String.num(
		BandFx.KIT_PEN_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS)
	var sled_role := DetailFormat.KIT_ROLE_PEN_CARRY_FORMAT % String.num(
		BandFx.KIT_HUNT_CARRY_EQUIPPED, DetailFormat.KIT_CARRY_DECIMALS)
	var gear_line := _kit_breakdown_line(popover, DetailFormat.KIT_LABEL_HUSBANDRY_GEAR)
	_assert_band_panel("HANDLING GEAR states the PEN's collection rate (%s), never the sled's (%s) — \"%s\""
			% [pen_role, sled_role, gear_line],
		gear_line.contains(pen_role) and not gear_line.contains(sled_role))
	_assert_band_panel("…beside its own condition (%s)" % String.num(
			BandFx.KIT_CONDITION_HUSBANDRY_GEAR, DetailFormat.KIT_CONDITION_DECIMALS),
		gear_line.contains(String.num(BandFx.KIT_CONDITION_HUSBANDRY_GEAR,
			DetailFormat.KIT_CONDITION_DECIMALS)))
	# **THE VANTAGE IS TILES, and the assertion says so in both directions.** A biomass-rate format
	# string here would print `2.0`, which reads as a rate and is not one.
	var vantage_role := DetailFormat.KIT_ROLE_SCOUT_VANTAGE_FORMAT % String.num(
		BandFx.KIT_SCOUT_VANTAGE_EQUIPPED, DetailFormat.KIT_VANTAGE_DECIMALS)
	var carry_shaped_vantage := String.num(BandFx.KIT_SCOUT_VANTAGE_EQUIPPED,
		DetailFormat.KIT_CARRY_DECIMALS)
	var wayfinding_line := _kit_breakdown_line(popover, DetailFormat.KIT_LABEL_WAYFINDING)
	_assert_band_panel("WAYFINDING states a SIGHT RANGE IN TILES (%s), not a per-worker rate (%s) — \"%s\""
			% [vantage_role, carry_shaped_vantage, wayfinding_line],
		wayfinding_line.contains(vantage_role)
			and not wayfinding_line.contains(carry_shaped_vantage))
	_assert_band_panel("…beside its own condition (%s)" % String.num(
			BandFx.KIT_CONDITION_WAYFINDING, DetailFormat.KIT_CONDITION_DECIMALS),
		wayfinding_line.contains(String.num(BandFx.KIT_CONDITION_WAYFINDING,
			DetailFormat.KIT_CONDITION_DECIMALS)))
	# **CLUBS READ `warrior_attack`, NOT the hunt kit's `hunter_attack`** — the one `kit_id` answers
	# for. Both needles are whole role phrases, so neither can match the SPEARS row two lines up.
	var clubs_role := DetailFormat.KIT_ROLE_WARRIOR_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_CLUBS, DetailFormat.KIT_CONDITION_DECIMALS)
	var hunt_kit_role := DetailFormat.KIT_ROLE_WARRIOR_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_EQUIPPED, DetailFormat.KIT_CONDITION_DECIMALS)
	var clubs_line := _kit_breakdown_line(popover, DetailFormat.KIT_LABEL_CLUBS)
	_assert_band_panel("CLUBS state the DEFENDERS' attack (%s), never the hunt kit's (%s) — \"%s\""
			% [clubs_role, hunt_kit_role, clubs_line],
		clubs_line.contains(clubs_role) and not clubs_line.contains(hunt_kit_role))
	_assert_band_panel("…beside its own condition (%s)" % String.num(
			BandFx.KIT_CONDITION_CLUBS, DetailFormat.KIT_CONDITION_DECIMALS),
		clubs_line.contains(String.num(BandFx.KIT_CONDITION_CLUBS,
			DetailFormat.KIT_CONDITION_DECIMALS)))
	# …and the hunt rows are still paired with THEIR tiers, so the three new ones cannot have been
	# added by making every row quote the same number.
	var sled_line := _kit_breakdown_line(popover, DetailFormat.KIT_LABEL_SLED)
	_assert_band_panel("…and the SLED still states the HUNT's carry (%s) — \"%s\""
			% [String.num(BandFx.KIT_HUNT_CARRY_EQUIPPED, DetailFormat.KIT_CARRY_DECIMALS), sled_line],
		sled_line.contains(DetailFormat.KIT_ROLE_HUNT_CARRY_FORMAT % String.num(
			BandFx.KIT_HUNT_CARRY_EQUIPPED, DetailFormat.KIT_CARRY_DECIMALS)))

## The open breakdown popover's RENDERED text — the popover is a Window and never lands in a capture,
## so this is the only witness to what it says. Parsed, so the BBCode tags are gone.
func _kit_popover_text() -> String:
	var label = _hud._disclosures._breakdown_popover_label
	return "" if label == null else String((label as RichTextLabel).get_parsed_text())

## ONE breakdown row out of the popover, by the item's NAME. Split per line rather than matched over
## the whole popover, because every row carries the same shape and a whole-popover `contains` could be
## satisfied by the wrong item's row — the exact substitution these assertions exist to catch.
func _kit_breakdown_line(popover: String, label: String) -> String:
	for line in popover.split("\n"):
		if String(line).contains(label):
			return String(line)
	return ""

## **EACH BAND-WIDE ROLE CARD NAMES ITS OWN KIT AND ITS OWN ITEM** — the picker's closed face and the
## gear line beneath it, asserted by EQUALITY on both cards.
##
## **THE ITEM IS THE CLAIM, AND IT IS WHY THE WARRIOR CARD IS HALF OF THIS.** A hint that guessed the
## item from the display AXIS maps `attack` to the SPEARS, which is right on a hunt sheet and wrong
## here: a warrior kit buys the same stat off `clubs`. Such a card renders
## `attack 6 defending the camp · Spears 74` — a perfectly plausible line, quoting the wear on gear
## this role never touches. The gear line reads `KitOption.item_ids` instead, which is the wire's own
## statement of what the kit carries.
##
## **THE EXPECTED STRINGS NAME THEIR ITEM LABELS OUTRIGHT**, never through `KitRoster.kit_item_ids`:
## composing the expectation through the derivation under test would assert only that it agrees with
## itself.
func _assert_role_card_gear() -> void:
	_assert_one_role_card_gear(HudWorkVocab.ROLE_NAME_SCOUT, KitRoster.JOB_SCOUT, "Wayfinding kit",
		DetailFormat.KIT_ROLE_SCOUT_VANTAGE_FORMAT % String.num(
			BandFx.KIT_SCOUT_VANTAGE_EQUIPPED, DetailFormat.KIT_VANTAGE_DECIMALS),
		DetailFormat.KIT_LABEL_WAYFINDING, BandFx.KIT_CONDITION_WAYFINDING)
	_assert_one_role_card_gear(HudWorkVocab.ROLE_NAME_WARRIOR, KitRoster.JOB_WARRIOR, "Warrior kit",
		DetailFormat.KIT_ROLE_WARRIOR_ATTACK_FORMAT % String.num(
			BandFx.KIT_ATTACK_CLUBS, DetailFormat.KIT_CONDITION_DECIMALS),
		DetailFormat.KIT_LABEL_CLUBS, BandFx.KIT_CONDITION_CLUBS)

func _assert_one_role_card_gear(role_name: String, job: String, kit_name: String, effect: String,
		item_label: String, condition: float) -> void:
	var card := _find_role_card(role_name)
	_assert_band_panel("the %s card carries a kit picker" % role_name, card != null)
	if card == null:
		return
	var picker := _find_meta_control(card, KitRoster.KIT_PICKER_META) as OptionButton
	if picker == null:
		_fail("the %s card has no kit picker" % role_name)
		return
	var face := HudComposeVocab.KIT_PICKER_FACE_FORMAT % [
		String(HudComposeVocab.KIT_JOB_GLYPHS[job]), kit_name]
	_assert_band_panel("…whose face names this role's own kit (\"%s\")" % picker.text,
		picker.text == face)
	var hint := _find_meta_control(card, KitRoster.KIT_HINT_META) as Label
	if hint == null:
		_fail("the %s card has no gear line" % role_name)
		return
	var want := HudComposeVocab.KIT_HINT_SEPARATOR.join([effect,
		HudComposeVocab.KIT_HINT_ROLE_ITEM_FORMAT % [item_label,
			String.num(condition, DetailFormat.KIT_CONDITION_DECIMALS)]])
	_assert_band_panel("…over a gear line stating what it buys and the item behind it — \"%s\""
			% hint.text, hint.text == want)

## **THE TWO ROLE CARDS DRAW TO THE SAME HEIGHT.** Reported on sight: side by side at unequal heights
## the pair reads as ragged. Their content genuinely differs in height — the Scout's description wraps
## to three lines against the Warrior's two, and either kit name can wrap — so the claim is that the
## SHORTER card is padded to the taller, never that the two hold the same amount.
##
## **IT IS ASSERTED RATHER THAN EYEBALLED BECAUSE THE PROPERTY RESTS ON ONE SIZE FLAG** — the
## `SIZE_FILL` the row's `HBoxContainer` stretches against, which is `Control`'s own default and so
## appears in no diff at all until someone writes something else there. A card an eighth short still
## draws its border, its background and every control inside it, so a PNG shows two plausible cards.
##
## **AND BECAUSE THE NEXT TEXT CHANGE IS WHAT WOULD REINTRODUCE IT.** A hardcoded minimum height would
## pass this by construction and be wrong the moment a description or a kit name changes length, so
## the assertion is deliberately a comparison of the two RENDERED heights and quotes both.
##
## `HEIGHT_EPSILON` is the sub-pixel disagreement between a container's rounded layout and the float
## rects it writes — the tolerance `_faction_keyless_rows` takes for the same reason.
func _assert_role_cards_are_level() -> void:
	var scout := _find_role_card(HudWorkVocab.ROLE_NAME_SCOUT)
	var warrior := _find_role_card(HudWorkVocab.ROLE_NAME_WARRIOR)
	if scout == null or warrior == null:
		_fail("both role cards must render to compare their heights")
		return
	# THE PRECONDITION. Two cards of zero height are trivially level, and so are two cards whose
	# content happens to measure the same — the claim is only worth making where one card's CONTENT is
	# genuinely shorter than the other's, which is what makes the padding observable.
	var scout_content := scout.get_combined_minimum_size().y
	var warrior_content := warrior.get_combined_minimum_size().y
	_assert_band_panel(
		"the two role cards hold DIFFERENT amounts (Scout wants %.0fpx, Warrior %.0fpx) — %s"
			% [scout_content, warrior_content,
				"so the shorter one has to be padded" if scout_content != warrior_content
					else "NOTHING TO PROVE"],
		scout_content != warrior_content and minf(scout_content, warrior_content) > 0.0)
	_assert_band_panel("…and both cards RENDER at the same height (Scout %.0fpx, Warrior %.0fpx)"
			% [scout.size.y, warrior.size.y],
		absf(scout.size.y - warrior.size.y) <= ROLE_CARD_HEIGHT_EPSILON)

## How far two role cards' rendered heights may differ before the pair reads as ragged — the sub-pixel
## disagreement between a container's rounded layout and the float rects it writes, and nothing more.
const ROLE_CARD_HEIGHT_EPSILON := 1.0

## **THE PICK REACHES THE COMMAND, AND THE DEFAULT STILL OMITS THE TOKEN** — a PAIR, because either
## claim alone is satisfied by a builder that gets the tail exactly backwards.
##
## Both halves are driven through the picker's REAL `item_selected` wiring — the signal
## `HudWidgets.build_option_picker` connects the entry callables to — and read off the payload the
## HUD actually emits, put through `Main.format_assign_labor`. A role card has no Send to commit at,
## so the pick emits on the press; a pick that only moved client state would leave the sim running
## the kit the card had stopped naming.
func _assert_role_kit_command_carries_the_pick() -> void:
	var card := _find_role_card(HudWorkVocab.ROLE_NAME_SCOUT)
	var picker := _find_meta_control(card, KitRoster.KIT_PICKER_META) as OptionButton if card != null else null
	if picker == null:
		_fail("no Scout kit picker to drive")
		return
	# The entry that is NOT tagged `(default)` — the roster's `none`, which the sim would never
	# resolve on its own, so the tail it produces cannot be an accident of the default.
	var other := -1
	var default_entry := -1
	for i in picker.item_count:
		if picker.get_item_text(i).contains(HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX):
			default_entry = i
		else:
			other = i
	_assert_band_panel("the Scout picker offers a non-default kit to pick (%d entries)"
			% picker.item_count, other >= 0 and default_entry >= 0)
	if other < 0 or default_entry < 0:
		return
	var picked := _emitted_assign_line(picker, other)
	_assert_band_panel("picking a kit on a STAFFED role card emits at once — \"%s\"" % picked,
		picked != "")
	if picked == "":
		return
	_assert_band_panel("…and the line carries the named kit tail — \"%s\"" % picked,
		picked.ends_with(" kit %s" % BandFx.KIT_ID_NONE))
	_assert_band_panel("…on the band-wide role's own grammar, which takes no other token — \"%s\""
			% picked,
		picked.contains(" %s " % HudConst.LABOR_KIND_SCOUT))
	# THE NEGATIVE HALF. Picking the job default back emits the line this client always emitted:
	# `Main._kit_token` omits the tail when the choice is what the sim would resolve anyway, and a
	# builder that appended unconditionally would pass every claim above.
	var restored := _emitted_assign_line(picker, default_entry)
	_assert_band_panel("…while picking the job DEFAULT back omits the tail — \"%s\"" % restored,
		restored != "" and not restored.contains(" kit "))

## Drive one entry of a live picker and hand back the command line the HUD's emit produced, `""` when
## nothing was emitted. The picker is re-found after the pick because the card rebuilds on it.
func _emitted_assign_line(picker: OptionButton, index: int) -> String:
	var seen: Array[Dictionary] = []
	var sink := func(payload: Dictionary) -> void: seen.append(payload)
	_hud.assign_labor_requested.connect(sink)
	picker.item_selected.emit(index)
	_hud.assign_labor_requested.disconnect(sink)
	if seen.is_empty():
		return ""
	return String(MAIN_SCRIPT.format_assign_labor(seen[0]).get("line", ""))

## The WORKFORCE zone's role CARD, found by its own title label. **Never by sibling index**: the two
## cards are built side by side and an order assumption would read the Warrior's picker for the
## Scout's, which is the exact substitution the assertions above exist to catch. Deepest match first,
## so an enclosing `PanelContainer` can never answer for a card inside it.
func _find_role_card(role_name: String) -> PanelContainer:
	var host := _band_flank_host()
	return null if host == null else _role_card_under(host, role_name)

func _role_card_under(node: Node, role_name: String) -> PanelContainer:
	for child in node.get_children():
		var found := _role_card_under(child, role_name)
		if found != null:
			return found
	if node is PanelContainer and _has_label_titled(node, role_name):
		return node as PanelContainer
	return null

## Does this subtree carry a Label whose whole text is `title`? EXACT, not `contains` — the role
## card's own hint mentions "scouts", and a substring test would match the hint before the title.
func _has_label_titled(node: Node, title: String) -> bool:
	if node is Label and (node as Label).text == title:
		return true
	for child in node.get_children():
		if _has_label_titled(child, title):
			return true
	return false

## The panel's SANCTIONED `ScrollContainer`s, by node name and by the zone each must sit under. Two,
## and the pairing is half the claim: a scroll is only safe in a zone whose builder declares a fixed
## minimum for it, so `PartiesList` under the band zone would be as much a stray as an unnamed one.
const SANCTIONED_SCROLLS := [
	[HudWorkVocab.PARTIES_LIST_NAME, BandCityPanel.ZONE_PARTIES],
	[HudWorkVocab.BAND_ZONE_SCROLL_NAME, BandCityPanel.ZONE_BAND],
]

## GUARD: the zone model is NO-SCROLL by construction, with **exactly two sanctioned exceptions** —
## the parties zone's row list and the band zone's block stack (`SANCTIONED_SCROLLS`). Any other
## `ScrollContainer` would silently reintroduce the content-dependent sizing the rework removed.
##
## **THE RULE IS NARROWED, NOT DELETED, and that is the whole point of asserting it this way.** Each
## exception is safe only because its builder declares a FIXED minimum on the scrolling axis, so what
## the scroll holds never reaches the zone's minimum; a scroll added anywhere else — the WORK board,
## most of all, which PAGES for exactly this reason — would carry its content's height straight into
## the reservation. So the walk collects EVERY `ScrollContainer` and requires each to be a sanctioned
## name AND to sit under the zone that sanctions it.
##
## **IT ALSO ASSERTS THE SANCTIONED ONES EXIST**, because "no strays" is satisfied by a panel that has
## lost a scroll altogether — which is the regression that would put the seven-line parties strip back
## to clipping, and the band zone back to deleting its chart.
##
## **The BAND zone's is claimed only on a BAND page.** `FactionRollup.build_band_zone` authors that
## zone for the faction subject and builds no scroll — its blocks are bounded lists, not a stack that
## can outgrow the box — so requiring one there would fail a page that is correct.
func _assert_scroll_only_where_sanctioned() -> void:
	var found: Array[Node] = []
	_collect_scroll_containers(_panel, found)
	# **EACH ZONE IS FOUND THROUGH THE PANEL'S OWN `_zones` DICT, never by host name.** The wide shell
	# mounts them in `Zone_parties` / `Zone_band` and the narrow one in the single `NarrowZoneHost`, so
	# a host-name test would call the narrow shell's own scroll a stray — which it did.
	var strays: Array[String] = []
	var counts := {}
	for pair in SANCTIONED_SCROLLS:
		counts[String(pair[0])] = 0
	for node in found:
		var matched := false
		for pair in SANCTIONED_SCROLLS:
			var zone: Variant = _panel._zones.get(pair[1])
			if String(node.name) == String(pair[0]) and zone is Node \
					and (zone as Node).is_ancestor_of(node):
				counts[String(pair[0])] = int(counts[String(pair[0])]) + 1
				matched = true
				break
		if not matched:
			strays.append(String(node.get_path()))
	if not strays.is_empty():
		_fail("%s — ScrollContainer outside a sanctioned zone at %s — no other zone may scroll"
			% [_current_state, ", ".join(strays)])
		return
	# A zone the panel does not currently own (no band, or the zones freed) has no scroll to find — an
	# absence that says nothing about the rule and must not fail.
	var parties_zone: Variant = _panel._zones.get(BandCityPanel.ZONE_PARTIES)
	if not (parties_zone is Node):
		print("band_panel_preview: assert OK — no ScrollContainer in the panel (the parties zone is not mounted here)")
		return
	_assert_band_panel("the parties zone scrolls its list and NOTHING unsanctioned in the panel scrolls (%d sanctioned, %d stray)"
		% [int(counts[HudWorkVocab.PARTIES_LIST_NAME]), strays.size()],
		int(counts[HudWorkVocab.PARTIES_LIST_NAME]) == 1)
	# **THE NARROW SHELL PARENTS ONLY THE ACTIVE TAB'S ZONE** (`_reparent_zones` DETACHES the rest), so
	# the band zone can exist and be nowhere the walk above could have found it. Asked there, this claim
	# would report every parties-tab state as a band zone that had lost its scroll.
	var band_zone: Variant = _panel._zones.get(BandCityPanel.ZONE_BAND)
	if not (band_zone is Node) or not _panel.is_ancestor_of(band_zone as Node) \
			or _hud._bandpanel._panel_is_faction:
		return
	_assert_band_panel("…and the band zone scrolls its block stack, so no tier can delete a block instead (%d sanctioned)"
		% int(counts[HudWorkVocab.BAND_ZONE_SCROLL_NAME]),
		int(counts[HudWorkVocab.BAND_ZONE_SCROLL_NAME]) == 1)

func _collect_scroll_containers(node: Node, into: Array[Node]) -> void:
	if node is ScrollContainer:
		into.append(node)
	for child in node.get_children():
		_collect_scroll_containers(child, into)

## The parties list's scrollbar, or `null` where the list is not mounted (the narrow shell's other
## tabs). Read off the live node rather than re-derived, since "is it scrolling?" is a question about
## what Godot decided, not about what the content measures.
func _parties_list_scroll() -> ScrollContainer:
	var parties_zone: Variant = _panel._zones.get(BandCityPanel.ZONE_PARTIES)
	if not (parties_zone is Node):
		return null
	var found: Array[Node] = []
	_collect_scroll_containers(parties_zone as Node, found)
	return found[0] as ScrollContainer if not found.is_empty() else null

## GUARD: **the list scrolls WHEN IT OVERFLOWS AND ONLY THEN.** `SCROLL_MODE_AUTO` is what makes that
## true, so this reads the bar's own visibility back and pairs it with the content's own measurement —
## a bar shown over content that fits is as wrong as content clipped with no bar.
##
## Stated as a relation rather than as a literal expectation, so ONE assertion serves the empty list
## and the seven-line worst case, and neither can pass by rendering the other's answer.
func _assert_parties_list_scrolls_iff_it_overflows(state_name: String) -> void:
	var scroll := _parties_list_scroll()
	if scroll == null:
		_fail("%s — no parties list to judge" % state_name)
		return
	var rows: Control = scroll.get_child(0)
	var needed := rows.get_combined_minimum_size().y
	var room := scroll.size.y
	var overflows := needed > room + ZONE_BOUNDS_TOLERANCE
	var bar := scroll.get_v_scroll_bar()
	_assert_band_panel("%s: the parties list scrolls iff it overflows (content %.0fpx of %.0fpx room, bar %s)"
		% [state_name, needed, room, "shown" if bar.visible else "hidden"],
		bar.visible == overflows)

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
		_fail("%s — %s" % [_current_state, failure])

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
		_fail("short-tier trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	# The Food row proves the vitals label is actually populated — without it, "no Trade row" would
	# pass vacuously on an empty label.
	if not text.contains("Food"):
		_fail("short-tier trade assert — vitals label has no Food row (vacuous)")
		return
	if text.contains("Trade"):
		_fail("SHORT tier still renders the Trade row — the compact gate is off")
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
## It is the same walk `_collect_zone_content_shortfall` makes — the deepest `top + needed` any
## measurable control reaches — with ONE deliberate divergence: it descends INTO a sanctioned
## `ScrollContainer` where the assertion stops at it. The two are asking different questions the
## moment a zone can scroll ("must the box hold this?" against "how tall is it?"), and the scroll's
## own declared minimum is its BOX, so a walk that stopped there would print the box back at itself
## and say nothing about how much of the stack is under the bar.
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
##
## **IT DESCENDS THROUGH EVERYTHING, unlike the assertion's walk, and takes the deepest answer it
## finds anywhere.** A `ScrollContainer` declares the BOX it was given, so measuring it — or the column
## that merely inherits that minimum through it — reports the box back at itself and says nothing about
## the stack under the bar, which is the one number this report exists to give. So nothing with a
## scroll anywhere beneath it is measured; everything else is measured AND descended into.
func _zone_content_extent(node: Node, host: Control) -> float:
	var deepest := 0.0
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		if not _contains_scroll(content):
			var needed := content.get_combined_minimum_size().y
			if needed > 0.0:
				deepest = maxf(deepest, content.global_position.y - host.global_position.y + needed)
		deepest = maxf(deepest, _zone_content_extent(content, host))
	return deepest

## Does a sanctioned scroll sit anywhere in this subtree? The measurement above skips such a control:
## its minimum is the scroll's declared box rather than anything its content asked for.
func _contains_scroll(node: Node) -> bool:
	if node is ScrollContainer:
		return true
	for child in node.get_children():
		if _contains_scroll(child):
			return true
	return false

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
		_fail("merged-food-row assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains(MERGED_FOOD_HAY_NEEDLE):
		_fail("the SHORT tier's Food row carries no hay clause — the merge is off (got: %s)" % text)
		return
	if text.contains(FODDER_ROW_NEEDLE):
		_fail("the SHORT tier still renders a standalone Fodder row beside the merged Food line")
		return
	# **THE ROW IS BOUNDED BY THE ROW THAT FOLLOWS IT, AND THAT ROW IS NOW `Kit`.** This read to
	# `Morale` while Food and Morale were adjacent; the Kit row landed between them and the cut then
	# measured TWO rows as one, reporting a 624px wrap on a line that fits comfortably. A bound naming
	# the row that actually follows is the only kind that survives an insertion, so it takes whichever
	# of the candidates comes FIRST rather than one fixed name.
	var food_run := _vitals_run(text, HudDisclosureVocab.DETAIL_ROW_FOOD,
		[HudDisclosureVocab.DETAIL_ROW_KIT, HudDisclosureVocab.DETAIL_ROW_MORALE])
	if food_run == "":
		_fail("merged-food-row assert cannot find the Food row (got: %s)" % text)
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
		_fail("merged-morale-row assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains(HudDisclosureVocab.DETAIL_ROW_GROWTH):
		_fail("the SHORT tier's Morale line carries no Growth clause — the merge is off (got: %s)" % text)
		return
	# Nothing follows Morale in the dock's vitals block (the Position row is the drawer host's), so the
	# run reaches the end of the label — stated as an EMPTY bound list rather than left implicit.
	var morale_run := _vitals_run(text, HudDisclosureVocab.DETAIL_ROW_MORALE, [])
	if morale_run == "":
		_fail("merged-morale-row assert cannot find the Morale row (got: %s)" % text)
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
		_fail("growth-row tier assert found no vitals label (%s)" % state_name)
		return
	var merged_needle := BandDetailLines.BAND_MORALE_GROWTH_CLAUSE_SEPARATOR \
		+ DetailFormat.DISCLOSURE_URL_OPEN
	if vitals.text.contains(merged_needle):
		_fail("%s merged Growth onto the Morale line — that is the SHORT tier's layout only" % state_name)
		return
	if not vitals.get_parsed_text().contains(DetailFormat.GROWTH_ROW_ANCHOR_SUFFIX):
		_fail("%s dropped the Growth row's `of normal` anchor — the SHORT tier's short form leaked up" % state_name)
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
		_fail("the %s line WRAPS — %.0fpx of run in a %.0fpx column" % [
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
		_fail("forage-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("+0.08"):
		_fail("Trade must read +0.08 (forage 0.04 + hunt 0.04) — got: %s" % text)
		return
	# The band-local STOCK, read off `stores.trade_goods` the way the Food row reads the larder.
	# Matched as the VALUE cell's own run (`12.0 · +0.08`) rather than `Trade 12.0`: the KV formatter
	# splits the row into table cells and the key cell carries the disclosure caret, so the two are never
	# adjacent in the parsed text. ONE DECIMAL — the stock is a float on screen because the sim
	# accumulates sub-unit trade income; the exact rendered value is what this pins.
	if not text.contains("12.0 · +0.08"):
		_fail("Trade row does not carry the band's stock of 12 — got: %s" % text)
		return
	var rows := _disclosure_rows(BAND_FIXTURE_DISCLOSURE_TRADE)
	var joined := "\n".join(rows)
	if not joined.contains(DetailFormat.FOOD_LABEL_GATHERED):
		_fail("the Trade breakdown has no Gathered row — the forage source's trade was dropped (rows: %s)" % joined)
		return
	if not joined.contains(DetailFormat.FOOD_LABEL_HUNTED):
		_fail("the Trade breakdown has no Hunted row (rows: %s)" % joined)
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
		_fail("zero-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("Trade"):
		_fail("a band earning no trade dropped its Trade row — it must read zero")
		return
	# `format_yield` writes a signed magnitude, so a zero rate renders "+0.00". Matching the NUMBER
	# rather than the row keeps this from passing on an earning band that merely has a Trade row.
	if not text.contains("+0.00"):
		_fail("zero-trade band's Trade row does not read +0.00 — got: %s" % text)
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
		_fail("%s" % failure)

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
		# **A SANCTIONED SCROLL'S RECT IS CHECKED AND ITS CONTENT IS NOT DESCENDED INTO.** This guard
		# exists because the zone hosts CLIP: a rect outside the host is content the player can never
		# see. Inside a `ScrollContainer` that premise is false — a stack taller than its viewport is
		# precisely what the bar is for — so descending would report every scrolled band zone as an
		# overflow. The scroll ITSELF is still bounded by the zone, which is the claim that matters.
		if content is ScrollContainer:
			continue
		_collect_zone_overflow(content, bounds, failures)

## **WHY THE DOCK RENDERED NOTHING FOR WILD FOWL — and the INVERSION that closed it.**
##
# **THE PARTY-LADDER BLOCK IS GONE, AND SO IS EVERYTHING IT PINNED.**
#
# `_assert_party_past_the_rungs_is_quoted`, `_assert_party_ladder_rounding`, `_ladder_hunt_estimates`,
# `_denial_ladder_rows`, `denial_herd_of` and the four `LADDER_*` / `DENIAL_LADDER_*` constants all
# existed to pin ONE behaviour: the snapshot's estimate tables sampled floor x party on a ladder, so a
# composed party usually fell between two rungs, the client quoted the nearest, and the sheet had to
# NAME the party the figures were costed for. Every one of those claims is unreachable now — the sim
# is asked for the exact party and answers it, so there is no rung, no rounding and no note. They are
# deleted rather than repointed: an assertion that a value equals itself is not a test.

## **WHERE THE COMPOSE SHEET CURRENTLY LIVES.** The sheet the parties zone cannot hold is rendered in
## `BandComposeFloat` instead (a card on the HUD `CanvasLayer`, beside the panel), so every assertion
## and measurement about that sheet has to follow it there. Pointed at `_panel` they would all go
## VACUOUS the moment the float works — which is exactly the failure `_assert_compose_float` below
## exists to make impossible for the fit claim, and this helper makes impossible for the rest.
func _compose_surface() -> Node:
	var float_card := _hud._bandpanel.compose_float()
	if float_card != null and _hud._bandpanel.compose_is_floating():
		return float_card
	return _panel

## **THE FIT CLAIM DOES NOT DISAPPEAR WITH THE SHEET.** `_assert_zone_content_fits` passes TRIVIALLY
## once the compose sheet leaves the zone — an empty box fits anything — so a float that moved the
## overflow somewhere unmeasured would look exactly like a fix. This is the other half: the sheet is
## really gone from the zone, the zone really fits what is left, and the float itself fits the VIEWPORT
## and clears the panel card it came from.
##
## Both rect claims are made in the `event_dock` inset idiom, negative control included: a
## non-overlap test on two rects that never share a band is not a claim, so the vacuity guard fires on
## the axis the two are NOT stacked along, and a live control first shows the very same `intersects`
## test firing on these very rects when one is moved onto the other.
func _assert_compose_float(state_name: String) -> void:
	var floater: BandComposeFloat = _hud._bandpanel.compose_float()
	if floater == null or not _hud._bandpanel.compose_is_floating():
		_fail("%s — the compose sheet did NOT float, so nothing below is a claim" % state_name)
		return
	# (1) IT REALLY LEFT THE ZONE. Asked of the Send button's own meta, which every branch of this
	# sheet renders and nothing else in the panel carries.
	_assert_band_panel("%s — the composed sheet is GONE from the parties zone" % state_name,
		_find_meta_control(_panel, HudWidgets.SEND_HUNT_CONFIRM_META) == null)
	_assert_band_panel("%s — …and it is in the float, whole (its Send is there)" % state_name,
		_find_meta_control(floater, HudWidgets.SEND_HUNT_CONFIRM_META) != null)
	# (2) THE ZONE FITS WHAT IS LEFT — the same CONTAINMENT walk `_assert_zone_content_fits` makes, which
	# stops at a sanctioned scroll (a scrolled stack is reached, not clipped), restated here with the
	# stack's measured height beside it so a zone that merely stopped overflowing by luck is visible.
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		var extent := _zone_content_extent(host, host)
		if extent <= 0.0:
			continue
		var shortfalls: Array[String] = []
		_collect_zone_content_shortfall(host, host, shortfalls)
		_assert_band_panel("%s — zone %s holds its remaining content (%s; its stack measures %.0fpx of a %.0fpx box)" % [
			state_name, host.name,
			"nothing clipped" if shortfalls.is_empty() else ", ".join(shortfalls), extent, host.size.y],
			shortfalls.is_empty())
	# (3) THE FLOAT FITS THE VIEWPORT. This is where the overflow went, so this is where it is measured.
	var view := get_viewport().get_visible_rect()
	var card := _panel.card_rect()
	var box := floater.get_global_rect()
	var over := _rect_overflow(box, view)
	_assert_band_panel("%s — the float fits the viewport (float %s in %s, worst overflow %.0fpx)" % [
		state_name, box, view, maxf(over.x, over.y)], over.x <= ZONE_BOUNDS_TOLERANCE and over.y <= ZONE_BOUNDS_TOLERANCE)
	# …and it is holding its content rather than growing out of itself, the `AutoSizingPanel` lie
	# `panel-framework.md` records: a card fitted too short still DRAWS at its content's size.
	_assert_band_panel("%s — the float's card fits the float (%.0fpx of %.0fpx)" % [
		state_name, floater.card().get_combined_minimum_size().y, box.size.y],
		floater.card().get_combined_minimum_size().y <= box.size.y + ZONE_BOUNDS_TOLERANCE
			or floater.card().get_combined_minimum_size().y > view.size.y)
	# (4) IT NEVER OVERLAPS THE CARD IT CAME FROM — with the vacuity guard on the axis the two are not
	# stacked along, and a live negative control on the same two rects.
	var stacked_vertically: bool = box.position.y >= card.end.y or card.position.y >= box.end.y
	var shares_other_axis: bool = box.position.x < card.end.x and card.position.x < box.end.x \
		if stacked_vertically else box.position.y < card.end.y and card.position.y < box.end.y
	if not shares_other_axis:
		_fail(("%s — VACUOUS: the float and the panel card share no band on "
			+ "either axis, so 'they do not overlap' claims nothing") % state_name)
		return
	var moved_onto_card := Rect2(card.position, box.size)
	_assert_band_panel("%s — negative control: the SAME test fires with the float moved onto the card" % state_name,
		moved_onto_card.intersects(card))
	_assert_band_panel("%s — the float clears the panel card (float %s vs card %s)" % [
		state_name, box, card], not box.intersects(card))

## **THE PAIRED NEGATIVE, and without it the fork is only ever asserted in one direction.** A trigger
## stuck ON is as wrong as one stuck off and every claim in `_assert_compose_float` would still pass
## under it — the sheet would be whole, in a float, clear of the card, in a dock that has ample room
## for it in the zone. This is the state that says the zone keeps the sheet it CAN hold.
func _assert_compose_in_zone(state_name: String) -> void:
	_assert_band_panel("%s — the zone HOLDS the sheet it has room for (no float)" % state_name,
		not _hud._bandpanel.compose_is_floating()
			and _find_meta_control(_panel, HudWidgets.SEND_HUNT_CONFIRM_META) != null)

## **OPEN THE EMPTY HUNT FORM THROUGH THE FOOTER BUTTON, AND JUDGE THE MEASUREMENT ITSELF** — the
## regression guard for the second report of the floating empty sheet. It is deliberately NOT a
## picture: a sheet floating on a phantom measurement and one floating on a real one render
## identically, and a sheet sitting in the zone on a mark that HAPPENS to be under the box is
## indistinguishable from one sitting there because the mark is honest.
##
## The mechanism, measured: between the render and the deferred container sort, the parties column has
## already been given its host's width by its anchors while nothing under it has been fitted, so
## `get_combined_minimum_size()` sums a column of autowrap `Label`s all shaping at wrap width 0 —
## **1278px where the laid-out answer is 207**. Recording that latches the float for the rest of the
## composing act, since the mark is a high-water mark. So the claim is made in that window and in the
## one after it, as a PAIR: unmeasurable before the layout pass, measurable after, and the mark that
## survives is the laid-out number. Either half alone passes on a guard wired to one answer.
func _assert_empty_compose_opens_in_the_zone(state_name: String) -> void:
	var launch := _find_meta_control_valued(_panel, HudWidgets.MISSION_LAUNCH_META,
		HudComposeVocab.COMPOSE_MISSION_HUNT) as Button
	if launch == null or launch.disabled:
		_fail("%s — no live 🏹 Hunt launch button, so nothing below is driven" % state_name)
		return
	# The REAL press. Everything until the next `await` runs inside the pre-layout window the phantom
	# lives in — the sheet is built and parented, and no container has sorted.
	launch.emit_signal("pressed")
	var col: VBoxContainer = _hud._bandpanel._parties_zone_col
	var box: Vector2 = _hud._bandpanel._parties_zone_box_known()
	var phantom: float = col.get_combined_minimum_size().y
	# **THE VACUITY GUARD.** If the unsorted reading would not have floated the sheet anyway, refusing
	# to record it proves nothing about this defect.
	if phantom <= box.y + HudComposeVocab.COMPOSE_FLOAT_SLACK:
		_fail(("%s — VACUOUS: the pre-layout column reads %.0fpx against a "
			+ "%.0fpx box, so recording it would not have floated the sheet either way") % [
			state_name, phantom, box.y])
		return
	_assert_band_panel(("%s — a sheet the layout has not fitted is NOT measurable (column %.0fpx wide "
		+ "reports %.0fpx of requirement, which WOULD float it out of its %.0fpx box)") % [
		state_name, col.size.x, phantom, box.y], not _hud._bandpanel._party_compose_measurable())
	await _settle()
	# …and the paired positive, without which a guard stuck at "never measurable" passes above.
	var settled: float = _hud._bandpanel._parties_zone_col.get_combined_minimum_size().y
	_assert_band_panel("%s — …and IS measurable once it has been (%.0fpx now)" % [state_name, settled],
		_hud._bandpanel._party_compose_measurable())
	_assert_band_panel(("%s — the mark that survived is the LAID-OUT number, not the phantom "
		+ "(%.0fpx recorded, %.0fpx laid out, %.0fpx unsorted)") % [
		state_name, _hud._bandpanel._party_compose_needed, settled, phantom],
		is_equal_approx(_hud._bandpanel._party_compose_needed, settled))

## **A ZONE THAT CAN HOLD THE SHEET NEVER FLOATS IT, stated against the measured numbers rather than
## the dock edge.** `_assert_compose_in_zone` says a particular tall dock kept its sheet; this says the
## RULE — whatever the mark and whatever the box, the sheet is in the zone exactly when the zone has
## room for it. The precondition is the room, so a state where it does not fit refuses to claim
## anything here rather than passing as "correctly floated".
##
## The sheet is located by NODE IDENTITY (the controller's own `_party_compose_sheet`, walked up to
## whichever surface owns it), never by a face: the empty form's Send is disabled and carries no
## confirm meta, being a reason rather than a confirm.
func _assert_zone_holds_its_compose_sheet(state_name: String) -> void:
	var needed: float = _hud._bandpanel._party_compose_needed
	var box: Vector2 = _hud._bandpanel._parties_zone_box_known()
	if box == Vector2.ZERO or needed > box.y + HudComposeVocab.COMPOSE_FLOAT_SLACK:
		_fail(("%s — VACUOUS: the zone (%.0fpx) cannot hold this sheet "
			+ "(%.0fpx), so 'a zone with room keeps its sheet' is not what is being tested") % [
			state_name, box.y, needed])
		return
	var sheet: Control = _hud._bandpanel._party_compose_sheet
	_assert_band_panel(("%s — the zone has room (%.0fpx of a %.0fpx box, %.0fpx spare) and so it KEEPS "
		+ "its sheet") % [state_name, needed, box.y, box.y - needed],
		sheet != null and is_instance_valid(sheet) and _is_descendant_of(sheet, _panel)
			and not _hud._bandpanel.compose_is_floating())

## Find a Control carrying `meta` with a specific VALUE — the identity handle on one of a family of
## controls built by a shared builder (the three parties-footer mission launchers).
func _find_meta_control_valued(node: Node, meta: String, value: Variant) -> Control:
	if node is Control and (node as Control).has_meta(meta) and (node as Control).get_meta(meta) == value:
		return node as Control
	for child in node.get_children():
		var found := _find_meta_control_valued(child, meta, value)
		if found != null:
			return found
	return null

func _is_descendant_of(node: Node, root: Node) -> bool:
	var walk: Node = node
	while walk != null:
		if walk == root:
			return true
		walk = walk.get_parent()
	return false

## **AN UNKNOWN ZONE BOX MUST NOT FLOAT — and no picture can carry this.** Reported from play: the
## sheet floated in a TALL left dock, where the zone offers ~1055px and the empty form wanted a couple
## of hundred. `BandCityPanel.zone_size(ZONE_PARTIES)` answers `Vector2.ZERO` whenever the panel is
## collapsed, hidden, or simply has not laid out yet, and the predicate used to fall back to
## `ZONE_FALLBACK_SIZE` (340×360) — so "I do not know yet" decided as "this overflows", and the
## high-water mark latched it ON for the rest of the composing act.
##
## Driven through the REAL predicate with the mark left exactly where the short dock measured it, and
## the box made unknown the way the live client makes it unknown (a collapsed panel). The precondition
## is the whole point: with the mark below the fallback's 360 the two answers coincide and the claim
## would be vacuous.
func _assert_unknown_zone_box_does_not_float(state_name: String) -> void:
	var needed: float = _hud._bandpanel._party_compose_needed
	if needed <= HudWorkVocab.ZONE_FALLBACK_SIZE.y:
		_fail(("%s — VACUOUS: the latched requirement is %.0fpx, under the "
			+ "%.0fpx fallback box, so an unknown box answers 'no float' either way") % [
			state_name, needed, HudWorkVocab.ZONE_FALLBACK_SIZE.y])
		return
	var was_collapsed: bool = _panel.is_collapsed()
	_panel.set_collapsed(true)
	var box: Vector2 = _hud._bandpanel._parties_zone_box_known()
	_assert_band_panel("%s — precondition: a collapsed panel answers no parties-zone box" % state_name,
		box == Vector2.ZERO)
	_assert_band_panel(("%s — an UNKNOWN zone box does not float (mark %.0fpx, which WOULD float "
		+ "against the %.0fpx fallback)") % [state_name, needed, HudWorkVocab.ZONE_FALLBACK_SIZE.y],
		not _hud._bandpanel._party_compose_floats())
	_panel.set_collapsed(was_collapsed)

## Stage a latched requirement no zone box in this client can hold, while the panel is still in the
## SHORT dock. **The mark is the INPUT to the rule under test, not the rule** — the rule is "a change of
## zone box drops the mark", and no fixture here produces a mark that naturally overflows the TALL dock
## (that dock holds this sheet comfortably, which is exactly what `_assert_compose_in_zone` asserts one
## state earlier), so a staged one is the only way the two answers can differ at all.
func _stage_impossible_compose_mark() -> float:
	var staged: float = get_viewport().get_visible_rect().size.y * IMPOSSIBLE_MARK_VIEWPORTS
	_hud._bandpanel._party_compose_needed = staged
	return staged

## **A MARK LATCHED IN ONE BOX MUST NOT SURVIVE A MOVE TO ANOTHER.** The requirement is a high-water
## mark that never falls during a composing act — which is right, since a mark tracking every shrink
## would hop the sheet back into the zone as a field cleared — so a mark CARRIED ACROSS a dock change
## keeps a sheet floating in a column that was never measured. Read right after a real `set_dock` +
## render, so what it judges is the shipped path's outcome.
func _assert_mark_dropped_on_dock_change(state_name: String, staged: float) -> void:
	var box: Vector2 = _hud._bandpanel._parties_zone_box_known()
	_assert_band_panel("%s — precondition: the new dock states a box (%s)" % [state_name, box],
		box != Vector2.ZERO)
	_assert_band_panel(("%s — precondition: the staged mark (%.0fpx) WOULD float in this %.0fpx "
		+ "box, so dropping it is what decides the fork") % [state_name, staged, box.y], staged > box.y)
	_assert_band_panel("%s — the mark from the SHORT dock did not survive the move (now %.0fpx)" % [
		state_name, _hud._bandpanel._party_compose_needed],
		_hud._bandpanel._party_compose_needed < staged)
	_assert_band_panel("%s — …so the tall dock keeps its sheet in the zone" % state_name,
		not _hud._bandpanel.compose_is_floating()
			and _find_meta_control(_panel, HudWidgets.SEND_HUNT_CONFIRM_META) != null)

## **THE MAP STILL TAKES THE PRESSES BESIDE THE FLOAT.** `BandComposeFloat` is deliberately the card
## and NOTHING more — no full-screen catcher — because the dock's sheet stays open through a map pick
## and a catcher would eat the very click the quarry picker needs. That is a behavioural claim and a
## PNG is pixel-identical either way, so it is driven through the real dispatch with `Viewport.push_input`,
## exactly as `_assert_open_strip_reaches_the_map` drives the open strip. Reading the float's
## `mouse_filter` back would only say what the node was configured as, not what the Viewport does with it.
func _assert_float_leaves_the_map_clickable(state_name: String) -> void:
	var floater: BandComposeFloat = _hud._bandpanel.compose_float()
	if floater == null or not _hud._bandpanel.compose_is_floating():
		_fail("%s — no float, so the click-through probe proves nothing" % state_name)
		return
	var box := floater.get_global_rect()
	var failures: Array[String] = []
	# PRECONDITION: the probe fires at all. The bare canvas beside the float — a full float width
	# outboard of it, on the float's own rows — is map in the live client and decoration here.
	var open_ground := Rect2(Vector2(box.end.x + FLOAT_PROBE_GAP, box.position.y),
		Vector2(get_viewport().get_visible_rect().size.x - box.end.x - FLOAT_PROBE_GAP, box.size.y))
	var reached := 0
	for point in _rect_probe_points(open_ground):
		if await _press_reaches_map(_canvas_to_window(point)):
			reached += 1
	if reached == 0:
		failures.append("no press in the %s band beside the float reached _unhandled_input, so this probe proves nothing" % str(open_ground))
	# THE CLAIM: the float eats its OWN rect and only its own.
	for point in _rect_ring_probe_points(box):
		if await _press_reaches_map(_canvas_to_window(point)):
			failures.append("a press at %s on the float itself fell through to the map's input path" % point)
			break
	# …and one canvas pixel outboard of its leading edge is already map again.
	for row in [box.position.y + PROBE_RECT_INSET, box.get_center().y, box.end.y - PROBE_RECT_INSET]:
		var just_outside := Vector2(box.end.x + FLOAT_EDGE_PROBE_OFFSET, row)
		if not await _press_reaches_map(_canvas_to_window(just_outside)):
			failures.append("a press at %s, just outboard of the float, never reached the map's input path" % just_outside)
			break
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the float eats its own rect and nothing else (%d/%d open-ground presses reached the map)" % [
			state_name, reached, _rect_probe_points(open_ground).size()])
		return
	for failure in failures:
		_fail("%s — %s" % [state_name, failure])

## How far outboard of the float the open-ground probe band starts, and where the "one pixel out is
## already map" samples sit. Both in canvas px; the second is deliberately just past the float's edge,
## since the claim is about the float's own rect and not about some comfortable distance from it.
const FLOAT_PROBE_GAP := 8.0
const FLOAT_EDGE_PROBE_OFFSET := 3.0

## How many viewport heights `_stage_impossible_compose_mark` asks for. Any multiple above 1 is past
## every box a dock can offer (a zone box is a fraction of the window); 4 leaves the claim legible in
## the printed numbers rather than sitting a pixel over the line.
const IMPOSSIBLE_MARK_VIEWPORTS := 4.0

## The party `band_panel_compose_hunt` is seeded to before it arms autofill — the stepper's own floor,
## i.e. the smallest party the form can express, so the fill has somewhere to move FROM whatever the
## states above left behind. `HudConst.WORKER_STEP` rather than a literal 1: it is the step the sheet's
## own `clampi` floors on, so the seed cannot drift out from under that clamp.
const COMPOSE_HUNT_SEED_PARTY := HudConst.WORKER_STEP

## GUARD: **THE PARTY CAP IS RESOLVED BEFORE THE FLOOR CHART IS COMPOSED** (`labor-ui.md` → "THE CAP IS
## RESOLVED BEFORE THE CHART ON BOTH SHEETS"). The chart's projection, its two crew targets and its
## verdict are all read against a CREW, so a sheet that composes the model ahead of its own
## `clampi`/autofill states a verdict for a party the stepper beneath then refuses to show.
##
## **IT CANNOT BE A PICTURE, and that is why it is here.** The disagreement lasts exactly one frame —
## the render on which autofill arms — and the next rerender resolves it, so a capture taken after the
## settle shows a chart and a stepper that have already been reconciled. What can see it is the two
## RENDERED numbers compared against each other: `HarvestFloorChart.crew()` (read off the live model,
## so a chart refreshed in place cannot answer staler than it draws) against the stepper row's
## `PARTY_STEPPER_COUNT_META` (the count the row was BUILT with, hence exactly the digit on screen).
## Neither side is a controller field, so the claim survives a sheet that clamps its member correctly
## and still hands the old number to the chart.
##
## The VACUITY guard rides first: autofill must really have moved the party off `seeded`, or the two
## numbers agree for free and the ordering is untested.
func _assert_chart_reads_the_settled_party(state_name: String, seeded: int) -> void:
	var surface := _compose_surface()
	var chart := _find_meta_control(surface, HudWidgets.FLOOR_CHART_META)
	var stepper := _find_meta_control(surface, HudWidgets.PARTY_STEPPER_COUNT_META)
	if chart == null or stepper == null:
		_fail("%s renders no %s — the cap-before-chart claim cannot be made" % [
			state_name, "floor chart" if chart == null else "party stepper"])
		return
	var settled := int(stepper.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META))
	var drawn_for := (chart as HarvestFloorChart).crew()
	_assert_band_panel("%s — autofill moved the party off its seed (%d → %d), so the order is testable"
			% [state_name, seeded, settled], settled != seeded)
	_assert_band_panel("%s — the chart is drawn for the party the stepper shows (chart %d, stepper %d)"
			% [state_name, drawn_for, settled], drawn_for == settled)

## GUARD: the dock hunt sheet's floor CHART is gated on the zone having room — present at TALL, absent
## at SHORT, where the parties zone is height-capped and clips. **Both halves are asserted**: a gate
## that never fires and a gate stuck on are both green to the bounds assertion, since a clipped chart
## still sits inside the zone rect.
func _assert_hunt_sheet_chart(want: bool, state_name: String) -> void:
	var chart := _find_meta_control(_compose_surface(), HudWidgets.FLOOR_CHART_META)
	var tier := _band_zone_tier_name()
	if want and chart == null:
		_fail("%s (%s tier) renders NO floor chart — the tier gate is stuck off" % [
			state_name, tier])
		return
	if not want and chart != null:
		_fail("%s (%s tier) renders a floor chart — the tier gate is stuck on" % [
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
	var surface := _compose_surface()
	var picker := _find_meta_control(surface, HudWidgets.POLICY_RUNG_META)
	# The rung's own meta rides the BUTTON; the grid that lays the three of them out is its
	# grandparent (button → cell `MarginContainer` → grid), and the GRID is what can wrap.
	var grid: Control = picker.get_parent().get_parent() as Control if picker != null else null
	if grid != null:
		print("band_panel_preview: %s — floor picker grid needs %.0fpx of a %.0fpx column (%d columns, %d rungs)" % [
			state_name, grid.get_combined_minimum_size().x, grid.size.x,
			(grid as GridContainer).columns if grid is GridContainer else -1,
			grid.get_child_count()])
	var chart := _find_meta_control(surface, HudWidgets.FLOOR_CHART_META)
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

## THE STANDING knowledge row every state outside the faction block renders against — the four
## rung-transition tracks fully learned, which is what the rung-ready board needs. It is a function
## rather than a literal at its two call sites because the faction block REPLACES the row (a push
## overwrites a faction's whole row) and has to put this exact one back afterwards.
func _standing_knowledge_row() -> Dictionary:
	return {"faction": 0, "cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0}

## THE KNOWLEDGE ZONE's craft tracks at the ladder's CEILING — all five, one of them FINISHED so the
## `known` word renders beside four live meters. Five is the most rows that block can ever draw
## (`FactionReadouts.KNOWLEDGE_TRACK_LABELS` is the whole ladder), which is what makes the zone's
## measured extent the worst case rather than a sample.
func _faction_knowledge_fixture() -> Dictionary:
	return {"faction": 0, "cultivation": 1.0, "seed_selection": 0.62, "herding": 0.41,
		"penning": 0.28, "foddering": 0.07}

## THE KNOWLEDGE ZONE's discovered sites: **MORE distinct kinds than `FACTION_LIST_ROWS_MAX` shows**,
## so the `+N more` row is inside the measurement and the block is staged at the tallest it can ever
## draw — which is what makes the zone's measured extent a worst case rather than a sample. One kind
## is found TWICE, so the head's INSTANCE total (8) and the row count (7) are DIFFERENT numbers; that
## gap is the block's own claim, and a rollup that collapsed the two would read the same figure in
## both places and pass a test that only counted rows.
func _faction_discoveries_fixture() -> Dictionary:
	return {"faction": 0, "sites": [
		{"site_id": "great_peak", "display_name": "Great Peak", "x": 61, "y": 12, "glyph": "⛰"},
		{"site_id": "great_peak", "display_name": "Great Peak", "x": 44, "y": 30, "glyph": "⛰"},
		{"site_id": "sky_arch", "display_name": "Sky Arch", "x": 70, "y": 22, "glyph": "⛰"},
		{"site_id": "salt_spring", "display_name": "Salt Spring", "x": 66, "y": 26, "glyph": "💧"},
		{"site_id": "bone_field", "display_name": "Bone Field", "x": 58, "y": 33, "glyph": "🦴"},
		{"site_id": "sky_lake", "display_name": "Sky Lake", "x": 52, "y": 19, "glyph": "💧"},
		{"site_id": "black_glass", "display_name": "Black Glass", "x": 48, "y": 41, "glyph": "◈"},
		{"site_id": "singing_cave", "display_name": "Singing Cave", "x": 63, "y": 37, "glyph": "◈"},
	]}

## THE FACTION PAGE's roster — TWO resident bands and one detached party.
##
## **Two bands is the fixture's whole point.** Every total this page prints is a sum, and on a
## one-band faction a sum and its single term are the same number: a page that had stopped summing
## entirely would render identically and every assertion below would pass. The party is here so the
## parties zone has a row to name a home band with, and so the WORKFORCE bar carries its Parties
## segment.
##
## The two bands share `_band_fixture`'s age brackets deliberately — see `_assert_faction_page`, where
## that is what makes the apportionment claim a real discriminator rather than an arithmetic identity.
func _faction_roster() -> Array:
	var second := _concerning_food_band_fixture()
	# **THE SECOND BAND KEEPS A HERD**, the corralled aurochs `_under_herded_work_herd_fixtures` stages.
	# Every other fixture band in this file hunts WILD game, so on an all-wild roster no band on this
	# page pays a pen's feed — and the pen feed below is a real term of `band_net_food`, which is the
	# headline the `band` zone's Food row is built on.
	second["labor_assignments"] = [
		{"kind": "hunt", "workers": 4, "fauna_id": UNDER_HERDED_WORK_HERD_ID, "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 0.30, "sustainable_yield": 0.30},
	]
	# …and PAYS ITS PEN'S FEED. It renders no row of its own — the Food block states a stock and a rate,
	# not a ledger — but it is a real term of `band_net_food`, so without it the headline net on this
	# page would be the one figure a live pen-keeping faction never sees. It is also what caught the
	# zone at **328px of its 300px box** when the rows were briefly a four-row ledger at the vitals type
	# size, which is why the fixture keeps paying it.
	second["pen_feed_upkeep"] = FACTION_PEN_FEED_UPKEEP
	# **IT IS A SMALLER BAND WITH WORSE MORALE, and both halves are load-bearing.** Population-weighted
	# and plain means agree exactly when every band is the same size, so a roster of two 30-person bands
	# makes `_assert_faction_weighted_morale` vacuous — it would pass under either rule. At 12 people and
	# 0.30 morale against 30 and 0.82 the two answers separate (67% weighted, 56% plain), which is the
	# only configuration in which that assertion says anything.
	second["size"] = FACTION_SECOND_BAND_SIZE
	second["morale"] = FACTION_SECOND_BAND_MORALE
	# The age brackets are scaled with it. They are a SECOND counting of the same band, and
	# `band_panel_people`'s own rule is that the two must agree — a band of 12 carrying a 30-person age
	# structure is a fixture no server can produce, and the PEOPLE assertion reads these floats.
	for bracket in ["age_children", "age_working", "age_elders"]:
		second[bracket] = float(second[bracket]) * FACTION_SECOND_BAND_SCALE
	second["working_age"] = int(round(float(second["working_age"]) * FACTION_SECOND_BAND_SCALE))
	return [_band_fixture(), second, _hunt_expedition_fixture()]

## The faction page's rendered claims: the total really is the faction's, the header names the right
## thing, and the two affordances a band's header carries are correctly OFF.
func _assert_faction_page() -> void:
	var bands: Array = []
	for entry in _faction_roster():
		if not bool((entry as Dictionary).get("is_expedition", false)):
			bands.append(entry)
	# **A CROSS-CHECK, composed here out of the fixtures' OWN floats rather than by asking the client
	# to sum them again.** `HudFormat.apportion_people` apportions to `roundi(Σ parts)`, so the page —
	# which sums the raw brackets across bands and apportions ONCE — must read `roundi(Σ)` = 61 on this
	# roster, where a page that apportioned each band first and added the results would read 30 + 30 =
	# 60, and a page still showing ONE band's people would read 30. Three distinguishable answers, which
	# is what makes this assertion worth making.
	var raw := 0.0
	for band_variant in bands:
		var band: Dictionary = band_variant
		raw += float(band.get("age_children", 0.0)) + float(band.get("age_working", 0.0)) \
			+ float(band.get("age_elders", 0.0))
	var expected := roundi(raw)
	var band_zone: Node = _panel._zones.get(BandCityPanel.ZONE_BAND)
	var work_zone: Node = _panel._zones.get(BandCityPanel.ZONE_WORK)
	var people := _zone_head_readout(band_zone, HudWorkVocab.ZONE_HEADER_PEOPLE) if band_zone != null else ""
	_assert_band_panel(
		"faction page: PEOPLE reads the whole faction (%d) — not one band's (%d), not the per-band sum (%d)" % [
			expected, roundi(raw / float(maxi(bands.size(), 1))), bands.size() * 30],
		people == str(expected))

	# **THE VITALS ARE THE BAND PAGE'S FIVE ROWS, ONE SCALE UP.** Asserted on the rendered BBCode rather
	# than on a frame: the rows are a `RichTextLabel`'s text, and a picture cannot tell a row that is
	# missing from one that scrolled. The KEYS are what is checked — their values are the aggregation
	# rules, which have their own assertions below.
	var vitals := _faction_vitals_text(band_zone)
	for row in [HudDisclosureVocab.DETAIL_ROW_FOOD, HudDisclosureVocab.DETAIL_ROW_TRADE,
			HudDisclosureVocab.DETAIL_ROW_KIT, HudDisclosureVocab.DETAIL_ROW_MORALE,
			HudDisclosureVocab.DETAIL_ROW_GROWTH]:
		# The KEY alone, not `key + ": "` — `detail_bbcode` splits the pair and emits the key into its
		# own `[cell]`, so the separator never survives into the rendered text.
		_assert_band_panel("faction page: the vitals carry the %s row" % row, vitals.contains(row))

	# **AN AGGREGATE WHERE ONE IS MEANINGFUL, AN ALERT WHERE IT IS NOT.** The roster's second band is
	# below the critical runway, so Food must carry the alert clause — and must NOT carry a faction
	# runway, which is the figure that would have hidden it behind two healthy bands.
	_assert_band_panel("faction page: Food alerts on the starving band",
		vitals.contains(HudWorkVocab.FACTION_ALERT_GLYPH))
	# The runway renders as a PARENTHETICAL (`78 (19 turns)`), and no faction row carries a parenthetical
	# of any kind — so the absence of `(` is the test. Matching on the unit word itself does not work:
	# `FOOD_RUNWAY_UNIT` is "turn", which is a substring of the `/turn` every rate on this row ends in.
	_assert_band_panel("faction page: Food states NO faction runway",
		not vitals.contains("("))

	# **THE KIT ROW CARRIES NO DURABILITIES**, a mean of three per band describing no band that exists.
	# Asserted as an ABSENCE against the fixture's own spear condition, which the band page's Kit row
	# would print — so a Kit row that quietly went back to summarising fails here.
	_assert_band_panel("faction page: Kit states no faction durability",
		not vitals.contains(str(KIT_SHARED_SPEARS_CONDITION)))

	# **MORALE IS POPULATION-WEIGHTED, and the fixture is built so that a PLAIN mean gives a different
	# answer** — otherwise the weighting is asserted by a number it would produce either way.
	_assert_faction_weighted_morale(vitals)

	# **THE TYPE SCALE IS ASKED OF THE KNOWLEDGE ZONE, not the work one, and that is where the page's
	# stat rows now live.** The work zone's own rows are `build_inline_link` BUTTONS (a row's name jumps
	# to its band), so the Label-pair walk finds heads there and no rows at all — which is exactly what
	# it reported the moment the tracks moved out, and a claim that measures nothing is worse than none.
	_assert_faction_type_scale(_panel._zones.get(BandCityPanel.ZONE_KNOWLEDGE))
	# **AND THAT SIZE IS THE `band` ZONE'S VITALS ROWS', READ OFF THE LIVE LABEL.** The const above is
	# Godot's stock default written down — the vitals `RichTextLabel` carries no size override at all —
	# so an assertion against the const alone says nothing if the engine default ever moves. This is
	# the claim as it was actually made: the other zones' rows are the size of the Food/Trade/Kit/
	# Morale/Growth lines, not a number that happens to match them today.
	_assert_faction_row_size_matches_vitals(band_zone)

	# **A CARET MUST OPEN ITS POPOVER, NOT CHANGE THE PANEL'S SUBJECT.** Reported from play: every
	# faction caret jumped straight to a band. The disclosure re-renders its hosts so the caret can
	# flip ▸→▾, and that re-render rendered a BAND unconditionally — the page keeps `panel_band()`
	# intact for the cycler, so nothing stopped it. Driven through the REAL `meta_clicked` with the
	# very meta the row's own text carries, because the bug was in the re-render and not in the row.
	_assert_faction_caret_keeps_the_page()

	# THE HEADER. A faction has no settlement stage and no tile, so the stage slot carries the band
	# COUNT — the identity fact at this scale — and the coordinate slot hides itself outright.
	_assert_band_panel("faction page: header names the faction",
		_panel._name_label.text == HudFormat.FACTION_PAGE_NAME)
	_assert_band_panel("faction page: header states the band count where a band states its stage",
		_panel._stage_label.text == HudFormat.faction_bands_label(bands.size()))
	_assert_band_panel("faction page: header states NO coordinates", not _panel._position_label.visible)
	_assert_band_panel("faction page: the cycler reads 1 / %d (pinned FIRST)" % (bands.size() + 1),
		_panel._count_label.text == "1 / %d" % (bands.size() + 1))

	# THE JUMP AFFORDANCE IS OFF, asserted BEHAVIOURALLY. Reading `mouse_filter` back would only say
	# what the setter was handed; driving the REAL handler with a REAL press says what a click does.
	var jumps := [0]
	var on_jump := func(): jumps[0] += 1
	_panel.subject_activated.connect(on_jump)
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	_panel._on_subject_gui_input(press)
	_panel.subject_activated.disconnect(on_jump)
	_assert_band_panel("faction page: a press on the header emits no jump", jumps[0] == 0)
	_assert_band_panel("faction page: the header's jump tooltip is gone with the jump",
		_panel._subject_cluster.tooltip_text == "")

	# **THE WHOLE TAB STRIP, BY EQUALITY** — the subject declares its own zone keys AND its own labels
	# (`BandCityPanel.set_zone_layout`), so what is asserted is the layout it declared and not a single
	# renamed word. This replaced a claim about `set_tab_label`, the per-zone label OVERRIDE that
	# existed solely to turn `Band` into `Faction`; asserting only that one word would now pass on a
	# page that had lost its `Know` tab entirely.
	var tabs: Array[String] = []
	for zone in _panel._zone_order():
		tabs.append(_panel._tab_label_text(zone))
	_assert_band_panel("faction page: the narrow shell's tabs read %s" % str(FACTION_TAB_LABELS),
		tabs == FACTION_TAB_LABELS)

	# The parties zone names the band each party LEFT — the "where they are" half of the rollup that a
	# one-line summary row can honestly carry.
	var parties_zone: Node = _panel._zones.get(BandCityPanel.ZONE_PARTIES)
	# `_has_label_containing` walks Labels only, and a summary row's name is a `build_inline_link`
	# BUTTON — it has to be, since clicking it jumps to that band — so the search has to know about
	# both. That is the whole difference between this row and the stat row it replaced.
	_assert_band_panel("faction page: a party row names the band it left",
		parties_zone != null and _has_text_containing(parties_zone, HudFormat.band_display_name({}, 1)))
	_assert_faction_party_row_jumps_home(parties_zone)

## **THE PARTIES ROW'S NAME LINK GOES WHERE THE NAME SAYS.** The row is named for the band the party
## LEFT and the link used to be bound to the PARTY's entity, so a link reading `Band 2` selected the
## expedition — the row named one subject and delivered another. No frame can carry this: both
## renderings draw the identical row, and the difference is only in what a press does.
##
## Driven through the row's REAL `pressed` handler, and the link is found STRUCTURALLY (a summary row
## is flag Label → name Button → body Button, so the row's FIRST button is the name) rather than by its
## face — the face is the very thing under test, so matching on it could only confirm the assumption.
##
## **It leaves the panel on a BAND, so it puts the page back.** Every state below this one is rendered
## as the faction page.
func _assert_faction_party_row_jumps_home(parties_zone: Node) -> void:
	var link := _faction_summary_name_link(parties_zone)
	if link == null:
		_assert_band_panel("faction page: the party row's name link is reachable", false)
		return
	var home := int(_hunt_expedition_fixture().get("home_band_entity", -1))
	var party := int(_hunt_expedition_fixture().get("entity", -1))
	link.emit_signal("pressed")
	# **TWO CLAIMS, because the two failures look nothing alike and one of them is silent.** Binding the
	# PARTY's entity to this link routes it through `jump_to_band_entity`, which cannot resolve a party
	# in the band roster and NO-OPS — leaving the page up with the previous subject still under it, so a
	# subject-only assertion reads the right entity for entirely the wrong reason.
	var left_page := not _hud._bandpanel._panel_is_faction
	var subject := int(_hud._bandpanel._band_labor.panel_band().get("entity", -1))
	_assert_band_panel(
		"faction page: the party row's NAME leaves the page — a jump is a subject change", left_page)
	_assert_band_panel(
		"faction page: the party row's NAME selects its HOME BAND (%d), not the party (%d) — got %d" % [
			home, party, subject],
		left_page and subject == home)
	# Back to the page for the states that follow. Conditional so a build whose link no-ops does not
	# cycle OFF the page it never left and take every state below down with it — the restore is
	# housekeeping, and the claim about it is the assertion under this line.
	if left_page:
		_hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	_assert_band_panel("faction page: the page is restored after the party-row jump",
		_hud._bandpanel._panel_is_faction)

## A faction SUMMARY row's name link: the FIRST `Button` of the first `HBoxContainer` holding two of
## them. `_summary_row` builds exactly that shape — a fixed-width flag Label, the name link, then the
## body link — and the two buttons are what tells a summary row from every other row on the page.
func _faction_summary_name_link(node: Node) -> Button:
	if node == null:
		return null
	if node is HBoxContainer:
		var buttons: Array = []
		for child in node.get_children():
			if child is Button:
				buttons.append(child)
		if buttons.size() >= 2:
			return buttons[0] as Button
	for child in node.get_children():
		var found := _faction_summary_name_link(child)
		if found != null:
			return found
	return null

## The ROUTING claims — reached by driving the real cycler, since none of them is visible in a frame.
##
## **The camera claim is asserted as a PAIR.** "Cycling onto the faction page recentred nothing" is
## satisfied by a cycler that had stopped recentring at all, so the walk back onto a band — which MUST
## recentre, that being decision 2 of `docs/plan_band_city_dock.md` — is what makes the first half mean
## something.
func _assert_faction_cycler() -> void:
	# **START FROM A KNOWN BAND.** The frames above leave the panel ON the page, and `◀` from there
	# wraps to the LAST band — so a walk that assumed a band start asserted the opposite of what it
	# meant and passed nothing. Landing on the first band explicitly makes the pair below a claim about
	# the cycler rather than about whatever the previous state happened to leave up.
	_hud.cycle_panel_band(BandCityPanel.CYCLE_NEXT)
	_assert_band_panel("faction cycler: the walk starts from a band (the frames above left the page up)",
		not _hud._bandpanel._panel_is_faction)
	var focuses := [0]
	var counter := func(_x: int, _y: int): focuses[0] += 1
	_hud.alert_focus_requested.connect(counter)
	_hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	_assert_band_panel("faction cycler: ◀ from the first band lands on the pinned page",
		_hud._bandpanel._panel_is_faction)
	_assert_band_panel("faction cycler: cycling ONTO the page moves no camera (%d focus requests)" % focuses[0],
		focuses[0] == 0)
	_hud.cycle_panel_band(BandCityPanel.CYCLE_NEXT)
	_assert_band_panel("faction cycler: ▶ walks back onto a band",
		not _hud._bandpanel._panel_is_faction)
	_assert_band_panel("faction cycler: cycling onto a BAND still recenters (the paired positive)",
		focuses[0] == 1)
	_hud.alert_focus_requested.disconnect(counter)

	# A SNAPSHOT LEAVES THE PAGE UP. Its totals are exactly what a tick moves, so a tick is when the
	# page must RE-RENDER — never when it hands the panel back to a band under the player.
	_hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	_push_bands(_faction_roster())
	_assert_band_panel("faction cycler: the page survives a snapshot",
		_hud._bandpanel._panel_is_faction)

	# …and the panel is left on a BAND, or every state below this one re-renders as the rollup on its
	# next `_push_bands`.
	_hud.cycle_panel_band(BandCityPanel.CYCLE_NEXT)
	_assert_band_panel("faction cycler: left on a band for the states that follow",
		not _hud._bandpanel._panel_is_faction)

## CLICKING A FACTION CARET OPENS ITS POPOVER AND LEAVES THE PAGE UP.
##
## Both halves are the claim. "The popover opened" alone passes on a build that opens it and then
## renders a band behind it; "the page survived" alone passes on a caret that does nothing at all.
##
## It drives the REAL `meta_clicked` with the meta the row's own text carries — the same idiom
## `_click_disclosure` uses for a band row — because the defect was in the RE-RENDER the click
## triggers, not in the row, so poking `_open_popover` directly would have proved nothing.
func _assert_faction_caret_keeps_the_page() -> void:
	var meta := "%s%s" % [HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX,
		DetailFormat.breakdown_key(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, {})]
	# **WITH A BAND SELECTED, which is the configuration the second half of this defect needs.** The
	# disclosure re-renders BOTH hosts, and the drawer's player-band branch re-asserts the selected band
	# as the panel's subject on every render — so a caret click stole the page whenever the selected hex
	# happened to hold a band. Reported from play AFTER the first fix, because with nothing selected the
	# drawer takes no band branch at all and the guard passed.
	#
	# **THE ORDER IS LOAD-BEARING: SELECT FIRST, THEN REACH THE PAGE.** A selection is the player's
	# explicit "make this band the subject" act and it correctly LEAVES the page (that is the other half
	# of the rule — a bare "not on the faction page" gate froze the panel on the rollup while a marker
	# click moved the map ring), so selecting AFTER the page is up would stage the opposite state and
	# this assertion would be about a band's own page. The play report's own order is this one.
	_hud.show_unit_selection(_faction_roster()[0])
	_hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	_assert_band_panel("faction caret: the setup reached the page with a band selected",
		_hud._bandpanel._panel_is_faction and not _hud._selection.unit().is_empty())
	# Re-read the vitals label AFTER that walk: reaching the page rebuilt every zone, so a handle taken
	# before it points at a freed tree.
	var label := _first_rich_text(_panel._zones.get(BandCityPanel.ZONE_BAND))
	if label == null:
		_assert_band_panel("faction caret: the vitals label is reachable", false)
		return
	label.meta_clicked.emit(meta)
	_assert_band_panel("faction caret: the page survives its own caret (still the faction subject)",
		_hud._bandpanel._panel_is_faction)
	_assert_band_panel("faction caret: the Food popover is open",
		_hud._disclosures._breakdown_popover_key == DetailFormat.breakdown_key(
			HudDisclosureVocab.BREAKDOWN_KIND_FOOD, {}))
	# …and the popover lists the BANDS, which is the whole point of the drill-down.
	_assert_band_panel("faction caret: the popover lists the bands",
		_hud._disclosures._breakdown_popover_label != null
			and _hud._disclosures._breakdown_popover_label.text.contains(
				HudDisclosureVocab.FACTION_BAND_JUMP_META_PREFIX))
	_hud._disclosures._close_popover()

## The first `RichTextLabel` under a node — the faction zone's vitals block, which is its only one.
func _first_rich_text(node: Node) -> RichTextLabel:
	if node is RichTextLabel:
		return node as RichTextLabel
	for child in node.get_children():
		var found := _first_rich_text(child)
		if found != null:
			return found
	return null

## Does any Label OR Button under `node` carry `text`? The faction summary rows are built from
## `HudWidgets.build_inline_link`, which returns a `Button` — so a Label-only walk cannot see a band's
## name on one, and the row's name has to be a button because clicking it jumps to that band.
func _has_text_containing(node: Node, text: String) -> bool:
	if node is Label and (node as Label).text.contains(text):
		return true
	if node is Button and (node as Button).text.contains(text):
		return true
	for child in node.get_children():
		if _has_text_containing(child, text):
			return true
	return false

## The faction vitals block's RAW BBCode — the `[url]` metas and the `[color]` tags included, since
## some claims are about a link existing and some about a number being absent. The first
## `RichTextLabel` under the zone is it; the zone has no other.
func _faction_vitals_text(zone: Node) -> String:
	if zone is RichTextLabel:
		return (zone as RichTextLabel).text
	for child in zone.get_children():
		var found := _faction_vitals_text(child)
		if found != "":
			return found
	return ""

## MORALE IS POPULATION-WEIGHTED, asserted against the PLAIN mean the same fixture would give.
##
## **The two must differ, or the claim is satisfiable by either rule.** The roster is built for it —
## its two bands differ in BOTH size and morale — so a plain mean lands on one number and the weighted
## one on another, and the assertion names both. This is the only thing in the harness that can see
## `FactionAggregate`'s weighting at all; every other reading it feeds is a single band's own value.
func _assert_faction_weighted_morale(vitals: String) -> void:
	var bands: Array = []
	for entry in _faction_roster():
		if not bool((entry as Dictionary).get("is_expedition", false)):
			bands.append(entry)
	var plain := 0.0
	var weighted := 0.0
	var weight := 0.0
	for band_variant in bands:
		var band: Dictionary = band_variant
		var morale := float(band.get("morale", 0.0))
		var size := float(band.get("size", 0))
		plain += morale
		weighted += morale * size
		weight += size
	var plain_pct := int(round((plain / float(bands.size())) * 100.0))
	var weighted_pct := int(round((weighted / weight) * 100.0))
	if plain_pct == weighted_pct:
		_assert_band_panel(
			"faction morale: the fixture must separate weighted from plain (both read %d%%)" % plain_pct,
			false)
		return
	_assert_band_panel("faction morale: population-weighted (%d%%), not a plain mean (%d%%)" % [
			weighted_pct, plain_pct],
		vitals.contains("%d%%" % weighted_pct) and not vitals.contains("%d%%" % plain_pct))

## THE PAGE'S TYPE SCALE: every zone head at `ALLOC_SECTION_FONT_SIZE`, every row at the work board's
## `WORK_ROW_FONT_SIZE` — the page's claim is that it uses the board's scale, so that is what is
## asserted, by EQUALITY against those constants.
##
## **This shipped wrong in both directions and neither was catchable by anything else here.** First the
## rows were pinned at 12, four steps under the surface they were meant to match; then, correcting that
## against the band zone's head-LESS vitals label, they came out at ~16 under a 10pt head — so `FOOD`
## rendered smaller than the `Larder` it labels. Both were reported by eye. A mis-sized Label sits
## inside its zone rect and fits its box, so the bounds and content-fits assertions pass on either; and
## at this harness's canvas scale the difference is a few pixels, so a frame does not carry it either.
##
## It reads the RENDERED size (`get_theme_font_size`), not the override, so "set no override and take
## the stock default" — which is exactly how the second version went wrong — is measured as what it
## actually draws at rather than as an absent property.
func _assert_faction_type_scale(zone: Node) -> void:
	# **A NULL ZONE IS A REPORTED FAILURE, NOT A CRASH.** The walk below dereferences the node, and an
	# unhandled error inside this harness's one long `await`ing `_ready()` ABORTS the whole run — so a
	# subject that had stopped declaring the zone this reads would take every later state with it
	# instead of naming itself.
	if zone == null:
		_assert_band_panel("faction page: the type-scale zone was declared", false)
		return
	var heads: Array = []
	var rows: Array = []
	_collect_faction_type_sizes(zone, heads, rows)
	if heads.is_empty() or rows.is_empty():
		_assert_band_panel("faction page: type scale is measurable (%d heads, %d rows)" % [
			heads.size(), rows.size()], false)
		return
	# **BY EQUALITY AGAINST THE NAMED SIZES, NEVER AS AN INEQUALITY BETWEEN THEM.** The first cut of this
	# asserted "no head is LARGER than its rows" and was decorative: 10 over 13 is the correct
	# relationship *and* 10 over 16 is the reported bug, so the test passed on the very defect it was
	# written for (verified by sabotage — it printed `largest head 10, smallest row 16` and PASSED).
	# The direction was never wrong; the MAGNITUDE was, and only the constants can say so.
	var stray_heads: Array = []
	for size in heads:
		if int(size) != HudWorkVocab.ALLOC_SECTION_FONT_SIZE:
			stray_heads.append(int(size))
	_assert_band_panel("faction page: every zone head renders at %d (%d stray: %s)" % [
			HudWorkVocab.ALLOC_SECTION_FONT_SIZE, stray_heads.size(), str(stray_heads)],
		stray_heads.is_empty())
	var stray_rows: Array = []
	for size in rows:
		if int(size) != HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE:
			stray_rows.append(int(size))
	_assert_band_panel("faction page: every row renders at the vitals rows' %d (%d stray: %s)" % [
			HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE, stray_rows.size(), str(stray_rows)],
		stray_rows.is_empty())

## THE PAGE'S ROW SIZE IS THE VITALS ROWS', asked of the two RENDERED surfaces rather than of the
## constant between them. The `band` zone's vitals are a bare `RichTextLabel` with no size override,
## so what they draw at is the engine's default — and a stat row pinned to a literal that merely
## equals that default today would drift silently the day it changes.
func _assert_faction_row_size_matches_vitals(band_zone: Node) -> void:
	var vitals := _first_rich_text(band_zone)
	if vitals == null:
		_assert_band_panel("faction page: the vitals label is reachable for the size comparison", false)
		return
	var vitals_size := vitals.get_theme_font_size("normal_font_size")
	_assert_band_panel("faction page: a stat row (%d) is the size of the vitals rows (%d)" % [
			HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE, vitals_size],
		vitals_size == HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE)

## Split a zone's Labels into `zone_head` TITLES and stat-row cells, by the structure each is built
## with: a head's first Label is UPPERCASED (`HudWidgets.alloc_section_label`) and a stat row's is not.
func _collect_faction_type_sizes(node: Node, heads: Array, rows: Array) -> void:
	if node is HBoxContainer:
		var labels: Array = []
		for child in node.get_children():
			if child is Label:
				labels.append(child)
		if labels.size() >= 2:
			var lead: Label = labels[0]
			var is_head := not lead.text.is_empty() and lead.text == lead.text.to_upper()
			for label_variant in labels:
				var label: Label = label_variant
				if is_head:
					heads.append(label.get_theme_font_size("font_size"))
				else:
					rows.append(label.get_theme_font_size("font_size"))
	for child in node.get_children():
		_collect_faction_type_sizes(child, heads, rows)

## THE KNOWLEDGE ZONE's three blocks, each asserted through the thing only it can say.
##
## **EVERY BLOCK OMITS ITSELF WHEN ITS DATA IS ABSENT, so "the zone rendered" is not the claim** — a
## zone that had lost two of its three blocks passes both geometric assertions comfortably, an empty
## box fitting anything. What is asserted is that each block's own heading AND a row only that block
## produces are on screen.
func _assert_faction_knowledge_zone() -> void:
	var zone: Node = _panel._zones.get(BandCityPanel.ZONE_KNOWLEDGE)
	if zone == null:
		_assert_band_panel("faction knowledge: the zone exists (the subject declared four)", false)
		return
	# SETTLING. The row is keyed by the STAGE, never by a word restating the head — so the stage from
	# `_ready`'s top-bar seed is what the key must read, and the meter's `62/100` is the value beside it.
	# SETTLING is a REAL HEAD with its reading on a ROW beneath it, like every other block here — so
	# both halves are asserted: the head is present (matched UPPER-CASED, since
	# `HudWidgets.alloc_section_label` upper-cases what it is given and the vocabulary const as written
	# matches nothing rendered) and the row is keyed by the STAGE and valued by the meter.
	_assert_band_panel("faction knowledge: SETTLING is a real head, not a row's key",
		_has_label_containing(zone, HudWorkVocab.FACTION_HEADER_SETTLING.to_upper()))
	# **THE KEY IS THE STAGE'S LABEL, NEVER THE WIRE TOKEN.** `SedentarizationStage::as_str()` spells the
	# stage `soft`, which is a database value; the row must render the player word the vocabulary maps it
	# to. Asserting the raw token here is what let a lowercase enum key ship on this row.
	var stage_label := String(HudWorkVocab.FACTION_SETTLING_STAGE_LABELS[TOPBAR_SEDENTARIZATION_STAGE])
	var settling := _faction_stat_value(zone, stage_label)
	_assert_band_panel("faction knowledge: the SETTLING row is keyed by the stage word '%s' (wire '%s') and reads %d/%d (got '%s')" % [
			stage_label, TOPBAR_SEDENTARIZATION_STAGE, int(round(TOPBAR_SEDENTARIZATION_SCORE)),
			HudWorkVocab.FACTION_SETTLING_SCALE, settling],
		settling.ends_with("%d/%d" % [int(round(TOPBAR_SEDENTARIZATION_SCORE)),
			HudWorkVocab.FACTION_SETTLING_SCALE]))
	# KNOWLEDGE. The fixture finishes exactly one track, so `known` and a live meter must BOTH render —
	# either alone passes on a block that renders one shape for every track.
	var finished := _faction_stat_value(zone, String(FactionReadouts.KNOWLEDGE_TRACK_LABELS["cultivation"]))
	var climbing := _faction_stat_value(zone, String(FactionReadouts.KNOWLEDGE_TRACK_LABELS["seed_selection"]))
	_assert_band_panel("faction knowledge: a FINISHED track reads '%s' (got '%s')" % [
			HudWorkVocab.FACTION_KNOWLEDGE_KNOWN, finished],
		finished == HudWorkVocab.FACTION_KNOWLEDGE_KNOWN)
	# **THE BAR MUST HAVE A FILLED CELL, and that half is not pedantry.** `HudFormat.meter_bar` grades a
	# 0–100 SCORE while a track's progress is 0..1, so a caller that forgets the scale fills zero cells
	# at every value under 0.5 — an empty meter beside a live `62%`, which is what BOTH blocks on this
	# page shipped. `ends_with("%")` alone passes on it comfortably.
	_assert_band_panel("faction knowledge: a track still climbing reads a FILLED meter and a percent (got '%s')" % climbing,
		climbing.ends_with("%") and climbing != HudWorkVocab.FACTION_KNOWLEDGE_KNOWN
			and climbing.contains(METER_FILLED_CELL))
	_assert_band_panel("faction knowledge: the SETTLING meter is filled too (got '%s')" % settling,
		settling.contains(METER_FILLED_CELL))
	# DISCOVERIES. The head counts INSTANCES and the rows are KINDS, and the fixture makes those two
	# DIFFERENT numbers on purpose — a block that collapsed them would read `4` in both places.
	var sites: Array = _faction_discoveries_fixture()["sites"]
	var kinds := {}
	for site_variant in sites:
		kinds[String((site_variant as Dictionary)["site_id"])] = true
	_assert_band_panel("faction knowledge: DISCOVERIES heads the INSTANCE count (%d), not the kind count (%d)" % [
			sites.size(), kinds.size()],
		_zone_head_readout(zone, HudWorkVocab.FACTION_HEADER_DISCOVERIES) == str(sites.size()))
	# The twice-found kind, named from the FIXTURE rather than by a literal, so the two cannot drift.
	var repeated := String((sites[0] as Dictionary)["display_name"])
	_assert_band_panel("faction knowledge: the twice-found kind '%s' reads its own count (2)" % repeated,
		_faction_stat_value(zone, repeated) == "2")
	# **THE CAP IS STATED, NEVER SILENT** — the fixture carries more kinds than the list shows, so the
	# `+N more` row must be there. A truncated list with nothing under it reads as the whole roster.
	_assert_band_panel("faction knowledge: the capped list states what it dropped (+%d more)" % (
			kinds.size() - HudWorkVocab.FACTION_LIST_ROWS_MAX),
		_has_label_containing(zone, HudWorkVocab.FACTION_LIST_MORE_FORMAT % (
			kinds.size() - HudWorkVocab.FACTION_LIST_ROWS_MAX)))
	# **EVERY STAT ROW RENDERS ITS KEY**, measured on the laid-out WIDTH rather than on `.text`: a
	# `clip_text` key Label is squeezed to nothing by the row's expanding spacer, so the block draws as a
	# column of right-aligned numbers with no names — and the text is set correctly in that build too, so
	# only the geometry can see it. It shipped that way once; both geometric assertions pass on it
	# comfortably, a zero-width Label being inside its zone and inside its box.
	#
	# **IT IS ASKED HERE RATHER THAN ON `_assert_faction_page`, and that is a constraint of the shell.**
	# The narrow shell parents ONLY the active tab's zone (`BandCityPanel._reparent_zones` DETACHES the
	# rest), so a zone read from another tab has never been laid out and every one of its rows measures
	# zero — the scan would report every row keyless. This is the state where the KNOWLEDGE tab is up and
	# its zone is in the tree, and it is also where all three of its blocks render, so it is the widest
	# set of stat rows the page ever lays out at once.
	var keyless := _faction_keyless_rows(zone)
	_assert_band_panel("faction knowledge: every stat row renders its key (%d keyless)" % keyless,
		keyless == 0)

## THE KNOWLEDGE ZONE'S HEIGHT TIER, on the height-capped horizontal dock: DISCOVERIES is dropped and
## the two blocks that survive are still there. **The second half is what stops this passing on a zone
## that rendered nothing at all**, which is the failure a gate is most likely to produce.
func _assert_faction_knowledge_tier() -> void:
	var zone: Node = _panel._zones.get(BandCityPanel.ZONE_KNOWLEDGE)
	if zone == null:
		_assert_band_panel("faction knowledge tier: the zone exists in the wide shell", false)
		return
	_assert_band_panel("faction knowledge tier: a ~300px box DROPS the DISCOVERIES block",
		not _has_label_containing(zone, HudWorkVocab.FACTION_HEADER_DISCOVERIES.to_upper()))
	_assert_band_panel("faction knowledge tier: …and KEEPS Settling and the craft tracks",
		_has_label_containing(zone, HudWorkVocab.FACTION_HEADER_SETTLING.to_upper())
			and _has_label_containing(zone, HudWorkVocab.FACTION_HEADER_KNOWLEDGE.to_upper()))

## THE FOUR-ZONE BODY ITSELF: the panel really is hosting four columns, in the declared order, and the
## KNOWLEDGE column takes the flank width the layout gave it.
##
## Asked of the wide shell's own HOSTS rather than of the layout array, because the layout is the
## INPUT: a `set_zone_layout` that accepted four specs and built three columns would satisfy any
## assertion made against `_zone_layout` and none made against the tree.
func _assert_faction_zone_layout() -> void:
	var hosts: Array[String] = []
	for child in _panel._wide_shell.get_children():
		if String(child.name).begins_with("Zone_"):
			hosts.append(String(child.name))
	_assert_band_panel("faction layout: the wide shell hosts FOUR zone columns in order (got %s)" % str(hosts),
		hosts == ["Zone_band", "Zone_work", "Zone_knowledge", "Zone_parties"])
	var box: Vector2 = _panel.zone_size(BandCityPanel.ZONE_KNOWLEDGE)
	_assert_band_panel("faction layout: the knowledge column is its declared %.0fpx flank (got %.0f)" % [
			BandCityPanel.ZONE_KNOWLEDGE_WIDTH, box.x],
		is_equal_approx(box.x, BandCityPanel.ZONE_KNOWLEDGE_WIDTH))

## THE FOUR-ZONE SHELL THRESHOLD, bracketed one pixel apart — the claim the whole generalization turns
## on, and the one the three-zone pair of frames above structurally cannot make.
##
## **A THRESHOLD LEFT AT THE THREE-ZONE VALUE IS INVISIBLE IN EVERY FRAME THIS HARNESS RENDERS.** The
## faction states sit on windows that clear 1569 comfortably, so a page that flipped wide 379px too
## early renders a perfectly plausible board there. Only a window BETWEEN the two thresholds can tell
## them apart, and this is that window.
##
## PNG-less: which shell a given width picks is a boolean, and both answers draw a plausible panel.
func _assert_faction_shell_threshold() -> void:
	var derived: float = _panel.wide_shell_min_width()
	# By EQUALITY against the widths restated in `FACTION_SHELL_MIN_WIDTH`, so the SEPARATOR COUNT is
	# pinned: three gaps between four columns, not the two a three-zone body has. That term is the one
	# the old `WIDE_SEPARATOR_SPAN` const hard-wired, and an off-by-one there is 25px — small enough to
	# survive a bracket that only tested "wide above, narrow below" against its own answer.
	_assert_band_panel("faction threshold: the four-zone derivation is %.0f (got %.0f)" % [
			FACTION_SHELL_MIN_WIDTH, derived],
		is_equal_approx(derived, FACTION_SHELL_MIN_WIDTH))
	var rail_span: float = _panel._rail_span()
	var at := int(ceil(derived + rail_span))
	print("band_panel_preview: faction shell threshold probes at %d / %d (threshold %.0f + rail span %.0f)" % [
		at - SHELL_THRESHOLD_UNDERSHOOT, at, derived, rail_span])
	await _pin_canvas(Vector2i(at - SHELL_THRESHOLD_UNDERSHOOT, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	_assert_shell_is_wide(false, "faction threshold (one pixel below)")
	await _pin_canvas(Vector2i(at, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	_assert_shell_is_wide(true, "faction threshold (exactly at)")
	# …and the wide shell it just entered must still give the board one readable column, which is the
	# invariant the threshold is DERIVED from rather than a second fact about it.
	_assert_work_zone_readable()

## A faction stat row's VALUE, found by its KEY. The row is an `HBoxContainer` whose first Label is the
## key and whose last is the value — the same structural shape `_zone_head_readout` reads, and it
## cannot collide with one: a head's title is UPPERCASED and a stat row's key is not.
func _faction_stat_value(node: Node, key: String) -> String:
	if node is HBoxContainer:
		var labels: Array = []
		for child in node.get_children():
			if child is Label:
				labels.append(child)
		if labels.size() >= 2 and (labels[0] as Label).text == key:
			return (labels[labels.size() - 1] as Label).text
	for child in node.get_children():
		var found := _faction_stat_value(child, key)
		if found != "":
			return found
	return ""

## How many of a zone's stat rows render a key too narrow to READ — the failure `clip_text` produces,
## which is invisible to every geometric assertion here. Measured on the WIDTH the label was laid out
## at, not on its `text`: the text is set correctly in both the working and the broken build, and it is
## the rendered column that differs.
##
## **IT IS COMPARED AGAINST THE TEXT'S OWN MEASURED WIDTH, NOT AGAINST ZERO, and that distinction is
## what makes it a test at all.** `clip_text` does NOT zero a `Label`'s minimum — Godot floors it at
## ONE PIXEL — so a `<= 0.0` scan reports a fully clipped column as perfectly healthy: verified by
## sabotage, which passed with `0 keyless` and the key squeezed to nothing. A key renders iff the row
## granted it at least the width its own font needs for its own string.
func _faction_keyless_rows(node: Node) -> int:
	var nameless := 0
	if node is HBoxContainer:
		var labels: Array = []
		for child in node.get_children():
			if child is Label:
				labels.append(child)
		if labels.size() >= 2:
			var key_label: Label = labels[0]
			if not key_label.text.is_empty() \
					and key_label.size.x < _label_text_width(key_label) - KEYLESS_KEY_WIDTH_TOLERANCE:
				nameless += 1
	for child in node.get_children():
		nameless += _faction_keyless_rows(child)
	return nameless

## The width this Label's own font needs for its own text, at the size it actually renders at. A label
## the row laid out narrower than this has had its key clipped away.
func _label_text_width(label: Label) -> float:
	var font := label.get_theme_font("font")
	if font == null:
		return 0.0
	return font.get_string_size(label.text, HORIZONTAL_ALIGNMENT_LEFT, -1.0,
		label.get_theme_font_size("font_size")).x

## A `HudWidgets.zone_head`'s right-hand readout, found by its TITLE — the head is an `HBoxContainer`
## whose first Label is the uppercased section name and whose last is the readout. Structural rather
## than a text match on the number itself, which is the very thing under test.
func _zone_head_readout(node: Node, title: String) -> String:
	if node is HBoxContainer:
		var labels: Array = []
		for child in node.get_children():
			if child is Label:
				labels.append(child)
		if labels.size() >= 2 and (labels[0] as Label).text == title.to_upper():
			return (labels[labels.size() - 1] as Label).text
	for child in node.get_children():
		var found := _zone_head_readout(child, title)
		if found != "":
			return found
	return ""

## Pass/fail reporting for this harness's assertions — the rung-ready ones among them — through
## `_fail`, the run's ONE sink, so a regression fails loudly in the run log AND is counted against the
## exit status rather than waiting to be noticed in a thumbnail. **Report a failure here or through
## `_fail` itself and nowhere else**: a bare `push_error` beside them prints the same line and counts
## for nothing, which is a red run reporting success.
func _assert_band_panel(label: String, ok: bool) -> void:
	if ok:
		print("band_panel_preview: PASS — ", label)
	else:
		_fail(label)

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

## THE YIELD SORT'S THIRD TIER (issue #449). `Sort by yield` ranks food, then trade, then FODDER, and
## the fodder tier is what stops a sown hay Field — which publishes `rate == 0.0` AND
## `trade_rate == 0.0` — landing among the rows paying nothing at all, i.e. below every trade-only
## wolf and off page one on a busy band. That is verbatim the failure the tiering was introduced to
## remove, one account further out.
##
## FOUR claims, one per boundary plus the one that says nothing else moved, because a single
## whole-order equality reports "the board is different" and names no cause. Neither the boundary
## claims nor a PNG can be swapped for the other: a board sorted any of these ways renders as a
## perfectly plausible board.
func _assert_work_sort_tiers() -> void:
	var controller = _hud._bandpanel
	var restore_sort: StringName = controller._work_sort
	controller._work_sort = HudWorkVocab.WORK_SORT_YIELD
	var order := _sorted_work_keys(controller, _work_sort_fixture_models())
	var trade_at := order.find(WORK_SORT_TRADE_ONLY_KEY)
	var fodder_at := order.find(WORK_SORT_FODDER_KEY)
	var dead_at := order.find(WORK_SORT_PAYS_NOTHING_KEY)
	# The food tier is asserted as a SLICE rather than by "the last food row is above the wolf": the
	# fodder tier must not be paid for by disturbing the order of the two tiers already there.
	var food_tier := order.slice(0, WORK_SORT_FOOD_TIER_ORDER.size())
	_assert_band_panel("work sort — every FOOD-paying source still leads, in its old order (%s)"
		% ", ".join(food_tier), food_tier == WORK_SORT_FOOD_TIER_ORDER)
	_assert_band_panel("work sort — the trade-only source still follows the food tier (`%s` at %d)"
		% [WORK_SORT_TRADE_ONLY_KEY, trade_at], trade_at == WORK_SORT_FOOD_TIER_ORDER.size())
	_assert_band_panel("work sort — a FODDER-only source ranks BELOW the trade-only one (%d vs %d: %s)"
		% [fodder_at, trade_at, ", ".join(order)], trade_at >= 0 and fodder_at > trade_at)
	_assert_band_panel("work sort — a FODDER-only source ranks ABOVE the rows paying nothing (%d vs %d: %s)"
		% [fodder_at, dead_at, ", ".join(order)], fodder_at >= 0 and dead_at > fodder_at)
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
		{"key": WORK_SORT_TRADE_ONLY_KEY, "label": "Hunt Grey Wolf", "kind": "hunt",
			"rate": 0.0, "trade_rate": 0.22},
		# The THIRD tier's pair (issue #449), and they only mean anything TOGETHER: a sown hay Field
		# pays neither food nor trade, so under the two-tier rule it sat at 0.0 among the rows paying
		# nothing at all and was separated from them by the `key` tiebreak alone. The barren row is
		# what makes "above the dead rows" falsifiable — without it the Field is last either way.
		{"key": WORK_SORT_FODDER_KEY, "label": "Forage (5, 5)", "kind": "forage",
			"rate": 0.0, "trade_rate": 0.0, "fodder_rate": WORK_SORT_FODDER_RATE},
		{"key": WORK_SORT_PAYS_NOTHING_KEY, "label": "Forage (6, 6)", "kind": "forage",
			"rate": 0.0, "trade_rate": 0.0, "fodder_rate": 0.0},
	]

## The tile the fixture's managed plant row sits on — only its label is read, so any coordinate does.
const WORK_SORT_TEND_TILE := Vector2i(8, 4)

## The three sources the TIER claims are made about, named because each assertion states which
## boundary it is asking about. The fodder rate is deliberately LARGER than the trade-only source's
## trade rate, so a comparator "fixed" into a raw cross-account magnitude sort ranks the hay Field
## above the wolf and fails the boundary claim rather than passing by luck.
const WORK_SORT_TRADE_ONLY_KEY := "hunt:wolf"
const WORK_SORT_FODDER_KEY := "forage:hay"
const WORK_SORT_PAYS_NOTHING_KEY := "forage:barren"
const WORK_SORT_FODDER_RATE := 0.40
## The food tier in the order it has always come out in — 0.60, 0.40, 0.30, then the 0.25 tie broken
## by `key` ascending. Stated so the fodder tier cannot be added by disturbing the two above it.
const WORK_SORT_FOOD_TIER_ORDER := ["forage:3,9", "hunt:boar_b", "forage:8,4", "forage:12,7", "hunt:boar_a"]

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
		_fail("herder-floor frame needs idle workers to gate on the source")
		return
	var found := false
	for model in _hud._bandpanel._work_source_models(band, idle):
		var m: Dictionary = model
		if String(m.get("herd_id", "")) != herd_id:
			continue
		found = true
		if not bool(m.get("under_herded", false)):
			_fail("expected under_herded on the Hunt row for %s" % herd_id)
		elif not bool(m.get("can_add", false)):
			_fail(("the under-herded row for %s disables its own `+` at %d "
				+ "workers with %d idle — the board flags the shed and refuses the fix")
				% [herd_id, int(m.get("workers", 0)), idle])
		else:
			print("band_panel_preview: assert OK — the under-herded row keeps its `+` live (crew %d > take-useful %d)"
				% [HERDER_FLOOR_HERDERS_NEEDED, HERDER_FLOOR_TAKE_USEFUL])
	if not found:
		_fail("no Hunt work row for %s" % herd_id)
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
		_fail("the compose stepper caps at %d, not the crew of %d"
			% [compose_cap, HERDER_FLOOR_HERDERS_NEEDED])
	elif not (row_below and not row_at):
		_fail(("the worked row does not gate at the crew of %d "
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
			_fail("expected under_herded on the Hunt row for %s" % herd_id)
		elif not String(m.get("marks", "")).contains(HudComposeVocab.OVERHUNT_FLAG):
			_fail("expected the ⚠ mark on the under-herded row for %s" % herd_id)
		elif not String(m.get("note", "")).contains("drifting off"):
			_fail("expected the drifting-off note on the under-herded row for %s" % herd_id)
		else:
			print("band_panel_preview: assert OK — under-herded Hunt row flags the shed (⚠ + note)")
	if not found:
		_fail("no Hunt work row for %s" % herd_id)

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
				_fail("%s expected row label '%s' but got '%s'" % [
					key, expected_labels[key], label])
			else:
				labels_seen += 1
		var glyph := String(m.get("rung_glyph", ""))
		if glyph != String(expected[key]):
			_fail("%s expected rung glyph '%s' but got '%s'" % [
				key, expected[key], glyph])
		elif glyph != "" and String(m.get("rung_tooltip", "")) == "":
			_fail("%s wears a rung glyph with no tooltip naming the rung" % key)
	for key in expected:
		if not seen.has(key):
			_fail("no work row for %s on the rung board" % key)
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
			_fail("rung mark '%s' carries no tooltip" % label.text)
			return
		if label.mouse_filter != Control.MOUSE_FILTER_PASS:
			_fail("rung mark '%s' has mouse_filter %d — PASS is the only value that both shows the tooltip and lets the row's click through" % [
				label.text, label.mouse_filter])
			return
	if marked == 0:
		_fail("no rung mark rendered in the panel (%d slots) — the mark is missing" % labels.size())
	else:
		print("band_panel_preview: assert OK — %d rung marks are hoverable (tooltip + PASS), %d wild slots bare" % [
			marked, labels.size() - marked])

## Every `Label` under `node`, in tree order — the read an assertion makes when its claim is about what
## the board actually RENDERED rather than about what a model answers.
func _collect_labels(node: Node, out: Array) -> void:
	if node is Label:
		out.append(node)
	for child in node.get_children():
		_collect_labels(child, out)

## The WORK head's FODDER sibling, found by its own TOOLTIP rather than by its face — on a band with one
## feed-paying source the row's rate and the header total render the SAME string, so a text match cannot
## say which of the two it found. `null` where the head renders none, which is the state a band growing
## no feed is asserted on.
func _work_head_fodder_total() -> Label:
	var labels: Array = []
	_collect_labels(_panel, labels)
	for label_variant in labels:
		var label: Label = label_variant
		if label.tooltip_text == HudWorkVocab.WORK_FODDER_TOTAL_TOOLTIP:
			return label
	return null

## **THE FODDER FACE, ON THE THREE SURFACES THE WORK ZONE COMPOSES ITSELF** (issue #449). None of them
## can be judged from a PNG: `+0.00` and `+0.40 fodder` are the same row at a thumbnail's size, and the
## header total and the row rate read identically here by construction.
##
## The HUNT row is asserted beside them and is the half that keeps the rest honest — a change that put a
## fodder term on every row would satisfy every positive claim above and fail exactly this one.
func _assert_work_fodder_readouts() -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var models: Array = _hud._bandpanel._work_source_models(band, 0)
	var paying: Array = models.filter(func(m): return SourceForecast.has_component(float(m.get("fodder_rate", 0.0))))
	_assert_band_panel("fodder — exactly one of this band's worked sources pays feed (found %d)"
		% paying.size(), paying.size() == 1)
	if paying.is_empty():
		return
	var field: Dictionary = paying[0]
	var field_rate := _hud._bandpanel._work_row_rate_text(field)
	_assert_band_panel("fodder — the board row states the feed rate instead of +0.00 (got \"%s\")"
		% field_rate, field_rate == FODDER_ROW_RATE_FACE)
	var field_sentence := _hud._bandpanel._work_inspector_sentence(field)
	_assert_band_panel("fodder — the inspector sentence names the account (got \"%s\")"
		% field_sentence, field_sentence.contains(FODDER_INSPECTOR_CLAUSE))
	var total := _work_head_fodder_total()
	_assert_band_panel("fodder — the WORK head renders the feed total (got \"%s\")"
		% ("<none>" if total == null else total.text),
		total != null and total.text == FODDER_ROW_RATE_FACE)
	var hunt_kind := SourceForecast.LABOR_KIND_HUNT
	var hunts: Array = models.filter(func(m): return String(m.get("kind", "")) == hunt_kind)
	_assert_band_panel("fodder — the control hunt row is on the board (found %d)" % hunts.size(),
		hunts.size() == 1)
	if hunts.is_empty():
		return
	var hunt: Dictionary = hunts[0]
	var hunt_rate := _hud._bandpanel._work_row_rate_text(hunt)
	_assert_band_panel("fodder — a hunt row still headlines its food rate (got \"%s\")" % hunt_rate,
		hunt_rate == FODDER_CONTROL_HUNT_FACE)
	_assert_band_panel("fodder — no fodder term reaches a hunt row's sentence",
		not _hud._bandpanel._work_inspector_sentence(hunt).contains("fodder"))

## The paired NEGATIVE: a band with no feed-paying source renders NO fodder sibling. Without it every
## claim above is satisfied by a head that renders the total unconditionally.
func _assert_no_work_fodder_total() -> void:
	var total := _work_head_fodder_total()
	_assert_band_panel("fodder — a band growing no feed renders no fodder total (got \"%s\")"
		% ("<none>" if total == null else total.text), total == null)

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
	var models: Array = _hud._bandpanel._work_source_models(band, 0)
	for model_variant in models:
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) != herd_id:
			continue
		_hud._bandpanel._toggle_work_inspector(String(model.get("key", "")))
		return
	_fail("%s" % _work_row_absence_report(herd_id, band, models))

## **Keyed on the HERD, not on the rung.** Both rows stand on the same stance now (issue #442 — the
## build verb moved to its own field), so a rung is no longer an identity; the source is.
func _open_work_policy_picker_for_herd(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var models: Array = _hud._bandpanel._work_source_models(band, 0)
	for model_variant in models:
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) != herd_id:
			continue
		_hud._bandpanel._work_open_key = String(model.get("key", ""))
		_hud._bandpanel._work_floor_open = true
		_hud._bandpanel._repage_work_zone()
		return
	_fail("%s" % _work_row_absence_report(herd_id, band, models))

## WHY A WORK ROW IS MISSING, in the terms the two helpers above can actually be wrong about.
## The message they used to share — "fixture drifted?" — named the ONE cause that is checked into the
## repo and therefore the one cause that cannot vary between two runs of the same tree. Every other
## cause is a SUBJECT mismatch: the panel is showing a band other than the one just pushed (the roster
## push never reached `render_band`, or `_resolve_panel_band` kept the previous subject), or the
## board's models were built off a stale one. So the report names the subject at each hop — the band
## the panel holds, the roster it was resolved out of, the assignments on it and the models the board
## actually built — and a reader can tell those apart at a glance instead of re-deriving them.
func _work_row_absence_report(herd_id: String, band: Dictionary, models: Array) -> String:
	var assignment_ids: Array = []
	for a in HudBandLaborState.labor_assignments_of(band):
		if not (a is Dictionary):
			continue
		var assignment: Dictionary = a
		assignment_ids.append("%s/%s" % [
			String(assignment.get("kind", "?")),
			String(assignment.get("fauna_id", "-"))])
	var model_ids: Array = []
	for m in models:
		var model: Dictionary = m
		model_ids.append("%s/%s" % [
			String(model.get("kind", "?")), String(model.get("herd_id", "-"))])
	var roster_ids: Array = []
	for b in _hud._band_labor.player_bands():
		roster_ids.append(int((b as Dictionary).get("entity", -1)))
	return ("no work row hunting '%s' — panel band entity %d (%s), roster %s, %d assignment(s) %s," +
		" %d work model(s) %s, %d pending edit(s)") % [
			herd_id, int(band.get("entity", -1)),
			"empty" if band.is_empty() else String(band.get("id", "?")),
			str(roster_ids), assignment_ids.size(), str(assignment_ids),
			model_ids.size(), str(model_ids),
			_hud._band_labor.pending_assigns_for(int(band.get("entity", -1))).size()]

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
		_fail("no '%s' rung in the work inspector's picker" % PICKED_RUNG_PRESET)
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
		_fail("'%s' row pick expected (confirm=%s, emit=%s) but got (confirm=%s, emit=%s)" % [
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
		_fail("expected only '%s' lit in the picker but got %s" % [standing, str(lit)])

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
		_fail("no Hunt work row for '%s' — fixture drifted?" % herd_id)
		return
	if String(before.get("improvement", "")) != improvement or String(before.get("building_glyph", "")) == "":
		_fail(("the '%s' row is not mid-build before the edit "
			+ "(improvement '%s', building glyph '%s') — the crew-edit assertion would be vacuous")
			% [herd_id, String(before.get("improvement", "")), String(before.get("building_glyph", ""))])
		return
	# The REAL row-stepper path, at one worker more than it stands on — the `+` a player presses.
	_hud._bandpanel._emit_work_assign(band, before, int(before.get("workers", 0)) + 1)
	var after := _find_work_model_for_herd(band, herd_id)
	if not bool(after.get("pending", false)):
		_fail("the crew edit on '%s' recorded no pending assign to judge" % herd_id)
	elif String(after.get("improvement", "")) != improvement:
		_fail(("a crew edit on '%s' dropped the improvement — the row now reads "
			+ "'%s' instead of '%s', so its build badge vanishes and the rung it is already climbing is "
			+ "re-offered for the rest of the turn")
			% [herd_id, String(after.get("improvement", "")), improvement])
	elif String(after.get("building_glyph", "")) == "":
		_fail(("a crew edit on '%s' kept the improvement but lost the BUILDING "
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

## Hand the CANVAS back to `project.godot`'s own stretch, for a state that pins a WINDOW and
## deliberately claims nothing about the projection (`band_panel_wide_ultrawide`).
##
## **A CANVAS PIN OUTLIVES THE STATE THAT SET IT, and `_pin_window` alone does not clear it** — it
## re-asserts `content_scale_size` from `_pinned_canvas` on every settle. A state that inherited a
## previous state's canvas therefore renders at a projection nobody chose: measured, the four-zone
## faction threshold's 1710x900 canvas left the 3440x900 ultrawide window projecting a 3440x900 logical
## viewport where the project's own base gives 4128x1080 — a different frame, and one the strict pin
## check correctly refused. Stating the condition rather than inheriting it is this harness's own rule.
func _release_canvas_pin() -> void:
	_pinned_canvas = Vector2i.ZERO
	get_window().content_scale_size = Vector2i(
		int(ProjectSettings.get_setting(PROJECT_VIEWPORT_WIDTH)),
		int(ProjectSettings.get_setting(PROJECT_VIEWPORT_HEIGHT)))

## `project.godot`'s authored base resolution — the canvas the `expand` stretch projects from when no
## state has pinned one. Read rather than restated so the release cannot drift from the project.
const PROJECT_VIEWPORT_WIDTH := "display/window/size/viewport_width"
const PROJECT_VIEWPORT_HEIGHT := "display/window/size/viewport_height"

## Force the window WINDOWED at `size` and wait for the WM to actually honour it, so a maximize
## cannot land between two states and render them at different resolutions.
##
## **IT WAITS ON THE LOGICAL VIEWPORT, NOT ONLY ON `window.size`, AND THAT IS THE THING EVERY
## ASSERTION IS MEASURED AGAINST.** `project.godot` stretches `canvas_items` with an `expand` aspect,
## so the logical viewport is a projection OF the window: while a resize is still in flight the
## window is one size and the canvas is another, and every width the panel and the yield rule read
## comes off the canvas. Measured directly — a window left at 2600x928 under a canvas pinned to
## 1920x1080 reports a logical viewport of **3025** wide, and `Main.band_dock_overlays_hud` answers
## for that 3025px row while the state believes it is testing 1920. That is a state rendering and
## asserting against a width it never asked for, which is exactly what this function exists to stop.
##
## **A PIN THAT DOES NOT PIN FAILS THE RUN.** It used to `push_warning`, which is invisible in a
## 500-line log from a harness whose whole value is bit-identity — a mis-pinned run passed. The ONE
## exception is a run with no window at all (`--headless`), which `_report_canvas_drift` warns about
## and skips: there the pin is unanswerable rather than broken.
##
## `strict` is `false` for exactly one caller, `_stabilize_canvas`, which is DELIBERATELY driving the
## window through a maximize and converging over up to `CANVAS_STABLE_MAX_FRAMES`; a transient miss
## there is the process working, and that function reports its own failure if it never settles.
##
## The viewport check is skipped until a canvas has been pinned: with `content_scale_size` left at the
## project's base, the logical viewport is the `expand` projection of whatever window the state asked
## for and the harness is not claiming to control it (which is the whole reason `_pin_canvas` exists).
func _pin_window(size: Vector2i, strict: bool = true) -> void:
	_pinned_size = size
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = size
	if _pinned_canvas != Vector2i.ZERO:
		window.content_scale_size = _pinned_canvas
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		if window.size == size and window.mode == Window.MODE_WINDOWED and _canvas_is_projected():
			return
		window.mode = Window.MODE_WINDOWED
		window.size = size
		await get_tree().process_frame
	if not strict:
		return
	if window.size != size:
		_report_canvas_drift("window pinned to %s but reports %s — every width this state asserts is measured against the canvas that window projects" % [size, window.size])
	elif not _canvas_is_projected():
		_report_canvas_drift("window is %s but the logical viewport is %s, not the %s canvas it was pinned to" % [
			size, get_viewport().get_visible_rect().size, _expected_canvas()])

## Does the LOGICAL viewport match the canvas this state pinned? True (vacuously) before any canvas
## has been pinned — see `_pin_window`.
##
## The window and `content_scale_size` are held equal by `_pin_canvas`, so the `expand` aspect's own
## scale factor is exactly 1 and the only remaining term is `content_scale_factor`, which the
## interface-scale states drive. Hence the expectation is the canvas over that factor, and it is a
## reading of the two window properties rather than a second model of the projection.
func _canvas_is_projected() -> bool:
	if _pinned_canvas == Vector2i.ZERO:
		return true
	return get_viewport().get_visible_rect().size.distance_to(_expected_canvas()) <= CANVAS_PROJECTION_TOLERANCE

func _expected_canvas() -> Vector2:
	return Vector2(_pinned_canvas) / maxf(get_window().content_scale_factor, CONTENT_SCALE_MIN)

## Sub-pixel slack between the canvas the state asked for and the projection Godot computes from it —
## `content_scale_factor` is a float divide, so an exact compare would fail on the scale states alone.
const CANVAS_PROJECTION_TOLERANCE := 1.5
## Floor on the scale divisor, so a zeroed `content_scale_factor` cannot make the expectation infinite.
const CONTENT_SCALE_MIN := 0.01

## Settle the window ONCE, in `_ready`, before any state renders — and take the maximize DELIBERATELY
## on the way, which is what closes the last of the drift.
##
## Whether a run passes through a monitor-sized window is a COIN FLIP — the window's mode and size are
## applied asynchronously by the WM — and it is a coin flip the pixels
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
			# NOT strict: this loop is deliberately driving the window through a maximize and has its
			# own terminal error below.
			await _pin_window(PREVIEW_SIZE, false)
		await get_tree().process_frame
	_report_canvas_drift("the window never held the pinned %s canvas — frames will drift" % PREVIEW_SIZE)

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
	_fail("viewport never came back to the pinned %s canvas for %s" % [_pinned_size, name])
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

## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL — ` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("band_panel_preview: FAIL — %s" % message)

## Is this run using the headless display driver, i.e. is there no window behind `_pin_window`?
##
## **A CONDITION THAT FAILS ONLY BECAUSE THERE IS NO RENDERER IS NOT A FAILURE.** `--headless` is the
## documented fast "does this still compile?" pass over this harness, and under it the window never
## leaves its stub geometry — it reports `MODE_MINIMIZED` and never accepts `MODE_WINDOWED` — so the
## canvas claims are unanswerable rather than false. They warn and skip; `_capture`'s null-image arm
## is the precedent. Every assertion that does not need a window still runs and still counts.
func _is_headless() -> bool:
	return DisplayServer.get_name() == HEADLESS_DISPLAY_DRIVER

## Report a window/canvas the pin would not hold. A real failure in a window — every width this
## harness asserts is measured against that canvas — and a skip under `--headless`, where the stub
## window can never hold it and reporting one would fail every clean run.
func _report_canvas_drift(message: String) -> void:
	if _is_headless():
		push_warning("band_panel_preview: %s (no window under the %s display driver — skipped; run windowed to capture)"
			% [message, HEADLESS_DISPLAY_DRIVER])
		return
	_fail(message)

## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place and the hang guard is stood down
## before shutdown (a slow shutdown is not a stall).
func _finish() -> void:
	if _watchdog != null:
		_watchdog.disarm()
	if _failures > 0:
		print("band_panel_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("band_panel_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)

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
		_fail("failed to save %s (err %d)" % [name, err])
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
		# **A CLICK THAT NEVER HAPPENED IS A FAILED PRECONDITION, NOT AN ADVISORY.** Every assertion the
		# disclosure states rides on this press, and each of them reads "the breakdown is not inline" —
		# i.e. passes on a panel that rendered no disclosure at all. Warning here printed a line nobody
		# reads and left the block claiming its result vacuously.
		_fail("no vitals label offering '%s' — the disclosure was never rendered, so nothing was clicked" % meta)
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
			_fail(("%s — herd \"%s\" declares %s %d but %s %d. The would-be "
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
			_fail(("%s — herd \"%s\" declares %s %d and %s %d. Once %s is "
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

## The field-pair guard's verdict, ONE line for the whole run (each violation has already gone through
## `_fail` against the frame it rendered in, so it is already counted against the run's exit status and
## this line only states the total). The scanned count is part of the claim: a guard that
## walked nothing would pass vacuously, and "0 herd dicts scanned" says so out loud.
func _assert_herd_field_pairs() -> void:
	if _herd_pair_violations > 0:
		_fail("%d herd dict(s) of %d scanned half-set the herders_needed pair"
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
	# **WHICH KIT BOTH TABLES ARE QUOTED FOR.** The sim writes the hunt job's default on every herd,
	# always, so a fixture leaving them blank would exercise only the client's fall-back reading and
	# the STATED path — the one live data takes — would go untested. Stamped on all three herds below
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
	}
	return [herd, near, home]

# `_stamp_estimate_kits` went with `hunt_trip_estimates_kit_id` / `denial_estimates_kit_id`. It wrote
# the kit those pre-sampled tables were priced at so the sheets could refuse to quote them for any
# other kit; a raid is costed for the composed kit now, so there is nothing to stamp.

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
##
## **AND IT WASTES NOTHING, WHICH IS A FACT ABOUT THE PRODUCT RATHER THAN ABOUT THE PARTY.**
## `carry_room_biomass` answers `NO_CARRY_BOUND` for a species paying no provisions — the pack is
## measured in provisions, so a quarry that pays none never fills it — and the sim's own take then
## carries every kill (`take.wasted` is empty). So the party hauls the WHOLE pelt yield and both
## waste halves are zero; inheriting the boar's food-bound carry share here would have quoted a wolf
## pack losing three quarters of its hides to a pack it cannot fill.
func _denial_trade_only_rows() -> Array:
	var rows: Array = []
	for row_variant in _denial_viable_rows():
		var row: Dictionary = (row_variant as Dictionary).duplicate(true)
		row["delivered_food"] = 0.0
		row["wasted_food"] = 0.0
		row["delivered_trade"] = float(int(row.get("animals_killed", 0))) * QUARRY_TRADE_PER_ANIMAL
		row["wasted_trade"] = 0.0
		rows.append(row)
	return rows

## The DENIAL raid's pre-launch table — an ARRAY with ONE row per party size and no other axis, which
## is the whole shape difference from `hunt_trip_estimates` above: denial carries no floor and no fill
## target, so party size is the only thing there is to sample and a row's `party_workers` is its id.
##
## `outcome` is on every row because the client renders nothing numeric without it, and a `0` turn
## count means "not within the horizon on that end" rather than "immediately".
## **`parties` states the AXIS where it is not `1..N`.** Every table below the ladder assertions samples
## contiguously, which is what an empty `parties` means; the ladder fixture states its own rungs, since
## the sampled axis is exactly what those assertions are about.
func _denial_rows(outcome: String, turns_row: Array, low_row: Array, high_row: Array,
		kills_row: Array, parties: Array = []) -> Array:
	var rows: Array = []
	for i in kills_row.size():
		var party: int = int(parties[i]) if i < parties.size() else i + 1
		var killed := int(kills_row[i])
		var killed_food := float(killed) * QUARRY_FOOD_PER_ANIMAL
		# What the pack holds, never what it killed: the raid banks a rounding error on the way home.
		var hauled := minf(killed_food, float(party) * DENIAL_CARRY_PER_WORKER)
		var hauled_share := (hauled / killed_food) if killed_food > 0.0 else 0.0
		# **BOTH PRODUCTS COME OFF ONE CONVERSION OF THE SAME BIOMASS**, which is what the sim does
		# (`hunt_yield.apply(take.carried)` beside `hunt_yield.apply(take.wasted)`): the pelts ride
		# whichever share of the kill the pack held, and the rest is left on the range with the meat.
		# A fixture stating a wasted_trade of 0 beside a large wasted_food would be a herd no live
		# server can produce, and the waste readout's trade half would have nothing to state.
		var killed_trade := float(killed) * QUARRY_TRADE_PER_ANIMAL
		var hauled_trade := killed_trade * hauled_share
		rows.append({
			"party_workers": party,
			"turns_to_collapse": int(turns_row[i]),
			"turns_to_collapse_low": int(low_row[i]),
			"turns_to_collapse_high": int(high_row[i]),
			"outcome": outcome,
			"animals_killed": killed,
			"delivered_food": hauled,
			"wasted_food": killed_food - hauled,
			"delivered_trade": hauled_trade,
			"wasted_trade": killed_trade - hauled_trade,
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

# `_assert_denial_party_needed_skips_horizon` and `_denial_party_needed_for` went with the sampled
# denial axis. The requirement is `DenialRaidForecastReply.party_needed` now — searched contiguously
# to the asking band's own last worker, server-side — so "which sampled rung does the client pick?"
# is not a question the client answers any more.

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

## Drive the kit picker through its REAL popup dispatch, choosing the entry whose label begins with
## this kit's display name. By the POPUP, never by writing `ComposeState` — the pick path (popup →
## `OptionButton._selected` → `item_selected` → callback → `set_party_kit_id` → rerender) is half of
## what the frames claim.
##
## **`index_pressed`, NOT `id_pressed`.** An `OptionButton` connects its popup's `index_pressed` and
## nothing else, so emitting `id_pressed` would run no handler, change no selection and leave the
## sheet exactly as it was — silently, which on the mismatch frame reads as the honesty rule failing
## rather than as the harness never having picked anything.
func _pick_kit(kit_id: String) -> void:
	var picker := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as OptionButton
	if picker == null:
		_assert_band_panel("picking a kit needs the picker to exist", false)
		return
	var want := KitRoster.display_name_for_id(_hud._band_labor.kits(), kit_id)
	var popup := picker.get_popup()
	for i in popup.item_count:
		if popup.get_item_text(i).begins_with(want):
			popup.index_pressed.emit(i)
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
## **SWITCHING KITS MOVES THE SHEET'S NUMBERS** — the substitution the whole compose readout rides on.
##
## Reported from play, twice, and both defects were the same shape: arithmetic that *looked* right
## against a source whose keys I had spelled from memory.
##
## 1. The food line never moved while trade moved by exactly 5×. The repricing scaled `"per_worker"`,
##    and food reads **`per_worker_yield`**. Trade's key happened to be right, which is what made the
##    bug look like a ceiling.
## 2. The retreat was applied as `effective / stay`, which assumes the wire's `engageRate` already
##    carries the species' own flight. It does not — it is animals brought INTO CONTACT — so a
##    trapping party was quoted above its own reach. The correction that followed folded the retreat
##    into `engage_rate` outright, which reprices the take and the CREW COUNT together and made the
##    stepper cap disagree with the sim's own `workersNeeded`. It rides `stay_fraction` now.
## 3. The ratio divided by the SOURCE's published `per_worker_biomass` rather than by the roster's
##    equipped tier. The two coincide on a live herd, so nothing said otherwise until a source whose
##    rates state a different throughput went through it — a seasonal-weighted patch, and every canned
##    harness fixture — and every crew count on the sheet moved.
##
## **Every key below is taken from `SourceForecast`'s constants, never typed**, which is the guard
## against the first one recurring; the retreat and reference assertions pin the other two by naming
## the sim's own expressions.
## **THE DOCK'S RAID CHART CARRIES THE KIT — and `dispersion` is what it carries.**
##
## The dock's hunt form is almost entirely `huntTripEstimates`: the trip readout, the preset metrics
## and the demand-side party cap are all lookups into a table the sim quotes at the hunt job's DEFAULT
## kit and does not reprice, so none of them may move with a selection (the honesty gate). The CHART is
## the exception — it is composed client-side from the herd's own wire terms — which makes it, beside
## the combat gate, the only thing on that sheet still answering for the kit the player picked.
##
## **THE TWO KITS DIFFER ONLY IN `dispersion`.** Same carry on both, so the carry half of the
## substitution cannot account for a single unit of the difference and what is left is the retreat.
## A locally-built roster rather than `BandFx.kit_roster_fixture()`, which ships no `dispersion` at all
## — asserting through that one would be comparing a kit against itself.
##
## **THE PAIR IS THE CLAIM, and the second half is the sim boundary.** The drawdown answers (the
## projection the curve draws, and the *clear* crew the verdict names) MUST move: a party keeping one
## animal in four needs four times the hands to pull a herd to a floor. The HOLD crew must NOT — on a
## whole-animal source it is `take_workers`, the client mirror of `fauna::hunt_take_workers`, and
## `max_useful_workers` floors the stepper cap on it, so a retreat reaching it would put the sheet at
## odds with the sim's own `workersNeeded`.
func _assert_dock_chart_carries_the_kit() -> void:
	const DOCK_KIT_CARRY := 40.0
	const DOCK_KIT_ENGAGE := 4.0
	# `1 - wariness` at 0.75 — three animals in four bolt before contact, so a device that is not
	# there to be seen is worth four times a spear party on this quarry.
	const DOCK_KIT_STAY := 0.25
	const DOCK_KIT_PARTY := 3
	var roster := [
		{
			KitRoster.KIT_ID_KEY: "spear_line", KitRoster.KIT_JOBS_KEY: [KitRoster.JOB_HUNT],
			KitRoster.KIT_HUNT_CARRY_KEY: DOCK_KIT_CARRY,
			KitRoster.KIT_FORAGE_CARRY_KEY: DOCK_KIT_CARRY,
			KitRoster.KIT_DISPERSION_KEY: KitRoster.DISPERSION_NEUTRAL,
		},
		{
			KitRoster.KIT_ID_KEY: "passive_device", KitRoster.KIT_JOBS_KEY: [KitRoster.JOB_HUNT],
			KitRoster.KIT_HUNT_CARRY_KEY: DOCK_KIT_CARRY,
			KitRoster.KIT_FORAGE_CARRY_KEY: DOCK_KIT_CARRY,
			KitRoster.KIT_DISPERSION_KEY: 0.0,
		},
	]
	var quarry := {
		SourceForecast.FORECAST_BIOMASS_KEY: 300.0,
		SourceForecast.FORECAST_CAPACITY_KEY: 400.0,
		SourceForecast.FORECAST_BODY_MASS_KEY: 2.0,
		SourceForecast.FORECAST_FOOD_PER_ANIMAL_KEY: 0.2,
		SourceForecast.FORECAST_PROVISIONS_PER_BIOMASS_KEY: 0.1,
		SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY: DOCK_KIT_CARRY,
		SourceForecast.FORECAST_PER_WORKER_KEY: DOCK_KIT_CARRY * 0.1,
		SourceForecast.FORECAST_ENGAGE_RATE_KEY: DOCK_KIT_ENGAGE,
		KitRoster.SOURCE_STAY_FRACTION: DOCK_KIT_STAY,
		SourceForecast.FORECAST_REGROWTH_SAMPLES_KEY: PackedFloat32Array([
			0.0, 6.0, 9.0, 8.0, 4.0, 0.0]),
	}
	var speared := _dock_chart_at_kit(quarry, roster, "spear_line", DOCK_KIT_PARTY)
	var trapped := _dock_chart_at_kit(quarry, roster, "passive_device", DOCK_KIT_PARTY)
	# The precondition both claims stand on: a chart answering `known == false` would make every
	# comparison below a comparison of two absent numbers.
	_assert_band_panel("precondition: the dock's raid chart is known under both kits",
		bool(speared.get("known", false)) and bool(trapped.get("known", false)))
	# **THE CURVE ITSELF** — what the player sees. `settled_fraction` is where the walk ends, i.e. the
	# stock this party leaves standing, and it is the number the whole chart is drawn around.
	_assert_band_panel("the passive device draws the herd further down than the spear line (%s against %s)"
			% [str(trapped.get("settled_fraction")), str(speared.get("settled_fraction"))],
		float(trapped.get("settled_fraction", 1.0))
			< float(speared.get("settled_fraction", 1.0)) - STOCK_FRACTION_MARGIN)
	# **AND THE REMEDY THE VERDICT NAMES MOVES WITH IT.** `crew_to_clear` floors on `crew_that_reaches`,
	# so this covers both drawdown answers at once — a spear party needs strictly more hands to pull the
	# same herd to the same floor.
	_assert_band_panel("…and the spear line needs more hands to clear the same room (%d against %d)"
			% [int(speared.get("crew_to_clear", 0)), int(trapped.get("crew_to_clear", 0))],
		int(speared.get("crew_to_clear", 0)) > int(trapped.get("crew_to_clear", 0)))
	# **AND THE HOLD CREW MOVES WITH IT TOO.** `crew_to_hold` is `fauna::hunt_take_workers`, and it
	# divides by what STAYS like every other crew answer on this sheet: a spear party keeping one
	# animal in four needs four times the hands to take the same regrowth every turn. It read the RAW
	# reach for a while, on the grounds that the stepper cap floors on it and the sim's own
	# `workersNeeded` must agree — and the consequence was a cap sized BELOW the *clear it now* pill
	# beside it, naming a crew the panel then refused to let the player assign.
	_assert_band_panel("…and so does the HOLD crew (%d against %d)"
			% [int(speared.get("crew_to_hold", -1)), int(trapped.get("crew_to_hold", -2))],
		int(speared.get("crew_to_hold", -1)) > int(trapped.get("crew_to_hold", -2)))

## The dock's own chart composition, at one kit — `KitRoster.priced_source` then `floor_chart_model`,
## the two calls `_fill_hunt_compose_sheet` makes in that order.
func _dock_chart_at_kit(quarry: Dictionary, roster: Array, kit_id: String,
		party: int) -> Dictionary:
	return SourceForecast.floor_chart_model(
		KitRoster.priced_source(quarry, HudComposeVocab.BARE_FORECAST_PREFIX, roster,
			KitRoster.JOB_HUNT, "spear_line", kit_id, {}),
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.floor_for_preset(SourceForecast.FLOOR_PRESET_PEAK), party,
		SourceForecast.IMPROVEMENT_NONE, HudComposeVocab.COMPOSE_FIELD_PARTY.to_lower(), false)

func _assert_kit_reprices_the_source() -> void:
	const PUBLISHED_CARRY := RETREAT_REFERENCE_CARRY
	const BARE_CARRY := 12.0
	const PUBLISHED_ENGAGE := 10.0
	# `1 - wariness` for a Rabbit Warren at 0.75: three animals in four bolt before contact.
	const STAY := 0.25
	var src := {
		SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY: PUBLISHED_CARRY,
		SourceForecast.FORECAST_PER_WORKER_KEY: 8.0,
		SourceForecast.FORECAST_PER_WORKER_TRADE_KEY: 2.0,
		SourceForecast.FORECAST_ENGAGE_RATE_KEY: PUBLISHED_ENGAGE,
		KitRoster.SOURCE_STAY_FRACTION: STAY,
	}

	# --- CARRY: a sledless crew hauls a third as much, in EVERY account at once.
	var bare := KitRoster.repriced_source(src, "", BARE_CARRY, PUBLISHED_CARRY, 1.0)
	var ratio := BARE_CARRY / PUBLISHED_CARRY
	_assert_band_panel("a kit's carry reprices the source's per-worker biomass (%s)"
			% str(bare[SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY]),
		is_equal_approx(float(bare[SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY]), BARE_CARRY))
	# **THE FOOD LINE, by the key food actually reads.** This is the assertion the shipped bug walked
	# straight past, because it scaled a key nothing reads and the trade line still moved.
	_assert_band_panel("…and FOOD reprices with it — the key the forecast reads, not one like it (%s)"
			% str(bare[SourceForecast.FORECAST_PER_WORKER_KEY]),
		is_equal_approx(float(bare[SourceForecast.FORECAST_PER_WORKER_KEY]), 8.0 * ratio))
	_assert_band_panel("…and so does TRADE, by the same ratio — one throughput, not two (%s)"
			% str(bare[SourceForecast.FORECAST_PER_WORKER_TRADE_KEY]),
		is_equal_approx(float(bare[SourceForecast.FORECAST_PER_WORKER_TRADE_KEY]), 2.0 * ratio))

	# --- **THE REFERENCE IS THE ROSTER'S TIER, NOT THE SOURCE'S OWN PUBLISHED RATE**, and the two are
	# separated here because in production they COINCIDE — a herd publishes `labor_config.hunt
	# .per_worker_biomass_capacity` and the sledded kit grants that same number, so a fixture whose
	# published rate equals its reference passes with either denominator and says nothing. This source
	# publishes a rate the roster never quoted, which is the shape a seasonal-weighted patch has (a
	# `KitOption`'s forage tier is stated BEFORE the tile's weight) and the shape every canned harness
	# fixture has (its `per_worker_biomass` is recovered from its own rates).
	const OFF_REFERENCE_PUBLISHED := 286.0
	var off_reference: Dictionary = src.duplicate()
	off_reference[SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY] = OFF_REFERENCE_PUBLISHED
	var against_roster := KitRoster.repriced_source(off_reference, "", BARE_CARRY, PUBLISHED_CARRY,
		1.0)
	_assert_band_panel("the ratio divides by the ROSTER's tier, never by the source's own rate (%s)"
			% str(against_roster[SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY]),
		is_equal_approx(float(against_roster[SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY]),
			OFF_REFERENCE_PUBLISHED * ratio))
	# …and the same kit at the reference tier is a NO-OP, which is what makes the claim above a
	# statement about the denominator rather than about repricing happening at all.
	var at_reference := KitRoster.repriced_source(off_reference, "", PUBLISHED_CARRY,
		PUBLISHED_CARRY, 1.0)
	_assert_band_panel("…so the kit the source was published at moves nothing (%s)"
			% str(at_reference[SourceForecast.FORECAST_PER_WORKER_KEY]),
		is_equal_approx(float(at_reference[SourceForecast.FORECAST_PER_WORKER_KEY]), 8.0))

	# --- THE RETREAT RIDES `stay_fraction` AND NEVER THE REACH. Folding it into `engage_rate` reprices
	# the take and the CREW COUNT together; the sim sizes a crew on the RAW reach
	# (`fauna::hunt_engage_workers`) and lets `HuntParty::stayers` cut only what those hands bring down.
	# The fold made the stepper cap disagree with the sim's own `workersNeeded`.
	var neutral := KitRoster.repriced_source(src, "", PUBLISHED_CARRY, PUBLISHED_CARRY, 1.0)
	var trapped := KitRoster.repriced_source(src, "", PUBLISHED_CARRY, PUBLISHED_CARRY, 0.0)
	_assert_band_panel("dispersion does not touch the reach term — the crew count is the sim's (%s)"
			% str(trapped[SourceForecast.FORECAST_ENGAGE_RATE_KEY]),
		is_equal_approx(float(trapped[SourceForecast.FORECAST_ENGAGE_RATE_KEY]), PUBLISHED_ENGAGE)
			and is_equal_approx(float(neutral[SourceForecast.FORECAST_ENGAGE_RATE_KEY]),
				PUBLISHED_ENGAGE))
	# **A NEUTRAL KIT PAYS THE SPECIES' OWN RETREAT, UNCHANGED** — `1 - (1 - stay) x 1` is `stay`, so a
	# spear party on this warren keeps one animal in four and the wire's own number stands.
	_assert_band_panel("a neutral kit leaves the species' own retreat alone (%s)"
			% str(neutral[KitRoster.SOURCE_STAY_FRACTION]),
		is_equal_approx(float(neutral[KitRoster.SOURCE_STAY_FRACTION]), STAY))
	# **…AND THE PASSIVE DEVICE KEEPS EVERYTHING IT REACHES.** `dispersion 0` is the trapping kit's
	# whole advantage, and this is the one number that carries it onto the sheet.
	_assert_band_panel("dispersion 0 means nothing breaks off (%s)"
			% str(trapped[KitRoster.SOURCE_STAY_FRACTION]),
		is_equal_approx(float(trapped[KitRoster.SOURCE_STAY_FRACTION]),
			KitRoster.STAY_FRACTION_NONE_BREAKS_OFF))
	# The claim that motivated the whole substitution: same carry, same reach, different take.
	_assert_band_panel("…so two kits with the SAME carry still quote different takes",
		not is_equal_approx(float(trapped[KitRoster.SOURCE_STAY_FRACTION]),
			float(neutral[KitRoster.SOURCE_STAY_FRACTION])))

	# --- **AND THE SUBSTITUTION REACHES THE SHEET — the whole point of it.** Everything above is about
	# one dict; this is the claim a player would make. Two kits, same carry, same reach, and BOTH the
	# take and the crew have to move: a party that keeps one animal in four takes a quarter as much per
	# hand, so it needs four times the hands to draw the same stock down. The pairing IS the assertion —
	# a repricing that reached neither would satisfy nothing, and one that reached only the take would
	# leave the stepper capping below the crew targets rendered beside it.
	const RETREAT_WORKERS := 4
	const RETREAT_BODY_MASS := 2.0
	const RETREAT_FOOD_PER_BIOMASS := 0.1
	var quarry := {
		SourceForecast.FORECAST_BIOMASS_KEY: 300.0,
		SourceForecast.FORECAST_CAPACITY_KEY: 400.0,
		SourceForecast.FORECAST_BODY_MASS_KEY: RETREAT_BODY_MASS,
		SourceForecast.FORECAST_FOOD_PER_ANIMAL_KEY: RETREAT_BODY_MASS * RETREAT_FOOD_PER_BIOMASS,
		SourceForecast.FORECAST_PROVISIONS_PER_BIOMASS_KEY: RETREAT_FOOD_PER_BIOMASS,
		SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY: PUBLISHED_CARRY,
		SourceForecast.FORECAST_PER_WORKER_KEY: PUBLISHED_CARRY * RETREAT_FOOD_PER_BIOMASS,
		SourceForecast.FORECAST_ENGAGE_RATE_KEY: PUBLISHED_ENGAGE,
		KitRoster.SOURCE_STAY_FRACTION: STAY,
	}
	var spear_take := _kit_hunt_take(quarry, 1.0, RETREAT_WORKERS)
	var trap_take := _kit_hunt_take(quarry, 0.0, RETREAT_WORKERS)
	_assert_band_panel("the passive device out-takes the spear on the same herd (%s against %s)"
			% [str(trap_take), str(spear_take)],
		trap_take > spear_take + SourceForecast.COMPONENT_RENDER_MIN)
	# **…AND THE CREW THE SHEET ASKS FOR MOVES WITH IT.** `engage_workers` divides the peak drop by what
	# STAYS, so the spear line's stepper caps strictly higher than the trap line's on one herd — which
	# is what keeps the cap at or above the *clear it now* pill beside it, that pill having divided by
	# the retreat-aware reach all along.
	var spear_cap := _kit_hunt_cap(quarry, 1.0)
	var trap_cap := _kit_hunt_cap(quarry, 0.0)
	_assert_band_panel("…and the crew the sheet asks for moves with it (%d against %d)"
			% [spear_cap, trap_cap],
		spear_cap > trap_cap)

	# --- A SOURCE WITH NO RETREAT STAGE (a patch, a pen) is untouched by that half.
	var patch := {
		"patch_" + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY: 8.0,
		"patch_" + SourceForecast.FORECAST_PER_WORKER_KEY: 6.0,
	}
	var gathered := KitRoster.repriced_source(patch, "patch_", 1.6, 8.0, 0.0)
	_assert_band_panel("a source that publishes no retreat is repriced on carry alone",
		is_equal_approx(float(gathered["patch_" + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY]),
				1.6)
			and is_equal_approx(
				float(gathered["patch_" + SourceForecast.FORECAST_PER_WORKER_KEY]), 6.0 * 0.2)
			and not gathered.has(KitRoster.SOURCE_STAY_FRACTION))

## The take a crew of `workers` lands on `quarry` under a kit of this `dispersion`, through the SAME
## `expected_yield_account` the compose sheet's readout reads.
func _kit_hunt_take(quarry: Dictionary, dispersion: float, workers: int) -> float:
	return SourceForecast.expected_yield(_kit_hunt_forecast(quarry, dispersion), workers, {})

## …and the crew that sheet's stepper caps at, which must NOT move with the kit's dispersion.
func _kit_hunt_cap(quarry: Dictionary, dispersion: float) -> int:
	return SourceForecast.max_useful_workers(_kit_hunt_forecast(quarry, dispersion))

## The one composition both read, so the take and the cap cannot be priced two ways. Both arms sit at
## the roster's reference tier, so the ONLY thing differing between two calls is the retreat.
func _kit_hunt_forecast(quarry: Dictionary, dispersion: float) -> Dictionary:
	return SourceForecast.forecast_inputs(
		KitRoster.repriced_source(quarry, "", RETREAT_REFERENCE_CARRY, RETREAT_REFERENCE_CARRY,
			dispersion),
		SourceForecast.SOURCE_KIND_HERD, "",
		SourceForecast.floor_for_preset(SourceForecast.FLOOR_PRESET_STRIP),
		SourceForecast.IMPROVEMENT_NONE)

func _assert_kit_picker_closed() -> void:
	var picker := _find_meta_control(_panel, KitRoster.KIT_PICKER_META) as OptionButton
	_assert_band_panel("the denial sheet carries a Kit picker", picker != null)
	if picker == null:
		return
	# **THE FACE CARRIES NO `(default)` SUFFIX AND NO CARET**, which is the whole of what the
	# `OptionButton` conversion has to get right: `select()` writes the item's own text into `text`,
	# so a face equal to the LIST entry means the override never ran, and the equality catches it.
	var face := HudComposeVocab.KIT_PICKER_FACE_FORMAT % [
		String(HudComposeVocab.KIT_JOB_GLYPHS[KitRoster.JOB_HUNT]), "Stalking kit"]
	_assert_band_panel("…whose face names the selected kit (\"%s\")" % picker.text,
		picker.text == face)
	var hint := HudComposeVocab.KIT_HINT_SEPARATOR.join([
		HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(BandFx.KIT_ATTACK_EQUIPPED,
			HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(BandFx.KIT_HUNT_CARRY_BARE,
			HudComposeVocab.KIT_TIER_DECIMALS),
		# **THE ITEMS ARE THE KIT'S OWN, IN ITS OWN ORDER** — `big_game` carries spears then the sled, so
		# the hint reads them out in that order and names nothing else. Taken from the ROSTER fixture's
		# item ids, which is where the wire states them.
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [BandFx.KIT_ITEM_SPEARS,
			int(KIT_FRAME_SPEARS_CONDITION)],
		HudComposeVocab.KIT_HINT_DRY_FORMAT % BandFx.KIT_ITEM_SLED,
	])
	var rendered := _find_meta_control(_panel, KitRoster.KIT_HINT_META) as Label
	_assert_band_panel("…over a hint stating the EFFECTIVE tier, not the fresh one — \"%s\"" % hint,
		rendered != null and rendered.text == hint)

## The picker OPEN. A screenshot cannot say which entry carries the radio dot, so the structure rides
## here: the roster's hunt kits and only those, the composed one marked, the job default TAGGED, and
## `none` LAST — which it is because the ROSTER authors it last and this client sorts nothing.
##
## **The MARK is now the `OptionButton`'s own**, not a hand-rolled `MENU_ENTRY_CHECKED` — the control
## builds radio-check items and checks the selected one itself — so this reads `is_item_checked` to
## assert that the SELECTED INDEX handed to the builder reached the popup.
func _assert_kit_picker_open(picker: OptionButton) -> void:
	_assert_band_panel("the Kit picker opens a menu", picker != null)
	if picker == null:
		return
	var popup := picker.get_popup()
	var labels: Array[String] = []
	for i in popup.item_count:
		labels.append(popup.get_item_text(i))
	# The GATHERING kit lists `forage` alone, so its absence is the filter working rather than a
	# roster that happens to hold two entries.
	var want_labels: Array[String] = [
		"Stalking kit" + HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX, "No kit"]
	_assert_band_panel("…listing exactly this verb's kits, the default tagged, `none` last — %s"
			% str(labels),
		labels == want_labels)
	var checked: Array[String] = []
	for i in popup.item_count:
		if popup.is_item_checked(i):
			checked.append(popup.get_item_text(i))
	_assert_band_panel("…marking exactly the composed kit (%s)" % str(checked),
		checked.size() == 1 and String(checked[0]).begins_with("Stalking kit"))

## **THE UNANSWERED SHEET, ASSERTED BY EQUALITY** — `none` composed while the seam has no sender.
##
## The claim is half an ABSENCE (no collapse verdict, no estimate caveat, no take line, no counted
## refusal — every one of them a figure this sheet has no answer for yet) and a
## `contains` search can only testify that something IS present. So the sheet's lines BELOW the kit
## hint are compared to the exact expected list: the combat gate, which is composed from wire terms
## and is honest at any tier and with no reply at all, then the line saying the forecast is still
## being costed. A verdict put back fails this, and so does a gate line dropped.
##
## **IT USED TO BE THE KIT-MISMATCH SHEET.** The state it pins is the same shape for the same reason —
## nothing derived may render when the sheet has no figures of its own — but the reason it has none
## moved: the table was priced for another kit, and now the answer simply has not landed. Its caller
## uninstalls the canned answerer to reach it, since the prologue otherwise answers everything.
func _assert_forecastless_sheet_suppresses_estimates() -> void:
	var hint := _find_meta_control(_panel, KitRoster.KIT_HINT_META) as Label
	_assert_band_panel("the unanswered sheet still states the picked kit's tier", hint != null)
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
	var note := HudComposeVocab.DENIAL_FORECAST_PENDING
	var want: Array[String] = [gate, note]
	_assert_band_panel(("…and below it says EXACTLY the gate and the quoted-kit note — "
			+ "no verdict, no caveat, no take, no refusal. Got %s") % str(tail),
		tail == want)
	# The send stays LIVE: the raid is perfectly launchable, we simply cannot quote its length. A
	# disabled button here would read as the kit being illegal, which it is not.
	var confirm := _find_meta_control(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META) as Button
	_assert_band_panel("…while the Send stays live — the raid launches, only its length is unquotable",
		confirm != null and not confirm.disabled)

# `_assert_denial_quoted_party_note` went with the note. A denial sheet's verdict and take are costed
# for the party on its stepper, so there is no second party to name.

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
	# **THE WASTE IS STATED, IN EVERY PRODUCT IT IS WASTED IN, AND IS NOT DRESSED AS A WARNING.** On a
	# hunt an unhauled kill wears `HUNT_FORECAST_WARN_GLYPH`; on a raid it IS the mission, so the line
	# is quiet and factual. The whole line is asserted BY EQUALITY rather than by `contains`, because
	# half the claim is what the sentence must not also say: a waste stated food-only would satisfy
	# every containment test while dropping the 26.75 of hides this boar leaves rotting beside the
	# meat, which is the food-only blindness this pair of figures exists to remove. The expectation is
	# composed from the VOCABULARY and from this fixture's own arithmetic, never through
	# `denial_take_bbcode` — re-deriving it through the formatter under test asserts nothing.
	var killed: int = DENIAL_KILLS_ROW[DENIAL_PARTY - 1]
	var killed_food := float(killed) * QUARRY_FOOD_PER_ANIMAL
	var hauled_food := minf(killed_food, float(DENIAL_PARTY) * DENIAL_CARRY_PER_WORKER)
	var left := killed_food - hauled_food
	# The pelts ride the same carried share the meat does (`_denial_rows`' one conversion), so this
	# boar's raid wastes BOTH products and neither figure can be a fabricated zero.
	var killed_trade := float(killed) * QUARRY_TRADE_PER_ANIMAL
	var hauled_trade := killed_trade * (hauled_food / killed_food)
	var left_trade := killed_trade - hauled_trade
	var want_take := SourceForecast.DENIAL_TAKE_KILLS_FORMAT % [killed, quarry]
	want_take += SourceForecast.DENIAL_TAKE_FOOD_FORMAT % SourceForecast.format_magnitude(hauled_food)
	want_take += SourceForecast.DENIAL_TAKE_TRADE_FORMAT % [
		FoodIcons.TRADE_GOODS_GLYPH, SourceForecast.format_magnitude(hauled_trade)]
	want_take += SourceForecast.DENIAL_TAKE_LEFT_FORMAT % SourceForecast.DENIAL_TAKE_LEFT_JOIN.join([
		SourceForecast.format_magnitude(left),
		SourceForecast.DENIAL_TAKE_LEFT_TRADE_FORMAT % [
			FoodIcons.TRADE_GOODS_GLYPH, SourceForecast.format_magnitude(left_trade)],
	])
	var take_line := _rich_text_containing(_panel,
		SourceForecast.DENIAL_TAKE_KILLS_FORMAT % [killed, quarry])
	_assert_band_panel("…and states the take PLAINLY, waste in BOTH products — wanted \"%s\", got \"%s\""
			% [want_take, take_line],
		take_line == want_take
			and not take_line.contains(SourceForecast.HUNT_FORECAST_WARN_GLYPH))
	# NO FLOOR ANYWHERE — not a picker, not even the row heading. Two surfaces,
	# one claim. **The heading is matched UPPER-CASED because `alloc_section_label` upper-cases what it
	# is given**, so the vocabulary const as written matches nothing and that clause would be vacuous
	# — which is exactly how it first shipped, passing with a Policy row put back on the form.
	_assert_band_panel("…and offers NO floor picker and no Policy row",
		_find_meta_control(_panel, HudWidgets.POLICY_RUNG_META) == null
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
	# calling the entry's callback — the popup's own dispatch is part of what is being asserted.
	var other := -1
	for i in popup.item_count:
		if not popup.is_item_checked(i):
			other = i
	popup.id_pressed.emit(popup.get_item_id(other))
	_assert_band_panel("…and choosing the other one re-targets the sheet (%s)"
			% _hud._compose.party_quarry_id(),
		_hud._compose.party_quarry_id() == SHARED_TILE_PELT_HERD_ID)
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
##
## **THE EXPANDED ROSTER'S THREE ITEMS AND THEIR THREE TIERS RIDE IT TOO**, because the claim this
## fixture backs is that the map marker carries the WHOLE cohort — a key it never states is a key the
## partition assertion says nothing about. Every value below is DISTINCT from every other tier on the
## band, so the gear popover's rows cannot pass with two of them swapped: the pen's rate is not the
## sled's 2.5, and the warriors' attack is not the hunters' 2.
const MAP_PATH_HUSBANDRY_GEAR_CONDITION := 45.0
const MAP_PATH_WAYFINDING_CONDITION := 66.0
const MAP_PATH_CLUBS_CONDITION := 22.0
const MAP_PATH_PEN_CARRY := 3.5
## Whole tiles, because that is what a posted vantage reveals at; the popover states it in tiles and
## never at the carries' one decimal.
const MAP_PATH_SCOUT_VANTAGE := 2.0
const MAP_PATH_WARRIOR_ATTACK := 5.0

func _kit_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["kit_item_conditions"] = [{"item_id": "spears", "remaining": 74.5}, {"item_id": "sled", "remaining": 58.0}, {"item_id": "baskets", "remaining": 91.0}, {"item_id": "traps", "remaining": 83.0}, {"item_id": "husbandry_gear", "remaining": MAP_PATH_HUSBANDRY_GEAR_CONDITION}, {"item_id": "wayfinding", "remaining": MAP_PATH_WAYFINDING_CONDITION}, {"item_id": "clubs", "remaining": MAP_PATH_CLUBS_CONDITION}]
	band["hunter_attack"] = 2.0
	band["hunt_carry_per_worker_biomass"] = 2.5
	band["forage_carry_per_worker_biomass"] = 1.75
	band["pen_carry_per_worker_biomass"] = MAP_PATH_PEN_CARRY
	band["scout_vantage_range"] = MAP_PATH_SCOUT_VANTAGE
	band["warrior_attack"] = MAP_PATH_WARRIOR_ATTACK
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

## The sledded haul tier, i.e. `labor_config.hunt.per_worker_biomass_capacity` — which is BOTH the rate
## a herd publishes as its `per_worker_biomass` and the roster's own maximum on that axis. It is at file
## scope because the retreat's end-to-end helpers below `_assert_kit_reprices_the_source` price against
## it too, and a second copy is how a reference and a published rate start disagreeing.
const RETREAT_REFERENCE_CARRY := 40.0

## Slack on a stock-fraction comparison — the projection walks 60 turns of float arithmetic, so two
## genuinely different settle points must differ by more than the accumulated noise to count.
const STOCK_FRACTION_MARGIN := 0.001
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
	# The list is REPLACED rather than extended, so the expanded roster's three are restated here —
	# a cohort the server publishes carries one row per item in the config's table, and dropping three
	# of them would render a band no live world produces.
	band["kit_item_conditions"] = [{"item_id": "spears", "remaining": KIT_FRAME_SPEARS_CONDITION}, {"item_id": "sled", "remaining": KIT_FRAME_SLED_DRY}, {"item_id": "baskets", "remaining": KIT_FRAME_BASKETS_CONDITION}, {"item_id": "traps", "remaining": KIT_FRAME_SPEARS_CONDITION}, {"item_id": "husbandry_gear", "remaining": BandFx.KIT_CONDITION_HUSBANDRY_GEAR}, {"item_id": "wayfinding", "remaining": BandFx.KIT_CONDITION_WAYFINDING}, {"item_id": "clubs", "remaining": BandFx.KIT_CONDITION_CLUBS}]
	# The band's OWN resolved tiers, i.e. what it gets under the JOB DEFAULT. They are the cohort's
	# statement and the `Kit` row reads them; the picker does NOT — it resolves the SELECTED kit's
	# tiers off the roster — so they are set consistently with the conditions above rather than being
	# what the picker's assertions read. The three the expanded roster added are inherited from
	# `_band_fixture` unchanged: this fixture's twist is the SLED, and only the hunt carry moves with it.
	band["hunter_attack"] = BandFx.KIT_ATTACK_EQUIPPED
	band["hunt_carry_per_worker_biomass"] = BandFx.KIT_HUNT_CARRY_BARE
	band["forage_carry_per_worker_biomass"] = BandFx.KIT_FORAGE_CARRY_EQUIPPED
	# **THE PICKER READS THE BAND'S OWN `kit_tiers` ROWS, NOT THESE THREE SCALARS.** The scalars above
	# answer for the job default alone; the hint is about whichever kit is SELECTED, and only the sim
	# can say what a given kit grants a given ledger (`KitRoster.BAND_KIT_TIERS_KEY`). The rows say what
	# the conditions above mean: the sled is dry, so every kit that carries one hauls at the bare tier
	# while its weapon and the untouched baskets stay equipped.
	# The warrior kit's own row is CLUBS, not the hunt weapon: the sled running dry says nothing about
	# what the camp is defended with, and quoting the spear's 20 there is the mis-pairing the per-kit
	# rows exist to make impossible. The VANTAGE stays equipped for the same reason — this fixture's
	# twist is the sled, and a scout's reach is bought by the wayfinding gear, which is untouched.
	band["kit_tiers"] = BandFx.kit_tiers_rows(BandFx.KIT_ATTACK_EQUIPPED,
		BandFx.KIT_HUNT_CARRY_BARE, BandFx.KIT_FORAGE_CARRY_EQUIPPED, BandFx.KIT_ATTACK_CLUBS,
		BandFx.KIT_SCOUT_VANTAGE_EQUIPPED)
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
		"kit_item_conditions": [{"item_id": "spears", "remaining": KIT_SHARED_SPEARS_CONDITION}, {"item_id": "sled", "remaining": KIT_SHARED_SLED_CONDITION}, {"item_id": "baskets", "remaining": KIT_SHARED_BASKETS_CONDITION}, {"item_id": "traps", "remaining": KIT_SHARED_SPEARS_CONDITION}, {"item_id": "husbandry_gear", "remaining": BandFx.KIT_CONDITION_HUSBANDRY_GEAR}, {"item_id": "wayfinding", "remaining": BandFx.KIT_CONDITION_WAYFINDING}, {"item_id": "clubs", "remaining": BandFx.KIT_CONDITION_CLUBS}],
		# The RESOLVED tiers the sim publishes beside them. Equipped throughout, matching the
		# conditions above — `hunter_attack` well clear of `QUARRY_DEFENSE`, so no compose sheet on
		# this band reads the combat gate's refusal and the frames that judge that refusal stay the
		# ones that compose a bare-handed kit.
		"hunter_attack": BandFx.KIT_ATTACK_EQUIPPED,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_EQUIPPED,
		# The expanded roster's three, resolved through the SAME job defaults `BandFx.with_equipped_kit`
		# resolves them through — one shared roster, so a band in this harness and a band in
		# `ui_preview` cannot get different answers off the same kits. The pen tier is the BARE one
		# because no entry of that roster equips husbandry gear (its own note records why), which is
		# also what keeps the pen row assertable against the sled's 40.
		"pen_carry_per_worker_biomass": BandFx.KIT_PEN_CARRY_BARE,
		"scout_vantage_range": BandFx.KIT_SCOUT_VANTAGE_EQUIPPED,
		"warrior_attack": BandFx.KIT_ATTACK_CLUBS,
		# The raid-forecast levers the sim echoes on every cohort: the slow-raid warn line and the
		# move rate the client adds round-trip travel from. Without them the compose sheet's forecast
		# degrades to hunting turns only and can never read "slow" — i.e. it would prove less.
		"expedition_viability_warn_turns": 20,
		# …and the horizon the "never completed" sentinels are relative to, without which the denial
		# sheet's horizon verdict falls back to naming a clock the player cannot see.
		"expedition_forecast_horizon_turns": BandFx.FORECAST_HORIZON_TURNS,
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

## THE FODDER-ONLY BAND (issue #449): one sown hay Field paying feed and NOTHING else, beside an
## ordinary deer hunt. The Field is what the change exists for — every food and trade field on it is
## honestly 0, so before this its row headlined `+0.00` and the tile read as dead while it fed the
## band's pens every turn. The DEER is the control and is not decoration: "a hunt is unchanged" is a
## claim about a row that has to be on the board to be wrong, and its trade term additionally puts the
## header's three siblings — food, trade, fodder — in one frame.
func _fodder_field_band_fixture() -> Dictionary:
	var band := _concerning_food_band_fixture()
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "workers_needed": 3, "floor": 0.5,
			"target_x": FODDER_FIELD_X, "target_y": FODDER_FIELD_Y,
			"actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0,
			# Both trade keys present and zero, the way the wire ships them, so the fodder branch is
			# reached through the ordinary render-only-when-non-zero gate rather than through absence.
			"trade_yield": 0.0, "realized_trade_yield": 0.0,
			"fodder_yield": FODDER_FIELD_RATE},
		{"kind": "hunt", "workers": 2, "fauna_id": EXTRACTIVE_ROW_HERD_ID, "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20,
			"realized_yield": FODDER_CONTROL_HUNT_RATE,
			"trade_yield": 0.04, "realized_trade_yield": 0.04},
		{"kind": "scout", "workers": 2},
	]
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

## A scout party that has ARRIVED and is awaiting orders — the one phase the founding action is
## offered in. Its position is what the settle confirm quotes as the site.
func _awaiting_scout_expedition_fixture() -> Dictionary:
	var exp := _scout_expedition_fixture()
	exp["entity"] = SCOUT_AWAITING_ENTITY
	exp["id"] = "Scouts 2"
	exp["expedition_phase"] = "awaiting"
	return exp

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

## A party outfitted by band 904 that HAS NOT LEFT: it stands on the band's own tile (71, 18) and owes
## it no map report, which is the sim's `cancel_party_standing_in_camp` exactly — so a recall folds it
## back the instant the command lands. It is the fixture the CANCEL branch of every single-party recall
## surface is judged on, and it differs from `_hunt_expedition_fixture` in its POSITION alone (plus the
## explicit zero report), so the pair is a controlled A/B on the predicate rather than on the party.
func _in_camp_expedition_fixture() -> Dictionary:
	var exp := _hunt_expedition_fixture()
	exp["id"] = "Hunters 4"
	exp["entity"] = HUNT_IN_CAMP_ENTITY
	exp["current_x"] = 71
	exp["current_y"] = 18
	# Stated, never left to the reader's default: "nothing owed" is a TERM of the predicate, and a
	# fixture silent on it would pass whether the client asked the question or not.
	exp["pending_reveal_count"] = 0
	return exp

## The same party in the FIELD — `_hunt_expedition_fixture` with the one term that is not about
## position made explicit, so the Recall half of the A/B states all four terms too.
func _in_field_expedition_fixture() -> Dictionary:
	var exp := _hunt_expedition_fixture()
	exp["pending_reveal_count"] = 0
	return exp

## A party standing in camp that still OWES ITS HOME BAND A MAP REPORT. The sim walks this one home —
## flushing `pending_reveal` to the faction map is the one thing an out-of-band fold-back cannot do —
## so it is the case that separates the real predicate from the tempting "is it on the band's tile".
func _in_camp_with_report_owed_fixture() -> Dictionary:
	var exp := _in_camp_expedition_fixture()
	exp["pending_reveal_count"] = 12
	return exp

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

## **THE WORST CASE FOR THE PARTIES INSPECTOR STRIP — every optional line live at once.**
##
## The strip is the party's detail panel and it lives in a `clip_contents` zone capped at ~300px on a
## horizontal dock, so what it costs is decided by how many of `BandDetailLines.expedition_summary_lines`'
## conditional lines a single party can light up. No fixture in this file had ever lit them all: the
## delivering party `band_panel_parties_inspector_wide` opens carries no fill target, no carry cap and
## no trip bound, and it read 310px of a 300px box on its own. This is the band-zone lesson
## (`band_panel_vitals_worst_case`) applied one zone over — a state built from the PRODUCER's gates
## rather than from the shape an existing fixture happens to have.
##
## The seven lines, each with the gate that lights it:
##  1. `Mission`        — unconditional
##  2. `Target`         — `is_raid` + a non-empty `expedition_target_herd`; the live `(x, y)` needs the
##                        herd to still be in `_world_herds`, hence `game_deer_79` from `_herd_fixtures`
##  3. `Orders`         — `is_hunt`; the fill target is > 0 so it names the quarry rather than the pack
##  4. `Phase`          — a non-empty `expedition_phase`
##  5. `Carried`        — `is_raid`; the carry cap is > 0 AND met, which is the LONGEST form (`/ cap`
##                        plus the `· FULL` badge)
##  6. `Next delivery`  — `is_hunt` + `has("expedition_projected_delivery")`; `expedition_recurring`
##                        appends the `↻`, the longest form again
##  7. trip bound       — a non-empty `expedition_trip_bound`
##
## **A DENIAL PARTY IS STRICTLY SHORTER, so the hunt is the worst case.** It renders Mission · Target ·
## Phase · Carried · Collapse — five — and the quoted-party note a between-rungs party earns rides the
## `Collapse:` row as a CLAUSE (`DetailFormat.DENIAL_COLLAPSE_QUOTED_PARTY_FORMAT`) rather than as a
## line of its own, which is exactly the choice this strip's height budget forces.
##
## **`Position` IS ABSENT AND THAT IS NOT AN OMISSION.** `expedition_summary_lines` renders it from
## `pos`, which is the MAP MARKER's stamp — `MapView._rebuild_unit_markers` writes it — while the
## parties zone reads the raw cohort dicts `update_band_alerts` pushed, and the native decoder emits no
## `pos` key at all (`current_x`/`current_y` instead). Staging one here would inflate this zone's
## requirement with a row it can never be handed, and the merge or the cut that paid for it would be
## paid for nothing. The row is live in the OTHER host — the Occupants drawer, reached through the
## marker — which is why the producer keeps it.
func _worst_case_party_fixture() -> Dictionary:
	return {
		"id": "Hunters 4",
		"entity": HUNT_WORST_CASE_ENTITY,
		"faction": 0,
		"size": 6,
		"current_x": 65,
		"current_y": 12,
		"turns_of_food": 5.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "delivering",
		# In `_herd_fixtures()`, so the Target row carries its live position.
		"expedition_target_herd": WORST_CASE_TARGET_HERD_ID,
		"expedition_floor": WORST_CASE_FLOOR,
		"home_band_entity": 904,
		"expedition_eta_turns": 6,
		"expedition_projected_delivery": 14.0,
		# The `↻` suffix — a recurring party's delivery line is the longer of the two.
		"expedition_recurring": true,
		# The pack is FULL, which is the Carried row's longest form (`N / cap` + the `· FULL` badge).
		"expedition_carry_cap": float(WORST_CASE_CARRY_CAP),
		"stores": {"provisions": float(WORST_CASE_CARRY_CAP)},
		# …and the sim's own answer for which stop ends the trip, which is a line of its own. It reads
		# `pack_full` because this party's pack IS full, and because `fill_target` — the bound this
		# fixture used to carry — is retired with the lever that named it (issue #491).
		"expedition_trip_bound": SourceForecast.TRIP_BOUND_PACK_FULL,
	}

