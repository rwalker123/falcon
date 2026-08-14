//! **The forecast reports a RANGE, and `forecast == actual` is restated**
//! (`docs/plan_hunt_through_combat.md` §6.4, slice 6).
//!
//! # The design call this file pins
//!
//! A forecast has **no event seed**. `fauna::retreat_seed` is composed from
//! `(map_seed, tick, herd, party)` and a projection is projecting into ticks that have not happened,
//! so a preview physically cannot draw the retreat — or the attack rolls — the live take will draw.
//! Two ways out were open: report the **expectation** and restate the invariant, or make the draw
//! forecast-reproducible by taking the tick out of the seed. **The expectation was chosen**, and the
//! invariant now reads:
//!
//! > `actualYield` is the take's **expectation** over the seed, and the take the sim pays lies within
//! > `[actualYieldLow, actualYieldHigh]`. Where no stage is stochastic the distribution is degenerate
//! > and `low == likely == high == the take`, **bit-for-bit**.
//!
//! Removing the tick from the seed was refused because it would make the draw a per-`(herd, party)`
//! **constant**: the same pairing would roll identically on turn 1 and turn 40, so "risk" would never
//! vary in play and a player could learn the answer — the spreadsheet §4.7 says variance exists to
//! prevent. It would also break §6.2's *per-event* seeding, whose event is `(herd, tick, party)`.
//!
//! # It shipped degenerate, and this file still measures that half
//!
//! When the band landed, `wariness` was `0` across the roster and `hit_chance` `1.0`, so both
//! binomials took their exact identities at every quantile and the reported range was a **point**.
//! Slice 7 authored the roster's wariness, so **this harness holds it at `0`** (see
//! `headless_with_species`) in order to keep stating the degenerate case at all — and then widens
//! exactly one term, a test-local `hit_chance`, to watch the band open.
//!
//! **The live half is `core_sim/tests/hunt_wariness.rs`**, which runs the containment sweep on the
//! shipped config. The two files together cover the invariant: a point where nothing is stochastic,
//! a real band where something is.

/// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
/// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since quality
/// tiers landed, so a fixture that wants "an ordinary band" asks the item table.
fn equipped_haul_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::HuntCarry,
        core_sim::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity,
    )
}

/// The gather twin of [`equipped_haul_rate`] — the baskets' own tier.
fn equipped_gather_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::ForageCarry,
        core_sim::LaborConfig::builtin()
            .forage
            .per_worker_biomass_capacity,
    )
}

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    advance_labor_allocation, build_headless_app, herd_hunt_yield, hunt_take,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_herds,
    CombatConfig, CombatConfigHandle, FactionId, FaunaConfigHandle, GenerationId, HerdRegistry,
    HuntDraw, HuntingParty, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LocalStore, MoraleCause, PopulationCohort, ResidentBand, SnapshotHistory, TileRegistry,
    NO_IMPROVEMENT_UNDERWAY,
};

/// A stock far above anything a crew can take, so the escapement floor never binds and the *fight*
/// is the only term that can move the take.
const TEST_CAPACITY: f32 = 4000.0;

/// The food peak — the default floor a fresh assignment gets.
const FOOD_PEAK: f32 = 0.5;

/// A defaulting species (its `hunt_yield` block is omitted, so it pays the global rate) — the
/// forecast's food half.
const DEFAULTING_SPECIES: &str = "Red Deer";
/// The **inedible** species: `provisions_per_biomass == 0`, so every food reading it publishes is an
/// honest zero. Since arc #527 retired the trade axis, what such a hunt really banks is **material
/// batches**, which the forecast does not project — so the wolf's arm here asserts the *shape* of
/// the row (a degenerate band on a zero) rather than a payload.
const INEDIBLE_SPECIES: &str = "Grey Wolf Pack";
/// Defenceless (`defense 0`), frail (`durability 2`), harmless (`ferocity 0`) and cheap to reach
/// (`engage_rate 10`) — the one roster row where a sub-1 `hit_chance` moves the take by **many whole
/// animals** rather than by a fraction of one the quantiser then rounds away.
const FRAIL_SPECIES: &str = "Rabbit Warren";

/// A crew whose throughput never binds against [`TEST_CAPACITY`], so what is under test is the
/// forecast and not the carry cap.
const CREW: u32 = 20;

/// The take is quantised onto the larder's fixed-point `Scalar` grid on the way out, so a forecast
/// `f32` and the paid figure agree to within a grid step rather than bit-for-bit. **The range's own
/// three readings ARE compared bit-for-bit** — they come out of one function at three quantiles, so
/// any difference there is a real difference.
const YIELD_EPSILON: f32 = 1e-3;

