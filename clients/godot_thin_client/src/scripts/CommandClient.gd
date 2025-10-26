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
