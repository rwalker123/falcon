//! **The commit trade** (Flora Roster S1, `docs/plan_flora_roster.md` §4.3; reframed by #433).
//!
//! Committing a patch to one named plant does two things and only two: it **reweights** the tile's
//! basket toward that plant (weeding at rung 2, planting at rung 3 — *the land owns `K`, and no rung
//! below 4 raises it or lowers it*) and it changes how well biomass **converts** to food (the
//! favored term carries `tended_conversion_gain`). This file asserts the arithmetic of that trade
//! against the **loaded** configs, never a literal, so a retune of either table fails the test
//! instead of quietly agreeing with a stale copy of itself.
//!
//! Both rungs pay `MSY × rate`, and `MSY = r · K / 4` is linear in `K` — and `K` is now the *same*
//! at every rung — so at a **fixed rung** (same `r`) the whole trade reduces to the patch's basket
//! rate against the wild basket's. That rate is exactly what these tests compare.

use bevy::math::UVec2;
use core_sim::{
    FactionId, FloraConfig, FloraShare, ForagePatch, LaborConfig, RungKey, BUILTIN_LABOR_CONFIG,
    WHOLE_BASKET,
};
use sim_runtime::TerrainType;

/// **Whose ground the hypothetical committed patch belongs to** — nobody's in particular. A
/// completed meter needs an owner (the accrual sets one on first progress) and these quotes are a
/// pure function of ground and config, so the faction cannot matter.
const QUOTE_OWNER: FactionId = FactionId(0);

/// A pinned seed for every per-tile realization sweep (§10).
const SWEEP_SEED: u64 = 0x_F10A_5EED_C011_0010;
/// Side of the tile grid a sweep samples per biome (8×8 = 64 tiles — enough draws to see a crop both
/// dominant and absent across a biome).
const SWEEP_SIDE: u32 = 8;

/// The tile coords a realization sweep samples.
fn sweep_tiles() -> impl Iterator<Item = UVec2> {
    (0..SWEEP_SIDE).flat_map(|x| (0..SWEEP_SIDE).map(move |y| UVec2::new(x, y)))
}

/// `species`' realized share of `terrain`'s basket on tile `coord` — `0.0` where it is absent.
fn realized_share(flora: &FloraConfig, terrain: TerrainType, species: &str, coord: UVec2) -> f32 {
    flora
        .realized_composition(terrain, coord, SWEEP_SEED)
        .iter()
        .find(|entry| entry.species == species)
        .map(|entry| entry.share)
        .unwrap_or(0.0)
}

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
    // The rung is FINISHED here — the quotes below are about a committed patch's basket, not about
    // the build that reaches it. A meter set to a bare `1.0` no longer completes anything now that a
    // job has a size (`docs/plan_unit_costed_work.md`), so this runs the real accrual.
    patch.complete_cultivation(QUOTE_OWNER, &core_sim::LadderConfig::builtin());
    patch.species = species.map(str::to_string);
    patch
}

/// What one unit of this patch's food-bearing land is worth per turn against `composition`:
/// `tile capacity × the patch basket's conversion rate`. The rung's `r` **and** the tile's `K` are
/// the same on both sides of every comparison below, so this product **is** the trade — which is
/// exactly the #433 model: production is constant, only the composition moves.
fn commit_value(patch: &ForagePatch, terrain: TerrainType, composition: &[FloraShare]) -> f32 {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let tile_capacity = labor.forage.capacity_for(terrain);
    tile_capacity
        * core_sim::patch_provisions_per_biomass(patch, composition, &flora, &labor.forage)
}

/// `species`' share of `basket` — `0.0` for a plant the basket does not name, which is exactly what
/// a species blended away to nothing (or not yet arrived) holds.
fn share_of(basket: &[FloraShare], species: &str) -> f32 {
    basket
        .iter()
        .find(|entry| entry.species == species)
        .map_or(0.0, |entry| entry.share)
}

/// **A basket sums to the whole of itself** — the invariant every reweight and every blend owes.
fn assert_basket_is_whole(basket: &[FloraShare], what: &str) {
    let total: f32 = basket.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "{what} must still be a whole basket, not {total}"
    );
}

/// **A basket is in the wire's total order** — share DESC, then species key ASC. Load-bearing beyond
/// presentation: `default_species_for_rung` reads the first entry as the basket's dominant plant, so
/// a blend that came back unsorted would silently change which plant a commitment falls to.
fn assert_basket_is_sorted(basket: &[FloraShare], what: &str) {
    for pair in basket.windows(2) {
        let ordered = pair[0].share > pair[1].share
            || (pair[0].share == pair[1].share && pair[0].species < pair[1].species);
        assert!(
            ordered,
            "{what} must be sorted share DESC then key ASC: {:?} before {:?}",
            pair[0], pair[1]
        );
    }
}