/// A world with its first game herd re-shaped into `display_name` at a healthy, whole-animal stock.
/// Re-speciating an existing herd keeps the placement/graze plumbing real while pinning the one
/// variable under test.
/// **The map seed every fixture here builds on.** The shipped default is `0` — *seed from entropy* —
/// so an un-pinned harness generates a different world (and a different quarry herd) on every run,
/// which a coverage-fraction assertion cannot afford. Same value as the sibling hunt suites.
const SEED: u64 = 119_304_647;

fn headless_with_species(display_name: &str) -> (App, String, UVec2) {
    let mut app = build_headless_app();
    app.world
        .resource_mut::<core_sim::SimulationConfig>()
        .map_seed = SEED;
    // **The retreat stage is held at its identity here, and the file's own subject is why.** These
    // tests assert what the forecast does when *nothing* is stochastic — the degenerate point — and
    // then widen exactly one term (`hit_chance`) to watch the band open. Slice 7's authored
    // `combat.wariness` (`docs/plan_hunt_through_combat.md` §3.1) would widen a second term
    // underneath every case, so the degenerate half could not be stated at all. Wariness's own live
    // band is asserted in `hunt_wariness.rs`; see `FaunaConfig::without_retreat`.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app.update();
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_"))
            .map(|h| h.id.clone())
            .expect("the map seeded game")
    };
    let body_mass = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(display_name)
        .expect("the species is in the shipped roster")
        .body_mass;
    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.species = display_name.to_string();
        herd.body_mass = body_mass;
        // Freeze the range-derived `K` so the forecast and the take see one capacity: this file
        // measures the FORECAST, not the grazing loop.
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = TEST_CAPACITY;
        herd.biomass = TEST_CAPACITY;
        herd.biomass_before_regrowth = TEST_CAPACITY;
        herd.hunt_credit = 0.0;
        herd.position()
    };
    // Re-emit the display telemetry, or the wire describes the herd this fixture replaced.
    app.world.run_system_once(spawn_initial_herds);
    (app, id, pos)
}

/// **Mark the herd's tile `Active` for the viewer**, so the fog-filtered `WorldSnapshot.herds` list
/// actually publishes it — a herd on dark ground is correctly withheld, and a test reading its wire
/// row has to reveal it first.
fn reveal_herd(app: &mut App, id: &str) {
    let pos = {
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(id).map(|herd| herd.position())
    };
    let grid = app.world.resource::<core_sim::SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    if let Some(pos) = pos {
        map.mark_active(pos.x, pos.y, 0);
    }
}

/// A resident band of `CREW` hunting `fauna_id` at `floor`, standing on the herd's tile.
fn spawn_hunters(app: &mut App, pos: UVec2, fauna_id: &str, floor: f32) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            ResidentBand,
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
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
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor,
                    },
                    workers: CREW,
                    improvement: NO_IMPROVEMENT_UNDERWAY,
                    kit: None,
                    improvement_workers: NO_IMPROVEMENT_UNDERWAY
                        .map_or(NO_CREW_ON_THIS_ACTIVITY, |_| CREW),
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// **Seed the band's assignment row from its pre-commit forecast**, exactly as
/// `server::seed_source_yield` does when the player composes the assignment — the *only* row that
/// carries a distribution, since a resolved row is a fact and reports the point it paid.
fn seed_the_forecast(app: &mut App, band: bevy::prelude::Entity, fauna_id: &str, floor: f32) {
    let seeded = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let combat = app.world.resource::<CombatConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(fauna_id).expect("the herd is on the map");
        core_sim::hunt_source_yield_preview(
            herd,
            &fauna,
            equipped_haul_rate(),
            &party_at(&combat),
            CONTENT_BAND_OUTPUT_MULTIPLIER,
            CREW,
            floor,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            combat.forecast_range_sigmas,
        )
    };
    let target = LaborTarget::Hunt {
        fauna_id: fauna_id.to_string(),
        floor,
    };
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band carries its allocation")
        .set_source_yield(&target, seeded);
}

/// A content band's output multiplier — morale `1.0`, so the forecast and the take share it.
const CONTENT_BAND_OUTPUT_MULTIPLIER: f32 = 1.0;

/// The shipped, fully-kitted party fighting at `combat`'s tuning — the same composition
/// `server::seed_source_yield` builds for a band whose spears are whole.
fn party_at(combat: &CombatConfig) -> HuntingParty {
    retuned(HuntingParty::builtin_equipped(), combat)
}

