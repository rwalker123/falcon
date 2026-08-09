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
//! 2. **Continuous in, discrete out.** The reading selects a [`RecipeGrade`], and the grade declares
//!    **absolutes** ([`crate::equipment_config::EquipmentEffect`] names the value a stat *takes*).
//!    Nothing may scale a resolved stat by a quality number — there is no representation for it, and
//!    that is what keeps *flat until expiry, then a step down* structural.
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

/// **The seam the lowest grade must open at.** Every drawn reading has to select *some* grade or the
/// output would be unrepresentable, and the lowest therefore opens at the bottom of the reading
/// range rather than at whatever the author happened to type — the exact twin of
/// `materials_config`'s first-band rule.
const FIRST_GRADE_WHEN: f32 = READING_MIN;

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

/// **One discrete output quality**, selected by where the drawn reading falls.
///
/// `when` is the seam the continuous reading quantises at; `effects` is the set of **absolutes** the
/// output takes at this grade.
///
/// **`effects` may be empty, and every shipped grade's is.** A grade can only declare a stat whose
/// value does not already live somewhere else, and today every stat the shipped items own is homed:
/// `attack` on the item, the two carry rates in `labor_config.json`. Writing them here as well would
/// give a shipped number a second home to drift from — the exact mistake `equipment.json`'s
/// one-home-per-fact rule exists to prevent — while the numbers sat inert, which is
/// `config-loading.md`'s *"looks live but isn't"*. The grade **name** is live now (it is what the
/// draw selects and what the finished item carries); the payload lands with the quality-tier slice
/// that re-homes those rates onto the tier.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeGrade {
    /// The reading at which this grade opens. The lowest must be [`FIRST_GRADE_WHEN`] and no two may
    /// share a seam, so every reading selects exactly one grade.
    pub when: f32,
    /// The absolutes the output takes at this grade. See the type doc for why the shipped ones are
    /// empty.
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
    /// The discrete output qualities, by name. **The map's key order is irrelevant** — the seams are
    /// what order them ([`Self::grades_by_seam`]), and `validate` rejects two grades sharing a
    /// `when` so that order is total.
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

    /// **The grades in seam order, lowest first.** Total by construction: `validate` requires the
    /// lowest seam to be [`FIRST_GRADE_WHEN`] and no two to be equal.
    pub fn grades_by_seam(&self) -> Vec<(&str, &RecipeGrade)> {
        let mut ordered: Vec<(&str, &RecipeGrade)> = self
            .grades
            .iter()
            .map(|(name, grade)| (name.as_str(), grade))
            .collect();
        ordered.sort_by(|a, b| {
            a.1.when
                .partial_cmp(&b.1.when)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ordered
    }

    /// **Which grade a reading selects** — the highest seam the reading has reached. `None` for a
    /// recipe that declares no grades, which is a real answer: an alloy has no quality to name.
    pub fn grade_for(&self, reading: f32) -> Option<&str> {
        self.grades_by_seam()
            .into_iter()
            .rev()
            .find(|(_, grade)| reading >= grade.when)
            .map(|(name, _)| name)
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
        let ordered = recipe.grades_by_seam();
        if ordered.is_empty() {
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
        let (lowest_name, lowest) = ordered[0];
        if lowest.when != FIRST_GRADE_WHEN {
            return Err(RecipesConfigError::Invalid {
                field: format!("recipes.{id}.grades.{lowest_name}.when"),
                constraint: format!(
                    "be exactly {FIRST_GRADE_WHEN} on the lowest grade - every reading must select \
                     one"
                ),
                value: lowest.when.to_string(),
            });
        }
        let mut previous = lowest.when;
        for (name, grade) in &ordered[1..] {
            if !reading_in_range(grade.when) {
                return Err(RecipesConfigError::Invalid {
                    field: format!("recipes.{id}.grades.{name}.when"),
                    constraint: format!("be finite and within {READING_MIN}..={READING_MAX}"),
                    value: grade.when.to_string(),
                });
            }
            if grade.when <= previous {
                return Err(RecipesConfigError::Invalid {
                    field: format!("recipes.{id}.grades.{name}.when"),
                    constraint: format!(
                        "be strictly greater than the grade below it ({previous}) - two grades \
                         sharing a seam make the selection depend on map order"
                    ),
                    value: grade.when.to_string(),
                });
            }
            previous = grade.when;
        }
        for (name, grade) in &ordered {
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

    /// The shipped book reconciled against the two tables it names ids from — the same call
    /// `build_headless_app` makes, run here so a broken cross-reference fails a unit test rather
    /// than a boot.
    #[test]
    fn the_builtin_book_parses_validates_and_reconciles() {
        let recipes = builtin();
        recipes
            .validate_against(&MaterialsConfig::builtin(), &EquipmentConfig::builtin())
            .expect("the shipped book must reconcile against the shipped tables");
        for (id, recipe) in recipes.recipes() {
            assert!(
                recipe.reads_axis().is_some(),
                "recipe '{id}' reads no characteristic - every shipped recipe makes equipment, and \
                 a graded output needs a reading"
            );
            assert!(
                recipe.grade_for(READING_MIN).is_some(),
                "recipe '{id}' leaves the bottom of the reading range ungraded"
            );
        }
    }

    /// **Every shipped grade's payload is empty**, which is the deliberate state — see
    /// [`RecipeGrade`]. Pinned so the day someone writes a number in one, they have to come back
    /// here and say why it does not already live on the item.
    #[test]
    fn no_shipped_grade_declares_an_effect() {
        for (id, recipe) in builtin().recipes() {
            for (name, grade) in recipe.grades_by_seam() {
                assert!(
                    grade.effects.is_empty(),
                    "recipe '{id}' grade '{name}' declares effects - every stat the shipped items \
                     own is already homed, so this is a second home for a shipped number"
                );
            }
        }
    }

    /// The grade ladder is total and monotone: a reading anywhere in range selects exactly one, and
    /// a higher reading never selects a lower grade.
    #[test]
    fn a_reading_selects_the_highest_grade_it_has_reached() {
        let recipes = builtin();
        let sled = recipes.recipe("sled").expect("the sled is shipped");
        let seams = sled.grades_by_seam();
        assert!(
            seams.len() >= 2,
            "the sled must have a grade ladder to test"
        );
        let lowest = seams[0].0.to_string();
        let highest = seams[seams.len() - 1].0.to_string();
        assert_eq!(sled.grade_for(READING_MIN), Some(lowest.as_str()));
        assert_eq!(sled.grade_for(READING_MAX), Some(highest.as_str()));
        assert_eq!(
            sled.grade_for(seams[1].1.when - f32::EPSILON),
            Some(lowest.as_str()),
            "a reading just under a seam stays below it"
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

    #[test]
    fn validate_rejects_a_grade_ladder_that_does_not_open_at_the_bottom() {
        let err = mutated(|json| {
            json["recipes"]["sled"]["grades"]["coarse"]["when"] = serde_json::json!(0.1)
        });
        assert!(
            matches!(&err, RecipesConfigError::Invalid { field, .. } if field.starts_with("recipes.sled.grades.")),
            "got {err}"
        );
    }

    #[test]
    fn validate_rejects_two_grades_sharing_a_seam() {
        let err = mutated(|json| {
            json["recipes"]["sled"]["grades"]["fine"]["when"] = serde_json::json!(0.45);
        });
        assert!(
            matches!(&err, RecipesConfigError::Invalid { constraint, .. } if constraint.contains("strictly greater")),
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
        assert_eq!(bronze.grade_for(READING_MAX), None);
    }
}
