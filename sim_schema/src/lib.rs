use ahash::RandomState;
use flatbuffers::{DefaultAllocator, FlatBufferBuilder, ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};

type FbBuilder<'a> = FlatBufferBuilder<'a, DefaultAllocator>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub tile_count: u32,
    pub logistics_count: u32,
    pub population_count: u32,
    pub power_count: u32,
    pub influencer_count: u32,
    pub hash: u64,
}

impl SnapshotHeader {
    pub fn new(
        tick: u64,
        tile_count: usize,
        logistics_count: usize,
        population_count: usize,
        power_count: usize,
        influencer_count: usize,
    ) -> Self {
        Self {
            tick,
            tile_count: tile_count as u32,
            logistics_count: logistics_count as u32,
            population_count: population_count as u32,
            power_count: power_count as u32,
            influencer_count: influencer_count as u32,
            hash: 0,
        }
    }
}

impl Default for SnapshotHeader {
    fn default() -> Self {
        Self {
            tick: 0,
            tile_count: 0,
            logistics_count: 0,
            population_count: 0,
            power_count: 0,
            influencer_count: 0,
            hash: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileState {
    pub entity: u64,
    pub x: u32,
    pub y: u32,
    pub element: u8,
    pub mass: i64,
    pub temperature: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogisticsLinkState {
    pub entity: u64,
    pub from: u64,
    pub to: u64,
    pub capacity: i64,
    pub flow: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PopulationCohortState {
    pub entity: u64,
    pub home: u64,
    pub size: u32,
    pub morale: i64,
    pub generation: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowerNodeState {
    pub entity: u64,
    pub generation: i64,
    pub demand: i64,
    pub efficiency: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CorruptionSubsystem {
    Logistics = 0,
    Trade = 1,
    Military = 2,
    Governance = 3,
}

impl Default for CorruptionSubsystem {
    fn default() -> Self {
        CorruptionSubsystem::Logistics
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorruptionEntry {
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub incident_id: u64,
    pub exposure_timer: u16,
    pub restitution_window: u16,
    pub last_update_tick: u64,
}

impl Default for CorruptionEntry {
    fn default() -> Self {
        Self {
            subsystem: CorruptionSubsystem::default(),
            intensity: 0,
            incident_id: 0,
            exposure_timer: 0,
            restitution_window: 0,
            last_update_tick: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionLedger {
    pub entries: Vec<CorruptionEntry>,
    pub reputation_modifier: i64,
    pub audit_capacity: u16,
}

impl CorruptionLedger {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn register_incident(&mut self, entry: CorruptionEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.incident_id == entry.incident_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn incident_mut(&mut self, incident_id: u64) -> Option<&mut CorruptionEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.incident_id == incident_id)
    }

    pub fn remove_incident(&mut self, incident_id: u64) -> Option<CorruptionEntry> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.incident_id == incident_id)?;
        Some(self.entries.remove(index))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceDomain {
    Sentiment = 0,
    Discovery = 1,
    Logistics = 2,
    Production = 3,
    Humanitarian = 4,
}

impl InfluenceDomain {
    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

pub fn influence_domain_mask(domains: &[InfluenceDomain]) -> u32 {
    domains.iter().fold(0u32, |acc, domain| acc | domain.bit())
}

pub fn influence_domains_from_mask(mask: u32) -> Vec<InfluenceDomain> {
    let mut domains = Vec::new();
    for value in 0..=4 {
        let domain = match value {
            0 => InfluenceDomain::Sentiment,
            1 => InfluenceDomain::Discovery,
            2 => InfluenceDomain::Logistics,
            3 => InfluenceDomain::Production,
            4 => InfluenceDomain::Humanitarian,
            _ => continue,
        };
        if mask & domain.bit() != 0 {
            domains.push(domain);
        }
    }
    domains
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceScopeKind {
    Local = 0,
    Regional = 1,
    Global = 2,
    Generation = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceLifecycle {
    Potential = 0,
    Active = 1,
    Dormant = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfluentialIndividualState {
    pub id: u32,
    pub name: String,
    pub influence: i64,
    pub growth_rate: i64,
    pub baseline_growth: i64,
    pub notoriety: i64,
    pub sentiment_knowledge: i64,
    pub sentiment_trust: i64,
    pub sentiment_equity: i64,
    pub sentiment_agency: i64,
    pub sentiment_weight_knowledge: i64,
    pub sentiment_weight_trust: i64,
    pub sentiment_weight_equity: i64,
    pub sentiment_weight_agency: i64,
    pub logistics_bonus: i64,
    pub morale_bonus: i64,
    pub power_bonus: i64,
    pub logistics_weight: i64,
    pub morale_weight: i64,
    pub power_weight: i64,
    pub support_charge: i64,
    pub suppress_pressure: i64,
    pub domains: u32,
    pub scope: InfluenceScopeKind,
    pub generation_scope: u16,
    pub supported: bool,
    pub suppressed: bool,
    pub lifecycle: InfluenceLifecycle,
    pub coherence: i64,
    pub ticks_in_status: u16,
    pub audience_generations: Vec<u16>,
    pub support_popular: i64,
    pub support_peer: i64,
    pub support_institutional: i64,
    pub support_humanitarian: i64,
    pub weight_popular: i64,
    pub weight_peer: i64,
    pub weight_institutional: i64,
    pub weight_humanitarian: i64,
}

impl InfluentialIndividualState {
    pub const NO_GENERATION_SCOPE: u16 = u16::MAX;

    pub fn generation_scope(&self) -> Option<u16> {
        if self.generation_scope == Self::NO_GENERATION_SCOPE {
            None
        } else {
            Some(self.generation_scope)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AxisBiasState {
    pub knowledge: i64,
    pub trust: i64,
    pub equity: i64,
    pub agency: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum SentimentDriverCategory {
    Policy = 0,
    Incident = 1,
    Influencer = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SentimentDriverState {
    pub category: SentimentDriverCategory,
    pub label: String,
    pub value: i64,
    pub weight: i64,
}

impl Default for SentimentDriverState {
    fn default() -> Self {
        Self {
            category: SentimentDriverCategory::Policy,
            label: String::new(),
            value: 0,
            weight: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentAxisTelemetry {
    pub policy: i64,
    pub incidents: i64,
    pub influencers: i64,
    pub total: i64,
    pub drivers: Vec<SentimentDriverState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentTelemetryState {
    pub knowledge: SentimentAxisTelemetry,
    pub trust: SentimentAxisTelemetry,
    pub equity: SentimentAxisTelemetry,
    pub agency: SentimentAxisTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub logistics: Vec<LogisticsLinkState>,
    pub populations: Vec<PopulationCohortState>,
    pub power: Vec<PowerNodeState>,
    pub axis_bias: AxisBiasState,
    pub sentiment: SentimentTelemetryState,
    pub generations: Vec<GenerationState>,
    pub corruption: CorruptionLedger,
    pub influencers: Vec<InfluentialIndividualState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldDelta {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub removed_tiles: Vec<u64>,
    pub logistics: Vec<LogisticsLinkState>,
    pub removed_logistics: Vec<u64>,
    pub populations: Vec<PopulationCohortState>,
    pub removed_populations: Vec<u64>,
    pub power: Vec<PowerNodeState>,
    pub removed_power: Vec<u64>,
    pub axis_bias: Option<AxisBiasState>,
    pub sentiment: Option<SentimentTelemetryState>,
    pub generations: Vec<GenerationState>,
    pub removed_generations: Vec<u16>,
    pub corruption: Option<CorruptionLedger>,
    pub influencers: Vec<InfluentialIndividualState>,
    pub removed_influencers: Vec<u32>,
}

impl WorldSnapshot {
    pub fn finalize(mut self) -> Self {
        let hash = hash_snapshot(&self);
        let mut header = self.header;
        header.hash = hash;
        self.header = header;
        self
    }
}

pub fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut clone = snapshot.clone();
    clone.header.hash = 0;
    let encoded = bincode::serialize(&clone).expect("snapshot serialization for hashing");
    let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(&encoded);
    hasher.finish()
}

pub fn encode_snapshot(snapshot: &WorldSnapshot) -> bincode::Result<Vec<u8>> {
    bincode::serialize(snapshot)
}

pub fn encode_delta(delta: &WorldDelta) -> bincode::Result<Vec<u8>> {
    bincode::serialize(delta)
}

pub fn encode_snapshot_flatbuffer(snapshot: &WorldSnapshot) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_snapshot_flatbuffer(&mut builder, snapshot);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_delta_flatbuffer(delta: &WorldDelta) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_delta_flatbuffer(&mut builder, delta);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_snapshot_json(snapshot: &WorldSnapshot) -> serde_json::Result<String> {
    serde_json::to_string(snapshot)
}

pub fn decode_snapshot_json(data: &str) -> serde_json::Result<WorldSnapshot> {
    serde_json::from_str(data)
}

pub fn encode_delta_json(delta: &WorldDelta) -> serde_json::Result<String> {
    serde_json::to_string(delta)
}

pub fn decode_delta_json(data: &str) -> serde_json::Result<WorldDelta> {
    serde_json::from_str(data)
}

fn build_snapshot_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::Envelope<'a>> {
    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: snapshot.header.tick,
            tileCount: snapshot.header.tile_count,
            logisticsCount: snapshot.header.logistics_count,
            populationCount: snapshot.header.population_count,
            powerCount: snapshot.header.power_count,
            influencerCount: snapshot.header.influencer_count,
            hash: snapshot.header.hash,
        },
    );

    let tiles_vec = create_tiles(builder, &snapshot.tiles);
    let logistics_vec = create_logistics(builder, &snapshot.logistics);
    let populations_vec = create_populations(builder, &snapshot.populations);
    let power_vec = create_power(builder, &snapshot.power);
    let axis_bias = fb::AxisBiasState::create(
        builder,
        &fb::AxisBiasStateArgs {
            knowledge: snapshot.axis_bias.knowledge,
            trust: snapshot.axis_bias.trust,
            equity: snapshot.axis_bias.equity,
            agency: snapshot.axis_bias.agency,
            ..Default::default()
        },
    );
    let sentiment = create_sentiment(builder, &snapshot.sentiment);
    let generations_vec = create_generations(builder, &snapshot.generations);
    let corruption = create_corruption(builder, &snapshot.corruption);
    let influencers_vec = create_influencers(builder, &snapshot.influencers);

    let snapshot_table = fb::WorldSnapshot::create(
        builder,
        &fb::WorldSnapshotArgs {
            header: Some(header),
            tiles: Some(tiles_vec),
            logistics: Some(logistics_vec),
            populations: Some(populations_vec),
            power: Some(power_vec),
            axisBias: Some(axis_bias),
            sentiment: Some(sentiment),
            generations: Some(generations_vec),
            corruption: Some(corruption),
            influencers: Some(influencers_vec),
            ..Default::default()
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::snapshot,
            payload: Some(snapshot_table.as_union_value()),
            ..Default::default()
        },
    )
}

fn build_delta_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::Envelope<'a>> {
    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: delta.header.tick,
            tileCount: delta.header.tile_count,
            logisticsCount: delta.header.logistics_count,
            populationCount: delta.header.population_count,
            powerCount: delta.header.power_count,
            influencerCount: delta.header.influencer_count,
            hash: delta.header.hash,
        },
    );

    let tiles_vec = create_tiles(builder, &delta.tiles);
    let removed_tiles_vec = builder.create_vector(&delta.removed_tiles);
    let logistics_vec = create_logistics(builder, &delta.logistics);
    let removed_logistics_vec = builder.create_vector(&delta.removed_logistics);
    let populations_vec = create_populations(builder, &delta.populations);
    let removed_populations_vec = builder.create_vector(&delta.removed_populations);
    let power_vec = create_power(builder, &delta.power);
    let removed_power_vec = builder.create_vector(&delta.removed_power);
    let axis_bias = delta.axis_bias.as_ref().map(|axis| {
        fb::AxisBiasState::create(
            builder,
            &fb::AxisBiasStateArgs {
                knowledge: axis.knowledge,
                trust: axis.trust,
                equity: axis.equity,
                agency: axis.agency,
                ..Default::default()
            },
        )
    });
    let sentiment = delta
        .sentiment
        .as_ref()
        .map(|s| create_sentiment(builder, s));
    let generations_vec = create_generations(builder, &delta.generations);
    let removed_generations_vec = builder.create_vector(&delta.removed_generations);
    let corruption = delta
        .corruption
        .as_ref()
        .map(|c| create_corruption(builder, c));
    let influencers_vec = create_influencers(builder, &delta.influencers);
    let removed_influencers_vec = builder.create_vector(&delta.removed_influencers);

    let delta_table = fb::WorldDelta::create(
        builder,
        &fb::WorldDeltaArgs {
            header: Some(header),
            tiles: Some(tiles_vec),
            removedTiles: Some(removed_tiles_vec),
            logistics: Some(logistics_vec),
            removedLogistics: Some(removed_logistics_vec),
            populations: Some(populations_vec),
            removedPopulations: Some(removed_populations_vec),
            power: Some(power_vec),
            removedPower: Some(removed_power_vec),
            axisBias: axis_bias,
            sentiment,
            generations: Some(generations_vec),
            removedGenerations: Some(removed_generations_vec),
            corruption,
            influencers: Some(influencers_vec),
            removedInfluencers: Some(removed_influencers_vec),
            ..Default::default()
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::delta,
            payload: Some(delta_table.as_union_value()),
            ..Default::default()
        },
    )
}

fn create_tiles<'a>(
    builder: &mut FbBuilder<'a>,
    tiles: &[TileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TileState<'a>>>> {
    let offsets: Vec<_> = tiles
        .iter()
        .map(|tile| {
            fb::TileState::create(
                builder,
                &fb::TileStateArgs {
                    entity: tile.entity,
                    x: tile.x,
                    y: tile.y,
                    element: tile.element,
                    mass: tile.mass,
                    temperature: tile.temperature,
                    ..Default::default()
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_logistics<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[LogisticsLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LogisticsLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            fb::LogisticsLinkState::create(
                builder,
                &fb::LogisticsLinkStateArgs {
                    entity: link.entity,
                    from: link.from,
                    to: link.to,
                    capacity: link.capacity,
                    flow: link.flow,
                    ..Default::default()
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_populations<'a>(
    builder: &mut FbBuilder<'a>,
    cohorts: &[PopulationCohortState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PopulationCohortState<'a>>>> {
    let offsets: Vec<_> = cohorts
        .iter()
        .map(|cohort| {
            fb::PopulationCohortState::create(
                builder,
                &fb::PopulationCohortStateArgs {
                    entity: cohort.entity,
                    home: cohort.home,
                    size: cohort.size,
                    morale: cohort.morale,
                    ..Default::default()
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power<'a>(
    builder: &mut FbBuilder<'a>,
    power_nodes: &[PowerNodeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerNodeState<'a>>>> {
    let offsets: Vec<_> = power_nodes
        .iter()
        .map(|node| {
            fb::PowerNodeState::create(
                builder,
                &fb::PowerNodeStateArgs {
                    entity: node.entity,
                    generation: node.generation,
                    demand: node.demand,
                    efficiency: node.efficiency,
                    ..Default::default()
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
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
            ..Default::default()
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
                    ..Default::default()
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
            ..Default::default()
        },
    )
}

fn create_generations<'a>(
    builder: &mut FbBuilder<'a>,
    generations: &[GenerationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GenerationState<'a>>>> {
    let offsets: Vec<_> = generations
        .iter()
        .map(|generation| {
            let name = builder.create_string(generation.name.as_str());
            fb::GenerationState::create(
                builder,
                &fb::GenerationStateArgs {
                    id: generation.id,
                    name: Some(name),
                    biasKnowledge: generation.bias_knowledge,
                    biasTrust: generation.bias_trust,
                    biasEquity: generation.bias_equity,
                    biasAgency: generation.bias_agency,
                    ..Default::default()
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_corruption<'a>(
    builder: &mut FbBuilder<'a>,
    ledger: &CorruptionLedger,
) -> WIPOffset<fb::CorruptionLedger<'a>> {
    let entries: Vec<_> = ledger
        .entries
        .iter()
        .map(|entry| {
            fb::CorruptionEntry::create(
                builder,
                &fb::CorruptionEntryArgs {
                    subsystem: to_fb_corruption_subsystem(entry.subsystem),
                    intensity: entry.intensity,
                    incidentId: entry.incident_id,
                    exposureTimer: entry.exposure_timer,
                    restitutionWindow: entry.restitution_window,
                    lastUpdateTick: entry.last_update_tick,
                    ..Default::default()
                },
            )
        })
        .collect();
    let entries_vec = builder.create_vector(&entries);
    fb::CorruptionLedger::create(
        builder,
        &fb::CorruptionLedgerArgs {
            entries: Some(entries_vec),
            reputationModifier: ledger.reputation_modifier,
            auditCapacity: ledger.audit_capacity,
            ..Default::default()
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
                    ..Default::default()
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

fn to_fb_corruption_subsystem(subsystem: CorruptionSubsystem) -> fb::CorruptionSubsystem {
    match subsystem {
        CorruptionSubsystem::Logistics => fb::CorruptionSubsystem::Logistics,
        CorruptionSubsystem::Trade => fb::CorruptionSubsystem::Trade,
        CorruptionSubsystem::Military => fb::CorruptionSubsystem::Military,
        CorruptionSubsystem::Governance => fb::CorruptionSubsystem::Governance,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationState {
    pub id: u16,
    pub name: String,
    pub bias_knowledge: i64,
    pub bias_trust: i64,
    pub bias_equity: i64,
    pub bias_agency: i64,
}
