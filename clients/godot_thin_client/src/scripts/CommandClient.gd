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

## **WHY THE LAST FAILURE'S REASON IS RETAINED** — `""` while the last send succeeded or none has
## been made.
##
## `send_line` answers an `Error`, which is two bits of information about a bridge that returns a
## SENTENCE. That sentence is the only thing that says WHICH of the bridge's failures happened, and
## it was being dropped on the floor after a `push_warning` no player ever sees — so `Inspector`
## reported a line the bridge had REFUSED as *"Not connected to the server"*. Reported from play: the
## Builders pool's `+` emitted `assign_labor 0 1 builders 1`, `parse_command_line` did not know the
## role, and the client blamed the network for its own line.
##
## **IT IS A FIELD RATHER THAN A RETURN SHAPE** because `send_line`'s `Error` is what every caller
## already branches on, and widening it to a Dictionary would move a decision that belongs to ONE
## caller into all of them.
var last_send_error: String = ""

## Hand a line to the native bridge. **`ERR_CANT_ACQUIRE_RESOURCE` and `ERR_CANT_CONNECT` mean two
## genuinely different things and `Inspector` forks on exactly that**: the first is *there is no
## bridge at all* — nothing was attempted, nothing was understood, the transport is simply absent —
## and the second is *the bridge answered, and it said no*, with `last_send_error` carrying its
## reason. **Do not collapse them.**
##
## The bridge parses a line before it sends it (`bridge/command.rs` → `parse_command_line`), so the
## second case covers a line the CLIENT could not build as well as one it could not deliver; the
## reason names which, because a parse error names the token that failed.
func send_line(line: String) -> Error:
    last_send_error = ""
    if _bridge == null:
        return ERR_CANT_ACQUIRE_RESOURCE
    var result = _bridge.call("send_line", host, proto_port, line)
    if typeof(result) == TYPE_DICTIONARY:
        if result.get("ok", false):
            return OK
        var err_msg: String = result.get("error", "unknown error")
        push_warning("CommandBridge error: %s" % err_msg)
        if result.has("error"):
            last_send_error = err_msg
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
