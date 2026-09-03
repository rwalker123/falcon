---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{BandMarkerRenderer,SecondaryMarkerRenderer}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{IconSprites,FoodIcons,SiteSprites,StageSprites}.gd"
---

<!-- Extracted verbatim from lines 1553-1605 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Map markers (MapView hex-icon stack UX)

Co-located hex markers no longer overlap at the hex center. Markers split into two
classes by their source array (not a predicate): **PRIMARY** = player bands, drawn by
`MapView._draw_primary_bands` over the `units`/`populations` array; **SECONDARY** = herds /
food sites / wondrous sites, placed by `MapView._compute_secondary_slots`. (Tuning consts
are grouped near the top of `MapView.gd`, after the FoW/height consts.)

- **PRIMARY — player bands** own the **center spotlight** as an offset card-stack
  (`_draw_primary_bands`/`_draw_band_stack`/`_draw_band_token`). Each band's token is its
  **settlement stage**, which the sim resolves from `settlement_stage_config.json`: the **bundled
  sprite** for its `settlement_stage_id` where we have art (`StageSprites` — see its row above; the
  sprite is tried BEFORE the empty-glyph placeholder branch, which returns early), else the opaque
  `settlement_stage_icon` emoji (⛺ nomadic / 🛖 camp / 🏘️ village). Either way at
  `BAND_STAGE_GLYPH_SIZE_FACTOR` via the shared drop-shadow helpers (`_draw_marker_sprite` /
  `_draw_marker_glyph`), **no faction ring or disc**. Ownership is carried by a **faction-colored nameplate banner** (`_draw_band_banner`,
  `BAND_BANNER_*` consts) — a short rounded bar under the token filled with the band's faction
  color, drawn for the **active (primary) card only** and LOD-suppressed below
  `ICON_MIN_DETAIL_RADIUS`. The banner is intentionally sized as the substrate for an optional
  faction/band **name label** later (text on the bar). When `settlement_stage_icon` is empty
  (pre-stage / missing snapshot — rare) the token draws a small **neutral non-circular** fallback
  marker (gray square, `BAND_FALLBACK_MARKER_*`) instead of the glyph, never a disc. The stage
  label (`settlement_stage_label`) surfaces as the Occupants roster row's hover tooltip.
  Multiple bands on one hex fan up-right: up to `BAND_STACK_MAX_CARDS` (3) cards,
  back cards **darkened** (glyph multiplied by `BAND_STACK_BEHIND_TINT` so they recede/shadow),
  the **active** band (the one whose `entity == selected_unit_id`, else the first) drawn
  full-brightness on top. The active band reads by brightness alone — there is **no per-token
  selection ring** (the hex selection outline marks the tile); `BAND_STACK_BEHIND_TINT` is the
  single lever for the recede effect (RGB<1 darkens, alpha<1 fades — swap between the two there).
  Beyond 3, a `×N` count pill folded onto the **right end of the banner** (nameplate-with-count).
  Food-days dot + the travel arrow draw on the active card only.
- **SECONDARY — herds / food sites / wondrous sites** ring the hex in **fixed edge slots**
  (`SECONDARY_SLOT_OFFSETS`, near the hex corners), computed once per frame in
  `_compute_secondary_slots` by category priority **wonder → food → herd** (sequential fill,
  so icons never jump frame-to-frame). Cap `SECONDARY_VISIBLE_CAP` (3) visible icons; extras
  collapse into a `+N` overflow chip (`_draw_secondary_overflow`). Glyphs drop the old dark
  backing disc for a 1px drop shadow (`_draw_marker_glyph`). Herd migration arrow is thinner
  and only drawn on the hovered/selected herd tile. The `×N`/`+N` pills share `_draw_count_pill`.
- **Selected + hovered hex outline** (`_draw_tile_selection_highlight`, reusing `_outline_hex`):
  a solid white hex outline on `selected_tile`, a faint one on `_hovered_tile` (skipped when
  hover == selection) — this replaces the old selection-as-marker-ring feel.
- **Select-then-cycle** (`handle_hex_click` + `cycle_index`): re-clicking the current
  `selected_tile` with >1 band advances `cycle_index` (mod band count) so the stack surfaces the
  next band on top; a fresh tile resets to the top band. `select_occupant` (roster click) syncs
  `cycle_index` to the picked band's stack position via `_cycle_index_for_unit`.
- **Zoom LOD**: below `ICON_MIN_DETAIL_RADIUS` (far zoom, tiny hexes) secondary icons + all
  count/overflow chips are suppressed; only primary tokens draw.

Verify visual changes via `tools/map_preview.gd` (`scripts/preview.sh res://tools/map_preview.tscn`
→ `ui_preview_out/map_band_stack.png` / `map_mixed_hex.png` / `map_far_zoom.png` /
`map_stage_glyphs.png` (the ⛺→🛖→🏘️ progression + empty-stage neutral non-circular fallback marker) + the existing
labor-highlight states).


## The slot lookup is public, and the overflow chip reports what it hides

The worked-source marks (`.claude/rules/client/overlay-channels.md`) dock to a source's OWN marker
rather than its hex, so the slot system's answers are public: **`slot_of(key)`** (`0..cap-1`, or `-1`
for overflowed/LOD-suppressed), **`slot_center`**, **`overflow_at(tile)`**, and the two key builders
**`food_key` / `herd_key`** — so a mark and the marker it rides can never disagree about a source's
identity. `BandOverlayRenderer` reaches all of them through `MapView` pass-throughs
(`secondary_slot_of` / `secondary_slot_center` / `secondary_food_key` / `secondary_herd_key`), the
same convention as `_hex_center` / `_herd_by_id` — **no renderer holds another**.

