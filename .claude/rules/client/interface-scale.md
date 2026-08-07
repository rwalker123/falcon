---
paths:
  - "clients/godot_thin_client/src/scripts/ui_scaler.gd"
  - "clients/godot_thin_client/src/scripts/ClientSettings.gd"
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/CachedMapRenderer.gd"
---

# Interface scale — the UI scales, the map is held still

The Options pane's **Interface scale** slider makes the whole HUD bigger or smaller without
touching the map. Four scripts share the flow — the preference store, the menu row that writes it,
the autoload that applies it to the window, and the map that undoes it for itself — and the last of
those is the half that is easy to lose. A change that scales the UI and forgets the map does not
look broken; it looks like the map zoomed, which is exactly the bug that got the previous
implementation deleted.

## Key scripts

| Script | Its part in the scale flow |
|--------|----------------------------|
| `ClientSettings.gd` | Holds `ui_scale` (default `1.0`, clamped to [0.75, 1.50], `[ui]` section of `user://client_settings.cfg`). Its own section, not `[map]` — it is not a map setting |
| `ui_scaler.gd` (`UiScaler` autoload) | The **only** writer of `Window.content_scale_factor`. Applies once on `_ready` and on every `ClientSettings.changed`, and does nothing else |
| `ui/MenuShell.gd` | The Options pane's "Interface scale" row — the first row in the pane, since it governs the legibility of every row under it. Writes `ClientSettings` **and stops**. Its arrival is why `_make_speed_slider_row` takes a `step` parameter: the builder is the pane's general slider row now, and a type-size increment is not a speed granularity |
| `MapView.gd` | Counter-scales **itself** so the map is immune, and owns `screen_size_local()` — the one expression that converts the screen into map-local units |
| `CachedMapRenderer.gd` | Sizes the cache SubViewport from `map_view.screen_size_local()`, never from the raw viewport rect |

## The lever is `content_scale_factor`, and the UI needed no changes at all

The project runs stretch mode `canvas_items` at a 1920×1080 base with `aspect = expand`. Under that
mode `Window.content_scale_factor = s` **shrinks the logical viewport by exactly `s`** — measured in
Godot 4.7 against this project: 1920 → 1280 at `s = 1.5`, and 1920 → 2560 at `s = 0.75`. A Control
hosted directly by a `CanvasLayer` resolves its anchors against that logical rect, so it re-lays-out
on its own.

That is why **no UI layout code is part of this feature**. Every anchor, every panel that positions
itself from `get_visible_rect()`, and every GUI input coordinate is already expressed in the logical
space the engine just resized. `Main`'s loading overlay is a `CanvasLayer` too, so it scales with
the rest without being mentioned anywhere. **If a UI script starts compensating for
`content_scale_factor`, that is a bug** — it is double-counting a scale the engine already applied.

The alternative — scaling each UI `CanvasLayer` and resizing its root Control to `viewport / s` —
was measured and rejected. A `CanvasLayer.scale` of 1.5 leaves its full-rect child Control at the
full 1920: the anchors still resolve against the viewport, so the layer renders 50% past the window
edge unless every root Control is resized by hand. That is a per-layer correction in six layers
plus every popover that reads the viewport, to buy nothing `content_scale_factor` does not already
give.

## The map counter-scales, and that is the whole point

`content_scale_factor` is viewport-global: the map is a `Node2D` (`MapLayer`, running `MapView.gd`)
in the same canvas, so it would grow with the chrome. It must not. The player already has a zoom
control, and an interface setting that silently also zooms the map is precisely what commit
`dc9f1e1b` removed — its message records the widget as "`content_scale_factor` masquerading as
zoom". **Interface scale returning without the counter-scale would re-create that bug under a new
label.**

So `MapView` sets its own `scale` to `Vector2.ONE / ClientSettings.ui_scale`. It subscribes to
`ClientSettings.changed` directly; **`UiScaler` has no handle to `MapView` and must not grow one**,
the same boundary the fog row keeps (see `fog-of-war.md`). `MIN_UI_SCALE` guards the reciprocal
against a hand-edited config file only — the slider and `ClientSettings` both clamp far above it —
and refuses rather than dividing by ~0.

The counter-scale means the screen, measured in map-local units, is a constant — and
`screen_size_local()` is that measurement:

```
get_viewport_rect().size / get_global_transform_with_canvas().get_scale()
```

