extends RefCounted
class_name WorkbenchPages

## THE PAGE REGISTRY — the one place the Workbench's contents are declared.
##
## **Adding a page is one row here plus one file under `pages/`.** There is deliberately no shell
## edit in that list, and that is the property this file exists to guarantee: `WorkbenchShell.gd`
## reads this array, instantiates `script`, and hands the page its identity. It never `preload`s a
## page, never names one, and therefore never grows when the surface does.
##
## A row with an empty `script` is a DECLARED but unbuilt page: the rail shows it, the shell renders
## the placeholder, and the click is honest about there being nothing there yet. That is how the
## intended shape of the replacement surface stays visible while it is built page by page.
##
## **A NEW `glyph` MUST BE RENDERED BEFORE IT IS TRUSTED.** The bundled font does not cover every
## symbol, and an uncovered one does not fail — it draws as a stub a couple of pixels tall, which is
## unreadable precisely where the glyph is all there is: the COLLAPSED rail, where it is the only
## thing identifying the entry. `≡` (U+2261) and `⌁` (U+2301) both shipped that way and were caught
## in `tools/workbench_preview`'s collapsed-rail frame; check a new one there the same way.

const PAGES: Array[Dictionary] = [
	{
		"id": &"config_tuning",
		"title": WorkbenchVocab.TUNING_PAGE_TITLE,
		"subtitle": WorkbenchVocab.TUNING_PAGE_SUBTITLE,
		"section": WorkbenchVocab.SECTION_SIM,
		"glyph": "⚙",
		"script": "res://src/scripts/ui/workbench/pages/ConfigTuningPage.gd",
	},
	{
		"id": &"equipment",
		"title": WorkbenchVocab.EQUIPMENT_PAGE_TITLE,
		"subtitle": WorkbenchVocab.EQUIPMENT_PAGE_SUBTITLE,
		"section": WorkbenchVocab.SECTION_SIM,
		# ▣ (U+25A3) — same Geometric Shapes block as `◈` / `▲` / `◔`, which the bundled font covers.
		# Rendered in `workbench_preview`'s collapsed-rail frame before it was trusted, per the rule
		# above: an uncovered symbol draws as a two-pixel stub with no error at all.
		"glyph": "▣",
		"script": "res://src/scripts/ui/workbench/pages/EquipmentPage.gd",
	},
	{
		"id": &"kits",
		"title": WorkbenchVocab.KITS_PAGE_TITLE,
		"subtitle": WorkbenchVocab.KITS_PAGE_SUBTITLE,
		"section": WorkbenchVocab.SECTION_SIM,
		# ◧ (U+25E7) — the same Geometric Shapes square family as Equipment's `▣`, which is the point:
		# the two config pages read as siblings in the rail. **Chosen off the RENDER, not the chart.**
		# `▤` (U+25A4) was the obvious pick and is a whole covered glyph — it draws its rules perfectly
		# at 30px — yet in `workbench_preview`'s collapsed-rail frame, at `FONT_SIZE_GLYPH` in `INK_DIM`
		# under the project's fractional canvas scale, they smear into a solid block indistinguishable
		# from tofu. So the rule above ("render it before you trust it") bites on legibility as well as
		# on coverage, and the lever is a glyph with no hairline strokes.
		"glyph": "◧",
		"script": "res://src/scripts/ui/workbench/pages/KitsPage.gd",
	},
	{
		"id": &"turn_control",
		"title": "Turn Control",
		"subtitle": "Step, autoplay, rollback",
		"section": WorkbenchVocab.SECTION_SIM,
		"glyph": "▶",
		"script": "",
	},
	{
		"id": &"world",
		"title": "World",
		"subtitle": "Presets, size, seed, start profile",
		"section": WorkbenchVocab.SECTION_WORLD,
		"glyph": "◈",
		"script": "",
	},
	{
		"id": &"terrain",
		"title": "Terrain",
		"subtitle": "Biome histogram and tile drill-down",
		"section": WorkbenchVocab.SECTION_WORLD,
		"glyph": "▲",
		"script": "",
	},
	{
		"id": &"fauna",
		"title": "Fauna",
		"subtitle": "Herd registry and density telemetry",
		"section": WorkbenchVocab.SECTION_WORLD,
		"glyph": "❋",
		"script": "",
	},
	{
		"id": &"logs",
		"title": "Logs",
		"subtitle": "Streaming tracing feed",
		"section": WorkbenchVocab.SECTION_DIAGNOSTICS,
		"glyph": "☰",
		"script": "",
	},
	{
		"id": &"profiling",
		"title": "Profiling",
		"subtitle": "Per-turn cost breakdown",
		"section": WorkbenchVocab.SECTION_DIAGNOSTICS,
		"glyph": "◔",
		"script": "",
	},
]


## The registry row for `id`, or an empty dictionary when nothing is registered under it.
static func find(id: StringName) -> Dictionary:
	for page in PAGES:
		if page.get("id", &"") == id:
			return page
	return {}


## The id of the page the surface opens on — the first row, so reordering the registry moves the
## landing page with it rather than needing a second declaration.
static func default_id() -> StringName:
	return PAGES[0].get("id", &"") if not PAGES.is_empty() else &""


## Section headings in registry order, deduped — the rail's grouping, derived rather than declared
## twice.
static func sections() -> Array[String]:
	var out: Array[String] = []
	for page in PAGES:
		var section: String = page.get("section", "")
		if not section.is_empty() and not out.has(section):
			out.append(section)
	return out


## The rows filed under one section, in registry order.
static func pages_in(section: String) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for page in PAGES:
		if page.get("section", "") == section:
			out.append(page)
	return out
