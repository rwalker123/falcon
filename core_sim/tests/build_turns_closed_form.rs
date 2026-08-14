//! **THE BUILD'S TURN ESTIMATE IS A CLOSED FORM, and the wire ships its TERMS**
//! (`.claude/rules/core_sim/yield-forecast.md` → "THE BOUNDARY, stated once").
//!
//! `buildTurnsRemaining` is the sim's answer for the crew **already** working the source, which is
//! the right thing for a card with no stepper and the wrong thing for a compose sheet whose whole
//! point is *add hands and watch it drop*. So the sheet evaluates the form itself, from terms the
//! sim publishes:
//!
//! ```text
//! gear(w)  = min(w, buildWorkSaturatingCrew) × buildWorkPerWorker
//! turns(w) = ceil((workCost − workDone − gear(w))
//!                 / (w × buildWorkPerWorkerTurn − meterRotPerTurn))
//! ```
//!
//! **The divisor's second term is the ROT and there is no floor factor** — `<rung>UpkeepDemand` is
//! the keeping pool's bill and is never netted off a build (`docs/plan_standing_upkeep.md` §4.6a),
//! and `learn_multiplier(floor)` came off the build accrual with the crews' separation.
//!
//! **The gear pair rides the band's own `kitTiers` row, not a source row**, because a kit's
//! saturation point is a fact about the band's ledger: an unstarted rung still has one, and picking
//! a different kit in the sheet re-prices the estimate off that kit's row.
//!
//! **This file is the safety argument for letting the client evaluate any of it.** At the
//! *committed* crew and floor, the gear term must reproduce the `buildWorkFromGear` the sim stamped
//! and the whole form must reproduce `buildTurnsRemaining` — **exactly**. If either can disagree,
//! the sheet lies about the very decision the tile card then reports differently.
//!
//! Read **off the exported snapshot**, never off the in-process registry: the claim is about what a
//! client can compute from what it is sent, so a term that never reached the codec must fail here.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    advance_herds, advance_labor_allocation, build_fraction, build_headless_app,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, BandEquipment,
    DiscoveryProgressLedger, EquipmentConfig, FactionId, FaunaConfigHandle, GenerationId,
    HerdRegistry, Improvement, LaborAllocation, LaborAssignment, LaborTarget, LocalStore,
    MoraleCause, PopulationCohort, ResidentBand, SimulationConfig, SnapshotHistory, TileRegistry,
    HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION, PENNING_DISCOVERY_ID,
};

/// **The species the fixture reshapes its herd into** — a `pen`-ceiling row, so `can_domesticate()`
/// holds and a `Tame` actually accrues. Named rather than "whatever worldgen spawned" because a
/// `wild`-ceiling herd would make the build silently refuse and the equality below trivially true of
/// two zeroes.
const TAMEABLE_SPECIES: &str = "Wild Boar";

/// **A tameable species whose `Tame` costs exactly the rung's own price** — `taming_cost_multiplier`
/// **1.0**, where the shipped Wild Boar is 1.25. It is what puts the over-geared case within reach of
/// a crew a band can actually staff: [`KEEPERS`] × the handling gear's 8.5 covers a 50-unit job
/// outright, and the shipped roster has five such rows (rabbit, fowl, crag goat, wild sheep, snow
/// hare).
const UNSCALED_TAMEABLE_SPECIES: &str = "Crag Goats";

/// **The kit the keepers are sent out with** — the one shipped roster entry declaring
/// `EquipmentStat::BuildWork`, so the gear term under test is non-zero rather than a term the
/// arithmetic never exercises.
const HANDLING_KIT: &str = "husbandry";

/// **The escapement floor the crew holds, and it is deliberately NOT the food peak.** The build
/// carries no floor term at all now, but the *guard against one being re-added* is exactly this:
/// `learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION` is `×1.0` at the peak, so a stray
/// `floor / foodPeak` in either producer would cancel there and the equality would pass on
/// arithmetic that is wrong everywhere else. Below the peak the herd also stands well above the
/// floor, so the crew is genuinely working the source.
const BUILDER_FLOOR: f32 = 0.30;

/// Keepers on the herd — enough that the build is not one turn (a one-turn build cannot tell
/// `ceil` from `round`) and enough that the flock is not under-contained.
const KEEPERS: u32 = 6;

/// A herd big enough that its stock stands far above [`BUILDER_FLOOR`], so the crew is working the
/// source every turn of the fixture.
///
/// **And small enough that its keeping demand is a hand or two.** The demand scales with the herd's
/// keeper load, so a 4,000-head flock needs a large `husbandry` role before it is held at all — and
/// on a band sized for that, `LaborAllocation::normalize` has room to shuffle crews the fixture
/// meant to pin. The build's arithmetic is what is under test, so the flock is sized to keep the
/// keeping out of the way of it. (The demand is **not** netted off the build —
/// `docs/plan_standing_upkeep.md` §4.6a — so it does not pace anything here.)
const TEST_CAPACITY: f32 = 400.0;

