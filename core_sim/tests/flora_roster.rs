//! **The flora roster is provably economy-neutral** (slice F1, `docs/plan_flora_roster.md` §8).
//!
//! F1's whole claim is that naming the plants *decomposes* the human food web's existing capacity
//! and never adds to it. That claim is not a promise about tuning — it is an arithmetic property of
//! the normalized share table plus the verbatim yield vector, and these tests assert exactly that.
//!
//! Every assertion is made against the **loaded** `labor_config`, never a literal, so if either
//! table drifts the test fails instead of quietly agreeing with a stale copy of itself.

use core_sim::{FloraConfig, LaborConfig, BUILTIN_LABOR_CONFIG, NO_FORAGE_CAPACITY};
use sim_runtime::TerrainType;

/// f32 slack for a sum of up to a handful of normalized shares.
const SHARE_EPSILON: f32 = 1e-5;

/// Relative slack for `Σ share × capacity` against the capacity itself (capacities run to ~210, so a
/// relative bound is the honest one for f32).
const CAPACITY_RELATIVE_EPSILON: f32 = 1e-5;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

#[test]
fn every_biome_is_either_fully_named_or_carries_no_forage() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    for terrain in TerrainType::VALUES {
        let capacity = forage.capacity_for(terrain);
        let shares = flora.composition(terrain);

        if capacity <= NO_FORAGE_CAPACITY {
            assert!(
                shares.is_empty(),
                "{terrain:?} carries no forage, so no plant may claim a share of it (got {shares:?})"
            );
            continue;
        }

        assert!(
            !shares.is_empty(),
            "{terrain:?} carries {capacity} forage but no plant names it"
        );
        let total: f32 = shares.iter().map(|share| share.share).sum();
        assert!(
            (total - 1.0).abs() <= SHARE_EPSILON,
            "{terrain:?} composition sums to {total}, not 1.0 — the decomposition is not normalized"
        );
    }
}

#[test]
fn the_named_shares_re_sum_to_exactly_the_biomes_capacity() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    for terrain in TerrainType::VALUES {
        let capacity = forage.capacity_for(terrain);
        if capacity <= NO_FORAGE_CAPACITY {
            continue;
        }
        // The decomposition ruling, stated as arithmetic: the parts re-sum to the whole, so naming
        // the plants cannot move a single tile's capacity.
        let decomposed: f32 = flora
            .composition(terrain)
            .iter()
            .map(|share| share.share * capacity)
            .sum();
        assert!(
            (decomposed - capacity).abs() <= capacity * CAPACITY_RELATIVE_EPSILON,
            "{terrain:?}: the named shares total {decomposed}, but the biome carries {capacity}"
        );
    }
}

/// **The navigable-river hole** — the one class of tile whose capacity is not a single
/// `capacity_by_biome` row. A navigable hex carries `capacity_for(underlying) +
/// navigable_river_forage_bonus`, so decomposing only the underlying biome would leave the whole
/// fishery bonus unnamed and `Σ share × capacity` would fall short by exactly that term. This is the
/// assertion that catches it.
#[test]
fn a_navigable_hex_names_both_its_valley_and_its_fishery() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    for underlying in TerrainType::VALUES {
        let capacity = forage.navigable_forage_capacity(underlying);
        let shares = flora.navigable_composition(underlying, &forage);

        // A navigable hex is always a fishery, so its capacity is always positive — there is no
        // "no forage here" case to skip.
        assert!(
            capacity > NO_FORAGE_CAPACITY,
            "a navigable hex over {underlying:?} must carry forage (it is always a fishery)"
        );
        assert!(
            !shares.is_empty(),
            "a navigable hex over {underlying:?} carries {capacity} forage but names no plant"
        );

        let total: f32 = shares.iter().map(|share| share.share).sum();
        assert!(
            (total - 1.0).abs() <= SHARE_EPSILON,
            "navigable over {underlying:?}: shares sum to {total}, not 1.0"
        );

        let decomposed: f32 = shares.iter().map(|share| share.share * capacity).sum();
        assert!(
            (decomposed - capacity).abs() <= capacity * CAPACITY_RELATIVE_EPSILON,
            "navigable over {underlying:?}: the named shares total {decomposed}, but the hex \
             carries {capacity} (valley + fishery)"
        );

        // The fishery term is a real, named part of the basket — not rounded away into the valley.
        // Skipped for the self-referential `underlying == NavigableRiver`, which the sim cannot
        // produce (`Tile::resource_terrain()` on a navigable hex is the biome the channel *cut*):
        // there the channel's own basket appears in both terms and correctly **merges** to 1.0, which
        // the duplicate check below is what actually pins.
        if underlying != TerrainType::NavigableRiver {
            let fishery: f32 = shares
                .iter()
                .filter(|share| share.species == "river_fish")
                .map(|share| share.share)
                .sum();
            let expected = forage.navigable_river_forage_bonus / capacity;
            assert!(
                (fishery - expected).abs() <= SHARE_EPSILON,
                "navigable over {underlying:?}: river_fish holds {fishery} of the basket, but the \
                 fishery bonus is {expected} of the capacity"
            );
        }

        // One row per species, always — a future roster edit that puts a plant on both terms must
        // merge, never duplicate.
        let mut keys: Vec<&str> = shares.iter().map(|share| share.species.as_str()).collect();
        keys.sort_unstable();
        let unique = keys.len();
        keys.dedup();
        assert_eq!(
            keys.len(),
            unique,
            "navigable over {underlying:?}: a species appears twice in one basket"
        );
    }
}

