---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{BandOverlayRenderer,AnnotationRenderer}.gd"
  - "clients/godot_thin_client/src/scripts/ui/overlay/**"
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
| `ready_for_improvement` | `HudStyle.HEALTHY`, a DIM wash of the good-news green (generic lerp) | **NOT a wire raster** — the aggregate `⌃`, synthesized client-side from the patches, the herds and the faction's knowledge row (see below) |

Legend rendering: min/avg/max values + channel description.

## The picker is three modules on the MINIMAP's border, and a channel is one registry row

`docs/plan_knowledge_screen.md` §6. `ui/overlay/` holds the whole of the player-facing channel
picker, split by KIND so that adding a channel is a data edit and never a code one:

| Module | Kind | Holds |
|---|---|---|
| `ui/overlay/OverlayChannels.gd` | all-`const` + `static`, **a registry** | The CLIENT-side channel descriptors, and the merge that folds them into the wire's `overlays.channels` roster. Three rows today: the empty key (`PLACEMENT_FIRST`), `terrain_tags` (`PLACEMENT_LAST`, gated on `MapView.has_terrain_tag_data`) and `ready_for_improvement` (`PLACEMENT_LAST`, gated on `MapView.has_ready_for_improvement_data`) — **and that placement is load-bearing precisely because the channel is built LAZILY**: the key is absent from `overlay_channel_order` for most of a frame's life, so the wire pass cannot place it and this row is what puts it in the list at all |
| `ui/overlay/OverlayLegend.gd` | all-`static`, stateless | Renders one channel's title / description / readout into a container. Two `legend_kind`s — `KIND_RAMP` (the channel's own legend rows) and `KIND_FACTS` (the lines a descriptor's provider answers) — and **no channel is named in the file** |
| `ui/overlay/OverlayPicker.gd` | the widget | The TWO buttons docked on `MinimapPanel`'s top border and the popover each opens; pushes the selection through `MapView.set_overlay_channel` and knows no channel by name |
| `ui/overlay/ReadyForImprovement.gd` | all-`static`, stateless | ONE channel's DERIVATION — the `ready_for_improvement` raster, its per-web counts and the tiles it lit (`MODEL_READY`), asked of `RungGates` (below). The registry names it; nothing else does |

**A WIRE CHANNEL NEEDS NO REGISTRY ROW.** The sim publishes a label, a description and a
`placeholder` flag per channel and `MapView._ingest_overlay_channels` holds them, so `roster()`
synthesizes a descriptor for every key the wire names. A row is for a channel the CLIENT adds, or to
give a wire channel a legend kind other than the ramp. **`available` and `facts` are METHOD NAMES on
`MapView`, not `Callable`s** — a `Callable` is not a constant expression, so a registry holding one
could not be `const`.

**THE RAMP LEGEND IS `MapView`'s OWN, pulled through `current_overlay_legend()` / pushed by
`overlay_legend_changed`** — the dict the retired right-dock legend card used to render. Re-deriving min/avg/max
from `overlay_stats_for_key` would report the map-wide minimum for every channel, which for `pasture`
and `forage` is the sea: exactly the reading those two channels' own legend builders exist to avoid
(below, "Zero pasture is NOT low pasture"). One producer, two surfaces, no way to disagree.

### TWO BUTTONS, and the `L` legend card is retired

**One rule for the whole cluster: a button opens its own popover, attached to itself.** `◐` opens the
channel MENU; the button beside it opens the LEGEND for whatever channel is painted.

The first cut had one button with the legend inside its menu, then tried making that legend a standing
panel above the minimap — which pushed the menu away from the button that opened it by however tall
the legend happened to be. **A menu that does not touch its own button is the tell.** Splitting them
also makes the menu a FIXED height: the roster changes only when the world does, while a legend is
three rows for a scalar ramp and twenty for the biome key. `map_preview` asserts the attachment as a
measured GAP on both popovers, a picture being unable to separate *attached* from *close*.

**THE LEGEND BUTTON'S FACE IS THE AMBIENT READOUT** — which channel is painted, without costing a
panel. Three faces, in order: the descriptor's `icon` if it names one; else a **TINTED `■`** in that
channel's own map colour; else a **NEUTRAL `□`** in `INK_DIM`. `icon` is a registry field with a LIVE
fallback (the `FaunaSprites` / `WonderSprites` shape) and every row is iconless today, so the colour
path is the exercised one rather than a guard. A glyph rather than a `ColorRect` child, which makes
the two buttons the same width by construction (both are one-glyph text buttons under one stylebox
padding, which an empty button is not) and makes the eventual icon a straight swap of the same
property.

**THE NEUTRAL FACE EXISTS BECAUSE NOT EVERY CHANNEL HAS A COLOUR, and the first cut claimed one
anyway.** `MapView.overlay_color_for` is `OVERLAY_COLORS.get(key, OVERLAY_FALLBACK_COLOR)`, and FOUR
keys paint through special paths rather than the generic `GRID_COLOR.lerp(overlay_color, value)` —
`""` (terrain art), `terrain_tags` (a per-tag blend), `pasture` and `forage`. Only pasture had a row,
so **`forage` stated blue while the map painted a wheat→green ramp**, and `""` / `terrain_tags` stated
that same blue meaning nothing at all — under a docstring promising the button "can never disagree
with the map". `forage` has its own row now (`FORAGE_RICH_COLOR`, pasture's twin); `""` and
`terrain_tags` take the neutral face, because neither has a single hue and any row invented for them
would be a lie. **A channel with no row that paints GENERICALLY is fine and keeps the colour** —
`visibility` is painted with the fallback too, so its face agrees; `paints_with_overlay_color` is what
tells the two cases apart.

**THE GUARD WAS THE ACTUAL DEFECT.** The face assertion read
`legend_face_color() == overlay_color_for(key)` — true by construction for any key, and only ever
asked of `sentiment`, which had a row. It could not see this and never would have. It is now a claim
over the WHOLE roster with **the painted map as its oracle**: every channel either wears the neutral
face or wears a colour `_tile_color` actually paints that channel with. Sabotage-verified both ways —
removing the `forage` row fails it naming forage, and forcing the tinted swatch everywhere fails it
naming `""` and `terrain_tags` wearing the meaningless fallback.

**THE RIGHT DOCK'S `L` TERRAIN TYPES CARD IS GONE** — `LegendController`, `TerrainLegendPanel`,
`Hud.update_overlay_legend` / `toggle_legend` / `_on_legend_sort_pressed` with its Name/Count sort
header, the `legend_suppressed` preference and the `L` binding. It was early scaffolding showing this
same dict in a place a player had to know a hotkey to reach.

**What it carried that nothing else did is the BIOME KEY** — `MapView._build_terrain_legend`, the
per-biome tile counts for the bare map. That is **not** the same table as `terrain_tags` (biomes, not
environmental tags), so the empty key's registry row is `KIND_RAMP` precisely to give it a home: the
legend button is never in a dead state, and `L`'s one irreplaceable job moved onto it. The biome names
remain reachable from the Inspector's Terrain tab and from any hex's tile card, which is why this is a
relocation rather than a loss.

### THE CATCHER MUST NOT EAT A PRESS AIMED AT THE PICKER'S OWN BUTTONS

The full-screen dismiss catcher sits on a layer ABOVE the bar, so with either popover open the OTHER
button never receives its click: pressing it read as *dismiss* rather than *switch*, and the player
had to click twice to reach the other card. `_on_catcher_input` resolves the two buttons itself now,
in the same terms their own `pressed` handlers use — the open one's button toggles it shut, the
other's swaps to it, everything else dismisses.

**No frame can carry that**: the failing state renders as a map with nothing open, which is an
ordinary map. `map_preview`'s `_assert_picker_buttons_swap` drives every leg as a REAL press through
`Viewport.push_input`, because driving a button's own `pressed` signal routes around the very thing
under test. It shares `ui_preview`'s `InputProbe` for the canvas→window conversion — an unconverted
press misses the bar entirely, which is what it did on the first attempt.

### THE BIOME KEY WEARS THE TERRAIN ART, NOT THE PALETTE

With terrain textures on, a flat colour swatch names a biome the player cannot match to anything on
screen — the hexes are painted art, not the palette entry. `_build_terrain_legend` therefore hands
each row `TerrainRenderer.hex_texture_for(id)`, the very texture the blend-OFF renderer stamps on a
hex, and `OverlayLegend._swatch` renders a `TextureRect` where a row carries one.

**Gated on the `T` toggle, and the claim is a PAIR.** With textures OFF the map really is flat
`_tile_color` fills and the palette swatch is the honest answer. A swatch is a small square either way
at a glance, so a frame cannot separate the two and a one-sided claim passes on a key that hands out
art whether the map is textured or not; `map_preview` asserts both directions over EVERY row.

Two mechanical traps, both of which shipped once: a `TextureRect` reports its TEXTURE's size as its
own minimum and `custom_minimum_size` is a FLOOR rather than a cap, so without `EXPAND_IGNORE_SIZE`
one biome filled the whole card; and a texture swatch needs its own larger box (`TEXTURE_SWATCH_SIZE`),
a hex-masked tile being mostly transparent and unreadable at the colour swatch's size.

**A `ScrollContainer`'s vertical bar is drawn OVER its content, not beside it**, so the legend
reserves a gutter for it (`LEGEND_SCROLL_GAP` plus the bar's own measured minimum) — without which the
value column ran under the bar. It is reserved whether or not the bar is shown, which also stops the
popover changing width per channel. `map_preview`'s biome fixture carries enough biomes to SCROLL,
deliberately: a short key renders the same either way, which is how this shipped.

### The picker owns the channel ACROSS A SNAPSHOT and nowhere else — two signals, two rules

**RE-ASSERT on `overlay_channels_ingested`.** `_ingest_overlay_channels` clears `active_overlay_key`
on every frame it ingests, so without a re-apply a chosen channel is painted for exactly one turn and
then reverts to bare terrain — which reads as a click that did nothing. The Inspector panel did this
from its own ingest; nothing else will now. The signal is emitted at the END of `display_snapshot`,
before `_emit_overlay_legend`, so a listener asking `has_terrain_tag_data()` sees THIS frame's tags
and the legend that follows describes the channel just re-asserted.

**ADOPT on `overlay_legend_changed`.** That signal also fires on every ordinary channel change, and
**a picker that re-asserted on it overwrites every other caller of `set_overlay_channel`** —
`MapView.set_terrain_mode` / `toggle_terrain_mode`, `set_fow_enabled`'s deliberate clear, and every
offline harness that drives a channel with no picker in the loop. That is not hypothetical: the first
cut did exactly this, and **seven `map_preview` overlay states rendered as bare terrain** —
`map_pasture`, `map_forage`, `map_hunt_danger`, `map_threat`, `map_crisis_annotations` and the two
pasture-selection frames — each a perfectly plausible picture of a map with no overlay on it, which is
why only a pixel diff against a pre-change render found it. Outside an ingest, `MapView` is the
authority for what is painted and the picker follows it.

`OverlayPicker._syncing` is what keeps the ADOPT branch off the picker's own echo. `_apply_to_map`
then reads the key back anyway, because **`set_overlay_channel` silently REFUSES a key it holds no
raster for** — an unread push would leave the lit row claiming a channel the map is not painting, and
re-assert the same rejected key on every later frame. `map_preview`'s `map_overlay_picker` asserts
both rules, the survival and the stand-down, in one place.

**THE POPOVER IS A `Control`, NOT A `PopupPanel`** — `TurnOrb`'s catcher shape. Every other popover
in this HUD is a `PopupPanel` because a Window cannot change a docked zone's height, a problem a
widget floating over the map does not have; what a Window WOULD cost here is the frame — it renders to
its own surface, so an opened popover would be absent from `map_preview`'s capture and the state
could not be judged at all.

**AND IT GETS ITS OWN `CanvasLayer` (105), ABOVE EVERY DOCKED SURFACE.** The picker rides the
minimap, which in the shipped client is EMBEDDED in the HUD's bottom bar — so the popover inherited
`Main.HUD_LAYER` (101) and the Band/City dock (`BandCityPanel.LAYER_INDEX` 103), the Workbench
(`Main.WORKBENCH_LAYER` 103) and the event dock (`EventDockPanel.LAYER_INDEX` 104) all drew straight
over it. Reported from play as *"the menu shows up under the band panel"*. It stays below
`Main.LOADING_OVERLAY_LAYER` (150), a world being built having to cover everything. `Main`'s HUD and
Inspector layers are NAMED constants now precisely because four surfaces are placed relative to them
and were all reasoning about bare literals to do it.

**Clearing the dock's LAYER is only half of it: the popover also has to open into the PLAY AREA.**
Right-aligning a ~290px popover to a button in the nav cluster puts its far edge inside a ~495px
docked panel, and drawing it *above* the panel instead of under it would merely trade an unreadable
popover for one covering what the player is reading. `MapView.unreserved_screen_rect()` is the bound —
the viewport less every edge a docked panel has reserved, in CANVAS units, which is the space the
reservations arrive in and the space a HUD `Control` positions in. It is deliberately not
`_reserved_inset_span_local()`, which converts the same numbers into the map's counter-scaled units
for the cover-fit maths (`interface-scale.md`).

**IT LEFT THE INSPECTOR ENTIRELY.** `ui/inspector/OverlayPanel.gd` was 308 lines doing four jobs, two
of which grew a branch per channel — an inline `terrain_tags` label/description/availability block in
its ingest, and hand-written `Culture` and `Military` placeholder tabs in its refresh, whose content
was exactly what the generic legend produces. Both are deleted along with the script, the
`OverlaySection`/`OverlayTabs` subtree in `InspectorLayer.tscn`, and `Inspector.gd`'s member and four
forwards. `Inspector._ingest_overlays` survives as a junction for the `terrain_palette` /
`terrain_tag_labels` / `crisis_annotations` side-routes, which are Terrain's and Crisis's; the
channels on that same key are read straight off `MapView`, which ingests the identical payload. The
Inspector is a modding tool that ships hidden behind `I`, so the map's own channel picker was
somewhere a player would never look — that, not the file's size, is why it moved.

**`MapView.set_overlay_channel` still special-cases `terrain_tags`, and that is the render path, not
the picker's.** Leave it alone; a new channel is a registry row plus whatever raster or derivation it
needs, **never a second `if key ==` there**.

**`overlays.default_channel` HAS NO READER, AND HAS NOT HAD ONE FOR LONGER THAN IT LOOKS.** The
native decoder publishes it (`DEFAULT_OVERLAY_CHANNEL`, `native/src/snapshot/mod.rs`) and nothing
consumes it: `OverlayChannels.roster()` / `descriptor_for()` read only `channel_order` / `labels` /
`descriptions` / `placeholder_flags`, and the picker's fallback is `_roster[0]`, which is always the
empty key (`PLACEMENT_FIRST`). **The Inspector panel did not honour it either** — its one use sat
behind `if not _overlay_channel_labels.has(_selected_overlay_key)`, and it added the empty key to
that table unconditionally with `""` as the initial selection, so the branch never fired. The client
has opened on NO OVERLAY for as long as both halves have looked like this; the paragraph that used to
stand here said otherwise, and was describing an intention rather than the code.

**AND THE MAP OPENING PLAIN IS THE WANTED BEHAVIOUR — decided 2026-08-23, do not "fix" it.** The
player gets bare terrain and picks an overlay if they want one. So `default_channel` is not a feature
waiting to be wired: it is a field expressing an intention nobody holds. The argument it was written
for still reads well — elevation rides `MapSection.elevationOverlay`, which worldgen publishes for
every world, so a default channel would never be a placeholder — and it lost anyway, which is why the
argument is recorded here rather than the field quietly deleted with no trace of what it wanted.

Retiring it is a one-line change to `native/src/snapshot/mod.rs` whenever the dead wire field is worth
the churn; nothing client-side would notice.

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
dropping the native channel registration removed the entry from both with no picker edit — the
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
channels appear with no picker edit. **The herd drawer shows the four RAW components, NOT a verdict
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
  Everything downstream — MapView's channel ingest, the picker's roster, the legend — then works
  with no special-casing. (The old caveat here — *"do not synthesize it client-side the way `province`
  is, a MapView-only channel never reaches the selector"* — **no longer holds and is why it is
  recorded rather than deleted.** The Inspector panel built its list from the SNAPSHOT payload, so a
  channel MapView added to itself was unreachable; the picker builds it from
  `MapView.overlay_channel_order`, which is that payload PLUS MapView's own additions, so `province`
  is pickable now and renders through the generic scalar path like any other. Reading MapView rather
  than the payload is what let the `overlays` routing leave the Inspector entirely.)
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
draws the legend in the minimap picker's own popover as `map_pasture_legend`, the frame that used to
be a printed dict here and a hand-transcribed fixture in `ui_preview`) and `ui_preview`
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
`map_forage_legend` beside it — the honest twin: `No forage` barren row, no Water row,
the gathering-sites sub-count). The forage `capacity_by_biome` table ships in the sim, so the live
inversion is real; the fixture stages it deterministically for the harness.

