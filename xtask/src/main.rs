use glob::glob;
use jsonschema::JSONSchema;
use serde_json::Value;
use sim_runtime::scripting::{manifest_schema, ScriptManifest};
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("prepare-client") => prepare_client(),
        Some("godot-build") => godot_build(),
        Some("manifest-schema") => generate_manifest_schema(),
        Some("validate-manifests") => validate_manifests(),
        Some("help") | None => {
            print_usage();
            Ok(())
        }
        Some(cmd) => {
            eprintln!("Unknown xtask '{cmd}'.");
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Usage: cargo xtask prepare-client");
    eprintln!("       cargo xtask godot-build");
    eprintln!("       cargo xtask manifest-schema");
    eprintln!("       cargo xtask validate-manifests");
    eprintln!("       cargo xtask help");
}

fn prepare_client() -> Result<(), Box<dyn Error>> {
    regenerate_flatbuffers()?;
    format_generated_bindings()?;
    godot_build()?;
    Ok(())
}

fn regenerate_flatbuffers() -> Result<(), Box<dyn Error>> {
    let status = Command::new("cargo")
        .args(["build", "--locked", "-p", "shadow_scale_flatbuffers"])
        .status()?;

    if !status.success() {
        return Err("flatbuffers generation failed".into());
    }

    let generated = Path::new("shadow_scale_flatbuffers")
        .join("src")
        .join("generated")
        .join("snapshot_generated.rs");
    if !generated.exists() {
        return Err(format!("expected generated file at {}", generated.display()).into());
    }

    println!("Generated FlatBuffers bindings at {}", generated.display());
    Ok(())
}

fn format_generated_bindings() -> Result<(), Box<dyn Error>> {
    let generated = Path::new("shadow_scale_flatbuffers")
        .join("src")
        .join("generated")
        .join("snapshot_generated.rs");
    if generated.exists() {
        let status = Command::new("rustfmt").arg(&generated).status()?;
        if !status.success() {
            return Err("rustfmt failed for generated bindings".into());
        }
    }
    Ok(())
}

fn godot_build() -> Result<(), Box<dyn Error>> {
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "shadow_scale_godot"])
        .status()?;

    if !status.success() {
        return Err("cargo build failed".into());
    }

    let (artifact_name, platform_dir) = platform_artifact();

    let source = Path::new("target").join("release").join(artifact_name);

    if !source.exists() {
        return Err(format!("expected build artifact at {}", source.display()).into());
    }

    let bin_dir = Path::new("clients/godot_thin_client/native/bin").join(platform_dir);
    fs::create_dir_all(&bin_dir)?;
    let dest = bin_dir.join(artifact_name);

    let _ = fs::copy(&source, &dest)?;

    println!("Copied {} -> {}", source.display(), dest.display());

    Ok(())
}

fn generate_manifest_schema() -> Result<(), Box<dyn Error>> {
    let schema = manifest_schema();
    let json = serde_json::to_string_pretty(&schema)?;
    let path = Path::new("docs").join("scripting_manifest.schema.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, json)?;
    println!("Wrote manifest schema to {}", path.display());
    Ok(())
}

fn validate_manifests() -> Result<(), Box<dyn Error>> {
    let schema = manifest_schema();
    let schema_value = serde_json::to_value(&schema)?;
    let compiled = JSONSchema::compile(&schema_value)
        .map_err(|err| Box::<dyn Error>::from(err.to_string()))?;
    let mut had_errors = false;

    for entry in glob("clients/**/manifest.json")? {
        let path = entry?;
        let data = fs::read_to_string(&path)?;
        let json: Value = serde_json::from_str(&data)?;

        if let Err(errors) = compiled.validate(&json) {
            had_errors = true;
            eprintln!("Schema validation failed for {}:", path.display());
            for error in errors {
                eprintln!("  - {}", error);
            }
        }

        let manifest: ScriptManifest = serde_json::from_str(&data)?;
        if let Err(err) = manifest.validate() {
            had_errors = true;
            eprintln!(
                "Capability validation failed for {}: {}",
                path.display(),
                err
            );
        }
    }

    if had_errors {
        return Err("manifest validation failed".into());
    }

    println!("All manifests validated successfully.");
    Ok(())
}

fn platform_artifact() -> (&'static str, &'static str) {
    #[cfg(target_os = "macos")]
    {
        ("libshadow_scale_godot.dylib", "macos")
    }

    #[cfg(target_os = "linux")]
    {
        ("libshadow_scale_godot.so", "linux")
    }

    #[cfg(target_os = "windows")]
    {
        ("shadow_scale_godot.dll", "windows")
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    compile_error!("Unsupported target OS for godot-build xtask");
}
