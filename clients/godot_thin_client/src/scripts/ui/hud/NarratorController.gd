class_name NarratorController
extends RefCounted

## THE SPOKEN NARRATOR (docs/plan_telling_narrator.md) — the controller half. It owns the native
## `NarratorEngine`, the `AudioStreamPlayer` it speaks through, the lines of the page being read and
## the current voice medium. `HudLayer` holds one as `_narrator` and delegates thinly; the
## triggers (a page turn, the opening card, a fork) are signals other controllers emit and the HUD
## wires to `speak` / `stop`.
##
## ## THE NEWEST REQUEST WINS
## `speak` cancels the engine's queue, stops what is playing and starts the new lines: a page turn or
## a card opening cuts the current line off. Lines of one page are requested together (the engine has
## ONE worker, so they finish in order) and played in order with `LINE_PAUSE_SECONDS` between them.
##
## ## INERT UNLESS THIS IS THE GAME
## The engine is configured by `activate` (or else the first `speak`), and only when the client booted as the game
## (`Main.tscn` is the current scene) on a real display. Every harness instances `HudLayer` under its
## own scene or headless, so no harness ever configures — let alone loads — the model.
## Anything missing (no model, no espeak-ng, no native class) turns narration off with ONE log line.

var _host: Node = null
## This HUD's handle on the process-wide engine (below), set once it exists.
var _engine: RefCounted = null
var _player: AudioStreamPlayer = null
## This controller has its engine handle and its player.
var _configured: bool = false

# ---- process-wide state ----------------------------------------------------
## ONE engine per PROCESS: a Main.tscn reload, Abandon → New Game or a world change builds a new HUD
## (and a new controller) but must not load the model again. `NarratorEngine` is a RefCounted, so this
## static is a strong reference — never freed early, and nothing else needs to free it.
static var _shared_engine: RefCounted = null
## A permanent verdict (no native class, no model, no espeak-ng, a bad path), logged once.
static var _shared_dead: bool = false
## Whichever controller spoke last owns the engine's results: only it polls, so a controller left over
## from a previous HUD can never drain a line a newer one asked for. (An instance id, not a reference,
## so a dead HUD's controller is not kept alive by it.)
static var _results_owner_id: int = 0
var _medium_id: String = HudNarratorVocab.MEDIUM_ORAL

# ---- the page being read ---------------------------------------------------
## Request ids in reading order, and each one's finished stream (null for a failed line).
var _pending_ids: Array[int] = []
var _streams: Dictionary = {}
var _next_index: int = 0
var _is_speaking: bool = false
## The source whose `speak` currently owns playback (`HudNarratorVocab.SOURCE_*`).
var _owner: String = HudNarratorVocab.SOURCE_NONE
var _pause_left: float = 0.0

func setup(host: Node) -> void:
	_host = host

## The narrator's medium picks the voice of the NEXT request; a line already playing is unaffected.
func set_voice_medium(medium_id: String) -> void:
	_medium_id = medium_id.strip_edges()

## Read `lines` aloud, cutting off anything already being read. Disabled narration only stops.
func speak(lines: PackedStringArray, source: String = HudNarratorVocab.SOURCE_NONE) -> void:
	stop()
	_owner = source
	if not ClientSettings.narration_enabled:
		return
	if not _ensure_configured():
		return
	if _engine_load_failed():
		return
	# A newer speaker preempts whoever spoke before, from any HUD.
	_engine.call("cancel_all")
	_results_owner_id = get_instance_id()
	var row: Dictionary = HudNarratorVocab.VOICES.get(
		_medium_id, HudNarratorVocab.VOICES[HudNarratorVocab.MEDIUM_ORAL])
	var voice := String(row[HudNarratorVocab.KEY_VOICE])
	var speed := float(row[HudNarratorVocab.KEY_SPEED])
	for line in lines:
		var text := String(line).strip_edges()
		if text.is_empty():
			continue
		var id := int(_engine.call("request", text, voice, speed))
		if id != HudNarratorVocab.NO_REQUEST:
			_pending_ids.append(id)
	_next_index = 0
	_pause_left = 0.0

