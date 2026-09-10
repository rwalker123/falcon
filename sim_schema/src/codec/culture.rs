//! Culture-section FlatBuffers serialization.

use crate::codec::{
    create_scalar_raster, decode_rows, decode_rows_if_present, decode_scalar_raster,
    decode_scalars, text, unknown_enum, DecodeError, FbBuilder,
};
use crate::state::culture::{
    AxisBiasState, CultureLayerScope, CultureLayerState, CultureTensionKind, CultureTensionState,
    CultureTraitAxis, InfluenceLifecycle, InfluenceScopeKind, InfluencerCultureResonanceEntry,
    InfluentialIndividualState, SentimentAxisTelemetry, SentimentDriverCategory,
    SentimentDriverState, SentimentTelemetryState,
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
    // Written only when the section changed: an unconditional (possibly empty) vector would make
    // "unchanged" and "now empty" identical on the wire. See `WorldDelta::culture_tensions`.
    let culture_tensions = delta
        .culture_tensions
        .as_ref()
        .map(|tensions| create_culture_tensions(builder, tensions));
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
            cultureTensions: culture_tensions,
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

fn create_culture_layers<'a>(
    builder: &mut FbBuilder<'a>,
    layers: &[CultureLayerState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureLayerState<'a>>>> {
    let offsets: Vec<_> = layers
        .iter()
        .map(|layer| {
            // ONLY the layer's topology goes to the client (#386).
            //
            // `MapView` reads exactly `id` / `owner` / `parent` / `scope`, and only to walk the
            // tree and resolve a tile's province. It never reads a trait, a divergence or a
            // threshold — the sole consumer of those was the Inspector's Culture tab, which is
            // expendable scaffolding.
            //
            // Those 45 numbers per layer (15 axes x baseline/modifier/value) were the residual
            // cost of delta streaming, and no amount of quantisation could fix it: each drifts
            // ~0.0025/turn, so across 45 of them SOMETHING crosses a hundredths boundary
            // essentially every turn and the layer is always "changed". Culture layers outnumber
            // tiles (4201 vs 4160), so this was the largest single item on the wire.
            // See `docs/plan_delta_streaming.md` §3.6.
            //
            // Topology changes only when the culture tree restructures, so a settled world now
            // sends no culture at all.
            fb::CultureLayerState::create(
                builder,
                &fb::CultureLayerStateArgs {
                    id: layer.id,
                    owner: layer.owner,
                    parent: layer.parent,
                    scope: to_fb_culture_layer_scope(layer.scope),
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

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` / `to_fb_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_culture_section(
    section: fb::CultureSection<'_>,
    snapshot: &mut WorldSnapshot,
) -> Result<(), DecodeError> {
    snapshot.culture_layers = decode_rows(section.cultureLayers(), decode_culture_layer)?;
    snapshot.culture_tensions = decode_rows(section.cultureTensions(), decode_culture_tension)?;
    snapshot.culture_raster = section
        .cultureRaster()
        .map(decode_scalar_raster)
        .unwrap_or_default();
    snapshot.influencers = decode_rows(section.influencers(), decode_influencer)?;
    snapshot.axis_bias = section.axisBias().map(decode_axis_bias).unwrap_or_default();
    snapshot.sentiment = section
        .sentiment()
        .map(decode_sentiment)
        .transpose()?
        .unwrap_or_default();
    snapshot.sentiment_raster = section
        .sentimentRaster()
        .map(decode_scalar_raster)
        .unwrap_or_default();
    Ok(())
}

pub(crate) fn decode_culture_section_delta(
    section: fb::CultureSection<'_>,
    delta: &mut WorldDelta,
) -> Result<(), DecodeError> {
    delta.culture_layers = decode_rows(section.cultureLayers(), decode_culture_layer)?;
    delta.removed_culture_layers = decode_scalars(section.removedCultureLayers());
    // Absent is "unchanged"; present-and-empty is "now empty". See `WorldDelta::culture_tensions`.
    delta.culture_tensions =
        decode_rows_if_present(section.cultureTensions(), decode_culture_tension)?;
    delta.culture_raster = section.cultureRaster().map(decode_scalar_raster);
    delta.influencers = decode_rows(section.influencers(), decode_influencer)?;
    delta.removed_influencers = decode_scalars(section.removedInfluencers());
    delta.axis_bias = section.axisBias().map(decode_axis_bias);
    delta.sentiment = section.sentiment().map(decode_sentiment).transpose()?;
    delta.sentiment_raster = section.sentimentRaster().map(decode_scalar_raster);
    Ok(())
}

fn decode_axis_bias(axis: fb::AxisBiasState<'_>) -> AxisBiasState {
    AxisBiasState {
        knowledge: axis.knowledge(),
        trust: axis.trust(),
        equity: axis.equity(),
        agency: axis.agency(),
    }
}

fn decode_sentiment(
    sentiment: fb::SentimentTelemetryState<'_>,
) -> Result<SentimentTelemetryState, DecodeError> {
    Ok(SentimentTelemetryState {
        knowledge: sentiment
            .knowledge()
            .map(decode_sentiment_axis)
            .transpose()?
            .unwrap_or_default(),
        trust: sentiment
            .trust()
            .map(decode_sentiment_axis)
            .transpose()?
            .unwrap_or_default(),
        equity: sentiment
            .equity()
            .map(decode_sentiment_axis)
            .transpose()?
            .unwrap_or_default(),
        agency: sentiment
            .agency()
            .map(decode_sentiment_axis)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn decode_sentiment_axis(
    axis: fb::SentimentAxisTelemetry<'_>,
) -> Result<SentimentAxisTelemetry, DecodeError> {
    Ok(SentimentAxisTelemetry {
        policy: axis.policy(),
        incidents: axis.incidents(),
        influencers: axis.influencers(),
        total: axis.total(),
        drivers: decode_rows(axis.drivers(), |driver| {
            Ok(SentimentDriverState {
                category: to_state_driver_category(driver.category())?,
                label: text(driver.label()),
                value: driver.value(),
                weight: driver.weight(),
            })
        })?,
    })
}

fn decode_influencer(
    inf: fb::InfluentialIndividualState<'_>,
) -> Result<InfluentialIndividualState, DecodeError> {
    Ok(InfluentialIndividualState {
        id: inf.id(),
        name: text(inf.name()),
        influence: inf.influence(),
        growth_rate: inf.growthRate(),
        baseline_growth: inf.baselineGrowth(),
        notoriety: inf.notoriety(),
        sentiment_knowledge: inf.sentimentKnowledge(),
        sentiment_trust: inf.sentimentTrust(),
        sentiment_equity: inf.sentimentEquity(),
        sentiment_agency: inf.sentimentAgency(),
        sentiment_weight_knowledge: inf.sentimentWeightKnowledge(),
        sentiment_weight_trust: inf.sentimentWeightTrust(),
        sentiment_weight_equity: inf.sentimentWeightEquity(),
        sentiment_weight_agency: inf.sentimentWeightAgency(),
        logistics_bonus: inf.logisticsBonus(),
        morale_bonus: inf.moraleBonus(),
        power_bonus: inf.powerBonus(),
        logistics_weight: inf.logisticsWeight(),
        morale_weight: inf.moraleWeight(),
        power_weight: inf.powerWeight(),
        support_charge: inf.supportCharge(),
        suppress_pressure: inf.suppressPressure(),
        domains: inf.domains(),
        scope: to_state_influence_scope(inf.scope())?,
        generation_scope: inf.generationScope(),
        supported: inf.supported(),
        suppressed: inf.suppressed(),
        lifecycle: to_state_influence_lifecycle(inf.lifecycle())?,
        coherence: inf.coherence(),
        ticks_in_status: inf.ticksInStatus(),
        audience_generations: decode_scalars(inf.audienceGenerations()),
        support_popular: inf.supportPopular(),
        support_peer: inf.supportPeer(),
        support_institutional: inf.supportInstitutional(),
        support_humanitarian: inf.supportHumanitarian(),
        weight_popular: inf.weightPopular(),
        weight_peer: inf.weightPeer(),
        weight_institutional: inf.weightInstitutional(),
        weight_humanitarian: inf.weightHumanitarian(),
        culture_resonance: decode_rows(inf.cultureResonance(), |entry| {
            Ok(InfluencerCultureResonanceEntry {
                axis: to_state_culture_trait_axis(entry.axis())?,
                weight: entry.weight(),
                output: entry.output(),
            })
        })?,
    })
}

/// ONLY the layer's topology crosses the wire (#386, see `create_culture_layers`), so the trait
/// vector and the divergence fields come back at their zero value: nothing was sent, and there
/// is nothing to read.
fn decode_culture_layer(
    layer: fb::CultureLayerState<'_>,
) -> Result<CultureLayerState, DecodeError> {
    Ok(CultureLayerState {
        id: layer.id(),
        owner: layer.owner(),
        parent: layer.parent(),
        scope: to_state_culture_layer_scope(layer.scope())?,
        traits: Vec::new(),
        divergence: 0,
        soft_threshold: 0,
        hard_threshold: 0,
        ticks_above_soft: 0,
        ticks_above_hard: 0,
        last_updated_tick: 0,
    })
}

fn decode_culture_tension(
    state: fb::CultureTensionState<'_>,
) -> Result<CultureTensionState, DecodeError> {
    Ok(CultureTensionState {
        layer_id: state.layerId(),
        scope: to_state_culture_layer_scope(state.scope())?,
        owner: state.owner(),
        severity: state.severity(),
        timer: state.timer(),
        kind: to_state_culture_tension_kind(state.kind())?,
    })
}

fn to_state_driver_category(
    category: fb::SentimentDriverCategory,
) -> Result<SentimentDriverCategory, DecodeError> {
    Ok(match category {
        fb::SentimentDriverCategory::Policy => SentimentDriverCategory::Policy,
        fb::SentimentDriverCategory::Incident => SentimentDriverCategory::Incident,
        fb::SentimentDriverCategory::Influencer => SentimentDriverCategory::Influencer,
        other => return Err(unknown_enum("SentimentDriverCategory", other.0)),
    })
}

fn to_state_influence_scope(
    scope: fb::InfluenceScopeKind,
) -> Result<InfluenceScopeKind, DecodeError> {
    Ok(match scope {
        fb::InfluenceScopeKind::Local => InfluenceScopeKind::Local,
        fb::InfluenceScopeKind::Regional => InfluenceScopeKind::Regional,
        fb::InfluenceScopeKind::Global => InfluenceScopeKind::Global,
        fb::InfluenceScopeKind::Generation => InfluenceScopeKind::Generation,
        other => return Err(unknown_enum("InfluenceScopeKind", other.0)),
    })
}

fn to_state_influence_lifecycle(
    lifecycle: fb::InfluenceLifecycle,
) -> Result<InfluenceLifecycle, DecodeError> {
    Ok(match lifecycle {
        fb::InfluenceLifecycle::Potential => InfluenceLifecycle::Potential,
        fb::InfluenceLifecycle::Active => InfluenceLifecycle::Active,
        fb::InfluenceLifecycle::Dormant => InfluenceLifecycle::Dormant,
        other => return Err(unknown_enum("InfluenceLifecycle", other.0)),
    })
}

fn to_state_culture_layer_scope(
    scope: fb::CultureLayerScope,
) -> Result<CultureLayerScope, DecodeError> {
    Ok(match scope {
        fb::CultureLayerScope::Global => CultureLayerScope::Global,
        fb::CultureLayerScope::Regional => CultureLayerScope::Regional,
        fb::CultureLayerScope::Local => CultureLayerScope::Local,
        other => return Err(unknown_enum("CultureLayerScope", other.0)),
    })
}

fn to_state_culture_trait_axis(
    axis: fb::CultureTraitAxis,
) -> Result<CultureTraitAxis, DecodeError> {
    Ok(match axis {
        fb::CultureTraitAxis::PassiveAggressive => CultureTraitAxis::PassiveAggressive,
        fb::CultureTraitAxis::OpenClosed => CultureTraitAxis::OpenClosed,
        fb::CultureTraitAxis::CollectivistIndividualist => {
            CultureTraitAxis::CollectivistIndividualist
        }
        fb::CultureTraitAxis::TraditionalistRevisionist => {
            CultureTraitAxis::TraditionalistRevisionist
        }
        fb::CultureTraitAxis::HierarchicalEgalitarian => CultureTraitAxis::HierarchicalEgalitarian,
        fb::CultureTraitAxis::SyncreticPurist => CultureTraitAxis::SyncreticPurist,
        fb::CultureTraitAxis::AsceticIndulgent => CultureTraitAxis::AsceticIndulgent,
        fb::CultureTraitAxis::PragmaticIdealistic => CultureTraitAxis::PragmaticIdealistic,
        fb::CultureTraitAxis::RationalistMystical => CultureTraitAxis::RationalistMystical,
        fb::CultureTraitAxis::ExpansionistInsular => CultureTraitAxis::ExpansionistInsular,
        fb::CultureTraitAxis::AdaptiveStubborn => CultureTraitAxis::AdaptiveStubborn,
        fb::CultureTraitAxis::HonorBoundOpportunistic => CultureTraitAxis::HonorBoundOpportunistic,
        fb::CultureTraitAxis::MeritOrientedLineageOriented => {
            CultureTraitAxis::MeritOrientedLineageOriented
        }
        fb::CultureTraitAxis::SecularDevout => CultureTraitAxis::SecularDevout,
        fb::CultureTraitAxis::PluralisticMonocultural => CultureTraitAxis::PluralisticMonocultural,
        other => return Err(unknown_enum("CultureTraitAxis", other.0)),
    })
}

fn to_state_culture_tension_kind(
    kind: fb::CultureTensionKind,
) -> Result<CultureTensionKind, DecodeError> {
    Ok(match kind {
        fb::CultureTensionKind::DriftWarning => CultureTensionKind::DriftWarning,
        fb::CultureTensionKind::AssimilationPush => CultureTensionKind::AssimilationPush,
        fb::CultureTensionKind::SchismRisk => CultureTensionKind::SchismRisk,
        other => return Err(unknown_enum("CultureTensionKind", other.0)),
    })
}

#[cfg(test)]
mod enum_round_trip_tests {
    use super::*;

    /// Every variant the encoder can write, the decoder reads back as the same variant.
    #[test]
    fn every_culture_enum_variant_round_trips() {
        for category in [
            SentimentDriverCategory::Policy,
            SentimentDriverCategory::Incident,
            SentimentDriverCategory::Influencer,
        ] {
            assert_eq!(
                to_state_driver_category(to_fb_driver_category(category)).expect("known"),
                category
            );
        }
        for scope in [
            InfluenceScopeKind::Local,
            InfluenceScopeKind::Regional,
            InfluenceScopeKind::Global,
            InfluenceScopeKind::Generation,
        ] {
            assert_eq!(
                to_state_influence_scope(to_fb_influence_scope(scope)).expect("known"),
                scope
            );
        }
        for lifecycle in [
            InfluenceLifecycle::Potential,
            InfluenceLifecycle::Active,
            InfluenceLifecycle::Dormant,
        ] {
            assert_eq!(
                to_state_influence_lifecycle(to_fb_influence_lifecycle(lifecycle)).expect("known"),
                lifecycle
            );
        }
        for scope in [
            CultureLayerScope::Global,
            CultureLayerScope::Regional,
            CultureLayerScope::Local,
        ] {
            assert_eq!(
                to_state_culture_layer_scope(to_fb_culture_layer_scope(scope)).expect("known"),
                scope
            );
        }
        for axis in CultureTraitAxis::ALL {
            assert_eq!(
                to_state_culture_trait_axis(to_fb_culture_trait_axis(axis)).expect("known"),
                axis
            );
        }
        for kind in [
            CultureTensionKind::DriftWarning,
            CultureTensionKind::AssimilationPush,
            CultureTensionKind::SchismRisk,
        ] {
            assert_eq!(
                to_state_culture_tension_kind(to_fb_culture_tension_kind(kind)).expect("known"),
                kind
            );
        }
    }
}
