//! Treaties — what two peoples have promised each other and hold in force
//! (`docs/plan_early_diplomacy.md` §3, issue #233).
//!
//! **A treaty is a promise between two peoples, held by both.** [`Treaties`] (`SimState`) is keyed by
//! the *unordered* faction pair and holds the terms in force between them. It has no end date: either
//! side may end it, and ending it is breaking it ([`end_treaty`], a breach that costs the breaker the
//! other people's goodwill). It dissolves with no penalty when either people stops existing
//! ([`dissolve_extinct_peoples`]).
//!
//! Three terms ([`TreatyTerms`]):
//!
//! - **road rights** — each people's built roads count fully for the other's walks
//!   ([`crate::routes::RoadAccess`]);
//! - **open borders** — each takes in the other's leavers and hosts the other's parties, whatever
//!   its general setting ([`crate::resources::FactionBorderPolicies::admits`]);
//! - **agreed trade** — standing [`struct@Bundle`]s a shipment between the two peoples is matched against on
//!   arrival and settled at once ([`match_bundles`]).
//!
//! **Amending is proposing again.** A treaty proposal ([`crate::dealings::DealingKind::TreatyProposal`])
//! carries the *full* terms it asks for and replaces whatever was in force when it is accepted;
//! dropping a term that way is not a breach. A treaty with no term and no bundle is not stored.
//!
//! The terms are *promises*, so a [`struct@Bundle`]'s lists are [`GoodsLines`] — unrated, because no goods
//! have been drawn — exactly as a stage-4 proposal's legs are.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{PopulationCohort, ResidentBand},
    dealings::GoodsLines,
    goodwill::{Goodwill, GoodwillSource},
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    scalar::{scalar_zero, Scalar},
};

/// One standing exchange: a shipment carrying `sent` is paid `returned` on arrival.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// What the shipping people sends. A bundle must send something — an empty `sent` would be
    /// covered by any cargo, however many times over.
    pub sent: GoodsLines,
    /// What the receiving people pays back. May be empty (a standing gift).
    pub returned: GoodsLines,
}

/// The terms a treaty holds or a proposal asks for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TreatyTerms {
    pub road_rights: bool,
    pub open_borders: bool,
    pub agreed_trade: Vec<Bundle>,
}

impl TreatyTerms {
    /// No term is true and there is no bundle: a treaty of this is not a treaty.
    pub fn is_empty(&self) -> bool {
        !self.road_rights && !self.open_borders && self.agreed_trade.is_empty()
    }

    /// How many terms are in force: road rights and open borders one each, and one more if there is
    /// any bundle at all (the agreed trade is one term however many bundles it lists). What
    /// `goodwill.per_term` is paid on.
    pub fn term_count(&self) -> u32 {
        u32::from(self.road_rights)
            + u32::from(self.open_borders)
            + u32::from(!self.agreed_trade.is_empty())
    }

    /// The terms as a sentence for a feed line.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.road_rights {
            parts.push("road rights".to_string());
        }
        if self.open_borders {
            parts.push("open borders".to_string());
        }
        if !self.agreed_trade.is_empty() {
            let count = self.agreed_trade.len();
            parts.push(format!(
                "agreed trade ({count} bundle{})",
                if count == 1 { "" } else { "s" }
            ));
        }
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(", ")
        }
    }
}

impl TreatyTerms {
    /// **Resolve a command's terms**: every bundle's two legs through [`GoodsLines::resolve`], each
    /// bundle sending something (an empty `sent` would be covered by any cargo, any number of
    /// times). `verb` prefixes a refusal so every verb reads as itself.
    pub fn resolve(
        world: &World,
        request: &sim_runtime::TreatyTermsRequest,
        verb: &str,
    ) -> Result<Self, String> {
        let mut agreed_trade = Vec::with_capacity(request.bundles.len());
        for bundle in &request.bundles {
            let sent = GoodsLines::resolve(world, &bundle.give, verb)?;
            let returned = GoodsLines::resolve(world, &bundle.ask, verb)?;
            if sent.is_empty() {
                return Err(format!(
                    "{verb}: a bundle must send something — name what the shipment carries after \
                     'give'."
                ));
            }
            agreed_trade.push(Bundle { sent, returned });
        }
        Ok(Self {
            road_rights: request.road_rights,
            open_borders: request.open_borders,
            agreed_trade,
        })
    }
}

