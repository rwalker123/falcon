class_name HudPalette
extends RefCounted

## THE THEME ROSTER — the four palettes the HUD can wear, and the one entry point that installs one.
##
## **RESTART TO APPLY, BY DESIGN.** A theme is chosen in the Options pane, persisted to
## `ClientSettings` (`[ui] theme`) and installed at the NEXT launch — never live. That is what makes
## the whole system free of a rebuild pass: `ClientSettings` is an autoload, so its `_ready` runs
## before the main scene is instantiated and therefore before a single Control has been built. Every
## panel then reads the theme's colours the same way it always did, on its first and only build.
##
## **WHY `HudStyle`'s PALETTE IS `static var` AND NOT `const`.** A `static var` reads IDENTICALLY at
## the call site — `HudStyle.DANGER` is the same expression either way — so swapping the storage class
## left all ~710 call sites untouched. The one thing it forbids is `const X := HudStyle.DANGER` in
## another script: a static variable is not a constant expression and that is a PARSE error, not a
## silent wrong colour. Those sites are now either direct references at their use site or `static var`s
## re-derived in an `apply_palette()` hook (below).
##
## **THE DERIVE-INSIDE-APPLY RULE.** Anything computed FROM a themed colour — `PANEL` from
## `PANEL_SOLID`, the `*_HEX` strings, `MapView.OVERLAY_COLORS`, the vocabulary modules' style tables —
## must be assigned INSIDE an `apply_palette()`, never as a static-var initializer. An initializer runs
## when its script is LOADED, which for anything reachable from an autoload's preload graph is before
## `apply()` has ever run; it would capture the DEFAULT palette's value and then never update, and the
## failure is silent (a correctly-themed panel with a few stubbornly cyan accents in it).
##
## Ordering inside `apply()` follows from that rule: `HudStyle` first (it owns the source colours),
## then `MapView` and the vocabulary modules, which derive from what `HudStyle` now holds.

## The palette a fresh install wears, and the one every preview harness pins itself to.
const DEFAULT_THEME := "ember"

## The three earth themes share ONE map ramp. The map's data ramps answer "how much of X is here?",
## which is a question about legibility, not about the HUD's warmth — so a second and third
## near-identical earth ramp would be three places to fix one contrast problem. `console` keeps its
## own (it is the original, preserved verbatim), and that is the only reason there are two.
const EARTH_MAP := {
	"SENTIMENT_COLOR": Color("c8674e"),          # oxide
	"CORRUPTION_COLOR": Color("b58a3f"),         # brass
	# The one cool violet in an otherwise warm set, kept because the five civic channels have to be
	# separable from each other before they have to be in the family.
	"CULTURE_COLOR": Color("9d7fa8"),            # mallow
	"MILITARY_COLOR": Color("6b8b6a"),           # sage
	"CRISIS_COLOR": Color("a8404a"),             # madder
	"OVERLAY_FALLBACK_COLOR": Color("6d8ba8"),
	# HYPSOMETRIC, not a heatmap: the three elevation stops read as lowland green → tan → bone, the
	# convention every physical map uses, in place of the blue/yellow/red ramp that read as "hot".
	"ELEVATION_LOW_COLOR": Color("4a5f43"),
	"ELEVATION_MID_COLOR": Color("c2a56f"),
	"ELEVATION_HIGH_COLOR": Color("e6ded0"),
	"PASTURE_POOR_COLOR": Color("c2ab7a"),       # dry straw
	"PASTURE_RICH_COLOR": Color("4f7038"),       # moss
	"PASTURE_DEAD_COLOR": Color("3a352e"),
	"PASTURE_WATER_COLOR": Color("16242b"),      # matches the deep_ocean terrain art
	"FORAGE_POOR_COLOR": Color("d0bd8a"),        # wheat
	"FORAGE_RICH_COLOR": Color("5c7f3c"),        # leaf
	"FORAGE_BARREN_COLOR": Color("26241f"),
}

