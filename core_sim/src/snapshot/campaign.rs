use super::*;

pub(crate) fn victory_snapshot_from_resource(state: &VictoryState) -> VictorySnapshotState {
    let modes = state
        .modes
        .iter()
        .map(|mode| VictoryModeSnapshotState {
            id: mode.id.0.clone(),
            kind: mode.kind.as_str().to_string(),
            progress: mode.progress,
            threshold: mode.threshold,
            achieved: mode.achieved,
        })
        .collect();

    let winner = state.winner.as_ref().map(|winner| VictoryResultState {
        mode: winner.mode.0.clone(),
        faction: winner.faction.0,
        tick: winner.tick,
    });

    VictorySnapshotState { modes, winner }
}

pub fn command_events_to_state(log: &CommandEventLog) -> Vec<CommandEventState> {
    log.iter()
        .map(|entry| CommandEventState {
            tick: entry.tick,
            kind: entry.kind.as_str().to_string(),
            faction: entry.faction.0,
            label: entry.label.clone(),
            detail: entry.detail.clone(),
        })
        .collect()
}
