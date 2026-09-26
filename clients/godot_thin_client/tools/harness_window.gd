class_name HarnessWindow
extends RefCounted

## ⛔ **A PIXEL HARNESS OPENS A REAL WINDOW, AND A REAL WINDOW RECEIVES THE HUMAN'S MOUSE.**
##
## Every harness that captures pixels must run WINDOWED (`--headless` selects the dummy rendering
## driver, which has no viewport texture to read back — `test-harnesses.md`). That window is an
## ordinary OS window: while it is the foreground window, macOS delivers the physical pointer's
## motion into its viewport as genuine `InputEventMouseMotion`, interleaved with whatever the
## harness is pushing through `Viewport.push_input`. **The harness does not own the pointer, and
## the run is not hermetic.**
##
## What that costs is not theoretical — it is the `band_panel_preview` flake this file was written
## for. A `BaseButton` recomputes `status.pressing_inside` from EVERY motion routed to it while a
## press is held, so one foreign motion landing between a simulated press and its release makes the
## release CANCEL the click instead of emitting `pressed`: a control that answers nothing, with no
## error and no warning. A live drag is hit the same way from two directions at once — Godot
## re-picks the drag-over control on motion, and localizes the drop from the REAL cursor.
##
## **The seal is an OS-LEVEL one, because the interference is OS-level.** `push_input` injects
## straight into the viewport and never goes near the window server, so a window that accepts no
## mouse events at all still runs every simulated gesture exactly as before — it simply stops
## hearing the hand resting on the desk.
##
## **MEASURED, on `band_panel_preview`** (`harness-band-panel.md` → "A SIMULATED GESTURE IS NOT
## HERMETIC"): with the window foregrounded and the pointer moved during the run, foreign motion
## events reaching the viewport went **41 → 163** and the run failed; with this seal in place the
## same adversary delivers **zero** and the run is green.
static func seal_from_real_mouse(window: Window) -> void:
	if window == null:
		return
	# A degenerate (zero-area) passthrough polygon means NO region of the window accepts mouse
	# events, so every one of them passes through to whatever is behind it. An EMPTY array is the
	# opposite — Godot documents it as "disable passthrough", i.e. the default where the window
	# intercepts everything — so the three coincident points are load-bearing rather than a
	# placeholder.
	window.mouse_passthrough_polygon = PackedVector2Array(
		[Vector2.ZERO, Vector2.ZERO, Vector2.ZERO])