**THE `+N` CHIP CARRIES WHAT IT HIDES.** Three visible slots is the right budget — six badges on a hex
is not a map — but a cap that drops state SILENTLY reads as "nothing here", which is exactly the
failure the worked-source marks exist to fix at a different scale. So the chip appends the hidden
sources' rolled-up state, severity-ordered and at most two marks wide: `⚠` trouble, `⌃` a rung on
offer, `⚒` merely worked (`_hidden_marks`, fed by `set_hidden_source_state` which `MapView._draw`
threads across from the mark pass). The marks are the badges' own vocabulary, so the chip needs no
legend.

**Reaching a hidden source is NOT the chip's job** — re-clicking the hex cycles the whole occupant
stack, land included (`map-renderers.md` → Select-then-cycle). The marks SIGNAL; the cycle REACHES.

**A ready source is deliberately NOT promoted into a visible slot.** Slot fill is sequential precisely
so icons never jump between frames; reordering on a state change would make a herd swap corners the
turn a knowledge track completes. Frame: `map_overflow_worked`.

## An expedition's disc wears its MISSION's mark, and there are four of them

`BandMarkerRenderer._draw_expedition_body`: ⚑ scout · 🏹 hunt · 💀 denial · **📦 trade** (arc #527).
One mission, one glyph, on all three surfaces it appears on — the map marker, the parties-strip row
(`HudFormat.PANEL_EXPEDITION_*_GLYPH`) and the footer button that launches it
(`HudComposeVocab.COMPOSE_MISSION_LABEL_*`) — so a party's mark means the same thing at every scale.

**The phase decorations stay gated on `is_hunt`, and the shipment is the second mission to want
that.** The green pip means *"carrying a haul HOME"*; a denial party's haul is a rounding error it
should not advertise, and a trade party's goods are going the OTHER way. Both therefore take the
glyph and none of the decorations.

## The LETHAL-GROUND mark — a ⚠ on a band standing where the sim is killing people (issue #614)

**It is a STATE, not an event**, and that is what makes it cheap to get right: true while the band is
on that hex, gone the turn it moves off. No edge to gate, and none of the *"has it camped or is it
passing through?"* judgement an event would have needed — the question that killed an earlier
event-shaped proposal for the same problem.

`TileSurvivability.is_lethal` is the test, off the temperature `MapView` already decoded — one
authority behind the tile chip's ⚠, the temperature overlay's hatch and this mark. A hex with no
reading draws nothing: unknown is not deadly.

**UP-LEFT, AND THE QUADRANT IS THE WHOLE PLACEMENT DECISION.** Three marks already hang off a band
token and each owns a direction — the food-runway dot up-RIGHT (`BAND_FOOD_DOT_OFFSET_FACTOR`), the
faction nameplate banner BELOW (`_draw_band_banner`), the over-cap count pill on the banner's right
end or bottom-right (`BAND_COUNT_BADGE_OFFSET`). Up-left is the one free corner. (The travel arrow
points wherever the destination is and can reserve nothing; it is a thin line and reads through a
glyph.)

### Two things the FIRST cut got wrong, and both needed a render to find

- **The offset must know the GLYPH's size, not just the token's.** It started as a fixed multiple of
  the token radius, which is fine at a huge zoom and puts the ⚠ straight on top of the tent the
  moment the glyph is drawn at a legible size. It is now pushed out along the diagonal by
  `token_radius + size × BAND_LETHAL_MARK_CLEARANCE_FACTOR`, so the glyph's BOX clears the token at
  every zoom.
- **The mark has its OWN detail gate, above `ICON_MIN_DETAIL_RADIUS`.** It first shared the banner's
  gate (16.0) with a 9 px floor under the glyph — and at that radius the ⚠ is not a warning triangle,
  it is a smudge indistinguishable from terrain. A dot or a nameplate survives being tiny; a
  pictogram does not. `BAND_LETHAL_MARK_MIN_RADIUS` (28.0) is where the glyph, which scales to
  `radius × 0.51`, reaches the ~14 px at which the triangle and its bar resolve. **There is no size
  FLOOR any more:** a floor draws an illegible mark rather than none, and has to fight the offset to
  stay off the token. Below the gate the TEMPERATURE OVERLAY's hatch is the map-scale answer to the
  same question.

> #### ⛔ THE HARNESS PROBE THAT PASSED WITH THE FEATURE DELETED
>
> The first `map_preview` assertion for this asserted the mark's ABSENCE at a far zoom **and passed
> with the LOD gate removed** — at that size the glyph is drawn and invisible either way. An absence
> is only worth asserting where a PRESENCE would have been visible, so the two LOD states now fit
> just either side of the gate (~29 and ~26.7) and the premise assertions state the measured radius.
>
> The colour probe was wrong too. `draw_string` antialiases, so a small ⚠ never reaches its own ink:
> measured, the closest pixel to `HudStyle.DANGER` inside the marked token's box was **0.192** away
> while bare khaki terrain was **0.235** — no threshold separates those. The discriminating property
> is REDNESS (`r − max(g, b)`), which measures ~100/255 on the glyph and ~18/255 on the terrain, so
> `_frame_marks_warning_near_hex` asks that instead and its margin sits clear of both.
