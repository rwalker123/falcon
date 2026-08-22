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


## Install the saved theme and rebuild the scene so it takes effect now.
##
## **REBUILT IN PROCESS RATHER THAN RELAUNCHED**, for two reasons. The first is the window: a process
## that replaces itself has to re-establish its own window state on the way up, and `project.godot`
## boots the game fullscreen while macOS animates fullscreen transitions, so a mode asked for during
## one is silently discarded and the new process comes up in the mode it was not asked for. A reload
## never touches the window, so that cannot arise. The second is reach — the client is not always the
## top of its own process tree: on `scripts/run_stack.sh`'s full-stack path it runs in the FOREGROUND
## and its exit triggers the script's cleanup, which stops the server. Rebuilding in place spawns no
## process and exits nothing, so it reaches neither.
##
## **ORDER IS THE CONTRACT.** The palette goes in BEFORE the reload, which is the same ordering
## `ClientSettings._ready` relies on at boot: nothing restyles a Control after the fact, so every
## Control must be built against the values already installed. `HudStyle.apply_palette` clears the
## generated icon rasters, so no cached art survives into the new tree either.
##
## A pause-mode apply ENDS THE RUN. Reloading `Main.tscn` re-runs `Main._ready`, which reconnects
## and sends `new_game` (`.claude/rules/core_sim/world-handoff.md`), so the server builds a fresh
## world rather than handing back the one in progress.
func apply_theme_now() -> void:
	HudPalette.apply(ClientSettings.theme)
	get_tree().reload_current_scene()
