extends Control

## Boot main-scene: a full-screen dark ground with the shared MenuShell in `landing` mode.
## "New Game" stashes the chosen world parameters in the GameLaunch autoload and swaps to
## Main.tscn (which consumes them into its `new_game` command); "Exit" quits.

@onready var _shell: MenuShell = $MenuShell


func _ready() -> void:
	_claim_player_window()
	_shell.mode = MenuShell.LANDING
	_shell.new_game_requested.connect(_on_new_game_requested)
	_shell.exit_requested.connect(_on_exit_requested)


## Take the window a PLAYER wants — fullscreen, focused, in front. `project.godot` deliberately boots
## the opposite (windowed, `no_focus`), because that same window is the one every render harness in
## `tools/` inherits, and those harnesses cannot run `--headless`. A fullscreen focus-grabbing window
## opening and closing per verification run is what made parallel worktrees unworkable; the harnesses
## are the common case, so they get the default and the game asks. See
## `.claude/rules/client/test-harnesses.md` → "The tool window is quiet by DEFAULT".
##
## Order is load-bearing: a window still carrying `WINDOW_FLAG_NO_FOCUS` cannot be made key, so the
## flag comes off BEFORE anything asks for focus. Idempotent — `Main` returns here on abandon, and
## re-promoting an already-promoted window is a no-op.
func _claim_player_window() -> void:
	DisplayServer.window_set_flag(DisplayServer.WINDOW_FLAG_NO_FOCUS, false)
	DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_FULLSCREEN)
	DisplayServer.window_move_to_foreground()
	get_window().grab_focus()


func _on_new_game_requested(preset_id: String, width: int, height: int, seed: int, profile_id: String) -> void:
	GameLaunch.pending_new_game = {
		"preset_id": preset_id,
		"width": width,
		"height": height,
		"seed": seed,
		"profile_id": profile_id,
	}
	get_tree().change_scene_to_file("res://src/Main.tscn")


func _on_exit_requested() -> void:
	get_tree().quit()
