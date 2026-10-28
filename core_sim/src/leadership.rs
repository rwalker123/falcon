//! **A chief emerges from work that needs an organiser** (`docs/plan_labor_at_scale.md` -> "How a
//! chief emerges: work that needs an organiser"). Rationale: `.claude/rules/core_sim/leadership.md`.
//!
//! Every resident band carries a [`BandLeadership`]: practice banked **per source** and, once the
//! band has accepted one, a [`BandLeader`]. A band banks points by doing things that logically need
//! someone organising them -- finishing a Cultivate or a Tame, splitting in two on command, walking
//! after a herd, growing into a larger settlement stage. When the sum reaches
//! `leadership_config.json -> practice_threshold` and the band has no leader,
//! [`advance_leadership`] gives it one -- once, permanently, and the bar then freezes.
//!
//! **The kind is fixed at emergence** by the sign of the band's own culture on the
//! `HierarchicalEgalitarian` axis: below zero a [`LeaderKind::Chief`], at or above it (ties go
//! egalitarian) an [`LeaderKind::ElderCouncil`]. The axis drifting afterwards does not flip it.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandId, PopulationCohort, ResidentBand},
    culture::{CultureManager, CultureOwner, CultureTraitAxis},
    intensification::RungDef,
    leader_names::LeaderNameCatalogHandle,
    leadership_config::{LeadershipConfig, LeadershipConfigHandle},
    mobility::HerdFollowSteps,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, SimulationConfig, SimulationTick,
    },
    settlement_stage_config::{
        resolve_settlement_stage, SettlementStageConfigHandle, SettlementStageInputs,
    },
};

/// The axis value below which a band's leader is a chief. A named constant because the cut is the
/// *meaning* of the axis (negative = hierarchical), not a tuning lever; a value equal to it goes to
/// the council.
const CHIEF_BELOW_AXIS_VALUE: f32 = 0.0;

/// Wire code for "no leader" on `PopulationCohortState.leaderKind`.
pub const LEADER_KIND_NONE: u8 = 0;
/// Wire code for [`LeaderKind::Chief`].
pub const LEADER_KIND_CHIEF: u8 = 1;
/// Wire code for [`LeaderKind::ElderCouncil`].
pub const LEADER_KIND_ELDER_COUNCIL: u8 = 2;

/// What kind of leadership a band accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaderKind {
    /// One person the band follows. Emerges where the band's culture leans hierarchical.
    Chief,
    /// A council of elders with one named speaker. Emerges where the culture leans egalitarian.
    ElderCouncil,
}

impl LeaderKind {
    /// The kind a band with `axis_value` on `HierarchicalEgalitarian` accepts.
    pub fn for_axis_value(axis_value: f32) -> Self {
        if axis_value < CHIEF_BELOW_AXIS_VALUE {
            LeaderKind::Chief
        } else {
            LeaderKind::ElderCouncil
        }
    }

    /// The `kind=` token a feed line carries.
    pub fn as_str(self) -> &'static str {
        match self {
            LeaderKind::Chief => "chief",
            LeaderKind::ElderCouncil => "elder_council",
        }
    }

    /// The wire code (`PopulationCohortState.leaderKind`).
    pub fn wire_code(self) -> u8 {
        match self {
            LeaderKind::Chief => LEADER_KIND_CHIEF,
            LeaderKind::ElderCouncil => LEADER_KIND_ELDER_COUNCIL,
        }
    }
}

/// The person a band follows. A council also has a named person -- its speaker -- in `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandLeader {
    pub name: String,
    pub kind: LeaderKind,
    /// The turn the leader emerged.
    pub since_turn: u64,
}

/// **A band's need for a leader, banked per source, and the leader it earned.** Spawned (default)
/// on every resident band. State, not derived: the practice is banked across turns and the leader is
/// permanent. Nothing accrues once a leader exists (the bar freezes where it stood).
#[derive(Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BandLeadership {
    /// From finished Cultivate, Sow, Tame and Corral builds (`RungDef::directed_work`).
    pub from_builds: f64,
    /// From commanded band splits, credited to both bands.
    pub from_splits: f64,
    /// From hexes walked following a herd.
    pub from_herd: f64,
    /// From settlement stages reached, each paid once.
    pub from_stages: f64,
    /// The highest settlement stage index already paid for (0 = the first stage, never paid). A band
    /// that shrinks and regrows is not paid again.
    pub stages_paid: u32,
    pub leader: Option<BandLeader>,
}

