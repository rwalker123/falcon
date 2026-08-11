//! **The materials table** (`data/materials.json`) — what a craftable thing is *made of*.
//!
//! Design: `docs/plan_crafting_and_materials.md` §1. Three rules this module exists to keep, all
//! three load-bearing:
//!
//! 1. **A material is GENERIC.** There is no `deer_hide` — there is [`hide`](MaterialsConfig). A
//!    *source* (an animal, a plant, later a deposit) states the characteristics of what **it**
//!    yields, through [`MaterialYieldDef`], which has the **same shape on every config**. Nothing
//!    here is fauna- or flora-shaped.
//! 2. **A characteristic VECTOR, never a quality scalar.** A material declares its own axes and a
//!    reading is stated per axis, so mammoth hide can be `toughness excellent · suppleness poor`
//!    and a hare pelt the reverse. A single quality number would rank them, name a winner, and
//!    delete the decision.
//! 3. **The band rates the AXIS, and it is the MERGE RULE.** Two arrivals whose per-axis bands match
//!    become one batch ([`crate::components::LocalStore`]), so a band hunting deer for two hundred
//!    turns holds one pile of hide rather than two hundred — and the batch keeps the **exact**
//!    amount-weighted reading, so two `good` hides are still not interchangeable.
//!
//! **Deriving a band needs the material's axis list, so it lives here** ([`MaterialsConfig::
//! band_key`]) and not on the store. `LocalStore` stores; it does not interpret.
//!
//! Loader mirrors [`crate::equipment_config`]: baked-in builtin + `MATERIALS_CONFIG_PATH` override +
//! [`MaterialsConfig::validate`] inside `from_json_str`, so **every** load path is validated and a
//! present-but-broken file is a boot panic rather than a silent fallback
//! (`.claude/rules/core_sim/config-loading.md`).

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    components::LocalStore,
    config_load::{load_config_from_env, ConfigLoadError},
    scalar::{scalar_from_f32, scalar_zero, Scalar},
};

pub const BUILTIN_MATERIALS_CONFIG: &str = include_str!("data/materials.json");

/// **The closed interval every characteristic reading and every band seam lives on.** A reading is a
/// position on an axis, not a quantity, so it has both ends — and the bands partition exactly this
/// range, which is what makes [`MaterialsConfig::band_index`] total.
pub const READING_MIN: f32 = 0.0;
/// See [`READING_MIN`].
pub const READING_MAX: f32 = 1.0;

/// **The seam the first band must start at.** Every reading has to fall in *some* band or the merge
/// key would be undefined for it, and the lowest band therefore has to open at the bottom of the
/// range rather than at whatever the author happened to type.
const FIRST_BAND_FROM: f32 = READING_MIN;

/// **One rung of the shared rating vocabulary** — `poor · fair · good · excellent`. `from` is the
/// reading at which this band opens; a band runs up to the next band's `from`, and the last runs to
/// [`READING_MAX`].
///
/// **The band rates the AXIS, not the material.** "Excellent toughness" says the toughness is
/// excellent; it makes no claim about the hide. That is what lets ordinary quality words coexist
/// with there being no best hide.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacteristicBand {
    /// Player-facing label. The sim never branches on it.
    pub name: String,
    /// The reading at which this band opens. The first must be [`FIRST_BAND_FROM`] and the list must
    /// be strictly ascending, so every reading selects exactly one band.
    pub from: f32,
}

/// **What working this material BARE-HANDED buys**, when it can be worked bare-handed at all.
///
/// Absent ⇒ the material cannot be worked without its tool, which is metal: the rate is `0` and that
/// zero is what refuses the craft, with no *"you cannot craft that"* branch anywhere. It is **one
/// property of the material**, never a prerequisite repeated on every recipe — and the ceiling
/// belongs here rather than to the absent tool, because a tool that is not there cannot declare
/// anything.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandWorking {
    /// Bench progress multiplier with no tool — the value the rate **takes**, never a delta.
    pub rate: f32,
    /// The best reading a bare-handed craft can realize: the output reads
    /// `min(material reading, ceiling)`. Excellent flax with no loom still makes a fair basket.
    pub quality_ceiling: f32,
}

/// **A named preset over a material's own axes** — `copper`, `bronze`, `iron`.
///
/// Varieties are **NAMING, not materials**: the material stays one generic thing with one craft and
/// one tool axis, and a batch simply *displays* as its nearest variety
/// ([`MaterialsConfig::nearest_variety`]). Without them an alloy recipe could not name its
/// ingredients — *"9 parts metal at hardness .20–.35"* is unreadable to the author and worse to the
/// player.
///
/// **Parsed, validated, and none ships.** Wood, stone, clay and metal have no producer yet, and an
/// unreachable row is visible dead content; the mechanism is exercised by a test fixture instead.
pub type VarietyReadings = BTreeMap<String, f32>;

