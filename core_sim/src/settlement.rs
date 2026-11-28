//! **A settlement is derived, never founded** (#176): a tribe, or a lone band in none.
//!
//! Each turn [`advance_settlements`] reads every resident band, groups them into settlements (a
//! [`crate::tribe::Tribe`]'s members are one settlement; a band in no tribe is a settlement of its
//! own) and resolves each one's stage through [`crate::settlement_stage_config::resolve_settlement_stage`] over four inputs: the
//! people summed over the members, the highest fixed-store rung any member's owned store holds, the
//! food in the members' larders and owned stores, and the belief on the settlement's anchor tile.
//! The result is parked in [`SettlementStages`] (checkpoint state), which is what the wire, the
//! leaderless band's chief credit and the tribe closeness input read. A settlement that newly
//! reaches an `announce` stage (the shipped city row) is told once on the command feed; the stored
//! stage is the edge gate.
//!
//! Schedule: after `advance_tribes`, so a tribe formed or changed this turn is read as it stands.
//! The readers that run earlier in the turn (`advance_leadership`, `advance_tribes`' settlement
//! pull) see the previous turn's stages.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    belief::BeliefRegistry,
    components::{BandId, PopulationCohort, ResidentBand, FOOD},
    demographics_config::{DemographicsConfigHandle, KeepingConfig},
    intensification::{LadderConfig, LadderConfigHandle},
    ladder_ids::RungId,
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    settlement_stage_config::{
        stage_index, SettlementStageConfig, SettlementStageConfigHandle, SettlementStageInputs,
    },
    systems::band_label,
    tile_improvement::TileImprovementRegistry,
    tribe::{Tribe, TribeId, TribeRegistry},
};

/// **Every settlement's stage this turn**, as an index into `settlement_stage_config.json`'s
/// ordered `stages`. Checkpoint state: the stored value is the edge gate of the stage-reached event
/// and what the wire publishes right after a load.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementStages {
    /// Each resident band's OWN stage, read over that band alone. This is what a band in no tribe
    /// is, what chief credit reads, and what a tribe member's row publishes as its own.
    bands: BTreeMap<BandId, u32>,
    /// Each tribe's stage, read over all its members.
    tribes: BTreeMap<TribeId, u32>,
}

impl SettlementStages {
    /// The stage index of `band` read over itself, if it was resident when last resolved.
    pub fn of_band(&self, band: BandId) -> Option<usize> {
        self.bands.get(&band).map(|index| *index as usize)
    }

    /// The stage index of `tribe`, if it existed when last resolved.
    pub fn of_tribe(&self, tribe: TribeId) -> Option<usize> {
        self.tribes.get(&tribe).map(|index| *index as usize)
    }
}

/// What the system reads of one resident band.
pub(crate) struct BandReading {
    faction: FactionId,
    size: u32,
    /// The larder's food.
    larder: f32,
    anchor: Option<UVec2>,
    /// Food in the band's owned fixed stores.
    stored: f32,
    /// The highest fixed-store rung any of the band's owned stores holds.
    store: Option<RungId>,
}

/// The higher of two store rungs by the ladder's own `order`.
fn higher_store(ladder: &LadderConfig, a: Option<RungId>, b: Option<RungId>) -> Option<RungId> {
    let order = |rung: RungId| ladder.rung_by_id(rung).map_or(0, |def| def.order);
    match (a, b) {
        (Some(a), Some(b)) => Some(if order(b) > order(a) { b } else { a }),
        (a, None) => a,
        (None, b) => b,
    }
}

/// **One settlement's inputs**: the people and food summed over `members`, the highest store rung
/// among them, and the belief on the first member anchor (a tribe's members share one; a lone
/// band reads its own).
fn settlement_inputs(
    members: &[&BandReading],
    belief: &BeliefRegistry,
    ladder: &LadderConfig,
) -> SettlementStageInputs {
    let mut inputs = SettlementStageInputs::default();
    for member in members {
        inputs.size += member.size;
        inputs.food += member.larder + member.stored;
        inputs.store = higher_store(ladder, inputs.store, member.store);
    }
    inputs.belief = members
        .iter()
        .find_map(|member| member.anchor)
        .map_or(0.0, |anchor| belief.get(anchor));
    inputs
}

/// The lowercase stage name a feed line uses ("city").
fn stage_noun(label: &str) -> String {
    label.to_lowercase()
}

fn stage_reached_entry(
    tick: u64,
    faction: FactionId,
    band: BandId,
    subject: String,
    detail: String,
    label: &str,
) -> CommandEventEntry {
    CommandEventEntry::new(
        tick,
        CommandEventKind::SettlementStageReached,
        faction,
        format!("{subject} becomes a {}", stage_noun(label)),
        Some(detail),
    )
    .with_band(band)
}

