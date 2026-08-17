//! **The flora roster is provably economy-neutral** (slice F1, `docs/plan_flora_roster.md` §8).
//!
//! F1's whole claim is that naming the plants *decomposes* the human food web's existing capacity
//! and never adds to it. That claim is not a promise about tuning — it is an arithmetic property of
//! the normalized share table plus the verbatim yield vector, and these tests assert exactly that.
//!
//! Every assertion is made against the **loaded** `labor_config`, never a literal, so if either
//! table drifts the test fails instead of quietly agreeing with a stale copy of itself.

use bevy::math::UVec2;
use core_sim::{FloraConfig, LaborConfig, BUILTIN_LABOR_CONFIG, NO_FORAGE_CAPACITY};
use sim_runtime::TerrainType;

/// f32 slack for a sum of up to a handful of normalized shares.
const SHARE_EPSILON: f32 = 1e-5;

/// Relative slack for `Σ share × capacity` against the capacity itself (capacities run to ~210, so a
/// relative bound is the honest one for f32).
const CAPACITY_RELATIVE_EPSILON: f32 = 1e-5;

/// A pinned seed for every per-tile realization sweep (§10) — so "best/worst realization" is the same
/// measured extreme run to run.
const SWEEP_SEED: u64 = 0x_F10A_5EED_2EA1_0010;
/// Side of the tile grid each sweep samples per biome (8×8 = 64 tiles — enough draws that a species
/// realizes both dominant and absent across its hosted ground).
const SWEEP_SIDE: u32 = 8;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

/// The tile coords a realization sweep samples — a small deterministic grid.
fn sweep_tiles() -> impl Iterator<Item = UVec2> {
    (0..SWEEP_SIDE).flat_map(|x| (0..SWEEP_SIDE).map(move |y| UVec2::new(x, y)))
}

/// `species`' realized share of `terrain`'s basket on tile `coord` under [`SWEEP_SEED`] — `0.0` where
/// the tile does not realize it.
fn realized_share(flora: &FloraConfig, terrain: TerrainType, species: &str, coord: UVec2) -> f32 {
    flora
        .realized_composition(terrain, coord, SWEEP_SEED)
        .iter()
        .find(|entry| entry.species == species)
        .map(|entry| entry.share)
        .unwrap_or(0.0)
}

/// **Is this species a CASH crop?** Since arc #527 the trade scalar that defined one is gone, so the
/// definition is read off the accounts that remain: a cash crop pays **no food and no fodder** and is
/// paid entirely in **materials**. That is not a weaker test than the retired "trade above the flat
/// token" — it is the same statement about the same five rows, made against the accounts the sim
/// actually credits.
fn is_cash_crop(def: &core_sim::FloraDef) -> bool {
    def.yield_.provisions_per_biomass == 0.0
        && def.yield_.fodder_per_biomass == 0.0
        && !def.yield_.materials.is_empty()
}