/// **The yield vector routes by account, and only the accounts F3/F4 have opened are live.** S1 made
/// `provisions_per_biomass` per-species (`docs/plan_flora_roster.md` §4.3); F3 opened the **fodder**
/// account for the one fodder crop, hay_grass. So a **staple** pays food and no fodder, the **fodder
/// crop** pays fodder and no food, and trade stays the flat F1 rate on staples (0.0 on the fodder
/// crop — a fodder crop pays no trade). `role` is a display tag, so this reads the *vector*, not the
/// tag. Regrowth is still verbatim on every row (S1/F3 move no regrowth).
#[test]
fn the_yield_vector_routes_by_account_and_only_opened_accounts_are_live() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    let flat_trade = forage.market.trade_goods_per_biomass;
    let mut fodder_crops = 0;
    let mut cash_crops = 0;
    for (key, def) in &flora.species {
        let is_fodder_crop = def.yield_.fodder_per_biomass > 0.0;
        // A CASH crop (Flora Roster F4) pays into the trade account alone: no food, no fodder, and a
        // trade rate ABOVE the flat F1 token every staple still carries. The three categories are
        // read off the *vector*, never the display `role` (which is derived from exactly this).
        let is_cash_crop = !is_fodder_crop
            && def.yield_.provisions_per_biomass == 0.0
            && def.yield_.trade_goods_per_biomass > flat_trade;
        if is_fodder_crop {
            // A fodder crop pays into the fodder account, NOT provisions and NOT trade.
            fodder_crops += 1;
            assert_eq!(
                def.yield_.provisions_per_biomass, 0.0,
                "fodder crop `{key}` must pay no provisions — hay feeds animals, not people"
            );
            assert_eq!(
                def.yield_.trade_goods_per_biomass, 0.0,
                "fodder crop `{key}` must pay no trade — its payoff is the fodder account"
            );
        } else if is_cash_crop {
            // A cash crop pays into the trade account alone (F4): no food, no fodder, trade dominant.
            cash_crops += 1;
            assert_eq!(
                def.yield_.provisions_per_biomass, 0.0,
                "cash crop `{key}` must pay no provisions — it is worthless as food"
            );
            assert_eq!(
                def.yield_.fodder_per_biomass, 0.0,
                "cash crop `{key}` must pay no fodder — its payoff is the trade account"
            );
            assert!(
                def.yield_.trade_goods_per_biomass > flat_trade,
                "cash crop `{key}` must pay trade ABOVE the flat token that differentiates it \
                 ({} vs {flat_trade})",
                def.yield_.trade_goods_per_biomass
            );
        } else {
            // A staple converts biomass to food positively, pays the flat F1 trade rate, and — since
            // the fodder account is for hay alone — pays no fodder.
            assert!(
                def.yield_.provisions_per_biomass > 0.0,
                "staple `{key}` must convert biomass into food at some positive rate"
            );
            assert_eq!(
                def.yield_.trade_goods_per_biomass, flat_trade,
                "staple `{key}` must still carry the flat trade token verbatim — F4 owns cash crops"
            );
            assert_eq!(
                def.yield_.fodder_per_biomass, 0.0,
                "staple `{key}` must pay no fodder — only a fodder crop does"
            );
        }
        assert_eq!(
            def.regrowth_rate, forage.ecology.regrowth_rate,
            "`{key}` must regrow at forage.ecology.regrowth_rate — S1/F3/F4 move no regrowth"
        );
    }
    assert_eq!(
        fodder_crops, 1,
        "F3 ships exactly one fodder crop (hay_grass)"
    );
    assert_eq!(
        cash_crops, 4,
        "F4 ships exactly four cash crops (cotton, flax, tobacco, tea)"
    );
    assert!(
        flora
            .species
            .values()
            .any(|def| def.yield_.provisions_per_biomass != forage.provisions_per_biomass),
        "the roster must actually differentiate — a flat table makes rung 2 a strict downgrade"
    );
}

