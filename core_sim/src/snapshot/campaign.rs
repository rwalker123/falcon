use std::collections::BTreeMap;

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

/// The Telling's pending forks, grouped **per faction** (the `SedentarizationState` /
/// `DiscoveredSitesState` shape), so a client only ever renders its own decisions.
///
/// `isDefer` is resolved here rather than left to the client: the client must not have to know
/// that an empty `writes` is what makes a choice a defer, and its turn gate depends on the answer.
pub(crate) fn snapshot_pending_forks(ledger: &BeatLedger) -> Vec<PendingForksState> {
    let mut by_faction: BTreeMap<u32, Vec<PendingForkState>> = BTreeMap::new();
    for fork in ledger.pending_forks() {
        by_faction
            .entry(fork.faction.0)
            .or_default()
            .push(PendingForkState {
                beat_id: fork.beat_id.clone(),
                wardrobe_id: fork.wardrobe_id.clone(),
                posted_tick: fork.posted_tick,
                narration: voice_lines(&fork.rendered),
                choices: fork
                    .choices
                    .iter()
                    .map(|choice| ForkChoiceState {
                        choice_id: choice.id.clone(),
                        label: voice_lines(&choice.label),
                        is_defer: choice.is_defer,
                    })
                    .collect(),
                gloss: fork
                    .gloss
                    .iter()
                    .map(|(signal, value)| GlossEntryState {
                        signal: signal.clone(),
                        value: value.to_f32() as f64,
                    })
                    .collect(),
            });
    }
    by_faction
        .into_iter()
        .map(|(faction, forks)| PendingForksState { faction, forks })
        .collect()
}

/// Every faction's **effective** stance per axis, so the client can show what the player's
/// identity currently reads as. Derived per turn by `telling_tick`, so a rehydrated ledger exports
/// nothing until the next tick.
pub(crate) fn snapshot_stance_axes(ledger: &BeatLedger) -> Vec<StanceState> {
    ledger
        .effective_stance_by_faction()
        .map(|(faction, axes)| StanceState {
            faction: faction.0,
            axes: axes
                .iter()
                .map(|(axis, value)| StanceAxisState {
                    axis: axis.clone(),
                    value: *value,
                })
                .collect(),
        })
        .collect()
}

/// Every faction's attained narrator **medium**, so the client can present the telling as an oral
/// saga / painted chronicle / written record. Presentational only — the medium never selects
/// different copy (see `core_sim/src/telling/medium.rs`).
pub(crate) fn snapshot_voice_medium(ledger: &BeatLedger) -> Vec<VoiceMediumState> {
    ledger
        .mediums_by_faction()
        .map(|(faction, medium)| VoiceMediumState {
            faction: faction.0,
            medium_id: medium.id.clone(),
            medium_index: medium.index,
        })
        .collect()
}

fn voice_lines(lines: &BTreeMap<String, String>) -> Vec<VoiceLineState> {
    lines
        .iter()
        .map(|(register, text)| VoiceLineState {
            register: register.clone(),
            text: text.clone(),
        })
        .collect()
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
