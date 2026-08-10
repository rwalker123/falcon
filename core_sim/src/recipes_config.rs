//! **The recipe book** (`data/recipes.json`) — how a pile of material becomes a thing.
//!
//! Design: `docs/plan_crafting_and_materials.md` §3. **One structure: `inputs → outputs`, both lists
//! of THINGS, where a thing is a material or a piece of equipment.** Bronze and steel are not
//! special cases; a tool is not a special case; the alloy and the sled are the same record with
//! different rows.
//!
//! # Four rules this module exists to keep
//!
//! 1. **A recipe reads ONE characteristic** ([`RecipeInput::reads`]), and that is what makes *"there
//!    is no best hide"* real. A sled reads `toughness` and a halter reads `suppleness`, so spending
//!    the mammoth hide on the halter is a mistake the player is free to make.
//! 2. **Continuous in, discrete out — and there is ONE quality ladder for the whole game.** The
//!    reading falls in a `characteristic_bands` rung, that rung's **name is the grade**, and the
//!    grade declares **absolutes** ([`crate::equipment_config::EquipmentEffect`] names the value a
//!    stat *takes*). Nothing may scale a resolved stat by a quality number — there is no
//!    representation for it, and that is what keeps *flat until expiry, then a step down*
//!    structural. **A recipe declares no seams of its own**: a second set of cut points beside the
//!    bands would be a second authority to drift from.
//! 3. **The grade is fixed at draw time and never moves.** It is not a taper.
//! 4. **Knowledge gates a recipe only when the recipe says so** ([`RecipeDef::requires_knowledge`]),
//!    and only the **tool** recipes say so — gated on the crafts of what the tool is MADE FROM,
//!    never on the craft it unlocks. Otherwise nothing could start: you would need the loom to learn
//!    the weaving that lets you build the loom.
//!
//! Loader mirrors [`crate::materials_config`]: baked-in builtin + `RECIPES_CONFIG_PATH` override +
//! [`RecipesConfig::validate`] inside `from_json_str`, plus the cross-config
//! [`RecipesConfig::validate_against`] at the composition seam — a material id and an item id are
//! both `String`s, so validate is the only thing between the file and a running sim
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
    config_load::{load_config_from_env, ConfigLoadError},
    crafting::crafts_declared_by,
    equipment_config::{EquipmentConfig, EquipmentEffect},
    materials_config::{MaterialsConfig, READING_MAX, READING_MIN},
};

pub const BUILTIN_RECIPES_CONFIG: &str = include_str!("data/recipes.json");

/// **The band a recipe's lowest grade must name** — the first rung of `characteristic_bands`.
///
/// Something has to answer for a reading of `0.0`. A recipe whose lowest grade sits higher would
/// leave the bottom of the range with no effects to inherit, which is the exact twin of
/// `materials_config`'s first-band rule and is checked for the same reason.
const FIRST_GRADE_BAND_INDEX: usize = 0;

/// Bench dials shared by every recipe.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraftingTuning {
    /// **Progress one worker contributes per turn at a rate of `1.0`.** The bench accrues
    /// `workers × progress_per_worker_turn × craft_speed`, where `craft_speed` is the bounding
    /// tool's equipped value or — with no tool — the material's own
    /// [`crate::materials_config::HandWorking::rate`], which is `0` for a material that cannot be
    /// worked bare-handed. **That zero is the whole refusal mechanism**: there is no *"you cannot
    /// craft that"* branch anywhere, exactly as `max(0, attack − defense)` refuses a hunt.
    pub progress_per_worker_turn: f32,
}

/// One row of a recipe's `inputs` — a material, how much of it, and at most one **axis the recipe
/// reads**.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeInput {
    /// The material id, resolved against the materials table at load.
    pub material: String,
    /// How much of it one output pass consumes, **before** the bench tool's
    /// `craft_material_efficiency`.
    pub amount: f32,
    /// A named variety of the material this row wants
    /// ([`crate::materials_config::MaterialDef::varieties`]) — what makes an alloy recipe
    /// expressible (*"9 parts copper, 1 part tin"*) without inventing two materials. **Parsed,
    /// validated and none ships**, for the same reason no variety ships: the material that needs
    /// them has no producer yet.
    #[serde(default)]
    pub variety: Option<String>,
    /// **The one axis this recipe reads.** At most one input across the whole recipe may name one,
    /// and it is what selects the output's [`RecipeGrade`] — a recipe that read two would have two
    /// answers to one question. Absent on every other row, and on every row of a recipe that has no
    /// grades to select (an alloy).
    #[serde(default)]
    pub reads: Option<String>,
}

/// One row of a recipe's `outputs` — **exactly one** of a piece of equipment or a material.
///
/// A single struct with two optional ids rather than an enum, because the JSON reads
/// `{ "equipment": "sled", "amount": 1 }` / `{ "material": "metal", "amount": 10 }` and an
/// externally-tagged enum would demand a wrapper key that says nothing. `validate` enforces the
/// exclusivity that the shape cannot.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeOutput {
    /// An item id from `equipment.json`. Mutually exclusive with [`Self::material`].
    #[serde(default)]
    pub equipment: Option<String>,
    /// A material id from `materials.json`. Mutually exclusive with [`Self::equipment`].
    #[serde(default)]
    pub material: Option<String>,
    /// How many units one pass produces.
    pub amount: f32,
    /// **What a MATERIAL output reads, per axis** — required on a material output and rejected on an
    /// equipment one. A produced material has to state its own characteristics for the same reason a
    /// hunted one does: there is nothing else for the store's merge key to be derived from, and a
    /// defaulted axis would be a silently wrong reading.
    #[serde(default)]
    pub characteristics: BTreeMap<String, f32>,
}

impl RecipeOutput {
    /// The equipment id this row makes, or `None` for a material row.
    pub fn equipment_id(&self) -> Option<&str> {
        self.equipment.as_deref()
    }

    /// The material id this row makes, or `None` for an equipment row.
    pub fn material_id(&self) -> Option<&str> {
        self.material.as_deref()
    }
}

