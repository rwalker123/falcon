extends Node

## Dev-only preview harness for the **Workbench** surface (rail + content host + its pages), the
## menu/HUD harnesses' third sibling. It builds a real `WorkbenchShell` in code — the shell has no
## `.tscn`, being assembled by `_build_chrome` — over a mid terrain tone standing in for the map, so
## the reserved-edge composition (surface on the left, map to the right of it) is visible in every
## frame. No server, no network: the tuning page reads the same `tuning_manifest.json` the client
## ships. Run from the repo root:
##
##   godot --headless --path clients/godot_thin_client --import        # if scenes/scripts changed
##   scripts/preview.sh res://tools/workbench_preview.tscn                       # NOT --headless
##
## then read ui_preview_out/workbench_*.png.

const OUT_DIR := "res://ui_preview_out"

## Window the surface renders into. Wide enough that the map strip beside the Workbench is a real
## composition rather than a sliver.
const PREVIEW_SIZE := Vector2i(1600, 900)
## Mid terrain tone behind the surface (the `menu_preview` `MAP_TONE`), so the Workbench's own
## backing and its trailing hairline read against something that is not black.
const MAP_TONE := Color(0.10, 0.15, 0.16)

## The page each state opens. `logs` is registered with no script, so it is the placeholder state.
const TUNING_PAGE := &"config_tuning"
const EQUIPMENT_PAGE := &"equipment"
const KITS_PAGE := &"kits"
const PLACEHOLDER_PAGE := &"logs"

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — a harness that printed an error and still exited 0 was
## indistinguishable from a green one to anything but a human reading stdout.
const EXIT_OK := 0
const EXIT_FAILED := 1

## The dirty state's edits, as `param label -> value`. Each value is a whole number of steps from its
## default, which is what a real click on the stepper produces.
##
## They are deliberately the FIRST group's parameters, and the frame is rendered UNSCROLLED: the
## action bar is pinned outside the shell's scroll now, so a frame showing the top of the page AND
## the bar at once is what proves it. Three of the four sit under one pointer prefix (`/climate/…`)
## and the fourth does not, so the patch assertion below is asked about a nested object with more
## than one key in it.
const DIRTY_EDITS := {
	"Equator temperature": 34.0,
	"Polar temperature": -8.0,
	"Temperate cut point": 22.0,
	"River density": 1.6,
}

var _root: Control
var _bg: ColorRect
var _shell: WorkbenchShell
# How many times `_fail` fired this run — the ONE input to the exit status (see `_finish`).
var _failures := 0


func _ready() -> void:
	_pin_window()
	DirAccess.make_dir_absolute(OUT_DIR)

	# The project stretches `canvas_items` with an `expand` aspect, so the LOGICAL viewport is not the
	# window's pixel size. Anchoring the root to the full rect (rather than sizing it to
	# `PREVIEW_SIZE`) is what makes the map tone fill the frame and the surface run full height.
	_root = Control.new()
	_root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(_root)

	_bg = ColorRect.new()
	_bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_bg.color = MAP_TONE
	_root.add_child(_bg)

	_shell = WorkbenchShell.new()
	# The surface reserves the LEFT edge at its nominal width, full height — the geometry `Main`
	# gives it through the reservation registry.
	_shell.set_anchors_and_offsets_preset(Control.PRESET_LEFT_WIDE)
	_shell.offset_right = WorkbenchVocab.SURFACE_WIDTH
	_root.add_child(_shell)
	await get_tree().process_frame

	# Nothing modified — the read state.
	_shell.show_page(TUNING_PAGE)
	await _settle()
	await _save("workbench_tuning_comfortable")
	_assert_rows_fit()

	# Dirty — modified dots, the counted status line, both actions live, all at the TOP of the page:
	# the action bar is pinned below the scroll, so it is on screen at every scroll position.
	_apply_dirty_edits()
	await _settle()
	await _save("workbench_tuning_dirty")
	_assert_patch_is_sparse()

	# Staged — the same four edits, now APPLIED. Its own frame because it is its own state: the dots
	# stay lit, Apply goes dead (there is nothing left to send) and `Revert all` stays live, because
	# it is the only control that can clear what the server is now holding.
	_install_fake_transport()
	_tuning_page()._on_apply_pressed()
	await _settle()
	await _save("workbench_tuning_staged")

	# Collapsed rail — glyphs only, the content column taking the width back. Reached through the
	# REAL revert (the fake transport accepts the clear), so the page returns to its clean state the
	# way a press returns it.
	_tuning_page()._on_revert_pressed()
	_tuning_page().set_services({})
	_shell.set_rail_collapsed(true)
	await _settle()
	await _save("workbench_rail_collapsed")

	# A declared-but-unbuilt page: the rail entry is live and the body says so.
	_shell.set_rail_collapsed(false)
	_shell.show_page(PLACEHOLDER_PAGE)
	await _settle()
	await _save("workbench_placeholder")

	# Both config pages with NO WORLD — the degradation every page owes this harness, which runs with
	# no server at all. Rendered before the fixture so they cannot be reached only by luck of
	# ordering: a page that crashed or came up blank here could not be iterated on.
	_shell.show_page(EQUIPMENT_PAGE)
	await _settle()
	await _save("workbench_equipment_empty")
	_shell.show_page(KITS_PAGE)
	await _settle()
	await _save("workbench_kits_empty")

	# …and with one. `update_snapshot` fans at the ACTIVE page only, so the frame lands on Kits here
	# and reaches Equipment through the page-switch replay on the line after the save.
	_shell.update_snapshot(_equipment_config_frame(), true)
	await _settle()
	await _save("workbench_kits")
	_assert_equipment_fits(_kits_page(), KITS_PAGE)

	_shell.show_page(EQUIPMENT_PAGE)
	await _settle()
	await _save("workbench_equipment")
	_assert_equipment_fits(_equipment_page(), EQUIPMENT_PAGE)
	_assert_the_pages_print_a_config_no_script_names()
	_assert_the_pages_partition_the_config()
	_assert_the_kits_page_titles_each_entry_by_its_own_name()
	# LAST TWO of the config block, in this order: the reset empties both pages, which is exactly the
	# precondition the catch-up assertion needs — and nothing else may read them after it.
	_assert_equipment_drops_the_world()
	_assert_equipment_catches_up_on_page_switch()

	# Back to the tuning page for the last assertion: `_tuning_page` finds the page in the TREE, and
	# the shell detaches a page it is not showing.
	_shell.show_page(TUNING_PAGE)
	await _settle()
	_assert_staged_survives_un_edit()
	_finish()


## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("workbench_preview: FAIL — %s" % message)


## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place.
func _finish() -> void:
	if _failures > 0:
		print("workbench_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("workbench_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)


