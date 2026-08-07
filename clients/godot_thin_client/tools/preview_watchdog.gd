extends Node

## **THE HANG GUARD FOR THE PNG PREVIEW HARNESSES — the one thing in them that cannot be aborted.**
##
## `ui_preview` / `band_panel_preview` render their whole walk from a single long `await`-ing
## `_ready()`, and GDScript's failure mode there is SILENCE: a runtime error aborts the function on
## the spot, so `get_tree().quit()` — the last line of that function — is never reached, and a
## coroutine awaiting the aborted one is never resumed either. The process then sits idle FOREVER,
## having stopped writing PNGs long before, so what is left on disk is a stale, plausible-looking
## partial frame set. One such process was found alive after 59 minutes. There is no FAIL token in
## that output and no exit status at all, which makes it invisible to every check we have — the worst
## possible shape for the only thing that verifies client work in this repo.
##
## **The guard therefore lives OUTSIDE the harness script, as a sibling node in the scene**, and that
## placement is load-bearing rather than tidy: the failure that prompted it was a chapter with a parse
## error, which — because the harness `preload`ed its chapters — made **`ui_preview.gd` itself fail to
## load**. The root node then came up with NO script at all, so nothing the harness could have done
## internally would have run. A sibling node's `_ready` still runs in that world. `ui_preview` now
## loads its chapters at runtime and validates them up front (so that specific defect fails loudly
## before a frame is written), but the general shape — a preload one level deeper, an aborted `await`,
## a WM that never honours the canvas — is what this node is for.
##
## **It PRELOADS NOTHING and depends on nothing**, deliberately: a guard that can fail to compile
## alongside the thing it guards is not a guard.
##
## It is a PROGRESS timer, not a deadline: the harness calls `note_progress()` as each chapter starts
## and each frame is saved, so the limit below is the longest gap between two signs of life, never a
## budget for the whole run (which grows every time a state is added, and would otherwise have to be
## re-tuned). A harness that never loaded reports no progress at all and is killed at the first limit.
##
## On firing it prints the harnesses' own `FAIL` token — so the existing output scanning catches it —
## and quits with a FAILING status.

## The longest gap between two signs of life before the run is declared hung. Deliberately generous
## by orders of magnitude against what a healthy run does: a full `ui_preview` walk is 270 frames in
## **13 seconds** of wall clock (measured), i.e. tens of milliseconds between progress calls, and the
## slowest single step in either harness — the prologue's canvas stabilization, bounded at 600 frames
## — is ~10 seconds at 60fps. The margin is for a loaded or headless-CI machine, where being slow
## must never be reported as being hung. The number this replaces is the one that matters: an
## unbounded wait, measured once at 59 minutes.
const PROGRESS_STALL_LIMIT_MSEC := 180_000

## Exit status when the watchdog fires. Non-zero so a wedged run is distinguishable from a clean one
## by status as well as by output — see `ui_preview.gd`'s `EXIT_FAILED`.
const EXIT_HUNG := 1

## Prefix on this node's own output, so a stall reads as the harness's failure rather than a stray
## engine message. Set by the harness scene's node name (`ui_preview` / `band_panel_preview`).
@export var harness_name: String = "preview"

var _last_progress_msec: int = 0
var _armed: bool = true


func _ready() -> void:
	_last_progress_msec = Time.get_ticks_msec()
	print("%s: watchdog armed (%d s without progress = hung)" % [
		harness_name, PROGRESS_STALL_LIMIT_MSEC / 1000])


## Called by the harness at every sign of life. Wall clock, NOT `delta`: these harnesses run with
## `Engine.time_scale = 0.0` (the determinism freeze), so every `delta` in the process is zero and a
## timer built on one would never expire.
func note_progress() -> void:
	_last_progress_msec = Time.get_ticks_msec()


## Called by the harness on its way out, so a slow shutdown cannot be reported as a stall.
func disarm() -> void:
	_armed = false


func _process(_delta: float) -> void:
	if not _armed:
		return
	var stalled_msec := Time.get_ticks_msec() - _last_progress_msec
	if stalled_msec < PROGRESS_STALL_LIMIT_MSEC:
		return
	_armed = false
	push_error(("%s: FAIL watchdog — no progress for %d s, so the run is hung and is being killed. "
		+ "The harness renders from one long `await`ing `_ready()`, and a runtime error there aborts "
		+ "it without ever reaching `get_tree().quit()`; the errors printed above this line are where "
		+ "it stopped. Whatever PNGs are on disk are a PARTIAL frame set from a failed run.")
		% [harness_name, stalled_msec / 1000])
	get_tree().quit(EXIT_HUNG)
