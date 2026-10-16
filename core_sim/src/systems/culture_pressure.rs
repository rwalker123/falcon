//! **Culture strength and culture pressure — a people's weight of tradition on one band of
//! another** (`docs/plan_minor_peoples.md` §"Peaceful absorption", #820). Rationale:
//! `.claude/rules/core_sim/factions.md` → "Culture strength and pressure".
//!
//! This slice is **read-only**: it measures and publishes, and no band changes hands.
//!
//! Each turn, for every people with a resident band:
//!
//! * **strength** = `base + Σ weight_i × (input_i / reference_i)` over the belief held at the
//!   anchors of its bands (each anchor once), the distinct foreign bands live-tied to its bands,
//!   and the wondrous sites it has discovered;
//! * for every `(band of A, people B)` where the band holds a live tie to a resident band of B,
//!   **pressure** = `(strength_B / strength_A) × (people_B / people_A) ^ size_exponent`, both
//!   sizes whole-people head-counts (bands and expedition parties alike).
//!
//! A pair at or above `joining.pulled` is stored with two clocks (`asked_turns`, `forced_turns`);
//! the phase follows from them. A pair below it is not stored, so dropping out resets its clock.

use std::collections::{BTreeMap, BTreeSet};

use bevy::{math::UVec2, prelude::*};
use serde::{Deserialize, Serialize};

use crate::{
    belief::BeliefRegistry,
    components::{BandId, PopulationCohort, ResidentBand},
    connections::{ConnectionLedger, NO_TIE},
    culture::{max_axis_gap, CultureManager, CultureOwner, CULTURE_TRAIT_AXES},
    culture_corruption_config::{
        CultureCorruptionConfigHandle, CultureJoiningSettings, CultureStrengthSettings,
    },
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    scalar::{scalar_from_f32, Scalar},
    sites::DiscoveredSites,
    systems::population::{band_label, faction_label},
};

/// Where one band stands under another people's pull. Declaration order is the escalation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JoiningPhase {
    /// Below `joining.pulled`: no entry is stored for the pair.
    None,
    /// `pressure >= pulled`: the band's owner is told, nothing else happens.
    Pulled,
    /// `pressure >= asked` and close, held `joining.turns` turns.
    Asked,
    /// `pressure >= forced` and close, held `joining.turns` turns.
    Forced,
}

impl JoiningPhase {
    /// The wire code of `CulturePressureState.phase`.
    pub fn wire_code(self) -> u8 {
        match self {
            JoiningPhase::None => 0,
            JoiningPhase::Pulled => 1,
            JoiningPhase::Asked => 2,
            JoiningPhase::Forced => 3,
        }
    }
}

/// One `(band, people)` pair at phase `Pulled` or above.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CulturePressureReading {
    /// The band's owner when the pair was read. An entry whose owner is no longer the band's is
    /// stale: the clocks restart.
    pub owner: FactionId,
    pub pressure: f32,
    pub phase: JoiningPhase,
    /// The band's culture is within `joining.max_culture_distance` of the people's average.
    pub close: bool,
    /// Consecutive turns `pressure >= asked` and close.
    pub asked_turns: u16,
    /// Consecutive turns `pressure >= forced` and close.
    pub forced_turns: u16,
}

impl CulturePressureReading {
    /// Turns left on the clock toward the next phase; `0` when that clock is not running.
    pub fn turns_to_next(&self, joining: &CultureJoiningSettings) -> u16 {
        let running = match self.phase {
            JoiningPhase::Pulled => self.asked_turns,
            JoiningPhase::Asked => self.forced_turns,
            JoiningPhase::None | JoiningPhase::Forced => 0,
        };
        if running == 0 {
            0
        } else {
            joining.turns().saturating_sub(running)
        }
    }
}

/// **Every people's culture strength, and every band's standing under each other people's
/// pull.** Checkpoint state, not derived: the `culture_pull` line is an edge read off the previous
/// turn's entry, the clocks run across turns, and the capture publishes the rows before a
/// restored world has run a turn.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CulturePressureLedger {
    pub strengths: BTreeMap<FactionId, f32>,
    pub pairs: BTreeMap<(BandId, FactionId), CulturePressureReading>,
}

impl CulturePressureLedger {
    pub fn strength(&self, faction: FactionId) -> Option<f32> {
        self.strengths.get(&faction).copied()
    }

