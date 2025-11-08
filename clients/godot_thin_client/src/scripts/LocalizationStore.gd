extends RefCounted
class_name LocalizationStore

var _tables: Dictionary = {}
var _fallback_language: String = "en"

func load_default() -> void:
    _tables.clear()
    _load_language(_fallback_language)

func _load_language(language: String) -> void:
    var path := "res://src/data/localization/%s.json" % language
    if not FileAccess.file_exists(path):
        push_warning("LocalizationStore: missing locale file %s" % path)
        return
    var file := FileAccess.open(path, FileAccess.READ)
    if file == null:
        push_warning("LocalizationStore: unable to open %s" % path)
        return
    var contents := file.get_as_text()
    file.close()
    var parsed: Variant = JSON.parse_string(contents)
    if typeof(parsed) != TYPE_DICTIONARY:
        push_warning("LocalizationStore: invalid JSON in %s" % path)
        return
    _tables[language] = parsed

func resolve(key: String, fallback: String = "") -> String:
    if key == "":
        return fallback
    if _tables.has(_fallback_language):
        var table: Dictionary = _tables[_fallback_language]
        if table.has(key):
            return str(table[key])
    return fallback
