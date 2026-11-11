//! **MESSENGERS** (`docs/plan_far_kin.md` §3, `.claude/rules/core_sim/connections.md`) - a knowledge
//! a people learns by keeping far kin in touch.
//!
//! It is not a unit and has no verb. It is learned through [`LadderKnowledge::ledger_credit`], the
//! seam `ritual` and the crafts use, **once per people** (knowledge is faction-level, never scaled
//! by hands), by two practices:
//!
//! | Practice | Credit | Lever (`intensification_ladder.json` -> `knowledge`) |
//! |---|---|---|
//! | Each turn any resident band holds a live two-way tie with a kin band standing farther than the free pooling reach | a flat amount per turn, once per people | `messengers_lesson_per_turn` |
//! | A trade shipment or envoy arrives at a band of its OWN people | per tile walked, launch tile to arrival tile | `messengers_lesson_per_tile` |
//!
//! What it unlocks is a **value the stat takes** once the people knows it, never a multiplier, each
//! in the config that owns the stat: `supply_network_config.json` -> `reach_tiles_with_messengers`
//! (pooling only), `connections_config.json` -> `kin_relay.max_hops_with_messengers` and
//! `strength.decay_per_turn_with_messengers`.

use std::collections::BTreeMap;

use bevy::{math::UVec2, prelude::*};

use crate::{
    components::{BandId, PopulationCohort, ResidentBand, Tile},
    connections::{ConnectionLedger, NO_TIE},
    grid_utils::hex_distance_wrapped,
    intensification::{knows, LadderConfigHandle, LadderKnowledge},
    orders::FactionId,
    resources::{DiscoveryProgressLedger, SimulationConfig, TileRegistry},
    scalar::scalar_from_f32,
    supply_network_config::SupplyNetworkConfigHandle,
};

/// The messengers lesson's name in `intensification_ladder.json` (`knowledge.lesson_costs`,
/// `standalone_knowledges`) and in `start_profile_knowledge_tags.json`.
pub const MESSENGERS_KNOWLEDGE: &str = "messengers";

/// The discovery-ledger id of the messengers lesson: the id after `ritual`'s
/// ([`crate::belief::RITUAL_DISCOVERY_ID`], 2024).
pub const MESSENGERS_DISCOVERY_ID: u32 = 2025;

/// **Does `faction` hold the messengers lesson?** The ladder's one knowledge gate ([`knows`]) at its
/// completion threshold - the question every unlock asks.
pub fn knows_messengers(
    ledger: &DiscoveryProgressLedger,
    faction: FactionId,
    knowledge: &LadderKnowledge,
) -> bool {
    knows(
        ledger,
        faction,
        MESSENGERS_DISCOVERY_ID,
        knowledge.completion_threshold,
    )
}

/// **ANY PRACTICE TOWARD MESSENGERS, through the one seam** - `practice` units converted by
/// [`LadderKnowledge::ledger_credit`] and added to `faction`'s ledger. A practice at or below zero
/// credits nothing.
pub fn credit_messengers_practice(
    knowledge: &LadderKnowledge,
    faction: FactionId,
    practice: f32,
    ledger: &mut DiscoveryProgressLedger,
) {
    if practice <= 0.0 {
        return;
    }
    let Some(amount) = knowledge.ledger_credit(MESSENGERS_KNOWLEDGE, practice) else {
        return;
    };
    ledger.add_progress(faction, MESSENGERS_DISCOVERY_ID, scalar_from_f32(amount));
}

/// **A DELIVERY BETWEEN KIN TEACHES MESSENGERS** - `messengers_lesson_per_tile` x the hex distance
/// the party walked from its launch tile to its arrival tile, credited to `faction` (the party's
/// own people, which is also the receiver's).
pub fn credit_trip_lesson(
    knowledge: &LadderKnowledge,
    faction: FactionId,
    tiles_walked: u32,
    ledger: &mut DiscoveryProgressLedger,
) {
    credit_messengers_practice(
        knowledge,
        faction,
        knowledge.messengers_lesson_per_tile * tiles_walked as f32,
        ledger,
    );
}

/// **Every people with a live two-way tie to a kin band standing farther than `reach_tiles`** - the
/// peoples the passive lesson credits. `bands` is each resident band's people and standing tile.
/// Pure, so the rule is unit-pinned without a world; a people appears once however many pairs
/// qualify.
pub fn peoples_in_far_touch(
    ledger: &ConnectionLedger,
    bands: &BTreeMap<BandId, (FactionId, UVec2)>,
    reach_tiles: u32,
    width: u32,
    wrap: bool,
) -> std::collections::BTreeSet<FactionId> {
    let mut peoples = std::collections::BTreeSet::new();
    for (key, edge) in ledger.iter() {
        // One direction is enough to find the pair; the other is read by the two-way test.
        if edge.strength <= NO_TIE || key.observer >= key.subject {
            continue;
        }
        let (Some((from, at)), Some((to, there))) =
            (bands.get(&key.observer), bands.get(&key.subject))
        else {
            continue;
        };
        if from != to || peoples.contains(from) {
            continue;
        }
        if hex_distance_wrapped(*at, *there, width, wrap) > reach_tiles
            && ledger.tied_both_ways(key.observer, key.subject)
        {
            peoples.insert(*from);
        }
    }
    peoples
}