/// **The client's own food-peak constant**, restated here because a client holds no config. It must
/// equal the sim's [`MSY_BIOMASS_FRACTION`] — asserted below, since the two are separate literals in
/// separate languages and nothing else pins them together.
const CLIENT_FLOOR_FOOD_PEAK: f32 = 0.5;

/// **The client's reading of a job with no work left in it** — one turn, the transcription of
/// `intensification::BUILD_FINISHES_IN_ONE_TURN`.
const CLIENT_BUILD_FINISHES_IN_ONE_TURN: u32 = 1;

/// A headless world whose one game herd has been reshaped into a tameable species standing at
/// [`TEST_CAPACITY`], with the viewer able to see it.
fn world_with_a_tameable_herd() -> (App, String, UVec2) {
    world_with_a_herd_of(TAMEABLE_SPECIES)
}

/// The same world, reshaped into a **named** species — the axis the over-geared case needs, since
/// whether a crew's gear can cover the whole job is decided by that species'
/// `taming_cost_multiplier`.
fn world_with_a_herd_of(species_display: &str) -> (App, String, UVec2) {
    let mut app = build_headless_app();
    app.update();
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|herd| herd.id.starts_with("game_"))
            .map(|herd| herd.id.clone())
            .expect("the map seeded short-range game")
    };
    let species = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(species_display)
        .expect("the shipped roster carries the fixture species")
        .clone();
    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd the id came from");
        herd.species = species_display.to_string();
        herd.body_mass = species.body_mass;
        herd.husbandry_ceiling = species.husbandry_ceiling;
        // Freeze the range-derived K: this file measures the BUILD's arithmetic, not the grazing
        // loop, and a capacity that moved under the turn would move the floor with it.
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = TEST_CAPACITY;
        herd.biomass = TEST_CAPACITY;
        herd.biomass_before_regrowth = TEST_CAPACITY;
        herd.position()
    };
    // The herd row is fog-filtered, so a snapshot of an unwatched herd simply omits it.
    {
        let grid = app.world.resource::<SimulationConfig>().grid_size;
        let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
        let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
        let map = ledger.ensure_faction(viewer, grid.x, grid.y);
        map.mark_active(pos.x, pos.y, 0);
    }
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), HERDING_DISCOVERY_ID, scalar_one());
    (app, id, pos)
}

/// **How much handling gear the keepers hold** — the fixture's one axis, because it is what decides
/// whether the gear term is **saturated** (fewer units than hands, the cap binding) or **linear**
/// (a unit for everybody, the cap inert). A form missing the cap is right in one regime and wrong in
/// the other, so the equality is asserted in both.
#[derive(Clone, Copy)]
enum GearHeld {
    /// **One set of hurdles for the whole band** — the shipped roster's own reference ledger, and
    /// the regime the cap exists for: six keepers take *one worker's worth* off the job.
    OneSet,
    /// **A party's worth**, what a spawn stocks — every hand equipped, so coverage is uniform and
    /// the gear term is the plain `workers × worth`.
    APartysWorth,
}

/// **The crew the closed-form fixture staffs its build at**, and every term of the estimate is
/// quoted at it — the work banked per turn *and* the gear taken off the job
/// (`docs/plan_standing_upkeep.md` §2.2). Three things pin the number and it cannot move freely:
/// it is small enough that the shipped job takes several turns (so `ceil` is exercised), **above**
/// what [`GearHeld::OneSet`] arms (so the saturation `min` binds in that arm), and **at or below**
/// what [`GearHeld::APartysWorth`] arms (so the other arm is the linear regime). It is deliberately
/// **not** [`KEEPERS`]: a build crew that happened to equal the take crew would let a form reading
/// the wrong one of the two pass.
const THE_BUILD_CREW: u32 = 3;
// **The gear regimes are what fix it**, per the doc above: it must out-number what `OneSet` arms and
// not out-number what `APartysWorth` does, and it must differ from [`KEEPERS`]. It carried a second
// reason for one slice — *"three, not two, because the maintenance rate is a tax on building and two
// nets under a worker-turn"* — and that reason retired with the tax (§4.6a): every hand banks a whole
// worker-turn now, so only the gear window still constrains the number.

/// **The keeping this herd would want, in whole hands** — used only to size the band's *working*
/// count, so `LaborAllocation::normalize` has headroom and cannot trim the very crews the fixture
/// states. It pads **no crew**: both allocations below are stated gross, because a build crew pays
/// none of the rate (`docs/plan_standing_upkeep.md` §4.6a) and `crew − rate` is not the pace.
fn rate_in_hands(app: &App, fauna_id: &str) -> u32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let Some(herd) = registry.find(fauna_id) else {
        return 0;
    };
    // **Measured at the herd's CARRYING CAPACITY**, not at its current stock: the rate rides the
    // keeper load and a Thriving herd grows while it is worked, so a crew sized against today's
    // flock slips back under the rate mid-build and the fixture measures a stall.
    let mut herd = herd.clone();
    herd.biomass = herd.biomass.max(herd.carrying_capacity);
    let herd = &herd;
    ladder
        .rung(core_sim::RungKey::AnimalPastoral)
        .upkeep_crew_needed(core_sim::herd_keeper_load(herd, &fauna))
        .max(
            ladder
                .rung(core_sim::RungKey::AnimalPen)
                .upkeep_crew_needed(core_sim::herd_keeper_load(herd, &fauna)),
        )
}

