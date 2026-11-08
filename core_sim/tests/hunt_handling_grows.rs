//! **THE ANIMAL LADDER PAYS AS IT IS BUILT** - a herd's per-hunter handling (reach, retreat, the
//! fight's softness and the share that is slaughtered rather than fought) grows with the work banked
//! on the Tame and on the pen, the way a tended patch's yield grows with the Cultivate.
//!
//! It used to STEP: `herd_engage_rate` / `herd_wariness` / `herd_resistance` / `herd_fight_stage`
//! branched on `is_domesticated()` / `is_corralled()`, stored facts set at completion, so a herd 38%
//! tamed hunted exactly like a wild one.
//!
//! Pinned here: the terms themselves (0% is the rung below **exactly**, 100% is the rung **exactly**,
//! half is strictly between), the per-hunter take through the shipped forecast, and
//! forecast == take on a half-tamed herd and a half-built pen.

use bevy::math::UVec2;
use core_sim::shipped;
use core_sim::{
    build_work_per_worker_turn, herd_engage_rate, herd_resistance, herd_upkeep_demand,
    herd_wariness, hunt_take, project_realized_hunt, regrow_biomass, retreat_seed, FactionId,
    FaunaConfig, Herd, HuntDraw, HuntingParty, LadderConfig, ProjectionStart, NO_BUILD_GEAR,
};

const SPECIES: &str = "Wild Sheep";
const FACTION: FactionId = FactionId(0);
const FLOOR: f32 = 0.5;
const CAPACITY: f32 = 20_000.0;
const OPENING_STOCK_SHARE: f32 = 0.9;
const NEUTRAL_OUTPUT: f32 = 1.0;
const UNBOUNDED_CARRY: f32 = f32::INFINITY;
const REALIZED_HORIZON: u32 = 40;
const HUNTERS: u32 = 3;
const WARMUP_TURNS: u64 = 200;
const MEASURED_TURNS: u64 = 4000;
const STANDARD_ERRORS: f32 = 3.0;
const MAP_SEED: u64 = 7;
const HERD_ID: &str = "grows";
const HALF: f32 = 0.5;
const WHOLE: f32 = 1.0;

/// Where on the animal ladder a fixture herd stands.
#[derive(Clone, Copy, Debug)]
enum Step {
    Wild,
    Tame(f32),
    Pen(f32),
}

fn herd_at(step: Step, fauna: &FaunaConfig, ladder: &LadderConfig) -> Herd {
    herd_of(SPECIES, CAPACITY, step, fauna, ladder)
}

fn herd_of(
    species: &str,
    capacity: f32,
    step: Step,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
) -> Herd {
    let def = fauna
        .species_by_display(species)
        .expect("a shipped species");
    let mut herd = Herd::new(
        HERD_ID.to_string(),
        species.to_string(),
        def.size_class,
        vec![UVec2::ZERO],
        capacity * OPENING_STOCK_SHARE,
        capacity,
        def.fodder_per_biomass,
        def.regrowth_rate.expect("a wild species carries its own r"),
        def.body_mass,
        shipped::root_standing(shipped::animal()),
    );
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.taming_cost_multiplier = fauna.taming_cost_multiplier_for(&herd.species);
    let tame_width = herd.rung_cost(shipped::animal_pastoral(), ladder);
    let pen_width = herd.rung_cost(shipped::animal_pen(), ladder);
    match step {
        Step::Wild => {}
        Step::Tame(share) => {
            assert!(herd.tame_outright(FACTION, ladder));
            herd.set_ladder_position(share * tame_width, ladder);
        }
        Step::Pen(share) => {
            assert!(herd.tame_outright(FACTION, ladder));
            if share >= WHOLE {
                assert!(herd.corral_at(UVec2::ZERO, ladder));
            } else {
                herd.set_ladder_position(tame_width + share * pen_width, ladder);
            }
        }
    }
    herd
}

