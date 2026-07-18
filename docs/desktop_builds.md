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
  run.bat            <- dbl-click       run.command          <- dbl-click
  ShadowScaleClient.exe                 ShadowScaleClient.app  (.dylib + data inside)
  shadow_scale_godot.dll                server
  ShadowScaleClient.pck                 README.txt
  server.exe
  README.txt
```

The server binary is self-contained: every `*_config.json` is `include_str!`-baked
into it, so no data files ship alongside it and the default binds are correct.

## Automated releases (CI)

`.github/workflows/release.yml` runs on every push to `main` (and on manual
*Run workflow*). It builds **both** packages on a single `macos-latest` runner —
Windows via the cross-compile `build_windows.sh`, macOS via the native
`build_macos.sh` (Godot's macOS editor exports both clients from one templates
install) — then **deletes and recreates** a single pre-release tagged `latest` with
the two zips attached. Only the newest build is kept; the build number
(`github.run_number`) is in the release title and the asset names
(`ShadowScale-windows-b<N>.zip`, `ShadowScale-macos-b<N>.zip`).

The repo is public, so anyone can download from the **Releases** page. The builds
are **unsigned** — Windows SmartScreen and macOS Gatekeeper will warn; each
package's `README.txt` tells the tester how to proceed (macOS needs a one-line
`xattr` to clear the download quarantine).

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

- Both platforms' binaries are **unsigned**. Windows: SmartScreen *More info ▸ Run
  anyway*. macOS: `xattr -dr com.apple.quarantine <folder>` (or right-click ▸ Open),
  per the package README.
- `server` listens only on `127.0.0.1`; a first-run firewall prompt can be allowed
  or dismissed either way.
