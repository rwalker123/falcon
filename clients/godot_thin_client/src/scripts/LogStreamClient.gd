extends RefCounted
class_name LogStreamClient

const HEADER_SIZE := 4

var tcp: StreamPeerTCP = StreamPeerTCP.new()
var buffer: PackedByteArray = PackedByteArray()
var host: String = "127.0.0.1"
var port: int = 41003

func connect_to(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    tcp = StreamPeerTCP.new()
    buffer = PackedByteArray()
    var err: Error = tcp.connect_to_host(host, port)
    if err != OK:
        return err
    tcp.set_no_delay(true)
    return OK

func close() -> void:
    if tcp != null:
        tcp.disconnect_from_host()
    buffer.clear()

func status() -> int:
    if tcp == null:
        return StreamPeerTCP.STATUS_NONE
    return tcp.get_status()

func poll() -> Array:
    var frames: Array = []
    if tcp == null:
        return frames
    tcp.poll()
    var st: int = status()
    if st == StreamPeerTCP.STATUS_CONNECTING:
        return frames
    if st != StreamPeerTCP.STATUS_CONNECTED:
        return frames
    var available: int = tcp.get_available_bytes()
    while available > 0:
        var chunk_len: int = min(available, 4096)
        var result: Array = tcp.get_partial_data(chunk_len)
        if result.size() != 2:
            break
        var err: Error = result[0]
        if err != OK:
            break
        var chunk: PackedByteArray = result[1]
        if chunk.is_empty():
            break
        buffer.append_array(chunk)
        available -= chunk.size()
    var offset: int = 0
    while buffer.size() - offset >= HEADER_SIZE:
        var frame_len: int = _read_u32_le(buffer, offset)
        if buffer.size() - offset < HEADER_SIZE + frame_len:
            break
        var payload: PackedByteArray = buffer.slice(
            offset + HEADER_SIZE,
            offset + HEADER_SIZE + frame_len
        )
        var text: String = payload.get_string_from_utf8()
        var parsed: Variant = JSON.parse_string(text)
        if typeof(parsed) == TYPE_DICTIONARY:
            frames.append(parsed)
        offset += HEADER_SIZE + frame_len
    if offset > 0:
        buffer = buffer.slice(offset, buffer.size())
    return frames

func _read_u32_le(data: PackedByteArray, idx: int) -> int:
    return data[idx] | (data[idx + 1] << 8) | (data[idx + 2] << 16) | (data[idx + 3] << 24)
