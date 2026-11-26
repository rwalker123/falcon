//! **The `bench` section of a scenario** (`docs/plan_scenarios.md` sections 2.2, 2.5, 2.8, 2.9): a
//! faction's bench queue, stated as the bench commands that build it.
//!
//! An entry is the `sim_runtime::CommandPayload` variant in snake_case, externally tagged, with the
//! faction implied by the section and the band named by its scenario id. Pass 1 ([`check`]) resolves
//! names only. [`apply`] is step 6 of the apply order: it replays the entries, in file order, through
//! the same `command_seam` bodies the server's handlers run, so a refusal is the handler's own
//! sentence. **The first refusal stops the load**, because a later entry may depend on an earlier one.
//! A queue is stated whole: the verbs that edit an existing queue are not entries.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::Deserialize;

use super::{FactionSection, ScenarioContext, ScenarioRefusal, ScenarioSetup};
use crate::command_seam::{apply_bench_enqueue, apply_bench_enqueue_kit};

/// One bench command. Field names match `sim_runtime::CommandPayload`'s; every field is required.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BenchEntry {
    /// `bench_enqueue`: queue `count` of one recipe.
    BenchEnqueue {
        #[schemars(extend("x-pick" = "band"))]
        band: String,
        recipe_id: String,
        count: u32,
    },
    /// `bench_enqueue_kit`: queue `kits` of a whole kit as one grouped order, one recipe per item.
    BenchEnqueueKit {
        #[schemars(extend("x-pick" = "band"))]
        band: String,
        kit_id: String,
        kits: u32,
        recipes: Vec<String>,
    },
}

impl BenchEntry {
    fn band(&self) -> &str {
        match self {
            BenchEntry::BenchEnqueue { band, .. } | BenchEntry::BenchEnqueueKit { band, .. } => {
                band
            }
        }
    }
}

/// **Pass 1: names only, every refusal collected.** Knowledge, counts and kit coverage are the
/// handler's to judge in step 6, when knowledge is seeded.
pub(super) fn check(
    factions: &[FactionSection],
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (i, section) in factions.iter().enumerate() {
        let band_ids: BTreeSet<&str> = section.bands.iter().map(|band| band.id.as_str()).collect();
        for (k, entry) in section.bench.iter().enumerate() {
            let at = format!("factions[{i}].bench[{k}]");
            if !band_ids.contains(entry.band()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.band"),
                    format!("no band '{}' in this faction", entry.band()),
                ));
            }
            match entry {
                BenchEntry::BenchEnqueue { recipe_id, .. } => {
                    check_recipe(recipe_id, &format!("{at}.recipe_id"), ctx, refusals);
                }
                BenchEntry::BenchEnqueueKit {
                    kit_id, recipes, ..
                } => {
                    if ctx.equipment.orderable_kit(kit_id).is_none() {
                        refusals.push(ScenarioRefusal::new(
                            format!("{at}.kit_id"),
                            format!("'{kit_id}' is not a kit that can be ordered"),
                        ));
                    }
                    for (r, recipe_id) in recipes.iter().enumerate() {
                        check_recipe(recipe_id, &format!("{at}.recipes[{r}]"), ctx, refusals);
                    }
                }
            }
        }
    }
}

fn check_recipe(
    recipe_id: &str,
    at: &str,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    if ctx.recipes.recipe(recipe_id).is_none() {
        refusals.push(ScenarioRefusal::new(
            at,
            format!("'{recipe_id}' is not a recipe"),
        ));
    }
}

/// **Step 6: replay every faction's bench entries, in faction order then file order.** The first
/// refusal is returned and nothing after it is applied.
pub(super) fn apply(world: &mut World, setup: &ScenarioSetup) -> Result<(), ScenarioRefusal> {
    for faction_setup in &setup.factions {
        let faction = faction_setup.faction;
        for (k, entry) in faction_setup.bench.iter().enumerate() {
            let at = format!("factions[{}].bench[{k}]", faction_setup.file_index);
            let Some(band) = setup.spawned.get(entry.band()).copied() else {
                return Err(ScenarioRefusal::new(
                    at,
                    format!("band '{}' was not placed", entry.band()),
                ));
            };
            let outcome = match entry {
                BenchEntry::BenchEnqueue {
                    recipe_id, count, ..
                } => apply_bench_enqueue(world, faction, band, recipe_id, *count).map(|_| ()),
                BenchEntry::BenchEnqueueKit {
                    kit_id,
                    kits,
                    recipes,
                    ..
                } => apply_bench_enqueue_kit(world, faction, band, kit_id, *kits, recipes)
                    .map(|_| ()),
            };
            if let Err(refusal) = outcome {
                return Err(ScenarioRefusal::new(at, refusal.to_string()));
            }
        }
    }
    Ok(())
}
