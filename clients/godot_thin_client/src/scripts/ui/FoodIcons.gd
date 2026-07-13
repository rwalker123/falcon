extends RefCounted
class_name FoodIcons

## Single source of truth for map-marker icons — food sources and fauna herds —
## shared by the selection panel's Harvest/Hunt button and the map's markers so a
## given module/species always reads the same. Food icons map to the ecosystem
## food modules in `core_sim/CLAUDE.md` (Coastal Littoral = shellfish, Savanna =
## grassland herds, Temperate Forest = nut groves, etc.); herd icons are picked by
## a species keyword in the herd label — the snapshot's `species` display name is
## embedded in the label (e.g. "Red Deer (game_deer_03)"), so wild game reads with
## the right animal glyph.

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
	"rabbit": "🐇",
	"fowl": "🐓",
}

# Take-policy glyphs (the extractive `LABOR_HUNT_POLICIES` set shared by forage + hunt, plus the two
# INVESTMENT rungs — Cultivate is forage-only, Corral is hunt-only). ONE source of
# truth, read by BOTH consumers: the Hud policy-picker buttons (`_build_policy_picker`) and the
# map's worked-source yield labels (`MapView._draw_yield_label`), so a policy always reads the
# same on the panel and on the map. Sustain = take only the regrowth; Surplus = take more now,
# accept a slow decline; Market = harvest for trade goods; Eradicate = strip it bare. Cultivate =
# prepare the patch into a tended one (low yield while working, then a much higher tended yield);
# Corral = build a pen for a domesticated herd (the same deal, animal side). The 🌱 seedling / 🐄 cow
# read at picker size (🐄 is already the drawer's Domesticated/Corralled badge).
# Market is ⇄ (exchange), NOT 🪙 (coin) / 💰 (money bag) / ⚖ (scales): the two pictographic emoji
# both render as a featureless grey ball at the sizes these glyphs are drawn (a ~13px HUD button, a
# ~12px map yield label), and the scales render tiny and faint — the known glyph-legibility hazard.
# What survives the downscale is bold line art (♻ ⬆ ⇄) plus the high-contrast 💀. All verified in
# the preview frames (band_panel_left / map_band_work).
const POLICY_ICONS := {
	"sustain": "♻",
	"surplus": "⬆",
	"market": "⇄",
	"eradicate": "💀",
	"cultivate": "🌱",
	"corral": "🐄",
}

## Icon for a take policy ("" for an unknown/absent policy, so callers render bare text).
static func for_policy(policy: String) -> String:
	return String(POLICY_ICONS.get(policy.strip_edges().to_lower(), ""))

# Action-STATUS glyphs, read by the Band panel's Current-actions + Active-expeditions rows (Hud) so
# a row states what it is doing with a glyph instead of a word (the words move into the row
# tooltip). TWO ORTHOGONAL LAYERS ride the same vocabulary and must stay separate:
#   • STATUS — what the action IS DOING. A confirmed local forage/hunt row has no sim phase: it is
#     simply `working`. An expedition's status is the sim's `ExpeditionPhase` (`outbound` /
#     `awaiting` / `hunting` / `delivering` / `returning`) — the same keys the wire sends, so
#     `for_status` maps a phase string straight through.
#   • `pending` — a state of the ORDER, not of the action: composed locally, not yet acknowledged by
#     the sim, resolves on turn advance. It rides on ANY row and is a MODIFIER, never a phase member.
# `hunting` deliberately shares `working`'s glyph — a hunt party in its hunting phase IS just working
# — and `delivering` shares `returning`'s: both are "coming home", and the tooltip is what
# distinguishes them.
# Legibility (the 🪙/💰 lesson): these are drawn at HUD label size (~13px), where pictographic emoji
# collapse into a grey blob. Only BOLD LINE ART survives, so every glyph here is a geometric shape
# (◌ ● ➤ ▮▮ ◄) — verified at true size in `band_panel_status_glyphs.png`. ⏸ (U+23F8) was rejected for
# `awaiting`: it carries emoji presentation and renders as tofu/a blob in the HUD font.
const STATUS_PENDING := "pending"
const STATUS_WORKING := "working"
const STATUS_ICONS := {
	STATUS_PENDING: "○",
	STATUS_WORKING: "●",
	"outbound": "➤",
	"awaiting": "▮▮",
	"hunting": "●",
	"delivering": "◄",
	"returning": "◄",
}

## Icon for an action status / expedition phase ("" for an unknown/absent key, so callers render
## bare text).
static func for_status(status: String) -> String:
	return String(STATUS_ICONS.get(status.strip_edges().to_lower(), ""))

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

# Species keywords sorted longest-first, built once on first use. `for_herd`
# runs per herd from the map draw loop, so this avoids re-sorting every frame.
static var _herd_keywords_by_length: Array = []

## Icon for a migratory herd, inferred from a species keyword in its label
## (falls back to a generic grazer). Matches the longest keyword first so a
## specific species wins over a shorter substring (e.g. "reindeer" is not
## mistaken for "deer") regardless of HERD_SPECIES declaration order.
static func for_herd(label: String) -> String:
	if _herd_keywords_by_length.is_empty():
		_herd_keywords_by_length = HERD_SPECIES.keys()
		_herd_keywords_by_length.sort_custom(func(a, b): return String(a).length() > String(b).length())
	var lower := label.to_lower()
	for keyword in _herd_keywords_by_length:
		if lower.find(keyword) != -1:
			return String(HERD_SPECIES[keyword])
	return HERD_DEFAULT
