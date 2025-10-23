# Shadow-Scale Prototype Workspace

This workspace scaffolds the **Prototype Plan (a)** headless simulation stack:

- `core_sim`: Bevy-based deterministic simulation core.
- `sim_schema`: Shared serialization schemas.
- `sim_runtime`: Shared runtime helpers re-used by tools.
- `clients/godot_thin_client`: Godot-based inspector connecting to the headless sim.
- `integration_tests`: Integration and determinism regression tests.

## Getting Started

```bash
cargo make setup
```

## Run the Prototype

Start the headless simulation server (provides snapshot + command sockets):

```bash
cargo run -p core_sim --bin server
```

Launch the Godot thin client inspector (requires Godot 4.2 or newer) to view live telemetry and issue commands:

```bash
godot4 --path clients/godot_thin_client src/Main.tscn
```

The scene connects to the default localhost sockets exposed by `core_sim`. Override the connection targets by exporting `STREAM_HOST`, `STREAM_PORT`, `COMMAND_HOST`, or `COMMAND_PORT` before starting Godot. The Terrain, Sentiment, Influencers, Corruption, Logs, and Commands tabs mirror the full debug surface; see `docs/godot_inspector_plan.md` for a guided tour.

Enable structured logs/metrics on the server (uses `tracing` with `RUST_LOG`):

```bash
RUST_LOG=info cargo run -p core_sim --bin server
```

The Godot inspector streams and renders the log feed automatically, including the recent-turn sparkline.

Run performance benchmarks:

```bash
cargo bench -p core_sim --bench turn_bench
```
Results (including HTML reports) are written under `target/criterion/turn/`.

## Developer Tooling

### Pre-commit Hooks

Install and enable the repo’s pre-commit hooks to lint and format code automatically:

```bash
pip install pre-commit  # or use your system package manager
pre-commit install
```

The hooks run `cargo fmt` and `cargo clippy` before each commit. You can execute them manually with:

```bash
pre-commit run --all-files
```

### Regenerating FlatBuffers bindings

Whenever the schema in `sim_schema/schemas/snapshot.fbs` changes, regenerate the Rust bindings with:

```bash
cargo xtask prepare-client
```

It regenerates the FlatBuffers bindings and refreshes the Godot GDExtension (`clients/godot_thin_client/native/bin/…`). No generated files are checked in, so commit any schema updates and rerun the command before pushing.

### Install Rust/Cargo

#### macOS
1. Install the Xcode Command Line Tools (required for compilers):
   ```bash
   xcode-select --install
   ```
2. Install Rustup (includes Cargo):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   - Choose the default install when prompted.
3. Reload your shell or source the environment updates:
   ```bash
   source "$HOME/.cargo/env"
   ```
4. Install `cargo-make` (used for repo tasks):
   ```bash
   cargo install cargo-make
   ```

If you do not already have GNU Make available, install it via Homebrew:
```bash
brew install make
```
Then optionally alias it to replace the BSD default:
```bash
echo 'alias make="gmake"' >> ~/.zshrc
```

#### Windows
1. Download and run the Rustup installer from [https://win.rustup.rs](https://win.rustup.rs) (or the main site’s Windows button).
2. Accept the default installation, ensuring the “MSVC” toolchain is selected.
3. After installation, open a new PowerShell or Command Prompt and verify:
   ```powershell
   cargo --version
   ```
4. If you do not have the Visual Studio build tools, rustup will prompt you with the required link; install them before continuing.
5. Install `cargo-make`:
   ```powershell
   cargo install cargo-make
   ```
6. Install GNU Make (if not already available) via [Chocolatey](https://community.chocolatey.org/packages/make) or the MSYS2 environment:
   - Using Chocolatey (Administrator PowerShell):
     ```powershell
     choco install make
     ```
   - Or install MSYS2 and include the `make` package.

#### Hex based board reference
https://www.redblobgames.com/grids/hexagons/
