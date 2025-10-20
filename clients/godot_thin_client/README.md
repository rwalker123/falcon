# Godot Thin Client Spike

This prototype evaluates Godot 4 as the graphical UX shell for Shadow-Scale. It renders a tactical
slice using mock snapshots that emulate the headless simulation stream, updates overlay heatmaps,
and sketches unit path previews.

## Requirements

- Godot 4.2 or newer (see install instructions below)
- The Shadow-Scale workspace (repo root) so relative paths resolve (`project.godot` lives under
  `clients/godot_thin_client`)
- FlatBuffers compiler (`flatc`) on your PATH so the native extension can generate schema bindings

### Install Godot 4

macOS (Homebrew):

```bash
brew install --cask godot
```

Windows/Linux: download the 64-bit editor from the official site and unzip/install:
<https://godotengine.org/download>

Verify the CLI is available:

```bash
godot4 --version
```

## Running the Spike

1. Build the native extension (the helper copies the artifact into `res://native/bin/<platform>`):

   ```bash
   brew install flatbuffers # macOS; install flatc via your package manager on other platforms
   cargo xtask godot-build
   ```

   The command wraps `cargo build --release -p shadow_scale_godot` and syncs the resulting dynamic
   library into the Godot project.

2. (Optional) Run the headless simulation server to produce FlatBuffers snapshots:

   ```bash
   RUST_LOG=shadow_scale::server=info,core_sim=info cargo run -p core_sim --bin server
   ```

   The default FlatBuffers socket is `127.0.0.1:41002` (`SimulationConfig::snapshot_flat_bind`). When
   `STREAM_ENABLED` is toggled to `true` in `src/scripts/Main.gd`, the client will subscribe to that
   stream; otherwise it replays the bundled mock timeline.

2. From the repo root, launch the editor pointing at this project:

   ```bash
   godot4 --path clients/godot_thin_client
   ```

3. In the editor, run the main scene (`src/Main.tscn`).
4. If streaming is enabled and the server is running, the scene connects automatically and streams live
   snapshots. On failure the client logs a warning and falls back to the bundled mock timeline
   (tick cadence ~1.5s). Use the default input
   actions to scrub:
   - `→` (`ui_right`) advance to the next snapshot.
   - `←` (`ui_left`) rewind to the previous snapshot.

## What the Scene Demonstrates

- **Layered map overlays:** each tile blends logistics throughput (blue) and sentiment pressure (red)
  sourced from the mock snapshot values.
- **Unit markers & paths:** units are color-coded per faction and show their active command path as a
  polyline, matching anticipated order previews.
- **Turn HUD:** top-left panel surfaces key metrics (turn, unit count, overlay averages) to validate HUD
  layering and typography.

## Extending the Spike

- Swap `mock_snapshots.json` with captured frames from the headless sim (convert to JSON prior to
  ingestion) to inspect real data offline.
- Streaming is disabled by default (`STREAM_ENABLED = false`). Set it to `true` to consume live
  FlatBuffers snapshots from `127.0.0.1:41002`.
- Integrate the shared scripting facade once defined so scripted panels can subscribe to the same
  snapshot events.

## Notes for Evaluation

- Record frame times (`Debugger → Monitors → Profiler`) while zooming/panning and during unit-rich
  scenes.
- Track developer workflow observations (scene organization, script hot-reload, asset iteration) to feed
  the comparison memo.