/// One material: its craft track, its characteristic axes, and whether it yields to bare hands.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialDef {
    /// The craft track that works this material — one craft per material (hide → tanning). The
    /// knowledge half of the ladder hangs off this id.
    ///
    /// **Absent ⇒ nothing works it** — the same statement [`Self::hand_working`] makes one level
    /// down, one rung further out: `hand_working: None` says *"not by hand"*, `craft: None` says
    /// *"not by anything, yet"*. A material with no craft has a **producer** but no bench: the yield
    /// edge credits it, the store holds it, the catalogue publishes it, and no recipe can name it —
    /// because [`crate::crafting::crafts_declared_by`] does not list a craft that is not there, and
    /// a recipe's `craft` is validated against exactly that list.
    ///
    /// It is deliberately **not** a licence to ship an unreachable material. The three that use it
    /// (`tobacco`, `tea`, `grape`) are *uncrafted*, not *unreachable*: a plant grows them and a band
    /// banks them today; what does not exist yet is the craft that would turn them into something.
    #[serde(default)]
    pub craft: Option<String>,
    /// **The axes this material is rated on**, in the order a [`BandKey`] reads them. Order is part
    /// of the contract: it is what makes a key comparable between two batches.
    pub characteristics: Vec<String>,
    /// See [`HandWorking`] — absent means *no bare-handed rate at all*.
    #[serde(default)]
    pub hand_working: Option<HandWorking>,
    /// Named presets over [`Self::characteristics`]. **Absent is the ordinary case**, not a missing
    /// tuning value: most materials have no varieties, and an empty map is exactly the statement
    /// *"this material is not named further"*.
    #[serde(default)]
    pub varieties: BTreeMap<String, VarietyReadings>,
}

impl MaterialDef {
    /// Whether this material can be worked with no tool at all. Derived from [`Self::hand_working`],
    /// never stored beside it — a stored flag would be a second statement of one fact.
    pub fn is_hand_workable(&self) -> bool {
        self.hand_working.is_some()
    }

    /// The bare-handed bench rate: `0` for a material with no [`HandWorking`], which is how metal
    /// refuses itself with no branch.
    pub fn hand_working_rate(&self) -> f32 {
        self.hand_working.map_or(0.0, |hand| hand.rate)
    }

    fn declares(&self, axis: &str) -> bool {
        self.characteristics.iter().any(|known| known == axis)
    }
}

/// **A batch's merge key: the per-axis band INDEX, in the material's declared axis order.**
///
/// Indices rather than names so the key cannot drift from the band table it was derived against, and
/// a `Vec` rather than a map because the axis order is already the material's contract. `Ord` so the
/// batch map serializes and iterates in a stable order — the same reason `BandEquipment` is a
/// `BTreeMap`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BandKey(pub Vec<usize>);

/// **What one unit of a source's biomass yields, in ONE material** — the yield edge, and it is the
/// *same type* on `fauna_config.json` and `flora_config.json` (and on a deposit, when there is one).
///
/// The source states the **exact** characteristics of what it gives. A mammoth's hide is tough and
/// stiff, a hare's is supple and weak, and neither is "better" — which is only expressible because
/// the reading is a vector and the source authors it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialYieldDef {
    /// The material id, resolved against [`MaterialsConfig::materials`] at load.
    pub material: String,
    /// Units of the material per unit of biomass **carried home**.
    pub per_biomass: f32,
    /// This source's exact reading per axis. Validated to name **exactly** the axes the material
    /// declares — a missing axis would be a silently defaulted, silently wrong reading.
    pub characteristics: BTreeMap<String, f32>,
}

/// The materials table plus the shared rating vocabulary the whole model speaks in.
///
/// **The root is deliberately open** — `materials.json` carries its rationale in `_comment*` keys,
/// exactly as `equipment.json` and `fauna_config.json` do. [`MaterialDef`] is *closed*, which is
/// where the protection is actually needed: a mistyped `hand_workng` there would silently make a
/// material unworkable, while a stray key at the root can only be prose.
#[derive(Debug, Clone, PartialEq, Resource, Deserialize)]
pub struct MaterialsConfig {
    /// The rating rungs, ascending. See [`CharacteristicBand`].
    pub characteristic_bands: Vec<CharacteristicBand>,
    /// Every material, by id. A `BTreeMap` so iteration — and therefore any published catalogue —
    /// has a stable order.
    pub materials: BTreeMap<String, MaterialDef>,
}

