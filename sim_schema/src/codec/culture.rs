//! Culture-section FlatBuffers serialization.

use crate::codec::{create_scalar_raster, FbBuilder};
use crate::state::culture::{
    CultureLayerScope, CultureLayerState, CultureTensionKind, CultureTensionState,
    CultureTraitAxis, CultureTraitEntry, InfluenceLifecycle, InfluenceScopeKind,
    InfluencerCultureResonanceEntry, InfluentialIndividualState, SentimentAxisTelemetry,
    SentimentDriverCategory, SentimentTelemetryState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_culture_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::CultureSection<'a>> {
    let culture_layers = create_culture_layers(builder, &snapshot.culture_layers);
    let culture_tensions = create_culture_tensions(builder, &snapshot.culture_tensions);
    let culture_raster = create_scalar_raster(builder, &snapshot.culture_raster);
    let influencers = create_influencers(builder, &snapshot.influencers);
    let axis_bias = fb::AxisBiasState::create(
        builder,
        &fb::AxisBiasStateArgs {
            knowledge: snapshot.axis_bias.knowledge,
            trust: snapshot.axis_bias.trust,
            equity: snapshot.axis_bias.equity,
            agency: snapshot.axis_bias.agency,
        },
    );
    let sentiment = create_sentiment(builder, &snapshot.sentiment);
    let sentiment_raster = create_scalar_raster(builder, &snapshot.sentiment_raster);
    fb::CultureSection::create(
        builder,
        &fb::CultureSectionArgs {
            cultureLayers: Some(culture_layers),
            cultureTensions: Some(culture_tensions),
            cultureRaster: Some(culture_raster),
            influencers: Some(influencers),
            axisBias: Some(axis_bias),
            sentiment: Some(sentiment),
            sentimentRaster: Some(sentiment_raster),
            removedInfluencers: None,
            removedCultureLayers: None,
        },
    )
}

pub(crate) fn serialize_culture_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::CultureSection<'a>> {
    let culture_layers = create_culture_layers(builder, &delta.culture_layers);
    let removed_culture_layers = builder.create_vector(&delta.removed_culture_layers);
    let culture_tensions = create_culture_tensions(builder, &delta.culture_tensions);
    let culture_raster = delta
        .culture_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let influencers = create_influencers(builder, &delta.influencers);
    let removed_influencers = builder.create_vector(&delta.removed_influencers);
    let axis_bias = delta.axis_bias.as_ref().map(|axis| {
        fb::AxisBiasState::create(
            builder,
            &fb::AxisBiasStateArgs {
                knowledge: axis.knowledge,
                trust: axis.trust,
                equity: axis.equity,
                agency: axis.agency,
            },
        )
    });
    let sentiment = delta
        .sentiment
        .as_ref()
        .map(|s| create_sentiment(builder, s));
    let sentiment_raster = delta
        .sentiment_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    fb::CultureSection::create(
        builder,
        &fb::CultureSectionArgs {
            cultureLayers: Some(culture_layers),
            cultureTensions: Some(culture_tensions),
            cultureRaster: culture_raster,
            influencers: Some(influencers),
            axisBias: axis_bias,
            sentiment,
            sentimentRaster: sentiment_raster,
            removedInfluencers: Some(removed_influencers),
            removedCultureLayers: Some(removed_culture_layers),
        },
    )
}

fn create_sentiment<'a>(
    builder: &mut FbBuilder<'a>,
    sentiment: &SentimentTelemetryState,
) -> WIPOffset<fb::SentimentTelemetryState<'a>> {
    let knowledge = create_sentiment_axis(builder, &sentiment.knowledge);
    let trust = create_sentiment_axis(builder, &sentiment.trust);
    let equity = create_sentiment_axis(builder, &sentiment.equity);
    let agency = create_sentiment_axis(builder, &sentiment.agency);
    fb::SentimentTelemetryState::create(
        builder,
        &fb::SentimentTelemetryStateArgs {
            knowledge: Some(knowledge),
            trust: Some(trust),
            equity: Some(equity),
            agency: Some(agency),
        },
    )
}

