//! **The commit trade** (Flora Roster S1, `docs/plan_flora_roster.md` §4.3).
//!
//! Committing a patch to one named plant does two things and only two: it **redistributes** the
//! tile's `K` toward that plant (concentration, bounded by the tile's own `K` — *the land owns `K`*)
//! and it changes how well biomass **converts** to food (the species' own rate instead of the
//! basket average). This file asserts the arithmetic of that trade against the **loaded** configs,
//! never a literal, so a retune of either table fails the test instead of quietly agreeing with a
//! stale copy of itself.
//!
//! Both rungs pay `MSY × rate`, and `MSY = r · K / 4` is linear in `K`, so at a **fixed rung** (same
//! `r`) the whole trade reduces to the product `concentration × species_rate` against the wild
//! `1.0 × forage.provisions_per_biomass`. That product is exactly what these tests compare.

use core_sim::{
    FloraConfig, ForagePatch, LaborConfig, RungKey, BUILTIN_LABOR_CONFIG, FULL_TILE_CONCENTRATION,
    NO_CONCENTRATION,
};
use sim_runtime::TerrainType;

/// f32 slack on a product of two normalized-ish terms.
const EPSILON: f32 = 1e-6;

/// Relative slack for a ratio of two provisions/turn quotes (each a chain of ~4 multiplications).
const RATIO_EPSILON: f32 = 1e-5;

/// The quotes are captured at neutral productivity — the client scales by the acting band's
/// `outputMultiplier`, exactly as the shipped per-patch forecast is.
const QUOTE_MULTIPLIER: f32 = 1.0;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

/// A patch standing on the **tended** rung, committed to `species`, on `terrain`'s basket.
fn tended_patch(terrain: TerrainType, species: Option<&str>, capacity: f32) -> ForagePatch {
    let mut patch = ForagePatch::new(bevy::math::UVec2::new(terrain as u32, 0), capacity);
    patch.cultivation_progress = 1.0;
    patch.species = species.map(str::to_string);
    patch
}

/// What one unit of this patch's food-bearing land is worth per turn, relative to the wild basket:
/// `effective_capacity × conversion rate`. The rung's `r` is the same on both sides of every
/// comparison below, so this product **is** the trade.
fn commit_value(patch: &ForagePatch, terrain: TerrainType) -> f32 {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let tile_capacity = labor.forage.capacity_for(terrain);
    let composition = flora.composition(terrain);
    core_sim::effective_forage_capacity(patch, tile_capacity, composition, &labor.forage)
        * core_sim::patch_provisions_per_biomass(patch, &flora, &labor.forage)
}

/// **Rung 1 is untouched.** A patch with no commitment reads the tile's full `K` and the flat wild
/// conversion rate — the same two numbers it read before the roster existed — so nothing about a
/// wild gather can have moved.
#[test]
fn an_uncommitted_patch_reads_the_full_tile_and_the_wild_rate() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    for terrain in TerrainType::VALUES {
        let capacity = labor.forage.capacity_for(terrain);
        let patch = tended_patch(terrain, None, capacity);
        assert_eq!(
            core_sim::patch_concentration(&patch, flora.composition(terrain), &labor.forage),
            NO_CONCENTRATION,
            "{terrain:?}: an uncommitted patch holds the whole basket"
        );
        assert_eq!(
            core_sim::patch_provisions_per_biomass(&patch, &flora, &labor.forage),
            labor.forage.provisions_per_biomass,
            "{terrain:?}: an uncommitted patch converts at the basket average"
        );
        assert_eq!(
            core_sim::effective_forage_capacity(
                &patch,
                capacity,
                flora.composition(terrain),
                &labor.forage
            ),
            capacity,
            "{terrain:?}: an uncommitted patch carries the tile's own K"
        );
    }
}

/// **A patch still being prepared has not displaced anything yet**, so it reads exactly like the wild
/// stand it still is. Both halves of the commitment switch on together when the rung completes —
/// there is no state where one applies and the other does not.
#[test]
fn a_commitment_takes_effect_only_when_the_improvement_completes() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let terrain = TerrainType::AlluvialPlain;
    let capacity = labor.forage.capacity_for(terrain);

    let mut building = tended_patch(terrain, Some("wild_emmer"), capacity);
    building.cultivation_progress = 0.5;
    assert_eq!(
        core_sim::patch_concentration(&building, flora.composition(terrain), &labor.forage),
        NO_CONCENTRATION
    );
    assert_eq!(
        core_sim::patch_provisions_per_biomass(&building, &flora, &labor.forage),
        labor.forage.provisions_per_biomass
    );
}

