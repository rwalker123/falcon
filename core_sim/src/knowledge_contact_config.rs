//! Tuning for the knowledge rider — how strongly each way two bands can be together teaches.
//!
//! Loaded from `data/knowledge_contact_config.json`, on the shared boot seam
//! (`crate::config_load`): an absent default path falls back to the builtin, anything else that is
//! there and wrong is a boot panic. Mirrors `connections_config.rs`.
//!
//! **A knowledge config, never `connections_config.json`** — the connection primitive has no rider
//! vocabulary (`.claude/rules/core_sim/connections.md`), so a channel rate parked beside its clocks
//! would be one rider's opinion wearing the primitive's clothes.
//!
//! **The numbers, and what they mean in play** (`docs/plan_contact_and_logistics.md` §Settled by
//! #531). Each is a channel's strength in the **practice units per turn** the ladder's `learn_rate`
//! is paid in (`1.0` = one worked turn at the food peak), so a full tie to a people that knows a
//! fully observable discovery, over a channel at `1.0`, learns it exactly as fast as practising it:
//!
//! - `channel_rates.watching` **0.1** — a live tie and nothing more. The weakest.
//! - `channel_rates.trade` **0.4** — a shipment landed between the two bands this turn.
//! - `channel_rates.road` **0.6** — the observer band stands on a road the subject band keeps.
//!
//! There is **no global multiplier** on top, and no default for a missing rate: each channel is one
//! lever, required.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_KNOWLEDGE_CONTACT_CONFIG: &str =
    include_str!("data/knowledge_contact_config.json");

/// Root configuration for the knowledge rider. The root is open so the file can carry its rationale
/// in `_comment*` keys; the rate table beneath it is closed.
#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeContactConfig {
    pub channel_rates: ContactChannelRates,
}

/// **Each channel's strength, in practice units per turn** — see the module docs. Every field is
/// required: a missing rate is a parse failure rather than a channel paced by a number nobody chose.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactChannelRates {
    /// A live tie toward the subject band.
    pub watching: f32,
    /// A shipment landed between the two bands this turn, in either direction.
    pub trade: f32,
    /// The observer band stands on a road tile the subject band keeps (a built rung, its keeping
    /// met — `crate::routes::Road::grants_sight`).
    pub road: f32,
}

impl KnowledgeContactConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_KNOWLEDGE_CONTACT_CONFIG)
                .expect("builtin knowledge contact config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, KnowledgeContactConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, KnowledgeContactConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| KnowledgeContactConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    /// A rate must be a finite, non-negative amount of practice. Zero is a legitimate setting — it
    /// switches that channel off — but a negative rate would un-teach a people by being near
    /// another, which no channel means.
    fn validate(&self) -> Result<(), KnowledgeContactConfigError> {
        let rates = &self.channel_rates;
        for (channel, rate) in [
            ("watching", rates.watching),
            ("trade", rates.trade),
            ("road", rates.road),
        ] {
            if !rate.is_finite() || rate < 0.0 {
                return Err(KnowledgeContactConfigError::Invalid {
                    field: format!("channel_rates.{channel}"),
                    value: rate,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum KnowledgeContactConfigError {
    #[error("failed to read knowledge contact config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse knowledge contact config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(
        "invalid knowledge contact config: {field} must be a finite, non-negative practice rate \
         (was {value})"
    )]
    Invalid { field: String, value: f32 },
}

impl ConfigLoadError for KnowledgeContactConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the knowledge contact configuration.
#[derive(Resource, Debug, Clone)]
pub struct KnowledgeContactConfigHandle(pub Arc<KnowledgeContactConfig>);

impl KnowledgeContactConfigHandle {
    pub fn new(config: Arc<KnowledgeContactConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<KnowledgeContactConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<KnowledgeContactConfig>) {
        self.0 = config;
    }
}

impl Default for KnowledgeContactConfigHandle {
    fn default() -> Self {
        Self(KnowledgeContactConfig::builtin())
    }
}

/// Metadata about the knowledge contact configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct KnowledgeContactConfigMetadata {
    path: Option<PathBuf>,
}

impl KnowledgeContactConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the knowledge contact config from environment (`KNOWLEDGE_CONTACT_CONFIG_PATH`) or the
/// default data path. Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`] for the rule and what it panics on.
pub fn load_knowledge_contact_config_from_env(
) -> (Arc<KnowledgeContactConfig>, KnowledgeContactConfigMetadata) {
    let (config, source) = load_config_from_env(
        "KNOWLEDGE_CONTACT_CONFIG_PATH",
        "knowledge_contact_config",
        "src/data/knowledge_contact_config.json",
        KnowledgeContactConfig::builtin,
        KnowledgeContactConfig::from_file,
    );
    (config, KnowledgeContactConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_with_the_deeper_channels_stronger() {
        let rates = &KnowledgeContactConfig::builtin().channel_rates;
        assert!(rates.watching > 0.0, "watching must teach something");
        assert!(
            rates.watching < rates.trade && rates.trade < rates.road,
            "the shipped first guess: the deeper the channel, the more is learned"
        );
    }

    #[test]
    fn a_missing_channel_rate_is_rejected_rather_than_defaulted() {
        let err = KnowledgeContactConfig::from_json_str(
            r#"{ "channel_rates": { "watching": 0.1, "trade": 0.4 } }"#,
        )
        .expect_err("a channel with no rate must not parse");
        assert!(matches!(err, KnowledgeContactConfigError::Parse(_)));
    }

    #[test]
    fn a_negative_or_non_finite_rate_is_rejected() {
        let err = KnowledgeContactConfig::from_json_str(
            r#"{ "channel_rates": { "watching": -0.1, "trade": 0.4, "road": 0.6 } }"#,
        )
        .expect_err("a negative rate would un-teach");
        assert!(
            matches!(err, KnowledgeContactConfigError::Invalid { ref field, .. } if field == "channel_rates.watching")
        );
    }
}
