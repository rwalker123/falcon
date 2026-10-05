//! **Founding lines** — the unrelated families a band descends from (issue #687,
//! `docs/plan_civilization_steps.md` §"The mechanism: an isolated people cannot grow past its
//! lines").
//!
//! The sim has three age brackets and no individuals, sexes or kinship, so relatedness cannot be
//! read from state. A *line* is the proxy: one unrelated family a group descends from. A starting
//! band is founded with `lineage.founding_lines` of them (`demographics_config.json`); a split
//! takes a proportional share of its parent's and **takes them with it** — the parent no longer
//! holds what walked off.
//!
//! **Lines are a SET of identities, not a count.** The count is what gets published, but the
//! slices that build on this one need the identities: contact merges line sets ("each side gains
//! the lines it lacks"), and a recent split shares every line it could gain and so adds nothing —
//! both are set operations, and neither can be answered from two numbers.
//!
//! **A [`LineId`] needs no allocator.** Lines are only ever *minted* on a starting band, whose
//! [`BandId`] is already unique, so `(origin_band, index)` is globally unique by construction; a
//! split partitions existing ids and never mints. The set is a [`BTreeSet`] so every walk over it is
//! in id order — the sim is seeded and must stay deterministic.

use std::collections::BTreeSet;
use std::num::{NonZeroU16, NonZeroU32};

use serde::{Deserialize, Serialize};

use crate::components::BandId;
use crate::scalar::Scalar;

/// The fewest lines a band can hold. A band of people descends from *someone*: a split may take
/// a share of its parent's lines but never leaves either half with none.
pub const MIN_BAND_LINES: u16 = 1;

/// One founding line's identity — the band it was founded on and its index among that band's
/// founding lines. Ordered so it can live in a [`BTreeSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LineId {
    /// The [`BandId`] of the starting band the line was founded on.
    pub origin_band: u64,
    /// Its index among that band's founding lines, `0..founding_lines`.
    pub index: u16,
}

/// **The set of founding lines a band descends from.** Carried on
/// [`crate::components::PopulationCohort`], so the checkpoint serializes it with the cohort and a
/// split's `cohort.clone()` starts from the parent's full set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundingLines(BTreeSet<LineId>);

impl FoundingLines {
    /// **A starting band's lines** — `count` fresh identities, all with `origin_band` as their
    /// origin. The only place a line is ever minted.
    pub fn founded(origin_band: BandId, count: u16) -> Self {
        Self(
            (0..count)
                .map(|index| LineId {
                    origin_band: origin_band.0,
                    index,
                })
                .collect(),
        )
    }

    /// How many lines the band holds — the published figure.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Never true of a live band (see [`MIN_BAND_LINES`]); present because `len` is.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The line identities, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &LineId> {
        self.0.iter()
    }

    /// **The lines a split walks off with, removed from `self`.**
    ///
    /// The splinter takes `round(len × share)`, where `share` is the same people share the split
    /// divides everything else on, clamped so **each half keeps at least [`MIN_BAND_LINES`]** — a
    /// tiny split still takes one, a large one never takes the last. The lines taken are the
    /// **highest** ids in set order, so the partition is deterministic.
    ///
    /// **A band holding a single line cannot partition it.** Both halves then descend from that
    /// same family: the splinter gets a copy and the parent keeps it.
    pub fn split_off_share(&mut self, share: Scalar) -> Self {
        let held = self.len();
        let floor = usize::from(MIN_BAND_LINES);
        if held <= floor {
            return self.clone();
        }
        let proportional = (Scalar::from_u32(held as u32) * share).to_u32() as usize;
        let taken = proportional.clamp(floor, held - floor);
        let walked: BTreeSet<LineId> = self.0.iter().rev().take(taken).copied().collect();
        self.0.retain(|line| !walked.contains(line));
        Self(walked)
    }
}

/// **A breeding population's ceiling, in people** (issue #688) — `lines × people_per_line`, where
/// `lines` is the size of the UNION of its member bands' line sets (`lineage.people_per_line`, `K`,
/// in `demographics_config.json`). Births stop there. Saturates rather than wrapping: a union no
/// real world reaches still reads as "no ceiling in sight", never as a tiny one.
pub fn breeding_ceiling(lines: usize, people_per_line: NonZeroU16) -> u32 {
    u32::try_from(lines)
        .unwrap_or(u32::MAX)
        .saturating_mul(u32::from(people_per_line.get()))
}

/// What `PopulationCohortState.breedingCeiling` publishes for a breeding population whose
/// inbreeding ceiling is lifted — "no inbreeding ceiling", never a ceiling of nobody.
pub const NO_INBREEDING_CEILING: u32 = 0;

/// The fewest breeding populations a line a population holds can be held by — itself.
const ONE_HOLDER: u32 = 1;