# ---- state driving ---------------------------------------------------------

## The page instance the shell built, found by TYPE rather than by reaching into the shell's private
## page table — the shell hands pages out to nobody, and a harness is not a reason to change that.
func _tuning_page() -> ConfigTuningPage:
	for node in _shell.find_children("*", "", true, false):
		if node is ConfigTuningPage:
			return node
	_fail("config tuning page not found")
	return null


## Move the `DIRTY_EDITS` rows off their defaults through the real controls, so the dots, the status
## line and the buttons all settle the way a designer's clicks settle them.
func _apply_dirty_edits() -> void:
	var page := _tuning_page()
	if page == null:
		return
	var moved := 0
	for spin in page.find_children("*", "SpinBox", true, false):
		var label := _row_label_for(spin)
		if DIRTY_EDITS.has(label):
			spin.value = DIRTY_EDITS[label]
			moved += 1
	if moved != DIRTY_EDITS.size():
		_fail("moved %d of %d dirty edits — labels drifted from the manifest"
			% [moved, DIRTY_EDITS.size()])


## The parameter label sitting on the same line as `spin`. The field is wrapped in its own frame, so
## the line is the SpinBox's GRANDparent, and the label is the first `Label` on it that is not the
## modified dot.
func _row_label_for(spin: SpinBox) -> String:
	var frame := spin.get_parent()
	var line: Control = frame.get_parent() if frame != null else null
	if line == null:
		return ""
	var label := _row_label_node(line)
	return label.text if label != null else ""


# ---- assertions ------------------------------------------------------------

## The patch `Apply` emits for the `DIRTY_EDITS`, keyed by config kind. It is written out by hand
## rather than derived from the manifest: a derived expectation would re-run the pointer walk under
## test and agree with it whatever it did.
const EXPECTED_PATCHES := {
	"simulation": {
		"climate": {
			"equator_temp": 34.0,
			"polar_temp": -8.0,
			"temperate_max_temp": 22.0,
		},
		"hydrology": {"river_density": 1.6},
	},
}

## **THE SPARSE CONTRACT, ASSERTED — a picture cannot carry this claim.** Every frame of a page
## whose patch quietly carried all 25 parameters at their defaults would look exactly like these
## ones. So the dirty state presses the REAL button and checks the emitted patch: the one edited
## kind and no others, the nested pointers rebuilt as nested dictionaries, and — the half that
## actually bites — not one untouched parameter anywhere in it.
func _assert_patch_is_sparse() -> void:
	var page := _tuning_page()
	if page == null:
		return
	var emitted: Array = []
	page.overrides_requested.connect(func(patches: Dictionary) -> void: emitted.append(patches))
	page._on_apply_pressed()
	if emitted.size() != 1:
		_fail("expected 1 overrides_requested, got %d" % emitted.size())
		return
	var patches: Dictionary = emitted[0]
	if patches != EXPECTED_PATCHES:
		_fail("patch is not the expected sparse shape:\n  got      %s\n  expected %s"
			% [patches, EXPECTED_PATCHES])
		return
	print("workbench_preview: assert OK — patch is sparse (%d kinds, %d pointers)"
		% [patches.size(), DIRTY_EDITS.size()])


## The row the staged-state assertion drives, and the two values it drives it between.
const STAGED_PARAM := "Lethality"
const STAGED_PARAM_KIND := "combat"
const STAGED_PARAM_POINTER := "lethality"
const STAGED_PARAM_EDITED := 2.0
const STAGED_PARAM_DEFAULT := 1.0

## **THE STATE A PICTURE CANNOT SHOW: the server holds an override the rows no longer admit to.**
## Edit a row, Apply (the server writes a config-override file), then type the default back. Every
## row now reads clean — and the first version of this page therefore said "no overrides", disabled
## BOTH buttons, and left `Revert all` (the only thing that clears the server) unreachable while the
## next new game still booted on the staged value. Every frame of that looked perfectly correct.
##
## So this drives the sequence through the real controls against a FAKE transport that reports
## success, and asserts the three things the frames cannot: that `Revert all` stays reachable, that
## the status line does not claim cleanliness, and that the next patch carries the returned-to-
## default row EXPLICITLY — omitting it would leave the server deep-merging onto the value it
## already has.
func _assert_staged_survives_un_edit() -> void:
	var page := _tuning_page()
	if page == null:
		return
	var sent := _install_fake_transport()
	var spin := _spin_for(page, STAGED_PARAM)
	if spin == null:
		_fail("no '%s' row — the manifest moved" % STAGED_PARAM)
		return
	var failed := 0

	spin.value = STAGED_PARAM_EDITED
	page._on_apply_pressed()
	if sent.size() != 1 or not sent[0].begins_with(WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE):
		failed += 1
		_fail("Apply sent %s, expected one %s"
			% [sent, WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE])
	if not page._apply_button.disabled:
		failed += 1
		_fail("Apply is still live with nothing unsent")

	# The un-edit: back to the shipped default, which used to read as "nothing to do".
	spin.value = STAGED_PARAM_DEFAULT
	if page._revert_button.disabled:
		failed += 1
		_fail("'Revert all' is DEAD while the server still holds an override — it is the only control that can clear it")
	if page._status.text == WorkbenchVocab.TUNING_CLEAN_STATUS:
		failed += 1
		_fail("the status line claims '%s' while the server holds a staged override"
			% WorkbenchVocab.TUNING_CLEAN_STATUS)
	var patch: Dictionary = page.build_patches()
	if not patch.get(STAGED_PARAM_KIND, {}).has(STAGED_PARAM_POINTER):
		failed += 1
		_fail("the returned-to-default row is OMITTED from the patch (%s) — the server would keep the staged value" % patch)

	# SECOND LEG: apply that un-edit. Now every row matches both its default AND what the server was
	# told, so the only fact left is that the server is holding a file — which nothing about the rows
	# can express. `Revert all` must survive on THAT alone.
	page._on_apply_pressed()
	if not page._apply_button.disabled:
		failed += 1
		_fail("Apply is live with every row applied and at its default")
	if page._revert_button.disabled:
		failed += 1
		_fail("'Revert all' is DEAD with all rows clean but the server still holding a staged override file — clearing it is unreachable")
	if page._status.text != WorkbenchVocab.TUNING_STAGED_CLEARED_STATUS:
		failed += 1
		_fail("status reads '%s', expected the staged-but-defaulted line '%s'"
			% [page._status.text, WorkbenchVocab.TUNING_STAGED_CLEARED_STATUS])
	if failed == 0:
		print("workbench_preview: assert OK — an un-edited row keeps Revert reachable, the status honest, and the patch explicit")


