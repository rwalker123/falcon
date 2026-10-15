//! **`Dealings`** — the seat answers what other peoples send it (`docs/plan_early_diplomacy.md` §1,
//! §4). Every pending dealing addressed to this people is answered every turn it is still open,
//! with `answer_dealing`; silence refuses it, so an AI that did not answer would leave a human's
//! party standing in its camp until the wait ran out.
//!
//! - **An offer** (a party arrived with goods and waits) is accepted, and the AI names the return:
//!   goods from its **surplus** — what its receiving band holds the most of, by load — sized so the
//!   return's load is `return_fairness × worth(cargo)`, the cargo's worth to the AI by the sim's own
//!   formula (`sim_runtime::worth`) on the AI's own holdings. The sender is assumed to hold none of
//!   what it is given back, so the return counts at its full load. Never food while the band's
//!   runway is under `food_runway_floor_turns`; never more than the party can carry home
//!   (`party_workers × carry_per_worker`) nor more than the band holds. A return nothing can fill
//!   is an empty one: the gift is still accepted.
//! - **A proposal or a counter** (the terms are written: what the shipper sends, what it asks back)
//!   is accepted when `worth_received × (1 + goodwill_toward_sender) >= worth_given`, the worth to
//!   the AI of each leg on its own holdings; otherwise refused. No AI counters yet. A leg the AI
//!   cannot cover from its band's holdings is refused rather than accepted into a failed answer.
//! - **A treaty proposal** (`docs/plan_early_diplomacy.md` §3, the terms asked for) is accepted
//!   when this people's own goodwill toward the proposer is at least `dealings.treaty_goodwill`,
//!   refused otherwise. The terms are not weighed: a people that likes you enough takes road
//!   rights, open borders and agreed trade as written. No AI counters, proposes or ends a treaty.
//!
//! **A duty, not a bid.** An answer spends no worker and claims no row, band or move, so it is a
//! standing proposal (`Proposal::standing`): paid first, before any bid is weighed, and never
//! outscored by one. The intent is `dealings:answer:<id>`; the `reason` carries the worth numbers,
//! so `decisions.jsonl` shows what the AI judged.
//!
//! **What it reads off its own frame:** the dealing rows addressed to it (`snapshot.dealings`,
//! including an offer's `party_workers`), its receiving band's holdings and runway
//! (`stores`, `fodder_store`, `material_batches`, `turns_of_food`), the pack echoes
//! (`carry_per_worker` and the two goods weights), and its own live opinion of the sender
//! (`snapshot.own_goodwill`).

use std::collections::BTreeMap;

use sim_runtime::{
    worth, CommandPayload, DealingGoodsState, DealingReply, DealingState, GoodAmount,
    PopulationCohortState, TradeCargoItem, FIXED_POINT_SCALE, FODDER_CARGO_KEY, FOOD_CARGO_KEY,
};

use super::{intent_key, Cost, Proposal, Proposals, Specialist, SpecialistId, SPECIALIST_DEALINGS};
use crate::ground::GroundReadings;
use crate::orchestrator::Plan;
use crate::profile::DealingsLevers;
use crate::view::{SeatMemory, SeatView};

/// The intent class of an answer (`dealings:answer:<id>`).
pub const INTENT_ANSWER: &str = "answer";
/// An answer is a duty and is weighed at face value; the score only orders the log.
pub const ANSWER_SCORE: f32 = 1.0;

/// The dealing kinds on the wire (`DealingState::kind`).
pub const KIND_OFFER: &str = "offer";
pub const KIND_PROPOSAL: &str = "proposal";
pub const KIND_COUNTER: &str = "counter";
pub const KIND_TREATY_PROPOSAL: &str = "treaty_proposal";

/// One unit of food is the load currency's numéraire (`carry.rs`): every other good's weight is
/// stated against it.
pub const FOOD_LOAD_WEIGHT: f32 = 1.0;
/// Amounts the AI names are rounded **down** to a whole number of these steps per unit (hundredths),
/// so a figure that crossed the wire as `f32` never exceeds what the band holds once the sim reads it
/// back as fixed-point.
const AMOUNT_STEPS_PER_UNIT: f32 = 100.0;
/// A return is filled until the load it still owes is under this — below it the rounding step above
/// is the only thing left.
const LOAD_EPSILON: f32 = 1e-4;

/// **A good, as the judgement sees it** — food and hay are two accounts, and each material id a
/// third, never summed with another.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Good {
    Food,
    Fodder,
    Material(String),
}