/// **What one BAND buys on this recipe's output** — the payload half of a grade, the name half being
/// the `characteristic_bands` rung the map is keyed by.
///
/// There is no `when`: the cut points already exist in the materials table, and a per-recipe seam
/// beside them would be a second authority free to drift. So the grade a craft comes out at is
/// simply the band of `min(material reading, tool quality ceiling)`.
///
/// **A band a recipe does not declare INHERITS THE ONE BELOW IT**
/// ([`RecipeDef::grade_effects_for`]), so a recipe wanting three steps writes three and a recipe
/// wanting none writes no `grades` block at all — the batch is still *stamped* with the band it was
/// made at, that band simply buys it no stat.
///
/// **A grade may only declare a stat the output item's TIERS declare**, and the **anchor** rung — the
/// band the bench material's bare-handed `hand_working.quality_ceiling` falls in — must declare the
/// same value that tier does ([`RecipesConfig::validate_grades_against_item`]). That is what keeps a
/// grade from becoming a second home for a shipped number: the tier stays the one home, the grades
/// are a spread around it, and a bare-handed craft off the best material a band can work by hand
/// reproduces the game as shipped.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeGrade {
    /// The absolutes the output takes at this band. May be empty, which states *"this band buys
    /// nothing extra"* — the same thing the band below it would have said.
    #[serde(default)]
    pub effects: Vec<EquipmentEffect>,
}

/// One recipe.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeDef {
    /// Player-facing label. The sim never branches on it.
    pub display_name: String,
    /// **The craft this recipe practises — and therefore TEACHES.** Validated to be the craft of the
    /// material the recipe [`reads`](RecipeInput::reads), so it cannot drift from the material the
    /// bench is actually working: one home per fact, stated in the file because that is where it is
    /// read.
    pub craft: String,
    /// **Worker-turns for one output pass** at `progress_per_worker_turn × 1.0`.
    pub work: f32,
    /// **The crafts a faction must already know to put this on a bench.** Empty on an ordinary kit
    /// recipe — you can craft a sled by hand from day one, and doing it is what teaches Tanning.
    ///
    /// Every **tool** recipe carries one entry per material it is made from, and `validate` enforces
    /// exactly that: a named craft must be the craft of one of this recipe's own **input**
    /// materials. Since a tool may never be made from the material it bounds (also validated), the
    /// craft a tool *unlocks* can never appear here — which is what makes the deadlock
    /// (*metal needs a crucible, the crucible needs metalworking*) unrepresentable rather than
    /// merely avoided.
    #[serde(default)]
    pub requires_knowledge: Vec<String>,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<RecipeOutput>,
    /// **The discrete output qualities, KEYED BY `characteristic_bands` NAME.** The map's own key
    /// order is irrelevant — the band table is what orders them ([`Self::grades_by_band`]), and
    /// `validate_against` rejects a key that is not a rung.
    ///
    /// **Absent is a real statement**, not a missing value: it says this item's payload is not
    /// tier-bought (the husbandry gear's `pen_carry`, the wayfinding gear's vantage) or is a bench
    /// stat nothing yet grades. The output is still stamped with the band it was made at.
    #[serde(default)]
    pub grades: BTreeMap<String, RecipeGrade>,
}

impl RecipeDef {
    /// The input row that names the axis the recipe reads, if any.
    pub fn reading_input(&self) -> Option<&RecipeInput> {
        self.inputs.iter().find(|input| input.reads.is_some())
    }

    /// **The material the bench is working** — the one whose craft is practised, whose tool is
    /// consulted and whose bare-handed rate applies. The read input's material, or (for a recipe
    /// that reads nothing) the first input's.
    pub fn bench_material(&self) -> Option<&str> {
        self.reading_input()
            .or_else(|| self.inputs.first())
            .map(|input| input.material.as_str())
    }

    /// The axis the recipe reads, if any.
    pub fn reads_axis(&self) -> Option<&str> {
        self.reading_input()
            .and_then(|input| input.reads.as_deref())
    }

    /// **The equipment id this recipe makes**, or `None` for one that makes only a material. The
    /// join key from a published craft offer to the band's own batches of the thing it would make.
    pub fn output_equipment_id(&self) -> Option<&str> {
        self.outputs.iter().find_map(|output| output.equipment_id())
    }

    /// **The declared grades in BAND order, lowest first**, each with the rung index it names. A key
    /// the band table does not carry is dropped, which `validate_against` makes unreachable.
    pub fn grades_by_band<'a>(
        &'a self,
        materials: &MaterialsConfig,
    ) -> Vec<(usize, &'a str, &'a RecipeGrade)> {
        let mut ordered: Vec<(usize, &str, &RecipeGrade)> = self
            .grades
            .iter()
            .filter_map(|(name, grade)| {
                materials
                    .band_index_of(name)
                    .map(|index| (index, name.as_str(), grade))
            })
            .collect();
        ordered.sort_by_key(|(index, ..)| *index);
        ordered
    }

    /// **The grade a reading comes out at: the BAND it falls in.** One vocabulary rates a hide's
    /// toughness and the sled made out of it, so a reading of `.55` is *good* in both places.
    ///
    /// The name is a property of the object whether or not this recipe declares that band —
    /// declaration governs *effects* only. `None` for a recipe that reads no characteristic, which is
    /// a real answer: an alloy has no quality to name.
    pub fn grade_for<'a>(&self, reading: f32, materials: &'a MaterialsConfig) -> Option<&'a str> {
        self.reads_axis()?;
        materials.band_name(materials.band_index(reading))
    }

    /// **What the band named `grade` buys — INHERITING THE ONE BELOW IT.** The effects of the
    /// highest-indexed declared grade whose band index is `<= index(grade)`, and an empty slice when
    /// the recipe declares nothing at or below it.
    ///
    /// Inheritance is what lets a recipe write three steps instead of one per rung, and it is why an
    /// undeclared band is still a legal stamp: *"excellent, which buys this item nothing"* is a
    /// coherent thing for an object to be.
    pub fn grade_effects_for(
        &self,
        grade: &str,
        materials: &MaterialsConfig,
    ) -> &[EquipmentEffect] {
        let Some(target) = materials.band_index_of(grade) else {
            return &[];
        };
        self.grades_by_band(materials)
            .into_iter()
            .take_while(|(index, ..)| *index <= target)
            .last()
            .map(|(_, _, grade)| grade.effects.as_slice())
            .unwrap_or_default()
    }
}

/// The recipe book plus the bench dials.
///
/// **The root is deliberately open** — `recipes.json` carries its rationale in `_comment*` keys,
/// exactly as every other config in `src/data/` does. Every record below it is *closed*, which is
/// where the protection is needed: a mistyped `readz` would silently make a recipe gradeless.
#[derive(Debug, Clone, PartialEq, Resource, Deserialize)]
pub struct RecipesConfig {
    pub crafting: CraftingTuning,
    /// Every recipe, by id. A `BTreeMap` so any published catalogue has a stable order.
    pub recipes: BTreeMap<String, RecipeDef>,
}

