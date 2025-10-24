//! Scripting capability registry and helper utilities shared by host runtimes.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
}
