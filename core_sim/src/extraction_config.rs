//! **The deposits table** (`data/extraction.json`) — where `wood` and `stone` come *from*.
//!
//! Design: `docs/plan_extraction.md` §2/§5. One idea, and everything here exists to keep it:
//!
//! > **A deposit is a stock with a capacity and a regrowth rate, and rock's rate is zero.**
//!
//! There is no `is_finite` flag, no finite-deposit branch and no special case: with
//! `regrowth_rate: 0.0` the growth term ([`crate::extraction::deposit_regrowth`]) returns the stock
//! unchanged, so *a quarry only ever goes down* survives as arithmetic rather than as a rule someone
//! has to remember not to break. That is what makes a metal a **deposit and a config row** instead
//! of a second system.
//!
//! Three rules the module enforces, all three load-bearing:
//!
//! 1. **Regrowth belongs to the DEPOSIT — the terrain — not to the material and not to the branch.**
//!    A plains flint scatter and a highland quarry are the same branch and the same material, and
//!    only one of them runs out. So both `capacity` and `regrowth_rate` are per-terrain, on one row
//!    per terrain.
//! 2. **Capacity is a pure function of the tile, never saved state** —
//!    [`crate::extraction::tile_deposit_capacity`] is the one seam every caller goes through,
//!    exactly as `forage::tile_forage_capacity` is for the human food web. The **only** saved thing
//!    is the stock.
//! 3. **A source states the characteristics of what it yields**, and the axes it states must be
//!    exactly the ones the material declares in `materials.json` — that file's own rule is that a
//!    material must be *rated* to exist. [`ExtractionConfig::validate_against_materials`] is where
//!    that is checked, because it is the one place both tables are in scope.
//!
//! **A terrain absent from `by_terrain` has no deposit of that material** — capacity `0`, no source
//! can be seeded there, and the rung's verb is refused there. Absence is the answer; there is no
//! `enabled` flag and no parked `0.0`.
//!
//! Loader mirrors [`crate::materials_config`]: baked-in builtin + `EXTRACTION_CONFIG_PATH`
//! override + [`ExtractionConfig::validate`] inside `from_json_str`, so **every** load path is
//! validated and a present-but-broken file is a boot panic rather than a silent fallback
//! (`.claude/rules/core_sim/config-loading.md`).

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use sim_schema::TerrainType;
use thiserror::Error;

use crate::{
    config_load::{load_config_from_env, ConfigLoadError},
    intensification::RungBranch,
    materials_config::MaterialsConfig,
};

pub const BUILTIN_EXTRACTION_CONFIG: &str = include_str!("data/extraction.json");

/// **WHAT A TILE WITH NO ROW IN A DEPOSIT'S TABLE HOLDS** — nothing, which is what makes *absence is
/// the answer* a reading rather than a convention. Named because the predicate *"is there a deposit
/// here at all"* is exactly the comparison against it, at the seeding path and at the site rule
/// alike.
pub const NO_DEPOSIT: f32 = 0.0;

/// **A DEPOSIT THAT DOES NOT RENEW** — `regrowth_rate`'s reading on every rock body, and the one
/// value the whole model is built around: `0 × anything` is still `0`, so a forestry rung's
/// `regrowth_multiplier` cannot make a quarry regrow however it is tuned.
pub const NEVER_RENEWS: f32 = 0.0;

/// **The closed interval a characteristic reading lives on** — the same one
/// [`crate::materials_config::READING_MIN`]/`READING_MAX` bound every other source's yield edge to,
/// restated here so a deposit's ratings are checked at *this* file's load rather than at the far end
/// of a merge.
const READING_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