/// A resident band of [`KEEPERS`] taming `fauna_id` on the [`HANDLING_KIT`], holding `gear`, with
/// `builders` of them on the verb.
fn spawn_taming_keepers(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    gear: GearHeld,
    builders: u32,
) -> bevy::prelude::Entity {
    spawn_keepers_of(app, pos, fauna_id, gear, Some(Improvement::Tame), builders)
}

/// The same band under a **stated** improvement — `None` is the crew that is hunting and *deciding*,
/// which is the state the compose sheet is by definition looking at and where the wire publishes a
/// projection rather than a running countdown.
fn spawn_keepers_of(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    gear: GearHeld,
    improvement: Option<Improvement>,
    // **The BUILD's own crew** — the hands on the verb, now that the player states them
    // (`docs/plan_standing_upkeep.md` §2.2). The gear stamp and the turns quote both read it.
    builders: u32,
) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    let rate = rate_in_hands(app, fauna_id);
    let equipment = EquipmentConfig::builtin();
    let kit = equipment
        .kit(HANDLING_KIT)
        .expect("the shipped roster carries the handling kit");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                size: 200,
                children: scalar_zero(),
                // **THE BAND MUST BE ABLE TO AFFORD BOTH CREWS.** The take and the build are
                // separate allocations drawing on one pool (`docs/plan_standing_upkeep.md` §2.2),
                // so a band of exactly [`KEEPERS`] staffing a build beside them is over-committed
                // and `LaborAllocation::normalize` trims the build away — leaving a fixture that
                // measures a job nobody is doing.
                // **Sized to what it actually staffs** — both crews carry the rate on top of the net
                // they state, so a pool sized at the bare counts lets `normalize` trim the build.
                working: scalar_from_f32((KEEPERS + builders + rate + rate) as f32),
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
            ResidentBand,
            match gear {
                GearHeld::OneSet => BandEquipment::start_stocked(&equipment),
                GearHeld::APartysWorth => {
                    BandEquipment::start_stocked_for(&equipment, KEEPERS as f32)
                }
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor: BUILDER_FLOOR,
                    },
                    // **The crews are stated GROSS here**, and the herd is sized so the maintenance
                    // rate is a single hand ([`TEST_CAPACITY`]) — so both allocations still clear it
                    // comfortably and the nets stay multi-turn, which is what a `ceil` check needs.
                    workers: KEEPERS,
                    improvement,
                    kit: Some(kit),
                    // **The build is staffed by the same keepers**, which is what this fixture meant
                    // when one crew did both jobs (`docs/plan_standing_upkeep.md` §2.2). The
                    // published turns estimate and the gear stamp are both quoted at the BUILD's
                    // crew, so the two have to agree for a closed-form check to mean anything.
                    improvement_workers: improvement.map_or(NO_CREW_ON_THIS_ACTIVITY, |_| builders),
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// **THE CLIENT'S GEAR TERM, transcribed** — `min(workers, buildWorkSaturatingCrew) ×
/// buildWorkPerWorker`, both terms off the band's own `kitTiers` row and neither off any source.
fn client_gear_term(
    build_work_per_worker: f32,
    build_work_saturating_crew: u32,
    workers: u32,
) -> f32 {
    workers.min(build_work_saturating_crew) as f32 * build_work_per_worker
}