## Lend the page a transport that ACCEPTS everything, so the states past a successful Apply can be
## reached with no server. Answers the array the sent command lines land in.
##
## It returns `true` rather than nothing on purpose: `WorkbenchPage.send_command` treats anything but
## `true` as "did not send", which is the contract `Main`'s real sender honours.
func _install_fake_transport() -> Array:
	var sent: Array = []
	var page := _tuning_page()
	if page != null:
		page.set_services({
			WorkbenchVocab.SERVICE_SEND_COMMAND: func(line: String) -> bool:
				sent.append(line)
				return true,
			WorkbenchVocab.SERVICE_NEW_GAME: func() -> void: pass,
		})
	return sent


## The `SpinBox` on the row labelled `label`.
func _spin_for(page: ConfigTuningPage, label: String) -> SpinBox:
	for spin in page.find_children("*", "SpinBox", true, false):
		if _row_label_for(spin) == label:
			return spin
	return null


## Sub-pixel slack when comparing a laid-out rect against the box it has to fit in.
const FIT_TOLERANCE := 0.5

## **EVERY ROW LABEL FITS ON ONE LINE, ASSERTED ACROSS ALL OF THEM.** `CONTROL_WIDTH` is chosen by
## what the LABEL column needs, and only about a third of the manifest's rows are on screen in any
## one frame — so the frames can show the setting is right for the rows they hold and say nothing
## about the other two thirds. This walks all of them.
##
## **A ROW THAT DOES NOT FIT DOES NOT WRAP — IT PUSHES THE SURFACE WIDER, and that is why this is an
## assertion and not an eyeball.** The label carries no autowrap, so an over-long one raises the
## row's minimum width; nothing clamps it (`ScrollContainer` grows its child to the child's minimum),
## so the whole content column swells past `SURFACE_WIDTH` and draws over the map. In the frame it
## reads as a slightly wide panel — nothing that says "the label did not fit".
##
## Hence the decisive check: the CONTENT COLUMN's own width against the width the surface's geometry
## says it should have. Two per-row checks ride with it for the failures that look different — the
## label WRAPPING (line count climbs, every row under it loses its alignment) and the label being
## CLIPPED (squeezed under its own minimum, a truncated name with no ellipsis to admit it) — and one
## more asks that a row leave room for the scrollbar it shares its right edge with, since a row that
## merely reaches the edge draws under it.
func _assert_rows_fit() -> void:
	var page := _tuning_page()
	if page == null:
		return
	var scroll := _content_scroll()
	if scroll == null:
		_fail("no content scroll to measure rows against")
		return

	var failed := 0
	# What the geometry says the content column is: the surface, less the rail and the padding either
	# side of it. Anything wider means a row has pushed the surface out of shape.
	var nominal_width := WorkbenchVocab.SURFACE_WIDTH - WorkbenchVocab.RAIL_WIDTH \
		- 2.0 * WorkbenchVocab.CONTENT_PADDING
	if scroll.size.x > nominal_width + FIT_TOLERANCE:
		failed += 1
		_fail("content column is %.1fpx wider than the surface allows (%.1f > %.1f) — a row does not fit"
			% [scroll.size.x - nominal_width, scroll.size.x, nominal_width])
	var row_limit := scroll.size.x - scroll.get_v_scroll_bar().size.x

	var checked := 0
	for spin in page.find_children("*", "SpinBox", true, false):
		var frame: Control = spin.get_parent()
		var line: Control = frame.get_parent() if frame != null else null
		if line == null:
			continue
		var label := _row_label_node(line)
		if label == null:
			continue
		checked += 1
		if label.get_line_count() > 1:
			failed += 1
			_fail("row label wraps to %d lines: '%s'"
				% [label.get_line_count(), label.text])
		if label.size.x + FIT_TOLERANCE < label.get_minimum_size().x:
			failed += 1
			_fail("row label is clipped (%.1f < %.1f): '%s'"
				% [label.size.x, label.get_minimum_size().x, label.text])
		if line.size.x > row_limit + FIT_TOLERANCE:
			failed += 1
			_fail("row runs under the scrollbar (%.1f > %.1f): '%s'"
				% [line.size.x, row_limit, label.text])
	if checked == 0:
		_fail("no rows measured — the row shape moved")
		return
	if failed == 0:
		print("workbench_preview: assert OK — %d rows fit at control width %.0f (widest row has %.0fpx to spare)"
			% [checked, WorkbenchWidgets.CONTROL_WIDTH, row_limit - _widest_row(page)])


## The widest laid-out row line on the page — the one the fit has least slack on.
func _widest_row(page: ConfigTuningPage) -> float:
	var widest := 0.0
	for spin in page.find_children("*", "SpinBox", true, false):
		var frame: Control = spin.get_parent()
		var line: Control = frame.get_parent() if frame != null else null
		if line != null:
			widest = maxf(widest, line.size.x)
	return widest


## The name Label on a row's first line — the one that is not the modified dot.
func _row_label_node(line: Control) -> Label:
	for sibling in line.get_children():
		if sibling is Label and sibling.text != WorkbenchVocab.MODIFIED_GLYPH:
			return sibling
	return null


## The shell's content scroll, found by type — the surface's one `ScrollContainer`.
func _content_scroll() -> ScrollContainer:
	for node in _shell.find_children("*", "ScrollContainer", true, false):
		return node
	return null


# ---- the two config pages --------------------------------------------------

## **THE INVENTED KEYS, AND THEY ARE THE POINT OF THIS WHOLE FIXTURE.**
##
## `wear_per_turn_carried` and `windbreak_kit` are not in `equipment.json` and are named by NO
## GDScript on the shipped surface — a field inside a gear block and a whole top-level block that
## exist only here. Both must render, because that is the entire claim the two pages make: they print
## whatever the config contains, so a field the sim adds tomorrow arrives with no client edit and a
## renamed one renames itself on screen. A page holding a list of field names would draw the three
## real keys of `hunting_kit` perfectly and silently skip the fourth, which looks exactly like a
## correct page.
##
## `spare_kit` is an empty object and `none`'s `uses` is an empty array — the two shapes that must
## render an explicit `—` rather than a blank right-hand column, which reads as a rendering fault
## rather than as the config's own answer.
const CONFIG_INVENTED_FIELD := "wear_per_turn_carried"
const CONFIG_INVENTED_BLOCK := "windbreak_kit"
const CONFIG_EMPTY_BLOCK := "spare_kit"
## **AND A THIRD, INSIDE A ROSTER ENTRY, because the Kits page is the one that got an exception.**
## That page promotes `display_name` and `id` into its block title; the two invented keys above both
## live on the EQUIPMENT page, so before this one existed a "simplification" of `KitsPage` down to a
## fixed jobs+uses body would have passed every assertion in this file. This key is what makes the
## Kits page's body generic as a *tested* fact rather than a claim in a docstring.
const CONFIG_INVENTED_KIT_FIELD := "morale_bonus"
## A gear block and a kit id the REAL config carries, so the fixture is recognisably the shipped
## shape and the partition assertion has something honest to point at.
const CONFIG_GEAR_BLOCK := "hunting_kit"
const CONFIG_HUNT_KIT_ID := "big_game"
const CONFIG_HUNT_JOB := "hunt"
const CONFIG_HUNT_KIT_DISPLAY := "Stalking kit"

