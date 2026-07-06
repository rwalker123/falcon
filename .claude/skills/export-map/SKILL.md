---
name: export-map
description: Export a running game's map to JSON and inspect it by hex coordinate or biome. Use when the user wants to evaluate the live/current map, dump the exact map for tests, or answer questions about specific hexes ("what's at hex 40,26?", "how much desert is there?").
user-invocable: true
argument-hint: [hex x,y | biome NAME | port PORT]
---

# Export & Inspect the Falcon Map

Dump a running game's exact map to JSON, then answer questions about it —
per-hex terrain/biome/elevation and map-wide biome distribution. The map already
lives in every server snapshot; the `export_map` command writes it to disk so it
can be read offline and reused as a test fixture.

## GOLDEN RULE — never read the export file whole

A full export is **large** (~15 MB for an 80×52 map, much larger for big maps).
**Never** `Read`/`cat` the whole file — it will blow up context. **Always** query
it surgically with `jq` and only surface the small result. For any multi-hex or
map-wide question, prefer delegating to the **`map-inspector` agent** (it runs
these same `jq` queries in its own context and returns just the findings).

## Coordinate convention

Hexes are **row-major `(x, y)`** — the same coordinate the Godot inspector shows
as `@x,y`, the same `x`/`y` on `TileState`, and the same index into the export
samples: `sample_index = y * width + x`. So "hex @12,7" is unambiguous across the
client, the file, and tests. No axial/offset conversion.

## Step 1 — Ensure a feature-capable server is running

The export writes JSON into `exports/` **relative to the server's working
directory** (normally the repo root). Two ways to produce a file:

- **From the game UI:** Terrain tab → **Export Map (JSON)** button.
- **From the CLI** (headless, no UI needed):
  ```bash
  cargo xtask command --port 41001 export_map
  ```
  `41001` is the default command port (`simulation_config.json` → `command_bind`);
  pass the actual port if the server was started with an override.

**Caveat — the server must be the rebuilt binary.** A server started before this
feature was added will silently drop `export_map` (it decodes the command proto
and the new field is unknown → the command is discarded). If nothing appears in
`exports/`, the running server predates the feature: rebuild and restart it
(`cargo build -p core_sim --bin server`, or `cargo xtask godot-build` + relaunch
the client stack), or spin up a throwaway server on alternate ports.

If no server is up, start one and wait for `headless server ready` in its log
before sending the command.

## Step 2 — Trigger the export and wait for the file

Sending the command is fire-and-forget; the file appears a fraction of a second
later. **Poll for it — do not check immediately** (that race returns "no file").
Use `find`, not an `exports/*.json` glob — under zsh a non-matching glob throws
`no matches found` and aborts the command:

```bash
newest() { find exports -maxdepth 1 -name '*.json' -exec ls -t {} + 2>/dev/null | head -1; }
before=$(newest)
cargo xtask command --port 41001 export_map
for i in $(seq 1 40); do
  latest=$(newest)
  [ -n "$latest" ] && [ "$latest" != "$before" ] && { echo "exported: $latest"; break; }
done
```

The server also logs `map.export.completed path=… seed=… tick=… width=… height=…`.

## Step 3 — Locate the export to query

```bash
FILE=$(find exports -maxdepth 1 -name '*.json' -exec ls -t {} + 2>/dev/null | head -1)  # newest export
```

Filenames are `map-tick<t>-seed<s>.json`. The `seed` makes the map reproducible
(see "Reproduce for a test" below).

## Step 4 — Query recipes (jq — keep outputs small)

**Metadata / dimensions:**
```bash
jq '{seed, preset, width, height}' "$FILE"
```

**One hex → terrain, tag bitmask, mountain:**
```bash
jq --argjson x 40 --argjson y 26 \
  '.snapshot.terrain as $t | $t.samples[($y*$t.width)+$x]' "$FILE"
# => {"terrain":"ContinentalShelf","tags":5,"mountain_kind":"None","relief_scale":1.0}
```

**One hex → elevation** (elevation is NOT on the terrain sample; it lives in a
separate raster of normalized `u16`s):
```bash
jq --argjson x 40 --argjson y 26 \
  '.snapshot.elevation_overlay as $e
   | {raw: $e.samples[($y*$e.width)+$x], min: $e.min_value, max: $e.max_value, sea_level: $e.sea_level}' "$FILE"
```

**Richer per-hex physical state** (temperature, mass, element) comes from the
`tiles` array rather than the compact terrain raster:
```bash
jq --argjson x 40 --argjson y 26 \
  '.snapshot.tiles[] | select(.x==$x and .y==$y)' "$FILE"
```

**Full biome histogram** (every biome, most common first):
```bash
jq -r '.snapshot.terrain.samples | group_by(.terrain)
       | map({t: .[0].terrain, n: length}) | sort_by(-.n)[]
       | "\(.n)\t\(.t)"' "$FILE"
```

**Every hex of a given biome, as `x,y` coordinates:**
```bash
jq -r --arg biome "Volcano" '.width as $w | .snapshot.terrain.samples
       | to_entries[] | select(.value.terrain==$biome)
       | "\(.key % $w),\((.key / $w) | floor)"' "$FILE"
```

## Tag bitmask decode

The `tags` field is a bitmask (`TerrainTags` in `sim_schema/src/lib.rs`):

| bit | value | tag | | bit | value | tag |
|-----|-------|-----|-|-----|-------|-----|
| 0 | 1 | WATER | | 6 | 64 | POLAR |
| 1 | 2 | FRESHWATER | | 7 | 128 | HIGHLAND |
| 2 | 4 | COASTAL | | 8 | 256 | VOLCANIC |
| 3 | 8 | WETLAND | | 9 | 512 | HAZARDOUS |
| 4 | 16 | FERTILE | | 10 | 1024 | SUBSURFACE |
| 5 | 32 | ARID | | 11 | 2048 | HYDROTHERMAL |

So `tags: 5` = WATER | COASTAL.

## Reproduce a map for a test

The export's `seed` + `preset` regenerate the exact map deterministically. To
turn a live map into a fixture-backed test, follow the pattern in
`integration_tests/tests/map_fixture.rs`: build a headless app with that seed and
preset, then assert on hexes via `MapExport::tile_at(x, y)` (or
`TerrainOverlayState.samples[y*width + x]`). `MapExport` / `encode_map_export_json`
/ `decode_map_export_json` live in `sim_schema` (re-exported from `sim_runtime`).

## When the user gives you hexes to evaluate

For more than a hex or two, or any map-wide question, **spawn the `map-inspector`
agent** with the coordinates/question — it queries the large file in its own
context and returns only the answer, keeping your context clean.
