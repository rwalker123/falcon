//! **The crafts — one knowledge track per material** (`docs/plan_crafting_and_materials.md` §5).
//!
//! Hide → Tanning, Fibre → Weaving, Bone → Bone-working. They sit in the faction's
//! [`crate::resources::DiscoveryProgressLedger`] beside the intensification ladder's five, and they
//! are earned the way every other knowledge in this game is: **by doing the thing**.
//!
//! | | scope | lifetime | answers |
//! |---|---|---|---|
//! | **Knowledge** (here) | faction-wide | permanent | *what can be made at all* |
//! | **Tools** ([`crate::equipment_config`]) | band-local | consumable, worn per use | *how well this band makes it* |
//!
//! **Crafting is the fourth teacher.** Hunting teaches Herding and Penning, foraging teaches
//! Cultivation and Seed Selection, keeping a pen teaches Foddering — and crafting teaches its own
//! crafts. **The lesson is charged on the SAME quantum as the tool's wear: per item completed**
//! ([`crate::systems::advance_crafting`]), so the thing that consumes the tool and the thing that
//! teaches the craft cannot drift apart. What is being made decides what is learned.
//!
//! **None ships known**, and none is start-grantable — the same rule the ladder's five follow. A
//! band learns Tanning by tanning, bare-handed, which is what makes *"tools are earned, never a
//! prerequisite"* true rather than aspirational.

use crate::materials_config::MaterialsConfig;

/// Discovery id for the faction-level **Tanning** knowledge — the craft of `hide`.
///
/// **Earned by tanning**: every item completed on a bench whose recipe's craft is `tanning` credits
/// one lesson. Declared as a start-profile knowledge tag (`tanning` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable, and deliberately **not**
/// listed in any start profile's `starting_knowledge_tags`. Next free id after `foddering` (2007).
pub const TANNING_DISCOVERY_ID: u32 = 2008;

/// Discovery id for the faction-level **Weaving** knowledge — the craft of `fibre`. See
/// [`TANNING_DISCOVERY_ID`]; next free id after it.
pub const WEAVING_DISCOVERY_ID: u32 = 2009;

/// Discovery id for the faction-level **Bone-working** knowledge — the craft of `bone`. See
/// [`TANNING_DISCOVERY_ID`]; next free id after [`WEAVING_DISCOVERY_ID`].
pub const BONE_WORKING_DISCOVERY_ID: u32 = 2010;

/// The craft name `materials.json` spells for [`TANNING_DISCOVERY_ID`]. Named here so the sim never
/// spells a craft outside this module and a test fixture.
pub const TANNING_CRAFT: &str = "tanning";
/// See [`TANNING_CRAFT`].
pub const WEAVING_CRAFT: &str = "weaving";
/// See [`TANNING_CRAFT`].
pub const BONE_WORKING_CRAFT: &str = "bone_working";

/// **The craft ids the sim has a discovery for** — the bounded coded set, exactly as
/// `intensification::discovery_id_for` is for the ladder's knowledge names. A craft a material
/// declares but this cannot resolve is a config typo that would silently make every recipe on it
/// ungateable and unteachable, so both `materials.json` and `recipes.json` are reconciled against it
/// at load.
pub fn craft_discovery_id(craft: &str) -> Option<u32> {
    match craft {
        TANNING_CRAFT => Some(TANNING_DISCOVERY_ID),
        WEAVING_CRAFT => Some(WEAVING_DISCOVERY_ID),
        BONE_WORKING_CRAFT => Some(BONE_WORKING_DISCOVERY_ID),
        _ => None,
    }
}

/// **Every craft the loaded materials table declares, in material-id order.** The candidate set a
/// recipe's `craft` and `requires_knowledge` are validated against — asked of the table rather than
/// listed here a second time, so a material retired from the roster retires its craft with it.
pub fn crafts_declared_by(materials: &MaterialsConfig) -> Vec<&str> {
    let mut crafts: Vec<&str> = materials
        .materials()
        .map(|(_, def)| def.craft.as_str())
        .collect();
    crafts.sort_unstable();
    crafts.dedup();
    crafts
}

/// **How an id becomes the word a player reads** — underscores become hyphens and the first letter
/// is capitalized, so `bone_working` reads *Bone-working* and `clay_working` reads *Clay-working*.
///
/// **Resolved sim-side, and derived rather than authored.** The panel's refusals are published
/// strings (*"Needs Clay-working"*), so something has to turn an id into English — and a client that
/// did it would be a second copy of this rule that a new craft would not update. A per-craft
/// `display_name` in config was rejected for the same reason `RecipeDef::craft` is derived: the id
/// already says it, and a second spelling drifts.
///
/// Used for a **craft** and for a **material** alike (`hide` → *Hide*), which is why it is named for
/// neither.
pub fn title_from_id(id: &str) -> String {
    let hyphenated = id.replace('_', "-");
    let mut chars = hyphenated.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// **What working a material bare-handed spends of the recipe's inputs: all of it.**
///
/// The tool's `craft_material_efficiency` is the *fraction actually consumed*, so the no-tool answer
/// is the identity — a bench with nothing on it saves nothing, and the recipe's stated amount is
/// what it stated. Named rather than inlined because it is the fourth bare-handed reading beside
/// [`crate::materials_config::HandWorking`]'s two, and the one the material has no reason to author:
/// there is no *"how efficiently can this be worked by hand"* — by hand is the baseline.
pub const HAND_WORKING_MATERIAL_EFFICIENCY: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The three craft ids are distinct and sit above the ladder's five — a collision would make two
    /// knowledges the same ledger row, which is the one failure a `u32` id cannot report.
    #[test]
    fn the_three_crafts_have_distinct_ids_above_the_ladders() {
        let ids = [
            TANNING_DISCOVERY_ID,
            WEAVING_DISCOVERY_ID,
            BONE_WORKING_DISCOVERY_ID,
        ];
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "the craft ids must be distinct");
        assert!(
            sorted[0] > crate::fauna::FODDERING_DISCOVERY_ID,
            "a craft id must not collide with the ladder's five"
        );
    }

    /// **Every craft the shipped table declares resolves to a discovery**, which is what the loader
    /// enforces — stated here against the builtin so the roster and the coded set cannot part.
    #[test]
    fn every_shipped_material_names_a_craft_the_sim_has_a_discovery_for() {
        let materials = MaterialsConfig::builtin();
        for craft in crafts_declared_by(&materials) {
            assert!(
                craft_discovery_id(craft).is_some(),
                "material craft '{craft}' has no discovery id"
            );
        }
        assert_eq!(
            crafts_declared_by(&materials),
            vec![BONE_WORKING_CRAFT, TANNING_CRAFT, WEAVING_CRAFT],
            "the shipped table declares exactly the three organic crafts"
        );
    }
}
