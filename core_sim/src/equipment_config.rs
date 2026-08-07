//! **The minimal TOE** (`data/equipment.json`) — the three consumable kits that lift a band's
//! hunting, hauling and gathering roles from their *unequipped* to their *equipped* tier.
//!
//! Design: `docs/plan_early_game_labor.md` → "Equipment / TOE" (the authoritative arc) and
//! `docs/plan_hunt_through_combat.md` §4.8 (why a minimal TOE has to land before the hunt resolves
//! through combat: a bare-handed hunter's `attack` is `1`, below every megafauna's `defense`, so
//! without a spear the gate is the entire game).
//!
//! **One kit, one job** (§4.8) — the three do not overlap, and the pairing is physical:
//!
//! | kit | raises |
//! |---|---|
//! | **spears** ([`HuntingKitConfig`]) | a hunter's `attack` |
//! | **sled** ([`SledKitConfig`]) | the **hunt's** carry — a carcass is one lumpy object you *drag* out whole |
//! | **baskets** ([`BasketKitConfig`]) | the **forage** web's carry — berries are loose, divisible, bounded by what you can hold |
//!
//! **Three rules this module exists to keep:**
//!
//! 1. **Two tiers, never a taper.** A kit's performance is *flat* until it expires and then the role
//!    **steps down** to its unequipped tier. Durability and performance are deliberately orthogonal
//!    axes — coupling them (a kit that gets worse as it wears) would be the modeling mistake the arc
//!    calls out, and would let a future crafting economy tune only one thing.
//! 2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2). A
//!    turn-based clock charges an idle march the same as a slaughter, which would make denial free.
//!    Each kit has its **own** quantum, so the three cannot cross-charge: spears wear per **animal
//!    killed**, the sled per **biomass hauled home from a hunt**, baskets per **biomass gathered**.
//! 3. **One home per fact.** All three *equipped* carry/attack tiers already had homes and stay
//!    there — a bare hand's attack is [`crate::creatures_config`]'s `person.combat.attack` (`1.0`),
//!    a hunt's per-hunter haul rate is `labor_config.json`'s `hunt.per_worker_biomass_capacity`
//!    (`40.0`) and a gatherer's is its `forage.per_worker_biomass_capacity` (`8.0`), both of which
//!    **are** the equipped tiers: the shipped game has always run kitted. This file carries only
//!    what the *kits themselves* own, so no shipped number gets a second home to drift from.
//!
//! **Start-stocked and NOT craftable.** There is no replenishment path in this slice; running dry is
//! the intended pressure. The band's state is *wear*, not *stock*, so a freshly spawned
//! [`crate::components::BandEquipment`] is a full kit by construction (`Default` = zero wear) and no
//! spawn site needs to read this config.
//!
//! Loader mirrors [`crate::creatures_config`]: baked-in builtin + `EQUIPMENT_CONFIG_PATH` override +
//! [`EquipmentConfig::validate`] inside `from_json_str`, so **every** load path is validated and a
//! present-but-broken file is a boot panic rather than a silent fallback
//! (`.claude/rules/core_sim/config-loading.md`).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::combat::CombatStats;
use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_EQUIPMENT_CONFIG: &str = include_str!("data/equipment.json");

/// **The hunting kit** — spears. What makes a hunter's `attack` a real number, and the half of the
/// TOE that opens `docs/plan_hunt_through_combat.md` §4.2's gate
/// (`effective_attack = max(0, attack − defense)`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HuntingKitConfig {
    /// **The equipped per-hunter `attack`** — an absolute tier, not a bonus, because the model is
    /// two tiers and the unequipped one is the `person` row's intrinsic `attack` (`1.0`). Shipped at
    /// **20.0**: `1 → 20` is the largest single multiplier in the design and is deliberate — the
    /// first spear should feel like a different game (§4.8, SETTLED).
    pub equipped_attack: f32,
    /// Condition a fresh kit carries, on the shared 0–100 scale. A band is equipped while its
    /// accumulated wear is **strictly below** this.
    pub starting_durability: f32,
    /// Condition spent per **animal killed** — the hunting kit's use quantum (§4.2: the fight is
    /// what a spear is *for*). Shipped at **0.4**, so a full kit is **250 kills** ≈ 15 turns for the
    /// shipped ~16-worker band hunting Red Deer, matching `plan_early_game_labor`'s ~15–20 turn
    /// kit-duration target. **A per-kill charge is species-blind**, so a party on small game
    /// (`Wild Fowl`, 10 engaged per hunter) burns the same kit in under two turns; that is a config
    /// skew to tune with a per-species use cost, never with a turn clock.
    pub wear_per_kill: f32,
}

