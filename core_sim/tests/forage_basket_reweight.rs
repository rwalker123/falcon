//! **A tile's production is constant across rungs 1–3; a rung changes only WHICH PLANTS it is made
//! of** (issue #433, `docs/plan_flora_roster.md` §4.3).
//!
//! The land owns `K`. No rung below 4 raises it — and, since #433, **none lowers it**: the retired
//! concentration term cut a committed tile's capacity to `share × gain` and *discarded the
//! remainder*, so committing a patch to a marginal plant cost production outright. What a rung does
//! instead is reweight the tile's realized basket — **Tended** weeds (the favored share rises to
//! `min(1, share × tended_weeding_gain)`, taken from the least abundant first), **Field** plants (the
//! favored share is forced to `1.0`) — and every yield rate in every currency is the share-weighted
//! average of that basket, at every rung **including wild**.
//!
//! These tests pin that model from both ends: the arithmetic of the reweight, and the economy that
//! falls out of it. The map-wide measurement at the bottom is the acceptance bar, and it carries a
//! **liveness** assertion beside it — a `basket_rate` that silently fell through to the flat fallback
//! on every tile would score a perfect map-wide ratio of exactly 1.0 from a dead feature.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::TakeSelection;
use core_sim::{
    advance_forage_regrowth, advance_labor_allocation, generate_hydrology, patch_composition,
    patch_provisions_per_biomass, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_world, tile_flora_composition, tile_forage_capacity, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle,
    FloraConfig, FloraShare, FoodModuleTag, ForagePatch, ForageRegistry, GenerationId,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfig, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore,
    MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SourcePriority, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry,
    WellbeingConfigHandle, BUILTIN_LABOR_CONFIG, FODDER, FODDERING_DISCOVERY_ID, WHOLE_BASKET,
};
use sim_runtime::TerrainType;

/// The map every fixture here stands on — the **standard map** the measurement bar is quoted
/// against, so the mechanic tests and the census cannot be reading different worlds.
const STANDARD_SEED: u64 = 119_304_647;

/// Whole-worker head-count on a forage assignment — large enough that `forage_take`'s worker cap
/// never binds, so a take is ceiling-bound and each account is a clean function of the policy.
const FORAGE_WORKERS: u32 = 5000;

/// The quotes and rates here are taken at neutral productivity, as the shipped per-patch forecast is.
const NEUTRAL_MULTIPLIER: f32 = 1.0;

/// **Whose ground a hand-seated fixture patch belongs to.** These tests write finished rungs
/// straight onto the registry — what is under test is a completed rung's basket, not the build that
/// gets there — and a completed meter needs an owner, so the faction is stated rather than left as a
/// bare `FactionId(0)` at each site.
const FIXTURE_OWNER: FactionId = FactionId(0);

/// **The patch's standing crop as a fraction of its capacity** — deliberately **above** Sustain's
/// escapement floor (`K/2`), so a Sustain gather has standing stock to take. At `K/2` exactly a
/// Sustain row is honestly `+0.00` (`docs/plan_harvest_floor.md` §1), which would make every rate
/// these tests measure a division by nothing.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// f32 slack on a share (normalized) or a rate (a chain of ~3 multiplications).
const EPSILON: f32 = 1e-4;

/// **A basket naming nothing** — the one input on which every rate falls back to its flat default
/// (`forage.provisions_per_biomass` for food). It is how the census re-creates the pre-#433 model
/// without re-spelling the MSY curve: the fallback *is* the old flat rate.
const NO_BASKET: &[FloraShare] = &[];

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

/// A hand-built basket, in the wire's total order (share DESC, then species key ASC). Written by
/// hand rather than realized so the weeding arithmetic below is asserted against shares a reader can
/// do in their head.
fn basket(entries: &[(&str, f32)]) -> Vec<FloraShare> {
    let shares: Vec<FloraShare> = entries
        .iter()
        .map(|(species, share)| FloraShare {
            species: (*species).to_string(),
            share: *share,
        })
        .collect();
    let total: f32 = shares.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "a fixture basket must be a whole basket: {total}"
    );
    shares
}

/// A patch standing on one rung, committed to `species`.
fn patch_on_rung(species: Option<&str>, field: bool) -> ForagePatch {
    let mut patch = ForagePatch::new(UVec2::ZERO, 1.0);
    patch.species = species.map(str::to_string);
    if species.is_some() {
        if field {
            patch.complete_field(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
        } else {
            patch.complete_cultivation(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
        }
    }
    patch
}

// ---------------------------------------------------------------------------------------------
// The reweight arithmetic.
// ---------------------------------------------------------------------------------------------

/// **THE HEADLINE INVARIANT: `K` is constant across all three rungs on the same tile.** Wild, tended
/// and Field patches on one tile all carry `tile_forage_capacity`, after the very system that owns
/// the write (`advance_forage_regrowth`, once a turn) has run on each.
///
/// This is the whole of #433 in one assertion. The retired concentration term made the tended and
/// Field arms read `tile_K × min(1, share × gain)`, which on any crop below `1/gain` of the basket
/// was **less** than the wild arm — a rung that cost you production.
#[test]
fn the_tiles_capacity_is_the_same_at_every_rung() {
    let mut app = spawn_standard_world();
    let (tile_entity, coord) = a_patch_tile_growing(&mut app, None);
    let expected = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile_entity).expect("the tile");
        tile_forage_capacity(&labor.forage, ground)
    };
    let crop = default_crop(&app, tile_entity);

    for (rung, field) in [
        ("wild", None),
        ("tended", Some(false)),
        ("field", Some(true)),
    ] {
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("patch exists");
            patch.species = field.map(|_| crop.clone());
            // One position states the rung: the top of the tended rung's span for a tended
            // patch, the top of the Field's for a Field, and zero for a wild stand.
            let ladder = core_sim::LadderConfig::builtin();
            let rung = match field {
                Some(true) => Some(core_sim::RungKey::PlantField),
                Some(false) => Some(core_sim::RungKey::PlantTended),
                None => None,
            };
            let position = rung.map_or(core_sim::RUNG_UNSTARTED, |key| {
                let (base, width) = core_sim::plant_rung_span(key, &ladder);
                base + width
            });
            patch.set_ladder_position(position, &ladder);
            // Deliberately wrong, so the assertion below can only pass if the system rewrote it.
            patch.carrying_capacity = expected * 0.25;
        }
        app.world.run_system_once(advance_forage_regrowth);
        let carried = app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch exists")
            .carrying_capacity;
        // **A RUNG MAY RAISE `K` AND MAY NEVER LOWER IT** (#433). Rungs 1-2 carry the tile's own
        // capacity verbatim; rung 3 raises it, because a sown field is planted densely with the
        // competitors pulled out — the one thing that must never happen is the retired concentration
        // term's *shrink*, which made a commitment cost production.
        let gain = match rung {
            "field" => labor().forage.cultivation.field_capacity_gain,
            _ => 1.0,
        };
        assert!(
            (carried - expected * gain).abs() <= EPSILON * (expected * gain).max(1.0),
            "a {rung} patch carries the tile's K times its rung's own capacity gain ({gain}): \
             {carried} vs {expected}"
        );
        assert!(
            carried >= expected - EPSILON * expected.max(1.0),
            "…and NO rung may leave it below the tile's own: {carried} vs {expected}"
        );
    }
}