## **THE FOUR ROSTER ENTRIES ARE THE FOUR TITLE CASES**, so the Kits page's degradation is exercised
## rather than described. `[0]` states both keys, `[1]` only `id`, `[2]` only `display_name`, and
## `[3]` neither — the last is what keeps the `kits[N]` fallback REACHABLE, which it would not be on a
## roster where every entry names itself.
const CONFIG_ID_ONLY_KIT_INDEX := 1
const CONFIG_ID_ONLY_KIT_ID := "gathering"
const CONFIG_NAME_ONLY_KIT_INDEX := 2
const CONFIG_NAME_ONLY_KIT_DISPLAY := "No kit"
const CONFIG_ANONYMOUS_KIT_INDEX := 3

## The titles the page must compose, spelled out LITERALLY rather than rebuilt from the fixture
## through the page's own format and branches — a derived expectation would re-run the logic under
## test and agree with whatever it did.
const CONFIG_HUNT_KIT_TITLE := "Stalking kit (big_game)"
const CONFIG_ID_ONLY_KIT_TITLE := "gathering"
const CONFIG_NAME_ONLY_KIT_TITLE := "No kit"
const CONFIG_ANONYMOUS_KIT_TITLE := "kits[3]"
## The roster field carrying both array shapes the tree has to tell apart: two components on `big_game`
## (comma-joined onto one row) and none at all on `none`, which is an ordinary roster member and not a
## sentinel — so its empty array has to say `—`.
const CONFIG_USES_KEY := "uses"


## The whole effective `EquipmentConfig` in its serialized shape — the Rust struct's own field names,
## the three shipped gear blocks, the roster and the job defaults, PLUS the invented field and the
## invented block above.
static func _equipment_config() -> Dictionary:
	return {
		CONFIG_GEAR_BLOCK: {
			"equipped_attack": 20.0,
			"starting_durability": 100.0,
			"wear_per_kill": 0.4,
			CONFIG_INVENTED_FIELD: 0.05,
		},
		"sled_kit": {
			"unequipped_per_worker_biomass_capacity": 12.0,
			"starting_durability": 100.0,
			"wear_per_biomass_hauled": 0.02,
		},
		"basket_kit": {
			"unequipped_per_worker_biomass_capacity": 1.6,
			"starting_durability": 100.0,
			"wear_per_biomass_gathered": 0.04,
		},
		CONFIG_INVENTED_BLOCK: {
			"equipped_cold_tolerance": 6.0,
			"starting_durability": 100.0,
			"wear_per_turn_sheltered": 0.1,
		},
		CONFIG_EMPTY_BLOCK: {},
		WorkbenchVocab.CONFIG_KITS_KEY: [
			# [0] BOTH keys — the ordinary entry, and the one carrying the invented kit field.
			{
				WorkbenchVocab.CONFIG_KIT_ID_KEY: CONFIG_HUNT_KIT_ID,
				WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY: CONFIG_HUNT_KIT_DISPLAY,
				"jobs": [CONFIG_HUNT_JOB], CONFIG_USES_KEY: [CONFIG_GEAR_BLOCK, "sled_kit"],
				CONFIG_INVENTED_KIT_FIELD: 3.0,
			},
			# [1] `id` only — the title is the id and ONLY the `id` row is suppressed.
			{
				WorkbenchVocab.CONFIG_KIT_ID_KEY: CONFIG_ID_ONLY_KIT_ID,
				"jobs": ["forage"], CONFIG_USES_KEY: ["basket_kit"],
			},
			# [2] `display_name` only — the mirror case, and the empty `uses` array.
			{
				WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY: CONFIG_NAME_ONLY_KIT_DISPLAY,
				"jobs": [CONFIG_HUNT_JOB, "forage"], CONFIG_USES_KEY: [],
			},
			# [3] NEITHER — the entry that keeps the walker's own `kits[N]` fallback reachable.
			{"jobs": ["forage"], CONFIG_USES_KEY: ["basket_kit"]},
		],
		WorkbenchVocab.CONFIG_DEFAULT_KITS_KEY: {
			CONFIG_HUNT_JOB: CONFIG_HUNT_KIT_ID,
			"forage": "gathering",
		},
	}


## One full frame carrying that config the way the wire carries it: a `serde_json` STRING under
## `SubsistenceSection.equipmentConfigJson`, decoded onto the frame dict as `equipment_config_json`.
## Both pages parse it themselves, so the harness hands over the string and nothing else.
##
## **`sort_keys` is OFF deliberately.** It defaults to TRUE, which would hand the pages an
## alphabetised config no server can produce — `serde_json` writes a struct in its declared field
## order, and the pages render in the order they are given, so a sorted fixture would render a frame
## the live client never shows.
static func _equipment_config_frame() -> Dictionary:
	return {WorkbenchVocab.CONFIG_JSON_KEY: JSON.stringify(_equipment_config(), "", false)}


## **THE PAGES ARE FOUND BY TYPE WHILE ON SCREEN AND REMEMBERED AFTERWARDS.** The shell DETACHES the
## page it is not showing, so a plain tree search only ever finds the ACTIVE one — and two of the
## claims below are precisely about a page that is not active: `reset_pages()` fans out at every BUILT
## page, and the catch-up replay is asked of a page that was away when the frame landed. Remembering
## the node is what keeps the harness out of the shell's private page table, which it hands to nobody.
var _remembered_equipment_page: EquipmentPage = null
var _remembered_kits_page: KitsPage = null


func _equipment_page() -> EquipmentPage:
	if _remembered_equipment_page == null:
		for node in _shell.find_children("*", "", true, false):
			if node is EquipmentPage:
				_remembered_equipment_page = node
				break
	if _remembered_equipment_page == null:
		_fail("equipment page not found")
	return _remembered_equipment_page


func _kits_page() -> KitsPage:
	if _remembered_kits_page == null:
		for node in _shell.find_children("*", "", true, false):
			if node is KitsPage:
				_remembered_kits_page = node
				break
	if _remembered_kits_page == null:
		_fail("kits page not found")
	return _remembered_kits_page


