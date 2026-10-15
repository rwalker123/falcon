//! Pending dealings — the one record every offer between peoples is held as
//! (`docs/plan_early_diplomacy.md` §1, issue #233).
//!
//! **A dealing is addressed to a people, answered by a command, and expires on silence.** This slice
//! builds the first kind, [`DealingKind::Offer`]: a trade party that arrived at another people's
//! band (or at a band of its own people whose standing answer is off) and is standing in the camp
//! with its cargo until the receiver names a return or refuses. [`PendingDealings`] is `SimState`;
//! the ids are monotonic and the counter is checkpointed with the book.
//!
//! **The goods stay on the party.** A dealing carries a manifest of what was sent ([`Dealing::
//! goods_sent`], the shipment's own [`LocalStore`] representation — food by class, hay, material
//! batches with their readings, never a summed value), but the physical cargo is still
//! [`crate::components::Expedition::cargo`], so a recall, a lost destination or a refusal
//! needs no goods-moving code of its own: the party turns `Returning` and the existing fold-back
//! carries the cargo home. A dealing whose party is no longer standing in the camp is *withdrawn*,
//! never answered.
//!
//! **The standing answer** ([`standing_answer_covers`]) is the only thing the delivery path asks
//! about the receiver: a gift from the receiver's own people, with that people's
//! "accept gifts from my own people" on (the default), lands on arrival exactly as a shipment
//! always has. Everything else becomes a dealing.
//!
//! **The answer** ([`answer_dealing`]) resolves in the turn it is submitted, and fails closed with a
//! reason before anything moves. Silence is a refusal at [`Dealing::expires_turn`]
//! ([`advance_dealings`]).

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sim_runtime::TradeCargoItem;

use crate::{
    components::{
        available_workers, BandId, Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation,
        PopulationCohort, ResidentBand, ShipmentTerms, TransferCause, TransferCounterparty,
        TransferCrossing, TransferDirection,
    },
    materials_config::MaterialsConfigHandle,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, FactionBorderPolicies, SimulationTick,
    },
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    LocalStore, FODDER, FOOD,
};

/// A pending dealing's identity — monotonic, never reused, and ordered (the book is a `BTreeMap`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct DealingId(pub u64);

/// What a dealing is. Later slices add `Proposal`, `Counter` and `TreatyProposal` here; the record
/// around the kind does not change shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealingKind {
    /// A stage-2 cargo waiting for the receiver's return: the party is standing in the camp.
    Offer,
}

impl DealingKind {
    /// The wire's token for the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            DealingKind::Offer => "offer",
        }
    }
}

/// One pending item: addressed to `receiver_faction`, expiring at `expires_turn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dealing {
    /// Assigned by [`PendingDealings::post`]; whatever a caller writes here is overwritten.
    pub id: DealingId,
    pub kind: DealingKind,
    pub sender_faction: FactionId,
    /// The band that sent it — the party's home band.
    pub sender_band: BandId,
    pub receiver_faction: FactionId,
    pub receiver_band: BandId,
    /// **The waiting party**, for the kinds that walk (an offer). `None` for the kinds a later slice
    /// adds that no party carries.
    pub party: Option<BandId>,
    /// What was sent: the shipment's own cargo representation, by class and by rating.
    pub goods_sent: LocalStore,
    /// What is asked or returned. Empty on a stage-2 offer — the receiver names the return in its
    /// answer — and carried so a counter or a proposal can name both legs.
    pub goods_asked: LocalStore,
    pub posted_turn: u64,
    /// `posted_turn + offer_wait_turns`: the first resolve at which silence refuses it.
    pub expires_turn: u64,
}

impl Dealing {
    /// How many resolves this dealing is still open for, counting the next one. The frame carries
    /// the tick the *next* turn will resolve at, so `expires_turn == now` reads **1** — "answer this
    /// turn or it is refused" — rather than a `0` that would read as already gone.
    pub fn turns_left(&self, now: u64) -> u32 {
        let left = (self.expires_turn + 1).saturating_sub(now);
        u32::try_from(left).unwrap_or(u32::MAX)
    }
}