## Every theme: `name` is what the Options row says, `hud` is the 26 authored `HudStyle` colours and
## `map` the 16 authored `MapView` data-ramp colours. Everything else in either script is DERIVED —
## see the rule at the top of this file.
##
## `console` is the original palette, and its values are the ORIGINAL `Color(...)` literals rather
## than hex re-spellings of them: round-tripping a float triple through 8-bit hex is a drift, and the
## one theme whose job is to be unchanged must be exactly unchanged.
const THEMES := {
	"loam": {
		"name": "Loam",
		"hud": {
			"GROUND": Color("17130f"),
			"GROUND_2": Color("1e1813"),
			"PANEL_SOLID": Color("241d17"),
			"LINE": Color("443528"),
			"LINE_SOFT": Color("2e251c"),
			"INK": Color("f2e9da"),
			"INK_DIM": Color("b7a893"),
			"INK_FAINT": Color("8a7c6a"),
			"SIGNAL": Color("79b0d6"),
			"SIGNAL_DEEP": Color("40708f"),
			"WARN": Color("d9a441"),
			"DANGER": Color("c8664e"),
			"HEALTHY": Color("8fa363"),
			"THREAT_ACCENT": Color("b03426"),
			"HUNT_DANGER_ACCENT": Color("d2842f"),
			"BUTTON_PRIMARY_BG": Color("2b3f4e"),
			"BUTTON_PRIMARY_TEXT": Color("dceaf4"),
			"BUTTON_ARMED_TEXT": Color("f0cfc3"),
			"GHOST_BG": Color("1f1a15"),
			"GHOST_BG_HOVER": Color("2c2620"),
			"PRIMARY_BG_HOVER": Color("365064"),
			"ARMED_BG": Color("2a1a15"),
			"ARMED_BG_HOVER": Color("331f19"),
			"ARMED_BORDER": Color("5a3a30"),
			"VOICE_PIGMENT": Color("c89c66"),
			"VOICE_INK": Color("8d9aa4"),
		},
		"map": EARTH_MAP,
	},
	"ember": {
		"name": "Ember",
		"hud": {
			"GROUND": Color("14100d"),
			"GROUND_2": Color("1b1611"),
			"PANEL_SOLID": Color("221b15"),
			"LINE": Color("46372a"),
			"LINE_SOFT": Color("2f261d"),
			"INK": Color("f4ead7"),
			"INK_DIM": Color("bdab90"),
			"INK_FAINT": Color("8d7e68"),
			"SIGNAL": Color("efe3cd"),
			"SIGNAL_DEEP": Color("a08d6d"),
			"WARN": Color("e0a63c"),
			"DANGER": Color("c05a41"),
			"HEALTHY": Color("94a361"),
			"THREAT_ACCENT": Color("a83125"),
			"HUNT_DANGER_ACCENT": Color("d98a2b"),
			"BUTTON_PRIMARY_BG": Color("3d3226"),
			"BUTTON_PRIMARY_TEXT": Color("fbf3e2"),
			"BUTTON_ARMED_TEXT": Color("f2cbbc"),
			"GHOST_BG": Color("1d1812"),
			"GHOST_BG_HOVER": Color("2b2419"),
			"PRIMARY_BG_HOVER": Color("4c4030"),
			"ARMED_BG": Color("2a1a14"),
			"ARMED_BG_HOVER": Color("332018"),
			"ARMED_BORDER": Color("5c3a2e"),
			"VOICE_PIGMENT": Color("d0a468"),
			"VOICE_INK": Color("93a0a8"),
		},
		"map": EARTH_MAP,
	},
	"kiln": {
		"name": "Kiln",
		"hud": {
			"GROUND": Color("16161a"),
			"GROUND_2": Color("1c1c20"),
			"PANEL_SOLID": Color("232328"),
			"LINE": Color("40403f"),
			"LINE_SOFT": Color("2b2b2f"),
			"INK": Color("ece6dc"),
			"INK_DIM": Color("ada79c"),
			"INK_FAINT": Color("7e786e"),
			"SIGNAL": Color("cf7a4d"),
			"SIGNAL_DEEP": Color("8c4b2b"),
			"WARN": Color("e3c26a"),
			"DANGER": Color("a8453f"),
			"HEALTHY": Color("7f9455"),
			"THREAT_ACCENT": Color("973026"),
			"HUNT_DANGER_ACCENT": Color("e0a53a"),
			"BUTTON_PRIMARY_BG": Color("47301f"),
			"BUTTON_PRIMARY_TEXT": Color("f7e2d4"),
			"BUTTON_ARMED_TEXT": Color("f0cabb"),
			"GHOST_BG": Color("1e1e22"),
			"GHOST_BG_HOVER": Color("2b2b30"),
			"PRIMARY_BG_HOVER": Color("573c27"),
			"ARMED_BG": Color("2a1c1a"),
			"ARMED_BG_HOVER": Color("33221f"),
			"ARMED_BORDER": Color("5a3a34"),
			"VOICE_PIGMENT": Color("c99a6b"),
			"VOICE_INK": Color("8f9aa6"),
		},
		"map": EARTH_MAP,
	},
	"console": {
		"name": "Console (original)",
		"hud": {
			"GROUND": Color(0.043, 0.067, 0.078, 1.0),
			"GROUND_2": Color(0.055, 0.086, 0.102, 1.0),
			"PANEL_SOLID": Color(0.067, 0.102, 0.118, 1.0),
			"LINE": Color(0.149, 0.212, 0.235, 1.0),
			"LINE_SOFT": Color(0.106, 0.157, 0.176, 1.0),
			"INK": Color(0.914, 0.937, 0.914, 1.0),
			"INK_DIM": Color(0.616, 0.690, 0.678, 1.0),
			"INK_FAINT": Color(0.435, 0.514, 0.502, 1.0),
			"SIGNAL": Color(0.310, 0.878, 0.812, 1.0),
			"SIGNAL_DEEP": Color(0.122, 0.612, 0.557, 1.0),
			"WARN": Color(0.949, 0.694, 0.247, 1.0),
			"DANGER": Color(0.910, 0.455, 0.416, 1.0),
			"HEALTHY": Color(0.463, 0.804, 0.502, 1.0),
			"THREAT_ACCENT": Color(0.85, 0.16, 0.16, 1.0),
			"HUNT_DANGER_ACCENT": Color(0.93, 0.52, 0.13, 1.0),
			"BUTTON_PRIMARY_BG": Color(0.086, 0.227, 0.204, 1.0),
			"BUTTON_PRIMARY_TEXT": Color(0.847, 1.0, 0.973, 1.0),
			"BUTTON_ARMED_TEXT": Color(0.941, 0.765, 0.741, 1.0),
			"GHOST_BG": Color(0.075, 0.129, 0.122, 1.0),
			"GHOST_BG_HOVER": Color(0.090, 0.188, 0.161, 1.0),
			"PRIMARY_BG_HOVER": Color(0.110, 0.275, 0.251, 1.0),
			"ARMED_BG": Color(0.165, 0.110, 0.102, 1.0),
			"ARMED_BG_HOVER": Color(0.200, 0.122, 0.114, 1.0),
			"ARMED_BORDER": Color(0.353, 0.227, 0.212, 1.0),
			"VOICE_PIGMENT": Color(0.784, 0.612, 0.400, 1.0),
			"VOICE_INK": Color(0.510, 0.635, 0.706, 1.0),
		},
		"map": {
			"SENTIMENT_COLOR": Color(1.0, 0.35, 0.25, 1.0),
			"CORRUPTION_COLOR": Color(0.92, 0.58, 0.18, 1.0),
			"CULTURE_COLOR": Color(0.72, 0.36, 0.88, 1.0),
			"MILITARY_COLOR": Color(0.36, 0.7, 0.43, 1.0),
			"CRISIS_COLOR": Color(0.92, 0.24, 0.46, 1.0),
			"OVERLAY_FALLBACK_COLOR": Color(0.15, 0.45, 1.0, 1.0),
			"ELEVATION_LOW_COLOR": Color(0.16, 0.32, 0.78, 1.0),
			"ELEVATION_MID_COLOR": Color(0.97, 0.82, 0.32, 1.0),
			"ELEVATION_HIGH_COLOR": Color(0.78, 0.14, 0.18, 1.0),
			"PASTURE_POOR_COLOR": Color(0.85, 0.78, 0.42, 1.0),
			"PASTURE_RICH_COLOR": Color(0.13, 0.62, 0.24, 1.0),
			"PASTURE_DEAD_COLOR": Color(0.34, 0.30, 0.38, 1.0),
			"PASTURE_WATER_COLOR": Color(0.10, 0.16, 0.28, 1.0),
			"FORAGE_POOR_COLOR": Color(0.88, 0.80, 0.44, 1.0),
			"FORAGE_RICH_COLOR": Color(0.18, 0.72, 0.38, 1.0),
			"FORAGE_BARREN_COLOR": Color(0.20, 0.21, 0.24, 1.0),
		},
	},
}