## Does this page draw a Label whose text is EXACTLY `text`? The tree prints a config key verbatim,
## so an exact match on a key is an exact match on the row that key labels — a `contains` would let
## `wear_per_kill` satisfy a claim about `wear_per_kill_bonus`.
func _page_states(page: WorkbenchPage, text: String) -> bool:
	if page == null:
		return false
	for node in page.find_children("*", "Label", true, false):
		if (node as Label).text == text:
			return true
	return false


## Every VALUE rendered opposite a row keyed `key`, anywhere under `root`, in render order.
##
## **The key is the row's FIRST label and the value its second, and reading the row positionally is
## not fussiness.** A value is often a string that some other row uses as a KEY — the fixture's
## `kits[0].jobs` renders the value `hunt` while `default_kits` renders the key `hunt` — so a search
## for "the Label whose text is `hunt`, then its sibling" answers `jobs` and asserts the opposite of
## what it was asked. `root` is a `Node` rather than a page so a claim can be scoped to ONE block:
## `starting_durability` occurs in four of them, and a page-wide search would answer whichever came
## first.
func _config_row_faces(root: Node, key: String) -> Array[String]:
	var out: Array[String] = []
	if root == null:
		return out
	for node in root.find_children("*", "HBoxContainer", true, false):
		var labels: Array[Label] = []
		for child in (node as HBoxContainer).get_children():
			if child is Label:
				labels.append(child)
		if labels.size() == 2 and labels[0].text == key:
			out.append(labels[1].text)
	return out


## The single value opposite `key` under `root`, or `""` when no such row rendered.
func _config_row_face(root: Node, key: String) -> String:
	var faces := _config_row_faces(root, key)
	return faces[0] if not faces.is_empty() else ""


## The body of the named block — the subtree an assertion scopes itself to when a key it is asking
## about is spelled the same in several blocks. A block is a `VBoxContainer` whose FIRST child is its
## own name Label, which is the shape `WorkbenchWidgets._config_block` builds.
func _config_block_body(page: WorkbenchPage, name: String) -> Node:
	if page == null:
		return null
	for node in page.find_children("*", "VBoxContainer", true, false):
		var block: VBoxContainer = node
		if block.get_child_count() == 0:
			continue
		var head := block.get_child(0)
		if head is Label and (head as Label).text == name:
			return block
	return null


## The shipped scripts a hardcoded field list could hide in. The harness itself is deliberately NOT
## in this list: it is the thing that invents the keys.
const CONFIG_PAGE_SOURCES := [
	"res://src/scripts/ui/workbench/pages/EquipmentPage.gd",
	"res://src/scripts/ui/workbench/pages/KitsPage.gd",
	"res://src/scripts/ui/workbench/WorkbenchWidgets.gd",
	"res://src/scripts/ui/workbench/WorkbenchVocab.gd",
]


## **THE ASSERTION THIS WHOLE DESIGN RESTS ON, AND THE ONLY THING STANDING BETWEEN IT AND SOMEONE
## QUIETLY REINTRODUCING A HARDCODED FIELD LIST.**
##
## The pages must print whatever the config contains. The fixture therefore carries a field no
## GDScript anywhere names (`wear_per_turn_carried`, inside a real gear block) and a whole top-level
## block no GDScript anywhere names (`windbreak_kit`), and both must render. A page that walked a
## hand-written list of field names would draw the three real keys of `hunting_kit` and silently drop
## the fourth — a page that looks entirely correct, which is exactly why no picture can carry this
## claim and why the previous page's roster had to go: another session is renaming the config's gear
## blocks right now, and a list of field names breaks on their merge without saying so.
##
## **AND IT HAS TO REACH THE KITS PAGE, because that page was given an exception.** Kits PROMOTES
## `display_name` and `id` into its block title and hides those two rows — a bounded, deliberate piece
## of field knowledge (`WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY`) that would be trivial to grow
## into a whitelist of "the fields a kit has". The two invented keys above both live on the EQUIPMENT
## page, so before `morale_bonus` existed a `KitsPage` simplified down to a fixed jobs+uses body would
## have passed every assertion in this file. The third invented key is what makes the Kits page's
## BODY generic as a tested fact rather than a claim in a docstring.
##
## Four legs plus the vacuity guards:
##   - the invented FIELD renders on Equipment, with the value it was given;
##   - the invented BLOCK renders on Equipment, with its own children under it;
##   - the invented KIT FIELD renders on Kits, inside the promoted block, with its value;
##   - none of the three strings appears in any shipped script, so the renders above cannot have come
##     from a page that happens to name them (this leg cannot see an allow-list of REAL key names —
##     the rendered legs are what catch that — but it does catch the allow-list that names the new
##     key too);
##   - the guards: the fixture really does carry all three, in the shapes claimed.
func _assert_the_pages_print_a_config_no_script_names() -> void:
	var page := _equipment_page()
	var kits := _kits_page()
	if page == null or kits == null:
		return
	var config := _equipment_config()
	var gear: Dictionary = config.get(CONFIG_GEAR_BLOCK, {})
	var roster: Array = config[WorkbenchVocab.CONFIG_KITS_KEY]
	var first_kit: Dictionary = roster[0]
	if not gear.has(CONFIG_INVENTED_FIELD):
		_fail("the fixture's '%s' block does not carry the invented field '%s' — this assertion would prove nothing"
			% [CONFIG_GEAR_BLOCK, CONFIG_INVENTED_FIELD])
		return
	if not config.has(CONFIG_INVENTED_BLOCK):
		_fail("the fixture carries no invented top-level block '%s' — this assertion would prove nothing"
			% CONFIG_INVENTED_BLOCK)
		return
	if not first_kit.has(CONFIG_INVENTED_KIT_FIELD):
		_fail("the fixture's first roster entry does not carry the invented field '%s' — nothing would then pin the KITS page's body as generic"
			% CONFIG_INVENTED_KIT_FIELD)
		return

	var failed := 0
	var gear_body := _config_block_body(page, CONFIG_GEAR_BLOCK)
	var expected_field_face := WorkbenchWidgets.config_value_face(gear[CONFIG_INVENTED_FIELD])
	var field_face := _config_row_face(gear_body, CONFIG_INVENTED_FIELD)
	if field_face != expected_field_face:
		failed += 1
		_fail("the config field '%s.%s' — which no GDScript names — did not render its value (wanted '%s', got '%s'). A hardcoded field list is back."
			% [CONFIG_GEAR_BLOCK, CONFIG_INVENTED_FIELD, expected_field_face, field_face])
	var invented_body := _config_block_body(page, CONFIG_INVENTED_BLOCK)
	if invented_body == null:
		failed += 1
		_fail("the whole config block '%s' — which no GDScript names — did not render at all. A hardcoded block list is back."
			% CONFIG_INVENTED_BLOCK)
	else:
		var invented_block: Dictionary = config.get(CONFIG_INVENTED_BLOCK, {})
		for child_key in invented_block:
			var wanted := WorkbenchWidgets.config_value_face(invented_block[child_key])
			var got := _config_row_face(invented_body, String(child_key))
			if got != wanted:
				failed += 1
				_fail("'%s.%s' did not render (wanted '%s', got '%s')"
					% [CONFIG_INVENTED_BLOCK, child_key, wanted, got])

	# …and the same claim on the KITS page, inside the block whose title consumed two other keys: the
	# promotion must suppress exactly those two and leave the rest of the entry walked blind.
	var kit_body := _config_block_body(kits, CONFIG_HUNT_KIT_TITLE)
	if kit_body == null:
		failed += 1
		_fail("no roster block titled '%s' on the Kits page" % CONFIG_HUNT_KIT_TITLE)
	else:
		var wanted_kit_face := WorkbenchWidgets.config_value_face(first_kit[CONFIG_INVENTED_KIT_FIELD])
		var kit_face := _config_row_face(kit_body, CONFIG_INVENTED_KIT_FIELD)
		if kit_face != wanted_kit_face:
			failed += 1
			_fail("the kit field '%s' — which no GDScript names — did not render its value under '%s' (wanted '%s', got '%s'). The Kits page's title promotion has become a whitelist."
				% [CONFIG_INVENTED_KIT_FIELD, CONFIG_HUNT_KIT_TITLE, wanted_kit_face, kit_face])

	for path in CONFIG_PAGE_SOURCES:
		var source := FileAccess.get_file_as_string(path)
		if source.is_empty():
			failed += 1
			_fail("could not read %s — the source scan is asserting nothing" % path)
			continue
		for invented in [CONFIG_INVENTED_FIELD, CONFIG_INVENTED_BLOCK, CONFIG_INVENTED_KIT_FIELD]:
			if source.contains(invented):
				failed += 1
				_fail("%s names '%s'. These three keys exist ONLY in this fixture; a shipped script naming one means the pages have started listing fields again."
					% [path, invented])

	# The empty shapes, asserted here because they are the other half of "print what is there": an
	# empty object and an empty array must SAY they are empty rather than draw a blank column.
	if _config_row_face(page, CONFIG_EMPTY_BLOCK) != WorkbenchVocab.CONFIG_EMPTY:
		failed += 1
		_fail("the empty block '%s' renders '%s', not the explicit '%s'"
			% [CONFIG_EMPTY_BLOCK, _config_row_face(page, CONFIG_EMPTY_BLOCK),
				WorkbenchVocab.CONFIG_EMPTY])
	if failed == 0:
		print("workbench_preview: assert OK — the pages printed '%s', '%s' and '%s', which no shipped script names (%d sources scanned)"
			% [CONFIG_INVENTED_FIELD, CONFIG_INVENTED_BLOCK, CONFIG_INVENTED_KIT_FIELD,
				CONFIG_PAGE_SOURCES.size()])


