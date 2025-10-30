use glob::glob;
use jsonschema::JSONSchema;
use serde_json::Value;
use sim_runtime::scripting::{manifest_schema, ScriptManifest};
use sim_runtime::{parse_command_line, CommandEnvelope, COMMAND_VERBS};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("command") => command_subcommand(args.collect()),
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
    eprintln!("       cargo xtask command [OPTIONS] <verb> [args...]");
    eprintln!("       cargo xtask help");
}

#[derive(Debug)]
struct CommandCliError(String);

impl CommandCliError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CommandCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CommandCliError {}

#[derive(Debug, Clone)]
enum ArgTokenKind {
    Positional(String),
    Flag { key: String, value: Option<String> },
}

#[derive(Debug, Clone)]
struct ArgToken {
    kind: ArgTokenKind,
    index: usize,
    consumed: bool,
}

struct CommandArgParser {
    tokens: Vec<ArgToken>,
}

impl CommandArgParser {
    fn new(tokens: Vec<ArgToken>) -> Self {
        Self { tokens }
    }

    fn flag_or_pos(&mut self, names: &[&str]) -> Result<Option<String>, CommandCliError> {
        if let Some(value) = self.take_flag_value(names)? {
            return Ok(Some(value));
        }
        Ok(self.take_positional())
    }

    fn flag_or_pos_required(
        &mut self,
        names: &[&str],
        description: &str,
    ) -> Result<String, CommandCliError> {
        self.flag_or_pos(names)?
            .ok_or_else(|| CommandCliError::new(format!("missing required argument {description}")))
    }

    fn take_flag_value(&mut self, names: &[&str]) -> Result<Option<String>, CommandCliError> {
        if names.is_empty() {
            return Ok(None);
        }
        let targets: Vec<String> = names.iter().map(|name| normalize_flag(name)).collect();
        for token in &mut self.tokens {
            if token.consumed {
                continue;
            }
            if let ArgTokenKind::Flag { key, value } = &token.kind {
                if targets.iter().any(|target| target == key) {
                    token.consumed = true;
                    return match value.clone() {
                        Some(value) => Ok(Some(value)),
                        None => Err(CommandCliError::new(format!(
                            "flag --{key} requires a value"
                        ))),
                    };
                }
            }
        }
        Ok(None)
    }

    fn take_positional(&mut self) -> Option<String> {
        for token in &mut self.tokens {
            if token.consumed {
                continue;
            }
            if let ArgTokenKind::Positional(value) = &token.kind {
                token.consumed = true;
                return Some(value.clone());
            }
        }
        None
    }

    fn into_remaining(self) -> Vec<ArgTokenKind> {
        let mut remaining: Vec<ArgToken> = self
            .tokens
            .into_iter()
            .filter(|token| !token.consumed)
            .collect();
        remaining.sort_by_key(|token| token.index);
        remaining.into_iter().map(|token| token.kind).collect()
    }
}