    pub fn reading(&self, band: BandId, people: FactionId) -> Option<&CulturePressureReading> {
        self.pairs.get(&(band, people))
    }

    /// One band's rows as `(people, reading)`, in people order.
    pub fn for_band(
        &self,
        band: BandId,
    ) -> impl Iterator<Item = (FactionId, &CulturePressureReading)> {
        self.pairs
            .range((band, FactionId(u32::MIN))..=(band, FactionId(u32::MAX)))
            .map(|((_, people), reading)| (*people, reading))
    }
}

/// `base + Σ weight_i × (input_i / reference_i)`. Inputs are not clamped.
pub fn culture_strength(
    settings: &CultureStrengthSettings,
    belief: f32,
    ties: usize,
    discoveries: usize,
) -> f32 {
    let (weights, references) = (settings.weights(), settings.references());
    settings.base()
        + weights.belief() * (belief / references.belief())
        + weights.ties() * (ties as f32 / references.ties())
        + weights.discoveries() * (discoveries as f32 / references.discoveries())
}

/// `(strength_B / strength_A) × (people_B / people_A) ^ size_exponent`.
pub fn joining_pressure(
    joining: &CultureJoiningSettings,
    strength_a: f32,
    strength_b: f32,
    people_a: u32,
    people_b: u32,
) -> f32 {
    (strength_b / strength_a) * (people_b as f32 / people_a as f32).powf(joining.size_exponent())
}

/// Advance one pair a turn. `previous` is last turn's entry (only if it was read for the same
/// owner); the result is `None` below `joining.pulled`.
pub fn advance_pair(
    joining: &CultureJoiningSettings,
    owner: FactionId,
    previous: Option<&CulturePressureReading>,
    pressure: f32,
    close: bool,
) -> Option<CulturePressureReading> {
    let held = previous.filter(|reading| reading.owner == owner);
    let tick_clock = |held_turns: u16, met: bool| {
        if met {
            held_turns.saturating_add(1)
        } else {
            0
        }
    };
    let asked_turns = tick_clock(
        held.map_or(0, |reading| reading.asked_turns),
        close && pressure >= joining.asked(),
    );
    let forced_turns = tick_clock(
        held.map_or(0, |reading| reading.forced_turns),
        close && pressure >= joining.forced(),
    );
    let phase = if forced_turns >= joining.turns() {
        JoiningPhase::Forced
    } else if asked_turns >= joining.turns() {
        JoiningPhase::Asked
    } else if pressure >= joining.pulled() {
        JoiningPhase::Pulled
    } else {
        JoiningPhase::None
    };
    (phase >= JoiningPhase::Pulled).then_some(CulturePressureReading {
        owner,
        pressure,
        phase,
        close,
        asked_turns,
        forced_turns,
    })
}

/// One resident band, as the pass reads it.
struct ResidentView {
    band: BandId,
    faction: FactionId,
    size: u32,
    anchor: Option<UVec2>,
}