## **THE TWO PAGES PARTITION THE CONFIG'S TOP LEVEL, AND NO FRAME CAN SAY SO** — each page renders a
## plausible tree of config keys, and whether the roster is on the right one of them is a question
## about the OTHER page, which is not in the picture.
##
## Kits owns exactly `kits` and `default_kits`; Equipment owns everything else, whatever it turns out
## to be. Both directions are asserted, because a page that drew the whole config would satisfy either
## half alone, and each leg carries its own positive so a page that rendered nothing cannot pass.
func _assert_the_pages_partition_the_config() -> void:
	var equipment := _equipment_page()
	var kits := _kits_page()
	if equipment == null or kits == null:
		return
	# The roster's presence is read off the block TITLE the Kits page composes, since the walker's
	# `kits[0]` coordinate is exactly what that title replaced.
	var roster_block := CONFIG_HUNT_KIT_TITLE

	var failed := 0
	if not _page_states(kits, roster_block):
		failed += 1
		_fail("the Kits page does not render the roster entry '%s'" % roster_block)
	if _config_row_face(kits, CONFIG_HUNT_JOB) != CONFIG_HUNT_KIT_ID:
		failed += 1
		_fail("the Kits page does not state the '%s' job default as '%s' (got '%s')"
			% [CONFIG_HUNT_JOB, CONFIG_HUNT_KIT_ID, _config_row_face(kits, CONFIG_HUNT_JOB)])
	if _page_states(equipment, roster_block):
		failed += 1
		_fail("the kit roster ('%s') rendered on the Equipment page, which owns everything the Kits page does NOT"
			% roster_block)

	if not _page_states(equipment, CONFIG_INVENTED_BLOCK):
		failed += 1
		_fail("the gear block '%s' does not render on the Equipment page"
			% CONFIG_INVENTED_BLOCK)
	if _page_states(kits, CONFIG_INVENTED_BLOCK):
		failed += 1
		_fail("the gear block '%s' rendered on the Kits page, which owns only '%s' and '%s'"
			% [CONFIG_INVENTED_BLOCK, WorkbenchVocab.CONFIG_KITS_KEY,
				WorkbenchVocab.CONFIG_DEFAULT_KITS_KEY])

	# The roster's `uses` rows carry BOTH array shapes: `kits[0]` uses two components and must render
	# them comma-joined on ONE row, while the bare kit uses none and must say `—` rather than draw a
	# blank column. Both are asserted here because the roster is the only place either shape occurs,
	# and the Kits page is the only page the roster reaches.
	var roster: Array = _equipment_config()[WorkbenchVocab.CONFIG_KITS_KEY]
	var uses_faces := _config_row_faces(kits, CONFIG_USES_KEY)
	if uses_faces.size() != roster.size():
		failed += 1
		_fail("the Kits page rendered %d '%s' rows for a roster of %d entries"
			% [uses_faces.size(), CONFIG_USES_KEY, roster.size()])
	var joined: String = WorkbenchVocab.CONFIG_LIST_SEPARATOR.join(
		PackedStringArray(roster[0][CONFIG_USES_KEY]))
	if not uses_faces.has(joined):
		failed += 1
		_fail("'%s.%s' does not render its components on one row as '%s' — got %s"
			% [CONFIG_HUNT_KIT_TITLE, CONFIG_USES_KEY, joined, uses_faces])
	if not uses_faces.has(WorkbenchVocab.CONFIG_EMPTY):
		failed += 1
		_fail("the bare kit's empty '%s' array renders no explicit '%s' — got %s"
			% [CONFIG_USES_KEY, WorkbenchVocab.CONFIG_EMPTY, uses_faces])
	if failed == 0:
		print("workbench_preview: assert OK — Kits draws the roster and the defaults, Equipment draws the %d other block(s), and neither draws the other's"
			% (_equipment_config().size() - 2))