impl MaterialsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_MATERIALS_CONFIG)
                .expect("builtin materials config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, MaterialsConfigError> {
        let config: MaterialsConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, MaterialsConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| MaterialsConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        MaterialsConfig::from_json_str(&contents)
    }

    /// The material with this id, or `None`.
    pub fn material(&self, id: &str) -> Option<&MaterialDef> {
        self.materials.get(id)
    }

    /// Every material, in id order — the stable iteration a catalogue rides.
    pub fn materials(&self) -> impl Iterator<Item = (&str, &MaterialDef)> {
        self.materials.iter().map(|(id, def)| (id.as_str(), def))
    }

    /// **Which band a reading falls in**, as an index into [`Self::characteristic_bands`].
    ///
    /// Total by construction: `validate` guarantees the first band opens at [`FIRST_BAND_FROM`] and
    /// the seams ascend, so the last band whose `from` the reading has reached is the answer. A
    /// reading below the range clamps into the lowest band rather than having no key at all.
    pub fn band_index(&self, reading: f32) -> usize {
        self.characteristic_bands
            .iter()
            .rposition(|band| reading >= band.from)
            .unwrap_or(0)
    }

    /// The band's player-facing label, or `None` for an index off the end (which
    /// [`Self::band_index`] cannot produce).
    pub fn band_name(&self, index: usize) -> Option<&str> {
        self.characteristic_bands
            .get(index)
            .map(|band| band.name.as_str())
    }

    /// **Which rung a band NAME is**, or `None` for a name this table does not carry — the inverse of
    /// [`Self::band_name`].
    ///
    /// It is what resolves a config keyed by band name: `recipes.json`'s grades are keyed by these
    /// words, so ordering them, inheriting between them and rejecting one that is not a rung all go
    /// through here rather than through a second copy of the vocabulary
    /// (`.claude/rules/core_sim/crafting.md` → "A grade key IS a band name").
    pub fn band_index_of(&self, name: &str) -> Option<usize> {
        self.characteristic_bands
            .iter()
            .position(|band| band.name == name)
    }

    /// **THE merge key** — the per-axis band of `readings`, in `material`'s declared axis order.
    ///
    /// `None` for a material the table does not carry; an axis missing from `readings` reads at
    /// [`READING_MIN`], which cannot occur after `validate_yield` and would place the batch in the
    /// lowest band rather than inventing a key of a different length.
    pub fn band_key(&self, material: &str, readings: &BTreeMap<String, f32>) -> Option<BandKey> {
        let def = self.material(material)?;
        Some(BandKey(
            def.characteristics
                .iter()
                .map(|axis| self.band_index(readings.get(axis).copied().unwrap_or(READING_MIN)))
                .collect(),
        ))
    }

    /// **How a batch DISPLAYS when its material declares varieties** — the nearest preset by
    /// Euclidean distance over the declared axes. `None` when the material names none, which is
    /// every shipped material.
    ///
    /// Ties resolve to the earliest variety in id order, because the fold keeps only a strictly
    /// nearer one — a tie broken by iteration order is how a display flickers between two names.
    pub fn nearest_variety(
        &self,
        material: &str,
        readings: &BTreeMap<String, f32>,
    ) -> Option<&str> {
        let def = self.material(material)?;
        def.varieties
            .iter()
            .map(|(name, preset)| {
                let distance_sq: f32 = def
                    .characteristics
                    .iter()
                    .map(|axis| {
                        let reading = readings.get(axis).copied().unwrap_or(READING_MIN);
                        let target = preset.get(axis).copied().unwrap_or(READING_MIN);
                        (reading - target).powi(2)
                    })
                    .sum();
                (name.as_str(), distance_sq)
            })
            .fold(
                None,
                |best: Option<(&str, f32)>, (name, distance_sq)| match best {
                    Some((_, best_distance)) if best_distance <= distance_sq => best,
                    _ => Some((name, distance_sq)),
                },
            )
            .map(|(name, _)| name)
    }

    /// **Validate a source's yield rows against this table** — the cross-config check every loader
    /// that carries a yield edge runs (`fauna_config`, `flora_config`, and a deposit's when one
    /// exists). `context` names the row for the error message (`species.deer.hunt_yield`).
    ///
    /// It is the `UnknownItem` debt again: a material id is a `String`, so this is the only thing
    /// between the file and a running sim.
    pub fn validate_yield(
        &self,
        context: &str,
        rows: &[MaterialYieldDef],
    ) -> Result<(), MaterialYieldError> {
        for row in rows {
            let Some(def) = self.material(&row.material) else {
                return Err(MaterialYieldError::UnknownMaterial {
                    context: context.to_string(),
                    material: row.material.clone(),
                    available: self.material_ids_for_message(),
                });
            };
            if !row.per_biomass.is_finite() || row.per_biomass <= 0.0 {
                return Err(MaterialYieldError::Invalid {
                    context: context.to_string(),
                    material: row.material.clone(),
                    constraint:
                        "be finite and greater than 0 — a row that yields nothing is a row \
                                 that should not be there"
                            .to_string(),
                    value: row.per_biomass.to_string(),
                });
            }
            // **EXACTLY the declared axes, both directions.** A missing axis would be a defaulted
            // and therefore silently wrong reading; an extra one is an author naming an axis this
            // material does not have, which would be read by nothing.
            for axis in &def.characteristics {
                if !row.characteristics.contains_key(axis) {
                    return Err(MaterialYieldError::AxisMismatch {
                        context: context.to_string(),
                        material: row.material.clone(),
                        axis: axis.clone(),
                        reason: "is declared by the material but missing from the row".to_string(),
                    });
                }
            }
            for (axis, reading) in &row.characteristics {
                if !def.declares(axis) {
                    return Err(MaterialYieldError::AxisMismatch {
                        context: context.to_string(),
                        material: row.material.clone(),
                        axis: axis.clone(),
                        reason: "is named by the row but not declared by the material".to_string(),
                    });
                }
                if !reading_in_range(*reading) {
                    return Err(MaterialYieldError::Invalid {
                        context: context.to_string(),
                        material: row.material.clone(),
                        constraint: format!(
                            "state `{axis}` finite and within {READING_MIN}..={READING_MAX}"
                        ),
                        value: reading.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Invariants a materials table must satisfy. Run inside [`Self::from_json_str`], so every load
    /// path is covered and a present-but-broken file is a boot panic.
    pub fn validate(&self) -> Result<(), MaterialsConfigError> {
        self.validate_bands()?;
        if self.materials.is_empty() {
            return Err(MaterialsConfigError::InvalidTable {
                reason: "the material table is empty - nothing could ever be crafted".to_string(),
            });
        }
        for (id, def) in &self.materials {
            // **An ABSENT craft is legal; a BLANK one is not.** `None` is the deliberate statement
            // *"nothing works this yet"* (see [`MaterialDef::craft`]); `""` is a typo that would
            // put an unnameable craft in `crafts_declared_by` and let a recipe match it.
            if def
                .craft
                .as_ref()
                .is_some_and(|craft| craft.trim().is_empty())
            {
                return Err(MaterialsConfigError::InvalidTable {
                    reason: format!(
                        "material '{id}' states a blank craft - omit the key to say nothing works it"
                    ),
                });
            }
            if def.characteristics.is_empty() {
                return Err(MaterialsConfigError::InvalidTable {
                    reason: format!(
                        "material '{id}' declares no characteristics - every batch of it would \
                         merge into one and no recipe could read it"
                    ),
                });
            }
            for (index, axis) in def.characteristics.iter().enumerate() {
                if def.characteristics[..index].contains(axis) {
                    return Err(MaterialsConfigError::InvalidTable {
                        reason: format!(
                            "material '{id}' declares the characteristic '{axis}' twice - the \
                             merge key would carry it twice and a recipe reading it would be \
                             ambiguous"
                        ),
                    });
                }
            }
            self.validate_hand_working(id, def)?;
            self.validate_varieties(id, def)?;
        }
        Ok(())
    }

    fn validate_bands(&self) -> Result<(), MaterialsConfigError> {
        let mut bands = self.characteristic_bands.iter().enumerate();
        let Some((_, first)) = bands.next() else {
            return Err(MaterialsConfigError::InvalidTable {
                reason:
                    "no characteristic bands - every reading would be unrateable and the merge \
                         key undefined"
                        .to_string(),
            });
        };
        if first.from != FIRST_BAND_FROM {
            return Err(MaterialsConfigError::Invalid {
                field: "characteristic_bands[0].from".to_string(),
                constraint: format!(
                    "be exactly {FIRST_BAND_FROM} - every reading must select a band"
                ),
                value: first.from.to_string(),
            });
        }
        let mut previous = first.from;
        for (index, band) in bands {
            if !reading_in_range(band.from) {
                return Err(MaterialsConfigError::Invalid {
                    field: format!("characteristic_bands[{index}].from"),
                    constraint: format!("be finite and within {READING_MIN}..={READING_MAX}"),
                    value: band.from.to_string(),
                });
            }
            if band.from <= previous {
                return Err(MaterialsConfigError::Invalid {
                    field: format!("characteristic_bands[{index}].from"),
                    constraint: format!(
                        "be strictly greater than the band below it ({previous}) - equal or \
                         descending seams make a band unreachable"
                    ),
                    value: band.from.to_string(),
                });
            }
            previous = band.from;
        }
        Ok(())
    }

    fn validate_hand_working(
        &self,
        id: &str,
        def: &MaterialDef,
    ) -> Result<(), MaterialsConfigError> {
        let Some(hand) = def.hand_working else {
            return Ok(());
        };
        if !hand.rate.is_finite() || hand.rate < 0.0 {
            return Err(MaterialsConfigError::Invalid {
                field: format!("materials.{id}.hand_working.rate"),
                constraint: "be finite and not negative".to_string(),
                value: hand.rate.to_string(),
            });
        }
        if !reading_in_range(hand.quality_ceiling) {
            return Err(MaterialsConfigError::Invalid {
                field: format!("materials.{id}.hand_working.quality_ceiling"),
                constraint: format!("be finite and within {READING_MIN}..={READING_MAX}"),
                value: hand.quality_ceiling.to_string(),
            });
        }
        Ok(())
    }

    fn validate_varieties(&self, id: &str, def: &MaterialDef) -> Result<(), MaterialsConfigError> {
        for (variety, readings) in &def.varieties {
            for axis in &def.characteristics {
                if !readings.contains_key(axis) {
                    return Err(MaterialsConfigError::InvalidTable {
                        reason: format!(
                            "material '{id}' variety '{variety}' omits the characteristic '{axis}' \
                             - a preset with a defaulted axis would name a point the author did \
                             not choose"
                        ),
                    });
                }
            }
            for (axis, reading) in readings {
                if !def.declares(axis) {
                    return Err(MaterialsConfigError::InvalidTable {
                        reason: format!(
                            "material '{id}' variety '{variety}' names the characteristic '{axis}', \
                             which the material does not declare"
                        ),
                    });
                }
                if !reading_in_range(*reading) {
                    return Err(MaterialsConfigError::Invalid {
                        field: format!("materials.{id}.varieties.{variety}.{axis}"),
                        constraint: format!("be finite and within {READING_MIN}..={READING_MAX}"),
                        value: reading.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn material_ids_for_message(&self) -> String {
        self.materials
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A reading is a position on the axis, so it has both ends. Named once so every check that bounds
/// one — a band seam, a variety preset, a source's stated yield — agrees about the range.
fn reading_in_range(value: f32) -> bool {
    value.is_finite() && (READING_MIN..=READING_MAX).contains(&value)
}

/// **How much of ONE material a source pays per turn** — the flat row every material readout in the
/// sim is a vector of: the crop picker's cash quote ([`crate::forage::commit_material_payoff`]), the
/// herd row's per-biomass rate, and what a resolved turn actually credited
/// ([`credit_material_yield`]'s return).
///
/// **It names the material and nothing about its quality**, deliberately. A material's *rating* is a
/// characteristic vector living on the batch the harvest creates ([`crate::components::MaterialBatch`]);
/// a readout row asks the flat question *"how much of what"*, and answering it with a rating too
/// would put the merge model in a preview.
///
/// **Never sum a vector of these into one number.** That is the retired trade-goods axis under a new
/// name (arc #527), and it collapses the distinction the whole materials model exists to keep.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPayoff {
    /// The `materials.json` id — `fibre`, `hide`, `tobacco`. Resolved client-side for display.
    pub material: String,
    /// Units of that material per turn.
    pub amount: f32,
}

/// **What `biomass` of a source yields, per material** — the projection twin of
/// [`credit_material_yield`], stating the same `biomass × per_biomass × output_multiplier`
/// expression without depositing anything.
///
/// **Every material QUOTE in the sim goes through here**: the crop picker's cash rows
/// ([`crate::forage::commit_material_payoff`]), the herd row's per-biomass and per-worker rates, and
/// a raid's projected haul. One expression, so a quote cannot drift from the credit it is quoting —
/// which is the whole reason a *resolved* row reports [`credit_material_yield`]'s own return instead
/// of calling this.
///
/// **Merged per material id, in id order.** A basket can name one material twice; those land in two
/// *batches* in the store (different readings), but a readout row says *"0.29 fibre"* and
/// [`LocalStore::material_total`] sums the same way, which is what makes a quote and a credit
/// comparable numbers.
///
/// **Empty in, empty out, and a non-positive row is dropped** — "no row", never a published zero.
pub fn material_yield_totals(
    rows: &[MaterialYieldDef],
    biomass: f32,
    output_multiplier: f32,
) -> Vec<MaterialPayoff> {
    let mut totals: BTreeMap<&str, f32> = BTreeMap::new();
    for row in rows {
        let amount = biomass * row.per_biomass * output_multiplier;
        if amount <= 0.0 {
            continue;
        }
        *totals.entry(row.material.as_str()).or_insert(0.0) += amount;
    }
    totals
        .into_iter()
        .map(|(material, amount)| MaterialPayoff {
            material: material.to_string(),
            amount,
        })
        .collect()
}

/// **Credit a take's material yield into a store** — the fourth account of the same harvest, on the
/// same seam the provisions are credited on.
///
/// `biomass` is what came **home** (never what was killed): you cannot tan a hide you left on the
/// range. The amount is a [`crate::scalar::Scalar`] and **accumulates** — a rate becomes a whole
/// unit by crossing a threshold in the store, never by rounding per turn.
///
/// A row naming a material the table does not carry is skipped rather than panicking; the loaders'
/// [`MaterialsConfig::validate_yield`] is what makes that unreachable.
///
/// **It RETURNS what it credited, merged per material id and in id order**, so a telemetry row can
/// report the deposit rather than recompute it — the discipline `SourceYield::fodder` already
/// carries ("the credited value, not a recomputation"). Recomputing it at the readout is how a row
/// starts disagreeing with the store it is describing: the skips above (a sub-quantum amount, an
/// unknown material) are invisible to any second derivation.
///
/// **Merged per material id, not per band key.** Two rows of one material with different readings
/// land in two *batches* — that is the merge model working — but a readout row says *"0.29 fibre"*,
/// and `LocalStore::material_total` sums the same way, which is what makes the two comparable.
pub fn credit_material_yield(
    store: &mut LocalStore,
    materials: &MaterialsConfig,
    rows: &[MaterialYieldDef],
    biomass: f32,
    output_multiplier: f32,
) -> Vec<MaterialPayoff> {
    let mut credited: BTreeMap<&str, Scalar> = BTreeMap::new();
    for row in rows {
        let amount = scalar_from_f32(biomass * row.per_biomass * output_multiplier);
        if amount <= scalar_zero() {
            continue;
        }
        let Some(band) = materials.band_key(&row.material, &row.characteristics) else {
            continue;
        };
        store.deposit_material(&row.material, band, amount, &row.characteristics);
        *credited
            .entry(row.material.as_str())
            .or_insert(scalar_zero()) += amount;
    }
    credited
        .into_iter()
        .map(|(material, amount)| MaterialPayoff {
            material: material.to_string(),
            // Reported in the store's own units — `Scalar` in, `f32` out, so a readout never
            // re-derives the fixed-point rounding the deposit already did.
            amount: amount.to_f32(),
        })
        .collect()
}

/// Why a materials table cannot be used.
#[derive(Debug, Error)]
pub enum MaterialsConfigError {
    #[error("failed to read materials config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse materials config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid materials config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
    #[error("invalid materials config: {reason}")]
    InvalidTable { reason: String },
}

impl ConfigLoadError for MaterialsConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is there
    /// and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Why a **source's** yield edge cannot be reconciled with the materials table. Its own type because
/// two different configs raise it and each wraps it into its own error enum.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MaterialYieldError {
    #[error(
        "{context} yields '{material}', which is not a material - the table carries {available}"
    )]
    UnknownMaterial {
        context: String,
        material: String,
        available: String,
    },
    #[error("{context} yields '{material}' but must {constraint}, got {value}")]
    Invalid {
        context: String,
        material: String,
        constraint: String,
        value: String,
    },
    #[error("{context} yields '{material}': the characteristic '{axis}' {reason}")]
    AxisMismatch {
        context: String,
        material: String,
        axis: String,
        reason: String,
    },
}

/// Handle for accessing the materials table.
#[derive(Resource, Debug, Clone)]
pub struct MaterialsConfigHandle(pub Arc<MaterialsConfig>);

impl MaterialsConfigHandle {
    pub fn new(config: Arc<MaterialsConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<MaterialsConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<MaterialsConfig>) {
        self.0 = config;
    }
}

impl Default for MaterialsConfigHandle {
    fn default() -> Self {
        Self(MaterialsConfig::builtin())
    }
}

/// Metadata about the materials configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct MaterialsConfigMetadata {
    path: Option<PathBuf>,
}

impl MaterialsConfigMetadata {
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

/// Load the materials table from the environment (`MATERIALS_CONFIG_PATH`) or the default data path.
/// The file is **validated** before it can reach the sim, and a broken invariant is as fatal as a
/// parse error — see [`crate::config_load::resolve_config`].
pub fn load_materials_config_from_env() -> (Arc<MaterialsConfig>, MaterialsConfigMetadata) {
    let (config, source) = load_config_from_env(
        "MATERIALS_CONFIG_PATH",
        "materials_config",
        "src/data/materials.json",
        MaterialsConfig::builtin,
        MaterialsConfig::from_file,
    );
    (config, MaterialsConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped material ids. Named here rather than inlined so the sim never spells one outside
    /// a fixture — a take resolves its rows off the source's config, never by naming a material.
    const HIDE: &str = "hide";
    const FIBRE: &str = "fibre";
    const BONE: &str = "bone";
    /// The three **uncrafted** luxury crops (arc #527): a producer exists, a craft does not.
    const GRAPE: &str = "grape";
    const TEA: &str = "tea";
    const TOBACCO: &str = "tobacco";

    fn builtin() -> MaterialsConfig {
        MaterialsConfig::from_json_str(BUILTIN_MATERIALS_CONFIG).expect("builtin parses")
    }

    fn readings(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
        pairs
            .iter()
            .map(|(axis, value)| ((*axis).to_string(), *value))
            .collect()
    }

    /// The shipped table with one field replaced, so each rejection test states exactly the one
    /// thing it broke.
    fn mutated(mutate: impl FnOnce(&mut serde_json::Value)) -> MaterialsConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_MATERIALS_CONFIG).expect("builtin parses as json");
        mutate(&mut json);
        MaterialsConfig::from_json_str(&json.to_string())
            .expect_err("the mutated table must be rejected")
    }

    /// **What ships, and the line between the two kinds of shipped material.**
    ///
    /// The three organics are **crafted** — each names a craft and yields to bare hands. The three
    /// luxury crops are **uncrafted**: `craft` and `hand_working` are both absent, which is the
    /// deliberate statement *nothing works this yet* (arc #527). Neither kind is *unreachable* —
    /// every one of the six has a producer on the shipped rosters — and the rule this pins is that a
    /// material with no craft is also a material with no bench, never one with a half-declared one.
    #[test]
    fn the_builtin_table_parses_and_validates() {
        let config = builtin();
        let ids: Vec<&str> = config.materials().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![BONE, FIBRE, GRAPE, HIDE, TEA, TOBACCO]);
        for (id, def) in config.materials() {
            let crafted = [BONE, FIBRE, HIDE].contains(&id);
            assert_eq!(
                def.craft.is_some(),
                crafted,
                "{id}: only the three organics name a craft"
            );
            assert_eq!(
                def.is_hand_workable(),
                crafted,
                "{id}: a material nothing works is not workable bare-handed either — the absent \
                 block is the refusal, with no branch anywhere"
            );
            assert!(
                def.varieties.is_empty(),
                "{id} must ship no varieties - an unreachable preset is visible dead content"
            );
        }
    }

    /// The four rungs, and the fact that makes the merge key total: every reading in range lands in
    /// exactly one of them.
    #[test]
    fn every_reading_lands_in_exactly_one_band() {
        let config = builtin();
        assert_eq!(config.band_name(config.band_index(0.00)), Some("poor"));
        assert_eq!(config.band_name(config.band_index(0.29)), Some("poor"));
        assert_eq!(config.band_name(config.band_index(0.30)), Some("fair"));
        assert_eq!(config.band_name(config.band_index(0.54)), Some("fair"));
        assert_eq!(config.band_name(config.band_index(0.55)), Some("good"));
        assert_eq!(config.band_name(config.band_index(0.79)), Some("good"));
        assert_eq!(config.band_name(config.band_index(0.80)), Some("excellent"));
        assert_eq!(config.band_name(config.band_index(1.00)), Some("excellent"));
    }

    /// **The merge key reads the material's axes in ITS declared order**, which is what makes two
    /// batches comparable at all. The liveness half is the second key: a key that ignored the
    /// readings would make these two equal.
    #[test]
    fn the_band_key_reads_the_declared_axes_in_order() {
        let config = builtin();
        let mammoth = config
            .band_key(
                HIDE,
                &readings(&[("toughness", 0.90), ("suppleness", 0.15)]),
            )
            .expect("hide is a material");
        let hare = config
            .band_key(
                HIDE,
                &readings(&[("toughness", 0.15), ("suppleness", 0.90)]),
            )
            .expect("hide is a material");
        assert_eq!(mammoth, BandKey(vec![3, 0]), "tough excellent, supple poor");
        assert_eq!(
            hare,
            BandKey(vec![0, 3]),
            "the reverse animal, the reverse key"
        );
        assert_ne!(mammoth, hare);
    }

    /// Varieties are exercised by a fixture, exactly as the design does for the bronze tier — the
    /// shipped table declares none.
    #[test]
    fn a_batch_displays_as_its_nearest_variety_when_the_material_declares_any() {
        let json = r#"{
            "characteristic_bands": [
                { "name": "poor", "from": 0.0 }, { "name": "good", "from": 0.5 }
            ],
            "materials": {
                "metal": {
                    "craft": "smithing",
                    "characteristics": ["hardness", "working_temp"],
                    "varieties": {
                        "bronze": { "hardness": 0.55, "working_temp": 0.30 },
                        "copper": { "hardness": 0.25, "working_temp": 0.20 },
                        "iron":   { "hardness": 0.75, "working_temp": 0.70 }
                    }
                }
            }
        }"#;
        let config = MaterialsConfig::from_json_str(json).expect("the fixture validates");
        assert!(
            !config.material("metal").expect("metal").is_hand_workable(),
            "an absent hand_working is what refuses metal to bare hands"
        );
        assert_eq!(
            config.nearest_variety(
                "metal",
                &readings(&[("hardness", 0.26), ("working_temp", 0.22)])
            ),
            Some("copper")
        );
        assert_eq!(
            config.nearest_variety(
                "metal",
                &readings(&[("hardness", 0.72), ("working_temp", 0.68)])
            ),
            Some("iron"),
            "a different reading must name a different variety, or the answer is a constant"
        );
        assert_eq!(
            builtin().nearest_variety(HIDE, &readings(&[("toughness", 0.5), ("suppleness", 0.5)])),
            None,
            "a material that declares no varieties has no name to display"
        );
    }

    #[test]
    fn validate_rejects_an_empty_material_table() {
        let err = mutated(|json| json["materials"] = serde_json::json!({}));
        assert!(
            matches!(&err, MaterialsConfigError::InvalidTable { reason } if reason.contains("empty")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_bands_that_do_not_start_at_the_bottom_of_the_range() {
        let err = mutated(|json| json["characteristic_bands"][0]["from"] = serde_json::json!(0.1));
        assert!(
            matches!(&err, MaterialsConfigError::Invalid { field, .. } if field == "characteristic_bands[0].from"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_bands_that_do_not_strictly_ascend() {
        let err = mutated(|json| json["characteristic_bands"][2]["from"] = serde_json::json!(0.30));
        assert!(
            matches!(&err, MaterialsConfigError::Invalid { field, .. } if field == "characteristic_bands[2].from"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_band_seam_outside_the_reading_range() {
        let err = mutated(|json| json["characteristic_bands"][3]["from"] = serde_json::json!(1.4));
        assert!(
            matches!(&err, MaterialsConfigError::Invalid { field, .. } if field == "characteristic_bands[3].from"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_material_with_no_characteristics() {
        let err =
            mutated(|json| json["materials"][HIDE]["characteristics"] = serde_json::json!([]));
        assert!(
            matches!(&err, MaterialsConfigError::InvalidTable { reason } if reason.contains("no characteristics")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_duplicate_characteristic_on_one_material() {
        let err = mutated(|json| {
            json["materials"][HIDE]["characteristics"] =
                serde_json::json!(["toughness", "toughness"]);
        });
        assert!(
            matches!(&err, MaterialsConfigError::InvalidTable { reason } if reason.contains("twice")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_negative_hand_working_rate() {
        let err = mutated(|json| {
            json["materials"][FIBRE]["hand_working"]["rate"] = serde_json::json!(-0.1);
        });
        assert!(
            matches!(&err, MaterialsConfigError::Invalid { field, .. } if field == "materials.fibre.hand_working.rate"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_quality_ceiling_outside_the_reading_range() {
        let err = mutated(|json| {
            json["materials"][BONE]["hand_working"]["quality_ceiling"] = serde_json::json!(1.5);
        });
        assert!(
            matches!(&err, MaterialsConfigError::Invalid { field, .. } if field == "materials.bone.hand_working.quality_ceiling"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_variety_that_omits_or_invents_an_axis() {
        let omits = mutated(|json| {
            json["materials"][HIDE]["varieties"] =
                serde_json::json!({ "buckskin": { "toughness": 0.4 } });
        });
        assert!(
            matches!(&omits, MaterialsConfigError::InvalidTable { reason } if reason.contains("omits")),
            "got {omits}"
        );
        let invents = mutated(|json| {
            json["materials"][HIDE]["varieties"] = serde_json::json!({
                "buckskin": { "toughness": 0.4, "suppleness": 0.6, "sheen": 0.9 }
            });
        });
        assert!(
            matches!(&invents, MaterialsConfigError::InvalidTable { reason } if reason.contains("does not declare")),
            "got {invents}"
        );
    }

    /// The yield edge's own rejections — the `UnknownItem` debt, paid at load.
    #[test]
    fn validate_yield_rejects_an_unknown_material() {
        let config = builtin();
        let err = config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: "unobtanium".to_string(),
                    per_biomass: 0.01,
                    characteristics: readings(&[("toughness", 0.5)]),
                }],
            )
            .expect_err("an unknown material must be rejected");
        assert!(
            matches!(err, MaterialYieldError::UnknownMaterial { .. }),
            "got {err}"
        );
    }

    #[test]
    fn validate_yield_rejects_a_non_positive_rate() {
        let config = builtin();
        let err = config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: HIDE.to_string(),
                    per_biomass: 0.0,
                    characteristics: readings(&[("toughness", 0.5), ("suppleness", 0.5)]),
                }],
            )
            .expect_err("a zero rate must be rejected");
        assert!(
            matches!(err, MaterialYieldError::Invalid { .. }),
            "got {err}"
        );
    }

    /// **Exactly the declared axes, both directions** — a defaulted axis is a silently wrong
    /// reading, and an invented one is read by nothing.
    #[test]
    fn validate_yield_rejects_a_row_that_omits_or_invents_an_axis() {
        let config = builtin();
        let omits = config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: HIDE.to_string(),
                    per_biomass: 0.01,
                    characteristics: readings(&[("toughness", 0.5)]),
                }],
            )
            .expect_err("a missing axis must be rejected");
        assert!(
            matches!(omits, MaterialYieldError::AxisMismatch { .. }),
            "got {omits}"
        );

        let invents = config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: HIDE.to_string(),
                    per_biomass: 0.01,
                    characteristics: readings(&[
                        ("toughness", 0.5),
                        ("suppleness", 0.5),
                        ("sheen", 0.5),
                    ]),
                }],
            )
            .expect_err("an invented axis must be rejected");
        assert!(
            matches!(invents, MaterialYieldError::AxisMismatch { .. }),
            "got {invents}"
        );

        config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: HIDE.to_string(),
                    per_biomass: 0.01,
                    characteristics: readings(&[("toughness", 0.5), ("suppleness", 0.5)]),
                }],
            )
            .expect("exactly the declared axes must be accepted");
    }

    #[test]
    fn validate_yield_rejects_a_reading_outside_the_range() {
        let config = builtin();
        let err = config
            .validate_yield(
                "species.deer.hunt_yield",
                &[MaterialYieldDef {
                    material: HIDE.to_string(),
                    per_biomass: 0.01,
                    characteristics: readings(&[("toughness", 1.4), ("suppleness", 0.5)]),
                }],
            )
            .expect_err("a reading off the axis must be rejected");
        assert!(
            matches!(err, MaterialYieldError::Invalid { .. }),
            "got {err}"
        );
    }
}
