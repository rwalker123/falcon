---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/NarratorController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_narrator_vocab.gd"
  - "clients/godot_thin_client/native/src/narrator/**"
  - "clients/godot_thin_client/native/tests/fixtures/phoneme_fixture.json"
  - "scripts/fetch_narrator_model.sh"
  - "clients/godot_thin_client/src/scripts/ui/LandingScreen.gd"
---

# The spoken narrator

The Telling's lines are read aloud by Kokoro-82M, a local neural voice, and **the voice follows the
narrator's medium** (oral → painted → written). Design and its decisions: `docs/plan_telling_narrator.md`.
The page/card seams it listens to are `telling-panel.md`.

## Key scripts

| Script | Purpose |
|--------|---------|
| `native/src/narrator/engine.rs` | `NarratorEngine` (`#[godot_api]`): `configure(model, voices, espeak_lib, espeak_data) -> bool` (paths only), `warm_up()` (load the model on the worker now, non-blocking), `is_ready()`, `request(text, voice, speed) -> id` (`-1` unconfigured), `poll() -> [{id, pcm16, sample_rate} \| {id, error}]`, `cancel_all()` |
| `native/src/narrator/mod.rs` | The ONE worker thread, the request queue, the generation counter `cancel_all` bumps, the 64-entry cache keyed (voice, speed bits, text); the parity, synth and cache tests |
| `native/src/narrator/espeak.rs` | espeak-ng `dlopen`ed via `libloading`, IPA phonemes, and a port of phonemizer's punctuation preserve/restore |
| `native/src/narrator/kokoro.rs` · `tokenizer.rs` · `vocab.json` | The ORT session, the npz voices (style row = token count − 1), the 510-phoneme chunker, trim and inter-batch pauses, PCM16 out |
| `ui/hud/NarratorController.gd` | `RefCounted` controller held by `HudLayer` as `_narrator`: the process-wide engine (static), its own `AudioStreamPlayer`, the page's line queue, the medium and the owning SOURCE. `speak(lines, source)` / `stop_source(source)` / `stop()` / `activate()` / `process(delta)`, and the static `warm_up(tree)` the landing screen calls |
| `ui/hud/hud_narrator_vocab.gd` | `HudNarratorVocab`: the medium → `{voice, speed}` table with its `oral` fallback, the `SOURCE_*` ids, `LINE_PAUSE_SECONDS`, the path candidates and env var names, the one-shot log strings |
| `scripts/fetch_narrator_model.sh` | Downloads the int8 model (default) or fp32 (`--fp32`) plus `voices-v1.0.bin` into the gitignored `clients/godot_thin_client/narrator_model/`; idempotent |

## espeak-ng is LOADED, never linked

Kokoro needs IPA phonemes and espeak-ng supplies them, and **espeak-ng is GPL-3**. So it is `dlopen`ed
at runtime from wherever the machine has it (`NARRATOR_ESPEAK_LIB` / `NARRATOR_ESPEAK_DATA`, else the
Homebrew and distro paths in `ESPEAK_LIB_CANDIDATES`, data at `<prefix>/share/espeak-ng-data`). It is
never a build dependency, never statically linked, never bundled. **Absent, narration is off and the
game is otherwise untouched** — one log line, no retry. The shippable replacement is a build-time
pronunciation lexicon, issue #906.

**Phonemes must match the Python reference bit for bit.** Kokoro was trained on phonemizer's output,
so a "close enough" phonemizer changes how the voice sounds. `phoneme_fixture.json` holds 16 real
tellings × {en-us, en-gb} with kokoro-onnx's phonemes and tokens, and the parity test asserts an
exact match (it skips, saying why, where espeak-ng is missing). The fixture was generated against
espeak-ng 1.52.0; a different espeak version can legitimately move a phoneme, so a parity failure
after an espeak upgrade means **regenerate the fixture from kokoro-onnx**, not loosen the test.
Voice language follows the voice id's first letter: `a*` → `en-us`, `b*` → `en-gb`.

