extends RefCounted
class_name SnapshotStream

const HEADER_SIZE := 4

var tcp: StreamPeerTCP = StreamPeerTCP.new()
var buffer: PackedByteArray = PackedByteArray()
var host: String = "127.0.0.1"
var port: int = 41002

func connect_to(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    tcp = StreamPeerTCP.new()
    buffer = PackedByteArray()
    var err := tcp.connect_to_host(host, port)
    if err != OK:
        return err
    tcp.set_no_delay(true)
    return OK

func close_connection() -> void:
    if tcp:
        tcp.disconnect_from_host()
    buffer.clear()

func status() -> int:
    if tcp == null:
        return StreamPeerTCP.STATUS_NONE
    return tcp.get_status()

func stream_is_connected() -> bool:
    return status() == StreamPeerTCP.STATUS_CONNECTED

func poll(_delta: float) -> Array:
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
        var chunk_size: int = min(available, 4096)
        var result: Array = tcp.get_partial_data(chunk_size)
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
    var offset := 0
    while buffer.size() - offset >= HEADER_SIZE:
        var frame_len := _read_u32_le(buffer, offset)
        if buffer.size() - offset < HEADER_SIZE + frame_len:
            break
        var payload := buffer.slice(offset + HEADER_SIZE, offset + HEADER_SIZE + frame_len)
        frames.append(payload)
        offset += HEADER_SIZE + frame_len
    if offset > 0:
        buffer = buffer.slice(offset, buffer.size())
    return frames

func _read_u32_le(data: PackedByteArray, idx: int) -> int:
    return data[idx] | (data[idx + 1] << 8) | (data[idx + 2] << 16) | (data[idx + 3] << 24)