/// **The sled kit** — a travois or drag harness. The haul side of every *hunt* take.
///
/// **A carcass is one lumpy object you drag out whole**, so a container does not help you move a
/// deer — a sled does (§4.8). This kit used to be called the "carry kit" and was described as
/// baskets, which is the physical nonsense §4.8's "One kit, one job" corrects.
///
/// **It is a flat per-hunter multiplier and it needs NO PULLERS — SETTLED** (§4.8, §11). A crew cost
/// would make the hunt's carry non-linear in party size, and `hunt_haul_workers`, the fill target's
/// arithmetic and §5.1's trip length all assume that linearity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SledKitConfig {
    /// **The unequipped per-hunter haul rate**, in biomass/turn — what a sledless party drags home
    /// by hand. The *equipped* tier is `labor_config.json`'s `hunt.per_worker_biomass_capacity`
    /// (`40.0`), which stays where it is: it is the rate the shipped, start-kitted game has always
    /// run on, and duplicating it here would give one number two homes.
    ///
    /// **The hunt is TRANSPORT-bound, so this ratio is the SMALLER of the two carries** (§4.8): a
    /// sledless party can always drag *something*, hence `40 → 12` (under a third) against the
    /// basket's `8 → 1.6` (exactly a fifth). The shortfall needs no new mechanic — a party that
    /// cannot haul its kill leaves more of it, which `AnimalTake::wasted` already reports.
    pub unequipped_per_worker_biomass_capacity: f32,
    /// Condition a fresh kit carries, on the shared 0–100 scale.
    pub starting_durability: f32,
    /// Condition spent per unit of **biomass hauled home from a hunt** — the sled's use quantum, and
    /// distinct from the basket's by construction so a band that only gathers wears no sled. Shipped
    /// at **0.02**, so a full kit hauls **5000 biomass** ≈ 20 turns for the reference party —
    /// deliberately on a comparable clock to the hunting kit, so neither is the only kit the player
    /// ever watches.
    pub wear_per_biomass_hauled: f32,
}

/// **The basket kit** — the *forage* web's carry, and `plan_early_game_labor`'s other role.
///
/// **Berries are loose, divisible and bounded entirely by what you can hold**, which is exactly what
/// a basket fixes (§4.8). Before the three-kit split the forage web had no kit at all: its
/// `forage.per_worker_biomass_capacity` sat at its shipped value whatever the band carried.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BasketKitConfig {
    /// **The unequipped per-gatherer throughput**, in biomass/turn before the seasonal weight — what
    /// two cupped hands and the fold of a hide bring back. The *equipped* tier is
    /// `labor_config.json`'s `forage.per_worker_biomass_capacity` (`8.0`), which stays where it is
    /// for the same one-home-per-fact reason the sled's does.
    ///
    /// **Forage is CONTAINMENT-bound, so this ratio is the LARGER of the two carries** (§4.8): bare
    /// hands is *a handful against a basketful*. Shipped at **1.6** — exactly one fifth of the
    /// equipped `8.0`, against the sled's under-a-third — because losing your baskets should be the
    /// harsher of the two losses, not merely another number.
    pub unequipped_per_worker_biomass_capacity: f32,
    /// Condition a fresh kit carries, on the shared 0–100 scale.
    pub starting_durability: f32,
    /// Condition spent per unit of **biomass gathered** — the basket's use quantum, distinct from the
    /// sled's so a band that only hunts wears no baskets. Shipped at **0.04**, so a full kit gathers
    /// **2500 biomass** ≈ 19.5 turns for the shipped ~16-worker band (`16 × 8 = 128` biomass/turn),
    /// on the same ~15–20 turn clock as the other two.
    pub wear_per_biomass_gathered: f32,
}

/// **One of the three consumable component blocks a kit may draw on.** The variant names *are* the
/// JSON block keys, so a roster entry naming a component that has no block cannot deserialize —
/// "every `uses` entry names a real component block" is enforced by the type rather than by a
/// validation pass someone has to remember to extend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitComponent {
    HuntingKit,
    SledKit,
    BasketKit,
}

/// **A job a kit may be sent out on** — the two verbs that resolve a tier off the TOE. The scouting
/// and warrior roles are deliberately absent: they consume no component, so there is no kit axis to
/// choose along and no default to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitJob {
    Hunt,
    Forage,
}

impl KitJob {
    /// The wire/command token for this job — the same string `assign_labor`'s role token uses, so a
    /// kit's `jobs` list and a labor role are compared in one language.
    pub fn as_str(self) -> &'static str {
        match self {
            KitJob::Hunt => "hunt",
            KitJob::Forage => "forage",
        }
    }
}

/// One roster entry: a **named mask** over the component blocks above.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitDefinition {
    /// Stable id — what a command names and what the wire carries. Unique across the roster.
    pub id: String,
    /// Player-facing label. The sim never branches on it.
    pub display_name: String,
    /// Which verbs this kit may be sent on. A kit named for a job outside this list is a **command
    /// failure**, never a silent fall back to the job's default.
    pub jobs: Vec<KitJob>,
    /// The components this kit actually puts in the party's hands. **Empty is an ordinary roster
    /// entry** (the shipped `none`), not a sentinel: it grants nothing, so every predicate reads
    /// false and — because wear rides the same predicate — nothing is spent either.
    pub uses: Vec<KitComponent>,
}

/// The kit each verb reaches for when the player names none. Validated to name a real roster entry
/// whose `jobs` covers that verb, so [`EquipmentConfig::default_kit`] cannot fail after load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultKitsConfig {
    pub hunt: String,
    pub forage: String,
}

