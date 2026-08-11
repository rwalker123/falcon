---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{BandOverlayRenderer,AnnotationRenderer}.gd"
  - "clients/godot_thin_client/src/scripts/ui/inspector/OverlayPanel.gd"
---

<!-- Extracted verbatim from lines 4027-4144 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Overlay Channels

Raster overlays streamed from `core_sim`:

| Channel | Color | Source |
|---------|-------|--------|
| `sentiment` | Red | Morale/agency composite |
| `corruption` | Amber | Ledger intensity + risk weights |
| `culture` | Violet | Divergence magnitude |
| `military` | Green | Readiness scalar |
| `terrain_tags` | Blended | Per-tag colors averaged |
| `pasture` | Straw→grass ramp, **+ two off-ramp barren tones** | The GRAZE layer's per-tile **capacity** (`TileState.grazeCapacity`) |
| `forage` | Wheat→green ramp, **+ one off-ramp barren tone** | The FORAGE (human food) layer's per-tile **capacity** (`TileState.forageCapacity`) |
| `hunt_danger` | Danger orange (generic lerp) | **NOT a wire raster** — projected client-side, `attack × ferocity` per herd (see below) |
| `threat` | Threat red (generic lerp) | **NOT a wire raster** — projected client-side, `attack × aggression` per herd (see below) |
| `elevation` | Elevation ramp | `MapSection.elevationOverlay` — **and the DEFAULT channel** (below) |

Legend rendering: min/avg/max values + channel description.

**`elevation` is the DEFAULT channel** (`overlays.default_channel`, the native decoder's
`DEFAULT_OVERLAY_CHANNEL`), which the Inspector's Overlays selector opens on when the player has
chosen nothing. A default has to be REAL on every map: elevation rides `MapSection.elevationOverlay`,
which worldgen publishes for every world, so it is never a placeholder, and relative height is
legible with no knowledge of the simulation's vocabulary.

**RETIRED: the `logistics` channel ("Logistics Throughput", blue), the top-level `contrast` alias,
and the whole trade-link overlay.** The sim no longer publishes a `logisticsRaster` or a link
network at all — `TradeLink`, `LogisticsLink` and the `Tile.mass` economy went with the dead trade
subsystem (`docs/plan_contact_and_logistics.md` §As-built) — so both had stopped saying anything:

- The **channel** kept rendering because the decoder had an absent-raster fallback that filled the
  plane with tile TEMPERATURE. Min-max stretched and labelled "Sum of supply flow touching the tile
  after current corruption multipliers", it was also the map's DEFAULT overlay: a mislabelled
  temperature map was the first thing a player saw. That fallback's one real job — deriving the grid
  extent from the tile list when no raster names it — survives as `tile_dims` in `snapshot_to_dict`,
  which is what it was always measuring.
- The **`contrast` key** was a top-level alias of `logistics_contrast` with **no GDScript consumer at
  all** (`MapView._ingest_overlay_channels` reads only each channel's own `normalized` / `raw` /
  `label` / `description` / `placeholder`), so it was deleted rather than re-pointed. Each channel's
  own `contrast` array inside `overlays.channels[key]` is unaffected and equally unread — leave it
  alone; it is the shape `insert_overlay_channel` publishes, not a per-channel decision.
- The **trade-link overlay** was `AnnotationRenderer.draw_trade_overlay` plus MapView's three
  reflective pass-throughs (`update_trade_overlay` / `set_trade_overlay_enabled` /
  `set_trade_overlay_selection`), the `trade_links` snapshot ingest, and the Map tab's
  `%LogisticsOverlayToggle`. With no links on the wire it drew the empty set every frame.

**This is "removed, and here is what replaces it", not "removed".** Issue #232 rebuilds a
route-network overlay against a network that actually exists, and the map-probe frame
(`map_trade_overlay`) is what it earns back. Do not resurrect the old seam names for it — they were
shaped around per-link `throughput` / `knowledge.openness` / `leak_timer`, which the new network
does not have.

