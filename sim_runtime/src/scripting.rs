//! Scripting capability registry and helper utilities shared by host runtimes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::fmt;

/// Describes a scripting capability exposed to user scripts.
#[derive(Debug)]
pub struct CapabilitySpec {
    /// Unique identifier exposed in manifests.
    pub id: &'static str,
    /// Player/author facing description.
    pub description: &'static str,
    /// Host requests that become available when the capability is granted.
    pub host_requests: &'static [&'static str],
    /// Telemetry topics that scripts may subscribe to when the capability is granted.
    pub subscriptions: &'static [&'static str],
    /// Whether the capability enables session storage read/write.
    pub session_access: SessionAccess,
    /// Whether the capability enables alerts.
    pub allows_alerts: bool,
}

impl CapabilitySpec {
    /// Returns true if this capability covers the provided telemetry topic.
    pub fn allows_subscription(&self, topic: &str) -> bool {
        self.subscriptions
            .iter()
            .any(|allowed| topic_matches(topic, allowed))
    }

    /// Returns true if this capability allows invoking the given host request.
    pub fn allows_host_request(&self, op: &str) -> bool {
        self.host_requests.contains(&op)
    }
}

/// Enumerates the level of access scripts have to session storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAccess {
    None,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptManifestRef {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
}

impl ScriptManifestRef {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        manifest_path: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            manifest_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimScriptState {
    pub manifest: ScriptManifestRef,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub session: JsonValue,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ScriptManifest {
    pub id: String,
    pub version: String,
    pub entry: String,
    #[serde(default)]
    /// List of capability IDs required by this script. Capabilities must be declared here before they can be referenced by subscriptions.
    pub capabilities: Vec<String>,
    #[serde(default)]
    /// List of telemetry topics to subscribe to. Each topic must be covered by at least one declared capability, as enforced by validation logic.
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub config: Option<JsonValue>,
    #[serde(skip)]
    #[schemars(skip)]
    pub manifest_path: Option<String>,
}

impl ScriptManifest {
    pub fn parse_str(contents: &str) -> Result<Self, ManifestValidationError> {
        let manifest: ScriptManifest = serde_json::from_str(contents).map_err(|err| {
            ManifestValidationError::single(format!("failed to parse manifest JSON: {err}"))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        validate_manifest(self, capability_registry())
    }
}

#[derive(Debug, Clone)]
pub struct ManifestValidationError {
    errors: Vec<String>,
}

impl ManifestValidationError {
    pub fn new(errors: Vec<String>) -> Self {
        Self { errors }
    }

    pub fn single(message: impl Into<String>) -> Self {
        Self {
            errors: vec![message.into()],
        }
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

impl fmt::Display for ManifestValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.errors.join("; "))
    }
}

impl std::error::Error for ManifestValidationError {}

const CAPABILITY_SPECS: &[CapabilitySpec] = &[
    CapabilitySpec {
        id: "telemetry.subscribe",
        description: "Grants access to telemetry topics exposed by the host (snapshots, deltas, overlays, discovery ledgers, log streams).",
        host_requests: &["telemetry.subscribe", "telemetry.unsubscribe"],
        subscriptions: &[
            "world.snapshot",
            "world.delta",
            "overlays.*",
            "ledger.discovery",
            "log.events",
        ],
        session_access: SessionAccess::None,
        allows_alerts: false,
    },
    CapabilitySpec {
        id: "ui.compose",
        description: "Allows scripts to publish declarative UI descriptions rendered in the client.",
        host_requests: &["ui.compose"],
        subscriptions: &[],
        session_access: SessionAccess::None,
        allows_alerts: false,
    },
    CapabilitySpec {
        id: "commands.issue",
        description: "Allows dispatching vetted command endpoints via the host command bridge.",
        host_requests: &["commands.issue"],
        subscriptions: &["commands.issue.result"],
        session_access: SessionAccess::None,
        allows_alerts: false,
    },
    CapabilitySpec {
        id: "storage.session",
        description: "Grants read/write access to per-session key/value storage that persists with saves.",
        host_requests: &["storage.session.get", "storage.session.set", "storage.session.clear"],
        subscriptions: &[],
        session_access: SessionAccess::ReadWrite,
        allows_alerts: false,
    },
    CapabilitySpec {
        id: "alerts.emit",
        description: "Allows emitting gameplay alerts/toasts that surface in the client inspector.",
        host_requests: &["alerts.emit"],
        subscriptions: &["alerts.*"],
        session_access: SessionAccess::None,
        allows_alerts: true,
    },
];

fn topic_matches(topic: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        topic.starts_with(prefix)
    } else if pattern.ends_with('.') {
        topic.starts_with(pattern)
    } else {
        topic == pattern
    }
}

/// Static registry containing all available capability specifications.
pub struct CapabilityRegistry {
    specs: &'static [CapabilitySpec],
}

impl CapabilityRegistry {
    /// Returns every capability specification.
    pub fn specs(&self) -> &'static [CapabilitySpec] {
        self.specs
    }

