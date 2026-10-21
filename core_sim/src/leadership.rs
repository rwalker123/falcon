//! **A chief emerges from directed work that succeeds** (`docs/plan_labor_at_scale.md` -> "How a
//! chief emerges"). Rationale: `.claude/rules/core_sim/leadership.md`.
//!
//! Every resident band carries a [`BandLeadership`]: a **practice** count and, once the band has
//! accepted one, a [`BandLeader`]. Directed work that succeeds adds a config quantum to the
//! practice; when it reaches `leadership_config.json -> practice_threshold` and the band has no
//! leader, [`advance_leadership`] gives it one -- once, permanently.
//!
//! **The kind is fixed at emergence** by the sign of the band's own culture on the
//! `HierarchicalEgalitarian` axis: below zero a [`LeaderKind::Chief`], at or above it (ties go
//! egalitarian) an [`LeaderKind::ElderCouncil`]. The axis drifting afterwards does not flip it.
//!
//! **The sources are credited where they succeed**, through the helpers in this module so no seam
//! restates the rule: a finished build of a rung flagged `directed_work`
//! ([`BandLeadership::credit_directed_build`]) and a hunt that lands a big-game kill
//! ([`BandLeadership::credit_big_game_hunt`]).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandId, PopulationCohort, ResidentBand},
    culture::{CultureManager, CultureOwner, CultureTraitAxis},
    fauna::HuntFight,
    fauna_config::FaunaConfig,
    intensification::RungDef,
    leader_names::LeaderNameCatalogHandle,
    leadership_config::{LeadershipConfig, LeadershipConfigHandle},
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, SimulationConfig, SimulationTick,
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

/// **A band's practice at following a plan, and the leader it earned.** Spawned (default) on every
/// resident band; a detached party credits its home band, never itself. State, not derived: the
/// practice is banked across turns and the leader is permanent.
#[derive(Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BandLeadership {
    /// Practice banked so far. Stops accruing once a leader exists (the value is kept).
    pub practice: f64,
    pub leader: Option<BandLeader>,
}

impl BandLeadership {
    /// Bank `quantum` of practice. A band that already has a leader banks nothing: practice stops
    /// mattering the moment it has done its work.
    pub fn credit(&mut self, quantum: f64) {
        if self.leader.is_none() {
            self.practice += quantum;
        }
    }

    /// A finished build of `rung` counts if the rung is flagged `directed_work`.
    pub fn credit_directed_build(&mut self, config: &LeadershipConfig, rung: &RungDef) {
        if rung.directed_work {
            self.credit(config.practice_per_directed_build);
        }
    }

    /// A hunt counts **once per hunt** (not per animal) when it fought, brought at least one animal
    /// down, and the quarry's species is `big_game`. `species` is the display name a `Herd` carries.
    pub fn credit_big_game_hunt(
        &mut self,
        config: &LeadershipConfig,
        fauna: &FaunaConfig,
        species: &str,
        fight: &HuntFight,
    ) {
        let landed_its_kill = fight.fought && fight.brought_down > 0.0;
        let big_game = fauna
            .species_by_display(species)
            .is_some_and(|def| def.big_game);
        if landed_its_kill && big_game {
            self.credit(config.practice_per_big_game_kill);
        }
    }

