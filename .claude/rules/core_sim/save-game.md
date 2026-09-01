---
paths:
  - "core_sim/src/save.rs"
  - "core_sim/src/save_store.rs"
  - "core_sim/src/config_fingerprint.rs"
  - "core_sim/src/bin/server.rs"
  - "sim_runtime/proto/command.proto"
  - "core_sim/tests/save_round_trip.rs"
  - "core_sim/tests/save_load_over_the_socket.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/src/snapshot/publish.rs"
---

# Save game: the blob, the slots, and what a load owes the client

The **format** — what a `SimState` plus its world encodes to, and why worldgen is not re-run — is
`.claude/rules/core_sim/checkpoints.md`. This file is the feature built on it: the wire, the files on
disk, the load path's obligations, and the autosave cadence.

## The blob is three parts, and only the last is compressed

```text
[ SAVE_MAGIC : 8 bytes ][ SaveHeader : CBOR ][ gzip( SavePayload : CBOR ) ]
```

The split exists so a **slot list costs a header, not a world**. Measured on a 160x104 world:

| | cost |
|---|---|
| `read_save_header` | **0.005 ms** |
| `decode_save` | 63 ms |
| `encode_save` (capture + CBOR + gzip) | 118 ms |
| a turn, for scale | 30 ms |
| blob | 20,766,409 → **1,257,874 bytes** (16.5x) |

Compression is level 6, not 9: level 9 gives 1,221,708 bytes — 2.9% smaller for materially more CPU
on a blob the autosave rewrites on a cadence. The header stays **uncompressed** for the same reason
it is a separate document: a listing that had to inflate before reading would pay a decompressor per
row.

> **The `BufWriter` around the gzip encoder is load-bearing, not tidiness.** `ciborium` writes in very
> small pieces and `GzEncoder` deflates on every `write` it receives, so streaming CBOR straight into
> the encoder made a 160x104 save take **1083 ms** — against 19 ms to encode the checkpoint and 86 ms
> to gzip it. Almost all of it was per-call overhead rather than work on the data. Buffering removed
> 9.1x. Anything else that streams a serializer into a compressor here needs the same treatment.

**A version mismatch is a refusal, never a decode.** There is no back-compat and therefore no
migration code; `SAVE_FORMAT_VERSION` exists so a stale save is *rejected by a typed error naming
both versions* rather than mis-read into a plausible wrong world. It is checked before the payload is
looked at, which is also what lets `read_save_header` gate a listing.

## Four operations, three commands and one query

| Operation | Proto | Why |
|---|---|---|
| `save_game` | `CommandEnvelope` field **66** | Writes a file |
| `load_game` | field **67** | Replaces the world |
| `delete_save` | field **68** | Removes a file |
| `list_saves` | `QueryCommand.query` field **5** | Mutates nothing, so it is a query |

Replies ride `QueryReplyEnvelope`: `ListSavesReply` at field **6**, `SaveOpReply` at **7**. The three
commands carry a `request_id` and answer on that envelope **because it is the socket's one way back**,
not because they are questions — a client that cannot tell whether its save landed has not saved.

`list_saves` is answered **before the `world_active` gate**, from disk. A player opens the load menu
when there is no world, and a `no_active_world` refusal there would make the feature unreachable from
the one screen that needs it.

## Slot names are whitelisted, because a slot name becomes a filename