/// **The same uniform party at `combat`'s dials** — the builtin fixture carries the builtin config's
/// tuning, and a test installing its own has to restate it. Uniform because every fixture here is:
/// [`HuntingParty::uniform`] is the shape a fully-covered party resolves to.
fn retuned(party: HuntingParty, combat: &CombatConfig) -> HuntingParty {
    HuntingParty::uniform(
        party.best_equipped_hunter(),
        combat.tuning(),
        combat.hunt_injury_damage_per_animal,
        party.dispersion,
    )
}

/// The **exported** assignment row — the shipped artifact, not the in-process `SourceYield`.
fn exported_row(app: &App, band: bevy::prelude::Entity) -> sim_runtime::LaborAssignmentState {
    app.world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot
        .populations
        .iter()
        .find(|p| p.entity == band.to_bits())
        .expect("the hunting band is in the snapshot")
        .labor_assignments
        .first()
        .expect("its one Hunt assignment is exported")
        .clone()
}

/// **THE DEGENERATE RANGE IS EXACT — on the exported snapshot, in both currencies.**
///
/// With `wariness 0` and `hit_chance 1.0` nothing in the take is stochastic, so the reported low,
/// likely and high must be **the same bits**, and that one number must be what the turn then pays.
/// This is the provable-identity shape slice 2 used, moved onto the readout: the whole slice can
/// ship with the range wired through every path and *no* number in the game moving.
///
/// Swept across both webs' shapes on the animal side — a **defaulting** species (food, the global
/// rate) and an **inedible** one (whose food band is honestly all-zero) — and across the floor,
/// because the floor is what decides whether the escapement or the crew binds.
#[test]
fn the_degenerate_range_is_a_point_and_it_is_the_take_the_sim_pays() {
    for species in [DEFAULTING_SPECIES, INEDIBLE_SPECIES] {
        for floor in [0.0, 0.3, FOOD_PEAK] {
            let (mut app, id, pos) = headless_with_species(species);
            let band = spawn_hunters(&mut app, pos, &id, floor);
            seed_the_forecast(&mut app, band, &id, floor);
            recapture_snapshot_in_place(&mut app.world);
            let seeded = exported_row(&app, band);

            // **Bit-for-bit**, not within an epsilon: the three readings come out of one function at
            // three quantiles, so any difference at all is a real one.
            assert_eq!(
                (seeded.actual_yield_low, seeded.actual_yield_high),
                (seeded.actual_yield, seeded.actual_yield),
                "{species} @ floor {floor}: with no stochastic term the FOOD range must be the \
                 point estimate, bit-for-bit: {seeded:?}"
            );

            // ...and the point is the take. (Within a `Scalar` grid step — see [`YIELD_EPSILON`];
            // the *range* above is the bit-for-bit claim.)
            app.world.run_system_once(advance_labor_allocation);
            recapture_snapshot_in_place(&mut app.world);
            let paid = exported_row(&app, band);
            assert!(
                (seeded.actual_yield - paid.actual_yield).abs() <= YIELD_EPSILON,
                "{species} @ floor {floor}: forecast FOOD {} must equal the paid {}",
                seeded.actual_yield,
                paid.actual_yield
            );
            // **Liveness, where there is a payload to be live about.** A range that is a point
            // because the take is zero "passes" every assertion above, so the EDIBLE species must
            // actually take something. The inedible one honestly pays no food at any floor — its
            // whole payload is hides the forecast does not project — so its arm asserts exactly
            // that, which is a claim rather than an exemption.
            if species == INEDIBLE_SPECIES {
                assert_eq!(
                    paid.actual_yield, 0.0,
                    "{species} @ floor {floor}: an inedible quarry pays no food, and the row must \
                     say so rather than inventing one ({paid:?})"
                );
            } else {
                assert!(
                    paid.actual_yield > 0.0,
                    "{species} @ floor {floor}: the harness must actually take something ({paid:?})"
                );
            }
        }
    }
}