/// The share-weighted food rate of a raw basket — what a **wild** patch on it converts at. Stated
/// here so the assertions below can name the wild baseline without re-deriving `basket_rate`.
fn wild_basket_rate(flora: &FloraConfig, labor: &LaborConfig, composition: &[FloraShare]) -> f32 {
    let wild = ForagePatch::new(UVec2::ZERO, 1.0);
    core_sim::patch_provisions_per_biomass(&wild, composition, flora, &labor.forage)
}

/// **A wild patch is the WHOLE basket, and it is priced as one.** No commitment means the tile's own
/// composition verbatim, and the food rate is that basket's share-weighted average — never the flat
/// `provisions_per_biomass`, which since #433 survives only as the empty-basket fallback.
#[test]
fn an_uncommitted_patch_reads_the_tiles_whole_basket_and_its_average_rate() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    for terrain in TerrainType::VALUES {
        let capacity = labor.forage.capacity_for(terrain);
        let composition = flora.composition(terrain);
        let patch = tended_patch(terrain, None, capacity);
        assert_eq!(
            core_sim::patch_composition(&patch, composition, &flora, &labor.forage).as_ref(),
            composition,
            "{terrain:?}: an uncommitted patch holds the tile's basket verbatim"
        );
        if composition.is_empty() {
            continue; // a barren biome names no plants; the fallback is its own test.
        }
        let expected: f32 = composition
            .iter()
            .map(|entry| entry.share * flora.species[&entry.species].yield_.provisions_per_biomass)
            .sum();
        let actual =
            core_sim::patch_provisions_per_biomass(&patch, composition, &flora, &labor.forage);
        assert!(
            (actual - expected).abs() <= EPSILON,
            "{terrain:?}: a wild patch converts at its own basket's average ({actual} vs {expected})"
        );
    }
}

/// **THE BASKET SLIDES AND SO DOES THE RATE** — both halves of a commitment interpolate on the
/// position, which is `docs/plan_standing_upkeep.md` §2.8 paying out on the plant web's *mix* and
/// not only on its rates.
///
/// A patch a hair up the tended rung is a hair weeded: the favored crop's share has climbed by that
/// fraction of the step and the least abundant volunteers have given up that fraction of what
/// weeding would take from them. Its rate is a hair above wild for the same reason.
///
/// **This reverses the earlier ruling that a basket "cannot be interpolated."** `weeded` is a
/// reweighting of the tile's own mix, so a blend names no plant the ground was not already growing
/// — and leaving the mix stepped left the one term that actually moves across a Sow as a cliff.
#[test]
fn a_build_in_flight_is_part_of_the_way_to_its_weeded_basket_and_its_tended_rate() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let terrain = TerrainType::AlluvialPlain;
    let capacity = labor.forage.capacity_for(terrain);
    let ladder = core_sim::LadderConfig::builtin();

    /// A single work unit banked — a hair up a fifty-unit rung, so a rate that had **stepped** to
    /// the tended value would overshoot the bound below by a factor of the conversion gain.
    const ONE_WORK_UNIT: f32 = 1.0;

    let composition = flora.composition(terrain);
    let mut building = tended_patch(terrain, Some("wild_emmer"), capacity);
    building.set_ladder_position(ONE_WORK_UNIT, &ladder);
    let (_, tended_span) = core_sim::plant_rung_span(core_sim::RungKey::PlantTended, &ladder);
    let share = ONE_WORK_UNIT / tended_span;

    // The mix, species by species, against the two baskets it is between. Asserted as the delta
    // form — `wild + credit × (weeded − wild)` — so a retune of `tended_weeding_gain` moves the
    // fixture with the game rather than failing it.
    let in_flight =
        core_sim::patch_composition(&building, composition, &flora, &labor.forage).into_owned();
    let weeded = core_sim::composition_for_rung(
        &building,
        composition,
        &flora,
        &labor.forage,
        RungKey::PlantTended,
    );
    assert_ne!(
        in_flight, composition,
        "a patch part-way up the rung has already started to weed"
    );
    assert_ne!(
        in_flight.as_slice(),
        weeded.as_ref(),
        "…and has not finished weeding either"
    );
    for entry in &in_flight {
        let from = share_of(composition, &entry.species);
        let to = share_of(&weeded, &entry.species);
        let expected = from + share * (to - from);
        assert!(
            (entry.share - expected).abs() <= EPSILON,
            "{}: {} is not {from} plus {share} of the way to {to}",
            entry.species,
            entry.share
        );
    }
    assert_basket_is_whole(&in_flight, "a basket part-way up a rung");
    assert_basket_is_sorted(&in_flight, "a basket part-way up a rung");

    let wild = wild_basket_rate(&flora, &labor, composition);
    let rate =
        core_sim::patch_provisions_per_biomass(&building, composition, &flora, &labor.forage);
    let mut finished = tended_patch(terrain, Some("wild_emmer"), capacity);
    finished.complete_cultivation(core_sim::FactionId(0), &ladder);
    let tended =
        core_sim::patch_provisions_per_biomass(&finished, composition, &flora, &labor.forage);
    assert!(
        tended > wild,
        "fixture: tending must pay more than wild, or there is no step to be part-way up"
    );

    // The rate is wild plus its share of the step — asserted as the delta form rather than as a
    // literal, so a retune of `tended_conversion_gain` moves the fixture with the game.
    //
    // **The basket and the gain are one interpolation, not two**: the rate is the blended mix
    // priced at the blended gain, and because `basket_rate` is bilinear in the two the product of
    // the blends is not the blend of the products in general. It is here because only the favored
    // term carries a gain and only the favored share moves in step with it — which is what makes
    // this identity a statement about the model rather than about f32 slack.
    let expected = wild + share * (tended - wild);
    assert!(
        (rate - expected).abs() <= EPSILON,
        "a build one work unit in pays wild plus {share} of the step: {rate} against {expected}"
    );
    assert!(
        rate > wild && rate < tended,
        "…which is strictly between the two rungs, never either of them: {wild} < {rate} < {tended}"
    );
}