/// **Weeding takes from the LEAST ABUNDANT first.** A three-species basket favored toward its
/// dominant member: the smallest member is consumed to nothing before the middle one is touched at
/// all, and the shares still sum to a whole basket.
///
/// Least abundant first is currency-free by design — ranking the weeds by *yield* would mean
/// comparing a food rate against a trade rate, an exchange rate this codebase does not have.
#[test]
fn weeding_takes_the_increase_from_the_least_abundant_species_first() {
    let labor = labor();
    let gain = labor.forage.cultivation.tended_weeding_gain;
    // 0.5 favored → 0.5 × 1.5 = 0.75, so 0.25 must come out of the other two. The smallest (0.20)
    // goes entirely, and the middle one gives up only the remaining 0.05.
    let composition = basket(&[("wild_emmer", 0.5), ("hazel", 0.3), ("oak_mast", 0.2)]);
    let tended = patch_on_rung(Some("wild_emmer"), false);
    let weeded = patch_composition(
        &tended,
        &composition,
        &FloraConfig::builtin(),
        &labor.forage,
    );

    let expected_favored = 0.5 * gain;
    let share_of = |species: &str| {
        weeded
            .iter()
            .find(|entry| entry.species == species)
            .map_or(0.0, |entry| entry.share)
    };
    assert!(
        (share_of("wild_emmer") - expected_favored).abs() <= EPSILON,
        "the favored crop rises to share × gain: {} vs {expected_favored}",
        share_of("wild_emmer")
    );
    assert_eq!(
        share_of("oak_mast"),
        0.0,
        "the LEAST abundant member is consumed first — and to nothing"
    );
    assert!(
        (share_of("hazel") - 0.25).abs() <= EPSILON,
        "the middle member gives up only what the smallest could not cover: {}",
        share_of("hazel")
    );
    assert!(
        !weeded.iter().any(|entry| entry.species == "oak_mast"),
        "a weeded-out plant is GONE from the basket, not present at zero"
    );
    let total: f32 = weeded.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "weeding moves share within the basket; it never creates or destroys any: {total}"
    );
}

/// **Weeding saturates at the whole basket, never past it.** A dominant crop under a gain that would
/// take it over `1.0` lands on exactly `1.0`, and every other member is gone.
#[test]
fn weeding_saturates_at_the_whole_basket() {
    let labor = labor();
    let gain = labor.forage.cultivation.tended_weeding_gain;
    assert!(gain > 1.0, "the fixture needs a gain that can overshoot");
    // 0.8 × 1.5 = 1.2, which must clamp.
    let composition = basket(&[("wild_emmer", 0.8), ("hazel", 0.2)]);
    let weeded = patch_composition(
        &patch_on_rung(Some("wild_emmer"), false),
        &composition,
        &FloraConfig::builtin(),
        &labor.forage,
    );

    assert_eq!(
        weeded.len(),
        1,
        "a saturated basket is one plant: {weeded:?}"
    );
    assert_eq!(weeded[0].species, "wild_emmer");
    assert!(
        (weeded[0].share - WHOLE_BASKET).abs() <= EPSILON,
        "the favored share lands on exactly the whole basket, never past it: {}",
        weeded[0].share
    );
}

/// **A Field's basket is 100% its crop.** You sowed it; there are no volunteers.
#[test]
fn a_field_is_entirely_its_own_crop() {
    let labor = labor();
    let composition = basket(&[("wild_emmer", 0.3), ("hazel", 0.5), ("oak_mast", 0.2)]);
    let planted = patch_composition(
        &patch_on_rung(Some("wild_emmer"), true),
        &composition,
        &FloraConfig::builtin(),
        &labor.forage,
    );

    assert_eq!(planted.len(), 1, "a Field grows one thing: {planted:?}");
    assert_eq!(planted[0].species, "wild_emmer");
    assert!(
        (planted[0].share - WHOLE_BASKET).abs() <= EPSILON,
        "and all of it: {}",
        planted[0].share
    );
}

// ---------------------------------------------------------------------------------------------
// **WHAT WORKING THE GROUND CANNOT CLEAR** — the members a reweight must leave standing.
//
// A navigable hex's basket carries `river_fish` at the `navigable_river_forage_bonus`: that share
// is a *fishery in the channel*, not a plant in the soil. Weeding the reeds on the bank cannot thin
// it, and sowing the field beside it cannot delete it — which is exactly what both rungs did.
// ---------------------------------------------------------------------------------------------

/// The shipped protected member — a *fishery*, and the one every fixture below is built around.
const PROTECTED: &str = "river_fish";

/// The favored crop in the fixtures below: clearable, and `field`-ceiling so it may be committed to
/// at both rungs under test.
const CROP: &str = "wild_emmer";

/// **WEEDING CANNOT TAKE A FISHERY'S SHARE — on the very tile where it is the least abundant
/// member**, which is the basket that would have deleted it. The ranking is by abundance alone, so
/// before the guard the smallest member was consumed first and to nothing: on this basket that is
/// the river's whole fishery, weeded away by a crew tending the bank.
#[test]
fn weeding_never_takes_share_from_a_member_outside_the_worked_ground() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let gain = labor.forage.cultivation.tended_weeding_gain;
    let composition = basket(&[(CROP, 0.5), ("hazel", 0.35), (PROTECTED, 0.15)]);

    // **THE PRECONDITION THIS TEST TURNS ON.** If the fishery is not the *least* abundant member the
    // ranking never reaches it and the fixture proves nothing — it would pass against the defect.
    let least = composition
        .iter()
        .min_by(|a, b| a.share.total_cmp(&b.share))
        .expect("a non-empty basket");
    assert_eq!(
        least.species, PROTECTED,
        "the fixture must put the protected member FIRST in line to be weeded out: {composition:?}"
    );
    assert!(
        !flora.stands_in_worked_ground(PROTECTED),
        "and the roster must actually protect it"
    );

    let weeded = patch_composition(
        &patch_on_rung(Some(CROP), false),
        &composition,
        &flora,
        &labor.forage,
    );
    let share_of = |species: &str| {
        weeded
            .iter()
            .find(|entry| entry.species == species)
            .map_or(0.0, |entry| entry.share)
    };

    assert!(
        (share_of(PROTECTED) - 0.15).abs() <= EPSILON,
        "you cannot weed fish out of a river by tending the reeds on its bank: {weeded:?}"
    );
    // The clearable pool covers the whole ask here (0.35 available against 0.25 owed), so the crop
    // still reaches `share × gain` — the guard protects the fishery without costing the crew
    // anything it could have had.
    assert!(
        (share_of(CROP) - 0.5 * gain).abs() <= EPSILON,
        "the favored crop still rises to share × gain out of the clearable pool: {weeded:?}"
    );
    assert!(
        (share_of("hazel") - 0.10).abs() <= EPSILON,
        "and the whole increase comes out of the one member that stands in the worked ground: \
         {weeded:?}"
    );
    let total: f32 = weeded.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "the shares still make a whole basket: {total}"
    );
}

