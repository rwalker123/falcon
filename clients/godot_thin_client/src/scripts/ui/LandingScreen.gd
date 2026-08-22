extends Control

## Boot main-scene: a full-screen dark ground with the shared MenuShell in `landing` mode.
## "New Game" stashes the chosen world parameters in the GameLaunch autoload and swaps to
## Main.tscn (which consumes them into its `new_game` command); "Exit" quits.

@onready var _shell: MenuShell = $MenuShell
## The full-bleed backdrop. Its colour is set HERE, not in `LandingScreen.tscn`: a scene file's
## `color = Color(...)` is a baked literal that no theme can reach, and this one was console `GROUND`
## — so the landing screen stayed slate-blue under every palette while the shell on top of it turned
## warm. A scene may hold the NODE; the palette holds its colour.
@onready var _ground: ColorRect = $Ground


func _ready() -> void:
	_ground.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell.new_game_requested.connect(_on_new_game_requested)
	_shell.exit_requested.connect(_on_exit_requested)
	_shell.restart_requested.connect(_on_restart_requested)


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


## The Options pane's "Restart now" — relaunch the client so a theme pick takes effect. The quit is
## conditional ON THE SPAWN: if no new process started, closing this one would leave the player with
## nothing, so the menu stays open and the row says what went wrong.
func _on_restart_requested() -> void:
	if GameLaunch.restart_client():
		get_tree().quit()
	else:
		_shell.show_restart_failed()