/// **A RESOLVED row is a fact, not a forecast** — it reports the point it paid.
///
/// The band is a pre-commit statement about a draw that has not happened. Once the turn resolves
/// there is no distribution left, so the exported row's low/high collapse onto its `actualYield`
/// **whatever the config's stochastic terms say** — asserted here under a live sub-1 `hit_chance`,
/// where a row that merely inherited the seed's band would visibly disagree.
#[test]
fn a_resolved_row_reports_the_point_it_paid_even_when_the_take_is_stochastic() {
    let (mut app, id, pos) = headless_with_species(FRAIL_SPECIES);
    make_the_fight_stochastic(&mut app);
    let band = spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
    let paid = exported_row(&app, band);

    assert!(
        paid.actual_yield > 0.0,
        "liveness: the resolved turn must have taken something ({paid:?})"
    );
    assert_eq!(
        (paid.actual_yield_low, paid.actual_yield_high),
        (paid.actual_yield, paid.actual_yield),
        "a resolved row has no distribution left: {paid:?}"
    );
}

/// The test-local `hit_chance` that makes the fight genuinely stochastic. Half, because a binomial's
/// variance is maximised there, so the widened range is at its most visible.
const COIN_FLIP_HIT_CHANCE: f32 = 0.5;

/// Replace the world's combat tuning with one whose attack rolls are a coin flip. **The roster ships
/// `1.0`**, which is an exact identity that draws nothing, so a test that wants to see a range has to
/// author the stochastic term itself.
fn make_the_fight_stochastic(app: &mut App) {
    let mut combat = CombatConfig::builtin().as_ref().clone();
    combat.hit_chance = COIN_FLIP_HIT_CHANCE;
    combat
        .validate()
        .expect("a coin-flip hit chance is a legal tuning");
    app.world
        .insert_resource(CombatConfigHandle::new(std::sync::Arc::new(combat)));
}

/// How many independent per-event seeds the containment sweep draws. Large enough that a coverage
/// fraction means something; the seeds are the *live* take's own, composed exactly as
/// `advance_labor_allocation` composes them.
const CONTAINMENT_SEEDS: u32 = 400;

/// **The share of live takes a `±2σ` band must contain.** A normal-approximated binomial's `2σ` band
/// is ~95%, and the whole-animal `floor()` shifts a *discrete* band's coverage either way, so the
/// floor is set below the nominal figure deliberately: asserting 95% of a quantised binomial is the
/// flaky-by-construction shape §6.3 warns about. It is still far above what a *broken* range would
/// reach — a point estimate would contain ~1 in 400.
const RANGE_COVERAGE_FLOOR: f64 = 0.85;

/// **THE RANGE WIDENS WHEN THERE IS SOMETHING TO BE UNCERTAIN ABOUT — and it contains the take.**
///
/// Drives a test-local `hit_chance` of `0.5` (the roster ships `1.0`), then draws the *live* take at
/// [`CONTAINMENT_SEEDS`] independent per-event seeds and checks each against the reported band.
///
/// **Three liveness assertions ride with it, because a range assertion alone is the weakest shape
/// there is** (§6.3): a band of `[0, ∞]` contains everything, a dead fight makes every take equal so
/// containment is trivial, and a range that never widened would pass the *mean* check.
/// So this pins, in the same run: the band is strictly wider than a point; the takes genuinely
/// **vary** across seeds; and their mean lands on the reported `likely`.
#[test]
fn the_range_widens_with_a_stochastic_fight_and_contains_the_take_across_many_seeds() {
    let (mut app, id, pos) = headless_with_species(FRAIL_SPECIES);
    make_the_fight_stochastic(&mut app);
    // The band under test, with its row seeded from the pre-commit forecast — so the band below is
    // the one that crossed the **wire**, not an in-process struct a client never sees.
    let band = spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    seed_the_forecast(&mut app, band, &id, FOOD_PEAK);
    recapture_snapshot_in_place(&mut app.world);
    let reported = exported_row(&app, band);

    // **Liveness 1 — the band is not a point.** Without this the containment sweep below would pass
    // on a forecast that had quietly stopped reading the stochastic term at all.
    assert!(
        reported.actual_yield_low < reported.actual_yield
            && reported.actual_yield < reported.actual_yield_high,
        "a coin-flip fight must widen the exported band: {reported:?}"
    );

    let fauna = app.world.resource::<FaunaConfigHandle>().get();

    let combat = app.world.resource::<CombatConfigHandle>().get();
    let party = party_at(&combat);
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .expect("the herd is on the map")
        .clone();
    let hunt_yield = herd_hunt_yield(&herd, &fauna);
    let mut takes = Vec::with_capacity(CONTAINMENT_SEEDS as usize);
    for seed in 0..CONTAINMENT_SEEDS {
        let mut quarry = herd.clone();
        let outcome = hunt_take(
            &mut quarry,
            CREW,
            FOOD_PEAK,
            equipped_haul_rate(),
            &party,
            &fauna,
            // A resident band eats/banks its whole take, exactly as the Hunt labor arm passes.
            f32::INFINITY,
            HuntDraw::Seeded(u64::from(seed)),
        );
        takes.push(
            hunt_yield
                .apply(outcome.take.carried, CONTENT_BAND_OUTPUT_MULTIPLIER)
                .provisions,
        );
    }

    // **Liveness 2 — the takes actually vary.** A dead fight stage would make every draw identical,
    // and a constant sits inside any band; the spread is what makes containment mean something.
    let spread = takes.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        - takes.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        spread > 0.0,
        "the live take must genuinely vary across seeds, or containment is vacuous: {takes:?}"
    );

    let contained = takes
        .iter()
        .filter(|take| **take >= reported.actual_yield_low && **take <= reported.actual_yield_high)
        .count();
    let coverage = contained as f64 / takes.len() as f64;
    assert!(
        coverage >= RANGE_COVERAGE_FLOOR,
        "the reported band must contain the take it is a band around: {coverage:.3} of \
         {CONTAINMENT_SEEDS} seeds fell inside [{}, {}]",
        reported.actual_yield_low,
        reported.actual_yield_high
    );

    // **Liveness 3 — the reported `likely` is the take's EXPECTATION**, which is the half of the
    // restated invariant a containment check cannot see. Compared against the band's own half-width,
    // so the tolerance is a property of the distribution rather than a hand-picked number.
    let mean = takes.iter().sum::<f32>() / takes.len() as f32;
    let half_width = (reported.actual_yield_high - reported.actual_yield_low) / 2.0;
    assert!(
        (mean - reported.actual_yield).abs() <= half_width,
        "the mean take {mean} must track the reported likely {} (band half-width {half_width})",
        reported.actual_yield
    );
}

