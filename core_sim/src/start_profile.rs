use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
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

/// **How a faction is driven.** There is no third option: every faction is either somebody's to
/// play or something the sim drives.
///
/// A profile does **not** declare it: the roster is one human plus the AI count the new game asked
/// for ([`crate::orders::FactionRegistry::with_ai_factions`]), so control is derived from an id's
/// position rather than authored anywhere. This type stays because the registry still *records*
/// how each faction is driven; it lives here because it was declared here, and moving it would
/// churn every import for nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionControl {
    Human,
    Ai,
}

/// **A profile does not say who plays the world.** It stocks the opening — units, knowledge,
/// inventory, the loadout window — and the roster comes from the new game's own AI-faction count,
/// so `Default` is derivable again: every field's default is the empty/`Default` one.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartProfileOverrides {
    #[serde(default)]
    pub starting_units: Vec<StartingUnitSpec>,
    #[serde(default)]
    pub starting_knowledge_tags: Vec<String>,
    #[serde(default)]
    pub inventory: Vec<InventoryEntry>,
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
/// # ⛔ THERE IS NO BUDGET HERE, AND THERE MUST NOT BE ONE
///
/// The window's budget is the spawned band's **carry**: its working-age hands
/// (`size × demographics.initial_distribution.working`, floored — the same `party_workers` the spawn
/// already computes) × one worker's pack, `expedition_config.json` `carry.per_worker_carry`
/// ([`crate::carry::carry_capacity`]). Kits and materials are spent from it alike,
/// in the one load currency. A dial here would be a second, independent statement of how much the
/// band can carry, free to disagree with the band itself the moment a band size, a working share or
/// the pack is retuned.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OpeningLoadoutConfig {
    /// **The pick list, in the order the client draws it.** A material absent from it cannot be
    /// bought at any price, which is what keeps `hurdles` and the three luxury crops off a list
    /// nobody would spend on. Validated non-empty, and every entry must name a material the
    /// materials table carries.
    pub pickable_materials: Vec<String>,
    /// **The material half of the DEFAULT OUTFIT the sim applies to the opening band at creation**
    /// ([`crate::starting_loadout::outfit_band_with_defaults`]), re-fitted to that band's own budget.
    /// A splinter's default is drawn from its parent's allocation instead.
    /// It is the allocation the window opens on and the player is free to spend elsewhere — but it
    /// is *applied*, not suggested, so a card nobody commits costs the band nothing. Empty is the
    /// ordinary case and means *"the window opens with everything unspent"*. Every key must be in
    /// [`Self::pickable_materials`]. There is no sum check: the budget is the band's carry, which
    /// does not exist until the band does, so the defaults are fitted to it when applied.
    #[serde(default)]
    pub material_defaults: BTreeMap<String, u32>,
    /// **The kit half of the default outfit** — the material twin above, on exactly the same terms.
    /// Empty is the ordinary case.
    ///
    /// Every key must name a kit the equipment roster carries **and one that actually carries
    /// items** (the roster's `none` buys nothing, so defaulting it would spend a hand on air), and
    /// every count must be `> 0`. Both are checked by
    /// [`StartProfiles::validate_against_equipment`].
    ///
    /// # ⛔ THERE IS NO SUM CHECK HERE, BECAUSE THE BUDGET IS NOT IN THIS FILE
    ///
    /// The carry budget is **derived from the spawned band's working-age head count**, which does
    /// not exist until worldgen has run — so an over-allocating default (kits and materials
    /// together) cannot be a parse error and is instead **fitted when it is applied** by
    /// [`crate::starting_loadout::fit_to_carry`], which states the fitting rule; the stamp warns
    /// when it binds.
    #[serde(default)]
    pub kit_defaults: BTreeMap<String, u32>,
}

impl StartProfileOverrides {
    pub fn from_profile(profile: &StartProfile) -> Self {
        profile.overrides.clone()
    }
}