/// **THE CLIENT'S FORM, transcribed** — the expression `SourceForecast` evaluates against a proposed
/// crew, here handed the crew and floor the sim actually resolved. Every input is a published wire
/// field; nothing here reads a config.
fn client_turns_estimate(
    work_cost: f32,
    work_done: f32,
    build_work_per_worker: f32,
    build_work_saturating_crew: u32,
    build_work_per_worker_turn: f32,
    // **THE BUILD'S OWN CREW, and it is the ONLY crew in this form**
    // (`docs/plan_standing_upkeep.md` §2.2). Both terms are quoted at it: the work banked per turn,
    // and the gear taken off the job — `build_work_per_worker` is a **rate per worker**, so the
    // count it multiplies has to be the workers actually doing the job. Handing it the band's
    // gathering crew would price a one-hand build with a large party's tools.
    builders: u32,
    // **WHAT THE METER IS LOSING PER TURN**, off `meterRotPerTurn` — the term that eats a build
    // (`docs/plan_standing_upkeep.md` §4.6a). It is emphatically **not** either `UpkeepDemand`: the
    // band's keeping pool owes the rate for every meter carrying work whatever the builders do, so
    // netting a *rate* here would price a build against a bill it does not pay. What a build can
    // fail to out-run is the ground going backwards under it, and the client cannot derive that —
    // it holds neither the grace state nor the rung's decay rate.
    meter_rot_per_turn: f32,
) -> Option<u32> {
    let gear = client_gear_term(build_work_per_worker, build_work_saturating_crew, builders);
    // **NO FLOOR TERM.** The build reads the assignment's escapement floor no longer
    // (`docs/plan_standing_upkeep.md` §2.2): a build is staffed in its own right, so the builders
    // are not pulling on the source and there is nothing of theirs for a floor to describe. The
    // client's form loses the factor with the sim's.
    let work_per_turn =
        (builders as f32 * build_work_per_worker_turn - meter_rot_per_turn).max(0.0);
    if work_per_turn <= 0.0 {
        return None;
    }
    let remaining = work_cost - work_done - gear;
    if remaining <= 0.0 {
        // **A bar the gear alone already pays off is ONE turn, not "no estimate"** — the sim's own
        // `build_turns_remaining` answers `1` there (`docs/plan_unit_costed_work.md` §6.2: a bar at
        // or below zero completes the build on its first worked turn), so a client form that
        // withheld the line would blank the readout at exactly the crew size that demonstrates the
        // arc's claim.
        return Some(CLIENT_BUILD_FINISHES_IN_ONE_TURN);
    }
    Some((remaining / work_per_turn).ceil() as u32)
}

/// **THE EQUALITY THAT LICENSES THE CLIENT TO COMPUTE THIS AT ALL**, asserted in two places at once
/// because the form has two halves that can fail independently:
///
/// 1. **The gear term.** `min(crew, buildWorkSaturatingCrew) × buildWorkPerWorker` — both off the
///    band's own `kitTiers` row — must equal the `buildWorkFromGear` the sim stamped on the source.
///    Those are two different resolutions of one fact (a per-kit saturation point against a
///    coverage-weighted rate over the committed crew), and if they can disagree the sheet prices a
///    job the running readout beside it prices differently.
/// 2. **The turn count.** The whole form must then reproduce `buildTurnsRemaining` exactly.
///
/// Both in **both** gear regimes, because a form missing the `min` is right in one of them and wrong
/// in the other. Liveness rides with it at every step, since the equality passes on a dead feature
/// in several ways: a build that never accrued (both sides quoting a whole untouched job), a
/// saturating crew of zero (the `min` never exercised), a one-turn build (`ceil` untested), and — the
/// one the field exists for — a fixture in which the crew never outnumbers the gear it holds, where
/// the saturation is inert and a naive `workers × worth` would pass.
#[test]
fn the_published_terms_reproduce_the_published_build_turns_at_the_committed_crew() {
    let mut saw_saturated = false;
    let mut saw_linear = false;

    for gear in [GearHeld::OneSet, GearHeld::APartysWorth] {
        let (mut app, id, pos) = world_with_a_tameable_herd();
        // The build's own crew, smaller than the party beside it — the gear stamp reads the
        // BUILD's crew now, so this is the count both halves of the form are judged at.
        let keepers = spawn_taming_keepers(&mut app, pos, &id, gear, THE_BUILD_CREW);
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
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
            .find(|row| row.id == id)
            .expect("the watched herd is on the wire");
        // **The KEEPER band's row, found by entity** — never `populations.first()`, which is the
        // start profile's own band. Its ledger is stocked for seventeen workers, so a fixture
        // reading it would quote a saturating crew this fixture never staffed.
        let band = snapshot
            .populations
            .iter()
            .find(|row| row.entity == keepers.to_bits())
            .expect("the keeper band is on the wire");
        let tiers = band
            .kit_tiers
            .iter()
            .find(|row| row.kit_id == HANDLING_KIT)
            .expect("the band publishes a tier row per roster kit");
        let build_work_per_worker = tiers.build_work_per_worker;
        let saturating_crew = tiers.build_work_saturating_crew;

        // The build actually ran, so neither side is quoting an untouched job.
        assert!(
            herd.tame_work_done > 0.0,
            "fixture: the Tame must have accrued, or the equality is about two whole jobs \
             (work_done {})",
            herd.tame_work_done
        );
        // The gear term is live, so the `min` under test is exercised at all.
        assert!(
            build_work_per_worker > 0.0 && saturating_crew > 0,
            "fixture: the handling kit must publish a build contribution and a crew above zero, got \
             per-worker {build_work_per_worker} crew {saturating_crew}"
        );
        if saturating_crew < THE_BUILD_CREW {
            saw_saturated = true;
        } else {
            saw_linear = true;
        }

        // (1) **THE GEAR TERM** — the two kit-row terms, evaluated at the committed crew, must equal
        // what the sim stamped on the source from that same crew's coverage.
        let staffed = THE_BUILD_CREW;
        let gear = client_gear_term(build_work_per_worker, saturating_crew, staffed);
        assert!(
            (gear - herd.build_work_from_gear).abs() < 1e-3,
            "the kit row's `min({staffed}, {saturating_crew}) × {build_work_per_worker}` must \
             equal the contribution the sim resolved for that same crew: {gear} vs {}",
            herd.build_work_from_gear
        );

        // (1b) **AND THE TWO UPKEEP READOUTS ARE ONE NUMBER ON A SOURCE MID-BUILD.** They answer
        // different questions — what this herd is *billed* now, and what the rung being *quoted*
        // costs to hold — and here those are the same rung, so a drift between the two seams would
        // show up as a disagreement the client's form below would silently inherit.
        assert!(
            herd.upkeep_demand > 0.0,
            "fixture: a mid-Tame herd owes its rung's rate, or this equality is vacuous"
        );
        assert_eq!(
            herd.tame_upkeep_demand, herd.upkeep_demand,
            "a herd raising the pastoral rung is billed exactly what that rung is quoted at"
        );
        // **AND NEITHER OF THEM IS WHAT THE FORM NETS.** The rot is its own published number and,
        // on the animal web, an honest `0`: no animal rung declares a `meter_decay`, so nothing
        // eats an animal build (`docs/plan_standing_upkeep.md` §4.6a). A form that had kept netting
        // the rate would differ from the sim by exactly `upkeep_demand` per turn.
        assert_eq!(
            herd.meter_rot_per_turn, 0.0,
            "an animal meter cannot rot — its shortfall is paid in animals, not in meter"
        );

        // (2) **THE TURN COUNT** — the whole form against the sim's own answer.
        let quoted = client_turns_estimate(
            herd.tame_work_cost,
            herd.tame_work_done,
            build_work_per_worker,
            saturating_crew,
            herd.build_work_per_worker_turn,
            // **The crew the wire publishes**, which is the net the fixture stated plus the rate it
            // also pays — the same number `improvementWorkers` carries.
            staffed,
            // **THE ROT, not either rate.** On the animal web it is always `0` — neither animal
            // rung declares a `meter_decay`, because an under-kept flock sheds animals instead — so
            // this arm also pins that the form nets *nothing* here, which is what makes a Tame's
            // pace `work_cost / crew`.
            herd.meter_rot_per_turn,
        );
        let published = u32::try_from(herd.build_turns_remaining).ok();

        assert!(
            published.is_some_and(|turns| turns > 1),
            "fixture: the sim must quote a multi-turn build, or `ceil` is untested (published {})",
            herd.build_turns_remaining
        );
        assert_eq!(
            quoted, published,
            "the client's closed form must reproduce the sim's own answer at the committed crew: \
             cost {} done {} gear/worker {build_work_per_worker} crew {saturating_crew} \
             per-worker-turn {} builders {THE_BUILD_CREW}",
            herd.tame_work_cost, herd.tame_work_done, herd.build_work_per_worker_turn,
        );
    }

    assert!(
        saw_saturated,
        "fixture: one arm must arm FEWER hands than the build staffs, or the saturation never \
         binds and \
         a form without the `min` would pass"
    );
    assert!(
        saw_linear,
        "fixture: one arm must hold a unit per hand, or saturation is the only regime under test"
    );
}

