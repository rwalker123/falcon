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

/// **A profile does not say who plays the world.** It stocks the opening — one [`StartArchetype`]
/// per kind of people — and the roster comes from the new game's own per-archetype counts, so
/// `Default` is derivable: every field's default is the empty/`Default` one.
///
/// The per-people opening (units, knowledge, inventory, loadout window) lives on the archetype;
/// what stays here is what is true of the whole campaign.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartProfileOverrides {
    /// **The kinds of people this campaign starts, in catalog order** — see [`StartArchetype`].
    /// Validated non-empty at load. A default-built overrides (a hand-rolled test world that never
    /// applied a profile) carries none, which worldgen reads as "no starting units".
    #[serde(default)]
    pub archetypes: Vec<StartArchetype>,
    #[serde(default)]
    pub victory_modes_enabled: Vec<String>,
    #[serde(default)]
    pub food_modules: FoodModulePreference,
}

impl StartProfileOverrides {
    /// The archetype at catalog position `index`, if the profile has one.
    pub fn archetype(&self, index: usize) -> Option<&StartArchetype> {
        self.archetypes.get(index)
    }

    /// **The human seat's archetype** — faction 0 always takes the first entry of the catalog.
    pub fn human_archetype(&self) -> Option<&StartArchetype> {
        self.archetypes.first()
    }
}

/// **Which archetype each roster id is** - the per-faction mapping worldgen reads to stock and place
/// every people with ITS archetype's opening.
///
/// Built from the per-archetype counts a world was asked for, in catalog order: faction 0 is the
/// human seat and always takes archetype 0, then archetype 0's rivals take the next ids, then
/// archetype 1's, and so on - the same positional ids `FactionRegistry::with_ai_factions` hands out.
///
/// ⛔ **Transient and config-side, NOT part of `SimState`.** It lives in `SimulationConfig` beside
/// `start_profile_overrides`, is rebuilt by every world-build path (boot, `new_game`, `ResetMap`),
/// and nothing after the world is built reads it - what a faction is stocked with is on its bands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartArchetypeAssignment {
    per_faction: Vec<usize>,
}

impl StartArchetypeAssignment {
    /// The assignment for a roster of one human plus `counts[k]` rivals of archetype `k`.
    pub fn from_counts(counts: &[u32]) -> Self {
        let mut per_faction = vec![0usize];
        for (archetype, count) in counts.iter().enumerate() {
            per_faction.extend(std::iter::repeat_n(archetype, *count as usize));
        }
        Self { per_faction }
    }

    /// The rival count of each of `archetype_count` archetypes, in catalog order - the inverse of
    /// [`Self::from_counts`]. The human seat is not a rival and is not counted.
    pub fn counts(&self, archetype_count: usize) -> Vec<u32> {
        let mut counts = vec![0u32; archetype_count];
        for index in self.per_faction.iter().skip(1) {
            if let Some(slot) = counts.get_mut(*index) {
                *slot += 1;
            }
        }
        counts
    }

    /// The catalog index of `faction`'s archetype. A roster id the assignment does not cover reads
    /// as archetype 0 - the only archetype a hand-rolled test world with no assignment has.
    pub fn archetype_index(&self, faction: crate::orders::FactionId) -> usize {
        self.per_faction
            .get(faction.0 as usize)
            .copied()
            .unwrap_or(0)
    }
}