impl RecipesConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_RECIPES_CONFIG)
                .expect("builtin recipes config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, RecipesConfigError> {
        let config: RecipesConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, RecipesConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| RecipesConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        RecipesConfig::from_json_str(&contents)
    }

    /// The recipe with this id, or `None`.
    pub fn recipe(&self, id: &str) -> Option<&RecipeDef> {
        self.recipes.get(id)
    }

    /// Every recipe, in id order — the stable iteration a catalogue rides.
    pub fn recipes(&self) -> impl Iterator<Item = (&str, &RecipeDef)> {
        self.recipes.iter().map(|(id, def)| (id.as_str(), def))
    }

    /// **What a readout calls an equipment item** — the display name of the recipe that makes it,
    /// or the item's own id when no recipe does.
    ///
    /// **The book is the item's name because `equipment.json` carries none.** An item id is a key
    /// (`bone_awl`, `husbandry_gear`); the recipe that makes it is where a human already wrote the
    /// player-facing words, so a refusal that has to say *"No bone awl"* asks here rather than
    /// growing a second name table beside the first. The fallback is the id, which is the honest
    /// answer for a thing the book cannot make.
    pub fn item_display_name<'a>(&'a self, item: &'a str) -> &'a str {
        self.recipes()
            .find(|(_, recipe)| recipe.output_equipment_id() == Some(item))
            .map(|(_, recipe)| recipe.display_name.as_str())
            .unwrap_or(item)
    }

    /// **The grade a bare-handed craft of `item` comes out at** — its recipe's [`anchor_band`], the
    /// band the bench material's own `hand_working.quality_ceiling` falls in. `None` for an item the
    /// book cannot make, or whose bench material cannot be worked bare-handed at all.
    ///
    /// It is what a **start-stocked** unit is stamped with
    /// ([`crate::components::BandEquipment::start_stocked_owned`]): `validate_grades_against_item`
    /// already enforces that the anchor grade agrees with the item's default tier for every stat it
    /// declares, and a spawn stocks that same default tier — so a shipped spear already performs
    /// exactly as an anchor-grade craft does, and this is the wire finally saying so.
    ///
    /// The join is `item_display_name`'s, for the same reason: the book is where an item's crafted
    /// facts are written. Two recipes making one item would resolve the **first in book order**,
    /// which no shipped book has and none needs — a second recipe for one item would be a second
    /// statement of what it is made of.
    pub fn anchor_grade_for_item<'a>(
        &self,
        item: &str,
        materials: &'a MaterialsConfig,
    ) -> Option<&'a str> {
        let (_, recipe) = self
            .recipes()
            .find(|(_, recipe)| recipe.output_equipment_id() == Some(item))?;
        anchor_band(recipe, materials)
    }

    /// The book's ids, for a refusal message — a player who mistypes a recipe is told what there is.
    pub fn recipe_ids_for_message(&self) -> String {
        self.recipes
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// **Invariants a recipe book must satisfy on its own** — everything that can be checked without
    /// the other two tables. Run inside [`Self::from_json_str`], so every load path is covered.
    pub fn validate(&self) -> Result<(), RecipesConfigError> {
        if !self.crafting.progress_per_worker_turn.is_finite()
            || self.crafting.progress_per_worker_turn <= 0.0
        {
            return Err(RecipesConfigError::Invalid {
                field: "crafting.progress_per_worker_turn".to_string(),
                constraint: "be finite and greater than 0 - at 0 no bench could ever finish \
                             anything and every recipe would be silently unmakeable"
                    .to_string(),
                value: self.crafting.progress_per_worker_turn.to_string(),
            });
        }
        if self.recipes.is_empty() {
            return Err(RecipesConfigError::InvalidBook {
                reason: "the recipe book is empty - nothing could ever be made".to_string(),
            });
        }
        for (id, recipe) in &self.recipes {
            self.validate_recipe(id, recipe)?;
        }
        Ok(())
    }

    fn validate_recipe(&self, id: &str, recipe: &RecipeDef) -> Result<(), RecipesConfigError> {
        if !recipe.work.is_finite() || recipe.work <= 0.0 {
            return Err(RecipesConfigError::Invalid {
                field: format!("recipes.{id}.work"),
                constraint:
                    "be finite and greater than 0 - a recipe finished before it is started \
                             would complete on the turn it is chosen, forever"
                        .to_string(),
                value: recipe.work.to_string(),
            });
        }
        if recipe.inputs.is_empty() {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!(
                    "recipe '{id}' consumes nothing - there would be no material to work, no craft \
                     to practise and no reading to grade it by"
                ),
            });
        }
        if recipe.outputs.is_empty() {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!("recipe '{id}' produces nothing"),
            });
        }
        for (index, input) in recipe.inputs.iter().enumerate() {
            if !input.amount.is_finite() || input.amount <= 0.0 {
                return Err(RecipesConfigError::Invalid {
                    field: format!("recipes.{id}.inputs[{index}].amount"),
                    constraint: "be finite and greater than 0".to_string(),
                    value: input.amount.to_string(),
                });
            }
            if recipe.inputs[..index]
                .iter()
                .any(|prior| prior.material == input.material)
            {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!(
                        "recipe '{id}' names the material '{}' twice in its inputs - one draw is \
                         made per material, so the second row would be silently dropped",
                        input.material
                    ),
                });
            }
        }
        // **EXACTLY ONE input may carry `reads`** - a recipe that read two axes would have two
        // answers to the one question the grade asks.
        let reading_rows = recipe
            .inputs
            .iter()
            .filter(|input| input.reads.is_some())
            .count();
        if reading_rows > 1 {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!(
                    "recipe '{id}' reads {reading_rows} characteristics - exactly one input may \
                     carry `reads`, because one reading selects one grade"
                ),
            });
        }
        for (index, output) in recipe.outputs.iter().enumerate() {
            if !output.amount.is_finite() || output.amount <= 0.0 {
                return Err(RecipesConfigError::Invalid {
                    field: format!("recipes.{id}.outputs[{index}].amount"),
                    constraint: "be finite and greater than 0".to_string(),
                    value: output.amount.to_string(),
                });
            }
            match (&output.equipment, &output.material) {
                (Some(_), Some(_)) | (None, None) => {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' outputs[{index}] must name exactly one of `equipment` \
                             or `material`"
                        ),
                    })
                }
                (Some(_), None) if !output.characteristics.is_empty() => {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' outputs[{index}] makes equipment but states \
                             characteristics - a piece of equipment carries a GRADE, not a \
                             characteristic vector, and the readings would be read by nothing"
                        ),
                    })
                }
                (None, Some(_)) if output.characteristics.is_empty() => {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' outputs[{index}] makes a material but states no \
                             characteristics - the store's merge key would have nothing to derive \
                             from"
                        ),
                    })
                }
                _ => {}
            }
            let duplicate = recipe.outputs[..index].iter().any(|prior| {
                prior.equipment_id() == output.equipment_id()
                    && prior.material_id() == output.material_id()
            });
            if duplicate {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!("recipe '{id}' outputs the same thing twice at [{index}]"),
                });
            }
        }
        self.validate_grades(id, recipe)
    }

    fn validate_grades(&self, id: &str, recipe: &RecipeDef) -> Result<(), RecipesConfigError> {
        if recipe.grades.is_empty() {
            return Ok(());
        }
        // **Grades need something to select them.** A recipe with grades and no `reads` would have
        // an unreachable ladder and no way to say which rung an output landed on.
        if recipe.reads_axis().is_none() {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!(
                    "recipe '{id}' declares grades but reads no characteristic - nothing would \
                     select the grade"
                ),
            });
        }
        // **A grade is a property of a piece of equipment.** A material output carries its own
        // characteristics instead, so grading one would be two answers to the same question.
        if recipe
            .outputs
            .iter()
            .all(|output| output.material_id().is_some())
        {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!(
                    "recipe '{id}' declares grades but outputs only materials - a material carries \
                     its own characteristics, so the grade would be read by nothing"
                ),
            });
        }
        // **The band NAMES are checked at the composition seam** — this table does not carry the
        // rating vocabulary, so `validate_against` is where a key that is not a band, and a lowest
        // grade that is not the first band, are rejected.
        for (name, grade) in &recipe.grades {
            for (index, effect) in grade.effects.iter().enumerate() {
                let value = effect.tier.value();
                if !value.is_finite() || value < 0.0 {
                    return Err(RecipesConfigError::Invalid {
                        field: format!("recipes.{id}.grades.{name}.effects[{index}]"),
                        constraint: "be finite and not negative".to_string(),
                        value: value.to_string(),
                    });
                }
                if grade.effects[..index]
                    .iter()
                    .any(|prior| prior.stat == effect.stat)
                {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' grade '{name}' declares the same stat twice - the \
                             second would be silently dead"
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// **The cross-config seam** — every id this book spells, reconciled against the two tables that
    /// own them. Run at the composition seam in `build_headless_app`, because it is the only place
    /// all three configs are in scope at once.
    ///
    /// It is the `UnknownItem` debt again: a material id and an item id are `String`s, so a recipe
    /// naming `spearz` would parse, validate, and then make nothing forever.
    pub fn validate_against(
        &self,
        materials: &MaterialsConfig,
        equipment: &EquipmentConfig,
    ) -> Result<(), RecipesConfigError> {
        let crafts = crafts_declared_by(materials);
        for (id, recipe) in &self.recipes {
            Self::validate_grade_bands(id, recipe, materials)?;
            for input in &recipe.inputs {
                let Some(def) = materials.material(&input.material) else {
                    return Err(RecipesConfigError::UnknownMaterial {
                        recipe: id.clone(),
                        material: input.material.clone(),
                    });
                };
                if let Some(variety) = &input.variety {
                    if !def.varieties.contains_key(variety) {
                        return Err(RecipesConfigError::InvalidBook {
                            reason: format!(
                                "recipe '{id}' wants the variety '{variety}' of '{}', which that \
                                 material does not declare",
                                input.material
                            ),
                        });
                    }
                }
                if let Some(axis) = &input.reads {
                    if !def.characteristics.iter().any(|known| known == axis) {
                        return Err(RecipesConfigError::InvalidBook {
                            reason: format!(
                                "recipe '{id}' reads '{axis}' off '{}', which is not an axis that \
                                 material declares",
                                input.material
                            ),
                        });
                    }
                }
            }
            for output in &recipe.outputs {
                if let Some(item) = output.equipment_id() {
                    if equipment.item(item).is_none() {
                        return Err(RecipesConfigError::UnknownItem {
                            recipe: id.clone(),
                            item: item.to_string(),
                        });
                    }
                    // **A TOOL IS NEVER MADE FROM THE MATERIAL IT BOUNDS.** Otherwise you would
                    // need the scarce material to build the thing that stretches it, which is the
                    // opposite of what a tool is for — and it is also half of what makes the
                    // knowledge deadlock unrepresentable (see `requires_knowledge`).
                    if let Some(bounded) =
                        equipment.item(item).and_then(|def| def.bounds_material())
                    {
                        if recipe.inputs.iter().any(|input| input.material == bounded) {
                            return Err(RecipesConfigError::InvalidBook {
                                reason: format!(
                                    "recipe '{id}' makes '{item}', which bounds '{bounded}', out of \
                                     '{bounded}' - a tool must never cost the material it stretches"
                                ),
                            });
                        }
                    }
                    // **A COUNT-BEARING LEDGER CANNOT BANK HALF A SPEAR.** An equipment output's
                    // `amount` becomes a batch's `count`, so a fractional row would either round
                    // (inventing or destroying an item) or truncate to nothing.
                    if output.amount.fract() != 0.0 {
                        return Err(RecipesConfigError::InvalidBook {
                            reason: format!(
                                "recipe '{id}' makes {} of '{item}' - an equipment output is counted \
                                 in whole items",
                                output.amount
                            ),
                        });
                    }
                    Self::validate_grades_against_item(id, recipe, item, equipment, materials)?;
                }
                if let Some(material) = output.material_id() {
                    let Some(def) = materials.material(material) else {
                        return Err(RecipesConfigError::UnknownMaterial {
                            recipe: id.clone(),
                            material: material.to_string(),
                        });
                    };
                    // **EXACTLY the declared axes, both directions** — the same rule a source's
                    // yield row follows, and for the same reason: a defaulted axis is a silently
                    // wrong reading and an invented one is read by nothing.
                    for axis in &def.characteristics {
                        if !output.characteristics.contains_key(axis) {
                            return Err(RecipesConfigError::InvalidBook {
                                reason: format!(
                                    "recipe '{id}' outputs '{material}' without stating '{axis}'"
                                ),
                            });
                        }
                    }
                    for (axis, reading) in &output.characteristics {
                        if !def.characteristics.iter().any(|known| known == axis) {
                            return Err(RecipesConfigError::InvalidBook {
                                reason: format!(
                                    "recipe '{id}' outputs '{material}' stating '{axis}', which \
                                     that material does not declare"
                                ),
                            });
                        }
                        if !reading_in_range(*reading) {
                            return Err(RecipesConfigError::Invalid {
                                field: format!("recipes.{id}.outputs.{material}.{axis}"),
                                constraint: format!(
                                    "be finite and within {READING_MIN}..={READING_MAX}"
                                ),
                                value: reading.to_string(),
                            });
                        }
                    }
                }
            }
            // **The recipe's craft IS the craft of the material it works.** Stated in the file
            // because that is where a reader needs it, validated here so it cannot drift from the
            // material the bench actually draws.
            let Some(bench_material) = recipe.bench_material() else {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!("recipe '{id}' works no material"),
                });
            };
            let expected = materials
                .material(bench_material)
                .map(|def| def.craft.as_str())
                .unwrap_or_default();
            if recipe.craft != expected {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!(
                        "recipe '{id}' declares the craft '{}' but works '{bench_material}', whose \
                         craft is '{expected}'",
                        recipe.craft
                    ),
                });
            }
            // **A tool is gated on the crafts of what it is MADE FROM, never on the craft it
            // unlocks** — enforced as *"every required craft is the craft of one of this recipe's
            // own inputs"*. Paired with the never-made-from-what-it-bounds rule above, that makes
            // the deadlock unrepresentable rather than merely avoided.
            for craft in &recipe.requires_knowledge {
                if !crafts.contains(&craft.as_str()) {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' requires the craft '{craft}', which no material declares"
                        ),
                    });
                }
                let from_an_input = recipe.inputs.iter().any(|input| {
                    materials
                        .material(&input.material)
                        .is_some_and(|def| def.craft == *craft)
                });
                if !from_an_input {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' requires the craft '{craft}', which none of its own \
                             input materials is worked by - a recipe is gated on the crafts of what \
                             it is MADE FROM, never on the craft it unlocks"
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// **A GRADE KEY IS A `characteristic_bands` NAME** — the whole of *"one quality ladder for the
    /// whole game"*, enforced rather than left to convention.
    ///
    /// Two rejections, and the second is the one that keeps the ladder total: the **lowest** declared
    /// grade must name the **first** band, because something has to answer for a reading of `0.0` and
    /// inheritance only ever looks *down*.
    fn validate_grade_bands(
        id: &str,
        recipe: &RecipeDef,
        materials: &MaterialsConfig,
    ) -> Result<(), RecipesConfigError> {
        let mut lowest: Option<usize> = None;
        for name in recipe.grades.keys() {
            let Some(index) = materials.band_index_of(name) else {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!(
                        "recipe '{id}' declares the grade '{name}', which is not a \
                         `characteristic_bands` rung - a grade IS a band, so a second vocabulary \
                         here is a second set of cut points to drift from"
                    ),
                });
            };
            lowest = Some(lowest.map_or(index, |current: usize| current.min(index)));
        }
        let Some(lowest) = lowest else {
            return Ok(());
        };
        if lowest != FIRST_GRADE_BAND_INDEX {
            return Err(RecipesConfigError::InvalidBook {
                reason: format!(
                    "recipe '{id}''s lowest grade names the band at rung {lowest}, not the first one \
                     - a band a recipe does not declare inherits the one BELOW it, so nothing would \
                     answer for a reading of {READING_MIN}"
                ),
            });
        }
        Ok(())
    }

    /// **WHAT A GRADE MAY SAY ABOUT THE THING IT MAKES.** A grade declares absolutes on a *crafted*
    /// item, so every rule here is about it not becoming a second home for a number that already
    /// has one — the objection that kept every grade's payload empty until the tier owned these.
    ///
    /// - **Only a stat the item's own tiers declare.** What the material buys sits on the tier; a
    ///   grade *replaces* one of those values and may never introduce a stat the item does not
    ///   otherwise have. A grade naming `pen_carry` — whose equipped side is the hunt haul's —
    ///   would be exactly the second home.
    /// - **The mass bounds are restated verbatim.** A grade's effect is what
    ///   [`crate::equipment_config::LiveItem::effect_entry`] answers with, bounds included, so an
    ///   excellent snare that dropped `max_body_mass` would quietly become a mammoth trap.
    /// - **The ANCHOR grade equals the shipped tier value** — see [`anchor_band`]. That is what makes
    ///   a bare-handed craft off the best material a band can work by hand reproduce today's game
    ///   exactly, and it is what stops the two numbers drifting: the tier stays the one home, and the
    ///   grades are a spread around it.
    fn validate_grades_against_item(
        id: &str,
        recipe: &RecipeDef,
        item: &str,
        equipment: &EquipmentConfig,
        materials: &MaterialsConfig,
    ) -> Result<(), RecipesConfigError> {
        let Some(def) = equipment.item(item) else {
            return Ok(());
        };
        for (name, grade) in &recipe.grades {
            for effect in &grade.effects {
                let Some(tier_effect) = def
                    .tiers
                    .iter()
                    .find_map(|tier| tier.effects.iter().find(|e| e.stat == effect.stat))
                else {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' grade '{name}' declares {:?}, which no tier of '{item}' \
                             declares - a grade replaces what the material bought, it may not be a \
                             second home for a number that lives elsewhere",
                            effect.stat
                        ),
                    });
                };
                if effect.min_body_mass != tier_effect.min_body_mass
                    || effect.max_body_mass != tier_effect.max_body_mass
                {
                    return Err(RecipesConfigError::InvalidBook {
                        reason: format!(
                            "recipe '{id}' grade '{name}' declares {:?} without '{item}''s own mass \
                             bounds - a grade replaces the effect entire, so a dropped bound would \
                             silently widen what the item reaches",
                            effect.stat
                        ),
                    });
                }
            }
        }
        // **The anchor, resolved AFTER inheritance** — a recipe that declares nothing at the anchor
        // band still answers with whatever it inherits there, and that is the value the check is
        // about. A bench material with no bare-handed rate at all has no anchor and no check.
        let Some(anchor) = anchor_band(recipe, materials) else {
            return Ok(());
        };
        for effect in recipe.grade_effects_for(anchor, materials) {
            let shipped = def
                .default_tier()
                .effects
                .iter()
                .find(|e| e.stat == effect.stat)
                .map_or(effect.tier.value(), |e| e.tier.value());
            if effect.tier.value() != shipped {
                return Err(RecipesConfigError::InvalidBook {
                    reason: format!(
                        "recipe '{id}' resolves {:?} = {} at its anchor band '{anchor}', which is not \
                         what '{item}''s default tier declares ({shipped}) - a bare-handed craft off \
                         the best material that band can work by hand must reproduce the shipped item \
                         exactly",
                        effect.stat,
                        effect.tier.value()
                    ),
                });
            }
        }
        Ok(())
    }
}