/// What a **tended** patch committed to `species` converts at on this basket, and what the same
/// ground converts at gathered **wild** — both through the sim's own [`core_sim::
/// patch_provisions_per_biomass`] seam, never a re-derivation of its arithmetic (§4.3's rule).
fn tended_and_wild_rate(
    flora: &FloraConfig,
    labor: &LaborConfig,
    composition: &[core_sim::FloraShare],
    species: &str,
) -> (f32, f32) {
    let coord = UVec2::ZERO;
    let capacity = 1.0; // rates are per unit biomass; the capacity is irrelevant to them.
    let mut tended = core_sim::ForagePatch::new(coord, capacity);
    // The rung is FINISHED here — a bare `1.0` no longer completes anything now that a job has a
    // size (`docs/plan_unit_costed_work.md`), so this runs the real accrual. The faction cannot
    // matter: a rate is a pure function of ground and config.
    tended.complete_cultivation(core_sim::FactionId(0), &core_sim::LadderConfig::builtin());
    tended.species = Some(species.to_string());
    let wild = core_sim::ForagePatch::new(coord, capacity);
    (
        core_sim::patch_provisions_per_biomass(&tended, composition, flora, &labor.forage),
        core_sim::patch_provisions_per_biomass(&wild, composition, flora, &labor.forage),
    )
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

/// **The yield vector routes by account, and EVERY species pays into at least one.** S1 made
/// `provisions_per_biomass` per-species (`docs/plan_flora_roster.md` §4.3); F3 opened the **fodder**
/// account for the one fodder crop, hay_grass; the cash crops are paid entirely in **materials**
/// (arc #527 retired the trade scalar that used to carry them). So a **staple** pays food and no
/// fodder, the **fodder crop** pays fodder and no food, and a **cash crop** pays neither and carries
/// material rows instead. `role` is a display tag, so this reads the *vector*, not the tag. Regrowth
/// is still verbatim on every row.
///
/// **The last clause is the one that would have caught the trade retirement silently breaking five
/// species**: a row paying into no account at all is a plant that grows and produces nothing, which
/// `FloraConfig::validate`'s `pays_something` now rejects at load.
#[test]
fn the_yield_vector_routes_by_account_and_every_species_pays_into_one() {
    let flora = FloraConfig::builtin();
    let forage = labor().forage;

    let mut fodder_crops = 0;
    let mut cash_crops = 0;
    for (key, def) in &flora.species {
        let is_fodder_crop = def.yield_.fodder_per_biomass > 0.0;
        // The three categories are read off the *vector*, never the display `role` (which is derived
        // from exactly this).
        let is_cash = !is_fodder_crop && is_cash_crop(def);
        assert!(
            def.yield_.provisions_per_biomass > 0.0
                || def.yield_.fodder_per_biomass > 0.0
                || !def.yield_.materials.is_empty(),
            "`{key}` pays into no account at all — it would grow forever and produce nothing"
        );
        if is_fodder_crop {
            // A fodder crop pays into the fodder account, NOT provisions.
            fodder_crops += 1;
            assert_eq!(
                def.yield_.provisions_per_biomass, 0.0,
                "fodder crop `{key}` must pay no provisions — hay feeds animals, not people"
            );
        } else if is_cash {
            // A cash crop pays in materials alone: no food, no fodder.
            cash_crops += 1;
            assert_eq!(
                def.yield_.provisions_per_biomass, 0.0,
                "cash crop `{key}` must pay no provisions — it is worthless as food"
            );
            assert_eq!(
                def.yield_.fodder_per_biomass, 0.0,
                "cash crop `{key}` must pay no fodder — its payoff is what it is MADE OF"
            );
            assert!(
                !def.yield_.materials.is_empty(),
                "cash crop `{key}` must name the material it pays in — it has no other account"
            );
        } else {
            // A staple converts biomass to food positively and — since the fodder account is for hay
            // alone — pays no fodder.
            assert!(
                def.yield_.provisions_per_biomass > 0.0,
                "staple `{key}` must convert biomass into food at some positive rate"
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
        cash_crops, 5,
        "F4's four cash crops (cotton, flax, tobacco, tea) + the F5 grapevine"
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

/// **THE commit trade, asserted as the design states it** (§4.3), on the PER-TILE REALIZED basket
/// (§10) and **reframed by #433**.
///
/// The older bar — *"committing must sometimes LOSE to leaving it wild"* — is retired, and
/// deliberately: with the rung-2 conversion gain on the favored term, any commitment with a real
/// share pays *something*, and rung 2's decision is which currency plus whether the 25-turn build is
/// worth it. What must still differ is **the crop choice**, and that is what this asserts:
///
/// - on its **best country** a crop's tended rate beats gathering that same tile wild — the rung pays
///   where the crop is at home;
/// - and a crop's **best country out-pays its worst** — a river valley is not a hillside, so *where*
///   you farm a crop is a real question rather than a formality.
///
/// Swept over a grid of tiles at a pinned seed, through the sim's own rate seam, so both readings are
/// measured extremes of the number the sim actually pays.
#[test]
fn every_climbing_species_pays_on_its_best_country_and_pays_less_on_its_worst() {
    let flora = FloraConfig::builtin();
    let labor = labor();

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
        // The best tended rate this crop reaches on each hosted biome, over a tile sweep — and the
        // wild rate of whichever tile that was, so "beats wild" is answered on the same ground.
        let mut per_country: Vec<f32> = Vec::new();
        let mut best = (f32::MIN, 0.0_f32);
        for terrain in def.host_biomes.keys() {
            let mut country_best = f32::MIN;
            for coord in sweep_tiles() {
                if realized_share(&flora, *terrain, key, coord) <= 0.0 {
                    continue; // this tile did not realize the crop; there is nothing to tend.
                }
                let composition = flora.realized_composition(*terrain, coord, SWEEP_SEED);
                let (tended, wild) = tended_and_wild_rate(&flora, &labor, &composition, key);
                country_best = country_best.max(tended);
                if tended > best.0 {
                    best = (tended, wild);
                }
            }
            if country_best > f32::MIN {
                per_country.push(country_best);
            }
        }
        assert!(
            !per_country.is_empty(),
            "`{key}` realizes on no sampled tile of any host biome — the sweep cannot judge it"
        );
        assert!(
            best.0 > best.1,
            "`{key}` never beats the wild basket even on its best country: tended {} vs wild {}",
            best.0,
            best.1
        );
        if per_country.len() < 2 {
            continue; // a one-country crop has no "best vs worst country" to compare.
        }
        let richest = per_country.iter().copied().fold(f32::MIN, f32::max);
        let poorest = per_country.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            richest > poorest,
            "`{key}` pays the same on every country it hosts ({richest}) — then WHERE you farm it \
             is not a decision"
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

/// **The cash crops pay a positive MATERIAL yield, and they compete with grain on SOWABLE ground**
/// (Flora Roster F4, `docs/plan_flora_roster.md` §6). A cash crop's worth-it bar is its material
/// account, not provisions: a cash Field's harvest credits the band's material batches, which the
/// worth-committing bar above (provisions-only) deliberately skips. The land-use tension is real only
/// if a cash crop grows where grain could: each must host at least one biome that also hosts a
/// **sowable** staple (one that reaches the field rung), so growing cash genuinely displaces calories
/// on ground a Field could have taken.
#[test]
fn the_cash_crops_pay_a_positive_material_yield_and_contest_sowable_ground() {
    let flora = FloraConfig::builtin();

    let cash: Vec<(&String, &_)> = flora
        .species
        .iter()
        .filter(|(_, def)| is_cash_crop(def))
        .collect();
    assert_eq!(
        cash.len(),
        5,
        "F4's four cash crops (cotton, flax, tobacco, tea) + the F5 grapevine"
    );

    for (key, def) in cash {
        assert!(
            def.yield_.materials.iter().all(|row| row.per_biomass > 0.0),
            "`{key}` must pay a positive rate in every material it names — it is the only account \
             a cash Field credits"
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

/// **THE PROOF the forage command grammar's disambiguation rests on** — no shipped
/// `flora_config.json` species key parses as an `f32`.
///
/// `assign_labor ... forage <x> <y> [floor] [species] <workers>` has two optional tokens and reads a
/// lone one as *the floor if it parses as a number, else the species*
/// (`sim_runtime::command_text`). That is unambiguous only while the two token languages are
/// disjoint, and this is the half of that claim which lives in **this** crate: a future crop keyed
/// `7` or `0.5` would silently become a floor, and would fail here first.
///
/// Its companion `command_text::tests::a_species_key_never_parses_as_a_floor` asserts the same
/// property against inlined key *shapes*, because `sim_runtime` cannot depend on `core_sim`.
#[test]
fn every_shipped_species_key_is_covered_by_the_command_grammar() {
    let flora = FloraConfig::builtin();
    assert!(
        !flora.species.is_empty(),
        "the shipped roster must have species, or this asserts nothing"
    );
    for key in flora.species.keys() {
        assert!(
            key.parse::<f32>().is_err(),
            "'{key}' parses as a float, so `assign_labor forage` would read it as a FLOOR rather \
             than as the crop selection — rename it, or the grammar needs a separator"
        );
        assert!(
            !key.trim().is_empty(),
            "an empty species key would be indistinguishable from an absent one"
        );
    }
}
