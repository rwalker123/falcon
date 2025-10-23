extends Object
class_name Typography

const DEFAULT_FONT_SIZE := 22
const MIN_FONT_SIZE := 12
const MAX_FONT_SIZE := 200

const STYLE_BODY := "body"
const STYLE_HEADING := "heading"
const STYLE_SUBHEADING := "subheading"
const STYLE_CAPTION := "caption"
const STYLE_LEGEND := "legend"
const STYLE_CONTROL := "control"

const _STYLE_DELTAS := {
    STYLE_BODY: 0,
    STYLE_HEADING: 4,
    STYLE_SUBHEADING: 2,
    STYLE_CAPTION: -3,
    STYLE_LEGEND: -2,
    STYLE_CONTROL: 0,
}

const _FONT_TYPES := [
    "Label",
    "RichTextLabel",
    "ItemList",
    "CheckButton",
    "Button",
    "OptionButton",
    "SpinBox",
    "TabContainer",
    "LineEdit",
    "TextEdit",
    "MenuButton",
    "PopupMenu",
    "Tree",
    "TooltipPanel"
]

static var _initialized := false
static var _base_font_size := DEFAULT_FONT_SIZE
static var _style_sizes: Dictionary = {}
static var _shared_theme: Theme = null
static var _fallback_font: Font = null

static func initialize() -> void:
    if _initialized:
        return
    _base_font_size = _resolve_base_font_size()
    _build_style_sizes()
    _shared_theme = _build_theme()
    _fallback_font = _probe_fallback_font()
    _initialized = true

static func base_font_size() -> int:
    initialize()
    return _base_font_size

static func theme() -> Theme:
    initialize()
    return _shared_theme

static func size_for(style: StringName) -> int:
    initialize()
    return int(_style_sizes.get(style, _base_font_size))

static func line_height(style: StringName) -> float:
    initialize()
    var font := _fallback_font
    if font != null:
        return font.get_height(size_for(style))
    return float(size_for(style)) * 1.25

static func apply_theme(target: Control) -> void:
    initialize()
    if target == null:
        return
    target.theme = _shared_theme

static func apply(control: Control, style: StringName = STYLE_BODY) -> void:
    initialize()
    if control == null:
        return
    var resolved_size: int = size_for(style)
    control.add_theme_font_size_override("font_size", resolved_size)
    if control is RichTextLabel:
        control.add_theme_font_size_override("default_font_size", resolved_size)
        control.add_theme_font_size_override("normal_font_size", resolved_size)
        control.add_theme_font_size_override("bold_font_size", resolved_size)
        control.add_theme_font_size_override("italics_font_size", resolved_size)
        control.add_theme_font_size_override("bold_italics_font_size", resolved_size)
        control.add_theme_font_size_override("mono_font_size", max(resolved_size - 1, MIN_FONT_SIZE))
    elif control is OptionButton:
        var popup: PopupMenu = control.get_popup()
        if popup != null:
            popup.add_theme_font_size_override("font_size", resolved_size)
    elif control is SpinBox:
        var editor: LineEdit = control.get_line_edit()
        if editor != null:
            editor.add_theme_font_size_override("font_size", resolved_size)

static func style_sizes() -> Dictionary:
    initialize()
    return _style_sizes.duplicate()

static func _resolve_base_font_size() -> int:
    var font_size := DEFAULT_FONT_SIZE
    var env_value := OS.get_environment("INSPECTOR_FONT_SIZE")
    if env_value != "":
        var parsed := int(env_value)
        if parsed >= MIN_FONT_SIZE and parsed <= MAX_FONT_SIZE:
            font_size = parsed
        else:
            print("Inspector typography: INSPECTOR_FONT_SIZE='%s' ignored (expected %d-%d)." % [
                env_value,
                MIN_FONT_SIZE,
                MAX_FONT_SIZE
            ])
    return font_size

static func _build_style_sizes() -> void:
    _style_sizes.clear()
    for style in _STYLE_DELTAS.keys():
        var delta: int = int(_STYLE_DELTAS[style])
        var resolved: int = clamp(_base_font_size + delta, MIN_FONT_SIZE, MAX_FONT_SIZE)
        _style_sizes[style] = resolved

static func _build_theme() -> Theme:
    var theme := Theme.new()
    for theme_type in _FONT_TYPES:
        theme.set_font_size("font_size", theme_type, _base_font_size)
    theme.set_font_size("default_font_size", "RichTextLabel", _base_font_size)
    theme.set_font_size("normal_font_size", "RichTextLabel", _base_font_size)
    return theme

static func _probe_fallback_font() -> Font:
    var probe := Label.new()
    var font := probe.get_theme_default_font()
    probe.free()
    return font