**`ready_for_improvement` — THE AGGREGATE `⌃`, and the first `KIND_FACTS` channel**
(`docs/plan_knowledge_screen.md` §7, Slice D). The map has marked the per-source case since issue
#412: a *worked* source that can climb a rung wears a `⌃` on its own badge. This is the map-wide view
of the same opportunity — and the legend is COUNT LINES rather than a ramp, because there is no "more
ready": *"4 sources · 2 patches, 2 herds"* and *"Nearest (7, 6)"*.

**WHAT LIGHTS IS A CANDIDATE SET AND ONE QUESTION, and a lit hex is a strict subset of the hexes
wearing a `⌃`.** `ReadyForImprovement._is_candidate` runs first because it is CHEAP — a dictionary
lookup and one string comparison — and `RungGates.next_rung_ready` runs only on what survives it:

1. **The source is IMPROVED, or a player band is WORKING it.** A union. Improved is read off the rung
   label; worked is the band's own `labor_assignments` rows plus a hunting party's quarry.
2. **No OTHER faction owns it** — a refusal, never a requirement; see below.
3. **A rung above it is available RIGHT NOW** — `next_rung_ready`, **the same call that draws the `⌃`
   on a marker's badge**. Knowing how, the ground taking seed and the species' own ceiling are all
   inside that one answer, so this channel adds no ladder logic and must not.