## The theme actually installed THIS session — `""` until `apply()` has run. The Options row compares
## the player's SELECTION against it to decide whether a restart is still owed, which is a different
## question from "what is saved" and cannot be answered by `ClientSettings.theme` alone.
static var applied_id: String = ""


## The roster, in the order the Options row lists it.
static func ids() -> PackedStringArray:
	var out := PackedStringArray()
	for id in THEMES.keys():
		out.append(String(id))
	return out


## The display name for a theme id — the id itself for one no longer in the roster, so a stale
## setting reads as *something* rather than as an empty caption.
static func display_name(id: String) -> String:
	if not THEMES.has(id):
		return id
	return String(THEMES[id]["name"])


## Install a theme. An unknown id DEGRADES to the default with a warning rather than crashing (the
## same posture as `ServerPortsFile`): a hand-edited or downlevel settings file must not stop the
## client from starting.
##
## Call order is the derive-inside-apply rule made concrete — `HudStyle` owns the source colours, and
## everything after it reads what `HudStyle` now holds.
static func apply(id: String) -> void:
	var theme_id := id
	if not THEMES.has(theme_id):
		push_warning("HudPalette: unknown theme '%s'; falling back to '%s'" % [id, DEFAULT_THEME])
		theme_id = DEFAULT_THEME
	var theme: Dictionary = THEMES[theme_id]
	HudStyle.apply_palette(theme["hud"])
	MapView.apply_palette(theme["map"])
	# The palette's DEPENDENTS: modules holding style tables built out of `HudStyle` colours. Each was
	# a `const` table until the palette became swappable; they take no palette of their own and only
	# re-read what `HudStyle` now holds.
	HudEventVocab.apply_palette()
	HudCraftingVocab.apply_palette()
	HudWidgets.apply_palette()
	TellingPanel.apply_palette()
	# THE WINDOW'S OWN BACKGROUND, and the third kind of baked colour this system has to reach.
	# `project.godot` sets `rendering/environment/defaults/default_clear_color` to the console
	# `GROUND` literal, and a project setting is read once at startup — so every pixel no Control
	# covers (the landing backdrop, a letterboxed preview window) stayed slate-blue under a warm
	# theme. Pushed here rather than left in the .godot file, which no palette can reach.
	RenderingServer.set_default_clear_color(HudStyle.GROUND)
	applied_id = theme_id
