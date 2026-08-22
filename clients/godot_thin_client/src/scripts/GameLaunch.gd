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
## Godot's own end-of-engine-arguments marker: everything after it is the GAME's, and comes back from
## `OS.get_cmdline_user_args()`.
const USER_ARGS_SEPARATOR := "--"
## Carries the window mode across a restart, so the new process comes up the way the old one was.
##
## **ARGV RATHER THAN A SETTINGS KEY, DELIBERATELY.** The render harnesses read the player's real
## `user://client_settings.cfg` (that is how a developer's saved theme once leaked into preview
## frames), so a key there could be consumed by a preview run instead of by the restart it was meant
## for — and applying a window mode inside a harness would fight the `override.cfg` that
## `scripts/preview.sh` uses to keep its window quiet. An argument reaches ONLY the process we
## spawned, so a harness is structurally immune rather than immune by remembering to opt out.
const WINDOW_MODE_FLAG := "--window-mode="
## How many times to ask for the carried mode before giving up. The ceiling is headroom: the worst
## case measured on macOS was four.
const WINDOW_MODE_ATTEMPTS := 8
## How long to let a transition land before reading the mode back. Below ~300ms the read catches
## macOS mid-animation and reports the mode the window is LEAVING.
const WINDOW_MODE_SETTLE_MSEC := 300.0
const MSEC_PER_SEC := 1000.0


func _ready() -> void:
	_restore_window_mode()


## Apply a window mode handed over by the process that spawned this one. Silent when the flag is
## absent, which is every launch except a restart.
##
## This runs from an autoload `_ready`, so the window already exists at `project.godot`'s configured
## mode and is being moved.
func _restore_window_mode() -> void:
	for arg in OS.get_cmdline_user_args():
		if not arg.begins_with(WINDOW_MODE_FLAG):
			continue
		var raw := arg.substr(WINDOW_MODE_FLAG.length())
		if not raw.is_valid_int():
			push_warning("GameLaunch: ignoring malformed window mode '%s'" % raw)
			return
		var mode := int(raw)
		if mode < DisplayServer.WINDOW_MODE_WINDOWED \
				or mode > DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN:
			push_warning("GameLaunch: ignoring out-of-range window mode %d" % mode)
			return
		await _apply_window_mode(mode)
		return


## ASK, WAIT, CHECK, ASK AGAIN — and a single `window_set_mode` is NOT enough, which is the whole
## reason this function exists rather than one line at the call site.
##
## `project.godot` boots the game FULLSCREEN, macOS animates every fullscreen transition, and a mode
## set while one is in flight is accepted and then silently discarded when the animation lands. The
## first cut of the carry-over did exactly that and produced the INVERSION it was written to fix —
## measured, from a fullscreen boot: asking for MAXIMIZED settled at WINDOWED, and asking for
## WINDOWED settled at FULLSCREEN. Waiting long enough made every transition correct on its own, so
## this is a race and not a wrong argument.
##
## Retrying rather than sleeping a fixed span keeps the window in the wrong mode for as short a time
## as macOS allows: measured over repeated runs, WINDOWED lands on the first attempt and MAXIMIZED
## takes three or four, so the ceiling is headroom rather than the expected cost.
func _apply_window_mode(mode: int) -> void:
	for attempt in WINDOW_MODE_ATTEMPTS:
		DisplayServer.window_set_mode(mode)
		# `ignore_time_scale`, because a harness that froze `Engine.time_scale` would otherwise hang
		# here. Harnesses never pass the flag that reaches this code, but a timer that can only work
		# under normal time is a trap for whoever adds one.
		await get_tree().create_timer(
			WINDOW_MODE_SETTLE_MSEC / MSEC_PER_SEC, true, false, true).timeout
		if DisplayServer.window_get_mode() == mode:
			return
	push_warning("GameLaunch: window mode %d did not take after %d attempts (now %d)"
		% [mode, WINDOW_MODE_ATTEMPTS, DisplayServer.window_get_mode()])


## The mode to hand the new process. MINIMIZED is deliberately NOT carried: a game that restarts
## into the dock with no window on screen is indistinguishable from one that failed to start, so it
## comes back windowed instead.
func _window_mode_to_carry() -> int:
	var mode := DisplayServer.window_get_mode()
	if mode == DisplayServer.WINDOW_MODE_MINIMIZED:
		return DisplayServer.WINDOW_MODE_WINDOWED
	return mode


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
	# WITHOUT THIS THE WINDOW MODE IS WHATEVER THE OS FELT LIKE. `project.godot` boots fullscreen, so
	# a restart from a maximized or windowed session came back in a different mode than it left —
	# nothing in the project inverts it, but nothing pins it either, and the window manager is free
	# to place the new process how it likes. Carrying the mode makes the restart continuous.
	args.append(USER_ARGS_SEPARATOR)
	args.append(WINDOW_MODE_FLAG + str(_window_mode_to_carry()))
	var pid := OS.create_process(exe, args)
	if pid < MIN_VALID_PID:
		push_warning("GameLaunch: could not restart — spawning '%s' failed (pid %d)" % [exe, pid])
		return false
	return true
