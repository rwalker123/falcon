//! Pending dealings — the one record every offer between peoples is held as
//! (`docs/plan_early_diplomacy.md` §1, issue #233).
//!
//! **A dealing is addressed to a people, answered by a command, and expires on silence.** Three kinds
//! share the record ([`DealingKind`]):
//!
//! - **`Offer`** — a trade party (or a scout handing over its pack) that arrived at another people's
//!   band and is standing in the camp with its cargo until the receiver names a return or refuses.
//! - **`Proposal`** — stage 4: a band proposes both legs of a trade (what it sends, what it asks
//!   back) over a **standing link**, with no party walking yet.
//! - **`Counter`** — the answer that changes the legs: the original closes and a new dealing goes the
//!   other way, its legs restated from the original sender's point of view so "accept" always means
//!   the same thing.
//!
//! [`PendingDealings`] is `SimState`; the ids are monotonic and the counter is checkpointed with the
//! book.
//!
//! **The goods stay on the party** (an offer). A dealing carries a manifest of what was sent
//! ([`Dealing::goods_sent`], the shipment's own [`LocalStore`] representation — food by class, hay,
//! material batches with their readings, never a summed value), but the physical cargo is still
//! [`crate::components::Expedition::cargo`], so a recall, a lost destination or a refusal needs no
//! goods-moving code of its own: the party turns `Returning` and the existing fold-back carries the
//! cargo home. A dealing whose party is no longer standing in the camp is *withdrawn*, never
//! answered. A proposal or counter carries its legs as [`GoodsLines`] — unrated lines, because no
//! goods have been drawn yet — in [`Dealing::agreement`].
//!
//! **The standing answer** ([`standing_answer_covers`]) is the only thing the delivery path asks
//! about the receiver: a gift from the receiver's own people, with that people's
//! "accept gifts from my own people" on (the default), lands on arrival exactly as a shipment
//! always has. Everything else becomes a dealing.
//!
//! **The answer** ([`answer_dealing`]) resolves in the turn it is submitted, and fails closed with a
//! reason before anything moves. Silence is a refusal at [`Dealing::expires_turn`]
//! ([`advance_dealings`]). **Every answer on a proposal or a counter requires the standing link
//! between the two bands to still hold** ([`bands_have_standing_link`]); a dealing whose road has
//! gone is withdrawn.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sim_runtime::TradeCargoItem;

use crate::treaties::{EnvoyOutcome, Treaties, TreatyTerms};
use crate::{
    carry::{carry_capacity, CarryConfig, CarryLoad},
    components::{
        available_workers, BandId, Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation,
        PopulationCohort, ResidentBand, ShipmentTerms, Tile, TransferCause, TransferCounterparty,
        TransferCrossing, TransferDirection,
    },
    connections::{ConnectionKey, ConnectionLedger, NO_TIE},
    materials_config::MaterialsConfigHandle,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, FactionBorderPolicies,
        SimulationConfig, SimulationTick, TileRegistry,
    },
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    LocalStore, FODDER, FOOD,
};

/// A pending dealing's identity — monotonic, never reused, and ordered (the book is a `BTreeMap`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct DealingId(pub u64);

/// What a dealing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealingKind {
    /// A cargo waiting for the receiver's return: the party is standing in the camp.
    Offer,
    /// **Stage 4:** both legs named, over a standing link, before any party walks.
    Proposal,
    /// The answer that changes the legs: a proposal in the other direction.
    Counter,
    /// **A treaty proposal** (`docs/plan_early_diplomacy.md` §3): the full terms asked for, carried
    /// by a standing link or by an envoy ([`TreatyProposal`]). Accepting puts them in force for both
    /// peoples, replacing any treaty already standing.
    TreatyProposal,
}

impl DealingKind {
    /// The wire's token for the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            DealingKind::Offer => "offer",
            DealingKind::Proposal => "proposal",
            DealingKind::Counter => "counter",
            DealingKind::TreatyProposal => "treaty_proposal",
        }
    }

    /// Is this a treaty proposal?
    pub fn is_treaty(self) -> bool {
        matches!(self, DealingKind::TreatyProposal)
    }

    /// Does this kind carry an [`Agreement`] (both legs named, no party yet)?
    pub fn is_agreement(self) -> bool {
        matches!(self, DealingKind::Proposal | DealingKind::Counter)
    }
}

/// **A list of goods by line** — food, hay, and one total per material id — before anything has
/// been drawn. The shape a command names a leg in (`give` / `ask`) and the shape an agreed trade's
/// legs are held in: no ratings, because a rating belongs to a batch that exists, and these goods
/// are only promised. The party draws real batches at launch; the receiver pays real batches at
/// settlement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoodsLines {
    pub food: Scalar,
    pub fodder: Scalar,
    /// One `(material id, amount)` per material, in the order they were named.
    pub materials: Vec<(String, Scalar)>,
}

impl GoodsLines {
    /// Resolve a command's lines: each a positive finite amount of a known commodity or material,
    /// summed per good. `verb` prefixes the refusal so every verb reads as itself.
    pub fn resolve(world: &World, items: &[TradeCargoItem], verb: &str) -> Result<Self, String> {
        let materials_cfg = world.resource::<MaterialsConfigHandle>().get();
        let mut goods = GoodsLines::default();
        for item in items {
            if !item.amount.is_finite() || item.amount <= 0.0 {
                return Err(format!(
                    "{verb}: '{}' must be a positive amount; got {}.",
                    item.id, item.amount
                ));
            }
            let amount = scalar_from_f32(item.amount);
            if item.is_material {
                if materials_cfg.material(&item.id).is_none() {
                    return Err(format!("{verb}: unknown material '{}'.", item.id));
                }
                match goods.materials.iter_mut().find(|(id, _)| id == &item.id) {
                    Some((_, held)) => *held += amount,
                    None => goods.materials.push((item.id.clone(), amount)),
                }
            } else {
                // **The commodity key is checked, not assumed**: `sim_runtime`'s cargo keys restate
                // this crate's `FOOD` and `FODDER`, and this match is what makes the duplication safe
                // on both keys — a drift refuses the line rather than loading the wrong good.
                match item.id.as_str() {
                    FOOD => goods.food += amount,
                    FODDER => goods.fodder += amount,
                    _ => return Err(format!("{verb}: unknown commodity '{}'.", item.id)),
                }
            }
        }
        Ok(goods)
    }

    /// **A store read as lines**: food, hay and one total per material id (its ratings summed, for
    /// matching only — the store itself keeps every batch).
    pub fn from_store(store: &LocalStore) -> Self {
        Self {
            food: store.get(FOOD),
            fodder: store.get(FODDER),
            materials: store
                .materials()
                .map(|(material, _)| (material.to_string(), store.material_total(material)))
                .collect(),
        }
    }

    /// No line carries anything.
    pub fn is_empty(&self) -> bool {
        self.food <= scalar_zero()
            && self.fodder <= scalar_zero()
            && self
                .materials
                .iter()
                .all(|(_, amount)| *amount <= scalar_zero())
    }

    /// The lines as sentences, for a feed line — food, hay, then each material.
    pub fn manifest(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.food > scalar_zero() {
            lines.push(format!("{:.2} {FOOD}", self.food.to_f32()));
        }
        if self.fodder > scalar_zero() {
            lines.push(format!("{:.2} {FODDER}", self.fodder.to_f32()));
        }
        for (material, amount) in &self.materials {
            if *amount > scalar_zero() {
                lines.push(format!("{:.2} {material}", amount.to_f32()));
            }
        }
        lines
    }

    /// The load these lines weigh in the one carry currency (`carry.rs`), items none.
    pub fn mass(&self, pack: &CarryConfig) -> f32 {
        CarryLoad {
            food: self.food,
            fodder: self.fodder,
            items: 0,
            materials: self
                .materials
                .iter()
                .fold(scalar_zero(), |total, (_, amount)| total + *amount),
        }
        .load(pack)
        .to_f32()
    }

    /// Does `store` hold every line? The refusal names the first good it is short of.
    pub fn held_in(&self, store: &LocalStore, verb: &str) -> Result<(), String> {
        if store.get(FOOD) < self.food {
            return Err(format!(
                "{verb}: the band holds {:.2} {FOOD}, not {:.2}.",
                store.get(FOOD).to_f32(),
                self.food.to_f32()
            ));
        }
        // The hay account answers for itself: a band with a full larder and an empty hayloft can no
        // more give a bale than one with neither.
        if store.get(FODDER) < self.fodder {
            return Err(format!(
                "{verb}: the band holds {:.2} {FODDER}, not {:.2}.",
                store.get(FODDER).to_f32(),
                self.fodder.to_f32()
            ));
        }
        for (material, amount) in &self.materials {
            let held = store.material_total(material);
            if held < *amount {
                return Err(format!(
                    "{verb}: the band holds {:.2} {material}, not {:.2}.",
                    held.to_f32(),
                    amount.to_f32()
                ));
            }
        }
        Ok(())
    }
}

