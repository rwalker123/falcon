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
  `_draw_marker_glyph`), **no faction ring or disc**. Ownership is carried by a **nameplate under the token**, drawn for the
  **active (primary) card only**, which takes **one of two forms by zoom** — the **band NAME PILL**
  at/above `BAND_NAME_PILL_MIN_RADIUS` (24.0), the **faction-colored bar** between that and
  `ICON_MIN_DETAIL_RADIUS` (16.0), and nothing below (see "The nameplate is two shapes" below).
  When `settlement_stage_icon` is empty
  (pre-stage / missing snapshot — rare) the token draws a small **neutral non-circular** fallback
  marker (gray square, `BAND_FALLBACK_MARKER_*`) instead of the glyph, never a disc. The stage
  label (`settlement_stage_label`) surfaces as the Occupants roster row's hover tooltip.
  Multiple bands on one hex fan up-right: up to `BAND_STACK_MAX_CARDS` (3) cards,
  back cards **darkened** (glyph multiplied by `BAND_STACK_BEHIND_TINT` so they recede/shadow),
  the **active** band (the one whose `entity == selected_unit_id`, else the first) drawn
  full-brightness on top. The active band reads by brightness alone — there is **no per-token
  selection ring** (the hex selection outline marks the tile); `BAND_STACK_BEHIND_TINT` is the
  single lever for the recede effect (RGB<1 darkens, alpha<1 fades — swap between the two there).
  Beyond 3, a `×N` count pill folded onto the **right end of whichever nameplate is drawn**
  (nameplate-with-count) — one anchoring rule, fed the `Rect2` the bar or the pill returns.
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
`map_stage_glyphs.png` (the ⛺→🛖→🏘️ progression + empty-stage neutral non-circular fallback marker) /
`map_band_names.png` / `map_band_names_overlap.png` / `map_band_names_gate.png` /
`map_band_names_below_gate.png` + the existing labor-highlight states).


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

## The nameplate is TWO SHAPES, and the pill REPLACES the bar rather than writing on it

A band's map token names itself: `_draw_band_name_pill` puts the band's own name — "Ashfell",
"Shepherd's Fold", or "Ashfell (Scout)" for a party — on the same dark rounded plate as the
`×N`/`+N` badges, anchored where the faction bar sits and returning the same `Rect2` so the over-cap
chip's anchoring code never learns which shape it got. The name is read straight off the marker's
`id`, which `MapView`'s marker loop stamps from `HudFormat.band_name`, so the map, the Occupants
drawer and the turn orb's rows always say the same thing about the same band. **Nothing here derives
or invents a name**; a band without one gets no pill.

**THE PILL REPLACES THE BAR — it is not text drawn on it**, and that is the correction to the
original plan for this bar ("intentionally sized as the substrate for a name label later"). The bar
is sized off the TOKEN radius (`BAND_BANNER_WIDTH_FACTOR` 2.4 × a token that is itself 0.34 × the hex
radius), which at a hex radius of 40 is about **33×7 px** — a strip that holds no legible text at any
font size, and one that would have to grow with zoom to hold a fixed-size label anyway. So above
`BAND_NAME_PILL_MIN_RADIUS` (24.0) the pill draws INSTEAD of the bar, and below it the bar draws
unchanged; the two never appear together, because two nameplate shapes in one frame read as two
kinds of band.

**FIXED SCREEN SIZE, FOR FREE.** `MapView` zooms by recomputing hex geometry from `radius` rather
than by a canvas transform, so a constant `BAND_NAME_PILL_FONT_SIZE` already IS constant screen
pixels — the same property the `×N`/`+N` badges have always relied on. Nothing counter-scales. The
only thing zoom moves is the anchor, which follows the token radius so the pill stays off the glyph;
`BAND_NAME_PILL_GAP` on top of it is a fixed pixel count, because a gap that scaled with the token
would drift the label away from its glyph at high zoom.

**THE GATE IS ABOUT CLUTTER, NOT LEGIBILITY — the opposite of `BAND_LETHAL_MARK_MIN_RADIUS`.** The ⚠
is gated because a pictogram stops resolving as a triangle when it gets small. A fixed-size pill
never gets small: it is exactly as readable at radius 12 as at radius 80. What changes is how much
MAP a ~100 px label covers — at the gate radius a 15-character name already spans about 2.5 hexes,
and below it the map becomes a wall of labels. The bar is the small-footprint answer to the same
ownership question, which is why the gate hands over to it rather than to nothing.

### The overlap cull, and why the SELECTED band places first

Fixed-size labels do not shrink out of each other's way, so neighbouring pills collide well before
the gate stops them. `BandMarkerRenderer` therefore reserves label rects in a **pre-pass**
(`_reserve_name_pills`) before anything is drawn: a rect that intersects one already placed is
**skipped entirely** — no pill, and no fall back to the scaled bar. The token, its card stack, its ⚠
and its food dot all still draw, so the band is never hidden; only its name is.

- **The pre-pass exists because the two orders differ.** Labels are placed with the tile holding
  `selected_unit_id` FIRST — a cull must never eat the label of the band the player is working —
  while TOKENS keep snapshot order so no glyph changes what it stacks over. Resolving placement up
  front leaves the draw pass byte-for-byte what it was.