/// **A bundle of goods by account** — a dealing's leg or a band's holdings.
#[derive(Debug, Clone, Default, PartialEq)]
struct Bundle {
    food: f32,
    fodder: f32,
    materials: BTreeMap<String, f32>,
}

impl Bundle {
    /// A leg off the wire; each material's quality batches sum by amount within that material.
    fn of_goods(goods: &DealingGoodsState) -> Self {
        let mut bundle = Self {
            food: goods.food,
            fodder: goods.fodder,
            materials: BTreeMap::new(),
        };
        for batch in &goods.materials {
            *bundle
                .materials
                .entry(batch.material_id.clone())
                .or_default() += batch.amount;
        }
        bundle
    }

    /// What the band holds, off its own cohort row.
    fn held_by(band: &PopulationCohortState) -> Self {
        let mut bundle = Self {
            food: band
                .stores
                .iter()
                .filter(|store| store.item == FOOD_CARGO_KEY)
                .map(|store| store.quantity as f32 / FIXED_POINT_SCALE as f32)
                .sum(),
            fodder: band.fodder_store,
            materials: BTreeMap::new(),
        };
        for batch in &band.material_batches {
            *bundle
                .materials
                .entry(batch.material_id.clone())
                .or_default() += batch.amount;
        }
        bundle
    }

    fn amount(&self, good: &Good) -> f32 {
        match good {
            Good::Food => self.food,
            Good::Fodder => self.fodder,
            Good::Material(id) => self.materials.get(id).copied().unwrap_or_default(),
        }
    }

    /// Every good this bundle names, in a fixed order.
    fn goods(&self) -> Vec<Good> {
        let mut goods = vec![Good::Food, Good::Fodder];
        goods.extend(self.materials.keys().cloned().map(Good::Material));
        goods
    }

    fn is_empty(&self) -> bool {
        self.food <= 0.0 && self.fodder <= 0.0 && self.materials.values().all(|&m| m <= 0.0)
    }
}

/// **The load of one unit of each good**, in the one carry currency: food `1.0`, the other two the
/// band's own echoes of the sim's levers (`expedition_trade_fodder_carry_weight`,
/// `expedition_trade_material_carry_weight`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Weights {
    fodder: f32,
    material: f32,
}

impl Weights {
    fn of(band: &PopulationCohortState) -> Self {
        Self {
            fodder: band.expedition_trade_fodder_carry_weight,
            material: band.expedition_trade_material_carry_weight,
        }
    }

    fn of_good(&self, good: &Good) -> f32 {
        match good {
            Good::Food => FOOD_LOAD_WEIGHT,
            Good::Fodder => self.fodder,
            Good::Material(_) => self.material,
        }
    }
}

/// **`Σ load × amount / (amount + held)` over a leg's goods** — the sim's formula
/// (`sim_runtime::worth`), on the holdings of the people judging it.
fn worth_of(leg: &Bundle, held: &Bundle, weights: &Weights) -> f32 {
    let goods: Vec<GoodAmount> = leg
        .goods()
        .iter()
        .map(|good| {
            let amount = leg.amount(good);
            GoodAmount {
                load: weights.of_good(good) * amount,
                amount,
                held: held.amount(good),
            }
        })
        .collect();
    worth(&goods)
}

/// Round an amount down to a whole number of [`AMOUNT_STEPS_PER_UNIT`] steps.
fn floor_to_step(amount: f32) -> f32 {
    (amount * AMOUNT_STEPS_PER_UNIT).floor() / AMOUNT_STEPS_PER_UNIT
}

