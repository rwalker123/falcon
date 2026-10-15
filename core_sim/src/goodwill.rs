//! Goodwill — what one people thinks of another (`docs/plan_early_diplomacy.md` §2, issue #233).
//!
//! **One opinion per ordered pair of peoples**: how much people A (the *holder*) thinks of people B
//! (*about*), from −1 (hostile) to +1 (friendly), starting at 0. It is a people-level opinion, never
//! a band tie and never stored on a connection.
//!
//! **The entry is its reasons.** [`GoodwillEntry`] stores one `f32` per source kind; the opinion is
//! their sum. Fade is proportional on every component alike, so the sum fades exactly as one number
//! would, and the readout can say *why* a people is warm or wary. The total is clamped to
//! `[-1, +1]` by scaling the **credit** that would cross a bound — existing components are never
//! rescaled.
//!
//! **A people knows its own opinion live; what another people thinks of it arrives as news.**
//! [`KnownGoodwill`] holds, keyed `(viewer, other)`, a snapshot of the other people's opinion of the
//! viewer and the turn it was observed. Parties carry the news
//! ([`crate::components::Expedition::goodwill_news`]) and land it where their other news lands.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    carry::{CarryConfig, CarryLoad},
    components::{
        available_workers, BandId, Expedition, ExpeditionPhase, PopulationCohort, ResidentBand,
        Tile,
    },
    diplomacy_config::DiplomacyConfigHandle,
    orders::FactionId,
    resources::{SimulationConfig, SimulationTick, TileRegistry},
    routes::RoadRegistry,
    scalar::Scalar,
    LocalStore, FODDER, FOOD,
};

/// The lowest total an opinion can reach.
pub const GOODWILL_FLOOR: f32 = -1.0;
/// The highest total an opinion can reach.
pub const GOODWILL_CEILING: f32 = 1.0;

/// Which kind of thing raised (or lowered) an opinion. One component of [`GoodwillEntry`] each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoodwillSource {
    /// A gift landed (an accepted offer asking nothing back).
    Gift,
    /// An exchange settled (an accepted offer with a return).
    Exchange,
    /// A party stationed in the holder's band.
    Stationed,
    /// A standing link between the two peoples ([`credit_link_goodwill`]).
    Link,
    /// A treaty term in force (a later slice writes it).
    Treaty,
    /// A breached treaty (a later slice writes it; negative).
    Breach,
}

/// **An opinion as its reasons.** The total is [`Self::total`], never stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct GoodwillEntry {
    pub gifts: f32,
    pub exchanges: f32,
    pub stationed: f32,
    pub link: f32,
    pub treaty: f32,
    pub breach: f32,
}

impl GoodwillEntry {
    /// The opinion: the sum of its reasons.
    pub fn total(&self) -> f32 {
        self.gifts + self.exchanges + self.stationed + self.link + self.treaty + self.breach
    }

    fn component_mut(&mut self, source: GoodwillSource) -> &mut f32 {
        match source {
            GoodwillSource::Gift => &mut self.gifts,
            GoodwillSource::Exchange => &mut self.exchanges,
            GoodwillSource::Stationed => &mut self.stationed,
            GoodwillSource::Link => &mut self.link,
            GoodwillSource::Treaty => &mut self.treaty,
            GoodwillSource::Breach => &mut self.breach,
        }
    }

    /// Proportional fade on every component: `x -= x × fade`.
    fn fade(&mut self, fade_per_turn: f32) {
        for component in [
            &mut self.gifts,
            &mut self.exchanges,
            &mut self.stationed,
            &mut self.link,
            &mut self.treaty,
            &mut self.breach,
        ] {
            *component -= *component * fade_per_turn;
        }
    }

    fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// **Every opinion, keyed `(holder, about)`.** `SimState`. A self-pair is never stored: an opinion of
/// your own people is meaningless, and [`Self::credit`] — the one seam every source writes through —
/// is where that is enforced.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Goodwill {
    entries: BTreeMap<(FactionId, FactionId), GoodwillEntry>,
}