/// **A chosen kit, resolved once against the roster** — an id plus the component mask it stands for.
///
/// It is the **only** way anything asks "is this gear serving?": the three predicates below are
/// `kit uses the component AND the band still has condition in it`, and
/// [`crate::components::BandEquipment`]'s own condition tests are crate-private so a second reading
/// cannot appear beside them. The two shipped working kits mask in exactly the components every call
/// site used to consult unconditionally, which is what makes the roster a **no-op** for them.
///
/// **Resolved once, then carried.** A detached party stores its choice at launch and prices its
/// whole life from it — re-resolving against the band's current stock would silently re-arm a party
/// sent out bare the moment the band's spears were counted again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitChoice {
    id: Arc<str>,
    uses_hunting: bool,
    uses_sled: bool,
    uses_basket: bool,
}

impl KitChoice {
    /// The roster id this choice was resolved from — what the command named and what the wire carries.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// **Is the hunting kit serving this party?** The mask, and then the band's condition — a kit
    /// that does not carry spears is bare-handed however fresh the band's are, and a kit that does
    /// still steps down when they wear out.
    pub fn hunting_equipped(
        &self,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.uses_hunting && wear.has_hunting_condition(config)
    }

    /// Is the sled serving this party? Same rule as [`Self::hunting_equipped`].
    pub fn sled_equipped(
        &self,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.uses_sled && wear.has_sled_condition(config)
    }

    /// Are the baskets serving this party? Same rule as [`Self::hunting_equipped`].
    pub fn basket_equipped(
        &self,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.uses_basket && wear.has_basket_condition(config)
    }
}

/// Why a named kit cannot be sent on a verb. Both readings are **command failures with a reason** —
/// a bad selection must never quietly become the default, because the whole point of naming a kit is
/// that the player is comparing tiers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KitSelectionError {
    #[error("unknown kit '{id}' — the roster offers {available}")]
    Unknown { id: String, available: String },
    #[error("the {display_name} cannot be sent on a {job} — it is a {jobs} kit")]
    WrongJob {
        display_name: String,
        job: &'static str,
        jobs: String,
    },
}

/// Root TOE configuration: one block per shipped **component**, plus the **roster** of named kits
/// that mask over them. **One kit, one job** — nothing here composes two roles onto one block, which
/// is what makes the cross-checks in this module's tests (baskets do not touch the hunt; the sled
/// does not touch foraging) statements about the *type*, not about a convention someone has to
/// remember.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentConfig {
    pub hunting_kit: HuntingKitConfig,
    pub sled_kit: SledKitConfig,
    pub basket_kit: BasketKitConfig,
    /// The named kits a party may be sent out with. See [`KitDefinition`].
    pub kits: Vec<KitDefinition>,
    /// What each verb reaches for when the player names none.
    pub default_kits: DefaultKitsConfig,
}

