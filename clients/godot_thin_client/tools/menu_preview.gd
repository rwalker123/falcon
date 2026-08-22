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

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — a harness that printed an error and still exited 0 was
## indistinguishable from a green one to anything but a human reading stdout.
const EXIT_OK := 0
const EXIT_FAILED := 1

var _root: Control
var _bg: ColorRect
var _shell: MenuShell
var _failures := 0


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
	await _save("menu_options")

	# …AND THE THEME DROPDOWN OPEN. Its own frame because a dropdown is TWO surfaces: the popup is a
	# `PopupMenu` on a separate embedded `Window`, which nothing set on the face reaches, so an
	# unstyled one renders Godot's stock light-grey menu over the console and no closed-face frame can
	# show it. It lands in the capture the way `band_panel_preview`'s confirm dialogs do.
	_shell._theme_picker.show_popup()
	await _settle()
	await _save("menu_options_theme_popup")
	_shell._theme_picker.get_popup().hide()

	_finish()


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