## One voice per MEDIUM

The voice table is keyed by `mediumId` and falls back to the `oral` row for an unknown id — the
`TellingPanel.MEDIUM_STYLES` rule, because `mediumId` is free-form on the wire. A medium change
affects the NEXT request; a line already playing finishes in the voice it started in. The table is a
lever judged by ear, not a fact the code depends on.

## What is spoken, and when

**The narrator reads what the player is looking at, at the moment it comes into view.** That is the
whole rule, and every trigger is an instance of it:

- **The Telling panel** emits `page_shown(lines)` on an oral beat's arrival (oral pins to newest, so
  arrival IS showing), on `leaf(±1)`, and on `reveal_newest()` when it changes the page. **Painted and
  written never speak on arrival** — the book does not turn its own page there, and a voice reading a
  line the player cannot see breaks that. Never on a full-snapshot backfill, the first paint, a
  medium-change re-render, `reset()`, `debug_jump_to`, or while collapsed; collapsing stops it.
- **The opening card** speaks its turn-0 lines when it shows, and **the hand-off to the outfitting
  card does NOT stop them** — they keep playing while the player picks kits. Only a newer `speak`
  or a world reset cuts them off. (Stopping on hand-off silenced the opening entirely in playtest:
  the player had clicked through before the first line was ready.) A first oral beat into an EMPTY
  panel is treated as initial population and not spoken — the tick-0 lines are the opening card's.
- **The fork card** speaks its narration in the active register on open, and again on a register
  toggle. Never the choices. Its echo line arrives as a `narrative_fork` feed entry and is the panel's.
- **Never the gloss** — the sampled numbers are proof for the reader, not prose.

**The newest request wins, and a source stops only its own narration.** Any `speak` cancels the
engine queue and stops playback before starting its own lines. A source's stop (`stop_source`) is a
no-op unless that source still owns playback, so a fork card closing cannot cut off a Telling page
the panel started after it. Lines of one page are requested together — one worker, so they finish
in order — and played with `LINE_PAUSE_SECONDS` between them; a failed line is skipped, never fatal.

## Inert under every harness — by scene, not by flag

The ui_preview and other harnesses drive the LIVE TellingPanel paths, so a `page_shown` fires under
them. The engine is therefore configured only on a **non-headless display whose current scene is
in `HudNarratorVocab.NARRATION_SCENE_PATHS`** — `Main.tscn` and `LandingScreen.tscn`. Every harness
runs under its own scene or headless, and none instances `LandingScreen`, so none configures the
engine, let alone loads 88 MB of model. "Not one of those scenes yet" is not a verdict — the scene
may simply not be current yet — so it is re-asked; a missing model, espeak or native class IS a
verdict, logged once and never retried.

## One engine per PROCESS, warmed up in the menu

Loading the model takes ~2.4 s warm and several seconds on a cold OS page cache; a line after that
~0.5 s. The opening card's lines are the first narration a player hears, and in playtest a load
paid at game start made them ~5 s late.

- **The engine and the permanent verdict are process-wide `static var`s on `NarratorController`.**
  A `Main.tscn` reload, Abandon → New Game and an in-session world change all reuse the loaded
  model. Only the `AudioStreamPlayer` is per HUD.
- **The last controller to `speak` owns the engine's results** (a static instance id, not a
  reference, so it keeps no freed HUD alive). Only the owner polls or cancels, so an old HUD's
  controller cannot drain a newer one's lines.
- **`LandingScreen._ready` warms it** (`NarratorController.warm_up(tree)`, deferred), so the load
  runs while the player is in the menu and is done before a world generates. `HudLayer._ready`'s
  `activate()` is the backstop for a direct `Main.tscn` launch.
- The worker's warm flag is separate from the request queue, so `cancel_all` / `stop()` cannot drop it.