impl EquipmentConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_EQUIPMENT_CONFIG)
                .expect("builtin equipment config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, EquipmentConfigError> {
        let config: EquipmentConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, EquipmentConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| EquipmentConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        EquipmentConfig::from_json_str(&contents)
    }

    /// Every roster entry, in file order — the client's picker list.
    pub fn kits(&self) -> &[KitDefinition] {
        &self.kits
    }

    /// The roster entry with this id, or `None`.
    pub fn kit_definition(&self, id: &str) -> Option<&KitDefinition> {
        self.kits.iter().find(|kit| kit.id == id)
    }

    /// Resolve a roster id into the mask it stands for. `None` for an id the roster does not carry.
    pub fn kit(&self, id: &str) -> Option<KitChoice> {
        self.kit_definition(id).map(Self::choice_from)
    }

    fn choice_from(definition: &KitDefinition) -> KitChoice {
        KitChoice {
            id: Arc::from(definition.id.as_str()),
            uses_hunting: definition.uses.contains(&KitComponent::HuntingKit),
            uses_sled: definition.uses.contains(&KitComponent::SledKit),
            uses_basket: definition.uses.contains(&KitComponent::BasketKit),
        }
    }

    /// The id this verb's default kit carries.
    pub fn default_kit_id(&self, job: KitJob) -> &str {
        match job {
            KitJob::Hunt => &self.default_kits.hunt,
            KitJob::Forage => &self.default_kits.forage,
        }
    }

    /// **What a verb runs on when the player names no kit.** Infallible after load: `validate`
    /// rejects a default that is not a real roster entry covering its own job, so a broken roster is
    /// a boot panic rather than a resolution that has to be handled at every call site.
    pub fn default_kit(&self, job: KitJob) -> KitChoice {
        self.kit(self.default_kit_id(job))
            .expect("validate guarantees every default kit names a roster entry")
    }

    /// **The command boundary's one resolution.** `None` = the player named no kit, which is the
    /// job's default; a named kit must exist *and* list this job, and anything else is an error the
    /// caller reports rather than a quiet fall back to the default.
    pub fn resolve_kit_for_job(
        &self,
        id: Option<&str>,
        job: KitJob,
    ) -> Result<KitChoice, KitSelectionError> {
        let Some(id) = id else {
            return Ok(self.default_kit(job));
        };
        let Some(definition) = self.kit_definition(id) else {
            return Err(KitSelectionError::Unknown {
                id: id.to_string(),
                available: self.kit_ids_for_message(),
            });
        };
        if !definition.jobs.contains(&job) {
            return Err(KitSelectionError::WrongJob {
                display_name: definition.display_name.clone(),
                job: job.as_str(),
                jobs: definition
                    .jobs
                    .iter()
                    .map(|job| job.as_str())
                    .collect::<Vec<_>>()
                    .join("/"),
            });
        }
        Ok(Self::choice_from(definition))
    }

    /// The roster's ids, for a refusal message — a player who mistypes a kit is told what there is.
    fn kit_ids_for_message(&self) -> String {
        self.kits
            .iter()
            .map(|kit| kit.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// **A hunter's per-unit combat profile, kit composed in** — `intrinsic ⊕ loadout`, the
    /// composition `docs/plan_predators.md` names and the *one* seam any consumer resolves a hunter's
    /// `attack` through.
    ///
    /// `equipped` swaps the **whole attack tier** (never adds to it): the unequipped number is the
    /// `person` row's own `attack`, so returning `intrinsic` unchanged is exactly what "dropped to the
    /// unequipped tier" means, and the step between the two is the cliff.
    ///
    /// **Defense, range and wariness are untouched** — the hunting kit is a weapon. Armour is the
    /// Warrior role's kit, which this slice deliberately does not ship.
    pub fn hunter_profile(&self, intrinsic: CombatStats, equipped: bool) -> CombatStats {
        if equipped {
            CombatStats {
                attack: self.hunting_kit.equipped_attack,
                ..intrinsic
            }
        } else {
            intrinsic
        }
    }

    /// **A band's per-worker HUNT haul rate** — the *sled's* tier, resolved against the equipped rate
    /// the caller already holds (`labor_config.hunt.per_worker_biomass_capacity`). The single seam
    /// every hunt-take, crew-size and hunt-forecast site reads, so the assign-time seed and the
    /// resolved row can never disagree about which tier a band is on.
    ///
    /// **Baskets are not consulted here, by construction** — dragging a carcass is not a containment
    /// problem (§4.8). The forage twin is [`Self::forage_per_worker_biomass_capacity`].
    pub fn hunt_per_worker_biomass_capacity(&self, equipped_rate: f32, equipped: bool) -> f32 {
        if equipped {
            equipped_rate
        } else {
            self.sled_kit.unequipped_per_worker_biomass_capacity
        }
    }

    /// **A band's per-worker GATHER throughput** — the *basket's* tier, resolved against the equipped
    /// rate the caller already holds (`labor_config.forage.per_worker_biomass_capacity`), before the
    /// tile's seasonal weight is folded in ([`crate::forage::forage_per_worker_biomass`]).
    ///
    /// **The sled is not consulted here, by construction** — a drag harness does not help you hold
    /// more berries (§4.8). The hunt twin is [`Self::hunt_per_worker_biomass_capacity`].
    pub fn forage_per_worker_biomass_capacity(&self, equipped_rate: f32, equipped: bool) -> f32 {
        if equipped {
            equipped_rate
        } else {
            self.basket_kit.unequipped_per_worker_biomass_capacity
        }
    }

    /// Invariants a TOE config must satisfy. **A kit with no wear rate is not consumable** and a kit
    /// with no durability is born dry, so both are rejected rather than shipped as a silently
    /// eternal (or silently absent) kit. Three kits × three dials.
    pub fn validate(&self) -> Result<(), EquipmentConfigError> {
        let checks: [(&'static str, f32); 9] = [
            (
                "hunting_kit.equipped_attack",
                self.hunting_kit.equipped_attack,
            ),
            (
                "hunting_kit.starting_durability",
                self.hunting_kit.starting_durability,
            ),
            ("hunting_kit.wear_per_kill", self.hunting_kit.wear_per_kill),
            (
                "sled_kit.unequipped_per_worker_biomass_capacity",
                self.sled_kit.unequipped_per_worker_biomass_capacity,
            ),
            (
                "sled_kit.starting_durability",
                self.sled_kit.starting_durability,
            ),
            (
                "sled_kit.wear_per_biomass_hauled",
                self.sled_kit.wear_per_biomass_hauled,
            ),
            (
                "basket_kit.unequipped_per_worker_biomass_capacity",
                self.basket_kit.unequipped_per_worker_biomass_capacity,
            ),
            (
                "basket_kit.starting_durability",
                self.basket_kit.starting_durability,
            ),
            (
                "basket_kit.wear_per_biomass_gathered",
                self.basket_kit.wear_per_biomass_gathered,
            ),
        ];
        for (field, value) in checks {
            if !value.is_finite() || value <= 0.0 {
                return Err(EquipmentConfigError::Invalid {
                    field,
                    constraint: "be finite and greater than 0".to_string(),
                    value: value.to_string(),
                });
            }
        }
        self.validate_roster()
    }

    /// **The roster's own invariants.** A broken roster is a boot panic
    /// (`.claude/rules/core_sim/config-loading.md`), because every one of these breakages ends with
    /// a party priced at a tier nobody chose:
    ///
    /// - **ids are unique** — otherwise `kit_definition` silently picks the first of two, and which
    ///   one a command got would depend on file order.
    /// - **a kit lists at least one job** — a kit that can be sent on nothing is dead config, and
    ///   the roster is the client's picker list.
    /// - **each default names a real kit** — [`Self::default_kit`] is infallible by construction, so
    ///   this is what makes that `expect` a statement rather than a hope.
    /// - **each default covers its own job** — a `forage` default the forage verb would itself
    ///   refuse would fail every gather assignment at the command boundary.
    ///
    /// "Every `uses` entry names a real component block" is not checked here: [`KitComponent`]'s
    /// variants *are* the block keys, so a bad name fails to deserialize — which is the same boot
    /// panic, one layer earlier.
    fn validate_roster(&self) -> Result<(), EquipmentConfigError> {
        for (index, kit) in self.kits.iter().enumerate() {
            if self.kits[..index].iter().any(|prior| prior.id == kit.id) {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("duplicate kit id '{}'", kit.id),
                });
            }
            if kit.jobs.is_empty() {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("kit '{}' lists no jobs — it can be sent on nothing", kit.id),
                });
            }
        }
        for job in [KitJob::Hunt, KitJob::Forage] {
            let id = self.default_kit_id(job);
            let Some(definition) = self.kit_definition(id) else {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "default_kits.{} names '{}', which is not in the roster",
                        job.as_str(),
                        id
                    ),
                });
            };
            if !definition.jobs.contains(&job) {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "default_kits.{} names '{}', whose jobs do not include {}",
                        job.as_str(),
                        id,
                        job.as_str()
                    ),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum EquipmentConfigError {
    #[error("failed to read equipment config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse equipment config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid equipment config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
    /// A roster the sim cannot resolve a kit through — see [`EquipmentConfig::validate_roster`].
    /// Carries the same weight as [`Self::Invalid`]: a file that is there and wrong.
    #[error("invalid equipment kit roster: {reason}")]
    InvalidRoster { reason: String },
}

