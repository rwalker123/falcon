//! **The retired harvest-floor levers, asserted absent** (`docs/plan_harvest_floor.md` §4).
//!
//! Slice 2b deleted `forage.market.trade_goods_multiplier` — the 4× markup a `Deplete`-depth gather
//! used to earn on its trade component — and this file was the arithmetic that said what that cost.
//!
//! **Arc #527 retired the trade-goods axis itself**, so the two measurements that lived here
//! (`trade_income_is_linear_in_the_take_at_every_floor` and the `measure_the_lost_markup` harness)
//! are deleted: they priced an account that no longer exists, and the property they asserted —
//! *no option carries a factor of any kind* — is now pinned on the accounts that do, by
//! `labor_allocation::a_deeper_floor_pays_more_because_it_takes_more` (food) and
//! `forage_tended_vector::a_deeper_floor_banks_more_material_off_a_tended_cash_crop` (materials).
//!
//! **What survives is the structural guard below**, which is the durable half: it reads the shipped
//! JSON and asserts the retired keys are really gone. The file keeps its name because that is the
//! name `fauna_config::tests::the_hunt_block_carries_no_take_multiplier` cites as its other half,
//! and because it names the arc rather than the account.

/// **THE DELETED CONFIG HAS NO READER LEFT** — the structural half of §4's acceptance test.
///
/// Slice 2b removed `forage.surplus_multiplier`, `forage.market` (whole, including the 4× markup),
/// `forage.eradicate.take_fraction`, and `hunt.{surplus_multiplier, deplete_multiplier,
/// surplus_escapement_fraction}`. A grep would prove nothing durable; this asserts the **shipped
/// JSON** carries none of those keys.
///
/// **It says nothing about the Rust structs, and that gap was a live bug.** The three `hunt.*` keys
/// were deleted from the file while `HuntConfig` kept the fields — `#[serde(default)]` filled them
/// silently, `FaunaConfig::validate` still enforced their ordering, and a `FAUNA_CONFIG_PATH` file
/// setting one could therefore **panic the server at boot over a lever with no reader**. This test
/// passed throughout. The struct half is asserted where the struct is in scope, at compile time:
/// `fauna_config::tests::the_hunt_block_carries_no_take_multiplier`.
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

    // **The trade-goods RATE went the way of its multiplier** (arc #527): it was written by every
    // take site and read by none, while the `materials` rows beside it named the same take's actual
    // hide, bone and fibre. Asserted on both webs' shipped JSON, because a key `#[serde(default)]`
    // would fill silently is exactly what this test exists to catch.
    assert!(
        fauna["hunt"].get("trade_goods_per_biomass").is_none(),
        "fauna_config `hunt.trade_goods_per_biomass` is retired with the trade axis"
    );
    for (key, species) in fauna["species"]
        .as_object()
        .expect("the species table is an object")
    {
        assert!(
            species["hunt_yield"]
                .get("trade_goods_per_biomass")
                .is_none(),
            "fauna_config species `{key}` still names a retired trade rate"
        );
    }
    let flora: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_FLORA_CONFIG).expect("the builtin flora parses");
    for (key, species) in flora["species"]
        .as_object()
        .expect("the flora species table is an object")
    {
        assert!(
            species["yield"].get("trade_goods_per_biomass").is_none(),
            "flora_config species `{key}` still names a retired trade rate"
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
