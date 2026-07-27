---
paths:
  - "core_sim/src/port_alloc.rs"
  - "core_sim/src/bin/server.rs"
  - "core_sim/src/resources.rs"
  - "clients/godot_thin_client/src/scripts/ServerPortsFile.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/ui/inspector/LogsPanel.gd"
  - "scripts/run_stack.sh"
---

<!-- NOT a split_claude_md.sh extraction. Hand-merged from the two hubs, which each
     carried a full copy of this contract (server-side allocation in core_sim/CLAUDE.md,
     client-side discovery in clients/godot_thin_client/CLAUDE.md). One spec, one home —
     it spans both halves, so the `paths:` above gate on server AND client files. -->

# Ports: block allocation, the handshake file, and client discovery

**This is a two-sided contract with one authority.** The server binds a block of ports and
publishes what it actually got; the client is a **pure reader** of that publication. The key
names in the handshake file are the contract — the client's reader breaks silently on a rename,
so change both halves together.

## Server: the block is bound up front, all-or-nothing

The server binds **the whole block at once** (`port_alloc.rs`, `port_alloc::allocate`) and hands
the already-bound `TcpListener`s to `start_snapshot_server` / `start_log_stream_server` /
`spawn_command_listener`. Previously each subsystem bound its own socket and failed differently
— the command listener **panicked** while snapshot/log streaming merely warned and disabled
themselves, so a conflict on 41002 left a *running* server that silently never streamed. **There
is no longer any path where the server runs with a socket disabled because it was in use.**

**Allocation policy.** If `SIM_PORT_BASE` was set, the base is honoured **exactly** — a conflict
is fatal (exit code `2`, with an actionable message), never bumped, because `scripts/run_stack.sh`
and the per-worktree port assignment depend on an explicit base being deterministic. Otherwise
the server starts at the configured base (the config's `port_base_bind` port, default 41000) and,
on `AddrInUse`, advances by `PORT_BLOCK_STRIDE` (**10**) for up to `PORT_SLOT_COUNT` (**100**)
slots — the same two constants `scripts/run_stack.sh` uses. Only `AddrInUse` bumps; any other IO
error (e.g. permission) surfaces immediately. Exhausting all 100 slots is fatal. A bump is logged
at **WARN** (`port_block.bumped`) and the `server ready` INFO line reports the *actual* bound
ports plus `port_base_bumped`.

The base maps to `command=base+1`, `snapshot_flat=base+2`, `log=base+3`; `base=41000`
reproduces the historical ports. **`base+0` is reserved and bound by nothing** — it carried the
bincode snapshot socket until #388 retired it, and the slot was left empty rather than reclaimed
so that every client default, `run_stack.sh` export and already-published `ports.json` keeps
meaning exactly what it did. The block is therefore still four slots wide and three deep;
`block_free` in `run_stack.sh` probes only the three, so a stranger squatting on base+0 does not
push the server to another block. `SIM_PORT_BASE` is applied in
`load_simulation_config_from_env` (`resources.rs`) over whatever the config JSON specifies,
preserving each bind's host. A non-numeric or out-of-range value (needs `1 ≤ base` and
`base+3 ≤ 65535`) is warned and ignored rather than fatal.

**Config hot-reload** re-applies the **resolved** base (the `ResolvedPortBase` resource in
`server.rs`), not the configured one, so a reload of an unchanged file after a bump keeps the
live binds and doesn't spuriously trip `socket_changed=restart_required`. Rebinding live sockets
is out of scope; the reloaded config describes the ports the server actually holds.

## The handshake file

The ports file lets the client discover a bumped block. **Both halves derive its path from the
environment only** — no shared library, so the two derivations must be kept identical by hand:

`SIM_PORTS_FILE` verbatim if set (a full path, not a directory); else Windows
`%LOCALAPPDATA%\ShadowScale\ports.json`, macOS `$HOME/Library/Application Support/ShadowScale/ports.json`,
Linux/other `$XDG_STATE_HOME/ShadowScale/ports.json` (falling back to `$HOME/.local/state/…`).
Deliberately **not** the temp dir, where AV heuristics are most aggressive; parent dirs are
created as needed. On the client it is a **real filesystem path, not `res://`/`user://`** —
opened with `FileAccess.open(abs_path, READ)`.

Contents — **the exact key names are the contract**:

```json
{"host":"127.0.0.1","command":41001,"snapshot_flat":41002,"log":41003,"pid":1234}
```

**There is no `snapshot` key** — writing one for the reserved slot would point a reader at a port
nothing is listening on. `port_alloc.rs`'s `ports_file_round_trips_the_contract_keys` asserts its
absence.

Written after the block is bound and before the main loop, overwriting unconditionally.
**Failure to write is never fatal** — it logs a warning and continues (only auto-discovery is
lost). A `PortsFileGuard` removes it when `main` returns; a file left behind by a crash or a
**signal** (SIGINT/SIGTERM skip `Drop`) is expected and tolerated — the client validates the file
and falls back to the default block, which is what the recorded `pid` is for. No liveness
machinery lives here.

## Client: env var → ports file → hardcoded default

The packaged playtest build pins the three client-facing ports, but if they are busy at launch
the server binds a different free block and publishes it. Every resolver
(`Main._determine_stream_*` / `_determine_command_*`, `LogsPanel._determine_host` /
`_determine_port`) applies the same three-step precedence:

1. the explicit env var (`STREAM_HOST`/`STREAM_PORT`/`COMMAND_HOST`/`COMMAND_PORT`/
   `COMMAND_PROTO_PORT`/`LOG_HOST`/`LOG_PORT`) — **the env var always wins**, so
   `scripts/run_stack.sh`, which exports them explicitly, is completely unaffected by this
   feature;
2. the ports file;
3. the hardcoded constant.

**The stream port is `snapshot_flat`, and it is now the only snapshot port.** The legacy
bincode socket that sat at `base+0` is gone (#388) and its slot is reserved, so a client that
still reads a `snapshot` key gets nothing and falls back — where it used to connect to a live
socket and then **silently never render**. Do not rebind that slot for something else without
changing both halves: a stale client would connect to it expecting a world.

`ServerPortsFile.gd` is a **static-func script, not an autoload** (it holds no node state, is
needed by both `Main.gd` and `LogsPanel.gd` before the tree settles, and both `preload` it like
their other collaborators; the static cache gives the once-per-launch read without an
`[autoload]` entry). It reads and parses **once per launch and caches the result — including the
absent/invalid one**. Missing file, unreadable file, malformed JSON, missing keys and
non-integer/out-of-range ports **all degrade silently to the defaults**: a playtester running a
normally-ported server must never see an error because of this. (It parses via
`JSON.new().parse()` rather than the `JSON.parse_string()` static, which pushes an engine-level
ERROR to the console on malformed input.) Exactly one informational line is logged, and only
when the file is actually used. A **stale file from a crashed server is expected and tolerated**
— the existing connect/retry behaviour handles the refused connection. The client never writes,
deletes, or liveness-checks the file.

---