// ---------------------------------------------------------------------------------------------
// §6.6 — THE HUNT EMITS EVENTS
// ---------------------------------------------------------------------------------------------

/// The feed kind a hunt report rides on. A **string** on the wire, so a new kind needs no schema
/// change — which is also why a test has to name it rather than a variant.
const HUNT_REPORT_KIND: &str = "hunt_report";

/// Every token §6.6 says a hunt report carries: animals engaged, how many fled before contact,
/// animals killed, hunters lost or wounded, what ran out first, and what came home against what was
/// left on the range. **Facts, never a composed judgement** — #272 owns importance and phrasing.
const REQUIRED_REPORT_TOKENS: [&str; 9] = [
    "engaged",
    "fled",
    "killed",
    "carried_biomass",
    "wasted_biomass",
    "hunters_killed",
    "hunters_wounded",
    "bound",
    "species",
];

/// Pull `key=value` out of a space-delimited detail string — the form the client's feed already
/// parses, and the form every other event kind in the log uses.
fn detail_token<'a>(detail: &'a str, key: &str) -> Option<&'a str> {
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(key)?.strip_prefix('='))
}

/// **THE HUNT REPORT REACHES THE WIRE, WITH THE FACTS §6.6 NAMES.**
///
/// Driven through a **real turn** — `advance_labor_allocation` on a real band and a real herd — and
/// read off the **exported snapshot**, not the in-process `CommandEventLog`: a row that never reached
/// the capture would still satisfy an in-process assertion.
///
/// Every token is checked for presence *and* for a value that agrees with what the turn did, because
/// a report whose numbers are all zero would pass a presence check while telling the player nothing.
#[test]
fn a_real_turn_exports_a_hunt_report_carrying_what_happened() {
    let (mut app, id, pos) = headless_with_species(DEFAULTING_SPECIES);
    spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let report = snapshot
        .command_events
        .iter()
        .find(|event| event.kind == HUNT_REPORT_KIND)
        .unwrap_or_else(|| {
            panic!(
                "a hunt that happened must publish a report; the exported feed held {:?}",
                snapshot
                    .command_events
                    .iter()
                    .map(|e| e.kind.as_str())
                    .collect::<Vec<_>>()
            )
        });
    let detail = report
        .detail
        .as_deref()
        .expect("the facts ride the detail, never the label");

    for key in REQUIRED_REPORT_TOKENS {
        assert!(
            detail_token(detail, key).is_some(),
            "§6.6 requires `{key}` on a hunt report: {detail}"
        );
    }

    // **The numbers must describe THIS hunt**, not be present-and-empty. The band engaged animals,
    // put some down, and hauled biomass home; nothing fled (the roster ships `wariness 0`).
    let engaged: f32 = detail_token(detail, "engaged").unwrap().parse().unwrap();
    let killed: u32 = detail_token(detail, "killed").unwrap().parse().unwrap();
    let carried: f32 = detail_token(detail, "carried_biomass")
        .unwrap()
        .parse()
        .unwrap();
    let fled: f32 = detail_token(detail, "fled").unwrap().parse().unwrap();
    assert!(engaged > 0.0, "the party engaged something: {detail}");
    assert!(killed > 0, "the party killed something: {detail}");
    assert!(carried > 0.0, "the party hauled something home: {detail}");
    assert_eq!(
        fled, 0.0,
        "this harness holds wariness at 0, so nothing may flee: {detail}"
    );
    assert!(
        matches!(
            detail_token(detail, "bound").unwrap(),
            "engagement" | "floor" | "carry" | "fight"
        ),
        "the bound must name one of the take's four limits: {detail}"
    );
    // **`species` is the LAST token, and it has to be**: a display name contains spaces, so in a
    // space-delimited `key=value` grammar it can only be the trailing remainder — which is exactly
    // where the `HuntDanger` line beside it puts the same value.
    assert!(
        detail.ends_with(&format!("species={DEFAULTING_SPECIES}")),
        "the report names the SPECIES, never the internal herd id, and it trails: {detail}"
    );
}

