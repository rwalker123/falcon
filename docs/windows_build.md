# Windows Playtest Builds (cross-compiled from macOS)

You can produce a Windows `.exe` playtest package from a Mac (or Linux) without a
Windows machine or VM. `scripts/build_windows.sh` does the whole thing and emits a
ZIP a playtester unzips and runs.

## What the package is

The game is **client/server**: the Godot thin client connects to the `core_sim`
server over local TCP (the client's default ports — snapshot `41002`, command
`41001`, log `41003` — line up with the server's default binds, so no configuration
is needed). The package therefore ships **both** programs plus a launcher:

```
ShadowScale-windows/
  run.bat                 # double-click this — starts the server, then the client
  ShadowScaleClient.exe   # the Godot client
  server.exe              # the core_sim server (binds 127.0.0.1:41000-41003)
  shadow_scale_godot.dll  # the GDExtension, beside the client exe
  *.pck                   # Godot game data
  README.txt
```

The server binary is self-contained: every `*_config.json` is `include_str!`-baked
into it (`core_sim/src/resources.rs` etc.), so no data files ship alongside it and
the default binds are correct out of the box.

## One-time setup (on the build machine)

Everything targets the **MSVC ABI** (`x86_64-pc-windows-msvc`), matching Godot's
official Windows builds — this is the most compatible target for the GDExtension.

1. **Rust Windows target**
   ```bash
   rustup target add x86_64-pc-windows-msvc
   ```
2. **cargo-xwin** — drives the MSVC-ABI cross-compile and auto-downloads the
   Windows CRT/SDK on first use (into `~/Library/Caches/cargo-xwin`):
   ```bash
   cargo install cargo-xwin
   ```
3. **LLVM** — provides `lld` (the MSVC-ABI linker) and `clang-cl`:
   ```bash
   brew install llvm
   ```
   `build_windows.sh` adds the Homebrew llvm keg to `PATH` automatically.
4. **Godot 4.7 export templates** — install the version matching the editor
   (`4.7.stable`). Either use the editor (**Editor ▸ Manage Export Templates ▸
   Download and Install**), or install just the Windows templates by hand:
   ```bash
   # Download Godot_v4.7-stable_export_templates.tpz from the Godot release, then:
   TPL="$HOME/Library/Application Support/Godot/export_templates/4.7.stable"
   mkdir -p "$TPL"
   unzip -o -j Godot_v4.7-stable_export_templates.tpz \
     'templates/windows_release_x86_64.exe' \
     'templates/windows_debug_x86_64.exe' \
     'templates/windows_release_x86_64_console.exe' \
     'templates/windows_debug_x86_64_console.exe' \
     'templates/version.txt' -d "$TPL"
   ```
   (The full `.tpz` is ~1.3 GB across all platforms; extracting only the four
   Windows files above is enough for a Windows export.)

## Building

```bash
scripts/build_windows.sh
```

It runs, in order:

1. Cross-compiles `server.exe` **and** `shadow_scale_godot.dll` via `cargo xwin`.
2. Stages the DLL into `clients/godot_thin_client/native/bin/windows/`, where the
   `.gdextension` references it — so the Godot export bundles it automatically.
3. Exports the Godot client `.exe` headless
   (`godot --headless --export-release "Windows Desktop" …`), using the
   `Windows Desktop` preset in `clients/godot_thin_client/export_presets.cfg`.
4. Assembles `dist/windows/ShadowScale-windows/` (both exes, the DLL, `run.bat`,
   `README.txt`) and zips it to `dist/windows/ShadowScale-windows.zip`.

Hand the ZIP to a Windows playtester. They unzip it and double-click `run.bat`.

`GODOT_BIN=/path/to/godot scripts/build_windows.sh` if `godot` isn't on `PATH`.

## Why these choices

- **MSVC over GNU (mingw):** Godot's Windows editor and export templates are MSVC
  builds; a GNU-ABI GDExtension DLL loading into an MSVC Godot has a history of
  subtle issues, so the whole chain stays MSVC.
- **`cargo-xwin` over a Windows VM:** it downloads just the CRT/SDK headers+libs
  and links with `lld-link`, so the cross-build is a normal `cargo` invocation.
- **The scripting sandbox uses `rquickjs` (bundled quickjs-ng):** the previous
  `quick-js`/`libquickjs-sys` C source used POSIX-only headers and would not
  cross-compile to Windows. `rquickjs`'s bundled quickjs-ng supports Windows/MSVC.

## Known notes

- The exes are **not code-signed**, so Windows SmartScreen shows an
  "unknown publisher" prompt (More info ▸ Run anyway). Fine for playtesting.
- `server.exe` listens only on `127.0.0.1`; a first-run Windows Firewall prompt can
  be allowed or dismissed either way.
