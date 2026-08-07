extends Node

## The INTERFACE SCALE applier — the one and only writer of `Window.content_scale_factor`.
##
## It reads `ClientSettings.ui_scale` and does nothing else: no theme, no font, no layout. Setting
## `content_scale_factor = s` shrinks the logical viewport by exactly `s` (with the project's
## `canvas_items` stretch and `expand` aspect, a 1920-wide base becomes 1280 at 1.5), so every UI
## `CanvasLayer`, every anchor and all GUI input re-lay-out at the new size with no code change —
## which is why nothing in the HUD had to be touched for this setting to exist.
##
## **The map is NOT scaled by it.** `MapView` counter-scales itself by `1 / ui_scale` off the same
## setting, so the world holds still while the chrome around it grows. That compensation belongs to
## `MapView` — this autoload deliberately holds no handle to it, and must not grow one.
##
## **It is an autoload, and that is load-bearing**: the setting has to hold on the landing screen
## (which has no `Main`) and survive the `LandingScreen.tscn` -> `Main.tscn` swap. It must also be
## declared AFTER `ClientSettings` in `project.godot`'s `[autoload]` block — autoload `_ready` runs
## in declaration order, and this one reads that one.

func _ready() -> void:
	_apply_ui_scale()
	ClientSettings.changed.connect(_apply_ui_scale)

## Push the persisted interface scale onto the window. `ClientSettings` has already clamped it to
## [UI_SCALE_MIN, UI_SCALE_MAX], so there is nothing to re-validate here.
func _apply_ui_scale() -> void:
	get_window().content_scale_factor = ClientSettings.ui_scale