/// **WHAT ONE TERRAIN HOLDS OF ONE MATERIAL** — the whole record, and deliberately one row per
/// terrain rather than three parallel terrain-keyed tables.
///
/// The capacity, the rate and the ratings are three readings of *the same ground*, so they live
/// together: parallel tables can drift out of step with one another and only a load check would
/// catch it, where a single row makes the drift unrepresentable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositTerrain {
    /// **WHAT THE LAND HOLDS AT FULL** — the stock a fresh source is seeded at, and the ceiling
    /// regrowth climbs back toward. Validated finite and `> NO_DEPOSIT`: a zero here would be a row
    /// saying *"there is no deposit"* while looking like a tuning value, and the table already says
    /// that by carrying no row.
    ///
    /// ⛔ **NO RUNG MAY RAISE IT.** Every per-rung quantity on these two branches is read off the
    /// ladder position, and this one deliberately is not — the extraction floor is
    /// `(1 − recovery_fraction) × capacity`, so a capacity that climbed with the rung would drag the
    /// floor up under a working already standing on it (`docs/plan_standing_upkeep.md` §6, the floor
    /// trap). Capacity comes from here and nowhere else, so the floor can only ever move **down**.
    pub capacity: f32,
    /// **THE ONE DIAL THAT DECIDES RENEWABLE FROM FINITE**, and it belongs to the terrain. The
    /// growth coefficient of the shared logistic curve (`fauna::logistic_regrowth`'s `r`).
    ///
    /// [`NEVER_RENEWS`] on every rock body; small and **positive** on a loose-stone scatter, which
    /// is what makes knapping flint available nearly anywhere and unable to run the map dry.
    /// Validated finite and `>= NEVER_RENEWS` — a negative rate would be a deposit that erodes
    /// itself, which nothing in the design asks for and which would make a wood unrecoverable by
    /// arithmetic nobody chose.
    pub regrowth_rate: f32,
    /// **WHAT THIS GROUND'S MATERIAL IS LIKE** — a reading per axis, on the axes the material
    /// declares in `materials.json` and no others
    /// ([`ExtractionConfig::validate_against_materials`]).
    ///
    /// A `BTreeMap` so the vector handed to `LocalStore::deposit_material` — and therefore any batch
    /// key struck from it — has a stable order.
    pub characteristics: BTreeMap<String, f32>,
}

/// **ONE MATERIAL'S DEPOSITS** — which branch works them, and what each terrain holds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositDef {
    /// **THE LADDER THAT WORKS THIS DEPOSIT** — [`RungBranch::Forestry`] for wood,
    /// [`RungBranch::Extraction`] for stone and, later, for every metal.
    ///
    /// It is stated per **material** rather than derived, because that is the seam the minerals arc
    /// rides: copper is a new deposit row naming `extraction`, not a new branch. Validated to be one
    /// of the two extraction branches — a deposit worked by the plant or animal ladder is a
    /// food-web source, which has its own config.
    pub branch: RungBranch,
    /// **HOW MUCH OF THIS DEPOSIT ONE KEEPER LOOKS AFTER** — the divisor that turns a tile's own
    /// capacity into the **keeper-loads** both branches' rungs quote their `upkeep.work_per_turn`
    /// per (`extraction::deposit_keeper_loads`, `UpkeepScale::SourceLoad`).
    ///
    /// **It is the per-material twin of `fauna_config`'s `animals_per_herder`** and of
    /// `labor_config`'s `cultivation.capacity_per_tender`: *the web owns the ratio, the rung owns
    /// the rate*. It lives here rather than on the ladder because 600 units of wood and 600 units of
    /// stone are not the same size of job — one global divisor would make the two branches' bills
    /// incomparable for a reason that is about units and not about workings.
    ///
    /// Validated finite and `> 0`: a zero divides by zero and a negative one inverts the load, and
    /// both read as live dials.
    pub capacity_per_keeper: f32,
    /// **WHAT EACH TERRAIN HOLDS.** A terrain with no row here has **no deposit** of this material.
    pub by_terrain: BTreeMap<TerrainType, DepositTerrain>,
}

