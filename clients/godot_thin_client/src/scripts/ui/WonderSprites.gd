extends RefCounted
class_name WonderSprites

## Bundled PNG art for map DISCOVERED-SITE (Wondrous Site) markers — the third art family behind
## `IconSprites`, after `FaunaSprites` (herds) and `SiteSprites` (food sites).
##
## WHY sprites at all (same reasoning as both siblings): the emoji path draws through
## `ThemeDB.fallback_font`, so the OS emoji font decides what a Great Peak or a Verdant Basin looks
## like, and ⛰/⛲ go blobby at marker size. A bundled illustration is ours and identical everywhere.
##
## KEYED ON `site_id`, NOT on the glyph. The site id is the sim's stable catalog key (from
## `core_sim/src/data/sites_config.json`), already on the wire and already read by
## `SecondaryMarkerRenderer._wonder_key`; the `glyph` string is presentation the server happens to
## also send, and two different sites may share one glyph (the fixture's `sky_arch` reuses ⛰), so
## keying on it would collapse distinct sites onto one sprite.
##
## THE `null` FALLBACK IS GENUINELY LIVE HERE — this is the difference from `FaunaSprites` /
## `SiteSprites`, whose coverage is complete and whose fallbacks only guard an unknown key. The two
## ids below are the WHOLE catalog today, but that catalog is **data-driven** (`sites_config.json`)
## and is expected to grow: a designer adds a site entry with a `glyph` and it ships with no art.
## So an unmapped `site_id` returning `null` — and `SecondaryMarkerRenderer.draw_discovered_site`
## falling through to the server-provided emoji — is a real, exercised path, not a safety net.
## Adding art is still just: drop the PNG in `assets/icons/wonders/`, add the id here.
##
## Static-only by design (same reasoning as `FaunaSprites`): a pure lookup with no node state,
## called from the map draw loop.

## Site catalog id (`DiscoveredSite.site_id`) → bundled texture path.
const SPRITE_DIR := "res://assets/icons/wonders/"
const SPRITE_PATHS := {
	"great_peak": SPRITE_DIR + "great_peak.png",
	"verdant_basin": SPRITE_DIR + "verdant_basin.png",
}

## Bundled sprite for a discovered site, or `null` when its `site_id` has no art (the caller then
## falls back to the site's server-provided emoji `glyph`). Load-and-cache lives in `IconSprites`,
## shared with `FaunaSprites` and `SiteSprites`.
static func for_site_id(site_id: String) -> Texture2D:
	if not SPRITE_PATHS.has(site_id):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[site_id]))
