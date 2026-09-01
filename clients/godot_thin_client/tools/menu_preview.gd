extends Node

## Dev-only preview harness for the shared MenuShell (landing + pause). Instances the real
## MenuShell scene, renders it in each mode, and dumps a PNG to `ui_preview_out/`. No server,
## no network — the actual render code against the real HudStyle. Run from the repo root:
##
##   godot --headless --path clients/godot_thin_client --import     # if scenes/scripts changed
##   scripts/preview.sh res://tools/menu_preview.tscn                       # NOT --headless
##
## then read ui_preview_out/menu_landing.png and menu_pause.png.

const MENU_SHELL := preload("res://src/ui/MenuShell.tscn")
const OUT_DIR := "res://ui_preview_out"

# Window the shell renders into.
const PREVIEW_SIZE := Vector2i(1500, 900)
# Ground behind the landing shell: `HudStyle.GROUND` itself, READ at its use site. It was a
# hand-copied literal of the console palette's value, so these frames kept rendering the retired
# palette's backdrop under every theme — the harness telling the same lie the shipped code was fixed
# for. A mid terrain tone stands in behind the pause scrim so the scrim + card chrome read against
# something non-black; that one is a stand-in for the WORLD, not a palette entry.
const MAP_TONE := Color(0.10, 0.15, 0.16)
# Nav id of the client-settings pane in MenuShell.ITEMS.
const OPTIONS_PANE_ID := "options"
# Nav ids of the two saves panes, driven through the same `_activate_item` the nav rail calls.
const LOAD_PANE_ID := "load"
const SAVE_PANE_ID := "save"

# ---- save-channel fixtures ------------------------------------------------------------------------
# The `SaveSlots` seam is fed through its REAL `deliver` path with dicts shaped exactly as
# `bridge/query.rs` composes them, so these frames exercise the actual decode/route/format code with
# no server. `modified_unix_seconds` is expressed as an AGE for the three relative buckets — a fixed
# stamp there would drift into another bucket as the branch aged — and as a FIXED stamp for the
# fourth, which is the one that renders the absolute-date branch.
const AGE_RECENT_SECONDS := 12 * 60
const AGE_HOURS_SECONDS := 5 * 3600
const AGE_DAYS_SECONDS := 3 * 86400
const FIXED_STAMP_UNIX := 1768726920  # 2026-01-18 09:02 UTC, rendered in the machine's local zone
const FIXTURE_SIZE_AUTOSAVE := 1257874   # the measured 160x104 blob
const FIXTURE_SIZE_MIDWINTER := 1198336
const FIXTURE_SIZE_FIRST := 902144
const FIXTURE_SIZE_TINY := 41984         # small enough to render in KB, the other size branch
const FIXTURE_TITLE := "Trail Sovereigns"
const FIXTURE_SLOT_MIDWINTER := "midwinter camp"
const FIXTURE_SLOT_FIRST := "first winter"
const FIXTURE_SLOT_SCRATCH := "scratch_2"
# The name typed into the Save pane's field for the "new slot" frame, and the one that is already on
# disk for the OVERWRITE frame.
const TYPED_NEW_NAME := "before the thaw"
# What a player might reasonably type that the whitelist refuses — the reserved slot, which is the
# one refusal the pane exists to make unreachable rather than merely reported.
const TYPED_RESERVED_NAME := "autosave"

# The drift rows, one per (saved -> live) pair the notice words differently, so one frame covers the
# whole vocabulary.
const DRIFT_FIXTURE := [
	{"file_name": "fauna_config.json", "saved": "file", "live": "file"},
	{"file_name": "simulation_config.json", "saved": "builtin", "live": "file"},
	{"file_name": "recipes.json", "saved": "file", "live": "builtin"},
]

# A roster theme that is NOT the one this harness pins as applied, so the Theme row renders its
# CHANGED state — caption in WARN, "Apply now" button present. Any id but `HudPalette.DEFAULT_THEME`
# would do; the frames are named for the state, not for this palette.
const PENDING_THEME := "kiln"

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — a harness that printed an error and still exited 0 was
## indistinguishable from a green one to anything but a human reading stdout.
const EXIT_OK := 0
const EXIT_FAILED := 1

