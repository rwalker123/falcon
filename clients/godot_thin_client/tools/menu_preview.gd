extends Node

## Dev-only preview harness for the shared MenuShell (landing + pause). Instances the real
## MenuShell scene, renders it in each mode, and dumps a PNG to `ui_preview_out/`. No server,
## no network — the actual render code against the real HudStyle. Run from the repo root:
##
##   godot --headless --path clients/godot_thin_client --import     # if scenes/scripts changed
##   godot --path clients/godot_thin_client res://tools/menu_preview.tscn   # NOT --headless
##
## then read ui_preview_out/menu_landing.png and menu_pause.png.

const MENU_SHELL := preload("res://src/ui/MenuShell.tscn")
const OUT_DIR := "res://ui_preview_out"

# Window the shell renders into.
const PREVIEW_SIZE := Vector2i(1500, 900)
# Ground behind the landing shell (mirrors HudStyle.GROUND). A mid terrain tone stands in behind
# the pause scrim so the scrim + card chrome read against something non-black.
const GROUND_TONE := Color(0.043, 0.067, 0.078)
const MAP_TONE := Color(0.10, 0.15, 0.16)

var _root: Control
var _bg: ColorRect
var _shell: MenuShell


func _ready() -> void:
	get_window().size = PREVIEW_SIZE
	DirAccess.make_dir_absolute(OUT_DIR)

	_root = Control.new()
	_root.position = Vector2.ZERO
	_root.size = Vector2(PREVIEW_SIZE)
	add_child(_root)

	_bg = ColorRect.new()
	_bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_bg.color = GROUND_TONE
	_root.add_child(_bg)

	_shell = MENU_SHELL.instantiate()
	_root.add_child(_shell)
	await get_tree().process_frame

	# Landing: full-bleed over the dark ground.
	_bg.color = GROUND_TONE
	_shell.mode = MenuShell.LANDING
	await _settle()
	await _save("menu_landing")

	# Pause: centered card over the scrim, mid-tone "map" behind so the scrim reads.
	_bg.color = MAP_TONE
	_shell.mode = MenuShell.PAUSE
	await _settle()
	await _save("menu_pause")

	get_tree().quit()


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
		push_error("menu_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("menu_preview: saved ", name, ".png")