impl DepositDef {
    /// **WHAT THIS TILE'S TERRAIN HOLDS**, or `None` where the table carries no row for it — the one
    /// lookup [`crate::extraction`]'s three per-tile seams share.
    pub fn terrain(&self, terrain: TerrainType) -> Option<&DepositTerrain> {
        self.by_terrain.get(&terrain)
    }
}

/// The deposits table.
///
/// **The root is deliberately open** — `extraction.json` carries its rationale in `_comment_*` keys,
/// exactly as `materials.json` and `equipment.json` do. [`DepositDef`] and [`DepositTerrain`] are
/// *closed*, which is where the protection is actually needed: a mistyped `regowth_rate` there would
/// silently make a wood finite.
#[derive(Debug, Clone, PartialEq, Resource, Deserialize)]
pub struct ExtractionConfig {
    /// **WHAT A DEPOSIT REGROWS FROM WHEN IT HAS BEEN TAKEN TO NOTHING**, as a fraction of capacity.
    ///
    /// ⛔ **IT IS NOT A LIFT ON THE STOCK**, which is where the plant web puts its
    /// (`forage.reseed_floor_fraction`): a `max(stock, fraction × capacity)` would raise a **quarry**
    /// off zero and break the one invariant this whole file rests on. The seed is evaluated *inside*
    /// the growth term instead — see [`crate::extraction::deposit_regrowth`] — so it is multiplied
    /// by the deposit's own rate, and a rate of [`NEVER_RENEWS`] seeds exactly nothing.
    ///
    /// Validated finite and in `0.0..1.0`: at or above `1.0` the seeded reading would sit at or past
    /// capacity, where the logistic term is zero or negative, and a cleared wood would never come
    /// back at all.
    pub seed_fraction: f32,
    /// Every deposit, by material id. A `BTreeMap` so iteration — and therefore any published
    /// catalogue, and the order two deposits on one tile are worked in — is stable.
    pub deposits: BTreeMap<String, DepositDef>,
}

/// **A SEED THAT IS THE WHOLE DEPOSIT** — [`ExtractionConfig::seed_fraction`]'s exclusive ceiling.
const SEED_IS_THE_WHOLE_DEPOSIT: f32 = 1.0;
/// **NO SEED AT ALL** — the fraction's inclusive floor, and a legal statement: a deposit that is
/// gone is gone.
const NO_SEED: f32 = 0.0;