> #### ⛔ THE CANDIDATE SET TOOK THREE ATTEMPTS, AND EVERY WRONG ONE PASSED ITS OWN FIXTURE
>
> **No set at all** — every wild patch admits Cultivate, so the turn Cultivation was learned the
> channel lit **every land tile the faction could see**: a sheet that named nothing, on the one channel
> whose job is to point somewhere.
>
> **"Already improved"** — unshowable-by-construction for the case that matters most. The FIRST rung on
> a source is an improvement onto ground carrying none, so requiring an existing one can never surface
> a first one, and **an entire knowledge that only ever unlocks a first step goes invisible**. Reported
> from play: a faction that had just learned Herding, hunting two fully tamable herds, saw an empty map.
>
> **"Worked"** — hid a field you built and walked away from, which is precisely what this channel
> should be pointing at.
>
> It is the UNION because the two halves answer different questions — *what you hold* and *what you
> have hands on*. **The lesson is the fixture, not the rule**: each wrong version shipped with a
> harness state built around its own set, which confirmed it rather than catching it. Both halves are
> now asserted POSITIVELY and separately, so losing either fails by name rather than moving a count.

> #### ⛔ THE OWNER TEST IS A REFUSAL, AND A REQUIREMENT WOULD HAVE RE-BROKEN THE FIRST RUNG
>
> `ForagePatch::owner` is `Some` **only once an improvement meter is above zero**, so an unimproved
> patch a band is working states no owner at all. `has_owner` as a REQUIREMENT would therefore have
> refused every first-rung opportunity on the plant web — the whole of the early game — reintroducing
> the defect above through a different door. `_not_another_faction_s` reads: no owner recorded is fine,
> our own owner is fine, only a stated foreign owner refuses. A herd row carries no owner at all (the
> pre-existing `RungGates.hunt_gates` gap), so there is nothing to ask on that web and nothing is
> invented.

