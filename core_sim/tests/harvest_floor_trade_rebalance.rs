//! **The 2b trade rebalance, measured** (`docs/plan_harvest_floor.md` §4).
//!
//! Slice 2b deleted `forage.market.trade_goods_multiplier` — the 4× markup a `Deplete`-depth gather
//! used to earn on its trade component. This file is the arithmetic that says what that cost, and it
//! is a **regression guard on the shape**, not on the old number: after this slice **no option
//! carries a factor of any kind**, so trade income must be exactly linear in the biomass taken.

use bevy::math::UVec2;
use core_sim::{FloraConfig, ForagePatch, LaborConfig};

/// The reference tile the plant figures in `.claude/rules/core_sim/` are quoted on.
const REFERENCE_BIOME: sim_runtime::TerrainType = sim_runtime::TerrainType::AlluvialPlain;
const REFERENCE_TILE: UVec2 = UVec2::new(0, 0);
const REFERENCE_MAP_SEED: u64 = 0xF10A_5EED_C011_0010;

/// The markup the deleted `forage.market.trade_goods_multiplier` used to apply at a
/// `Deplete`-depth draw. Retained **here only**, as the measurement's baseline — it is not read by
/// the sim and has no config row left.
const RETIRED_DEPLETE_MARKUP: f32 = 4.0;

/// A neutral band, so the numbers are the ground's own.
const UNIT_OUTPUT_MULTIPLIER: f32 = 1.0;

/// **TRADE INCOME IS LINEAR IN THE TAKE — no option carries a factor.**
///
/// That is §4's acceptance test, and it is what the measurement below is *for*: with the markup gone
/// there is exactly one way depth can raise trade income, which is by taking more biomass. Doubling
/// the biomass taken must double the trade earned, at every depth, with no discontinuity at the
/// floor the retired markup used to key on (`0.15`).
#[test]
fn trade_income_is_linear_in_the_take_at_every_floor() {
    let labor = LaborConfig::builtin();
    let flora = FloraConfig::builtin();
    let composition =
        flora.realized_composition(REFERENCE_BIOME, REFERENCE_TILE, REFERENCE_MAP_SEED);
    let capacity = labor.forage.capacity_for(REFERENCE_BIOME);
    let patch = ForagePatch::new(REFERENCE_TILE, capacity);
    let rate = core_sim::patch_trade_per_biomass(&patch, &composition, &flora, &labor.forage);
    assert!(
        rate > 0.0,
        "the reference basket must carry a trade component, or this asserts nothing"
    );

    // The floor the retired markup keyed on, its neighbours either side, and the ends of the dial.
    for floor in [0.0_f32, 0.14, 0.15, 0.16, 0.5, 0.8] {
        let take = (capacity - floor * capacity).max(0.0);
        let earned = core_sim::forage_provisions(take, rate, UNIT_OUTPUT_MULTIPLIER);
        let doubled = core_sim::forage_provisions(take * 2.0, rate, UNIT_OUTPUT_MULTIPLIER);
        assert!(
            (doubled - 2.0 * earned).abs() < 1e-4,
            "floor {floor}: twice the biomass must earn twice the trade — a factor anywhere breaks \
             this ({earned} vs {doubled})"
        );
    }

    // …and there is no step at the retired markup's floor: `0.14` and `0.16` differ only by the
    // biomass between them. Under the markup, `0.15` alone paid 4× and this ratio was ~4.
    let trade_at = |floor: f32| {
        core_sim::forage_provisions(
            (capacity - floor * capacity).max(0.0),
            rate,
            UNIT_OUTPUT_MULTIPLIER,
        )
    };
    let step = trade_at(0.15) / trade_at(0.16);
    assert!(
        step < 1.05,
        "no discontinuity at the retired markup's floor: 0.15 earns {step}× what 0.16 does, and \
         under the 4× markup it earned ~{RETIRED_DEPLETE_MARKUP}×"
    );
}