/// **The land owns `K`.** However hard a rung concentrates, the patch can never carry more than the
/// tile's own capacity — raising `K` itself is rung 4. Checked with the *field* gain (the higher of
/// the two) on the biome where the committed plant is the entire basket, i.e. the case that would
/// blow the bound if anything did.
#[test]
fn concentration_never_exceeds_the_tiles_own_capacity() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    for terrain in TerrainType::VALUES {
        let capacity = labor.forage.capacity_for(terrain);
        for share in flora.composition(terrain) {
            let mut patch = tended_patch(terrain, Some(&share.species), capacity);
            patch.field_progress = 1.0; // the higher (field) concentration gain
            let concentration =
                core_sim::patch_concentration(&patch, flora.composition(terrain), &labor.forage);
            assert!(
                concentration <= FULL_TILE_CONCENTRATION + EPSILON,
                "{terrain:?}/{}: concentration {concentration} exceeded the tile's own K",
                share.species
            );
            assert!(
                core_sim::effective_forage_capacity(
                    &patch,
                    capacity,
                    flora.composition(terrain),
                    &labor.forage
                ) <= capacity + EPSILON,
                "{terrain:?}/{}: effective capacity exceeded the tile's own",
                share.species
            );
        }
    }
}

/// **The commit trade is real, in both directions.** Committing a tile to the plant that already
/// dominates its basket beats leaving it wild; committing it to a marginal one does not. If the
/// first were false rung 2 would be a rung nobody climbs; if the second were false it would be a
/// free lunch, and the roster would have stopped being a decision.
#[test]
fn committing_beats_wild_on_a_dominant_share_and_loses_on_a_marginal_one() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    // Wild Emmer is most of an alluvial plain's basket and a bit-part on rolling hills — the same
    // plant, the same rung, the same rate, judged only by how much of the ground is already it.
    let dominant = TerrainType::AlluvialPlain;
    let marginal = TerrainType::RollingHills;
    let crop = "wild_emmer";
    assert!(
        flora.species[crop].cultivation_ceiling.allows_cultivate(),
        "the fixture crop must actually climb"
    );

    for (terrain, expect_worth_it) in [(dominant, true), (marginal, false)] {
        let capacity = labor.forage.capacity_for(terrain);
        let committed = commit_value(&tended_patch(terrain, Some(crop), capacity), terrain);
        let wild = commit_value(&tended_patch(terrain, None, capacity), terrain);
        if expect_worth_it {
            assert!(
                committed > wild,
                "{terrain:?}: tending the dominant plant must beat the wild basket \
                 ({committed} vs {wild})"
            );
        } else {
            assert!(
                committed < wild,
                "{terrain:?}: tending a marginal plant must LOSE to the wild basket \
                 ({committed} vs {wild}) — otherwise committing is free"
            );
        }
    }
}

/// **The PUBLISHED ratio IS the sim's own payoff, divided** — not a re-derivation that happens to
/// agree. `commit_yield_ratio` must equal `rung_payoff` for a patch committed to that plant and
/// worked up to that rung, over `rung_payoff` for the same tile left wild — both produced by the
/// functions the sim itself quotes and pays with.
///
/// **This is the test the first version of this file got wrong**, and the bug it missed is worth
/// naming: the old assertion compared `effective_capacity × conversion_rate` on both sides — a
/// *capacity*-based basis, in which the ecology's `r` cancels. But rungs 1–2 pay **MSY** (`r · K / 4`)
/// and tending's whole payoff is that it multiplies `r` by `cultivation.tended_regrowth_gain`. So `r`
/// must **not** cancel, the old basis silently dropped it, and code and test shared the same wrong
/// assumption and agreed with each other — publishing every Cultivate ratio at exactly half its true
/// value. Asserting against the *payoff functions* rather than against their arithmetic is what
/// closes that hole.
#[test]
fn the_published_commit_ratio_is_the_sims_own_payoff_divided_by_the_wild_payoff() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    for terrain in TerrainType::VALUES {
        let capacity = forage.capacity_for(terrain);
        if capacity <= 0.0 {
            continue;
        }
        let tile = bevy::math::UVec2::new(terrain as u32, 0);
        let wild = core_sim::wild_payoff(tile, capacity, &flora, forage, QUOTE_MULTIPLIER);
        assert!(
            wild > 0.0,
            "{terrain:?}: a forage-bearing tile pays something wild"
        );

        for share in flora.composition(terrain) {
            for rung in [RungKey::PlantTended, RungKey::PlantField] {
                let payoff = core_sim::commit_payoff(
                    tile,
                    capacity,
                    &share.species,
                    share.share,
                    &flora,
                    forage,
                    QUOTE_MULTIPLIER,
                    rung,
                );
                let ratio = core_sim::commit_yield_ratio(payoff, wild);
                let climbs = match rung {
                    RungKey::PlantField => flora.species[&share.species]
                        .cultivation_ceiling
                        .allows_sow(),
                    _ => flora.species[&share.species]
                        .cultivation_ceiling
                        .allows_cultivate(),
                };
                if !climbs {
                    assert_eq!(
                        ratio,
                        core_sim::CANNOT_CLIMB_RATIO,
                        "{terrain:?}/{}: a plant that cannot climb {rung:?} quotes the sentinel",
                        share.species
                    );
                    assert_eq!(payoff, core_sim::CANNOT_CLIMB_RATIO);
                    continue;
                }
                let expected = payoff / wild;
                assert!(
                    (ratio - expected).abs() <= RATIO_EPSILON * expected.max(1.0),
                    "{terrain:?}/{} at {rung:?}: published {ratio} but the sim pays {payoff}/turn \
                     against the wild {wild}/turn = {expected}",
                    share.species
                );
            }
        }
    }
}