fn create_sentiment_axis<'a>(
    builder: &mut FbBuilder<'a>,
    axis: &SentimentAxisTelemetry,
) -> WIPOffset<fb::SentimentAxisTelemetry<'a>> {
    let drivers: Vec<_> = axis
        .drivers
        .iter()
        .map(|driver| {
            let label = builder.create_string(driver.label.as_str());
            fb::SentimentDriverState::create(
                builder,
                &fb::SentimentDriverStateArgs {
                    category: to_fb_driver_category(driver.category),
                    label: Some(label),
                    value: driver.value,
                    weight: driver.weight,
                },
            )
        })
        .collect();
    let drivers_vec = builder.create_vector(&drivers);
    fb::SentimentAxisTelemetry::create(
        builder,
        &fb::SentimentAxisTelemetryArgs {
            policy: axis.policy,
            incidents: axis.incidents,
            influencers: axis.influencers,
            total: axis.total,
            drivers: Some(drivers_vec),
        },
    )
}

fn create_influencers<'a>(
    builder: &mut FbBuilder<'a>,
    influencers: &[InfluentialIndividualState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluentialIndividualState<'a>>>> {
    let offsets: Vec<_> = influencers
        .iter()
        .map(|inf| {
            let name = builder.create_string(inf.name.as_str());
            let audience_vec = builder.create_vector(&inf.audience_generations);
            let resonance_vec =
                create_influencer_culture_resonance(builder, &inf.culture_resonance);
            fb::InfluentialIndividualState::create(
                builder,
                &fb::InfluentialIndividualStateArgs {
                    id: inf.id,
                    name: Some(name),
                    influence: inf.influence,
                    growthRate: inf.growth_rate,
                    baselineGrowth: inf.baseline_growth,
                    notoriety: inf.notoriety,
                    sentimentKnowledge: inf.sentiment_knowledge,
                    sentimentTrust: inf.sentiment_trust,
                    sentimentEquity: inf.sentiment_equity,
                    sentimentAgency: inf.sentiment_agency,
                    sentimentWeightKnowledge: inf.sentiment_weight_knowledge,
                    sentimentWeightTrust: inf.sentiment_weight_trust,
                    sentimentWeightEquity: inf.sentiment_weight_equity,
                    sentimentWeightAgency: inf.sentiment_weight_agency,
                    logisticsBonus: inf.logistics_bonus,
                    moraleBonus: inf.morale_bonus,
                    powerBonus: inf.power_bonus,
                    logisticsWeight: inf.logistics_weight,
                    moraleWeight: inf.morale_weight,
                    powerWeight: inf.power_weight,
                    supportCharge: inf.support_charge,
                    suppressPressure: inf.suppress_pressure,
                    domains: inf.domains,
                    scope: to_fb_influence_scope(inf.scope),
                    generationScope: inf.generation_scope,
                    supported: inf.supported,
                    suppressed: inf.suppressed,
                    lifecycle: to_fb_influence_lifecycle(inf.lifecycle),
                    coherence: inf.coherence,
                    ticksInStatus: inf.ticks_in_status,
                    audienceGenerations: Some(audience_vec),
                    supportPopular: inf.support_popular,
                    supportPeer: inf.support_peer,
                    supportInstitutional: inf.support_institutional,
                    supportHumanitarian: inf.support_humanitarian,
                    weightPopular: inf.weight_popular,
                    weightPeer: inf.weight_peer,
                    weightInstitutional: inf.weight_institutional,
                    weightHumanitarian: inf.weight_humanitarian,
                    cultureResonance: Some(resonance_vec),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_influencer_culture_resonance<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[InfluencerCultureResonanceEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::InfluencerCultureResonanceEntry::create(
                builder,
                &fb::InfluencerCultureResonanceEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    weight: entry.weight,
                    output: entry.output,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_traits<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[CultureTraitEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTraitEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::CultureTraitEntry::create(
                builder,
                &fb::CultureTraitEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    baseline: entry.baseline,
                    modifier: entry.modifier,
                    value: entry.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_layers<'a>(
    builder: &mut FbBuilder<'a>,
    layers: &[CultureLayerState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureLayerState<'a>>>> {
    let offsets: Vec<_> = layers
        .iter()
        .map(|layer| {
            let traits_vec = create_culture_traits(builder, &layer.traits);
            fb::CultureLayerState::create(
                builder,
                &fb::CultureLayerStateArgs {
                    id: layer.id,
                    owner: layer.owner,
                    parent: layer.parent,
                    scope: to_fb_culture_layer_scope(layer.scope),
                    traits: Some(traits_vec),
                    divergence: layer.divergence,
                    softThreshold: layer.soft_threshold,
                    hardThreshold: layer.hard_threshold,
                    ticksAboveSoft: layer.ticks_above_soft,
                    ticksAboveHard: layer.ticks_above_hard,
                    lastUpdatedTick: layer.last_updated_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_tensions<'a>(
    builder: &mut FbBuilder<'a>,
    tensions: &[CultureTensionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTensionState<'a>>>> {
    let offsets: Vec<_> = tensions
        .iter()
        .map(|state| {
            fb::CultureTensionState::create(
                builder,
                &fb::CultureTensionStateArgs {
                    layerId: state.layer_id,
                    scope: to_fb_culture_layer_scope(state.scope),
                    owner: state.owner,
                    severity: state.severity,
                    timer: state.timer,
                    kind: to_fb_culture_tension_kind(state.kind),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn to_fb_driver_category(category: SentimentDriverCategory) -> fb::SentimentDriverCategory {
    match category {
        SentimentDriverCategory::Policy => fb::SentimentDriverCategory::Policy,
        SentimentDriverCategory::Incident => fb::SentimentDriverCategory::Incident,
        SentimentDriverCategory::Influencer => fb::SentimentDriverCategory::Influencer,
    }
}

fn to_fb_influence_scope(scope: InfluenceScopeKind) -> fb::InfluenceScopeKind {
    match scope {
        InfluenceScopeKind::Local => fb::InfluenceScopeKind::Local,
        InfluenceScopeKind::Regional => fb::InfluenceScopeKind::Regional,
        InfluenceScopeKind::Global => fb::InfluenceScopeKind::Global,
        InfluenceScopeKind::Generation => fb::InfluenceScopeKind::Generation,
    }
}

fn to_fb_influence_lifecycle(lifecycle: InfluenceLifecycle) -> fb::InfluenceLifecycle {
    match lifecycle {
        InfluenceLifecycle::Potential => fb::InfluenceLifecycle::Potential,
        InfluenceLifecycle::Active => fb::InfluenceLifecycle::Active,
        InfluenceLifecycle::Dormant => fb::InfluenceLifecycle::Dormant,
    }
}

fn to_fb_culture_layer_scope(scope: CultureLayerScope) -> fb::CultureLayerScope {
    match scope {
        CultureLayerScope::Global => fb::CultureLayerScope::Global,
        CultureLayerScope::Regional => fb::CultureLayerScope::Regional,
        CultureLayerScope::Local => fb::CultureLayerScope::Local,
    }
}

fn to_fb_culture_trait_axis(axis: CultureTraitAxis) -> fb::CultureTraitAxis {
    match axis {
        CultureTraitAxis::PassiveAggressive => fb::CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed => fb::CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist => {
            fb::CultureTraitAxis::CollectivistIndividualist
        }
        CultureTraitAxis::TraditionalistRevisionist => {
            fb::CultureTraitAxis::TraditionalistRevisionist
        }
        CultureTraitAxis::HierarchicalEgalitarian => fb::CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist => fb::CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent => fb::CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic => fb::CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical => fb::CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular => fb::CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn => fb::CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic => fb::CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented => {
            fb::CultureTraitAxis::MeritOrientedLineageOriented
        }
        CultureTraitAxis::SecularDevout => fb::CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural => fb::CultureTraitAxis::PluralisticMonocultural,
    }
}

fn to_fb_culture_tension_kind(kind: CultureTensionKind) -> fb::CultureTensionKind {
    match kind {
        CultureTensionKind::DriftWarning => fb::CultureTensionKind::DriftWarning,
        CultureTensionKind::AssimilationPush => fb::CultureTensionKind::AssimilationPush,
        CultureTensionKind::SchismRisk => fb::CultureTensionKind::SchismRisk,
    }
}