**THERE IS NO "ALREADY BEING BUILT" TEST, and its absence is a decision.** `next_rung_ready` already
declines the verb a crew has DECLARED, so a patch mid-Cultivate is not offered Cultivate by the one
call this channel makes. Where a source genuinely has a DIFFERENT rung available while one is in
flight — or a meter carrying work nobody ordered — that rung really is orderable, and saying so is
correct. The marker's badge keeps its own in-progress branch because it must choose ONE face to draw;
a channel that only lights or does not has no such choice, so the branch would be a special case
bought for nothing.

**THE LEGEND'S SECOND LINE IS THE NEAREST LIT SOURCE**, measured from the SELECTED band. It was
*"N unworked · nearest (x, y)"*; an improved source nobody is on now lights in its own right, so the
interesting ones are in the lit set rather than behind a second count.

**THE RUNG LABEL IS WHAT MAKES THE IMPROVED HALF BRANCH-BLIND.** `ForagePatchState.currentRung` /
`HerdTelemetryState.currentRung` spell the rung a source STANDS on as `<branch>:<id>` —
`plant:tended`, `animal:pastoral` — so `SourceForecast.rung_above_branch_floor` answers without being
told which food web it is looking at. The alternative is each web's private booleans (`is_cultivated` +
`is_field` here, `domestication` against a threshold + `corralled` there), which costs every consumer
a reader per web and a route ladder a third. **A new branch costs this channel nothing: one entry in
`SourceForecast.RUNG_BRANCHES`.** An unknown key — a branch a stale client has not been taught, or the
`""` a hand-built fixture carries — answers `false`, which shows nothing rather than lighting a whole
branch including its untouched floor.