/// **One kind of people a campaign can start** (`docs/plan_minor_peoples.md` §Starts): how far apart
/// its starts are held, how many a world gets by default, and what each of them opens with.
///
/// The catalog is ordered: the human seat takes entry 0, roster ids are handed out in catalog order
/// (entry 0's rivals, then entry 1's, ...), and placement lays the entries down in the same order so
/// the later, smaller-separation ones fill the gaps the earlier ones left.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StartArchetype {
    /// Stable key - the wire names an archetype by it (`new_game <...> <id>=<count>`).
    pub id: String,
    /// What a New Game screen calls this kind of people.
    pub display_label: String,
    /// **How far apart worldgen tries to put one of this archetype's starts from every start
    /// already placed, in tiles.** Euclidean on tile coordinates, compared squared - the metric the
    /// curated food-site pass spaces markers by. A *target*: on a map with no land pair that far
    /// apart, worldgen takes the best remaining tile and warns. Validated `> 0`, because zero would
    /// let two peoples open on one hex.
    pub min_separation: u32,
    /// **The pin on how many of this archetype a world gets when nobody picked**, or `None` for
    /// "derive it": a New Game screen pre-selects a share of what the map seats, and an unattended
    /// boot opens with none. `None` is not `Some(0)`. A number pins both. Always clamped to what the
    /// map can seat - see `systems::worldgen::archetype_capacities`.
    #[serde(default)]
    pub default_count: Option<u32>,
    /// **The share of what fits that a New Game screen pre-selects**, or `None` for the shared
    /// floor-plus-half rule. When present the derived default is `floor(ceiling x default_share)`,
    /// the ceiling being computed against the EARLIER archetypes' defaults. A `default_count` pin
    /// beats it. Validated finite and in `(0, 1]`.
    #[serde(default)]
    pub default_share: Option<f32>,
    #[serde(default)]
    pub starting_units: Vec<StartingUnitSpec>,
    #[serde(default)]
    pub starting_knowledge_tags: Vec<String>,
    #[serde(default)]
    pub inventory: Vec<InventoryEntry>,
    /// **The turn-one outfitting window's dials** - see [`OpeningLoadoutConfig`]. **Required**: an
    /// archetype with no opening loadout spawns a band that owns nothing and can never be given
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
            validate_archetypes(&profile.id, &profile.overrides.archetypes)?;
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
            for archetype in &profile.overrides.archetypes {
                let loadout = &archetype.opening_loadout;
                for id in loadout
                    .pickable_materials
                    .iter()
                    .chain(loadout.material_defaults.keys())
                {
                    if materials.material(id).is_none() {
                        return Err(StartProfilesError::UnknownOpeningMaterial {
                            profile: archetype_scope(&profile.id, &archetype.id),
                            material: id.clone(),
                        });
                    }
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
            for archetype in &profile.overrides.archetypes {
                for id in archetype.opening_loadout.kit_defaults.keys() {
                    let Some(definition) = equipment.kit_definition(id) else {
                        return Err(StartProfilesError::UnknownOpeningKit {
                            profile: archetype_scope(&profile.id, &archetype.id),
                            kit: id.clone(),
                        });
                    };
                    if definition.uses.is_empty() {
                        return Err(StartProfilesError::OpeningKitBuysNothing {
                            profile: archetype_scope(&profile.id, &archetype.id),
                            kit: id.clone(),
                        });
                    }
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

/// How an error names one archetype of one profile: `profile/archetype`.
fn archetype_scope(profile: &str, archetype: &str) -> String {
    format!("{profile}/{archetype}")
}

/// **The archetype catalog's intra-file bounds**: at least one entry (the human seat takes entry
/// 0), unique ids (the wire names an archetype by id), a positive separation, and a valid opening
/// loadout on each.
fn validate_archetypes(
    profile: &str,
    archetypes: &[StartArchetype],
) -> Result<(), StartProfilesError> {
    if archetypes.is_empty() {
        return Err(StartProfilesError::NoArchetypes {
            profile: profile.to_string(),
        });
    }
    for (index, archetype) in archetypes.iter().enumerate() {
        if archetypes[..index]
            .iter()
            .any(|seen| seen.id == archetype.id)
        {
            return Err(StartProfilesError::DuplicateArchetypeId {
                profile: profile.to_string(),
                archetype: archetype.id.clone(),
            });
        }
        if archetype.min_separation == 0 {
            return Err(StartProfilesError::ZeroArchetypeSeparation {
                profile: profile.to_string(),
                archetype: archetype.id.clone(),
            });
        }
        if let Some(share) = archetype.default_share {
            if !share.is_finite() || share <= 0.0 || share > 1.0 {
                return Err(StartProfilesError::InvalidDefaultShare {
                    profile: profile.to_string(),
                    archetype: archetype.id.clone(),
                    share,
                });
            }
        }
        validate_opening_loadout(
            &archetype_scope(profile, &archetype.id),
            &archetype.opening_loadout,
        )?;
    }
    Ok(())
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
    #[error("start profile `{profile}` lists no archetypes: the human seat takes the first one")]
    NoArchetypes { profile: String },
    #[error("start profile `{profile}` lists archetype `{archetype}` twice")]
    DuplicateArchetypeId { profile: String, archetype: String },
    #[error(
        "start profile `{profile}` archetype `{archetype}` has a `min_separation` of 0: it is the \
         distance worldgen holds between starts, and 0 would let two peoples open standing on the \
         same hex - which is not a cramped map, it is no placement at all"
    )]
    ZeroArchetypeSeparation { profile: String, archetype: String },
    #[error(
        "start profile `{profile}` archetype `{archetype}` has a `default_share` of {share}: it \
         is the fraction of what fits that a New Game screen pre-selects, so it must be finite and \
         in (0, 1]"
    )]
    InvalidDefaultShare {
        profile: String,
        archetype: String,
        share: f32,
    },
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
        // The campaign card describes the human seat's people - archetype 0.
        let human = self.overrides.human_archetype();
        let starting_units: Vec<CampaignStartingUnitState> = human
            .map(|archetype| archetype.starting_units.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|unit| CampaignStartingUnitState {
                kind: unit.kind.clone(),
                count: unit.count,
                tags: unit.tags.clone(),
            })
            .collect();
        let inventory: Vec<CampaignInventoryEntryState> = human
            .map(|archetype| archetype.inventory.as_slice())
            .unwrap_or_default()
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
            knowledge_tags: human
                .map(|archetype| archetype.starting_knowledge_tags.clone())
                .unwrap_or_default(),
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
        fauna::{HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID},
        forage::{CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID},
    };
    use serde_json::Value;

    /// Every knowledge the intensification ladder gates on (or earns), and the id it must map to.
    const LADDER_KNOWLEDGE: [(&str, u32); 6] = [
        ("cultivation", CULTIVATION_DISCOVERY_ID),
        ("herding", HERDING_DISCOVERY_ID),
        ("seed_selection", SEED_SELECTION_DISCOVERY_ID),
        ("penning", PENNING_DISCOVERY_ID),
        ("worked_land", crate::forage::WORKED_LAND_DISCOVERY_ID),
        (
            "selective_breeding",
            crate::fauna::SELECTIVE_BREEDING_DISCOVERY_ID,
        ),
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
                        .archetypes
                        .iter()
                        .any(|archetype| archetype
                            .starting_knowledge_tags
                            .iter()
                            .any(|granted| granted == tag)),
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
                        .archetypes
                        .iter()
                        .any(|archetype| archetype
                            .starting_knowledge_tags
                            .iter()
                            .any(|granted| granted == tag)),
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
        mutate(&mut json["profiles"][0]["archetypes"][0]["opening_loadout"]);
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
            for archetype in &profile.overrides().archetypes {
                let loadout = &archetype.opening_loadout;
                checked += 1;
                assert!(
                    !loadout.pickable_materials.is_empty(),
                    "{}/{}",
                    profile.id,
                    archetype.id
                );
            }
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
        json["profiles"][0]["archetypes"][0]
            .as_object_mut()
            .expect("an archetype is an object")
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
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["material_points"] =
            Value::from(30);
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
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["pickable_materials"] =
            Value::Array(vec![Value::from("hyde")]);
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["material_defaults"] =
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
            .archetypes[0]
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
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["kit_defaults"] =
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
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["kit_defaults"] =
            serde_json::json!({ "none": 1 });
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
        json["profiles"][0]["archetypes"][0]["opening_loadout"]["kit_defaults"] =
            serde_json::json!({ "big_game": 900 });
        let profiles =
            StartProfiles::from_json_str(&json.to_string()).expect("no load-time sum check exists");
        assert_eq!(
            profiles
                .get("late_forager_tribe")
                .expect("the shipped profile")
                .overrides()
                .archetypes[0]
                .opening_loadout
                .kit_defaults
                .get("big_game"),
            Some(&900)
        );
    }

    /// **The assignment is the roster ids in catalog order**: the human first, then archetype 0's
    /// rivals, then archetype 1's - and `counts` reads it back.
    #[test]
    fn the_assignment_hands_out_roster_ids_in_catalog_order() {
        use crate::orders::FactionId;
        let assignment = StartArchetypeAssignment::from_counts(&[2, 3]);
        let indices: Vec<usize> = (0..6)
            .map(|id| assignment.archetype_index(FactionId(id)))
            .collect();
        assert_eq!(indices, vec![0, 0, 0, 1, 1, 1]);
        assert_eq!(assignment.counts(2), vec![2, 3]);
        assert_eq!(
            assignment.archetype_index(FactionId(99)),
            0,
            "an id the assignment does not cover reads as archetype 0"
        );
        assert_eq!(StartArchetypeAssignment::default().counts(2), vec![0, 0]);
    }

    /// The shipped file with the archetype catalog replaced.
    fn mutated_archetypes(
        mutate: impl FnOnce(&mut Value),
    ) -> Result<StartProfiles, StartProfilesError> {
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        mutate(&mut json["profiles"][0]["archetypes"]);
        StartProfiles::from_json_str(&json.to_string())
    }

    /// **The shipped catalog**: majors first (the human seat takes entry 0) at the wide separation,
    /// minors second at the tight one, each with its own band size - and minors carry no kits.
    #[test]
    fn the_shipped_profile_ships_a_major_then_a_minor_archetype() {
        let profiles = StartProfiles::builtin();
        let archetypes = &profiles
            .get("late_forager_tribe")
            .expect("the shipped profile")
            .overrides()
            .archetypes;
        assert_eq!(
            archetypes.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["major", "minor"]
        );
        assert!(archetypes[0].min_separation > archetypes[1].min_separation);
        assert_eq!(archetypes[0].starting_units[0].band_size(), 30);
        assert_eq!(archetypes[1].starting_units[0].band_size(), 12);
        assert!(!archetypes[0].opening_loadout.kit_defaults.is_empty());
        assert!(archetypes[1].opening_loadout.kit_defaults.is_empty());
    }

    #[test]
    fn a_profile_with_no_archetypes_is_rejected() {
        assert!(matches!(
            mutated_archetypes(|archetypes| *archetypes = Value::Array(Vec::new())),
            Err(StartProfilesError::NoArchetypes { .. })
        ));
        let mut json: Value =
            serde_json::from_str(BUILTIN_START_PROFILES).expect("the builtin parses as json");
        json["profiles"][0]
            .as_object_mut()
            .expect("a profile is an object")
            .remove("archetypes");
        assert!(matches!(
            StartProfiles::from_json_str(&json.to_string()),
            Err(StartProfilesError::NoArchetypes { .. })
        ));
    }

    #[test]
    fn a_duplicate_archetype_id_is_rejected() {
        assert!(matches!(
            mutated_archetypes(|archetypes| archetypes[1]["id"] = Value::from("major")),
            Err(StartProfilesError::DuplicateArchetypeId { .. })
        ));
    }

    #[test]
    fn a_default_share_outside_zero_to_one_is_rejected() {
        for bad in [0.0, -0.5, 1.5] {
            assert!(
                matches!(
                    mutated_archetypes(
                        |archetypes| archetypes[1]["default_share"] = Value::from(bad)
                    ),
                    Err(StartProfilesError::InvalidDefaultShare { .. })
                ),
                "{bad} must be rejected"
            );
        }
        assert!(
            mutated_archetypes(|archetypes| archetypes[1]["default_share"] = Value::from(1.0))
                .is_ok()
        );
    }

    #[test]
    fn a_zero_archetype_separation_is_rejected() {
        assert!(matches!(
            mutated_archetypes(|archetypes| archetypes[1]["min_separation"] = Value::from(0)),
            Err(StartProfilesError::ZeroArchetypeSeparation { .. })
        ));
    }

    /// `deny_unknown_fields` on the archetype: a per-people field left on the profile is silently
    /// dead, but one mistyped on the archetype is refused.
    #[test]
    fn an_unknown_archetype_field_is_rejected() {
        assert!(matches!(
            mutated_archetypes(
                |archetypes| archetypes[0]["faction_start_min_separation"] = Value::from(20)
            ),
            Err(StartProfilesError::Parse(_))
        ));
    }

    /// A bad loadout on a LATER archetype is caught too - every archetype runs the same checks.
    #[test]
    fn every_archetype_runs_the_opening_loadout_checks() {
        assert!(matches!(
            mutated_archetypes(|archetypes| archetypes[1]["opening_loadout"]
                ["pickable_materials"] = Value::Array(Vec::new())),
            Err(StartProfilesError::InvalidOpeningLoadout { .. })
        ));
    }
}