fn command_subcommand(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    const DEFAULT_HOST: &str = "127.0.0.1";
    const DEFAULT_PORT: u16 = 41001;

    let mut host = DEFAULT_HOST.to_string();
    let mut port = DEFAULT_PORT;
    let mut correlation_id: Option<u64> = None;
    let mut list_requested = false;
    let mut help_requested = false;

    let mut remaining: Vec<String> = Vec::new();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        if arg == "--" {
            remaining.extend(iter);
            break;
        }
        if !arg.starts_with("--") {
            remaining.push(arg);
            remaining.extend(iter);
            break;
        }

        let key = normalize_flag(arg.trim_start_matches("--"));
        match key.as_str() {
            "host" => {
                host = iter
                    .next()
                    .ok_or_else(|| CommandCliError::new("--host requires a value"))?;
            }
            "port" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CommandCliError::new("--port requires a value"))?;
                port = value.parse::<u16>().map_err(|err| {
                    CommandCliError::new(format!("invalid --port value '{value}': {err}"))
                })?;
            }
            "address" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CommandCliError::new("--address requires a value"))?;
                let (addr_host, addr_port) = parse_host_port(&value)?;
                host = addr_host;
                port = addr_port;
            }
            "correlation" | "correlation_id" | "correlationid" | "id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CommandCliError::new("--correlation requires a value"))?;
                correlation_id = Some(value.parse::<u64>().map_err(|err| {
                    CommandCliError::new(format!("invalid correlation id '{value}': {err}"))
                })?);
            }
            "list" => {
                list_requested = true;
            }
            "help" => {
                list_requested = true;
                help_requested = true;
            }
            _ => {
                remaining.push(arg);
                remaining.extend(iter);
                break;
            }
        }
    }

    let verb = remaining.first().cloned();

    if list_requested {
        if help_requested || verb.is_none() {
            print_command_usage_details();
        }
        print_command_list(verb.as_deref());
        return Ok(());
    }

    let verb = match verb {
        Some(value) => value,
        None => {
            print_command_usage_details();
            print_command_list(None);
            return Ok(());
        }
    };

    if verb.eq_ignore_ascii_case("help") {
        print_command_usage_details();
        print_command_list(None);
        return Ok(());
    }

    let command_args: Vec<String> = remaining.into_iter().skip(1).collect();
    let tokens = parse_command_tokens(&command_args);
    let pieces = build_command_pieces(&verb, tokens)?;
    let command_line = pieces.join(" ");
    let payload = parse_command_line(&command_line).map_err(|err| {
        CommandCliError::new(format!(
            "failed to parse command '{}': {}",
            command_line, err
        ))
    })?;
    let envelope = CommandEnvelope {
        payload,
        correlation_id,
    };
    send_proto_command(&host, port, &envelope)?;
    println!("Sent `{}` to {}:{}.", command_line, host, port);
    Ok(())
}

fn parse_host_port(input: &str) -> Result<(String, u16), CommandCliError> {
    let (host_raw, port_str) = input
        .rsplit_once(':')
        .ok_or_else(|| CommandCliError::new("--address expects <host>:<port> format"))?;
    let port = port_str
        .parse::<u16>()
        .map_err(|err| CommandCliError::new(format!("invalid port '{port_str}': {err}")))?;
    let host = host_raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    if host.is_empty() {
        return Err(CommandCliError::new(
            "--address host component cannot be empty",
        ));
    }
    Ok((host, port))
}

fn parse_command_tokens(args: &[String]) -> Vec<ArgToken> {
    let mut tokens = Vec::new();
    let mut index = 0usize;
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            for value in args.iter().skip(i + 1) {
                tokens.push(ArgToken {
                    kind: ArgTokenKind::Positional(value.clone()),
                    index,
                    consumed: false,
                });
                index += 1;
            }
            break;
        }

        if let Some(stripped) = arg.strip_prefix("--") {
            if let Some((name, value)) = stripped.split_once('=') {
                tokens.push(ArgToken {
                    kind: ArgTokenKind::Flag {
                        key: normalize_flag(name),
                        value: Some(value.to_string()),
                    },
                    index,
                    consumed: false,
                });
                i += 1;
                index += 1;
                continue;
            }
            let key = normalize_flag(stripped);
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                let value = args[i + 1].clone();
                tokens.push(ArgToken {
                    kind: ArgTokenKind::Flag {
                        key,
                        value: Some(value),
                    },
                    index,
                    consumed: false,
                });
                i += 2;
            } else {
                tokens.push(ArgToken {
                    kind: ArgTokenKind::Flag { key, value: None },
                    index,
                    consumed: false,
                });
                i += 1;
            }
        } else {
            tokens.push(ArgToken {
                kind: ArgTokenKind::Positional(arg.clone()),
                index,
                consumed: false,
            });
            i += 1;
        }
        index += 1;
    }
    tokens
}

