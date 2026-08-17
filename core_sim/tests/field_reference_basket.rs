//! **THE REFERENCE BASKET, MEASURED** — the acceptance bar for the rung-3 re-expression.
//!
//! A Field stopped being a *managed harvest* and became a *production gain*: it is foraged through
//! the ordinary drawn-down path exactly as a wild stand and a tended patch are, and what rung 3 buys
//! is **more standing crop and faster regrowth**. That is a re-expression, not a rebalance, so the
//! measured yield has to land where it already was.
//!
//! **The basket, quoted because a per-tile figure means nothing without it** (`cultivation.md`):
//! `AlluvialPlain`, `K = 195`, the realization of tile `(0,0)` under seed `0xF10A_5EED_C011_0010` —
//! `wild_emmer` 0.375 / `wild_tubers` 0.292 / `tobacco` 0.208 / `wild_rice` 0.125.

use bevy::math::UVec2;
use core_sim::{
    commit_payoff, wild_payoff, FloraConfig, LaborConfig, RungKey, BUILTIN_LABOR_CONFIG,
};
use sim_runtime::TerrainType;

/// The pinned realization seed — the one the shipped `sweep_tiles` fixtures use.
const REFERENCE_SEED: u64 = 0x_F10A_5EED_C011_0010;
/// The pinned tile.
const REFERENCE_TILE: UVec2 = UVec2::new(0, 0);
/// The pinned ground.
const REFERENCE_TERRAIN: TerrainType = TerrainType::AlluvialPlain;
/// The pinned crop — the basket's best staple on this ground.
const REFERENCE_CROP: &str = "wild_emmer";
/// Quotes are captured at neutral productivity, as the shipped per-patch forecasts are.
const QUOTE_MULTIPLIER: f32 = 1.0;

/// **What rung 3 paid on this basket under the retired managed-harvest model**, provisions/turn.
/// The re-expression has to land here.
const FIELD_YIELD_BEFORE: f32 = 6.240;
/// **What rung 2 pays**, unchanged by this arc — quoted so the ordering claim is checkable.
const TENDED_YIELD_BEFORE: f32 = 1.328;
/// **What the same ground pays left wild**, likewise unchanged.
const WILD_YIELD_BEFORE: f32 = 0.703;

/// How far the re-expressed Field may land from the number it replaced. A **5%** band: this is a
/// re-expression, so the product of the two new gains is chosen to hit the old figure, and the slack
/// is there for the arithmetic reaching it by a different route (capacity × regrowth through the
/// logistic MSY rather than a flat rate on the standing crop), not for a rebalance to hide in.
const ACCEPTANCE_BAND: f32 = 0.05;

/// Slack on the two rungs this arc does not touch — tight, because nothing about them moved.
const UNCHANGED_BAND: f32 = 0.01;

fn measured(rung: Option<RungKey>) -> f32 {
    let labor = LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG).expect("builtin labor config");
    let flora = FloraConfig::builtin();
    let capacity = labor.forage.capacity_for(REFERENCE_TERRAIN);
    let composition = flora.realized_composition(REFERENCE_TERRAIN, REFERENCE_TILE, REFERENCE_SEED);
    match rung {
        Some(rung) => commit_payoff(
            REFERENCE_TILE,
            capacity,
            REFERENCE_CROP,
            &composition,
            &flora,
            &labor.forage,
            QUOTE_MULTIPLIER,
            rung,
        ),
        None => wild_payoff(
            REFERENCE_TILE,
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
        "rung 3 must land where the managed rate did: {field} against {FIELD_YIELD_BEFORE} \
         (wild {wild}, tended {tended})"
    );
    // And the ladder still climbs, which is the claim the yields exist to make.
    assert!(
        field > tended && tended > wild,
        "the ladder must pay more at every rung: {wild} -> {tended} -> {field}"
    );
}
