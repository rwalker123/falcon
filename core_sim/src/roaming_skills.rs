//! **THE ROAMING SKILLS** (#799, `docs/plan_roaming_bands.md` §"The roaming skills — decided") —
//! three faction knowledges a nomadic people learns by moving: **herd lore**, **travois** and
//! **drives**.
//!
//! They are not ladder rungs: nothing is built or held, so no rung's `earns_knowledge` teaches
//! them. Each has its own **practice signal**, the way `storage` is taught by rot
//! ([`crate::spoilage::storage_practice`]), and all three are priced in `lesson_costs` and credited
//! through `LadderKnowledge::ledger_credit` — the one seam that turns practice units into ledger
//! progress.
//!
//! | Skill | Taught by | Practice quantum | Effect |
//! |---|---|---|---|
//! | [`HERD_LORE`] | each hex a band steps while following a herd on a `Migrate` leg | `learn_rate` per hex | the viewer sees a migratory herd's next seasonal ground ([`next_ground`]) |
//! | [`TRAVOIS`] | the share of the load a long move sheds | `learn_rate × shed share`, once per departure | `carry.travois_carry_multiplier` on `per_worker_carry` |
//! | [`DRIVES`] | each turn a crew works a followed herd as a camp kill | `learn_rate` per turn | engage-rate gain growing with head count ([`drive_engage_multiplier`]) |
//!
//! They sit on the knowledge roster under the config's `standalone_knowledges` (branch `roam`, area
//! `reach`), so the client draws them with no list of its own.

use bevy::math::UVec2;

use crate::fauna::{Herd, RoamState};
use crate::intensification::{knows, LadderConfig, LadderKnowledge};
use crate::orders::FactionId;
use crate::resources::DiscoveryProgressLedger;
use crate::scalar::scalar_from_f32;

/// The herd-lore lesson's name in `intensification_ladder.json` and `start_profile_knowledge_tags.json`.
pub const HERD_LORE: &str = "herd_lore";
/// The travois lesson's name.
pub const TRAVOIS: &str = "travois";
/// The drives lesson's name.
pub const DRIVES: &str = "drives";

/// Every roaming skill — what `LadderConfig::validate` requires a price for.
pub const ROAMING_SKILLS: [&str; 3] = [HERD_LORE, TRAVOIS, DRIVES];

/// The three discovery ids in [`ROAMING_SKILLS`] order - what the mobility score reads progress off.
pub const ROAMING_SKILL_DISCOVERY_IDS: [u32; 3] = [
    HERD_LORE_DISCOVERY_ID,
    TRAVOIS_DISCOVERY_ID,
    DRIVES_DISCOVERY_ID,
];

/// Discovery-ledger ids. 2011 is retired, 2018 is storage; these continue the sequence.
pub const HERD_LORE_DISCOVERY_ID: u32 = 2019;
pub const TRAVOIS_DISCOVERY_ID: u32 = 2020;
pub const DRIVES_DISCOVERY_ID: u32 = 2021;

/// **One hex walked beside a migrating herd** — the unit herd-lore practice is counted in. A band
/// moves several hexes a turn, and each is a hex of the herd's leg learned.
const PRACTICE_PER_HEX: f32 = 1.0;

/// **One turn a crew works a followed herd as a camp kill** — the unit drives practice is counted
/// in. Credited once per source per turn, like every ladder lesson, never scaled by the crew.
const PRACTICE_PER_DRIVE_TURN: f32 = 1.0;

/// **Whether a band's step beside this herd is a step beside a MIGRATION** - the herd is on a
/// `Migrate` leg. One predicate for the two things that count such a step: herd-lore practice
/// ([`herd_lore_practice`]) and the mobility score's distance input
/// ([`crate::mobility::HerdFollowSteps`]). A herd that loiters counts for neither.
pub fn walks_beside_a_migration(herd: &Herd) -> bool {
    matches!(herd.roam, RoamState::Migrate)
}

/// The practice a band's walk beside a herd is worth: `learn_rate` per hex stepped, and only when
/// the herd is on a `Migrate` leg — a herd that loiters teaches the band nothing about where it
/// goes next.
pub fn herd_lore_practice(hexes_stepped: u32, herd: &Herd, knowledge: &LadderKnowledge) -> f32 {
    if hexes_stepped == 0 || !walks_beside_a_migration(herd) {
        return 0.0;
    }
    knowledge.learn_rate * PRACTICE_PER_HEX * hexes_stepped as f32
}