/// **THE ANCHOR BAND — DERIVED, never a literal.**
///
/// The band the recipe's **bench material**'s bare-handed
/// [`crate::materials_config::HandWorking::quality_ceiling`] falls in. That is the best a band with
/// no tool can reach, so pinning it to the shipped item is what makes *"a tool run dry drops the band
/// back to the rate the game already ships at rather than into a spiral"* true by construction.
///
/// Reading the ceiling rather than naming a rung is the same shape this model already chose twice —
/// `dispersion` multiplies a species' own `wariness` rather than reading a "jumpy" flag, and
/// `max_body_mass` reads `body_mass` rather than a `size_class`. A material with **no**
/// `hand_working` cannot be worked bare-handed at all, so it has no anchor and the check does not
/// apply.
fn anchor_band<'a>(recipe: &RecipeDef, materials: &'a MaterialsConfig) -> Option<&'a str> {
    let ceiling = materials
        .material(recipe.bench_material()?)?
        .hand_working?
        .quality_ceiling;
    materials.band_name(materials.band_index(ceiling))
}

/// A reading is a position on an axis, so it has both ends. Same bound the materials table applies,
/// named once per file that checks one.
fn reading_in_range(value: f32) -> bool {
    value.is_finite() && (READING_MIN..=READING_MAX).contains(&value)
}

