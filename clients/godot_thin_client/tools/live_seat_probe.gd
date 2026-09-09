extends Node

## **THE ONE HARNESS THAT NEEDS A SERVER — the seated command link, end to end, against a live sim.**
##
## ## What it needs
##
## A **running server** on a port block this process can see, and nothing else. It stands the REAL
## `Main.tscn` up as a child and lets `Main` do its own boot: resolve the endpoints, claim the seat,
## greet the snapshot stream with the token, and send the dev-default `new_game`. So the server may be
## freshly booted and idle — the probe asks for its own world.
##
##     # server only, on this checkout's own port block (never the default 41000-41003)
##     scripts/run_stack.sh --server-only --port-base 41040
##     # then, in another shell, with the SAME block:
##     godot --headless --path clients/godot_thin_client --import
##     env STREAM_ENABLED=true STREAM_HOST=127.0.0.1 STREAM_PORT=41042 \
##         COMMAND_HOST=127.0.0.1 COMMAND_PORT=41041 COMMAND_PROTO_PORT=41041 \
##         scripts/preview.sh res://tools/live_seat_probe.tscn
##     echo $?   # 0 = pass. THE STATUS IS THE VERDICT (`.claude/rules/client/test-harnesses.md`)
##
## ⛔ **Through `scripts/preview.sh`, and never with `--headless`.** It stands up the whole client
## scene — `MapView` and its terrain shaders included — so it needs a real renderer, and the wrapper is
## what stops the window stealing the keyboard from another session. The env vars carry the port block;
## the client's own precedence is env → ports file → default, so passing them is what keeps a probe run
## off the default block.
##
## ## What it proves, and why a rendered fixture cannot
##
## Three things, in the order a session does them, each an assertion with the exit status behind it:
##
##   1. **The seat handshake.** The claim is granted, it carries a token, and per-seat frames addressed
##      by that token arrive — which is what `_world_revealed` means.
##   2. **One faction-bearing command.** `split_band` is sent through `Main._on_hud_split_band`, the
##      same handler the HUD's signal reaches, and the world comes back holding one more band. The
##      server takes the acting faction from the seat the *connection* holds, so an unseated link
##      answers this with `command.rejected=not_this_connections_seat` and no new band.
##   3. **One turn submission.** `order <faction> ready` through `Main._on_hud_next_turn`, and the turn
##      number advances.
##
## **This path broke twice on one branch** — a per-command connection that killed the seat, and then a
## missing stream greeting — and each time the only thing that caught it was a real client against a
## real server. `ui_preview` cannot: it feeds canned fixtures to the HUD and opens no socket. macOS
## blocks synthetic clicks, so driving `Main`'s handlers by name is the only route to this path at all.
##
## **It is deliberately three assertions wide.** The seat, one command that names a faction, one turn.
## Anything else about gameplay belongs in `core_sim`'s tests, where it costs no window.

## The real client, unmodified. Instanced as a child rather than made the scene root so this node
## keeps its own `_ready` (and its watchdog sibling) while `Main` boots exactly as it does in play.
const MAIN_SCENE := preload("res://src/Main.tscn")

## Prefix on every line, so a probe line is greppable and cannot be mistaken for engine output.
const TAG := "live_seat_probe"
## The failure token, spelled as every other harness spells it (`<name>: FAIL — <text>`).
const FAIL_FORMAT := "%s: FAIL — %s"

const EXIT_OK := 0
const EXIT_FAILED := 1

## **HOW LONG THE WORLD MAY TAKE TO ARRIVE.** It covers a cold server: the connect, the claim, the
## `new_game` this probe's boot sends, a full worldgen and the first full frame. Sized off the client's
## own `Main.NEW_GAME_ANSWER_TIMEOUT` (30 s, itself ~7x the measured worst-case worldgen) plus room for
## one of its re-sends, because a re-sent `new_game` is a legitimate way for this to succeed.
const REVEAL_TIMEOUT_MSEC := 75_000

## **HOW LONG A COMMAND'S EFFECT MAY TAKE TO COME BACK.** The server re-captures and broadcasts
## immediately after applying a command, so this is a round trip and not a turn: generous, but nowhere
## near the reveal budget, because a command that has not landed in this long has not landed.
const COMMAND_EFFECT_TIMEOUT_MSEC := 15_000

## **HOW LONG A SUBMITTED TURN MAY TAKE TO RESOLVE.** One turn of the whole sim plus the frame that
## reports it. Vacant seats never hold a turn, so with one seated player this is the resolve alone.
const TURN_TIMEOUT_MSEC := 30_000

