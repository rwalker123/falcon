extends RefCounted
class_name CommandClient

var host: String = "127.0.0.1"
var port: int = 41001
var proto_port: int = 41001

var _bridge: Object = null

func _init() -> void:
    if ClassDB.class_exists("CommandBridge"):
        _bridge = ClassDB.instantiate("CommandBridge")

func connect_to_host(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    proto_port = port_number
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
    if typeof(result) == TYPE_DICTIONARY and result.get("ok", false):
        return OK
    return ERR_CANT_ACQUIRE_RESOURCE

func set_proto_port(value: int) -> void:
    proto_port = value

func get_proto_port() -> int:
    return proto_port
