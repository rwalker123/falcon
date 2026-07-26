extends Node

## Headless regression guard for `SnapshotLoader.poll_stream`'s frame-supersession rule.
##
## **A DELTA MAY NEVER BE SILENTLY DROPPED.** `poll_stream` used to keep only the newest frame a
## poll delivered. That is free when every frame is a full snapshot — each one restates the whole
## world, so an older one is pure waste — and silently destructive once deltas became the
## steady-state carrier (#386): each delta carries one turn's changes, nothing later restates them,
## and the client's world just drifts from the server's with no error, no warning, and no visible
## symptom beyond a map that looks a little calm.
##
## So the rule is **supersession, not recency**: a full snapshot supersedes everything before it; a
## delta supersedes nothing. This guard pins that rule against the REAL decoder and the REAL
## fixtures, because the failure it guards against is invisible in play and the "obvious"
## optimisation (keep the newest, drop the rest) is one line away at all times.
##
## It also pins the **gap-detection** half: a delta whose `baseFrameSeq` names a frame this client
## never applied must be DROPPED and must raise `resync_needed`, never merged against the wrong
## baseline — merging produces a world that is silently wrong rather than visibly broken
## (`docs/plan_delta_streaming.md` §3.3, §7).
##
## The transport is faked (a stub exposing the two methods `poll_stream` calls) but nothing else is:
## the payloads are the committed fixtures and the decode is the live `SnapshotDecoder`, so a
## decoder that stops accepting the delta fails here too.
##
## Regenerate the fixtures with `cargo xtask decode-fixture` after any schema change.
##
## Run as a scene (NOT --script: the loader chain reaches project autoloads). No GPU needed:
##   godot --headless --path . res://tools/stream_frame_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const SnapshotLoaderScript := preload("res://src/scripts/SnapshotLoader.gd")

const SNAPSHOT_FIXTURE := "res://tests/fixtures/snapshot_envelope.bin"
const DELTA_FIXTURE := "res://tests/fixtures/snapshot_delta_envelope.bin"

## What `poll_stream` reads off the transport. `STATUS_CONNECTED` keeps it on the quiet path (an
## error status only pushes a warning, but a guard that prints warnings trains people to ignore them).
const FAKE_STATUS := StreamPeerTCP.STATUS_CONNECTED

## Frame kinds as `SnapshotLoader`/`bridge/decoder.rs` stamp them.
const KIND_KEY := "frame_kind"
const KIND_SNAPSHOT := "snapshot"
const KIND_DELTA := "delta"

## A transport stub. `poll_stream` calls exactly these two methods on `loader.stream`, so this is the
## whole surface — deliberately not a `SnapshotStream` subclass, which would drag in a real socket.
class FakeStream:
	extends RefCounted
	var batches: Array = []

	func poll(_delta: float) -> Array:
		if batches.is_empty():
			return []
		return batches.pop_front()

	func status() -> int:
		return FAKE_STATUS

var _failures: Array[String] = []
var _snapshot_bytes := PackedByteArray()
var _delta_bytes := PackedByteArray()

func _ready() -> void:
	_snapshot_bytes = _read_fixture(SNAPSHOT_FIXTURE)
	_delta_bytes = _read_fixture(DELTA_FIXTURE)
	if _failures.is_empty():
		_case_full_then_delta_keeps_both()
		_case_full_supersedes_the_prefix()
		_case_full_supersedes_an_earlier_full()
		_case_unapplicable_delta_is_dropped_and_asks_for_a_resync()
	_finish()

## THE property. A poll delivering a full snapshot and then a delta must apply BOTH, in order.
## Keep-only-the-newest returns 1 here, which is exactly the regression this guard exists to catch.
func _case_full_then_delta_keeps_both() -> void:
	var loader := _fresh_loader([[_snapshot_bytes, _delta_bytes]])
	var frames := loader.poll_stream(0.0)
	if not _expect_size(frames, 2, "a poll carrying a full snapshot THEN a delta applied only some of them — a delta that is dropped is gone forever, since nothing later restates it (this is keep-only-the-newest coming back)"):
		return
	_expect_kind(frames[0], KIND_SNAPSHOT, "the first applied frame of a full+delta batch")
	_expect_kind(frames[1], KIND_DELTA, "the second applied frame of a full+delta batch")