/// The practice a departure's shed is worth: `learn_rate` times the share of the load left behind
/// (`carry::shed_share`), clamped to `0..=1`. A move that sheds nothing teaches nothing.
pub fn travois_practice(shed_share: f32, knowledge: &LadderKnowledge) -> f32 {
    if !shed_share.is_finite() || shed_share <= 0.0 {
        return 0.0;
    }
    knowledge.learn_rate * shed_share.min(1.0)
}

/// The practice one drive turn is worth: `learn_rate` per turn a crew works the followed herd as a
/// camp kill.
pub fn drives_practice(knowledge: &LadderKnowledge) -> f32 {
    knowledge.learn_rate * PRACTICE_PER_DRIVE_TURN
}

/// Credit `practice` of `name` to `faction` through the ledger seam. A no-op for zero practice.
pub fn credit(
    ledger: &mut DiscoveryProgressLedger,
    ladder: &LadderConfig,
    faction: FactionId,
    name: &str,
    discovery_id: u32,
    practice: f32,
) {
    if practice <= 0.0 {
        return;
    }
    if let Some(amount) = ladder.knowledge.ledger_credit(name, practice) {
        if amount > 0.0 {
            ledger.add_progress(faction, discovery_id, scalar_from_f32(amount));
        }
    }
}

/// Whether `faction` has learned the lesson with `discovery_id` — progress at or past the ladder's
/// `completion_threshold`, the same bar every ladder gate reads.
pub fn faction_knows(
    ledger: &DiscoveryProgressLedger,
    ladder: &LadderConfig,
    faction: FactionId,
    discovery_id: u32,
) -> bool {
    knows(
        ledger,
        faction,
        discovery_id,
        ladder.knowledge.completion_threshold,
    )
}

/// [`faction_knows`] for a caller holding only the `World` (a query or a command): reads the
/// ledger and the ladder off it. A world with no ledger knows nothing.
pub fn world_faction_knows(
    world: &bevy::prelude::World,
    faction: FactionId,
    discovery_id: u32,
) -> bool {
    let Some(ledger) = world.get_resource::<DiscoveryProgressLedger>() else {
        return false;
    };
    let ladder = world
        .resource::<crate::intensification::LadderConfigHandle>()
        .get();
    faction_knows(ledger, &ladder, faction, discovery_id)
}

/// **The grounds a migratory herd goes to next** — `route[(step_index + 1) % len]`.
///
/// The same anchor in both phases, which is the choice this function documents: `step_index` is
/// the anchor the herd is AT while it loiters and the anchor it LEFT while it migrates (arrival
/// advances it, [`crate::fauna`]'s `Migrate` arm), so `+1` is the ground it leaves for after the
/// current loiter, and — mid-leg — the destination it is already walking to. `None` for a herd
/// that does not migrate (graze-wander cycles a local cluster, not a season), and for a route with
/// fewer than two anchors, where the next ground is the one it stands on.
pub fn next_ground(herd: &Herd) -> Option<UVec2> {
    if matches!(herd.roam, RoamState::GrazeWander) {
        return None;
    }
    if herd.route.len() < MIN_ANCHORS_FOR_A_NEXT_GROUND {
        return None;
    }
    herd.route
        .get((herd.step_index + 1) % herd.route.len())
        .copied()
}

/// A route with one anchor is a haul-out, not a migration: nowhere else to go.
const MIN_ANCHORS_FOR_A_NEXT_GROUND: usize = 2;

