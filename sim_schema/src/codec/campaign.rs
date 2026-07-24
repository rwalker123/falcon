//! Campaign-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::campaign::{
    CampaignLabel, CampaignProfileState, CommandEventState, PendingForksState, StanceState,
    VictorySnapshotState, VoiceLineState, VoiceMediumState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_campaign_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
    victory_state: WIPOffset<fb::VictoryState<'a>>,
) -> WIPOffset<fb::CampaignSection<'a>> {
    let campaign_profiles = create_campaign_profiles(builder, &snapshot.campaign_profiles);
    let command_events = create_command_events(builder, &snapshot.command_events);
    let pending_forks = create_pending_forks(builder, &snapshot.pending_forks);
    let stance_axes = create_stance_axes(builder, &snapshot.stance_axes);
    let voice_medium = create_voice_medium(builder, &snapshot.voice_medium);
    fb::CampaignSection::create(
        builder,
        &fb::CampaignSectionArgs {
            campaignProfiles: Some(campaign_profiles),
            commandEvents: Some(command_events),
            victory: Some(victory_state),
            pendingForks: Some(pending_forks),
            stanceAxes: Some(stance_axes),
            voiceMedium: Some(voice_medium),
        },
    )
}

pub(crate) fn serialize_campaign_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
    victory_state: Option<WIPOffset<fb::VictoryState<'a>>>,
) -> WIPOffset<fb::CampaignSection<'a>> {
    let command_events = delta
        .command_events
        .as_ref()
        .map(|entries| create_command_events(builder, entries));
    let pending_forks = delta
        .pending_forks
        .as_ref()
        .map(|entries| create_pending_forks(builder, entries));
    let stance_axes = delta
        .stance_axes
        .as_ref()
        .map(|entries| create_stance_axes(builder, entries));
    let voice_medium = delta
        .voice_medium
        .as_ref()
        .map(|entries| create_voice_medium(builder, entries));
    fb::CampaignSection::create(
        builder,
        &fb::CampaignSectionArgs {
            campaignProfiles: None,
            commandEvents: command_events,
            victory: victory_state,
            pendingForks: pending_forks,
            stanceAxes: stance_axes,
            voiceMedium: voice_medium,
        },
    )
}

pub(crate) fn create_campaign_label<'a>(
    builder: &mut FbBuilder<'a>,
    label: &CampaignLabel,
) -> Option<WIPOffset<fb::CampaignLabel<'a>>> {
    let has_any = label.title.is_some()
        || label.title_loc_key.is_some()
        || label.subtitle.is_some()
        || label.subtitle_loc_key.is_some()
        || label.profile_id.is_some();
    if !has_any {
        return None;
    }

    let profile_id = label
        .profile_id
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title = label
        .title
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title_loc_key = label
        .title_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle = label
        .subtitle
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle_loc_key = label
        .subtitle_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));

    Some(fb::CampaignLabel::create(
        builder,
        &fb::CampaignLabelArgs {
            profileId: profile_id,
            title,
            titleLocKey: title_loc_key,
            subtitle,
            subtitleLocKey: subtitle_loc_key,
        },
    ))
}