impl BandLeadership {
    /// Total banked, the figure compared with `practice_threshold`.
    pub fn practice(&self) -> f64 {
        self.from_builds + self.from_splits + self.from_herd + self.from_stages
    }

    fn open(&self) -> bool {
        self.leader.is_none()
    }

    /// A finished build of `rung` counts if the rung is flagged `directed_work`.
    pub fn credit_directed_build(&mut self, config: &LeadershipConfig, rung: &RungDef) {
        if self.open() && rung.directed_work {
            self.from_builds += config.practice_per_directed_build;
        }
    }

    /// A commanded split: called once for the parent and once for the new band.
    pub fn credit_split(&mut self, config: &LeadershipConfig) {
        if self.open() {
            self.from_splits += config.practice_per_split;
        }
    }

    /// `hexes` stepped this turn while following a herd.
    pub fn credit_herd_hexes(&mut self, config: &LeadershipConfig, hexes: u32) {
        if self.open() {
            self.from_herd += f64::from(hexes) * config.practice_per_herd_hex;
        }
    }

    /// The band now stands in settlement stage `stage_index`; each stage above the highest already
    /// paid is paid, so reaching two at once pays both and regrowing pays nothing.
    pub fn credit_stages(&mut self, config: &LeadershipConfig, stage_index: u32) {
        if self.open() && stage_index > self.stages_paid {
            self.from_stages +=
                f64::from(stage_index - self.stages_paid) * config.practice_per_stage_reached;
            self.stages_paid = stage_index;
        }
    }

    /// Whether the band has banked enough practice and has no leader yet.
    fn ready(&self, config: &LeadershipConfig) -> bool {
        self.open() && self.practice() >= config.practice_threshold
    }

    /// Give the band its leader if it is ready. `axis_value` is the band's own
    /// `HierarchicalEgalitarian` reading; `name` is only evaluated on emergence. Returns the new
    /// leader (a copy) when one emerged.
    pub fn emerge(
        &mut self,
        config: &LeadershipConfig,
        axis_value: f32,
        turn: u64,
        name: impl FnOnce() -> String,
    ) -> Option<BandLeader> {
        if !self.ready(config) {
            return None;
        }
        let leader = BandLeader {
            name: name(),
            kind: LeaderKind::for_axis_value(axis_value),
            since_turn: turn,
        };
        self.leader = Some(leader.clone());
        Some(leader)
    }
}

/// The feed line for a leader's emergence: a short label and the space-delimited detail.
pub fn emergence_entry(
    tick: u64,
    faction: crate::orders::FactionId,
    band: BandId,
    leader: &BandLeader,
) -> CommandEventEntry {
    let label = match leader.kind {
        LeaderKind::Chief => format!("{} is chief", leader.name),
        LeaderKind::ElderCouncil => {
            format!("The elders' council forms; {} speaks for it", leader.name)
        }
    };
    CommandEventEntry::new(
        tick,
        CommandEventKind::LeaderEmerged,
        faction,
        label,
        Some(format!(
            "kind={} name={}",
            leader.kind.as_str(),
            leader.name
        )),
    )
    .with_band(band)
}

/// **How much a leader runs well, and how much it actually runs** (`docs/plan_labor_at_scale.md` ->
/// "A leader can only run so much well"). `reach` and `load` are in the leader's own unit: people
/// for a band leader, member bands for a tribe leader. Past the reach an order is carried out
/// worse, never refused: [`Self::orders_carried_out`] is the factor tax, aid and tribe staffing
/// apply. A pure function of current state and config, so nothing here is saved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeaderReach {
    /// What the leader runs well (`reach_chief_people` or `reach_paramount_bands`).
    pub reach: f64,
    /// What the leader actually runs: the band's people, or the tribe's member bands.
    pub load: f64,
}

