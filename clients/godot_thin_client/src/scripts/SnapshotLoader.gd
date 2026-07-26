extends RefCounted
class_name SnapshotLoader

const SnapshotStream := preload("res://src/scripts/SnapshotStream.gd")

## Getter the native `SnapshotDecoder` exposes for its own decode time (see
## `native/src/bridge/decoder.rs`). Probed with `has_method` because the loader has to keep working
## against a native extension built before the getter existed.
const DECODER_TIMING_METHOD := "get_last_decode_usec"

const USEC_PER_MSEC := 1000.0

## Frames a poll keeps: `poll_stream` renders only the newest snapshot, so everything before it in
## the batch was decoded and thrown away.
const FRAMES_KEPT_PER_POLL := 1

var stream: Object = null
var stream_enabled: bool = false
var last_stream_snapshot: Dictionary = {}
var connection_error: Error = OK
var decoder: Object = null
var _last_stream_status: int = StreamPeerTCP.STATUS_NONE
var _warned_stream_error: bool = false
var _warned_decoder_missing: bool = false
var _decoder_reports_timing: bool = false

## Decode cost of the LAST poll that produced a frame, for the per-turn profile line `Main` prints
## (see `TurnProfile`). Three numbers rather than one because the interesting finding may be the
## waste, not the cost: `last_poll_decoded_frames` counts every frame the poll converted into a
## Godot Dictionary and `last_poll_discarded_frames` how many of those were then dropped on the
## floor because a newer one followed in the same batch.
##
## `last_poll_decode_msec` is measured on the GDScript side (the full binding call);
## `last_poll_native_decode_msec` is what the decoder reports for the conversion itself, so the
## difference is the Variant marshalling. Zero when the native build predates the getter.
var last_poll_decode_msec: float = 0.0
var last_poll_native_decode_msec: float = 0.0
var last_poll_decoded_frames: int = 0
var last_poll_discarded_frames: int = 0

func enable_stream(host: String, port: int) -> Error:
    print("SnapshotLoader: attempting stream connection to %s:%d" % [host, port])
    stream = SnapshotStream.new()
    var err_variant: Variant = stream.call("connect_to", host, port)
    var err: Error = err_variant if typeof(err_variant) == TYPE_INT else ERR_BUG
    if err != OK:
        stream = null
        connection_error = err
        stream_enabled = false
        push_warning("Snapshot stream connect failed (%s:%d): %s" % [host, port, error_string(err)])
        return err
    stream_enabled = true
    connection_error = OK
    last_stream_snapshot = {}
    _last_stream_status = StreamPeerTCP.STATUS_NONE
    _warned_stream_error = false
    _warned_decoder_missing = false
    return OK

func disable_stream() -> void:
    stream_enabled = false
    if stream != null:
        stream.call("close_connection")
    stream = null
    _warned_stream_error = false
    _warned_decoder_missing = false

func is_streaming() -> bool:
    if not stream_enabled or stream == null:
        return false
    var status: int = stream_status()
    return status == StreamPeerTCP.STATUS_CONNECTED or status == StreamPeerTCP.STATUS_CONNECTING

func stream_status() -> int:
    if stream == null:
        return StreamPeerTCP.STATUS_NONE
    var status_variant: Variant = stream.call("status")
    if typeof(status_variant) == TYPE_INT:
        var status_int: int = status_variant
        if status_int != _last_stream_status:
            _last_stream_status = status_int
            print("SnapshotLoader: stream status -> %d" % status_int)
        return status_variant
    return StreamPeerTCP.STATUS_NONE

func poll_stream(delta: float) -> Dictionary:
    if stream == null:
        return {}
    var payloads: Variant = stream.call("poll", delta)
    if typeof(payloads) != TYPE_ARRAY:
        return {}
    var status_now := stream_status()
    if status_now == StreamPeerTCP.STATUS_ERROR:
        if not _warned_stream_error:
            push_warning("Snapshot stream error state detected; connection may be closed.")
            _warned_stream_error = true
    elif status_now == StreamPeerTCP.STATUS_CONNECTED:
        _warned_stream_error = false
    var updated := false
    var decode_usec: int = 0
    var native_decode_usec: int = 0
    var decoded_frames: int = 0
    for payload in payloads:
        if typeof(payload) != TYPE_PACKED_BYTE_ARRAY:
            continue
        if decoder == null:
            if ClassDB.class_exists("SnapshotDecoder"):
                decoder = ClassDB.instantiate("SnapshotDecoder")
                _decoder_reports_timing = decoder != null and decoder.has_method(DECODER_TIMING_METHOD)
                _warned_decoder_missing = false
            else:
                if not _warned_decoder_missing:
                    push_warning("SnapshotDecoder class missing; streaming disabled.")
                    _warned_decoder_missing = true
                return {}
        var decode_started: int = Time.get_ticks_usec()
        var snapshot_dict: Dictionary = decoder.decode_snapshot(payload)
        decode_usec += Time.get_ticks_usec() - decode_started
        decoded_frames += 1
        if _decoder_reports_timing:
            native_decode_usec += int(decoder.call(DECODER_TIMING_METHOD))
        if snapshot_dict.is_empty():
            continue
        last_stream_snapshot = snapshot_dict
        updated = true
    if updated:
        last_poll_decode_msec = float(decode_usec) / USEC_PER_MSEC
        last_poll_native_decode_msec = float(native_decode_usec) / USEC_PER_MSEC
        last_poll_decoded_frames = decoded_frames
        last_poll_discarded_frames = maxi(decoded_frames - FRAMES_KEPT_PER_POLL, 0)
        return last_stream_snapshot
    return {}