- **Everything after the selected tile keeps snapshot order**, for the reason the secondary slots
  fill sequentially: placement has to give the same answer frame to frame, or labels flicker on and
  off as the array shuffles.
- **Both halves of the state are rebuilt every pass.** A rect surviving a frame would cull a label
  that has nothing to collide with.
- **A plate's half-extent is ONE expression, `MapView.pill_half_extent`.** `_fill_pill` draws end-cap
  circles of radius `half_h` centred at `±half_w`, so a plate inks `half_h` FURTHER on each side than
  its body. A measurement that forgets the caps under-reserves ~10 px per side at
  `BAND_NAME_PILL_FONT_SIZE` 11 — enough for two labels the cull has just cleared to visibly overlap,
  which is the one thing the cull exists to prevent. `count_pill_reach` and `_name_plate_half` are
  both that single function now, rather than two formulas for one shape that have to agree.
- **The reservation is TWO rects, because the footprint and the chip anchor are different
  questions.** `_name_pill_rects` measures once and returns both. The **FOOTPRINT** is everything the
  label inks — the plate's caps included, plus BOTH halves of the over-cap `×N` chip — and it is what
  `_label_rects` holds and the cull intersects. The **ANCHOR** ends where the chip's CENTRE goes, and
  it is what the draw pass is handed, so `_draw_band_stack`'s one-line chip anchor never learns which
  nameplate shape it got. One rect could not be both: `count_pill_reach` is a HALF-width, so a rect
  that reserves the whole chip ends a chip-radius past where the chip should be centred.
- **The chip anchors past the plate's BODY edge, not its inked edge**, so the chip's round left cap
  nests into the plate's round right cap and `Thornhollow ×4` reads as one nameplate; anchoring past
  the ink stands it a full cap clear and it reads as a separate badge. On a bar carrying no text the
  anchor was harmless wherever it landed — on a pill it is what keeps the chip off the name's last
  letters. The bar needs no allowance at all, which is why all of this lives on the pill rather than
  in the anchoring code.

Foreign bands take the pill exactly as your own do — the fog rule already means a foreign band you
cannot see is not drawn at all, so it needs no rule of its own. **Expeditions get no pill**, the same
`is_expedition` guard that has always kept the bar off them: a party's faction reads off its hollow
flag-disc ring.

> #### ⛔ THE BAR IS INVISIBLE TO AN EXACT-COLOUR PROBE, AND THE TOKEN WILL ANSWER FOR IT
>
> `map_band_names_below_gate` asserts the pill's ABSENCE, which is only worth asserting because the
> BAR is visibly present in the same frame — the `map-preview` rule the ⚠'s LOD probe was rebuilt
> around. Two things make that presence hard to measure, and the first cut of this probe got the
> second one wrong.
>
> The bar cannot be found by matching its faction colour: just under the gate it is ~19×4 px and the
> frame is resampled on its way to the framebuffer, so measured on that frame the closest pixel was
> **0.26** away from the flat faction colour while bare terrain reached **0.40**. The discriminating
> property is REDNESS again (`FACTION_BAR_INK_RED_MARGIN`).
>
> **But a redness probe centred on the hex measures the TOKEN, not the bar.** `village.png` peaks at
> **0.490** redness across **7908** pixels — its roofs — and `camp.png` at 0.310, so a box containing
> the glyph passes with `_draw_band_banner` deleted: the same "passed with the feature removed"
> failure the ⚠'s callout above was written about, reproduced one section later by reusing its box.
> The probe is a WINDOW BELOW THE GLYPH now (`_frame_inks_red_below_hex`,
> `NAME_PILL_BAR_PROBE_TOP`/`BOTTOM`/`HALF_W` — 0.36–0.85 hex radii down, ±0.6 wide) and the gate
> fixtures build with `STAGE_NOMADIC`, whose sprite has **zero** pixels over the margin.
> `_frame_inks_red_near_hex` keeps the ⚠'s box, which must contain the token; both share
> `_frame_inks_red_in_box`.
>
> **The margin is 0.12, measured in the window it is used in**: the bar peaks there at **0.180** and
> bare terrain under a bannerless token at **0.063**. The 0.24 it replaced was above the bar's
> arithmetic CEILING — faction 1 is `Color(0.95, 0.62, 0.2)`, so 0.33 at full strength and strictly
> less after blending. **A threshold a feature cannot reach is the arithmetic saying the calibration
> was taken off something else**, and it is the cheapest check available on a probe like this.
>
> The cull's own claim is STRUCTURAL, not pixel-based — `MapView.band_label_tiles()` reports which
> tiles placed a label, because a culled label leaves no ink and "dropped" is otherwise
> indistinguishable from "drawn somewhere I did not probe". The crowded fixture puts the selected
> band SECOND in snapshot order, so a renderer with no priority rule keeps the wrong label and fails.
> It also carries an **END-CAP band**, sitting in the ~21 px window between a "Shepherd's Fold"
> plate's body (98.0 px) and its inked width (119.4 px), so a cull that measures bodies rather than
> whole plates fails on that band and only that band.

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
nameplate BELOW (`_draw_band_banner` / `_draw_band_name_pill`), the over-cap count pill on the banner's right
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
