//! **A HUNT'S PUBLISHED STEADY RATE IS WHAT THE TAKE PAYS ON AVERAGE** — `realizedYield` and the
//! arrival schedule are the live take's *expectation*, not the take evaluated at the retreat's mean.
//!
//! Both projections used to resolve the fight on the retreat's mean head count
//! (`HuntDraw::EXPECTED` → `1.6` Steppe Runners standing), while the live take draws whole bodies
//! (`0`–`4`) and the fight clamps each turn's blow to the bodies actually standing
//! (`combat::damage_absorbed`). A clamp is concave, so the average of the live blows is **less**
//! than the blow at the average standing, and every hunt that both the fight and the retreat bind
//! over-read. Measured at 8 speared hunters: Steppe Runners `5.09` projected against `4.58` paid,
//! Red Deer `5.04` against `4.54`, Wild Aurochs `5.38` against `4.70`. The band's runway is walked
//! off those projections, so it read `inf` on turns the larder was falling
//! (`stationary_push_probe`, seed 2).
//!
//! **Why this drives the functions and not a world.** The claim is statistical — a mean over the
//! retreat's draw — and resolving it to a few percent takes thousands of turns on one steady herd. A
//! world re-derives the herd's `K` from its grazing every turn, walks it, and wears the band's spears
//! out long before that, so a world run measures those instead. The test therefore drives the two
//! functions the turn itself calls, in the turn's order: `regrow_biomass` (Logistics), then the
//! resolved row's `project_realized_hunt` at `ProjectionStart::AfterRegrowth`, then
//! `systems::hunt_take` with the live per-event seed, then `project_arrivals_hunt` off the post-take
//! herd — exactly the four calls `systems::labor`'s wild hunt arm makes.

use bevy::math::UVec2;
use core_sim::{
    hunt_take, project_arrivals_hunt, project_realized_hunt, regrow_biomass, retreat_seed,
    FaunaConfig, Herd, HuntDraw, HuntingParty, ProjectionStart, SizeClass,
};

/// The hunters on every herd — the probe's band, and enough that the fight and the retreat both
/// bind on every species below.
const HUNTERS: u32 = 8;
/// The default escapement floor.
const FLOOR: f32 = 0.5;
/// A resident band's carry never binds these takes — one hunter carries `40` biomass and the biggest
/// body here is `120`, against eight hunters' `320`. Passed unbounded so the carry is not the term
/// being measured.
const UNBOUNDED_CARRY: f32 = f32::INFINITY;
/// A band at neutral productivity.
const NEUTRAL_OUTPUT: f32 = 1.0;
/// The shipped `yield_average_horizon_turns` and `arrivals_horizon_turns`.
const REALIZED_HORIZON: u32 = 40;
const ARRIVALS_HORIZON: u32 = 20;
/// Turns run before anything is counted, so the herd settles to the take.
const WARMUP_TURNS: u64 = 200;
/// Turns averaged. At 4000 the standard error of the live mean is about 1% of it on these herds.
const MEASURED_TURNS: u64 = 4000;
/// **The tolerance: three standard errors of the live mean**, computed from the turns themselves.
/// The live take is one draw a turn from the retreat; its long-run mean is known only to its
/// sampling error, and three of them is the conventional bound on a mean that has converged.
const STANDARD_ERRORS: f32 = 3.0;
/// **The test must be able to see the defect**: the tolerance has to sit well inside the ~10% the
/// mean-retreat projection over-read by, or a pass would say nothing.
const RESOLVES_A_SHARE_OF: f32 = 0.05;
/// A herd starts here as a share of its `K`, above the floor so the first turns take.
const OPENING_STOCK_SHARE: f32 = 0.9;
/// The map seed the live retreat is drawn from.
const MAP_SEED: u64 = 7;
/// The herd's id — the retreat seed hashes it.
const HERD_ID: &str = "steady";

/// `(species, K)`: a capacity large enough that the herd's regrowth outpaces the take, so the herd
/// settles high above its floor and the take is bound by the hunt, not the stock.
const HERDS: [(&str, f32); 3] = [
    ("Steppe Runners", 20_000.0),
    ("Red Deer", 6_000.0),
    ("Wild Aurochs", 8_000.0),
];

struct Readings {
    live: Vec<f32>,
    realized: Vec<f32>,
    arrivals: Vec<f32>,
}