**RETIRED: the `fog` channel ("Fog of Knowledge", slate, inverted knowledge coverage).** It was a
selectable *data* overlay fed by `VisionSection.fogRaster`, and it had nothing to do with fog of war —
two unrelated systems sharing a word, which is exactly why it went. **Fog of war is the only fog the
client keeps**: the `visibility` channel (labelled "Fog of War"), `MapView._fow_*` /
`_is_tile_visible` / `_unit_hidden_by_fog`, `heightfield_config.json`'s `"fog_of_war"` block, and the
blend shader's `fog_color` uniform — none of which this retirement touched. A grep for "fog" in the
client hits fog-of-war in almost every case; check which one you have before deleting anything. On the
client side the removal was three lines (`MapView.FOG_COLOR`, its `OVERLAY_COLORS` row, and the dead
`avg_fog` metric): the selector and legend are data-driven off the snapshot's `channel_order`, so
dropping the native channel registration removed the entry from both with no OverlayPanel edit — the
same property the derived-danger channels rely on, described below.

**`hunt_danger` / `threat` — the two derived-danger overlays (Predators Phase 0).** STRENGTH ≠ DANGER:
the wire carries four RAW components on `HerdTelemetryState` — `attack` / `defense` (open-ended, against
the human-strength anchor 1.0) and `ferocity` / `aggression` (native 0..1, fights-back-vs-flees /
initiates-unprovoked) — and **danger is DERIVED, never a stored field**. There are TWO: **HUNT danger =
`attack × ferocity`** (cost to hunt it — a mammoth reads high) and **THREAT = `attack × aggression`**
(menace unprovoked — a grazer reads ~0 however deadly it is to hunt; predators read high in Phase 1).
Both are per-ENTITY, NOT per-tile wire fields, so the **native decoder projects each onto tiles**
(`snapshot_dict`, beside the pasture/forage blocks): a grid-sized zero-init array, `max(existing, value)`
stamped at each herd's tile index, normalized against **that channel's own** map-max. Each is **guarded
on its own max > 0** — in Phase 0 nothing is aggressive yet, so `threat` is typically absent, and that
is correct. Neither is a two-tone ramp: MapView's `_color_for_tile` rides the generic
`GRID_COLOR.lerp(overlay_color, value)` path off `OVERLAY_COLORS` (`HUNT_DANGER_OVERLAY_COLOR` orange /
`THREAT_OVERLAY_COLOR` red, so the two read apart) — empty ground stays grid-colored — and the generic
scalar legend handles both. The overlay selector + legend are data-driven off `channel_order`, so the
channels appear with no OverlayPanel edit. **The herd drawer shows the four RAW components, NOT a verdict
word** (`Hud._herd_summary_lines` → `_append_danger_component_lines`, after Ecology, on EVERY herd): a
word can't survive the roster (a mammoth and later mech-infantry can't both be "Deadly"), so each is a
relative bar + raw value, Elevation-style — **Attack** / **Defense** bar against the max across
`_world_herds` (falling back to the bare value with no reference), **Fights back** / **Aggressive** as a
0..1 bar + %, plus a compact derived `Danger: Hunt X · Threat Y` summary. No `DetailFormat.detail_bbcode` tint
case — the component rows carry no verdict word.

**THE DERIVED ROW LEADS, AND THE THREE ROWS IT IS MADE OF INDENT UNDER IT.** `Hunt` is
`attack × ferocity` and `Threat` is `attack × aggression`, so exactly three of the four components
compose it — **`Defense` is in NEITHER**, and it rises above `Danger` to sit flat with Size / Herd /
Range, the other facts about what the herd IS. Indenting all four would assert a contribution Defense
does not make; it answers a different question (how hard the herd is to kill, and on the predator side
whether something else eats it), and pairing it with Attack by convention is what made it read as a
fourth input. Grouping also makes the arithmetic nearly readable off the page — attack is in both
terms, the other two split them.

