extends VBoxContainer
class_name FaunaInspectorPanel

## Inspector "Fauna" tab. Lists herds, shows per-herd detail + follow rewards, and
## requests a follow-herd command. The follow command needs the active faction
## (coordinator-owned), so the button emits follow_herd_requested and the coordinator
## resolves the faction + issues the command. herd_selected mirrors the selection into
## the Commands-tab follow field.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const HERD_CONSUMPTION_BIOMASS := 250.0
const HERD_PROVISIONS_YIELD_PER_BIOMASS := 0.02
const HERD_TRADE_GOODS_YIELD_PER_BIOMASS := 0.005
const HERD_FOLLOW_MORALE_GAIN := 0.03
const HERD_KNOWLEDGE_PROGRESS_PER_BIOMASS := 0.0004
const HERD_KNOWLEDGE_PROGRESS_CAP := 0.25

## Emitted when the user asks to follow the selected herd; the coordinator resolves
## the active faction and issues the command.
signal follow_herd_requested(herd_id: String)
## Emitted on selection so the coordinator can mirror the id into the Commands follow field.
signal herd_selected(herd_id: String)

@onready var _list: ItemList = %FaunaList
@onready var _detail_text: RichTextLabel = %FaunaDetail
@onready var _follow_button: Button = %FaunaFollowButton

var _entries: Array = []
var _selected_herd_id: String = ""
var _connected: bool = false

func _ready() -> void:
	if _list != null:
		_list.item_selected.connect(_on_item_selected)
	if _follow_button != null:
		_follow_button.pressed.connect(_on_follow_pressed)
	_render()

## Coordinator contract: read the herds key; re-render.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	if data.has("herds"):
		var herds_variant: Variant = data["herds"]
		if herds_variant is Array:
			_entries = (herds_variant as Array).duplicate(true)
			_render()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_entries.clear()
	_selected_herd_id = ""
	_render()

## Coordinator collaborator: command-socket connection state. The follow button is
## enabled only while connected AND a herd is selected.
func set_command_connected(connected: bool) -> void:
	_connected = connected
	_update_follow_button()

func _render() -> void:
	if _list == null or _detail_text == null:
		return
	_list.clear()
	_selected_herd_id = ""
	if _entries.is_empty():
		_detail_text.text = "[i]Awaiting telemetry.[/i]"
		_update_follow_button()
		return
	for herd_variant in _entries:
		if not (herd_variant is Dictionary):
			continue
		var herd: Dictionary = herd_variant
		var label := String(herd.get("label", herd.get("id", "Herd")))
		var x := int(herd.get("x", 0))
		var y := int(herd.get("y", 0))
		var biomass := float(herd.get("biomass", 0.0))
		var entry_label := "%s — (%d,%d) · %.0f" % [label, x, y, biomass]
		var item_index := _list.add_item(entry_label)
		_list.set_item_metadata(item_index, herd)
	_detail_text.text = "[i]Select a herd to view details.[/i]"
	_update_follow_button()

func _on_item_selected(index: int) -> void:
	if _list == null or _detail_text == null:
		return
	var meta: Variant = _list.get_item_metadata(index)
	if not (meta is Dictionary):
		_detail_text.text = "[i]No herd details available.[/i]"
		_selected_herd_id = ""
		_update_follow_button()
		return
	var info: Dictionary = meta
	_selected_herd_id = String(info.get("id", ""))
	var label := String(info.get("label", info.get("id", "Herd")))
	var species := String(info.get("species", ""))
	var x := int(info.get("x", 0))
	var y := int(info.get("y", 0))
	var biomass := float(info.get("biomass", 0.0))
	var route_length := int(info.get("route_length", 0))
	var lines: Array[String] = []
	lines.append("[b]%s[/b]" % label)
	if species != "":
		lines.append(species)
	lines.append("Position (%d, %d)" % [x, y])
	lines.append("Biomass %.0f" % biomass)
	lines.append("Route length %d" % route_length)
	var reward_lines := _herd_reward_summary_lines(biomass)
	if not reward_lines.is_empty():
		lines.append("")
		lines.append_array(reward_lines)
	var next_x := int(info.get("next_x", -1))
	var next_y := int(info.get("next_y", -1))
	if next_x >= 0 and next_y >= 0:
		lines.append("")
		lines.append("Next waypoint (%d, %d)" % [next_x, next_y])
	_detail_text.text = "\n".join(lines)
	herd_selected.emit(String(info.get("id", "")))
	_update_follow_button()

func _on_follow_pressed() -> void:
	if _selected_herd_id == "":
		return
	follow_herd_requested.emit(_selected_herd_id)

func _update_follow_button() -> void:
	if _follow_button != null:
		_follow_button.disabled = not (_connected and _selected_herd_id != "")

func _herd_reward_summary_lines(biomass: float) -> Array[String]:
	var lines: Array[String] = []
	if biomass <= 0.0:
		return lines
	var consumption: float = min(biomass, HERD_CONSUMPTION_BIOMASS)
	if consumption <= 0.0:
		return lines
	var provisions: float = round(consumption * HERD_PROVISIONS_YIELD_PER_BIOMASS)
	var trade_goods: float = round(consumption * HERD_TRADE_GOODS_YIELD_PER_BIOMASS)
	var lore_progress: float = min(
		consumption * HERD_KNOWLEDGE_PROGRESS_PER_BIOMASS,
		HERD_KNOWLEDGE_PROGRESS_CAP
	)
	lines.append("[b]Hunt rewards[/b]")
	lines.append("• Morale +%.2f per band" % HERD_FOLLOW_MORALE_GAIN)
	if provisions > 0 or trade_goods > 0:
		lines.append("• Supplies: +%d provisions, +%d trade goods" % [int(provisions), int(trade_goods)])
	if lore_progress > 0.0:
		lines.append("• Fauna lore +%.1f%% progress" % (lore_progress * 100.0))
	lines.append("• Fog pulse reveals nearby tiles")
	return lines