/// **A WOUNDED-ONLY HUNT IS VISIBLE — and `HuntDanger` did not have to widen to make it so.**
///
/// The hunt's baseline injury risk (§4.6) lands as `wounded` and **never** as `killed`, so before the
/// report a hunt on a harmless quarry produced casualties nothing in the feed mentioned: `HuntDanger`
/// is gated on a **death** precisely because every engagement now wounds someone and a "cost 0 lives"
/// line per band per turn is not a report. The gate stays; the report carries the wounded instead.
///
/// Pinned on the exported feed, over a **harmless** species (`ferocity 0` — a one-sided engagement
/// that kills nobody), so the two halves are asserted together: no `hunt_danger` row, and a
/// `hunt_report` row whose `hunters_wounded` is strictly positive.
#[test]
fn a_harmless_hunt_publishes_its_wounded_without_a_danger_line() {
    let (mut app, id, pos) = headless_with_species(FRAIL_SPECIES);
    spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    assert!(
        !snapshot
            .command_events
            .iter()
            .any(|event| event.kind == "hunt_danger"),
        "a harmless quarry kills nobody, so the death-gated line must stay silent"
    );
    let detail = snapshot
        .command_events
        .iter()
        .find(|event| event.kind == HUNT_REPORT_KIND)
        .expect("the hunt still reports")
        .detail
        .clone()
        .expect("the facts ride the detail");
    let wounded: f32 = detail_token(&detail, "hunters_wounded")
        .unwrap()
        .parse()
        .unwrap();
    let killed: f32 = detail_token(&detail, "hunters_killed")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        wounded > 0.0,
        "the baseline injury risk must be visible somewhere, and this is where: {detail}"
    );
    assert_eq!(
        killed, 0.0,
        "a hunt's own hazards have no attacker, so they never kill: {detail}"
    );
}

// ---------------------------------------------------------------------------------------------
// §6.5 — A FIGHT THE PARTY CANNOT WIN MUST SAY SO BEFORE IT IS LAUNCHED
// ---------------------------------------------------------------------------------------------

/// The shipped megafauna: `defense 12`, `durability 500` — the row a bare-handed band (`attack 1`)
/// cannot hurt at any headcount, and the reason §6.5 exists.
const GATED_SPECIES: &str = "Thunder Mammoths";

