extends RefCounted
class_name SnapshotLoader

const SnapshotStream := preload("res://src/scripts/SnapshotStream.gd")

var frames: Array[Dictionary] = []
var index: int = 0
var stream: Object = null
var stream_enabled: bool = false
var last_stream_snapshot: Dictionary = {}
var connection_error: Error = OK
var decoder: Object = null
var _last_stream_status: int = StreamPeerTCP.STATUS_NONE
var _warned_stream_error: bool = false
var _warned_decoder_missing: bool = false

func load_mock_data(path: String) -> void:
    var file: FileAccess = FileAccess.open(path, FileAccess.READ)
    if file == null:
        push_error("Unable to open mock snapshot data at %s" % path)
        frames = []
        return
    var json_text: String = file.get_as_text()
    var parsed: Variant = JSON.parse_string(json_text)
    if typeof(parsed) != TYPE_ARRAY:
        push_error("Expected mock snapshot data to be an array of frames")
        frames = []
        return
    frames = []
    for entry in parsed:
        if typeof(entry) == TYPE_DICTIONARY:
            frames.append(entry)
    index = 0

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
    for payload in payloads:
        if typeof(payload) != TYPE_PACKED_BYTE_ARRAY:
            continue
        if decoder == null:
            if ClassDB.class_exists("SnapshotDecoder"):
                decoder = ClassDB.instantiate("SnapshotDecoder")
                _warned_decoder_missing = false
            else:
                if not _warned_decoder_missing:
                    push_warning("SnapshotDecoder class missing; streaming disabled.")
                    _warned_decoder_missing = true
                return {}
        var snapshot_dict: Dictionary = decoder.decode_snapshot(payload)
        if snapshot_dict.is_empty():
            continue
        last_stream_snapshot = snapshot_dict
        updated = true
    if updated:
        return last_stream_snapshot
    return {}

func current() -> Dictionary:
    if frames.is_empty():
        return {}
    return frames[index]

func advance() -> Dictionary:
    if frames.is_empty():
        return {}
    index = (index + 1) % frames.size()
    return frames[index]

func rewind() -> Dictionary:
    if frames.is_empty():
        return {}
    index = (index - 1 + frames.size()) % frames.size()
    return frames[index]