/// **THE GEAR OFFSET IS QUOTED AT THE BUILD'S CREW, NOT THE BAND'S CREW ON THE SOURCE**
/// (`docs/plan_standing_upkeep.md` §2.2).
///
/// `buildWorkPerWorker` is a **rate per worker**, so the count it multiplies has to be the workers
/// actually doing the job. Reading the take crew instead was reachable the moment the two crews
/// came apart, and — since `LadderConfig::effective_build_cost` is deliberately unfloored — it let a
/// **single** builder standing beside a large gathering party take that whole party's tools off the
/// job, in the worst case paying a rung off outright on its first turn.
///
/// Asserted on three things at once, because each fails independently:
/// 1. the stamp **scales with the build crew** — one builder takes one worker's worth off the job;
/// 2. it **saturates** at the units the band actually holds, so a builder with no gear left to pick
///    up adds nothing further; and
/// 3. it **does not move when only the take crew moves** — the negative control, and the one a form
///    reading the wrong crew fails.
#[test]
fn the_gear_offset_scales_with_the_build_crew_and_ignores_the_take_crew() {
    // `GearHeld::OneSet` is the band's reference ledger: it arms a **prefix** of the party, so the
    // saturation point sits well below the crews swept here and both regimes are exercised.
    let saturating = gear_stamped_for(GearHeld::OneSet, KEEPERS, SOLE_BUILDER);
    assert!(
        saturating > NO_GEAR_AT_ALL,
        "fixture: one set of handling gear must take something off the job, or every arm below is \
         a comparison of zeroes"
    );

    // (1) Two builders take more off the job than one — up to the point the gear runs out.
    let two_builders = gear_stamped_for(GearHeld::APartysWorth, KEEPERS, TWO_BUILDERS);
    let one_builder = gear_stamped_for(GearHeld::APartysWorth, KEEPERS, SOLE_BUILDER);
    assert!(
        two_builders > one_builder,
        "the offset must scale with the BUILD crew (one {one_builder}, two {two_builders})"
    );

    // (2) …and saturates: with one set of gear between them, a second builder finds nothing to
    // pick up, so the offset is the same as one builder's.
    assert_eq!(
        gear_stamped_for(GearHeld::OneSet, KEEPERS, TWO_BUILDERS),
        gear_stamped_for(GearHeld::OneSet, KEEPERS, SOLE_BUILDER),
        "the saturating prefix binds against the BUILD crew — an unarmed builder takes nothing \
         further off the job"
    );

    // (3) The negative control: hold the build crew and double the party gathering beside it. A
    // stamp resolved at the take crew would move here; the shipped one must not.
    assert_eq!(
        gear_stamped_for(GearHeld::APartysWorth, KEEPERS, TWO_BUILDERS),
        gear_stamped_for(GearHeld::APartysWorth, KEEPERS * 2, TWO_BUILDERS),
        "the gathering crew beside a build is not holding the build's tools"
    );
}

