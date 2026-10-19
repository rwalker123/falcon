//! `dealings`, `goodwill`, `treaties` and `trade_history` sections -- early diplomacy (issue #233,
//! `docs/plan_early_diplomacy.md`), one module because the four are one arc and every converter
//! shares the goods-list and treaty-terms shapes.
//!
//! **A goods list is never a summed value.** `food` and `fodder` are two accounts that never
//! convert, and each material keeps every rating (`dict::material_batch_to_dict`, the cohort
//! store's own row shape). A bundle's lists carry NO ratings on the wire (nothing has been drawn
//! yet), so a bundle material row has empty `readings`.
//!
//! **Every section here is a whole-section replace** (present, even empty, means "this is the
//! roster now"), decoded on BOTH the snapshot and the delta path -- a section read on one path only
//! republishes the baseline's rows for the life of the world.
//!
//! **The word cuts and the turns left are the SIM's.** `cuts` is published once so the client never
//! restates `wary_below` / `friendly_above`; `turns_left` is never 0 on a published row.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use super::material_batch_to_dict;

/// `{food, fodder, materials: [...]}`. Absent list = the all-empty dict, so a reader never branches
/// on "no goods" versus "an empty list".
pub(crate) fn goods_to_dict(goods: Option<fb::DealingGoodsState<'_>>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut materials = VarArray::new();
    let (food, fodder) = goods.map_or((0.0, 0.0), |g| (g.food(), g.fodder()));
    if let Some(batches) = goods.and_then(|g| g.materials()) {
        for batch in batches.iter() {
            materials.push(&material_batch_to_dict(&batch).to_variant());
        }
    }
    let _ = dict.insert("food", f64::from(food));
    let _ = dict.insert("fodder", f64::from(fodder));
    let _ = dict.insert("materials", &materials);
    dict
}

/// `{road_rights, open_borders, bundles: [{sent, returned}]}`. Always a full dict (an absent table
/// is "no terms"), so an envoy cohort and a non-treaty dealing carry the same shape.
pub(crate) fn treaty_terms_to_dict(terms: Option<fb::TreatyTermsState<'_>>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("road_rights", terms.is_some_and(|t| t.roadRights()));
    let _ = dict.insert("open_borders", terms.is_some_and(|t| t.openBorders()));
    let mut bundles = VarArray::new();
    if let Some(list) = terms.and_then(|t| t.bundles()) {
        for bundle in list.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("sent", &goods_to_dict(bundle.sent()));
            let _ = row.insert("returned", &goods_to_dict(bundle.returned()));
            bundles.push(&row.to_variant());
        }
    }
    let _ = dict.insert("bundles", &bundles);
    dict
}

pub(crate) fn dealings_to_array(
    list: Vector<'_, ForwardsUOffset<fb::DealingState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for dealing in list {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("id", dealing.id() as i64);
        // "offer" | "proposal" | "counter" | "treaty_proposal" -- published as the sim's token.
        let _ = dict.insert("kind", dealing.kind().unwrap_or_default());
        let _ = dict.insert("sender_faction", dealing.senderFaction() as i64);
        let _ = dict.insert("sender_band", dealing.senderBand() as i64);
        let _ = dict.insert("receiver_faction", dealing.receiverFaction() as i64);
        let _ = dict.insert("receiver_band", dealing.receiverBand() as i64);
        let _ = dict.insert("goods_sent", &goods_to_dict(dealing.goodsSent()));
        let _ = dict.insert("goods_asked", &goods_to_dict(dealing.goodsAsked()));
        let _ = dict.insert("turns_left", dealing.turnsLeft() as i64);
        let _ = dict.insert("shipper_band", dealing.shipperBand() as i64);
        let _ = dict.insert("party_workers", dealing.partyWorkers() as i64);
        let _ = dict.insert("treaty", &treaty_terms_to_dict(dealing.treaty()));
        // "road" | "envoy" on a treaty proposal, "" on every other kind.
        let _ = dict.insert("carrier", dealing.carrier().unwrap_or_default());
        // The waiting party's band id (== that party's cohort `band_id`), `0` when no party carries it.
        let _ = dict.insert("party_band", dealing.partyBand() as i64);
        // "gift" | "offer" on an offer-kind dealing (a gift takes nothing back), "" on every other kind.
        let _ = dict.insert("terms", dealing.terms().unwrap_or_default());
        array.push(&dict.to_variant());
    }
    array
}

/// `[{turn, own_band, other_faction, other_band, dealing_id, outcome, goods_out, goods_in}]`, oldest
/// first, exactly as the sim published it (a whole-section replace on both paths). `outcome` is the
/// sim's token (`exchanged`, `gift_given`, ...); the goods lists are the proposal-leg form (unrated
/// material rows) and are always a full dict, so a reader never branches on "no goods".
pub(crate) fn trade_history_to_array(
    list: Vector<'_, ForwardsUOffset<fb::TradeRecordState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for record in list {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("turn", record.turn() as i64);
        let _ = dict.insert("own_band", record.ownBand() as i64);
        let _ = dict.insert("other_faction", record.otherFaction() as i64);
        let _ = dict.insert("other_band", record.otherBand() as i64);
        let _ = dict.insert("dealing_id", record.dealingId() as i64);
        let _ = dict.insert("outcome", record.outcome().unwrap_or_default());
        let _ = dict.insert("goods_out", &goods_to_dict(record.goodsOut()));
        let _ = dict.insert("goods_in", &goods_to_dict(record.goodsIn()));
        array.push(&dict.to_variant());
    }
    array
}