> **`DANGER_COMPONENT_INDENT` (3 spaces) must NOT begin with `MORALE_BREAKDOWN_INDENT` (4).**
> `detail_bbcode` routes any line starting with that prefix to the FULL-WIDTH sub-line branch, and
> these rows have to stay KV table rows or their bars stop sharing a column — which is the entire
> point of a bar. Both halves are asserted in `ui_preview` (the prefixes cannot collide, AND an
> indented factor really did render inside a `[cell]`), because the collision is silent: the rows
> still appear, just unaligned.

Verify via `map_preview` states **"hunt_danger"**
(`map_hunt_danger.png` — mammoth + wolf glow orange) / **"threat"** (`map_threat.png` — only the
aggressive wolf glows red) + the printed legends, and `ui_preview` `herd_verbs` (harmless deer, all-empty
bars) / `herd_danger` (mammoth: high Attack/Fights-back, empty Aggressive), whose behavioural assertions
prove the component rows render and NO Harmless/Deadly word appears.

**`pasture` — the graze (pasture) layer, Grazing Phase 2a** (`docs/plan_grazing_foundation.md`;
`core_sim/CLAUDE.md` → The Graze (Pasture) Layer). Graze is the **animal-edible** vegetal stock
(grass and browse — cellulose humans cannot digest), the twin of the **human-edible** `ForagePatch`
biomass, and it sits on nearly every land tile with its own per-biome distribution. Four things about
this channel are load-bearing:
- **It is NOT a wire raster.** Graze rides `TileState` (per-entity diffed → zero delta bytes on an
  ungrazed turn), so the channel is **assembled in the native decoder from the tiles**
  (`snapshot_dict`'s `OverlaySlices.pasture_capacity`), rather than read off a `ScalarRaster`.
  Everything downstream — MapView's channel ingest, the OverlayPanel selector, the legend — then works
  with no special-casing. (Do **not** synthesize it client-side in MapView the way `province` is: a
  MapView-only channel never reaches OverlayPanel's selector, so it can't be picked.)
- **It paints CAPACITY, not fill.** "How good a pasture is this ground?" is the question the layer
  exists to answer (is prairie really pasture; is forest really poor?) and it is a property of the
  biome. The *fill* (`biomass / capacity` — "how eaten-down is it?") is a different question: it rides
  the legend as a map-wide standing-stock %, and per-tile on the tile card. It earns its own ramp only
  once herds actually eat graze (Phase 2b).
- **Zero pasture is NOT low pasture, and the ramp must never say it is.** A desert at 8/8 (full, but
  marginal) and a glacier that carries no pasture at all are completely different facts — and a naive
  `biomass/capacity` ratio renders BOTH as 100%. So capacity 0 leaves the ramp entirely:
  `MapView._pasture_color` paints **water** (Water terrain tag — server truth, not the render-side
  `blend_class`) a drowned slate and **dead land** a bare rock-violet, while any positive capacity
  starts at `PASTURE_POOR_COLOR` straw. The normalization is against the map's **richest** pasture, not
  min-max (min-max would rebase the ramp onto the worst *land* value and make a marginal desert read
  like a dead glacier).
- **Its legend is its own** (`_build_pasture_legend`, not `_build_scalar_overlay_legend`): the generic
  builder reports min/avg/max over EVERY tile, and here the map-wide min is 0 (the sea), which would
  report the world's poorest pasture as "0". Rows: Poorest / Average / Richest **over the tiles that
  actually carry pasture**, then `Barren ground` + `Water` counts. Keep row labels short — the legend
  panel clips.

Verify with `map_preview` state **"pasture"** (`map_pasture.png` — an earthlike-shaped map; it also
prints the legend dict, since that harness has no HUD) and `ui_preview` `pasture_legend` /
`tile_pasture_stressed` / `tile_pasture_none` (+ `food_tile`, which carries both stocks). **The live
earthlike map generates zero forest** (the biome palette thins `MixedWoodland`/`BorealTaiga` out
entirely — tracked in `core_sim/CLAUDE.md`), so the forest-is-poor-pasture inversion the two-stock
split exists to create is **unobservable in a live frame**; `map_preview`'s fixture stages a woodland
block deliberately so it can be seen at all.

**`forage` — the human-food layer, the twin of `pasture`** (`docs/plan_grazing_foundation.md` §1.1;
`core_sim/CLAUDE.md` → The two food webs). Forage is the **human-edible** potential of a tile — seeds,
nuts, tubers, fruit and inshore fish — from `forage.capacity_by_biome` (`labor_config.json`), the
mirror table of graze's. It is a **per-tile POTENTIAL on every tile**, exactly like pasture (NOT the
sparse per-`ForagePatch` stock), sourced from a new per-tile `TileState.forageCapacity`. Built the SAME
way as pasture — assembled in the native decoder (`OverlaySlices.forage_capacity`, from
`tile.forageCapacity()` in the tiles loop), normalized against the map's **richest** forage tile, and
cached client-side in `MapView.tile_forage` (from `tile_to_dict`'s `forage_capacity`, only tiles > 0)
for the legend's Poorest/Average/Richest figures. **THE ONE THING THAT DIFFERS FROM PASTURE:** "no
forage" and "no pasture" mean **opposite** things, and the render must not lie about it —
- **Water is NOT uniformly barren.** ContinentalShelf (130) / CoralShelf (180) / InlandSea (110) carry
  real fishing potential and sit **ON the ramp**, so coastal shelves **glow** on the forage map where
  they are dead water on the pasture map — the signature divergence of the two food webs. Only
  genuinely-zero biomes (DeepOcean, Glacier, lava, salt flat) leave the ramp.
- **There is NO "land but no site" middle category and NO Water off-row.** `MapView._forage_color` is a
  straight twin of `_pasture_color` minus the water/dead split: `normalized > 0` → the wheat→green ramp
  (`FORAGE_POOR_COLOR`→`FORAGE_RICH_COLOR`, a distinct green from pasture's so the two layers read
  apart); `normalized <= 0` → the single `FORAGE_BARREN_COLOR` slate. (A dark forage tile can be
  perfectly good FARMLAND — the barren fill is only the genuinely-zero biomes.)
- **Its legend is its own** (`_build_forage_legend`): Poorest/Average/Richest over the tiles that carry
  forage, then **one** honest `No forage` barren row (no Water row — shelves are on the ramp). The
  description carries a **`Gathering sites: N tiles`** sub-count (from `MapView.food_sites`, the tiles
  you can actually forage today — a subset of the potential), so the ramp reads as POTENTIAL without
  calling the rest of the land dead.

Verify with `map_preview` state **"forage"** (`map_forage.png`, same earthlike fixture as `map_pasture`
so the two compare tile-for-tile — forest/river valleys read RICH on forage where prairie/steppe reads
richest on pasture, and the shelf column glows on forage where it is barren on pasture; it prints the
legend dict) and `ui_preview` `forage_legend` (the honest twin — `No forage` barren row, no Water row,
the gathering-sites sub-count). The forage `capacity_by_biome` table ships in the sim, so the live
inversion is real; the fixture stages it deterministically for the harness.

---

## The on-tile yield label carries ONE component (issues #337 / #449 / #527)

A source can pay more than one account, but the label sits on a hex a few pixels wide beside a floor
mark and a `⚠` — there is no room for a second rate. `BandOverlayRenderer._draw_yield_label` therefore
shows the account the source PAYS, falling through **food → fodder → materials** in the wire's own
order: food when `realized_yield` is non-zero (every forage patch and edible quarry, so those frames
are unchanged), else the assignment's `fodder_yield` spelled with the WORD — a sown hay Field reads
`+0.40 fodder ♻` instead of `+0.00` — else its `material_yield`, each material naming itself, so a
hunted wolf pack reads `+0.22 hide ⇊`. **The word, never a borrowed glyph**: fodder has none, and a
material has a NAME, which is a better mark than an arrow saying only "not food".

**A TRADE branch stood between food and fodder until arc #527** — the wolf read `⇄+0.22 ⇊`, marked
with the retired `FoodIcons.TRADE_GOODS_GLYPH` — and for one release after that the inedible quarry
had no fall-through at all and read `+0.00`. The material arm is what closed it: `material_yield` is
the RESOLVED take, what the source actually credited to the band's `MaterialStore` this turn.

**THE MATERIAL ARM STATES EVERY MATERIAL.** Naming one of a vector picks a winner the sim does not
name, and summing them is the retired trade axis under a new name. `_draw_pill_plate` sizes to the
MEASURED run, so a two-material label is wide rather than clipped — a legibility question for
`map_band_label_overlap`, not a reason to state less than the truth.

**A HUNT call site passes NO fodder argument**, deliberately: no animal is harvested for feed, so a
hunt row's fodder is a structural zero and passing it would offer the label a branch it can never
take. **It DOES pass the materials.** `_yield_label_rate_text(value, fodder, materials)` is split out
of the draw call so a harness can ask it — a draw renders to a canvas and no assertion can read a
glyph back off one — and `_entry_materials` is its reader, the vector twin of `_entry_fodder` with
the same "no realized fallback" reasoning.

`YIELD_LABEL_COMPONENT_MIN` is the map twin of `SourceForecast.FOOD_FLOW_MIN` and is the test that
decides between the two SCALARS, applied to both so neither can show at a magnitude the other would
be hidden at; the material arm is gated instead by `SourceForecast.signed_material_components`
answering `""`, which is the HUD's own display floor and the same gate the work board's rate column
tests. Frame: `map_preview` `map_band_work` (the hay Field's label beside the deer's `+0.20`), with
`_assert_yield_label_component` driving the fall-through directly; the general
render-only-when-non-zero rule lives in `labor-ui.md`.

### The floor MARK is resolved ONCE and appended verbatim

The label's trailing glyph is the assignment's floor ZONE mark — `_entry_floor_glyph(entry)` =
`FoodIcons.for_floor_zone(SourceForecast.floor_zone(entry.floor))`, the same mark the work board's
mark column and the floor picker wear. It travels through `_queue_yield_label` → the deferred batch →
`_draw_yield_label` as `floor_glyph`, a **resolved glyph**, and every one of those hops spends it
as-is.

**A GLYPH RESOLVED ONCE AND RE-RESOLVED IS A MARK THAT VANISHES SILENTLY, and this one did.** The
parameter was called `policy` and `_draw_yield_label` ran it back through `FoodIcons.for_policy` — a
table keyed on the four IMPROVEMENT verbs since #442, which a floor-zone glyph is never a key of — so
the lookup answered `""` and **the map drew no harvest mark at all** for the life of the harvest-floor
arc. Nothing failed: a plain `+0.48` on a pill is a perfectly plausible label, and the frames that
would have shown it were frozen with it missing. The parameter carries its content in its NAME now
(`floor_glyph`), which is what makes the second lookup unwritable.

The guard is `map_preview._assert_work_floor_marks` — see `harness-map-probes.md`.


---

## Worked-source marks — one ring grammar for both food webs

`docs/plan_worked_source_marks.md` (issue #412). Two passes, and the split between them is the whole
design: **`draw_worked_source_marks` is always on, `draw_band_work_highlights` is what SELECTION
buys.**

**THE MARK BELONGS TO THE SOURCE, NOT THE HEX.** A hex holds a forage patch and several herds at once,
worked by different bands at different rungs, so a tile-level mark has to pick one answer out of four.
Each mark docks to the ring of the source's OWN secondary marker, via the slot
`SecondaryMarkerRenderer.compute_slots` assigned it — which is why `MapView._draw` **hoists
`compute_slots()` above the overlay pass**. That hoist is safe because it is a pure computation over
`discovered_sites` / `food_sites` / `herds` / `last_hex_radius`, none of which mutate during `_draw`.

- **The ring: green = we forage this, red = we hunt this**, at two weights — thin
  (`WORKED_RING_OTHER_ALPHA`) for any player band, bold plus a faint disc for the selected one.
  `FORAGE_WORKED_FILL`, the old whole-hex green tint, is **retired**: a fill belongs to a hex and a hex
  has no single answer. Radius `WORKED_RING_FACTOR` (0.34) sits deliberately INSIDE
  `MapView.FOOD_HARVEST_RING_FACTOR` (0.42) — the harvest ring is a different statement about the same
  marker and the two must read apart.
- **The badge** (`_queue_source_badge` / `_draw_source_badge`): ONE plate per source under its marker,
  carrying `⚒N` crew and, when the source can climb, a `⌃` chevron + the verb glyph. One plate rather
  than two because three sources × two elements is six things in forty pixels. **Below the icon,
  never upper-right** — `HERD_DISTRESS_BADGE_OFFSET_FACTOR` owns that corner, and a herd can be both
  penned-and-starving and ready-to-something. Ready rides the plate's BORDER as well as its glyph
  (`HudStyle.SIGNAL`), so an offer reads without resolving a small glyph. **Cyan, not amber**: amber is
  trouble here, and an opportunity in the trouble channel teaches the player to misread good news.
- **The badge shows a rung ON OFFER or a rung UNDER WAY, never both** — one axis in two states,
  mutually exclusive by construction. Under way renders `<verb glyph><percent>%` in
  `HudStyle.SIGNAL_DEEP` with **no chevron** (`⌃` offers; this reports); on offer renders `⌃<glyph>` in
  `HudStyle.SIGNAL`. The first cut shipped only the offer, which left a patch you were actively
  cultivating looking emptier than the untouched one beside it — the state the player is *waiting on*
  had no mark. `rung_in_progress` keys on the POLICY, not on a non-zero meter: a half-built source
  nobody works is a standing rung, which the rung glyph already reports.
- **CREW IS AGGREGATED PER SOURCE, NOT PER BAND** — two bands can work one patch, and two plates on
  one marker would be a lie about a single number.
- **A HUNTING EXPEDITION'S QUARRY IS A WORKED SOURCE.** It rides the COHORT
  (`expedition_target_herd`), not a `labor_assignments` row, so a pass that only walks assignments
  misses it — which is how the first cut shipped, with parties crossing the map and nothing saying
  what they walked to. Marked at **every** phase, `outbound` included: "this herd is already claimed"
  is what the player needs before committing a second crew.
- **The tile outline takes the SOURCE's colour**, never a fixed green. A hunted herd's hex outlined in
  the gather colour is a different claim and a wrong one. It is the only tile-level mark left, and it
  earns its place as the LOD/overflow fallback: `compute_slots` returns early below
  `ICON_MIN_DETAIL_RADIUS` and answers `-1` for an overflowed source, and in both cases there is no
  marker to ring.
- **Badges are DEFERRED into `flush_yield_labels`**, the same batch and the same reason as the yield
  labels: they annotate the map, and drawn inline they are painted over by the marker glyphs, rings
  and pending overlays that follow.
- **A yield label anchors to its source's SLOT** (`_label_anchor`), not the hex centre. The hex centre
  alone was a co-location bug: two hunted herds on one hex drew two rates at the identical point, and
  a herd standing on a worked patch did the same.

**MapView holds one new input and no derived model.** It already has `forage_patch_lookup`, `herds`
and every band's `labor_assignments`; the only thing missing was faction knowledge, so
`Hud.faction_knowledge_changed` → `Main` → `MapView.set_faction_knowledge` pushes the raw
`{track: progress}` row and the renderer asks `RungGates` itself. Pushing the row rather than a
digested mark model keeps ONE derivation for the map, the work board and the compose sheet.
`faction_knowledge` is cleared in `reset_world_state` — pushed in from another surface, keyed by
tracks a new world reuses (`.claude/rules/core_sim/world-handoff.md`).

Frames: `map_band_work` (both webs ringed, badges, the green/red outline split),
`map_worked_ready` (the ⌃ contrast — a tended patch offers Sow, a tamed pen-ceiling deer offers
Corral, a wild-ceiling wolf offers nothing), `map_hunt_expedition_quarry`, `map_overflow_worked`,
`map_band_label_overlap` (the slot-anchored labels).