var _root: Control
var _bg: ColorRect
var _shell: MenuShell
var _failures := 0
## The save channel, driven with no server: the harness IS the transport. `_last_request_id` is what
## the fake sender captured, and it is what the canned replies correlate against — so the seam's real
## in-flight bookkeeping is exercised rather than bypassed.
var _save_seam: SaveSlots
var _last_request_id := 0
var _drift_notice: ConfigDriftNotice


func _ready() -> void:
	get_window().size = PREVIEW_SIZE
	# PIN THE INTERFACE SCALE, the same determinism source `ui_preview` / `map_preview` /
	# `band_panel_preview` pin: `ClientSettings` is an autoload that has already read the developer's
	# real `user://client_settings.cfg` and `UiScaler` has already pushed it onto the window's
	# `content_scale_factor`. This harness sizes `_root` to a fixed PREVIEW_SIZE, so a moved slider
	# would leave that root larger than the logical viewport and push the Options pane out of frame —
	# in the ONE PNG that exists to show the Options pane. Reading the real config for the row VALUES
	# is deliberate here (see the docstring); rendering at the real config's SCALE is not.
	# Assign the MEMBER, never `set_ui_scale` (the setter `_save`s over that file), then re-emit
	# `changed` so `UiScaler` applies the pin through its own real path.
	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()
	# PIN THE PALETTE, the theme half of the same contamination. `ClientSettings` read the developer's
	# real `user://client_settings.cfg` at boot and `HudPalette.apply()` has ALREADY installed whatever
	# theme it found, so a developer running Kiln would re-tint every frame in this set. Re-applying the
	# default here is safe at any point before UI is built: `HudStyle`/`MapView` and the vocabulary
	# modules are all re-derived by `apply`, and nothing on screen has read a colour yet.
	HudPalette.apply(HudPalette.DEFAULT_THEME)
	# …AND THE SAVED PICK, which is a SECOND setting from the same contaminated file. The Theme row
	# compares the saved pick against the applied palette, so pinning only the palette left the row
	# rendering whatever the developer last chose: on a machine saved to any non-default theme every
	# Options frame came out in the row's PENDING state — not-applied caption, Apply button — and the
	# settled state had no frame at all. Same MEMBER assignment, for the same reason: `set_theme`
	# would write the developer's config.
	ClientSettings.theme = HudPalette.DEFAULT_THEME
	DirAccess.make_dir_absolute(OUT_DIR)

	_root = Control.new()
	_root.position = Vector2.ZERO
	_root.size = Vector2(PREVIEW_SIZE)
	add_child(_root)

	_bg = ColorRect.new()
	_bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_bg.color = HudStyle.GROUND
	_root.add_child(_bg)

	_shell = MENU_SHELL.instantiate()
	_root.add_child(_shell)
	await get_tree().process_frame

	# Landing: full-bleed over the dark ground.
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	await _settle()
	await _save("menu_landing")

	# Pause: centered card over the scrim, mid-tone "map" behind so the scrim reads.
	_bg.color = MAP_TONE
	_shell.mode = MenuShell.PAUSE
	await _settle()
	await _save("menu_pause")

	# Options pane — the client-settings rows (Fog of war toggle + the two speed sliders). Driven
	# through the same `_activate_item` the nav rail calls, so the pane is built exactly as a click
	# builds it. Rendered from the PAUSE mode; the shared ITEMS registry gives the landing menu the
	# identical pane, so one frame covers both.
	_shell._activate_item(OPTIONS_PANE_ID)
	await _settle()
	# The saved pick and the applied palette agree here, so there is nothing to apply and the Theme row
	# must offer nothing to press. This is the settled half of the pair asserted below.
	if _shell._theme_apply != null and _shell._theme_apply.visible:
		_fail("theme row: the Apply now button is showing with no pending pick")
	await _save("menu_options")

	# …AND THE THEME DROPDOWN OPEN. Its own frame because a dropdown is TWO surfaces: the popup is a
	# `PopupMenu` on a separate embedded `Window`, which nothing set on the face reaches, so an
	# unstyled one renders Godot's stock light-grey menu over the console and no closed-face frame can
	# show it. It lands in the capture the way `band_panel_preview`'s confirm dialogs do.
	_shell._theme_picker.show_popup()
	await _settle()
	await _save("menu_options_theme_popup")
	_shell._theme_picker.get_popup().hide()

	# …AND THE ROW IN ITS CHANGED STATE — the only state in which the "Apply now" button exists, so
	# it is the only frame that can show it. The pick is made by assigning the MEMBER
	# `ClientSettings.theme` and rebuilding the pane, never by driving `_on_theme_selected`: that path
	# calls `set_theme`, which SAVES over the developer's real `user://client_settings.cfg` — the same
	# contamination the interface-scale pin above exists to avoid, and this one would overwrite the
	# developer's own saved theme. Rendered from PAUSE, where the button is `armed` and says the
	# run will be lost. NOTHING PRESSES IT: applying reloads the current scene, which would tear down
	# the tree this harness is capturing.
	ClientSettings.theme = PENDING_THEME
	_shell._activate_item(OPTIONS_PANE_ID)
	await _settle()
	_assert_apply_visible("pause")
	await _save("menu_options_theme_pending_pause")

	# The same row in LANDING mode, where no run exists: shorter label, `primary` variant, and the
	# caption without the run-loss clause. `set_mode` rebuilds the active pane, so the row re-derives
	# its wording for the new mode rather than keeping the pause one.
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	await _settle()
	_assert_apply_visible("landing")
	await _save("menu_options_theme_pending_landing")

	await _run_saves_states()

	_finish()