/// **A reweighted basket is still a WHOLE basket.** Weeding and planting move share *within* the
/// tile's composition; neither may create or destroy any of it. Swept over every biome × plant ×
/// both committed rungs, because a basket that stopped summing to 1 would silently rescale every
/// rate derived from it.
#[test]
fn a_reweighted_basket_still_sums_to_the_whole_basket() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    for terrain in TerrainType::VALUES {
        let capacity = labor.forage.capacity_for(terrain);
        let composition = flora.composition(terrain);
        for share in composition {
            for field in [false, true] {
                let mut patch = tended_patch(terrain, Some(&share.species), capacity);
                // A quote fixture standing at (or below) the Field rung: the position is the
                // whole ladder now, so "is this a Field" is where the position sits.
                patch.set_ladder_position(
                    if field {
                        let (base, width) = core_sim::plant_rung_span(
                            core_sim::RungKey::PlantField,
                            &core_sim::LadderConfig::builtin(),
                        );
                        base + width
                    } else {
                        core_sim::RUNG_UNSTARTED
                    },
                    &core_sim::LadderConfig::builtin(),
                );
                let effective =
                    core_sim::patch_composition(&patch, composition, &flora, &labor.forage);
                let total: f32 = effective.iter().map(|entry| entry.share).sum();
                assert!(
                    (total - WHOLE_BASKET).abs() <= EPSILON,
                    "{terrain:?}/{} (field={field}): the basket summed to {total}",
                    share.species
                );
                assert!(
                    effective.iter().all(|entry| entry.share > 0.0),
                    "{terrain:?}/{} (field={field}): a weeded-out plant must be GONE, not zero",
                    share.species
                );
            }
        }
    }
}