## A full snapshot restates the whole world, so frames queued BEFORE it are moot and dropping them
## is correct. Frames after it are not.
func _case_full_supersedes_the_prefix() -> void:
	var loader := _fresh_loader([[_delta_bytes, _snapshot_bytes, _delta_bytes]])
	var frames := loader.poll_stream(0.0)
	if not _expect_size(frames, 2, "a batch of delta,FULL,delta did not collapse to the full snapshot and what followed it — the leading delta (which has no baseline to apply to anyway) must be dropped and the trailing one kept"):
		return
	_expect_kind(frames[0], KIND_SNAPSHOT, "the frame a delta,FULL,delta batch starts from")
	_expect_kind(frames[1], KIND_DELTA, "the frame following the full snapshot in a delta,FULL,delta batch")

## Two full snapshots in one poll: the older is pure waste. This is the case the old
## keep-only-the-newest behaviour got right, kept here so a fix for the others cannot lose it.
func _case_full_supersedes_an_earlier_full() -> void:
	var loader := _fresh_loader([[_snapshot_bytes, _snapshot_bytes]])
	var frames := loader.poll_stream(0.0)
	if not _expect_size(frames, 1, "two full snapshots in one poll both applied — the older one is pure waste, since the newer restates the entire world"):
		return
	_expect_kind(frames[0], KIND_SNAPSHOT, "the surviving frame of a full,full batch")

## Gap detection. The SAME delta applied twice: the second time its `baseFrameSeq` names a frame
## that is no longer what the client holds, so it must be dropped rather than merged against the
## wrong baseline — and dropping it must raise `resync_needed`, or the client renders a frozen world
## forever with nothing asking the server for a fresh one.
func _case_unapplicable_delta_is_dropped_and_asks_for_a_resync() -> void:
	var loader := _fresh_loader([[_snapshot_bytes, _delta_bytes], [_delta_bytes]])
	var first := loader.poll_stream(0.0)
	if not _expect_size(first, 2, "the setup poll did not apply the baseline and its delta"):
		return
	loader.resync_needed = false
	var second := loader.poll_stream(0.0)
	_expect_size(second, 0, "a delta whose baseFrameSeq names a frame the client no longer holds was APPLIED — merging against the wrong baseline is how the client's world silently diverges from the server's")
	if not loader.resync_needed:
		_fail("a delta was dropped as unapplicable but `resync_needed` was NOT raised — nothing will ask the server for a full world, so the client stays frozen at the last good frame with no error anywhere")

func _fresh_loader(batches: Array) -> SnapshotLoader:
	# A fresh loader per case so each gets its own SnapshotDecoder, and therefore its own baseline.
	var loader: SnapshotLoader = SnapshotLoaderScript.new()
	var fake := FakeStream.new()
	fake.batches = batches.duplicate()
	loader.stream = fake
	loader.stream_enabled = true
	return loader

func _read_fixture(path: String) -> PackedByteArray:
	if not FileAccess.file_exists(path):
		_fail("no fixture at %s — generate it with: cargo xtask decode-fixture" % path)
		return PackedByteArray()
	var bytes := FileAccess.get_file_as_bytes(path)
	if bytes.is_empty():
		_fail("fixture at %s is empty" % path)
	return bytes

func _expect_size(frames: Array, expected: int, message: String) -> bool:
	if frames.size() == expected:
		return true
	_fail("%s (applied %d, expected %d)" % [message, frames.size(), expected])
	return false

func _expect_kind(frame: Dictionary, expected: String, what: String) -> void:
	var actual := String(frame.get(KIND_KEY, ""))
	if actual != expected:
		_fail("%s is a '%s', expected '%s' — the batch was applied in the wrong order" % [what, actual, expected])

func _fail(msg: String) -> void:
	_failures.append(msg)

func _finish() -> void:
	if _failures.is_empty():
		print("stream_frame_guard: PASS — supersession holds, no delta silently dropped, gaps ask for a resync")
		get_tree().quit(0)
	else:
		printerr("stream_frame_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