> **`SourceForecast.improvement_is_done` READS THE SAME LABEL NOW**, which is what makes the branch
> blindness the whole client's rather than this channel's. It is inside `next_rung_ready` and asked of
> every candidate here, and it was a TAME special case plus three keyed tables, one of them existing
> only to say a Field is also tended. It is one comparison — `rung_at_or_above(current_rung, the rung
> this verb builds)` — over `IMPROVEMENT_RUNG_KEYS`, the inverse of `RUNG_KEY_IMPROVEMENTS` and the
> table that must be kept in step with it. **The two readings are provably one fact**:
> `forage::patch_rung_key` IS `patch.standing().held` and `ForagePatch::is_cultivated` is
> `standing.held.is_at_or_above(PlantTended)`, so the retention-bar divergence that would have made the
> swap unsafe does not exist (`forage.rs`, "RETIRED: `cultivation_meter_full`"). `FORECAST_DONE_FLAG_KEYS`
> now has **no shipped reader at all** — it survived for `rung_needs_repair`, which is itself retired
> (`labor-ui.md` → "THE OFFER TEST AND THE TRACK TEST ASK ONE QUESTION"), and the test tree keeps the
> table to derive a fixture's `current_rung` from its flags. `FORECAST_RETIRED_BY_HIGHER_RUNG` is
> deleted: *a higher rung retires the one below it* is the ORDER of `RUNG_BRANCHES` now rather than a
> table beside it.
>
> **AN UNKNOWN OR EMPTY RUNG KEY ANSWERS `false` ON BOTH SIDES OF THE COMPARISON**, for the reason the
> paragraph above gives for `rung_above_branch_floor`: a stale client and a fixture that never stated
> the field must read as *nothing has been built here*, which offers the player a rung they may already
> hold, and never as *everything is built*, which would retire every climb on that branch in silence.
>
> **THE FIXTURE MIGRATION IS DONE, AND IT IS ONE DERIVATION FOR THE WHOLE TEST TREE.**
> `tools/ui_preview/fixtures_rung.gd` transcribes the sim's own *"sown → field, cultivated → tended,
> else wild"* (and its animal twin) once, and `map_preview`, `band_panel_preview`,
> `snapshot_alias_guard` and the chapters all state their rows' `current_rung` through it — `stamp_patch`
> / `stamp_herd` derive it from the row's OWN flags, so no fixture can stand on a rung its
> `is_cultivated` / `corralled` pair contradicts, and a row re-dialled by a caller is re-stamped where
> it is re-dialled.

**THE UNLOCK NEVER LIGHTS THE MAP, and that is the whole reason this is a channel.** Nothing anywhere
gets a timed highlight when a track completes; the attention row states a count and the player who
wants to see them all turns the channel on. A channel is a thing the player asks for, which is what
makes it the right shape for news that does not expire.

**IT IS ALWAYS OFFERED ON A WORLD THAT HAS SOURCES, INCLUDING WHEN NOTHING IS READY** — the legend
then reads *"Nothing can be improved right now."* — `ReadyForImprovement.FACTS_NONE`, and note that it says **improve**: "rung" and "climb" are this arc's internal vocabulary and never reach the player. Gating it on the COUNT would make the channel appear
the turn the first discovery lands, which is the map lighting up for an unlock under another name,
and a roster row that comes and goes is a row a player cannot learn. `has_ready_for_improvement_data` is
therefore a question about the WORLD — a grid, and a source of either web — never about the count, and
never about the raster, which for most of a frame's life does not exist (below).