/// **THE GAIN IS AN ASK, NOT A GUARANTEE.** When the members standing in the worked ground cannot
/// cover what `share × gain` wants, the favored share rises by **only what was there to take** —
/// the protected members are never reached into for the difference.
#[test]
fn weeding_rises_only_as_far_as_the_clearable_members_can_pay_for() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let gain = labor.forage.cultivation.tended_weeding_gain;
    // 0.5 × 1.5 asks for 0.25; only `hazel`'s 0.10 stands in the worked ground.
    let composition = basket(&[(CROP, 0.5), (PROTECTED, 0.4), ("hazel", 0.1)]);
    let asked = 0.5 * gain - 0.5;
    let available = 0.1;
    assert!(
        asked > available,
        "the fixture must ask for more than the clearable pool holds: {asked} vs {available}"
    );

    let weeded = patch_composition(
        &patch_on_rung(Some(CROP), false),
        &composition,
        &flora,
        &labor.forage,
    );
    let share_of = |species: &str| {
        weeded
            .iter()
            .find(|entry| entry.species == species)
            .map_or(0.0, |entry| entry.share)
    };

    assert!(
        (share_of(CROP) - (0.5 + available)).abs() <= EPSILON,
        "the favored share rises by what was clearable and no further: {weeded:?}"
    );
    assert!(
        (share_of(PROTECTED) - 0.4).abs() <= EPSILON,
        "the shortfall is NOT made up out of the fishery: {weeded:?}"
    );
    assert_eq!(
        share_of("hazel"),
        0.0,
        "the clearable member is spent to nothing paying for it"
    );
    let total: f32 = weeded.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "the shares still make a whole basket: {total}"
    );
}

/// **A FIELD TAKES THE CLEARABLE REMAINDER, NOT THE WHOLE BASKET** — the worse half of the defect.
/// The rung-3 reweight forced the crop to `1.0` and every other member to nothing with no ranking
/// involved, so sowing a navigable hex deleted the river's fishery **outright**. A fix to the
/// weeding path alone would have left this standing.
#[test]
fn a_field_leaves_a_member_outside_the_worked_ground_standing() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let composition = basket(&[("hazel", 0.42), (CROP, 0.30), (PROTECTED, 0.28)]);
    assert!(
        composition.iter().any(|entry| entry.species == PROTECTED),
        "the fixture must contain a protected member, or the remainder IS the whole basket"
    );
    assert!(!flora.stands_in_worked_ground(PROTECTED));

    let planted = patch_composition(
        &patch_on_rung(Some(CROP), true),
        &composition,
        &flora,
        &labor.forage,
    );
    let share_of = |species: &str| {
        planted
            .iter()
            .find(|entry| entry.species == species)
            .map_or(0.0, |entry| entry.share)
    };

    assert!(
        (share_of(PROTECTED) - 0.28).abs() <= EPSILON,
        "the fishery keeps its own share through a Sow: {planted:?}"
    );
    // **The crop takes the REMAINDER, and asserting that it is not `1.0` is the whole point** — a
    // Field that still read the whole basket would publish the same 1.0 the defect did.
    assert!(
        (share_of(CROP) - (WHOLE_BASKET - 0.28)).abs() <= EPSILON,
        "the crop takes everything the crew could clear and no more: {planted:?}"
    );
    assert!(
        share_of(CROP) < WHOLE_BASKET - EPSILON,
        "and that is strictly less than the whole basket: {planted:?}"
    );
    assert_eq!(
        planted.len(),
        2,
        "a Field beside a fishery is the crop and the fishery, and nothing else: {planted:?}"
    );
    let total: f32 = planted.iter().map(|entry| entry.share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "the shares still make a whole basket: {total}"
    );
}

/// **INLAND GROUND IS UNCHANGED, AT BOTH RUNGS — and the guard is NOT the cultivation ceiling.**
/// `oak_mast` and `mesquite` are gather-only, and a crew tending the ground genuinely clears them:
/// acorns and mesquite pods come out. A guard keyed on `cultivation_ceiling: wild` would have
/// shielded both and quietly made every Cultivate on woodland and scrub much weaker.
#[test]
fn a_basket_with_nothing_outside_the_worked_ground_reweights_exactly_as_before() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let gain = labor.forage.cultivation.tended_weeding_gain;
    let composition = basket(&[(CROP, 0.5), ("oak_mast", 0.3), ("mesquite", 0.2)]);
    for gathered in ["oak_mast", "mesquite"] {
        assert!(
            !flora.species[gathered]
                .cultivation_ceiling
                .allows_cultivate(),
            "{gathered} must be gather-only, or this says nothing about the ceiling"
        );
        assert!(
            flora.stands_in_worked_ground(gathered),
            "{gathered} grows in soil — the ceiling is not the predicate"
        );
    }

    let weeded = patch_composition(
        &patch_on_rung(Some(CROP), false),
        &composition,
        &flora,
        &labor.forage,
    );
    let share_of = |species: &str| {
        weeded
            .iter()
            .find(|entry| entry.species == species)
            .map_or(0.0, |entry| entry.share)
    };
    assert!(
        (share_of(CROP) - 0.5 * gain).abs() <= EPSILON,
        "the favored crop rises to the full share × gain: {weeded:?}"
    );
    assert!(
        !weeded.iter().any(|entry| entry.species == "mesquite"),
        "the least abundant gathered plant is consumed to nothing and GONE: {weeded:?}"
    );
    assert!(
        (share_of("oak_mast") - 0.25).abs() <= EPSILON,
        "and the middle one gives up only the remainder: {weeded:?}"
    );

    let planted = patch_composition(
        &patch_on_rung(Some(CROP), true),
        &composition,
        &flora,
        &labor.forage,
    );
    assert_eq!(
        planted.len(),
        1,
        "an inland Field is still one plant: {planted:?}"
    );
    assert_eq!(planted[0].species, CROP);
    assert!(
        (planted[0].share - WHOLE_BASKET).abs() <= EPSILON,
        "…holding the whole basket: {}",
        planted[0].share
    );
}