`validate_slot_name` accepts letters, digits, spaces, `-` and `_`, up to `MAX_SLOT_NAME_LEN`. That is
a **whitelist, not a blacklist of the traversal spellings anyone thought of**: `..`, `/`, `\`, a
leading `~`, a drive letter, a NUL and every control character are refused by the one rule, rather
than by a list that has to stay ahead of three platforms' path grammars. The name arrives over the
wire from a text field, so the question is not whether *this* client is careful. Refusal happens
before any `PathBuf` is built.

**`autosave` is reserved.** Only the autosave hook writes it; an explicit save naming it is refused
with `save_error::RESERVED_SLOT`. A rolling backup a player can overwrite by accident is not a
backup.

Writes go to a `.partial` file and are renamed, so a crash mid-write leaves the previous save intact
— which matters most for the slot the autosave rewrites on a cadence.

A file that is not a save, or is one this build cannot read, is **skipped from the listing with a
warning** rather than failing it: one corrupt file must not make the load menu unopenable, which is
precisely when a player needs it.

**A listing reads a bounded prefix of each file, not the file.** `read_save_header` stops after the
first CBOR document, but handing it the whole blob still costs the whole blob: ~1.2 MB per slot at
160x104, re-paid on every pane open and after every save and delete. `read_header_only` reads
`HEADER_PREFIX_BYTES` (**8 KiB**, against a measured 1,313-byte header — only the
`config_fingerprint` grows, one entry per boot config, so that is roughly 200 configs of headroom)
and takes the row's `size_bytes` off the same open handle, since the buffer's length is no longer the
file's. **A header that outgrew the bound is a bound that is wrong, not a save that is broken**: a
CBOR document cut short fails exactly as a corrupt one does, so believing it would drop good slots off
the load menu — and every slot at once, headers being all much the same size. A decode failure on a
prefix that was cut short therefore re-reads that one file whole and warns naming the constant.

## What a load owes beyond restoring the world

`handle_load_game` follows `handle_new_game`'s shape because it has the same obligations, plus one
only a load has:

- the snapshot sink is attached **before** the first capture, so the loaded world's baseline frame is
  broadcast like every frame after it;
- `WorldEpoch` is bumped, so the client's epoch gate treats it as a new world;
- `world_active` flips;
- **the `CommandLog` is re-based.** Everything before a load is unreachable, exactly as for
  `new_game` and `reset_map` — without it a rollback would replay across the load into a world that
  never existed.
- **the runtime-owned config fields are carried.** The replacement app is built by
  `build_headless_app`, so its whole `SimulationConfig` is the file's — right for every tunable and
  wrong for the two rows `carry_runtime_owned_fields` owns (`config-loading.md` → "A `load_game`
  needs the same two"). Left uncarried, the player's fog switch comes back on in the reveal frame.

### Publishing a loaded world needs a THIRD kind of capture

A world that arrives already resolved is neither of the two cases the snapshot layer had, and both
obvious answers shipped as defects.

| | ring entry | tick | first frame |
|---|---|---|---|
| `run_turn` | pushed | advanced by `advance_tick` | full |
| `recapture_snapshot_in_place` | **refreshed, never pushed** | held | delta |
| `publish_baseline_snapshot` | pushed | held | full |

**`run_turn` was wrong in a way the tick did not show.** It resolves a *real* turn, so the population
aged and food was eaten; restoring the tick number afterwards only hid it.

**`recapture_snapshot_in_place` was wrong in a way a published frame did not show.** A recapture
refreshes `history.back_mut()`, and a freshly built app's publication ring is **empty** — so nothing
is refreshed, no entry is pushed, and `latest_entry()` stays `None`. The live symptom was a client
stuck on the loading overlay forever: `Resync` answered `resync.no_world`, and the only frame the new
epoch ever saw was a *delta*, which is not a baseline (a field that happens to equal its default
compares unchanged and is never sent).

`publish_baseline_snapshot` is the third thing, and it is expressible only because `capture_snapshot`
takes `SimulationTick` as a read-only `Res` — the advance lives in `advance_tick`, a separate system
in the same stage. The frame reaches the socket with no further call: the publisher thread broadcasts
every `PublishRequest::Frame` whatever its `Publication` kind.

`new_game` and `reset_map` share `rebuild_world_from_config`, which ends in `run_turn`, so they never
had this hole — asserted rather than assumed, because the assumption is what let it through.

> **The test that passed while this was broken asserted the wrong thing.** It checked that a frame was
> *published* — `last_snapshot()`, which **both** publication kinds set — rather than that a ring entry
> *exists*, which only `Publication::Turn` does. Assert on the state a client reads (`latest_entry()`,
> and that the entry carries `encoded_snapshot_flat`), never on "something was sent".

**Decoding happens before anything about the running world changes**, so a save this build cannot
read leaves the player where they were rather than half-way into a world that failed to arrive.

`build_headless_app` runs worldgen in `Startup`, so the load inserts `SuppressWorldgen` before the
first update. **Absence means "generate a world"** — every existing caller is unaffected, and it is
the same shape as `Replaying`: a flag whose only job is to make one scheduled thing not happen.

## The config-drift warning names files

On load, the header's `ConfigFingerprint` is compared against the live one and the differing files
are returned on `SaveOpReply.config_drift` — reaching the client, not just the server log, because
the client is what shows it. Per file, because *"config changed"* is not actionable and
*"`fauna_config.json` and `recipes.json` changed"* is.

`ConfigDigestKind::{Absent, Builtin, File}` keeps **`Builtin` and `File` a real difference**: "no file
was there, so the compiled-in copy loaded" and "a file was there and hashed to N" are different facts
about where tuning came from, and collapsing them would report no change when the shipped file
appeared or vanished. `Absent` holds 0 so a field the wire omits reads as *"this side said nothing
about that config"*, which is exactly what it names. The hash itself is not published — a client can
act on *this file changed* and can do nothing with the number it changed to.

The world is restored **exactly as saved**; drift describes the tuning the turns *from here* will run
under. Empty is the good case.

## Autosave is a cadence, and the cadence is a lever

Writing a save is not free — 118 ms against a 30 ms turn at 160x104 — so autosaving every turn would
make one turn cost five. `autosave_interval_turns` decides how often instead; `0` switches it off.
The cost then lands on one turn in N rather than on all of them, which a human-paced game absorbs.

The alternative considered was keeping the encode on the turn thread and moving compress + write off
it. Rejected: gzip is ~89 of the 118 ms, so it would help, but it buys a thread, a channel, overlapping
-write semantics and a partial-file question — real concurrency for a cost a one-line cadence already
removes.

The hook runs **after** the turn is pushed to the command log, so a crash between the two leaves an
autosave whose world the log can still reproduce. An autosave that cannot be written warns and the
turn carries on: it is a convenience, and killing a live campaign over a full disk would be worse
than losing the backup.

## The socket test — because every other save/load test asserts on the wrong side of the wire

Both defects above shipped **under a passing suite**, and the common cause is not that the
assertions were weak. It is that every save/load test drives `apply_command` in process and asserts
on server state, so nothing exercised the path a client actually uses.
`core_sim/tests/save_load_over_the_socket.rs` closes that: new game → turns → `save_game` →
`load_game`, driven over TCP, asserting on the decoded FlatBuffers frames.

**It spawns the built `server` binary as a child process.** Standing the socket layer up in process
was the alternative and it is the wrong one here: the main loop in `bin/server.rs` — the
`world_active` gate, the dispatch, the load handler, the post-command recapture — is *where both
defects lived*, so a test that rebuilt any of it would test the copy. `CARGO_BIN_EXE_server` is
also what makes cargo build the binary before the test runs, and it is defined **only for the
package that owns the bin**, which is why the file lives in `core_sim/tests/` and not in
`integration_tests/` (from there the path would be guessed, and `cargo test -p integration_tests`
would never build it).

**It talks the client's transport, verb for verb: one TCP connection per command.**
`transmit_proto_command` in the Godot native bridge connects, writes one length-prefixed frame and
drops the socket; the save and query verbs do the same and merely hold the connection open to read
the answer (`bridge/query.rs`). Holding one persistent connection instead would exercise a path no
client uses, which is the mistake the whole test exists to stop making. The cost of fire-and-forget
is that two commands in flight could reach the dispatch loop in either order, so **every command is
sent only after the previous one's effect has been observed** — a frame on the snapshot socket, or a
`SaveOpReply` on the command socket. That is the synchronisation; there are no sleeps anywhere in
the test.

**`frameSeq == 1` is what makes the full-frame assertion mean anything.** "The load published a
delta" and "the test joined after the load's baseline" are indistinguishable from the frame alone,
and the second is a live possibility (`world-handoff.md` — a frame broadcast while a connection sits
in the listen backlog reaches nobody). A world's first publication is always `frameSeq == 1`
(`next_publication` counts up from a fresh `SnapshotHistory` per world), so asserting it *first*
turns the ambiguity into a distinguishable failure. The `new_game` reveal handles the same race the
way the client does — one retry, taken only when the first frame proves we joined late.

Ports are never fixed: the base is moved out of the human-facing range through a patched
`simulation_config.json` under `SIM_CONFIG_PATH` (**not** `SIM_PORT_BASE`, which makes the base
explicit and therefore fatal on a collision), so `port_alloc` auto-bumps past a busy block, and the
test reads the block the server actually bound from `SIM_PORTS_FILE` — the client's own discovery
path. `SIM_SAVE_DIR`, the config and the log all live in one scratch directory that drops with the
test; the child is killed on drop, panic included. Every wait is a deadline whose message names what
never arrived and quotes the server's own log. It adds ~6 s to the suite.

**Both defects were re-introduced and confirmed caught**: `recapture_snapshot_in_place` in
`publish_loaded_world` fails on *"a loaded world's first frame must be a FULL snapshot … it was a
delta"*, and a `run_turn` there fails on the tick (`left: 5, right: 4`). A test for a defect that has
already been fixed is worth exactly what its sabotage run proves.

## An accepted command socket must be put back into blocking mode

`spawn_command_listener` sets the **listener** non-blocking so its accept loop can poll. On BSD and
macOS an accepted socket **inherits that flag**; on Linux it does not. `handle_proto_client` then
blocks in `read_exact` waiting for the client's next frame — which on a non-blocking socket returns
`WouldBlock` the instant nothing is queued, and the read loop can only read that as a broken
connection, so it warns and drops it.

The live symptom on macOS was `Proto command length read error: Resource temporarily unavailable`
on **every** connection, and a genuine race: a command written a hair after the accept was read as a
dead socket and **silently lost**, with only a WARN. It never became a visible bug because the
shipped client opens a connection per command and the reply writer holds its own `try_clone`d half,
so the answer to the one command still went out. The accept arm now calls `set_nonblocking(false)`,
which also makes the two platforms behave identically.

## None of the save verbs are replayable

`SaveGame` and `DeleteSave` touch a file and no world, so replaying them would re-do disk writes to
change nothing about the world being replayed — the same reason the staged config overrides are
excluded. `LoadGame` **replaces** the world, so there is nothing before it to replay from; it re-bases
the origin instead of being logged.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/simulation_config.json` | `autosave_interval_turns` (**10**) | How often the `autosave` slot is rewritten, in turns. `0` disables it. At 160x104 that is ~118 ms of encode on one turn in ten rather than on every turn |

## Environment

`SIM_SAVE_DIR` overrides where slots live; the default is `./saves`, relative to the server's working
directory. Listed in `core_sim/CLAUDE.md` → Environment Overrides with the rest of the family.