/// **Every pending dealing, keyed by id.** `SimState`: a dealing in flight is real state — the goods
/// have already left the sender — so a rollback that dropped it would strand a party in a camp with
/// nobody waiting on it.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PendingDealings {
    /// The next id to hand out. Checkpointed with the book: a restore that rewound the dealings but
    /// not the counter would re-issue an id a client may still hold.
    next_id: u64,
    items: BTreeMap<DealingId, Dealing>,
}

/// The first id handed out. Never `0`, so a zero on the wire reads as "no dealing".
const FIRST_DEALING_ID: u64 = 1;

impl PendingDealings {
    /// Post a dealing, assigning its id.
    pub fn post(&mut self, mut dealing: Dealing) -> DealingId {
        self.next_id = self.next_id.max(FIRST_DEALING_ID);
        let id = DealingId(self.next_id);
        self.next_id += 1;
        dealing.id = id;
        self.items.insert(id, dealing);
        id
    }

    pub fn get(&self, id: DealingId) -> Option<&Dealing> {
        self.items.get(&id)
    }

    pub fn remove(&mut self, id: DealingId) -> Option<Dealing> {
        self.items.remove(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Dealing> {
        self.items.values()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// **What one viewer's frame carries**: the dealings addressed to their people and the ones
    /// their people sent. Another pair's dealings are theirs to know.
    pub fn visible_to(&self, viewer: FactionId) -> impl Iterator<Item = &Dealing> {
        self.items.values().filter(move |dealing| {
            dealing.receiver_faction == viewer || dealing.sender_faction == viewer
        })
    }
}

/// **Does the receiver's standing answer settle this shipment on arrival?** Today there is one:
/// a gift from the receiver's own people, while that people's "accept gifts from my own people" is
/// on. An offer, a gift to another people and a gift to one's own people with the answer off are
/// all pending. A world with no policy resource reads the seeded default (on).
pub fn standing_answer_covers(
    terms: ShipmentTerms,
    sender: FactionId,
    receiver: FactionId,
    policies: Option<&FactionBorderPolicies>,
) -> bool {
    terms == ShipmentTerms::Gift
        && sender == receiver
        && policies.is_none_or(|policies| policies.accepts_own_gifts(receiver))
}

/// A dealing's two ends, for the feed — copied out so an event can be written after the dealing has
/// left the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DealingEnds {
    pub sender_faction: FactionId,
    pub sender_band: BandId,
    pub receiver_faction: FactionId,
    pub receiver_band: BandId,
}

impl DealingEnds {
    pub fn of(dealing: &Dealing) -> Self {
        Self {
            sender_faction: dealing.sender_faction,
            sender_band: dealing.sender_band,
            receiver_faction: dealing.receiver_faction,
            receiver_band: dealing.receiver_band,
        }
    }
}

/// **A store read as a list of lines** — food, hay, then one line per material (summed over its
/// ratings only for the sentence; the dealing itself keeps every batch). Empty for an empty store.
pub fn manifest(store: &LocalStore) -> Vec<String> {
    let mut lines = Vec::new();
    let food = store.get(FOOD);
    if food > scalar_zero() {
        lines.push(format!("{:.2} {FOOD}", food.to_f32()));
    }
    let fodder = store.get(FODDER);
    if fodder > scalar_zero() {
        lines.push(format!("{:.2} {FODDER}", fodder.to_f32()));
    }
    for (material, _) in store.materials() {
        let total = store.material_total(material);
        if total > scalar_zero() {
            lines.push(format!("{:.2} {material}", total.to_f32()));
        }
    }
    lines
}

fn band_label(band: BandId) -> String {
    format!("band {}", band.0)
}

fn manifest_phrase(lines: &[String]) -> String {
    if lines.is_empty() {
        "nothing".to_string()
    } else {
        lines.join(", ")
    }
}

/// **The two feed lines an offer arriving writes**, one per people (the feed is per-faction on the
/// wire). The sender's names the destination and the party; the receiver's names the sender and
/// what it offers. Band ids ride as tokens so a client swaps in its own label.
pub fn push_posted_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    ends: &DealingEnds,
    manifest: &[String],
    expedition_bits: u64,
) {
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.sender_faction,
            format!(
                "Your shipment is waiting on {} to answer ({})",
                band_label(ends.receiver_band),
                manifest_phrase(manifest)
            ),
            Some(format!(
                "status=posted dealing={} kind={} destination={} expedition={}",
                id.0,
                DealingKind::Offer.as_str(),
                ends.receiver_band.0,
                expedition_bits
            )),
        )
        .with_band(ends.sender_band),
    );
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.receiver_faction,
            format!(
                "A party from {} offers {}",
                band_label(ends.sender_band),
                manifest_phrase(manifest)
            ),
            Some(format!(
                "status=offered dealing={} kind={} from={} sender={} band={}",
                id.0,
                DealingKind::Offer.as_str(),
                ends.sender_faction.0,
                ends.sender_band.0,
                ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// How a dealing ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Accepted; `returned` is the manifest of what the receiver paid.
    Accepted {
        returned: Vec<String>,
    },
    Refused,
    /// Silence at the deadline.
    Expired,
    /// The party was no longer standing in the camp (recalled, re-sent, or the destination gone).
    Withdrawn,
}

/// **The feed lines a dealing's end writes**, one per people — except a withdrawal, which only the
/// receiver needs (the sender watched its own party leave).
pub fn push_resolution_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    ends: &DealingEnds,
    resolution: &Resolution,
) {
    let (status, sender_line, receiver_line) = match resolution {
        Resolution::Accepted { returned } => (
            "accepted",
            Some(format!(
                "{} accepted your shipment and sent back {}",
                band_label(ends.receiver_band),
                manifest_phrase(returned)
            )),
            format!(
                "You accepted the shipment from {}",
                band_label(ends.sender_band)
            ),
        ),
        Resolution::Refused => (
            "refused",
            Some(format!(
                "{} refused your shipment — it is coming home",
                band_label(ends.receiver_band)
            )),
            format!(
                "You refused the shipment from {}",
                band_label(ends.sender_band)
            ),
        ),
        Resolution::Expired => (
            "expired",
            Some(format!(
                "{} did not answer — your shipment is coming home",
                band_label(ends.receiver_band)
            )),
            format!(
                "The offer from {} lapsed unanswered",
                band_label(ends.sender_band)
            ),
        ),
        Resolution::Withdrawn => (
            "withdrawn",
            None,
            format!(
                "The offer from {} was withdrawn",
                band_label(ends.sender_band)
            ),
        ),
    };
    if let Some(label) = sender_line {
        log.push(
            CommandEventEntry::new(
                tick,
                CommandEventKind::Dealing,
                ends.sender_faction,
                label,
                Some(format!(
                    "status={status} dealing={} kind={} destination={}",
                    id.0,
                    DealingKind::Offer.as_str(),
                    ends.receiver_band.0
                )),
            )
            .with_band(ends.sender_band),
        );
    }
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.receiver_faction,
            receiver_line,
            Some(format!(
                "status={status} dealing={} kind={} from={} sender={} band={}",
                id.0,
                DealingKind::Offer.as_str(),
                ends.sender_faction.0,
                ends.sender_band.0,
                ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// What a receiver answered.
#[derive(Debug, Clone, PartialEq)]
pub enum DealingAnswer {
    /// Take the cargo, and pay these goods back from the receiving band (empty is allowed — the
    /// receiver names the return).
    Accept {
        returned: Vec<TradeCargoItem>,
    },
    Refuse,
}

/// What a successful answer did, for the caller's log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerReport {
    pub id: DealingId,
    pub accepted: bool,
    /// The manifest of what the receiver paid (empty on a refusal or an empty return).
    pub returned: Vec<String>,
}

const VERB: &str = "answer_dealing";

fn resident_band_entity(world: &mut World, band: BandId) -> Option<Entity> {
    let mut query = world.query_filtered::<(Entity, &BandId), With<ResidentBand>>();
    query
        .iter(world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
}

fn party_entity(world: &mut World, party: BandId) -> Option<Entity> {
    let mut query = world.query_filtered::<(Entity, &BandId), With<Expedition>>();
    query
        .iter(world)
        .find(|(_, id)| **id == party)
        .map(|(entity, _)| entity)
}

/// The return the receiver named, summed per good and checked line by line.
struct ReturnGoods {
    food: Scalar,
    fodder: Scalar,
    /// One `(material id, amount)` per material, in the order the receiver named them.
    materials: Vec<(String, Scalar)>,
}

fn resolve_return(world: &World, returned: &[TradeCargoItem]) -> Result<ReturnGoods, String> {
    let materials_cfg = world.resource::<MaterialsConfigHandle>().get();
    let mut goods = ReturnGoods {
        food: scalar_zero(),
        fodder: scalar_zero(),
        materials: Vec::new(),
    };
    for item in returned {
        if !item.amount.is_finite() || item.amount <= 0.0 {
            return Err(format!(
                "{VERB}: '{}' must be a positive amount; got {}.",
                item.id, item.amount
            ));
        }
        let amount = scalar_from_f32(item.amount);
        if item.is_material {
            if materials_cfg.material(&item.id).is_none() {
                return Err(format!("{VERB}: unknown material '{}'.", item.id));
            }
            match goods.materials.iter_mut().find(|(id, _)| id == &item.id) {
                Some((_, held)) => *held += amount,
                None => goods.materials.push((item.id.clone(), amount)),
            }
        } else {
            match item.id.as_str() {
                FOOD => goods.food += amount,
                FODDER => goods.fodder += amount,
                _ => return Err(format!("{VERB}: unknown commodity '{}'.", item.id)),
            }
        }
    }
    Ok(goods)
}

/// **Answer a pending dealing.** Seat-gating and membership are the caller's (the command
/// dispatcher); what is checked here is that the dealing is addressed to `faction`'s people, that its
/// party is still standing in the camp, and — for an accept — that the receiving band holds the
/// return it named and the party can carry it home. **Every check runs before anything moves**, so
/// a refused answer leaves both bands and the party exactly as they stood.
///
/// An accept lands the cargo in the receiving band (the arrival hand-over,
/// [`crate::systems::land_cargo_at_band`]), draws the named return from that band into the party's cargo, books it
/// [`TransferCause::ExchangeOut`], and turns the party `Returning`; the return lands in its home band
/// on the fold-back ([`TransferCause::ExchangeIn`]). A refusal turns the party home with its own
/// cargo (`ShipmentReturned`).
pub fn answer_dealing(
    world: &mut World,
    faction: FactionId,
    id: DealingId,
    answer: &DealingAnswer,
) -> Result<AnswerReport, String> {
    let tick = world.resource::<SimulationTick>().0;
    let Some(dealing) = world
        .resource::<PendingDealings>()
        .get(id)
        .filter(|dealing| dealing.receiver_faction == faction)
        .cloned()
    else {
        return Err(format!(
            "{VERB}: no dealing {} is waiting on your people.",
            id.0
        ));
    };
    let ends = DealingEnds::of(&dealing);

    // --- the party is still standing in the camp -------------------------------------------
    let party = dealing
        .party
        .and_then(|party| party_entity(world, party))
        .filter(|&party| {
            world
                .get::<Expedition>(party)
                .is_some_and(|expedition| expedition.phase == ExpeditionPhase::AwaitingOrders)
        });
    let Some(party) = party else {
        // Nothing to answer: withdraw it, so the receiver stops being asked.
        world.resource_mut::<PendingDealings>().remove(id);
        push_resolution_events(
            &mut world.resource_mut::<CommandEventLog>(),
            tick,
            id,
            &ends,
            &Resolution::Withdrawn,
        );
        return Err(format!(
            "{VERB}: the party that made offer {} has gone.",
            id.0
        ));
    };

    let DealingAnswer::Accept { returned } = answer else {
        // --- refuse: the party turns for home with its own cargo ----------------------------
        if let Some(mut expedition) = world.get_mut::<Expedition>(party) {
            expedition.phase = ExpeditionPhase::Returning;
        }
        world.resource_mut::<PendingDealings>().remove(id);
        push_resolution_events(
            &mut world.resource_mut::<CommandEventLog>(),
            tick,
            id,
            &ends,
            &Resolution::Refused,
        );
        return Ok(AnswerReport {
            id,
            accepted: false,
            returned: Vec::new(),
        });
    };

    // --- accept: every check, before anything moves ----------------------------------------
    let Some(receiver) = resident_band_entity(world, dealing.receiver_band) else {
        return Err(format!(
            "{VERB}: band {} is not there to answer.",
            dealing.receiver_band.0
        ));
    };
    let goods = resolve_return(world, returned)?;

    let (party_workers, party_faction) = world
        .get::<PopulationCohort>(party)
        .map(|cohort| (available_workers(cohort.working), cohort.faction))
        .ok_or_else(|| format!("{VERB}: the party that made offer {} has gone.", id.0))?;
    let pack = world
        .resource::<crate::expedition_config::ExpeditionConfigHandle>()
        .get()
        .carry
        .for_faction(crate::roaming_skills::world_faction_knows(
            world,
            party_faction,
            crate::roaming_skills::TRAVOIS_DISCOVERY_ID,
        ));
    let cap = crate::carry::carry_capacity(party_workers, &pack).to_f32();
    let mass = crate::carry::CarryLoad {
        food: goods.food,
        fodder: goods.fodder,
        items: 0,
        materials: goods
            .materials
            .iter()
            .fold(scalar_zero(), |total, (_, amount)| total + *amount),
    }
    .load(&pack)
    .to_f32();
    if mass > cap {
        return Err(format!(
            "{VERB}: the party can carry {cap:.2} home; that return weighs {mass:.2}."
        ));
    }
    {
        let Some(cohort) = world.get::<PopulationCohort>(receiver) else {
            return Err(format!(
                "{VERB}: band {} has no store to pay from.",
                dealing.receiver_band.0
            ));
        };
        let store = &cohort.stores;
        if store.get(FOOD) < goods.food {
            return Err(format!(
                "{VERB}: the band holds {:.2} {FOOD}, not {:.2}.",
                store.get(FOOD).to_f32(),
                goods.food.to_f32()
            ));
        }
        if store.get(FODDER) < goods.fodder {
            return Err(format!(
                "{VERB}: the band holds {:.2} {FODDER}, not {:.2}.",
                store.get(FODDER).to_f32(),
                goods.fodder.to_f32()
            ));
        }
        for (material, amount) in &goods.materials {
            let held = store.material_total(material);
            if held < *amount {
                return Err(format!(
                    "{VERB}: the band holds {:.2} {material}, not {:.2}.",
                    held.to_f32(),
                    amount.to_f32()
                ));
            }
        }
    }

    // --- everything holds: land the cargo, draw the return, turn the party home ------------
    let sender = TransferCounterparty {
        band: dealing.sender_band,
        faction: dealing.sender_faction,
    };
    let party_band = dealing.party;
    let mut cargo = std::mem::take(
        &mut world
            .get_mut::<Expedition>(party)
            .expect("the party was resolved above")
            .cargo,
    );
    let mut return_store = LocalStore::new();
    let delivered: Vec<String>;
    // **The two worths, judged against what each side held BEFORE the transfer** — the cargo against
    // the receiving band's larder and stores, the return against the sender's home band's
    // (`goodwill::cargo_worth`).
    let carry = world
        .resource::<crate::expedition_config::ExpeditionConfigHandle>()
        .get()
        .carry
        .clone();
    let home_held = world
        .get::<Expedition>(party)
        .and_then(|expedition| world.get::<PopulationCohort>(expedition.home_band))
        .map(|home| home.stores.clone())
        .unwrap_or_default();
    let cargo_worth;
    {
        let mut query = world.query::<(&mut PopulationCohort, Option<&mut LaborAllocation>)>();
        let (mut host, mut allocation) = query
            .get_mut(world, receiver)
            .expect("the receiving band was resolved above");
        delivered = manifest(&cargo);
        cargo_worth = crate::goodwill::cargo_worth(&cargo, &host.stores, &carry);
        crate::systems::land_cargo_at_band(
            &mut cargo,
            &mut host,
            allocation.as_deref_mut(),
            Some(sender),
            party_band,
        );
        // The receiver pays: food by class, hay, then each material batch at its own rating.
        let food = host.stores.take_food_mix(goods.food);
        return_store.add_food_mix(&food);
        let fodder = host.stores.take(FODDER, goods.fodder);
        if fodder > scalar_zero() {
            return_store.add(FODDER, fodder);
        }
        let mut drawn = Vec::new();
        for (material, amount) in &goods.materials {
            for draw in host.stores.take_material_batches(material, *amount) {
                return_store.deposit_material(
                    material,
                    draw.band.clone(),
                    draw.amount,
                    &draw.characteristics,
                );
                drawn.push((material.clone(), draw));
            }
        }
        if let Some(allocation) = allocation.as_deref_mut() {
            for (commodity, amount) in [(FOOD, food.total()), (FODDER, fodder)] {
                allocation.book_crossing(
                    TransferCrossing::goods(
                        commodity,
                        TransferDirection::Out,
                        TransferCause::ExchangeOut,
                        amount.to_f32(),
                    )
                    .with_counterparty(Some(sender))
                    .with_party(party_band),
                );
            }
            allocation.book_material_draws(
                &drawn,
                TransferDirection::Out,
                TransferCause::ExchangeOut,
                Some(sender),
                party_band,
            );
        }
        host.sync_size();
    }
    let paid = manifest(&return_store);
    // **Goodwill, judged by the receiver of each leg**: a gift (a return of nothing) raises the
    // receiver's opinion of the giver; a settled exchange raises each side's opinion of the other by
    // the worth of what it received. Refusal, expiry and a failed exchange credit nothing.
    let return_worth = crate::goodwill::cargo_worth(&return_store, &home_held, &carry);
    let per_load = world
        .get_resource::<crate::diplomacy_config::DiplomacyConfigHandle>()
        .map(|handle| handle.get().goodwill.per_load)
        .unwrap_or_else(|| {
            crate::diplomacy_config::DiplomacyConfig::builtin()
                .goodwill
                .per_load
        });
    if let Some(mut goodwill) = world.get_resource_mut::<crate::goodwill::Goodwill>() {
        use crate::goodwill::GoodwillSource;
        if paid.is_empty() {
            goodwill.credit(
                ends.receiver_faction,
                ends.sender_faction,
                GoodwillSource::Gift,
                per_load * cargo_worth,
            );
        } else {
            goodwill.credit(
                ends.receiver_faction,
                ends.sender_faction,
                GoodwillSource::Exchange,
                per_load * cargo_worth,
            );
            goodwill.credit(
                ends.sender_faction,
                ends.receiver_faction,
                GoodwillSource::Exchange,
                per_load * return_worth,
            );
        }
    }
    {
        let mut expedition = world
            .get_mut::<Expedition>(party)
            .expect("the party was resolved above");
        expedition.cargo = return_store;
        expedition.phase = ExpeditionPhase::Returning;
        if let ExpeditionMission::Trade { terms, .. } = &mut expedition.mission {
            *terms = ShipmentTerms::Returned;
        }
    }
    world.resource_mut::<PendingDealings>().remove(id);
    // **The hand-over is still a delivery**: the sender is told what landed, on the same kind and
    // detail a shipment that landed on arrival writes, so a reader of deliveries sees both.
    world.resource_mut::<CommandEventLog>().push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::TradeDelivered,
            ends.sender_faction,
            format!(
                "Trade party delivered {} to {}",
                manifest_phrase(&delivered),
                band_label(ends.receiver_band)
            ),
            Some(format!(
                "status=delivered destination={} dealing={} expedition={}",
                ends.receiver_band.0,
                id.0,
                party.to_bits()
            )),
        )
        .with_band(ends.sender_band),
    );
    push_resolution_events(
        &mut world.resource_mut::<CommandEventLog>(),
        tick,
        id,
        &ends,
        &Resolution::Accepted {
            returned: paid.clone(),
        },
    );
    Ok(AnswerReport {
        id,
        accepted: true,
        returned: paid,
    })
}

/// **Silence is a refusal, and a dealing whose party has left is withdrawn** — run each turn after
/// [`crate::systems::advance_expeditions`], which posts them.
///
/// A dealing is withdrawn when its party is no longer `AwaitingOrders` in the camp: recalled, sent
/// elsewhere with `move_band`, or turned for home because its destination was gone. Otherwise, at
/// [`Dealing::expires_turn`] the party turns `Returning` with its own cargo and the receiver is told
/// the offer lapsed. Answers are commands and resolve in the turn they are submitted, so by the time
/// this runs a dealing that was answered is already out of the book.
pub fn advance_dealings(
    tick: Res<SimulationTick>,
    mut pending: ResMut<PendingDealings>,
    mut event_log: ResMut<CommandEventLog>,
    mut parties: Query<(&BandId, &mut Expedition)>,
) {
    if pending.is_empty() {
        return;
    }
    let now = tick.0;
    let due: Vec<(DealingId, DealingEnds, Option<BandId>, bool)> = pending
        .iter()
        .map(|dealing| {
            (
                dealing.id,
                DealingEnds::of(dealing),
                dealing.party,
                now >= dealing.expires_turn,
            )
        })
        .collect();
    for (id, ends, party, expired) in due {
        let waiting = party.and_then(|party| {
            parties
                .iter_mut()
                .find(|(band, expedition)| {
                    **band == party && expedition.phase == ExpeditionPhase::AwaitingOrders
                })
                .map(|(_, expedition)| expedition)
        });
        let resolution = match waiting {
            None => Resolution::Withdrawn,
            Some(_) if !expired => continue,
            Some(mut expedition) => {
                expedition.phase = ExpeditionPhase::Returning;
                Resolution::Expired
            }
        };
        pending.remove(id);
        push_resolution_events(&mut event_log, now, id, &ends, &resolution);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dealing(posted: u64, wait: u64) -> Dealing {
        Dealing {
            id: DealingId::default(),
            kind: DealingKind::Offer,
            sender_faction: FactionId(0),
            sender_band: BandId(1),
            receiver_faction: FactionId(1),
            receiver_band: BandId(2),
            party: Some(BandId(3)),
            goods_sent: LocalStore::new(),
            goods_asked: LocalStore::new(),
            posted_turn: posted,
            expires_turn: posted + wait,
        }
    }

    #[test]
    fn ids_are_monotonic_and_never_reused_after_a_removal() {
        let mut book = PendingDealings::default();
        let first = book.post(dealing(1, 1));
        let second = book.post(dealing(1, 1));
        assert!(first < second);
        assert_eq!(first, DealingId(FIRST_DEALING_ID));
        book.remove(second);
        let third = book.post(dealing(2, 1));
        assert!(third > second, "a removed id is not handed out again");
    }

    #[test]
    fn a_viewer_sees_dealings_addressed_to_or_sent_by_their_people_only() {
        let mut book = PendingDealings::default();
        book.post(dealing(1, 1));
        assert_eq!(book.visible_to(FactionId(0)).count(), 1, "the sender");
        assert_eq!(book.visible_to(FactionId(1)).count(), 1, "the receiver");
        assert_eq!(book.visible_to(FactionId(2)).count(), 0, "a third people");
    }

    #[test]
    fn turns_left_counts_the_resolve_the_next_turn_runs_at() {
        let posted = dealing(5, 1);
        assert_eq!(posted.turns_left(6), 1, "answer this turn or it is refused");
        assert_eq!(dealing(5, 3).turns_left(6), 3);
    }

    #[test]
    fn only_a_gift_to_ones_own_people_with_the_answer_on_is_covered() {
        let mut policies = FactionBorderPolicies::new(&[FactionId(0), FactionId(1)]);
        let own = |terms, policies: &FactionBorderPolicies| {
            standing_answer_covers(terms, FactionId(0), FactionId(0), Some(policies))
        };
        assert!(own(ShipmentTerms::Gift, &policies));
        assert!(
            !own(ShipmentTerms::Offer, &policies),
            "an offer is always pending"
        );
        assert!(!standing_answer_covers(
            ShipmentTerms::Gift,
            FactionId(0),
            FactionId(1),
            Some(&policies)
        ));
        policies.set_accept_own_gifts(FactionId(0), false);
        assert!(!own(ShipmentTerms::Gift, &policies), "the answer is off");
        assert!(standing_answer_covers(
            ShipmentTerms::Gift,
            FactionId(0),
            FactionId(0),
            None
        ));
    }
}