/// **How an envoy's errand ended**, as the envoy carries it home: the receiving people's answer.
/// Held on the party ([`crate::components::ShipmentTerms::Envoy`]) until it folds back, where the
/// envoy's own people are told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EnvoyOutcome {
    Accepted,
    Refused,
    /// Silence at the deadline.
    Expired,
    /// The receiving people asked for these terms instead.
    Countered(TreatyTerms),
}

/// A treaty in force: its terms and the turn it took effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Treaty {
    pub road_rights: bool,
    pub open_borders: bool,
    pub agreed_trade: Vec<Bundle>,
    pub formed_turn: u64,
}

impl Treaty {
    pub fn from_terms(terms: TreatyTerms, formed_turn: u64) -> Self {
        Self {
            road_rights: terms.road_rights,
            open_borders: terms.open_borders,
            agreed_trade: terms.agreed_trade,
            formed_turn,
        }
    }

    pub fn terms(&self) -> TreatyTerms {
        TreatyTerms {
            road_rights: self.road_rights,
            open_borders: self.open_borders,
            agreed_trade: self.agreed_trade.clone(),
        }
    }
}

/// The unordered pair a treaty is keyed by: low id first.
fn pair(a: FactionId, b: FactionId) -> (FactionId, FactionId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// **Every treaty in force, keyed by the unordered faction pair.** `SimState`: a treaty is a
/// standing promise, so a rollback that dropped it would quietly cancel it for both peoples.
///
/// **A people has no treaty with itself** — a same-people pair is never stored and every reader
/// answers "no term" for one, so no caller needs the faction comparison.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Treaties {
    items: BTreeMap<(FactionId, FactionId), Treaty>,
}

/// **No treaty at all** — what a reader with no [`Treaties`] resource (a hand-rolled test world)
/// reads, so it needs no `Option` branch of its own.
pub static NO_TREATIES: Treaties = Treaties {
    items: BTreeMap::new(),
};

impl Treaties {
    pub fn get(&self, a: FactionId, b: FactionId) -> Option<&Treaty> {
        if a == b {
            return None;
        }
        self.items.get(&pair(a, b))
    }

    /// **Put these terms in force between two peoples**, replacing whatever was (an amendment).
    /// Terms that hold nothing remove the treaty; a same-people pair stores nothing. Returns
    /// whether a treaty stands afterwards.
    pub fn set(&mut self, a: FactionId, b: FactionId, terms: TreatyTerms, turn: u64) -> bool {
        if a == b {
            return false;
        }
        let key = pair(a, b);
        if terms.is_empty() {
            self.items.remove(&key);
            return false;
        }
        self.items.insert(key, Treaty::from_terms(terms, turn));
        true
    }

    /// Remove the treaty between two peoples, handing it back.
    pub fn end(&mut self, a: FactionId, b: FactionId) -> Option<Treaty> {
        if a == b {
            return None;
        }
        self.items.remove(&pair(a, b))
    }

    /// Does a road-rights term join these two peoples?
    pub fn road_rights(&self, a: FactionId, b: FactionId) -> bool {
        self.get(a, b).is_some_and(|treaty| treaty.road_rights)
    }

    /// Does an open-borders term join these two peoples?
    pub fn open_borders(&self, a: FactionId, b: FactionId) -> bool {
        self.get(a, b).is_some_and(|treaty| treaty.open_borders)
    }

    /// The standing bundles between two peoples, empty when none.
    pub fn bundles(&self, a: FactionId, b: FactionId) -> &[Bundle] {
        self.get(a, b)
            .map_or(&[][..], |treaty| treaty.agreed_trade.as_slice())
    }