/// **What you GATHER sits at or below the wild baseline.** Every `wild`-ceiling species can never be
/// committed at all, so its rate is inert by construction — and it must read as inert: an oak's mast
/// or a bed of shellfish is what the basket already averages, not a crop.
#[test]
fn the_gathered_wild_things_never_beat_the_basket_average() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    for (key, def) in &flora.species {
        if def.cultivation_ceiling.allows_cultivate() {
            continue;
        }
        assert!(
            def.yield_.provisions_per_biomass <= forage.provisions_per_biomass,
            "`{key}` is a wild harvest — it must not convert better than the basket average \
             ({} vs {})",
            def.yield_.provisions_per_biomass,
            forage.provisions_per_biomass
        );
    }
}

/// **THE commit trade, asserted as the design states it** (§4.3):
///
/// ```text
/// tending is worth it  ⟺  concentration × species_rate  >  1.0 × wild_rate
/// ```
///
/// For every species that *can* climb, the roster must make that true **somewhere** — on its best
/// country — and false on its worst hosted ground. A species that clears the bar everywhere it grows
/// is a free lunch; one that clears it nowhere is a rung nobody would ever climb.
#[test]
fn every_climbing_species_is_worth_committing_on_its_best_country_and_not_on_its_worst() {
    let flora = FloraConfig::builtin();
    let labor = labor();
    let forage = &labor.forage;
    let tended_gain = forage.cultivation.tended_concentration_gain;

    for (key, def) in &flora.species {
        if !def.cultivation_ceiling.allows_cultivate() {
            continue;
        }
        // A **fodder crop** (F3) or a **cash crop** (F4) climbs the ladder too, but its payoff is in
        // the fodder / trade account, not provisions — the provisions bar below would read its `0.0`
        // food rate as "never worth tending". A fodder crop's worth-it bar is
        // `the_fodder_crop_pays_a_positive_fodder_yield`; a cash crop's is
        // `the_cash_crops_pay_a_positive_trade_yield` (and, end to end, `flora_f4_cash.rs`).
        if def.yield_.provisions_per_biomass == 0.0 {
            continue;
        }
        // The commit value on each biome this species hosts: `min(1, share × gain) × rate`, against
        // the wild basket's `1.0 × forage.provisions_per_biomass`.
        let mut values: Vec<(TerrainType, f32)> = def
            .host_biomes
            .keys()
            .map(|terrain| {
                let share = flora
                    .composition(*terrain)
                    .iter()
                    .find(|entry| entry.species == *key)
                    .map(|entry| entry.share)
                    .expect("a hosted biome names its host");
                let concentration = (share * tended_gain).min(1.0);
                (*terrain, concentration * def.yield_.provisions_per_biomass)
            })
            .collect();
        values.sort_by(|a, b| b.1.total_cmp(&a.1));
        let wild = forage.provisions_per_biomass;
        let (best_biome, best) = values[0];
        let (worst_biome, worst) = *values.last().expect("a species hosts at least one biome");
        assert!(
            best > wild,
            "`{key}` is never worth tending — best country {best_biome:?} pays {best} against the \
             wild basket's {wild}"
        );
        assert!(
            worst < wild,
            "`{key}` is worth tending even on {worst_biome:?}, where it is marginal ({worst} vs \
             {wild}) — a commitment that is right everywhere is not a decision"
        );
    }
}

