use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let schema = Path::new("../sim_schema/schemas/snapshot.fbs");
    let out_dir = Path::new("src/generated");
    println!("cargo:rerun-if-changed={}", schema.display());
    fs::create_dir_all(out_dir).expect("failed to create generated dir");
    flatc_rust::run(flatc_rust::Args {
        inputs: &[schema],
        out_dir,
        ..Default::default()
    })
    .expect("Failed to run flatc");

    // flatc emits non-rustfmt output (2-space indent, over-long lines). Since the
    // bindings are checked into the repo, an unformatted regeneration otherwise fails
    // CI's `cargo fmt --check`. Format in place with the crate edition so the result
    // matches `cargo fmt`. Best-effort: a missing rustfmt only warns, never fails the build.
    let generated = out_dir.join("snapshot_generated.rs");
    match Command::new("rustfmt")
        .args(["--edition", "2021"])
        .arg(&generated)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => println!(
            "cargo:warning=rustfmt on {} exited with status {status}",
            generated.display()
        ),
        Err(err) => {
            println!("cargo:warning=could not run rustfmt on generated flatbuffers bindings: {err}")
        }
    }
}