## **THE LOAD / SAVE PANES, over a fake transport.** The seam is real, its decode and routing are
## real, and every frame below is the shipped builder rendering the seam's actual state — the only
## thing standing in for a server is `_send`, which records the request id and answers nothing until
## this harness says so. That is what lets the failure states (no server, no saves, a refused name)
## be rendered at all: none of them is reachable from a healthy stack.
func _run_saves_states() -> void:
	_save_seam = SaveSlots.new()
	_save_seam.set_sender(_send)
	_shell.set_save_slots(_save_seam)

	# --- LOAD, on the landing screen: the list, from a real `list_saves` answer -------------------
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list(_slot_fixtures())
	await _settle()
	if _save_seam.list_state != SaveSlots.LIST_READY:
		_fail("load list: a delivered answer left the seam in %s" % _save_seam.list_state)
	await _save("menu_load_list")

	# --- …with a row selected, which is what arms the buttons ------------------------------------
	_select(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_load_selected")

	# --- …and the DELETE two-step armed. The confirm button carries what it will destroy, which is
	#     this shell's whole confirmation pattern — there is no modal.
	_shell._on_delete_pressed()
	await _settle()
	_assert_confirm_names_the_slot()
	await _save("menu_load_delete_confirm")
	_shell._on_delete_cancelled()

	# --- LOAD from inside a run: the same pane, the destructive wording ---------------------------
	_bg.color = MAP_TONE
	_shell.mode = MenuShell.PAUSE
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list(_slot_fixtures())
	_select(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_load_in_run")

	# --- SAVE: a NEW slot name typed into the field ----------------------------------------------
	_shell._activate_item(SAVE_PANE_ID)
	_answer_list(_slot_fixtures())
	_type(TYPED_NEW_NAME)
	await _settle()
	await _save("menu_save")

	# --- …the same field naming a slot that EXISTS, which is a different act and says so ----------
	_type(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_save_overwrite")

	# --- …and the reserved name, refused under the field rather than by a round trip --------------
	_type(TYPED_RESERVED_NAME)
	await _settle()
	if SaveSlots.slot_name_error(TYPED_RESERVED_NAME) == "":
		_fail("slot names: the reserved autosave slot was accepted by the whitelist")
	await _save("menu_save_reserved_name")
	_type("")

	# --- NOTHING SAVED YET. `LIST_READY` with no rows, which is an invitation and not a failure ---
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list([])
	await _settle()
	await _save("menu_load_empty")

	# --- NO SERVER. The landing screen is reachable with none, so this is a first-run state, not an
	#     edge case: it must name the problem and offer the ask again.
	_shell._activate_item(LOAD_PANE_ID)
	_fail_list(SaveSlots.ERROR_TRANSPORT)
	await _settle()
	if _save_seam.list_state != SaveSlots.LIST_FAILED:
		_fail("no-server list: a refusal left the seam in %s" % _save_seam.list_state)
	await _save("menu_load_no_server")

	# --- THE CONFIG-DRIFT NOTICE. Not part of the shell — `Main` raises it over the loaded world —
	#     but it is the other half of this feature's UI and this is the harness that can see it.
	_drift_notice = ConfigDriftNotice.new()
	_root.add_child(_drift_notice)
	_drift_notice.show_drift(DRIFT_FIXTURE)
	await _settle()
	if not _drift_notice.visible:
		_fail("config drift: a non-empty drift list rendered nothing")
	await _save("config_drift")


## The canned slot list: the reserved autosave row, two named saves, and one small enough to render
## in the OTHER size unit. Newest first, the order the server answers in.
func _slot_fixtures() -> Array:
	var now := int(Time.get_unix_time_from_system())
	return [
		_slot_row(SaveSlots.AUTOSAVE_SLOT, 47, "earthlike", 80, 52,
			FIXTURE_SIZE_AUTOSAVE, now - AGE_RECENT_SECONDS),
		_slot_row(FIXTURE_SLOT_MIDWINTER, 31, "earthlike", 80, 52,
			FIXTURE_SIZE_MIDWINTER, now - AGE_HOURS_SECONDS),
		_slot_row(FIXTURE_SLOT_FIRST, 12, "polar_contrast", 64, 40,
			FIXTURE_SIZE_FIRST, now - AGE_DAYS_SECONDS),
		_slot_row(FIXTURE_SLOT_SCRATCH, 3, "earthlike", 48, 32,
			FIXTURE_SIZE_TINY, FIXED_STAMP_UNIX),
	]


func _slot_row(slot: String, turn: int, preset: String, width: int, height: int,
		size_bytes: int, modified: int) -> Dictionary:
	return {
		"slot": slot,
		"turn": turn,
		"campaign_title": FIXTURE_TITLE,
		"map_preset_id": preset,
		"width": width,
		"height": height,
		"world_seed": 0,
		"start_profile_id": "late_forager_tribe",
		"size_bytes": size_bytes,
		"modified_unix_seconds": modified,
	}


## The fake transport. Records the id so a reply can correlate, and reports the ask as sent.
func _send(request_id: int, _ask: Dictionary) -> bool:
	_last_request_id = request_id
	return true


## Answer the list query that is in flight, through the seam's real `deliver`.
func _answer_list(rows: Array) -> void:
	_save_seam.deliver([{
		"request_id": _last_request_id,
		"ok": true,
		"kind": SaveSlots.KIND_LIST,
		"slots": rows,
	}])


## …and refuse it, with a token from the server's own vocabulary.
func _fail_list(token: String) -> void:
	_save_seam.deliver([{
		"request_id": _last_request_id,
		"ok": false,
		"error": token,
	}])


## Click a row, through the same handler the row's `gui_input` reaches.
func _select(slot: String) -> void:
	var click := InputEventMouseButton.new()
	click.button_index = MOUSE_BUTTON_LEFT
	click.pressed = true
	_shell._on_slot_row_input(click, slot, _shell._active_pane == SAVE_PANE_ID)


## Type into the Save pane's name field, through the field's own `text_changed` handler.
func _type(text: String) -> void:
	_shell._on_save_name_changed(text)


## The armed delete button must NAME the slot it will destroy — that label IS the confirmation, so a
## generic "Confirm" would quietly remove the only thing standing between a click and a lost save.
func _assert_confirm_names_the_slot() -> void:
	if not _find_button_containing(_shell, FIXTURE_SLOT_MIDWINTER):
		_fail("delete confirm: no button names the slot it would delete")


func _find_button_containing(node: Node, needle: String) -> bool:
	if node is Button and (node as Button).text.contains(needle):
		return true
	for child in node.get_children():
		if _find_button_containing(child, needle):
			return true
	return false


## The button is BUILT hidden and shown only while the pick differs from what is on screen, so a
## frame that silently lost it would look like a deliberate layout and pass review. Checked, not eyeballed.
func _assert_apply_visible(mode_name: String) -> void:
	if _shell._theme_apply == null or not _shell._theme_apply.visible:
		_fail("theme row (%s): a pending pick did not surface the Apply now button" % mode_name)


## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("menu_preview: FAIL — %s" % message)


## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place.
func _finish() -> void:
	if _failures > 0:
		print("menu_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("menu_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)


func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame


func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("menu_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		_fail("failed to save %s (err %d)" % [name, err])
	else:
		print("menu_preview: saved ", name, ".png")
