//! `campaign` section -- campaign profiles and labels, victory state, the command
//! event feed, and The Telling's voice/fork/stance payloads.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::strings_to_variant_array;

pub(crate) fn campaign_label_to_dict(label: fb::CampaignLabel<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    if let Some(profile_id) = label.profileId() {
        let _ = dict.insert("profile_id", profile_id);
    }
    if let Some(title) = label.title() {
        let _ = dict.insert("title", title);
    }
    if let Some(loc_key) = label.titleLocKey() {
        let _ = dict.insert("title_loc_key", loc_key);
    }
    if let Some(subtitle) = label.subtitle() {
        let _ = dict.insert("subtitle", subtitle);
    }
    if let Some(loc_key) = label.subtitleLocKey() {
        let _ = dict.insert("subtitle_loc_key", loc_key);
    }
    dict
}

pub(crate) fn campaign_profile_to_dict(profile: fb::CampaignProfile<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    if let Some(id) = profile.id() {
        let _ = dict.insert("id", id);
    }
    if let Some(title) = profile.title() {
        let _ = dict.insert("title", title);
    }
    if let Some(loc_key) = profile.titleLocKey() {
        let _ = dict.insert("title_loc_key", loc_key);
    }
    if let Some(subtitle) = profile.subtitle() {
        let _ = dict.insert("subtitle", subtitle);
    }
    if let Some(loc_key) = profile.subtitleLocKey() {
        let _ = dict.insert("subtitle_loc_key", loc_key);
    }
    if let Some(units) = profile.startingUnits() {
        let units_array = campaign_starting_units_to_array(units);
        if !units_array.is_empty() {
            let _ = dict.insert("starting_units", &units_array);
        }
    }
    if let Some(entries) = profile.inventory() {
        let inventory_array = campaign_inventory_to_array(entries);
        if !inventory_array.is_empty() {
            let _ = dict.insert("inventory", &inventory_array);
        }
    }
    if let Some(tags) = profile.knowledgeTags() {
        let tag_array = strings_to_variant_array(tags);
        if !tag_array.is_empty() {
            let _ = dict.insert("knowledge_tags", &tag_array);
        }
    }
    let radius = profile.surveyRadius();
    if radius > 0 {
        let _ = dict.insert("survey_radius", radius as i64);
    }
    if let Some(mode) = profile.fogMode() {
        if !mode.is_empty() {
            let _ = dict.insert("fog_mode", mode);
        }
    }
    if let Some(primary) = profile.primaryFoodModule() {
        if !primary.is_empty() {
            let _ = dict.insert("primary_food_module", primary);
        }
    }
    if let Some(secondary) = profile.secondaryFoodModule() {
        if !secondary.is_empty() {
            let _ = dict.insert("secondary_food_module", secondary);
        }
    }
    dict
}

fn campaign_starting_units_to_array(
    units: Vector<'_, ForwardsUOffset<fb::CampaignStartingUnit<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for unit in units {
        let mut dict = VarDictionary::new();
        if let Some(kind) = unit.kind() {
            let _ = dict.insert("kind", kind);
        }
        let _ = dict.insert("count", unit.count() as i64);
        if let Some(tags) = unit.tags() {
            let tag_array = strings_to_variant_array(tags);
            if !tag_array.is_empty() {
                let _ = dict.insert("tags", &tag_array);
            }
        }
        array.push(&dict.to_variant());
    }
    array
}

fn campaign_inventory_to_array(
    inventory: Vector<'_, ForwardsUOffset<fb::CampaignInventoryEntry<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for entry in inventory {
        let mut dict = VarDictionary::new();
        if let Some(item) = entry.item() {
            let _ = dict.insert("item", item);
        }
        let _ = dict.insert("quantity", entry.quantity());
        array.push(&dict.to_variant());
    }
    array
}

/// The Telling (docs/plan_the_telling.md): each faction's narrator MEDIUM — oral saga ->
/// painted chronicle -> written record. PRESENTATIONAL only; it never selects different copy.
/// `medium_id` stays a free-form string (the species/policy/register convention), so a new
/// medium needs no schema change and the client must key its styling off a table with a
/// fallback rather than a match assuming the shipped three are exhaustive.
pub(crate) fn voice_medium_to_array(
    states: Vector<'_, ForwardsUOffset<fb::VoiceMediumState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        if let Some(medium_id) = state.mediumId() {
            let _ = dict.insert("medium_id", medium_id);
        }
        let _ = dict.insert("medium_index", state.mediumIndex() as i64);
        array.push(&dict.to_variant());
    }
    array
}

/// The Telling (docs/plan_the_telling.md): one register's rendering of a player-visible line.
/// `register` is a FREE-FORM string by design — a new voice register needs no schema change — so
/// the decoder never enumerates them and the client builds its toggle from what is present.
fn voice_lines_to_array(lines: Vector<'_, ForwardsUOffset<fb::VoiceLine<'_>>>) -> VarArray {
    let mut array = VarArray::new();
    for line in lines {
        let mut dict = VarDictionary::new();
        if let Some(register) = line.register() {
            let _ = dict.insert("register", register);
        }
        if let Some(text) = line.text() {
            let _ = dict.insert("text", text);
        }
        array.push(&dict.to_variant());
    }
    array
}

