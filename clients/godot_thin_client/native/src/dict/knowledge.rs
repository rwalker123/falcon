//! `knowledge` section -- discovery progress, great discoveries, and discovered sites.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::{fixed64_to_f64, string_vector_to_packed};

pub(crate) fn discovered_sites_to_array(
    states: Vector<'_, ForwardsUOffset<fb::DiscoveredSitesState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        let mut sites = VarArray::new();
        if let Some(entries) = state.sites() {
            for site in entries {
                let mut site_dict = VarDictionary::new();
                let _ = site_dict.insert("x", site.x() as i64);
                let _ = site_dict.insert("y", site.y() as i64);
                if let Some(site_id) = site.site_id() {
                    let _ = site_dict.insert("site_id", site_id);
                }
                if let Some(category) = site.category() {
                    let _ = site_dict.insert("category", category);
                }
                if let Some(display_name) = site.display_name() {
                    let _ = site_dict.insert("display_name", display_name);
                }
                if let Some(glyph) = site.glyph() {
                    let _ = site_dict.insert("glyph", glyph);
                }
                sites.push(&site_dict.to_variant());
            }
        }
        let _ = dict.insert("sites", &sites);
        array.push(&dict.to_variant());
    }
    array
}

fn knowledge_field_label(field: fb::KnowledgeField) -> &'static str {
    match field {
        fb::KnowledgeField::Physics => "Physics",
        fb::KnowledgeField::Chemistry => "Chemistry",
        fb::KnowledgeField::Biology => "Biology",
        fb::KnowledgeField::Data => "Data",
        fb::KnowledgeField::Communication => "Communication",
        fb::KnowledgeField::Exotic => "Exotic",
        _ => "Unknown",
    }
}

fn great_discovery_effect_labels(mask: u32) -> PackedStringArray {
    let mut labels = PackedStringArray::new();
    if mask & (1 << 0) != 0 {
        labels.push(&GString::from("Power"));
    }
    if mask & (1 << 1) != 0 {
        labels.push(&GString::from("Crisis"));
    }
    if mask & (1 << 2) != 0 {
        labels.push(&GString::from("Diplomacy"));
    }
    if mask & (1 << 3) != 0 {
        labels.push(&GString::from("Forced Publication"));
    }
    labels
}

fn discovery_progress_entry_to_dict(entry: fb::DiscoveryProgressEntry<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("faction", entry.faction() as i64);
    let _ = dict.insert("discovery", entry.discovery() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(entry.progress()));
    let _ = dict.insert("progress_raw", entry.progress());
    dict
}

pub(crate) fn discovery_progress_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::DiscoveryProgressEntry<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for entry in list {
        let dict = discovery_progress_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_state_to_dict(state: fb::GreatDiscoveryState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("faction", state.faction() as i64);
    let _ = dict.insert("field_label", knowledge_field_label(state.field()));
    let _ = dict.insert("tick", state.tick() as i64);
    let _ = dict.insert("publicly_deployed", state.publiclyDeployed());
    let effect_flags = state.effectFlags();
    let _ = dict.insert("effect_flags", effect_flags as i64);
    let _ = dict.insert("effects", &great_discovery_effect_labels(effect_flags));
    dict
}

pub(crate) fn great_discovery_states_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GreatDiscoveryState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in list {
        let dict = great_discovery_state_to_dict(state);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_requirement_definition_to_dict(
    req: fb::GreatDiscoveryRequirementDefinition<'_>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("discovery_id", req.discoveryId() as i64);
    let _ = dict.insert("weight", f64::from(req.weight()));
    let _ = dict.insert("minimum_progress", f64::from(req.minimumProgress()));
    if let Some(name) = req.name() {
        let _ = dict.insert("name", &GString::from(name));
    }
    if let Some(summary) = req.summary() {
        let _ = dict.insert("summary", &GString::from(summary));
    }
    dict
}

fn great_discovery_requirements_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::GreatDiscoveryRequirementDefinition<'_>>,
    >,
) -> VarArray {
    let mut array = VarArray::new();
    for req in list {
        let dict = great_discovery_requirement_definition_to_dict(req);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_definition_to_dict(
    definition: fb::GreatDiscoveryDefinition<'_>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("id", definition.id() as i64);
    if let Some(name) = definition.name() {
        let _ = dict.insert("name", &GString::from(name));
    }
    let field = definition.field();
    let _ = dict.insert("field", &GString::from(knowledge_field_label(field)));
    let _ = dict.insert(
        "observation_threshold",
        definition.observationThreshold() as i64,
    );
    let _ = dict.insert("cooldown_ticks", definition.cooldownTicks() as i64);
    if definition.hasFreshnessWindow() {
        let _ = dict.insert("freshness_window", definition.freshnessWindow() as i64);
    }
    let _ = dict.insert("effect_flags", definition.effectFlags() as i64);
    let _ = dict.insert("covert_until_public", definition.covertUntilPublic());
    if let Some(tier) = definition.tier() {
        let _ = dict.insert("tier", &GString::from(tier));
    }
    if let Some(summary) = definition.summary() {
        let _ = dict.insert("summary", &GString::from(summary));
    }
    if let Some(tags) = definition.tags() {
        let packed = string_vector_to_packed(tags);
        let _ = dict.insert("tags", &packed);
    } else {
        let _ = dict.insert("tags", &PackedStringArray::new());
    }
    if let Some(effects) = definition.effectsSummary() {
        let packed = string_vector_to_packed(effects);
        let _ = dict.insert("effects_summary", &packed);
    } else {
        let _ = dict.insert("effects_summary", &PackedStringArray::new());
    }
    if let Some(notes) = definition.observationNotes() {
        let _ = dict.insert("observation_notes", &GString::from(notes));
    }
    if let Some(profile) = definition.leakProfile() {
        let _ = dict.insert("leak_profile", &GString::from(profile));
    }
    if let Some(requirements) = definition.requirements() {
        let array = great_discovery_requirements_to_array(requirements);
        let _ = dict.insert("requirements", &array);
    } else {
        let _ = dict.insert("requirements", &VarArray::new());
    }
    dict
}

pub(crate) fn great_discovery_definitions_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GreatDiscoveryDefinition<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for definition in list {
        let dict = great_discovery_definition_to_dict(definition);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_progress_state_to_dict(
    state: fb::GreatDiscoveryProgressState<'_>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let progress_raw = state.progress();
    let _ = dict.insert("faction", state.faction() as i64);
    let _ = dict.insert("discovery", state.discovery() as i64);
    let _ = dict.insert("progress_raw", progress_raw);
    let _ = dict.insert("progress", fixed64_to_f64(progress_raw));
    let _ = dict.insert("observation_deficit", state.observationDeficit() as i64);
    let _ = dict.insert("eta_ticks", state.etaTicks() as i64);
    let _ = dict.insert("covert", state.covert());
    dict
}

pub(crate) fn great_discovery_progress_states_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::GreatDiscoveryProgressState<'_>>,
    >,
) -> VarArray {
    let mut array = VarArray::new();
    for state in list {
        let dict = great_discovery_progress_state_to_dict(state);
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn great_discovery_telemetry_to_dict(
    state: fb::GreatDiscoveryTelemetryState<'_>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("total_resolved", state.totalResolved() as i64);
    let _ = dict.insert("pending_candidates", state.pendingCandidates() as i64);
    let _ = dict.insert("active_constellations", state.activeConstellations() as i64);
    dict
}
