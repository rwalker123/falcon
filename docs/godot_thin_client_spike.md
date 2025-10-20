# Godot Thin Client Spike Notes

## Goal
Evaluate Godot 4 as the primary UX host by rendering a tactical slice of Shadow-Scale using mock
simulation snapshots. Focus on map overlays, unit visualisation, and rapid designer iteration.

## Artifacts

- Project path: `clients/godot_thin_client`
- Main scene: `src/Main.tscn`
- Mock data: `src/data/mock_snapshots.json`

## Usage

1. Install Godot 4.2+
2. Build the native extension via `cargo xtask godot-build`; the helper copies the library into
   `res://native/bin/<platform>/` as referenced by `native/shadow_scale_godot.gdextension`.
3. (Optional) Run the headless sim so the client can attach to the FlatBuffers stream on
   `SimulationConfig::snapshot_flat_bind` (default `127.0.0.1:41002`).
4. Open the project (`project.godot`) and run `Main.tscn`. Enable streaming via
   `STREAM_ENABLED = true` in `Main.gd`; otherwise the mock timeline plays back (scrub with `← / →`).

## Evaluation Checklist

- Frame pacing while panning/zooming (Profiler)
- Overlay blending clarity for logistics vs sentiment
- Unit/path readability at multiple zoom levels
- Script hot-reload ergonomics and scene organisation
- Streaming stability, reconnect behaviour, and latency when consuming live FlatBuffers snapshots

## Next Steps

- Flesh out FlatBuffers decoding so overlays, units, and orders expose real simulation data
- Integrate shared scripting capability layer once defined
- Capture findings + metrics in the frontend evaluation memo (see `TASKS.md`)