    /// Every treaty, in pair order.
    pub fn iter(&self) -> impl Iterator<Item = (FactionId, FactionId, &Treaty)> {
        self.items.iter().map(|((a, b), treaty)| (*a, *b, treaty))
    }

    /// The treaties `viewer` is party to, as `(other people, treaty)`, in the other's id order.
    pub fn of_viewer(&self, viewer: FactionId) -> Vec<(FactionId, &Treaty)> {
        let mut rows: Vec<(FactionId, &Treaty)> = self
            .items
            .iter()
            .filter_map(|((a, b), treaty)| {
                if *a == viewer {
                    Some((*b, treaty))
                } else if *b == viewer {
                    Some((*a, treaty))
                } else {
                    None
                }
            })
            .collect();
        rows.sort_by_key(|(other, _)| *other);
        rows
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// **Silently dissolve every treaty a people is party to**, returning the peoples it was with.
    pub fn dissolve_for(&mut self, faction: FactionId) -> Vec<FactionId> {
        let others: Vec<FactionId> = self
            .items
            .keys()
            .filter_map(|(a, b)| {
                if *a == faction {
                    Some(*b)
                } else if *b == faction {
                    Some(*a)
                } else {
                    None
                }
            })
            .collect();
        for other in &others {
            self.items.remove(&pair(faction, *other));
        }
        others
    }
}

// ------------------------------------------------------------------------------------------------
// Ending
// ------------------------------------------------------------------------------------------------

/// **Break a treaty: remove it at once and cost the breaker.** The other people's goodwill toward
/// the breaker takes `-broken_treaty` into the `breach` component (the breaker's own opinion of the
/// other does not move). Both peoples are told. Returns the treaty that ended, `None` when there was
/// none (and nothing moved).
#[allow(clippy::too_many_arguments)] // the three ledgers it writes, the clock, both peoples and why
pub fn end_treaty(
    treaties: &mut Treaties,
    goodwill: &mut Goodwill,
    log: &mut CommandEventLog,
    tick: u64,
    breaker: FactionId,
    other: FactionId,
    broken_treaty: f32,
    reason: &'static str,
) -> Option<Treaty> {
    let ended = treaties.end(breaker, other)?;
    goodwill.credit(other, breaker, GoodwillSource::Breach, -broken_treaty);
    for (told, partner, line) in [
        (
            breaker,
            other,
            format!("You ended your treaty with Faction {}", other.0),
        ),
        (
            other,
            breaker,
            format!("Faction {} ended its treaty with you", breaker.0),
        ),
    ] {
        log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Treaty,
            told,
            line,
            Some(format!(
                "status=ended reason={reason} with={} breaker={}",
                partner.0, breaker.0
            )),
        ));
    }
    Some(ended)
}

// ------------------------------------------------------------------------------------------------
// Agreed trade — matching a cargo against the standing bundles
// ------------------------------------------------------------------------------------------------

/// What a cargo covers of a treaty's bundles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BundleMatch {
    /// How many whole times each bundle (in bundle order) is covered.
    pub counts: Vec<u32>,
    /// The covered part of the cargo: `Σ kᵢ × sentᵢ`.
    pub covered: GoodsLines,
    /// What the receiver pays for it: `Σ kᵢ × returnedᵢ`.
    pub returned: GoodsLines,
}

impl BundleMatch {
    /// Nothing is covered.
    pub fn is_empty(&self) -> bool {
        self.counts.iter().all(|k| *k == 0)
    }
}

fn scaled(lines: &GoodsLines, times: u32) -> GoodsLines {
    let factor = Scalar::from_u32(times);
    GoodsLines {
        food: lines.food * factor,
        fodder: lines.fodder * factor,
        materials: lines
            .materials
            .iter()
            .map(|(id, amount)| (id.clone(), *amount * factor))
            .collect(),
    }
}

