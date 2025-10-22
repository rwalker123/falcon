extends RefCounted
class_name CommandClient

var host: String = "127.0.0.1"
var port: int = 41001
var tcp: StreamPeerTCP = null

func connect_to_host(hostname: String, port_number: int) -> Error:
    host = hostname
    port = port_number
    tcp = StreamPeerTCP.new()
    var err := tcp.connect_to_host(host, port)
    if err != OK:
        tcp = null
        return err
    tcp.set_no_delay(true)
    return OK

func close() -> void:
    if tcp != null:
        tcp.disconnect_from_host()
    tcp = null

func status() -> int:
    if tcp == null:
        return StreamPeerTCP.STATUS_NONE
    return tcp.get_status()

func is_connection_active() -> bool:
    return status() == StreamPeerTCP.STATUS_CONNECTED

func send_line(line: String) -> Error:
    if tcp == null:
        return ERR_CONNECTION_ERROR
    var current_status := tcp.get_status()
    if current_status == StreamPeerTCP.STATUS_CONNECTING:
        tcp.poll()
        current_status = tcp.get_status()
    if current_status != StreamPeerTCP.STATUS_CONNECTED:
        return ERR_CONNECTION_ERROR
    var command_line := line.strip_edges(false, true) + "\n"
    var payload := command_line.to_utf8_buffer()
    var err := tcp.put_data(payload)
    if err == OK:
        tcp.poll()
    return err
