# Suggested Commands
- `cargo make setup` — bootstrap workspace dependencies.
- `cargo run -p core_sim --bin server` — launch the headless simulation server with snapshot/log sockets.
- `godot4 --path clients/godot_thin_client src/Main.tscn` — start the inspector client against the local server.
- `RUST_LOG=info cargo run -p core_sim --bin server` — run the server with structured tracing output.
- `cargo test` / `cargo test -p core_sim` — execute Rust unit/integration suites.
- `cargo bench -p core_sim --bench turn_bench` — run the deterministic turn benchmark suite.
- `cargo xtask prepare-client` — regenerate FlatBuffers bindings and refresh the Godot GDExtension after schema changes.
- `pre-commit run --all-files` — invoke formatting/lint hooks (wraps `cargo fmt` + `cargo clippy`).