/// **One builder** — the smallest crew a build can have, and the one that makes reading the *take*
/// crew instead most obviously wrong.
const SOLE_BUILDER: u32 = 1;
/// Two, so "scales with the build crew" is a comparison rather than a single reading.
const TWO_BUILDERS: u32 = 2;
/// What a crew carrying nothing that helps takes off a job — `intensification::NO_BUILD_GEAR`.
const NO_GEAR_AT_ALL: f32 = 0.0;

/// Resolve one turn of a `Tame` with `take` hands gathering and `builders` on the verb, and answer
/// the `buildWorkFromGear` the sim stamped on the herd.
fn gear_stamped_for(gear: GearHeld, take: u32, builders: u32) -> f32 {
    let (mut app, id, pos) = world_with_a_tameable_herd();
    let keepers = spawn_taming_keepers(&mut app, pos, &id, gear, builders);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(keepers)
            .expect("the keeper band keeps its allocation");
        allocation.assignments[0].workers = take;
    }
    // The band has to afford both crews, or `normalize` trims the build away and the arm measures
    // a job nobody is doing (`docs/plan_standing_upkeep.md` §2.2).
    app.world
        .get_mut::<PopulationCohort>(keepers)
        .expect("the keeper band is a cohort")
        .working = scalar_from_f32((take + builders) as f32);
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    snapshot
        .herds
        .iter()
        .find(|row| row.id == id)
        .expect("the watched herd is on the wire")
        .build_work_from_gear
}

/// A watchdog on the fixture's climb of both animal rungs, **not a model number**: at the shipped
/// costs, this crew and this floor both builds land in a handful of turns each, so a run that
/// exhausts it has stalled rather than been paced.
const MAX_BUILD_TURNS: u32 = 200;

/// One resolved turn in the **real stage order** — `advance_herds` (Logistics, where the display
/// telemetry is rebuilt) then `advance_labor_allocation` (Population, where the build accrues) then
/// the capture. That ordering is the whole point of this file: it is the only place a herd row is
/// assembled from a telemetry entry that is genuinely a turn older than the registry beside it.
fn resolve_one_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// Hand the keeper band its next verb — the fixture climbs Tame → Corral on one crew, because the
/// pen rung is what publishes `corralled`.
fn set_improvement(app: &mut App, band: bevy::prelude::Entity, improvement: Improvement) {
    let mut band = app.world.entity_mut(band);
    let mut allocation = band
        .get_mut::<LaborAllocation>()
        .expect("the keeper band keeps its allocation across the completed build");
    let assignment = allocation
        .assignments
        .first_mut()
        .expect("the keeper band keeps its one assignment");
    assignment.improvement = Some(improvement);
    // **A verb needs a crew** (`docs/plan_standing_upkeep.md` §2.2): completion frees the previous
    // build's hands, so re-staffing the next rung is part of setting it — exactly what the
    // `corral` command does.
    assignment.improvement_workers = KEEPERS;
}