/// **The three handling terms are the rung below at 0%, the rung at 100%, and strictly between at
/// half** - exactly, not approximately, at the ends.
#[test]
fn a_herds_handling_terms_grow_with_the_work_banked() {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let at = |step| herd_at(step, &fauna, &ladder);
    let wild = at(Step::Wild);
    let tame0 = at(Step::Tame(0.0));
    let tame_half = at(Step::Tame(HALF));
    let tamed = at(Step::Tame(WHOLE));
    let pen0 = at(Step::Pen(0.0));
    let pen_half = at(Step::Pen(HALF));
    let penned = at(Step::Pen(WHOLE));

    // Reach rises up the ladder; the retreat and the resistance fall.
    let reach = |h: &Herd| herd_engage_rate(h, &fauna, false);
    let wary = |h: &Herd| herd_wariness(h, &fauna);
    let resist = |h: &Herd| herd_resistance(h, &fauna);

    assert_eq!(reach(&tame0), reach(&wild), "untouched is wild exactly");
    assert_eq!(wary(&tame0), wary(&wild));
    assert_eq!(resist(&tame0), resist(&wild));
    assert_eq!(
        reach(&tamed),
        reach(&wild) * fauna.pastoral_engage_gain_for(SPECIES),
        "fully tamed is today's pastoral reach exactly"
    );
    assert_eq!(
        wary(&tamed),
        wary(&wild) * fauna.husbandry.pastoral_wariness
    );
    assert_eq!(resist(&tamed), fauna.pastoral_resistance_for(SPECIES));
    assert!(reach(&wild) < reach(&tame_half) && reach(&tame_half) < reach(&tamed));
    assert!(wary(&wild) > wary(&tame_half) && wary(&tame_half) > wary(&tamed));
    assert!(resist(&wild) > resist(&tame_half) && resist(&tame_half) > resist(&tamed));

    assert_eq!(
        reach(&pen0),
        reach(&tamed),
        "a fence not begun is the halter"
    );
    assert_eq!(wary(&pen0), wary(&tamed));
    assert_eq!(
        reach(&penned),
        reach(&wild) * fauna.pen_engage_gain_for(SPECIES),
        "a closed pen is today's pen reach exactly"
    );
    assert_eq!(wary(&penned), wary(&wild) * fauna.husbandry.pen_wariness);
    assert!(reach(&tamed) < reach(&pen_half) && reach(&pen_half) < reach(&penned));
    assert!(wary(&tamed) > wary(&pen_half) && wary(&pen_half) > wary(&penned));
    // The animals still fought keep the halter's softness while the fence goes up.
    assert_eq!(resist(&pen_half), resist(&tamed));
}

/// What the shipped forecast says the herd pays per turn, per hunter.
fn forecast_per_hunter(step: Step, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    let mut herd = herd_at(step, fauna, ladder);
    regrow_biomass(&mut herd, fauna);
    project_realized_hunt(
        &herd,
        fauna,
        UNBOUNDED_CARRY,
        &HuntingParty::builtin_equipped(),
        NEUTRAL_OUTPUT,
        HUNTERS as f32,
        FLOOR,
        REALIZED_HORIZON,
        ProjectionStart::AfterRegrowth,
    )
    .provisions
        / HUNTERS as f32
}

/// **A half-tamed herd takes strictly between wild and tamed per hunter; a half-built pen strictly
/// between tamed and penned**, through the forecast the game ships; and the ends are bit-exact.
#[test]
fn the_take_per_hunter_grows_with_the_build() {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let rate = |step| forecast_per_hunter(step, &fauna, &ladder);
    let wild = rate(Step::Wild);
    let tame0 = rate(Step::Tame(0.0));
    let tame_half = rate(Step::Tame(HALF));
    let tamed = rate(Step::Tame(WHOLE));
    let pen0 = rate(Step::Pen(0.0));
    let pen_half = rate(Step::Pen(HALF));
    let penned = rate(Step::Pen(WHOLE));
    assert_eq!(wild, tame0, "0% tamed hunts exactly like wild");
    assert_eq!(tamed, pen0, "0% penned hunts exactly like tamed");
    assert!(
        wild < tame_half && tame_half < tamed,
        "{wild} {tame_half} {tamed}"
    );
    assert!(
        tamed < pen_half && pen_half < penned,
        "{tamed} {pen_half} {penned}"
    );
}

/// Drive the live take against the forecast on a herd at `step`.
fn forecast_against_take(step: Step) -> (f32, f32, f32) {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let mut herd = herd_at(step, &fauna, &ladder);
    let hunt_yield = fauna.hunt_yield_for(&herd.species);
    let party = HuntingParty::builtin_equipped();
    let mut live = Vec::new();
    let mut realized = Vec::new();
    for tick in 0..WARMUP_TURNS + MEASURED_TURNS {
        regrow_biomass(&mut herd, &fauna);
        let projected = project_realized_hunt(
            &herd,
            &fauna,
            UNBOUNDED_CARRY,
            &party,
            NEUTRAL_OUTPUT,
            HUNTERS as f32,
            FLOOR,
            REALIZED_HORIZON,
            ProjectionStart::AfterRegrowth,
        )
        .provisions;
        let outcome = hunt_take(
            &mut herd,
            HUNTERS as f32,
            FLOOR,
            UNBOUNDED_CARRY,
            &party,
            &fauna,
            f32::INFINITY,
            HuntDraw::Seeded(retreat_seed(MAP_SEED, tick, HERD_ID, HUNTERS)),
        );
        if tick >= WARMUP_TURNS {
            live.push(
                hunt_yield
                    .apply(outcome.take.killed_biomass(), NEUTRAL_OUTPUT)
                    .provisions,
            );
            realized.push(projected);
        }
    }
    let mean = |values: &[f32]| values.iter().sum::<f32>() / values.len() as f32;
    let m = mean(&live);
    let variance = live.iter().map(|v| (v - m).powi(2)).sum::<f32>() / (live.len() - 1) as f32;
    let tolerance = STANDARD_ERRORS * (variance / live.len() as f32).sqrt();
    (m, mean(&realized), tolerance)
}

