extends RefCounted
class_name SnapshotStream

const HEADER_SIZE := 4

## **THE GREETING: the seat token, as this many little-endian bytes, and nothing else.**
##
## Mirrors `core_sim::network::SEAT_TOKEN_BYTES`. The server reads EXACTLY this many bytes off a
## freshly accepted stream socket before it will deliver anything, and there is no framing, no length
## prefix and no reply to catch a disagreement: a client that writes a different width is registered
## **unseated** and receives no frames for the life of the connection, with the only trace a
## `presented no seat token` line in the SERVER's log. So the two constants must move together.
const SEAT_TOKEN_BYTES := 8

## The greeting meaning *"I hold no seat"* — `core_sim::SeatToken::NONE`. A connection that sends it is
## registered unseated immediately instead of sitting out the server's handshake timeout
## (`core_sim::network::DEFAULT_HANDSHAKE_TIMEOUT`), which is what a watching tool wants. It is also
## this seam's "no token known yet" value, so a stream opened without one still says something rather
## than stalling every other client's accept.
##
## **It is the one token value that is not a secret**, which is why it is spelled here and the others
## are never printed (see `SEAT_TOKEN_LOG_REDACTION`).
const NO_SEAT_TOKEN := 0

## **THE TOKEN NEVER GOES IN A LOG LINE — this is what goes there instead.**
##
## A `SeatToken` is minted per CLAIM from a CSPRNG and is the whole of what the stream socket presents
## to be sent this seat's frames (`core_sim::SeatToken`), so it is a bearer secret: anything holding
## one can read another player's world. The server logs no token at all — `SeatToken` has no `Display`
## and its `Debug` renders `SeatToken(redacted)` — and a client that printed the value would be the
## only way to correlate a greeting with a claim by reading logs, which is exactly the hole the server
## side closed.
##
## **A truncation or a hash would be no better**: a partial secret is still a secret, and a stable
## fingerprint is a correlator, which is the whole of what an attacker reading logs wants. So the
## EVENT is logged and the VALUE never is — the handshake lines below are how both transport bugs on
## this branch were diagnosed, so losing them would cost more than the secret is worth.
const SEAT_TOKEN_LOG_REDACTION := "<redacted>"

var tcp: StreamPeerTCP = StreamPeerTCP.new()
var buffer: PackedByteArray = PackedByteArray()
var host: String = "127.0.0.1"
var port: int = 41002
## The token this connection greets with, presented once per socket. See `connect_to`.
var seat_token: int = NO_SEAT_TOKEN

## Whether the greeting has been written on the CURRENT socket. Reset per connect, because a token is
## presented once per connection and a reconnect is a new connection.
var _greeting_sent: bool = false
## Has a failed greeting write already been reported? The retry runs every poll, so without this a
## dying socket would fill the log.
var _warned_greeting_failed: bool = false

## Open the stream and arrange to greet the server with `token`.
##
## **The greeting cannot be written here**: `connect_to_host` is asynchronous, so the socket is still
## `STATUS_CONNECTING` on return and a write would be dropped. It goes out from `poll` the first time
## the socket is seen `STATUS_CONNECTED` — inside the server's handshake timeout, since the caller
## polls every rendered frame.
func connect_to(hostname: String, port_number: int, token: int = NO_SEAT_TOKEN) -> Error:
    host = hostname
    port = port_number
    seat_token = token
    tcp = StreamPeerTCP.new()
    buffer = PackedByteArray()
    _greeting_sent = false
    _warned_greeting_failed = false
    var err := tcp.connect_to_host(host, port)
    if err != OK:
        return err
    tcp.set_no_delay(true)
    return OK

func close_connection() -> void:
    if tcp:
        tcp.disconnect_from_host()
    buffer.clear()
    _greeting_sent = false

func status() -> int:
    if tcp == null:
        return StreamPeerTCP.STATUS_NONE
    return tcp.get_status()

func stream_is_connected() -> bool:
    return status() == StreamPeerTCP.STATUS_CONNECTED

## **Has this socket greeted the server with its seat token yet?** Until it has, the server holds the
## connection as unseated and a frame published in that window is addressed past us — which is why the
## world request waits on this and not merely on `stream_is_connected`.
func seat_token_presented() -> bool:
    return _greeting_sent

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
    _present_seat_token()
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

## **Write the greeting, once, on a socket that has just come up.**
##
## `encode_s64` rather than `encode_u64` on purpose: the bridge hands the token up as an `i64` (a
## `u64` reinterpreted, see `bridge/query.rs`), and the two encoders write the same eight bytes for
## the same bit pattern — so a token with the high bit set survives the round trip, while
## `encode_u64` would reject it as negative.
##
## A failed write is retried on the next poll rather than latched: the server's handshake timeout is
## seconds and the poll runs every frame, so a transient full send buffer costs nothing, whereas
## giving up would leave a live socket that never receives a frame.
func _present_seat_token() -> void:
    if _greeting_sent:
        return
    var greeting := PackedByteArray()
    greeting.resize(SEAT_TOKEN_BYTES)
    greeting.encode_s64(0, seat_token)
    var err: Error = tcp.put_data(greeting)
    if err != OK:
        if not _warned_greeting_failed:
            _warned_greeting_failed = true
            push_warning("Snapshot stream could not present its seat token (%s); retrying." % error_string(err))
        return
    _greeting_sent = true
    _warned_greeting_failed = false
    print("SnapshotStream: presented its seat token %s to %s:%d" % [SEAT_TOKEN_LOG_REDACTION, host, port])

func _read_u32_le(data: PackedByteArray, idx: int) -> int:
    return data[idx] | (data[idx + 1] << 8) | (data[idx + 2] << 16) | (data[idx + 3] << 24)