/// Pending narrative forks, per faction — the same per-faction-with-nested-children shape as
/// `discovered_sites_to_array`, one level deeper (each fork nests narration / choices / gloss).
///
/// `is_defer` is computed SERVER-side and exactly one choice per fork carries it; it is copied
/// through verbatim so the client never has to re-derive which choice writes nothing.
pub(crate) fn pending_forks_to_array(
    states: Vector<'_, ForwardsUOffset<fb::PendingForksState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        let mut forks = VarArray::new();
        if let Some(entries) = state.forks() {
            for fork in entries {
                let mut fork_dict = VarDictionary::new();
                if let Some(beat_id) = fork.beatId() {
                    let _ = fork_dict.insert("beat_id", beat_id);
                }
                if let Some(wardrobe_id) = fork.wardrobeId() {
                    let _ = fork_dict.insert("wardrobe_id", wardrobe_id);
                }
                let _ = fork_dict.insert("posted_tick", fork.postedTick() as i64);
                if let Some(narration) = fork.narration() {
                    let _ = fork_dict.insert("narration", &voice_lines_to_array(narration));
                }
                let mut choices = VarArray::new();
                if let Some(entries) = fork.choices() {
                    for choice in entries {
                        let mut choice_dict = VarDictionary::new();
                        if let Some(choice_id) = choice.choiceId() {
                            let _ = choice_dict.insert("choice_id", choice_id);
                        }
                        if let Some(label) = choice.label() {
                            let _ = choice_dict.insert("label", &voice_lines_to_array(label));
                        }
                        let _ = choice_dict.insert("is_defer", choice.isDefer());
                        choices.push(&choice_dict.to_variant());
                    }
                }
                let _ = fork_dict.insert("choices", &choices);
                let mut gloss = VarArray::new();
                if let Some(entries) = fork.gloss() {
                    for entry in entries {
                        let mut gloss_dict = VarDictionary::new();
                        if let Some(signal) = entry.signal() {
                            let _ = gloss_dict.insert("signal", signal);
                        }
                        let _ = gloss_dict.insert("value", entry.value());
                        gloss.push(&gloss_dict.to_variant());
                    }
                }
                let _ = fork_dict.insert("gloss", &gloss);
                forks.push(&fork_dict.to_variant());
            }
        }
        let _ = dict.insert("forks", &forks);
        array.push(&dict.to_variant());
    }
    array
}

/// Per-faction stance axes — the EFFECTIVE value of each axis in [-1, 1] (behaviour + declared
/// offsets). `axis` is a free-form string for the same reason `register` is.
pub(crate) fn stance_axes_to_array(
    states: Vector<'_, ForwardsUOffset<fb::StanceState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        let mut axes = VarArray::new();
        if let Some(entries) = state.axes() {
            for entry in entries {
                let mut axis_dict = VarDictionary::new();
                if let Some(axis) = entry.axis() {
                    let _ = axis_dict.insert("axis", axis);
                }
                let _ = axis_dict.insert("value", entry.value());
                axes.push(&axis_dict.to_variant());
            }
        }
        let _ = dict.insert("axes", &axes);
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn command_events_to_array(
    events: Vector<'_, ForwardsUOffset<fb::CommandEventState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for event in events {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("tick", event.tick() as i64);
        if let Some(kind) = event.kind() {
            let _ = dict.insert("kind", kind);
        }
        let _ = dict.insert("faction", event.faction() as i64);
        if let Some(label) = event.label() {
            let _ = dict.insert("label", label);
        }
        if let Some(detail) = event.detail() {
            let _ = dict.insert("detail", detail);
        }
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn victory_state_to_dict(state: fb::VictoryState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut modes_array = VarArray::new();
    let winner_mode_id = state
        .winner()
        .and_then(|winner| winner.mode())
        .map(|raw| raw.to_owned());
    let mut winner_label: Option<String> = None;

    if let Some(modes) = state.modes() {
        for mode in modes {
            let id = mode.id().unwrap_or("");
            let kind = mode.kind().unwrap_or("");
            let label_source = if id.is_empty() { kind } else { id };
            let label_text = format_victory_label(label_source);
            let kind_label = format_victory_label(kind);
            let progress = mode.progress();
            let threshold = mode.threshold();
            let mut progress_pct = 0.0f32;
            if threshold > f32::EPSILON {
                progress_pct = (progress / threshold).clamp(0.0, 1.0);
            }

            let mut entry = VarDictionary::new();
            let _ = entry.insert("id", id);
            let _ = entry.insert("kind", kind);
            let _ = entry.insert("label", label_text.clone());
            let _ = entry.insert("kind_label", kind_label);
            let _ = entry.insert("progress", f64::from(progress));
            let _ = entry.insert("threshold", f64::from(threshold));
            let _ = entry.insert("achieved", mode.achieved());
            let _ = entry.insert("progress_pct", f64::from(progress_pct));
            modes_array.push(&entry.to_variant());

            if let Some(target) = winner_mode_id.as_ref() {
                if !target.is_empty() && target == id {
                    winner_label = Some(label_text);
                }
            }
        }
    }

    if !modes_array.is_empty() {
        let _ = dict.insert("modes", &modes_array);
    }

    if let Some(winner) = state.winner() {
        let mut winner_dict = VarDictionary::new();
        if let Some(mode) = winner.mode() {
            let _ = winner_dict.insert("mode", mode);
        }
        let label_text = winner_label
            .or_else(|| winner.mode().map(format_victory_label))
            .unwrap_or_else(|| "Victory".to_string());
        let _ = winner_dict.insert("label", label_text);
        let _ = winner_dict.insert("faction", winner.faction() as i64);
        let _ = winner_dict.insert("tick", winner.tick() as i64);
        let _ = dict.insert("winner", &winner_dict);
    }

    dict
}

fn format_victory_label(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    raw.split(['_', '-', '.'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            let mut formatted = String::new();
            if let Some(first) = chars.next() {
                formatted.extend(first.to_uppercase());
            }
            formatted.extend(chars.flat_map(|c| c.to_lowercase()));
            formatted
        })
        .collect::<Vec<_>>()
        .join(" ")
}