## A source's own stop: silences narration only while that source still owns playback, so a fork
## card closing never cuts off a Telling page being read.
func stop_source(source: String) -> void:
	if source == _owner:
		stop()

## Load the model as soon as narration can run here, so the first line (the opening card's) is not
## delayed by it. Called by the HUD once it is in the tree; inert under every harness, because
## `_ensure_configured` never configures outside the game scene.
func activate() -> void:
	if ClientSettings.narration_enabled:
		_ensure_configured()

## Stop reading and drop everything queued.
func stop() -> void:
	if _engine != null and (_results_owner_id == 0 or _results_owner_id == get_instance_id()):
		_engine.call("cancel_all")
	if _player != null and _player.playing:
		_player.stop()
	_pending_ids.clear()
	_streams.clear()
	_next_index = 0
	_is_speaking = false
	_pause_left = 0.0
	_owner = HudNarratorVocab.SOURCE_NONE

## Driven every frame by the HUD: collects finished lines and starts the next one when due.
func process(delta: float) -> void:
	if _engine != null and _engine_load_failed():
		return
	if _engine == null or _results_owner_id != get_instance_id():
		return
	if not ClientSettings.narration_enabled:
		if not _pending_ids.is_empty() or _is_speaking:
			stop()
		return
	_collect_finished()
	_apply_volume()
	if _is_speaking:
		return
	if _pause_left > 0.0:
		_pause_left -= delta
		return
	_play_next_if_ready()

# ---- internals -------------------------------------------------------------

## True once the shared engine's model failed to load. The first sighting logs the error ONCE, marks
## the process-wide verdict dead and drops the engine, so nothing is requested again this process;
## every controller (each holds the same engine) then silences its own playback and queue.
func _engine_load_failed() -> bool:
	if _engine == null:
		return false
	if _shared_engine == _engine and bool(_engine.call("is_failed")):
		push_warning(HudNarratorVocab.LOG_LOAD_FAILED_FORMAT % String(_engine.call("load_error")))
		_shared_dead = true
		_shared_engine = null
	if _shared_engine == _engine:
		return false
	stop()
	_engine = null
	return true

func _collect_finished() -> void:
	for result_variant in (_engine.call("poll") as Array):
		var result: Dictionary = result_variant
		var id := int(result.get(HudNarratorVocab.ID_KEY, HudNarratorVocab.NO_REQUEST))
		if not _pending_ids.has(id):
			continue
		if result.has(HudNarratorVocab.ERROR_KEY):
			push_warning(HudNarratorVocab.LOG_ERROR_FORMAT % String(result[HudNarratorVocab.ERROR_KEY]))
			_streams[id] = null
			continue
		var stream := AudioStreamWAV.new()
		stream.format = AudioStreamWAV.FORMAT_16_BITS
		stream.mix_rate = int(result.get(HudNarratorVocab.SAMPLE_RATE_KEY, 0))
		stream.stereo = false
		stream.data = result[HudNarratorVocab.PCM_KEY]
		_streams[id] = stream

func _play_next_if_ready() -> void:
	# A failed line is skipped, so one bad line never silences the page.
	while _next_index < _pending_ids.size():
		var id := _pending_ids[_next_index]
		if not _streams.has(id):
			return
		_next_index += 1
		var stream: AudioStream = _streams[id]
		if stream == null:
			continue
		_player.stream = stream
		_player.play()
		_is_speaking = true
		return

func _on_line_finished() -> void:
	_is_speaking = false
	_pause_left = HudNarratorVocab.LINE_PAUSE_SECONDS

func _apply_volume() -> void:
	if _player != null:
		_player.volume_db = linear_to_db(ClientSettings.narration_volume)

## Take the shared engine and build this HUD's player, once. False when narration cannot run here.
func _ensure_configured() -> bool:
	if _configured:
		return true
	if _host == null:
		return false
	var engine := _shared_engine_for(_host.get_tree())
	if engine == null:
		return false
	_engine = engine
	_player = AudioStreamPlayer.new()
	_player.name = HudNarratorVocab.PLAYER_NODE_NAME
	_player.finished.connect(_on_line_finished)
	_host.add_child(_player)
	_configured = true
	return true

