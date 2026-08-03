//! **The minimal TOE** (`data/equipment.json`) — the two consumable kits that lift a band's
//! Hunting and carry roles from their *unequipped* to their *equipped* tier.
//!
//! Design: `docs/plan_early_game_labor.md` → "Equipment / TOE" (the authoritative arc) and
//! `docs/plan_hunt_through_combat.md` §4.8 (why a minimal TOE has to land before the hunt resolves
//! through combat: a bare-handed hunter's `attack` is `1`, below every megafauna's `defense`, so
//! without a spear the gate is the entire game).
//!
//! **Three rules this module exists to keep:**
//!
//! 1. **Two tiers, never a taper.** A kit's performance is *flat* until it expires and then the role
//!    **steps down** to its unequipped tier. Durability and performance are deliberately orthogonal
//!    axes — coupling them (a kit that gets worse as it wears) would be the modeling mistake the arc
//!    calls out, and would let a future crafting economy tune only one thing.
//! 2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2). A
//!    turn-based clock charges an idle march the same as a slaughter, which would make denial free.
//!    The hunting kit wears per **animal killed**; the carry kit wears per **biomass hauled home**.
//! 3. **One home per fact.** Both *unequipped* tiers already had homes and stay there — a bare
//!    hand's attack is [`crate::creatures_config`]'s `person.combat.attack` (`1.0`) and a hunt's
//!    per-hunter haul rate is `labor_config.json`'s `hunt.per_worker_biomass_capacity` (`40.0`,
//!    which **is** the equipped tier: the shipped game has always run kitted). This file carries
//!    only what the *kit itself* owns, so no shipped number gets a second home to drift from.
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
use serde::Deserialize;
use thiserror::Error;

use crate::combat::CombatStats;
use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_EQUIPMENT_CONFIG: &str = include_str!("data/equipment.json");

/// **The hunting kit** — spears. What makes a hunter's `attack` a real number, and the half of the
/// TOE that opens `docs/plan_hunt_through_combat.md` §4.2's gate
/// (`effective_attack = max(0, attack − defense)`).
#[derive(Debug, Clone, Copy, Deserialize)]
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

/// **The carry kit** — baskets. The haul side of every hunt take, and the other half of
/// `plan_early_game_labor`'s role table.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CarryKitConfig {
    /// **The unequipped per-worker haul rate**, in biomass/turn — what a crew manages with bare arms
    /// once the baskets are gone. The *equipped* tier is `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity` (`40.0`), which stays where it is: it is the rate the
    /// shipped, start-kitted game has always run on, and duplicating it here would give one number
    /// two homes.
    pub unequipped_per_worker_biomass_capacity: f32,
    /// Condition a fresh kit carries, on the shared 0–100 scale.
    pub starting_durability: f32,
    /// Condition spent per unit of **biomass carried home** — the carry kit's use quantum. Shipped
    /// at **0.02**, so a full kit hauls **5000 biomass** ≈ 20 turns for the same reference party —
    /// deliberately on a comparable clock to the hunting kit, so neither kit is the only one the
    /// player ever watches.
    pub wear_per_biomass_carried: f32,
}