/// **The engage-rate multiplier a drive buys**: `1 + gain × min(head_count / full_head_count, 1)`.
/// Grows with the herd's head count and caps at the full-drive count; `1.0` (the identity) when the
/// faction does not know drives, the herd is empty, or either lever makes no gain.
pub fn drive_engage_multiplier(
    drives_known: bool,
    head_count: f32,
    gain: f32,
    full_head_count: f32,
) -> f32 {
    let usable = [head_count, full_head_count, gain]
        .iter()
        .all(|value| value.is_finite() && *value > 0.0);
    if !drives_known || !usable {
        return 1.0;
    }
    1.0 + gain * (head_count / full_head_count).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fauna_config::SizeClass;

    const HEAD: f32 = 1.0;
    const GAIN: f32 = 0.5;
    const FULL: f32 = 100.0;
    const SAME: f32 = 1e-6;

    fn knowledge() -> LadderKnowledge {
        LadderConfig::builtin().knowledge.clone()
    }

    fn herd(roam: RoamState, route: &[UVec2], step_index: usize) -> Herd {
        let mut herd = Herd::new(
            "probe".to_string(),
            "Wild Reindeer".to_string(),
            SizeClass::Migratory,
            route.to_vec(),
            100.0,
            100.0,
            0.0,
            0.04,
            20.0,
        );
        herd.roam = roam;
        herd.step_index = step_index;
        herd
    }

    #[test]
    fn herd_lore_is_paid_per_hex_and_only_beside_a_herd_on_a_migrate_leg() {
        let knowledge = knowledge();
        let route = [UVec2::new(0, 0), UVec2::new(9, 0)];
        let migrating = herd(RoamState::Migrate, &route, 0);
        let loitering = herd(RoamState::Loiter { turns_left: 4 }, &route, 0);
        assert!(
            (herd_lore_practice(3, &migrating, &knowledge) - 3.0 * knowledge.learn_rate).abs()
                < SAME
        );
        assert_eq!(herd_lore_practice(0, &migrating, &knowledge), 0.0);
        assert_eq!(herd_lore_practice(3, &loitering, &knowledge), 0.0);
    }

    #[test]
    fn travois_is_paid_in_proportion_to_the_share_shed_and_clamped() {
        let knowledge = knowledge();
        assert_eq!(travois_practice(0.0, &knowledge), 0.0);
        assert_eq!(travois_practice(f32::NAN, &knowledge), 0.0);
        assert!((travois_practice(0.25, &knowledge) - 0.25 * knowledge.learn_rate).abs() < SAME);
        assert!((travois_practice(7.0, &knowledge) - knowledge.learn_rate).abs() < SAME);
    }

    #[test]
    fn a_practice_credit_divides_by_the_lessons_cost_and_lands_on_the_ledger() {
        let ladder = LadderConfig::builtin();
        let mut ledger = DiscoveryProgressLedger::default();
        let faction = FactionId(0);
        credit(
            &mut ledger,
            &ladder,
            faction,
            DRIVES,
            DRIVES_DISCOVERY_ID,
            drives_practice(&ladder.knowledge),
        );
        let cost = ladder
            .knowledge
            .lesson_cost(DRIVES)
            .expect("drives is priced");
        assert!(
            (ledger.get_progress(faction, DRIVES_DISCOVERY_ID).to_f32()
                - ladder.knowledge.learn_rate / cost)
                .abs()
                < 1e-4
        );
        credit(
            &mut ledger,
            &ladder,
            faction,
            DRIVES,
            DRIVES_DISCOVERY_ID,
            0.0,
        );
        assert!(!faction_knows(
            &ledger,
            &ladder,
            faction,
            DRIVES_DISCOVERY_ID
        ));
    }

    #[test]
    fn every_roaming_skill_is_priced_and_has_a_distinct_id() {
        let ladder = LadderConfig::builtin();
        let mut ids = vec![
            HERD_LORE_DISCOVERY_ID,
            TRAVOIS_DISCOVERY_ID,
            DRIVES_DISCOVERY_ID,
        ];
        for name in ROAMING_SKILLS {
            assert!(
                ladder.knowledge.lesson_cost(name).is_some(),
                "{name} is priced"
            );
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ROAMING_SKILLS.len());
    }

    #[test]
    fn the_next_ground_is_the_next_route_anchor_and_wraps() {
        let route = [UVec2::new(0, 0), UVec2::new(5, 0), UVec2::new(9, 0)];
        let loitering = herd(RoamState::Loiter { turns_left: 4 }, &route, 0);
        let migrating = herd(RoamState::Migrate, &route, 0);
        assert_eq!(next_ground(&loitering), Some(route[1]));
        assert_eq!(next_ground(&migrating), Some(route[1]));
        assert_eq!(
            next_ground(&herd(RoamState::Migrate, &route, 2)),
            Some(route[0])
        );
        assert_eq!(next_ground(&herd(RoamState::GrazeWander, &route, 0)), None);
        assert_eq!(next_ground(&herd(RoamState::Migrate, &route[..1], 0)), None);
    }

    #[test]
    fn a_drive_is_an_identity_until_known_and_grows_to_a_cap() {
        assert_eq!(drive_engage_multiplier(false, FULL, GAIN, FULL), 1.0);
        assert_eq!(drive_engage_multiplier(true, 0.0, GAIN, FULL), 1.0);
        assert_eq!(drive_engage_multiplier(true, FULL, 0.0, FULL), 1.0);
        let half = drive_engage_multiplier(true, FULL / 2.0, GAIN, FULL);
        let full = drive_engage_multiplier(true, FULL, GAIN, FULL);
        assert!((half - (1.0 + GAIN / 2.0)).abs() < SAME);
        assert!((full - (1.0 + GAIN)).abs() < SAME);
        assert_eq!(drive_engage_multiplier(true, FULL * 5.0, GAIN, FULL), full);
        assert!(drive_engage_multiplier(true, HEAD, GAIN, FULL) > 1.0);
    }
}
