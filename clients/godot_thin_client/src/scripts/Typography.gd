extends Object
class_name Typography

const DEFAULT_FONT_SIZE := 18
const MIN_FONT_SIZE := 12
const MAX_FONT_SIZE := 200

const STYLE_BODY := "body"
const STYLE_HEADING := "heading"
const STYLE_SUBHEADING := "subheading"
const STYLE_CAPTION := "caption"
const STYLE_LEGEND := "legend"
const STYLE_CONTROL := "control"

static func initialize() -> void:
    pass

static func base_font_size() -> int:
    return DEFAULT_FONT_SIZE

static func theme() -> Theme:
    return null

static func size_for(_style: StringName) -> int:
    return DEFAULT_FONT_SIZE

static func line_height(_style: StringName) -> float:
    return float(DEFAULT_FONT_SIZE)

static func apply_theme(_target: Control) -> void:
    pass

static func apply(_control: Control, _style: StringName = STYLE_BODY) -> void:
    pass

static func style_sizes() -> Dictionary:
    return {}
