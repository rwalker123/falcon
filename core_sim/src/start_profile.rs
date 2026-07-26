use std::{
    collections::HashMap,
    fs, io,
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

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::food::FoodModule;

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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FoodModulePreference {
    pub primary: Option<FoodModule>,
    pub secondary: Option<FoodModule>,
}

impl FoodModulePreference {
    pub fn matches(&self, module: FoodModule) -> bool {
        self.primary == Some(module) || self.secondary == Some(module)
    }

    pub fn any(&self) -> bool {
        self.primary.is_some() || self.secondary.is_some()
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
    pub stockpile_access_radius: Option<u32>,
    #[serde(default)]
    pub ai_profile_overrides: HashMap<String, Value>,
    #[serde(default)]
    pub victory_modes_enabled: Vec<String>,
    #[serde(default)]
    pub food_modules: FoodModulePreference,
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

impl ConfigLoadError for KnowledgeTagCatalogError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::ReadFailed { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// People in a starting band when a profile's unit omits `band_size`. The band is a
/// labor pool (see `docs/plan_early_game_labor.md`): one food source sustainably feeds
/// ~10, so a starting band is a small group whose working-age bracket is the labor pool,
/// not a 900-person settlement. Overridable per starting unit via `band_size`.
pub const DEFAULT_STARTING_BAND_SIZE: u32 = 30;

#[derive(Debug, Clone, Deserialize)]
pub struct StartingUnitSpec {
    pub kind: String,
    #[serde(default = "default_unit_count")]
    pub count: u32,
    #[serde(default)]
    pub position: Option<[i32; 2]>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Head-count each spawned band of this kind starts with. Falls back to
    /// [`DEFAULT_STARTING_BAND_SIZE`] when unset. Brackets + larder are seeded from
    /// `demographics_config.json` (`initial_distribution` + `startup.food_reserve_days`).
    #[serde(default)]
    pub band_size: Option<u32>,
}

impl StartingUnitSpec {
    /// Resolved starting head-count for this unit, applying the default when unset.
    /// Clamped to at least 1 (mirroring `count.max(1)` in `spawn_profile_population`) so a
    /// misconfigured `band_size: 0` yields a 1-person band rather than a degenerate empty cohort.
    pub fn band_size(&self) -> u32 {
        self.band_size.unwrap_or(DEFAULT_STARTING_BAND_SIZE).max(1)
    }
}

impl Default for StartingUnitSpec {
    fn default() -> Self {
        Self {
            kind: String::new(),
            count: default_unit_count(),
            position: None,
            tags: Vec::new(),
            band_size: None,
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

impl ConfigLoadError for StartProfilesError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::ReadFailed { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
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

/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `START_PROFILES_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_start_profiles_from_env() -> (Arc<StartProfiles>, StartProfilesMetadata) {
    let (profiles, source) = load_config_from_env(
        "START_PROFILES_PATH",
        "start_profiles",
        "src/data/start_profiles.json",
        StartProfiles::builtin,
        StartProfiles::from_file,
    );
    (profiles, StartProfilesMetadata::new(source))
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

/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `START_PROFILE_KNOWLEDGE_TAGS_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_start_profile_knowledge_tags_from_env() -> (
    Arc<StartProfileKnowledgeTags>,
    StartProfileKnowledgeTagsMetadata,
) {
    let (catalog, source) = load_config_from_env(
        "START_PROFILE_KNOWLEDGE_TAGS_PATH",
        "start_profile_knowledge_tags",
        "src/data/start_profile_knowledge_tags.json",
        StartProfileKnowledgeTags::builtin,
        StartProfileKnowledgeTags::from_file,
    );
    (catalog, StartProfileKnowledgeTagsMetadata::new(source))
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
            primary_food_module: self
                .overrides
                .food_modules
                .primary
                .map(|module| module.as_str().to_string()),
            secondary_food_module: self
                .overrides
                .food_modules
                .secondary
                .map(|module| module.as_str().to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fauna::{FODDERING_DISCOVERY_ID, HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID},
        forage::{CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID},
    };

    /// Every knowledge the intensification ladder gates on (or earns), and the id it must map to.
    /// `foddering` (F3) is earned by running a pen but gates no rung of its own — still it must be
    /// mappable and, like every ladder knowledge, never start-granted.
    const LADDER_KNOWLEDGE: [(&str, u32); 5] = [
        ("cultivation", CULTIVATION_DISCOVERY_ID),
        ("herding", HERDING_DISCOVERY_ID),
        ("seed_selection", SEED_SELECTION_DISCOVERY_ID),
        ("penning", PENNING_DISCOVERY_ID),
        ("foddering", FODDERING_DISCOVERY_ID),
    ];

    /// **Nothing on the ladder is start-granted** (`docs/plan_intensification_ladder.md` §2a) — the
    /// whole model is that knowledge is *earned by practice*, so a profile that shipped one would
    /// silently hand the player a rung they never climbed.
    ///
    /// Each tag is declared in `start_profile_knowledge_tags.json` **purely so it is mappable**, and
    /// that is exactly the hazard this pins: the mapping's existence is what would let a profile list
    /// it by name. Every doc comment on the four ids asserts this in prose; here it is a test.
    #[test]
    fn no_start_profile_grants_a_ladder_knowledge() {
        let profiles = StartProfiles::builtin();
        for profile in &profiles.profiles {
            for (tag, _) in LADDER_KNOWLEDGE {
                assert!(
                    !profile
                        .overrides
                        .starting_knowledge_tags
                        .iter()
                        .any(|granted| granted == tag),
                    "start profile '{}' grants the ladder knowledge '{tag}' — it must be EARNED \
                     (practice rung N unlocks rung N+1), never handed out at start",
                    profile.id
                );
            }
        }
    }

    /// The four ladder knowledges are **mappable** — each tag resolves to its discovery id. This is
    /// what lets `intensification::discovery_id_for` name them, and it is the other half of the
    /// contract above: declared, but never granted.
    #[test]
    fn every_ladder_knowledge_tag_maps_to_its_discovery() {
        let catalog = StartProfileKnowledgeTags::builtin();
        for (tag, id) in LADDER_KNOWLEDGE {
            let def = catalog
                .get(tag)
                .unwrap_or_else(|| panic!("'{tag}' must be declared in the knowledge-tag catalog"));
            assert_eq!(
                def.discovery_id(),
                id,
                "'{tag}' maps to the wrong discovery"
            );
        }
    }
}
