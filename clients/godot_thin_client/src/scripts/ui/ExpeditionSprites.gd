extends RefCounted
class_name ExpeditionSprites

## Bundled PNG art for EXPEDITION MAP MARKERS — the sprite half of the mission glyphs a detached
## party's marker wears on the map (`MapView.EXPEDITION_GLYPH` ⚑ for a scouting party,
## `EXPEDITION_DENY_GLYPH` 💀, `EXPEDITION_TRADE_GLYPH` 📦), drawn by
## `BandMarkerRenderer._draw_expedition_body` centred in the marker's dark disc and faction ring.
##
## **THE MAP-MARKER HOUSE STYLE, NOT `hud/`'s.** These draw over the MAP, at marker size, over whatever
## terrain is under the half-transparent disc — the fauna situation — so they keep the thick dark
## charcoal outline (`assets/icons/icon_prompts.txt` → EXPEDITION MARKERS). The SUBJECT is the band
## verb's `hud/` mark redrawn in that style — the footprints, the animal skull, the cloth bundle — so a
## party's Scout / Deny / Trade button and its map marker are recognisably one thing.
##
## **THE KEY IS THE MISSION ID** (`HudExpeditionVocab.EXPEDITION_MISSION_*`, the same strings as
## `MapView.EXPEDITION_*_MISSION`; read off the vocab leaf so this table never loads `MapView`).
## **Hunt has no art**: the expedition hunt is being retired, so a hunting party answers `null` here
## and keeps its 🏹 glyph through the renderer's fallback.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state.

## Mission id → bundled texture path.
const SPRITE_DIR := "res://assets/icons/expeditions/"
const SPRITE_PATHS := {
	HudExpeditionVocab.EXPEDITION_MISSION_SCOUT: SPRITE_DIR + "scout.png",
	HudExpeditionVocab.EXPEDITION_MISSION_DENY: SPRITE_DIR + "deny.png",
	HudExpeditionVocab.EXPEDITION_MISSION_TRADE: SPRITE_DIR + "trade.png",
}

## Bundled sprite for a party's mission, or `null` when the mission has none (a hunting party, an
## unknown mission) — the caller then draws the mission's text glyph, the contract every art family
## has with its emoji.
##
## Takes `IconSprites.texture_for`'s DEFAULT `warn: true`: this family's coverage is complete for what
## it declares — all three keys above have a committed, imported PNG behind them — so a failed load is
## a DEFECT and must surface.
static func for_mission(mission: String) -> Texture2D:
	if mission == "" or not SPRITE_PATHS.has(mission):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[mission]))
