//! **THE REFERENCE BASKET, MEASURED** — the acceptance bar for rung 3's yield.
//!
//! A Field stopped being a *managed harvest* and became a *production gain*: it is foraged through
//! the ordinary drawn-down path exactly as a wild stand and a tended patch are, and what rung 3 buys
//! is **more standing crop and faster regrowth**. That is a re-expression, not a rebalance, so the
//! measured yield has to land where it already was.
//!
//! **The basket, quoted because a per-tile figure means nothing without it** (`cultivation.md`):
//! `AlluvialPlain`, `K = 195`, the realization of tile `(0,0)` under seed `0xF10A_5EED_C011_0010` —
//! `wild_emmer` 0.375 / `wild_tubers` 0.292 / `tobacco` 0.208 / `wild_rice` 0.125.

use core_sim::{
    commit_payoff, wild_payoff, FloraConfig, LaborConfig, RungKey, BUILTIN_LABOR_CONFIG,
};

mod common;
/// **The seed, tile, ground and crop are the SHARED pin** — `food_economy_table.rs` quotes its plant
/// ladder on the same realization, and two copies of it would drift.
use common::reference_basket as basket;

/// Quotes are captured at neutral productivity, as the shipped per-patch forecasts are.
const QUOTE_MULTIPLIER: f32 = 1.0;

/// **What rung 3 pays on this basket**, provisions/turn.
///
/// # ⛔ IT MOVED, AND THE MOVE IS THE POINT — 6.240 → 12.482
///
/// This number held at `6.240` through §4.10 because that change was a **re-expression**: the Field
/// stopped being a managed harvest and became a production gain, and the two new gains were chosen
/// to land the measured yield exactly where the retired flat rate had it.
///
/// **This change is not a re-expression.** `forage::favored_conversion_gain` returned the tended
/// rung's gain at `plant:tended` and the **identity** at every other rung, Field included — so rung 3
/// converted each unit of biomass at *half* the rung beneath it, and the ladder inverted at any crew
/// the carry limit binds. Reported from play: 2.00 food/turn on a tended patch, **1.33** on the same
/// tile sown to a Field, same two tenders. `cultivation.field_conversion_gain` restores the term rung
/// 3 was designed with and lost, so the pin doubles — exactly, because the gain does.
///
/// **The value is §4.14's to own from here**, and the epsilon below is deliberately unchanged: a
/// model change moves the number, it does not loosen the band that guards it.
const FIELD_YIELD_BEFORE: f32 = 12.482;
/// **What rung 2 pays**, unchanged by this arc — quoted so the ordering claim is checkable.
const TENDED_YIELD_BEFORE: f32 = 1.328;
/// **What the same ground pays left wild**, likewise unchanged.
const WILD_YIELD_BEFORE: f32 = 0.703;

/// How far the Field may land from its pinned figure. A **5%** band, and it has not moved: the slack
/// is there for the arithmetic reaching the number by a different route (capacity × regrowth through
/// the logistic MSY rather than a flat rate on the standing crop), **not** for a rebalance to hide
/// in. When the model changes, the pin moves and this stays — which is what happened when
/// `field_conversion_gain` restored rung 3's missing conversion term.
const ACCEPTANCE_BAND: f32 = 0.05;

/// Slack on the two rungs this arc does not touch — tight, because nothing about them moved.
const UNCHANGED_BAND: f32 = 0.01;

fn measured(rung: Option<RungKey>) -> f32 {
    let labor = LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG).expect("builtin labor config");
    let flora = FloraConfig::builtin();
    let capacity = basket::capacity(&labor);
    let composition = basket::composition(&flora);
    match rung {
        Some(rung) => commit_payoff(
            basket::TILE,
            capacity,
            basket::CROP,
            &composition,
            &flora,
            &labor.forage,
            QUOTE_MULTIPLIER,
            rung,
        ),
        None => wild_payoff(
            basket::TILE,
            capacity,
            &composition,
            &flora,
            &labor.forage,
            QUOTE_MULTIPLIER,
        ),
    }
}

/// **THE ACCEPTANCE BAR.** The Field's yield is re-expressed, not rebalanced: it must land within
/// [`ACCEPTANCE_BAND`] of what the retired managed rate paid on this basket.
///
/// **The two rungs below it are asserted at a far tighter band**, because this arc changes neither —
/// a drift there would mean the capacity or regrowth gain leaked onto a rung that must not have it.
#[test]
fn the_re_expressed_field_lands_where_the_managed_rate_did() {
    let wild = measured(None);
    let tended = measured(Some(RungKey::PlantTended));
    let field = measured(Some(RungKey::PlantField));

    assert!(
        (wild - WILD_YIELD_BEFORE).abs() <= WILD_YIELD_BEFORE * UNCHANGED_BAND,
        "rung 1 must not move: {wild} against {WILD_YIELD_BEFORE}"
    );
    assert!(
        (tended - TENDED_YIELD_BEFORE).abs() <= TENDED_YIELD_BEFORE * UNCHANGED_BAND,
        "rung 2 must not move: {tended} against {TENDED_YIELD_BEFORE} — a capacity or regrowth gain \
         that reached the tended rung would show up exactly here"
    );
    assert!(
        (field - FIELD_YIELD_BEFORE).abs() <= FIELD_YIELD_BEFORE * ACCEPTANCE_BAND,
        "rung 3 must land on its pinned figure: {field} against {FIELD_YIELD_BEFORE} \
         (wild {wild}, tended {tended})"
    );
    // And the ladder still climbs, which is the claim the yields exist to make.
    assert!(
        field > tended && tended > wild,
        "the ladder must pay more at every rung: {wild} -> {tended} -> {field}"
    );
}