/// **The two legs of an agreed trade and who is on each end.** `shipper` is the band whose party
/// will walk — the original proposer, whichever side of a counter is speaking — and `consignee` the
/// band it walks to. `give` is what the shipper sends and `ask` what it gets back, **always stated
/// from the shipper's point of view**, so "accept" means the same thing on a proposal and on every
/// counter after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agreement {
    pub shipper_faction: FactionId,
    pub shipper_band: BandId,
    pub consignee_faction: FactionId,
    pub consignee_band: BandId,
    /// How many workers the shipper's party carries. Fixed by the proposal; a counter inherits it.
    pub party_workers: u32,
    pub give: GoodsLines,
    pub ask: GoodsLines,
}

/// **How a treaty proposal reaches the people it is addressed to.** `Road`: over a standing link,
/// posted the turn it is made, and answerable only while the link still stands. `Envoy`: a party
/// stands in the receiver's camp (or, on a counter the envoy carried home, nothing is needed): the
/// proposer learns the answer when the envoy walks home.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreatyCarrier {
    Road,
    Envoy,
}

impl TreatyCarrier {
    /// The wire's token.
    pub fn as_str(self) -> &'static str {
        match self {
            TreatyCarrier::Road => "road",
            TreatyCarrier::Envoy => "envoy",
        }
    }
}

/// **A treaty proposal's payload**: the full terms asked for (a proposal replaces whatever stood —
/// amending is proposing again) and who carries it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreatyProposal {
    pub terms: TreatyTerms,
    pub carrier: TreatyCarrier,
}