    /// Finds a capability spec by identifier.
    pub fn get(&self, id: &str) -> Option<&'static CapabilitySpec> {
        self.specs.iter().find(|spec| spec.id == id)
    }
}

const REGISTRY: CapabilityRegistry = CapabilityRegistry {
    specs: CAPABILITY_SPECS,
};

/// Returns the global capability registry.
pub const fn capability_registry() -> &'static CapabilityRegistry {
    &REGISTRY
}

fn validate_manifest(
    manifest: &ScriptManifest,
    registry: &CapabilityRegistry,
) -> Result<(), ManifestValidationError> {
    let mut errors = Vec::new();

    if manifest.id.trim().is_empty() {
        errors.push("manifest id cannot be empty".to_string());
    }
    if manifest.version.trim().is_empty() {
        errors.push("manifest version cannot be empty".to_string());
    }
    if manifest.entry.trim().is_empty() {
        errors.push("manifest entry cannot be empty".to_string());
    }

    let mut declared = HashSet::new();
    let mut resolved_specs: Vec<&'static CapabilitySpec> = Vec::new();

    for capability in &manifest.capabilities {
        let entry = capability.trim();
        if entry.is_empty() {
            errors.push("capability entries cannot be blank".to_string());
            continue;
        }
        if !declared.insert(entry.to_string()) {
            errors.push(format!("duplicate capability '{entry}'"));
            continue;
        }
        match registry.get(entry) {
            Some(spec) => resolved_specs.push(spec),
            None => errors.push(format!("unknown capability '{entry}'")),
        }
    }

    for topic in &manifest.subscriptions {
        let trimmed = topic.trim();
        if trimmed.is_empty() {
            errors.push("subscription entries cannot be blank".to_string());
            continue;
        }
        if !resolved_specs
            .iter()
            .any(|spec| spec.allows_subscription(trimmed))
        {
            errors.push(format!(
                "subscription '{trimmed}' not covered by declared capabilities"
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ManifestValidationError::new(errors))
    }
}

pub fn manifest_schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(ScriptManifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for spec in CAPABILITY_SPECS {
            assert!(
                seen.insert(spec.id),
                "duplicate capability id detected: {}",
                spec.id
            );
        }
    }

    #[test]
    fn wildcard_topics_match() {
        let spec = CapabilitySpec {
            id: "alerts.emit",
            description: "",
            host_requests: &[],
            subscriptions: &["alerts.*"],
            session_access: SessionAccess::None,
            allows_alerts: true,
        };
        assert!(spec.allows_subscription("alerts.demo"));
        assert!(!spec.allows_subscription("alert"));
    }

    #[test]
    fn manifest_rejects_unknown_capability() {
        let json = r#"{
            "id": "demo",
            "version": "0.1.0",
            "entry": "./index.js",
            "capabilities": ["not_a_cap"],
            "subscriptions": []
        }"#;
        let err = ScriptManifest::parse_str(json).expect_err("expected parse failure");
        assert!(err.to_string().contains("unknown capability"));
    }

    #[test]
    fn manifest_rejects_uncovered_subscription() {
        let json = r#"{
            "id": "demo",
            "version": "0.1.0",
            "entry": "./index.js",
            "capabilities": ["commands.issue"],
            "subscriptions": ["world.snapshot"]
        }"#;
        let err = ScriptManifest::parse_str(json).expect_err("expected parse failure");
        assert!(err
            .to_string()
            .contains("subscription 'world.snapshot' not covered"));
    }

    #[test]
    fn manifest_accepts_valid_capabilities() {
        let json = r#"{
            "id": "demo",
            "version": "0.1.0",
            "entry": "./index.js",
            "capabilities": ["telemetry.subscribe", "commands.issue"],
            "subscriptions": ["world.snapshot"]
        }"#;
        let manifest = ScriptManifest::parse_str(json).expect("manifest should be valid");
        assert_eq!(manifest.capabilities.len(), 2);
    }
}