fn add_lines(into: &mut GoodsLines, from: &GoodsLines) {
    into.food += from.food;
    into.fodder += from.fodder;
    for (id, amount) in &from.materials {
        match into.materials.iter_mut().find(|(held, _)| held == id) {
            Some((_, held)) => *held += *amount,
            None => into.materials.push((id.clone(), *amount)),
        }
    }
}

fn material_of(lines: &GoodsLines, id: &str) -> Scalar {
    lines
        .materials
        .iter()
        .filter(|(held, _)| held == id)
        .fold(scalar_zero(), |total, (_, amount)| total + *amount)
}

/// How many whole times `cargo` covers `sent`: the least over every good the bundle names of
/// `cargo ÷ sent`, floored. Zero when the bundle sends nothing (it can never be covered).
fn times_covered(cargo: &GoodsLines, sent: &GoodsLines) -> u32 {
    if sent.is_empty() {
        return 0;
    }
    let mut times = u32::MAX;
    let mut consider = |held: Scalar, wanted: Scalar| {
        if wanted > scalar_zero() {
            // Fixed-point raw units divide exactly: whole copies, no float in the count.
            let whole = (held.raw().max(0) / wanted.raw()).min(i64::from(u32::MAX));
            times = times.min(whole as u32);
        }
    };
    consider(cargo.food, sent.food);
    consider(cargo.fodder, sent.fodder);
    for (id, wanted) in &sent.materials {
        consider(material_of(cargo, id), *wanted);
    }
    if times == u32::MAX {
        0
    } else {
        times
    }
}

/// **Match a cargo against a treaty's bundles — whole bundles, greedily, in bundle order.** Each
/// bundle in turn takes as many whole copies as the cargo still covers, and what it takes is not
/// available to the next. Cargo beyond the whole bundles is the shipment's own business.
pub fn match_bundles(cargo: &GoodsLines, bundles: &[Bundle]) -> BundleMatch {
    let mut remaining = cargo.clone();
    let mut result = BundleMatch::default();
    for bundle in bundles {
        let times = times_covered(&remaining, &bundle.sent);
        result.counts.push(times);
        if times == 0 {
            continue;
        }
        let taken = scaled(&bundle.sent, times);
        // Spend what the bundle took, so the next bundle sees only what is left.
        remaining.food -= taken.food;
        remaining.fodder -= taken.fodder;
        for (id, amount) in &taken.materials {
            if let Some((_, held)) = remaining.materials.iter_mut().find(|(held, _)| held == id) {
                *held -= *amount;
            }
        }
        add_lines(&mut result.covered, &taken);
        add_lines(&mut result.returned, &scaled(&bundle.returned, times));
    }
    result
}

// ------------------------------------------------------------------------------------------------
// Systems
// ------------------------------------------------------------------------------------------------

/// **A people that has stopped existing takes its treaties with it, with no breach.** A people
/// exists while it has a resident band; the roster never drops an id, so the end of a people is the
/// end of its last band (a band that went over to another people, a band lost to hunger). Run each
/// turn in the Visibility stage, after the stage's other writers of band state.
pub fn dissolve_extinct_peoples(
    mut treaties: ResMut<Treaties>,
    tick: Res<SimulationTick>,
    mut log: ResMut<CommandEventLog>,
    bands: Query<&PopulationCohort, With<ResidentBand>>,
) {
    if treaties.is_empty() {
        return;
    }
    let living: std::collections::BTreeSet<FactionId> =
        bands.iter().map(|cohort| cohort.faction).collect();
    let gone: Vec<FactionId> = treaties
        .iter()
        .flat_map(|(a, b, _)| [a, b])
        .filter(|faction| !living.contains(faction))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for faction in gone {
        for partner in treaties.dissolve_for(faction) {
            if living.contains(&partner) {
                log.push(CommandEventEntry::new(
                    tick.0,
                    CommandEventKind::Treaty,
                    partner,
                    format!(
                        "Your treaty with Faction {} lapsed: that people is no more",
                        faction.0
                    ),
                    Some(format!(
                        "status=ended reason=people_gone with={} breaker=0",
                        faction.0
                    )),
                ));
            }
        }
    }
}