/// **The return an offer is paid back with**: goods from the band's surplus — the goods it holds the
/// most load of, first — until the return's load is `target_load` or the party's `cap_load` or the
/// band's holdings run out. Food is left out when `allow_food` is false. Empty when nothing fills it.
fn size_return(
    held: &Bundle,
    weights: &Weights,
    target_load: f32,
    cap_load: f32,
    allow_food: bool,
) -> Vec<TradeCargoItem> {
    let mut candidates: Vec<(Good, f32, f32)> = held
        .goods()
        .into_iter()
        .filter(|good| allow_food || *good != Good::Food)
        .map(|good| {
            let weight = weights.of_good(&good);
            let amount = held.amount(&good);
            (good, weight, amount)
        })
        .filter(|(_, weight, amount)| *weight > 0.0 && *amount > 0.0)
        .collect();
    // The surplus first: the most load held, then the key so the order is a fixed one.
    candidates.sort_by(|a, b| {
        (b.1 * b.2)
            .total_cmp(&(a.1 * a.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut owed = target_load.min(cap_load);
    let mut returned = Vec::new();
    for (good, weight, amount) in candidates {
        if owed <= LOAD_EPSILON {
            break;
        }
        let named = floor_to_step(amount.min(owed / weight));
        if named <= 0.0 {
            continue;
        }
        owed -= named * weight;
        returned.push(match good {
            Good::Food => TradeCargoItem {
                id: FOOD_CARGO_KEY.to_owned(),
                is_material: false,
                amount: named,
            },
            Good::Fodder => TradeCargoItem {
                id: FODDER_CARGO_KEY.to_owned(),
                is_material: false,
                amount: named,
            },
            Good::Material(id) => TradeCargoItem {
                id,
                is_material: true,
                amount: named,
            },
        });
    }
    returned
}

/// A cargo line as the log reads it: `food 1.20 material hide 0.50`.
fn cargo_text(items: &[TradeCargoItem]) -> String {
    if items.is_empty() {
        return "nothing".to_owned();
    }
    items
        .iter()
        .map(|item| {
            if item.is_material {
                format!("material {} {:.2}", item.id, item.amount)
            } else {
                format!("{} {:.2}", item.id, item.amount)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Does the band hold every account a leg names?
fn covers(held: &Bundle, leg: &Bundle) -> bool {
    leg.goods()
        .iter()
        .all(|good| held.amount(good) >= leg.amount(good))
}

pub struct Dealings {
    faction: u32,
    levers: DealingsLevers,
}

impl Dealings {
    pub fn new(faction: u32, levers: DealingsLevers) -> Self {
        Self { faction, levers }
    }

    /// This people's live opinion of `other`, `0` where it holds none (`snapshot.own_goodwill` has
    /// a row only for a people it holds an entry about).
    fn goodwill_toward(view: &SeatView, other: u32) -> f32 {
        view.snapshot
            .own_goodwill
            .iter()
            .find(|row| row.other_faction == other)
            .map_or(0.0, |row| row.total)
    }

    fn answer(&self, view: &SeatView, dealing: &DealingState) -> Option<Proposal> {
        let band = view
            .own_bands(self.faction)
            .find(|band| band.band_id == dealing.receiver_band)?;
        let held = Bundle::held_by(band);
        let weights = Weights::of(band);
        let (reply, reason) = match dealing.kind.as_str() {
            KIND_OFFER => self.answer_offer(dealing, band, &held, &weights),
            KIND_PROPOSAL | KIND_COUNTER => self.answer_terms(view, dealing, &held, &weights),
            KIND_TREATY_PROPOSAL => self.answer_treaty(view, dealing),
            _ => return None,
        };
        let commands = vec![CommandPayload::AnswerDealing {
            faction: self.faction,
            dealing_id: dealing.id,
            reply,
        }];
        Some(Proposal {
            commands,
            intent: intent_key(SPECIALIST_DEALINGS, INTENT_ANSWER, dealing.id),
            score: ANSWER_SCORE,
            cost: Cost::default(),
            reason,
            memo: None,
            standing: true,
        })
    }

    fn answer_offer(
        &self,
        dealing: &DealingState,
        band: &PopulationCohortState,
        held: &Bundle,
        weights: &Weights,
    ) -> (DealingReply, String) {
        let cargo = Bundle::of_goods(&dealing.goods_sent);
        let cargo_worth = worth_of(&cargo, held, weights);
        let target = self.levers.return_fairness * cargo_worth;
        let cap = dealing.party_workers as f32 * band.carry_per_worker;
        let allow_food = band.turns_of_food >= self.levers.food_runway_floor_turns;
        let returned = size_return(held, weights, target, cap, allow_food);
        let load: f32 = returned
            .iter()
            .map(|item| {
                let good = if item.is_material {
                    Good::Material(item.id.clone())
                } else if item.id == FODDER_CARGO_KEY {
                    Good::Fodder
                } else {
                    Good::Food
                };
                weights.of_good(&good) * item.amount
            })
            .sum();
        let reason = format!(
            "offer {}: cargo worth {:.3} to us; fairness {:.2} -> return load {:.3} of {:.3} \
             (party cap {:.2}, runway {:.1}t {} floor {:.1}t): return {}",
            dealing.id,
            cargo_worth,
            self.levers.return_fairness,
            load,
            target,
            cap,
            band.turns_of_food,
            if allow_food { ">=" } else { "<" },
            self.levers.food_runway_floor_turns,
            cargo_text(&returned),
        );
        (DealingReply::Accept { returned }, reason)
    }

    /// **A treaty proposal is taken when the AI's own goodwill toward the proposer reaches
    /// `dealings.treaty_goodwill`**, and refused otherwise.
    fn answer_treaty(&self, view: &SeatView, dealing: &DealingState) -> (DealingReply, String) {
        let goodwill = Self::goodwill_toward(view, dealing.sender_faction);
        let accept = goodwill >= self.levers.treaty_goodwill;
        let reason = format!(
            "treaty proposal {}: goodwill toward faction {} {:+.3} {} threshold {:+.3}: {}",
            dealing.id,
            dealing.sender_faction,
            goodwill,
            if accept { ">=" } else { "<" },
            self.levers.treaty_goodwill,
            if accept { "accept" } else { "refuse" },
        );
        let reply = if accept {
            DealingReply::Accept {
                returned: Vec::new(),
            }
        } else {
            DealingReply::Refuse
        };
        (reply, reason)
    }

    fn answer_terms(
        &self,
        view: &SeatView,
        dealing: &DealingState,
        held: &Bundle,
        weights: &Weights,
    ) -> (DealingReply, String) {
        // The legs are stated from the shipper's point of view. This people is the shipper only on a
        // counter of a proposal it made; otherwise it is the consignee: it receives what the shipper
        // sends and gives what the shipper asks back.
        let shipper_is_us = dealing.shipper_band == dealing.receiver_band;
        let (received, given) = if shipper_is_us {
            (&dealing.goods_asked, &dealing.goods_sent)
        } else {
            (&dealing.goods_sent, &dealing.goods_asked)
        };
        let received = Bundle::of_goods(received);
        let given = Bundle::of_goods(given);
        let goodwill = Self::goodwill_toward(view, dealing.sender_faction);
        let worth_received = worth_of(&received, held, weights);
        let worth_given = worth_of(&given, held, weights);
        let weighed = worth_received * (1.0 + goodwill);
        let covered = covers(held, &given);
        let accept =
            covered && weighed >= worth_given && !(received.is_empty() && given.is_empty());
        let reason = format!(
            "{} {}: worth received {:.3} x (1 + goodwill {:+.3}) = {:.3} {} worth given {:.3}{}: {}",
            dealing.kind,
            dealing.id,
            worth_received,
            goodwill,
            weighed,
            if weighed >= worth_given { ">=" } else { "<" },
            worth_given,
            if covered { "" } else { " (cannot cover the ask)" },
            if accept { "accept" } else { "refuse" },
        );
        let reply = if accept {
            DealingReply::Accept {
                returned: Vec::new(),
            }
        } else {
            DealingReply::Refuse
        };
        (reply, reason)
    }
}

impl Specialist for Dealings {
    fn id(&self) -> SpecialistId {
        SPECIALIST_DEALINGS
    }

    fn propose(
        &mut self,
        view: &SeatView,
        _plan: &Plan,
        _memory: &SeatMemory,
        _ground: &GroundReadings,
    ) -> Proposals {
        let proposals = view
            .snapshot
            .dealings
            .iter()
            .filter(|dealing| dealing.receiver_faction == self.faction)
            .filter_map(|dealing| self.answer(view, dealing))
            .collect();
        Proposals {
            proposals,
            ..Proposals::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::{CohortStoreState, GoodwillState, MaterialBatchState, WorldSnapshot};

    const FACTION: u32 = 2;
    const SENDER: u32 = 1;
    const AI_BAND: u64 = 9001;
    const SENDER_BAND: u64 = 7001;
    const FODDER_WEIGHT: f32 = 0.5;
    const MATERIAL_WEIGHT: f32 = 1.0;
    const PER_WORKER_CARRY: f32 = 7.0;

    fn levers() -> DealingsLevers {
        DealingsLevers {
            return_fairness: 1.0,
            food_runway_floor_turns: 10.0,
            treaty_goodwill: 0.2,
        }
    }

    fn materials(rows: &[(&str, f32)]) -> Vec<MaterialBatchState> {
        rows.iter()
            .map(|(id, amount)| MaterialBatchState {
                material_id: (*id).to_owned(),
                amount: *amount,
                ..Default::default()
            })
            .collect()
    }

    /// The AI's receiving band: holds `food` provisions, `fodder` hay and the given materials.
    fn ai_band(food: f32, fodder: f32, held: &[(&str, f32)], runway: f32) -> PopulationCohortState {
        PopulationCohortState {
            faction: FACTION,
            band_id: AI_BAND,
            stores: vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (food * FIXED_POINT_SCALE as f32) as i64,
            }],
            fodder_store: fodder,
            material_batches: materials(held),
            turns_of_food: runway,
            carry_per_worker: PER_WORKER_CARRY,
            expedition_trade_fodder_carry_weight: FODDER_WEIGHT,
            expedition_trade_material_carry_weight: MATERIAL_WEIGHT,
            ..Default::default()
        }
    }

    fn goods(food: f32, fodder: f32, rows: &[(&str, f32)]) -> DealingGoodsState {
        DealingGoodsState {
            food,
            fodder,
            materials: materials(rows),
        }
    }

    fn an_offer(id: u64, cargo: DealingGoodsState, party_workers: u32) -> DealingState {
        DealingState {
            id,
            kind: KIND_OFFER.to_owned(),
            sender_faction: SENDER,
            sender_band: SENDER_BAND,
            receiver_faction: FACTION,
            receiver_band: AI_BAND,
            goods_sent: cargo,
            goods_asked: DealingGoodsState::default(),
            turns_left: 3,
            shipper_band: 0,
            party_workers,
            ..Default::default()
        }
    }

    fn a_proposal(id: u64, sends: DealingGoodsState, asks: DealingGoodsState) -> DealingState {
        DealingState {
            id,
            kind: KIND_PROPOSAL.to_owned(),
            sender_faction: SENDER,
            sender_band: SENDER_BAND,
            receiver_faction: FACTION,
            receiver_band: AI_BAND,
            goods_sent: sends,
            goods_asked: asks,
            turns_left: 3,
            shipper_band: SENDER_BAND,
            party_workers: 2,
            ..Default::default()
        }
    }

    fn a_treaty_proposal(id: u64) -> DealingState {
        DealingState {
            id,
            kind: KIND_TREATY_PROPOSAL.to_owned(),
            sender_faction: SENDER,
            sender_band: SENDER_BAND,
            receiver_faction: FACTION,
            receiver_band: AI_BAND,
            turns_left: 3,
            treaty: sim_runtime::TreatyTermsState {
                road_rights: true,
                ..Default::default()
            },
            carrier: "road".to_owned(),
            ..Default::default()
        }
    }

    /// A treaty proposal is accepted when the AI's goodwill toward the proposer reaches the lever
    /// (`treaty_goodwill`, 0.2 here) and refused below it; the edge is inclusive.
    #[test]
    fn a_treaty_proposal_is_accepted_at_the_goodwill_threshold_and_refused_below_it() {
        let reply_for = |goodwill: f32| {
            let band = ai_band(40.0, 0.0, &[], 30.0);
            let proposals = propose(&view(band, vec![a_treaty_proposal(6)], goodwill));
            assert_eq!(proposals.len(), 1);
            assert_eq!(proposals[0].intent, "dealings:answer:6");
            assert!(proposals[0].standing);
            reply_of(&proposals[0]).clone()
        };
        assert_eq!(
            reply_for(0.5),
            DealingReply::Accept {
                returned: Vec::new()
            }
        );
        assert_eq!(
            reply_for(0.2),
            DealingReply::Accept {
                returned: Vec::new()
            },
            "the threshold itself accepts"
        );
        assert_eq!(reply_for(0.19), DealingReply::Refuse);
        assert_eq!(reply_for(-0.4), DealingReply::Refuse);
        // A people it holds no opinion of reads 0, which is under the opening threshold.
        assert_eq!(reply_for(0.0), DealingReply::Refuse);
    }

    fn view(band: PopulationCohortState, dealings: Vec<DealingState>, goodwill: f32) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.populations.push(band);
        snapshot.dealings = dealings;
        if goodwill != 0.0 {
            snapshot.own_goodwill.push(GoodwillState {
                other_faction: SENDER,
                total: goodwill,
                ..Default::default()
            });
        }
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    fn propose(view: &SeatView) -> Vec<Proposal> {
        Dealings::new(FACTION, levers())
            .propose(
                view,
                &Plan::pass_through(0),
                &SeatMemory::default(),
                &GroundReadings::new(),
            )
            .proposals
    }

    fn reply_of(proposal: &Proposal) -> &DealingReply {
        match &proposal.commands[..] {
            [CommandPayload::AnswerDealing { reply, .. }] => reply,
            other => panic!("one answer_dealing expected, got {other:?}"),
        }
    }

    fn returned(proposal: &Proposal) -> &[TradeCargoItem] {
        match reply_of(proposal) {
            DealingReply::Accept { returned } => returned,
            other => panic!("an accept expected, got {other:?}"),
        }
    }

    fn load_of(items: &[TradeCargoItem]) -> f32 {
        items
            .iter()
            .map(|item| match item.id.as_str() {
                FOOD_CARGO_KEY => item.amount,
                FODDER_CARGO_KEY => item.amount * FODDER_WEIGHT,
                _ => item.amount * MATERIAL_WEIGHT,
            })
            .sum()
    }

    /// An offer is accepted, and the return's load is the fairness times the cargo's worth: a gift
    /// of 3 hide to a band holding none is worth its full load, and the return pays it back in the
    /// good the band holds the most of.
    #[test]
    fn an_offer_is_accepted_and_paid_back_in_load_from_the_biggest_holding() {
        let band = ai_band(40.0, 0.0, &[("bone", 20.0)], 30.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 3.0)]), 3);
        let proposals = propose(&view(band, vec![offer], 0.0));
        assert_eq!(proposals.len(), 1);
        let proposal = &proposals[0];
        assert_eq!(proposal.intent, "dealings:answer:5");
        assert!(proposal.standing, "an answer is a duty, paid first");
        assert_eq!(proposal.cost, Cost::default());
        let items = returned(proposal);
        // Food (40) is the biggest holding, so the whole return is food, at the cargo's full load.
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].id, FOOD_CARGO_KEY);
        assert!(!items[0].is_material);
        assert!((load_of(items) - 3.0).abs() < 0.011, "{items:?}");
        assert!(
            proposal.reason.contains("cargo worth 3.000"),
            "{}",
            proposal.reason
        );
    }

    /// The return is sized from what the cargo is worth to the AI on its own holdings: a good it
    /// already holds ten times over counts for about a tenth.
    #[test]
    fn a_good_the_ai_already_holds_a_lot_of_is_worth_less() {
        let band = ai_band(40.0, 0.0, &[("hide", 30.0)], 30.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 3.0)]), 3);
        let proposals = propose(&view(band, vec![offer], 0.0));
        let items = returned(&proposals[0]);
        // 1.0 * 3 / (3 + 30) of 3 load = 0.2727... of load.
        let expected = 3.0 * 3.0 / 33.0;
        assert!((load_of(items) - expected).abs() < 0.011, "{items:?}");
    }

    /// The surplus is chosen by the load held, not the count: 30 hay at half weight is 15 load and
    /// loses to 20 bone at full weight.
    #[test]
    fn the_surplus_is_the_good_it_holds_the_most_load_of() {
        let held = Bundle {
            food: 0.0,
            fodder: 30.0,
            materials: BTreeMap::from([("bone".to_owned(), 20.0)]),
        };
        let weights = Weights {
            fodder: FODDER_WEIGHT,
            material: MATERIAL_WEIGHT,
        };
        let items = size_return(&held, &weights, 2.0, 100.0, true);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "bone");
        assert!(items[0].is_material);
        assert!((items[0].amount - 2.0).abs() < 0.011);
    }

    /// When the largest holding cannot cover the load, the next fills the rest.
    #[test]
    fn a_return_the_first_good_cannot_fill_continues_into_the_next() {
        let held = Bundle {
            food: 0.0,
            fodder: 0.0,
            materials: BTreeMap::from([("bone".to_owned(), 1.5), ("fibre".to_owned(), 1.0)]),
        };
        let weights = Weights {
            fodder: FODDER_WEIGHT,
            material: MATERIAL_WEIGHT,
        };
        let items = size_return(&held, &weights, 2.0, 100.0, true);
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["bone", "fibre"]);
        assert!((items[0].amount - 1.5).abs() < 0.011);
        assert!((items[1].amount - 0.5).abs() < 0.011);
    }

    /// A band whose runway is under the floor names no food, however much it holds.
    #[test]
    fn a_band_under_the_food_floor_returns_no_food() {
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 3.0)]), 3);
        let short = ai_band(40.0, 0.0, &[("bone", 2.0)], 9.0);
        let items_short = {
            let proposals = propose(&view(short, vec![offer.clone()], 0.0));
            returned(&proposals[0]).to_vec()
        };
        assert!(
            items_short.iter().all(|item| item.id != FOOD_CARGO_KEY),
            "no food while the runway is under the floor: {items_short:?}"
        );
        assert_eq!(items_short[0].id, "bone", "it pays in what it can spare");

        // At the floor exactly food is allowed again: the lever is a floor, not a ceiling.
        let fed = ai_band(40.0, 0.0, &[("bone", 2.0)], 10.0);
        let proposals = propose(&view(fed, vec![offer], 0.0));
        assert_eq!(returned(&proposals[0])[0].id, FOOD_CARGO_KEY);
    }

    /// The return never exceeds what the party can carry home, nor what the band holds.
    #[test]
    fn the_return_is_bounded_by_the_party_carry_and_the_holdings() {
        // A big gift, a 1-worker party carrying 7: the return stops at the pack.
        let band = ai_band(500.0, 0.0, &[], 50.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 40.0)]), 1);
        let proposals = propose(&view(band, vec![offer], 0.0));
        let load = load_of(returned(&proposals[0]));
        assert!((load - PER_WORKER_CARRY).abs() < 0.011, "{load}");

        // The same gift to a band holding only 2 food: the return stops at the holding.
        let poor = ai_band(2.0, 0.0, &[], 50.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 40.0)]), 6);
        let proposals = propose(&view(poor, vec![offer], 0.0));
        let items = returned(&proposals[0]);
        assert_eq!(items.len(), 1);
        assert!((items[0].amount - 2.0).abs() < 0.011);
    }

    /// Nothing to name is an accepted gift with an empty return, not a refusal; so is a party the
    /// frame says has no hands.
    #[test]
    fn a_gift_the_band_cannot_repay_is_accepted_with_an_empty_return() {
        let empty = ai_band(0.0, 0.0, &[], 50.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 3.0)]), 3);
        let proposals = propose(&view(empty, vec![offer.clone()], 0.0));
        assert!(returned(&proposals[0]).is_empty());

        let rich = ai_band(40.0, 0.0, &[], 50.0);
        let no_hands = an_offer(6, goods(0.0, 0.0, &[("hide", 3.0)]), 0);
        let proposals = propose(&view(rich, vec![no_hands], 0.0));
        assert!(
            returned(&proposals[0]).is_empty(),
            "a party of none carries nothing home"
        );
    }

    /// The fairness lever scales the return and zero keeps everything.
    #[test]
    fn the_fairness_lever_scales_the_return() {
        let band = ai_band(40.0, 0.0, &[], 50.0);
        let offer = an_offer(5, goods(0.0, 0.0, &[("hide", 4.0)]), 3);
        let view = view(band, vec![offer], 0.0);
        let mut half = Dealings::new(
            FACTION,
            DealingsLevers {
                return_fairness: 0.5,
                ..levers()
            },
        );
        let proposals = half
            .propose(
                &view,
                &Plan::pass_through(0),
                &SeatMemory::default(),
                &GroundReadings::new(),
            )
            .proposals;
        assert!((load_of(returned(&proposals[0])) - 2.0).abs() < 0.011);
        let mut none = Dealings::new(
            FACTION,
            DealingsLevers {
                return_fairness: 0.0,
                ..levers()
            },
        );
        let proposals = none
            .propose(
                &view,
                &Plan::pass_through(0),
                &SeatMemory::default(),
                &GroundReadings::new(),
            )
            .proposals;
        assert!(returned(&proposals[0]).is_empty());
    }

    /// A proposal is accepted when what it receives, weighted by its goodwill toward the sender,
    /// is worth at least what it gives — and refused below it. Goodwill is the lever that tips it.
    #[test]
    fn a_proposal_is_accepted_at_the_goodwill_weighted_threshold() {
        // The AI holds 10 food and nothing else. The shipper sends 2 hide (worth 2 to a band with
        // none) and asks for 10 food: given worth = 1 * 10 / (10 + 10) * 10 = 5.
        let band = ai_band(10.0, 0.0, &[], 50.0);
        let proposal = a_proposal(8, goods(0.0, 0.0, &[("hide", 2.0)]), goods(10.0, 0.0, &[]));
        let neutral = propose(&view(band.clone(), vec![proposal.clone()], 0.0));
        assert_eq!(*reply_of(&neutral[0]), DealingReply::Refuse);

        // 2 * (1 + 1.0) = 4 < 5: even full goodwill is not enough.
        let adored = propose(&view(band.clone(), vec![proposal.clone()], 1.0));
        assert_eq!(*reply_of(&adored[0]), DealingReply::Refuse);

        // A sweeter deal: ask for 4 food (given worth 1 * 4 / 14 * 4 = 1.142857...), received 2.
        let sweeter = a_proposal(9, goods(0.0, 0.0, &[("hide", 2.0)]), goods(4.0, 0.0, &[]));
        let accepted = propose(&view(band.clone(), vec![sweeter], 0.0));
        assert_eq!(
            *reply_of(&accepted[0]),
            DealingReply::Accept {
                returned: Vec::new()
            }
        );

        // The threshold itself: received 2, given worth 2.5 refuses at no goodwill and accepts at
        // +0.25 (2 * 1.25 = 2.5, equal accepts) and refuses at -0.25.
        let held = Bundle::held_by(&band);
        let weights = Weights::of(&band);
        let ask = goods(5.0, 0.0, &[]);
        let given_worth = worth_of(&Bundle::of_goods(&ask), &held, &weights);
        let received_worth = worth_of(
            &Bundle::of_goods(&goods(0.0, 0.0, &[("hide", given_worth)])),
            &held,
            &weights,
        );
        assert!((received_worth - given_worth).abs() < 1e-4);
        let at_edge = a_proposal(10, goods(0.0, 0.0, &[("hide", given_worth)]), ask);
        let liked = propose(&view(band.clone(), vec![at_edge.clone()], 0.1));
        assert_eq!(
            *reply_of(&liked[0]),
            DealingReply::Accept {
                returned: Vec::new()
            }
        );
        let disliked = propose(&view(band, vec![at_edge], -0.1));
        assert_eq!(*reply_of(&disliked[0]), DealingReply::Refuse);
    }

    /// An ask the band cannot cover is refused, whatever it is worth.
    #[test]
    fn a_proposal_asking_more_than_the_band_holds_is_refused() {
        let band = ai_band(3.0, 0.0, &[], 50.0);
        let proposal = a_proposal(8, goods(0.0, 0.0, &[("hide", 40.0)]), goods(5.0, 0.0, &[]));
        let proposals = propose(&view(band, vec![proposal], 1.0));
        assert_eq!(*reply_of(&proposals[0]), DealingReply::Refuse);
        assert!(
            proposals[0].reason.contains("cannot cover"),
            "{}",
            proposals[0].reason
        );
    }

    /// The legs are the shipper's point of view: on a counter of the AI's own proposal (the AI is
    /// the shipper), what it receives is what the counter asks back.
    #[test]
    fn on_a_counter_of_its_own_proposal_the_legs_read_the_other_way() {
        let band = ai_band(10.0, 0.0, &[("bone", 5.0)], 50.0);
        let mut counter = a_proposal(11, goods(2.0, 0.0, &[]), goods(0.0, 0.0, &[("hide", 3.0)]));
        counter.kind = KIND_COUNTER.to_owned();
        counter.shipper_band = AI_BAND;
        let proposals = propose(&view(band, vec![counter], 0.0));
        // Received = the ask (3 hide, worth 3), given = the send (2 food, worth 1 * 2 / 12 * 2).
        assert_eq!(
            *reply_of(&proposals[0]),
            DealingReply::Accept {
                returned: Vec::new()
            }
        );
        assert!(
            proposals[0].reason.contains("counter 11"),
            "{}",
            proposals[0].reason
        );
    }

    /// Only dealings addressed to this people are answered; the ones it sent are another's to answer,
    /// and a receiving band the frame does not show is skipped rather than guessed at.
    #[test]
    fn only_dealings_addressed_to_this_people_are_answered() {
        let band = ai_band(40.0, 0.0, &[], 50.0);
        let mut ours_sent = an_offer(1, goods(1.0, 0.0, &[]), 1);
        ours_sent.sender_faction = FACTION;
        ours_sent.receiver_faction = SENDER;
        let mut elsewhere = an_offer(2, goods(1.0, 0.0, &[]), 1);
        elsewhere.receiver_band = AI_BAND + 1;
        let addressed = an_offer(3, goods(1.0, 0.0, &[]), 1);
        let proposals = propose(&view(band, vec![ours_sent, elsewhere, addressed], 0.0));
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].intent, "dealings:answer:3");
    }
}