fn create_campaign_profiles<'a>(
    builder: &mut FbBuilder<'a>,
    profiles: &[CampaignProfileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CampaignProfile<'a>>>> {
    let mut entries = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let id = profile
            .id
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title = profile
            .title
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title_loc_key = profile
            .title_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle = profile
            .subtitle
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle_loc_key = profile
            .subtitle_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let starting_units = if profile.starting_units.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.starting_units.len());
            for unit in &profile.starting_units {
                let kind = builder.create_string(unit.kind.as_str());
                let tags = if unit.tags.is_empty() {
                    None
                } else {
                    let tag_offsets: Vec<_> = unit
                        .tags
                        .iter()
                        .map(|tag| builder.create_string(tag.as_str()))
                        .collect();
                    Some(builder.create_vector(&tag_offsets))
                };
                let unit_entry = fb::CampaignStartingUnit::create(
                    builder,
                    &fb::CampaignStartingUnitArgs {
                        kind: Some(kind),
                        count: unit.count,
                        tags,
                    },
                );
                offsets.push(unit_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let inventory = if profile.inventory.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.inventory.len());
            for entry in &profile.inventory {
                let item = builder.create_string(entry.item.as_str());
                let inv_entry = fb::CampaignInventoryEntry::create(
                    builder,
                    &fb::CampaignInventoryEntryArgs {
                        item: Some(item),
                        quantity: entry.quantity,
                    },
                );
                offsets.push(inv_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let knowledge_tags = if profile.knowledge_tags.is_empty() {
            None
        } else {
            let offsets: Vec<_> = profile
                .knowledge_tags
                .iter()
                .map(|tag| builder.create_string(tag.as_str()))
                .collect();
            Some(builder.create_vector(&offsets))
        };
        let fog_mode = profile
            .fog_mode
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let primary_food_module = profile
            .primary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let secondary_food_module = profile
            .secondary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let survey_radius = profile.survey_radius.unwrap_or(0);
        let entry = fb::CampaignProfile::create(
            builder,
            &fb::CampaignProfileArgs {
                id,
                title,
                titleLocKey: title_loc_key,
                subtitle,
                subtitleLocKey: subtitle_loc_key,
                startingUnits: starting_units,
                inventory,
                knowledgeTags: knowledge_tags,
                surveyRadius: survey_radius,
                fogMode: fog_mode,
                primaryFoodModule: primary_food_module,
                secondaryFoodModule: secondary_food_module,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_voice_lines<'a>(
    builder: &mut FbBuilder<'a>,
    lines: &[VoiceLineState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::VoiceLine<'a>>>> {
    let mut entries = Vec::with_capacity(lines.len());
    for line in lines {
        let register = builder.create_string(line.register.as_str());
        let text = builder.create_string(line.text.as_str());
        entries.push(fb::VoiceLine::create(
            builder,
            &fb::VoiceLineArgs {
                register: Some(register),
                text: Some(text),
            },
        ));
    }
    builder.create_vector(&entries)
}

fn create_pending_forks<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[PendingForksState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PendingForksState<'a>>>> {
    let mut faction_entries = Vec::with_capacity(states.len());
    for state in states {
        let mut fork_entries = Vec::with_capacity(state.forks.len());
        for fork in &state.forks {
            let beat_id = builder.create_string(fork.beat_id.as_str());
            let wardrobe_id = builder.create_string(fork.wardrobe_id.as_str());
            let narration = create_voice_lines(builder, &fork.narration);
            let mut choice_entries = Vec::with_capacity(fork.choices.len());
            for choice in &fork.choices {
                let choice_id = builder.create_string(choice.choice_id.as_str());
                let label = create_voice_lines(builder, &choice.label);
                choice_entries.push(fb::ForkChoiceState::create(
                    builder,
                    &fb::ForkChoiceStateArgs {
                        choiceId: Some(choice_id),
                        label: Some(label),
                        isDefer: choice.is_defer,
                    },
                ));
            }
            let choices = builder.create_vector(&choice_entries);
            let mut gloss_entries = Vec::with_capacity(fork.gloss.len());
            for entry in &fork.gloss {
                let signal = builder.create_string(entry.signal.as_str());
                gloss_entries.push(fb::GlossEntry::create(
                    builder,
                    &fb::GlossEntryArgs {
                        signal: Some(signal),
                        value: entry.value,
                    },
                ));
            }
            let gloss = builder.create_vector(&gloss_entries);
            fork_entries.push(fb::PendingForkState::create(
                builder,
                &fb::PendingForkStateArgs {
                    beatId: Some(beat_id),
                    wardrobeId: Some(wardrobe_id),
                    postedTick: fork.posted_tick,
                    narration: Some(narration),
                    choices: Some(choices),
                    gloss: Some(gloss),
                },
            ));
        }
        let forks = builder.create_vector(&fork_entries);
        faction_entries.push(fb::PendingForksState::create(
            builder,
            &fb::PendingForksStateArgs {
                faction: state.faction,
                forks: Some(forks),
            },
        ));
    }
    builder.create_vector(&faction_entries)
}

fn create_stance_axes<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[StanceState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::StanceState<'a>>>> {
    let mut faction_entries = Vec::with_capacity(states.len());
    for state in states {
        let mut axis_entries = Vec::with_capacity(state.axes.len());
        for axis in &state.axes {
            let name = builder.create_string(axis.axis.as_str());
            axis_entries.push(fb::StanceAxisState::create(
                builder,
                &fb::StanceAxisStateArgs {
                    axis: Some(name),
                    value: axis.value,
                },
            ));
        }
        let axes = builder.create_vector(&axis_entries);
        faction_entries.push(fb::StanceState::create(
            builder,
            &fb::StanceStateArgs {
                faction: state.faction,
                axes: Some(axes),
            },
        ));
    }
    builder.create_vector(&faction_entries)
}

/// The narrator's medium, per faction. `mediumId` rides as a string so a new rung on the ladder
/// needs no schema change.
fn create_voice_medium<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[VoiceMediumState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::VoiceMediumState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let medium_id = builder.create_string(state.medium_id.as_str());
        entries.push(fb::VoiceMediumState::create(
            builder,
            &fb::VoiceMediumStateArgs {
                faction: state.faction,
                mediumId: Some(medium_id),
                mediumIndex: state.medium_index,
            },
        ));
    }
    builder.create_vector(&entries)
}

fn create_command_events<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[CommandEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CommandEventState<'a>>>> {
    let mut entries = Vec::with_capacity(events.len());
    for event in events {
        let kind = builder.create_string(event.kind.as_str());
        let label = builder.create_string(event.label.as_str());
        let detail = event
            .detail
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let entry = fb::CommandEventState::create(
            builder,
            &fb::CommandEventStateArgs {
                tick: event.tick,
                kind: Some(kind),
                faction: event.faction,
                label: Some(label),
                detail,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

pub(crate) fn create_victory_state<'a>(
    builder: &mut FbBuilder<'a>,
    state: &VictorySnapshotState,
) -> WIPOffset<fb::VictoryState<'a>> {
    let mut mode_entries = Vec::with_capacity(state.modes.len());
    for mode in &state.modes {
        let id = builder.create_string(mode.id.as_str());
        let kind = builder.create_string(mode.kind.as_str());
        let entry = fb::VictoryModeState::create(
            builder,
            &fb::VictoryModeStateArgs {
                id: Some(id),
                kind: Some(kind),
                progress: mode.progress,
                threshold: mode.threshold,
                achieved: mode.achieved,
            },
        );
        mode_entries.push(entry);
    }
    let modes_vec = builder.create_vector(&mode_entries);

    let winner = state.winner.as_ref().map(|winner| {
        let mode = builder.create_string(winner.mode.as_str());
        fb::VictoryResult::create(
            builder,
            &fb::VictoryResultArgs {
                mode: Some(mode),
                faction: winner.faction,
                tick: winner.tick,
            },
        )
    });

    fb::VictoryState::create(
        builder,
        &fb::VictoryStateArgs {
            modes: Some(modes_vec),
            winner,
        },
    )
}