impl Goodwill {
    /// What `holder` thinks of `about`, as its reasons. Zero for a pair never credited.
    pub fn get(&self, holder: FactionId, about: FactionId) -> GoodwillEntry {
        self.entries
            .get(&(holder, about))
            .copied()
            .unwrap_or_default()
    }

    /// **THE credit seam.** Add `amount` to `holder`'s opinion of `about` under `source`. A self-pair
    /// and a non-finite or zero amount do nothing. When the credit would carry the total past
    /// `[-1, +1]` it is scaled down so the total lands on the bound; existing components are never
    /// rescaled.
    pub fn credit(
        &mut self,
        holder: FactionId,
        about: FactionId,
        source: GoodwillSource,
        amount: f32,
    ) {
        if holder == about || !amount.is_finite() || amount == 0.0 {
            return;
        }
        let entry = self.entries.entry((holder, about)).or_default();
        let total = entry.total();
        let landed = (total + amount).clamp(GOODWILL_FLOOR, GOODWILL_CEILING);
        *entry.component_mut(source) += landed - total;
    }

    /// Fade every entry once, proportionally. Entries that have faded to nothing are dropped so the
    /// map does not grow without bound.
    pub fn fade(&mut self, fade_per_turn: f32) {
        for entry in self.entries.values_mut() {
            entry.fade(fade_per_turn);
        }
        self.entries.retain(|_, entry| !entry.is_zero());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(FactionId, FactionId), &GoodwillEntry)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// **One observation of an opinion, carried by a party**: the components as they stood and the turn
/// they were observed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GoodwillNews {
    pub components: GoodwillEntry,
    pub turn: u64,
}

/// A reported opinion, as the viewer holds it.
pub type KnownGoodwillEntry = GoodwillNews;

/// **What other peoples think of you, as last reported**, keyed `(viewer, other)`. `SimState`. Never
/// live: it moves only when a party carrying news lands it. A report never moves the clock backwards
/// (`connections.md`'s clock-1 rule): an older or equal `turn` than the one held is ignored.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KnownGoodwill {
    entries: BTreeMap<(FactionId, FactionId), KnownGoodwillEntry>,
}

impl KnownGoodwill {
    /// Land one piece of news. A self-pair is ignored; a report no newer than the one held is too.
    pub fn record(&mut self, viewer: FactionId, other: FactionId, news: GoodwillNews) {
        if viewer == other {
            return;
        }
        match self.entries.get(&(viewer, other)) {
            Some(held) if held.turn >= news.turn => {}
            _ => {
                self.entries.insert((viewer, other), news);
            }
        }
    }

    pub fn get(&self, viewer: FactionId, other: FactionId) -> Option<KnownGoodwillEntry> {
        self.entries.get(&(viewer, other)).copied()
    }