/// **A breeding population's ceiling when its lines are shared** (issue #688) — `Σ K / holders`
/// over the lines in its union, where `holders` is how many distinct breeding populations hold
/// that line this turn, summed in fixed point and floored to whole people.
///
/// **A line held by several separate populations splits its `K` between them.** A one-line band's
/// split gives both halves a copy of its line ([`FoundingLines::split_off_share`]), so counting the
/// line whole in each would let an isolated people split and scatter past `L × K` with no contact.
/// Shared, the world's ceilings sum to at most `distinct lines × K` however bands split, and two
/// halves that relink are one holder again. With every line held once this is
/// [`breeding_ceiling`]. Each term truncates toward zero and the sum is floored, so the share never
/// rounds up past its line.
pub fn shared_breeding_ceiling(
    holders: impl IntoIterator<Item = u32>,
    people_per_line: NonZeroU16,
) -> u32 {
    let k = Scalar::from_u32(u32::from(people_per_line.get()));
    let total = holders.into_iter().fold(Scalar::zero(), |sum, holders| {
        sum + k / Scalar::from_u32(holders.max(ONE_HOLDER))
    });
    u32::try_from(total.raw().div_euclid(Scalar::SCALE)).unwrap_or(u32::MAX)
}

/// **The inbreeding ceiling that binds a breeding population, if any** (issue #688): its `ceiling`
/// ([`shared_breeding_ceiling`]), or `None` once that reaches `free_breeding_at` — a people that
/// large finds mates without outside contact, so inbreeding no longer restricts its growth.
pub fn inbreeding_ceiling(ceiling: u32, free_breeding_at: NonZeroU32) -> Option<u32> {
    (ceiling < free_breeding_at.get()).then_some(ceiling)
}

/// **A band's breeding population as of this turn** (issue #688) — the bands in its supply-network
/// component (or the band alone, in no network), read after the turn's demographics. Parked on
/// [`crate::components::PopulationCohort::last_breeding`] for publication only: nothing steers off
/// it, `simulate_population` rewrites it every turn, and `Default` (all zero) is what a cohort reads
/// before its first turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreedingReading {
    /// Everyone in the breeding population after this turn's births and deaths, in whole people —
    /// the members' fixed-point head-counts summed, then rounded once. On the wire as
    /// `PopulationCohortState.breedingPopulation`.
    pub headcount: u32,
    /// The ceiling births stopped at: [`shared_breeding_ceiling`] over the members' lines, or
    /// [`NO_INBREEDING_CEILING`] once that reaches `free_breeding_at` and no longer applies. On the
    /// wire as `PopulationCohortState.breedingCeiling`.
    pub ceiling: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: BandId = BandId(42);
    const LINES: u16 = 8;

    fn share(asked: u32, of: u32) -> Scalar {
        Scalar::from_u32(asked) / Scalar::from_u32(of)
    }

    const K: u16 = 19;

    fn k() -> NonZeroU16 {
        NonZeroU16::new(K).unwrap()
    }

    #[test]
    fn a_line_held_once_carries_its_whole_k() {
        assert_eq!(shared_breeding_ceiling([1, 1, 1], k()), 3 * u32::from(K));
        assert_eq!(
            shared_breeding_ceiling([1; 8], k()),
            breeding_ceiling(8, k())
        );
    }

    #[test]
    fn a_line_held_by_two_or_three_splits_its_k_and_floors() {
        // 19 / 2 = 9.5 → 9; 19 / 3 = 6.33 → 6.
        assert_eq!(shared_breeding_ceiling([2], k()), 9);
        assert_eq!(shared_breeding_ceiling([3], k()), 6);
    }

    /// Mixed holder counts sum their shares BEFORE flooring: 19 + 9.5 + 6.333 = 34.83 → 34.
    #[test]
    fn mixed_lines_sum_their_shares_then_floor() {
        assert_eq!(shared_breeding_ceiling([1, 2, 3], k()), 34);
    }

    /// Every holder of one line together never gets more than the line's `K`.
    #[test]
    fn the_holders_of_a_line_never_share_out_more_than_k() {
        for holders in 1..=7_u32 {
            let each = shared_breeding_ceiling([holders], k());
            assert!(each * holders <= u32::from(K), "{holders} holders × {each}");
        }
    }

    #[test]
    fn a_founded_set_holds_count_lines_all_from_its_origin() {
        let lines = FoundingLines::founded(ORIGIN, LINES);
        assert_eq!(lines.len(), usize::from(LINES));
        assert!(lines.iter().all(|line| line.origin_band == ORIGIN.0));
    }

    #[test]
    fn a_split_partitions_the_highest_ids_and_keeps_both_halves_nonempty() {
        let original = FoundingLines::founded(ORIGIN, LINES);
        for (asked, of) in [(1, 30), (5, 30), (15, 30), (29, 30)] {
            let mut parent = original.clone();
            let child = parent.split_off_share(share(asked, of));
            let floor = usize::from(MIN_BAND_LINES);
            assert!(child.len() >= floor && parent.len() >= floor);
            assert!(parent.0.is_disjoint(&child.0));
            let union: BTreeSet<_> = parent.0.union(&child.0).copied().collect();
            assert_eq!(union, original.0);
            assert!(parent.iter().max() < child.iter().min());
        }
    }
}
