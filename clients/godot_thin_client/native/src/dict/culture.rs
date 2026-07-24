//! `culture` section -- culture layers/traits/tensions, sentiment, and the
//! influential individuals who carry them.

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::fixed64_to_f64;
use crate::dict::population::audience_generations_to_array;

const CULTURE_AXIS_KEYS: [&str; 15] = [
    "PassiveAggressive",
    "OpenClosed",
    "CollectivistIndividualist",
    "TraditionalistRevisionist",
    "HierarchicalEgalitarian",
    "SyncreticPurist",
    "AsceticIndulgent",
    "PragmaticIdealistic",
    "RationalistMystical",
    "ExpansionistInsular",
    "AdaptiveStubborn",
    "HonorBoundOpportunistic",
    "MeritOrientedLineageOriented",
    "SecularDevout",
    "PluralisticMonocultural",
];

const CULTURE_AXIS_LABELS: [&str; 15] = [
    "Passive ↔ Aggressive",
    "Open ↔ Closed",
    "Collectivist ↔ Individualist",
    "Traditionalist ↔ Revisionist",
    "Hierarchical ↔ Egalitarian",
    "Syncretic ↔ Purist",
    "Ascetic ↔ Indulgent",
    "Pragmatic ↔ Idealistic",
    "Rationalist ↔ Mystical",
    "Expansionist ↔ Insular",
    "Adaptive ↔ Stubborn",
    "Honor-Bound ↔ Opportunistic",
    "Merit ↔ Lineage",
    "Secular ↔ Devout",
    "Pluralistic ↔ Monocultural",
];

const CULTURE_SCOPE_LABELS: [&str; 3] = ["Global", "Regional", "Local"];
const CULTURE_TENSION_LABELS: [&str; 3] = ["Drift Warning", "Assimilation Push", "Schism Risk"];

pub(crate) fn axis_bias_to_dict(axis: fb::AxisBiasState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("knowledge", fixed64_to_f64(axis.knowledge()));
    let _ = dict.insert("trust", fixed64_to_f64(axis.trust()));
    let _ = dict.insert("equity", fixed64_to_f64(axis.equity()));
    let _ = dict.insert("agency", fixed64_to_f64(axis.agency()));
    dict
}

fn sentiment_driver_category_label(category: fb::SentimentDriverCategory) -> &'static str {
    match category {
        fb::SentimentDriverCategory::Policy => "Policy",
        fb::SentimentDriverCategory::Incident => "Incident",
        fb::SentimentDriverCategory::Influencer => "Influencer",
        _ => "Unknown",
    }
}

fn sentiment_axis_to_dict(axis: fb::SentimentAxisTelemetry<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("policy", fixed64_to_f64(axis.policy()));
    let _ = dict.insert("incidents", fixed64_to_f64(axis.incidents()));
    let _ = dict.insert("influencers", fixed64_to_f64(axis.influencers()));
    let _ = dict.insert("total", fixed64_to_f64(axis.total()));

    let mut drivers = VarArray::new();
    if let Some(list) = axis.drivers() {
        for driver in list {
            let mut driver_dict = VarDictionary::new();
            let _ = driver_dict.insert(
                "category",
                sentiment_driver_category_label(driver.category()),
            );
            let _ = driver_dict.insert("label", driver.label().unwrap_or_default());
            let _ = driver_dict.insert("value", fixed64_to_f64(driver.value()));
            let _ = driver_dict.insert("weight", fixed64_to_f64(driver.weight()));
            let variant = driver_dict.to_variant();
            drivers.push(&variant);
        }
    }
    let _ = dict.insert("drivers", &drivers);
    dict
}