/// **THE GATE IS COMPOSABLE FROM EXPORTED TERMS — and the sim exports no verdict.**
///
/// §6.5 wants two independent signals before a launch: the panel checks the gate *in words*, and the
/// forecast independently estimates **zero food**. This pins the sim's half of both.
///
/// The client already held `PopulationCohortState.hunterAttack` and `HerdTelemetryState.defense`, so
/// `max(0, attack − defense)` was already composable and **no "can this band win" boolean is
/// exported** — this arc's own rule is that a linear, exact bound ships as a term. What was genuinely
/// missing is `durability`: without it the panel can say *"you cannot"* but not *"…and with spears it
/// would take 62 hunter-turns"*. So `durability` ships, and the composition is asserted here against
/// the sim's own answer rather than against a literal.
#[test]
fn the_exported_terms_compose_the_gate_and_the_forecast_agrees() {
    let (mut app, id, _pos) = headless_with_species(GATED_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is in the snapshot");

    // The species' own attrition denominator, off `fauna_config.json`.
    let authored = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(GATED_SPECIES)
        .expect("the species is in the shipped roster")
        .combat
        .durability;
    assert_eq!(
        herd.durability, authored,
        "the wire must carry the species' authored durability, not a derived one"
    );
    // **Liveness / the split that must not blur**: `defense` is whether a hit counts, `durability` is
    // how many counting hits it takes. Both positive and *different* here, so a test that read one
    // for the other would fail.
    assert!(herd.defense > 0.0 && herd.durability > herd.defense);

    // The gate, composed exactly as a client would — bare hands cannot hurt it, spears can.
    let bare = HuntingParty::builtin_unequipped()
        .best_equipped_hunter()
        .attack;
    let speared = HuntingParty::builtin_equipped()
        .best_equipped_hunter()
        .attack;
    assert_eq!(
        (bare - herd.defense).max(0.0),
        0.0,
        "bare hands must compose to a zero effective attack against megafauna"
    );
    assert!(
        (speared - herd.defense).max(0.0) > 0.0,
        "spears must compose to a positive one"
    );

    // ...and the forecast's independent signal: a bare-handed band is quoted **zero food**, from a
    // different path (the fight inside `hunt_source_yield_preview`), so a failure in either still
    // leaves the player warned.
    // A crew above the one-turn threshold `ceil(durability / (attack − defense))` = 63, so a
    // *speared* party's honest one-turn quote is non-zero and the liveness half below means
    // something. (Below it the fight is steep rather than absolute — the party grinds the animal
    // down over turns — which is `HuntTakeBound::Fight`, not the gate.)
    const ABOVE_THE_ONE_TURN_THRESHOLD: u32 = 80;
    let quote = |party: HuntingParty, workers: u32| {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let combat = app.world.resource::<CombatConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        core_sim::hunt_source_yield_preview(
            registry.find(&id).expect("the herd is on the map"),
            &fauna,
            equipped_haul_rate(),
            &retuned(party, &combat),
            CONTENT_BAND_OUTPUT_MULTIPLIER,
            workers,
            FOOD_PEAK,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            combat.forecast_range_sigmas,
        )
    };
    let bare_handed = quote(
        HuntingParty::builtin_unequipped(),
        ABOVE_THE_ONE_TURN_THRESHOLD,
    );
    assert_eq!(
        (
            bare_handed.actual,
            bare_handed.range.low,
            bare_handed.range.high
        ),
        (0.0, 0.0, 0.0),
        "below the gate the forecast must quote zero at EVERY quantile — the range cannot promise \
         a take the gate forbids"
    );
    // **Liveness**: the same crew with spears is quoted something, or the zero above would be a
    // fact about the harness rather than about the gate.
    let speared_quote = quote(
        HuntingParty::builtin_equipped(),
        ABOVE_THE_ONE_TURN_THRESHOLD,
    );
    assert!(
        speared_quote.actual > 0.0,
        "the same crew with spears must be quoted a real take: {speared_quote:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// THE PLANT WEB — the other half of "forecast == actual, in its new form, across both webs"
// ---------------------------------------------------------------------------------------------

/// **THE PLANT WEB'S RANGE IS A POINT BY CONSTRUCTION — and the point is the take.**
///
/// A gather has **no stochastic stage at all**: no engagement, no retreat, no fight
/// (`SourceYieldForecast::fight` is `None` on every plant source and its `engage_rate` is
/// `f32::INFINITY`). So the restated invariant collapses back to the old one there, and it must keep
/// doing so however wide `forecast_range_sigmas` is set — which is what makes the range safe to ship
/// on both webs from one `forecast_source_yield`.
///
/// Driven end-to-end: seed the row from `forage_source_yield_preview` exactly as
/// `server::seed_source_yield` does, read the **exported** band, resolve a real turn, and compare.
#[test]
fn a_gather_reports_a_point_and_pays_it() {
    let mut app = build_headless_app();
    app.update();

    // A live patch with standing crop **that actually pays FOOD**, and the tile it sits on.
    //
    // **The food test is load-bearing, not belt-and-braces.** `map_seed` is entropy by default and
    // `patches` is a hash map, so "the first patch with biomass" is a different tile every run —
    // and since #433 a tile's yield comes from its own flora basket, which can be **cash-crop only**
    // (flax, cotton, hay: `provisions_per_biomass == 0`, a real trade income and no gather at all).
    // Landing on one made the liveness assertion at the foot of this test fail on an honest `0.0`
    // beside a live `trade_yield`, at a low rate and with nothing in the message to say why. The
    // fixture now states the property it needs instead of hoping the draw supplies it.
    let coord = {
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let tiles = app.world.resource::<TileRegistry>();
        let mut edible: Vec<_> = app
            .world
            .resource::<core_sim::ForageRegistry>()
            .patches
            .iter()
            .filter(|(_, patch)| patch.biomass > 0.0)
            .filter(|(coord, patch)| {
                tiles
                    .index(coord.x, coord.y)
                    .and_then(|tile| app.world.get::<core_sim::Tile>(tile))
                    .is_some_and(|ground| {
                        let composition = core_sim::tile_flora_composition(
                            &flora,
                            &labor.forage,
                            ground,
                            map_seed,
                        );
                        core_sim::patch_provisions_per_biomass(
                            patch,
                            &composition,
                            &flora,
                            &labor.forage,
                        ) > 0.0
                    })
            })
            .map(|(coord, _)| *coord)
            .collect();
        // …and in a stable order, so a hash map's iteration order is not a second source of drift.
        edible.sort_by_key(|coord| (coord.y, coord.x));
        *edible
            .first()
            .expect("the map seeded a food-bearing forage patch")
    };
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("the patch's tile resolves");

    let band = {
        let cohort = PopulationCohort {
            home: tile,
            current_tile: tile,
            last_fertility_factors: Default::default(),
            size: 200,
            children: scalar_zero(),
            working: scalar_from_f32(CREW as f32),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: Default::default(),
            discontent_fraction: scalar_zero(),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0 as GenerationId,
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        };
        app.world
            .spawn((
                ResidentBand,
                cohort,
                LaborAllocation {
                    assignments: vec![LaborAssignment {
                        target: LaborTarget::Forage {
                            tile: coord,
                            floor: FOOD_PEAK,
                            species: None,
                        },
                        workers: CREW,
                        improvement: NO_IMPROVEMENT_UNDERWAY,
                        kit: None,
                        improvement_workers: NO_IMPROVEMENT_UNDERWAY
                            .map_or(NO_CREW_ON_THIS_ACTIVITY, |_| CREW),
                    }],
                    ..Default::default()
                },
            ))
            .id()
    };

    // Seed the row from the plant web's own preview — the same seam the server's assign path calls.
    let seeded_row = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let ground = app.world.get::<core_sim::Tile>(tile).expect("the tile");
        let composition = core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        let seasonal = app
            .world
            .get::<core_sim::FoodModuleTag>(tile)
            .map_or(0.0, |module| module.seasonal_weight.max(0.0));
        let registry = app.world.resource::<core_sim::ForageRegistry>();
        core_sim::forage_source_yield_preview(
            registry.patch(coord).expect("the patch is live"),
            &composition,
            &labor.forage,
            &flora,
            equipped_gather_rate(),
            equipped_gather_rate(),
            seasonal,
            CONTENT_BAND_OUTPUT_MULTIPLIER,
            CREW,
            FOOD_PEAK,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            // Deliberately WIDER than the shipped lever: a plant range must be a point because the
            // model has no draw, not because the configured width happens to be small.
            ABSURDLY_WIDE_RANGE_SIGMAS,
        )
    };
    let target = LaborTarget::Forage {
        tile: coord,
        floor: FOOD_PEAK,
        species: None,
    };
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band carries its allocation")
        .set_source_yield(&target, seeded_row);
    recapture_snapshot_in_place(&mut app.world);
    let seeded = exported_row(&app, band);

    assert_eq!(
        (seeded.actual_yield_low, seeded.actual_yield_high),
        (seeded.actual_yield, seeded.actual_yield),
        "a gather has no stochastic stage, so its band is a point at any width: {seeded:?}"
    );

    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
    let paid = exported_row(&app, band);
    assert!(
        (seeded.actual_yield - paid.actual_yield).abs() <= YIELD_EPSILON,
        "forecast {} must equal the paid gather {}",
        seeded.actual_yield,
        paid.actual_yield
    );
    // **Liveness**: a barren patch would make every number above a zero that trivially agrees.
    assert!(
        paid.actual_yield > 0.0,
        "the harness must actually gather something: {paid:?}"
    );
}

/// A range width far past anything the config would ship, used to prove the plant web's point-ness
/// is **structural** (no draw to quantile) rather than an artifact of a narrow lever.
const ABSURDLY_WIDE_RANGE_SIGMAS: f32 = 25.0;