/// **A wild patch pays its OWN basket's average, in every account.**
///
/// Two tiles of one biome realize different baskets (§10), so they pay **different** food rates —
/// which the flat `provisions_per_biomass` could not express in either direction. And a basket that
/// happens to contain a cash crop banks that crop's **material** under `Sustain`, because you
/// gathered the grapes; that account read exactly zero on a wild patch before #433.
#[test]
fn a_wild_patch_pays_its_own_baskets_average_in_every_currency() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let wild = ForagePatch::new(UVec2::ZERO, 1.0);

    // Two tiles of one biome, different realizations, different food rates.
    let terrain = TerrainType::MixedWoodland;
    let rates: Vec<f32> = (0..64)
        .map(|i| {
            let composition =
                flora.realized_composition(terrain, UVec2::new(i % 8, i / 8), STANDARD_SEED);
            patch_provisions_per_biomass(&wild, &composition, &flora, &labor.forage)
        })
        .collect();
    let min = rates.iter().copied().fold(f32::MAX, f32::min);
    let max = rates.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        max - min > EPSILON,
        "two tiles of one biome must pay different wild food rates — the flat rate could not: \
         {min} vs {max}"
    );

    // A basket holding a cash crop banks that crop's material under Sustain — you gathered them.
    let mut app = spawn_standard_world();
    let (tile_entity, coord) = a_patch_tile_growing(&mut app, Some("grapevine"));
    seat_patch(&mut app, coord, None);
    let band = spawn_forager(&mut app, tile_entity, coord, 0.5);
    app.world.run_system_once(advance_labor_allocation);
    assert!(
        band_materials(&app, band) > 0.0,
        "a Sustain gather of wild ground holding a vine must bank grapes — you gathered them"
    );
}

/// **The conversion gain rides the FAVORED species' term ONLY.** On one tile with one gain, favoring
/// the basket's dominant plant moves the food rate far more than favoring a marginal one.
///
/// A blanket multiplier on the whole basket would make both commitments pay ~`gain`, which erases
/// the crop choice — the exact failure this shape exists to prevent.
#[test]
fn the_conversion_gain_is_on_the_favored_term_only() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let gain = labor.forage.cultivation.tended_conversion_gain;
    assert!(gain > 1.0, "the fixture needs a conversion gain to see");

    // A basket with a clear dominant and a clear marginal, both real food plants.
    let composition = basket(&[("wild_emmer", 0.7), ("oak_mast", 0.3)]);
    let wild = ForagePatch::new(UVec2::ZERO, 1.0);
    let wild_rate = patch_provisions_per_biomass(&wild, &composition, &flora, &labor.forage);

    let rate_favoring = |species: &str| {
        patch_provisions_per_biomass(
            &patch_on_rung(Some(species), false),
            &composition,
            &flora,
            &labor.forage,
        )
    };
    let dominant = rate_favoring("wild_emmer");
    let marginal = rate_favoring("oak_mast");

    assert!(
        dominant > wild_rate && marginal > wild_rate,
        "either commitment beats gathering the whole basket: {dominant} / {marginal} vs {wild_rate}"
    );
    assert!(
        dominant > marginal,
        "favoring the dominant plant must pay MORE than favoring the marginal one on the same \
         tile: {dominant} vs {marginal}"
    );
    // The blanket-multiplier failure mode, named: if the gain applied to the whole basket, both
    // commitments would land on `gain × wild` and be indistinguishable.
    let blanket = gain * wild_rate;
    assert!(
        marginal < blanket - EPSILON,
        "favoring a marginal plant must NOT pay the full gain — that would be a blanket \
         multiplier: {marginal} vs {blanket}"
    );
}

/// **THE ONE MATERIAL RULE AT EVERY DRAWN-DOWN RUNG** (#433, restated on the account that survived
/// arc #527).
///
/// There is exactly one expression — `take × the patch basket's material rows` — at rung 1 and rung
/// 2 alike, and **no factor of any kind** rides the depth of the draw
/// (`docs/plan_harvest_floor.md` §4; the retired `market.trade_goods_multiplier` used to pay one
/// depth 4×). Landing on that expression exactly is also the "no double credit" pin.
#[test]
fn a_deep_gather_banks_the_baskets_materials_at_both_drawn_down_rungs() {
    let labor = labor();
    let flora = FloraConfig::builtin();

    for crop in [None, Some("grapevine")] {
        let mut app = spawn_standard_world();
        let (tile_entity, coord) = a_patch_tile_growing(&mut app, Some("grapevine"));
        seat_patch(&mut app, coord, crop);
        let before = standing_crop(&app, coord);
        let band = spawn_forager(&mut app, tile_entity, coord, 0.15);
        app.world.run_system_once(advance_labor_allocation);

        let take = before - standing_crop(&app, coord);
        assert!(take > 0.0, "a Deplete gather draws the stand down");
        let composition = tile_composition(&app, coord);
        let patch = app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch exists")
            .clone();
        // The sim's own per-species material rows for this patch, summed — the expression the
        // credit site puts `take` through.
        let bare: f32 =
            core_sim::patch_material_yields(&patch, &composition, &flora, &labor.forage)
                .iter()
                .map(|row| row.per_biomass * take * NEUTRAL_MULTIPLIER)
                .sum();
        assert!(bare > 0.0, "the fixture's basket must carry a material row");
        let banked = band_materials(&app, band);
        assert!(
            (banked - bare).abs() <= EPSILON * bare.max(1.0),
            "{crop:?}: a deep gather banks the basket's rows on its take: {banked} vs {bare}"
        );
        assert!(
            banked < 2.0 * bare - EPSILON,
            "{crop:?}: and is credited ONCE — {banked} against a double credit of {bare}"
        );
    }
}

