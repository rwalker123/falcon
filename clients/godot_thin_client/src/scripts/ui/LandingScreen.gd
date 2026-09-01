extends Control

## Boot main-scene: a full-screen dark ground with the shared MenuShell in `landing` mode.
## "New Game" stashes the chosen world parameters in the GameLaunch autoload and swaps to
## Main.tscn (which consumes them into its `new_game` command); "Exit" quits.
##
## **IT ALSO HOLDS A COMMAND CLIENT, and only for the save channel.** `list_saves` is answered from
## disk before the server's `world_active` gate precisely so the load menu works here, with no world
## running (`.claude/rules/core_sim/save-game.md`). So this screen owns a `CommandClient` and a
## `SaveSlots` seam, injects the seam into the shell, and pumps the replies once a frame — the same
## coordinator-mediation arrangement `Main` has for `ForecastQuery`. Loading is still a handoff: the
## slot goes into `GameLaunch` and `Main` sends `load_game` itself, so the reveal gate governs it
## exactly as it governs `new_game` (`.claude/rules/core_sim/world-handoff.md`).

@onready var _shell: MenuShell = $MenuShell
## The full-bleed backdrop. Its colour is set HERE, not in `LandingScreen.tscn`: a scene file's
## `color = Color(...)` is a baked literal that no theme can reach, and this one was console `GROUND`
## — so the landing screen stayed slate-blue under every palette while the shell on top of it turned
## warm. A scene may hold the NODE; the palette holds its colour.
@onready var _ground: ColorRect = $Ground

var _command_client: CommandClient = null
var _save_slots: SaveSlots = null


func _ready() -> void:
	_ground.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell.new_game_requested.connect(_on_new_game_requested)
	_shell.exit_requested.connect(_on_exit_requested)
	_shell.apply_theme_requested.connect(_on_apply_theme_requested)
	_shell.load_requested.connect(_on_load_requested)
	_setup_save_channel()


## Stand the save channel up. **A failure here is not fatal and must not be**: the landing screen is
## reachable with no server running at all, and the shell renders that state as a line the player can
## read plus a "Try again" — the seam is injected either way, so nothing about the pane depends on
## whether the connect succeeded.
func _setup_save_channel() -> void:
	_command_client = CommandClient.new()
	_command_client.set_proto_port(CommandClient.resolve_proto_port())
	var err: Error = _command_client.connect_to_host(
		CommandClient.resolve_host(), CommandClient.resolve_port())
	if err != OK:
		push_warning("LandingScreen: no command bridge (error %d); the saves list will report it." % err)
	_save_slots = SaveSlots.new()
	_save_slots.set_sender(_send_save_query)
	_shell.set_save_slots(_save_slots)


## The one hop from the native query worker onto the main thread — the same once-a-frame drain
## `Main._pump_forecast_queries` performs. A query triggers no snapshot, so this is the ONLY path an
## answer takes.
func _process(_delta: float) -> void:
	if _save_slots != null and _command_client != null:
		_save_slots.deliver(_command_client.poll_query_replies())


func _send_save_query(request_id: int, ask: Dictionary) -> bool:
	if _command_client == null:
		return false
	return _command_client.send_query(request_id, ask)


func _on_new_game_requested(preset_id: String, width: int, height: int, seed: int, profile_id: String) -> void:
	GameLaunch.pending_new_game = {
		"preset_id": preset_id,
		"width": width,
		"height": height,
		"seed": seed,
		"profile_id": profile_id,
	}
	GameLaunch.pending_load_slot = ""
	get_tree().change_scene_to_file("res://src/Main.tscn")


## **THE LOAD IS A HANDOFF, NOT A SEND.** Arming the slot and swapping scenes puts the load through
## the identical path `new_game` takes — `Main` sends it, retries it until a world reveals, and holds
## the loading overlay until the epoch gate says the frame belongs to the loaded world. Sending
## `load_game` from here would leave this screen holding a reply about a world it is not going to show.
func _on_load_requested(slot: String) -> void:
	GameLaunch.pending_load_slot = slot
	GameLaunch.pending_new_game = null
	get_tree().change_scene_to_file("res://src/Main.tscn")


func _on_exit_requested() -> void:
	get_tree().quit()


## The Options pane's "Apply now" — install the picked theme and rebuild this scene so it shows.
## Nothing quits and nothing is spawned: the landing screen simply comes back in the new palette.
func _on_apply_theme_requested() -> void:
	GameLaunch.apply_theme_now()