impl ExtractionConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_EXTRACTION_CONFIG)
                .expect("builtin extraction config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, ExtractionConfigError> {
        let config: ExtractionConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, ExtractionConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ExtractionConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        ExtractionConfig::from_json_str(&contents)
    }

    /// The deposit for this material, or `None` for a material nothing digs up.
    pub fn deposit(&self, material: &str) -> Option<&DepositDef> {
        self.deposits.get(material)
    }

    /// Every deposit, in material-id order — the stable iteration the seeding sweep rides.
    pub fn deposits(&self) -> impl Iterator<Item = (&str, &DepositDef)> {
        self.deposits.iter().map(|(id, def)| (id.as_str(), def))
    }

    /// **EVERY MATERIAL THIS BRANCH WORKS**, in id order. `forestry` answers `wood` and `extraction`
    /// answers `stone` today; the minerals arc adds rows here and nothing else.
    pub fn materials_on(&self, branch: RungBranch) -> impl Iterator<Item = (&str, &DepositDef)> {
        self.deposits().filter(move |(_, def)| def.branch == branch)
    }

    fn validate(&self) -> Result<(), ExtractionConfigError> {
        if !self.seed_fraction.is_finite()
            || !(NO_SEED..SEED_IS_THE_WHOLE_DEPOSIT).contains(&self.seed_fraction)
        {
            return Err(ExtractionConfigError::Invalid {
                field: "seed_fraction".to_string(),
                constraint: format!(
                    "be finite and within {NO_SEED}..{SEED_IS_THE_WHOLE_DEPOSIT} — at or above \
                     {SEED_IS_THE_WHOLE_DEPOSIT} the seeded reading sits at or past capacity, where \
                     the logistic term is zero or negative, and a cleared wood never comes back"
                ),
                value: self.seed_fraction.to_string(),
            });
        }
        if self.deposits.is_empty() {
            return Err(ExtractionConfigError::Invalid {
                field: "deposits".to_string(),
                constraint: "carry at least one deposit — a table with none is a producer for \
                             nothing, which is the gap this file exists to close"
                    .to_string(),
                value: "{}".to_string(),
            });
        }
        for (material, def) in self.deposits() {
            if !matches!(def.branch, RungBranch::Forestry | RungBranch::Extraction) {
                return Err(ExtractionConfigError::Invalid {
                    field: format!("deposits.{material}.branch"),
                    constraint: "be worked by one of the two extraction ladders (`forestry` or \
                                 `extraction`) — a deposit on a food web's branch would be a source \
                                 that pays no food standing on a ladder whose every rung is about \
                                 food"
                        .to_string(),
                    value: def.branch.as_str().to_string(),
                });
            }
            if !def.capacity_per_keeper.is_finite() || def.capacity_per_keeper <= NO_DEPOSIT {
                return Err(ExtractionConfigError::Invalid {
                    field: format!("deposits.{material}.capacity_per_keeper"),
                    constraint: format!(
                        "give a finite ratio above {NO_DEPOSIT} — it is the divisor that turns a \
                         tile's capacity into keeper-loads, so a zero divides by zero and a \
                         negative one inverts the load"
                    ),
                    value: def.capacity_per_keeper.to_string(),
                });
            }
            if def.by_terrain.is_empty() {
                return Err(ExtractionConfigError::Invalid {
                    field: format!("deposits.{material}.by_terrain"),
                    constraint: "name at least one terrain that holds it — a deposit no ground \
                                 carries is a material with no producer, which is the gap"
                        .to_string(),
                    value: "{}".to_string(),
                });
            }
            for (terrain, ground) in &def.by_terrain {
                let where_ = format!("deposits.{material}.by_terrain.{terrain:?}");
                if !ground.capacity.is_finite() || ground.capacity <= NO_DEPOSIT {
                    return Err(ExtractionConfigError::Invalid {
                        field: format!("{where_}.capacity"),
                        constraint: format!(
                            "hold a finite amount above {NO_DEPOSIT} — a row at {NO_DEPOSIT} says \
                             'no deposit' while reading as a tuning value, and the table already \
                             says that by carrying no row"
                        ),
                        value: ground.capacity.to_string(),
                    });
                }
                if !ground.regrowth_rate.is_finite() || ground.regrowth_rate < NEVER_RENEWS {
                    return Err(ExtractionConfigError::Invalid {
                        field: format!("{where_}.regrowth_rate"),
                        constraint: format!(
                            "be finite and at least {NEVER_RENEWS} — a negative rate is a deposit \
                             that erodes itself, and {NEVER_RENEWS} is already how this table says \
                             'this one never renews'"
                        ),
                        value: ground.regrowth_rate.to_string(),
                    });
                }
                for (axis, reading) in &ground.characteristics {
                    if !reading.is_finite() || !READING_RANGE.contains(reading) {
                        return Err(ExtractionConfigError::Invalid {
                            field: format!("{where_}.characteristics.{axis}"),
                            constraint: format!(
                                "be a finite reading within {:?}..={:?} — a characteristic is a \
                                 position on an axis, not a quantity, so it has both ends",
                                READING_RANGE.start(),
                                READING_RANGE.end()
                            ),
                            value: reading.to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// **THE DEPOSITS RECONCILED AGAINST THE MATERIALS TABLE** — the same `UnknownItem` debt every
    /// other roster pays (`docs/plan_crafting_and_materials.md` §2), and the reason it is a separate
    /// call: this is checked where both tables are in scope, at boot, rather than at either file's
    /// own load.
    ///
    /// Two failures, both silent without it:
    /// - a deposit of `wodo` would parse, validate, and pay a material nothing can hold;
    /// - a rating on an axis the material does not declare — or a **missing** rating on one it does
    ///   — would reach `LocalStore::deposit_material`, which merges *every axis either side names*,
    ///   and quietly invent or drop an axis on the band's pile.
    ///
    /// **The axis set must match EXACTLY**, both ways. `materials.json`'s own rule is that a
    /// material must be rated to exist, and a partly-rated arrival is a batch whose merged reading
    /// on the missing axis is an unweighted zero.
    pub fn validate_against_materials(
        &self,
        materials: &MaterialsConfig,
    ) -> Result<(), ExtractionConfigError> {
        for (material, def) in self.deposits() {
            let Some(known) = materials.material(material) else {
                return Err(ExtractionConfigError::UnknownMaterial {
                    material: material.to_string(),
                    available: materials
                        .materials()
                        .map(|(id, _)| id)
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            };
            let declared: Vec<&str> = known.characteristics.iter().map(String::as_str).collect();
            for (terrain, ground) in &def.by_terrain {
                let where_ = format!("deposits.{material}.by_terrain.{terrain:?}.characteristics");
                for axis in ground.characteristics.keys() {
                    if !declared.contains(&axis.as_str()) {
                        return Err(ExtractionConfigError::AxisMismatch {
                            field: where_.clone(),
                            material: material.to_string(),
                            axis: axis.clone(),
                            reason: format!(
                                "'{material}' declares {declared:?} and nothing else — a reading on \
                                 an axis the material does not carry reaches the band's store and \
                                 invents one"
                            ),
                        });
                    }
                }
                for axis in &declared {
                    if !ground.characteristics.contains_key(*axis) {
                        return Err(ExtractionConfigError::AxisMismatch {
                            field: where_.clone(),
                            material: material.to_string(),
                            axis: (*axis).to_string(),
                            reason: format!(
                                "'{material}' declares {declared:?} and every one must be rated — a \
                                 partly-rated arrival merges into the band's pile as an unweighted \
                                 zero on the axis it left out"
                            ),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// Why a deposits table cannot be used.
#[derive(Debug, Error)]
pub enum ExtractionConfigError {
    #[error("failed to read extraction config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse extraction config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid extraction config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
    #[error(
        "invalid extraction config: a deposit yields '{material}', which is not a material - the \
         table carries {available}"
    )]
    UnknownMaterial { material: String, available: String },
    #[error("invalid extraction config: `{field}` rates '{material}' on '{axis}', but {reason}")]
    AxisMismatch {
        field: String,
        material: String,
        axis: String,
        reason: String,
    },
}

impl ConfigLoadError for ExtractionConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is there
    /// and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the deposits table.
#[derive(Resource, Debug, Clone)]
pub struct ExtractionConfigHandle(pub Arc<ExtractionConfig>);

impl ExtractionConfigHandle {
    pub fn new(config: Arc<ExtractionConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<ExtractionConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<ExtractionConfig>) {
        self.0 = config;
    }
}

impl Default for ExtractionConfigHandle {
    fn default() -> Self {
        Self(ExtractionConfig::builtin())
    }
}

/// Metadata about the deposits configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct ExtractionConfigMetadata {
    path: Option<PathBuf>,
}

impl ExtractionConfigMetadata {
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

/// Load the deposits table from the environment (`EXTRACTION_CONFIG_PATH`) or the default data path.
/// The file is **validated** before it can reach the sim, and a broken invariant is as fatal as a
/// parse error — see [`crate::config_load::resolve_config`].
pub fn load_extraction_config_from_env() -> (Arc<ExtractionConfig>, ExtractionConfigMetadata) {
    let (config, source) = load_config_from_env(
        "EXTRACTION_CONFIG_PATH",
        "extraction_config",
        "src/data/extraction.json",
        ExtractionConfig::builtin,
        ExtractionConfig::from_file,
    );
    (config, ExtractionConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two shipped deposits — the materials this arc gives a producer.
    const WOOD: &str = "wood";
    const STONE: &str = "stone";

    fn builtin() -> ExtractionConfig {
        ExtractionConfig::from_json_str(BUILTIN_EXTRACTION_CONFIG).expect("builtin parses")
    }

    /// The shipped table with one thing changed, so each rejection test states exactly what it
    /// broke — `materials_config::tests::mutated`'s idiom.
    fn mutated(mutate: impl FnOnce(&mut serde_json::Value)) -> ExtractionConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_EXTRACTION_CONFIG).expect("builtin parses as json");
        mutate(&mut json);
        ExtractionConfig::from_json_str(&json.to_string())
            .expect_err("the mutated table must be rejected")
    }

    /// ⛔ **THE ONE IDEA, ASSERTED ON THE SHIPPED TABLE**: rock's rate is zero and wood's is not.
    ///
    /// It sweeps rather than naming rows, because the claim is about the *table* — a rock body that
    /// picked up a positive rate would be a quarry that renews, and a wood row that lost its rate
    /// would be a forest that is worked out.
    #[test]
    fn every_wood_row_renews_and_the_rock_bodies_never_do() {
        let config = builtin();
        let wood = config
            .deposit(WOOD)
            .expect("the shipped table carries wood");
        assert_eq!(wood.branch, RungBranch::Forestry);
        for (terrain, ground) in &wood.by_terrain {
            assert!(
                ground.regrowth_rate > NEVER_RENEWS,
                "{terrain:?} wood must renew — a forest that is worked out is not a forest"
            );
        }
        let stone = config
            .deposit(STONE)
            .expect("the shipped table carries stone");
        assert_eq!(stone.branch, RungBranch::Extraction);
        // **The two populations in one table.** Every row at or above the quarry threshold is a rock
        // body and never renews; the scatters below it all do. The threshold itself is the ladder's
        // (`extraction:quarry`'s `min_deposit_capacity`) and is asserted against this table in
        // `intensification.rs`; here the claim is only that the split is clean.
        let (finite, renewing): (Vec<_>, Vec<_>) = stone
            .by_terrain
            .iter()
            .partition(|(_, ground)| ground.regrowth_rate == NEVER_RENEWS);
        assert!(
            !finite.is_empty() && !renewing.is_empty(),
            "both populations must be present, or the table proves nothing about the model"
        );
        let smallest_body = finite
            .iter()
            .map(|(_, ground)| ground.capacity)
            .fold(f32::INFINITY, f32::min);
        let largest_scatter = renewing
            .iter()
            .map(|(_, ground)| ground.capacity)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            smallest_body > largest_scatter,
            "the rock bodies and the loose-stone scatters must not overlap in size, or \
             'you cannot quarry just anywhere' stops being a capacity reading"
        );
    }

    /// **The shipped table reconciles with the shipped materials table** — the boot check, run here
    /// so a bad edit fails a unit test rather than a server start.
    #[test]
    fn the_shipped_deposits_reconcile_with_the_materials_table() {
        let materials = MaterialsConfig::builtin();
        builtin()
            .validate_against_materials(&materials)
            .expect("the shipped deposits must rate their materials on the axes those declare");
    }

    /// **A stray key inside a deposit row is a parse error**, not a silently ignored dial —
    /// `deny_unknown_fields` on the closed records.
    #[test]
    fn an_unknown_key_on_a_terrain_row_is_refused() {
        let err = mutated(|json| {
            json["deposits"]["wood"]["by_terrain"]["MixedWoodland"]["is_finite"] =
                serde_json::json!(true);
        });
        assert!(
            matches!(err, ExtractionConfigError::Parse(_)),
            "a mistyped or invented dial must fail the parse, got {err}"
        );
    }

    /// **A rating on an axis the material does not declare is refused** — `materials.json` gives
    /// wood `hardness`/`pliancy`, so `workability` there is an axis that would reach the band's
    /// store and invent itself.
    #[test]
    fn a_characteristic_the_material_does_not_declare_is_refused() {
        let mut config = builtin();
        config
            .deposits
            .get_mut(WOOD)
            .expect("wood ships")
            .by_terrain
            .get_mut(&TerrainType::MixedWoodland)
            .expect("mixed woodland holds wood")
            .characteristics
            .insert("workability".to_string(), 0.5);
        let err = config
            .validate_against_materials(&MaterialsConfig::builtin())
            .expect_err("an undeclared axis must be rejected");
        assert!(
            matches!(err, ExtractionConfigError::AxisMismatch { ref axis, .. } if axis == "workability"),
            "got {err}"
        );
    }

    /// **And a MISSING one is refused too**, which is the half that fails silently: an arrival that
    /// rates only `hardness` merges into the band's pile as an unweighted zero on `pliancy`.
    #[test]
    fn a_characteristic_the_material_declares_may_not_be_left_out() {
        let mut config = builtin();
        config
            .deposits
            .get_mut(WOOD)
            .expect("wood ships")
            .by_terrain
            .get_mut(&TerrainType::MixedWoodland)
            .expect("mixed woodland holds wood")
            .characteristics
            .remove("pliancy");
        let err = config
            .validate_against_materials(&MaterialsConfig::builtin())
            .expect_err("an unrated declared axis must be rejected");
        assert!(
            matches!(err, ExtractionConfigError::AxisMismatch { ref axis, .. } if axis == "pliancy"),
            "got {err}"
        );
    }

    /// **A deposit of a material that does not exist is refused** — the `UnknownItem` debt.
    #[test]
    fn a_deposit_of_no_known_material_is_refused() {
        let mut config = builtin();
        let wood = config.deposits.remove(WOOD).expect("wood ships");
        config.deposits.insert("wodo".to_string(), wood);
        let err = config
            .validate_against_materials(&MaterialsConfig::builtin())
            .expect_err("an unknown material must be rejected");
        assert!(
            matches!(err, ExtractionConfigError::UnknownMaterial { ref material, .. } if material == "wodo"),
            "got {err}"
        );
    }

    /// **A capacity of zero is refused rather than read as absence.** The table already says *"no
    /// deposit"* by carrying no row, so a `0.0` is a dial that reads live and behaves as nothing.
    #[test]
    fn a_zero_capacity_row_is_refused_because_absence_is_the_answer() {
        let err = mutated(|json| {
            json["deposits"]["stone"]["by_terrain"]["RollingHills"]["capacity"] =
                serde_json::json!(0.0);
        });
        assert!(
            matches!(err, ExtractionConfigError::Invalid { ref field, .. } if field.ends_with("capacity")),
            "got {err}"
        );
    }

    /// **A negative regrowth rate is refused** — a deposit that erodes itself is not a shape the
    /// model has, and zero is already how the table says *"never renews"*.
    #[test]
    fn a_negative_regrowth_rate_is_refused() {
        let err = mutated(|json| {
            json["deposits"]["wood"]["by_terrain"]["MixedWoodland"]["regrowth_rate"] =
                serde_json::json!(-0.01);
        });
        assert!(
            matches!(err, ExtractionConfigError::Invalid { ref field, .. } if field.ends_with("regrowth_rate")),
            "got {err}"
        );
    }

    /// **A seed fraction at or above the whole deposit is refused** — the seeded reading would sit
    /// at capacity, where the logistic term is zero, and a cleared wood would never come back.
    #[test]
    fn a_seed_fraction_of_the_whole_deposit_is_refused() {
        let err = mutated(|json| json["seed_fraction"] = serde_json::json!(1.0));
        assert!(
            matches!(err, ExtractionConfigError::Invalid { ref field, .. } if field == "seed_fraction"),
            "got {err}"
        );
    }
}