/// **Read every resident band** as a settlement input: its people, larder, anchor, and the fixed
/// stores it owns. The one definition both [`advance_settlements`] and the leaderless band's chief
/// credit (`leadership::advance_leadership`) resolve a band's own stage from, so the two cannot
/// disagree about what a band is.
pub(crate) fn read_bands<'a>(
    bands: impl Iterator<Item = (&'a BandId, &'a PopulationCohort)>,
    improvements: Option<&TileImprovementRegistry>,
    ladder: &LadderConfig,
    keeping: &KeepingConfig,
) -> BTreeMap<BandId, BandReading> {
    let mut readings: BTreeMap<BandId, BandReading> = bands
        .map(|(band, cohort)| {
            (
                *band,
                BandReading {
                    faction: cohort.faction,
                    size: cohort.size,
                    larder: cohort.stores.get(FOOD).to_f32().max(0.0),
                    anchor: cohort.belief_anchor,
                    stored: 0.0,
                    store: None,
                },
            )
        })
        .collect();
    // The owned fixed stores: an improvement holding a store rung, kept by a band of its people.
    for improvement in improvements.iter().flat_map(|registry| registry.iter()) {
        let Some(owner) = improvement.owner else {
            continue;
        };
        let Some(spec) = keeping.store_spec(ladder, improvement.held_rung()) else {
            continue;
        };
        let Some(reading) = readings.get_mut(&owner) else {
            continue;
        };
        if reading.faction != improvement.faction {
            continue;
        }
        reading.stored += improvement.contents.total().to_f32().max(0.0);
        reading.store = higher_store(ladder, reading.store, Some(spec.rung));
    }

    readings
}

/// **One band's OWN stage index** this turn, read over itself alone: the shared resolve of
/// [`advance_settlements`] and the chief credit.
pub(crate) fn band_stage(
    reading: &BandReading,
    belief: &BeliefRegistry,
    ladder: &LadderConfig,
    config: &SettlementStageConfig,
) -> u32 {
    let inputs = settlement_inputs(&[reading], belief, ladder);
    stage_index(&inputs, &config.stages, ladder).unwrap_or(0) as u32
}

/// **Resolve every settlement's stage**, then tell the player of any that newly reached an
/// `announce` stage. Bands and tribes are visited in id order, so the feed is deterministic.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_settlements(
    tick: Res<SimulationTick>,
    stage_config: Res<SettlementStageConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    demographics: Res<DemographicsConfigHandle>,
    belief: Res<BeliefRegistry>,
    tribes: Res<TribeRegistry>,
    improvements: Option<Res<TileImprovementRegistry>>,
    mut stages: ResMut<SettlementStages>,
    mut event_log: ResMut<CommandEventLog>,
    bands: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
) {
    let config = stage_config.get();
    let ladder = ladder.get();
    let demographics = demographics.get();
    let keeping = &demographics.keeping;

    let readings = read_bands(bands.iter(), improvements.as_deref(), &ladder, keeping);

    let resolve = |members: &[&BandReading]| -> u32 {
        let inputs = settlement_inputs(members, &belief, &ladder);
        stage_index(&inputs, &config.stages, &ladder).unwrap_or(0) as u32
    };

    let mut next = SettlementStages::default();
    for (band, reading) in &readings {
        next.bands
            .insert(*band, band_stage(reading, &belief, &ladder, &config));
    }
    for tribe in tribes.iter() {
        let members: Vec<&BandReading> = tribe
            .members
            .iter()
            .filter_map(|band| readings.get(band))
            .collect();
        if !members.is_empty() {
            next.tribes.insert(tribe.id, resolve(&members));
        }
    }

    // The rising crossings, against the stored stage: a tribe first, then each lone band.
    let rose =
        |previous: u32, now: u32| -> Option<&crate::settlement_stage_config::SettlementStageDef> {
            let def = config.stages.get(now as usize)?;
            (def.announce && now > previous).then_some(def)
        };
    for tribe in tribes.iter() {
        let Some(&now) = next.tribes.get(&tribe.id) else {
            continue;
        };
        // A tribe with no stored stage (formed this turn) inherits the highest stage its members
        // already stood at, so a stage a band had announced is not announced again for the tribe.
        let previous = stages.tribes.get(&tribe.id).copied().unwrap_or_else(|| {
            tribe
                .members
                .iter()
                .filter_map(|band| stages.bands.get(band).copied())
                .max()
                .unwrap_or(0)
        });
        if let Some(def) = rose(previous, now) {
            if let Some(entry) = tribe_entry(tick.0, tribe, def, &readings) {
                event_log.push(entry);
            }
        }
    }
    for (band, reading) in &readings {
        if tribes.tribe_of(*band).is_some() {
            continue;
        }
        let now = next.bands[band];
        let previous = stages.bands.get(band).copied().unwrap_or(0);
        if let Some(def) = rose(previous, now) {
            event_log.push(stage_reached_entry(
                tick.0,
                reading.faction,
                *band,
                band_label(*band),
                format!("stage={} band={}", def.id, band.0),
                &def.label,
            ));
        }
    }
    *stages = next;
}

/// The feed line for a tribe reaching `def`, filed against its leader's band while that band is
/// resident and else its first resident member.
fn tribe_entry(
    tick: u64,
    tribe: &Tribe,
    def: &crate::settlement_stage_config::SettlementStageDef,
    readings: &BTreeMap<BandId, BandReading>,
) -> Option<CommandEventEntry> {
    let band = if readings.contains_key(&tribe.leader.band) {
        tribe.leader.band
    } else {
        *tribe.members.iter().find(|b| readings.contains_key(b))?
    };
    Some(stage_reached_entry(
        tick,
        tribe.faction,
        band,
        format!("The {}", tribe.name),
        format!("stage={} tribe={}", def.id, tribe.id.0),
        &def.label,
    ))
}

/// The stage config's stage at `index`, for a reader that publishes it.
pub fn stage_def(
    config: &SettlementStageConfig,
    index: Option<usize>,
) -> Option<&crate::settlement_stage_config::SettlementStageDef> {
    config.stages.get(index?)
}