> #### ⛔ IT IS BUILT LAZILY, AND A MEASUREMENT IS WHY — `MapView.DEFERRED_OVERLAY_BUILDERS`
>
> The sim seeds a `ForagePatch` on **every** food-module tile carrying any human-edible capacity
> (`core_sim/src/forage.rs` → `spawn_initial_forage`) and `snapshot_forage_patches` caps nothing, so
> the pass is one `RungGates` evaluation per source over a world-sized set. `map_preview`'s scale
> probe walks the ceiling — a full-size 256×192 world with a patch on every tile — at **~6.7 µs a
> source, 331 ms for 49,152** (PR #572). Deriving that on every turn boundary for a channel nobody
> has selected is not a constant worth tuning; it is work that should not happen.
>
> So `MapView` does **not** build it during the ingest, unlike `province` (which is a partition over
> TILES and genuinely cheap). `DEFERRED_OVERLAY_BUILDERS` is a `{key: builder method}` table and
> `set_overlay_channel`'s FIRST line realizes whatever that table names — which is not the second
> `if key ==` §6b forbids, because it names no channel and a second expensive channel is one row.
> **The per-turn refresh then falls out of a seam that already existed**: the picker re-asserts the
> painted channel on every `overlay_channels_ingested`, so the channel the player is HOLDING is
> rebuilt once per turn and every other one costs nothing at all.
>
> `ready_for_improvement_facts()` realizes too, because a legend can be opened on a channel the map is not
> painting. Past that the facts are answered off the cached model.

**AND THE KNOWLEDGE PUSH ARRIVES AFTER THE INGEST, so `set_faction_knowledge` stales it.**
`Main._apply_snapshot` calls `display_snapshot` first and fans the HUD out after it, so
`faction_knowledge_changed` always lands behind the map's own ingest — a channel built only during the
ingest would state the PREVIOUS turn's knowledge for the whole turn a discovery arrives on, which is
the one turn it is wrong on and the one turn a player looks. Marking it stale is free; the REBUILD
happens there and then only when it is the channel being PAINTED. The staleness test compares against
`_ready_for_improvement_knowledge`, a **COPY** — `faction_knowledge` is the HUD's own dict held by reference
(`FactionReadouts.faction_tracks` returns it uncopied), so storing the reference would compare a row
against itself and never fire.

**`ReadyForImprovement` ASKS `RungGates`; IT DECIDES NOTHING ABOUT THE LADDER.** The ladder question is
ONE call — `next_rung_ready`, the same one that draws the `⌃` on a marker's badge — so the aggregate
and the mark on the hex under it cannot disagree. Knowing how, the ground taking seed and the species'
own ceiling all live inside that answer, and none of them is re-derived here.

> **A PARAGRAPH HERE USED TO DESCRIBE A SECOND CALL, and it survived the change that removed it.** It
> said `_offers_a_rung` asked `rung_in_progress` first and `next_rung_ready` second, and that asking
> only the ready test would wrongly light a patch mid-Cultivate. That function is gone, the channel
> makes one call, and the harness now asserts a mid-Field patch nobody declared **lights** — see "THERE
> IS NO 'ALREADY BEING BUILT' TEST" above, which is the live rule. It is recorded rather than deleted
> because a rule file that contradicts itself is worse than one that is merely out of date: an agent
> loading this file to fix a *"a mid-build tile is lit"* report would have followed the stale half and
> re-broken the fixture that pins the fix.

**THE `improvement` AXIS COSTS A WALK OVER BANDS, NOT OVER SOURCES.** `ReadyForImprovement.worked_sources`
answers `{secondary key: declared improvement}` off `units` and their assignment rows (tens of
iterations), and its KEY SET is also the WORKED half of the candidate union (`_is_candidate`) — two
jobs, one walk. The declaration is not redundant with the meters: a Sow ordered this turn stands at 0% on
every rung, so without it the source reads as untouched on the very turn the player committed it.
**It is a SECOND reading of `BandOverlayRenderer`'s same set**, deliberately — that one is fused into
a draw (effective columns, crew and builder accumulation, ring and badge all fall out of one loop),
so there is no seam to share short of restructuring a shipped render path. What the two share is the
IDENTITY (`secondary_food_key` / `secondary_herd_key`) and the gate layer, which is where a
disagreement would actually come from.

**THE FACTS ARE ANSWERED ON DEMAND, NOT STAMPED INTO THE MODEL.** *Nearest* is measured from the
SELECTED band (falling back to the player's first, and dropping the coordinate entirely when there is
no band at all), and a selection change is not a snapshot — so `ready_for_improvement_facts` scans the
cached LIT list (`MODEL_READY`, tens) rather than re-deriving. It measures through `MapView._hex_distance` and
`_wrapped_col_delta`, the map's own metric and its own seam rule, so "nearest" here is the nearest the
map draws.

**THE HERD WEB HAS NO OWNER ON THE WIRE, so condition 2 is a PATCH-ONLY test.** `HerdTelemetryState`
carries no owning faction at all — the pre-existing gap `RungGates.hunt_gates`' own docstring records
— so a herd another faction has tamed reads as ours here, exactly as on the compose sheet, and
condition 1 is the whole faction test on that web. Nothing in the channel invents an ownership signal
from `domestication` or a nearby band: closing the gap means putting an owner on the herd row and
reading it in the shared gate, for all four surfaces at once.

**ITS `OVERLAY_COLORS` ROW IS `HudStyle.HEALTHY`, DERIVED** — the themed "well-supplied / good"
green. The aggregate is a reading about LAND, and land that could carry more is the same good news
every other healthy mark on this client states in green. **The per-source `⌃` badge keeps `SIGNAL`**,
and the two being different marks is deliberate: a badge pinned to one source and a dim wash over a
whole map are not one vocabulary said twice. The channel wore `SIGNAL` for one release on the
agreement-with-the-badge argument, and the shipped palette's `SIGNAL` is a near-white parchment cream
— painted over a map it blew the lit hexes out against the dark grid, which is what retired that
argument. It is still DERIVED rather than authored, and for the reason that survives the swap: a
hand-picked value would agree with the palette in one theme and fight it in three. Amber would be
wrong twice: it is the trouble channel here, and teaching a player to read good news in it is how a
warning stops being read.

> **THE ROSTER-WIDE FACE GUARD DOES NOT COVER THIS ROW, and it cannot — measured by sabotage.** That
> guard (above, "THE GUARD WAS THE ACTUAL DEFECT") asks whether a face states a colour the map really
> paints. This channel rides the GENERIC `GRID_COLOR.lerp(OVERLAY_COLORS.get(key, FALLBACK), value)`
> path, so with its row DELETED the map paints the fallback blue and the button states that same blue
> — honestly — and the guard passes, exactly as it does for `visibility`. It catches a channel with a
> paint path of its OWN; that is how it caught `forage`. What a dropped row costs a GENERIC channel is
> not a lie but the HEALTHY GREEN, so `map_preview` asserts that instead and by name — and as FOUR
> terms, because the declared hue, the hue the legend button states and the tint a lit hex wears are
> three different values: the channel declares `HudStyle.HEALTHY`, the button states it UNDIMMED, the
> map paints `GRID_COLOR.lerp(HEALTHY, TILE_READY)` on some tile, and the map paints the undimmed hue
> on NONE. That last term is what pins the dimness — the expected wash is composed from `TILE_READY`,
> so it moves with the constant and the third term alone passes at a full fill (sabotage-verified).
>
> The guard is also out of reach here for a second reason worth knowing before writing one like it:
> `_overlay_picker_state`'s fixture publishes no patches and no herds, so `has_ready_for_improvement_data`
> is false in that world and this channel is not in that roster at all.

The raster is BINARY — a hex with an offer wears a DIM WASH of the channel colour
(`ReadyForImprovement.TILE_READY`), every other stays grid-coloured — because "there is an opportunity
here" is the whole claim and shading by count would say a hex with three offers is a better place to
stand than one with a Sow. The wash level is a fixed value rather than a per-hex strength, and it is a
wash rather than a full fill because a full fill blew the lit hexes out against the grid and a map of
them was a glare instead of a reading. The `raw` plane carries the count for anything that wants to
quote it.

Verify with `map_preview` state **"ready for improvement"** (`map_ready_for_improvement` — the CONTRAST, not the
glow: SEVEN sources lit — four patches and three herds — while FIVE controls stay dark: a worked patch
whose crew DECLARED its rung, a wild half-cultivated patch nobody works, the wild-ceiling wolf, a
worked patch whose plants may climb no further, and a worked patch another faction owns. Read the
counts off `map_preview`'s `READY_EXPECTED_*` constants rather than from here — this sentence has been
wrong twice; `map_ready_for_improvement_legend` is its facts card) plus the
assertion block beside it, which drives the late knowledge push, the counts split by web, the tiles a
picture cannot separate, and the nearest answer moving with the selection off the cached model.

---

## The TEMPERATURE channel — colour carries degrees, the LETHAL band does not (issue #614)

There were overlays for elevation, moisture, culture, pasture, forage, terrain tags, hunt danger,
threat and crisis, and none for the one tile property that decides whether a camp site is survivable.

**Nothing new is on the wire.** `MapView.tile_temperature` already holds per-tile °C for every
decoded tile, so the channel is CLIENT-DERIVED like `terrain_tags` and `ready_for_improvement` — one
`OverlayChannels.CHANNELS` row, one `DEFERRED_OVERLAY_BUILDERS` entry
(`MapView._build_temperature_channel`), no schema change and no server work. The row is gated on
`MapView.has_temperature_data()`, which asks the TILES and never the raster, for the reason
`ready_for_improvement`'s row already records: the channel is built lazily, so a predicate that asked
for a raster could only offer the row after something had already built the thing the row exists to
let the player ask for.

The registry owns the channel's three player-facing strings (`OverlayChannels.TEMPERATURE_KEY` /
`_LABEL` / `_DESCRIPTION`) and `MapView` reads them back, so the picker's list and the channel the
renderer stamps into its own table cannot become two names for one thing.