/// Why a recipe book cannot be used.
#[derive(Debug, Error)]
pub enum RecipesConfigError {
    #[error("failed to read recipes config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse recipes config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid recipes config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
    #[error("invalid recipes config: {reason}")]
    InvalidBook { reason: String },
    #[error("recipe '{recipe}' names '{material}', which is not a material")]
    UnknownMaterial { recipe: String, material: String },
    #[error("recipe '{recipe}' names '{item}', which is not an equipment item")]
    UnknownItem { recipe: String, item: String },
}

impl ConfigLoadError for RecipesConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is there
    /// and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the recipe book.
#[derive(Resource, Debug, Clone)]
pub struct RecipesConfigHandle(pub Arc<RecipesConfig>);

impl RecipesConfigHandle {
    pub fn new(config: Arc<RecipesConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<RecipesConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<RecipesConfig>) {
        self.0 = config;
    }
}

impl Default for RecipesConfigHandle {
    fn default() -> Self {
        Self(RecipesConfig::builtin())
    }
}

/// Metadata about the recipe book source.
#[derive(Resource, Debug, Clone, Default)]
pub struct RecipesConfigMetadata {
    path: Option<PathBuf>,
}

impl RecipesConfigMetadata {
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

/// Load the recipe book from the environment (`RECIPES_CONFIG_PATH`) or the default data path.
///
/// **Cross-config validation is the caller's**, because the materials table and the item table are
/// only both in scope at the composition seam — see [`RecipesConfig::validate_against`], which
/// `build_headless_app` runs immediately after this.
pub fn load_recipes_config_from_env() -> (Arc<RecipesConfig>, RecipesConfigMetadata) {
    let (config, source) = load_config_from_env(
        "RECIPES_CONFIG_PATH",
        "recipes_config",
        "src/data/recipes.json",
        RecipesConfig::builtin,
        RecipesConfig::from_file,
    );
    (config, RecipesConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> RecipesConfig {
        RecipesConfig::from_json_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses")
    }

    /// The shipped book with one field replaced, so each rejection test states exactly the one thing
    /// it broke.
    fn mutated(mutate: impl FnOnce(&mut serde_json::Value)) -> RecipesConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
        mutate(&mut json);
        RecipesConfig::from_json_str(&json.to_string())
            .expect_err("the mutated book must be rejected")
    }

    /// [`mutated`]'s twin for a rule only the **composition seam** can see: the book still validates
    /// on its own, and is then reconciled against the two tables it names ids — and band names —
    /// from.
    fn reconciled(mutate: impl FnOnce(&mut serde_json::Value)) -> RecipesConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
        mutate(&mut json);
        RecipesConfig::from_json_str(&json.to_string())
            .expect("the mutated book is still self-consistent")
            .validate_against(&MaterialsConfig::builtin(), &EquipmentConfig::builtin())
            .expect_err("the cross-config check must reject it")
    }

