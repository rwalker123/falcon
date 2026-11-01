class_name HudNarratorVocab
extends RefCounted

## The spoken narrator's vocabulary (docs/plan_telling_narrator.md) — consts only, zero vars.
## Which voice speaks for which medium, the pause between lines, and where the model + espeak-ng
## are looked for. The voice table is a LEVER judged by ear in playtest, not a gate on the build.

# ---- voice per medium ------------------------------------------------------
## A table with a fallback row, never a `match` (`TellingPanel.MEDIUM_STYLES`' rule): `mediumId` is
## free-form on the wire, so an unknown id speaks as the first rung.
const MEDIUM_ORAL := "oral"
const KEY_VOICE := "voice"
const KEY_SPEED := "speed"
const VOICES := {
	"oral": {KEY_VOICE: "bm_george", KEY_SPEED: 0.85},
	"painted": {KEY_VOICE: "am_michael", KEY_SPEED: 0.90},
	"written": {KEY_VOICE: "af_heart", KEY_SPEED: 0.95},
}

## Who asked for the voice. A source may only stop narration it started: its stop is a no-op once
## another source's `speak` has taken over (the newest request still preempts any other).
const SOURCE_NONE := ""
const SOURCE_PANEL := "panel"
const SOURCE_OPENING := "opening"
const SOURCE_FORK := "fork"

## Silence between the lines of one page.
const LINE_PAUSE_SECONDS := 0.6

# ---- the engine's input ----------------------------------------------------
const SAMPLE_RATE_KEY := "sample_rate"
const PCM_KEY := "pcm16"
const ERROR_KEY := "error"
const ID_KEY := "id"
## `NarratorEngine.request` answers this when narration is not configured.
const NO_REQUEST := -1

# ---- where the files are ---------------------------------------------------
const ENV_MODEL_DIR := "NARRATOR_MODEL_DIR"
const ENV_ESPEAK_LIB := "NARRATOR_ESPEAK_LIB"
const ENV_ESPEAK_DATA := "NARRATOR_ESPEAK_DATA"
## The directory `scripts/fetch_narrator_model.sh` fills, relative to the project root.
const FETCHED_MODEL_DIR := "res://narrator_model"
const MODEL_FILE_INT8 := "kokoro-v1.0.int8.onnx"
const MODEL_FILE_FP32 := "kokoro-v1.0.onnx"
const VOICES_FILE := "voices-v1.0.bin"
const ESPEAK_LIB_CANDIDATES := [
	"/opt/homebrew/lib/libespeak-ng.dylib",
	"/usr/local/lib/libespeak-ng.dylib",
	"/usr/lib/x86_64-linux-gnu/libespeak-ng.so.1",
	"/usr/lib/libespeak-ng.so.1",
]
## espeak's data sits at `<prefix>/share/espeak-ng-data`, the prefix being two levels above its lib.
const ESPEAK_DATA_RELATIVE := "share/espeak-ng-data"

# ---- the native class + the scene it may speak in --------------------------
const ENGINE_CLASS := "NarratorEngine"
## Narration runs only in the game proper: every harness instances HudLayer under its own scene.
const GAME_SCENE_PATH := "res://src/Main.tscn"
## The menu the game boots into: the model loads there, while the player is still choosing, so it is
## ready by the time the opening card shows. Narration (and its load) is allowed in these two scenes only.
const LANDING_SCENE_PATH := "res://src/ui/LandingScreen.tscn"
const NARRATION_SCENE_PATHS := [GAME_SCENE_PATH, LANDING_SCENE_PATH]
const PLAYER_NODE_NAME := "NarratorPlayer"

# ---- log lines (one each, never repeated) ----------------------------------
const LOG_OFF_FORMAT := "[narrator] narration is off: %s"
const REASON_NO_ENGINE := "the native extension has no NarratorEngine"
const REASON_NO_MODEL := "no model directory with %s / %s (run scripts/fetch_narrator_model.sh)"
const REASON_NO_ESPEAK := "espeak-ng not found (set %s / %s)"
const REASON_CONFIGURE_FAILED := "a model, voices or espeak path does not exist"
const LOG_LOAD_FAILED_FORMAT := "[narrator] the model failed to load: %s"
const LOG_ERROR_FORMAT := "[narrator] synthesis failed: %s"