### The treatment: an honest gradient, with lethality on a SECOND channel

**The default scalar path is exactly what must not happen here.** `GRID_COLOR.lerp(overlay_color,
value)` with Low/Average/High rows would make the player interpolate a smooth gradient to find where
the deadly line falls — and they would find it in the wrong place, because there is nothing in a
gradient to find. So the two readings are carried by different things and never compete:

- **Colour carries the temperature.** A five-stop ramp, cold → warm
  (`MapView._temperature_ramp_color`), NORMALIZED MIN-MAX across the map rather than against the max
  the way pasture and forage are. Their zero is a categorical fact — "this ground carries no pasture"
  — so a max-relative scale reads correctly there; temperature has no such zero, and min-max is what
  makes THIS map's actual range legible. The RAW °C array rides alongside so the legend prints real
  degrees rather than a fraction.
- **Lethality is drawn, not tinted** — `MapView._draw_temperature_lethality`, a hatch on every
  killing hex and a heavy contour along every edge where killing ground meets living ground. Its
  placement and its zoom gate are `map-renderers.md`'s, the pass living in `MapView`.

Lethality is `TileSurvivability.is_lethal` — the SAME authority the tile card's `⚠ Lethal cold` chip
reads (`band-readouts.md`), never a threshold re-derived here, so the map and the card cannot
disagree. The pass is silent until the sim has published its model, exactly as the chip is.

### The legend is its own, and its Lethal row is the SIM's range

`_build_temperature_legend`, not `_build_scalar_overlay_legend` — the pasture/forage precedent, and it
qualifies for a sharper reason than they do: Low/Average/High of a normalized fraction would name
neither a degree nor, far worse, the THRESHOLD. The rows are the map's real **Coldest / Average /
Warmest** in °C, plus a **Lethal** row whose value text is read off `TileSurvivability`
(`outside 0.0 – 40.0 °C` at the shipped tuning — the two tails' ONSETS, an interval rather than a
spread around an ambient) and is absent entirely until the model is published.
A legend that transcribed those degrees would be a second opinion able to drift from the map it is
describing; `map_preview` asks the question by RETUNING the model and re-reading the row.

Swatches come from the same `_temperature_ramp_color` the map paints through — the
`_pasture_ramp_color` rule, because a swatch is a claim about the map — and the harness asserts it by
comparing each swatch against what `_tile_color` hands back for that extreme's own tile.