impl LeaderReach {
    /// The share of an order carried out, in `(0, 1]`: all of it within the reach, and past it the
    /// reach in full plus `over_reach_efficiency` of the excess, over the whole load.
    pub fn orders_carried_out(&self, config: &LeadershipConfig) -> f64 {
        if self.load <= self.reach {
            return 1.0;
        }
        (self.reach + config.over_reach_efficiency * (self.load - self.reach)) / self.load
    }
}

/// A band leader's reach (chief or elders' council alike): `reach_chief_people` against the band's
/// total people.
pub fn band_leader_reach(config: &LeadershipConfig, people: f64) -> LeaderReach {
    LeaderReach {
        reach: config.reach_chief_people,
        load: people,
    }
}

/// A tribe leader's reach (paramount chief or council of chiefs alike): `reach_paramount_bands`
/// against the number of member bands.
pub fn tribe_leader_reach(config: &LeadershipConfig, member_bands: usize) -> LeaderReach {
    LeaderReach {
        reach: config.reach_paramount_bands,
        load: member_bands as f64,
    }
}

/// The factor a tribe member band's orders arrive at: the tribe leader's own
/// [`LeaderReach::orders_carried_out`], times `over_reach_efficiency` again when the band has no
/// leader of its own (the same lossy shape as over-reach).
pub fn tribe_orders_carried_out_for_member(
    config: &LeadershipConfig,
    tribe: &LeaderReach,
    member_has_own_leader: bool,
) -> f64 {
    let carried = tribe.orders_carried_out(config);
    if member_has_own_leader {
        carried
    } else {
        carried * config.over_reach_efficiency
    }
}

/// What the leadership pass reads of a resident band: its identity, cohort, leadership, name and
/// allocation (the last two only to tell a chief's takeover).
type LeadershipBandItems = (
    &'static BandId,
    &'static PopulationCohort,
    &'static mut BandLeadership,
    Option<&'static crate::components::BandName>,
    Option<&'static mut crate::components::LaborAllocation>,
);

/// **Bank this turn's passive practice, then give every ready band its leader.** Runs right after
/// `advance_labor_allocation` in the same turn, so a build finished this turn counts this turn.
///
/// Two sources are read here off the band's state: the hexes it walked following a herd
/// (`HerdFollowSteps`, which `advance_band_movement` filled earlier this turn and `mobility_tick`
/// drains later -- this system is ordered between the two, so it reads the same measure the mobility
/// score does without recomputing it) and the settlement stage its current size stands in.
///
/// A band with no culture layer yet banks but waits to emerge: the kind is fixed forever, so it is
/// not read off a default. Bands are visited in id order so the feed order is deterministic.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_leadership(
    tick: Res<SimulationTick>,
    config: Res<LeadershipConfigHandle>,
    names: Res<LeaderNameCatalogHandle>,
    sim_config: Res<SimulationConfig>,
    stage_config: Res<SettlementStageConfigHandle>,
    culture: Res<CultureManager>,
    follow_steps: Option<Res<HerdFollowSteps>>,
    mut event_log: ResMut<CommandEventLog>,
    mut bands: Query<LeadershipBandItems, With<ResidentBand>>,
) {
    let config = config.get();
    let stages = stage_config.get();
    let mut open: Vec<_> = bands
        .iter_mut()
        .filter(|(_, _, leadership, _, _)| leadership.leader.is_none())
        .collect();
    open.sort_by_key(|(band, _, _, _, _)| **band);
    for (band, cohort, leadership, _, _) in open.iter_mut() {
        if let Some(steps) = follow_steps.as_deref() {
            leadership.credit_herd_hexes(&config, steps.hexes_of(**band));
        }
        let inputs = SettlementStageInputs { size: cohort.size };
        if let Some(stage) = resolve_settlement_stage(&inputs, &stages.stages) {
            if let Some(index) = stages.stages.iter().position(|def| def.id == stage.id) {
                leadership.credit_stages(&config, index as u32);
            }
        }
        if !leadership.ready(&config) {
            continue;
        }
        let Some(layer) = culture.band_layer_by_owner(CultureOwner::from_band(**band)) else {
            continue;
        };
        let axis_value =
            layer.traits.values()[CultureTraitAxis::HierarchicalEgalitarian.index()].to_f32();
        let emerged = leadership.emerge(&config, axis_value, tick.0, || {
            names.catalog().name_for_band(sim_config.map_seed, **band)
        });
        if let Some(leader) = emerged {
            event_log.push(emergence_entry(tick.0, cohort.faction, **band, &leader));
        }
    }
    drop(open);
    announce_chief_takeover(tick.0, &mut event_log, &mut bands);
}

