extends Control

## Boot main-scene: a full-screen dark ground with the shared MenuShell in `landing` mode.
## "New Game" stashes the chosen world parameters in the GameLaunch autoload and swaps to
## Main.tscn (which consumes them into its `new_game` command); "Exit" quits.
##
## **IT ALSO HOLDS A COMMAND CLIENT, for the two questions this screen can ask with no world
## running.** `list_saves` is answered from disk before the server's `world_active` gate precisely so
## the load menu works here (`.claude/rules/core_sim/save-game.md`), and `faction_capacity` is
## answered idle for the same reason: the New Game pane has to know how many rivals a grid can seat
## BEFORE the command that would generate it. So this screen owns a `CommandClient`, a `SaveSlots`
## seam and a `FactionCapacity` seam, injects both into the shell, and pumps the replies once a frame
## — the same coordinator-mediation arrangement `Main` has for `ForecastQuery`. Loading is still a handoff: the
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
var _faction_capacity: FactionCapacity = null


func _ready() -> void:
	_ground.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell.new_game_requested.connect(_on_new_game_requested)
	_shell.exit_requested.connect(_on_exit_requested)
	_shell.apply_theme_requested.connect(_on_apply_theme_requested)
	_shell.load_requested.connect(_on_load_requested)
	_setup_query_seams()


## Stand the query seams up. **A failure here is not fatal and must not be**: the landing screen is
## reachable with no server running at all, and the shell renders that state as a line the player can
## read — a "Try again" in the saves panes, and in the New Game pane a caption saying the world will
## be built with the server's own rival count. The seams are injected either way, so nothing about
## either pane depends on whether the connect succeeded.
func _setup_query_seams() -> void:
	_command_client = CommandClient.new()
	_command_client.set_proto_port(CommandClient.resolve_proto_port())
	var err: Error = _command_client.connect_to_host(
		CommandClient.resolve_host(), CommandClient.resolve_port())
	if err != OK:
		push_warning("LandingScreen: no command bridge (error %d); the saves list will report it." % err)
	_save_slots = SaveSlots.new()
	_save_slots.set_sender(_send_query)
	_shell.set_save_slots(_save_slots)
	_faction_capacity = FactionCapacity.new()
	_faction_capacity.set_sender(_send_query)
	# Injection is what puts the first ask in flight — the setup pane is already up by now, and its
	# rival control has nothing to offer until this answers.
	_shell.set_faction_capacity(_faction_capacity)


## The one hop from the native query worker onto the main thread — the same once-a-frame drain
## `Main._pump_forecast_queries` performs. A query triggers no snapshot, so this is the ONLY path an
## answer takes.
## **DRAINED ONCE, DELIVERED TO BOTH SEAMS.** `poll_query_replies` empties the native queue, so two
## drains would race and each swallow the other's answers. The seams tell their own replies apart by
## `request_id` and their id blocks are disjoint by construction (`QueryRequestIds`), so handing each
## the whole batch is correct — the same arrangement `Main._pump_forecast_queries` uses.
func _process(_delta: float) -> void:
	if _command_client == null:
		return
	var replies: Array = _command_client.poll_query_replies()
	if _save_slots != null:
		_save_slots.deliver(replies)
	if _faction_capacity != null:
		_faction_capacity.deliver(replies)


func _send_query(request_id: int, ask: Dictionary) -> bool:
	if _command_client == null:
		return false
	return _command_client.send_query(request_id, ask)


func _on_new_game_requested(preset_id: String, width: int, height: int, seed: int, profile_id: String, ai_faction_count: int) -> void:
	GameLaunch.pending_new_game = {
		"preset_id": preset_id,
		"width": width,
		"height": height,
		"seed": seed,
		"profile_id": profile_id,
		# `FactionCapacity.NO_COUNT` here means the shell never got an answer to offer a choice from;
		# `Main` omits the argument for it, which is not the same request as an explicit 0.
		"ai_faction_count": ai_faction_count,
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
