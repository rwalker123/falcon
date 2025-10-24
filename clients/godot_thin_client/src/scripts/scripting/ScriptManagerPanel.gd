extends VBoxContainer
class_name ScriptManagerPanel

const ScriptHostManager := preload("res://src/scripts/scripting/ScriptHostManager.gd")

@onready var _refresh_button: Button = %RefreshButton
@onready var _package_list: ItemList = %PackageList
@onready var _enable_button: Button = %EnableButton
@onready var _disable_button: Button = %DisableButton
@onready var _reload_button: Button = %ReloadButton
@onready var _details_text: RichTextLabel = %DetailsText

var _manager: ScriptHostManager = null
var _packages: Array = []

func _ready() -> void:
    _package_list.item_selected.connect(_on_package_selected)
    _refresh_button.pressed.connect(_on_refresh_pressed)
    _enable_button.pressed.connect(_on_enable_pressed)
    _disable_button.pressed.connect(_on_disable_pressed)
    _reload_button.pressed.connect(_on_reload_pressed)
    _update_button_states()

func set_manager(manager: ScriptHostManager) -> void:
    if _manager != null and _manager.is_connected("packages_changed", Callable(self, "_on_packages_changed")):
        _manager.disconnect("packages_changed", Callable(self, "_on_packages_changed"))
    _manager = manager
    if _manager != null:
        _manager.packages_changed.connect(_on_packages_changed)
        _on_packages_changed(_manager.packages_snapshot())
    else:
        _on_packages_changed([])

func _on_packages_changed(packages: Array) -> void:
    _packages = packages.duplicate(true)
    _package_list.clear()
    for pkg in _packages:
        var manifest: Dictionary = pkg.get("manifest", {})
        var label: String = manifest.get("id", pkg.get("key", ""))
        var enabled: bool = pkg.get("enabled", false)
        var script_id: int = pkg.get("script_id", -1)
        var status: String = "●" if enabled else "○"
        var item_text := "%s %s" % [status, label]
        if script_id >= 0:
            item_text += " [id %d]" % script_id
        var idx := _package_list.add_item(item_text)
        _package_list.set_item_metadata(idx, pkg.get("key", ""))
    if _packages.size() == 0:
        _details_text.text = "[i]No script packages discovered.[/i]"
    _update_button_states()

func _on_package_selected(index: int) -> void:
    _show_details(index)
    _update_button_states()

func _show_details(index: int) -> void:
    if index < 0 or index >= _packages.size():
        _details_text.text = ""
        return
    var pkg: Dictionary = _packages[index]
    var manifest: Dictionary = pkg.get("manifest", {})
    var lines: Array[String] = []
    lines.append("[b]%s[/b]" % manifest.get("id", pkg.get("key", "")))
    if manifest.has("version"):
        lines.append("Version: %s" % manifest.get("version", ""))
    if manifest.has("author"):
        lines.append("Author: %s" % manifest.get("author", ""))
    if manifest.has("description") and not String(manifest.get("description", "")).is_empty():
        lines.append(manifest.get("description", ""))
    lines.append("Manifest: %s" % pkg.get("manifest_path", ""))
    lines.append("Entry: %s" % pkg.get("entry_path", ""))
    lines.append("Status: %s" % ("Enabled" if pkg.get("enabled", false) else "Disabled"))
    if pkg.get("last_error", "") != "":
        lines.append("[color=#ff6d6d]Last error: %s[/color]" % pkg.get("last_error", ""))
    var capabilities: Array = manifest.get("capabilities", [])
    if capabilities.size() > 0:
        lines.append("Capabilities: %s" % ", ".join(capabilities))
    var subs: Array = pkg.get("subscriptions", [])
    if subs.size() > 0:
        lines.append("Subscriptions: %s" % ", ".join(subs))
    _details_text.text = "\n".join(lines)

func _update_button_states() -> void:
    var has_selection := _package_list.get_selected_items().size() > 0
    if not has_selection:
        _enable_button.disabled = true
        _disable_button.disabled = true
        _reload_button.disabled = true
        return
    var index: int = _package_list.get_selected_items()[0]
    if index < 0 or index >= _packages.size():
        _enable_button.disabled = true
        _disable_button.disabled = true
        _reload_button.disabled = true
        return
    var pkg: Dictionary = _packages[index]
    var enabled: bool = pkg.get("enabled", false)
    _enable_button.disabled = enabled
    _disable_button.disabled = not enabled
    _reload_button.disabled = false

func _selected_package_key() -> String:
    var selected := _package_list.get_selected_items()
    if selected.size() == 0:
        return ""
    var index: int = selected[0]
    if index < 0 or index >= _packages.size():
        return ""
    return _packages[index].get("key", "")

func _on_refresh_pressed() -> void:
    if _manager != null:
        _manager.refresh()

func _on_enable_pressed() -> void:
    var key := _selected_package_key()
    if key == "" or _manager == null:
        return
    var result: Dictionary = _manager.enable_package(key)
    if not result.get("ok", false):
        _details_text.text = "[color=#ff6d6d]%s[/color]" % result.get("error", "Enable failed")

func _on_disable_pressed() -> void:
    var key := _selected_package_key()
    if key == "" or _manager == null:
        return
    var result: Dictionary = _manager.disable_package(key)
    if not result.get("ok", false):
        _details_text.text = "[color=#ff6d6d]%s[/color]" % result.get("error", "Disable failed")

func _on_reload_pressed() -> void:
    var key := _selected_package_key()
    if key == "" or _manager == null:
        return
    var result: Dictionary = _manager.reload_package(key)
    if not result.get("ok", false):
        _details_text.text = "[color=#ff6d6d]%s[/color]" % result.get("error", "Reload failed")

func clear() -> void:
    _packages.clear()
    _package_list.clear()
    _details_text.text = ""
    _update_button_states()