## Polled, not awaited on a signal: `Main` exposes no "world revealed" signal, and a poll over
## `process_frame` is what the rest of the tools do (`turn_orb_click_probe`).
const POLL_FRAMES := 1

## The snapshot section every band rides in, and the fields this probe reads off a band row. Spelled
## from `native/src/dict/population.rs`, which is the contract — a rename there must break here.
const POPULATIONS_KEY := "populations"
const BAND_ID_KEY := "band_id"
const FACTION_KEY := "faction"
const IS_EXPEDITION_KEY := "is_expedition"
const WORKING_AGE_KEY := "working_age"
const SPLIT_MIN_WORKERS_KEY := "founding_min_workers"
const SPLIT_PARENT_MIN_WORKERS_KEY := "founding_parent_min_workers"
const TURN_KEY := "turn"
const NO_TURN := -1

var _main: Node = null
var _failures: int = 0
var _watchdog: Node = null


func _ready() -> void:
	_watchdog = get_node_or_null("Watchdog")
	print("%s: booting the real Main.tscn against a live server." % TAG)
	_main = MAIN_SCENE.instantiate()
	add_child(_main)
	await _run()
	_finish()


## **THE ONE FAILURE SINK**, holding this file's only `push_error` — the same contract the render
## harnesses keep, so the tally can never drift from what was printed.
func _fail(message: String) -> void:
	_failures += 1
	push_error(FAIL_FORMAT % [TAG, message])


## **THE ONE EXIT**, so the status is derived in exactly one place. Disarms the hang guard on the way
## out: a slow shutdown is not a stall.
func _finish() -> void:
	if _watchdog != null and _watchdog.has_method("disarm"):
		_watchdog.call("disarm")
	if _failures > 0:
		print("%s: %d failure(s)." % [TAG, _failures])
		get_tree().quit(EXIT_FAILED)
		return
	print("%s: PASS — seat granted and streaming, split_band obeyed, turn submitted and resolved." % TAG)
	get_tree().quit(EXIT_OK)


func _note_progress() -> void:
	if _watchdog != null and _watchdog.has_method("note_progress"):
		_watchdog.call("note_progress")


## The last frame the loader published, i.e. what the client currently believes the world is.
func _snapshot() -> Dictionary:
	var loader: Object = _main.get("snapshot_loader")
	if loader == null:
		return {}
	var last: Variant = loader.get("last_stream_snapshot")
	return last if last is Dictionary else {}


## The player's own RESIDENT bands — parties are excluded exactly as `Hud.update_band_alerts` excludes
## them, because a split makes a band and a detached party would flatter the count.
func _player_bands(snapshot: Dictionary) -> Array:
	var out: Array = []
	var rows: Variant = snapshot.get(POPULATIONS_KEY, [])
	if not (rows is Array):
		return out
	for row in rows:
		if not (row is Dictionary):
			continue
		var band: Dictionary = row
		if int(band.get(FACTION_KEY, HudConst.NO_FACTION_ID)) != HudConst.PLAYER_FACTION_ID:
			continue
		if bool(band.get(IS_EXPEDITION_KEY, false)):
			continue
		out.append(band)
	return out


## Poll `predicate` until it answers true or `timeout_msec` elapses. Wall clock, so a slow frame rate
## cannot shorten a budget the way a frame count would.
func _await_until(what: String, timeout_msec: int, predicate: Callable) -> bool:
	var deadline := Time.get_ticks_msec() + timeout_msec
	_note_progress()
	while Time.get_ticks_msec() < deadline:
		if bool(predicate.call()):
			return true
		for _i in range(POLL_FRAMES):
			await get_tree().process_frame
	print("%s: timed out after %d s waiting for %s." % [TAG, timeout_msec / 1000, what])
	return false