/// **What the rebalance cost, in the sim's own units** — printed rather than asserted, because the
/// point is the magnitude, not a pinned number a retune would have to chase.
///
/// Run with `cargo test -p core_sim --test harvest_floor_trade_rebalance -- --ignored --nocapture`.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn measure_the_lost_markup() {
    let labor = LaborConfig::builtin();
    let flora = FloraConfig::builtin();
    let composition =
        flora.realized_composition(REFERENCE_BIOME, REFERENCE_TILE, REFERENCE_MAP_SEED);
    let capacity = labor.forage.capacity_for(REFERENCE_BIOME);
    let patch = ForagePatch::new(REFERENCE_TILE, capacity);
    let rate = core_sim::patch_trade_per_biomass(&patch, &composition, &flora, &labor.forage);
    let food_rate =
        core_sim::patch_provisions_per_biomass(&patch, &composition, &flora, &labor.forage);

    println!("\n=== 2b trade rebalance — {REFERENCE_BIOME:?}, K = {capacity} ===");
    println!("basket trade rate {rate:.5}/biomass, food rate {food_rate:.5}/biomass");
    println!(
        "\n{:<8} {:>10} {:>12} {:>12} {:>10}",
        "floor", "take", "trade NOW", "trade WAS", "food"
    );
    for floor in [0.0_f32, 0.15, 0.3, 0.5, 0.8] {
        let take = (capacity - floor * capacity).max(0.0);
        let now = core_sim::forage_provisions(take, rate, UNIT_OUTPUT_MULTIPLIER);
        // Only the 0.15 row ever earned the markup — it keyed on Deplete's floor alone.
        let was = if (floor - 0.15).abs() < 1e-6 {
            now * RETIRED_DEPLETE_MARKUP
        } else {
            now
        };
        println!(
            "{floor:<8} {take:>10.1} {now:>12.3} {was:>12.3} {:>10.3}",
            core_sim::forage_provisions(take, food_rate, UNIT_OUTPUT_MULTIPLIER)
        );
    }
}

/// **THE DELETED CONFIG HAS NO READER LEFT** — the structural half of §4's acceptance test.
///
/// Slice 2b removed `forage.surplus_multiplier`, `forage.market` (whole, including the 4× markup),
/// `forage.eradicate.take_fraction`, and `hunt.{surplus_multiplier, deplete_multiplier,
/// surplus_escapement_fraction}`. A grep would prove nothing durable; this asserts the shape those
/// keys lived in, so re-adding a field to the struct fails here before it can quietly grow a reader.
///
/// **`ecology.collapse_fraction` STAYS and is asserted present**: it is the Allee/depensation
/// threshold `net_biomass_delta` reads, and it only ever moonlighted as one stance's floor. Deleting
/// it with its neighbours would have been the bug.
#[test]
fn the_deleted_levers_are_gone_and_the_allee_threshold_is_not() {
    let labor: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_LABOR_CONFIG).expect("the builtin labor parses");
    let fauna: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_FAUNA_CONFIG).expect("the builtin fauna parses");

    for key in ["surplus_multiplier", "market", "eradicate"] {
        assert!(
            labor["forage"].get(key).is_none(),
            "labor_config `forage.{key}` is retired — no option carries a factor any more"
        );
    }
    for key in [
        "surplus_multiplier",
        "deplete_multiplier",
        "surplus_escapement_fraction",
    ] {
        assert!(
            fauna["hunt"].get(key).is_none(),
            "fauna_config `hunt.{key}` is retired with the stance axis it tuned"
        );
    }

    // The Allee threshold is a live ecology term, not a retired floor.
    let collapse = fauna["ecology"]["collapse_fraction"]
        .as_f64()
        .expect("ecology.collapse_fraction is the depensation threshold and must stay");
    assert!(
        collapse > 0.0 && collapse < 1.0,
        "…and it is still a fraction of K: {collapse}"
    );
    assert!(
        (core_sim::FaunaConfig::builtin().ecology.collapse_fraction - collapse as f32).abs() < 1e-6,
        "the struct still carries it, and reads the shipped value"
    );
}