    /// The viewer's rows, in `other` order — what a frame carries.
    pub fn for_viewer(
        &self,
        viewer: FactionId,
    ) -> impl Iterator<Item = (FactionId, &KnownGoodwillEntry)> {
        self.entries
            .iter()
            .filter(move |((held_by, _), _)| *held_by == viewer)
            .map(|((_, other), entry)| (*other, entry))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// Worth
// ---------------------------------------------------------------------------------------------

// `GoodAmount`, `good_worth` and `worth` are `sim_runtime::worth`: one formula, shared with the AI.
pub use sim_runtime::{good_worth, worth, GoodAmount};

fn unit_load(carry: &CarryConfig, food: i64, fodder: i64, materials: i64) -> f32 {
    CarryLoad {
        food: Scalar::from_i64(food),
        fodder: Scalar::from_i64(fodder),
        items: 0,
        materials: Scalar::from_i64(materials),
    }
    .load(carry)
    .to_f32()
}

/// **A cargo's worth against what the receiving band holds**: food, fodder and each material id are
/// separate goods (a material's quality batches sum by amount within that material only), each
/// weighted by the load of its amount in the one carry currency.
pub fn cargo_worth(cargo: &LocalStore, held: &LocalStore, carry: &CarryConfig) -> f32 {
    let mut goods = vec![
        GoodAmount {
            load: unit_load(carry, 1, 0, 0) * cargo.get(FOOD).to_f32(),
            amount: cargo.get(FOOD).to_f32(),
            held: held.get(FOOD).to_f32(),
        },
        GoodAmount {
            load: unit_load(carry, 0, 1, 0) * cargo.get(FODDER).to_f32(),
            amount: cargo.get(FODDER).to_f32(),
            held: held.get(FODDER).to_f32(),
        },
    ];
    let material_load = unit_load(carry, 0, 0, 1);
    for (material, _) in cargo.materials() {
        goods.push(GoodAmount {
            load: material_load * cargo.material_total(material).to_f32(),
            amount: cargo.material_total(material).to_f32(),
            held: held.material_total(material).to_f32(),
        });
    }
    worth(&goods)
}

// ---------------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------------

/// **Fade every opinion once**, in `TurnStage::Influence` — the first stage of the turn, so it runs
/// before any credit this turn's systems make (the stationed credit is in `Population`; stage sets
/// are chained). A steady source paying `s` a turn settles at `s / fade_per_turn`.
pub fn fade_goodwill(config: Res<DiplomacyConfigHandle>, mut goodwill: ResMut<Goodwill>) {
    if goodwill.is_empty() {
        return;
    }
    goodwill.fade(config.get().goodwill.fade_per_turn);
}

/// **A stationed party raises the host people's opinion of its people**, each turn it stands in a
/// foreign host's camp: `per_person_stationed × party workers`, host people → sender people, into
/// `stationed`. Runs in the `Population` chain after the stage's expedition systems have settled
/// every party's phase for the turn.
pub fn credit_stationed_goodwill(
    config: Res<DiplomacyConfigHandle>,
    mut goodwill: ResMut<Goodwill>,
    parties: Query<(&Expedition, &PopulationCohort)>,
    bands: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
) {
    let per_person = config.get().goodwill.per_person_stationed;
    for (expedition, party) in parties.iter() {
        if expedition.phase != ExpeditionPhase::Stationed {
            continue;
        }
        let Some(host) = expedition.mission.host_band() else {
            continue;
        };
        let Some(host_faction) = bands
            .iter()
            .find(|(id, _)| **id == host)
            .map(|(_, cohort)| cohort.faction)
        else {
            continue;
        };
        goodwill.credit(
            host_faction,
            party.faction,
            GoodwillSource::Stationed,
            per_person * available_workers(party.working) as f32,
        );
    }
}

/// **A standing link raises each people's opinion of the other**, once per turn per pair of
/// peoples: `per_link` into `link`, both ways, whenever any band of one people and any band of the
/// other are joined by a standing link ([`crate::routes::standing_link`]).
///
/// **ONCE PER PEOPLE PAIR PER TURN, however many band pairs the road joins.** Goodwill is a
/// people-level opinion (`docs/plan_early_diplomacy.md` §2), and a steady source is meant to settle
/// at `s / fade_per_turn`; crediting per band pair would let a people multiply a road's worth by
/// founding bands along it. Traffic over the link counts through its exchanges, not here.
///
/// The same pass **refreshes what each people knows of the other's opinion**
/// ([`KnownGoodwill`], both ways) with the value standing right now: word travels along a kept
/// road, so no party has to carry it. It runs in `TurnStage::Visibility`, where the roads' keeping
/// has been judged for the turn (`Road::keeping_is_met` reads the bill the labour pass stamped —
/// the same moment `light_kept_routes` reads it at), after the fade (`Influence`) and after the
/// turn's other credits.
#[allow(clippy::too_many_arguments)] // the geometry, the roads, the config and the two ledgers
pub fn credit_link_goodwill(
    tick: Res<SimulationTick>,
    config: Res<DiplomacyConfigHandle>,
    roads: Res<RoadRegistry>,
    tile_registry: Res<TileRegistry>,
    sim_config: Res<SimulationConfig>,
    tiles: Query<&Tile>,
    bands: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
    mut goodwill: ResMut<Goodwill>,
    mut known: ResMut<KnownGoodwill>,
) {
    let config = config.get();
    let floor = config.offers.standing_link_floor();
    // Only a band standing on a qualifying road can end a standing link, so filter to those first:
    // a world with no roads walks no pair at all.
    let mut on_a_road: Vec<(BandId, FactionId, UVec2)> = bands
        .iter()
        .filter_map(|(id, cohort)| {
            let position = tiles.get(cohort.current_tile).ok()?.position;
            roads
                .road(position)
                .is_some_and(|road| road.keeping_is_met() && road.held_rung().is_at_or_above(floor))
                .then_some((*id, cohort.faction, position))
        })
        .collect();
    if on_a_road.len() < 2 {
        return;
    }
    on_a_road.sort_by_key(|(id, _, _)| *id);
    let mut linked: BTreeSet<(FactionId, FactionId)> = BTreeSet::new();
    for (i, (_, a_faction, a_pos)) in on_a_road.iter().enumerate() {
        for (_, b_faction, b_pos) in &on_a_road[i + 1..] {
            if a_faction == b_faction {
                continue;
            }
            let pair = (*a_faction.min(b_faction), *a_faction.max(b_faction));
            if linked.contains(&pair) {
                continue;
            }
            if crate::routes::standing_link(
                &roads,
                *a_pos,
                *b_pos,
                tile_registry.width,
                tile_registry.height,
                sim_config.map_topology.wrap_horizontal,
                floor,
            ) {
                linked.insert(pair);
            }
        }
    }
    let per_link = config.goodwill.per_link;
    let now = tick.0;
    for (a, b) in linked {
        goodwill.credit(a, b, GoodwillSource::Link, per_link);
        goodwill.credit(b, a, GoodwillSource::Link, per_link);
        // What `b` thinks of `a` is what `a` knows of `b`, and the other way round.
        known.record(
            a,
            b,
            GoodwillNews {
                components: goodwill.get(b, a),
                turn: now,
            },
        );
        known.record(
            b,
            a,
            GoodwillNews {
                components: goodwill.get(a, b),
                turn: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: FactionId = FactionId(0);
    const B: FactionId = FactionId(1);
    const EPSILON: f32 = 1e-5;

    fn good(held_multiple: f32) -> GoodAmount {
        GoodAmount {
            load: 4.0,
            amount: 4.0,
            held: 4.0 * held_multiple,
        }
    }

    #[test]
    fn a_good_nobody_holds_counts_at_its_full_load() {
        assert!((good_worth(good(0.0)) - 4.0).abs() < EPSILON);
    }

    #[test]
    fn a_good_held_ten_times_over_counts_for_about_a_tenth() {
        assert!((good_worth(good(10.0)) - 4.0 / 11.0).abs() < EPSILON);
    }

    #[test]
    fn worth_is_per_good_and_an_empty_gift_is_worth_nothing() {
        assert_eq!(worth(&[]), 0.0);
        assert_eq!(
            good_worth(GoodAmount {
                load: 0.0,
                amount: 0.0,
                held: 0.0
            }),
            0.0
        );
        let two = worth(&[good(0.0), good(10.0)]);
        assert!((two - (4.0 + 4.0 / 11.0)).abs() < EPSILON);
    }

    #[test]
    fn cargo_worth_weighs_each_good_by_its_carry_load_and_judges_goods_apart() {
        let carry = crate::expedition_config::ExpeditionConfig::builtin()
            .carry
            .clone();
        let mut cargo = LocalStore::new();
        cargo.add_food("dry", Scalar::from_i64(4));
        cargo.add(FODDER, Scalar::from_i64(4));
        let empty = LocalStore::new();
        let expected = 4.0 * unit_load(&carry, 1, 0, 0) + 4.0 * unit_load(&carry, 0, 1, 0);
        assert!((cargo_worth(&cargo, &empty, &carry) - expected).abs() < 1e-3);
        // Food already in the larder discounts the food only; the hay is judged by its own hayloft.
        let mut fed = LocalStore::new();
        fed.add_food("dry", Scalar::from_i64(40));
        let discounted = 4.0 * unit_load(&carry, 1, 0, 0) / 11.0 + 4.0 * unit_load(&carry, 0, 1, 0);
        assert!((cargo_worth(&cargo, &fed, &carry) - discounted).abs() < 1e-3);
        assert_eq!(cargo_worth(&empty, &fed, &carry), 0.0);
    }

    #[test]
    fn a_self_pair_is_never_stored_or_credited() {
        let mut goodwill = Goodwill::default();
        goodwill.credit(A, A, GoodwillSource::Gift, 0.5);
        assert!(goodwill.is_empty());
    }

    #[test]
    fn the_total_is_the_sum_of_the_reasons_and_each_credit_lands_in_its_own() {
        let mut goodwill = Goodwill::default();
        goodwill.credit(A, B, GoodwillSource::Gift, 0.1);
        goodwill.credit(A, B, GoodwillSource::Stationed, 0.2);
        let entry = goodwill.get(A, B);
        assert!((entry.gifts - 0.1).abs() < EPSILON);
        assert!((entry.stationed - 0.2).abs() < EPSILON);
        assert!((entry.total() - 0.3).abs() < EPSILON);
        assert_eq!(goodwill.get(B, A), GoodwillEntry::default(), "ordered pair");
    }

    #[test]
    fn a_credit_that_would_cross_a_bound_lands_exactly_on_it() {
        let mut goodwill = Goodwill::default();
        goodwill.credit(A, B, GoodwillSource::Gift, 0.7);
        goodwill.credit(A, B, GoodwillSource::Exchange, 0.9);
        let entry = goodwill.get(A, B);
        assert!((entry.total() - GOODWILL_CEILING).abs() < EPSILON);
        assert!(
            (entry.gifts - 0.7).abs() < EPSILON,
            "existing components stay"
        );
        assert!(
            (entry.exchanges - 0.3).abs() < EPSILON,
            "the credit is what scaled"
        );
        goodwill.credit(A, B, GoodwillSource::Breach, -5.0);
        assert!((goodwill.get(A, B).total() - GOODWILL_FLOOR).abs() < EPSILON);
    }

    #[test]
    fn fade_is_proportional_on_every_component_so_the_sum_fades_exactly() {
        let mut goodwill = Goodwill::default();
        goodwill.credit(A, B, GoodwillSource::Gift, 0.4);
        goodwill.credit(A, B, GoodwillSource::Stationed, 0.2);
        let before = goodwill.get(A, B);
        goodwill.fade(0.1);
        let after = goodwill.get(A, B);
        assert!((after.gifts - before.gifts * 0.9).abs() < EPSILON);
        assert!((after.total() - before.total() * 0.9).abs() < EPSILON);
    }

    #[test]
    fn known_goodwill_never_moves_backwards_and_ignores_a_self_pair() {
        let mut known = KnownGoodwill::default();
        let news = |turn, gifts| GoodwillNews {
            components: GoodwillEntry {
                gifts,
                ..Default::default()
            },
            turn,
        };
        known.record(A, B, news(10, 0.3));
        known.record(A, B, news(5, 0.9));
        assert_eq!(
            known.get(A, B).expect("held").turn,
            10,
            "a stale report loses"
        );
        known.record(A, B, news(10, 0.9));
        assert!((known.get(A, B).expect("held").components.gifts - 0.3).abs() < EPSILON);
        known.record(A, B, news(11, 0.5));
        assert_eq!(known.get(A, B).expect("held").turn, 11);
        known.record(A, A, news(99, 0.5));
        assert_eq!(known.len(), 1);
    }
}