func _run() -> void:
	# ---- 1. THE SEAT HANDSHAKE -----------------------------------------------------------------
	var revealed: bool = await _await_until("the world to arrive", REVEAL_TIMEOUT_MSEC,
		func() -> bool: return bool(_main.get("_world_revealed")))
	var seat: Object = _main.get("seat_claim")
	var seated := seat != null and bool(seat.get("seated_now"))
	var token := int(seat.get("seat_token")) if seat != null else SnapshotStream.NO_SEAT_TOKEN
	# **THE TOKEN IS A SECRET AND A HARNESS LOG IS A LOG** — whether one was granted is the whole of
	# what this assertion needs, and printing the value here would reopen the hole every client-side
	# log line was just redacted to close (`SnapshotStream.SEAT_TOKEN_LOG_REDACTION`).
	print("%s: seated=%s token_granted=%s world_revealed=%s" % [
		TAG, seated, token != SnapshotStream.NO_SEAT_TOKEN, revealed])
	if not seated:
		var refusal := String(seat.get("refusal")) if seat != null else "no seat seam"
		_fail("the seat was never granted (%s) — every faction-bearing command would be refused" % refusal)
		return
	if token == SnapshotStream.NO_SEAT_TOKEN:
		_fail("the seat was granted with no token, so the snapshot stream has nothing to greet with")
		return
	if not revealed:
		_fail("no full snapshot for this seat ever arrived — the seat is held but frames are not reaching this stream socket")
		return

	var before := _snapshot()
	var bands := _player_bands(before)
	var turn_before := int(before.get(TURN_KEY, NO_TURN))
	print("%s: turn=%d player_bands=%d" % [TAG, turn_before, bands.size()])
	if bands.is_empty():
		_fail("the revealed world holds no band for faction %d, so there is nothing to command" % HudConst.PLAYER_FACTION_ID)
		return

	# ---- 2. ONE FACTION-BEARING COMMAND --------------------------------------------------------
	var band: Dictionary = bands[0]
	var band_id := int(band.get(BAND_ID_KEY, HudConst.NO_BAND_ID))
	# The ask is the sim's OWN floor, read off the cohort rather than copied from `expedition_config`
	# — the same numbers the compose sheet states — so a retuned floor moves this probe with it.
	var workers := int(band.get(SPLIT_MIN_WORKERS_KEY, 0))
	var parent_floor := int(band.get(SPLIT_PARENT_MIN_WORKERS_KEY, 0))
	var working_age := int(band.get(WORKING_AGE_KEY, 0))
	if workers <= 0 or working_age - workers < parent_floor:
		# Not a transport fault: the starting band cannot legally split, so this probe cannot prove
		# anything about the command path. Loud rather than skipped — a gate that quietly proves less
		# than it claims is worse than a failing one.
		_fail(("band %d cannot legally split (%d working-age, a new band needs %d and the parent must "
			+ "keep %d), so the faction-bearing command could not be tested — the starting loadout or "
			+ "the settle floors moved") % [band_id, working_age, workers, parent_floor])
		return
	print("%s: split_band band=%d workers=%d (of %d working-age, parent floor %d)" % [
		TAG, band_id, workers, working_age, parent_floor])
	var bands_before := bands.size()
	_main.call("_on_hud_split_band", {
		BAND_ID_KEY: band_id,
		"workers": workers,
		FACTION_KEY: HudConst.PLAYER_FACTION_ID,
	})
	var split_landed: bool = await _await_until("split_band to reach the world",
		COMMAND_EFFECT_TIMEOUT_MSEC,
		func() -> bool: return _player_bands(_snapshot()).size() > bands_before)
	if not split_landed:
		_fail(("split_band changed nothing: still %d band(s) after the command. The seat is held, so "
			+ "either the command never reached the seated link or the server refused it — check the "
			+ "server log for `command.rejected`") % bands_before)
		return
	print("%s: split_band obeyed — player_bands %d -> %d" % [
		TAG, bands_before, _player_bands(_snapshot()).size()])

	# ---- 3. ONE TURN SUBMISSION ----------------------------------------------------------------
	# `_on_hud_next_turn` sends `order <faction> ready`, which is the SEAT's verb; the host verb
	# (`turn N`) rides a throwaway connection and would prove nothing about the seat.
	print("%s: submitting the turn (order %d ready)" % [TAG, HudConst.PLAYER_FACTION_ID])
	_main.call("_on_hud_next_turn", 1)
	var advanced: bool = await _await_until("the turn to resolve", TURN_TIMEOUT_MSEC,
		func() -> bool: return int(_snapshot().get(TURN_KEY, NO_TURN)) > turn_before)
	if not advanced:
		_fail(("the turn never advanced past %d after `order %d ready`. With one seated player and "
			+ "vacant rivals the submission alone resolves it, so the submission did not arrive") % [
			turn_before, HudConst.PLAYER_FACTION_ID])
		return
	print("%s: turn %d -> %d" % [TAG, turn_before, int(_snapshot().get(TURN_KEY, NO_TURN))])