/// **THE PUBLISHED WILD MATERIAL RATE IS WHAT THE GATHER BANKS** (arc #527) — the rung-1 twin of the
/// wolf guard, and the close on the third instance of one mistake.
///
/// `ForagePatchState.tradePerBiomass` was `(deprecated)` with **no replacement**: the crop picker's
/// quotes cover a *commitment* at rungs 2 and 3, and a **wild gather had nothing at all**. A tile
/// whose basket carries a cash crop read food-and-fodder-only while the turn banked its fibre.
///
/// **The mixed basket is the case that matters, and the fixture insists on it.** A herd is one
/// species; a patch is a basket, and two species that both give fibre must sum into **one** fibre
/// rate — which is what a rate means, and what `LocalStore::material_total` sums the same way. A
/// per-species rate that forgot to merge would pass a single-species fixture and fail here. Their
/// **characteristic readings** are never merged: those ride the batches the take creates, which is
/// why this asserts the *total* and `materials.rs` asserts the *batches*.
///
/// Asserted against what the band's store actually **holds** after a real turn, not a re-derivation.
#[test]
fn a_wild_gathers_published_material_rate_is_what_the_band_banks() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let mut app = spawn_standard_world();
    let (tile_entity, coord) = a_patch_tile_growing(&mut app, Some("cotton"));
    seat_patch(&mut app, coord, None); // WILD — no commitment, which is the rung under test

    let composition = tile_composition(&app, coord);
    // **The merge case, insisted on rather than hoped for**: at least one material must be named by
    // two different species of this basket, or the assertion below cannot distinguish a merged rate
    // from a per-species one.
    let mut contributors: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for entry in &composition {
        for row in &flora.species[&entry.species].yield_.materials {
            *contributors.entry(row.material.as_str()).or_insert(0) += 1;
        }
    }
    let shared = contributors
        .iter()
        .find(|(_, count)| **count > 1)
        .map(|(material, _)| (*material).to_string());
    assert!(
        shared.is_some(),
        "the fixture tile's basket must name one material from TWO species, or the merge is \
         untested: {contributors:?} over {:?}",
        composition
            .iter()
            .map(|entry| entry.species.as_str())
            .collect::<Vec<_>>()
    );

    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile_entity, coord, 0.15);
    app.world.run_system_once(advance_labor_allocation);
    let take = before - standing_crop(&app, coord);
    assert!(take > 0.0, "the gather must draw the stand down");

    // **THE PUBLISHED RATE**, through the very seam the capture publishes verbatim — the patch's
    // decomposed rows merged by material id, per unit of biomass.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let rows = core_sim::patch_material_yields(&patch, &composition, &flora, &labor.forage);
    let published = core_sim::material_yield_totals(&rows, 1.0, NEUTRAL_MULTIPLIER);
    assert!(
        !published.is_empty(),
        "a basket carrying a cash crop must publish a material rate"
    );

    let cohort = app
        .world
        .get::<core_sim::PopulationCohort>(band)
        .expect("the foraging band still exists");
    for rate in &published {
        let held = cohort.stores.material_total(&rate.material).to_f32();
        let expected = rate.amount * take * NEUTRAL_MULTIPLIER;
        assert!(
            (held - expected).abs() <= EPSILON * expected.max(1.0),
            "the published {} rate composes to {expected} over a {take}-biomass gather, and the \
             band holds {held}",
            rate.material
        );
        assert!(rate.amount > 0.0, "a published rate is a rate that pays");
    }
    // …and the shared material really did come out as ONE row, not two.
    let shared = shared.expect("checked above");
    assert_eq!(
        published
            .iter()
            .filter(|rate| rate.material == shared)
            .count(),
        1,
        "two species giving {shared} must merge into ONE rate: {published:?}"
    );
}

/// **A WILD patch's fodder credit is gated on Foddering; a COMMITTED one is not.**
///
/// The invariant reaches fodder too — a wild tile realizing `hay_grass` pays hay on any harvest — but
/// crediting it to a faction with nowhere to put it hands out animal feed nobody bid for. So the wild
/// credit reads the same 2007 capability the pen's own draw does, at the **credit site**, while
/// committing a patch to `hay_grass` *is* the bid and needs no capability at all.
#[test]
fn wild_fodder_is_gated_on_foddering_and_a_committed_hay_patch_is_not() {
    let fodder_credited = |crop: Option<&str>, knows_foddering: bool| -> f32 {
        let mut app = spawn_standard_world();
        let (tile_entity, coord) = a_patch_tile_growing(&mut app, Some("hay_grass"));
        seat_patch(&mut app, coord, crop);
        if knows_foddering {
            app.world
                .resource_mut::<DiscoveryProgressLedger>()
                .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
        }
        let band = spawn_forager(&mut app, tile_entity, coord, 0.5);
        app.world.run_system_once(advance_labor_allocation);
        app.world
            .get::<PopulationCohort>(band)
            .expect("the band forages")
            .stores
            .get(FODDER)
            .to_f32()
    };

    let wild_with = fodder_credited(None, true);
    let wild_without = fodder_credited(None, false);
    assert!(
        wild_with > 0.0,
        "a hay-bearing wild basket pays hay to a faction that knows Foddering: {wild_with}"
    );
    assert_eq!(
        wild_without, 0.0,
        "and nothing to one that does not — the credit is bid for at the consumer"
    );

    let committed_with = fodder_credited(Some("hay_grass"), true);
    let committed_without = fodder_credited(Some("hay_grass"), false);
    assert!(
        committed_with > 0.0 && committed_without > 0.0,
        "committing a patch to hay IS the bid, so rung 2 is ungated either way: \
         {committed_with} / {committed_without}"
    );
    assert!(
        (committed_with - committed_without).abs() <= EPSILON,
        "and the capability changes nothing on a committed patch: \
         {committed_with} vs {committed_without}"
    );
}

/// **THE ROW REPORTS THE CREDIT, GATE AND ALL** (issue #449) — `SourceYield::fodder` is the number
/// the band's `FODDER` store was actually moved by, not a second derivation of it.
///
/// This is the invariant the readout exists to keep, and the **gate** is what makes it a real test
/// rather than a restatement: a row that recomputed `tended_take_fodder` would publish a positive
/// hay rate to a faction that has not learned Foddering and was therefore paid **nothing** — a
/// compact readout stating income the band never received. Swept over the same four cases the gate
/// itself is pinned on, so the equality has to hold on both sides of it.
#[test]
fn the_published_fodder_is_the_fodder_the_band_was_actually_credited() {
    let credited_and_published = |crop: Option<&str>, knows_foddering: bool| -> (f32, f32) {
        let mut app = spawn_standard_world();
        let (tile_entity, coord) = a_patch_tile_growing(&mut app, Some("hay_grass"));
        seat_patch(&mut app, coord, crop);
        if knows_foddering {
            app.world
                .resource_mut::<DiscoveryProgressLedger>()
                .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
        }
        let band = spawn_forager(&mut app, tile_entity, coord, 0.5);
        app.world.run_system_once(advance_labor_allocation);
        let credited = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band forages")
            .stores
            .get(FODDER)
            .to_f32();
        (credited, published_fodder(&app, band))
    };

    let mut saw_a_live_credit = false;
    for crop in [None, Some("hay_grass")] {
        for knows_foddering in [false, true] {
            let (credited, published) = credited_and_published(crop, knows_foddering);
            saw_a_live_credit |= credited > 0.0;
            assert!(
                (published - credited).abs() <= EPSILON,
                "{crop:?} / foddering={knows_foddering}: the row must state the credit, \
                 not recompute it: published {published} vs credited {credited}"
            );
        }
    }
    // **Liveness**: an equality between two zeros is satisfied by a feature that never fires, and a
    // gate that refused everything would pass the sweep above unnoticed.
    assert!(
        saw_a_live_credit,
        "at least one case must credit real fodder, or the equality above is vacuous"
    );
}

