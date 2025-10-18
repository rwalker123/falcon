# Shadow-Scale Prototype Workspace

This workspace scaffolds the **Prototype Plan (a)** headless simulation stack:

- `core_sim`: Bevy-based deterministic simulation core.
- `sim_proto`: Shared serialization schemas.
- `cli_inspector`: Terminal-based inspector connecting to the headless sim.
- `integration_tests`: Integration and determinism regression tests.

## Getting Started

```bash
cargo make setup
```

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

## Next Steps
- Flesh out deterministic ECS systems in `core_sim`.
- Implement snapshot/delta serialization using types in `sim_proto`.
- Extend `cli_inspector` to display live stats and accept commands.
- Add benchmarks and profiling tasks per the technology plan.
