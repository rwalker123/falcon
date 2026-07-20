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
