# Desktop Playtest Builds (Windows + macOS)

Playtest packages for Windows and macOS are produced two ways:

- **Automatically in CI** — every merge to `main` refreshes a rolling **`latest`**
  pre-release on GitHub with both zips attached (see *Automated releases* below).
- **Locally**, with `scripts/build_windows.sh` / `scripts/build_macos.sh` — the same
  scripts CI uses, so a developer build and a CI build are identical.

## What a package is

The game is **client/server**: the Godot thin client connects to the `core_sim`
server over local TCP (the client's default ports — snapshot `41002`, command
`41001`, log `41003` — line up with the server's default binds, so no configuration
is needed). Each package therefore ships **both** programs plus a launcher.

```
ShadowScale-windows/                 ShadowScale-macos/
  ShadowScale.exe    <- dbl-click       ShadowScale.app      <- dbl-click
  ShadowScaleClient.exe                   Contents/MacOS/shadowscale_launcher
  shadow_scale_godot.dll                  Contents/Helpers/server
  ShadowScaleClient.pck                   Contents/Helpers/ShadowScaleClient.app
  server.exe                            README.txt
  README.txt
```

The server binary is self-contained: every `*_config.json` is `include_str!`-baked
into it, so no data files ship alongside it and the default binds are correct.

### The launcher

Both packages are started by one cross-platform Rust binary, the `launcher` crate
(`launcher/src/main.rs`). It replaced the previous `run.bat` / `run.command` pair —
two scripts that had to be maintained in parallel and could not express the two
things that actually matter here:

- **Readiness is observed, not guessed.** Both scripts slept a fixed two seconds
  and hoped. The launcher instead hands *both* children an explicit
  `SIM_PORTS_FILE` and waits for the server to publish it (see
  `core_sim/src/port_alloc.rs` and `clients/godot_thin_client/src/scripts/ServerPortsFile.gd`,
  which both honour that variable verbatim). Owning the path also removes any
  chance of reading a stale handshake left by an earlier crashed run, and it means
  the client follows the server automatically when the default port block is busy
  and the server bumps.
- **The server cannot be orphaned on Windows.** `run.bat`'s `taskkill` only ran on
  the clean exit path, so a client crash left `server.exe` alive holding the ports.
  The launcher puts the server in a **Job Object** with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so Windows reaps it even if the launcher
  itself is killed.

It also sets the server's working directory to the per-user data directory, because
`export_map` writes `exports/…json` relative to CWD and a process launched from
inside a `.app` bundle inherits `/`.

On macOS the launcher is the bundle executable and the Godot client `.app` nests in
`Contents/Helpers/` — the standard pattern for a bundle shipping a helper app. The
launcher execs the inner binary directly rather than via `open`, so it keeps the
child PID. It never touches the window server, so the only Dock tile is the
client's.

## Automated releases (CI)

`.github/workflows/release.yml` runs on every push to `main` (and on manual
*Run workflow*). It builds **both** packages on a single `macos-latest` runner —
Windows via the cross-compile `build_windows.sh`, macOS via the native
`build_macos.sh` (Godot's macOS editor exports both clients from one templates
install) — then **deletes and recreates** a single pre-release tagged `latest` with
the two zips attached. Only the newest build is kept; the build number
(`github.run_number`) is in the release title and the asset names
(`ShadowScale-windows-b<N>.zip`, `ShadowScale-macos-b<N>.zip`).

The repo is public, so anyone can download from the **Releases** page. Neither build
is signed by a paid developer account — Windows SmartScreen and macOS Gatekeeper
will warn once; each package's `README.txt` tells the tester how to proceed (macOS
needs a one-time approval in System Settings ▸ Privacy & Security).

To cut a build without merging: **Actions ▸ Desktop Playtest Release ▸ Run workflow**.

## Building locally

### Windows (cross-compiled from macOS/Linux)

One-time setup:

```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin        # downloads the Windows CRT/SDK on first build
brew install llvm               # provides lld (the MSVC-ABI linker)
# + Godot 4.7 Windows export templates (see "Export templates" below)
```

```bash
scripts/build_windows.sh        # -> dist/windows/ShadowScale-windows.zip
```

Everything targets the **MSVC ABI** (`x86_64-pc-windows-msvc`), matching Godot's
official Windows builds — the most compatible target for the GDExtension.

### macOS (native, on a Mac)

Needs Rust stable, `flatc` on `PATH`, and Godot 4.7 with the macOS export
templates. Then:

```bash
scripts/build_macos.sh          # -> dist/macos/ShadowScale-macos.zip
```

The client is exported as a **universal** `.app`, so it runs natively on Apple
Silicon and Intel. (This requires `rendering/textures/vram_compression/import_etc2_astc`
in `clients/godot_thin_client/project.godot` — already enabled.)

`GODOT_BIN=/path/to/godot` if `godot` isn't on `PATH`; `BUILD_NUMBER=<n>` to suffix
the zip name.

### Export templates

Install the templates matching the editor (`4.7.stable`) via the editor
(**Editor ▸ Manage Export Templates**), or by hand:

```bash
TPL="$HOME/Library/Application Support/Godot/export_templates/4.7.stable"
mkdir -p "$TPL"
# From Godot_v4.7-stable_export_templates.tpz — Windows and/or macOS entries:
unzip -o -j Godot_v4.7-stable_export_templates.tpz \
  'templates/windows_release_x86_64.exe' 'templates/windows_debug_x86_64.exe' \
  'templates/windows_release_x86_64_console.exe' 'templates/windows_debug_x86_64_console.exe' \
  'templates/macos.zip' 'templates/version.txt' -d "$TPL"
```

## Why these choices

- **MSVC over GNU (mingw) for Windows:** Godot's Windows editor and templates are
  MSVC builds; a GNU-ABI GDExtension in an MSVC Godot has a history of subtle
  issues, so the whole chain stays MSVC.
- **`cargo-xwin` over a Windows VM:** it downloads just the CRT/SDK and links with
  `lld-link`, so the cross-build is a normal `cargo` invocation.
- **CI builds both on one macOS runner:** reuses the exact local build scripts (so
  CI ≡ local) and one Godot install exports both targets — the mac editor can
  export a Windows client.
- **The scripting sandbox uses `rquickjs` (bundled quickjs-ng):** the previous
  `quick-js`/`libquickjs-sys` C source used POSIX-only headers and would not
  cross-compile to Windows/MSVC.

## Known notes

- Neither platform is signed by a paid developer account. Windows: SmartScreen
  *More info ▸ Run anyway*. macOS: **System Settings ▸ Privacy & Security ▸ Open
  Anyway**, once — macOS 15 Sequoia removed the old right-click ▸ Open bypass, so
  that is now the only route and the package README says so.
- The macOS bundle **is ad-hoc signed** (`codesign -s -`, done inside-out in
  `build_macos.sh`: server → client `.app` → launcher → outer bundle). This is not
  notarization and does not remove the prompt; it exists because *unsigned* arm64
  binaries can fail to launch outright with "app is damaged", which offers the
  player no override at all. Godot's own export signing stays off
  (`codesign/codesign=0`) so all signing happens in one place, in one order.
  Removing the prompt entirely needs an Apple Developer ID ($99/yr) plus
  notarization — the natural next step if playtester friction warrants it.
- `server` listens only on `127.0.0.1`; a first-run firewall prompt can be allowed
  or dismissed either way.
