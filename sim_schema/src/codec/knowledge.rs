//! Knowledge-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::knowledge::{
    DiscoveredSitesState, DiscoveryProgressEntry, GreatDiscoveryDefinitionState,
    GreatDiscoveryProgressState, GreatDiscoveryRequirementState, GreatDiscoveryState,
    GreatDiscoveryTelemetryState, KnowledgeCountermeasureKind, KnowledgeCountermeasureState,
    KnowledgeField, KnowledgeInfiltrationState, KnowledgeLedgerEntryState, KnowledgeMetricsState,
    KnowledgeModifierBreakdownState, KnowledgeModifierSource, KnowledgeSecurityPosture,
    KnowledgeTimelineEventKind, KnowledgeTimelineEventState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_knowledge_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::KnowledgeSection<'a>> {
    let great_discovery_definitions =
        create_great_discovery_definitions(builder, &snapshot.great_discovery_definitions);
    let great_discoveries = create_great_discoveries(builder, &snapshot.great_discoveries);
    let great_discovery_progress =
        create_great_discovery_progress(builder, &snapshot.great_discovery_progress);
    let great_discovery_telemetry =
        create_great_discovery_telemetry(builder, &snapshot.great_discovery_telemetry);
    let knowledge_ledger = create_knowledge_ledger(builder, &snapshot.knowledge_ledger);
    let knowledge_timeline = create_knowledge_timeline(builder, &snapshot.knowledge_timeline);
    let knowledge_metrics = create_knowledge_metrics(builder, &snapshot.knowledge_metrics);
    let discovered_sites = create_discovered_sites(builder, &snapshot.discovered_sites);
    let discovery_progress = create_discovery_progress(builder, &snapshot.discovery_progress);
    fb::KnowledgeSection::create(
        builder,
        &fb::KnowledgeSectionArgs {
            greatDiscoveryDefinitions: Some(great_discovery_definitions),
            greatDiscoveries: Some(great_discoveries),
            greatDiscoveryProgress: Some(great_discovery_progress),
            greatDiscoveryTelemetry: Some(great_discovery_telemetry),
            knowledgeLedger: Some(knowledge_ledger),
            knowledgeTimeline: Some(knowledge_timeline),
            knowledgeMetrics: Some(knowledge_metrics),
            discoveredSites: Some(discovered_sites),
            discoveryProgress: Some(discovery_progress),
            removedKnowledgeLedger: None,
        },
    )
}

pub(crate) fn serialize_knowledge_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::KnowledgeSection<'a>> {
    let great_discovery_definitions = delta
        .great_discovery_definitions
        .as_ref()
        .map(|definitions| create_great_discovery_definitions(builder, definitions));
    let great_discoveries = create_great_discoveries(builder, &delta.great_discoveries);
    let great_discovery_progress =
        create_great_discovery_progress(builder, &delta.great_discovery_progress);
    let great_discovery_telemetry = delta
        .great_discovery_telemetry
        .as_ref()
        .map(|telemetry| create_great_discovery_telemetry(builder, telemetry));
    let knowledge_ledger = create_knowledge_ledger(builder, &delta.knowledge_ledger);
    let removed_knowledge_ledger = builder.create_vector(&delta.removed_knowledge_ledger);
    let knowledge_timeline = create_knowledge_timeline(builder, &delta.knowledge_timeline);
    let knowledge_metrics = delta
        .knowledge_metrics
        .as_ref()
        .map(|metrics| create_knowledge_metrics(builder, metrics));
    let discovered_sites = delta
        .discovered_sites
        .as_ref()
        .map(|entries| create_discovered_sites(builder, entries));
    let discovery_progress = create_discovery_progress(builder, &delta.discovery_progress);
    fb::KnowledgeSection::create(
        builder,
        &fb::KnowledgeSectionArgs {
            greatDiscoveryDefinitions: great_discovery_definitions,
            greatDiscoveries: Some(great_discoveries),
            greatDiscoveryProgress: Some(great_discovery_progress),
            greatDiscoveryTelemetry: great_discovery_telemetry,
            knowledgeLedger: Some(knowledge_ledger),
            knowledgeTimeline: Some(knowledge_timeline),
            knowledgeMetrics: knowledge_metrics,
            discoveredSites: discovered_sites,
            discoveryProgress: Some(discovery_progress),
            removedKnowledgeLedger: Some(removed_knowledge_ledger),
        },
    )
}

