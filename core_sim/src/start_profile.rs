use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use serde_json::Value;
use sim_schema::{
    CampaignInventoryEntryState, CampaignLabel as SchemaCampaignLabel, CampaignProfileState,
    CampaignStartingUnitState,
};
use thiserror::Error;

pub const BUILTIN_START_PROFILES: &str = include_str!("data/start_profiles.json");
pub const BUILTIN_START_PROFILE_KNOWLEDGE_TAGS: &str =
    include_str!("data/start_profile_knowledge_tags.json");

#[derive(Debug, Clone, Deserialize)]
struct StartProfilesData {
    profiles: Vec<StartProfile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DisplayTextRecord {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, rename = "loc_key")]
    pub loc_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DisplayText {
    Plain(String),
    Record(DisplayTextRecord),
}

impl DisplayText {
    pub fn into_record(self) -> DisplayTextRecord {
        match self {
            DisplayText::Plain(value) => DisplayTextRecord {
                text: Some(value),
                loc_key: None,
            },
            DisplayText::Record(record) => record,
        }
    }

    pub fn as_record(&self) -> DisplayTextRecord {
        match self {
            DisplayText::Plain(value) => DisplayTextRecord {
                text: Some(value.clone()),
                loc_key: None,
            },
            DisplayText::Record(record) => record.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartProfile {
    pub id: String,
    #[serde(default)]
    pub manual_ref: Option<String>,
    #[serde(default)]
    pub display_title: Option<DisplayText>,
    #[serde(default)]
    pub display_subtitle: Option<DisplayText>,
    #[serde(flatten)]
    pub overrides: StartProfileOverrides,
}

impl StartProfile {
    pub fn placeholder(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            manual_ref: None,
            display_title: None,
            display_subtitle: None,
            overrides: StartProfileOverrides::default(),
        }
    }

    pub fn overrides(&self) -> &StartProfileOverrides {
        &self.overrides
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartProfileOverrides {
    #[serde(default)]
    pub starting_units: Vec<StartingUnitSpec>,
    #[serde(default)]
    pub starting_knowledge_tags: Vec<String>,
    #[serde(default)]
    pub inventory: Vec<InventoryEntry>,
    #[serde(default)]
    pub survey_radius: Option<u32>,
    #[serde(default)]
    pub fog_mode: Option<FogMode>,
    #[serde(default)]
    pub ai_profile_overrides: HashMap<String, Value>,
    #[serde(default)]
    pub victory_modes_enabled: Vec<String>,
}

impl StartProfileOverrides {
    pub fn from_profile(profile: &StartProfile) -> Self {
        profile.overrides.clone()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeTagDefinition {
    pub discovery_id: u32,
    #[serde(default = "default_tag_progress")]
    pub progress: f32,
    #[serde(default = "default_tag_fidelity")]
    pub fidelity: f32,
}

impl KnowledgeTagDefinition {
    pub fn discovery_id(&self) -> u32 {
        self.discovery_id
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn fidelity(&self) -> f32 {
        self.fidelity
    }
}

fn default_tag_progress() -> f32 {
    0.5
}

fn default_tag_fidelity() -> f32 {
    0.75
}

#[derive(Debug, Clone)]
pub struct StartProfileKnowledgeTags {
    tags: HashMap<String, KnowledgeTagDefinition>,
}

impl StartProfileKnowledgeTags {
    pub fn builtin() -> Arc<Self> {
        Self::from_json_str(BUILTIN_START_PROFILE_KNOWLEDGE_TAGS)
            .map(Arc::new)
            .expect("builtin start profile knowledge tags should parse")
    }

    pub fn from_json_str(input: &str) -> Result<Self, KnowledgeTagCatalogError> {
        let tags: HashMap<String, KnowledgeTagDefinition> = serde_json::from_str(input)?;
        Ok(Self { tags })
    }

    pub fn from_file(path: &Path) -> Result<Self, KnowledgeTagCatalogError> {
        let contents =
            fs::read_to_string(path).map_err(|source| KnowledgeTagCatalogError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    pub fn get(&self, tag: &str) -> Option<&KnowledgeTagDefinition> {
        self.tags.get(tag)
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum KnowledgeTagCatalogError {
    #[error("failed to parse start profile knowledge tags: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read start profile knowledge tags from {path:?}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartingUnitSpec {
    pub kind: String,
    #[serde(default = "default_unit_count")]
    pub count: u32,
    #[serde(default)]
    pub position: Option<[i32; 2]>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Default for StartingUnitSpec {
    fn default() -> Self {
        Self {
            kind: String::new(),
            count: default_unit_count(),
            position: None,
            tags: Vec::new(),
        }
    }
}

fn default_unit_count() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct InventoryEntry {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FogMode {
    #[default]
    Standard,
    Revealed,
    Shroud,
}

impl FogMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FogMode::Standard => "standard",
            FogMode::Revealed => "revealed",
            FogMode::Shroud => "shroud",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartProfiles {
    profiles: Vec<StartProfile>,
    index: HashMap<String, usize>,
}

impl StartProfiles {
    pub fn builtin() -> Arc<Self> {
        Self::from_json_str(BUILTIN_START_PROFILES)
            .map(Arc::new)
            .expect("builtin start profiles should parse")
    }

    pub fn from_json_str(input: &str) -> Result<Self, StartProfilesError> {
        let data: StartProfilesData = serde_json::from_str(input)?;
        Self::from_data(data)
    }

    pub fn from_file(path: &Path) -> Result<Self, StartProfilesError> {
        let contents =
            fs::read_to_string(path).map_err(|source| StartProfilesError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    fn from_data(data: StartProfilesData) -> Result<Self, StartProfilesError> {
        let mut index = HashMap::new();
        for (idx, profile) in data.profiles.iter().enumerate() {
            if index.insert(profile.id.clone(), idx).is_some() {
                return Err(StartProfilesError::DuplicateId(profile.id.clone()));
            }
        }

        Ok(Self {
            profiles: data.profiles,
            index,
        })
    }

    pub fn get(&self, id: &str) -> Option<&StartProfile> {
        self.index.get(id).and_then(|idx| self.profiles.get(*idx))
    }

    pub fn first(&self) -> Option<&StartProfile> {
        self.profiles.first()
    }

    pub fn iter(&self) -> impl Iterator<Item = &StartProfile> {
        self.profiles.iter()
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }
}

#[derive(Debug, Error)]
pub enum StartProfilesError {
    #[error("failed to parse start profiles: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read start profiles from {path:?}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("duplicate start profile id `{0}`")]
    DuplicateId(String),
}

#[derive(Resource, Debug, Clone)]
pub struct StartProfilesHandle(Arc<StartProfiles>);

impl StartProfilesHandle {
    pub fn new(profiles: Arc<StartProfiles>) -> Self {
        Self(profiles)
    }

    pub fn get(&self) -> Arc<StartProfiles> {
        self.0.clone()
    }
}

#[derive(Resource, Debug, Clone)]
pub struct StartProfilesMetadata {
    path: Option<PathBuf>,
}

impl StartProfilesMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

pub fn load_start_profiles_from_env() -> (Arc<StartProfiles>, StartProfilesMetadata) {
    let override_path = env::var("START_PROFILES_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/start_profiles.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match StartProfiles::from_file(&path) {
            Ok(profiles) => {
                tracing::info!(
                    target: "shadow_scale::campaign",
                    path = %path.display(),
                    profiles = profiles.len(),
                    "start_profiles.loaded=file"
                );
                return (Arc::new(profiles), StartProfilesMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::campaign",
                    path = %path.display(),
                    error = %err,
                    "start_profiles.load_failed"
                );
            }
        }
    }

    let profiles = StartProfiles::builtin();
    tracing::info!(target: "shadow_scale::campaign", "start_profiles.loaded=builtin");
    (profiles, StartProfilesMetadata::new(None))
}

#[derive(Resource, Debug, Clone)]
pub struct StartProfileKnowledgeTagsHandle(Arc<StartProfileKnowledgeTags>);

impl StartProfileKnowledgeTagsHandle {
    pub fn new(tags: Arc<StartProfileKnowledgeTags>) -> Self {
        Self(tags)
    }

    pub fn get(&self) -> Arc<StartProfileKnowledgeTags> {
        self.0.clone()
    }
}

#[derive(Resource, Debug, Clone)]
pub struct StartProfileKnowledgeTagsMetadata {
    path: Option<PathBuf>,
}

impl StartProfileKnowledgeTagsMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

pub fn load_start_profile_knowledge_tags_from_env() -> (
    Arc<StartProfileKnowledgeTags>,
    StartProfileKnowledgeTagsMetadata,
) {
    let override_path = env::var("START_PROFILE_KNOWLEDGE_TAGS_PATH")
        .ok()
        .map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/data/start_profile_knowledge_tags.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match StartProfileKnowledgeTags::from_file(&path) {
            Ok(tags) => {
                tracing::info!(
                    target: "shadow_scale::campaign",
                    path = %path.display(),
                    tags = tags.len(),
                    "start_profile_knowledge_tags.loaded=file"
                );
                return (
                    Arc::new(tags),
                    StartProfileKnowledgeTagsMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::campaign",
                    path = %path.display(),
                    error = %err,
                    "start_profile_knowledge_tags.load_failed"
                );
            }
        }
    }

    let tags = StartProfileKnowledgeTags::builtin();
    tracing::info!(
        target: "shadow_scale::campaign",
        "start_profile_knowledge_tags.loaded=builtin"
    );
    (tags, StartProfileKnowledgeTagsMetadata::new(None))
}

#[derive(Clone, Debug, Default)]
pub struct CampaignText {
    pub text: Option<String>,
    pub loc_key: Option<String>,
}

impl CampaignText {
    fn from_display(display: Option<&DisplayText>, fallback: Option<&str>) -> Self {
        match display {
            Some(value) => {
                let record = value.as_record();
                Self {
                    text: record.text,
                    loc_key: record.loc_key,
                }
            }
            None => Self {
                text: fallback.map(|v| v.to_string()),
                loc_key: None,
            },
        }
    }

    pub fn text_as_str(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn loc_key(&self) -> Option<&str> {
        self.loc_key.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.loc_key.is_none()
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CampaignLabel {
    pub profile_id: String,
    pub title: CampaignText,
    pub subtitle: CampaignText,
}

impl CampaignLabel {
    pub fn from_profile(profile: &StartProfile) -> Self {
        let title = CampaignText::from_display(profile.display_title.as_ref(), Some(&profile.id));
        let subtitle = CampaignText::from_display(profile.display_subtitle.as_ref(), None);
        Self {
            profile_id: profile.id.clone(),
            title,
            subtitle,
        }
    }

    pub fn to_snapshot(&self) -> SchemaCampaignLabel {
        SchemaCampaignLabel {
            profile_id: Some(self.profile_id.clone()),
            title: self.title.text.clone(),
            title_loc_key: self.title.loc_key.clone(),
            subtitle: self.subtitle.text.clone(),
            subtitle_loc_key: self.subtitle.loc_key.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.profile_id.is_empty() && self.title.is_empty() && self.subtitle.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CampaignProfileSnapshot {
    pub id: String,
    pub title: CampaignText,
    pub subtitle: CampaignText,
    pub overrides: StartProfileOverrides,
}

impl CampaignProfileSnapshot {
    pub fn from_profile(profile: &StartProfile) -> Self {
        Self {
            id: profile.id.clone(),
            title: CampaignText::from_display(profile.display_title.as_ref(), Some(&profile.id)),
            subtitle: CampaignText::from_display(profile.display_subtitle.as_ref(), None),
            overrides: profile.overrides.clone(),
        }
    }

    pub fn to_schema(&self) -> CampaignProfileState {
        let starting_units: Vec<CampaignStartingUnitState> = self
            .overrides
            .starting_units
            .iter()
            .map(|unit| CampaignStartingUnitState {
                kind: unit.kind.clone(),
                count: unit.count,
                tags: unit.tags.clone(),
            })
            .collect();
        let inventory: Vec<CampaignInventoryEntryState> = self
            .overrides
            .inventory
            .iter()
            .map(|entry| CampaignInventoryEntryState {
                item: entry.item.clone(),
                quantity: entry.quantity,
            })
            .collect();
        CampaignProfileState {
            id: Some(self.id.clone()),
            title: self.title.text.clone(),
            title_loc_key: self.title.loc_key.clone(),
            subtitle: self.subtitle.text.clone(),
            subtitle_loc_key: self.subtitle.loc_key.clone(),
            starting_units,
            inventory,
            knowledge_tags: self.overrides.starting_knowledge_tags.clone(),
            survey_radius: self.overrides.survey_radius,
            fog_mode: self
                .overrides
                .fog_mode
                .map(|mode| mode.as_str().to_string()),
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ActiveStartProfile {
    inner: StartProfile,
}

impl ActiveStartProfile {
    pub fn new(profile: StartProfile) -> Self {
        Self { inner: profile }
    }

    pub fn profile(&self) -> &StartProfile {
        &self.inner
    }
}

#[derive(Resource, Debug, Clone)]
pub struct StartProfileLookup {
    pub id: String,
}

impl StartProfileLookup {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

pub fn resolve_active_profile(
    handle: &StartProfilesHandle,
    profile_id: &str,
) -> (StartProfile, bool) {
    let profiles = handle.get();
    if let Some(found) = profiles.get(profile_id) {
        return (found.clone(), false);
    }

    let fallback = profiles
        .first()
        .cloned()
        .unwrap_or_else(|| StartProfile::placeholder(profile_id.to_string()));
    (fallback, true)
}

pub fn snapshot_profiles(handle: &StartProfilesHandle) -> Vec<CampaignProfileSnapshot> {
    let profiles = handle.get();
    profiles
        .iter()
        .map(CampaignProfileSnapshot::from_profile)
        .collect()
}
