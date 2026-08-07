extends Control

## Boot main-scene: a full-screen dark ground with the shared MenuShell in `landing` mode.
## "New Game" stashes the chosen world parameters in the GameLaunch autoload and swaps to
## Main.tscn (which consumes them into its `new_game` command); "Exit" quits.

@onready var _shell: MenuShell = $MenuShell


func _ready() -> void:
	_shell.mode = MenuShell.LANDING
	_shell.new_game_requested.connect(_on_new_game_requested)
	_shell.exit_requested.connect(_on_exit_requested)


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