fn build_command_pieces(
    verb: &str,
    raw_tokens: Vec<ArgToken>,
) -> Result<Vec<String>, CommandCliError> {
    let mut parser = CommandArgParser::new(raw_tokens);
    let verb_lower = verb.to_ascii_lowercase();
    let mut parts = vec![verb_lower.clone()];

    match verb_lower.as_str() {
        "turn" => {
            if let Some(steps) = parser.flag_or_pos(&["steps", "count"])? {
                parts.push(steps);
            }
        }
        "map_size" => {
            let width = parser.flag_or_pos_required(&["width", "w"], "map width")?;
            let height = parser.flag_or_pos_required(&["height", "h"], "map height")?;
            parts.push(width);
            parts.push(height);
        }
        "heat" => {
            let entity = parser.flag_or_pos_required(&["entity", "id"], "entity id")?;
            parts.push(entity);
            if let Some(delta) = parser.flag_or_pos(&["delta", "amount"])? {
                parts.push(delta);
            }
        }
        "order" => {
            let faction = parser.flag_or_pos_required(&["faction", "id"], "faction id")?;
            parts.push(faction);
            if let Some(directive) = parser.flag_or_pos(&["directive"])? {
                parts.push(directive);
            }
        }
        "rollback" => {
            let tick = parser.flag_or_pos_required(&["tick"], "tick")?;
            parts.push(tick);
        }
        "bias" => {
            let axis = parser.flag_or_pos_required(&["axis"], "axis")?;
            let value = parser.flag_or_pos_required(&["value"], "value")?;
            parts.push(axis);
            parts.push(value);
        }
        "support" | "suppress" => {
            let id = parser.flag_or_pos_required(&["id"], "influencer id")?;
            parts.push(id);
            if let Some(magnitude) = parser.flag_or_pos(&["magnitude", "value"])? {
                parts.push(magnitude);
            }
        }
        "support_channel" => {
            let id = parser.flag_or_pos_required(&["id"], "influencer id")?;
            let channel = parser.flag_or_pos_required(&["channel"], "support channel")?;
            parts.push(id);
            parts.push(channel);
            if let Some(magnitude) = parser.flag_or_pos(&["magnitude", "value"])? {
                parts.push(magnitude);
            }
        }
        "spawn_influencer" => {
            let mut scope_is_generation = false;
            if let Some(scope) = parser.flag_or_pos(&["scope"])? {
                scope_is_generation =
                    matches!(scope.to_ascii_lowercase().as_str(), "generation" | "gen");
                parts.push(scope);
            }
            if scope_is_generation {
                if let Some(generation) = parser.flag_or_pos(&["generation", "gen"])? {
                    parts.push(generation);
                }
            } else if parts.len() == 1 {
                if let Some(generation) = parser.flag_or_pos(&["generation", "gen"])? {
                    parts.push("generation".to_string());
                    parts.push(generation);
                }
            }
        }
        "counterintel_policy" => {
            let faction = parser.flag_or_pos_required(&["faction", "id"], "faction id")?;
            let policy = parser.flag_or_pos_required(&["policy"], "policy")?;
            parts.push(faction);
            parts.push(policy);
        }
        "counterintel_budget" => {
            let faction = parser.flag_or_pos_required(&["faction", "id"], "faction id")?;
            parts.push(faction);

            if let Some(reserve) = parser.take_flag_value(&["reserve", "set"])? {
                parts.push("reserve".to_string());
                parts.push(reserve);
            } else if let Some(delta) = parser.take_flag_value(&["delta", "adjust"])? {
                parts.push("delta".to_string());
                parts.push(delta);
            } else if let Some(value) = parser.flag_or_pos(&["value"])? {
                parts.push(value);
            } else if let Some(token) = parser.take_positional() {
                parts.push(token);
                if let Some(extra) = parser.take_positional() {
                    parts.push(extra);
                }
            }
        }
        "queue_espionage_mission" | "queue_mission" => {
            let mission = parser.flag_or_pos_required(&["mission", "mission_id"], "mission id")?;
            parts.push(mission);
        }
        "corruption" => {
            let subsystem = parser.flag_or_pos(&["subsystem"])?;
            let intensity = parser.flag_or_pos(&["intensity"])?;
            let exposure = parser.flag_or_pos(&["exposure", "exposure_ticks", "timer"])?;
            if let Some(subsystem) = subsystem {
                parts.push(subsystem);
            } else if intensity.is_some() || exposure.is_some() {
                parts.push("logistics".to_string());
            }
            if let Some(intensity) = intensity {
                parts.push(intensity);
            }
            if let Some(exposure) = exposure {
                parts.push(exposure);
            }
        }
        "reload_config" | "reload_sim_config" => {
            if let Some(kind) = parser.flag_or_pos(&["kind"])? {
                parts.push(kind);
            }
            if let Some(path) = parser.flag_or_pos(&["path"])? {
                parts.push(path);
            }
        }
        "crisis_autoseed" | "crisis_auto_seed" => {
            if let Some(value) = parser.flag_or_pos(&["enabled", "value", "state"])? {
                parts.push(value);
            }
        }
        "spawn_crisis" => {
            let archetype = parser.flag_or_pos_required(&["archetype", "id"], "archetype id")?;
            parts.push(archetype);
            if let Some(faction) = parser.flag_or_pos(&["faction", "faction_id"])? {
                parts.push(faction);
            }
        }
        _ => {}
    }

    let remaining = parser.into_remaining();
    for token in remaining {
        match token {
            ArgTokenKind::Positional(value) => parts.push(value),
            ArgTokenKind::Flag { key, value } => {
                parts.push(key);
                if let Some(value) = value {
                    parts.push(value);
                }
            }
        }
    }

    Ok(parts)
}

