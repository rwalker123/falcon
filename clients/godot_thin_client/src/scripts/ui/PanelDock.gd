extends RefCounted
class_name PanelDock

## Ordered controller for one dock region (a VBoxContainer inside a
## ScrollContainer). Panels register with a priority; the dock reparents them
## into its stack in priority order. Visibility is data-driven: hiding a panel
## (`set_relevant(panel, false)`) removes it from layout flow and the stack
## reflows with no gap. The ScrollContainer that wraps the stack owns overflow,
## so individual panels never need bespoke height clamping.

var _container: VBoxContainer
var _entries: Array = []  # Array of {panel: Control, priority: int}

func _init(container: VBoxContainer) -> void:
	_container = container

## Register (or re-prioritise) a panel in this dock, then reorder the stack.
func add(panel: Control, priority: int) -> void:
	if panel == null or _container == null:
		return
	for entry in _entries:
		if entry.get("panel") == panel:
			entry["priority"] = priority
			_reorder()
			return
	_entries.append({"panel": panel, "priority": priority})
	_reorder()

## Drop a panel from this dock's registry. The node keeps its current parent
## until another dock adopts it, so callers moving a panel between docks should
## remove() from the source then add() to the target.
func remove(panel: Control) -> void:
	for idx in range(_entries.size()):
		if _entries[idx].get("panel") == panel:
			_entries.remove_at(idx)
			return

## Show or hide a panel; the stack reflows to close the gap when hidden.
func set_relevant(panel: Control, relevant: bool) -> void:
	if panel != null:
		panel.visible = relevant

func _reorder() -> void:
	_entries.sort_custom(_sort_by_priority)
	for idx in range(_entries.size()):
		var panel: Control = _entries[idx].get("panel")
		if panel == null:
			continue
		if panel.get_parent() != _container:
			if panel.get_parent() != null:
				panel.get_parent().remove_child(panel)
			_container.add_child(panel)
		_container.move_child(panel, idx)

func _sort_by_priority(a: Dictionary, b: Dictionary) -> bool:
	return int(a.get("priority", 0)) < int(b.get("priority", 0))