/// One row of `start_profile_knowledge_tags.json` — the table that maps every knowledge tag to its
/// discovery id.
///
/// `deny_unknown_fields` so a retired key fails loudly instead of being silently ignored: the
/// `fidelity` this row used to carry was deleted by #531, and a stale copy of it must not parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeTagDefinition {
    pub discovery_id: u32,
    #[serde(default = "default_tag_progress")]
    pub progress: f32,
    /// **How much being around this discovery teaches**, `0..=1` — the knowledge rider's per-discovery
    /// term (`crate::knowledge_contact`, `docs/plan_contact_and_logistics.md` §Settled by #531). A
    /// pen is impossible to miss (`1.0`); seed selection happens in the ground and in someone's
    /// judgment (`0.2`).
    ///
    /// **Required, with no default** — a defaulted observability would pace a lesson off a number
    /// nobody chose. Named apart from the fog's `Seen`/`Discovered` vocabulary on purpose: this is
    /// about an idea, not a tile.
    pub observability: f32,
}

impl KnowledgeTagDefinition {
    pub fn discovery_id(&self) -> u32 {
        self.discovery_id
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn observability(&self) -> f32 {
        self.observability
    }
}

fn default_tag_progress() -> f32 {
    0.5
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
        for (tag, definition) in &tags {
            let observability = definition.observability;
            if !observability.is_finite() || !(0.0..=1.0).contains(&observability) {
                return Err(KnowledgeTagCatalogError::Invalid {
                    tag: tag.clone(),
                    reason: format!(
                        "observability must be a fraction in 0..=1 (was {observability})"
                    ),
                });
            }
        }
        Ok(Self { tags })
    }

    /// Every tag, in no particular order — callers that need a stable order sort.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &KnowledgeTagDefinition)> {
        self.tags
            .iter()
            .map(|(tag, definition)| (tag.as_str(), definition))
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
    #[error("invalid start profile knowledge tag {tag:?}: {reason}")]
    Invalid { tag: String, reason: String },
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
    for id in loadout.material_defaults.keys() {
        if !loadout.pickable_materials.iter().any(|pick| pick == id) {
            return Err(StartProfilesError::InvalidOpeningLoadout {
                profile: profile.to_string(),
                reason: format!(
                    "`material_defaults` pre-fills '{id}', which `pickable_materials` does not \
                     offer - the window would open holding something the player cannot re-buy"
                ),
            });
        }
    }
    // **No sum check on either side**, because the budget is the spawned band's own carry rather
    // than a number in this file — see `OpeningLoadoutConfig::kit_defaults`. A count of zero is
    // still a fault here: a pre-fill of nothing is exactly what an absent key already says.
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
    use serde_json::Value;

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
            assert!(!loadout.pickable_materials.is_empty(), "{}", profile.id);
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

    /// ⛔ **THE RETIRED `material_points` KEY IS REFUSED**, not ignored: the block is
    /// `deny_unknown_fields`, so a profile still naming the old flat material budget fails to parse
    /// rather than silently losing a dial its author thinks is live.
    #[test]
    fn the_retired_material_points_key_is_refused() {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]["opening_loadout"]["material_points"] = Value::from(30);
        assert!(matches!(
            StartProfiles::from_json_str(&json.to_string()),
            Err(StartProfilesError::Parse(_))
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

    /// **The kit column opens on the three subsistence kits, a kit per hand.**
    ///
    /// The counts are asserted as literals because they are the shipped *opening state of the game*
    /// — the first thing a player sees in that column — so a pre-fill that drifted to something else
    /// should have to be changed on purpose. Stalking 5 / Harvesting 7 / Trapping 5 is the
    /// maintainer's playtest outfit: 17 kits for the shipped band's 17 hands, so an untouched split
    /// hands a kit per splinter worker.
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
            vec![("big_game", 5), ("gathering", 7), ("trapping", 5)],
            "Stalking, Harvesting and Trapping, a kit per hand on the shipped 17 - a plausible band \
             rather than a column of zeros"
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

    /// ⛔ **THERE IS NO SUM CHECK, AND THAT IS DELIBERATE.** The budget is the spawned band's
    /// carry, which does not exist at load; an over-allocating pre-fill parses and is fitted when it
    /// is applied to a band instead ([`crate::starting_loadout::fit_to_carry`]).
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