fn send_proto_command(
    host: &str,
    port: u16,
    envelope: &CommandEnvelope,
) -> Result<(), CommandCliError> {
    let bytes = envelope
        .encode_to_vec()
        .map_err(|err| CommandCliError::new(format!("failed to encode command: {err}")))?;
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|err| CommandCliError::new(format!("failed to connect to {addr}: {err}")))?;
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|err| {
            CommandCliError::new(format!("failed to write frame length to {addr}: {err}"))
        })?;
    stream.write_all(&bytes).map_err(|err| {
        CommandCliError::new(format!("failed to write command payload to {addr}: {err}"))
    })?;
    stream
        .flush()
        .map_err(|err| CommandCliError::new(format!("failed to flush command to {addr}: {err}")))?;
    Ok(())
}

fn print_command_usage_details() {
    eprintln!("Usage: cargo xtask command [OPTIONS] <verb> [args...]");
    eprintln!("Options:");
    eprintln!("  --host <host>           Command server host (default 127.0.0.1)");
    eprintln!("  --port <port>           Command server port (default 41001)");
    eprintln!("  --address <host:port>   Set host and port together");
    eprintln!("  --correlation <id>      Optional correlation id for the envelope");
    eprintln!("  --list                  Show available command verbs");
    eprintln!("  --help                  Show this help and available verbs");
    eprintln!("Examples:");
    eprintln!("  cargo xtask command turn --steps 5");
    eprintln!("  cargo xtask command spawn_crisis --archetype plague_bloom --faction 0");
}

fn print_command_list(filter: Option<&str>) {
    let entries: Vec<&sim_runtime::CommandVerbHelp> = if let Some(filter) = filter {
        let needle = normalize_flag(filter);
        let matches: Vec<&sim_runtime::CommandVerbHelp> = COMMAND_VERBS
            .iter()
            .filter(|entry| {
                entry.verb == needle
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| normalize_flag(alias) == needle)
            })
            .collect();
        if matches.is_empty() {
            println!("No command verb matching '{}' found.", filter);
            return;
        }
        matches
    } else {
        COMMAND_VERBS.iter().collect()
    };

    println!("Available runtime command verbs:");
    for entry in entries {
        println!("  {:<20}{}", entry.verb, entry.summary);
        println!("      usage: {}", entry.usage);
        if !entry.aliases.is_empty() {
            let alias_text = entry.aliases.join(", ");
            println!("      aliases: {}", alias_text);
        }
    }
}

fn normalize_flag(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('-')
        .replace('-', "_")
        .to_ascii_lowercase()
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
