extends RefCounted
class_name CommandClient

var host: String = "127.0.0.1"
var port: int = 41001
var tcp: StreamPeerTCP = null
var _no_delay_configured: bool = false

const CONNECT_WAIT_MS = 1000
const CONNECT_POLL_INTERVAL_MS = 20

func connect_to_host(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    tcp = StreamPeerTCP.new()
    var err: Error = tcp.connect_to_host(host, port)
    if err != OK:
        tcp = null
        return err
    _no_delay_configured = false
    return _await_connection()

func close() -> void:
    if tcp != null:
        tcp.disconnect_from_host()
    tcp = null
    _no_delay_configured = false

func status() -> int:
    if tcp == null:
        return StreamPeerTCP.STATUS_NONE
    tcp.poll()
    return tcp.get_status()

func is_connection_active() -> bool:
    return status() == StreamPeerTCP.STATUS_CONNECTED

func poll() -> void:
    if tcp != null:
        tcp.poll()

func ensure_connected() -> Error:
    if tcp == null:
        return connect_to_host(host, port)
    var current_status: int = status()
    match current_status:
        StreamPeerTCP.STATUS_CONNECTED:
            if not _no_delay_configured:
                if tcp != null:
                    tcp.set_no_delay(true)
                _no_delay_configured = true
            return OK
        StreamPeerTCP.STATUS_CONNECTING:
            return _await_connection()
        _:
            close()
            return connect_to_host(host, port)

func send_line(line: String) -> Error:
    var ensure_err: Error = ensure_connected()
    if ensure_err != OK:
        return ensure_err
    var command_line: String = line.strip_edges(false, true) + "\n"
    var payload: PackedByteArray = command_line.to_utf8_buffer()
    var err: Error = tcp.put_data(payload)
    tcp.poll()
    return err

func _await_connection() -> Error:
    if tcp == null:
        return ERR_CONNECTION_ERROR
    var waited_ms: int = 0
    while waited_ms < CONNECT_WAIT_MS:
        tcp.poll()
        var st: int = tcp.get_status()
        match st:
            StreamPeerTCP.STATUS_CONNECTED:
                return OK
            StreamPeerTCP.STATUS_ERROR:
                tcp.disconnect_from_host()
                tcp = null
                return ERR_CONNECTION_ERROR
        OS.delay_msec(CONNECT_POLL_INTERVAL_MS)
        waited_ms += CONNECT_POLL_INTERVAL_MS
    return ERR_BUSY