/// **Measure every people's culture strength and every band's pressure; tell the owner the turn a
/// pull begins.** Runs in the Population chain right after `advance_culture_splits`, so every
/// band's people is final for the turn. Reads the contact ties as the previous Visibility stage
/// left them.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_culture_pressure(
    config: Res<CultureCorruptionConfigHandle>,
    tick: Res<SimulationTick>,
    connections: Res<ConnectionLedger>,
    belief: Res<BeliefRegistry>,
    sites: Res<DiscoveredSites>,
    culture: Res<CultureManager>,
    mut ledger: ResMut<CulturePressureLedger>,
    mut event_log: ResMut<CommandEventLog>,
    cohorts: Query<(&PopulationCohort, Option<&BandId>, Option<&ResidentBand>)>,
) {
    let config = config.get();
    let (strength_settings, joining) = (config.culture().strength(), config.culture().joining());

    // ---- who is there: resident bands, and the whole-people head-count of each people ----
    let mut residents: Vec<ResidentView> = Vec::new();
    let mut people_size: BTreeMap<FactionId, u32> = BTreeMap::new();
    for (cohort, band, resident) in cohorts.iter() {
        *people_size.entry(cohort.faction).or_default() += cohort.size;
        if let (Some(band), Some(_)) = (band, resident) {
            residents.push(ResidentView {
                band: *band,
                faction: cohort.faction,
                size: cohort.size,
                anchor: cohort.belief_anchor,
            });
        }
    }
    residents.sort_by_key(|view| view.band);
    let faction_of: BTreeMap<BandId, FactionId> =
        residents.iter().map(|v| (v.band, v.faction)).collect();

    // ---- live ties across peoples: per people, the foreign bands; per band, the peoples ----
    let mut foreign_bands: BTreeMap<FactionId, BTreeSet<BandId>> = BTreeMap::new();
    let mut touching: BTreeSet<(BandId, FactionId)> = BTreeSet::new();
    for (key, connection) in connections.iter() {
        if connection.strength <= NO_TIE {
            continue;
        }
        let (Some(&from), Some(&to)) =
            (faction_of.get(&key.observer), faction_of.get(&key.subject))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        foreign_bands.entry(from).or_default().insert(key.subject);
        foreign_bands.entry(to).or_default().insert(key.observer);
        touching.insert((key.observer, to));
        touching.insert((key.subject, from));
    }

    // ---- strength per people ----
    let mut anchors: BTreeMap<FactionId, BTreeSet<UVec2Key>> = BTreeMap::new();
    for view in &residents {
        let set = anchors.entry(view.faction).or_default();
        if let Some(anchor) = view.anchor {
            set.insert(UVec2Key::from(anchor));
        }
    }
    let strengths: BTreeMap<FactionId, f32> = anchors
        .iter()
        .map(|(&faction, set)| {
            let held: f32 = set.iter().map(|key| belief.get(key.position())).sum();
            let ties = foreign_bands.get(&faction).map_or(0, BTreeSet::len);
            let discoveries = sites.for_faction(faction).len();
            (
                faction,
                culture_strength(strength_settings, held, ties, discoveries),
            )
        })
        .collect();

    // ---- pressure per (band, people) ----
    let mut averages: BTreeMap<FactionId, [Scalar; CULTURE_TRAIT_AXES]> = BTreeMap::new();
    let mut pairs: BTreeMap<(BandId, FactionId), CulturePressureReading> = BTreeMap::new();
    for view in &residents {
        for &(band, people) in touching.range((view.band, FactionId(u32::MIN))..) {
            if band != view.band {
                break;
            }
            if people == view.faction {
                continue;
            }
            let (Some(&strength_a), Some(&strength_b)) =
                (strengths.get(&view.faction), strengths.get(&people))
            else {
                continue;
            };
            let people_a = people_size.get(&view.faction).copied().unwrap_or_default();
            let people_b = people_size.get(&people).copied().unwrap_or_default();
            if people_a == 0 {
                continue;
            }
            let pressure = joining_pressure(joining, strength_a, strength_b, people_a, people_b);
            let average = *averages.entry(people).or_insert_with(|| {
                let weights: Vec<(CultureOwner, u32)> = residents
                    .iter()
                    .filter(|other| other.faction == people)
                    .map(|other| (CultureOwner::from_band(other.band), other.size))
                    .collect();
                culture.faction_trait_average(&weights).map(scalar_from_f32)
            });
            let close = culture
                .band_layer_by_owner(CultureOwner::from_band(view.band))
                .is_some_and(|layer| {
                    max_axis_gap(layer.traits.values(), &average).to_f32()
                        <= joining.max_culture_distance()
                });
            let previous = ledger.pairs.get(&(view.band, people));
            let Some(reading) = advance_pair(joining, view.faction, previous, pressure, close)
            else {
                continue;
            };
            if previous.is_none_or(|prior| prior.owner != view.faction) {
                event_log.push(CommandEventEntry::new(
                    tick.0,
                    CommandEventKind::CulturePull,
                    view.faction,
                    format!(
                        "{}'s culture is pulling {}.",
                        faction_label(people),
                        band_label(view.band)
                    ),
                    Some(format!("band={} faction={}", view.band.0, people.0)),
                ));
            }
            pairs.insert((view.band, people), reading);
        }
    }

    ledger.strengths = strengths;
    ledger.pairs = pairs;
}

/// A tile position ordered row-major, so the anchor set iterates in a fixed order (`UVec2` has no
/// `Ord`). The sum over anchors is order-independent in exact arithmetic but not in `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UVec2Key {
    y: u32,
    x: u32,
}

impl From<UVec2> for UVec2Key {
    fn from(position: UVec2) -> Self {
        Self {
            y: position.y,
            x: position.x,
        }
    }
}

impl UVec2Key {
    fn position(self) -> UVec2 {
        UVec2::new(self.x, self.y)
    }
}
