extends RefCounted
class_name FoodIcons

## Single source of truth for map-marker icons — food sources and fauna herds —
## shared by the selection panel's Harvest/Hunt button and the map's markers so a
## given module/species always reads the same. Food icons map to the ecosystem
## food modules in `core_sim/CLAUDE.md` (Coastal Littoral = shellfish, Savanna =
## grassland herds, Temperate Forest = nut groves, etc.); herd icons are inferred
## from the herd label since the snapshot carries no species field.

const DEFAULT := "🌿"
const HUNT := "🦌"

# Migratory herd markers. Generic grazer by default (kept distinct from the HUNT
# deer); a species keyword in the herd label upgrades it to a specific animal.
const HERD_DEFAULT := "🦬"
const HERD_SPECIES := {
	"mammoth": "🦣",
	"aurochs": "🦬",
	"bison": "🦬",
	"buffalo": "🦬",
	"cattle": "🐂",
	"oxen": "🐂",
	"reindeer": "🦌",
	"caribou": "🦌",
	"deer": "🦌",
	"elk": "🦌",
	"horse": "🐎",
	"boar": "🐗",
	"goat": "🐐",
	"ibex": "🐐",
	"sheep": "🐑",
}

const ICONS := {
	"coastal_littoral": "🐚",
	"riverine_delta": "🐟",
	"savanna_grassland": "🌾",
	"temperate_forest": "🌰",
	"boreal_arctic": "🦭",
	"montane_highland": "🥔",
	"wetland_swamp": "🪷",
	"semi_arid_scrub": "🌵",
	"coastal_upwelling": "🦐",
	"mixed_woodland": "🍄",
}

## Icon for a food module key (falls back to a generic sprig).
static func for_module(module_key: String) -> String:
	return String(ICONS.get(module_key.strip_edges(), DEFAULT))

## Icon for a food site, using the game-trail (hunt) icon when the site is hunted
## rather than gathered.
static func for_site(module_key: String, is_hunt: bool) -> String:
	if is_hunt:
		return HUNT
	return for_module(module_key)

## Icon for a migratory herd, inferred from a species keyword in its label
## (falls back to a generic grazer). Matches the longest keyword first so a
## specific species wins over a shorter substring (e.g. "reindeer" is not
## mistaken for "deer") regardless of HERD_SPECIES declaration order.
static func for_herd(label: String) -> String:
	var lower := label.to_lower()
	var keywords := HERD_SPECIES.keys()
	keywords.sort_custom(func(a, b): return String(a).length() > String(b).length())
	for keyword in keywords:
		if lower.find(keyword) != -1:
			return String(HERD_SPECIES[keyword])
	return HERD_DEFAULT