Measured across `ui_scale` ∈ {0.75, 1.0, 1.5}, that expression returns a flat 1920 while the raw
viewport rect swings from 2560 to 1280. `get_global_transform_with_canvas()` composes the canvas
transform **with the node's own transform**, so it *subsumes* the camera-scale division
`_get_adjusted_viewport_size` used to do by hand — that is why the two lines collapsed into one
call rather than stacking. Every map-side consumer of "how big is the screen" goes through it:
`_get_adjusted_viewport_size`, the map-cache sizing and its pan-buffer test, the keyboard-zoom
centre, and `CachedMapRenderer`. **A raw `get_viewport_rect().size` in map code is a defect** — it
reads the shrunken logical rect and the map silently mis-sizes its cache or clamps pan to the wrong
bounds.

Mouse handling needed nothing. `MapView` hit-tests through `get_local_mouse_position()`, which
composes ancestor transforms, so the node's own scale is already in it.

### The insets cross the coordinate boundary — but the node's `position` does not

`set_reserved_inset` is fed by docked UI panels, so its values are in **canvas** units; the map
subtracts them in **map-local** units. Those two spaces are the same only at `ui_scale = 1.0`, so
`_reserved_inset_span_local()` converts them — **at the point of use, not at set time**, because the
scale can change after a panel reports its width and a value converted on arrival would be stale the
moment the slider moves.

**The node's own `position = Vector2(_inset_left, _inset_top)` stays raw, and that is correct, not
an oversight.** `position` is expressed in the PARENT's space, which is the canvas space the insets
already arrive in; converting it would apply the scale twice. The rule is the space the number is
consumed in, not the number's origin: lengths compared against `screen_size_local()` convert, the
translation does not.

## Three traps that fail quietly

**Autoload order.** `UiScaler._ready` reads `ClientSettings`, and autoload `_ready` runs in
declaration order, so the `UiScaler=` line in `project.godot` sits **after** `ClientSettings=`.
Reordering them back gives a client that boots at scale 1.0 and only obeys the setting after the
player next moves the slider.

**Handler order on `changed` — and why it is safe.** `MapView._apply_ui_scale` recomputes layout
metrics through `screen_size_local()`, which reads the viewport rect, in the same emission that
`UiScaler` uses to resize that rect. Two measured facts make that ordering sound: assigning
`content_scale_factor` updates `get_viewport().get_visible_rect()` **synchronously, within the same
statement** (no deferred frame), and `UiScaler` is an autoload so it connects to `changed` before any
scene node does — including a `MapView` built later by the `LandingScreen` → `Main` swap. The map
therefore never recomputes against a stale rect. **Anything that makes `UiScaler` connect later
re-opens this**, and the symptom would be a map drawn at the wrong hex size until the next pan, zoom
or snapshot happened to recompute the metrics.

**Harness contamination.** `ClientSettings` is an autoload that reads the developer's real
`user://client_settings.cfg`, and `UiScaler` pushes whatever it finds onto the window — so **every
offline PNG would re-project on a machine whose slider has been moved**. `ui_preview`,
`map_preview`, `band_panel_preview` and `menu_preview` each pin `ui_scale` to its default in their
prologue: assign the **member**, never `set_ui_scale` (the setter would `_save()` over the
developer's own file), then re-emit `changed` so the pin travels the real path. `menu_preview` reads
the real config for the row *values* on purpose — its docstring says to judge the rows, not the
numbers — but rendering at the real config's *scale* is a different thing, and it would push the
Options pane out of the one frame that exists to show it.

The mirror of that: a harness state that *exercises* the scale must restore 1.0 when it is done.
`content_scale_factor` is window state, not scene state, so a leak corrupts every PNG after it in
the same run with no error and no failed assertion — the same class of silent failure as the fog
default that blanked five `map_preview` frames (`fog-of-war.md` → the offline harnesses must state
their fog condition). `tools/ui_preview/chapters/interface_scale.gd` walks 0.75 and 1.50 and
restores the default; it is appended **last** in `CHAPTERS` so no existing frame moves.

## The autoload was dead before this

`ui_scaler.gd` existed and was registered as the `UiScaler` autoload, but had never had any effect:
it set `ProjectSettings.set_setting("display/window/gui/theme_scale", …)` from the screen DPI — no
such project setting exists in Godot 4 — and then assigned `get_tree().root.theme = null`, which
was already null. The name and the autoload slot were right, so this work replaced the body rather
than adding a fifth script. Worth knowing if you go looking for when scaling "regressed": it never
worked.
