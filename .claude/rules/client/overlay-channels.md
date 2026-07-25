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
| `logistics` | Blue | Throughput flow |
| `sentiment` | Red | Morale/agency composite |
| `corruption` | Amber | Ledger intensity + risk weights |
| `fog` | Slate | Inverted knowledge coverage |
| `culture` | Violet | Divergence magnitude |
| `military` | Green | Readiness scalar |
| `terrain_tags` | Blended | Per-tag colors averaged |
| `pasture` | Straw→grass ramp, **+ two off-ramp barren tones** | The GRAZE layer's per-tile **capacity** (`TileState.grazeCapacity`) |
| `forage` | Wheat→green ramp, **+ one off-ramp barren tone** | The FORAGE (human food) layer's per-tile **capacity** (`TileState.forageCapacity`) |
| `hunt_danger` | Danger orange (generic lerp) | **NOT a wire raster** — projected client-side, `attack × ferocity` per herd (see below) |
| `threat` | Threat red (generic lerp) | **NOT a wire raster** — projected client-side, `attack × aggression` per herd (see below) |

Legend rendering: min/avg/max values + channel description.

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
case — the component rows carry no verdict word. Verify via `map_preview` states **"hunt_danger"**
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
  (`snapshot_dict`'s `OverlaySlices.pasture_capacity`), exactly as the logistics fallback already is.
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