/// **The fodder crop pays a positive fodder yield, and it is a Field crop that competes with grain**
/// (Flora Roster F3, `docs/plan_flora_roster.md` §5). Its worth-it bar is the fodder account, not
/// provisions: a hay Field's harvest is `> 0`, so a pen keeper who grows it has hay to draw. It hosts
/// the good sowable farmland — so growing hay costs a grain tile — and it climbs to the Field rung
/// (you Sow it).
#[test]
fn the_fodder_crop_pays_a_positive_fodder_yield() {
    let flora = FloraConfig::builtin();

    let fodder: Vec<(&String, &_)> = flora
        .species
        .iter()
        .filter(|(_, def)| def.yield_.fodder_per_biomass > 0.0)
        .collect();
    assert_eq!(fodder.len(), 1, "F3 ships exactly one fodder crop");

    let (key, def) = fodder[0];
    assert!(
        def.yield_.fodder_per_biomass > 0.0,
        "`{key}` must pay a positive fodder rate — it is what a pen draws"
    );
    assert!(
        def.cultivation_ceiling.allows_sow(),
        "`{key}` is a Field crop (you Sow hay) — it must reach the field rung"
    );
    // It competes with grain for scarce sowable tiles: every biome it hosts must also host at least
    // one staple, so growing hay genuinely displaces calories.
    for terrain in def.host_biomes.keys() {
        let contested = flora
            .composition(*terrain)
            .iter()
            .any(|share| flora.species[&share.species].yield_.provisions_per_biomass > 0.0);
        assert!(
            contested,
            "`{key}` hosts {terrain:?} but no staple does — hay must contest grain's ground"
        );
    }
}

/// **The cash crops pay a positive trade yield, and they compete with grain on SOWABLE ground**
/// (Flora Roster F4, `docs/plan_flora_roster.md` §6). A cash crop's worth-it bar is the trade
/// account, not provisions: a cash Field's harvest credits the faction `trade_goods` stockpile,
/// which the worth-committing bar above (provisions-only) deliberately skips. The land-use tension is
/// real only if a cash crop grows where grain could: each must host at least one biome that also
/// hosts a **sowable** staple (one that reaches the field rung), so growing cash genuinely displaces
/// calories on ground a Field could have taken.
#[test]
fn the_cash_crops_pay_a_positive_trade_yield_and_contest_sowable_ground() {
    let flora = FloraConfig::builtin();
    let flat_trade = labor().forage.market.trade_goods_per_biomass;

    let cash: Vec<(&String, &_)> = flora
        .species
        .iter()
        .filter(|(_, def)| {
            def.yield_.provisions_per_biomass == 0.0
                && def.yield_.fodder_per_biomass == 0.0
                && def.yield_.trade_goods_per_biomass > flat_trade
        })
        .collect();
    assert_eq!(
        cash.len(),
        4,
        "F4 ships exactly four cash crops (cotton, flax, tobacco, tea)"
    );

    for (key, def) in cash {
        assert!(
            def.yield_.trade_goods_per_biomass > flat_trade,
            "`{key}` must pay a trade rate above the flat token — it is what a cash Field credits"
        );
        assert!(
            def.cultivation_ceiling.allows_sow(),
            "`{key}` is a Field crop (you Sow it) — it must reach the field rung"
        );
        // It must contest a sowable staple's ground: at least one biome it hosts also hosts a staple
        // that itself reaches the field rung, so growing cash there genuinely forgoes calories.
        let contests_sowable_grain = def.host_biomes.keys().any(|terrain| {
            flora.composition(*terrain).iter().any(|share| {
                let staple = &flora.species[&share.species];
                staple.yield_.provisions_per_biomass > 0.0
                    && staple.cultivation_ceiling.allows_sow()
            })
        });
        assert!(
            contests_sowable_grain,
            "`{key}` must host at least one biome where a sowable grain also grows — cash must \
             contest grain's SOWABLE ground for the land-use tension to land"
        );
    }
}