/// Root TOE configuration: one block per shipped kit.
#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentConfig {
    pub hunting_kit: HuntingKitConfig,
    pub carry_kit: CarryKitConfig,
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

    /// **A band's per-worker hunt haul rate** — the carry kit's tier, resolved against the *equipped*
    /// rate the caller already holds (`labor_config.hunt.per_worker_biomass_capacity`). The single
    /// seam every hunt-take, crew-size and forecast site reads, so the assign-time seed and the
    /// resolved row can never disagree about which tier a band is on.
    pub fn per_worker_biomass_capacity(&self, equipped_rate: f32, equipped: bool) -> f32 {
        if equipped {
            equipped_rate
        } else {
            self.carry_kit.unequipped_per_worker_biomass_capacity
        }
    }

    /// Invariants a TOE config must satisfy. **A kit with no wear rate is not consumable** and a kit
    /// with no durability is born dry, so both are rejected rather than shipped as a silently
    /// eternal (or silently absent) kit.
    pub fn validate(&self) -> Result<(), EquipmentConfigError> {
        let checks: [(&'static str, f32); 6] = [
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
                "carry_kit.unequipped_per_worker_biomass_capacity",
                self.carry_kit.unequipped_per_worker_biomass_capacity,
            ),
            (
                "carry_kit.starting_durability",
                self.carry_kit.starting_durability,
            ),
            (
                "carry_kit.wear_per_biomass_carried",
                self.carry_kit.wear_per_biomass_carried,
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

    /// The shipped equipped haul rate — `labor_config.json`'s `hunt.per_worker_biomass_capacity`,
    /// named here so this module's unit tests do not restate a number that lives elsewhere.
    fn equipped_haul_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity
    }

    #[test]
    fn builtin_config_ships_both_kits() {
        let config = EquipmentConfig::builtin();
        assert_eq!(config.hunting_kit.equipped_attack, 20.0);
        assert!(config.hunting_kit.starting_durability > 0.0);
        assert!(config.hunting_kit.wear_per_kill > 0.0);
        assert!(config.carry_kit.unequipped_per_worker_biomass_capacity > 0.0);
        assert!(config.carry_kit.wear_per_biomass_carried > 0.0);
    }

    /// **The equipped tier must beat the unequipped one on BOTH axes** — a "kit" that made you worse
    /// is incoherent. The two unequipped tiers live in the two configs that own them, so this is the
    /// one place the three files are compared.
    #[test]
    fn every_equipped_tier_beats_its_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let bare_attack = CreaturesConfig::builtin().person().attack;
        assert!(
            equipment.hunting_kit.equipped_attack > bare_attack,
            "the hunting kit must raise attack above the bare-handed {bare_attack}"
        );
        assert!(
            equipped_haul_rate() > equipment.carry_kit.unequipped_per_worker_biomass_capacity,
            "the carry kit must raise the haul rate above the bare-armed tier"
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
    fn the_carry_tier_resolves_to_one_of_exactly_two_rates() {
        let equipment = EquipmentConfig::builtin();
        let equipped = equipped_haul_rate();
        assert_eq!(
            equipment.per_worker_biomass_capacity(equipped, true),
            equipped
        );
        assert_eq!(
            equipment.per_worker_biomass_capacity(equipped, false),
            equipment.carry_kit.unequipped_per_worker_biomass_capacity
        );
    }

    #[test]
    fn validate_rejects_a_kit_that_never_wears_out() {
        // `wear_per_kill = 0` would make a "consumable" kit eternal — the one value that silently
        // deletes the whole pressure this slice exists to create.
        let json = r#"{
            "hunting_kit": { "equipped_attack": 20.0, "starting_durability": 100.0, "wear_per_kill": 0.0 },
            "carry_kit": { "unequipped_per_worker_biomass_capacity": 12.0, "starting_durability": 100.0, "wear_per_biomass_carried": 0.05 }
        }"#;
        let err = EquipmentConfig::from_json_str(json).expect_err("a zero wear rate is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Invalid { field, .. } if field == "hunting_kit.wear_per_kill"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_a_kit_born_dry() {
        let json = r#"{
            "hunting_kit": { "equipped_attack": 20.0, "starting_durability": 100.0, "wear_per_kill": 2.0 },
            "carry_kit": { "unequipped_per_worker_biomass_capacity": 12.0, "starting_durability": 0.0, "wear_per_biomass_carried": 0.05 }
        }"#;
        let err = EquipmentConfig::from_json_str(json).expect_err("zero durability is invalid");
        assert!(
            matches!(err, EquipmentConfigError::Invalid { field, .. } if field == "carry_kit.starting_durability"),
            "unexpected error: {err}"
        );
    }
}
