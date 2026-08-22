extends Node

## Cross-scene handoff for a pending "New Game" request (registered as the `GameLaunch`
## autoload). The landing screen writes the chosen world parameters here and then swaps to
## `Main.tscn`; `Main._ready` consumes them to build its `new_game` command, then clears the
## slot. Null when no launch is pending (Main falls back to a dev-default world in that case,
## so launching `Main.tscn` directly still yields a playable map).
##
## Shape when set: {preset_id: String, width: int, height: int, seed: int, profile_id: String}.
var pending_new_game = null

## The world epoch (monotonic worldgen counter from the snapshot header) that `Main` last REVEALED.
## Persists across `Main.tscn` reloads so a restart's reveal gate can ignore the server's replayed
## pre-rebuild frame (epoch == this) and reveal only on the rebuild's higher epoch. Starts at 0
## (fresh launch, nothing revealed yet); `Main` writes the revealed epoch here on reveal.
var last_world_epoch: int = 0


## `OS.create_process` returns the child's pid, or -1 when the spawn failed. Anything below this is
## "no process was started", which is the one result that must never be followed by a quit.
const MIN_VALID_PID := 1
## Godot's "run the project at this path" CLI flag — what turns the editor binary into this game.
const PROJECT_PATH_FLAG := "--path"


## Relaunch the client process. Returns false if the new process could not be started, in which case
## the caller must NOT quit — quitting after a failed spawn closes the game with nothing to replace
## it. On success the CALLER quits, so the decision to close stays visible where the button is.
##
## This is how a theme pick reaches the screen at all: the palette is installed once at boot
## (`ClientSettings._ready` -> `HudPalette.apply`), so the setting is restart-to-apply.
func restart_client() -> bool:
	var exe := OS.get_executable_path()
	var args := PackedStringArray()
	# UNDER THE EDITOR THE EXECUTABLE IS THE GODOT BINARY, NOT THE GAME. Launched bare it opens the
	# Project Manager and the game never comes back, so the project path has to be handed to it. An
	# exported build IS the game and takes no argument.
	if OS.has_feature("editor"):
		args.append(PROJECT_PATH_FLAG)
		args.append(ProjectSettings.globalize_path("res://"))
	var pid := OS.create_process(exe, args)
	if pid < MIN_VALID_PID:
		push_warning("GameLaunch: could not restart — spawning '%s' failed (pid %d)" % [exe, pid])
		return false
	return true