/// **ONE FEED LINE PER BAND WHEN A CHIEF TAKES OVER** (#860): the managing pass
/// (`keep_on_hand::manage_dig_rows`) queues the kinds an arrival created orders for on
/// [`crate::components::LaborAllocation::unannounced_takeover`]; this drains the queue into one
/// `Extraction`-kind line per band (`"Orrin is chief of Bitterbrook - he takes over the stone
/// digging and keeps 30 on hand."`), filed under the band like the emergence line. It is not a
/// `LeaderEmerged` row: the turn orb reads that kind's `kind=`/`name=` detail as a new leader. Called
/// at the foot of [`advance_leadership`], which follows the labor pass that fills the queue, so it
/// shares that system's ordering.
fn announce_chief_takeover(
    tick: u64,
    event_log: &mut CommandEventLog,
    bands: &mut Query<LeadershipBandItems, With<ResidentBand>>,
) {
    let mut announcing: Vec<_> = bands
        .iter_mut()
        .filter(|(_, _, _, _, allocation)| {
            allocation
                .as_ref()
                .is_some_and(|allocation| !allocation.unannounced_takeover.is_empty())
        })
        .collect();
    announcing.sort_by_key(|(band, _, _, _, _)| **band);
    for (band, cohort, leadership, name, allocation) in announcing.iter_mut() {
        let Some(allocation) = allocation.as_mut() else {
            continue;
        };
        let kinds = std::mem::take(&mut allocation.unannounced_takeover);
        let band_name = name
            .as_ref()
            .map_or_else(|| format!("Band {}", band.0), |name| name.0.clone());
        let Some(keeper) = crate::keep_on_hand::Keeper::of(Some(&**leadership), &band_name) else {
            continue;
        };
        let taken: Vec<(String, u32)> = kinds
            .into_iter()
            .filter_map(|kind| {
                let target = allocation.order_for(&kind)?.target;
                Some((kind, target))
            })
            .collect();
        if taken.is_empty() {
            continue;
        }
        let label = crate::keep_on_hand::takeover_line(&keeper, &band_name, &taken);
        event_log.push(
            CommandEventEntry::new(
                tick,
                CommandEventKind::Extraction,
                cohort.faction,
                label,
                Some(format!(
                    "status=applied action=band_order order=takeover kinds={}",
                    taken
                        .iter()
                        .map(|(kind, _)| kind.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                )),
            )
            .with_band(**band),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LeadershipConfig {
        LeadershipConfig::default()
    }

    #[test]
    fn orders_are_whole_within_and_at_the_reach_and_for_no_load() {
        let config = config();
        for load in [0.0, 3.0, 4.0] {
            let reach = LeaderReach { reach: 4.0, load };
            assert_eq!(reach.orders_carried_out(&config), 1.0);
        }
    }

    #[test]
    fn over_the_reach_the_excess_is_carried_out_at_the_efficiency() {
        let config = LeadershipConfig {
            over_reach_efficiency: 0.5,
            ..config()
        };
        let reach = LeaderReach {
            reach: 4.0,
            load: 5.0,
        };
        assert!((reach.orders_carried_out(&config) - 0.9).abs() < 1e-12);
    }

    #[test]
    fn a_chiefless_member_multiplies_by_the_efficiency() {
        let config = config();
        let tribe = tribe_leader_reach(&config, 2);
        assert_eq!(
            tribe_orders_carried_out_for_member(&config, &tribe, true),
            1.0
        );
        assert_eq!(
            tribe_orders_carried_out_for_member(&config, &tribe, false),
            config.over_reach_efficiency
        );
    }

    #[test]
    fn the_constructors_read_the_levers_for_their_level() {
        let config = config();
        let band = band_leader_reach(&config, 200.0);
        assert_eq!(band.reach, config.reach_chief_people);
        assert_eq!(band.load, 200.0);
        let tribe = tribe_leader_reach(&config, 6);
        assert_eq!(tribe.reach, config.reach_paramount_bands);
        assert_eq!(tribe.load, 6.0);
    }

    #[test]
    fn kind_follows_the_sign_and_zero_goes_to_the_council() {
        assert_eq!(LeaderKind::for_axis_value(-0.01), LeaderKind::Chief);
        assert_eq!(LeaderKind::for_axis_value(0.0), LeaderKind::ElderCouncil);
        assert_eq!(LeaderKind::for_axis_value(0.4), LeaderKind::ElderCouncil);
    }

    /// A leadership record with `practice` banked from builds.
    fn banked(practice: f64) -> BandLeadership {
        BandLeadership {
            from_builds: practice,
            ..BandLeadership::default()
        }
    }

    #[test]
    fn a_leader_emerges_once_at_the_threshold_and_never_again() {
        let config = config();
        let mut leadership = BandLeadership::default();
        assert!(leadership
            .emerge(&config, -1.0, 1, || "Orrin".into())
            .is_none());
        leadership = banked(config.practice_threshold);
        let leader = leadership
            .emerge(&config, -1.0, 5, || "Orrin".into())
            .expect("threshold reached");
        assert_eq!(leader.kind, LeaderKind::Chief);
        assert_eq!(leader.since_turn, 5);
        // The axis flipping later does not re-make or flip it.
        assert!(leadership
            .emerge(&config, 1.0, 9, || "Hale".into())
            .is_none());
        assert_eq!(leadership.leader, Some(leader));
    }

    #[test]
    fn the_sources_sum_to_the_practice() {
        let config = config();
        let mut leadership = BandLeadership::default();
        leadership.credit_split(&config);
        leadership.credit_herd_hexes(&config, 4);
        leadership.credit_stages(&config, 1);
        assert_eq!(leadership.from_splits, config.practice_per_split);
        assert_eq!(leadership.from_herd, 4.0 * config.practice_per_herd_hex);
        assert_eq!(leadership.from_stages, config.practice_per_stage_reached);
        assert_eq!(
            leadership.practice(),
            leadership.from_builds
                + leadership.from_splits
                + leadership.from_herd
                + leadership.from_stages
        );
    }

    #[test]
    fn each_stage_pays_once_and_two_at_once_pay_both() {
        let config = config();
        let mut leadership = BandLeadership::default();
        leadership.credit_stages(&config, 0);
        assert_eq!(leadership.from_stages, 0.0, "the first stage is never paid");
        leadership.credit_stages(&config, 1);
        assert_eq!(leadership.from_stages, config.practice_per_stage_reached);
        // Shrinking and regrowing into the same stage pays nothing more.
        leadership.credit_stages(&config, 0);
        leadership.credit_stages(&config, 1);
        assert_eq!(leadership.from_stages, config.practice_per_stage_reached);
        // Jumping two stages at once pays each.
        let mut jumped = BandLeadership::default();
        jumped.credit_stages(&config, 2);
        assert_eq!(jumped.from_stages, 2.0 * config.practice_per_stage_reached);
    }

    #[test]
    fn only_directed_work_rungs_credit_a_finished_build() {
        let config = config();
        let ladder = crate::intensification::LadderConfig::builtin();
        let credited = |verb: &str| {
            let rung = ladder
                .rungs
                .iter()
                .find(|rung| rung.verb.as_deref() == Some(verb))
                .unwrap_or_else(|| panic!("the shipped ladder has a {verb} rung"));
            let mut leadership = BandLeadership::default();
            leadership.credit_directed_build(&config, rung);
            leadership.practice()
        };
        for verb in ["cultivate", "sow", "tame", "corral"] {
            assert_eq!(credited(verb), config.practice_per_directed_build, "{verb}");
        }
        for verb in ["grade", "pave", "fell", "coppice", "quarry"] {
            assert_eq!(credited(verb), 0.0, "{verb} is not directed work");
        }
    }

    #[test]
    fn nothing_accrues_once_there_is_a_leader() {
        let config = config();
        let mut leadership = banked(config.practice_threshold);
        leadership.emerge(&config, 0.0, 1, || "Orrin".into());
        let frozen = leadership.clone();
        leadership.credit_split(&config);
        leadership.credit_herd_hexes(&config, 9);
        leadership.credit_stages(&config, 2);
        assert_eq!(leadership, frozen);
    }
}
