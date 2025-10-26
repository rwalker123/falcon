extends Node

# Baseline DPI and scale reference
const BASE_DPI := 96.0

func _ready():
    # Get current screen DPI
    var dpi := DisplayServer.screen_get_dpi()
    var scale := dpi / BASE_DPI

    # Apply global theme scaling
    ProjectSettings.set_setting("display/window/gui/theme_scale", scale)

    # Optional: force settings to take effect immediately
    get_tree().root.theme = null
