//! ⛔ **`sim_ai` links the wire, never the world.**
//!
//! `docs/plan_ai_opponents.md` §1: an AI is a player process, and the crate boundary is what makes
//! "the AI may not read the simulation directly" a build error rather than a review comment. This
//! test reads the crate's own manifest and refuses `core_sim` and `bevy` by name, so the boundary
//! cannot be crossed by a dependency line nobody noticed.
//!
//! It is an *integration* test on purpose: an integration test target is what makes `cargo test
//! --workspace` build the `sim_ai` binary itself, which `core_sim/tests/ai_seat_scenario.rs` then
//! drives against a real server.

use std::path::Path;

/// The two names a `sim_ai` dependency line may never carry.
const FORBIDDEN_DEPENDENCIES: [&str; 2] = ["core_sim", "bevy"];

#[test]
fn the_manifest_names_neither_core_sim_nor_bevy() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("the crate's manifest reads");
    let dependency_lines: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .collect();
    for forbidden in FORBIDDEN_DEPENDENCIES {
        let offending: Vec<&&str> = dependency_lines
            .iter()
            .filter(|line| {
                line.starts_with(&format!("{forbidden} "))
                    || line.starts_with(&format!("{forbidden}="))
                    || line.starts_with(&format!("\"{forbidden}\""))
            })
            .collect();
        assert!(
            offending.is_empty(),
            "sim_ai must never depend on `{forbidden}` — an AI reaches the world through a socket \
             and nothing else (docs/plan_ai_opponents.md §1); found {offending:?}"
        );
    }
}