pub(crate) fn sentiment_to_dict(sentiment: fb::SentimentTelemetryState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    if let Some(axis) = sentiment.knowledge() {
        let _ = dict.insert("knowledge", &sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.trust() {
        let _ = dict.insert("trust", &sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.equity() {
        let _ = dict.insert("equity", &sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.agency() {
        let _ = dict.insert("agency", &sentiment_axis_to_dict(axis));
    }
    dict
}

fn influence_scope_label(scope: fb::InfluenceScopeKind) -> &'static str {
    match scope {
        fb::InfluenceScopeKind::Local => "Local",
        fb::InfluenceScopeKind::Regional => "Regional",
        fb::InfluenceScopeKind::Global => "Global",
        fb::InfluenceScopeKind::Generation => "Generation",
        _ => "Unknown",
    }
}

fn influence_lifecycle_label(lifecycle: fb::InfluenceLifecycle) -> &'static str {
    match lifecycle {
        fb::InfluenceLifecycle::Potential => "Potential",
        fb::InfluenceLifecycle::Active => "Active",
        fb::InfluenceLifecycle::Dormant => "Dormant",
        _ => "Unknown",
    }
}

fn influence_domain_labels(mask: u32) -> PackedStringArray {
    let mut labels = PackedStringArray::new();
    for value in 0..=4 {
        let bit = 1u32 << value;
        if mask & bit == 0 {
            continue;
        }
        let label = match value {
            0 => "Sentiment",
            1 => "Discovery",
            2 => "Logistics",
            3 => "Production",
            4 => "Humanitarian",
            _ => continue,
        };
        let gstring = GString::from(label);
        labels.push(&gstring);
    }
    labels
}

fn influencer_to_dict(state: fb::InfluentialIndividualState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("influence", fixed64_to_f64(state.influence()));
    let _ = dict.insert("growth_rate", fixed64_to_f64(state.growthRate()));
    let _ = dict.insert("baseline_growth", fixed64_to_f64(state.baselineGrowth()));
    let _ = dict.insert("notoriety", fixed64_to_f64(state.notoriety()));
    let _ = dict.insert(
        "sentiment_knowledge",
        fixed64_to_f64(state.sentimentKnowledge()),
    );
    let _ = dict.insert("sentiment_trust", fixed64_to_f64(state.sentimentTrust()));
    let _ = dict.insert("sentiment_equity", fixed64_to_f64(state.sentimentEquity()));
    let _ = dict.insert("sentiment_agency", fixed64_to_f64(state.sentimentAgency()));
    let _ = dict.insert(
        "sentiment_weight_knowledge",
        fixed64_to_f64(state.sentimentWeightKnowledge()),
    );
    let _ = dict.insert(
        "sentiment_weight_trust",
        fixed64_to_f64(state.sentimentWeightTrust()),
    );
    let _ = dict.insert(
        "sentiment_weight_equity",
        fixed64_to_f64(state.sentimentWeightEquity()),
    );
    let _ = dict.insert(
        "sentiment_weight_agency",
        fixed64_to_f64(state.sentimentWeightAgency()),
    );
    let _ = dict.insert("logistics_bonus", fixed64_to_f64(state.logisticsBonus()));
    let _ = dict.insert("morale_bonus", fixed64_to_f64(state.moraleBonus()));
    let _ = dict.insert("power_bonus", fixed64_to_f64(state.powerBonus()));
    let _ = dict.insert("logistics_weight", fixed64_to_f64(state.logisticsWeight()));
    let _ = dict.insert("morale_weight", fixed64_to_f64(state.moraleWeight()));
    let _ = dict.insert("power_weight", fixed64_to_f64(state.powerWeight()));
    let _ = dict.insert("support_charge", fixed64_to_f64(state.supportCharge()));
    let _ = dict.insert(
        "suppress_pressure",
        fixed64_to_f64(state.suppressPressure()),
    );
    let domains_mask = state.domains();
    let _ = dict.insert("domains_mask", domains_mask as i64);
    let _ = dict.insert("domains", &influence_domain_labels(domains_mask));
    let _ = dict.insert("scope", influence_scope_label(state.scope()));
    let generation_scope = state.generationScope();
    if generation_scope != u16::MAX {
        let _ = dict.insert("generation_scope", generation_scope as i64);
    }
    let _ = dict.insert("supported", state.supported());
    let _ = dict.insert("suppressed", state.suppressed());
    let _ = dict.insert("lifecycle", influence_lifecycle_label(state.lifecycle()));
    let _ = dict.insert("coherence", fixed64_to_f64(state.coherence()));
    let _ = dict.insert("ticks_in_status", state.ticksInStatus() as i64);
    let audience = audience_generations_to_array(state.audienceGenerations());
    let _ = dict.insert("audience_generations", &audience);
    let _ = dict.insert("support_popular", fixed64_to_f64(state.supportPopular()));
    let _ = dict.insert("support_peer", fixed64_to_f64(state.supportPeer()));
    let _ = dict.insert(
        "support_institutional",
        fixed64_to_f64(state.supportInstitutional()),
    );
    let _ = dict.insert(
        "support_humanitarian",
        fixed64_to_f64(state.supportHumanitarian()),
    );
    let _ = dict.insert("weight_popular", fixed64_to_f64(state.weightPopular()));
    let _ = dict.insert("weight_peer", fixed64_to_f64(state.weightPeer()));
    let _ = dict.insert(
        "weight_institutional",
        fixed64_to_f64(state.weightInstitutional()),
    );
    let _ = dict.insert(
        "weight_humanitarian",
        fixed64_to_f64(state.weightHumanitarian()),
    );
    if let Some(resonance) = state.cultureResonance() {
        let array = culture_resonance_to_array(resonance);
        let _ = dict.insert("culture_resonance", &array);
    }
    dict
}

pub(crate) fn influencers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::InfluentialIndividualState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in list {
        let dict = influencer_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_resonance_entry_to_dict(
    entry: fb::InfluencerCultureResonanceEntry<'_>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("weight", fixed64_to_f64(entry.weight()));
    let _ = dict.insert("output", fixed64_to_f64(entry.output()));
    dict
}

fn culture_resonance_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'_>>,
    >,
) -> VarArray {
    let mut array = VarArray::new();
    for entry in list {
        let dict = culture_resonance_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn culture_layer_to_dict(layer: fb::CultureLayerState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let id = layer.id();
    let scope = layer.scope();
    let scope_label = culture_scope_to_label(scope);
    let owner = layer.owner();
    let parent = layer.parent();
    let baseline = layer.divergence();
    let soft = layer.softThreshold();
    let hard = layer.hardThreshold();
    let _ = dict.insert("id", id as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", scope_label);
    let _ = dict.insert("owner", format!("{owner:016X}"));
    if owner <= i64::MAX as u64 {
        let _ = dict.insert("owner_value", owner as i64);
    }
    let _ = dict.insert("parent", parent as i64);
    let _ = dict.insert("divergence", fixed64_to_f64(baseline));
    let _ = dict.insert("soft_threshold", fixed64_to_f64(soft));
    let _ = dict.insert("hard_threshold", fixed64_to_f64(hard));
    let _ = dict.insert("ticks_above_soft", layer.ticksAboveSoft() as i64);
    let _ = dict.insert("ticks_above_hard", layer.ticksAboveHard() as i64);
    let _ = dict.insert("last_updated_tick", layer.lastUpdatedTick() as i64);

    let mut traits_array = VarArray::new();
    if let Some(traits) = layer.traits() {
        for trait_entry in traits {
            let trait_dict = culture_trait_to_dict(trait_entry);
            traits_array.push(&trait_dict.to_variant());
        }
    }
    let _ = dict.insert("traits", &traits_array);

    dict
}

fn culture_trait_to_dict(entry: fb::CultureTraitEntry<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("baseline", fixed64_to_f64(entry.baseline()));
    let _ = dict.insert("modifier", fixed64_to_f64(entry.modifier()));
    let _ = dict.insert("value", fixed64_to_f64(entry.value()));
    dict
}

fn culture_tension_to_dict(state: fb::CultureTensionState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let scope = state.scope();
    let kind = state.kind();
    let _ = dict.insert("layer_id", state.layerId() as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", culture_scope_to_label(scope));
    let _ = dict.insert("kind", culture_tension_to_key(kind));
    let _ = dict.insert("kind_label", culture_tension_to_label(kind));
    let _ = dict.insert("severity", fixed64_to_f64(state.severity()));
    let _ = dict.insert("timer", state.timer() as i64);
    dict
}

pub(crate) fn culture_layers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureLayerState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for layer in list {
        let dict = culture_layer_to_dict(layer);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

pub(crate) fn culture_tensions_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureTensionState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for tension in list {
        let dict = culture_tension_to_dict(tension);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_scope_to_key(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => "Global",
        fb::CultureLayerScope::Regional => "Regional",
        fb::CultureLayerScope::Local => "Local",
        _ => "Unknown",
    }
}

fn culture_scope_to_label(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => CULTURE_SCOPE_LABELS[0],
        fb::CultureLayerScope::Regional => CULTURE_SCOPE_LABELS[1],
        fb::CultureLayerScope::Local => CULTURE_SCOPE_LABELS[2],
        _ => "Unknown",
    }
}

fn culture_axis_to_key(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_KEYS.get(idx).copied().unwrap_or("Trait")
}

fn culture_axis_to_label(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_LABELS.get(idx).copied().unwrap_or("Trait")
}

fn culture_tension_to_key(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => "DriftWarning",
        fb::CultureTensionKind::AssimilationPush => "AssimilationPush",
        fb::CultureTensionKind::SchismRisk => "SchismRisk",
        _ => "Unknown",
    }
}

fn culture_tension_to_label(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => CULTURE_TENSION_LABELS[0],
        fb::CultureTensionKind::AssimilationPush => CULTURE_TENSION_LABELS[1],
        fb::CultureTensionKind::SchismRisk => CULTURE_TENSION_LABELS[2],
        _ => "Unknown",
    }
}
