class_name LeaderPortrait
extends Control

## **THE LEADER'S PORTRAIT SQUARE** (#859) at the head of the Band tab's leader block. Three states:
## `empty` (no leader yet -- a dimmed frame and a faint silhouette), filled (a chief), and filled
## with `council` (an elders' council, whose speaker's portrait wears a small group badge).
##
## **THE ART SEAM**: if `PORTRAIT_TEXTURE_PATH` exists as a resource it is drawn scaled into the square
## in place of the silhouette. Until art lands the head-and-shoulders silhouette is drawn in the band's
## faction map colour (`MapView.faction_color`, the tint FactionMark and the map's banners use).

## Drop-in path for the portrait art; the drawn silhouette is the fallback when no file is there.
const PORTRAIT_TEXTURE_PATH := "res://assets/portraits/leader_placeholder.png"

## The square's logical side.
const PORTRAIT_SIZE := 48.0
## Frame outline width, and the inset the artwork keeps from it.
const FRAME_WIDTH := 1.0
const FRAME_INSET := 2.0
## Silhouette geometry as fractions of the square: head circle centre/radius, and the shoulders'
## half-ellipse centre-y and radii (its lower half is never drawn, so it reads as a bust).
const HEAD_CENTER := Vector2(0.5, 0.36)
const HEAD_RADIUS := 0.17
const SHOULDER_CENTER_Y := 0.92
const SHOULDER_RADIUS_X := 0.34
const SHOULDER_RADIUS_Y := 0.34
const SHOULDER_POINTS := 24
## Alphas: a leader's silhouette is full strength; an empty slot is a ghost of it.
const FILLED_ALPHA := 1.0
const EMPTY_ALPHA := 0.22
const EMPTY_FRAME_ALPHA := 0.55
## The council badge: a disc in the bottom-right corner holding three dots.
const BADGE_RADIUS := 8.0
const BADGE_MARGIN := 1.0
const BADGE_DOT_RADIUS := 1.4
const BADGE_DOT_SPACING := 4.0
const BADGE_OUTLINE_WIDTH := 1.0
const BADGE_DOT_COUNT := 3
const BADGE_ARC_POINTS := 24

var _faction: int = HudConst.PLAYER_FACTION_ID
var _empty := false
var _council := false

## A portrait for `faction`'s leader. `empty` dims it to a vacancy; `council` adds the group badge.
static func make(faction: int, empty: bool, council: bool) -> LeaderPortrait:
    var portrait := LeaderPortrait.new()
    portrait._faction = faction
    portrait._empty = empty
    portrait._council = council and not empty
    portrait.custom_minimum_size = Vector2(PORTRAIT_SIZE, PORTRAIT_SIZE)
    portrait.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    portrait.mouse_filter = Control.MOUSE_FILTER_PASS
    return portrait

func is_empty_slot() -> bool:
    return _empty

func is_council() -> bool:
    return _council

## True when real art is on disk at the seam path.
static func has_art() -> bool:
    return ResourceLoader.exists(PORTRAIT_TEXTURE_PATH)

func _draw() -> void:
    var box := Rect2(Vector2.ZERO, Vector2(PORTRAIT_SIZE, PORTRAIT_SIZE))
    draw_rect(box, HudStyle.GROUND_2, true)
    var alpha := EMPTY_ALPHA if _empty else FILLED_ALPHA
    # Through IconSprites' cache, never a bare `load`: a texture only this local holds is freed when
    # `_draw` returns, before the canvas renders the command, and the square draws solid white.
    var tex := IconSprites.texture_for(PORTRAIT_TEXTURE_PATH, false) if has_art() else null
    if tex != null:
        draw_texture_rect(tex, box.grow(-FRAME_INSET), false, Color(1.0, 1.0, 1.0, alpha))
    else:
        _draw_silhouette(alpha)
    var frame := HudStyle.LINE
    frame.a = EMPTY_FRAME_ALPHA if _empty else 1.0
    draw_rect(box, frame, false, FRAME_WIDTH)
    if _council:
        _draw_badge()

func _draw_silhouette(alpha: float) -> void:
    var tint := MapView.faction_color(_faction, MapView.BAND_FACTION_FALLBACK_COLOR)
    tint.a = alpha
    var side := PORTRAIT_SIZE
    draw_circle(HEAD_CENTER * side, HEAD_RADIUS * side, tint)
    var points := PackedVector2Array()
    var centre := Vector2(0.5 * side, SHOULDER_CENTER_Y * side)
    for i in range(SHOULDER_POINTS + 1):
        var angle := PI + PI * float(i) / float(SHOULDER_POINTS)
        points.append(centre + Vector2(cos(angle) * SHOULDER_RADIUS_X, sin(angle) * SHOULDER_RADIUS_Y) * side)
    draw_colored_polygon(points, tint)

func _draw_badge() -> void:
    var inset := BADGE_RADIUS + BADGE_MARGIN
    var centre := Vector2(PORTRAIT_SIZE - inset, PORTRAIT_SIZE - inset)
    draw_circle(centre, BADGE_RADIUS, HudStyle.PANEL_SOLID)
    draw_arc(centre, BADGE_RADIUS, 0.0, TAU, BADGE_ARC_POINTS, HudStyle.INK_DIM, BADGE_OUTLINE_WIDTH)
    var first := -BADGE_DOT_SPACING * float(BADGE_DOT_COUNT - 1) * 0.5
    for i in range(BADGE_DOT_COUNT):
        draw_circle(centre + Vector2(first + BADGE_DOT_SPACING * float(i), 0.0), BADGE_DOT_RADIUS, HudStyle.INK)
