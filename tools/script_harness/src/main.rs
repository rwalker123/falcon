use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::Value as JsonValue;
use shadow_scale_godot::{script_responses_to_json, ScriptManifest, ScriptRuntimeManager};

#[derive(Parser, Debug)]
#[command(author, version, about="Shared scripting harness for Shadow-Scale", long_about = None)]
struct Args {
    /// Path to manifest JSON file
    #[arg(long)]
    manifest: PathBuf,

    /// Override script source path (defaults to manifest entry)
    #[arg(long)]
    script: Option<PathBuf>,

    /// Number of ticks to run
    #[arg(long, default_value_t = 1)]
    ticks: u32,

    /// Simulation delta passed to each tick (seconds)
    #[arg(long, default_value_t = 0.0)]
    delta: f64,

    /// Execution budget per tick (milliseconds)
    #[arg(long, default_value_t = 8.0)]
    budget_ms: f64,

    /// Inject an event for the script (format: topic=payload.json or topic=@inline_json)
    #[arg(long = "event")]
    events: Vec<String>,
}

struct PendingEvent {
    topic: String,
    payload: JsonValue,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let manifest_path = args
        .manifest
        .canonicalize()
        .with_context(|| "Unable to canonicalize manifest path")?;
    let manifest_json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest at {}", manifest_path.display()))?;
    let manifest = ScriptManifest::parse_str(&manifest_json).with_context(|| {
        format!(
            "Failed to parse manifest JSON at {}",
            manifest_path.display()
        )
    })?;

    let script_path = match &args.script {
        Some(path) => path
            .canonicalize()
            .with_context(|| "Unable to canonicalize script path override")?,
        None => resolve_entry_path(&manifest_path, manifest.entry.as_str())
            .with_context(|| format!("Unable to resolve script entry '{}'", manifest.entry))?,
    };
    let script_source = fs::read_to_string(&script_path)
        .with_context(|| format!("Failed to read script at {}", script_path.display()))?;

    let manager = ScriptRuntimeManager::new();
    let script_id = manager
        .spawn_script(manifest.clone(), script_source)
        .with_context(|| "Failed to spawn script runtime")?;

    let events = parse_events(&manifest_path, &args.events)?;
    for event in events {
        manager
            .dispatch_event(script_id, event.topic.as_str(), event.payload.clone())
            .with_context(|| format!("Failed to dispatch event '{}'", event.topic))?;
    }

    for tick_index in 0..args.ticks {
        manager
            .tick(script_id, args.delta, args.budget_ms)
            .with_context(|| format!("Tick {} failed", tick_index))?;
        let responses = manager
            .poll_responses(script_id)
            .with_context(|| "Polling script responses failed")?;
        if !responses.is_empty() {
            let json = script_responses_to_json(responses);
            println!("=== tick {} responses ===", tick_index);
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
    }

    let session = manager
        .snapshot_session(script_id)
        .with_context(|| "Failed to snapshot session state")?;
    if !session.is_null() {
        println!("=== session ===");
        println!("{}", serde_json::to_string_pretty(&session)?);
    }

    Ok(())
}

fn parse_events(manifest_path: &Path, raw_events: &[String]) -> Result<Vec<PendingEvent>> {
    raw_events
        .iter()
        .map(|raw| {
            let (topic, payload_spec) = raw
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Event must be in topic=payload form"))?;
            let payload = if let Some(inline) = payload_spec.strip_prefix('@') {
                serde_json::from_str::<JsonValue>(inline).with_context(|| {
                    format!("Failed to parse inline JSON payload for event '{}'", topic)
                })?
            } else {
                let path = canonicalize_relative(manifest_path.parent(), payload_spec)?;
                let data = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read payload file {}", path.display()))?;
                serde_json::from_str::<JsonValue>(&data).with_context(|| {
                    format!("Failed to parse JSON payload file {}", path.display())
                })?
            };
            Ok(PendingEvent {
                topic: topic.to_string(),
                payload,
            })
        })
        .collect()
}

fn resolve_entry_path(manifest_path: &Path, entry: &str) -> Result<PathBuf> {
    if entry.is_empty() {
        return Err(anyhow::anyhow!("Manifest entry is empty"));
    }
    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Manifest path has no parent"))?;

    let trimmed = entry.trim_start_matches("./");
    let direct = manifest_dir.join(trimmed);
    if direct.exists() {
        return Ok(direct.canonicalize()?);
    }

    let stripped = entry
        .trim_start_matches("res://")
        .trim_start_matches("user://")
        .trim_start_matches('/');
    if !stripped.is_empty() {
        if let Some(root) = find_project_root(manifest_dir) {
            let candidate = root.join(stripped);
            if candidate.exists() {
                return Ok(candidate.canonicalize()?);
            }
        }
    }

    Err(anyhow::anyhow!(
        "Unable to resolve script entry '{}'. Provide --script override.",
        entry
    ))
}

fn canonicalize_relative(base: Option<&Path>, value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path.canonicalize()?)
    } else {
        let base_dir = base.unwrap_or(Path::new("."));
        Ok(base_dir.join(value).canonicalize()?)
    }
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("project.godot").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}