/// One pending item: addressed to `receiver_faction`, expiring at `expires_turn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dealing {
    /// Assigned by [`PendingDealings::post`]; whatever a caller writes here is overwritten.
    pub id: DealingId,
    pub kind: DealingKind,
    /// **Who posted it.** For an offer, the party's home band; for a proposal, the band that will
    /// send the party; for a counter, the band that countered (the consignee, or — on a counter of
    /// a counter — the shipper again).
    pub sender_faction: FactionId,
    pub sender_band: BandId,
    /// **Who must answer it.**
    pub receiver_faction: FactionId,
    pub receiver_band: BandId,
    /// **The waiting party**, for an offer. `None` for a proposal and a counter, which no party
    /// carries.
    pub party: Option<BandId>,
    /// **How many workers the waiting party has**, for an offer: what bounds the return it can walk
    /// home (`party_workers × per-worker carry`), published so the answering people can size a
    /// return that fits. `0` for a proposal and a counter, whose `agreement` carries its own.
    pub party_workers: u32,
    /// What was sent (an offer): the shipment's own cargo representation, by class and by rating.
    pub goods_sent: LocalStore,
    /// What is asked or returned (an offer). Empty — the receiver names the return in its answer.
    pub goods_asked: LocalStore,
    /// **The named legs**, for a proposal or a counter; `None` for an offer.
    pub agreement: Option<Agreement>,
    /// **The terms and the carrier**, for a treaty proposal; `None` for every other kind.
    pub treaty: Option<TreatyProposal>,
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
/// of an offer have already left the sender — so a rollback that dropped it would strand a party in
/// a camp with nobody waiting on it.
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
/// on. An offer, a gift to another people, a gift to one's own people with the answer off and an
/// agreed trade (which settles by its own terms) are all outside it. A world with no policy
/// resource reads the seeded default (on).
pub fn standing_answer_covers(
    terms: &ShipmentTerms,
    sender: FactionId,
    receiver: FactionId,
    policies: Option<&FactionBorderPolicies>,
) -> bool {
    *terms == ShipmentTerms::Gift
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

/// **The two feed lines a proposal or a counter being posted writes**, one per people. The poster's
/// is told it is waiting; the addressee's is told what is on the table, in the shipper's terms
/// (what it sends, what it asks back).
pub fn push_agreement_posted_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    kind: DealingKind,
    ends: &DealingEnds,
    agreement: &Agreement,
) {
    let give = manifest_phrase(&agreement.give.manifest());
    let ask = manifest_phrase(&agreement.ask.manifest());
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.sender_faction,
            format!(
                "Your {} to {} waits for an answer (sending {give}, asking {ask})",
                kind.as_str(),
                band_label(ends.receiver_band)
            ),
            Some(format!(
                "status=posted dealing={} kind={} destination={}",
                id.0,
                kind.as_str(),
                ends.receiver_band.0
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
                "{} proposes: {} sends {give} and asks {ask}",
                band_label(ends.sender_band),
                band_label(agreement.shipper_band)
            ),
            Some(format!(
                "status=offered dealing={} kind={} from={} sender={} band={}",
                id.0,
                kind.as_str(),
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
    /// Accepted; `returned` is the manifest of what the receiver paid (an offer — an agreed trade's
    /// return is settled on arrival).
    Accepted {
        returned: Vec<String>,
    },
    Refused,
    /// Silence at the deadline.
    Expired,
    /// The party was no longer standing in the camp (recalled, re-sent, or the destination gone).
    Withdrawn,
    /// **Answered with a counter**: this dealing closed and a new one went the other way.
    Countered,
    /// **The road between the two bands is gone**, so the proposal can no longer be answered.
    LinkBroken,
    /// **Accepted, and the shipper's party could not set out.** Nothing moved.
    LaunchFailed(String),
}

/// Detail suffix naming the reason a resolution carries one.
fn reason_token(resolution: &Resolution) -> &'static str {
    match resolution {
        Resolution::LinkBroken => " reason=link_broken",
        Resolution::LaunchFailed(_) => " reason=launch_failed",
        _ => "",
    }
}

/// **The feed lines a dealing's end writes**, one per people — except a withdrawal of an offer,
/// which only the receiver needs (the sender watched its own party leave).
pub fn push_resolution_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    kind: DealingKind,
    ends: &DealingEnds,
    resolution: &Resolution,
) {
    let noun = kind.as_str();
    let (status, sender_line, receiver_line) = match (kind, resolution) {
        (DealingKind::Offer, Resolution::Accepted { returned }) => (
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
        (DealingKind::Offer, Resolution::Refused) => (
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
        (DealingKind::Offer, Resolution::Expired) => (
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
        (DealingKind::Offer, Resolution::Withdrawn) => (
            "withdrawn",
            None,
            format!(
                "The offer from {} was withdrawn",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::Accepted { .. }) => (
            "accepted",
            Some(format!(
                "{} accepted your {noun} — the party sets out",
                band_label(ends.receiver_band)
            )),
            format!(
                "You accepted the {noun} from {}",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::Refused) => (
            "refused",
            Some(format!(
                "{} refused your {noun}",
                band_label(ends.receiver_band)
            )),
            format!(
                "You refused the {noun} from {}",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::Expired) => (
            "expired",
            Some(format!(
                "{} did not answer your {noun}",
                band_label(ends.receiver_band)
            )),
            format!(
                "The {noun} from {} lapsed unanswered",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::Withdrawn) => (
            "withdrawn",
            None,
            format!(
                "The {noun} from {} was withdrawn",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::Countered) => (
            "countered",
            Some(format!(
                "{} answered your {noun} with a counter",
                band_label(ends.receiver_band)
            )),
            format!(
                "You countered the {noun} from {}",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::LinkBroken) => (
            "withdrawn",
            Some(format!(
                "The road to {} is broken — your {noun} was withdrawn",
                band_label(ends.receiver_band)
            )),
            format!(
                "The road to {} is broken — the {noun} from them was withdrawn",
                band_label(ends.sender_band)
            ),
        ),
        (_, Resolution::LaunchFailed(reason)) => (
            "failed",
            Some(format!(
                "The {noun} was accepted but could not go ahead: {reason}"
            )),
            format!("The {noun} was accepted but could not go ahead: {reason}"),
        ),
    };
    let reason = reason_token(resolution);
    if let Some(label) = sender_line {
        log.push(
            CommandEventEntry::new(
                tick,
                CommandEventKind::Dealing,
                ends.sender_faction,
                label,
                Some(format!(
                    "status={status} dealing={} kind={noun} destination={}{reason}",
                    id.0, ends.receiver_band.0
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
                "status={status} dealing={} kind={noun} from={} sender={} band={}{reason}",
                id.0, ends.sender_faction.0, ends.sender_band.0, ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// What a receiver answered.
#[derive(Debug, Clone, PartialEq)]
pub enum DealingAnswer {
    /// Take the cargo, and pay these goods back from the receiving band (empty is allowed — the
    /// receiver names the return). **On a proposal or a counter the terms are already written, so
    /// the list must be empty** — to change them, counter.
    Accept {
        returned: Vec<TradeCargoItem>,
    },
    Refuse,
    /// **Answer a proposal or a counter with different legs**, stated from the shipper's point of
    /// view: `give` is what the original sender would send, `ask` what it gets back.
    Counter {
        give: Vec<TradeCargoItem>,
        ask: Vec<TradeCargoItem>,
    },
    /// **Answer a treaty proposal with different terms** — the full terms asked for.
    CounterTerms {
        terms: sim_runtime::TreatyTermsRequest,
    },
}

/// What a successful answer did, for the caller's log line.
#[derive(Debug, Clone, PartialEq)]
pub struct AnswerReport {
    pub id: DealingId,
    /// What kind of dealing was answered.
    pub kind: DealingKind,
    pub accepted: bool,
    /// The manifest of what the receiver paid (empty on a refusal or an empty return).
    pub returned: Vec<String>,
    /// **The party to send**, when an accepted proposal or counter leaves the shipper's party to
    /// launch with the agreed cargo and `ShipmentTerms::Agreed`. The launch lives in the server
    /// binary; it reads this.
    pub launch: Option<Agreement>,
    /// The new dealing a counter posted.
    pub counter: Option<DealingId>,
    /// Why an accepted proposal could not go ahead (the shipper could not honour its leg). The
    /// dealing is closed and nothing moved.
    pub failed: Option<String>,
}

impl AnswerReport {
    fn bare(id: DealingId, kind: DealingKind, accepted: bool) -> Self {
        Self {
            id,
            kind,
            accepted,
            returned: Vec::new(),
            launch: None,
            counter: None,
            failed: None,
        }
    }
}

const VERB: &str = "answer_dealing";
const PROPOSE_VERB: &str = "propose_trade";

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

/// `offers.offer_wait_turns` — the installed config's, else the builtin's.
fn offer_wait_turns(world: &World) -> u32 {
    match world.get_resource::<crate::diplomacy_config::DiplomacyConfigHandle>() {
        Some(handle) => handle.get().offers.offer_wait_turns,
        None => {
            crate::diplomacy_config::DiplomacyConfig::builtin()
                .offers
                .offer_wait_turns
        }
    }
}

/// `goodwill.per_load` — the installed config's, else the builtin's.
fn per_load(world: &World) -> f32 {
    world
        .get_resource::<crate::diplomacy_config::DiplomacyConfigHandle>()
        .map(|handle| handle.get().goodwill.per_load)
        .unwrap_or_else(|| {
            crate::diplomacy_config::DiplomacyConfig::builtin()
                .goodwill
                .per_load
        })
}

/// **The pack a people carries in** — travois lifts it (`band-carry.md`).
fn pack_of(world: &World, faction: FactionId) -> CarryConfig {
    world
        .resource::<crate::expedition_config::ExpeditionConfigHandle>()
        .get()
        .carry
        .for_faction(crate::roaming_skills::world_faction_knows(
            world,
            faction,
            crate::roaming_skills::TRAVOIS_DISCOVERY_ID,
        ))
}

// ------------------------------------------------------------------------------------------------
// The shipment gates — shared by the trade verb, a proposal, and an agreed launch
// ------------------------------------------------------------------------------------------------

/// **Where a shipment is bound**, resolved and gated — the destination band, its people, where it
/// stands, and where the sender's people believe it is (the party's `aim`).
#[derive(Debug, Clone, Copy)]
pub struct ShipmentGate {
    pub destination_band: BandId,
    /// The destination's people — fixed onto the shipment's `ShipmentOut` crossings.
    pub destination_faction: FactionId,
    pub destination_pos: UVec2,
    pub aim: UVec2,
}

/// **The destination is a real, resident band, and the home band holds a live tie to it.** Faction is
/// never asked: faction is a property of the endpoint (`connections.md`), so a destination in
/// another people works by construction. A parked tie refuses exactly as a missing one does —
/// *"at zero, nothing flows"* (`docs/plan_contact_and_logistics.md` §Q6).
pub fn check_destination(
    world: &mut World,
    faction: FactionId,
    home_entity: Entity,
    destination_band_id: u64,
    verb: &str,
) -> Result<ShipmentGate, String> {
    let wanted = BandId(destination_band_id);
    let destination = {
        let mut query = world.query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
        query
            .iter(world)
            .find(|(id, _)| **id == wanted)
            .map(|(id, cohort)| (*id, cohort.current_tile, cohort.faction))
    };
    let Some((destination_band, destination_tile, destination_faction)) = destination else {
        return Err(format!(
            "{verb}: band {destination_band_id} is not a band to deliver to."
        ));
    };
    let Some(destination_pos) = world.get::<Tile>(destination_tile).map(|t| t.position) else {
        return Err(format!(
            "{verb}: band {destination_band_id} is not standing anywhere."
        ));
    };
    let Some(&home_band) = world.get::<BandId>(home_entity) else {
        return Err(format!(
            "{verb}: the sending band has no id to trade under."
        ));
    };
    let tie = world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(home_band, destination_band))
        .map(|connection| connection.strength)
        .unwrap_or(NO_TIE);
    if tie <= NO_TIE {
        return Err(format!(
            "{verb}: no dealings with band {destination_band_id} — meet them before you ship to \
             them."
        ));
    }
    let aim = world.resource::<ConnectionLedger>().launch_aim(
        world.resource::<crate::visibility::VisibilityLedger>(),
        faction,
        home_band,
        destination_band,
        destination_faction,
        destination_pos,
    );
    Ok(ShipmentGate {
        destination_band,
        destination_faction,
        destination_pos,
        aim,
    })
}

/// **The goods fit the party's pack, and the home band holds them.** Both are checked before
/// anything is drawn, so a refused order leaves the band exactly as it stood.
pub fn check_cargo(
    world: &World,
    faction: FactionId,
    home_entity: Entity,
    party_workers: u32,
    goods: &GoodsLines,
    verb: &str,
) -> Result<(), String> {
    let pack = pack_of(world, faction);
    let cap = carry_capacity(party_workers, &pack).to_f32();
    let mass = goods.mass(&pack);
    if mass > cap {
        return Err(format!(
            "{verb}: {party_workers} workers can carry {cap:.2}; this shipment weighs {mass:.2}."
        ));
    }
    let Some(store) = world
        .get::<PopulationCohort>(home_entity)
        .map(|cohort| &cohort.stores)
    else {
        return Err(format!(
            "{verb}: the sending band has no store to load from."
        ));
    };
    goods.held_in(store, verb)
}

// ------------------------------------------------------------------------------------------------
// The standing link
// ------------------------------------------------------------------------------------------------

/// **The geometry [`crate::routes::standing_link`] needs from the world**, or `None` where a world
/// has no roads, tiles or topology (a hand-rolled fixture) — in which case nothing is linked.
struct LinkWorld {
    width: u32,
    height: u32,
    wrap: bool,
    floor: crate::intensification::RungKey,
}

fn link_world(world: &World) -> Option<LinkWorld> {
    let tiles = world.get_resource::<TileRegistry>()?;
    let config = world.get_resource::<SimulationConfig>()?;
    world.get_resource::<crate::routes::RoadRegistry>()?;
    let floor = match world.get_resource::<crate::diplomacy_config::DiplomacyConfigHandle>() {
        Some(handle) => handle.get().offers.standing_link_floor(),
        None => crate::diplomacy_config::DiplomacyConfig::builtin()
            .offers
            .standing_link_floor(),
    };
    Some(LinkWorld {
        width: tiles.width,
        height: tiles.height,
        wrap: config.map_topology.wrap_horizontal,
        floor,
    })
}

/// **Does a standing link hold between these two resident bands right now?** The one predicate
/// ([`crate::routes::standing_link`]) read off the live world: every tile of the path between the
/// two camps carries a kept road at or above `offers.standing_link_min_rung`. False when either band
/// is gone or the world has no roads.
pub fn bands_have_standing_link(world: &mut World, a: BandId, b: BandId) -> bool {
    let Some(geometry) = link_world(world) else {
        return false;
    };
    let mut query = world.query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
    let mut tile_of = |wanted: BandId| {
        query
            .iter(world)
            .find(|(id, _)| **id == wanted)
            .map(|(_, cohort)| cohort.current_tile)
    };
    let (Some(a_tile), Some(b_tile)) = (tile_of(a), tile_of(b)) else {
        return false;
    };
    let (Some(a_pos), Some(b_pos)) = (
        world.get::<Tile>(a_tile).map(|tile| tile.position),
        world.get::<Tile>(b_tile).map(|tile| tile.position),
    ) else {
        return false;
    };
    crate::routes::standing_link(
        world.resource::<crate::routes::RoadRegistry>(),
        a_pos,
        b_pos,
        geometry.width,
        geometry.height,
        geometry.wrap,
        geometry.floor,
    )
}

// ------------------------------------------------------------------------------------------------
// Settlement — shared by an accepted offer and an agreed trade arriving
// ------------------------------------------------------------------------------------------------

/// What a settlement did.
pub struct Settled {
    /// The cargo's manifest, as it landed.
    pub delivered: Vec<String>,
    /// The manifest of what the receiver paid.
    pub paid: Vec<String>,
    /// The receiver's payment, drawn batch by batch — the party's cargo from here on.
    pub return_store: LocalStore,
    /// The cargo's worth to the receiver, judged against what it held BEFORE the landing.
    pub cargo_worth: f32,
}

/// **THE exchange — the cargo lands in the receiving band and the named return is drawn out of it**,
/// one routine for an accepted offer and for an agreed trade arriving, so the two cannot book an
/// exchange differently. The cargo lands through [`crate::systems::land_cargo_at_band`] (booked
/// `ShipmentIn`); the return is drawn food by class, hay, and each material batch at its own rating,
/// booked `ExchangeOut` on the receiver. The caller has already checked the receiver holds every
/// line. `host` is not left with a stale size.
pub fn settle_exchange(
    host: &mut PopulationCohort,
    mut allocation: Option<&mut LaborAllocation>,
    cargo: &mut LocalStore,
    returned: &GoodsLines,
    sender: TransferCounterparty,
    party_band: Option<BandId>,
    carry: &CarryConfig,
) -> Settled {
    let delivered = manifest(cargo);
    // The cargo's worth is judged against what the receiving band held BEFORE the transfer
    // (`goodwill::cargo_worth`).
    let cargo_worth = crate::goodwill::cargo_worth(cargo, &host.stores, carry);
    crate::systems::land_cargo_at_band(
        cargo,
        host,
        allocation.as_deref_mut(),
        Some(sender),
        party_band,
    );
    let mut return_store = LocalStore::new();
    // The receiver pays: food by class, hay, then each material batch at its own rating.
    let food = host.stores.take_food_mix(returned.food);
    return_store.add_food_mix(&food);
    let fodder = host.stores.take(FODDER, returned.fodder);
    if fodder > scalar_zero() {
        return_store.add(FODDER, fodder);
    }
    let mut drawn = Vec::new();
    for (material, amount) in &returned.materials {
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
    if let Some(allocation) = allocation {
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
    let paid = manifest(&return_store);
    Settled {
        delivered,
        paid,
        return_store,
        cargo_worth,
    }
}

/// **Goodwill, judged by the receiver of each leg**: a gift (a return of nothing) raises the
/// receiver's opinion of the giver; a settled exchange raises each side's opinion of the other by
/// the worth of what it received. Refusal, expiry and a failed exchange credit nothing.
pub fn credit_settlement_goodwill(
    goodwill: &mut crate::goodwill::Goodwill,
    per_load: f32,
    receiver: FactionId,
    sender: FactionId,
    gift: bool,
    cargo_worth: f32,
    return_worth: f32,
) {
    use crate::goodwill::GoodwillSource;
    if gift {
        goodwill.credit(
            receiver,
            sender,
            GoodwillSource::Gift,
            per_load * cargo_worth,
        );
    } else {
        goodwill.credit(
            receiver,
            sender,
            GoodwillSource::Exchange,
            per_load * cargo_worth,
        );
        goodwill.credit(
            sender,
            receiver,
            GoodwillSource::Exchange,
            per_load * return_worth,
        );
    }
}

/// **The feed lines an agreed trade's arrival writes**, one per people: the settled exchange, or the
/// failed one. A sender is told what landed on the same kind and detail a shipment that landed on
/// arrival writes ([`CommandEventKind::TradeDelivered`]).
#[allow(clippy::too_many_arguments)] // two ends, a kind of outcome, and both manifests
pub fn push_exchange_events(
    log: &mut CommandEventLog,
    tick: u64,
    dealing: DealingId,
    ends: &DealingEnds,
    outcome: ExchangeOutcome<'_>,
    expedition_bits: u64,
) {
    let (status, sender_line, receiver_line, reason) = match outcome {
        ExchangeOutcome::Settled { delivered, paid } => {
            log.push(
                CommandEventEntry::new(
                    tick,
                    CommandEventKind::TradeDelivered,
                    ends.sender_faction,
                    format!(
                        "Trade party delivered {} to {}",
                        manifest_phrase(delivered),
                        band_label(ends.receiver_band)
                    ),
                    Some(format!(
                        "status=delivered destination={} dealing={} expedition={}",
                        ends.receiver_band.0, dealing.0, expedition_bits
                    )),
                )
                .with_band(ends.sender_band),
            );
            (
                "settled",
                format!(
                    "{} paid {} for the shipment",
                    band_label(ends.receiver_band),
                    manifest_phrase(paid)
                ),
                format!(
                    "{} delivered {} and was paid {}",
                    band_label(ends.sender_band),
                    manifest_phrase(delivered),
                    manifest_phrase(paid)
                ),
                "",
            )
        }
        ExchangeOutcome::ReceiverShort => (
            "failed",
            format!(
                "{} no longer holds the agreed return — your party is coming home with its cargo",
                band_label(ends.receiver_band)
            ),
            format!(
                "You no longer hold the goods agreed for the shipment from {} — it is going home",
                band_label(ends.sender_band)
            ),
            " reason=receiver_short",
        ),
    };
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.sender_faction,
            sender_line,
            Some(format!(
                "status={status} dealing={} kind=exchange destination={}{reason}",
                dealing.0, ends.receiver_band.0
            )),
        )
        .with_band(ends.sender_band),
    );
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Dealing,
            ends.receiver_faction,
            receiver_line,
            Some(format!(
                "status={status} dealing={} kind=exchange from={} sender={} band={}{reason}",
                dealing.0, ends.sender_faction.0, ends.sender_band.0, ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// How an agreed trade's arrival went.
pub enum ExchangeOutcome<'a> {
    Settled {
        delivered: &'a [String],
        paid: &'a [String],
    },
    /// The receiver no longer holds the return it agreed to pay.
    ReceiverShort,
}

// ------------------------------------------------------------------------------------------------
// The answer
// ------------------------------------------------------------------------------------------------

/// **Answer a pending dealing.** Seat-gating and membership are the caller's (the command
/// dispatcher); what is checked here is that the dealing is addressed to `faction`'s people, that its
/// party is still standing in the camp (an offer) or the standing link still holds (a proposal or
/// counter), and — for an accept — that the answering side holds what it promised. **Every check
/// runs before anything moves**, so a refused answer leaves both bands and the party exactly as
/// they stood.
///
/// An accepted **offer** lands the cargo in the receiving band (the arrival hand-over,
/// [`crate::systems::land_cargo_at_band`]), draws the named return from that band into the party's
/// cargo, books it [`TransferCause::ExchangeOut`], and turns the party `Returning` (a scout picks
/// its scouting back up); the return lands in its home band on the fold-back
/// ([`TransferCause::ExchangeIn`]). A refusal turns the party home with its own cargo
/// (`ShipmentReturned`).
///
/// An accepted **proposal or counter** moves nothing here: it returns the [`Agreement`] for the
/// server to launch the shipper's party with, because the launch lives in the server binary.
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
    if dealing.kind.is_agreement() {
        return answer_agreement(world, dealing, answer, tick);
    }
    if dealing.kind.is_treaty() {
        return answer_treaty(world, dealing, answer, tick);
    }
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
            dealing.kind,
            &ends,
            &Resolution::Withdrawn,
        );
        return Err(format!(
            "{VERB}: the party that made offer {} has gone.",
            id.0
        ));
    };

    let returned = match answer {
        DealingAnswer::Accept { returned } => returned,
        DealingAnswer::Counter { .. } | DealingAnswer::CounterTerms { .. } => {
            return Err(format!(
                "{VERB}: only a proposal or a counter can be countered; offer {} waits on an \
                 answer of accept or refuse.",
                id.0
            ));
        }
        DealingAnswer::Refuse => {
            // --- refuse: the party turns for home with its own cargo (a scout resumes) --------
            if let Some(mut expedition) = world.get_mut::<Expedition>(party) {
                expedition.after_dealing();
            }
            world.resource_mut::<PendingDealings>().remove(id);
            push_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                dealing.kind,
                &ends,
                &Resolution::Refused,
            );
            return Ok(AnswerReport::bare(id, dealing.kind, false));
        }
    };

    // --- accept: every check, before anything moves ----------------------------------------
    let Some(receiver) = resident_band_entity(world, dealing.receiver_band) else {
        return Err(format!(
            "{VERB}: band {} is not there to answer.",
            dealing.receiver_band.0
        ));
    };
    let goods = GoodsLines::resolve(world, returned, VERB)?;

    let (party_workers, party_faction) = world
        .get::<PopulationCohort>(party)
        .map(|cohort| (available_workers(cohort.working), cohort.faction))
        .ok_or_else(|| format!("{VERB}: the party that made offer {} has gone.", id.0))?;
    let pack = pack_of(world, party_faction);
    let cap = carry_capacity(party_workers, &pack).to_f32();
    let mass = goods.mass(&pack);
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
        goods.held_in(&cohort.stores, VERB)?;
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
    let carry = world
        .resource::<crate::expedition_config::ExpeditionConfigHandle>()
        .get()
        .carry
        .clone();
    // The return's worth is judged against what the SENDER's home band held before the exchange.
    let home_held = world
        .get::<Expedition>(party)
        .and_then(|expedition| world.get::<PopulationCohort>(expedition.home_band))
        .map(|home| home.stores.clone())
        .unwrap_or_default();
    let settled = {
        let mut query = world.query::<(&mut PopulationCohort, Option<&mut LaborAllocation>)>();
        let (mut host, mut allocation) = query
            .get_mut(world, receiver)
            .expect("the receiving band was resolved above");
        settle_exchange(
            &mut host,
            allocation.as_deref_mut(),
            &mut cargo,
            &goods,
            sender,
            party_band,
            &carry,
        )
    };
    let return_worth = crate::goodwill::cargo_worth(&settled.return_store, &home_held, &carry);
    let per_load = per_load(world);
    if let Some(mut goodwill) = world.get_resource_mut::<crate::goodwill::Goodwill>() {
        credit_settlement_goodwill(
            &mut goodwill,
            per_load,
            ends.receiver_faction,
            ends.sender_faction,
            settled.paid.is_empty(),
            settled.cargo_worth,
            return_worth,
        );
    }
    {
        let mut expedition = world
            .get_mut::<Expedition>(party)
            .expect("the party was resolved above");
        expedition.cargo = settled.return_store;
        expedition.after_dealing();
        if let ExpeditionMission::Trade { terms, .. } = &mut expedition.mission {
            *terms = ShipmentTerms::Returned;
        } else {
            expedition.scout_pack.returned = true;
            expedition.scout_pack.handover = Some(TransferCounterparty {
                band: dealing.receiver_band,
                faction: dealing.receiver_faction,
            });
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
                manifest_phrase(&settled.delivered),
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
        dealing.kind,
        &ends,
        &Resolution::Accepted {
            returned: settled.paid.clone(),
        },
    );
    Ok(AnswerReport {
        returned: settled.paid,
        ..AnswerReport::bare(id, dealing.kind, true)
    })
}

/// **Answer a proposal or a counter.** The standing link between the two bands must still hold or
/// the dealing is withdrawn (`link_broken`, told to both). A refusal closes it; a counter closes it
/// and posts a new one the other way; an accept returns the [`Agreement`] for the server to launch.
fn answer_agreement(
    world: &mut World,
    dealing: Dealing,
    answer: &DealingAnswer,
    tick: u64,
) -> Result<AnswerReport, String> {
    let id = dealing.id;
    let ends = DealingEnds::of(&dealing);
    let Some(agreement) = dealing.agreement.clone() else {
        world.resource_mut::<PendingDealings>().remove(id);
        return Err(format!("{VERB}: dealing {} carries no terms.", id.0));
    };
    // --- the road still stands ------------------------------------------------------------
    if !bands_have_standing_link(world, agreement.shipper_band, agreement.consignee_band) {
        world.resource_mut::<PendingDealings>().remove(id);
        push_resolution_events(
            &mut world.resource_mut::<CommandEventLog>(),
            tick,
            id,
            dealing.kind,
            &ends,
            &Resolution::LinkBroken,
        );
        return Err(format!(
            "{VERB}: the road between band {} and band {} no longer holds — the {} was withdrawn.",
            agreement.shipper_band.0,
            agreement.consignee_band.0,
            dealing.kind.as_str()
        ));
    }
    // Which end of the trade the answering band is.
    let answerer_is_shipper = dealing.receiver_band == agreement.shipper_band;

    match answer {
        DealingAnswer::CounterTerms { .. } => Err(format!(
            "{VERB}: {} {} is a trade; counter it with 'give … ask …'.",
            dealing.kind.as_str(),
            id.0
        )),
        DealingAnswer::Refuse => {
            world.resource_mut::<PendingDealings>().remove(id);
            push_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                dealing.kind,
                &ends,
                &Resolution::Refused,
            );
            Ok(AnswerReport::bare(id, dealing.kind, false))
        }
        DealingAnswer::Counter { give, ask } => {
            let give = GoodsLines::resolve(world, give, VERB)?;
            let ask = GoodsLines::resolve(world, ask, VERB)?;
            if give.is_empty() && ask.is_empty() {
                return Err(format!(
                    "{VERB}: a counter that sends nothing and asks nothing is not a trade."
                ));
            }
            // The legs fit the shipper's party, and the answering side holds its own leg.
            let pack = pack_of(world, agreement.shipper_faction);
            let cap = carry_capacity(agreement.party_workers, &pack).to_f32();
            let mass = give.mass(&pack);
            if mass > cap {
                return Err(format!(
                    "{VERB}: {} workers can carry {cap:.2}; that give weighs {mass:.2}.",
                    agreement.party_workers
                ));
            }
            check_own_leg(world, &agreement, answerer_is_shipper, &give, &ask)?;
            let counter = Dealing {
                id: DealingId::default(),
                kind: DealingKind::Counter,
                // The answering band speaks; the other end must answer.
                sender_faction: dealing.receiver_faction,
                sender_band: dealing.receiver_band,
                receiver_faction: dealing.sender_faction,
                receiver_band: dealing.sender_band,
                party: None,
                party_workers: 0,
                goods_sent: LocalStore::new(),
                goods_asked: LocalStore::new(),
                agreement: Some(Agreement {
                    give,
                    ask,
                    ..agreement
                }),
                treaty: None,
                posted_turn: tick,
                expires_turn: tick + u64::from(offer_wait_turns(world)),
            };
            let counter_ends = DealingEnds::of(&counter);
            let counter_terms = counter
                .agreement
                .clone()
                .expect("the counter carries its terms");
            world.resource_mut::<PendingDealings>().remove(id);
            push_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                dealing.kind,
                &ends,
                &Resolution::Countered,
            );
            let new_id = world.resource_mut::<PendingDealings>().post(counter);
            push_agreement_posted_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                new_id,
                DealingKind::Counter,
                &counter_ends,
                &counter_terms,
            );
            Ok(AnswerReport {
                counter: Some(new_id),
                ..AnswerReport::bare(id, dealing.kind, false)
            })
        }
        DealingAnswer::Accept { returned } => {
            if !returned.is_empty() {
                return Err(format!(
                    "{VERB}: the terms of {} {} are already written; accept it as it stands, or \
                     counter.",
                    dealing.kind.as_str(),
                    id.0
                ));
            }
            check_own_leg(
                world,
                &agreement,
                answerer_is_shipper,
                &agreement.give,
                &agreement.ask,
            )?;
            if !answerer_is_shipper {
                // The shipper must be able to honour its leg NOW; if it cannot, the dealing fails
                // with a reason and nothing moves.
                if let Err(reason) = check_shipper(world, &agreement) {
                    world.resource_mut::<PendingDealings>().remove(id);
                    push_resolution_events(
                        &mut world.resource_mut::<CommandEventLog>(),
                        tick,
                        id,
                        dealing.kind,
                        &ends,
                        &Resolution::LaunchFailed(reason.clone()),
                    );
                    return Ok(AnswerReport {
                        failed: Some(reason),
                        ..AnswerReport::bare(id, dealing.kind, false)
                    });
                }
            }
            world.resource_mut::<PendingDealings>().remove(id);
            push_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                dealing.kind,
                &ends,
                &Resolution::Accepted {
                    returned: Vec::new(),
                },
            );
            Ok(AnswerReport {
                launch: Some(agreement),
                ..AnswerReport::bare(id, dealing.kind, true)
            })
        }
    }
}

/// **The answering band holds the leg it would be bound to**: the shipper must be able to launch
/// (the destination, the tie, the party, the goods), the consignee must hold the return it would
/// pay. Checked on the answering side only — the other end's holdings are its own business, found
/// out at launch (the shipper) or at arrival (the consignee).
fn check_own_leg(
    world: &mut World,
    agreement: &Agreement,
    answerer_is_shipper: bool,
    give: &GoodsLines,
    ask: &GoodsLines,
) -> Result<(), String> {
    if answerer_is_shipper {
        let mut shaped = agreement.clone();
        shaped.give = give.clone();
        check_shipper(world, &shaped)
    } else {
        let Some(consignee) = resident_band_entity(world, agreement.consignee_band) else {
            return Err(format!(
                "{VERB}: band {} is not there to answer.",
                agreement.consignee_band.0
            ));
        };
        let Some(cohort) = world.get::<PopulationCohort>(consignee) else {
            return Err(format!(
                "{VERB}: band {} has no store to pay from.",
                agreement.consignee_band.0
            ));
        };
        ask.held_in(&cohort.stores, VERB)
    }
}

/// **Can the shipper's party set out on these terms right now?** The same gates the trade verb
/// runs: the band is a resident band of the shipper's people with the workers, the destination is a
/// band it holds a live tie to, and the goods fit the pack and the larder.
fn check_shipper(world: &mut World, agreement: &Agreement) -> Result<(), String> {
    let Some(home) = resident_band_entity(world, agreement.shipper_band) else {
        return Err(format!(
            "band {} is no longer there to send the party",
            agreement.shipper_band.0
        ));
    };
    let workers = world
        .get::<PopulationCohort>(home)
        .map(|cohort| available_workers(cohort.working))
        .unwrap_or(0);
    if agreement.party_workers < 1 || agreement.party_workers > workers {
        return Err(format!(
            "band {} can field {workers} workers, not {}",
            agreement.shipper_band.0, agreement.party_workers
        ));
    }
    check_destination(
        world,
        agreement.shipper_faction,
        home,
        agreement.consignee_band.0,
        VERB,
    )?;
    check_cargo(
        world,
        agreement.shipper_faction,
        home,
        agreement.party_workers,
        &agreement.give,
        VERB,
    )
}

/// **Propose a trade over a standing link** (`propose_trade`): name both legs before any party walks.
/// Posts a [`DealingKind::Proposal`] from `home_band` to `destination`, addressed to the
/// destination's people, **the turn it is made** — word travels along a kept road. Fails closed
/// before anything is recorded: the band is the proposer's resident band with the workers, the
/// destination is a band it holds a live tie to (the same tie gate a shipment launch asks), a
/// **standing link** holds between the two, the sender holds the give and it fits the party's pack,
/// and the trade names something. **Nothing is drawn.**
pub fn propose_trade(
    world: &mut World,
    faction: FactionId,
    home_band: BandId,
    party_workers: u32,
    destination: BandId,
    give: &[TradeCargoItem],
    ask: &[TradeCargoItem],
) -> Result<DealingId, String> {
    let tick = world.resource::<SimulationTick>().0;
    let Some(home) = resident_band_entity(world, home_band) else {
        return Err(format!(
            "{PROPOSE_VERB}: band {} is not a band.",
            home_band.0
        ));
    };
    let workers = match world.get::<PopulationCohort>(home) {
        Some(cohort) if cohort.faction == faction => available_workers(cohort.working),
        _ => {
            return Err(format!(
                "{PROPOSE_VERB}: band {} is not one of your people's bands.",
                home_band.0
            ));
        }
    };
    if party_workers < 1 || party_workers > workers {
        return Err(format!(
            "{PROPOSE_VERB}: band {} can field 1..{workers} workers, not {party_workers}.",
            home_band.0
        ));
    }
    let give = GoodsLines::resolve(world, give, PROPOSE_VERB)?;
    let ask = GoodsLines::resolve(world, ask, PROPOSE_VERB)?;
    if give.is_empty() && ask.is_empty() {
        return Err(format!(
            "{PROPOSE_VERB}: a trade that sends nothing and asks nothing is not a trade."
        ));
    }
    let gate = check_destination(world, faction, home, destination.0, PROPOSE_VERB)?;
    if !bands_have_standing_link(world, home_band, gate.destination_band) {
        return Err(format!(
            "{PROPOSE_VERB}: no standing link to band {} — a kept road must run the whole way \
             between you before you can haggle.",
            destination.0
        ));
    }
    check_cargo(world, faction, home, party_workers, &give, PROPOSE_VERB)?;
    let agreement = Agreement {
        shipper_faction: faction,
        shipper_band: home_band,
        consignee_faction: gate.destination_faction,
        consignee_band: gate.destination_band,
        party_workers,
        give,
        ask,
    };
    let dealing = Dealing {
        id: DealingId::default(),
        kind: DealingKind::Proposal,
        sender_faction: faction,
        sender_band: home_band,
        receiver_faction: gate.destination_faction,
        receiver_band: gate.destination_band,
        party: None,
        party_workers: 0,
        goods_sent: LocalStore::new(),
        goods_asked: LocalStore::new(),
        agreement: Some(agreement.clone()),
        treaty: None,
        posted_turn: tick,
        expires_turn: tick + u64::from(offer_wait_turns(world)),
    };
    let ends = DealingEnds::of(&dealing);
    let id = world.resource_mut::<PendingDealings>().post(dealing);
    push_agreement_posted_events(
        &mut world.resource_mut::<CommandEventLog>(),
        tick,
        id,
        DealingKind::Proposal,
        &ends,
        &agreement,
    );
    Ok(id)
}

/// **Silence is a refusal, and a dealing whose party has left is withdrawn** — run each turn after
/// [`crate::systems::advance_expeditions`], which posts them.
///
/// An offer is withdrawn when its party is no longer `AwaitingOrders` in the camp: recalled, sent
/// elsewhere with `move_band`, or turned for home because its destination was gone. Otherwise, at
/// [`Dealing::expires_turn`] the party turns `Returning` with its own cargo (a scout picks its
/// scouting back up) and the receiver is told the offer lapsed. A proposal or a counter has no
/// party: at `expires_turn` it simply lapses, told to both. Answers are commands and resolve in the
/// turn they are submitted, so by the time this runs a dealing that was answered is already out of
/// the book.
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
    let due: Vec<(DealingId, DealingKind, DealingEnds, Option<BandId>, bool)> = pending
        .iter()
        .map(|dealing| {
            (
                dealing.id,
                dealing.kind,
                DealingEnds::of(dealing),
                dealing.party,
                now >= dealing.expires_turn,
            )
        })
        .collect();
    for (id, kind, ends, party, expired) in due {
        // A treaty proposal: a road-carried one (or an envoy's counter) has no party and lapses at
        // its deadline; an envoy's is an offer's shape, its party waiting in the camp.
        if kind.is_treaty() {
            let terms = pending
                .get(id)
                .and_then(|dealing| dealing.treaty.as_ref())
                .map(|proposal| proposal.terms.clone())
                .unwrap_or_default();
            let (status, tell_sender) = match party {
                None => {
                    if !expired {
                        continue;
                    }
                    (TreatyStatus::Expired, true)
                }
                Some(party) => {
                    let waiting = parties.iter_mut().find(|(band, expedition)| {
                        **band == party && expedition.phase == ExpeditionPhase::AwaitingOrders
                    });
                    match waiting {
                        None => (TreatyStatus::Withdrawn, false),
                        Some(_) if !expired => continue,
                        Some((_, mut expedition)) => {
                            set_envoy_outcome(&mut expedition, EnvoyOutcome::Expired);
                            expedition.after_dealing();
                            (TreatyStatus::Expired, false)
                        }
                    }
                }
            };
            pending.remove(id);
            push_treaty_resolution_events(
                &mut event_log,
                now,
                id,
                &ends,
                status,
                &terms,
                tell_sender,
            );
            continue;
        }
        let resolution = if kind.is_agreement() {
            // No party to wait on: silence refuses it at the deadline.
            if !expired {
                continue;
            }
            Resolution::Expired
        } else {
            let waiting = party.and_then(|party| {
                parties
                    .iter_mut()
                    .find(|(band, expedition)| {
                        **band == party && expedition.phase == ExpeditionPhase::AwaitingOrders
                    })
                    .map(|(_, expedition)| expedition)
            });
            match waiting {
                None => Resolution::Withdrawn,
                Some(_) if !expired => continue,
                Some(mut expedition) => {
                    expedition.after_dealing();
                    Resolution::Expired
                }
            }
        };
        pending.remove(id);
        push_resolution_events(&mut event_log, now, id, kind, &ends, &resolution);
    }
}

// ------------------------------------------------------------------------------------------------
// Treaty proposals (`docs/plan_early_diplomacy.md` §3)
// ------------------------------------------------------------------------------------------------

const PROPOSE_TREATY_VERB: &str = "propose_treaty";

/// How a treaty dealing ended, for its feed lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreatyStatus {
    /// Now in force.
    Accepted,
    Refused,
    /// Silence at the deadline.
    Expired,
    /// The carrier is gone: the envoy left the camp, or the road no longer stands.
    Withdrawn,
    /// The receiver asked for different terms; a new proposal goes back.
    Countered,
}

impl TreatyStatus {
    fn token(self) -> &'static str {
        match self {
            TreatyStatus::Accepted => "accepted",
            TreatyStatus::Refused => "refused",
            TreatyStatus::Expired => "expired",
            TreatyStatus::Withdrawn => "withdrawn",
            TreatyStatus::Countered => "countered",
        }
    }
}

/// **The feed lines a treaty proposal being posted writes.** A road-carried one tells both peoples
/// (word travels along a kept road); an envoy-carried one tells only the receiver, because the
/// proposer learns when the envoy walks home.
pub fn push_treaty_posted_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    ends: &DealingEnds,
    proposal: &TreatyProposal,
) {
    let terms = proposal.terms.describe();
    if proposal.carrier == TreatyCarrier::Road {
        log.push(
            CommandEventEntry::new(
                tick,
                CommandEventKind::Treaty,
                ends.sender_faction,
                format!(
                    "Your treaty proposal to {} waits for an answer ({terms})",
                    band_label(ends.receiver_band)
                ),
                Some(format!(
                    "status=posted dealing={} kind={} carrier={} destination={}",
                    id.0,
                    DealingKind::TreatyProposal.as_str(),
                    proposal.carrier.as_str(),
                    ends.receiver_band.0
                )),
            )
            .with_band(ends.sender_band),
        );
    }
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Treaty,
            ends.receiver_faction,
            format!(
                "{} proposes a treaty: {terms}",
                band_label(ends.sender_band)
            ),
            Some(format!(
                "status=offered dealing={} kind={} carrier={} from={} sender={} band={}",
                id.0,
                DealingKind::TreatyProposal.as_str(),
                proposal.carrier.as_str(),
                ends.sender_faction.0,
                ends.sender_band.0,
                ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// **The feed lines a treaty dealing's end writes.** The receiver is always told at once. The
/// sender is told at once only when `tell_sender` — a road-carried proposal; an envoy's proposer
/// learns at the fold-back ([`report_envoy_home`]).
pub fn push_treaty_resolution_events(
    log: &mut CommandEventLog,
    tick: u64,
    id: DealingId,
    ends: &DealingEnds,
    status: TreatyStatus,
    terms: &TreatyTerms,
    tell_sender: bool,
) {
    let kind = DealingKind::TreatyProposal.as_str();
    let described = terms.describe();
    let (sender_line, receiver_line) = match status {
        TreatyStatus::Accepted => (
            format!(
                "{} accepted your treaty — now in force: {described}",
                band_label(ends.receiver_band)
            ),
            format!(
                "You accepted the treaty from {} — now in force: {described}",
                band_label(ends.sender_band)
            ),
        ),
        TreatyStatus::Refused => (
            format!("{} refused your treaty", band_label(ends.receiver_band)),
            format!(
                "You refused the treaty from {}",
                band_label(ends.sender_band)
            ),
        ),
        TreatyStatus::Expired => (
            format!(
                "{} did not answer your treaty proposal",
                band_label(ends.receiver_band)
            ),
            format!(
                "The treaty proposal from {} lapsed unanswered",
                band_label(ends.sender_band)
            ),
        ),
        TreatyStatus::Withdrawn => (
            format!(
                "Your treaty proposal to {} was withdrawn",
                band_label(ends.receiver_band)
            ),
            format!(
                "The treaty proposal from {} was withdrawn",
                band_label(ends.sender_band)
            ),
        ),
        TreatyStatus::Countered => (
            format!(
                "{} answered your treaty with different terms",
                band_label(ends.receiver_band)
            ),
            format!(
                "You countered the treaty from {}",
                band_label(ends.sender_band)
            ),
        ),
    };
    let status = status.token();
    if tell_sender {
        log.push(
            CommandEventEntry::new(
                tick,
                CommandEventKind::Treaty,
                ends.sender_faction,
                sender_line,
                Some(format!(
                    "status={status} dealing={} kind={kind} destination={}",
                    id.0, ends.receiver_band.0
                )),
            )
            .with_band(ends.sender_band),
        );
    }
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Treaty,
            ends.receiver_faction,
            receiver_line,
            Some(format!(
                "status={status} dealing={} kind={kind} from={} sender={} band={}",
                id.0, ends.sender_faction.0, ends.sender_band.0, ends.receiver_band.0
            )),
        )
        .with_band(ends.receiver_band),
    );
}

/// Record how an envoy's errand ended on the party, for it to carry home. A party whose mission is
/// not an envoy's is left alone.
pub fn set_envoy_outcome(expedition: &mut Expedition, outcome: EnvoyOutcome) {
    if let ExpeditionMission::Trade {
        terms: crate::components::ShipmentTerms::Envoy { outcome: slot, .. },
        ..
    } = &mut expedition.mission
    {
        *slot = Some(outcome);
    }
}

/// **The envoy is home, and its people learn the answer.** Writes the proposer's feed line and, when
/// the receiving people countered, posts their counter-proposal into the book — carried by the
/// envoy, so answering it needs no road. A world without the book loses the counter and still
/// tells the proposer.
#[allow(clippy::too_many_arguments)] // both ends, what was asked, how it ended, and the book
pub fn report_envoy_home(
    log: &mut CommandEventLog,
    pending: Option<&mut PendingDealings>,
    tick: u64,
    wait: u32,
    ends: &DealingEnds,
    proposed: &TreatyTerms,
    outcome: &EnvoyOutcome,
) {
    let kind = DealingKind::TreatyProposal.as_str();
    let (status, line) = match outcome {
        EnvoyOutcome::Accepted => (
            "accepted",
            format!(
                "Your envoy returns: {} accepted your treaty ({})",
                band_label(ends.receiver_band),
                proposed.describe()
            ),
        ),
        EnvoyOutcome::Refused => (
            "refused",
            format!(
                "Your envoy returns: {} refused your treaty",
                band_label(ends.receiver_band)
            ),
        ),
        EnvoyOutcome::Expired => (
            "expired",
            format!(
                "Your envoy returns: {} never answered your treaty",
                band_label(ends.receiver_band)
            ),
        ),
        EnvoyOutcome::Countered(terms) => (
            "countered",
            format!(
                "Your envoy returns: {} asks for different terms ({})",
                band_label(ends.receiver_band),
                terms.describe()
            ),
        ),
    };
    log.push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Treaty,
            ends.sender_faction,
            line,
            Some(format!(
                "status={status} kind={kind} carrier=envoy destination={} reported=envoy",
                ends.receiver_band.0
            )),
        )
        .with_band(ends.sender_band),
    );
    if let (EnvoyOutcome::Countered(terms), Some(pending)) = (outcome, pending) {
        // The counter goes the other way: from the receiving band back to the one that sent the
        // envoy, whose people answer it where they stand.
        let counter = Dealing {
            id: DealingId::default(),
            kind: DealingKind::TreatyProposal,
            sender_faction: ends.receiver_faction,
            sender_band: ends.receiver_band,
            receiver_faction: ends.sender_faction,
            receiver_band: ends.sender_band,
            party: None,
            party_workers: 0,
            goods_sent: LocalStore::new(),
            goods_asked: LocalStore::new(),
            agreement: None,
            treaty: Some(TreatyProposal {
                terms: terms.clone(),
                carrier: TreatyCarrier::Envoy,
            }),
            posted_turn: tick,
            expires_turn: tick + u64::from(wait),
        };
        let counter_ends = DealingEnds::of(&counter);
        let proposal = counter.treaty.clone().expect("the counter carries terms");
        let id = pending.post(counter);
        // The counter's addressee is the envoy's own people: a normal "offered" line for them.
        push_treaty_posted_events(log, tick, id, &counter_ends, &proposal);
    }
}

/// **Post an envoy's proposal when it reaches the receiver's camp.** The party stands in the camp
/// (`AwaitingOrders`, the wait an offer uses) for the answer; only the receiving people are told.
#[allow(clippy::too_many_arguments)] // both ends, the party, the terms and the clock
pub fn post_envoy_proposal(
    pending: &mut PendingDealings,
    log: &mut CommandEventLog,
    tick: u64,
    wait: u32,
    ends: &DealingEnds,
    party: BandId,
    party_workers: u32,
    terms: &TreatyTerms,
) -> DealingId {
    let proposal = TreatyProposal {
        terms: terms.clone(),
        carrier: TreatyCarrier::Envoy,
    };
    let id = pending.post(Dealing {
        id: DealingId::default(),
        kind: DealingKind::TreatyProposal,
        sender_faction: ends.sender_faction,
        sender_band: ends.sender_band,
        receiver_faction: ends.receiver_faction,
        receiver_band: ends.receiver_band,
        party: Some(party),
        party_workers,
        goods_sent: LocalStore::new(),
        goods_asked: LocalStore::new(),
        agreement: None,
        treaty: Some(proposal.clone()),
        posted_turn: tick,
        expires_turn: tick + u64::from(wait),
    });
    push_treaty_posted_events(log, tick, id, ends, &proposal);
    id
}

/// **Answer a treaty proposal.** A road-carried one needs the standing link between the two bands
/// to still hold (or it is withdrawn, as a stage-4 proposal is); an envoy-carried one needs its
/// party still standing in the camp — and nothing else. Accept puts the terms in force at once for
/// both peoples (replacing any treaty standing: an amendment, never a breach); refuse closes it;
/// counter closes it and sends the receiver's terms back.
fn answer_treaty(
    world: &mut World,
    dealing: Dealing,
    answer: &DealingAnswer,
    tick: u64,
) -> Result<AnswerReport, String> {
    let id = dealing.id;
    let ends = DealingEnds::of(&dealing);
    let Some(proposal) = dealing.treaty.clone() else {
        world.resource_mut::<PendingDealings>().remove(id);
        return Err(format!("{VERB}: dealing {} carries no terms.", id.0));
    };
    // The carrier still holds: a standing link (road) or the envoy standing in the camp.
    let party = match (proposal.carrier, dealing.party) {
        (TreatyCarrier::Road, _) => {
            if !bands_have_standing_link(world, ends.sender_band, ends.receiver_band) {
                world.resource_mut::<PendingDealings>().remove(id);
                push_treaty_resolution_events(
                    &mut world.resource_mut::<CommandEventLog>(),
                    tick,
                    id,
                    &ends,
                    TreatyStatus::Withdrawn,
                    &proposal.terms,
                    true,
                );
                return Err(format!(
                    "{VERB}: the road between band {} and band {} no longer holds — the treaty \
                     proposal was withdrawn.",
                    ends.sender_band.0, ends.receiver_band.0
                ));
            }
            None
        }
        (TreatyCarrier::Envoy, Some(party)) => {
            let standing = party_entity(world, party).filter(|&entity| {
                world
                    .get::<Expedition>(entity)
                    .is_some_and(|expedition| expedition.phase == ExpeditionPhase::AwaitingOrders)
            });
            let Some(entity) = standing else {
                world.resource_mut::<PendingDealings>().remove(id);
                push_treaty_resolution_events(
                    &mut world.resource_mut::<CommandEventLog>(),
                    tick,
                    id,
                    &ends,
                    TreatyStatus::Withdrawn,
                    &proposal.terms,
                    false,
                );
                return Err(format!(
                    "{VERB}: the envoy that made treaty proposal {} has gone.",
                    id.0
                ));
            };
            Some(entity)
        }
        // A counter an envoy carried home: nothing walks, nothing to check.
        (TreatyCarrier::Envoy, None) => None,
    };
    // The envoy's proposer is told when it walks home; a road-carried or envoy-less one at once.
    let tell_sender = party.is_none();
    let send_home = |world: &mut World, outcome: EnvoyOutcome| {
        if let Some(entity) = party {
            if let Some(mut expedition) = world.get_mut::<Expedition>(entity) {
                set_envoy_outcome(&mut expedition, outcome);
                expedition.after_dealing();
            }
        }
    };

    match answer {
        DealingAnswer::Refuse => {
            send_home(world, EnvoyOutcome::Refused);
            world.resource_mut::<PendingDealings>().remove(id);
            push_treaty_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                &ends,
                TreatyStatus::Refused,
                &proposal.terms,
                tell_sender,
            );
            Ok(AnswerReport::bare(id, dealing.kind, false))
        }
        DealingAnswer::Accept { returned } => {
            if !returned.is_empty() {
                return Err(format!(
                    "{VERB}: the terms of treaty proposal {} are already written; accept it as it \
                     stands, or counter.",
                    id.0
                ));
            }
            if resident_band_entity(world, ends.receiver_band).is_none() {
                return Err(format!(
                    "{VERB}: band {} is not there to answer.",
                    ends.receiver_band.0
                ));
            }
            world.get_resource_or_insert_with(Treaties::default).set(
                ends.sender_faction,
                ends.receiver_faction,
                proposal.terms.clone(),
                tick,
            );
            send_home(world, EnvoyOutcome::Accepted);
            world.resource_mut::<PendingDealings>().remove(id);
            push_treaty_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                &ends,
                TreatyStatus::Accepted,
                &proposal.terms,
                tell_sender,
            );
            Ok(AnswerReport::bare(id, dealing.kind, true))
        }
        DealingAnswer::CounterTerms { terms } => {
            let terms = TreatyTerms::resolve(world, terms, VERB)?;
            if terms.is_empty() {
                return Err(format!(
                    "{VERB}: a counter that asks for no term is not a treaty; refuse instead."
                ));
            }
            world.resource_mut::<PendingDealings>().remove(id);
            push_treaty_resolution_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                id,
                &ends,
                TreatyStatus::Countered,
                &proposal.terms,
                tell_sender,
            );
            let wait = offer_wait_turns(world);
            if party.is_some() {
                // The counter rides home with the envoy and is posted at the fold-back.
                send_home(world, EnvoyOutcome::Countered(terms));
                return Ok(AnswerReport::bare(id, dealing.kind, false));
            }
            let counter = Dealing {
                id: DealingId::default(),
                kind: DealingKind::TreatyProposal,
                sender_faction: ends.receiver_faction,
                sender_band: ends.receiver_band,
                receiver_faction: ends.sender_faction,
                receiver_band: ends.sender_band,
                party: None,
                party_workers: 0,
                goods_sent: LocalStore::new(),
                goods_asked: LocalStore::new(),
                agreement: None,
                treaty: Some(TreatyProposal {
                    terms,
                    carrier: proposal.carrier,
                }),
                posted_turn: tick,
                expires_turn: tick + u64::from(wait),
            };
            let counter_ends = DealingEnds::of(&counter);
            let counter_proposal = counter.treaty.clone().expect("the counter carries terms");
            let new_id = world.resource_mut::<PendingDealings>().post(counter);
            push_treaty_posted_events(
                &mut world.resource_mut::<CommandEventLog>(),
                tick,
                new_id,
                &counter_ends,
                &counter_proposal,
            );
            Ok(AnswerReport {
                counter: Some(new_id),
                ..AnswerReport::bare(id, dealing.kind, false)
            })
        }
        DealingAnswer::Counter { .. } => Err(format!(
            "{VERB}: treaty proposal {} names terms, not legs; counter it with the terms you \
             would accept.",
            id.0
        )),
    }
}

/// **Propose a treaty over a standing link** (`propose_treaty`). Posts a
/// [`DealingKind::TreatyProposal`] from `home_band` to `destination`, addressed to the
/// destination's people, **the turn it is made**. Fails closed before anything is recorded: the band
/// is the proposer's resident band, the destination is a band it holds a live tie to (the same tie
/// gate a shipment launch asks) and of another people, a **standing link** holds between the two,
/// and the terms ask for something.
pub fn propose_treaty(
    world: &mut World,
    faction: FactionId,
    home_band: BandId,
    destination: BandId,
    terms: &sim_runtime::TreatyTermsRequest,
) -> Result<DealingId, String> {
    let tick = world.resource::<SimulationTick>().0;
    let Some(home) = resident_band_entity(world, home_band) else {
        return Err(format!(
            "{PROPOSE_TREATY_VERB}: band {} is not a band.",
            home_band.0
        ));
    };
    match world.get::<PopulationCohort>(home) {
        Some(cohort) if cohort.faction == faction => {}
        _ => {
            return Err(format!(
                "{PROPOSE_TREATY_VERB}: band {} is not one of your people's bands.",
                home_band.0
            ));
        }
    }
    let terms = TreatyTerms::resolve(world, terms, PROPOSE_TREATY_VERB)?;
    check_treaty_terms_named(&terms, PROPOSE_TREATY_VERB)?;
    let gate = check_destination(world, faction, home, destination.0, PROPOSE_TREATY_VERB)?;
    check_not_own_people(faction, gate.destination_faction, PROPOSE_TREATY_VERB)?;
    if !bands_have_standing_link(world, home_band, gate.destination_band) {
        return Err(format!(
            "{PROPOSE_TREATY_VERB}: no standing link to band {} — a kept road must run the whole \
             way between you; send an envoy instead.",
            destination.0
        ));
    }
    let proposal = TreatyProposal {
        terms,
        carrier: TreatyCarrier::Road,
    };
    let dealing = Dealing {
        id: DealingId::default(),
        kind: DealingKind::TreatyProposal,
        sender_faction: faction,
        sender_band: home_band,
        receiver_faction: gate.destination_faction,
        receiver_band: gate.destination_band,
        party: None,
        party_workers: 0,
        goods_sent: LocalStore::new(),
        goods_asked: LocalStore::new(),
        agreement: None,
        treaty: Some(proposal.clone()),
        posted_turn: tick,
        expires_turn: tick + u64::from(offer_wait_turns(world)),
    };
    let ends = DealingEnds::of(&dealing);
    let id = world.resource_mut::<PendingDealings>().post(dealing);
    push_treaty_posted_events(
        &mut world.resource_mut::<CommandEventLog>(),
        tick,
        id,
        &ends,
        &proposal,
    );
    Ok(id)
}

/// A treaty proposal names at least one term.
pub fn check_treaty_terms_named(terms: &TreatyTerms, verb: &str) -> Result<(), String> {
    if terms.is_empty() {
        return Err(format!(
            "{verb}: a treaty that asks for no term is not a treaty — name road_rights, \
             open_borders or a bundle."
        ));
    }
    Ok(())
}

/// A people has no treaty with itself.
pub fn check_not_own_people(
    faction: FactionId,
    destination: FactionId,
    verb: &str,
) -> Result<(), String> {
    if faction == destination {
        return Err(format!(
            "{verb}: that band is one of your own people's — a treaty is between two peoples."
        ));
    }
    Ok(())
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
            party_workers: 1,
            goods_sent: LocalStore::new(),
            goods_asked: LocalStore::new(),
            agreement: None,
            treaty: None,
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
        let own = |terms: &ShipmentTerms, policies: &FactionBorderPolicies| {
            standing_answer_covers(terms, FactionId(0), FactionId(0), Some(policies))
        };
        assert!(own(&ShipmentTerms::Gift, &policies));
        assert!(
            !own(&ShipmentTerms::Offer, &policies),
            "an offer is always pending"
        );
        assert!(
            !own(
                &ShipmentTerms::Agreed {
                    agreed_return: GoodsLines::default(),
                    dealing: DealingId(1)
                },
                &policies
            ),
            "an agreed trade settles by its own terms, never by the standing answer"
        );
        assert!(!standing_answer_covers(
            &ShipmentTerms::Gift,
            FactionId(0),
            FactionId(1),
            Some(&policies)
        ));
        policies.set_accept_own_gifts(FactionId(0), false);
        assert!(!own(&ShipmentTerms::Gift, &policies), "the answer is off");
        assert!(standing_answer_covers(
            &ShipmentTerms::Gift,
            FactionId(0),
            FactionId(0),
            None
        ));
    }

    #[test]
    fn goods_lines_know_when_they_are_empty_and_name_what_they_hold() {
        let mut lines = GoodsLines::default();
        assert!(lines.is_empty());
        assert!(lines.manifest().is_empty());
        lines.food = scalar_from_f32(3.0);
        lines
            .materials
            .push(("hide".to_string(), scalar_from_f32(2.0)));
        assert!(!lines.is_empty());
        assert_eq!(lines.manifest(), vec!["3.00 provisions", "2.00 hide"]);
    }
}