    /// The shipped book reconciled against the two tables it names ids from — the same call
    /// `build_headless_app` makes, run here so a broken cross-reference fails a unit test rather
    /// than a boot.
    #[test]
    fn the_builtin_book_parses_validates_and_reconciles() {
        let recipes = builtin();
        let materials = MaterialsConfig::builtin();
        recipes
            .validate_against(&materials, &EquipmentConfig::builtin())
            .expect("the shipped book must reconcile against the shipped tables");
        for (id, recipe) in recipes.recipes() {
            assert!(
                recipe.reads_axis().is_some(),
                "recipe '{id}' reads no characteristic - every shipped recipe makes equipment, and \
                 a graded output needs a reading"
            );
            assert!(
                recipe.grade_for(READING_MIN, &materials).is_some(),
                "recipe '{id}' leaves the bottom of the reading range ungraded"
            );
        }
    }

    /// **THE ANCHOR GRADE IS THE SHIPPED NUMBER**, and the anchor is *derived*: the band the recipe's
    /// bench material's bare-handed `hand_working.quality_ceiling` falls in. So a bare-handed craft
    /// off the best material that band can work by hand reproduces today's game exactly, and a tool
    /// run dry drops back to the shipped rate rather than into a spiral.
    ///
    /// **Both directions, plus a liveness assertion.** *"Every anchor grade agrees with its item"* is
    /// trivially true of a book whose grades declare nothing, so the rungs either side must be seen
    /// to genuinely bracket it — one strictly below and one strictly above, on the shipped book.
    #[test]
    fn the_anchor_grade_reproduces_the_shipped_item_and_the_others_bracket_it() {
        let equipment = EquipmentConfig::builtin();
        let materials = MaterialsConfig::builtin();
        let (mut saw_anchor, mut saw_below, mut saw_above) = (false, false, false);
        for (id, recipe) in builtin().recipes() {
            let Some(item) = recipe
                .outputs
                .iter()
                .find_map(|output| output.equipment_id())
                .and_then(|item| equipment.item(item))
            else {
                continue;
            };
            let anchor = anchor_band(recipe, &materials)
                .unwrap_or_else(|| panic!("recipe '{id}' works a hand-workable material"));
            let anchor_index = materials
                .band_index_of(anchor)
                .expect("the anchor is a declared band");
            for (index, name, grade) in recipe.grades_by_band(&materials) {
                for effect in &grade.effects {
                    let shipped = item
                        .default_tier()
                        .effects
                        .iter()
                        .find(|tier| tier.stat == effect.stat)
                        .map(|tier| tier.tier.value())
                        .unwrap_or_else(|| {
                            panic!("recipe '{id}' grade '{name}' declares a stat no tier declares")
                        });
                    let value = effect.tier.value();
                    match index.cmp(&anchor_index) {
                        std::cmp::Ordering::Equal => {
                            saw_anchor = true;
                            assert_eq!(
                                value, shipped,
                                "recipe '{id}' grade '{name}' is the anchor and must reproduce the \
                                 shipped item exactly"
                            );
                        }
                        std::cmp::Ordering::Less => {
                            saw_below = true;
                            assert!(
                                value < shipped,
                                "recipe '{id}' grade '{name}' sits below the anchor and must cost \
                                 something: {value} vs {shipped}"
                            );
                        }
                        std::cmp::Ordering::Greater => {
                            saw_above = true;
                            assert!(
                                value > shipped,
                                "recipe '{id}' grade '{name}' sits above the anchor and must buy \
                                 something: {value} vs {shipped}"
                            );
                        }
                    }
                }
            }
        }
        assert!(
            saw_anchor && saw_below && saw_above,
            "the shipped book must actually BRACKET its anchor - saw anchor {saw_anchor}, below \
             {saw_below}, above {saw_above}; without all three the agreement above is vacuous"
        );
    }