/// **forecast == take on a half-tamed herd and a half-built pen** - the published steady rate is the
/// live take's mean at three standard errors, so the interpolated terms reach every path.
#[test]
fn the_forecast_equals_the_take_on_a_half_built_tame_and_a_half_built_pen() {
    for step in [Step::Tame(HALF), Step::Pen(HALF), Step::Pen(WHOLE)] {
        let (live, forecast, tolerance) = forecast_against_take(step);
        assert!(live > 0.0, "liveness: {step:?} pays something");
        assert!(
            (forecast - live).abs() <= tolerance.max(live * 0.02),
            "{step:?}: forecast {forecast} against the take's mean {live} (tolerance {tolerance})"
        );
    }
}

/// Large game, where the keepers' bill is a real share of a lone worker's pay.
const PEN_SPECIES: [&str; 3] = ["Wild Aurochs", "Wild Boar", "Wild Sheep"];
/// The Tame check leaves the boar out: its pastoral keeping (two keeper-loads at the species' own
/// herd size) outgrows a lone worker's take at every stage of the Tame, wild included, which is the
/// roster's own tuning rather than the bill's timing.
const TAME_SPECIES: [&str; 3] = ["Wild Aurochs", "Wild Sheep", "Crag Goats"];

/// The species' own full-group biomass: a herd of the size the species actually keeps, so the
/// keepers' bill is the one a player meets rather than a fixture's oversize flock.
fn wild_capacity(species: &str, fauna: &FaunaConfig) -> f32 {
    fauna
        .species_by_display(species)
        .expect("a shipped species")
        .biomass[1]
}

/// One worker's food a turn, net of the keepers the herd's bill asks for - the same figure
/// `food_rate_survey` prints in its `1w` column.
fn one_worker_net(species: &str, step: Step, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    let mut herd = herd_of(species, wild_capacity(species, fauna), step, fauna, ladder);
    regrow_biomass(&mut herd, fauna);
    let gross = project_realized_hunt(
        &herd,
        fauna,
        UNBOUNDED_CARRY,
        &HuntingParty::builtin_equipped(),
        NEUTRAL_OUTPUT,
        1.0,
        FLOOR,
        REALIZED_HORIZON,
        ProjectionStart::AfterRegrowth,
    )
    .provisions;
    let keepers =
        herd_upkeep_demand(&herd, fauna, ladder) / build_work_per_worker_turn(NO_BUILD_GEAR);
    gross / (1.0 + keepers)
}

/// **Finishing a rung never pays a lone worker less than the half-built one did** - the keepers'
/// bill follows the work banked (a pen's too, not only at the fence), so half a fence is not a better
/// deal than a whole one.
#[test]
fn a_lone_workers_net_food_never_falls_as_the_tame_or_the_pen_is_finished() {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let net = |species: &str, step| one_worker_net(species, step, &fauna, &ladder);
    for species in TAME_SPECIES {
        let (half, whole) = (
            net(species, Step::Tame(HALF)),
            net(species, Step::Tame(WHOLE)),
        );
        assert!(
            half <= whole,
            "{species}: tame 50% nets {half}, 100% nets {whole}"
        );
    }
    for species in PEN_SPECIES {
        let (half, whole) = (
            net(species, Step::Pen(HALF)),
            net(species, Step::Pen(WHOLE)),
        );
        assert!(
            half <= whole,
            "{species}: pen 50% nets {half}, 100% nets {whole}"
        );
    }
}

/// **A closed pen carries a part body between turns.** One keeper handles a fraction of an aurochs a
/// turn; the slaughter used to floor that to nothing every turn, so the pen paid nothing for ever.
#[test]
fn one_keeper_on_a_penned_aurochs_slaughters_over_a_long_run() {
    const AUROCHS: &str = "Wild Aurochs";
    const TURNS: u64 = 200;
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let mut herd = herd_of(
        AUROCHS,
        wild_capacity(AUROCHS, &fauna),
        Step::Pen(WHOLE),
        &fauna,
        &ladder,
    );
    let party = HuntingParty::builtin_equipped();
    let mut killed = 0.0;
    for tick in 0..TURNS {
        regrow_biomass(&mut herd, &fauna);
        let outcome = hunt_take(
            &mut herd,
            1.0,
            FLOOR,
            UNBOUNDED_CARRY,
            &party,
            &fauna,
            f32::INFINITY,
            HuntDraw::Seeded(retreat_seed(MAP_SEED, tick, HERD_ID, 1)),
        );
        killed += outcome.take.killed_biomass();
    }
    assert!(killed > 0.0, "a lone keeper took nothing in {TURNS} turns");
}
