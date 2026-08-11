use super::*;

use crate::connections::ConnectionLedger;

/// **The viewer's own ties, on the wire** (`docs/plan_contact_and_logistics.md` §Q2).
///
/// Filtered to edges whose **observer** band belongs to `viewer`: you see who *you* know, not who
/// other peoples know. The subject is published whatever faction it belongs to — that is the whole
/// point of a contact — and faction never appears on the row, because it is a property of the
/// endpoints (`band_factions` resolves it here and nowhere else).
///
/// **Order is the ledger's `BTreeMap` order**, so the section is stable frame to frame and diffs
/// out when nothing moved. An observer band the cohort query cannot resolve — despawned this turn —
/// is skipped rather than published against a guessed faction.
pub(crate) fn connection_states(
    ledger: &ConnectionLedger,
    band_factions: &HashMap<BandId, FactionId>,
    viewer: FactionId,
) -> Vec<sim_runtime::ConnectionState> {
    ledger
        .iter()
        .filter(|(key, _)| band_factions.get(&key.observer) == Some(&viewer))
        .map(|(key, connection)| sim_runtime::ConnectionState {
            observer_band_id: key.observer.0,
            subject_band_id: key.subject.0,
            strength: connection.strength.to_f32(),
            last_seen_x: connection.last_seen_position.x,
            last_seen_y: connection.last_seen_position.y,
            last_seen_turn: connection.last_seen_turn,
            last_contact_turn: connection.last_contact_turn,
            first_contact_turn: connection.first_contact_turn,
        })
        .collect()
}