/// **TWO FIELDS DESCRIBING ONE METER MUST AGREE IN THE FRAME THEY SHIP IN.**
///
/// A herd row is assembled from two sources — the display `HerdTelemetry` entry, written in
/// Logistics, and the live `Herd` in the registry, whose build meters accrue in Population *after*
/// it. So the entry's copy of `domestication` / `corralled` / `corralProgress` is always the meter
/// as of the **previous** turn, while `tameWorkDone` / `corralWorkDone` beside them are this turn's.
/// A player finishing a Tame read *"Domesticating 50 / 50 work (99%)"* on the completing turn: one
/// sentence, two frames.
///
/// Asserted **every** turn of a real climb and, pointedly, **on the completing turn**, which is the
/// turn the disagreement is visible on — a `0/0` equality on an untouched herd is vacuous, so the
/// fixture is required to reach completion on both rungs and to have passed through a genuine
/// partial on each.
#[test]
fn a_herds_published_build_meters_agree_with_their_work_pairs_on_the_turn_they_complete() {
    let (mut app, id, pos) = world_with_a_tameable_herd();
    // Both rungs' unlock knowledge up front: this file is about which *frame* a meter is published
    // from, so a knowledge gate that paced the climb would only lengthen the fixture.
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), PENNING_DISCOVERY_ID, scalar_one());
    let keepers = spawn_taming_keepers(&mut app, pos, &id, GearHeld::APartysWorth, KEEPERS);

    let mut tamed_on = None;
    let mut penned_on = None;
    let mut saw_partial_tame = false;
    let mut saw_partial_pen = false;

    for turn in 1..=MAX_BUILD_TURNS {
        resolve_one_turn(&mut app);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let herd = snapshot
            .herds
            .iter()
            .find(|row| row.id == id)
            .expect("the watched herd is on the wire");

        assert_eq!(
            herd.domestication,
            build_fraction(herd.tame_work_done, herd.tame_work_cost),
            "turn {turn}: the published `domestication` must be the published work pair's own \
             fraction — {} / {}",
            herd.tame_work_done,
            herd.tame_work_cost
        );
        assert_eq!(
            herd.corral_progress,
            build_fraction(herd.corral_work_done, herd.corral_work_cost),
            "turn {turn}: the published `corralProgress` must be the published work pair's own \
             fraction — {} / {}",
            herd.corral_work_done,
            herd.corral_work_cost
        );

        if herd.domestication > 0.0 && herd.domestication < 1.0 {
            saw_partial_tame = true;
        }
        if herd.corral_progress > 0.0 && herd.corral_progress < 1.0 {
            saw_partial_pen = true;
        }

        if tamed_on.is_none() {
            if herd.tame_work_done >= herd.tame_work_cost {
                tamed_on = Some(turn);
                assert_eq!(
                    herd.domestication, 1.0,
                    "turn {turn}: the Tame's meter reached its cost, so the row must read a \
                     finished build in the same frame, not last turn's 99%"
                );
                assert!(
                    !herd.corralled,
                    "turn {turn}: a tamed herd is not yet penned"
                );
                set_improvement(&mut app, keepers, Improvement::Corral);
            }
            continue;
        }

        if herd.corral_work_done >= herd.corral_work_cost {
            penned_on = Some(turn);
            assert_eq!(
                herd.corral_progress, 1.0,
                "turn {turn}: the pen's meter reached its cost, so the row must read a finished \
                 build in the same frame"
            );
            assert!(
                herd.corralled,
                "turn {turn}: `corralled` must flip the instant the pen's meter fills — it is the \
                 same fact the meter is stating"
            );
            break;
        }
        assert!(
            !herd.corralled,
            "turn {turn}: `corralled` must not lead its own meter ({} / {})",
            herd.corral_work_done, herd.corral_work_cost
        );
    }

    let tamed_on = tamed_on.expect("fixture: the Tame must complete, or the equality is vacuous");
    let penned_on = penned_on.expect("fixture: the pen must complete, or the equality is vacuous");
    assert!(
        penned_on > tamed_on,
        "fixture: the pen is the rung above the Tame, so it must complete later (tamed {tamed_on}, \
         penned {penned_on})"
    );
    assert!(
        saw_partial_tame,
        "fixture: the Tame must be seen part-built, or the row never had two frames to disagree \
         across"
    );
    assert!(
        saw_partial_pen,
        "fixture: the pen must be seen part-built, or the row never had two frames to disagree \
         across"
    );
}

/// **The client's food-peak constant is the sim's, and nothing else says so.** The form divides the
/// assignment's floor by it (`learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION`), and the
/// client holds its own literal because no config crosses the wire — so a retune of the sim's peak
/// would silently re-scale every turn estimate the sheet draws.
///
/// **IT READS THE GDSCRIPT, and that is what makes this a pin rather than a restatement.** Asserting
/// a third hand-transcribed Rust literal against the sim's constant guards nothing: editing
/// `SourceForecast.gd` fires no test, which is exactly the drift the pairing exists to forbid. So
/// the client's file is parsed for its own `const FLOOR_FOOD_PEAK`, the way
/// `core_sim/tests/tuning_manifest_drift.rs` reads the client's tuning manifest.
/// [`CLIENT_FLOOR_FOOD_PEAK`] survives as the value the closed form above is *evaluated* at, and is
/// asserted equal to both.
#[test]
fn the_clients_food_peak_constant_is_the_sims_own() {
    let declared = client_floor_food_peak();
    assert_eq!(
        declared, MSY_BIOMASS_FRACTION,
        "SourceForecast.gd declares FLOOR_FOOD_PEAK {declared} where the sim's \
         MSY_BIOMASS_FRACTION is {MSY_BIOMASS_FRACTION}; they are separate literals in separate \
         languages and a retune of one must move the other"
    );
    assert_eq!(
        CLIENT_FLOOR_FOOD_PEAK, declared,
        "and the form this file evaluates must be the one the client ships"
    );
}