    /// Whether the band has banked enough practice and has no leader yet.
    fn ready(&self, config: &LeadershipConfig) -> bool {
        self.leader.is_none() && self.practice >= config.practice_threshold
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

/// **Give every ready band its leader.** Runs after labor (which also follows the expeditions) in
/// the same turn, so a success credited this turn can make a leader this turn.
///
/// A band with no culture layer yet waits: the kind is fixed forever at emergence, so it is not
/// read off a default. Bands are visited in id order so the feed order is deterministic.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_leadership(
    tick: Res<SimulationTick>,
    config: Res<LeadershipConfigHandle>,
    names: Res<LeaderNameCatalogHandle>,
    sim_config: Res<SimulationConfig>,
    culture: Res<CultureManager>,
    mut event_log: ResMut<CommandEventLog>,
    mut bands: Query<(&BandId, &PopulationCohort, &mut BandLeadership), With<ResidentBand>>,
) {
    let config = config.get();
    let mut ready: Vec<_> = bands
        .iter_mut()
        .filter(|(_, _, leadership)| leadership.ready(&config))
        .collect();
    ready.sort_by_key(|(band, _, _)| **band);
    for (band, cohort, leadership) in ready.iter_mut() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LeadershipConfig {
        LeadershipConfig::default()
    }

    #[test]
    fn kind_follows_the_sign_and_zero_goes_to_the_council() {
        assert_eq!(LeaderKind::for_axis_value(-0.01), LeaderKind::Chief);
        assert_eq!(LeaderKind::for_axis_value(0.0), LeaderKind::ElderCouncil);
        assert_eq!(LeaderKind::for_axis_value(0.4), LeaderKind::ElderCouncil);
    }

    #[test]
    fn a_leader_emerges_once_at_the_threshold_and_never_again() {
        let config = config();
        let mut leadership = BandLeadership::default();
        assert!(leadership
            .emerge(&config, -1.0, 1, || "Orrin".into())
            .is_none());
        leadership.credit(config.practice_threshold);
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

    fn fight(fought: bool, brought_down: f32) -> HuntFight {
        HuntFight {
            brought_down,
            expected_brought_down: brought_down,
            casualties: Default::default(),
            fought,
            wounds: Default::default(),
            strike_charges: Vec::new(),
        }
    }

    /// A species the shipped roster flags `big_game`, and one it does not.
    const BIG_GAME: &str = "Wild Aurochs";
    const SMALL_GAME: &str = "Red Deer";

    #[test]
    fn a_fought_big_game_kill_credits_one_quantum_per_hunt() {
        let (config, fauna) = (config(), FaunaConfig::builtin());
        let mut leadership = BandLeadership::default();
        // Three animals down in one hunt is still ONE quantum.
        leadership.credit_big_game_hunt(&config, &fauna, BIG_GAME, &fight(true, 3.0));
        assert_eq!(leadership.practice, config.practice_per_big_game_kill);
    }

    #[test]
    fn a_hunt_that_is_not_a_fought_big_game_kill_credits_nothing() {
        let (config, fauna) = (config(), FaunaConfig::builtin());
        let mut leadership = BandLeadership::default();
        // Not big game.
        leadership.credit_big_game_hunt(&config, &fauna, SMALL_GAME, &fight(true, 2.0));
        // A pen slaughter is not a fight.
        leadership.credit_big_game_hunt(&config, &fauna, BIG_GAME, &fight(false, 2.0));
        // A fight that brought nothing down has not landed its kill.
        leadership.credit_big_game_hunt(&config, &fauna, BIG_GAME, &fight(true, 0.0));
        // A species the table cannot resolve is not big game.
        leadership.credit_big_game_hunt(&config, &fauna, "Unknown Beast", &fight(true, 1.0));
        assert_eq!(leadership.practice, 0.0);
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
            leadership.practice
        };
        for verb in ["cultivate", "tame", "corral"] {
            assert_eq!(credited(verb), config.practice_per_directed_build, "{verb}");
        }
        for verb in ["sow", "grade", "pave"] {
            assert_eq!(credited(verb), 0.0, "{verb} is not directed work");
        }
    }

    #[test]
    fn practice_stops_accruing_once_there_is_a_leader() {
        let config = config();
        let mut leadership = BandLeadership::default();
        leadership.credit(config.practice_threshold);
        leadership.emerge(&config, 0.0, 1, || "Orrin".into());
        let banked = leadership.practice;
        leadership.credit(1.0);
        assert_eq!(leadership.practice, banked);
    }
}