impl ConfigLoadError for EquipmentConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the TOE configuration.
#[derive(Resource, Debug, Clone)]
pub struct EquipmentConfigHandle(pub Arc<EquipmentConfig>);

impl EquipmentConfigHandle {
    pub fn new(config: Arc<EquipmentConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<EquipmentConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<EquipmentConfig>) {
        self.0 = config;
    }
}

impl Default for EquipmentConfigHandle {
    fn default() -> Self {
        Self(EquipmentConfig::builtin())
    }
}

/// Metadata about the TOE configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct EquipmentConfigMetadata {
    path: Option<PathBuf>,
}

impl EquipmentConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Load TOE configuration from environment (`EQUIPMENT_CONFIG_PATH`) or the default data path. The
/// file is **validated** before it can reach the sim, and a broken invariant is as fatal as a parse
/// error. Only an absent *default* path falls back to the builtin; a present-but-broken file, or an
/// `EQUIPMENT_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_equipment_config_from_env() -> (Arc<EquipmentConfig>, EquipmentConfigMetadata) {
    let (config, source) = load_config_from_env(
        "EQUIPMENT_CONFIG_PATH",
        "equipment_config",
        "src/data/equipment.json",
        EquipmentConfig::builtin,
        EquipmentConfig::from_file,
    );
    (config, EquipmentConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::RangeBand;
    use crate::creatures_config::CreaturesConfig;

    /// The shipped equipped **hunt** haul rate — `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity`, named here so this module's unit tests do not restate a
    /// number that lives elsewhere.
    fn equipped_haul_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity
    }

    /// The shipped equipped **gather** throughput — `labor_config.json`'s
    /// `forage.per_worker_biomass_capacity`, for the same reason.
    fn equipped_gather_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .forage
            .per_worker_biomass_capacity
    }

    #[test]
    fn builtin_config_ships_all_three_kits() {
        let config = EquipmentConfig::builtin();
        assert_eq!(config.hunting_kit.equipped_attack, 20.0);
        assert!(config.hunting_kit.starting_durability > 0.0);
        assert!(config.hunting_kit.wear_per_kill > 0.0);
        assert!(config.sled_kit.unequipped_per_worker_biomass_capacity > 0.0);
        assert!(config.sled_kit.wear_per_biomass_hauled > 0.0);
        assert!(config.basket_kit.unequipped_per_worker_biomass_capacity > 0.0);
        assert!(config.basket_kit.wear_per_biomass_gathered > 0.0);
    }

    /// **The equipped tier must beat the unequipped one on ALL THREE axes** — a "kit" that made you
    /// worse is incoherent. The three unequipped tiers live in the configs that own them, so this is
    /// the one place the files are compared.
    #[test]
    fn every_equipped_tier_beats_its_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let bare_attack = CreaturesConfig::builtin().person().attack;
        assert!(
            equipment.hunting_kit.equipped_attack > bare_attack,
            "the hunting kit must raise attack above the bare-handed {bare_attack}"
        );
        assert!(
            equipped_haul_rate() > equipment.sled_kit.unequipped_per_worker_biomass_capacity,
            "the sled must raise the hunt's haul rate above the bare-armed tier"
        );
        assert!(
            equipped_gather_rate() > equipment.basket_kit.unequipped_per_worker_biomass_capacity,
            "baskets must raise the gather rate above the bare-handed tier"
        );
    }

    /// **The two carries want different SHAPES, not merely different numbers** (§4.8). Forage is
    /// *containment*-bound — a handful against a basketful — so its drop is the harsher one; the hunt
    /// is *transport*-bound, and a sledless party can always drag something. Asserted as the ordering
    /// between the two ratios, with a liveness assertion on each ratio so a kit whose tiers collapsed
    /// to equal cannot satisfy it vacuously.
    #[test]
    fn losing_your_baskets_costs_proportionally_more_than_losing_your_sled() {
        let equipment = EquipmentConfig::builtin();
        let sled_ratio =
            equipped_haul_rate() / equipment.sled_kit.unequipped_per_worker_biomass_capacity;
        let basket_ratio =
            equipped_gather_rate() / equipment.basket_kit.unequipped_per_worker_biomass_capacity;
        assert!(
            sled_ratio > 1.0,
            "the sled must be live at all: ratio {sled_ratio}"
        );
        assert!(
            basket_ratio > 1.0,
            "baskets must be live at all: ratio {basket_ratio}"
        );
        assert!(
            basket_ratio > sled_ratio,
            "containment must bite harder than transport: baskets ×{basket_ratio} vs sled ×{sled_ratio}"
        );
    }

    #[test]
    fn the_hunter_profile_swaps_only_the_attack_tier() {
        let equipment = EquipmentConfig::builtin();
        let bare = CreaturesConfig::builtin().person();
        let kitted = equipment.hunter_profile(bare, true);
        assert_eq!(kitted.attack, equipment.hunting_kit.equipped_attack);
        // Defense/range/wariness are the Warrior kit's business, not the spear's.
        assert_eq!(kitted.defense, bare.defense);
        assert_eq!(kitted.range, RangeBand::Melee);
        assert_eq!(kitted.wariness, bare.wariness);
        // Unequipped is the intrinsic row itself — the tier it drops back to.
        assert_eq!(equipment.hunter_profile(bare, false), bare);
    }

    #[test]
    fn each_carry_tier_resolves_to_one_of_exactly_two_rates() {
        let equipment = EquipmentConfig::builtin();
        let hunt = equipped_haul_rate();
        let gather = equipped_gather_rate();
        assert_eq!(equipment.hunt_per_worker_biomass_capacity(hunt, true), hunt);
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, false),
            equipment.sled_kit.unequipped_per_worker_biomass_capacity
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, true),
            gather
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, false),
            equipment.basket_kit.unequipped_per_worker_biomass_capacity
        );
    }

    /// **ONE KIT, ONE JOB, at the resolver seam** (§4.8) — the cross-check that would have caught the
    /// original defect, where the "carry kit" called baskets raised the *hunt's* haul and foraging
    /// got nothing. A dry basket must leave the hunt's tier untouched and a dry sled must leave the
    /// gather's untouched, so each resolver is swept across the other kit's state.
    #[test]
    fn a_dry_basket_does_not_touch_the_hunt_and_a_dry_sled_does_not_touch_the_gather() {
        let equipment = EquipmentConfig::builtin();
        let hunt = equipped_haul_rate();
        let gather = equipped_gather_rate();
        // Liveness on both sides: each kit really does move its own number...
        assert_ne!(
            equipment.hunt_per_worker_biomass_capacity(hunt, true),
            equipment.hunt_per_worker_biomass_capacity(hunt, false),
            "the sled tier must be live, or the cross-check below is vacuous"
        );
        assert_ne!(
            equipment.forage_per_worker_biomass_capacity(gather, true),
            equipment.forage_per_worker_biomass_capacity(gather, false),
            "the basket tier must be live, or the cross-check below is vacuous"
        );
        // ...and neither resolver can even be *asked* about the other kit — each takes one
        // `equipped` flag and reads one block — so the equipped answers are the shipped rates
        // whatever the other kit's state is.
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, true),
            hunt,
            "an equipped sled hauls at the shipped rate whatever the baskets are doing"
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, true),
            gather,
            "equipped baskets gather at the shipped rate whatever the sled is doing"
        );
    }

    #[test]
    fn validate_rejects_a_kit_that_never_wears_out() {
        // `wear_per_kill = 0` would make a "consumable" kit eternal — the one value that silently
        // deletes the whole pressure this slice exists to create.
        let err = EquipmentConfig::from_json_str(&kit_json(
            "\"wear_per_kill\": 0.0",
            "\"wear_per_biomass_hauled\": 0.02",
            "\"starting_durability\": 100.0",
        ))
        .expect_err("a zero wear rate is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Invalid { field, .. } if field == "hunting_kit.wear_per_kill"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_a_kit_born_dry() {
        let err = EquipmentConfig::from_json_str(&kit_json(
            "\"wear_per_kill\": 2.0",
            "\"wear_per_biomass_hauled\": 0.02",
            "\"starting_durability\": 0.0",
        ))
        .expect_err("zero durability is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Invalid { field, .. } if field == "basket_kit.starting_durability"),
            "unexpected error: {err}"
        );
    }

    /// **The basket block is REQUIRED, not defaulted.** A file that forgot it would otherwise leave
    /// the forage web silently unkitted again — the exact defect §4.8 corrects.
    #[test]
    fn a_config_missing_the_basket_kit_is_rejected() {
        let json = r#"{
            "hunting_kit": { "equipped_attack": 20.0, "starting_durability": 100.0, "wear_per_kill": 0.4 },
            "sled_kit": { "unequipped_per_worker_biomass_capacity": 12.0, "starting_durability": 100.0, "wear_per_biomass_hauled": 0.02 }
        }"#;
        let err = EquipmentConfig::from_json_str(json).expect_err("a missing kit block is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Parse(_)),
            "unexpected error: {err}"
        );
    }

    /// A three-kit fixture with one dial per kit under the test's control — so a validate test says
    /// which dial it is breaking instead of restating the whole file.
    fn kit_json(hunting_wear: &str, sled_wear: &str, basket_durability: &str) -> String {
        format!(
            r#"{{
            "hunting_kit": {{ "equipped_attack": 20.0, "starting_durability": 100.0, {hunting_wear} }},
            "sled_kit": {{ "unequipped_per_worker_biomass_capacity": 12.0, "starting_durability": 100.0, {sled_wear} }},
            "basket_kit": {{ "unequipped_per_worker_biomass_capacity": 1.6, {basket_durability}, "wear_per_biomass_gathered": 0.04 }},
            {ROSTER_JSON}
        }}"#
        )
    }

    // -----------------------------------------------------------------------------------------
    // THE KIT ROSTER
    // -----------------------------------------------------------------------------------------

    /// **The two working kits mask in exactly what every call site used to consult
    /// unconditionally** — which is the whole load-bearing claim of the roster arc. If this fails,
    /// choosing a shipped kit is not the no-op the design rests on and every number in the game has
    /// moved.
    #[test]
    fn the_two_shipped_kits_reproduce_the_pre_roster_predicates() {
        let equipment = EquipmentConfig::builtin();
        let fresh = crate::components::BandEquipment::default();
        let big_game = equipment
            .kit("big_game")
            .expect("the roster ships big_game");
        let gathering = equipment
            .kit("gathering")
            .expect("the roster ships gathering");

        // The hunt job's kit reaches for the hunt's two components and nothing else.
        assert!(big_game.hunting_equipped(&fresh, &equipment));
        assert!(big_game.sled_equipped(&fresh, &equipment));
        assert!(
            !big_game.basket_equipped(&fresh, &equipment),
            "one kit, one job — a spear-and-sled party carries no baskets"
        );
        // And the forage job's reaches for the basket and nothing else.
        assert!(gathering.basket_equipped(&fresh, &equipment));
        assert!(!gathering.hunting_equipped(&fresh, &equipment));
        assert!(!gathering.sled_equipped(&fresh, &equipment));

        // **The tiers those masks resolve to are the shipped equipped ones, bit for bit.**
        let labor = crate::labor_config::LaborConfig::builtin();
        let intrinsic = CreaturesConfig::builtin().person();
        assert_eq!(
            equipment
                .hunter_profile(intrinsic, big_game.hunting_equipped(&fresh, &equipment))
                .attack,
            equipment.hunting_kit.equipped_attack
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                big_game.sled_equipped(&fresh, &equipment)
            ),
            labor.hunt.per_worker_biomass_capacity
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                gathering.basket_equipped(&fresh, &equipment)
            ),
            labor.forage.per_worker_biomass_capacity
        );
    }

    /// **A kit that uses nothing reads false everywhere, however fresh the band's gear is** — and
    /// the tiers it resolves to are the three unequipped ones. `none` is an ordinary roster member,
    /// so this is a statement about an empty `uses` list rather than about a sentinel id.
    #[test]
    fn a_kit_that_uses_nothing_runs_at_every_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let fresh = crate::components::BandEquipment::default();
        let none = equipment.kit("none").expect("the roster ships none");
        assert!(!none.hunting_equipped(&fresh, &equipment));
        assert!(!none.sled_equipped(&fresh, &equipment));
        assert!(!none.basket_equipped(&fresh, &equipment));

        let labor = crate::labor_config::LaborConfig::builtin();
        let intrinsic = CreaturesConfig::builtin().person();
        assert_eq!(
            equipment
                .hunter_profile(intrinsic, none.hunting_equipped(&fresh, &equipment))
                .attack,
            intrinsic.attack,
            "a party carrying no spears fights bare-handed"
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                none.sled_equipped(&fresh, &equipment)
            ),
            equipment.sled_kit.unequipped_per_worker_biomass_capacity
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                none.basket_equipped(&fresh, &equipment)
            ),
            equipment.basket_kit.unequipped_per_worker_biomass_capacity
        );
    }

    /// **A wrong-job kit is refused, and an unknown one too** — never a quiet fall back to the
    /// default. Both readings are the same defect from the player's side: they asked to compare
    /// tiers and were answered about a different one.
    #[test]
    fn a_kit_is_refused_for_a_job_it_does_not_list_and_for_an_id_that_is_not_there() {
        let equipment = EquipmentConfig::builtin();
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("gathering"), KitJob::Hunt),
            Err(KitSelectionError::WrongJob { .. })
        ));
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("big_game"), KitJob::Forage),
            Err(KitSelectionError::WrongJob { .. })
        ));
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("spear_of_destiny"), KitJob::Hunt),
            Err(KitSelectionError::Unknown { .. })
        ));
        // `none` covers both jobs, so it resolves on either.
        assert!(equipment
            .resolve_kit_for_job(Some("none"), KitJob::Hunt)
            .is_ok());
        assert!(equipment
            .resolve_kit_for_job(Some("none"), KitJob::Forage)
            .is_ok());
        // Naming none at all is the job's default, which is not an error.
        assert_eq!(
            equipment
                .resolve_kit_for_job(None, KitJob::Hunt)
                .expect("no selection resolves to the default")
                .id(),
            equipment.default_kit_id(KitJob::Hunt)
        );
    }

    /// **Every broken roster shape is rejected at load**, which under
    /// `.claude/rules/core_sim/config-loading.md` makes it a boot panic rather than a sim quietly
    /// running a kit table nobody authored. Swept rather than asserted one at a time so a new
    /// invariant has an obvious place to join.
    #[test]
    fn validate_rejects_every_broken_roster_shape() {
        let cases: [(&str, &str); 4] = [
            (
                "duplicate ids",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt"], "uses": [] },
                    { "id": "big_game", "display_name": "B", "jobs": ["hunt"], "uses": [] }
                ],
                "default_kits": { "hunt": "big_game", "forage": "big_game" }"#,
            ),
            (
                "a kit that can be sent on nothing",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": [], "uses": [] }
                ],
                "default_kits": { "hunt": "big_game", "forage": "big_game" }"#,
            ),
            (
                "a default naming no roster entry",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "ghost", "forage": "big_game" }"#,
            ),
            (
                "a default whose jobs do not cover its own job",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt"], "uses": [] },
                    { "id": "gathering", "display_name": "B", "jobs": ["forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "gathering", "forage": "gathering" }"#,
            ),
        ];
        for (what, roster) in cases {
            let err = EquipmentConfig::from_json_str(&component_json(roster))
                .expect_err(&format!("{what} must be rejected"));
            assert!(
                matches!(err, EquipmentConfigError::InvalidRoster { .. }),
                "{what}: unexpected error: {err}"
            );
        }
    }

    /// **A `uses` entry naming a component block that does not exist fails to DESERIALIZE**, which
    /// is the same boot panic one layer earlier — the variant names of [`KitComponent`] *are* the
    /// block keys, so this invariant is carried by the type rather than by a validation pass.
    #[test]
    fn a_kit_using_a_component_that_does_not_exist_fails_to_parse() {
        let err = EquipmentConfig::from_json_str(&component_json(
            r#""kits": [
                { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": ["net_kit"] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "big_game" }"#,
        ))
        .expect_err("a component block that does not exist is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Parse(_)),
            "unexpected error: {err}"
        );
    }

    /// The three shipped **component** blocks at their shipped values, with the roster left to the
    /// caller — the mirror of [`kit_json`], for fixtures that break the roster instead of a dial.
    fn component_json(roster: &str) -> String {
        format!(
            r#"{{
            "hunting_kit": {{ "equipped_attack": 20.0, "starting_durability": 100.0, "wear_per_kill": 0.4 }},
            "sled_kit": {{ "unequipped_per_worker_biomass_capacity": 12.0, "starting_durability": 100.0, "wear_per_biomass_hauled": 0.02 }},
            "basket_kit": {{ "unequipped_per_worker_biomass_capacity": 1.6, "starting_durability": 100.0, "wear_per_biomass_gathered": 0.04 }},
            {roster}
        }}"#
        )
    }

    /// A minimal **valid** roster, so a fixture testing one of the three *component* blocks does not
    /// have to restate the shipped kit list to get past the roster's own validation.
    const ROSTER_JSON: &str = r#""kits": [
                { "id": "big_game", "display_name": "Big-game kit", "jobs": ["hunt"], "uses": ["hunting_kit", "sled_kit"] },
                { "id": "gathering", "display_name": "Gathering kit", "jobs": ["forage"], "uses": ["basket_kit"] },
                { "id": "none", "display_name": "No kit", "jobs": ["hunt", "forage"], "uses": [] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "gathering" }"#;
}