/// **The commit trade is real, and it is now PER-TILE** (§10). On its best country a crop realizes
/// dominant on *some* tiles — where committing beats leaving it wild — while other tiles of the same
/// biome realize a different crop entirely (not every alluvial tile is wheat, which is the whole
/// point of realization). And a crop's best *country* still out-pays its marginal one: wheat's best
/// alluvial realization beats its best rolling-hills realization. If the first were false rung 2
/// would be a rung nobody climbs; if the second were false a river valley and a hillside would be
/// interchangeable.
#[test]
fn committing_is_worth_it_where_a_crop_realizes_dominant_and_a_tile_is_not_always_that_crop() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let crop = "wild_emmer";
    assert!(
        flora.species[crop].cultivation_ceiling.allows_cultivate(),
        "the fixture crop must actually climb"
    );

    // The best realized commit value for `crop` on `terrain`, over a tile sweep — the tile where it
    // realizes most dominant.
    let best_realized = |terrain: TerrainType| -> f32 {
        let capacity = labor.forage.capacity_for(terrain);
        sweep_tiles()
            .map(|coord| {
                let comp = flora.realized_composition(terrain, coord, SWEEP_SEED);
                commit_value(&tended_patch(terrain, Some(crop), capacity), terrain, &comp)
            })
            .fold(f32::MIN, f32::max)
    };
    // The wild value a committed one must beat: the *same tile's* realized basket, gathered whole.
    // Taken as the best over the sweep so both sides are answered on the tile a farmer would pick.
    let best_wild = |terrain: TerrainType| -> f32 {
        let capacity = labor.forage.capacity_for(terrain);
        sweep_tiles()
            .map(|coord| {
                let comp = flora.realized_composition(terrain, coord, SWEEP_SEED);
                commit_value(&tended_patch(terrain, None, capacity), terrain, &comp)
            })
            .fold(f32::MIN, f32::max)
    };

    let dominant = TerrainType::AlluvialPlain;
    let marginal = TerrainType::RollingHills;

    // On its best country, some tile realizes wheat dominant enough to beat the wild basket.
    assert!(
        best_realized(dominant) > best_wild(dominant),
        "{dominant:?}: wheat's best realization must beat the wild basket \
         ({} vs {})",
        best_realized(dominant),
        best_wild(dominant)
    );
    // But not every alluvial tile is wheat — realization spreads the crops, so wheat is *absent* on
    // at least one sampled tile. That absence is what makes "which tile grows wheat" a real question.
    assert!(
        sweep_tiles().any(|coord| realized_share(&flora, dominant, crop, coord) == 0.0),
        "{dominant:?}: some tile must realize a basket WITHOUT wheat — realization is what spreads \
         the crops across a biome's tiles"
    );
    // And the best *country* still out-pays the marginal one: a river valley is not a hillside.
    assert!(
        best_realized(dominant) > best_realized(marginal),
        "wheat's best alluvial realization must out-pay its best rolling-hills realization \
         ({} vs {})",
        best_realized(dominant),
        best_realized(marginal)
    );
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
        let composition = flora.composition(terrain);
        let wild = core_sim::wild_payoff(
            tile,
            capacity,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
        );
        assert!(
            wild > 0.0,
            "{terrain:?}: a forage-bearing tile pays something wild"
        );

        for share in composition {
            for rung in [RungKey::PlantTended, RungKey::PlantField] {
                let payoff = core_sim::commit_payoff(
                    tile,
                    capacity,
                    &share.species,
                    composition,
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

/// **Tending's published ratio carries BOTH gains and the regrowth curve** — stated as its own test
/// because the regrowth term is the exact one the first implementation dropped, and the conversion
/// term is the one #433 added. On a delta tile realizing reeds, the ratio must be exactly
/// `tended_regrowth_gain × (the weeded basket's rate ÷ the wild basket's rate)`, re-derived here from
/// the *config* rather than from the seam under test.
#[test]
fn the_cultivate_ratio_carries_the_regrowth_curve_and_both_tended_gains() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    // The delta tile a reed farmer would pick — reeds' most-dominant realized share over a sweep.
    let terrain = TerrainType::RiverDelta;
    let crop = "reed_and_root";
    let (composition, share) = sweep_tiles()
        .map(|coord| {
            (
                flora.realized_composition(terrain, coord, SWEEP_SEED),
                realized_share(&flora, terrain, crop, coord),
            )
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("the sweep samples at least one tile");
    assert!(
        share > 0.0,
        "reeds must realize on some delta tile — they are the delta's dominant crop"
    );

    // The weeded basket, re-derived from the config: the favored share rises to `min(1, share ×
    // weeding_gain)` off the least abundant first, and only the favored term takes the conversion
    // gain.
    let weeded_share = (share * forage.cultivation.tended_weeding_gain).min(WHOLE_BASKET);
    let taken = weeded_share - share;
    let mut others: Vec<FloraShare> = composition
        .iter()
        .filter(|entry| entry.species != crop)
        .cloned()
        .collect();
    others.sort_by(|a, b| {
        a.share
            .total_cmp(&b.share)
            .then_with(|| a.species.cmp(&b.species))
    });
    let mut owed = taken;
    let mut tended_rate = weeded_share
        * flora.species[crop].yield_.provisions_per_biomass
        * forage.cultivation.tended_conversion_gain;
    for entry in &others {
        let give = entry.share.min(owed.max(0.0));
        owed -= give;
        tended_rate +=
            (entry.share - give) * flora.species[&entry.species].yield_.provisions_per_biomass;
    }
    let wild_rate = wild_basket_rate(&flora, &labor, &composition);
    let expected = forage.cultivation.tended_regrowth_gain * tended_rate / wild_rate;

    let capacity = forage.capacity_for(terrain);
    let tile = bevy::math::UVec2::new(terrain as u32, 0);
    let ratio = core_sim::commit_yield_ratio(
        core_sim::commit_payoff(
            tile,
            capacity,
            crop,
            &composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantTended,
        ),
        core_sim::wild_payoff(
            tile,
            capacity,
            &composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
        ),
    );
    assert!(
        (ratio - expected).abs() <= RATIO_EPSILON * expected,
        "the Cultivate ratio must carry the regrowth curve and both tended gains: {ratio} vs \
         {expected}"
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