/// **PASSIVE CONTACT TEACHES MESSENGERS** - each turn every people in [`peoples_in_far_touch`] is
/// credited `messengers_lesson_per_turn`, once per people. Runs in the Visibility stage behind the
/// knowledge rider: the ties it reads are the ones this turn's sight sweep and kin relay just
/// refreshed. `reach_tiles` is the SHIPPED free reach, never the messengers one.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn credit_far_kin_contact(
    ledger: Res<ConnectionLedger>,
    supply: Res<SupplyNetworkConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    cohorts: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
) {
    let ladder = ladder.get();
    let per_turn = ladder.knowledge.messengers_lesson_per_turn;
    if per_turn <= 0.0 || ledger.is_empty() {
        return;
    }
    let bands: BTreeMap<BandId, (FactionId, UVec2)> = cohorts
        .iter()
        .filter_map(|(id, cohort)| {
            let position = tiles.get(cohort.current_tile).ok()?.position;
            Some((*id, (cohort.faction, position)))
        })
        .collect();
    let reach = supply.get().reach_tiles;
    let wrap = sim_config.map_topology.wrap_horizontal;
    for faction in peoples_in_far_touch(&ledger, &bands, reach, tile_registry.width, wrap) {
        credit_messengers_practice(&ladder.knowledge, faction, per_turn, &mut discovery);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::{ConnectionKey, Sighting};

    const KIN: FactionId = FactionId(0);
    const WIDTH: u32 = 64;
    const REACH: u32 = 3;

    fn tie_both_ways(ledger: &mut ConnectionLedger, a: u64, b: u64) {
        for (from, to) in [(a, b), (b, a)] {
            ledger.insert_full_tie(
                ConnectionKey::new(BandId(from), BandId(to)),
                &Sighting::new(UVec2::ZERO, 1, "", KIN),
                1,
            );
        }
    }

    fn standing(entries: &[(u64, FactionId, u32)]) -> BTreeMap<BandId, (FactionId, UVec2)> {
        entries
            .iter()
            .map(|(id, faction, x)| (BandId(*id), (*faction, UVec2::new(*x, 2))))
            .collect()
    }

    #[test]
    fn a_far_two_way_tie_of_one_people_counts_and_a_near_one_does_not() {
        let mut ledger = ConnectionLedger::default();
        tie_both_ways(&mut ledger, 1, 2);
        let far = standing(&[(1, KIN, 2), (2, KIN, 2 + REACH + 1)]);
        let near = standing(&[(1, KIN, 2), (2, KIN, 2 + REACH)]);
        assert_eq!(
            peoples_in_far_touch(&ledger, &far, REACH, WIDTH, false),
            [KIN].into()
        );
        assert!(peoples_in_far_touch(&ledger, &near, REACH, WIDTH, false).is_empty());
    }

    #[test]
    fn a_one_way_tie_or_another_peoples_band_does_not_count() {
        let mut one_way = ConnectionLedger::default();
        one_way.insert_full_tie(
            ConnectionKey::new(BandId(1), BandId(2)),
            &Sighting::new(UVec2::ZERO, 1, "", KIN),
            1,
        );
        let far = standing(&[(1, KIN, 2), (2, KIN, 20)]);
        assert!(peoples_in_far_touch(&one_way, &far, REACH, WIDTH, false).is_empty());

        let mut two_way = ConnectionLedger::default();
        tie_both_ways(&mut two_way, 1, 2);
        let strangers = standing(&[(1, KIN, 2), (2, FactionId(1), 20)]);
        assert!(peoples_in_far_touch(&two_way, &strangers, REACH, WIDTH, false).is_empty());
    }

    #[test]
    fn a_people_is_counted_once_however_many_far_pairs_it_holds() {
        let mut ledger = ConnectionLedger::default();
        tie_both_ways(&mut ledger, 1, 2);
        tie_both_ways(&mut ledger, 1, 3);
        let far = standing(&[(1, KIN, 2), (2, KIN, 20), (3, KIN, 30)]);
        assert_eq!(
            peoples_in_far_touch(&ledger, &far, REACH, WIDTH, false).len(),
            1
        );
    }
}