/// **The WIRE publishes the patch's EFFECTIVE basket, not the tile's raw one.** `ForagePatchState`
/// keeps its shape — no schema change — but the numbers on it now move with the rung, so the tile
/// card can show a commitment taking hold: a tended patch's basket visibly collapses toward its crop,
/// and a Field publishes a single 100% entry. Zero-share entries are filtered out.
///
/// Asserted against the shipped snapshot rather than the seam, because the seam already has its own
/// tests above and what is at stake here is the *published artifact*.
#[test]
fn the_published_composition_is_the_patchs_effective_basket() {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_seed = STANDARD_SEED;
    app.world.insert_resource(config);
    app.update(); // the real Startup chain: worldgen, patch seeding, one capture.

    let (tile_entity, coord) = a_patch_tile_growing(&mut app, None);
    let crop = default_crop(&app, tile_entity);
    let tile_basket = tile_composition(&app, coord);
    assert!(
        tile_basket.len() > 1,
        "the fixture tile must grow more than one plant for a reweight to be visible"
    );

    let published = |app: &mut App| -> Vec<(String, f32)> {
        app.world
            .run_system_once(core_sim::recapture_snapshot_in_place);
        let snapshot = app
            .world
            .resource::<core_sim::SnapshotHistory>()
            .last_snapshot()
            .map(|snapshot| (*snapshot).clone())
            .expect("a capture after the update");
        snapshot
            .forage_patches
            .iter()
            .find(|patch| patch.x == coord.x && patch.y == coord.y)
            .expect("the fixture patch is on the wire")
            .composition
            .iter()
            .map(|share| (share.species.clone(), share.share))
            .collect()
    };

    // Wild: the tile's own basket, verbatim.
    let wild = published(&mut app);
    assert_eq!(
        wild.len(),
        tile_basket.len(),
        "a wild patch publishes the tile's whole basket: {wild:?}"
    );

    // Tended: the favored crop's share has risen, and nothing was invented.
    seat_patch(&mut app, coord, Some(&crop));
    let tended = published(&mut app);
    let share_in = |rows: &[(String, f32)], species: &str| {
        rows.iter()
            .find(|(key, _)| key == species)
            .map_or(0.0, |(_, share)| *share)
    };
    assert!(
        share_in(&tended, &crop) > share_in(&wild, &crop) + EPSILON,
        "a tended patch's published basket must collapse toward its crop: {tended:?} vs {wild:?}"
    );
    assert!(
        tended.iter().all(|(_, share)| *share > 0.0),
        "a weeded-out plant is filtered off the wire, not published at zero: {tended:?}"
    );
    let total: f32 = tended.iter().map(|(_, share)| *share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "the published basket is still a whole basket: {total}"
    );

    // Field: one entry, all of it.
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch exists");
        patch.complete_field(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
    }
    let field = published(&mut app);
    // **A Field is the crop plus whatever the crew could not clear** — and on this fixture tile
    // that is not academic: the stand is a navigable hex, so its basket carries the channel's own
    // fishery (`river_fish`, the `navigable_river_forage_bonus` term). Sowing the valley cannot
    // drain the river, so the crop takes the **clearable remainder** and the fishery stands. Before
    // that guard this row published exactly one plant at `1.0` and the fishery was gone.
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let standing_outside: f32 = field
        .iter()
        .filter(|(species, _)| *species != crop)
        .map(|(species, share)| {
            assert!(
                !flora.stands_in_worked_ground(species),
                "a Field's only volunteers are the members working the ground cannot clear: \
                 {field:?}"
            );
            *share
        })
        .sum();
    assert!(
        standing_outside > EPSILON,
        "this fixture tile must carry a member outside the worked ground, or the remainder IS the \
         whole basket and this asserts nothing: {field:?}"
    );
    let crop_share = field
        .iter()
        .find(|(species, _)| *species == crop)
        .map_or(0.0, |(_, share)| *share);
    assert!(
        (crop_share - (WHOLE_BASKET - standing_outside)).abs() <= EPSILON,
        "the sown crop takes the clearable remainder, not the whole basket: {field:?}"
    );
    let total: f32 = field.iter().map(|(_, share)| *share).sum();
    assert!(
        (total - WHOLE_BASKET).abs() <= EPSILON,
        "…and the published basket is still a whole basket: {total}"
    );
}