    /// **A reading is graded by the BAND it falls in** — the same table that rates the hide it was
    /// made from, with no seams of the recipe's own.
    #[test]
    fn a_reading_is_graded_by_the_band_it_falls_in() {
        let recipes = builtin();
        let materials = MaterialsConfig::builtin();
        let sled = recipes.recipe("sled").expect("the sled is shipped");
        for (reading, expected) in [
            (READING_MIN, "poor"),
            (0.29, "poor"),
            (0.30, "fair"),
            (0.55, "good"),
            (READING_MAX, "excellent"),
        ] {
            assert_eq!(
                sled.grade_for(reading, &materials),
                Some(expected),
                "a reading of {reading} is '{expected}' on the rail and on the sled alike"
            );
        }
    }

    /// **A BAND A RECIPE DOES NOT DECLARE INHERITS THE ONE BELOW IT.** Exercised by a fixture,
    /// because every shipped recipe either declares all four rungs or declares none — so nothing in
    /// the book reaches this rule, exactly as nothing reaches `materials.json`'s varieties or
    /// `equipment.json`'s bronze tier.
    #[test]
    fn a_band_a_recipe_does_not_declare_inherits_the_one_below_it() {
        let materials = MaterialsConfig::builtin();
        let equipment = EquipmentConfig::builtin();
        let recipes = RecipesConfig::from_json_str(
            r#"{
                "crafting": { "progress_per_worker_turn": 1.0 },
                "recipes": {
                    "spears": {
                        "display_name": "Spears",
                        "craft": "bone_working",
                        "work": 6.0,
                        "inputs": [ { "material": "bone", "amount": 1.0, "reads": "density" } ],
                        "outputs": [ { "equipment": "spears", "amount": 1.0 } ],
                        "grades": {
                            "poor": { "effects": [ { "stat": "attack", "equipped": 15.0 } ] },
                            "good": { "effects": [ { "stat": "attack", "equipped": 20.0 } ] }
                        }
                    }
                }
            }"#,
        )
        .expect("a two-rung ladder is a legal book");
        recipes
            .validate_against(&materials, &equipment)
            .expect("its lowest rung is the first band and its anchor is `good`");
        let spears = recipes.recipe("spears").expect("shipped in the fixture");
        let value_at = |band: &str| {
            spears
                .grade_effects_for(band, &materials)
                .iter()
                .map(|effect| effect.tier.value())
                .collect::<Vec<_>>()
        };
        assert_eq!(value_at("poor"), vec![15.0]);
        assert_eq!(
            value_at("fair"),
            vec![15.0],
            "an undeclared band takes the effects of the highest rung at or below it"
        );
        assert_eq!(value_at("good"), vec![20.0]);
        assert_eq!(
            value_at("excellent"),
            vec![20.0],
            "and inheritance only ever looks DOWN - there is nothing above `good` to reach for"
        );
        // **The stamp is the band regardless**, which is what makes the grade a property of the
        // object rather than of the recipe's authoring.
        assert_eq!(
            spears.grade_for(READING_MAX, &materials),
            Some("excellent"),
            "a craft off excellent bone reads `excellent` even where the recipe declares nothing \
             there"
        );
    }

    #[test]
    fn validate_rejects_an_empty_book() {
        let err = mutated(|json| json["recipes"] = serde_json::json!({}));
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("empty")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_non_positive_work() {
        let err = mutated(|json| json["recipes"]["sled"]["work"] = serde_json::json!(0.0));
        assert!(
            matches!(&err, RecipesConfigError::Invalid { field, .. } if field == "recipes.sled.work"),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_a_second_input_that_also_reads() {
        let err = mutated(|json| {
            let inputs = json["recipes"]["sled"]["inputs"]
                .as_array_mut()
                .expect("the sled has inputs");
            for input in inputs.iter_mut() {
                input["reads"] = serde_json::json!(input["material"]
                    .as_str()
                    .map(|m| if m == "hide" { "toughness" } else { "strength" })
                    .unwrap_or("strength"));
            }
        });
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("exactly one input")),
            "got {err}"
        );
    }

    /// **A GRADE KEY THAT IS NOT A BAND IS REJECTED** — the whole of *"one quality ladder"*, and a
    /// cross-config check because this table does not carry the rating vocabulary.
    #[test]
    fn validate_against_rejects_a_grade_key_that_is_not_a_band() {
        let err = reconciled(|json| {
            let grades = json["recipes"]["sled"]["grades"]
                .as_object_mut()
                .expect("the sled declares grades");
            let excellent = grades.remove("excellent").expect("the top rung");
            grades.insert("fine".to_string(), excellent);
        });
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("not a `characteristic_bands` rung")),
            "got {err}"
        );
    }

    /// **SOMETHING MUST ANSWER FOR A READING OF `0.0`.** Inheritance only ever looks down, so a
    /// ladder whose lowest rung is not the first band leaves the bottom of the range with nothing to
    /// inherit.
    #[test]
    fn validate_against_rejects_a_grade_ladder_that_does_not_open_at_the_first_band() {
        let err = reconciled(|json| {
            json["recipes"]["sled"]["grades"]
                .as_object_mut()
                .expect("the sled declares grades")
                .remove("poor");
        });
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("not the first one")),
            "got {err}"
        );
    }

    /// **THE ANCHOR IS DERIVED FROM THE BENCH MATERIAL'S BARE-HANDED CEILING**, so retuning that
    /// ceiling into a different band moves which rung has to reproduce the shipped item — and the
    /// book that agreed at `good` no longer agrees at `fair`.
    #[test]
    fn validate_against_rejects_an_anchor_grade_that_disagrees_with_the_shipped_item() {
        use crate::materials_config::BUILTIN_MATERIALS_CONFIG;

        let equipment = EquipmentConfig::builtin();
        let recipes = builtin();
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_MATERIALS_CONFIG).expect("the materials table is json");
        json["materials"]["hide"]["hand_working"]["quality_ceiling"] = serde_json::json!(0.4);
        let materials = MaterialsConfig::from_json_str(&json.to_string())
            .expect("a lower bare-handed ceiling is a legal table");
        let err = recipes
            .validate_against(&materials, &equipment)
            .expect_err("the anchor moved to `fair`, which is not the shipped sled");
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("anchor band 'fair'")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_grades_on_a_recipe_that_reads_nothing() {
        let err = mutated(|json| {
            let inputs = json["recipes"]["sled"]["inputs"]
                .as_array_mut()
                .expect("the sled has inputs");
            for input in inputs.iter_mut() {
                input.as_object_mut().expect("an input row").remove("reads");
            }
        });
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("reads no characteristic")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_an_output_naming_both_a_material_and_an_item() {
        let err = mutated(|json| {
            json["recipes"]["sled"]["outputs"][0]["material"] = serde_json::json!("hide");
        });
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("exactly one")),
            "got {err}"
        );
    }

    /// The cross-config rejections — the `UnknownItem` debt, paid at the composition seam.
    #[test]
    fn validate_against_rejects_an_unknown_material_item_or_axis() {
        let materials = MaterialsConfig::builtin();
        let equipment = EquipmentConfig::builtin();

        let rebuilt = |mutate: &dyn Fn(&mut serde_json::Value)| -> RecipesConfigError {
            let mut json: serde_json::Value =
                serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
            mutate(&mut json);
            let config =
                RecipesConfig::from_json_str(&json.to_string()).expect("still self-consistent");
            config
                .validate_against(&materials, &equipment)
                .expect_err("the cross-config check must reject it")
        };

        let unknown_material = rebuilt(&|json| {
            json["recipes"]["sled"]["inputs"][0]["material"] = serde_json::json!("unobtanium");
        });
        assert!(
            matches!(unknown_material, RecipesConfigError::UnknownMaterial { .. }),
            "got {unknown_material}"
        );

        let unknown_item = rebuilt(&|json| {
            json["recipes"]["sled"]["outputs"][0]["equipment"] = serde_json::json!("spearz");
        });
        assert!(
            matches!(unknown_item, RecipesConfigError::UnknownItem { .. }),
            "got {unknown_item}"
        );

        let unknown_axis = rebuilt(&|json| {
            json["recipes"]["sled"]["inputs"][0]["reads"] = serde_json::json!("sheen");
        });
        assert!(
            matches!(&unknown_axis, RecipesConfigError::InvalidBook { reason } if reason.contains("not an axis")),
            "got {unknown_axis}"
        );
    }

    /// **A required craft that none of the recipe's own inputs is worked by is rejected**, which is
    /// what makes *"gated on what it is MADE FROM"* structural. Broken here by demanding the very
    /// craft the tool unlocks — the deadlock the rule exists to forbid.
    #[test]
    fn validate_against_rejects_a_tool_gated_on_the_craft_it_unlocks() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
        json["recipes"]["loom"]["requires_knowledge"] = serde_json::json!(["weaving"]);
        let config =
            RecipesConfig::from_json_str(&json.to_string()).expect("still self-consistent");
        let err = config
            .validate_against(&MaterialsConfig::builtin(), &EquipmentConfig::builtin())
            .expect_err("a loom gated on weaving can never be built");
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("MADE FROM")),
            "got {err}"
        );
    }

    /// **A tool made from the material it bounds is rejected.** The other half of the same rule.
    #[test]
    fn validate_against_rejects_a_tool_made_from_the_material_it_bounds() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
        json["recipes"]["loom"]["inputs"] = serde_json::json!([
            { "material": "fibre", "amount": 4.0, "reads": "strength" }
        ]);
        json["recipes"]["loom"]["craft"] = serde_json::json!("weaving");
        json["recipes"]["loom"]["requires_knowledge"] = serde_json::json!([]);
        let config =
            RecipesConfig::from_json_str(&json.to_string()).expect("still self-consistent");
        let err = config
            .validate_against(&MaterialsConfig::builtin(), &EquipmentConfig::builtin())
            .expect_err("a loom made of fibre is the thing the rule forbids");
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("stretches")),
            "got {err}"
        );
    }

    /// **A recipe whose `craft` is not the craft of the material it works is rejected** — the
    /// one-home-per-fact guard on a field that exists only to be readable.
    #[test]
    fn validate_against_rejects_a_craft_that_is_not_the_worked_materials() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_RECIPES_CONFIG).expect("builtin parses as json");
        json["recipes"]["sled"]["craft"] = serde_json::json!("weaving");
        let config =
            RecipesConfig::from_json_str(&json.to_string()).expect("still self-consistent");
        let err = config
            .validate_against(&MaterialsConfig::builtin(), &EquipmentConfig::builtin())
            .expect_err("a hide recipe cannot practise weaving");
        assert!(
            matches!(&err, RecipesConfigError::InvalidBook { reason } if reason.contains("whose craft is")),
            "got {err}"
        );
    }

    /// **Varieties and material outputs are parsed and validated but nothing ships one**, exactly as
    /// the materials table treats varieties — so the alloy shape is exercised by a fixture rather
    /// than by a row nobody can reach.
    #[test]
    fn an_alloy_recipe_needs_no_reads_and_no_grades() {
        let materials = MaterialsConfig::from_json_str(
            r#"{
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
                            "tin":    { "hardness": 0.10, "working_temp": 0.05 }
                        }
                    }
                }
            }"#,
        )
        .expect("the materials fixture validates");
        let recipes = RecipesConfig::from_json_str(
            r#"{
                "crafting": { "progress_per_worker_turn": 1.0 },
                "recipes": {
                    "bronze": {
                        "display_name": "Bronze",
                        "craft": "smithing",
                        "work": 4.0,
                        "inputs": [
                            { "material": "metal", "variety": "copper", "amount": 9.0 }
                        ],
                        "outputs": [
                            { "material": "metal", "amount": 9.0,
                              "characteristics": { "hardness": 0.55, "working_temp": 0.30 } }
                        ]
                    }
                }
            }"#,
        )
        .expect("an alloy with no reads and no grades is legal");
        recipes
            .validate_against(&materials, &EquipmentConfig::builtin())
            .expect("the alloy reconciles");
        let bronze = recipes.recipe("bronze").expect("shipped in the fixture");
        assert_eq!(bronze.reads_axis(), None);
        assert_eq!(
            bronze.grade_for(READING_MAX, &materials),
            None,
            "a recipe that reads no characteristic resolves no grade - an alloy has no quality to \
             name"
        );
    }
}