/// **`SourceForecast.gd`'s own `FLOOR_FOOD_PEAK` literal**, parsed out of the shipped script.
///
/// Located relative to `CARGO_MANIFEST_DIR` rather than an absolute path, so it resolves from any
/// checkout — including the several worktrees this repo is developed in at once
/// (`tuning_manifest_drift.rs`'s rule). A missing file or a renamed constant **panics**: the pin's
/// whole job is to notice that the pairing has moved, and a silent skip would leave two rule files
/// claiming a guard that no longer exists.
fn client_floor_food_peak() -> f32 {
    const DECLARATION: &str = "const FLOOR_FOOD_PEAK :=";
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../clients/godot_thin_client/src/scripts/ui/hud/SourceForecast.gd");
    let script = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "read the client's SourceForecast at {}: {err} — the sim's food peak is pinned against \
             its literal",
            path.display()
        )
    });
    let declaration = script
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(DECLARATION))
        .unwrap_or_else(|| {
            panic!(
                "{} declares no `{DECLARATION}` — if the constant was renamed, re-point this pin \
                 rather than dropping it",
                path.display()
            )
        });
    declaration.trim().parse().unwrap_or_else(|err| {
        panic!("SourceForecast.gd's FLOOR_FOOD_PEAK is not a number ({declaration:?}): {err}")
    })
}

/// **AN OVER-GEARED CREW IS QUOTED ONE TURN ON THE WIRE, NEVER `-1`.**
///
/// `LadderConfig::effective_build_cost` is deliberately unfloored (`cost − t`, the build floor was
/// tried and rejected), so a crew holding enough handling gear drives the bar to or below zero. That
/// is a **finished** job, not an unanswerable one: `docs/plan_unit_costed_work.md` §6.2 says such a
/// bar "completes the build on its first worked turn".
///
/// **It is reachable on the shipped roster at a crew a band can staff** — six keepers each holding a
/// set of handling gear take `6 × 8.5 = 51` units off a 50-unit `Tame` — and publishing
/// `NO_BUILD_TURNS_ESTIMATE` there broke the arc's own headline claim at exactly the crew size that
/// demonstrates it: the estimate fell 25 → 13 → 4 → 2 → *nothing* as hands were added.
///
/// Asserted **off the exported snapshot** in both states the field has: the **projection** a crew
/// that is deciding reads (the compose sheet's case, and the one the player meets), and the **live**
/// stamp of the crew that ran the build. The client's own form is held to the same answer, because a
/// sheet withholding the line while the card states it is the disagreement this file exists to
/// forbid.
#[test]
fn a_crew_whose_gear_pays_the_tame_off_is_quoted_one_turn_on_the_wire() {
    for improvement in [None, Some(Improvement::Tame)] {
        let (mut app, id, pos) = world_with_a_herd_of(UNSCALED_TAMEABLE_SPECIES);
        let keepers = spawn_keepers_of(
            &mut app,
            pos,
            &id,
            GearHeld::APartysWorth,
            improvement,
            KEEPERS,
        );
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
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
            .find(|row| row.id == id)
            .expect("the watched herd is on the wire");
        let band = snapshot
            .populations
            .iter()
            .find(|row| row.entity == keepers.to_bits())
            .expect("the keeper band is on the wire");
        let tiers = band
            .kit_tiers
            .iter()
            .find(|row| row.kit_id == HANDLING_KIT)
            .expect("the band publishes a tier row per roster kit");

        // **The fixture is only about anything if the gear genuinely over-pays the job**, which is
        // the whole regime under test — a crew short of that is the ordinary multi-turn case the
        // test above already covers.
        let gear = client_gear_term(
            tiers.build_work_per_worker,
            tiers.build_work_saturating_crew,
            KEEPERS,
        );
        assert!(
            gear >= herd.tame_work_cost,
            "fixture: {KEEPERS} keepers' gear ({gear}) must cover the whole {} of the Tame, or the \
             bar never reaches zero",
            herd.tame_work_cost
        );

        assert_eq!(
            herd.build_turns_remaining, 1,
            "a Tame the crew's gear pays off outright finishes on its first worked turn — \
             improvement {improvement:?}, cost {} done {} gear {gear}",
            herd.tame_work_cost, herd.tame_work_done
        );

        // And the client's transcribed form must say the same thing, or the compose sheet blanks a
        // line the tile card is rendering.
        assert_eq!(
            client_turns_estimate(
                herd.tame_work_cost,
                herd.tame_work_done,
                tiers.build_work_per_worker,
                tiers.build_work_saturating_crew,
                herd.build_work_per_worker_turn,
                KEEPERS,
                herd.meter_rot_per_turn,
            ),
            u32::try_from(herd.build_turns_remaining).ok(),
            "the client's form must reproduce the sim's answer in the over-geared regime too"
        );
    }
}