/// **EACH RUNG'S PAYOFF FUNCTION PROJECTS TO ITS OWN RUNG.** A "what would a Field here pay" quote
/// reads the **planted** basket with **no** rung-2 conversion gain, whether or not the patch it is
/// asked about happens to be tended — so the wire's `fieldYield` on a tended patch is exactly the
/// number the crop picker quotes for Sowing that crop on that tile.
///
/// This is the same principle `hypothetical_patch`'s per-rung standing crop and
/// `ceiling_cultivate`/`ceiling_sow` already encode: **two investment rungs on one branch must never
/// share a number.** Letting the Field quote inherit rung 2's basket made `fieldYield` on a tended
/// patch overstate by roughly `tended_conversion_gain` — a published quote disagreeing with the
/// payout, which is the failure §4.3's history exists to warn about.
///
/// Asserted on the **shipped snapshot**, because `fieldYield` on a live tended patch is the one place
/// the defect was actually reachable.
#[test]
fn the_published_field_yield_never_inherits_the_tended_rungs_basket() {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_seed = STANDARD_SEED;
    app.world.insert_resource(config);
    app.update();

    let (tile_entity, coord) = a_patch_tile_growing(&mut app, None);
    let crop = default_crop(&app, tile_entity);

    /// The patch row the wire publishes for `coord`, recaptured from the world as it stands.
    fn row(app: &mut App, coord: UVec2) -> sim_runtime::ForagePatchState {
        app.world
            .run_system_once(core_sim::recapture_snapshot_in_place);
        app.world
            .resource::<core_sim::SnapshotHistory>()
            .last_snapshot()
            .expect("a capture")
            .forage_patches
            .iter()
            .find(|patch| patch.x == coord.x && patch.y == coord.y)
            .expect("the fixture patch is on the wire")
            .clone()
    }

    // The same tile, the same crop, the same standing crop (its full `K`, so the quote and the
    // picker's full-standing-crop payoff are answered at the same biomass) — one patch on rung 2,
    // one on rung 3. A **Field** quote must not be able to tell them apart.
    let seat = |app: &mut App, field: bool| {
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("patch exists");
            patch.species = Some(crop.clone());
            patch.complete_cultivation(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
            if field {
                patch.complete_field(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
            }
        }
        // **The sim's own capacity write, because a rung may RAISE `K`** — rung 3 does, so a fixture
        // that seated the rung by hand and left the stored capacity behind would be comparing a Field
        // standing on rung-2 land.
        app.world.run_system_once(advance_forage_regrowth);
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch exists");
        patch.biomass = patch.carrying_capacity;
    };

    seat(&mut app, false);
    let tended = row(&mut app, coord);
    seat(&mut app, true);
    let as_field = row(&mut app, coord);

    assert!(
        as_field.field_yield > 0.0,
        "the fixture must quote a real Field"
    );
    assert!(
        (tended.field_yield - as_field.field_yield).abs() <= EPSILON * as_field.field_yield,
        "fieldYield on a tended patch must be what the Field it would become actually pays: \
         {} vs {}",
        tended.field_yield,
        as_field.field_yield
    );

    // …and it IS the crop picker's own Sow payoff for that crop on this tile — quote and payout are
    // one number, published from two places on the same row.
    let picker = tended
        .composition
        .iter()
        .find(|share| share.species == crop)
        .expect("the committed crop is on its own patch's basket")
        .sow_payoff;
    assert!(
        (tended.field_yield - picker).abs() <= EPSILON * picker.max(1.0),
        "the published fieldYield on a tended patch must BE the picker's sowPayoff: {} vs {picker}",
        tended.field_yield
    );

    // And the converse, so the two rungs cannot collapse onto one number from the other side: the
    // rung-2 quote on the same row is the picker's own **Cultivate** payoff (MSY plateaus above `K/2`,
    // so the quote's operating point and this patch's full standing crop skim the same amount), and
    // it is a *different* number from the rung-3 one.
    let cultivate = tended
        .composition
        .iter()
        .find(|share| share.species == crop)
        .expect("the committed crop is on its own patch's basket")
        .cultivate_payoff;
    assert!(
        (tended.tended_yield - cultivate).abs() <= EPSILON * cultivate.max(1.0),
        "tendedYield must BE the picker's cultivatePayoff: {} vs {cultivate}",
        tended.tended_yield
    );
    assert!(
        (tended.tended_yield - tended.field_yield).abs() > EPSILON,
        "the two rungs must not share a number: tended {} vs field {}",
        tended.tended_yield,
        tended.field_yield
    );
}

/// **The rung-keyed seam itself** — which is what makes the fodder and trade Field quotes immune to
/// the same leak without each needing its own guard. `field_provisions`, `field_fodder` and
/// `field_trade_goods` are one shape routed through three components of one yield vector, and all
/// three derive their basket from this seam, so pinning it pins all three at once.
///
/// The claim: `composition_for_rung(.., PlantField)` is the **planted** basket and
/// `(.., PlantTended)` the **weeded** one, for a patch standing on *any* rung — the answer depends on
/// the rung asked about, never on the rung the patch happens to be on.
#[test]
fn the_composition_seam_answers_the_rung_it_is_asked_about_not_the_one_the_patch_stands_on() {
    let labor = labor();
    let composition = basket(&[("wild_emmer", 0.5), ("hazel", 0.3), ("oak_mast", 0.2)]);

    let mut wild = ForagePatch::new(UVec2::ZERO, 1.0);
    wild.species = Some("wild_emmer".to_string());
    let mut tended = wild.clone();
    tended.complete_cultivation(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
    let mut field = wild.clone();
    field.complete_field(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());

    for (name, patch) in [("building", &wild), ("tended", &tended), ("field", &field)] {
        let planted = core_sim::composition_for_rung(
            patch,
            &composition,
            &FloraConfig::builtin(),
            &labor.forage,
            core_sim::RungKey::PlantField,
        );
        assert_eq!(
            planted.len(),
            1,
            "{name}: the Field reading is one plant whatever rung the patch stands on: {planted:?}"
        );
        assert!((planted[0].share - WHOLE_BASKET).abs() <= EPSILON);

        let weeded = core_sim::composition_for_rung(
            patch,
            &composition,
            &FloraConfig::builtin(),
            &labor.forage,
            core_sim::RungKey::PlantTended,
        );
        let favored = weeded
            .iter()
            .find(|entry| entry.species == "wild_emmer")
            .map_or(0.0, |entry| entry.share);
        assert!(
            (favored - 0.5 * labor.forage.cultivation.tended_weeding_gain).abs() <= EPSILON,
            "{name}: the tended reading is the weeded basket whatever rung the patch stands on: \
             {weeded:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The measurement (§9's acceptance bar) — and the liveness assertion beside it.
// ---------------------------------------------------------------------------------------------

/// **The wild-gather balance bar, map-wide, plus the liveness that keeps it honest.**
///
/// The whole map's wild Sustain income under the **old** flat rate (`Σ MSY × provisions_per_biomass`)
/// against the **new** basket average (`Σ MSY × basket_rate`) must land within ±5%: the basket is an
/// *average of the same plants* that always summed to the tile's capacity, so if it moved the
/// map-wide total materially, the averages are wrong rather than the balance.
///
/// **The bar alone passes when the feature is dead.** A `basket_rate` that fell through to the flat
/// fallback on every tile would produce a map-wide ratio of exactly `1.0` — a perfect score from a
/// broken feature. So the spread must also be **non-degenerate**: strictly more than half the
/// food-bearing tiles differ from the flat rate by more than 1%, and `max/min` across the map is well
/// above 1. Both are asserted here, in the same test, for exactly that reason.
///
/// Run with `--nocapture` for the census the PR quotes.
#[test]
fn the_map_wide_wild_income_is_unmoved_and_the_basket_is_alive() {
    let mut app = spawn_standard_world();
    let labor = app.world.resource::<LaborConfigHandle>().get().clone();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let flat = labor.forage.provisions_per_biomass;
    let wild = ForagePatch::new(UVec2::ZERO, 1.0);

    let mut old_food = 0.0_f64;
    let mut new_food = 0.0_f64;
    let mut ratios: Vec<f32> = Vec::new();

    let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
    let tiles: Vec<Tile> = query
        .iter(&app.world)
        .map(|(tile, _)| tile.clone())
        .collect();
    for tile in &tiles {
        let capacity = tile_forage_capacity(&labor.forage, tile);
        if capacity <= 0.0 {
            continue;
        }
        let composition = tile_flora_composition(&flora, &labor.forage, tile, map_seed);
        // **Both sides through the sim's own payoff function**, one turn's wild Sustain skim in
        // provisions: the NEW side on the tile's realized basket, the OLD side on an **empty** one —
        // because the empty basket is precisely where `basket_rate` falls back to the flat
        // `provisions_per_biomass`, so "the old flat model" needs no re-derivation of the MSY curve.
        let new_tile = f64::from(core_sim::wild_payoff(
            tile.position,
            capacity,
            &composition,
            &flora,
            &labor.forage,
            NEUTRAL_MULTIPLIER,
        ));
        let old_tile = f64::from(core_sim::wild_payoff(
            tile.position,
            capacity,
            NO_BASKET,
            &flora,
            &labor.forage,
            NEUTRAL_MULTIPLIER,
        ));
        if old_tile <= 0.0 {
            continue; // a collapsed/zero-capacity stand offers no skim to compare.
        }
        let food_rate = patch_provisions_per_biomass(&wild, &composition, &flora, &labor.forage);
        old_food += old_tile;
        new_food += new_tile;
        ratios.push(food_rate / flat);
    }

    assert!(
        ratios.len() > 1000,
        "the standard map must carry a real census of food-bearing tiles: {}",
        ratios.len()
    );
    let food_ratio = new_food / old_food;
    let min = ratios.iter().copied().fold(f32::MAX, f32::min);
    let max = ratios.iter().copied().fold(f32::MIN, f32::max);
    let mean = ratios.iter().copied().sum::<f32>() / ratios.len() as f32;
    // **A tile whose realization is cash crops alone genuinely pays NO food wild** — cotton is not
    // dinner. That is a real consequence of pricing a tile by what grows on it, not a degenerate
    // reading, but it makes `max/min` meaningless, so the spread is reported over the tiles that feed
    // anyone at all and the barren-of-food count is reported beside it.
    let no_food = ratios.iter().filter(|ratio| **ratio <= 0.0).count();
    let min_feeding = ratios
        .iter()
        .copied()
        .filter(|ratio| *ratio > 0.0)
        .fold(f32::MAX, f32::min);
    let outside_10 = ratios
        .iter()
        .filter(|ratio| (**ratio - 1.0).abs() > 0.10)
        .count();
    let live = ratios
        .iter()
        .filter(|ratio| (**ratio - 1.0).abs() > LIVENESS_DEVIATION)
        .count();

    println!(
        "--- #433 wild-gather census, standard map (earthlike 80x52, seed {STANDARD_SEED}) ---"
    );
    println!("food-bearing tiles:      {}", ratios.len());
    println!("map-wide food/turn old:  {old_food:.3}");
    println!("map-wide food/turn new:  {new_food:.3}");
    println!("food ratio (new/old):    {food_ratio:.5}");
    println!(
        "basket_rate / flat  min: {min:.5}  max: {max:.5}  mean: {mean:.5}  \
         (min over feeding tiles: {min_feeding:.5})"
    );
    println!(
        "outside +/-10%:          {outside_10} ({:.1}%)",
        100.0 * outside_10 as f32 / ratios.len() as f32
    );
    println!("tiles paying no food:    {no_food} (cash-crop-only realizations)");
    println!(
        "LIVENESS differing >1%:  {live} ({:.1}%)   max/min over feeding tiles: {:.3}",
        100.0 * live as f32 / ratios.len() as f32,
        max / min_feeding
    );

    assert!(
        (food_ratio - 1.0).abs() <= MAP_WIDE_FOOD_TOLERANCE,
        "map-wide wild food moved by more than the bar: ratio {food_ratio}"
    );
    // **Liveness, so the bar cannot be met by a dead feature.**
    assert!(
        live * 2 > ratios.len(),
        "the basket is degenerate — only {live} of {} tiles differ from the flat rate by >1%, so a \
         perfect map-wide ratio would mean the rate fell through to the fallback everywhere",
        ratios.len()
    );
    assert!(
        max / min_feeding > LIVENESS_SPREAD,
        "the map-wide spread of basket_rate is flat ({min_feeding}..{max}) — the tiles are not \
         being priced by what grows on them"
    );
}

/// **The map-wide food bar**: ±5%. The basket is an average of the very plants that always summed to
/// the tile's capacity, so a larger move would mean the averages are wrong, not that the balance is.
const MAP_WIDE_FOOD_TOLERANCE: f64 = 0.05;

/// **The per-tile deviation that counts a tile as ALIVE** — 1%. Below it a tile is indistinguishable
/// from the flat fallback, which is exactly the dead-feature reading the liveness assertion exists to
/// catch.
const LIVENESS_DEVIATION: f32 = 0.01;

/// **The map-wide `max/min` spread a live basket must clear.** Well above `1`: the roster's food
/// rates run from `arctic_greens` 0.040 to `wild_emmer` 0.080, so an honestly-averaged map cannot be
/// flat.
const LIVENESS_SPREAD: f32 = 1.3;

// ---------------------------------------------------------------------------------------------
// Fixture.
// ---------------------------------------------------------------------------------------------

/// The standard map, with hydrology run — the world the measurement is quoted against, and the one
/// the mechanic tests stand on so a fixture tile is a tile the census also counted.
fn spawn_standard_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = STANDARD_SEED;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    // The rung-3 site rule reads fresh water, and rivers/deltas are hydrology's — a fixture that
    // skips this measures a map with no river valleys on it at all.
    generate_hydrology(&mut app.world);

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// The richest in-season patch tile whose **realized** basket grows `species` (any, when `None`).
///
/// The species filter is the honest model rather than a convenience: a rung-2 commitment weeds the
/// tile's realized basket toward its crop, so a crop the tile does not grow cannot be weeded toward
/// at all. Capacity is the tiebreak because the take is what these tests measure — on thin ground a
/// real harvest rounds away at the integer trade stockpile.
fn a_patch_tile_growing(app: &mut App, species: Option<&str>) -> (bevy::prelude::Entity, UVec2) {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let coord = {
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(_, module)| module.seasonal_weight > 0.0)
            .filter(|(tile, _)| {
                species.is_none_or(|species| {
                    tile_flora_composition(&flora, &labor.forage, tile, map_seed)
                        .iter()
                        .any(|entry| entry.species == species)
                })
            })
            .filter_map(|(tile, _)| registry.patch(tile.position))
            .max_by(|a, b| {
                a.carrying_capacity
                    .total_cmp(&b.carrying_capacity)
                    .then_with(|| b.tile.y.cmp(&a.tile.y))
                    .then_with(|| b.tile.x.cmp(&a.tile.x))
            })
            .unwrap_or_else(|| {
                panic!("the standard map must carry an in-season patch growing {species:?}")
            })
            .tile
    };
    drop(labor);
    drop(flora);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// Seat the patch at `coord` at its MSY operating point, either wild (`None`) or as a **completed
/// Tended Patch** committed to `crop`. Written straight onto the registry: what is under test is a
/// finished rung's harvest routing, not the build that gets there.
fn seat_patch(app: &mut App, coord: UVec2, crop: Option<&str>) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = crop.map(str::to_string);
    if crop.is_some() {
        patch.complete_cultivation(FIXTURE_OWNER, &core_sim::LadderConfig::builtin());
    }
    patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
}

/// The default crop the tended rung would auto-pick on this tile — the same
/// `default_species_for_rung` answer the labor arm reaches.
fn default_crop(app: &App, tile_entity: bevy::prelude::Entity) -> String {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let ground = app.world.get::<Tile>(tile_entity).expect("the tile");
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    core_sim::default_species_for_rung(&composition, &flora, core_sim::RungKey::PlantTended)
        .expect("the fixture tile grows something the tended rung can commit to")
}

/// What is growing on the tile at `coord`, through the one `tile_flora_composition` seam.
fn tile_composition(app: &App, coord: UVec2) -> Vec<FloraShare> {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(entity).expect("the tile");
    tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
}

fn standing_crop(app: &App, coord: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .biomass
}

/// **Every material on the band's own store, summed** — what a harvest banks beyond food and hay.
/// A bare total is the right shape here: these tests ask *how much stuff* a take produced, and which
/// material it is (and what its characteristics read) is `materials.rs`'s subject.
fn band_materials(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<core_sim::PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .materials()
        .flat_map(|(_, batches)| batches.values())
        .map(|batch| batch.amount.to_f32())
        .sum()
}

/// The published per-source **fodder** quote (`SourceYield::fodder`, issue #449) — the twin of
/// [`band_materials`] in the feed currency, and what the compact yield readouts render.
fn published_fodder(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band forages")
        .last_yields
        .first()
        .expect("the forage assignment has a yield row")
        .fodder
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    policy: f32,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(FORAGE_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: patch,
                        floor: policy,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: FORAGE_WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}
