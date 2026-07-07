extends Node2D

## Renders every FoodIcons glyph via the exact draw path the map hex uses
## (draw_string + ThemeDB.fallback_font), so the harness can confirm none render
## as missing-glyph boxes before they ship on the map.

func _draw() -> void:
	var font: Font = ThemeDB.fallback_font
	if font == null:
		return
	var y := 50.0
	var rows: Array = []
	for key in FoodIcons.ICONS.keys():
		rows.append([String(FoodIcons.ICONS[key]), String(key)])
	rows.append([FoodIcons.HUNT, "hunt (game_trail)"])
	rows.append([FoodIcons.DEFAULT, "default fallback"])
	rows.append([FoodIcons.HERD_DEFAULT, "herd (generic)"])
	for key in FoodIcons.HERD_SPECIES.keys():
		rows.append([String(FoodIcons.HERD_SPECIES[key]), "herd: " + String(key)])
	for row in rows:
		var icon: String = row[0]
		var label: String = row[1]
		draw_circle(Vector2(60, y - 9), 20.0, Color(0.04, 0.06, 0.07, 0.6))
		var size := font.get_string_size(icon, HORIZONTAL_ALIGNMENT_LEFT, -1, 30)
		draw_string(font, Vector2(60 - size.x * 0.5, y + 30 * 0.34 - 9), icon, HORIZONTAL_ALIGNMENT_LEFT, -1, 30, Color(0.96, 0.97, 0.92))
		draw_string(font, Vector2(110, y), label, HORIZONTAL_ALIGNMENT_LEFT, -1, 19, Color(0.7, 0.8, 0.78))
		y += 46.0
