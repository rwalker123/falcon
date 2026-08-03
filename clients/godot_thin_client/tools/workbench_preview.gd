extends Node

## Dev-only preview harness for the **Workbench** surface (rail + content host + its pages), the
## menu/HUD harnesses' third sibling. It builds a real `WorkbenchShell` in code — the shell has no
## `.tscn`, being assembled by `_build_chrome` — over a mid terrain tone standing in for the map, so
## the reserved-edge composition (surface on the left, map to the right of it) is visible in every
## frame. No server, no network: the tuning page reads the same `tuning_manifest.json` the client
## ships. Run from the repo root:
##
##   godot --headless --path clients/godot_thin_client --import        # if scenes/scripts changed
##   godot --path clients/godot_thin_client res://tools/workbench_preview.tscn   # NOT --headless
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
const PLACEHOLDER_PAGE := &"logs"

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

	# Back to the tuning page for the last assertion: `_tuning_page` finds the page in the TREE, and
	# the shell detaches a page it is not showing.
	_shell.show_page(TUNING_PAGE)
	await _settle()
	_assert_staged_survives_un_edit()
	get_tree().quit()


# ---- state driving ---------------------------------------------------------

## The page instance the shell built, found by TYPE rather than by reaching into the shell's private
## page table — the shell hands pages out to nobody, and a harness is not a reason to change that.
func _tuning_page() -> ConfigTuningPage:
	for node in _shell.find_children("*", "", true, false):
		if node is ConfigTuningPage:
			return node
	push_error("workbench_preview: config tuning page not found")
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
		push_error("workbench_preview: moved %d of %d dirty edits — labels drifted from the manifest"
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
		push_error("workbench_preview: expected 1 overrides_requested, got %d" % emitted.size())
		return
	var patches: Dictionary = emitted[0]
	if patches != EXPECTED_PATCHES:
		push_error("workbench_preview: patch is not the expected sparse shape:\n  got      %s\n  expected %s"
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
		push_error("workbench_preview: no '%s' row — the manifest moved" % STAGED_PARAM)
		return
	var failed := 0

	spin.value = STAGED_PARAM_EDITED
	page._on_apply_pressed()
	if sent.size() != 1 or not sent[0].begins_with(WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE):
		failed += 1
		push_error("workbench_preview: Apply sent %s, expected one %s"
			% [sent, WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE])
	if not page._apply_button.disabled:
		failed += 1
		push_error("workbench_preview: Apply is still live with nothing unsent")

	# The un-edit: back to the shipped default, which used to read as "nothing to do".
	spin.value = STAGED_PARAM_DEFAULT
	if page._revert_button.disabled:
		failed += 1
		push_error("workbench_preview: 'Revert all' is DEAD while the server still holds an override — it is the only control that can clear it")
	if page._status.text == WorkbenchVocab.TUNING_CLEAN_STATUS:
		failed += 1
		push_error("workbench_preview: the status line claims '%s' while the server holds a staged override"
			% WorkbenchVocab.TUNING_CLEAN_STATUS)
	var patch: Dictionary = page.build_patches()
	if not patch.get(STAGED_PARAM_KIND, {}).has(STAGED_PARAM_POINTER):
		failed += 1
		push_error("workbench_preview: the returned-to-default row is OMITTED from the patch (%s) — the server would keep the staged value" % patch)

	# SECOND LEG: apply that un-edit. Now every row matches both its default AND what the server was
	# told, so the only fact left is that the server is holding a file — which nothing about the rows
	# can express. `Revert all` must survive on THAT alone.
	page._on_apply_pressed()
	if not page._apply_button.disabled:
		failed += 1
		push_error("workbench_preview: Apply is live with every row applied and at its default")
	if page._revert_button.disabled:
		failed += 1
		push_error("workbench_preview: 'Revert all' is DEAD with all rows clean but the server still holding a staged override file — clearing it is unreachable")
	if page._status.text != WorkbenchVocab.TUNING_STAGED_CLEARED_STATUS:
		failed += 1
		push_error("workbench_preview: status reads '%s', expected the staged-but-defaulted line '%s'"
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
		push_error("workbench_preview: no content scroll to measure rows against")
		return

	var failed := 0
	# What the geometry says the content column is: the surface, less the rail and the padding either
	# side of it. Anything wider means a row has pushed the surface out of shape.
	var nominal_width := WorkbenchVocab.SURFACE_WIDTH - WorkbenchVocab.RAIL_WIDTH \
		- 2.0 * WorkbenchVocab.CONTENT_PADDING
	if scroll.size.x > nominal_width + FIT_TOLERANCE:
		failed += 1
		push_error("workbench_preview: content column is %.1fpx wider than the surface allows (%.1f > %.1f) — a row does not fit"
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
			push_error("workbench_preview: row label wraps to %d lines: '%s'"
				% [label.get_line_count(), label.text])
		if label.size.x + FIT_TOLERANCE < label.get_minimum_size().x:
			failed += 1
			push_error("workbench_preview: row label is clipped (%.1f < %.1f): '%s'"
				% [label.size.x, label.get_minimum_size().x, label.text])
		if line.size.x > row_limit + FIT_TOLERANCE:
			failed += 1
			push_error("workbench_preview: row runs under the scrollbar (%.1f > %.1f): '%s'"
				% [line.size.x, row_limit, label.text])
	if checked == 0:
		push_error("workbench_preview: no rows measured — the row shape moved")
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


# ---- capture ---------------------------------------------------------------

## `project.godot` opens MAXIMIZED and macOS applies that asynchronously, so the window is re-pinned
## here and again from `_settle` — the treatment `blend_probe`/`map_preview` carry. Without it a
## frame silently renders at the monitor's size and the surface is judged at a width it never ships
## at.
func _pin_window() -> void:
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = PREVIEW_SIZE


## Pinned TWICE around a frame, because macOS applies `project.godot`'s MODE_MAXIMIZED
## asynchronously: a single pin before the draw can be undone between the pin and the capture, and
## the frame silently lands at monitor size (one state rendered at 3840x1050 among four at 1600x900,
## which is only obvious if you happen to compare them).
func _settle() -> void:
	_pin_window()
	await get_tree().process_frame
	_pin_window()
	RenderingServer.force_draw()
	await get_tree().process_frame


## How many times a capture is re-taken when the window has escaped its pin. The maximize lands once
## and is undone once, so one retry is the expected cost; the rest is slack.
const CAPTURE_RETRIES := 4

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("workbench_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	# **THE GEOMETRY GUARD, AND IT RE-CAPTURES RATHER THAN JUST COMPLAINING.** macOS applies (and
	# re-applies) `project.godot`'s MODE_MAXIMIZED asynchronously, so a pin can be undone between
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
		push_error("workbench_preview: %s captured at %s after %d retries, not %s — the frame is not comparable with the others"
			% [name, image.get_size(), CAPTURE_RETRIES, PREVIEW_SIZE])
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("workbench_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("workbench_preview: saved ", name, ".png")