/// **Tending's payoff INCLUDES the regrowth gain** — stated as its own test because it is the exact
/// term the first implementation dropped. A tended patch rides `tended_ecology`, so on a tile where
/// concentration and conversion both come out neutral (`share × gain >= 1`, a plant at the wild
/// rate) the ratio is the gain itself, not `1.0`.
#[test]
fn the_cultivate_ratio_carries_the_tended_regrowth_gain() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    // Reeds are the whole basket on a river delta, so concentration saturates and the ratio is
    // exactly `tended_regrowth_gain × the crop's conversion advantage`.
    let terrain = TerrainType::RiverDelta;
    let crop = "reed_and_root";
    let share = flora
        .composition(terrain)
        .iter()
        .find(|entry| entry.species == crop)
        .expect("reeds are the delta's basket")
        .share;
    let expected = forage.cultivation.tended_regrowth_gain
        * core_sim::concentration_for_share(
            share,
            core_sim::concentration_gain(forage, RungKey::PlantTended),
        )
        * flora.species[crop].yield_.provisions_per_biomass
        / forage.provisions_per_biomass;
    let capacity = forage.capacity_for(terrain);
    let tile = bevy::math::UVec2::new(terrain as u32, 0);
    let ratio = core_sim::commit_yield_ratio(
        core_sim::commit_payoff(
            tile,
            capacity,
            crop,
            share,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantTended,
        ),
        core_sim::wild_payoff(tile, capacity, &flora, forage, QUOTE_MULTIPLIER),
    );
    assert!(
        (ratio - expected).abs() <= RATIO_EPSILON * expected,
        "the Cultivate ratio must carry tended_regrowth_gain: {ratio} vs {expected}"
    );
    assert!(
        ratio > forage.cultivation.tended_regrowth_gain,
        "on its own delta a committed crop must beat the bare regrowth gain: {ratio}"
    );
}

/// **The legality rule, and what the auto-pick falls to.** A basket whose whole membership stops at
/// the `wild` ceiling can be committed to nothing at all — "not every plant climbs" reaching the
/// build meter — while an ordinary land basket resolves to its highest-share legal member.
#[test]
fn legality_follows_the_cultivation_ceiling_and_the_tiles_own_basket() {
    let flora = FloraConfig::builtin();

    // An open-water fishery: shellfish alone, `wild` forever.
    let shelf = flora.composition(TerrainType::ContinentalShelf);
    assert!(
        core_sim::default_species_for_rung(shelf, &flora, RungKey::PlantTended).is_none(),
        "a basket of wild harvests can be committed to nothing"
    );

    // A river plain: emmer leads the basket and climbs the whole ladder.
    let plain = flora.composition(TerrainType::AlluvialPlain);
    assert_eq!(
        core_sim::default_species_for_rung(plain, &flora, RungKey::PlantField).as_deref(),
        Some("wild_emmer"),
        "the auto-pick is the highest-share species the rung permits"
    );
    // A plant that grows elsewhere is not legal here, however well it climbs.
    assert!(
        !core_sim::species_is_legal_here("date_palm", plain, &flora, RungKey::PlantTended),
        "a plant that does not grow on this tile may not be committed to it"
    );
}
