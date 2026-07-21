extends RefCounted
class_name MapSizes

## Canonical map-size registry — the single source of truth shared by the inspector's
## Map tab (`ui/inspector/MapPanel.gd`) and the New Game setup pane
## (`ui/MenuShell.gd`). Both surfaces offer the SAME five sizes, so the list lives
## here (DRY) rather than being duplicated per surface. Pure data; never instantiated.

const OPTIONS := [
	{"key": "tiny", "label": "Tiny", "width": 56, "height": 36},
	{"key": "small", "label": "Small", "width": 66, "height": 42},
	{"key": "standard", "label": "Standard", "width": 80, "height": 52},
	{"key": "large", "label": "Large", "width": 104, "height": 64},
	{"key": "huge", "label": "Huge", "width": 128, "height": 80},
]

const DEFAULT_KEY := "standard"

## The {key,label,width,height} descriptor for a key, or the Standard default if unknown.
static func option_for(key: String) -> Dictionary:
	for option in OPTIONS:
		if String(option.get("key", "")) == key:
			return option
	return option_for(DEFAULT_KEY) if key != DEFAULT_KEY else OPTIONS[2]