/// **A treaty in force raises each people's opinion of the other, every turn**: each way,
/// `goodwill.per_term` per term held (road rights, open borders, and agreed trade as one term however
/// many bundles). It settles near `per_term × terms / fade` like the link credit does, and word of it
/// reaches the other people at once (a treaty is between two governments that know it).
///
/// Runs in `Visibility`, after the fade (`Influence`) and after [`dissolve_extinct_peoples`], so a
/// treaty whose people just ended credits nothing.
pub fn credit_treaty_goodwill(
    tick: Res<SimulationTick>,
    config: Res<crate::diplomacy_config::DiplomacyConfigHandle>,
    treaties: Res<Treaties>,
    mut goodwill: ResMut<Goodwill>,
    mut known: ResMut<crate::goodwill::KnownGoodwill>,
) {
    if treaties.is_empty() {
        return;
    }
    let per_term = config.get().goodwill.per_term;
    for (a, b, treaty) in treaties.iter() {
        let credit = per_term * treaty.terms().term_count() as f32;
        goodwill.credit(a, b, GoodwillSource::Treaty, credit);
        goodwill.credit(b, a, GoodwillSource::Treaty, credit);
        for (viewer, other) in [(a, b), (b, a)] {
            known.record(
                viewer,
                other,
                crate::goodwill::GoodwillNews {
                    components: goodwill.get(other, viewer),
                    turn: tick.0,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn food(amount: f32) -> GoodsLines {
        GoodsLines {
            food: crate::scalar::scalar_from_f32(amount),
            ..Default::default()
        }
    }

    fn hide(amount: f32) -> GoodsLines {
        GoodsLines {
            materials: vec![("hide".to_string(), crate::scalar::scalar_from_f32(amount))],
            ..Default::default()
        }
    }

    #[test]
    fn the_pair_is_unordered_and_a_people_has_no_treaty_with_itself() {
        let mut treaties = Treaties::default();
        let terms = TreatyTerms {
            road_rights: true,
            ..Default::default()
        };
        assert!(treaties.set(FactionId(2), FactionId(1), terms.clone(), 5));
        assert!(treaties.road_rights(FactionId(1), FactionId(2)));
        assert!(treaties.road_rights(FactionId(2), FactionId(1)));
        assert!(!treaties.set(FactionId(1), FactionId(1), terms, 5));
        assert!(!treaties.road_rights(FactionId(1), FactionId(1)));
        assert_eq!(treaties.len(), 1);
    }

    #[test]
    fn terms_that_hold_nothing_are_not_stored_and_replace_what_was() {
        let mut treaties = Treaties::default();
        treaties.set(
            FactionId(0),
            FactionId(1),
            TreatyTerms {
                open_borders: true,
                ..Default::default()
            },
            3,
        );
        assert_eq!(
            treaties
                .get(FactionId(0), FactionId(1))
                .unwrap()
                .formed_turn,
            3
        );
        treaties.set(FactionId(0), FactionId(1), TreatyTerms::default(), 4);
        assert!(treaties.is_empty());
    }

    #[test]
    fn a_term_count_is_one_each_and_one_for_any_bundles() {
        let terms = TreatyTerms {
            road_rights: true,
            open_borders: true,
            agreed_trade: vec![
                Bundle {
                    sent: food(1.0),
                    returned: hide(1.0),
                },
                Bundle {
                    sent: food(2.0),
                    returned: hide(1.0),
                },
            ],
        };
        assert_eq!(terms.term_count(), 3);
        assert_eq!(TreatyTerms::default().term_count(), 0);
    }

    #[test]
    fn bundles_are_matched_whole_greedily_in_order() {
        let bundles = vec![
            Bundle {
                sent: food(4.0),
                returned: hide(1.0),
            },
            Bundle {
                sent: food(3.0),
                returned: hide(2.0),
            },
        ];
        // 10 food: the first bundle takes two copies (8), leaving 2 — not a whole second bundle.
        let matched = match_bundles(&food(10.0), &bundles);
        assert_eq!(matched.counts, vec![2, 0]);
        assert_eq!(matched.covered.food, crate::scalar::scalar_from_f32(8.0));
        assert_eq!(
            material_of(&matched.returned, "hide"),
            crate::scalar::scalar_from_f32(2.0)
        );
        // 12 food: two copies of the first (8) then one of the second (3), 1 over.
        let matched = match_bundles(&food(15.0), &bundles);
        assert_eq!(matched.counts, vec![3, 1]);
        assert_eq!(matched.covered.food, crate::scalar::scalar_from_f32(15.0));
        assert_eq!(
            material_of(&matched.returned, "hide"),
            crate::scalar::scalar_from_f32(5.0)
        );
    }

    #[test]
    fn a_bundle_naming_two_goods_is_covered_by_the_scarcer() {
        let bundle = Bundle {
            sent: GoodsLines {
                food: crate::scalar::scalar_from_f32(2.0),
                materials: vec![("hide".to_string(), crate::scalar::scalar_from_f32(1.0))],
                ..Default::default()
            },
            returned: food(1.0),
        };
        let cargo = GoodsLines {
            food: crate::scalar::scalar_from_f32(10.0),
            materials: vec![("hide".to_string(), crate::scalar::scalar_from_f32(3.0))],
            ..Default::default()
        };
        let matched = match_bundles(&cargo, &[bundle]);
        assert_eq!(matched.counts, vec![3]);
    }

    #[test]
    fn a_cargo_short_of_every_bundle_covers_nothing() {
        let bundles = vec![Bundle {
            sent: food(5.0),
            returned: hide(1.0),
        }];
        assert!(match_bundles(&food(4.9), &bundles).is_empty());
        assert!(match_bundles(&hide(9.0), &bundles).is_empty());
    }

    #[test]
    fn ending_a_treaty_costs_the_breaker_the_others_goodwill_only() {
        let mut treaties = Treaties::default();
        treaties.set(
            FactionId(0),
            FactionId(1),
            TreatyTerms {
                road_rights: true,
                ..Default::default()
            },
            1,
        );
        let mut goodwill = Goodwill::default();
        let mut log = CommandEventLog::default();
        let ended = end_treaty(
            &mut treaties,
            &mut goodwill,
            &mut log,
            2,
            FactionId(0),
            FactionId(1),
            0.5,
            "ended",
        );
        assert!(ended.is_some());
        assert!(treaties.is_empty());
        assert!(
            (goodwill.get(FactionId(1), FactionId(0)).breach + 0.5).abs() < 1e-6,
            "the other people thinks worse of the breaker"
        );
        assert_eq!(goodwill.get(FactionId(0), FactionId(1)).breach, 0.0);
        // Nothing to end: nothing moves.
        assert!(end_treaty(
            &mut treaties,
            &mut goodwill,
            &mut log,
            3,
            FactionId(0),
            FactionId(1),
            0.5,
            "ended"
        )
        .is_none());
    }

    #[test]
    fn dissolving_a_people_removes_its_treaties_and_names_the_partners() {
        let mut treaties = Treaties::default();
        for other in [1, 2] {
            treaties.set(
                FactionId(0),
                FactionId(other),
                TreatyTerms {
                    open_borders: true,
                    ..Default::default()
                },
                1,
            );
        }
        treaties.set(
            FactionId(1),
            FactionId(2),
            TreatyTerms {
                open_borders: true,
                ..Default::default()
            },
            1,
        );
        let partners = treaties.dissolve_for(FactionId(0));
        assert_eq!(partners, vec![FactionId(1), FactionId(2)]);
        assert_eq!(treaties.len(), 1, "the third pair's treaty stands");
    }
}