fn create_discovered_sites<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[DiscoveredSitesState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DiscoveredSitesState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let mut site_offsets = Vec::with_capacity(state.sites.len());
        for site in &state.sites {
            let site_id = builder.create_string(site.site_id.as_str());
            let category = builder.create_string(site.category.as_str());
            let display_name = builder.create_string(site.display_name.as_str());
            let glyph = builder.create_string(site.glyph.as_str());
            let site_offset = fb::DiscoveredSite::create(
                builder,
                &fb::DiscoveredSiteArgs {
                    x: site.x,
                    y: site.y,
                    site_id: Some(site_id),
                    display_name: Some(display_name),
                    category: Some(category),
                    glyph: Some(glyph),
                },
            );
            site_offsets.push(site_offset);
        }
        let sites_vec = builder.create_vector(&site_offsets);
        let entry = fb::DiscoveredSitesState::create(
            builder,
            &fb::DiscoveredSitesStateArgs {
                faction: state.faction,
                sites: Some(sites_vec),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[DiscoveryProgressEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DiscoveryProgressEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::DiscoveryProgressEntry::create(
                builder,
                &fb::DiscoveryProgressEntryArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn to_fb_knowledge_field(field: KnowledgeField) -> fb::KnowledgeField {
    match field {
        KnowledgeField::Physics => fb::KnowledgeField::Physics,
        KnowledgeField::Chemistry => fb::KnowledgeField::Chemistry,
        KnowledgeField::Biology => fb::KnowledgeField::Biology,
        KnowledgeField::Data => fb::KnowledgeField::Data,
        KnowledgeField::Communication => fb::KnowledgeField::Communication,
        KnowledgeField::Exotic => fb::KnowledgeField::Exotic,
    }
}

fn create_great_discoveries<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryState::create(
                builder,
                &fb::GreatDiscoveryStateArgs {
                    id: entry.id,
                    faction: entry.faction,
                    field: to_fb_knowledge_field(entry.field),
                    tick: entry.tick,
                    publiclyDeployed: entry.publicly_deployed,
                    effectFlags: entry.effect_flags,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definition_requirements<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryRequirementState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryRequirementDefinition<'a>>>>
{
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = entry
                .name
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            fb::GreatDiscoveryRequirementDefinition::create(
                builder,
                &fb::GreatDiscoveryRequirementDefinitionArgs {
                    discoveryId: entry.discovery,
                    weight: entry.weight,
                    minimumProgress: entry.minimum_progress,
                    name,
                    summary,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definitions<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryDefinitionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryDefinition<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = builder.create_string(entry.name.as_str());
            let tier = entry
                .tier
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let tags = if entry.tags.is_empty() {
                None
            } else {
                let mut tag_offsets = Vec::with_capacity(entry.tags.len());
                for tag in &entry.tags {
                    tag_offsets.push(builder.create_string(tag.as_str()));
                }
                Some(builder.create_vector(&tag_offsets))
            };
            let effects_summary = if entry.effects_summary.is_empty() {
                None
            } else {
                let mut effect_offsets = Vec::with_capacity(entry.effects_summary.len());
                for line in &entry.effects_summary {
                    effect_offsets.push(builder.create_string(line.as_str()));
                }
                Some(builder.create_vector(&effect_offsets))
            };
            let observation_notes = entry
                .observation_notes
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let leak_profile = entry
                .leak_profile
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let requirements =
                create_great_discovery_definition_requirements(builder, &entry.requirements);

            fb::GreatDiscoveryDefinition::create(
                builder,
                &fb::GreatDiscoveryDefinitionArgs {
                    id: entry.id,
                    name: Some(name),
                    field: to_fb_knowledge_field(entry.field),
                    observationThreshold: entry.observation_threshold,
                    cooldownTicks: entry.cooldown_ticks,
                    freshnessWindow: entry.freshness_window.unwrap_or_default(),
                    hasFreshnessWindow: entry.freshness_window.is_some(),
                    effectFlags: entry.effect_flags,
                    covertUntilPublic: entry.covert_until_public,
                    tier,
                    summary,
                    tags,
                    effectsSummary: effects_summary,
                    observationNotes: observation_notes,
                    leakProfile: leak_profile,
                    requirements: Some(requirements),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryProgressState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryProgressState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryProgressState::create(
                builder,
                &fb::GreatDiscoveryProgressStateArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                    observationDeficit: entry.observation_deficit,
                    etaTicks: entry.eta_ticks,
                    covert: entry.covert,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &GreatDiscoveryTelemetryState,
) -> WIPOffset<fb::GreatDiscoveryTelemetryState<'a>> {
    fb::GreatDiscoveryTelemetryState::create(
        builder,
        &fb::GreatDiscoveryTelemetryStateArgs {
            totalResolved: telemetry.total_resolved,
            pendingCandidates: telemetry.pending_candidates,
            activeConstellations: telemetry.active_constellations,
        },
    )
}

fn create_knowledge_ledger<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeLedgerEntryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeLedgerState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let countermeasures = create_knowledge_countermeasures(builder, &entry.countermeasures);
            let infiltrations = create_knowledge_infiltrations(builder, &entry.infiltrations);
            let modifiers = create_knowledge_modifiers(builder, &entry.modifiers);
            fb::KnowledgeLedgerState::create(
                builder,
                &fb::KnowledgeLedgerStateArgs {
                    discoveryId: entry.discovery_id,
                    ownerFaction: entry.owner_faction,
                    tier: entry.tier,
                    progressPercent: entry.progress_percent,
                    halfLifeTicks: entry.half_life_ticks,
                    timeToCascade: entry.time_to_cascade,
                    securityPosture: to_fb_knowledge_security_posture(entry.security_posture),
                    countermeasures: Some(countermeasures),
                    infiltrations: Some(infiltrations),
                    modifiers: Some(modifiers),
                    flags: entry.flags.bits(),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_countermeasures<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeCountermeasureState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeCountermeasureState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeCountermeasureState::create(
                builder,
                &fb::KnowledgeCountermeasureStateArgs {
                    kind: to_fb_knowledge_countermeasure(entry.kind),
                    potency: entry.potency,
                    upkeep: entry.upkeep,
                    remainingTicks: entry.remaining_ticks,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_infiltrations<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeInfiltrationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeInfiltrationState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeInfiltrationState::create(
                builder,
                &fb::KnowledgeInfiltrationStateArgs {
                    faction: entry.faction,
                    blueprintFidelity: entry.blueprint_fidelity,
                    suspicion: entry.suspicion,
                    cells: entry.cells,
                    lastActivityTick: entry.last_activity_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_modifiers<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeModifierBreakdownState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeModifierBreakdownState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let note = entry
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeModifierBreakdownState::create(
                builder,
                &fb::KnowledgeModifierBreakdownStateArgs {
                    source: to_fb_knowledge_modifier_source(entry.source),
                    deltaHalfLife: entry.delta_half_life,
                    deltaProgress: entry.delta_progress,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_timeline<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[KnowledgeTimelineEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeTimelineEventState<'a>>>> {
    let offsets: Vec<_> = events
        .iter()
        .map(|event| {
            let note = event
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeTimelineEventState::create(
                builder,
                &fb::KnowledgeTimelineEventStateArgs {
                    tick: event.tick,
                    kind: to_fb_knowledge_timeline_kind(event.kind),
                    sourceFaction: event.source_faction,
                    deltaPercent: event.delta_percent,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &KnowledgeMetricsState,
) -> WIPOffset<fb::KnowledgeMetricsState<'a>> {
    fb::KnowledgeMetricsState::create(
        builder,
        &fb::KnowledgeMetricsStateArgs {
            leakWarnings: metrics.leak_warnings,
            leakCriticals: metrics.leak_criticals,
            countermeasuresActive: metrics.countermeasures_active,
            commonKnowledgeTotal: metrics.common_knowledge_total,
        },
    )
}

fn to_fb_knowledge_security_posture(
    posture: KnowledgeSecurityPosture,
) -> fb::KnowledgeSecurityPosture {
    match posture {
        KnowledgeSecurityPosture::Minimal => fb::KnowledgeSecurityPosture::Minimal,
        KnowledgeSecurityPosture::Standard => fb::KnowledgeSecurityPosture::Standard,
        KnowledgeSecurityPosture::Hardened => fb::KnowledgeSecurityPosture::Hardened,
        KnowledgeSecurityPosture::BlackVault => fb::KnowledgeSecurityPosture::BlackVault,
    }
}

fn to_fb_knowledge_countermeasure(
    kind: KnowledgeCountermeasureKind,
) -> fb::KnowledgeCountermeasureKind {
    match kind {
        KnowledgeCountermeasureKind::SecurityInvestment => {
            fb::KnowledgeCountermeasureKind::SecurityInvestment
        }
        KnowledgeCountermeasureKind::CounterIntelSweep => {
            fb::KnowledgeCountermeasureKind::CounterIntelSweep
        }
        KnowledgeCountermeasureKind::Misinformation => {
            fb::KnowledgeCountermeasureKind::Misinformation
        }
        KnowledgeCountermeasureKind::KnowledgeDebtRelief => {
            fb::KnowledgeCountermeasureKind::KnowledgeDebtRelief
        }
    }
}

fn to_fb_knowledge_modifier_source(source: KnowledgeModifierSource) -> fb::KnowledgeModifierSource {
    match source {
        KnowledgeModifierSource::Visibility => fb::KnowledgeModifierSource::Visibility,
        KnowledgeModifierSource::Security => fb::KnowledgeModifierSource::Security,
        KnowledgeModifierSource::Spycraft => fb::KnowledgeModifierSource::Spycraft,
        KnowledgeModifierSource::Culture => fb::KnowledgeModifierSource::Culture,
        KnowledgeModifierSource::Exposure => fb::KnowledgeModifierSource::Exposure,
        KnowledgeModifierSource::Debt => fb::KnowledgeModifierSource::Debt,
        KnowledgeModifierSource::Treaty => fb::KnowledgeModifierSource::Treaty,
        KnowledgeModifierSource::Event => fb::KnowledgeModifierSource::Event,
    }
}

fn to_fb_knowledge_timeline_kind(
    kind: KnowledgeTimelineEventKind,
) -> fb::KnowledgeTimelineEventKind {
    match kind {
        KnowledgeTimelineEventKind::LeakProgress => fb::KnowledgeTimelineEventKind::LeakProgress,
        KnowledgeTimelineEventKind::SpyProbe => fb::KnowledgeTimelineEventKind::SpyProbe,
        KnowledgeTimelineEventKind::CounterIntel => fb::KnowledgeTimelineEventKind::CounterIntel,
        KnowledgeTimelineEventKind::Exposure => fb::KnowledgeTimelineEventKind::Exposure,
        KnowledgeTimelineEventKind::Treaty => fb::KnowledgeTimelineEventKind::Treaty,
        KnowledgeTimelineEventKind::Cascade => fb::KnowledgeTimelineEventKind::Cascade,
        KnowledgeTimelineEventKind::Digest => fb::KnowledgeTimelineEventKind::Digest,
    }
}