fn goodwill_rows_to_array(
    list: Option<Vector<'_, ForwardsUOffset<fb::GoodwillState<'_>>>>,
) -> VarArray {
    let mut array = VarArray::new();
    let Some(list) = list else {
        return array;
    };
    for row in list {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("other_faction", row.otherFaction() as i64);
        // The reasons: one component per source kind. `total` is their sum, clamped to -1..+1.
        let _ = dict.insert("gifts", f64::from(row.gifts()));
        let _ = dict.insert("exchanges", f64::from(row.exchanges()));
        let _ = dict.insert("stationed", f64::from(row.stationed()));
        let _ = dict.insert("link", f64::from(row.link()));
        let _ = dict.insert("treaty", f64::from(row.treaty()));
        let _ = dict.insert("breach", f64::from(row.breach()));
        let _ = dict.insert("total", f64::from(row.total()));
        let _ = dict.insert("reported_turn", row.reportedTurn() as i64);
        array.push(&dict.to_variant());
    }
    array
}

/// `{known: [...], own: [...], cuts: {wary_below, friendly_above}}`. `known` is what another people
/// thinks of the viewer AS LAST REPORTED (never live, `reported_turn` says when); `own` is the
/// viewer's own live opinion of each other people.
pub(crate) fn goodwill_to_dict(section: fb::GoodwillSection<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("known", &goodwill_rows_to_array(section.goodwill()));
    let _ = dict.insert("own", &goodwill_rows_to_array(section.own()));
    let mut cuts = VarDictionary::new();
    if let Some(c) = section.cuts() {
        let _ = cuts.insert("wary_below", f64::from(c.waryBelow()));
        let _ = cuts.insert("friendly_above", f64::from(c.friendlyAbove()));
    }
    let _ = dict.insert("cuts", &cuts);
    dict
}

/// `{treaties: [{other_faction, terms, formed_turn}], standing_links: [{viewer_band, other_band,
/// other_faction}]}`. A standing link is the sim's published fact; the client never derives one.
pub(crate) fn treaties_to_dict(section: fb::TreatySection<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut treaties = VarArray::new();
    if let Some(list) = section.treaties() {
        for treaty in list {
            let mut row = VarDictionary::new();
            let _ = row.insert("other_faction", treaty.otherFaction() as i64);
            let _ = row.insert("terms", &treaty_terms_to_dict(treaty.terms()));
            let _ = row.insert("formed_turn", treaty.formedTurn() as i64);
            treaties.push(&row.to_variant());
        }
    }
    let mut links = VarArray::new();
    if let Some(list) = section.standingLinks() {
        for link in list {
            let mut row = VarDictionary::new();
            let _ = row.insert("viewer_band", link.viewerBand() as i64);
            let _ = row.insert("other_band", link.otherBand() as i64);
            let _ = row.insert("other_faction", link.otherFaction() as i64);
            links.push(&row.to_variant());
        }
    }
    let _ = dict.insert("treaties", &treaties);
    let _ = dict.insert("standing_links", &links);
    dict
}

/// A delta's `goodwill` section, merged over what the client already holds. **The section table is
/// always present on a delta** (the codec requires it) and each of its three parts is an `Option`
/// ("absent = unchanged this frame"), so presence of the TABLE is no change signal at all: reading
/// it as one republished empty `known`/`own` rows on every delta and wiped the client's opinion of
/// every people. `None` when no part rode this delta; otherwise the held dict with exactly the
/// parts that did ride replaced.
pub(crate) fn goodwill_delta_dict(
    held: Option<VarDictionary>,
    section: fb::GoodwillSection<'_>,
) -> Option<VarDictionary> {
    if section.goodwill().is_none() && section.own().is_none() && section.cuts().is_none() {
        return None;
    }
    let full = goodwill_to_dict(section);
    let mut dict = held.map_or_else(VarDictionary::new, |d| d.duplicate_shallow());
    for (present, key) in [
        (section.goodwill().is_some(), "known"),
        (section.own().is_some(), "own"),
        (section.cuts().is_some(), "cuts"),
    ] {
        if present || !dict.contains_key(key) {
            if let Some(value) = full.get(key) {
                let _ = dict.insert(key, &value);
            }
        }
    }
    Some(dict)
}

/// A delta's `treaties` section, merged over the held one on [`goodwill_delta_dict`]'s rules: the
/// table is always there, `treaties` and `standingLinks` are each "absent = unchanged".
pub(crate) fn treaties_delta_dict(
    held: Option<VarDictionary>,
    section: fb::TreatySection<'_>,
) -> Option<VarDictionary> {
    if section.treaties().is_none() && section.standingLinks().is_none() {
        return None;
    }
    let full = treaties_to_dict(section);
    let mut dict = held.map_or_else(VarDictionary::new, |d| d.duplicate_shallow());
    for (present, key) in [
        (section.treaties().is_some(), "treaties"),
        (section.standingLinks().is_some(), "standing_links"),
    ] {
        if present || !dict.contains_key(key) {
            if let Some(value) = full.get(key) {
                let _ = dict.insert(key, &value);
            }
        }
    }
    Some(dict)
}
