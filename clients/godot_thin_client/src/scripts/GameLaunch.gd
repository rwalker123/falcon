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

## The parameters the CURRENT run's world was actually built from — what `Main._build_world_request`
## resolved AFTER the dev-default fallback, so it is populated on every path into `Main.tscn`, handoff
## or not. `pending_new_game` cannot answer this: it is consumed and cleared on the first `_ready`, and
## a scene reload re-runs that `_ready` with an empty slot. `apply_theme_now` re-arms the pending slot
## from this so a mid-run apply rebuilds THE RUN'S world (its preset, size, seed and profile) instead
## of the dev default. Null before the first run, and cleared when a run is abandoned — the landing
## screen owns the parameters again from there.
##
## Same shape as `pending_new_game`. A run whose seed is 0 ("derive from the run clock") still lands on
## a different map, because 0 is what gets re-sent — the request is for a NEW world either way.
var active_new_game = null

## **THE LOAD HANDOFF — the same slot mechanism, for the other way a world arrives.** The landing
## screen (or `Main`'s pause menu, which reloads its own scene) writes the SLOT NAME here and
## `Main._ready` consumes it, sending `load_game` instead of `new_game`. A load rebuilds the world
## server-side and bumps the world epoch exactly as `new_game` does, so the reveal gate, the
## retry-until-answered and the per-world cache reset all apply unchanged
## (`.claude/rules/core_sim/world-handoff.md`) — which is the whole reason a load reuses this path
## rather than inventing one.
##
## `""` when no load is pending. It takes PRECEDENCE over `pending_new_game`: the two are never armed
## together, and a load is the more specific request.
var pending_load_slot: String = ""

## The slot the CURRENT run was loaded from, `""` for a generated world — the load's counterpart to
## `active_new_game`, and held for the same reason. `apply_theme_now` rebuilds the scene, which
## re-runs `Main._ready`; without this a theme applied after a load would find only `active_new_game`
## armed and silently GENERATE a fresh world in place of the save the player was playing.
var active_load_slot: String = ""


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
##
## **WHICH world it builds is re-armed here.** That second `_ready` finds `pending_new_game` already
## consumed by the first one, and would fall through to `Main.DEV_DEFAULT_NEW_GAME` — a theme apply
## would silently swap an 80x52 `earthlike` in for the preset, size, seed and profile the player chose.
## Re-stashing `active_new_game` makes the rebuilt world the run's own. Consume-and-clear stays intact
## for every other caller: nothing else writes the pending slot, and on the landing screen
## `active_new_game` is null (fresh boot) or cleared (a run was abandoned), so nothing is armed and a
## direct `Main.tscn` launch still gets the dev default.
func apply_theme_now() -> void:
	# The LOADED slot wins when there is one: the run in progress is that save, and re-arming the
	# generated-world parameters instead would swap a different world in under the player.
	if active_load_slot != "":
		pending_load_slot = active_load_slot
	elif active_new_game is Dictionary:
		pending_new_game = active_new_game
	HudPalette.apply(ClientSettings.theme)
	get_tree().reload_current_scene()