## **THE ONE PIECE OF FIELD KNOWLEDGE ON EITHER PAGE, AND ITS FOUR DEGRADATIONS.**
##
## `kits[0]` is a coordinate that names nothing a reader can use, so the Kits page promotes a roster
## entry's `display_name` and `id` into the block's title — and then hides ONLY the rows the title
## consumed. Two things have to hold for that to stay an exception rather than a whitelist, and
## neither is visible in a frame: that the promoted rows are gone from the BODY (a title plus an `id`
## row reads as correct, just repetitive), and that nothing ELSE is gone
## (`_assert_the_pages_print_a_config_no_script_names` owns that half, through a kit field no script
## names).
##
## A roster entry can be edited into any of four shapes and each promotes only what it can use, so all
## four are fixtured and asserted:
##   [0] both keys  → `display_name (id)`, both rows gone;
##   [1] `id` only  → the id alone, and the `display_name` row was never there to lose;
##   [2] name only  → the display name alone, `id` untouched;
##   [3] neither    → the walker's own `kits[3]`, whole entry still in the body.
##
## **The fourth is the one that rots first.** A roster where every entry names itself leaves the
## `kits[N]` fallback unreachable, and an unreachable branch reads as covered — so the fixture carries
## an anonymous entry for no other purpose.
func _assert_the_kits_page_titles_each_entry_by_its_own_name() -> void:
	var kits := _kits_page()
	if kits == null:
		return
	var roster: Array = _equipment_config()[WorkbenchVocab.CONFIG_KITS_KEY]
	var display_key := WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY
	var id_key := WorkbenchVocab.CONFIG_KIT_ID_KEY

	# Each case must actually BE the shape it claims, or the branch it stands for is untested.
	var shapes := {
		CONFIG_HUNT_KIT_TITLE: [true, true],
		CONFIG_ID_ONLY_KIT_TITLE: [false, true],
		CONFIG_NAME_ONLY_KIT_TITLE: [true, false],
		CONFIG_ANONYMOUS_KIT_TITLE: [false, false],
	}
	var indices := [0, CONFIG_ID_ONLY_KIT_INDEX, CONFIG_NAME_ONLY_KIT_INDEX,
		CONFIG_ANONYMOUS_KIT_INDEX]
	var titles := shapes.keys()
	for slot in titles.size():
		var entry: Dictionary = roster[indices[slot]]
		var wanted: Array = shapes[titles[slot]]
		if entry.has(display_key) != bool(wanted[0]) or entry.has(id_key) != bool(wanted[1]):
			_fail("roster entry %d is not the shape '%s' stands for (display_name %s, id %s) — that title branch is untested"
				% [indices[slot], titles[slot], entry.has(display_key), entry.has(id_key)])
			return

	var failed := 0
	for slot in titles.size():
		var title: String = titles[slot]
		var wanted: Array = shapes[title]
		var body := _config_block_body(kits, title)
		if body == null:
			failed += 1
			_fail("no roster block titled '%s' — the Kits page did not compose the title for entry %d"
				% [title, indices[slot]])
			continue
		# A key the title USED must be gone from the body; a key it could not use must still be there.
		for pair in [[display_key, bool(wanted[0])], [id_key, bool(wanted[1])]]:
			var key: String = pair[0]
			var promoted: bool = pair[1]
			var present := not _config_row_faces(body, key).is_empty()
			if promoted and present:
				failed += 1
				_fail("'%s' still carries a '%s' row — the title already said it, so the row is a repetition"
					% [title, key])
			elif not promoted and _entry_states(roster[indices[slot]], key) and not present:
				failed += 1
				_fail("'%s' lost its '%s' row, which the title never used — the promotion is suppressing more than it promoted"
					% [title, key])
		# Every entry keeps the keys the title had no claim on.
		for kept in ["jobs", CONFIG_USES_KEY]:
			if _config_row_faces(body, kept).is_empty():
				failed += 1
				_fail("'%s' has no '%s' row — the block body is not the generic tree"
					% [title, kept])
	if failed == 0:
		print("workbench_preview: assert OK — the Kits page titles all %d entries by their own names (both keys, id only, name only, and the '%s' fallback), suppressing only what each title used"
			% [roster.size(), CONFIG_ANONYMOUS_KIT_TITLE])


func _entry_states(entry: Dictionary, key: String) -> bool:
	return entry.has(key)


## **`reset()` IS REAL ON BOTH CONFIG PAGES, which is the counter-case to `ConfigTuningPage`'s
## documented no-op** — and a doc claim with no test is how the two get confused. Everything either
## page holds is the ended world's config, and it is re-sent ONLY on a world rebuild, so a page that
## kept it would show the previous world's tunables indefinitely rather than for a frame.
##
## Driven through the SHELL's `reset_pages()`, the way `Main`'s per-world reset reaches it, rather
## than by calling either page's hook directly — the fan-out at every BUILT page is part of the
## contract, and the Kits page is not even on screen when this runs, which is precisely the case that
## fan-out exists for. The precondition is that there was something to drop.
func _assert_equipment_drops_the_world() -> void:
	var equipment := _equipment_page()
	var kits := _kits_page()
	if equipment == null or kits == null:
		return
	if not _page_states(equipment, CONFIG_INVENTED_BLOCK) \
			or not _page_states(kits, CONFIG_HUNT_KIT_TITLE):
		_fail("nothing was on the config pages to drop — the reset claim would pass vacuously")
		return

	_shell.reset_pages()

	var failed := 0
	if _page_states(equipment, CONFIG_INVENTED_BLOCK):
		failed += 1
		_fail("the gear block '%s' survived the world boundary on the Equipment page"
			% CONFIG_INVENTED_BLOCK)
	if _page_states(kits, CONFIG_HUNT_KIT_TITLE):
		failed += 1
		_fail("the kit roster survived the world boundary on the Kits page")
	if not _page_states(equipment, WorkbenchVocab.EQUIPMENT_NO_CONFIG):
		failed += 1
		_fail("the Equipment page does not say '%s' after reset_pages()"
			% WorkbenchVocab.EQUIPMENT_NO_CONFIG)
	if not _page_states(kits, WorkbenchVocab.KITS_NO_CONFIG):
		failed += 1
		_fail("the Kits page does not say '%s' after reset_pages()"
			% WorkbenchVocab.KITS_NO_CONFIG)
	if failed == 0:
		print("workbench_preview: assert OK — reset_pages() drops the config on both pages, including the one that is not on screen")