### ⛔ THE LETHAL SWATCH IS HATCHED, BECAUSE THE MAP'S MARK IS

It shipped as a **solid crimson block** on the reasoning that a legend swatch is a flat `Color` and
the channel description could carry the shape. On screen that reads as *this ground is painted solid
red* — a fill the map paints nowhere. Individually defensible, misleading in composite: the same
defect class as a `Temperate` chip on ground that kills, inside the fix for it.

`OverlayLegend` grew a row-declared **swatch KIND** for it (`SWATCH_KIND_SOLID` / `SWATCH_KIND_HATCHED`),
which is how that file is extended — **no channel is named there, and none may be**, exactly as
`legend_kind` names none. `SWATCH_KIND_SOLID` is the default, so every row written before it exists
keeps its flat colour untouched, and a TEXTURE row still wins over both (a textured biome row has no
business also being hatched). The harness asserts the default did not leak, on this legend and on a
channel that predates the kind.

**The hatch has ONE definition.** The row carries `hatch_color` / `hatch_direction` / `edge_color`,
handed over by `_build_temperature_legend` from `MapView.TEMPERATURE_HATCH_COLOR`,
`TEMPERATURE_HATCH_DIRECTION` and `TEMPERATURE_CONTOUR_COLOR` — the very constants
`_draw_lethal_hatch` and `_draw_lethal_contour` draw with, never transcriptions. That is the
`_pasture_ramp_color` rule applied to a drawn mark instead of a ramp, and `map_preview` asserts the
identity rather than trusting it: a transcribed copy renders identically today and drifts the first
time either side is retuned. `OverlayLegend` owns only the swatch's OWN geometry (line spacing,
weights, `clip_contents` to trim the lines to the box at any angle) — a hex at play zoom is several
times that box, so the map's spacing would put one line in it.

**The swatch is `HATCHED_SWATCH_SIZE` (20 px), not `SWATCH_SIZE` (11) — measured, not assumed.**
Rendered at both: at 11 px the 2 px edge takes 4 px of the box and the 1.5 px lines land in ~7 px of
interior as a dither/checkerboard that reads *dirtier* than the solid it replaced. At 20 px they
resolve as unmistakably diagonal with the contour legible around them. It carries the EDGE as well as
the hatch, so the swatch names both marks the map makes. The cost is a slightly ragged swatch column
where this row sits beside flat ones; an illegible swatch is the worse readout. This is the
`OverlayChannels` "render it before trusting it" rule paying out a second time.

**The picker gives this channel the NEUTRAL glyph, correctly.** It is in
`MapView.SPECIAL_PAINT_OVERLAY_KEYS` (it paints through a ramp of its own) and has no
`OVERLAY_COLORS` row, having no single hue — so `_selection_has_map_color` answers false and the
legend button wears `◐` rather than a swatch that could only name one stop of five.

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
- **A rung UNDER WAY WITH NOBODY ON IT drops the percentage.** `BADGE_UNSTAFFED_FORMAT` renders
  `<verb glyph>⚠` in `HudStyle.WARN` — the one rung face that earns amber, an unstaffed commitment
  being the trouble channel's own subject rather than the opportunity `BADGE_READY_COLOR`'s note
  reserves cyan for. **The percentage IS the lie**: a `0%` plate over a build the player staffed with
  nobody is pixel-identical to one they started this turn. The plate still says WHICH rung is
  promised here, and stops saying it is being worked. **The verdict is
  `SourceForecast.build_is_stalled`, and it is ONE function shared with the WORK board's rung slot**
  (`labor-ui.md` → "A build that is not moving does not get to wear a percent"): the two halves —
  *declared and never started* off the meter `RungGates.rung_in_progress` has just resolved, and the
  wire's own rot verdict — were composed here while the board had no fork at all, so the map showed an
  alert the WORK tab did not. Neither surface re-derives it from a crew count or a percentage; a build
  merely PARKED with its keeping covered answers `false` and keeps its number.
  **The build crew is the band's `builders` ROLE row now**
  (`docs/plan_standing_upkeep.md` §2.5) — `_builders_pool_of_marker` reads it off the marker's own
  `labor_assignments` and the renderer credits it to each source that band WORKS, because "nobody is
  building this" is a claim about the source and not about one band's row. **A LOCAL copy of that
  read, deliberately**: a renderer must not depend on the HUD's band-labor model, the rule
  `_labor_assignments_of_marker` beside it already follows. Full rationale, and the other three
  surfaces, in `selection-card.md` → "A build DECLARED with nobody on it is a fourth state". Frames:
  `map_worked_ready` / `map_worked_unstaffed`, an A/B on ONE band with only its `builders` ROW moving.
- **THE RUNG IT REPORTS IS THE LEG IN FLIGHT, NOT THE ENTRY'S DESTINATION** (§2.8). A `sow` ordered
  on untended ground is one queue entry and two legs, so the declared rung's own meter reads 0% for
  as long as the crew is clearing — the plate sat at `▦0%` beside a tile card reading 18%.
  `RungGates.leg_in_progress` re-points `rung_in_progress`'s answer at the first published leg still
  owing work, and this renderer takes it for the same reason it takes `build_is_stalled`: the plate
  and the WORK board are held to ONE verdict, so a leg-aware board beside a destination-bound badge
  is the two-surface disagreement that shared producer exists to stop. The autopsy is
  `band-city-panel.md` → "THE PERCENTAGE IS THE LEG IN FLIGHT'S".
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