fn drive(species: &str, capacity: f32, hunters: u32, carry: f32) -> Readings {
    let fauna = FaunaConfig::builtin();
    let def = fauna
        .species_by_display(species)
        .expect("a shipped species");
    let mut herd = Herd::new(
        HERD_ID.to_string(),
        species.to_string(),
        SizeClass::Big,
        vec![UVec2::ZERO],
        capacity * OPENING_STOCK_SHARE,
        capacity,
        def.fodder_per_biomass,
        def.regrowth_rate.expect("a wild species carries its own r"),
        def.body_mass,
    );
    let party = HuntingParty::builtin_equipped();
    let hunt_yield = fauna.hunt_yield_for(&herd.species);
    let mut readings = Readings {
        live: Vec::new(),
        realized: Vec::new(),
        arrivals: Vec::new(),
    };
    for tick in 0..WARMUP_TURNS + MEASURED_TURNS {
        regrow_biomass(&mut herd, &fauna);
        let realized = project_realized_hunt(
            &herd,
            &fauna,
            carry,
            &party,
            NEUTRAL_OUTPUT,
            hunters as f32,
            FLOOR,
            REALIZED_HORIZON,
            ProjectionStart::AfterRegrowth,
        )
        .provisions;
        let outcome = hunt_take(
            &mut herd,
            hunters as f32,
            FLOOR,
            carry,
            &party,
            &fauna,
            f32::INFINITY,
            HuntDraw::Seeded(retreat_seed(MAP_SEED, tick, HERD_ID, hunters)),
        );
        let arrivals = project_arrivals_hunt(
            &herd,
            &fauna,
            carry,
            &party,
            NEUTRAL_OUTPUT,
            hunters as f32,
            FLOOR,
            ARRIVALS_HORIZON,
        );
        if tick >= WARMUP_TURNS {
            readings.live.push(
                hunt_yield
                    .apply(outcome.take.carried, NEUTRAL_OUTPUT)
                    .provisions,
            );
            readings.realized.push(realized);
            readings
                .arrivals
                .push(arrivals.iter().sum::<f32>() / ARRIVALS_HORIZON as f32);
        }
    }
    readings
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

fn standard_error(values: &[f32]) -> f32 {
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f32>() / (values.len() - 1) as f32;
    (variance / values.len() as f32).sqrt()
}

#[test]
fn a_steady_hunts_published_rate_is_the_takes_mean() {
    for (species, capacity) in HERDS {
        let readings = drive(species, capacity, HUNTERS, UNBOUNDED_CARRY);
        let live = mean(&readings.live);
        let tolerance = STANDARD_ERRORS * standard_error(&readings.live);
        assert!(
            live > 0.0,
            "liveness: {species} must be paying something ({live})"
        );
        assert!(
            tolerance < RESOLVES_A_SHARE_OF * live,
            "{species}: the run must resolve the take to {RESOLVES_A_SHARE_OF} of itself to see \
             the defect ({tolerance} against {live})"
        );
        for (name, published) in [
            ("realized", mean(&readings.realized)),
            ("arrivals", mean(&readings.arrivals)),
        ] {
            assert!(
                (published - live).abs() <= tolerance,
                "{species}: the published {name} rate must be the take's mean — published \
                 {published}, paid {live} (tolerance {tolerance})"
            );
        }
    }
}

/// **A carry-bound hunt on a heavy body — the projection removes what the TAKE removes.**
///
/// Fourteen speared hunters on Thunder Mammoths: their packs seat `14 × 40 = 560` biomass against an
/// `800`-unit body, so every kill leaves `240` on the ground. The live take removes the whole carcass
/// from the herd and brings home the carried share. The projection used to remove only the carried
/// share, so its herd stood `240` fatter after every kill, re-cleared a body sooner, and quoted
/// `6.85` food a turn against `4.94` paid on this herd (`7.06` / `4.93` on the survey's).
///
/// **The take is deterministic here**, which is what lets the tolerance be tight: fourteen hunters
/// reach `0.7` of a mammoth, under one whole body, so the retreat keeps that part body at its
/// expectation and draws nothing, and `hit_chance 1.0` draws nothing in the fight. What remains
/// between the two readings is the projection window's own edge — a kill that lands just past the
/// horizon — averaged over thousands of windows.
#[test]
fn a_carry_bound_heavy_hunt_projects_what_the_take_pays() {
    /// Hunters whose reach is under one mammoth, so the take draws nothing.
    const MAMMOTH_HUNTERS: u32 = 14;
    /// One speared hunter's haul — the equipped sled tier.
    const SLED_CARRY: f32 = 40.0;
    /// The roster's full Thunder Mammoth group — a herd whose regrowth, not the party, bounds the take.
    const MAMMOTH_CAPACITY: f32 = 12_000.0;
    /// Relative agreement between the projection and the take's mean.
    const TIGHT_SHARE: f32 = 0.01;

    let readings = drive(
        "Thunder Mammoths",
        MAMMOTH_CAPACITY,
        MAMMOTH_HUNTERS,
        SLED_CARRY,
    );
    let live = mean(&readings.live);
    assert!(
        live > 0.0,
        "liveness: the hunt must be paying something ({live})"
    );
    for (name, published) in [
        ("realized", mean(&readings.realized)),
        ("arrivals", mean(&readings.arrivals)),
    ] {
        assert!(
            (published - live).abs() <= TIGHT_SHARE * live,
            "the published {name} rate must be what the take pays — published {published}, paid \
             {live}"
        );
    }
}
