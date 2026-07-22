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
##
## The `riverine_delta` food module spans three terrains (Floodplain/AlluvialPlain/
## NavigableRiver → one module in `core_sim/src/food.rs`), so its site glyph is
## split by tile terrain in `for_site`: real open water (navigable_river) reads as
## 🐟 (fish), dry floodplain LAND (alluvial_plain/floodplain) reads as 🎋 (reeds).

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
	"gazelle": "🦌",
	"horse": "🐎",
	"boar": "🐗",
	"goat": "🐐",
	"ibex": "🐐",
	"sheep": "🐑",
	"seal": "🦭",
	"rabbit": "🐇",
	"fowl": "🐓",
	"grouse": "🐓",
	"hare": "🐇",
	"catfish": "🐟",
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
## The Corral rung's key, named because a THIRD consumer now reads its glyph off this table by key:
## the turn orb's `starving_pen` attention row (an unfed pen is a corral problem, so it wears the
## corral glyph). The picker/map look policies up by their snapshot string; the orb has no policy in
## hand, so it needs the constant.
const POLICY_CORRAL := "corral"
# The four INVESTMENT rungs, one per rung-transition of the two ladders
# (docs/plan_intensification_ladder.md §2a):
#   plants:  wild --cultivate--> Tended Patch --sow--> Field
#   animals: wild --tame------> Pastoral herd --corral--> Pen
# Each verb wears the glyph of THE RUNG IT BUILDS (🌱 the crop Cultivate starts, 🐄 the livestock
# Corral pens), so `tame` and `sow` follow the same rule: ▦ = the plotted Field that Sow places (and
# it reads as laid-out ground — a *different thing* from the 🌾 Tended Patch badge, which is the
# point of rung 3); ◎ = the pastoral herd that now keeps to your camp, the rung's defining effect
# (proximity: far → near → fixed, §3).
#
# BOTH ARE TEXT-PRESENTATION SYMBOLS, DELIBERATELY — and that is a sharper rule than "bold line art".
# ♻ ⬆ ⇄ render BOLD because they inherit the label's font colour; an EMOJI carries its own colours
# and cannot be tinted, so it renders at whatever contrast its art happens to have. 🐾 was tried for
# `tame` and REJECTED on exactly that: at picker size it came out a faint washed-out tan against the
# dark console — the weakest glyph in the row, next to a crisp white 💀 (see the first cut of
# `two_meter_split.png`). This is the 🪙/💰 hazard's real mechanism, and it is why a *disabled* rung
# also matters: a text glyph greys out with its button, an emoji stays stubbornly coloured.
const POLICY_TAME := "tame"
const POLICY_SOW := "sow"
const POLICY_ICONS := {
	"sustain": "♻",
	"surplus": "⬆",
	"market": "⇄",
	"eradicate": "💀",
	"cultivate": "🌱",
	POLICY_SOW: "▦",
	POLICY_TAME: "◎",
	POLICY_CORRAL: "🐄",
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
# The one expedition phase named here: `awaiting` is a DEMAND ON THE PLAYER, so besides the row it
# also drives the turn-orb attention producer (Hud `ATTENTION_KIND_AWAITING_ORDERS`) and the orb's
# kind→icon map reads its glyph from here — one glyph, panel row and orb row alike.
const STATUS_AWAITING := "awaiting"
const STATUS_ICONS := {
	STATUS_PENDING: "○",
	STATUS_WORKING: "●",
	"outbound": "➤",
	STATUS_AWAITING: "▮▮",
	"hunting": "●",
	"delivering": "◄",
	"returning": "◄",
}

## Icon for an action status / expedition phase ("" for an unknown/absent key, so callers render
## bare text).
static func for_status(status: String) -> String:
	return String(STATUS_ICONS.get(status.strip_edges().to_lower(), ""))

const RIVERINE_DELTA_MODULE := "riverine_delta"
const RIVERINE_REED_ICON := "🎋"
# riverine_delta terrains that are dry floodplain LAND (reed/cattail beds + tubers), not open river
# water — they read as reeds, not fish. NavigableRiver is real open water you fish → keeps 🐟.
# Terrain config `name`s (terrain_config.json): floodplain=9, alluvial_plain=10, navigable_river=37.
const RIVERINE_REED_TERRAINS := ["alluvial_plain", "floodplain"]

const ICONS := {
	"coastal_littoral": "🐚",
	# riverine_delta default (legend/for_module path); for_site splits this fish↔reeds (🎋) by terrain.
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

# Site ART KEYS that are not themselves food-module keys. Deliberately disjoint from `ICONS`'
# keys so `site_key_for`'s return value is unambiguous: no food module is named "hunt", "reeds"
# or "default", and none may be added with those names.
const SITE_KEY_HUNT := "hunt"
const SITE_KEY_REEDS := "reeds"
const SITE_KEY_DEFAULT := "default"

## The stable ART KEY for a food site. This is the ONE site-resolution implementation: `for_site`
## maps the key to an emoji and `SiteSprites.for_site` maps the same key to a bundled PNG, exactly
## as `species_key_for` is shared by the herd pair. A hunted site is `hunt`; a riverine_delta site
## on dry floodplain LAND is `reeds`; any known food module is its own key (riverine_delta with an
## unknown terrain therefore lands here, keyed as itself — the open-river 🐟 reading, not `reeds`);
## only an unknown module is `default`.
static func site_key_for(module_key: String, is_hunt: bool, terrain_id: int = -1) -> String:
	if is_hunt:
		return SITE_KEY_HUNT
	var key := module_key.strip_edges()
	if key == RIVERINE_DELTA_MODULE and terrain_id >= 0:
		if RIVERINE_REED_TERRAINS.has(_terrain_name(terrain_id)):
			return SITE_KEY_REEDS
	if ICONS.has(key):
		return key
	return SITE_KEY_DEFAULT

## Icon for a food site, using the game-trail (hunt) icon when the site is hunted
## rather than gathered. `terrain_id` (>=0) splits the riverine_delta module's glyph
## by tile terrain — dry floodplain LAND reads as reeds (🎋), open river keeps 🐟;
## terrain_id < 0 (unknown) or any other module falls through to the plain module glyph.
static func for_site(module_key: String, is_hunt: bool, terrain_id: int = -1) -> String:
	var key := site_key_for(module_key, is_hunt, terrain_id)
	if key == SITE_KEY_HUNT:
		return HUNT
	if key == SITE_KEY_REEDS:
		return RIVERINE_REED_ICON
	# SITE_KEY_DEFAULT is not an ICONS key, so it lands on DEFAULT — the same sprig `for_module`
	# would have returned for an unknown module.
	return String(ICONS.get(key, DEFAULT))

# Terrain id → config `name`, memoized. Uses `get_names_dict()` (not the `get_name(id)` static): a
# script resource's built-in 0-arg `get_name` shadows that static when called via the global class, so
# the whole codebase reads terrain names through the dict. `for_site` runs per-marker per-frame, so the
# dict is fetched once and cached (terrain config is fixed at runtime).
static var _terrain_names: Dictionary = {}

static func _terrain_name(terrain_id: int) -> String:
	if _terrain_names.is_empty():
		_terrain_names = TerrainDefinitions.get_names_dict()
	return String(_terrain_names.get(terrain_id, ""))

# Species keywords sorted longest-first, built once on first use. `for_herd`
# runs per herd from the map draw loop, so this avoids re-sorting every frame.
static var _herd_keywords_by_length: Array = []

## The HERD_SPECIES key matched by a species keyword in a herd label, or "" when the label
## names no species we know. Matches the longest keyword first so a specific species wins
## over a shorter substring (e.g. "reindeer" is not mistaken for "deer") regardless of
## HERD_SPECIES declaration order.
##
## This is the ONE species-matching implementation: `for_herd` maps the key to an emoji, and
## `FaunaSprites.for_herd` maps the same key to a bundled PNG. Adding a species keyword here
## therefore serves both renderers.
static func species_key_for(label: String) -> String:
	if _herd_keywords_by_length.is_empty():
		_herd_keywords_by_length = HERD_SPECIES.keys()
		_herd_keywords_by_length.sort_custom(func(a, b): return String(a).length() > String(b).length())
	var lower := label.to_lower()
	for keyword in _herd_keywords_by_length:
		if lower.find(keyword) != -1:
			return String(keyword)
	return ""

## Icon for a migratory herd, inferred from a species keyword in its label
## (falls back to a generic grazer).
static func for_herd(label: String) -> String:
	var key := species_key_for(label)
	if key == "":
		return HERD_DEFAULT
	return String(HERD_SPECIES[key])
