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
            seq: entry.seq,
        })
        .collect()
}

/// **The CAMPAIGN-WIDE half of what a loadout picker needs** — the profile's pick list, its two
/// pre-fills, and the recipes this faction could put on a bench today.
///
/// **The per-band half is `PopulationCohortState.loadoutWindow`**: whether *this* band's window is
/// open, and what caps it. Every band gets a window and a splinter's budgets are not the spawned
/// band's, so nothing here may be read as a statement about a particular band.
///
/// The **kit roster is deliberately not here**: it already rides
/// `SubsistenceSection.equipmentConfigJson`, and a recipe's input costs already ride the per-band
/// `craftOffers` rows. `craftable_recipe_ids` is here rather than inferred client-side because the
/// alternative is sniffing a craft offer's *refusal sentence* — turning a player-facing string into
/// a machine contract.
pub(crate) fn snapshot_opening_loadout(
    loadout_windows: &crate::starting_loadout::StartingLoadout,
    profile: &crate::start_profile::StartProfile,
    recipes: &crate::recipes_config::RecipesConfig,
    known_crafts: &BTreeMap<String, bool>,
) -> OpeningLoadoutState {
    let loadout = &profile.overrides().opening_loadout;
    OpeningLoadoutState {
        pickable_materials: loadout.pickable_materials.clone(),
        material_defaults: loadout
            .material_defaults
            .iter()
            .map(|(material_id, units)| OpeningMaterialDefaultState {
                material_id: material_id.clone(),
                units: *units,
            })
            .collect(),
        // The same test `handle_set_bench` applies: every craft a recipe requires must be learned.
        // A recipe requiring nothing is craftable by anyone, which is what puts the four opening
        // recipes on the list and keeps the three knowledge-gated bench tools off it.
        craftable_recipe_ids: recipes
            .recipes()
            .filter(|(_, recipe)| {
                recipe
                    .requires_knowledge
                    .iter()
                    .all(|craft| known_crafts.get(craft).copied().unwrap_or(false))
            })
            .map(|(id, _)| id.to_string())
            .collect(),
        // **Clamped here, warned about once at world build.** The publish site owns the value
        // because this is where the budget and the profile are both in scope; the warn lives in
        // `stamp_starting_loadout` so a config fault is reported once per world rather than once per
        // captured frame. One rule, one helper, two callers.
        kit_defaults: crate::starting_loadout::clamped_kit_defaults(
            &loadout.kit_defaults,
            opening_kit_budget(loadout_windows),
        )
        .0
        .into_iter()
        .map(|(kit_id, count)| OpeningKitDefaultState { kit_id, count })
        .collect(),
    }
}

/// **The budget the campaign's kit pre-fill is fitted to: the OPENING band's.**
///
/// The pre-fill is the *opening* suggestion, so it is drawn against the window the world build
/// stamped — the lowest-id band still holding a grant. A world whose windows have all shut publishes
/// nothing, which needs no special case: `clamped_kit_defaults` floors every row against a budget of
/// zero, and a picker is only drawn while some window is open anyway.
fn opening_kit_budget(loadout_windows: &crate::starting_loadout::StartingLoadout) -> u32 {
    loadout_windows
        .iter()
        .find(|(_, window)| window.grants())
        .map(|(_, window)| window.supply.kit_budget())
        .unwrap_or_default()
}
