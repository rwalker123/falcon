extends RefCounted
class_name CommandClient

var host: String = "127.0.0.1"
var port: int = 41001
var proto_port: int = 41001

var _bridge: Object = null
var _proto_port_override: bool = false

func _init() -> void:
    _try_init_bridge()

func connect_to_host(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    if not _proto_port_override:
        proto_port = port_number
    _try_init_bridge()
    if _bridge == null:
        return ERR_CANT_ACQUIRE_RESOURCE
    return OK

func close() -> void:
    pass

func status() -> int:
    if _bridge == null:
        return StreamPeerTCP.STATUS_ERROR
    return StreamPeerTCP.STATUS_CONNECTED

func is_connection_active() -> bool:
    return status() == StreamPeerTCP.STATUS_CONNECTED

func poll() -> void:
    pass

func ensure_connected() -> Error:
    return OK if _bridge != null else ERR_CANT_ACQUIRE_RESOURCE

func send_line(line: String) -> Error:
    if _bridge == null:
        return ERR_CANT_ACQUIRE_RESOURCE
    var result = _bridge.call("send_line", host, proto_port, line)
    if typeof(result) == TYPE_DICTIONARY:
        if result.get("ok", false):
            return OK
        var err_msg: String = result.get("error", "unknown error")
        push_warning("CommandBridge error: %s" % err_msg)
        if result.has("error"):
            return ERR_CANT_CONNECT
    return ERR_CANT_ACQUIRE_RESOURCE

## **ASK the sim a forecast question.** Returns whether the question reached the socket — the ANSWER
## arrives later through `poll_query_replies`, because the server writes it back on the same stream
## after evaluating it. See `native/src/bridge/query.rs` for why a query cannot ride `send_line`.
func send_query(request_id: int, ask: Dictionary) -> bool:
    if _bridge == null:
        return false
    var result: Variant = _bridge.call("send_query", host, proto_port, request_id, ask)
    if typeof(result) != TYPE_DICTIONARY:
        return false
    if bool(result.get("ok", false)):
        return true
    push_warning("CommandBridge query error: %s" % result.get("error", "unknown error"))
    return false

## Drain the answers that have landed since the last call. **Call it once a frame**: this is the one
## hop from the native query worker onto the main thread.
func poll_query_replies() -> Array:
    if _bridge == null:
        return []
    var result: Variant = _bridge.call("poll_query_replies")
    return result if result is Array else []

func set_proto_port(value: int) -> void:
    proto_port = value
    _proto_port_override = true

func get_proto_port() -> int:
    return proto_port

func _try_init_bridge() -> void:
    if _bridge != null:
        return
    if ClassDB.class_exists("CommandBridge"):
        _bridge = ClassDB.instantiate("CommandBridge")
    else:
        push_warning("CommandBridge class unavailable; commands disabled")