## Configure and warm the process-wide engine if narration can run here. Called by the landing screen
## (while the player is still in the menu) and by the HUD; a no-op unless narration is enabled, on a
## real display, in the landing or game scene.
static func warm_up(tree: SceneTree) -> void:
	if ClientSettings.narration_enabled:
		_shared_engine_for(tree)

## The one engine, created and warmed on first use. Null while narration cannot run — either not
## here (a harness, headless, the scene not current yet: asked again later) or never (a verdict).
static func _shared_engine_for(tree: SceneTree) -> RefCounted:
	if _shared_engine != null:
		return _shared_engine
	if _shared_dead or not _is_a_narration_scene(tree):
		return null
	_shared_dead = true
	if not ClassDB.class_exists(HudNarratorVocab.ENGINE_CLASS):
		_log_off(HudNarratorVocab.REASON_NO_ENGINE)
		return null
	var model_dir := _model_dir()
	var model := _model_file(model_dir)
	var voices := model_dir.path_join(HudNarratorVocab.VOICES_FILE)
	if model.is_empty() or not FileAccess.file_exists(voices):
		_log_off(HudNarratorVocab.REASON_NO_MODEL % [
			HudNarratorVocab.MODEL_FILE_INT8, HudNarratorVocab.VOICES_FILE])
		return null
	var espeak_lib := _espeak_lib()
	var espeak_data := _espeak_data(espeak_lib)
	if espeak_lib.is_empty() or espeak_data.is_empty():
		_log_off(HudNarratorVocab.REASON_NO_ESPEAK % [
			HudNarratorVocab.ENV_ESPEAK_LIB, HudNarratorVocab.ENV_ESPEAK_DATA])
		return null
	var engine: RefCounted = ClassDB.instantiate(HudNarratorVocab.ENGINE_CLASS)
	if not bool(engine.call("configure", model, voices, espeak_lib, espeak_data)):
		_log_off(HudNarratorVocab.REASON_CONFIGURE_FAILED)
		return null
	_shared_dead = false
	_shared_engine = engine
	engine.call("warm_up")
	return engine

## True only on a real display, in the menu or the game proper. Every harness runs under its own
## scene (or headless), so none of them can pass — and none can trigger the model load.
static func _is_a_narration_scene(tree: SceneTree) -> bool:
	if DisplayServer.get_name() == "headless":
		return false
	if tree == null or tree.current_scene == null:
		return false
	return HudNarratorVocab.NARRATION_SCENE_PATHS.has(tree.current_scene.scene_file_path)

static func _model_dir() -> String:
	var env := OS.get_environment(HudNarratorVocab.ENV_MODEL_DIR)
	if not env.is_empty():
		return env
	return ProjectSettings.globalize_path(HudNarratorVocab.FETCHED_MODEL_DIR)

## The fp32 model if present (it sounds better; the fetch script's `--fp32` drops it beside int8),
## else the int8 one, else "".
static func _model_file(dir: String) -> String:
	for file_name in [HudNarratorVocab.MODEL_FILE_FP32, HudNarratorVocab.MODEL_FILE_INT8]:
		var path := dir.path_join(file_name)
		if FileAccess.file_exists(path):
			return path
	return ""

static func _espeak_lib() -> String:
	var env := OS.get_environment(HudNarratorVocab.ENV_ESPEAK_LIB)
	if not env.is_empty():
		return env
	for candidate in HudNarratorVocab.ESPEAK_LIB_CANDIDATES:
		if FileAccess.file_exists(candidate):
			return candidate
	return ""

static func _espeak_data(lib: String) -> String:
	var env := OS.get_environment(HudNarratorVocab.ENV_ESPEAK_DATA)
	if not env.is_empty():
		return env
	if lib.is_empty():
		return ""
	var prefix := lib.get_base_dir().get_base_dir()
	var data := prefix.path_join(HudNarratorVocab.ESPEAK_DATA_RELATIVE)
	return data if DirAccess.dir_exists_absolute(data) else ""

static func _log_off(reason: String) -> void:
	print(HudNarratorVocab.LOG_OFF_FORMAT % reason)
