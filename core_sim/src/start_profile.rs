use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
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
    pub ai_profile_overrides: HashMap<String, Value>,
    #[serde(default)]
    pub victory_modes_enabled: Vec<String>,
    #[serde(default)]
    pub food_modules: FoodModulePreference,
    /// **The turn-one outfitting window's dials** — see [`OpeningLoadoutConfig`]. **Required**: a
    /// profile with no opening loadout spawns a band that owns nothing and can never be given
    /// anything, so an absent block is a campaign that cannot be played rather than a default worth
    /// guessing.
    pub opening_loadout: OpeningLoadoutConfig,
}

/// **What the player may spend on the opening loadout**, per start profile.
///
/// The window opens at world build and closes on the first turn advance
/// ([`crate::starting_loadout::StartingLoadout`]); anything unspent is forfeited. It is the **one**
/// source of opening gear and material — `equipment.json` ships `start_stock_fraction: 0.0` and no
/// material declares a start stock any more.
///
/// # ⛔ THE KIT BUDGET IS NOT HERE, AND MUST NOT MOVE HERE
///
/// One kit per working-age hand is a **model fact**, derived from the band that actually spawned
/// (`size × demographics.initial_distribution.working`, floored — the same `party_workers` the spawn
/// already computes). A dial here would be a second, independent statement of how many people the
/// band has, free to disagree with the band itself the moment a band size or a working share is
/// retuned. The **material** budget has no such head count to derive from — nothing says how much
/// bone a band walked in with — so it is, and can only be, a number.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OpeningLoadoutConfig {
    /// Material points the player may spend, where **one point buys one unit**. Validated `> 0`.
    pub material_points: u32,
    /// **The pick list, in the order the client draws it.** A material absent from it cannot be
    /// bought at any price, which is what keeps `hurdles` and the three luxury crops off a list
    /// nobody would spend on. Validated non-empty, and every entry must name a material the
    /// materials table carries.
    pub pickable_materials: Vec<String>,
    /// **The allocation the window OPENS on** — a suggestion the player is free to spend elsewhere,
    /// never a grant: nothing is applied until a `SetStartingLoadout` arrives. Empty is the ordinary
    /// case and means *"the window opens with everything unspent"*. Every key must be in
    /// [`Self::pickable_materials`], and the values must sum at or below [`Self::material_points`].
    #[serde(default)]
    pub material_defaults: BTreeMap<String, u32>,
    /// **The kit column's pre-fill** — the material twin above, and a suggestion on exactly the same
    /// terms: nothing is applied until a `SetStartingLoadout` arrives, and a client is free to draw
    /// it and then send something else. Empty is the ordinary case.
    ///
    /// Every key must name a kit the equipment roster carries **and one that actually carries
    /// items** (the roster's `none` buys nothing, so pre-filling it would suggest spending a hand on
    /// air), and every count must be `> 0`. Both are checked by
    /// [`StartProfiles::validate_against_equipment`].
    ///
    /// # ⛔ THERE IS NO SUM CHECK HERE, BECAUSE THE BUDGET IS NOT IN THIS FILE
    ///
    /// [`Self::material_defaults`] can be validated against [`Self::material_points`] at load
    /// because both are config. The kit budget is **derived from the spawned band's working-age head
    /// count**, which does not exist until worldgen has run — so an over-allocating pre-fill cannot
    /// be a parse error and is instead **clamped at publish time** by
    /// [`crate::starting_loadout::clamped_kit_defaults`], which states the clamping rule and warns
    /// when it binds.
    #[serde(default)]
    pub kit_defaults: BTreeMap<String, u32>,
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
            validate_opening_loadout(&profile.id, &profile.overrides.opening_loadout)?;
        }

        Ok(Self {
            profiles: data.profiles,
            index,
        })
    }

    /// **The cross-config half of the opening loadout's validation** — every pickable material and
    /// every default must name a material the table carries.
    ///
    /// Separate from [`Self::from_data`] for the reason
    /// [`crate::equipment_config::EquipmentConfig::validate_against_materials`] is separate: the two
    /// files are loaded independently and only `build_headless_app` holds both at once. A pick list
    /// naming `hyde` would otherwise parse, validate, and be an entry the player can spend points on
    /// that deposits nothing.
    pub fn validate_against_materials(
        &self,
        materials: &crate::materials_config::MaterialsConfig,
    ) -> Result<(), StartProfilesError> {
        for profile in &self.profiles {
            let loadout = &profile.overrides.opening_loadout;
            for id in loadout
                .pickable_materials
                .iter()
                .chain(loadout.material_defaults.keys())
            {
                if materials.material(id).is_none() {
                    return Err(StartProfilesError::UnknownOpeningMaterial {
                        profile: profile.id.clone(),
                        material: id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// **The kit half of the same cross-config debt** — every `opening_loadout.kit_defaults` key must
    /// name a roster kit, and one that actually puts something in a hand.
    ///
    /// Its own method rather than an arm of [`Self::validate_against_materials`] because each
    /// cross-config check is named for the config it reconciles against, which is the idiom every
    /// other `validate_against_*` in this crate follows. Both are called from `build_headless_app`,
    /// the one place all three tables are in scope.
    ///
    /// **The empty-`uses` rejection is the command's rule, applied one layer earlier.**
    /// `apply_starting_loadout` refuses an allocation of a kit that carries nothing
    /// ([`crate::starting_loadout::LoadoutRejection::KitBuysNothing`]); a *pre-fill* of one would
    /// draw the picker opening on a row the server would then refuse, which is a worse failure than
    /// a boot panic because nothing reports it.
    pub fn validate_against_equipment(
        &self,
        equipment: &crate::equipment_config::EquipmentConfig,
    ) -> Result<(), StartProfilesError> {
        for profile in &self.profiles {
            for id in profile.overrides.opening_loadout.kit_defaults.keys() {
                let Some(definition) = equipment.kit_definition(id) else {
                    return Err(StartProfilesError::UnknownOpeningKit {
                        profile: profile.id.clone(),
                        kit: id.clone(),
                    });
                };
                if definition.uses.is_empty() {
                    return Err(StartProfilesError::OpeningKitBuysNothing {
                        profile: profile.id.clone(),
                        kit: id.clone(),
                    });
                }
            }
        }
        Ok(())
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

    /// **True only for a profiles file that listed none** — loading tolerates that, and no shipped
    /// file is empty. It is the counterpart [`Self::len`] requires (`clippy::len_without_is_empty`)
    /// rather than a case the sim branches on.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

/// The opening loadout's **intra-file** bounds. Cross-config checks (does the materials table carry
/// this id?) live in [`StartProfiles::validate_against_materials`], because this file is loaded
/// before the materials table is.
fn validate_opening_loadout(
    profile: &str,
    loadout: &OpeningLoadoutConfig,
) -> Result<(), StartProfilesError> {
    if loadout.material_points == 0 {
        return Err(StartProfilesError::InvalidOpeningLoadout {
            profile: profile.to_string(),
            reason: "`material_points` must be greater than 0 - a window that can buy nothing is \
                     a window that should not open"
                .to_string(),
        });
    }
    if loadout.pickable_materials.is_empty() {
        return Err(StartProfilesError::InvalidOpeningLoadout {
            profile: profile.to_string(),
            reason: "`pickable_materials` is empty - there would be nothing to spend the points on"
                .to_string(),
        });
    }
    for (index, id) in loadout.pickable_materials.iter().enumerate() {
        if loadout.pickable_materials[..index].contains(id) {
            return Err(StartProfilesError::InvalidOpeningLoadout {
                profile: profile.to_string(),
                reason: format!(
                    "`pickable_materials` names '{id}' twice - the picker would draw two rows \
                     spending one budget"
                ),
            });
        }
    }
    let mut defaulted = 0u32;
    for (id, units) in &loadout.material_defaults {
        if !loadout.pickable_materials.iter().any(|pick| pick == id) {
            return Err(StartProfilesError::InvalidOpeningLoadout {
                profile: profile.to_string(),
                reason: format!(
                    "`material_defaults` pre-fills '{id}', which `pickable_materials` does not \
                     offer - the window would open holding something the player cannot re-buy"
                ),
            });
        }
        defaulted = defaulted.saturating_add(*units);
    }
    if defaulted > loadout.material_points {
        return Err(StartProfilesError::InvalidOpeningLoadout {
            profile: profile.to_string(),
            reason: format!(
                "`material_defaults` sum to {defaulted}, over the {} `material_points` on offer - \
                 the window would open already overdrawn",
                loadout.material_points
            ),
        });
    }
    // **No sum check on the kit side**, because the kit budget is the spawned band's own head count
    // rather than a number in this file — see `OpeningLoadoutConfig::kit_defaults`. A count of zero
    // is still a fault here: a pre-fill of nothing is exactly what an absent key already says.
    for (id, count) in &loadout.kit_defaults {
        if *count == 0 {
            return Err(StartProfilesError::InvalidOpeningLoadout {
                profile: profile.to_string(),
                reason: format!(
                    "`kit_defaults` pre-fills '{id}' with 0 - omit the key to open that row empty, \
                     which is the same statement without a number that looks like a dial"
                ),
            });
        }
    }
    Ok(())
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
    #[error("start profile `{profile}` has an invalid opening_loadout: {reason}")]
    InvalidOpeningLoadout { profile: String, reason: String },
    #[error(
        "start profile `{profile}` opening_loadout names material `{material}`, which the \
         materials table does not carry"
    )]
    UnknownOpeningMaterial { profile: String, material: String },
    #[error(
        "start profile `{profile}` opening_loadout pre-fills kit `{kit}`, which the equipment \
         roster does not carry"
    )]
    UnknownOpeningKit { profile: String, kit: String },
    #[error(
        "start profile `{profile}` opening_loadout pre-fills kit `{kit}`, which carries no items - \
         the picker would open on a row the server refuses"
    )]
    OpeningKitBuysNothing { profile: String, kit: String },
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

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
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

    /// **The three CRAFTS follow the same rule, for the same reason.** They are not ladder rungs —
    /// a bench earns them, per item completed — but *"earned by practice, never handed out"* is the
    /// whole model, and a start profile granting Tanning would hand the player a tool they never
    /// worked for. Declared in the tag catalog so they are mappable, granted by nothing.
    #[test]
    fn no_start_profile_grants_a_craft_and_every_craft_tag_maps_to_its_discovery() {
        let crafts = [
            (
                crate::crafting::TANNING_CRAFT,
                crate::crafting::TANNING_DISCOVERY_ID,
            ),
            (
                crate::crafting::WEAVING_CRAFT,
                crate::crafting::WEAVING_DISCOVERY_ID,
            ),
            (
                crate::crafting::BONE_WORKING_CRAFT,
                crate::crafting::BONE_WORKING_DISCOVERY_ID,
            ),
        ];
        let catalog = StartProfileKnowledgeTags::builtin();
        for (tag, id) in crafts {
            let def = catalog
                .get(tag)
                .unwrap_or_else(|| panic!("craft '{tag}' must be declared in the tag catalog"));
            assert_eq!(
                def.discovery_id(),
                id,
                "'{tag}' maps to the wrong discovery"
            );
        }
        for profile in &StartProfiles::builtin().profiles {
            for (tag, _) in crafts {
                assert!(
                    !profile
                        .overrides
                        .starting_knowledge_tags
                        .iter()
                        .any(|granted| granted == tag),
                    "start profile '{}' grants the craft '{tag}' — a band learns Tanning by \
                     tanning, which is what makes 'tools are earned, never a prerequisite' true",
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

    /// The shipped file with one part of its opening loadout replaced, so each rejection test states
    /// exactly the one thing it broke.
    fn mutated_loadout(mutate: impl FnOnce(&mut Value)) -> StartProfilesError {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        mutate(&mut json["profiles"][0]["opening_loadout"]);
        StartProfiles::from_json_str(&json.to_string())
            .expect_err("the mutated profile must be rejected")
    }

    /// **Every shipped profile declares a coherent opening loadout**, which is what makes the
    /// window the one source of opening gear rather than a block a campaign can silently omit.
    #[test]
    fn the_builtin_profiles_declare_a_spendable_opening_loadout() {
        let profiles = StartProfiles::builtin();
        let materials = crate::materials_config::MaterialsConfig::builtin();
        profiles
            .validate_against_materials(&materials)
            .expect("every pickable material must be on the shipped roster");
        profiles
            .validate_against_equipment(&crate::equipment_config::EquipmentConfig::builtin())
            .expect("every pre-filled kit must be on the shipped roster and carry something");
        let mut checked = 0;
        for profile in profiles.iter() {
            let loadout = &profile.overrides().opening_loadout;
            checked += 1;
            assert!(loadout.material_points > 0, "{}", profile.id);
            assert!(!loadout.pickable_materials.is_empty(), "{}", profile.id);
            assert!(
                loadout.material_defaults.values().sum::<u32>() <= loadout.material_points,
                "{} pre-fills more than it offers",
                profile.id
            );
        }
        assert!(
            checked > 0,
            "**LIVENESS**: the file must ship a profile, or this asserts nothing"
        );
    }

    /// **An absent block is a parse error, not a defaulted one.** A profile with no opening loadout
    /// spawns a band that owns nothing and can never be given anything — a campaign that cannot be
    /// played, which is worse than one that refuses to boot.
    #[test]
    fn a_profile_with_no_opening_loadout_is_rejected() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]
            .as_object_mut()
            .expect("a profile is an object")
            .remove("opening_loadout");
        assert!(matches!(
            StartProfiles::from_json_str(&json.to_string()),
            Err(StartProfilesError::Parse(_))
        ));
    }

    #[test]
    fn a_non_positive_material_budget_is_rejected() {
        assert!(matches!(
            mutated_loadout(|loadout| loadout["material_points"] = Value::from(0)),
            StartProfilesError::InvalidOpeningLoadout { .. }
        ));
    }

    #[test]
    fn an_empty_pick_list_is_rejected() {
        assert!(matches!(
            mutated_loadout(|loadout| loadout["pickable_materials"] = Value::Array(Vec::new())),
            StartProfilesError::InvalidOpeningLoadout { .. }
        ));
    }

    #[test]
    fn a_default_outside_the_pick_list_is_rejected() {
        assert!(matches!(
            mutated_loadout(|loadout| {
                loadout["material_defaults"]["hurdles"] = Value::from(1);
            }),
            StartProfilesError::InvalidOpeningLoadout { .. }
        ));
    }

    #[test]
    fn defaults_over_the_budget_are_rejected() {
        assert!(matches!(
            mutated_loadout(|loadout| {
                let points = loadout["material_points"].as_u64().expect("a number");
                loadout["material_defaults"]["bone"] = Value::from(points + 1);
            }),
            StartProfilesError::InvalidOpeningLoadout { .. }
        ));
    }

    /// **The cross-config arm**, which is the one the loader runs after the materials table exists:
    /// a pick list naming a material the roster does not carry would otherwise be a row the player
    /// can spend points on that deposits nothing.
    #[test]
    fn a_pickable_material_the_roster_does_not_carry_is_rejected() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]["opening_loadout"]["pickable_materials"] =
            Value::Array(vec![Value::from("hyde")]);
        json["profiles"][0]["opening_loadout"]["material_defaults"] =
            Value::Object(serde_json::Map::new());
        let profiles =
            StartProfiles::from_json_str(&json.to_string()).expect("it parses and self-validates");
        assert!(matches!(
            profiles
                .validate_against_materials(&crate::materials_config::MaterialsConfig::builtin()),
            Err(StartProfilesError::UnknownOpeningMaterial { .. })
        ));
    }

    /// **The kit column opens on the three subsistence kits, four hands each.**
    ///
    /// The counts are asserted as literals because they are the shipped *opening state of the game*
    /// — the first thing a player sees in that column — so a pre-fill that drifted to something else
    /// should have to be changed on purpose.
    #[test]
    fn the_shipped_profile_pre_fills_the_three_subsistence_kits() {
        let profiles = StartProfiles::builtin();
        let loadout = &profiles
            .get("late_forager_tribe")
            .expect("the shipped profile")
            .overrides()
            .opening_loadout;
        assert_eq!(
            loadout
                .kit_defaults
                .iter()
                .map(|(id, count)| (id.as_str(), *count))
                .collect::<Vec<_>>(),
            vec![("big_game", 4), ("gathering", 4), ("trapping", 4)],
            "Stalking, Harvesting and Trapping, four hands each - a plausible band rather than a \
             column of zeros, with hands still left to spend deliberately"
        );
    }

    /// A pre-filled kit the roster does not carry would draw a picker row the `set_starting_loadout`
    /// handler then refuses, with nothing reporting why.
    #[test]
    fn a_pre_filled_kit_the_roster_does_not_carry_is_rejected() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]["opening_loadout"]["kit_defaults"] =
            serde_json::json!({ "ballista": 1 });
        let profiles =
            StartProfiles::from_json_str(&json.to_string()).expect("it parses and self-validates");
        assert!(matches!(
            profiles
                .validate_against_equipment(&crate::equipment_config::EquipmentConfig::builtin()),
            Err(StartProfilesError::UnknownOpeningKit { .. })
        ));
    }

    /// **The command's own rule, one layer earlier.** `apply_starting_loadout` refuses an allocation
    /// of a kit that carries nothing; a pre-fill of one would suggest spending a hand on air.
    #[test]
    fn a_pre_filled_kit_that_carries_nothing_is_rejected() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]["opening_loadout"]["kit_defaults"] = serde_json::json!({ "none": 1 });
        let profiles =
            StartProfiles::from_json_str(&json.to_string()).expect("it parses and self-validates");
        assert!(matches!(
            profiles
                .validate_against_equipment(&crate::equipment_config::EquipmentConfig::builtin()),
            Err(StartProfilesError::OpeningKitBuysNothing { .. })
        ));
    }

    /// A zero pre-fill is what an absent key already says, so a number that looks like a dial and
    /// means nothing is a fault rather than a no-op.
    #[test]
    fn a_zero_kit_pre_fill_is_rejected() {
        assert!(matches!(
            mutated_loadout(|loadout| {
                loadout["kit_defaults"] = serde_json::json!({ "big_game": 0 });
            }),
            StartProfilesError::InvalidOpeningLoadout { .. }
        ));
    }

    /// ⛔ **THERE IS NO SUM CHECK ON THE KIT SIDE, AND THAT IS DELIBERATE.** The budget is the
    /// spawned band's head count, which does not exist at load; an over-allocating pre-fill parses
    /// and is clamped at publish time instead
    /// ([`crate::starting_loadout::clamped_kit_defaults`]).
    #[test]
    fn a_kit_pre_fill_over_any_plausible_budget_still_parses() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]["opening_loadout"]["kit_defaults"] =
            serde_json::json!({ "big_game": 900 });
        let profiles =
            StartProfiles::from_json_str(&json.to_string()).expect("no load-time sum check exists");
        assert_eq!(
            profiles
                .get("late_forager_tribe")
                .expect("the shipped profile")
                .overrides()
                .opening_loadout
                .kit_defaults
                .get("big_game"),
            Some(&900)
        );
    }
}
