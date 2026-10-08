//! **Contact between two peoples merges their founding lines** (issue #689,
//! `docs/plan_civilization_steps.md` §"The mechanism: an isolated people cannot grow past its
//! lines"). Rationale: `.claude/rules/core_sim/campaign.md` §"Founding lines".
//!
//! A band's [`FoundingLines`] only ever shrinks by split; this is the only thing that grows them.
//! Meeting another people is how a closed gene pool opens: each side gains the lines the other
//! holds, which raises its breeding ceiling (`resolve_breeding_ceilings` counts a line's holders
//! per people, so the copy does not divide anyone's `K`).
//!
//! **The signal is the ledger's own.** "Contact this turn" is every edge whose
//! `last_contact_turn == SimulationTick.0` — the documented contact-this-turn stamp. There is no
//! second notion of contact here, so an expedition's flushed sighting counts, as it does for the
//! tie. `connections.rs` stays faction-blind; the faction check lives only in this system.
//!
//! **Order-independent and non-transitive within a turn.** Every involved band's line set is
//! snapshotted before anything merges, and each band gains the union of its cross-faction
//! contacts' *pre-merge* sets: with A–B and B–C in contact on one turn, C does not get A's lines
//! until the next turn. Walks are over [`BTreeMap`]/[`BTreeSet`] in [`BandId`] order — the sim is
//! seeded. Lines never decay: nothing here or elsewhere removes a gained line when the tie bleeds.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use crate::{
    components::{BandId, PopulationCohort, ResidentBand},
    connections::ConnectionLedger,
    lineage::FoundingLines,
    orders::FactionId,
    resources::SimulationTick,
};

/// Fold this turn's cross-people contacts into the bands' founding lines.
///
/// Scheduled in `TurnStage::Visibility` directly after `connections::advance_connections`, which
/// stamps `last_contact_turn`. `TurnStage::Population` (where the ceiling is resolved) runs
/// *before* Visibility, so a merge lifts the ceiling on the **next** turn's population pass.
pub fn merge_founding_lines_on_contact(
    ledger: Res<ConnectionLedger>,
    tick: Res<SimulationTick>,
    mut bands: Query<(&BandId, &mut PopulationCohort), With<ResidentBand>>,
) {
    let snapshot: BTreeMap<BandId, (FactionId, FoundingLines)> = bands
        .iter()
        .map(|(id, cohort)| (*id, (cohort.faction, cohort.founding_lines.clone())))
        .collect();
    // Cross-people contacts this turn, undirected: band -> the bands it met.
    let mut met: BTreeMap<BandId, BTreeSet<BandId>> = BTreeMap::new();
    for (key, connection) in ledger.iter() {
        if connection.last_contact_turn != tick.0 {
            continue;
        }
        let (Some((faction_a, _)), Some((faction_b, _))) =
            (snapshot.get(&key.observer), snapshot.get(&key.subject))
        else {
            continue;
        };
        if faction_a == faction_b {
            continue;
        }
        met.entry(key.observer).or_default().insert(key.subject);
        met.entry(key.subject).or_default().insert(key.observer);
    }
    if met.is_empty() {
        return;
    }
    for (id, mut cohort) in bands.iter_mut() {
        let Some(contacts) = met.get(id) else {
            continue;
        };
        for contact in contacts {
            // Pre-merge sets, so a line cannot hop a second band in the same turn.
            let (_, lines) = &snapshot[contact];
            cohort.founding_lines.absorb(lines);
        }
    }
}