## **A PAGE ACTIVATED BETWEEN FRAMES MUST CATCH UP ON THE FRAME ALREADY IN HAND, and no picture can
## carry that**: a page that has never been fed and a page fed an empty world render the same thing —
## the degraded "nothing on the wire yet" line — so the defect and the fix are the same frame.
##
## It is the shell's claim, not the page's. `update_snapshot` fans at the ACTIVE page only, and
## snapshots arrive on turn resolution and world-mutating commands, with no heartbeat behind them; so
## before `show_page` replayed the cached frame, opening this page mid-turn showed its degraded state
## until the next turn resolved. Driven the way a designer reaches it: a frame lands while ANOTHER
## page is up, then the rail switches here.
##
## Its precondition is the reset immediately above, which is why it runs last — the page must be empty
## first, or a page that never caught up passes on what the previous state left behind.
func _assert_equipment_catches_up_on_page_switch() -> void:
	var page := _equipment_page()
	if page == null:
		return
	if _page_states(page, CONFIG_INVENTED_BLOCK):
		_fail("the Equipment page is not empty before the page-switch replay — the catch-up claim would pass vacuously")
		return

	# The frame arrives while the TUNING page is active, so it is fanned at that page and never at this
	# one; the rail switch afterwards is the only thing that can put it here.
	_shell.show_page(TUNING_PAGE)
	_shell.update_snapshot(_equipment_config_frame(), true)
	_shell.show_page(EQUIPMENT_PAGE)

	var failed := 0
	if not _page_states(page, CONFIG_INVENTED_BLOCK):
		failed += 1
		_fail("the gear block '%s' is missing after switching to the Equipment page — the page never saw the frame that landed while another page was active"
			% CONFIG_INVENTED_BLOCK)
	if _page_states(page, WorkbenchVocab.EQUIPMENT_NO_CONFIG):
		failed += 1
		_fail("the Equipment page still says '%s' after the page switch — the config did not come back with the replayed frame"
			% WorkbenchVocab.EQUIPMENT_NO_CONFIG)
	if failed == 0:
		print("workbench_preview: assert OK — a page switched to between frames catches up on the cached frame ('%s')"
			% CONFIG_INVENTED_BLOCK)


## A config page's half of "a row that does not fit swells the whole column".
##
## **THE COLUMN WIDTH IS THE DECISIVE CHECK, and on these pages it is very nearly the only one that
## can fire.** Every line the tree draws except one is a `build_caption`, which wraps; what does not
## wrap is a BLOCK NAME, and an over-long one raises its block's minimum width, the `ScrollContainer`
## grows to it, and the content column swells past `SURFACE_WIDTH` and draws over the map. That reads
## as a slightly wide panel, not as a broken row. The per-label clip check rides beside it for the
## opposite failure — a label squeezed under its own minimum, i.e. a truncated name with no ellipsis
## to admit it.
##
## **It is asked of a page rather than of THE page**, because the config's shape is the sim's to
## choose: the roster and the gear blocks nest differently and reach different widths, so measuring
## one of them says nothing about the other.
func _assert_equipment_fits(page: WorkbenchPage, id: StringName) -> void:
	if page == null:
		return
	var scroll := _content_scroll()
	if scroll == null:
		_fail("no content scroll to measure the %s page against" % id)
		return

	var failed := 0
	var nominal_width := WorkbenchVocab.SURFACE_WIDTH - WorkbenchVocab.RAIL_WIDTH \
		- 2.0 * WorkbenchVocab.CONTENT_PADDING
	if scroll.size.x > nominal_width + FIT_TOLERANCE:
		failed += 1
		_fail("the %s page's content column is %.1fpx wider than the surface allows (%.1f > %.1f) — a label does not fit"
			% [id, scroll.size.x - nominal_width, scroll.size.x, nominal_width])

	var checked := 0
	var widest := 0.0
	for node in page.find_children("*", "Label", true, false):
		var label: Label = node
		if label.autowrap_mode != TextServer.AUTOWRAP_OFF:
			continue
		checked += 1
		widest = maxf(widest, label.get_minimum_size().x)
		if label.size.x + FIT_TOLERANCE < label.get_minimum_size().x:
			failed += 1
			_fail("%s label is clipped (%.1f < %.1f): '%s'"
				% [id, label.size.x, label.get_minimum_size().x, label.text])
	if checked == 0:
		_fail("no non-wrapping %s labels measured — the block shape moved" % id)
		return
	if failed == 0:
		print("workbench_preview: assert OK — %d non-wrapping %s labels fit (the widest needs %.0f of %.0fpx)"
			% [checked, id, widest, nominal_width])



# ---- capture ---------------------------------------------------------------

## macOS applies a window mode/size change asynchronously, so the window is re-pinned
## here and again from `_settle` — the treatment `blend_probe`/`map_preview` carry. Without it a
## frame silently renders at the monitor's size and the surface is judged at a width it never ships
## at.
func _pin_window() -> void:
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = PREVIEW_SIZE


## Pinned TWICE around a frame, because macOS applies a window mode/size change
## asynchronously: a single pin before the draw can be undone between the pin and the capture, and
## the frame silently lands at monitor size (one state rendered at 3840x1050 among four at 1600x900,
## which is only obvious if you happen to compare them).
func _settle() -> void:
	_pin_window()
	await get_tree().process_frame
	_pin_window()
	RenderingServer.force_draw()
	await get_tree().process_frame


## How many times a capture is re-taken when the window has escaped its pin. The WM's own resize
## lands once and is undone once, so one retry is the expected cost; the rest is slack.
const CAPTURE_RETRIES := 4

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("workbench_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	# **THE GEOMETRY GUARD, AND IT RE-CAPTURES RATHER THAN JUST COMPLAINING.** macOS applies (and
	# re-applies) a window mode/size change asynchronously, so a pin can be undone between
	# `_settle` and the capture: measured, ONE of four frames came back at the monitor's 3840x1050
	# while its siblings were 1600x900 — a frame judged at a width the surface never ships at, and
	# nothing says so unless you compare the files. `blend_probe`/`map_preview` carry the same guard.
	var attempts := 0
	while image.get_size() != PREVIEW_SIZE and attempts < CAPTURE_RETRIES:
		attempts += 1
		await _settle()
		image = get_viewport().get_texture().get_image()
		if image == null:
			return
	if image.get_size() != PREVIEW_SIZE:
		_fail("%s captured at %s after %d retries, not %s — the frame is not comparable with the others"
			% [name, image.get_size(), CAPTURE_RETRIES, PREVIEW_SIZE])
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		_fail("failed to save %s (err %d)" % [name, err])
	else:
		print("workbench_preview: saved ", name, ".png")
