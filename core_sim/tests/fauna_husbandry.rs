//! Phase E husbandry: a sustained Sustain hunt on a Thriving herd tames it into domesticated
//! livestock (emergent accrual + decay), which then yields steady provisions and is immune to the
//! overhunting collapse. Uses the source-centric labor allocation (a Hunt assignment) that replaced
//! the retired persistent follow.

mod pen_materials_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use bevy::math::UVec2;
use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, herd_ecology, quantise_animal_take,
    scalar_from_f32, scalar_one, scalar_zero, spawn_initial_herds, spawn_initial_world,
    CommandEventEntry, CommandEventKind, CommandEventLog, CultureManager, DiscoveryProgressLedger,
    FactionId, FactionInventory, FaunaConfigHandle, ForageRegistry, GenerationId,
    GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry, Improvement,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, RungKey,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SourcePriority, StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
    StartingUnit, TileRegistry, WellbeingConfigHandle, FODDER, FODDERING_DISCOVERY_ID, FOOD,
    FULLY_HERDED, HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION, PENNING_DISCOVERY_ID,
};

/// Whole-worker head-count assigned to the hunt — large enough that the per-worker biomass cap
/// never binds, so a Sustain hunt takes exactly the net regrowth (herd stays Thriving → accrues).
const HUNT_WORKERS: u32 = 5000;

/// **A crew small enough that its CARRY is the binding term.** Since `docs/plan_harvest_floor.md`
/// §3.1 the build dip multiplies `workers × per_worker_carry` rather than the take ceiling, so it is
/// invisible at a staffing the herd's own escapement binds — a build costs yield only while hands
/// are the scarce thing. The dip tests stand a full herd against this crew, which is the regime a
/// real Tame or Corral build lives in.
const DIP_VISIBLE_HUNTERS: u32 = 2;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
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

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    // **This harness is a deterministic pin, so the retreat stage is held at its identity.**
    // Slice 7 authored a non-zero `combat.wariness` across the roster
    // (`docs/plan_hunt_through_combat.md` §3.1); `FaunaConfig::without_retreat` carries the whole
    // reasoning for why the pre-existing suite neutralises it rather than re-baselining.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    // **The road ledger `advance_labor_allocation` counts spare road keepers against.** Empty
    // is the shipped turn-1 state — no traffic has worn anything in yet — so this harness's
    // keeping numbers are the roadless reading they have always been.
    app.world.insert_resource(core_sim::RoadRegistry::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// The species every husbandry-ladder fixture in this file runs on, **pinned by name on purpose**.
///
/// These tests measure *rates* — the Tame dip, the pen's MSY harvest, its upkeep debit — and a take
/// is quantized to **whole animals**. So the fixture needs a species whose one-turn MSY comfortably
/// exceeds one `body_mass`; on a heavy-bodied or slow-breeding one the same correct code takes `0`
/// this turn and banks kill-credit instead, and the assertions read a rate of zero. The Rabbit
/// Warren (`r` 0.35, the lightest body on the roster, `pen` husbandry ceiling) is the species that
/// makes every rung's rate visible in a single turn.
///
/// **It used to be whichever short-range herd worldgen happened to place first**, which is an
/// incidental dependency on map generation, not a property of the mechanic under test: a
/// `macro_land` retune that moved one herd swapped the fixture to Crag Goats and failed four tests
/// with "expected ~0.2, got 0" — a fixture artifact wearing the costume of a husbandry regression.
const FIXTURE_SPECIES: &str = "Rabbit Warren";

/// A stationary [`FIXTURE_SPECIES`] herd (route length 1) primed to half its cap → Thriving and a
/// clean domestication candidate. Returns its id.
/// **⛔ AN UNKEPT HERD STILL GROWS AT ITS LAND'S RATE — the growth freeze is deleted.**
///
/// `regrow_biomass` used to zero the growth of an owned herd whose keeping went wholly unmet. It is
/// gone: *a herd's growth is a fact about the land it stands on, not about who is watching it.* The
/// price of not keeping a herd is that it **leaves**, and that is the whole of it — a second penalty
/// on the same trigger made neglect cost twice and made the two impossible to tune apart.
///
/// **The pair is the test.** *"It still grows"* alone would pass on a herd that grew and never shed,
/// which would delete the neglect penalty altogether; *"it still sheds"* alone was already true.
/// Asserted on ONE turn's regrowth, in isolation from the shed, so the growth term is read directly
/// rather than inferred from a net movement the shed also touches.
#[test]
fn an_unkept_herd_still_grows_at_its_lands_rate_and_still_sheds() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);

    // Seat it below capacity so there is real growth to see, with NOBODY keeping it.
    const ROOM_TO_GROW: f32 = 0.5;
    let (before, cap) = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.biomass = herd.carrying_capacity * ROOM_TO_GROW;
        herd.upkeep_supplied = 0.0;
        (herd.biomass, herd.carrying_capacity)
    };
    assert!(
        herd_of(&app, &id).owner.is_some(),
        "fixture: the herd must be OWNED, or the retired gate would never have fired on it"
    );

    // **Logistics regrowth alone**, without the shed — `advance_herds` runs `regrow_biomass`.
    app.world.run_system_once(advance_herds);
    let grown = herd_of(&app, &id).biomass;
    assert!(
        grown > before,
        "an unkept herd still grows at the land's rate: {before} -> {grown} (cap {cap})"
    );

    // …and it still pays the real price: the shed takes animals every turn past the grace.
    let before_shed = herd_of(&app, &id).biomass;
    run_turns_untended(&mut app, TURNS_PAST_THE_SHEDS_GRACE);
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .map_or(0.0, |herd| herd.biomass);
    assert!(
        after < before_shed,
        "…and it still SHEDS, which is now the only penalty for not keeping it: \
         {before_shed} -> {after}"
    );
}

/// Enough turns for the pastoral rung's neglect grace to be spent and the shed to have bitten
/// several times, so the comparison is not reading a forgiven turn.
const TURNS_PAST_THE_SHEDS_GRACE: u32 = 8;

/// **Set the escape acceleration for one fixture**, so the off-switch and the shipped value can be
/// measured on the same world.
fn set_escape_acceleration(app: &mut App, accel: f32) {
    let mut fauna = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
    fauna.husbandry.escape_acceleration = accel;
    app.world
        .insert_resource(FaunaConfigHandle::new(std::sync::Arc::new(fauna)));
}

/// Turns to run a wholly unkept herd out — comfortably past the ~30 the shipped acceleration needs.
const TURNS_TO_RUN_A_HERD_OUT: u32 = 120;

/// **⛔ A WHOLLY UNKEPT HERD ENDS AT NOTHING** — the design, restored. *"If no herders are present,
/// eventually, the entire herd leaves and you are left with nothing."*
///
/// Before `escape_acceleration`, a constant shed fraction balanced against the growth curve and the
/// herd settled at ~0.64·K, **still owned, for ever**. Both halves are asserted: the animals go, and
/// the claim on them goes with them.
///
/// **The preconditions are load-bearing** — a herd that was never owned, or that started empty, ends
/// at nothing for reasons that have nothing to do with the shed.
#[test]
fn a_wholly_unkept_herd_ends_at_nothing_and_loses_its_owner() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);

    let started_full = herd_of(&app, &id).biomass;
    assert!(
        started_full > 0.0,
        "PRECONDITION: the herd must start with animals in it"
    );
    assert!(
        herd_of(&app, &id).owner.is_some(),
        "PRECONDITION: and it must be OWNED, or 'loses its owner' is vacuous"
    );

    run_turns_untended(&mut app, TURNS_TO_RUN_A_HERD_OUT);

    match app.world.resource::<HerdRegistry>().find(&id) {
        None => {} // despawned on shed-to-zero — the whole of "you are left with nothing"
        Some(herd) => panic!(
            "an unkept herd must end at nothing, not settle: {} biomass left, owner {:?}",
            herd.biomass, herd.owner
        ),
    }
}

/// **⛔ THE DEPARTURE ACCELERATES** — *"the longer you don't tend it, the quicker the remaining herd
/// leaves, meaning it isn't linear."*
///
/// # It is asserted on the RATE, and the raw head count is the wrong statistic
///
/// The shed takes a *fraction* of the overage, and the overage shrinks with the herd — so a rising
/// rate and a falling stock fight each other in the raw count, and the count can be flat or even
/// falling while the rate doubles. Measured at the shipped `0.05`, the per-turn losses run
/// `…6 6 5 6 6 6 7 7 6 8 7…`: the acceleration is real and the count barely shows it. Asserting the
/// count would therefore be asserting noise, and would pass on a **constant** rate about as often as
/// not. The fraction of the standing herd that leaves is what "quicker" means, and it is monotone.
#[test]
fn the_departure_accelerates_rather_than_holding_a_constant_rate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    // **No jitter**, so the shape under test is the acceleration and not the seeded ±25% band.
    {
        let mut fauna = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
        fauna.husbandry.escape_fraction_jitter = 0.0;
        app.world
            .insert_resource(FaunaConfigHandle::new(std::sync::Arc::new(fauna)));
    }

    let mut shed_fractions: Vec<f32> = Vec::new();
    for _ in 0..12 {
        let before = match app.world.resource::<HerdRegistry>().find(&id) {
            Some(herd) => herd.biomass,
            None => break,
        };
        run_turns_untended(&mut app, 1);
        let after = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .map_or(0.0, |herd| herd.biomass);
        if after < before && before > 0.0 {
            shed_fractions.push((before - after) / before);
        }
    }

    assert!(
        shed_fractions.len() >= 3,
        "PRECONDITION: the herd must actually shed on several turns, or there is no shape to read \
         ({} shedding turns)",
        shed_fractions.len()
    );
    // **THE TAIL, NOT THE WHOLE SEQUENCE.** The opening turns are dominated by the herd's own
    // regrowth — it is still climbing toward `K` while the shed starts, so the share-of-standing-herd
    // ratio *falls* for a few turns before the acceleration takes over. Measured, the sequence runs
    // `0.159 0.106 0.083 0.076 0.0735 0.0756 0.079 0.084 0.094 0.104`: a trough, then a strict climb.
    // The claim is about the climb, so the assertion reads the tail.
    const TAIL: usize = 5;
    assert!(
        shed_fractions.len() >= TAIL,
        "PRECONDITION: not enough shedding turns to read a tail ({shed_fractions:?})"
    );
    let tail = &shed_fractions[shed_fractions.len() - TAIL..];
    for pair in tail.windows(2) {
        assert!(
            pair[1] > pair[0],
            "each turn must take a LARGER share of what is left than the one before it — that is \
             what 'the longer you don't tend it, the quicker it goes' means, and a CONSTANT rate \
             would read flat here: {pair:?} within {shed_fractions:?}"
        );
    }
}

/// **THE OFF-SWITCH IS HONEST** — `escape_acceleration = 0` reproduces the constant-rate behaviour
/// exactly, which is the state that settled at an equilibrium. Asserted because a dial documented as
/// having a legible "off" must actually have one.
#[test]
fn a_zero_escape_acceleration_reproduces_the_constant_rate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    set_escape_acceleration(&mut app, 0.0);
    domesticate(&mut app, &id);

    run_turns_untended(&mut app, TURNS_TO_RUN_A_HERD_OUT);

    let herd =
        app.world.resource::<HerdRegistry>().find(&id).expect(
            "with no acceleration the shed balances the growth curve and the herd SURVIVES",
        );
    assert!(
        herd.biomass > 0.0 && herd.owner.is_some(),
        "…at an equilibrium, still owned — which is exactly the defect the acceleration fixes"
    );
}

/// **A PENNED HERD STILL TERMINATES, AND THE FENCE IS WORTH TIME RATHER THAN IMMUNITY.** It starts
/// from the slower `pen_escape_fraction` and accelerates from there, so it arrives at nothing later.
///
/// **Both halves matter**: "terminates" alone would pass on a fence that bought nothing, and "slower"
/// alone would pass on a fence that saved the herd outright — which is what the pen used to do, by
/// accident, because a separate unfed-pen gate stopped its growth.
#[test]
fn a_penned_herd_still_terminates_but_the_fence_buys_time() {
    let run = |penned: bool| -> Option<u32> {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);
        if penned {
            corral_herd(&mut app, &id);
        }
        for turn in 1..=TURNS_TO_RUN_A_HERD_OUT {
            run_turns_untended(&mut app, 1);
            if app.world.resource::<HerdRegistry>().find(&id).is_none() {
                return Some(turn);
            }
        }
        None
    };

    let open_range = run(false).expect("an unfenced herd ends at nothing");
    let penned = run(true).expect("and so does a penned one — the fence delays, it does not save");
    assert!(
        penned > open_range,
        "the fence must buy TIME: penned ended on turn {penned}, open range on turn {open_range}"
    );
}

/// Enough consecutive unkept turns to put real pressure on a herd without killing it.
const TURNS_TO_FRAY_A_HERD: u32 = 4;

/// **⛔ ONE GOOD TURN DOES NOT ERASE ACCUMULATED NEGLECT — the test this whole amendment exists for.**
///
/// The escape acceleration used to key off `neglect_turns`, which **resets outright** when the bill is
/// met. Measured, that let a herd survive indefinitely on **one tended turn in fourteen**, at *above*
/// its starting size: every good turn wiped the entire acceleration. `Herd::neglect_pressure` is a
/// separate meter that **decays** instead, so attention helps and does not absolve.
///
/// **Lower AND non-zero are both asserted**, with the precondition that the pressure was substantially
/// above zero first — a herd that was barely frayed would read "non-zero" for a rounding.
#[test]
fn one_tended_turn_lowers_the_neglect_pressure_without_erasing_it() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    run_turns_untended(&mut app, TURNS_TO_FRAY_A_HERD);

    let frayed = herd_of(&app, &id).neglect_pressure;
    // **THE PRECONDITION** — really frayed, not a rounding away from zero.
    assert!(
        frayed > 1.0,
        "PRECONDITION: {TURNS_TO_FRAY_A_HERD} unkept turns must build real pressure, got {frayed}"
    );

    keep_herd_for_a_turn(&mut app, &id);
    run_turns_untended(&mut app, 1);
    let after = herd_of(&app, &id).neglect_pressure;

    assert!(
        after < frayed,
        "one tended turn must LOWER the pressure — attention has to count for something: \
         {frayed} -> {after}"
    );
    assert!(
        after > 0.0,
        "…and must NOT erase it: {frayed} -> {after}. A full reset is what let a herd be held for \
         ever on one tended turn in fourteen"
    );
}

/// **PARTIAL KEEPING FRAYS A HERD MORE SLOWLY THAN NONE** — the *"I tend it, but not enough"* case.
/// The pressure rises by the **shortfall fraction**, so half-staffed keeping raises it at half speed,
/// the same proportionality the shed and the plant rot already use.
///
/// **Both arms must actually be rising**, or "slower" would pass on a pair where one of them was flat.
#[test]
fn partial_keeping_frays_a_herd_more_slowly_than_none() {
    let pressure_after = |share: f32| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);
        for _ in 0..TURNS_TO_FRAY_A_HERD {
            if share > 0.0 {
                let fauna = app.world.resource::<FaunaConfigHandle>().get();
                let ladder = app.world.resource::<LadderConfigHandle>().get();
                let part =
                    core_sim::herd_upkeep_demand(&herd_of(&app, &id), &fauna, &ladder) * share;
                if let Some(herd) = app
                    .world
                    .resource_mut::<HerdRegistry>()
                    .herds
                    .iter_mut()
                    .find(|herd| herd.id == id)
                {
                    herd.upkeep_supplied = part;
                }
            }
            run_turns_untended(&mut app, 1);
        }
        herd_of(&app, &id).neglect_pressure
    };

    const HALF_STAFFED: f32 = 0.5;
    let none = pressure_after(0.0);
    let half = pressure_after(HALF_STAFFED);

    // **THE PRECONDITION** — both are rising, so "slower" is a comparison of two climbs.
    assert!(
        none > 0.0 && half > 0.0,
        "PRECONDITION: both arms must accumulate pressure — none {none}, half {half}"
    );
    assert!(
        half < none,
        "half-staffed keeping must fray the herd more slowly than none at all: {half} against {none}"
    );
}

/// **SUSTAINED KEEPING WORKS THE PRESSURE BACK TO ZERO** — recovery is real, not merely a slowdown.
/// Without this the meter would be a ratchet, and a herd once neglected could never be made whole.
#[test]
fn sustained_keeping_works_the_neglect_pressure_back_to_zero() {
    /// Comfortably more than the `4:1` the shipped recovery rate implies for this much fraying.
    const KEPT_TURNS: u32 = 40;
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    run_turns_untended(&mut app, TURNS_TO_FRAY_A_HERD);
    assert!(
        herd_of(&app, &id).neglect_pressure > 1.0,
        "PRECONDITION: there must be real pressure to work off"
    );

    for _ in 0..KEPT_TURNS {
        keep_herd_for_a_turn(&mut app, &id);
        run_turns_untended(&mut app, 1);
    }
    assert_eq!(
        herd_of(&app, &id).neglect_pressure,
        0.0,
        "sustained keeping must bring the pressure all the way back — a meter that only slowed \
         would make one bad season permanent"
    );
}

/// **THE GRACE STILL RESETS OUTRIGHT** — it is a *different quantity* from the pressure and this
/// change must not have caught it.
///
/// `neglect_turns` is a **forgiveness window**: how long before the penalty starts. Resetting it is
/// correct — you tended, you earned the window back — and its own comment says it measures
/// *consecutive* shortfall "rather than a lifetime budget". Only the herd's **condition** decays.
#[test]
fn the_grace_still_resets_outright_while_the_pressure_only_decays() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    run_turns_untended(&mut app, TURNS_TO_FRAY_A_HERD);
    assert!(
        herd_of(&app, &id).neglect_turns > 0,
        "PRECONDITION: the grace counter must have advanced"
    );

    keep_herd_for_a_turn(&mut app, &id);
    run_turns_untended(&mut app, 1);

    assert_eq!(
        herd_of(&app, &id).neglect_turns,
        0,
        "the grace resets OUTRIGHT on a tended turn — it is forgiveness, not condition"
    );
    assert!(
        herd_of(&app, &id).neglect_pressure > 0.0,
        "…while the pressure beside it does not: the two are different quantities, and only one of \
         them is erasable"
    );
}

/// **⛔ KEEPING NEVER REPAIRS DAMAGE.** Animals that left are gone; only *work* — re-breeding, and on
/// the build side re-taming and re-queueing — brings a herd back. Tending a herd restores its
/// *condition*, never its losses.
///
/// This guards a rule the acceleration work came close to breaking: it is the shipped rule on both
/// webs (`a_rung_completes_erodes_and_is_repaired_only_by_re_queueing_it` is its plant twin), and a
/// recovery meter that also handed animals back would quietly make neglect free.
#[test]
fn keeping_restores_condition_but_never_the_animals_that_left() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    let before = herd_of(&app, &id).biomass;

    // Long enough past the grace that the shed has actually taken animals — the herd regrows
    // toward `K` at first, so a short window measures growth rather than loss.
    run_turns_untended(&mut app, 14);
    let after_shedding = herd_of(&app, &id).biomass;
    assert!(
        after_shedding < before,
        "PRECONDITION: the herd must actually have LOST animals: {before} -> {after_shedding}"
    );

    // Now keep it perfectly. Its condition recovers; its losses do not come back.
    // At the shipped 4:1 recovery, 14 unkept turns need ~56 kept ones — this is comfortably past it.
    const KEPT_TURNS_TO_CLEAR: u32 = 80;
    let mut best = after_shedding;
    for _ in 0..KEPT_TURNS_TO_CLEAR {
        keep_herd_for_a_turn(&mut app, &id);
        run_turns_untended(&mut app, 1);
        best = best.max(herd_of(&app, &id).biomass);
    }
    assert_eq!(
        herd_of(&app, &id).neglect_pressure,
        0.0,
        "keeping restores CONDITION — the pressure comes back to zero"
    );
    // The herd may re-*breed* toward its capacity, which is growth and not repair. What must never
    // happen is the shed handing anything back: the animals went to the wild web.
    assert!(
        best <= herd_of(&app, &id).carrying_capacity + 1e-3,
        "…but nothing is ever handed BACK — a herd only regrows within its own capacity: {best} \
         against K {}",
        herd_of(&app, &id).carrying_capacity
    );
}

/// **⛔ THE TWO KEEPER QUESTIONS, SIDE BY SIDE, UP A RUNG** — the harness that found the defect and
/// the one that now shows the shape of the fix.
///
/// The wire states `upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT)` and tells the
/// client to do no arithmetic of its own. It was false for most of a `Tame`: the bill interpolates on
/// the herd's position and `upkeepWorkersNeeded` was published from `herd_herders_needed`, which reads
/// the rung's **bare** rate — 0.185 work billed against two keepers demanded, at a tenth of the way up.
///
/// What this prints now is the settled model: **`workersNeeded` is `ceil` of the bill and climbs with
/// the position; `herdersNeeded` is the HEAD-COUNT requirement and is flat.** They meet at the top of
/// the rung. The identity column is the guard's subject — `snapshot::tests::
/// the_published_upkeep_crew_is_the_ceil_of_the_published_bill_all_the_way_up_a_rung` asserts it swept,
/// on both webs, against the exported rows.
///
/// Run with `cargo test -p core_sim --test fauna_husbandry probe_the_herd_rows_self_consistency --
/// --ignored --nocapture`.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_herd_rows_self_consistency() {
    let ladder = core_sim::LadderConfig::builtin();
    let fauna = core_sim::FaunaConfig::builtin();
    println!("\n=== the herd row's stated identity: workersNeeded == ceil(demand) ===");
    for fraction in [0.0f32, 0.1, 0.25, 0.5, 0.9, 1.0] {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            let cost = herd.rung_cost(core_sim::RungKey::AnimalPastoral, &ladder);
            herd.set_ladder_position(cost * fraction, &ladder);
        }
        let herd = herd_of(&app, &id);
        let demand = core_sim::herd_keeping_basis(&herd, &fauna, &ladder);
        // **The published pair, each from its own seam** — the bill's crew and the head-count
        // requirement, which is the whole point of printing them in one row.
        let needed = core_sim::herd_upkeep_workers_needed(&herd, &fauna, &ladder);
        let heads = core_sim::herd_herders_needed(&herd, &fauna, &ladder);
        let implied = (demand / core_sim::PER_WORKER_OUTPUT).ceil() as u32;
        println!(
            "  {fraction:>4} up the rung: demand {demand:6.3}  workersNeeded {needed}  ceil(demand) {implied}  herdersNeeded {heads}  {}",
            if needed == implied { "agree" } else { "*** DISAGREE ***" }
        );
    }
}

/// **⛔ DOES `advance_labor_allocation` DO ANYTHING ON A SECOND CONSECUTIVE CALL?** — the sweep item
/// from task 1. If it does not, every two-turn test driver in the suite that drives the labor system
/// twice without a Logistics pass between is quietly measuring **one** turn.
///
/// Run with `cargo test -p core_sim --test fauna_husbandry probe_the_double_labor_pass --
/// --ignored --nocapture`.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_double_labor_pass() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    grant_herding(&mut app);
    let (tile, coord) = (herd_of(&app, &id).position(), herd_of(&app, &id).position());
    let _ = (tile, coord);
    domesticate(&mut app, &id);
    let keepers = keeper_crew(&app, &id);
    spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);

    println!("\n=== advance_labor_allocation, called repeatedly with NO Logistics between ===");
    for pass in 1..=4 {
        app.world.run_system_once(advance_labor_allocation);
        let herd = herd_of(&app, &id);
        println!(
            "  pass {pass}: upkeep_supplied = {:8.4}   upkeep_demanded = {:?}   biomass = {:8.2}",
            herd.upkeep_supplied, herd.upkeep_demanded, herd.biomass
        );
    }
    println!("\n=== the same, with advance_husbandry between (the real stage order) ===");
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    grant_herding(&mut app);
    domesticate(&mut app, &id);
    let keepers = keeper_crew(&app, &id);
    spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);
    for pass in 1..=4 {
        app.world.run_system_once(advance_labor_allocation);
        let herd = herd_of(&app, &id);
        println!(
            "  pass {pass}: upkeep_supplied = {:8.4}   biomass = {:8.2}",
            herd.upkeep_supplied, herd.biomass
        );
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
}

/// **⛔ WHAT ONE TENDED TURN BUYS, AND WHAT FULL RECOVERY COSTS** — the pair §4.14 tunes
/// `husbandry.neglect_recovery_rate` against.
///
/// The pressure rises by the shortfall fraction (`1.0` a turn on a wholly unkept herd) and falls by
/// the recovery rate, so the asymmetry is the whole design: **N turns of neglect take more than N
/// turns of good keeping to work off.** This reports both ends of it — how long a frayed herd takes
/// to come back to zero under sustained keeping, and how much delay a *single* tended turn buys.
///
/// It replaces the token-attention measurement: with the acceleration keyed to the grace, a herd
/// survived indefinitely on one tended turn in fourteen, at above its starting size. The pressure
/// meter is what closed that, and these are the numbers that say whether it closed it by the right
/// amount.
///
/// Run with `cargo test -p core_sim --test fauna_husbandry probe_the_price_of_recovery --
/// --ignored --nocapture`.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_price_of_recovery() {
    println!("\n=== THE PRICE OF RECOVERY ===");
    let rate = core_sim::FaunaConfig::builtin()
        .husbandry
        .neglect_recovery_rate;
    println!("  neglect_recovery_rate = {rate} (pressure rises 1.0 per wholly-unkept turn)");

    println!("\n  (a) sustained keeping — kept turns to bring the pressure back to zero:");
    for neglected in [2u32, 4, 8, 12] {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);
        run_turns_untended(&mut app, neglected);
        let peak = herd_of(&app, &id).neglect_pressure;
        let mut kept = 0u32;
        while herd_of(&app, &id).neglect_pressure > 0.0 && kept < 200 {
            keep_herd_for_a_turn(&mut app, &id);
            run_turns_untended(&mut app, 1);
            kept += 1;
        }
        println!(
            "    {neglected:2} unkept turns -> pressure {peak:5.2}; {kept:3} kept turns to clear it \
             ({:.1} kept per unkept)",
            kept as f32 / neglected as f32
        );
    }

    println!("\n  (b) ONE tended turn — the delay it buys against dying:");
    let died_on = |tended_at: Option<u32>| -> Option<u32> {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);
        for turn in 1..=200u32 {
            if Some(turn) == tended_at {
                keep_herd_for_a_turn(&mut app, &id);
            }
            run_turns_untended(&mut app, 1);
            if app.world.resource::<HerdRegistry>().find(&id).is_none() {
                return Some(turn);
            }
        }
        None
    };
    let baseline = died_on(None);
    println!("    never tended:        died on turn {baseline:?}");
    for at in [5u32, 10, 15] {
        let with = died_on(Some(at));
        println!(
            "    one tended turn @{at:2}: died on turn {with:?} (buys {} turns)",
            match (with, baseline) {
                (Some(w), Some(b)) => format!("{}", w as i64 - b as i64),
                _ => "n/a".to_string(),
            }
        );
    }
}

/// **Meet this herd's keeping bill for one turn**, exactly as a staffed `husbandry` role would — the
/// fixture's stand-in, and the only way to exercise the pressure's *fall*.
fn keep_herd_for_a_turn(app: &mut App, id: &str) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    // **Comfortably above the bill, which is what a fully staffed pool is.** `advance_herds` regrows
    // the herd between this stamp and the turn that reads it, which raises the keeper load and hence
    // the demand — stamping the bill exactly would leave the herd fractionally short every turn and
    // the pressure could never fall.
    const A_FULLY_STAFFED_POOL: f32 = 4.0;
    let demand =
        core_sim::herd_upkeep_demand(&herd_of(app, id), &fauna, &ladder) * A_FULLY_STAFFED_POOL;
    if let Some(herd) = app
        .world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
    {
        herd.upkeep_supplied = demand;
    }
}

/// **⛔ WHAT BECOMES OF AN ABANDONED HERD, now that its growth is no longer frozen.**
///
/// The retired `abandoned_pastoral` gate in `regrow_biomass` zeroed the growth of an owned herd whose
/// keeping went wholly unmet, and its own comment claimed that freeze was what made such a herd go
/// **fully feral** *"instead of persisting at a leaky ~0.6·K equilibrium"*. Deleting it therefore has
/// to be measured rather than argued: if the shed no longer outruns the growth, abandonment stops
/// ending in a wild herd and starts ending in a permanent half-sized tame one.
///
/// Reports, for a fully domesticated herd and for a penned one, both with **zero** keeping staffed:
/// the biomass curve as a fraction of `K`, whether ownership clears, and — if it settles instead —
/// the equilibrium.
///
/// Run with `cargo test -p core_sim --test fauna_husbandry probe_the_abandoned_herds_fate --
/// --ignored --nocapture`.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_abandoned_herds_fate() {
    /// Long enough for either outcome to be unambiguous: a 25%/turn shed empties a herd in well
    /// under this, and an equilibrium is flat long before it.
    const TURNS: u32 = 120;

    for (penned, accel) in [
        (false, 0.02),
        (false, 0.05),
        (false, 0.10),
        (true, 0.05),
        (false, 0.0),
    ] {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        {
            // The dial under measurement, overridden per arm.
            let mut fauna = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
            fauna.husbandry.escape_acceleration = accel;
            app.world
                .insert_resource(FaunaConfigHandle::new(std::sync::Arc::new(fauna)));
        }
        domesticate(&mut app, &id);
        if penned {
            corral_herd(&mut app, &id);
        }
        let body = herd_of(&app, &id).body_mass.max(1.0);
        let cap = herd_of(&app, &id).carrying_capacity.max(1.0);
        let mut curve: Vec<String> = Vec::new();
        let mut cleared_on = None;
        // **Both bases, printed together and labelled.** A figure quoted as "turns" without saying
        // *which* turns is what let the report and the shipped config comment drift apart: one was
        // counting from turn 1, the other from the end of the grace.
        let grace = {
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            let key = if penned {
                core_sim::RungKey::AnimalPen
            } else {
                core_sim::RungKey::AnimalPastoral
            };
            ladder.rung(key).upkeep_grace_turns()
        };

        for turn in 1..=TURNS {
            // **NOBODY KEEPING IT** — `advance_husbandry` clears `upkeep_supplied` each turn and the
            // fixture never restaffs it, so the herd is short by its whole demand every turn.
            run_turns_untended(&mut app, 1);
            let gone = app.world.resource::<HerdRegistry>().find(&id).is_none();
            let owned = app
                .world
                .resource::<HerdRegistry>()
                .find(&id)
                .and_then(|herd| herd.owner)
                .is_some();
            let fraction = app
                .world
                .resource::<HerdRegistry>()
                .find(&id)
                .map_or(0.0, |herd| herd.biomass / cap);
            if turn <= 40 {
                // **HEAD COUNT**, not a fraction of `K`: the ruling is about animals leaving, and a
                // fraction of a capacity that is itself moving hides the shape.
                let heads = app
                    .world
                    .resource::<HerdRegistry>()
                    .find(&id)
                    .map_or(0.0, |herd| herd.biomass / body);
                curve.push(format!("{heads:.0}"));
            }
            let _ = fraction;
            if (gone || !owned) && cleared_on.is_none() {
                cleared_on = Some(turn);
            }
            if gone {
                break;
            }
        }

        let end = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .map(|herd| (herd.biomass / cap, herd.owner.is_some()));
        println!(
            "\n=== {} herd, ZERO keeping, escape_acceleration = {accel} ===",
            if penned { "PENNED" } else { "PASTORAL" }
        );
        println!(
            "  gone on: {}",
            cleared_on.map_or("NEVER".to_string(), |t| format!(
                "TURN {t} (= {} turns past the {grace}-turn grace)",
                t.saturating_sub(grace)
            ))
        );
        match end {
            None => println!("  ended: herd despawned (fully feral / gone)"),
            Some((f, owned)) => println!(
                "  ended: B/K = {f:.4}, still owned = {owned} -> EQUILIBRIUM at {:.1}% of K",
                f * 100.0
            ),
        }
        println!("  head count per turn: {}", curve.join(" "));
    }
}

fn prime_thriving_herd(app: &mut App) -> String {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| {
                h.id.starts_with("game_") && h.route_length() == 1 && h.species == FIXTURE_SPECIES
            })
            .map(|h| h.id.clone())
            .unwrap_or_else(|| {
                panic!(
                    "husbandry fixtures need a stationary {FIXTURE_SPECIES} on the generated map \
                     and worldgen placed none — see FIXTURE_SPECIES for why the species is pinned. \
                     Re-point the constant at another light-bodied, fast-breeding `pen`-ceiling \
                     species rather than falling back to an arbitrary herd."
                )
            })
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = (herd.carrying_capacity * 0.5).max(1.0);
    id
}

/// A band hunting `herd_id` under `policy`, building nothing. The improvement axis has its own
/// helper — [`spawn_builder`] — because the two are independent (issue #442).
fn spawn_hunter(app: &mut App, herd_id: &str, policy: f32) -> bevy::prelude::Entity {
    spawn_crew(app, herd_id, policy, None)
}

/// **THE KEEPER CREW A BUILD FIXTURE STAFFS, and it is deliberately NOT [`HUNT_WORKERS`]** — the
/// herd's own `would_be_herders_needed`, which is the crew the animal web sizes a managed herd's
/// labor with.
///
/// Two reasons that now coincide. **Under-staffing it sheds animals**, which would confound every
/// measurement below. And since the crew became the build's *throughput*
/// (`docs/plan_unit_costed_work.md` §1.2) it is also the honest answer to *"how long does this
/// take"*: an animal build's pace is its herd's own keeper crew, where the rung declares none.
///
/// 5000 was chosen so a *take* is ceiling-bound rather than labor-bound; it became a **build-pacing**
/// number only when the crew stopped being capped, and at that head count every animal build
/// finishes in a single turn — leaving no part-built pen and no per-species pace to measure at all.
/// **That one-turn over-crewed build is real and is pinned on purpose** by
/// `a_bigger_keeper_crew_tames_materially_faster`.
fn keeper_crew(app: &App, herd_id: &str) -> u32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let herd = herd_of(app, herd_id);
    core_sim::would_be_herders_needed(&herd, &fauna, &ladder)
}

/// **The keeper crew this herd would want at its CARRYING CAPACITY** — the largest the animal
/// rungs' maintenance rate can grow to while a build runs, since a Thriving herd recovers as it is
/// worked. See [`spawn_builder`] for why a build fixture must be sized against it.
fn keeper_crew_at_capacity(app: &App, herd_id: &str) -> u32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let mut herd = herd_of(app, herd_id);
    // **The capacity a MANAGED herd actually grows to**, not the raw wild field: a herd picks up an
    // owner on the first accrual and moves onto the pastoral ecology, whose capacity is higher — so
    // sizing off `carrying_capacity` leaves the crew under the live rate by the time the build is
    // half done, and the fixture measures a stall.
    herd.owner = Some(FactionId(0));
    herd.biomass = core_sim::herd_capacity(&herd, &fauna).max(herd.carrying_capacity);
    // **The RAW rung reading at that load**, deliberately not `would_be_herders_needed`: that one
    // prefers the herd's *stabilized* `herders_needed`, which is last turn's count and would ignore
    // the biomass this helper just set — the whole point of the helper. Taken at the DEARER of the
    // two managed rungs, so the crew clears the rate on either side of a Corral as well.
    let load = core_sim::herd_keeper_load(&herd, &fauna);
    ladder
        .rung(RungKey::AnimalPastoral)
        .upkeep_crew_needed(load)
        .max(ladder.rung(RungKey::AnimalPen).upkeep_crew_needed(load))
}

/// A band hunting `herd_id` at the food peak while **building** `improvement`, staffed at the herd's
/// own keeper crew **plus one hand**.
///
/// The build's **net** supply is this crew: `spawn_crew_of` adds the maintenance rate the builders
/// are also paying, so the meter advances at the herd's own keeper crew per turn.
fn spawn_builder(app: &mut App, herd_id: &str, improvement: Improvement) -> bevy::prelude::Entity {
    let keepers = keeper_crew(app, herd_id);
    spawn_crew_of(
        app,
        herd_id,
        MSY_BIOMASS_FRACTION,
        Some(improvement),
        keepers,
    )
}

/// Re-staff a band mid-run — the climb test changes rung between legs, and the two animal rungs were
/// priced against different reference crews.
fn set_hunt_workers(app: &mut App, band: bevy::prelude::Entity, workers: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .assignments[0]
        .workers = workers;
    fit_band_to_its_crews(app, band);
}

/// **Grow the fixture band to whatever its assignment now staffs.** The take and the build draw on
/// one pool (`docs/plan_standing_upkeep.md` §2.2), so a re-staffing that lands over the band's head
/// count is trimmed by `LaborAllocation::normalize` — tail-first, which on a one-assignment fixture
/// means the build crew quietly goes to zero and the leg under measurement stalls forever. These
/// fixtures are about **rates**, not about the pool, so the band is sized to fit rather than the
/// crews being sized to the band.
fn fit_band_to_its_crews(app: &mut App, band: bevy::prelude::Entity) {
    let staffed = app
        .world
        .get::<LaborAllocation>(band)
        .expect("band exists")
        .assignments
        .iter()
        .map(|assignment| assignment.staffed_total())
        .sum::<u32>();
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists")
        .working = scalar_from_f32(staffed as f32);
}

fn spawn_crew(
    app: &mut App,
    herd_id: &str,
    policy: f32,
    improvement: Option<Improvement>,
) -> bevy::prelude::Entity {
    spawn_crew_of(app, herd_id, policy, improvement, HUNT_WORKERS)
}

/// [`spawn_crew`] with an explicit head-count — the dip tests need a crew the carry binds.
fn spawn_crew_of(
    app: &mut App,
    herd_id: &str,
    policy: f32,
    improvement: Option<Improvement>,
    hunters: u32,
) -> bevy::prelude::Entity {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    // **The same crew staffs the build**, which is what this fixture meant when one crew did every
    // job (`docs/plan_standing_upkeep.md` §2.2) — **plus the maintenance rate it is also paying**.
    //
    // The rate is owed while a meter is being raised, and below the meter's cost the *build crew* is
    // what supplies it (§2.4), so a build staffed at `hunters` alone would net `hunters − rate` and
    // a small fixture crew nets nothing at all. Adding the rate on top keeps every fixture's **net**
    // at `hunters`, which is the pace they were all written against. **Measured at the herd's
    // CARRYING CAPACITY**, because a Thriving herd grows while it is worked and a crew sized against
    // today's flock slips back under the rate as it recovers.
    let builders = improvement.map_or(NO_CREW_ON_THIS_ACTIVITY, |_| {
        hunters.saturating_add(keeper_crew_at_capacity(app, herd_id))
    });
    // **And the keeping is staffed as generously as the take is.** These fixtures measure *rates* —
    // what a rung pays, how fast a build runs — so the keeping must never be the binding term, and
    // [`HUNT_WORKERS`] is chosen for exactly that reason on the take side. It is also what they got
    // for free until slice 4: the retired `herded_fraction = min(1, workers / needed)` read the
    // **take** crew, so a 5000-hand hunting party held any flock outright. The fixtures that are
    // about staffing seat the keeping themselves (`seat_keeping` / `run_understaffed_turns`) and
    // never run the labor arm.
    let keepers = hunters;
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                // **THE BAND HAS TO AFFORD BOTH CREWS, WHETHER OR NOT ONE IS STAFFED YET.** The
                // take and the build draw on one pool (`docs/plan_standing_upkeep.md` §2.2), so a
                // band sized at the hunting party alone is over-committed the moment a verb is
                // staffed beside it — and `LaborAllocation::normalize` then trims the build away,
                // leaving a fixture that measures a job nobody is doing. The room is stated at spawn
                // rather than at the assignment because several fixtures set the verb *later*
                // (the full climb sets one per leg), when the pool is already fixed. Idle hands cost
                // these fixtures nothing: every take is capped by the crew the assignment names.
                // **Sized to what it actually staffs**: the take row, the `builders` pool and the
                // `husbandry` role. Every one of them draws on the same band, so a band sized at
                // `hunters` twice over is short and `normalize` trims the tail — which is exactly
                // the stall these fixtures must not measure.
                // **Plus room for a `builders` pool a caller may stand on the band afterwards** —
                // the projection arm quotes at the band's own pool since
                // `docs/plan_standing_upkeep.md` §2.5, and a band with no headroom for one would
                // have `normalize` trim the very row under measurement.
                working: scalar_from_f32((hunters + builders + keepers + hunters) as f32),
                elders: scalar_zero(),
                stores: pen_materials_support::stocked_with_pen_materials(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
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
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                // **THE HUNT ROW, AND THE BAND'S KEEPING ROLE BESIDE IT.** A managed herd is owed
                // its keeping every turn (`docs/plan_standing_upkeep.md` §2.4), and hunting it is
                // not keeping it — a band that staffed none would watch its own flock drift off
                // while it worked. Since §2.5 the keeping is a **band-level pool** rather than a
                // crew on the herd, so the fixture staffs the `husbandry` role at the herd's own
                // demand (`keeper_crew`), which is what a player reading `upkeepWorkersNeeded`
                // would staff.
                assignments: with_keeping_role(
                    with_builders_pool(
                        vec![LaborAssignment {
                            target: LaborTarget::Hunt {
                                fauna_id: herd_id.to_string(),
                                floor: policy,
                            },
                            workers: hunters,
                            kit: None,
                            priority: SourcePriority::default(),
                            upkeep_kit: None,
                        }],
                        improvement.map_or(0, |_| hunters),
                    ),
                    keepers,
                ),
                // **The declaration** — a verb states what is raised, and the `builders` pool above
                // raises it (`docs/plan_standing_upkeep.md` §2.5).
                build_queue: improvement
                    .map(|declared| core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Herd(herd_id.to_string()),
                        declared: core_sim::BuildJob::Rung(declared),
                        kit: None,
                    })
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            // A taming/keeping crew is a **resident** band (only resident bands allocate labor;
            // expeditions can't tame), so mark it one. This is what makes a *tamed* herd's
            // `drift_to_owner` movement keep it beside its keeper instead of "roaming normally" (the
            // no-owner-band fallthrough) and wandering out of the hunt leash — which decouples these
            // rate fixtures from wherever worldgen happens to place the herd, the same robustness the
            // `FIXTURE_SPECIES` pin exists for.
            ResidentBand,
        ))
        .id()
}

/// One full turn's fauna pipeline in real stage order: Logistics (herds regrow, husbandry upkeep)
/// then Population (labor allocation resolves the hunt + accrues husbandry).
fn run_turns_with_hunt(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_labor_allocation);
    }
}

/// Turns with no active band: only the Logistics-stage systems run.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
}

/// **Nobody is holding this flock** — the staffing fraction a fixture seats to make a herd fully
/// under-contained. The sim's own `NOT_HERDED`, restated here because it is crate-internal.
const NOT_HERDED_FIXTURE: f32 = 0.0;

/// **What holding this herd costs, per turn, off the shipped ladder** — the rung's `upkeep_demand`
/// at the herd's own keeper load (`docs/plan_standing_upkeep.md` §2.4). A fixture multiplies a
/// staffing *fraction* by this to get the work its keepers supplied.
fn keeping_demand(app: &App, id: &str) -> f32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    core_sim::herd_upkeep_demand(&herd_of(app, id), &fauna, &ladder)
}

/// **Seat the herd's keeping at `fraction` of what it owes** — the fixture's stand-in for
/// `maintain <faction> hunt <herd_id> <workers>`, and the one place a staffing fraction becomes the
/// work units the sim actually stores. `1.0` holds the herd outright; `0.0` is nobody at all.
fn seat_keeping(app: &mut App, id: &str, fraction: f32) {
    let supplied = fraction * keeping_demand(app, id);
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .unwrap()
        .upkeep_supplied = supplied;
}

/// **The neglect grace of the rung a managed herd stands on**, read off the shipped ladder rather
/// than restated as a literal — so a retune moves these tests with the game. `animal:pen` for a
/// penned herd, `animal:pastoral` otherwise (see `fauna::herd_keeping_rung`).
fn neglect_grace(app: &App, id: &str) -> u32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let herd = herd_of(app, id);
    ladder
        .rung(if herd.is_corralled() {
            RungKey::AnimalPen
        } else {
            RungKey::AnimalPastoral
        })
        // **The UPKEEP's grace** — the animal branch counts consecutive turns of unmet keeping now
        // (`docs/plan_standing_upkeep.md` §2.4), so `build.grace_turns` is `null` on both rungs and
        // this is the live number.
        .upkeep_grace_turns()
}

/// **Run `turns` under-herded turns, re-seating the keeping each one.** `advance_husbandry` clears
/// `upkeep_supplied` after reading it (the Population→Logistics lag the labor arm writes across), so
/// a fixture that seated it once would read "fully abandoned" from the second turn on — and this
/// helper exists precisely because the neglect grace made *multiple* under-herded turns the normal
/// case in these tests. Runs the husbandry pass alone (no regrowth), so the only biomass change is
/// the shed.
///
/// **Re-seated every turn against the herd's CURRENT demand**, which is what makes a partial
/// staffing mean the same thing as the herd shrinks: the demand falls with the head count, so a
/// fraction stays a fraction.
fn run_understaffed_turns(app: &mut App, id: &str, herded_fraction: f32, turns: u32) {
    for _ in 0..turns {
        seat_keeping(app, id, herded_fraction);
        app.world.run_system_once(advance_husbandry);
    }
}

/// Run untended turns one at a time until the pen is lost (a "drifted off" corral feed line appears),
/// up to `cap`. The pen is announced lost the turn the fully-abandoned herd bleeds out, and the empty
/// managed entity is despawned that same `advance_husbandry` pass — so after this returns the herd is
/// **gone from the registry**. Returns the turn count, or `None` if the pen was not lost within `cap`.
fn run_untended_until_pen_lost(app: &mut App, cap: u32) -> Option<u32> {
    for turn in 1..=cap {
        run_turns_untended(app, 1);
        if corral_feed_lines(app)
            .iter()
            .any(|e| e.label.contains("drifted off"))
        {
            return Some(turn);
        }
    }
    None
}

/// Count the wild (unowned, uncorralled, undomesticated) herds of `species` in the registry — the wild
/// web the shed drifts escapees into.
fn wild_herds_of(app: &App, species: &str) -> Vec<Herd> {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|h| {
            h.owner.is_none()
                && !h.is_corralled()
                && h.ladder_position() == 0.0
                && h.species == species
        })
        .cloned()
        .collect()
}

/// A fingerprint of the whole herd registry, for the determinism guard: `(id, biomass, x, y)` per herd,
/// sorted by id, biomass quantised so two runs compare bit-for-bit rather than on raw f32 noise.
fn herd_fingerprint(app: &App) -> Vec<(String, i64, u32, u32)> {
    let mut rows: Vec<(String, i64, u32, u32)> = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| {
            let pos = h.position();
            (
                h.id.clone(),
                (h.biomass * 1_000.0).round() as i64,
                pos.x,
                pos.y,
            )
        })
        .collect();
    rows.sort();
    rows
}

/// The live herd (panics if it despawned — every test here expects it to survive).
fn herd_of(app: &App, id: &str) -> Herd {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .cloned()
        .expect("herd persists")
}

/// Re-seat a herd at a chosen carrying capacity / biomass — how a *species*' K is put under test
/// without depending on which species the map happened to spawn.
///
/// **Seats `biomass_before_regrowth` too, and that is load-bearing.** A Sustain hunt sizes its rate
/// against the herd's *pre-regrowth* biomass (`Herd::biomass_before_regrowth`, captured at the top of
/// `regrow_biomass` — see "The hunt policy axis"), **not** `biomass`. A fixture that seats only
/// `biomass` therefore leaves the rate reading whatever biomass **worldgen** happened to spawn the herd
/// at, so any test that runs the labor arm without first running `advance_herds` silently measures a
/// worldgen artifact instead of the state it asked for. Seating both says "this herd has been standing
/// here", which is exactly what the fixture is claiming.
fn reseat(app: &mut App, id: &str, cap: f32, biomass: f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.carrying_capacity = cap;
    herd.biomass = biomass;
    herd.biomass_before_regrowth = biomass;
    herd.refresh_ecology_phase(&fauna);
}

/// Hand a herd a **completed, staffed** pastoral rung — the "give me a tamed herd" fixture.
///
/// **Both halves are needed since slice 8.** `accrue_domestication` fills the meter, but a tamed herd
/// now also demands *herders* every turn (`fauna::herders_needed`), and `advance_husbandry` (Logistics)
/// runs **before** the labor arm (Population) that would staff it. So a herd handed only the meter is
/// read as *unherded* on its very first turn, decays a step, drops under the `>= 1.0` bar, and every
/// row measuring it silently measures a **wild** herd instead. Seating `herded_fraction` too says "a
/// crew was already with them last turn", which is exactly the state the fixture is claiming.
fn domesticate(app: &mut App, id: &str) {
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    }
    seat_keeping(app, id, FULLY_HERDED);
}

/// The single band's FOOD larder.
fn larder_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

/// The provisions the band's (only) assignment produced last turn — the retained yield telemetry, i.e.
/// what the sim *actually paid*, not a preview.
fn yield_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("band exists")
        .last_yields
        .first()
        .map(|y| y.actual)
        .unwrap_or(0.0)
}

/// Top the band's larder up to `amount` (so a keeper can always pay its pen's feed).
fn stock_larder(app: &mut App, band: bevy::prelude::Entity, amount: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists");
    cohort.stores.set(FOOD, scalar_from_f32(amount));
}

// **RETIRED: `drain_larder`.** It emptied the band's `FOOD` store so a keeper *could not pay* its
// pen's feed, which is how every starvation fixture here used to be posed. A pen is fed grass and hay
// now — the larder is what the *people* eat — so emptying it starves nobody's animals. What starves a
// pen is a barren footprint with no hay behind it: see `feed_the_pens` for the other end of the same
// lever.

/// **Fill the band's hay store and grant it Foddering**, which is the whole of *"the keeper feeds its
/// pen"*. `run_turns_with_hunt` never runs `advance_herd_grazing`, so a fixture pen's footprint grows
/// nothing and hay is its only feed — which makes this an exact on/off switch for the feed.
fn feed_the_pens(app: &mut App, band: bevy::prelude::Entity, hay: f32) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists");
    cohort.stores.set(FODDER, scalar_from_f32(hay));
}

fn progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| {
            h.rung_work_done(
                core_sim::RungKey::AnimalPastoral,
                &core_sim::LadderConfig::builtin(),
            )
        })
        .unwrap_or(0.0)
}

/// Total provisions carried by faction 0's bands (food is band-local now, so the husbandry yield
/// lands in the owner's cohort larders, not the faction pool).
fn provisions(app: &mut App) -> i64 {
    provisions_f32(app).round() as i64
}

/// Un-rounded total FOOD carried by faction 0's bands — needed to observe sub-1 fractional yields
/// that the rounding `provisions` helper would collapse to zero.
fn provisions_f32(app: &mut App) -> f32 {
    let mut total = 0.0f32;
    let mut query = app.world.query::<&PopulationCohort>();
    for cohort in query.iter(&app.world) {
        if cohort.faction == FactionId(0) {
            total += cohort.stores.get(FOOD).to_f32();
        }
    }
    total
}

/// **The `Tame` verb tames** — sustained work under the `Tame` improvement on a Thriving herd climbs
/// `domestication_progress` to 1.0 and the taming faction owns it.
///
/// **Retargeted from `sustain_hunt_domesticates_thriving_herd`.** The guarantee "sustained work on a
/// Thriving herd domesticates it, and the worker owns it" is *preserved verbatim* — only the verb
/// that earns it changed, from the `Sustain` harvest policy to the explicit `Tame` investment
/// (`plan_intensification_ladder.md` §4.1: taming was a hidden side effect of a harvest policy; it is
/// now a paid verb). Its inverse — that Sustain *no longer* does this — is
/// `sustain_hunt_no_longer_tames_it_only_teaches_herding` below.
#[test]
fn tame_policy_domesticates_thriving_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    grant_herding(&mut app);
    spawn_builder(&mut app, &id, Improvement::Tame);

    // A herd under active taming is spared the decay pass (`tamed_this_turn`, mirroring a patch under
    // Cultivate), so it accrues the FULL progress_per_turn(0.04) → 25 turns at the rung's own pace.
    // The map picks the species, and the taming COST is per-species (a `taming_cost_multiplier` 5.0
    // herd is five times the work), so run the dearest tameable row's worth of turns rather than a
    // bare 30 — this test is about "sustained Tame work tames and the tamer owns it", not the pace.
    let dearest_taming_cost = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        fauna
            .species
            .values()
            .map(|def| def.taming_cost_multiplier)
            .fold(1.0_f32, f32::max)
    };
    run_turns_with_hunt(&mut app, (30.0 * dearest_taming_cost).ceil() as u32);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("domesticated herd persists");
    assert!(
        herd.is_domesticated(),
        "sustained Tame work should domesticate: progress {}",
        herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
    assert_eq!(herd.owner, Some(FactionId(0)), "the tamer owns the herd");
    assert_eq!(registry.domesticated_count(FactionId(0)), 1);
}

/// **Sustain no longer tames anything — it only TEACHES.** The §4.1 de-conflation, at the sim level:
/// the one `Sustain` branch that used to advance Herding knowledge *and* `accrue_domestication` now
/// does only the former, exactly mirroring the plant side's Sustain→Cultivation branch.
///
/// This is the inverse half of the retargeted `sustain_hunt_domesticates_thriving_herd`: run the
/// *same* herd under the *same* policy for well past the old ~34-turn taming horizon and assert the
/// meter never moves — while the knowledge it *does* earn climbs to complete.
#[test]
fn sustain_hunt_no_longer_tames_it_only_teaches_herding() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, 0.5);

    // Far past the 34 turns that used to be a full taming under the old conflated branch.
    run_turns_with_hunt(&mut app, 45);

    let herd = herd_of(&app, &id);
    assert_eq!(
        herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ),
        0.0,
        "a Sustain hunt must never tame — Tame is the taming verb"
    );
    assert_eq!(
        herd.owner, None,
        "a Sustain hunt must not claim ownership of a wild herd either"
    );
    // ...but it DOES teach, which is the whole point: Sustain is how you earn the Tame verb.
    assert!(
        app.world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FactionId(0), HERDING_DISCOVERY_ID)
            >= scalar_one(),
        "Sustain-hunting a Thriving herd must still teach the faction Herding"
    );
}

/// **A Tame costs HANDS, not a fraction of the take.** The hunters beside a gentling crew carry
/// exactly what they carried before it started (`docs/plan_standing_upkeep.md` §2.2) — what a Tame
/// costs is the people who are on it instead, which is a number the player typed.
///
/// **It used to pay `yield_fraction_while_building ×` the hunting crew's take**, and only where
/// hands were the scarce thing: a crew the herd's own escapement bound paid nothing at all. The
/// separate allocation charges every crew size alike and states the price in the one currency the
/// player controls.
#[test]
fn a_tame_leaves_the_hunters_take_alone() {
    // One turn of a plain Sustain hunt (the MSY baseline) vs one turn of the same Sustain hunt with
    // a Tame build running, from an identical start — the two differ only in the improvement axis.
    let harvest = |improvement: Option<Improvement>| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        // Seated at **capacity**, so the escapement standing above the floor is `K/2` rather than
        // one turn's regrowth. A crew can only be the binding term against a real standing stock,
        // and the dip is only visible where the crew binds.
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            herd.biomass = herd.carrying_capacity;
        }
        grant_herding(&mut app);
        spawn_crew_of(&mut app, &id, 0.5, improvement, DIP_VISIBLE_HUNTERS);
        let before = provisions_f32(&mut app);
        run_turns_with_hunt(&mut app, 1);
        provisions_f32(&mut app) - before
    };

    let sustained = harvest(None);
    let tamed = harvest(Some(Improvement::Tame));
    assert!(sustained > 0.0, "the Sustain baseline must pay something");
    assert_eq!(
        tamed, sustained,
        "the hunters are untouched by the Tame staffed beside them"
    );
}

/// **A deep floor beside a running build is LEGAL, and it is PRICED rather than gated** (issue #442
/// as amended by `docs/plan_harvest_floor.md` §3 and `docs/plan_standing_upkeep.md` §2.2).
///
/// Two facts, one run:
/// 1. **The sim accepts it** — a floor of `0.15` with a `Tame` build in flight resolves; no arm
///    refuses the combination. **And it buys nothing now**: a builder's whole work budget is on the
///    meter, so it takes the same nothing at every floor. That is the change the one-budget model
///    made — this assertion used to read *"a deeper floor takes strictly MORE now"*, which was true
///    while the crew kept a `yield_fraction_while_building` share of a bigger standing stock.
/// 2. **It does not finish in the span the food peak does.** The build accrues at
///    `crew output × learn_multiplier(floor)`, so `0.15` runs at `0.3×` the peak's rate; the
///    food-peak control on identical ground tames the herd inside the horizon, so the shortfall is
///    the floor's doing and not the fixture's. The meter **slows, it does not stop**, which is what
///    removed the lapse state the old gate needed.
///
/// **The floor's pressure is DEFERRED, not escaped.** A gentling crew draws nothing, so the herd is
/// untouched while the build runs and the draw begins the turn the Tame completes — which is what
/// the ceiling-bound run still ends Stressed on. While a build is in flight the floor is purely a
/// *pacing* dial: the price is entirely in the rate, and it is proportionate — ease off and the
/// meter speeds back up.
#[test]
fn a_deep_floor_beside_a_tame_build_buys_nothing_now_and_finishes_later() {
    // Long enough to tame the fixture species outright under Sustain — so a stalled meter is a
    // statement about the stance, not about the horizon.
    const TURNS: u32 = 60;

    // **Two runs per floor, at two staffings, because the two claims need different crews.** The
    // TAKE half wants a crew the ceiling binds ([`HUNT_WORKERS`]) or it measures the carry cap; the
    // BUILD half wants the rung's reference keeper crew ([`TAME_KEEPERS`]) or the build finishes in
    // one turn at every floor and there is no pace left to compare
    // (`docs/plan_unit_costed_work.md` §1.2). One number cannot serve both since the crew became the
    // build's throughput.
    let run = |policy: f32, big_crew: bool| {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        grant_herding(&mut app);
        let keepers = if big_crew {
            HUNT_WORKERS
        } else {
            keeper_crew(&app, &id)
        };
        spawn_crew_of(&mut app, &id, policy, Some(Improvement::Tame), keepers);
        let before = provisions_f32(&mut app);
        run_turns_with_hunt(&mut app, 1);
        let first_turn_take = provisions_f32(&mut app) - before;
        run_turns_with_hunt(&mut app, TURNS - 1);
        let herd = herd_of(&app, &id);
        (first_turn_take, herd)
    };

    let (sustain_take, _) = run(0.5, true);
    let (deplete_take, deplete_drawn) = run(0.15, true);
    let (_, sustain_herd) = run(0.5, false);
    let (_, deplete_herd) = run(0.15, false);

    // (1) It is sayable, and **a deeper floor still buys more food today** — the hunters beside a
    // gentling crew are untouched by it, so the pressure axis is exactly what it is without a build.
    assert!(
        deplete_take > sustain_take,
        "a deeper floor leaves less standing, so its hunters take more now: deplete \
         {deplete_take} vs sustain {sustain_take}"
    );

    // The control: the same build under Sustain finishes, so the horizon is not what stalls it.
    assert!(
        sustain_herd.is_domesticated(),
        "control: a food-peak builder tames the herd within {TURNS} turns (progress {})",
        sustain_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );

    // (2) And the floor it named still catches up with it: the herd ends the span out of Thriving.
    // **The pressure is DEFERRED, not escaped** — a gentling crew draws nothing, so the draw begins
    // the turn the Tame completes and the assignment hands its verb back. Read off the ceiling-bound
    // run, whose big crew finishes the build early and then hunts at `0.15` for the rest of the span;
    // a keeper-sized crew is labor-bound and could not draw the herd down far enough to show it.
    assert_ne!(
        deplete_drawn.ecology_phase,
        core_sim::EcologyPhase::Thriving,
        "a floor below the food peak has no equilibrium above it — once the build lets go of the \
         crew's budget the herd is drawn out of Thriving. That is a consequence of the draw, not a \
         gate on the build."
    );
    // **AND THE BUILD IS UNTOUCHED BY IT** — the floor came off the build rate
    // (`docs/plan_standing_upkeep.md` §2.2), so a deep-floor build finishes on exactly the schedule
    // a food-peak one does. This assertion read the opposite until then: the deep-floor builder was
    // *still short* at the horizon, because it accrued at `learn_multiplier(0.15)` = 0.3× the peak's
    // rate.
    assert!(
        deplete_herd.is_domesticated(),
        "the floor paces the LESSON, not the build — a deep-floor Tame finishes with the rest \
         (progress {})",
        deplete_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
    assert!(
        deplete_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ) <= sustain_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ),
        "the Deplete builder banks strictly less progress over the same span: {} vs {}",
        deplete_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ),
        sustain_herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
}

/// Re-badge a primed herd **as another species** — the display name is what
/// `FaunaConfig::taming_cost_multiplier_for` resolves, so this puts the *same herd, on the same code
/// path*, at a different species' taming COST with one dial changed and nothing else. The husbandry ceiling
/// is taken from the same roster row, so the fixture can never be an incoherent species (a herd that
/// tames at a ceiling its species forbids). Everything the turn loop keys off the herd itself
/// (`size_class` → graze range → `K`, `fodder_per_biomass`, `regrowth_rate`) is untouched, so the
/// ecology under the two runs is identical and only the taming cost differs.
fn rebadge_as(app: &mut App, id: &str, species_key: &str) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let def = fauna
        .species
        .get(species_key)
        .expect("the roster defines the species under test");
    let (display, ceiling) = (def.display_name.clone(), def.husbandry_ceiling);
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.species = display;
    herd.husbandry_ceiling = ceiling;
}

/// Turns of sustained `Tame` work before the herd is domesticated, at an explicit keeper crew
/// (capped, so a species that can never tame fails loudly instead of hanging).
fn turns_to_tame_with(species_key: &str, keepers: u32, cap_turns: u32) -> u32 {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    rebadge_as(&mut app, &id, species_key);
    grant_herding(&mut app);
    spawn_crew_of(
        &mut app,
        &id,
        MSY_BIOMASS_FRACTION,
        Some(Improvement::Tame),
        keepers,
    );
    for turn in 1..=cap_turns {
        run_turns_with_hunt(&mut app, 1);
        if herd_of(&app, &id).is_domesticated() {
            return turn;
        }
    }
    panic!("{species_key} never tamed within {cap_turns} turns");
}

/// **Taming is a PER-SPECIES COST on one shared rung** (`docs/plan_unit_costed_work.md` §3.1).
/// Before the dial existed the `animal:pastoral` rung priced every animal alike — a rabbit cost
/// what a Steppe Runner cost. Now the rung owns the mechanic and the species prices it: a quick,
/// forgiving warren is the rung's own 50 work units; binding a large migratory herd is 250.
///
/// **This asserts the COST ratio off the ladder, and a live turn ORDERING beside it.** Turns are an
/// *output*, and the published count is a `ceil` of `bar / crew` on a bar the crew's gear has
/// already paid part of — so a turn *ratio* is not the cost ratio. What is invariant is the price,
/// and that the dearer species really does take longer to reach.
#[test]
fn taming_is_a_per_species_cost_on_the_shared_rung() {
    /// The crew both runs are staffed at. Large enough that neither rebadged species is under-herded
    /// (an under-staffed keeper sheds animals, which would confound the measurement), small enough
    /// that a 250-unit Tame is not one turn.
    const SHARED_KEEPERS: u32 = 10;
    /// The multiple `fauna_config.json` declares between the two rows: `taming_cost_multiplier`
    /// 5.0 against 1.0.
    const STEPPE_RUNNER_IS_THIS_MUCH_MORE_WORK: f32 = 5.0;

    // **The COST ratio, exactly** — the species prices the rung's own job, which is the whole claim.
    let (rabbit_cost, runner_cost) = {
        let app = spawn_world();
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let rung = ladder.rung(RungKey::AnimalPastoral);
        (
            rung.build_cost(fauna.taming_cost_multiplier_for("Rabbit Warren"))
                .expect("the pastoral rung builds"),
            rung.build_cost(fauna.taming_cost_multiplier_for("Steppe Runners"))
                .expect("the pastoral rung builds"),
        )
    };
    assert!(
        (runner_cost - rabbit_cost * STEPPE_RUNNER_IS_THIS_MUCH_MORE_WORK).abs() < 1e-3,
        "a Steppe Runner is {STEPPE_RUNNER_IS_THIS_MUCH_MORE_WORK}x the work: {rabbit_cost} vs \
         {runner_cost}"
    );

    // **And the ordering survives end to end**, which is what says the multiplier actually reaches
    // the build rather than sitting in config. The *ratio* of turns does not, and asserting one
    // would be asserting `cost / crew` — the identity this arc changed.
    let rabbit = turns_to_tame_with("rabbit", SHARED_KEEPERS, 60);
    let steppe_runner = turns_to_tame_with("steppe_runner", SHARED_KEEPERS, 300);

    assert!(
        steppe_runner > rabbit,
        "the dearer species takes longer at the same crew: rabbit {rabbit}, steppe runner \
         {steppe_runner}"
    );
}

/// **AN UNTAMED HERD QUOTES THE TAME IT WOULD TAKE ON, AND DOUBLING THE CREW HALVES THE QUOTE** —
/// `buildTurnsRemaining` is a *projection*, not "`-1` because nothing is being built"
/// (`docs/plan_unit_costed_work.md` §11).
///
/// A compose sheet is by definition looking at a herd nobody has started taming, so a sentinel there
/// withholds the readout at the one moment it drives the decision. It is the same defect, and the
/// same remedy, as `HerdTelemetryState.corralYield` projecting an unpenned herd's payoff.
///
/// The halving is the arc's thesis stated directly — add hands and watch the number fall — and is
/// asserted as a **relation** rather than against turn literals, because turns are an output of the
/// crew now. `None` is pinned beside it for a faction that has not learned Herding: a projection must
/// never quote a rung `validate_tame` would refuse.
#[test]
fn an_untamed_herd_quotes_the_tame_it_would_take_on_and_the_quote_halves_with_the_crew() {
    /// Both runs staff at or above the fixture herd's keeper requirement, so neither is shedding —
    /// the only thing that differs between them is how many hands are on the job.
    const SMALL_CREW: u32 = 10;

    let projection = |keepers: u32, herding: bool| -> Option<u32> {
        // A finite count only — the never/no-estimate pair is asserted on the wire in
        // `build_turns_on_the_wire.rs`; this closure is about the halving.
        fn count(turns: Option<core_sim::BuildTurns>) -> Option<u32> {
            match turns {
                Some(core_sim::BuildTurns::Turns(n)) => Some(n),
                _ => None,
            }
        }
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        if herding {
            grant_herding(&mut app);
        }
        // **Nothing queued on this herd: the band is hunting it and DECIDING**, which is by
        // definition the state a compose sheet is looking at. The quote is what the band's own
        // `builders` pool would take, so the fixture stands one — since
        // `docs/plan_standing_upkeep.md` §2.5 there is a real crew to quote and the projection no
        // longer falls back to the gatherers beside it.
        let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation")
            .assignments
            .push(LaborAssignment {
                target: LaborTarget::Builders,
                workers: keepers,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
        run_turns_with_hunt(&mut app, 1);
        count(herd_of(&app, &id).build_turns_remaining)
    };

    // **Twice the POOL, half the turns.** The pool's whole output is progress — a build supplies
    // nothing toward the keeping rate (`docs/plan_standing_upkeep.md` §4.6a) — so the quote is
    // `work_cost / pool` and doubling the pool halves it.
    //
    // The `rate +` padding survives from the slice where a build crew paid the rate itself, and is
    // harmless: both arms carry the same offset, so what is doubled is still the pool.
    let rate = {
        let app = spawn_world();
        let mut probe = app;
        let id = prime_thriving_herd(&mut probe);
        let fauna = probe.world.resource::<FaunaConfigHandle>().get();
        let ladder = probe.world.resource::<LadderConfigHandle>().get();
        let herd = herd_of(&probe, &id);
        ladder
            .rung(RungKey::AnimalPastoral)
            .upkeep_crew_needed(core_sim::herd_keeper_load(&herd, &fauna))
    };
    let quoted =
        projection(rate + SMALL_CREW, true).expect("a wild herd with a crew on it quotes its Tame");
    let doubled = projection(rate + SMALL_CREW * 2, true).expect("a bigger crew is still quotable");
    assert_eq!(
        doubled,
        quoted.div_ceil(2),
        "twice the net supply, half the turns: {quoted} at {SMALL_CREW}, {doubled} at {}",
        SMALL_CREW * 2
    );

    assert_eq!(
        projection(SMALL_CREW, false),
        None,
        "a faction that has not learned Herding is quoted no Tame at all"
    );
}

/// **A BIGGER KEEPER CREW TAMES MATERIALLY FASTER, and that is the arc's own claim on the animal
/// web** (`docs/plan_unit_costed_work.md` §1.2). Both animal rungs declare `crew_needed: null`, so
/// before improvements were priced in work they were not merely uncapped but **crew-BLIND**: a Tame
/// took 25 turns whether two hands or twenty worked the herd. Turns are the output now, so hands buy
/// them — on every rung, with no cap.
#[test]
fn a_bigger_keeper_crew_tames_materially_faster() {
    /// Both runs use a crew at or above the fixture herd's own keeper requirement, so neither is
    /// shedding — what differs is only how many hands are on the job.
    const SMALL_CREW: u32 = 10;
    const BIG_CREW: u32 = SMALL_CREW * 4;

    let slow = turns_to_tame_with("rabbit", SMALL_CREW, 120);
    let fast = turns_to_tame_with("rabbit", BIG_CREW, 120);

    assert!(
        fast < slow,
        "four times the keepers must finish the same job sooner: {fast} vs {slow}"
    );
    // **And in proportion — the crew IS the throughput.** Stated with a one-turn allowance because
    // the published count is a `ceil` of the true span, on a bar the crew's own gear has already
    // paid part of, and the two arms round **independently**: an exact 4:1 holds only where both
    // divide. The exact arithmetic is pinned where there is no rounding to confound it, at the seam
    // (`intensification::tests::a_bigger_crew_finishes_the_same_job_sooner_on_every_rung`).
    const CEILING_SLACK_TURNS: u32 = 1;
    assert!(
        fast <= slow.div_ceil(4) + CEILING_SLACK_TURNS,
        "four times the hands must bank four times the work: {fast} vs {slow}"
    );
}

// **RETIRED (`docs/plan_fauna_neglect_escape.md` §2.1):** `a_slow_taming_species_is_equally_slow_to_forget`
// measured the per-species *forget* rate — the tameness-decay timescale. Neglect no longer decays
// tameness at all (it sheds animals), so there is no forget rate to scale. The *taming* half of that
// per-species timescale is still live and covered by `taming_speed_is_a_per_species_dial_on_the_shared_rung`
// above; the forget half is gone. Neglect's new cost is guarded by `neglect_never_un_tames_a_herd` and
// the shed tests at the bottom of this file.

/// Only a Sustain hunt tames; an Eradicate hunt never accrues husbandry.
#[test]
fn eradicate_hunt_does_not_domesticate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, 0.0);
    run_turns_with_hunt(&mut app, 10);
    assert_eq!(
        progress_of(&app, &id),
        0.0,
        "eradicate accrues no husbandry"
    );
}

/// **A fully-abandoned pastoral herd goes FERAL — it bleeds its whole flock into the wild web and
/// DESPAWNS — and its taming is never decayed on the way** (`docs/plan_fauna_neglect_escape.md` §2.4;
/// the inverse of the retired `abandoning_a_tame_decays_the_progress`). Walking off costs you the
/// ANIMALS: with no keeper and no regrowth the shed drives the herd down until it can shed no more, then
/// the empty entity is removed — no ownerless husk. The tameness meter is **never reset**; it reads
/// exactly what was earned right up to the turn the herd is gone, because the tameness leaves with the
/// animals (each shed batch is a wild herd at domestication 0).
#[test]
fn a_fully_abandoned_pastoral_herd_goes_feral_without_decaying_its_taming() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let species = herd_of(&app, &id).species.clone();
    let wild_before: f32 = wild_herds_of(&app, &species)
        .iter()
        .map(|h| h.biomass)
        .sum();
    grant_herding(&mut app);
    let band = spawn_builder(&mut app, &id, Improvement::Tame);
    run_turns_with_hunt(&mut app, 6);
    let built = progress_of(&app, &id);
    assert!(built > 0.0, "some progress should have accrued");

    // Nobody keeps it: it bleeds out and despawns. Every turn it is still alive, its taming meter must
    // read exactly what was earned — never decayed toward the wild.
    app.world.despawn(band);
    let mut despawned = false;
    for _ in 0..200 {
        run_turns_untended(&mut app, 1);
        match app.world.resource::<HerdRegistry>().find(&id) {
            Some(herd) => assert_eq!(
                herd.rung_work_done(
                    core_sim::RungKey::AnimalPastoral,
                    &core_sim::LadderConfig::builtin()
                ),
                built,
                "tameness is never decayed by neglect: {built} -> {}",
                herd.rung_work_done(
                    core_sim::RungKey::AnimalPastoral,
                    &core_sim::LadderConfig::builtin()
                )
            ),
            None => {
                despawned = true;
                break;
            }
        }
    }
    // **⛔ IT GOES FERAL AGAIN, AND THIS ASSERTION HAS BEEN ROUND THE LOOP ONCE — do not re-derive it.**
    //
    // §2b deleted `regrow_biomass`'s growth freeze (*a herd's growth is a fact about the land it
    // stands on, not about who is watching it*) and this test had to be restated: with a **constant**
    // escape rate the shed no longer out-ran the pastoral growth curve, and an abandoned herd settled
    // at ~0.64·K, still owned, for ever.
    //
    // That equilibrium was a **defect**, not a number to tune. The design has always been that a herd
    // nobody tends terminates — *"if no herders are present, eventually, the entire herd leaves and
    // you are left with nothing"* — so §2d made the escape rate **accelerate** with consecutive
    // unkept turns (`husbandry.escape_acceleration`). The shed now out-runs any growth curve, and
    // going feral is the behaviour again.
    //
    // The freeze is still deleted and is not what does this: the *shed* does.
    assert!(
        despawned,
        "a fully-abandoned pastoral herd bleeds out entirely and despawns — the accelerating escape \
         rate is what makes that certain, not a growth freeze"
    );
    // Its escapees went to the wild web, not into thin air — still true, and still the point of the
    // shed: what changed is only that the herd it leaves behind keeps replacing them.
    let wild_after: f32 = wild_herds_of(&app, &species)
        .iter()
        .map(|h| h.biomass)
        .sum();
    assert!(
        wild_after > wild_before,
        "the abandoned flock re-entered the wild web: {wild_before} -> {wild_after}"
    );
}

/// A domesticated (managed) herd is immune to the overhunting collapse: driven below the Allee
/// threshold it recovers logistically instead of crashing to extinction.
///
/// **Collapse-immunity is an ECOLOGY property, so the herd must stay HERDED** (the neglect-escape
/// model, `docs/plan_fauna_neglect_escape.md` §2.4 option B): a *fully abandoned* domesticated herd now
/// goes feral (regrowth suppressed → sheds to zero), which is a different mechanic. Here a keeper is
/// present every turn (`herded_fraction` re-seeded full), so nothing sheds and the only question is
/// whether the ecology curve recovers or crashes.
#[test]
fn domesticated_herd_is_collapse_immune() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let low = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin()); // sets owner + progress = 1.0 → domesticated
                                                                              // Below the 15% collapse threshold — a wild herd here would crash.
        let low = herd.carrying_capacity * 0.10;
        herd.biomass = low;
        low
    };

    // A keeper is with the herd every turn (full staffing), so it does not shed — it just regrows.
    for _ in 0..10 {
        seat_keeping(&mut app, &id, FULLY_HERDED);
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&id)
        .expect("a domesticated herd never collapses to extinction");
    assert!(
        herd.biomass > low,
        "managed herd should recover, not crash: {low} -> {}",
        herd.biomass
    );
}

/// **A pastoral herd pays NOTHING without workers — every rung is worker-driven**
/// (`docs/plan_intensification_ladder.md` §3, slice 3b).
///
/// **Retargeted from `a_domesticated_herd_worked_by_labor_is_not_also_paid_the_passive_rung`.** That
/// test guarded the no-double-pay skip: the passive rung had to be withheld from a herd a band was
/// already working, because paying both turned the corral's *investment cost* into a profit. Retiring
/// passive-free pastoral makes that guarantee **structural** — there is no second payment left to
/// stack — so this asserts the stronger thing the same run measures: an unworked tamed herd earns its
/// owner nothing at all, and is not even drawn down. A tamed herd is livestock, not an annuity; it is
/// worked, or it is idle capital. (What the *workers* then get is the pastoral rung's 1.5× `r` — see
/// `the_husbandry_ladder_is_a_per_species_growth_rate_ladder`.)
#[test]
fn a_pastoral_herd_pays_nothing_without_workers() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    assert_eq!(provisions_f32(&mut app), 0.0, "larders start empty");

    // Nobody works it. Run the herd's whole Logistics pipeline for a long stretch — the old passive
    // rung would have printed its MSY into the owner's larders every one of these turns.
    run_turns_untended(&mut app, 20);

    assert_eq!(
        provisions_f32(&mut app),
        0.0,
        "an unworked tame herd must yield its owner NOTHING — the passive rung is retired"
    );
    // Under the neglect-escape model an unherded tame herd SHEDS animals over its (zero-worker) labor
    // capacity into the wild web, so it no longer sits at capacity — but the shed is not a yield: the
    // owner's larder stays empty (asserted above). The cost of neglect landed on the herd, not the pot.
    assert!(
        herd_of(&app, &id).biomass < cap,
        "an unherded tame herd sheds animals (the visible cost of neglect), it does not sit at capacity"
    );
}

// **RETIRED: `a_low_floor_tame_takes_materially_longer_than_a_food_peak_one`** — the animal half of
// the rule that a crew pulling hard on the source it was improving built more slowly, in proportion
// to `learn_multiplier(floor)`.
//
// **That rule was written when one crew did both jobs.** The build is staffed in its own right now
// (`docs/plan_standing_upkeep.md` §2.2), so the builders are not pulling anything and a build crew on
// a source nobody is harvesting has no floor to read at all. The floor stays on
// `RungDef::knowledge_accrual`, where restraint still shapes what is *learned*; the pacing-neutrality
// of taking it off the build is pinned by
// `intensification::taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak`.

/// **The Corral build is a genuine net LOSS while it runs** — the investment the whole intensification
/// ladder is built on.
///
/// **Retargeted baseline, same guarantee.** The comparison used to be "building the pen vs *walking
/// away*", because a tamed herd left alone paid its owner the passive rung for free; that free path is
/// what made the dip a profit before the no-double-pay fix, and slice 3b deletes it outright (walking
/// away now pays **0**, which `a_pastoral_herd_pays_nothing_without_workers` pins). So the baseline is
/// now the *real* alternative use of the same crew: **Sustain-hunting that same tamed herd**. The
/// guarantee is unchanged and if anything sharper — the pen must cost the builder something against
/// the best thing those workers could otherwise be doing on this herd, or there is no decision.
///
/// **Both crews are [`DIP_VISIBLE_HUNTERS`]**, because `docs/plan_harvest_floor.md` §3.1 moved the
/// dip onto crew throughput: the cost is what the *builders* fail to carry home, so it only exists
/// while hands are the scarce thing. Against an ample crew the herd's own escapement binds either
/// way and the pen is free — legibly so ("hire twice the people"), which is the change's point.
#[test]
fn building_a_corral_costs_more_than_hunting_the_same_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;

    // (a) The alternative: the same band Sustain-hunts the tamed herd → the full pastoral MSY.
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    let hunter = spawn_crew_of(&mut app, &id, 0.5, None, DIP_VISIBLE_HUNTERS);
    run_turns_with_hunt(&mut app, 1);
    let hunting = yield_of(&app, hunter);
    assert!(hunting > 0.0, "the alternative use of the crew pays");

    // (b) Build the pen: the same band, same herd, under Corral → the dip and nothing else.
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    grant_penning(&mut app);
    let builder = spawn_crew_of(
        &mut app,
        &id,
        0.5,
        Some(Improvement::Corral),
        DIP_VISIBLE_HUNTERS,
    );
    run_turns_with_hunt(&mut app, 1);
    let building = yield_of(&app, builder);

    assert_eq!(
        building, hunting,
        "the hunters are untouched by the fence going up beside them — **what a Corral costs is the \
         hands on it** (`docs/plan_standing_upkeep.md` §2.2), which the player states, not a \
         fraction of what the rest of the band carries"
    );
}

/// **The pastoral rung pays its pastoral MSY, and the harvest DRAWS THE HERD DOWN** — which is what
/// makes it sustainable (the flow-based ladder, `docs/plan_corral_managed_population.md`).
///
/// **Retargeted from the passive path, guarantee intact.** The *what* is verbatim — a tamed herd's
/// harvest is the MSY of the **pastoral** ecology (per-species `r` × `pastoral_gain`, resolved through
/// the one `herd_ecology` seam) and it is a real take out of the herd, not a share of standing stock.
/// Only the *who* changed: it is paid to a **worker** on a Hunt assignment rather than dropped into
/// the owner's larders for free (slice 3b, §3 — every rung is worker-driven). That the pastoral `r`
/// really does reach the worker's take is the crux of the slice, so it is asserted here against the
/// herd's own resolved ecology.
#[test]
fn a_worker_hunting_a_pastoral_herd_takes_its_pastoral_msy_and_draws_the_herd_down() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);

    let cap = herd_of(&app, &id).carrying_capacity;
    let (expected_take, expected_provisions) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let herd = herd_of(&app, &id);
        // Per-species pastoral rate (Grazing 2d): read the same seam the sim harvests through.
        let pastoral_r = herd_ecology(&herd, &fauna).regrowth_rate;
        let msy = pastoral_r * cap / 4.0;
        // **Quantised through the take path's OWN helper, never a second copy of the rule.** A hunt
        // pays in whole animals (slice 8), so the continuous MSY is only ever *approximately* what the
        // herd hands over — the residue is `msy mod body_mass`, and it is a hard 1-animal step. Which
        // species this harness measures is decided by **worldgen** (`prime_thriving_herd` takes the
        // first game herd the map placed), and since Grazing 2b-ii `K` is derived from the graze under
        // the herd's range — so a worldgen change moves both `body_mass` and `cap` and the residue
        // with them. Comparing against the quantised expectation makes the assertion exact for every
        // species instead of accidentally-tight for the one the current seed happens to place.
        // `HUNT_WORKERS` is sized so the carry cap never binds, so the crew hauls everything it kills:
        // the MSY is both the policy ceiling and the collection.
        let take = quantise_animal_take(
            msy,
            herd.body_mass,
            core_sim::animals_affordable(msy, herd.body_mass),
            core_sim::EngagementStop::WhenPackFull,
        )
        .carried;
        (take, take * fauna.hunt.provisions_per_biomass)
    };
    // **Seated at the OPERATING POINT, not at capacity** (slice 8). A Sustain hunt is constant
    // escapement to `K/2`, so what a herd hands over is the **standing surplus above that point**,
    // not a rate. This test is about the *rate* — that the pastoral `r` reaches the worker's take —
    // so it seats the herd exactly where a converged herd stands when the Population stage runs:
    // `K/2` **plus the turn's own regrowth** (`r·K/4`, the MSY). The escapement it spares is then
    // precisely that MSY, and the assertion below measures the thing it was written to measure.
    //
    // Seating at `B = K` (as this used to) makes the herd spare `K/2` — the accumulated **stock**,
    // which is identical for every rung because `r` cancels out of `K − K/2`. That is correct
    // behaviour and it is exactly why it cannot measure a growth rate; see
    // `the_husbandry_ladder_is_a_per_species_growth_rate_ladder` for the long-run form.
    let biomass_before = cap * MSY_BIOMASS_FRACTION + expected_take;
    reseat(&mut app, &id, cap, biomass_before);
    assert_eq!(provisions(&mut app), 0);

    let band = spawn_hunter(&mut app, &id, 0.5);
    app.world.run_system_once(advance_labor_allocation);

    let paid = yield_of(&app, band);
    assert!(
        (paid - expected_provisions).abs() < expected_provisions * 0.02,
        "a worker's take on a tamed herd is the PASTORAL MSY: expected {expected_provisions}, got {paid}"
    );
    assert!(
        (larder_of(&app, band) - paid).abs() < paid * 0.02,
        "and it lands in the working band's own larder, place-local"
    );
    // **The premise that used to be false:** the managed harvest is a real take out of the herd.
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert!(
        (biomass_before - after - expected_take).abs() < expected_take * 0.02,
        "the harvest draws the herd down by exactly its MSY: {biomass_before} -> {after}"
    );
}

// --- Corral (Intensification Rung 1c) -------------------------------------------------------------

/// Faction Herding knowledge for faction 0's ledger.
fn herding_knowledge(app: &App) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), HERDING_DISCOVERY_ID)
        .to_f32()
}

/// Teach faction 0 **Herding** — the unlock gate on the **`Tame`** verb (rung 2), and since the
/// §4.3 reshuffle that alone. A taming test must grant it; a *corralling* test needs
/// [`grant_penning`] instead.
fn grant_herding(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), HERDING_DISCOVERY_ID, scalar_one());
}

/// Teach faction 0 **Penning** — the unlock gate on the **`Corral`** verb (rung 3) since the §4.3
/// reshuffle, so a Corral assignment actually accrues pen progress. Earned in play by working a
/// *pastoral* herd; granted directly here, as these tests are about the pen, not the climb to it.
fn grant_penning(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), PENNING_DISCOVERY_ID, scalar_one());
}

/// The `Corral`-kind command-feed entries — the pen's whole life (completion AND escape) rides this
/// one kind.
fn corral_feed_lines(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| matches!(entry.kind, CommandEventKind::Corral))
        .cloned()
        .collect()
}

/// A herd's pen-construction progress (0 = no pen, 1.0 = built).
fn corral_progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| {
            h.rung_work_done(
                core_sim::RungKey::AnimalPen,
                &core_sim::LadderConfig::builtin(),
            )
        })
        .unwrap_or(0.0)
}

/// Pen a herd: prime it to full biomass (Thriving, and at cap so logistic regrowth is 0 → a clean
/// no-draw-down check), domesticate it for faction 0, and corral it at its current tile.
fn corral_herd(app: &mut App, id: &str) -> UVec2 {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = herd.carrying_capacity;
    herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    // A freshly-penned herd has a crew (`corral_at` grants the tending grace for the same reason) —
    // see `domesticate` for why the meter alone is not enough.
    let tile = herd.position();
    assert!(
        herd.corral_at(tile, &core_sim::LadderConfig::builtin()),
        "the fixture species must be pennable"
    );
    tile
}

/// Rung 1c earned knowledge: a Sustain hunt on a Thriving herd teaches the faction **Herding** (the
/// `corral` gate), accrued in the shared `DiscoveryProgressLedger`.
#[test]
fn sustain_hunt_earns_herding_knowledge() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, 0.5);
    assert_eq!(
        herding_knowledge(&app),
        0.0,
        "no faction starts knowing Herding"
    );

    run_turns_with_hunt(&mut app, 3);

    assert!(
        herding_knowledge(&app) > 0.0,
        "Sustain-hunting a Thriving herd earns Herding knowledge"
    );
}

/// A corralled herd does NOT roam: `advance_herds` leaves its position fixed at the pen tile (and
/// clears any heading arrow), even given a multi-tile route it would otherwise wander.
#[test]
fn corralled_herd_stops_roaming() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let pen = corral_herd(&mut app, &id);
    // Give it a route to a distant tile + prime it to step, so an un-penned herd would move.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.route = vec![pen, UVec2::new(pen.x.saturating_add(3), pen.y)];
        herd.step_index = 1;
        herd.dwell_remaining = 0;
    }

    for _ in 0..5 {
        app.world.run_system_once(advance_herds);
        let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
        assert_eq!(herd.position(), pen, "a corralled herd stays put");
        assert_eq!(herd.next_position(), None, "a penned herd shows no heading");
    }
}

/// **The pen is a managed population.** A tended corral harvests the *pen's* MSY (`r` = 0.60) each
/// turn, which **draws the herd down** — and that is exactly what makes it sustainable: taking MSY
/// while the herd regrows logistically converges it on `K_pen/2` and holds it there, paying `r·K/4`
/// forever. (The retired flat rate never drew the herd down at all: a penned herd parked at capacity
/// and printed food.)
#[test]
fn tended_corral_harvests_msy_and_settles_at_half_capacity() {
    const CONVERGENCE_TURNS: u32 = 80;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let cap = herd_of(&app, &id).carrying_capacity;
    let (pen_r, prov_rate) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        // Per-species pen rate (Grazing 2d): the corralled herd's own rung, via the shared seam.
        (
            herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate,
            fauna.hunt.provisions_per_biomass,
        )
    };
    // MSY = r·K/4 (the ceiling plateaus for any biomass at or above K/2).
    let msy_provisions = pen_r * cap / 4.0 * prov_rate;

    // A Hunt assignment on the penned herd = herding/tending it. Keep its HAY store stocked so the
    // pen's feed is always covered (the starvation path has its own test) — the harness never grazes
    // the footprint, so hay is the only feed a fixture pen can have.
    let keeper = spawn_hunter(&mut app, &id, 0.5);
    feed_the_pens(&mut app, keeper, cap);

    let mut last_yield = 0.0f32;
    for _ in 0..CONVERGENCE_TURNS {
        feed_the_pens(&mut app, keeper, cap); // never let the feed run out
        run_turns_with_hunt(&mut app, 1);
        last_yield = yield_of(&app, keeper);
    }

    let herd = herd_of(&app, &id);
    assert!(herd.is_corralled(), "a tended corral stays penned");
    // **Converged on the MSY point**, not parked at capacity.
    assert!(
        (herd.biomass - cap * 0.5).abs() < cap * 0.05,
        "a harvested pen settles at K/2 ({}): got {}",
        cap * 0.5,
        herd.biomass
    );
    // ...and it pays the full MSY there, stably, forever.
    assert!(
        (last_yield - msy_provisions).abs() < msy_provisions * 0.05,
        "the settled pen pays r·K/4 × p = {msy_provisions}: got {last_yield}"
    );
}

// ---- The pen's collection stage (`fauna::resolve_hunt_engagement`) -------------------------------
//
// A hunt bounds what it **goes after** by the room above the floor, and since
// `docs/plan_standing_upkeep.md` §4.9 item 12b **so does a pen, through the very same seam**: the
// tend branch takes through `systems::hunt_take` like the range does, so the room clamp, the retreat
// and the fight are one expression at every rung and the pen-side `animals_handled` helper is gone.
// The three tests below are that bound: nothing is collected at the floor, no crew collects past it,
// and what is collected is whole animals.

/// **A single keeper** — the smallest crew a pen can have, so a floor that holds for it and for
/// [`HUNT_WORKERS`] holds for every crew in between (the collection is monotone in the crew).
const ONE_KEEPER: u32 = 1;

/// **Leave the whole herd standing** — the floor AT capacity, so nothing ever stands above it and
/// the growth share it would otherwise pay is `× (1 − 1.0)`. This is a pen *on* its floor stated in
/// the one way no regrowth can undo.
const FLOOR_AT_CAPACITY: f32 = 1.0;

/// How close two biomass readings must be to describe the same animals. Relative slop is pointless
/// here — the quantities are whole bodies of a `0.27`-unit rabbit — so this is the absolute float
/// noise of one `f32` subtraction on a stock of a few hundred.
const SAME_BIOMASS: f32 = 1e-4;

/// **One pen turn resolved in STAGE ORDER, read either side of the collection.** Logistics regrows
/// (and feeds, and sheds) before Population takes, so a reading of *"what the keepers took"* must be
/// taken across the labor arm alone — a before/after around the whole turn measures the regrowth as
/// well and cannot see a take at all.
fn pen_collection_turn(
    app: &mut App,
    id: &str,
    band: bevy::prelude::Entity,
    hay: f32,
) -> (f32, f32) {
    feed_the_pens(app, band, hay);
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_husbandry);
    let before = herd_of(app, id).biomass;
    app.world.run_system_once(advance_labor_allocation);
    (before, herd_of(app, id).biomass)
}

/// **A PEN AT ITS FLOOR COLLECTS NOTHING, AT ANY CREW SIZE** — the keepers have nothing to spare, so
/// they walk no animal out and the flock is untouched by the Population stage.
///
/// This is the pen's half of *"restraint is free"*: the room is spent **before** the retreat
/// ([`core_sim::resolve_hunt_engagement`]), at the pen exactly as on the range, so a pen whose floor
/// is at capacity is not merely prevented from banking a take — it never takes one.
///
/// **Each crew is paired with the same crew at a workable floor**, or the assertion would pass just
/// as well on a fixture whose assignment had lapsed, whose pen had escaped, or whose species was not
/// pennable at all.
#[test]
fn a_pen_at_its_floor_collects_nothing_however_many_keepers() {
    for keepers in [ONE_KEEPER, HUNT_WORKERS] {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        corral_herd(&mut app, &id);
        let cap = herd_of(&app, &id).carrying_capacity;
        let band = spawn_crew_of(&mut app, &id, FLOOR_AT_CAPACITY, None, keepers);

        let (before, after) = pen_collection_turn(&mut app, &id, band, cap);

        assert!(
            (after - before).abs() < SAME_BIOMASS,
            "{keepers} keepers at a floor of {FLOOR_AT_CAPACITY} must leave the flock untouched: \
             {before} -> {after}"
        );
        assert_eq!(
            yield_of(&app, band),
            0.0,
            "…and bank nothing, {keepers} keepers"
        );

        // **Liveness** — the same crew on the same pen at a workable floor really does collect, so
        // the zero above is the floor and not the fixture.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        corral_herd(&mut app, &id);
        let cap = herd_of(&app, &id).carrying_capacity;
        let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);
        let (before, after) = pen_collection_turn(&mut app, &id, band, cap);
        assert!(
            before - after > SAME_BIOMASS && yield_of(&app, band) > 0.0,
            "liveness: {keepers} keepers at {MSY_BIOMASS_FRACTION} must collect ({before} -> \
             {after})"
        );
    }
}

/// **NO CREW COLLECTS A PEN BELOW ITS FLOOR** — the case the missing bound allowed: the tend branch
/// handed `herd_engage_rate × workers` straight to the quantiser, so the only thing between a big
/// keeper crew and a stripped pen was a post-hoc clamp inside it. The clamp is
/// `resolve_hunt_engagement`'s own since §4.9 item 12b, shared with the range.
///
/// Asserted on **the herd's biomass after the turn**, not on the take: a take that is bounded and a
/// herd that ends up above its floor are the same claim only when the bound is the one the herd is
/// actually drawn against, which is precisely what a call-site bound has to prove.
///
/// It stops **within one whole animal** of the floor, which is the whole-animal quantum rather than
/// slack in the bound — the keepers take every body the room affords and leave the part-body
/// standing.
#[test]
fn a_large_keeper_crew_cannot_collect_a_pen_below_its_floor() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let cap = herd_of(&app, &id).carrying_capacity;
    // The whole band on one pen: the keepers' own handling and haul are nowhere near binding, so the
    // room above the floor is the only thing that can stop them.
    let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, HUNT_WORKERS);

    let (before, after) = pen_collection_turn(&mut app, &id, band, cap);

    let herd = herd_of(&app, &id);
    let floor_biomass = herd.carrying_capacity * MSY_BIOMASS_FRACTION;
    assert!(
        before - after > SAME_BIOMASS,
        "liveness: the crew must actually collect, or the floor below is vacuous: {before} -> \
         {after}"
    );
    assert!(
        after >= floor_biomass - SAME_BIOMASS,
        "a keeper crew of {HUNT_WORKERS} must not collect below the floor {floor_biomass}: got \
         {after}"
    );
    assert!(
        after - floor_biomass < herd.body_mass,
        "…and it stops within one whole animal of it ({} against a body of {}): got {after}",
        after - floor_biomass,
        herd.body_mass
    );
}

/// **A FRACTIONAL HANDLING RATE COLLECTS WHOLE ANIMALS** — the herd loses exactly `killed ×
/// body_mass`, so the count and the biomass describe one event.
///
/// The tend branch handed the raw rate in, and [`core_sim::AnimalTake::killed`] truncates: a crew
/// handling `3.6` animals reported **3 killed** while the flock lost **3.6 bodies**. A keeper does
/// not walk out half a beast, so the rate is floored where the fight resolves it — the same
/// `resolve_hunt_fight` the range runs, since §4.9 item 12b — and the remainder stays standing in
/// the pen, the way every other whole-animal wait carries (the herd's own biomass is the
/// accumulator).
///
/// **The fixture has to author the rate**, because the shipped `pen_engage_gain` of `20` is
/// deliberately high enough that the keepers' *carry* binds first on every pennable species — the
/// handling arm exists to be reachable, not to be reached.
#[test]
fn a_fractional_pen_handling_rate_collects_whole_animals() {
    // **Two rates in two different whole-animal bands**, so the drop is pinned to the handling arm
    // rather than to something that happens to be three animals wide: a carry or a room binding
    // below `4` would answer the second case with the first case's number.
    for (handling_per_keeper, whole_animals) in [(3.6_f32, 3.0_f32), (4.6, 4.0)] {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        corral_herd(&mut app, &id);
        let cap = herd_of(&app, &id).carrying_capacity;
        author_pen_handling_rate(&mut app, handling_per_keeper);

        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        assert!(
            (core_sim::herd_engage_rate(&herd_of(&app, &id), &fauna) - handling_per_keeper).abs()
                < SAME_BIOMASS,
            "the fixture must hand one keeper a fractional rate, or this test is about nothing"
        );

        let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, ONE_KEEPER);
        let (before, after) = pen_collection_turn(&mut app, &id, band, cap);

        let herd = herd_of(&app, &id);
        // **The room is not what bound this turn** — it affords far more than the keeper can
        // handle, which is what makes the drop a reading of the handling arm.
        let room = core_sim::herd_take_room(&herd, MSY_BIOMASS_FRACTION, &fauna);
        assert!(
            core_sim::animals_affordable(room, herd.body_mass) > handling_per_keeper,
            "the fixture's room ({room}) must not be the binding arm"
        );

        let taken = before - after;
        let animals = taken / herd.body_mass;
        assert!(
            (animals - whole_animals).abs() < SAME_BIOMASS,
            "one keeper handling {handling_per_keeper} animals collects {whole_animals}: the flock \
             lost {taken} biomass, which is {animals} bodies"
        );

        // …and the two currencies describe ONE event: the biomass the flock lost is an exact whole
        // number of BODIES, never the fractional handling rate shaved off the stock. This used to
        // be re-derived through the pen's own `animals_handled` clamp; since §4.9 item 12b there is
        // no pen-side helper to re-derive from — the whole-animal floor is `resolve_hunt_fight`'s,
        // inside `systems::hunt_take`, like every other rung's — so the claim is asserted where it
        // is observable, on the herd the turn actually drew from.
        assert!(
            (taken - whole_animals * herd.body_mass).abs() < SAME_BIOMASS,
            "the count and the biomass must describe one event: {whole_animals} bodies of \
             {} is {}, and the flock lost {taken}",
            herd.body_mass,
            whole_animals * herd.body_mass
        );
    }
}

/// Author [`FIXTURE_SPECIES`] a pen handling rate of `animals_per_keeper` — through the species'
/// `engage_rate`, so `husbandry.pen_engage_gain` (validated `>= 1.0`) keeps the value it ships with
/// and the fixture changes only how hard *this* animal is to handle.
fn author_pen_handling_rate(app: &mut App, animals_per_keeper: f32) {
    let mut handle = app.world.resource_mut::<FaunaConfigHandle>();
    let mut config = (*handle.get()).clone();
    // **THE GAIN IS RESOLVED PER SPECIES, and reading the global here made the fixture author the
    // wrong rate.** `fauna::herd_engage_rate` asks `FaunaConfig::pen_engage_gain_for`, which prefers
    // this species' own override and falls back to `husbandry.pen_engage_gain` — so a species that
    // grew a row (the fixture's own did) is divided by a number the sim never multiplies back, and
    // the liveness assertion above is what caught it. Ask the same seam the sim asks.
    let gain = config.pen_engage_gain_for(FIXTURE_SPECIES);
    let species = config
        .species
        .values_mut()
        .find(|def| def.display_name == FIXTURE_SPECIES)
        .expect("the fixture species is in the roster");
    species.engage_rate = animals_per_keeper / gain;
    handle.replace(std::sync::Arc::new(config));
}

/// **THE ACCEPTANCE BAR FOR THE RUNG-3 RE-EXPRESSION** — the animal twin of
/// `field_reference_basket.rs`, on the herd this file already pins ([`FIXTURE_SPECIES`]).
///
/// A pen stopped being a *managed harvest* and became a *production gain*: it is hunted through the
/// ordinary drawn-down path exactly as a wild and a pastoral herd are — escapement floor live,
/// worker-capped, **engagement-bounded**, over-hunt reachable — and what rung 3 buys is faster
/// breeding, a denser `K`, a slower escape and an easier handle. That is a **re-expression, not a
/// rebalance**, so the settled yield has to land where it already was.
///
/// **Measured at the SETTLED operating point**, over a long enough run for each rung to converge,
/// because that is the only place a rate means anything: a herd seated at `K` hands over a one-off
/// windfall identical at every rung, which measures the fixture rather than the ladder.
#[test]
fn the_re_expressed_pen_lands_where_the_managed_rate_did() {
    /// Long enough for every rung's logistic to settle on its own operating point.
    const CONVERGENCE_TURNS: u32 = 80;
    /// Sustain — the stance a managed source is worked under, and the one the retired
    /// constant-escapement take coincided with.
    const REFERENCE_FLOOR: f32 = 0.5;
    /// How far the re-expressed pen may land from the number it replaced.
    const ACCEPTANCE_BAND: f32 = 0.05;
    /// Slack on the two rungs this arc does not touch — a pen gain leaking onto either fails here.
    const UNCHANGED_BAND: f32 = 0.01;

    /// **What each rung paid under the retired managed-harvest model**, provisions/turn on this
    /// herd. Recorded from the measurement itself, so the re-expression is checkable rather than
    /// asserted against algebra.
    const WILD_BEFORE: f32 = 0.3510;
    const PASTORAL_BEFORE: f32 = 0.6966;
    const PEN_BEFORE: f32 = 0.9990;

    let settled = |rung: u8| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        match rung {
            0 => {}
            1 => {
                let mut registry = app.world.resource_mut::<HerdRegistry>();
                let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
                herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            }
            _ => {
                corral_herd(&mut app, &id);
            }
        }
        let keeper = spawn_hunter(&mut app, &id, REFERENCE_FLOOR);
        let cap = herd_of(&app, &id).carrying_capacity;
        let mut last = 0.0f32;
        for _ in 0..CONVERGENCE_TURNS {
            // Never let a pen's feed run out — the starvation path has its own test, and a hungry
            // pen would be measuring that instead. **Hay**, because that (with the footprint's grass,
            // which this harness does not run) is what a pen eats.
            feed_the_pens(&mut app, keeper, cap);
            run_turns_with_hunt(&mut app, 1);
            last = yield_of(&app, keeper);
        }
        last
    };

    let wild = settled(0);
    let pastoral = settled(1);
    let pen = settled(2);

    assert!(
        (wild - WILD_BEFORE).abs() <= WILD_BEFORE * UNCHANGED_BAND,
        "rung 1 must not move: {wild} against {WILD_BEFORE}"
    );
    assert!(
        (pastoral - PASTORAL_BEFORE).abs() <= PASTORAL_BEFORE * UNCHANGED_BAND,
        "rung 2 must not move: {pastoral} against {PASTORAL_BEFORE} — a pen gain that reached the \
         pastoral rung would show up exactly here"
    );
    assert!(
        (pen - PEN_BEFORE).abs() <= PEN_BEFORE * ACCEPTANCE_BAND,
        "rung 3 must land where the managed rate did: {pen} against {PEN_BEFORE} (wild {wild}, \
         pastoral {pastoral})"
    );
    assert!(
        pen > pastoral && pastoral > wild,
        "the ladder must pay more at every rung: {wild} -> {pastoral} -> {pen}"
    );
}

/// **THE PEN EATS, AND IT DOES NOT EAT THE PEOPLE'S FOOD.** A keeper tending a pen credits its
/// larder with the harvest and debits it for nothing at all — the larder comes out at exactly
/// `stock + gross yield`.
///
/// This test used to assert the opposite: `larder == stock − upkeep_per_biomass × biomass + gross`,
/// *"the keeper must bring it food"*. **Human food is not animal feed.** A penned herd eats the grass
/// its fenced footprint grows and the hay its keeper carries in; what those leave short leaves the
/// **herd** hungry, never the band. The pen here is on a barren footprint with no hay, so it is
/// maximally short — which is precisely the state the retired larder draw would have billed for.
///
/// **Seated at the OPERATING POINT (slice 8).** `corral_herd` seats the herd at `B = K`, where the
/// pen's escapement harvest is the standing **stock** `K/2` — a one-off windfall identical at every
/// rung, not the `r·K/4` *rate* this test measures the gross yield against. So it re-seats to where a
/// running pen actually stands (`K/2` + the turn's regrowth) before measuring.
#[test]
fn tending_a_pen_never_debits_the_keepers_larder() {
    const STOCK: f32 = 500.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let (pen_r, prov_rate) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            // Per-species pen rate (Grazing 2d): the corralled herd's own rung.
            herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate,
            fauna.hunt.provisions_per_biomass,
        )
    };
    // Re-seat onto the settled operating point — `corral_herd` leaves the herd at capacity.
    let cap = herd_of(&app, &id).carrying_capacity;
    let pen_msy = pen_r * cap / 4.0;
    reseat(&mut app, &id, cap, cap * MSY_BIOMASS_FRACTION + pen_msy);

    let keeper = spawn_hunter(&mut app, &id, 0.5);
    stock_larder(&mut app, keeper, STOCK);

    // One Population turn only, so the herd's biomass (and thus the demand) is the one we measured.
    app.world.run_system_once(advance_labor_allocation);

    let gross = yield_of(&app, keeper);
    let expected_gross = pen_msy * prov_rate;
    assert!(
        (gross - expected_gross).abs() < expected_gross * 0.02,
        "the credited yield is GROSS, and now also net: {gross} vs {expected_gross}"
    );
    // **NOT ONE UNIT WAS TAKEN FOR FEED**: larder = stock + gross yield, with no third term.
    let expected_larder = STOCK + gross;
    let larder = larder_of(&app, keeper);
    assert!(
        (larder - expected_larder).abs() < 0.05,
        "the pen debits NOTHING from the larder: larder {larder} vs stock {STOCK} + yield {gross} \
         = {expected_larder}"
    );
    // **Not vacuous**: the pen really was short of feed this turn, which is the exact state the
    // retired larder draw billed for.
    assert!(
        fed_fraction_of(&app, &id) < 1.0,
        "the pen is on a barren footprint with no hay, so it must read underfed (got {})",
        fed_fraction_of(&app, &id)
    );
}

/// A penned herd's `pen_fed_fraction` — the share of its fodder demand grass and hay covered.
fn fed_fraction_of(app: &App, id: &str) -> f32 {
    herd_of(app, id).pen_fed_fraction
}

/// **An underfed pen starves — and recovers.** A pen whose fenced footprint grows nothing and whose
/// keeper has no hay for it cannot be fed, so the herd shrinks (its yield falling with it) and floors
/// at the extinction floor rather than despawning or losing the pen. Grow it some hay and it comes
/// back. Letting your animals go hungry is a *decision* — what you spend a Field on — not an
/// accident.
///
/// **The fixture is back in its natural shape.** It was posed at a harvest floor of `1.0` (take
/// nothing at all) only to defeat the retired larder fallback: the feed was settled off the larder at
/// the *end* of the labor pass, so a keeper who slaughtered out of the pen had that meat in hand and
/// fed the herd back some of it, and draining the store alone no longer starved a pen that paid for
/// itself. With human food out of the feed model the keeper works its pen at [`SUSTAIN`] like any
/// other, harvests it, and the herd still starves — because what it is short of is grass.
#[test]
fn an_underfed_pen_shrinks_to_a_remnant_then_recovers_when_fed() {
    const STARVE_TURNS: u32 = 40;
    const RECOVER_TURNS: u32 = 30;
    /// Hold the herd on its most productive biomass and carry the surplus home — the ordinary
    /// keeper's floor, and the one the fixture wanted all along.
    const SUSTAIN: f32 = 0.5;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let cap = herd_of(&app, &id).carrying_capacity;
    let floor = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        fauna.husbandry.pen.ecology.extinction_floor * cap
    };
    let keeper = spawn_hunter(&mut app, &id, SUSTAIN);

    // Starve it: no hay, ever. `run_turns_with_hunt` never grazes the footprint either, so the pen
    // has no feed at all. The larder is kept **full** throughout — the point is that a stocked band
    // does not rescue a pen it cannot feed.
    let mut previous = herd_of(&app, &id).biomass;
    for _ in 0..STARVE_TURNS {
        stock_larder(&mut app, keeper, cap);
        run_turns_with_hunt(&mut app, 1);
        let now = herd_of(&app, &id).biomass;
        assert!(
            now <= previous + 1e-3,
            "an unfed pen must never grow: {previous} -> {now}"
        );
        previous = now;
    }

    let starved = herd_of(&app, &id);
    assert!(
        starved.is_corralled(),
        "a starved pen is NOT lost — it withers"
    );
    assert!(
        (starved.biomass - floor).abs() < floor * 0.05,
        "a starved herd converges on the extinction floor ({floor}), not zero and not oscillating: {}",
        starved.biomass
    );
    // The famine is announced exactly once (edge-gated), naming the species, never the internal id.
    let lines = corral_feed_lines(&app);
    let starving: Vec<_> = lines
        .iter()
        .filter(|e| {
            e.detail
                .as_deref()
                .unwrap_or_default()
                .contains("status=starving")
        })
        .collect();
    assert_eq!(starving.len(), 1, "the famine is announced ONCE: {lines:?}");
    assert!(
        starving[0].label.contains(&starved.species) && starving[0].label.contains("starving"),
        "the line names the species and says what happened: {}",
        starving[0].label
    );

    // Feed it again — HAY, not bread → it recovers (the pen's r = 0.60 is the fastest curve on the
    // ladder).
    let remnant = herd_of(&app, &id).biomass;
    for _ in 0..RECOVER_TURNS {
        feed_the_pens(&mut app, keeper, cap);
        run_turns_with_hunt(&mut app, 1);
    }
    let recovered = herd_of(&app, &id);
    assert!(
        recovered.biomass > remnant * 2.0,
        "a re-fed pen recovers: {remnant} -> {}",
        recovered.biomass
    );
    assert!(recovered.is_corralled(), "and it still has its pen");
}

/// **The husbandry ladder, per species — now a per-species GROWTH-RATE ladder (Grazing 2d §3).**
/// Rabbit Warren (K=200) → Red Deer (K=1200) → Thunder Mammoths (K=12000), each measured at its **own
/// per-species wild `r`** (rabbit 0.35, deer 0.10, mammoth 0.04). Every number below is **measured from
/// a real sim run** (a band's actual take / a real larder debit), never arithmetic.
///
/// **2d retires the flat pastoral 0.25 / pen 0.90 and the fast-breeder pastoral inversion with them.**
/// The managed rungs now scale each species' own wild `r` (`pastoral_gain` 1.5, `pen_gain` 3.0, capped
/// at 0.75), so `pastoral_r = wild_r × 1.5 > wild_r` for **every** species — the pastoral rung out-pays
/// wild Sustain unconditionally, and the pen's GROSS growth-rate tops pastoral unconditionally. That
/// GROSS ladder (`wild < pastoral < pen_gross`) is what "management buys a growth rate" means, and it is
/// the invariant asserted here.
///
/// **Slice 3b makes it a YIELD-PER-WORKER ladder, which is what the ladder now promises.** Every rung
/// is worker-driven, so all three rows are measured the same way: the **same band, the same head-count
/// (`HUNT_WORKERS`), the same `K`, the same per-species wild `r`** — only the rung differs. The
/// monotone `wild < pastoral < pen` therefore reads directly as *food per worker*, and the
/// `pastoral / wild` ratio is asserted to be exactly `pastoral_gain` — the payoff that replaced
/// "pastoral = zero workers".
///
/// **Slice 8 makes it a LONG-RUN average, and that is a correction to the MEASUREMENT, not a
/// weakening of the guarantee.** Every hunt is constant escapement now, so a herd hands over
/// `B − K/2` — a **stock**, not a rate. At `B = K` that is `K/2` **for every rung**: `r` cancels
/// clean out of `K − K/2`, so a full herd's first harvest is *identical* wild, pastoral and penned.
/// That is not a bug and it must not be "fixed": the surplus standing above the escapement point is
/// **accumulated stock**, and stock does not care how fast you breed. What management buys is that
/// **the next animal comes sooner** — so the ladder is monotone in the rate a rung sustains over
/// time, which is exactly `r·K/4`, and the only way to see it is to average a run long enough to
/// contain the refills. Hence: seat at the operating point, run `MEASURE_TURNS`, average.
///
/// A single turn cannot measure this any more, at either biomass. At `B = K` you read the stock
/// (rung-blind). At `B = K/2` you read *zero* for any species whose one-turn MSY is lighter than one
/// animal (a wild mammoth: 120 biomass of regrowth against an 800-unit beast) — the herd correctly
/// **waits**. Both readings are honest; neither is the ladder.
///
/// **What 2d does NOT guarantee at the BARREN worst case** (this harness runs no graze layer, so the pen
/// is fully larder-fed): the pen's *net* payoff over pastoral. A penned herd normally grazes its fenced
/// footprint and the larder pays only the shortfall (§2.3), so on real pasture `upkeep → 0` and
/// `pen_net → pen_gross`, topping pastoral. Fully larder-fed, the feed is a real cost that can erase the
/// advantage — and for a slow breeder the barren pen is a **net loss by design** (§2.4: mammoth pen
/// `r ≈ 0.12`, feed > yield). `FaunaConfig::validate` enforces only a best-case floor (the *fastest*
/// breeder stays net-positive even fully larder-fed); the rest is a placement decision, not a config
/// error. So this test asserts the GROSS growth-rate ladder + that the barren pen eats real hay, and
/// records the hay rate for observability rather than netting it against the yield — a pen pays
/// provisions and eats fodder, two stores that never trade.
#[test]
fn the_husbandry_ladder_is_a_per_species_growth_rate_ladder() {
    /// The `FODDER` store every pen row is topped back up to before each measured turn — deep enough
    /// that hay is never the binding constraint, so the draw measures the pen's demand.
    const MEASURE_HAY: f32 = 50_000.0;
    /// Turns averaged per rung, seeded at the operating point.
    ///
    /// Sized by the **slowest pulse the table contains**: a wild Thunder Mammoth sustains
    /// `r·K/4` = 120 biomass/turn against an **800-unit body**, so it spares one beast roughly every
    /// 7 turns. 600 turns is ~85 of those cycles — enough that the ≤1 uncollected body still standing
    /// at the end is a fraction of a percent of the run, so the average reads the rung's rate rather
    /// than where its last pulse happened to land. Every other row pulses far faster.
    const MEASURE_TURNS: u32 = 600;

    // The pastoral rung's promised multiple of the wild rung — read off the config, never pinned.
    let pastoral_gain = FaunaConfigHandle::default().get().husbandry.pastoral_gain;
    // (display, cap, per-species wild r, body_mass) — the wild rung must be measured at each species'
    // OWN r, and since slice 8 at its own **body**: the take quantises to whole animals, so a
    // mammoth's `K` measured against the fixture's 1-unit fowl body would be a different economy
    // entirely (it would never wait).
    let species_caps: Vec<(String, f32, f32, f32)> = {
        let fauna = FaunaConfigHandle::default().get();
        let wild_default = fauna.ecology.regrowth_rate;
        ["rabbit", "deer", "mammoth"]
            .iter()
            .map(|key| {
                let def = &fauna.species[*key];
                (
                    def.display_name.clone(),
                    def.carrying_capacity(),
                    def.regrowth_rate_or(wild_default),
                    def.body_mass,
                )
            })
            .collect()
    };

    // **Measured once, as a long-run average from the settled operating point** (`B* = K/2` — where a
    // harvested herd converges, and the point the pen's net-positive invariant is derived against).
    // The retired "at capacity (B = K)" pass measured the standing **stock** every rung shares (see
    // the doc comment), not the ladder.
    //
    // Every row runs **full turns in real stage order** (Logistics: `advance_herds` regrows →
    // `advance_husbandry`; Population: `advance_labor_allocation`), so the numbers are what the sim
    // pays, not what a single system does in isolation. The feed is charged on the *post-regrowth*
    // biomass (you feed every animal in the pen, including the ones you are about to harvest).
    println!(
        "\n=== husbandry ladder, MEASURED as the {MEASURE_TURNS}-turn average from the operating \
         point (B* = K/2) (provisions/turn) ==="
    );
    println!(
        "{:<18} {:>8} {:>9} {:>9} {:>11} {:>9}",
        "species", "K", "wild", "pastoral", "pen gross", "hay/turn"
    );
    for (species, cap, wild_r, body_mass) in &species_caps {
        let (species, cap, wild_r, body_mass) = (species.clone(), *cap, *wild_r, *body_mass);
        let biomass = cap * MSY_BIOMASS_FRACTION;

        // --- Wild Sustain: a band hunting a wild herd — its ACTUAL take, from the yield telemetry.
        // Seat the herd at THIS species' per-species wild `r` (2b-ii) and body (slice 8), since the
        // spawned short-range game the harness reuses carries its own.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, biomass);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        let band = spawn_hunter(&mut app, &id, 0.5);
        let wild = average_yield_over_run(&mut app, band, MEASURE_TURNS);

        // --- Pastoral: **the same band, the same head-count, hunting a TAMED herd** — its ACTUAL
        // take. Passive-free pastoral is retired (slice 3b), so this row is now measured exactly
        // like the wild one and the three rows are directly comparable **per worker**: same
        // workers, same `K`, only the rung differs.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, biomass);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        domesticate(&mut app, &id);
        let band = spawn_hunter(&mut app, &id, 0.5);
        let pastoral = average_yield_over_run(&mut app, band, MEASURE_TURNS);

        // --- Pen: the gross yield credited, and the HAY it ate — the feed is fodder, so it is read
        // off the `FODDER` store and never off the larder, which a pen no longer touches.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, cap);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        corral_herd(&mut app, &id);
        reseat(&mut app, &id, cap, biomass); // corral_herd seats at cap; re-seat for B*
        let keeper = spawn_hunter(&mut app, &id, 0.5);
        let (pen_gross, hay_eaten) =
            average_pen_yield_and_hay(&mut app, keeper, MEASURE_TURNS, MEASURE_HAY);

        println!(
            "{species:<18} {cap:>8.0} {wild:>9.3} {pastoral:>9.3} {pen_gross:>11.3} {hay_eaten:>9.3}"
        );

        assert_growth_rate_ladder(
            &species,
            wild_r,
            pastoral_gain,
            wild,
            pastoral,
            pen_gross,
            hay_eaten,
        );
    }
    println!();
}

/// **One rung's LONG-RUN average take**, in provisions/turn: run the full turn pipeline `turns` times
/// and average the band's *actual* take off the retained yield telemetry.
///
/// This is the only honest way to read a rung since slice 8 made the hunt constant escapement on
/// whole animals: a single turn reads either the standing **stock** (at `B = K`, identical at every
/// rung) or a **pulse** (at `B*`, where a herd whose MSY is lighter than one animal takes nothing and
/// waits). Averaged over many refill cycles, both artifacts wash out and what is left is the rate the
/// rung sustains — which *is* the thing the ladder claims to raise. See the caller's doc comment.
fn average_yield_over_run(app: &mut App, band: bevy::prelude::Entity, turns: u32) -> f32 {
    let mut total = 0.0;
    for _ in 0..turns {
        run_turns_with_hunt(app, 1);
        total += yield_of(app, band);
    }
    total / turns as f32
}

/// [`average_yield_over_run`] for the **pen**, which also has a bill: returns
/// `(mean gross yield, mean hay eaten)`.
///
/// The hay store is topped back up to `hay` **before every turn**, so each turn's draw is readable in
/// isolation (`eaten = hay − remaining`) *and* the pen never goes hungry mid-run — an unfed
/// pen shrinks (`starve_shrink_rate`), which would quietly turn this into a measurement of starvation
/// rather than of the rung.
fn average_pen_yield_and_hay(
    app: &mut App,
    keeper: bevy::prelude::Entity,
    turns: u32,
    hay: f32,
) -> (f32, f32) {
    let mut gross_total = 0.0;
    let mut hay_total = 0.0;
    for _ in 0..turns {
        feed_the_pens(app, keeper, hay);
        run_turns_with_hunt(app, 1);
        gross_total += yield_of(app, keeper);
        // What the pen ate came out of the FODDER store, which this turn topped up to `hay`.
        hay_total += hay - fodder_of(app, keeper);
    }
    (gross_total / turns as f32, hay_total / turns as f32)
}

/// The band's remaining `FODDER` (hay) store.
fn fodder_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("band exists")
        .stores
        .get(FODDER)
        .to_f32()
}

/// The **per-species GROWTH-RATE ladder** (Grazing 2d §3), asserted on **measured** numbers. Since the
/// managed rungs now scale each species' own wild `r` (`pastoral_gain` 1.5 < `pen_gain` 3.0, capped),
/// the ladder is monotone in GROSS yield for **every** species — the old fast-breeder pastoral
/// inversion is gone. The pen's payoff over pastoral is realized by SELF-FEEDING: this barren harness
/// runs the pen entirely on **hay**, so it only asserts that the pen eats real fodder — which is what
/// a lush footprint saves the keeper from having to farm.
#[allow(clippy::too_many_arguments)] // one measured column per argument — a struct would only rename them
fn assert_growth_rate_ladder(
    species: &str,
    wild_r: f32,
    pastoral_gain: f32,
    wild: f32,
    pastoral: f32,
    pen_gross: f32,
    hay_eaten: f32,
) {
    assert!(
        wild > 0.0,
        "{species}: a thriving wild herd has a positive Sustain MSY ({wild})"
    );
    // The fast-breeder pastoral inversion is FIXED (2d §3): pastoral r = wild_r × 1.5 > wild_r for
    // every species, so the pastoral rung out-pays wild Sustain unconditionally.
    assert!(
        pastoral > wild,
        "{species}: pastoral ({pastoral}) out-pays wild Sustain ({wild}) — per-species pastoral r = \
         wild r ({wild_r}) × pastoral_gain > wild r"
    );
    // **And by exactly the gain** (slice 3b): the rows are equal-worker and equal-K, so this ratio IS
    // the yield-per-worker payoff for taming — the whole of what replaced passive-free pastoral.
    assert!(
        (pastoral / wild - pastoral_gain).abs() < 0.02 * pastoral_gain,
        "{species}: the SAME workers on a tamed herd take pastoral_gain ({pastoral_gain}×) the wild \
         take — that multiple IS the taming payoff. wild {wild} → pastoral {pastoral}"
    );
    // Management buys a growth rate: the pen's GROSS yield tops the pastoral rung for every species.
    assert!(
        pen_gross > pastoral,
        "{species}: the pen's GROSS yield ({pen_gross}) tops the pastoral rung ({pastoral})"
    );
    // The barren pen costs real feed — the worst-case cost self-feeding removes (§2.3) — and that
    // feed is **HAY**, out of the `FODDER` store, never the people's bread. There is no `pen_net`
    // column any more because the two quantities are in different units and different stores: a pen
    // pays provisions and eats fodder, and subtracting one from the other was only ever possible
    // while its feed came out of the larder.
    assert!(
        hay_eaten > 0.0,
        "{species}: the barren pen eats real hay ({hay_eaten}/turn) — the fodder cost self-feeding \
         is what removes"
    );
}

/// Seat the two cached per-species traits a rung is measured against — the **wild** regrowth rate
/// (Grazing 2b-ii) and the **body mass** (slice 8) — since the harness reuses one spawned short-range
/// herd for every row and that herd carries whatever species the map happened to place.
///
/// **Both, together, or the row is a different animal.** `r` alone was enough while the take was a
/// smooth flow; now the take quantises to whole bodies, so `r` and `body_mass` jointly decide the
/// *rhythm* (`body_mass / (r·K/4)` turns per animal). A mammoth's `K` and `r` measured against a
/// 1-unit fowl body would never wait for a whole animal — the exact property the slice added.
fn seat_species_traits(app: &mut App, id: &str, r: f32, body_mass: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.regrowth_rate = r;
    herd.body_mass = body_mass;
}

// **RETIRED (`docs/plan_fauna_neglect_escape.md`):** the two tameness-bleed tests
// `a_properly_herded_tamed_herd_does_not_decay_under_a_harvest_policy` and
// `an_under_herded_tamed_herd_decays_proportionally_and_recovers` are gone with `decay_under_herded`.
// Neglect no longer touches `domestication_progress` at all — it sheds ANIMALS. The replacement
// guarantee ("tameness does not change under neglect") is `neglect_never_un_tames_a_herd` below, and
// the shed mechanic itself is covered by `an_over_stocked_managed_herd_converges_to_its_labor_capacity`,
// `a_pen_sheds_slower_than_a_pastoral_herd`, `total_abandonment_sheds_the_flock_and_loses_the_pen`,
// `shed_animals_appear_in_the_wild_web`, and `the_shed_is_deterministic`.

/// A corralled herd left **totally untended** no longer breaks out in one turn — under the neglect-escape
/// model it **sheds its flock to the wild web over many turns** and, when the last animal is gone, the
/// pen is announced lost and the empty entity **despawns** (`docs/plan_fauna_neglect_escape.md` §2.4).
/// What survives from the old binary escape: the pen IS eventually lost — just gradually and visibly,
/// not on a silent turn-2 flip. Fully asserted by `total_abandonment_sheds_the_flock_and_loses_the_pen`
/// below; this stub keeps the old name pointed at the new behaviour so a future reader greps here.
#[test]
fn untended_corral_escapes_to_mobile() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    assert!(app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .is_corralled());

    // A handful of untended turns is NOT enough to lose the pen now — the flock is still shedding.
    run_turns_untended(&mut app, 3);
    assert!(
        corral_feed_lines(&app).is_empty(),
        "the pen is not lost after 3 turns — it sheds gradually now, not in a binary escape"
    );

    // Run it out: the flock bleeds to nothing, the pen is announced lost, and the entity despawns.
    assert!(
        run_untended_until_pen_lost(&mut app, 200).is_some(),
        "an untended pen eventually sheds to nothing and loses the pen"
    );
    assert!(
        corral_feed_lines(&app)
            .iter()
            .any(|e| e.label.contains("drifted off") && e.label.contains("pen is lost")),
        "the loss is announced in the feed"
    );
    assert!(
        app.world.resource::<HerdRegistry>().find(&id).is_none(),
        "the bled-out herd is gone from the registry — no ownerless husk"
    );
}

/// The one-turn grace holds: a **freshly-penned** herd is spared its first `advance_husbandry` pass
/// (`corral_at` marks it tended), so a keeper has a turn to take up the tending assignment.
#[test]
fn freshly_penned_herd_survives_its_grace_turn() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);

    run_turns_untended(&mut app, 1);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(
        herd.is_corralled(),
        "the grace turn spares a freshly-penned herd"
    );
    // **A completed rung reads its own cost, not a normalized `1.0`.** The retired `corral_at`
    // fabricated a one-unit job when nobody had banked the work; the position is seated at the
    // rung's real top now, so the honest statement of *"still complete"* is done == cost.
    let ladder = core_sim::LadderConfig::builtin();
    assert_eq!(
        herd.rung_work_done(core_sim::RungKey::AnimalPen, &ladder),
        herd.rung_cost(core_sim::RungKey::AnimalPen, &ladder),
        "a spared pen keeps its completed progress"
    );
    assert!(
        corral_feed_lines(&app).is_empty(),
        "no escape line on the grace turn — nothing was lost"
    );
}

/// Losing the pen (now on shed-to-zero, `docs/plan_fauna_neglect_escape.md` §2.4) **destroys a 25-turn
/// investment**, so it must never be silent: it pushes a `CommandEventKind::Corral` feed line naming the
/// **species** (not the internal herd id) and saying both what happened and why, with the
/// machine-readable bits in the detail field.
#[test]
fn corral_escape_announces_the_lost_pen_in_the_feed() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let species = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .species
        .clone();

    // Run the abandonment out to the bleed-out, when the pen is lost and the entity despawns.
    assert!(
        run_untended_until_pen_lost(&mut app, 200).is_some(),
        "an untended pen eventually bleeds out and loses the pen"
    );

    let lines = corral_feed_lines(&app);
    assert_eq!(lines.len(), 1, "exactly one pen-lost line: {lines:?}");
    let entry = &lines[0];
    assert_eq!(entry.faction, FactionId(0), "the owner is told");
    assert!(
        entry.label.contains(&species) && !entry.label.contains(&id),
        "the human line names the species, not the internal id: {}",
        entry.label
    );
    assert!(
        entry.label.contains("drifted off") && entry.label.contains("pen is lost"),
        "the line says what happened AND why: {}",
        entry.label
    );
    let detail = entry.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("status=escaped")
            && detail.contains("reason=untended")
            && detail.contains(&format!("herd={id}")),
        "the detail carries the machine-readable fields: {detail}"
    );
}

/// **The pen is lost, not merely opened — the whole entity is gone.** When an untended pen bleeds out,
/// the empty managed herd **despawns** (`docs/plan_fauna_neglect_escape.md` §2.4): nothing is inherited
/// because nothing remains. Re-penning is a fresh herd's fresh investment, not a snap-back from a
/// retained meter — there is no herd to snap back.
#[test]
fn escaped_corral_loses_its_pen_progress_and_must_rebuild() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);

    // Bleed the flock out — the pen is lost and the entity despawns the turn the last animal goes.
    assert!(
        run_untended_until_pen_lost(&mut app, 200).is_some(),
        "an untended pen eventually bleeds out and loses the pen"
    );
    assert!(
        app.world.resource::<HerdRegistry>().find(&id).is_none(),
        "the pen — and the herd with it — is gone, so nothing is inherited on re-penning"
    );
}

/// **The lost pen's whole FENCE dies with the entity** (`docs/plan_fauna_neglect_escape.md` §2.4). A
/// completed pen with a grown radius (and even a ring mid-extension) that bleeds out despawns entirely,
/// so `pen_radius` / `pen_extend_progress` / `pen_extending` cannot be inherited for free — the entity
/// that carried them is gone. (The old model reset the fields on a surviving mobile herd; the refined
/// one removes the herd, which is the same guarantee with less state to keep straight.)
#[test]
fn escaped_corral_resets_the_fenced_footprint_no_free_extension() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    // Give the completed pen a grown, mid-extension fence.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.pen_radius = 2;
        herd.pen_extend_progress = 0.5;
        herd.pen_extending = true;
        herd.pen_pasture_fraction = 1.0;
    }

    // Bleed the flock out — the pen is lost and the whole entity (fence and all) despawns.
    assert!(
        run_untended_until_pen_lost(&mut app, 200).is_some(),
        "an untended pen eventually bleeds out and loses the pen"
    );
    assert!(
        app.world.resource::<HerdRegistry>().find(&id).is_none(),
        "the grown fence is gone with the despawned herd — nothing to inherit"
    );
}

/// Guard against over-reaching the escape fix: a **half-built** pen whose gate lapses (its keeper
/// leaves mid-build) **keeps** its progress — materials on the ground at a tile the herd is still at.
/// Only a *completed* pen that escapes loses it.
#[test]
fn half_built_pen_keeps_progress_when_its_keeper_leaves() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    }
    grant_penning(&mut app);
    let band = spawn_builder(&mut app, &id, Improvement::Corral);

    run_turns_with_hunt(&mut app, 5);
    let half_built = corral_progress_of(&app, &id);
    assert!(
        half_built > 0.0 && !herd_of(&app, &id).is_corralled(),
        "the pen should be part-built: {half_built} work units banked"
    );

    // The keeper walks off mid-build — the investment is NOT an escape and is NOT lost.
    app.world.despawn(band);
    run_turns_untended(&mut app, 5);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(
        !herd.is_corralled(),
        "a half-built pen never penned the herd"
    );
    assert_eq!(
        herd.rung_work_done(
            core_sim::RungKey::AnimalPen,
            &core_sim::LadderConfig::builtin()
        ),
        half_built,
        "a mid-build lapse keeps its progress"
    );
}

/// Regression (fully-fractional FOOD income): a **tiny** tamed herd whose per-turn MSY harvest is
/// below 1.0 provisions must still credit the larder — rounding the credit to an i64 used to drop it
/// entirely.
///
/// **Retargeted to the worker path** (slice 3b retired the passive payout this used to ride), and
/// re-seated onto a deliberately tiny `K` so the take is sub-unit for **every** shipped species'
/// `r` rather than for the one the map happened to spawn.
///
/// **Re-seated again for whole animals (slice 8).** It used to seat `B = 0.52 · K` and lean on the
/// take being the *flow* `r·K/4` — a fraction of an animal, which now quantises to **zero** and the
/// hunt waits. That is correct behaviour and it deletes the case the test exists for, so the seat is
/// now stated in the unit the take is actually denominated in: **`K/2` plus exactly one body**, so
/// the herd spares precisely one beast. The credit is then `body_mass × provisions_per_biomass` —
/// **0.02 on the fixture's 1-unit Wild Fowl**, still emphatically sub-unit, so the i64-rounding
/// regression is exercised at the smallest take the sim can now produce. (There is no longer *any*
/// seat that yields a sub-unit take on a heavy species: one mammoth is 16 provisions. The
/// fractional-credit path is a small-game property now, which is exactly what the quantiser means.)
#[test]
fn sub_unit_pastoral_yield_credits_larder() {
    /// A tiny herd, so `K/2 + one body` is still a Thriving fraction of `K`.
    const SUB_UNIT_CAP: f32 = 40.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    // Seat the herd a little over one animal above its Sustain escapement point, so the take is the
    // **smallest whole take that exists**: one body. `reseat` refreshes the phase, and the seat is
    // comfortably Thriving.
    //
    // **The margin is load-bearing, not slack.** The escapement ceiling is `B − K/2`, a subtraction
    // of two near-equal `f32`s, so seating *exactly* `K/2 + body` yields a room a few parts per
    // million under one body and the herd correctly (but uninterestingly) waits a turn. The margin
    // puts the fixture on the mechanic instead of on the rounding.
    const ONE_ANIMAL_WITH_MARGIN: f32 = 1.5;
    let body_mass = herd_of(&app, &id).body_mass;
    reseat(
        &mut app,
        &id,
        SUB_UNIT_CAP,
        SUB_UNIT_CAP * MSY_BIOMASS_FRACTION + body_mass * ONE_ANIMAL_WITH_MARGIN,
    );
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    spawn_hunter(&mut app, &id, 0.5);
    app.world.run_system_once(advance_labor_allocation);

    let larder = provisions_f32(&mut app);
    assert!(
        larder > 0.0 && larder < 1.0,
        "a sub-1 pastoral yield must credit a positive fractional amount (got {larder})"
    );
}

// --- The full climb (intensification ladder slice 4) ----------------------------------------------

/// Faction 0's ledger progress on `discovery`.
/// **A lesson fully learned** — the ladder's `knowledge.completion_threshold`, the bar
/// `intensification::knows` compares a faction's ledger progress against. It is deliberately NOT the
/// old `RUNG_COMPLETE`: the *ledger* is still normalized to `1.0` (slice 2 owns that half), while a
/// per-source build meter now reads in work units and has no single completion value at all.
const LESSON_LEARNED: f32 = 1.0;

fn ladder_knowledge(app: &App, discovery: u32) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), discovery)
        .to_f32()
}

/// Declare or withdraw the build on the band's (only) Hunt assignment — the sim side of the
/// client's checkbox, which is what the player does at each build leg of the climb. The stance is
/// untouched (issue #442): the crew keeps Sustain-hunting throughout.
///
/// **A DECLARATION AND A POOL ARE TWO THINGS** (`docs/plan_standing_upkeep.md` §2.5). The verb
/// appends a queue entry and names no crew; the hands stand on the band's `builders` row. So this
/// does both, and withdrawing takes both away — a queue entry with an idle pool builds nothing, and
/// a staffed pool with an empty queue has nothing to build.
fn set_hunt_improvement(
    app: &mut App,
    band: bevy::prelude::Entity,
    improvement: Option<Improvement>,
) {
    let (herd_id, stated) = {
        let allocation = app.world.get::<LaborAllocation>(band).expect("band exists");
        let assignment = &allocation.assignments[0];
        let herd_id = match &assignment.target {
            LaborTarget::Hunt { fauna_id, .. } => fauna_id.clone(),
            _ => panic!("the climb's band hunts"),
        };
        (herd_id, assignment.workers)
    };
    // The whole band builds, which is what this climb meant before the crews split — **plus the
    // maintenance rate the keeping is also paying** (§2.4), or a pool sized at the herd's own keeper
    // count nets nothing and the leg never completes.
    let rate = keeper_crew_at_capacity(app, &herd_id);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("band exists");
        let source = core_sim::BuildSource::Herd(herd_id.clone());
        match improvement {
            Some(declared) => {
                let builders = stated.saturating_add(rate);
                match allocation
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.target == LaborTarget::Builders)
                {
                    Some(row) => row.workers = builders,
                    None => allocation.assignments.push(LaborAssignment {
                        target: LaborTarget::Builders,
                        workers: builders,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    }),
                }
                assert!(
                    allocation.enqueue_build(source, core_sim::BuildJob::Rung(declared)),
                    "the climb's band works the herd it is building on"
                );
            }
            None => {
                allocation.unqueue_build(&source);
                allocation
                    .assignments
                    .retain(|assignment| assignment.target != LaborTarget::Builders);
            }
        }
    }
    fit_band_to_its_crews(app, band);
}

/// Run turns until `done`, returning how many it took. Capped so a leg that can never complete fails
/// loudly with its own name instead of hanging the suite.
fn turns_until(app: &mut App, leg: &str, cap: u32, done: impl Fn(&App) -> bool) -> u32 {
    for turn in 1..=cap {
        run_turns_with_hunt(app, 1);
        if done(app) {
            return turn;
        }
    }
    panic!("the '{leg}' leg never completed within {cap} turns");
}

/// **The pacing consequence of the knowledge pattern, measured end-to-end** (slice 4,
/// `docs/plan_intensification_ladder.md` §4/§4.3).
///
/// Reaching a pen is now a **four-leg climb**, and each leg is paced by *practising the rung below*:
///
/// | leg | what the player does | gated by / earns |
/// |---|---|---|
/// | 1 | Sustain-hunt the **wild** herd | earns **Herding** (~20 turns) |
/// | 2 | **`Tame`** it | needs Herding; fills this herd's meter |
/// | 3 | Sustain-hunt the **pastoral** herd | earns **Penning** (~20 turns) — *the new leg* |
/// | 4 | **`Corral`** it | needs Penning; builds the pen |
///
/// **Leg 3 is what slice 4 added**: before the §4.3 reshuffle, Herding gated `Corral` directly, so
/// the climb was legs 1-2-4 and a pen cost ~20 turns less. That is deliberate — one knowledge per
/// transition, and you cannot skip a rung you have not practised.
///
/// Asserted as **bands, not exact turn counts**: this pins the shape of the climb (and that no leg
/// silently collapses to zero — e.g. a gate accidentally left open, or rung 2 teaching the wrong
/// knowledge) without becoming a change-detector for the `knowledge`/`build` playtest dials.
#[test]
fn the_full_wild_to_pen_climb_is_paced_by_practising_each_rung() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    // A `pen`-ceiling species that actually reaches the top of the ladder, at `taming_rate` 0.8.
    rebadge_as(&mut app, &id, "boar");
    // The knowledge legs are crew-BLIND (a lesson is credited once per source per turn), so this
    // band is staffed at the rung's reference keeper crew throughout and re-staffed at leg 4 — the
    // two animal rungs were priced against different reference crews. See [`TAME_KEEPERS`].
    let keepers = keeper_crew(&app, &id);
    let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);

    // Leg 1 — practise the WILD rung: a Sustain hunt teaches Herding, and nothing else.
    let leg1 = turns_until(&mut app, "learn Herding", 60, |app| {
        ladder_knowledge(app, HERDING_DISCOVERY_ID) >= LESSON_LEARNED
    });
    assert_eq!(
        ladder_knowledge(&app, PENNING_DISCOVERY_ID),
        0.0,
        "a WILD herd teaches Herding only — Penning must NOT come free with it, or the climb \
         skips the rung the reshuffle exists to add"
    );

    // Leg 2 — `Tame` fills this herd's meter (Herding is the gate the leg above just opened).
    set_hunt_improvement(&mut app, band, Some(Improvement::Tame));
    let leg2 = turns_until(&mut app, "tame the herd", 120, |app| {
        herd_of(app, &id).is_domesticated()
    });

    // Leg 3 — **the new leg**: practise the PASTORAL rung. The same Sustain hunt now teaches
    // Penning, because the herd stands on a different rung.
    assert!(
        ladder_knowledge(&app, PENNING_DISCOVERY_ID) < LESSON_LEARNED,
        "Penning cannot already be known — taming a WILD herd practises rung 1, not rung 2"
    );
    set_hunt_improvement(&mut app, band, None);
    let leg3 = turns_until(&mut app, "learn Penning", 60, |app| {
        ladder_knowledge(app, PENNING_DISCOVERY_ID) >= LESSON_LEARNED
    });

    // Leg 4 — `Corral`, gated on the Penning the leg above just earned. Re-staffed to the herd's
    // CURRENT keeper crew: it has grown while the legs above ran, and an under-staffed keeper sheds.
    set_hunt_improvement(&mut app, band, Some(Improvement::Corral));
    let keepers = keeper_crew(&app, &id);
    set_hunt_workers(&mut app, band, keepers);
    let leg4 = turns_until(&mut app, "build the pen", 60, |app| {
        herd_of(app, &id).is_corralled()
    });

    let total = leg1 + leg2 + leg3 + leg4;
    println!(
        "wild -> pen climb (Wild Boar): Herding {leg1} + Tame {leg2} + Penning {leg3} + Corral \
         {leg4} = {total} turns"
    );

    // Each knowledge leg is ~20 turns of practice (threshold / progress_per_turn).
    for (leg, turns) in [("Herding", leg1), ("Penning", leg3)] {
        assert!(
            (18..=22).contains(&turns),
            "the {leg} leg should be ~20 turns of practice, got {turns}"
        );
    }
    // **THE CLIMB IS KNOWLEDGE-PACED, and since improvements were priced in work it is emphatically
    // so** (`docs/plan_unit_costed_work.md` §1.2). A build's turns are now `work_cost / crew output`,
    // and an animal build's crew is the herd's own `herders_needed` — a real boar herd wants enough
    // keepers to clear a 62.5-unit Tame in a handful of turns. Under-staffing it to slow the build
    // down is not available: an under-herded flock **sheds**, so the keeper crew is the herd's, not
    // the player's. So the two build legs are short and the two lessons dominate, which is exactly
    // what this test's name claims.
    for (leg, turns) in [("Tame", leg2), ("Corral", leg4)] {
        assert!(turns >= 1, "the {leg} leg must take at least a turn");
        assert!(
            turns < leg1,
            "the {leg} leg ({turns}) is paid in hands, so it is shorter than a lesson ({leg1}) — \
             the climb is paced by PRACTISING each rung, not by building it"
        );
    }
    // The headline: a pen is a ~45-turn commitment on this fixture herd, and better than half of it
    // is the two lessons. Broad band — these are playtest dials.
    assert!(
        (38..=60).contains(&total),
        "the whole climb should run ~45 turns, got {total}"
    );
}

/// **Penning accrues from WORKING a pastoral herd, on EVERY turn — not only on turns an animal is
/// killed** (slice 8b regression — a playtest report of "Penning stuck at 0%").
///
/// The kill-credit model (slice 8b) makes a Sustain hunt of a big-bodied species a pulse: many
/// **wait-turns** (no kill while the credit bank fills), then a kill. If knowledge earning had been
/// tied to the kill, learning would stall for big game. It is not — the earn path in
/// `advance_labor_allocation`'s Hunt arm resolves the herd's rung and credits its `earns_knowledge`
/// **before** the take branches, gated on the *policy* (stewardship) and the herd being *Thriving*,
/// never on a kill. This pins that: an **Aurochs** (`body_mass` 80 — Sustain waits several turns per
/// kill) pastoral herd Sustain-hunted accrues Penning to completion in ~20 turns, and at least one of
/// those was a 0-kill wait-turn (so the assertion genuinely exercises the decoupling).
#[test]
fn penning_accrues_every_worked_turn_not_only_on_kill_turns() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    rebadge_as(&mut app, &id, "aurochs"); // pen-ceiling, heavy body 80 ⇒ Sustain wait-turns
    domesticate(&mut app, &id); // a completed PASTORAL herd (domesticated, not corralled)
    let band = spawn_hunter(&mut app, &id, 0.5);
    let _ = band;

    assert_eq!(
        herd_of(&app, &id).ecology_phase,
        core_sim::EcologyPhase::Thriving,
        "the fixture must be a Thriving pastoral herd, the earning scenario"
    );

    let mut wait_turns = 0u32;
    let mut turns = 0u32;
    while ladder_knowledge(&app, PENNING_DISCOVERY_ID) < LESSON_LEARNED {
        let before = herd_of(&app, &id).biomass;
        run_turns_with_hunt(&mut app, 1);
        // A wait-turn: the herd's biomass did not fall (no whole animal was spared/killed this turn).
        if herd_of(&app, &id).biomass >= before - 1e-3 {
            wait_turns += 1;
        }
        turns += 1;
        assert!(turns <= 30, "Penning must accrue to completion, not stall");
    }
    assert!(
        (18..=22).contains(&turns),
        "Penning completes in ~20 turns of working the pastoral herd, got {turns}"
    );
    assert!(
        wait_turns > 0,
        "the fixture must include a 0-kill wait-turn, or it does not exercise the kill-decoupling \
         (Penning still reached completion across {turns} turns)"
    );
}

// ==========================================================================================
// Neglect-escape arc (`docs/plan_fauna_neglect_escape.md`): neglect sheds ANIMALS, it does not
// un-tame them. Below the fixture helpers isolate the shed from regrowth/harvest by running only
// `advance_husbandry` (the Logistics pass that sheds) with a manually-seated `herded_fraction`.
// ==========================================================================================

/// The `animals_per_herder` for the fixture species, and a herd's `body_mass` — the two numbers the
/// shed's overage/whole-animal math is denominated in.
fn species_shed_params(app: &App, id: &str) -> (f32, f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).unwrap();
    (fauna.animals_per_herder_for(&herd.species), herd.body_mass)
}

/// **Seat the keeping exactly as `herders` hands would supply it** — `herders` worker-turns, which is
/// what the labor arm stamps for a `maintain` crew of that size. The shed's capacity is then
/// `herders × animals_per_herder` by construction, so it tracks the herd as it shrinks (the §2.2
/// convergence, without running the labor system).
///
/// **It seats WORK, not a fraction**, which is the whole of what slice 4 changed: a keeper is worth a
/// worker-turn wherever they stand, so a fixture no longer has to reconstruct `needed` to express
/// "two hands are on this herd" — and cannot disagree with the shed about what `needed` was.
fn seat_staffing(app: &mut App, id: &str, herders: f32, _aph: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.upkeep_supplied = herders * core_sim::PER_WORKER_OUTPUT;
}

/// **MEASUREMENT HARNESS — what a band pays to HOLD a herd, per species.** Not a guard; run with
/// `--ignored --nocapture`. These are the numbers the animal web's keeper burden should be judged on
/// (`docs/plan_standing_upkeep.md` §2.4).
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_price_of_holding_a_herd() {
    /// Herd sizes to quote each species at, in **ANIMALS** — deliberately the same head counts for
    /// every species, because the whole point of `animals_per_herder` is that the same flock costs
    /// different hands depending on what is in it.
    const HEADS: [f32; 3] = [20.0, 100.0, 500.0];
    let app = spawn_world();
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();

    println!("\nWHAT IT COSTS TO HOLD A HERD, per turn, forever:");
    println!(
        "  rung rates: pastoral {:.2} work/keeper-load (grace {}), pen {:.2} (grace {})",
        ladder
            .rung(RungKey::AnimalPastoral)
            .upkeep_demand(core_sim::ONE_KEEPER_LOAD),
        ladder.rung(RungKey::AnimalPastoral).upkeep_grace_turns(),
        ladder
            .rung(RungKey::AnimalPen)
            .upkeep_demand(core_sim::ONE_KEEPER_LOAD),
        ladder.rung(RungKey::AnimalPen).upkeep_grace_turns(),
    );
    println!(
        "\n  {:<18} {:>9}  {:>26}",
        "species", "an/keeper", "keepers at 20 / 100 / 500 head"
    );
    let mut rows: Vec<(String, f32, Vec<u32>)> = Vec::new();
    for species in fauna.species.values() {
        if !matches!(
            species.husbandry_ceiling,
            core_sim::HusbandryCeiling::Pastoral | core_sim::HusbandryCeiling::Pen
        ) {
            continue;
        }
        let aph = fauna.animals_per_herder_for(&species.display_name);
        let mut counts = Vec::new();
        for heads in HEADS {
            let mut herd = herd_of(&app, &prime_thriving_herd(&mut spawn_world()));
            herd.species = species.display_name.clone();
            herd.body_mass = species.body_mass;
            herd.biomass = heads * species.body_mass;
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            herd.herders_needed = 0;
            counts.push(core_sim::herd_herders_needed(&herd, &fauna, &ladder));
        }
        rows.push((species.display_name.clone(), aph, counts));
    }
    rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    for (name, aph, counts) in rows {
        println!(
            "  {name:<18} {aph:>9.0}  {:>10} {:>8} {:>8}",
            counts[0], counts[1], counts[2]
        );
    }
    println!(
        "\n  A keeper is owed EVERY TURN, on wait turns too, and the count moves with the herd: \
         grow the flock past what your hands hold and the overage drifts off."
    );
}

/// **THE SHIPPED ROSTER ASKS FOR EXACTLY THE KEEPERS IT ALWAYS ASKED FOR** — the pacing-neutrality
/// claim of `docs/plan_standing_upkeep.md` §2.4's animal half, asserted per species.
///
/// The keeper demand stopped being a declared **head count** and became **work per turn**: the rung
/// declares `work_per_turn: 1.0, scaled_by: source_load` and the species supplies the load
/// (`head count / animals_per_herder`). Since one worker-turn is `PER_WORKER_OUTPUT`, `ceil(demand)`
/// is the same `ceil((biomass/body_mass)/animals_per_herder)` the retired helper computed — so a
/// shepherd still minds 200 fowl and a cowherd still minds 12 aurochs.
///
/// **Asserted against the retired arithmetic restated here, not against a table of literals**, so a
/// roster retune moves the expectation with the game and only a change to the *model* can fail it.
#[test]
fn every_species_asks_for_the_keepers_it_asked_for_before() {
    /// The demand `fauna::herders_needed` computed before it became an upkeep — the one thing this
    /// test may not read off the sim, because reproducing it is the whole assertion.
    fn retired_herders_needed(biomass: f32, body_mass: f32, animals_per_herder: f32) -> u32 {
        let animals = biomass / body_mass;
        ((animals / animals_per_herder).ceil() as u32).max(1)
    }

    let app = spawn_world();
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let mut checked = 0;
    for species in fauna.species.values() {
        // A `wild`-ceiling species is never herded, so it declares no `animals_per_herder` and has
        // no keeper demand to be neutral about.
        if !matches!(
            species.husbandry_ceiling,
            core_sim::HusbandryCeiling::Pastoral | core_sim::HusbandryCeiling::Pen
        ) {
            continue;
        }
        let aph = fauna.animals_per_herder_for(&species.display_name);
        let body_mass = species.body_mass;
        // Sweep head counts across several `animals_per_herder` boundaries — the `ceil` is where an
        // inversion like this goes wrong, so the boundaries are the point.
        for heads in [1.0_f32, aph - 0.5, aph, aph + 0.5, aph * 3.0, aph * 7.25] {
            let mut herd = herd_of(&app, &prime_thriving_herd(&mut spawn_world()));
            herd.species = species.display_name.clone();
            herd.body_mass = body_mass;
            herd.biomass = heads * body_mass;
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            herd.herders_needed = 0; // unstabilized, so the raw reading is what answers
            assert_eq!(
                core_sim::herd_herders_needed(&herd, &fauna, &ladder),
                retired_herders_needed(herd.biomass, body_mass, aph),
                "{} at {heads} head (animals_per_herder {aph}): the keeper count must not have \
                 moved when it became a work rate",
                species.display_name
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 30,
        "fixture: the sweep must actually cover the herdable roster, got {checked} readings"
    );
}

/// **THE SHED IS CONTINUOUS IN THE SHORTFALL** (`docs/plan_standing_upkeep.md` §2.4) — half the
/// keepers a herd wants sheds half as many animals, where the retired gate asked only *whether*
/// `herded_fraction < FULLY_HERDED` and so said the same thing about one keeper short and all of
/// them.
///
/// Measured as a **ratio across three staffings** on the same herd, so the per-rung escape fraction
/// and its jitter cancel and what is left is the claim about the shortfall.
#[test]
fn the_shed_is_continuous_in_the_keeping_shortfall() {
    let lost_at = |fraction: f32| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            herd.biomass = herd.carrying_capacity;
        }
        let before = herd_of(&app, &id).biomass;
        let grace = neglect_grace(&app, &id);
        run_understaffed_turns(&mut app, &id, fraction, grace + 1);
        before - herd_of(&app, &id).biomass
    };

    let unkept = lost_at(NOT_HERDED_FIXTURE);
    let half_kept = lost_at(0.5);
    let fully_kept = lost_at(FULLY_HERDED_FIXTURE);

    assert!(unkept > 0.0, "fixture: an unkept herd must actually shed");
    // **⛔ PROPORTIONAL PER TURN, NOT CUMULATIVELY — and that is the acceleration, not a drift.**
    //
    // The *overage* is still exactly `shortfall × head count`, so on any single turn half the keepers
    // leave half the flock uncontained. What is no longer linear across several turns is the **rate**
    // it sheds at: `Herd::neglect_pressure` rises by the shortfall fraction, so a wholly unkept herd
    // frays at `1.0` a turn and a half-kept one at `0.5`, and the wholly unkept herd's escape rate
    // therefore compounds **faster**. Over `grace + 1` turns that makes the unkept loss more than
    // twice the half-kept one, which is the design: *the longer you don't tend it, the quicker the
    // remainder goes*, and half-tending is a slower fraying as well as a smaller overage.
    //
    // So the claim asserted here is the **ordering with a floor**: half-staffing must leave
    // materially less than half the loss of no staffing at all — never more, and not a rounding.
    assert!(
        half_kept < unkept * 0.5 + 1e-3,
        "half the keepers must leave AT MOST half the flock uncontained: {unkept} unkept vs \
         {half_kept} half-kept"
    );
    assert!(
        half_kept > unkept * 0.25,
        "…and materially more than nothing, or the two arms are not on the same curve: {unkept} \
         unkept vs {half_kept} half-kept"
    );
    assert_eq!(
        fully_kept, 0.0,
        "and a herd whose keeping is met loses nothing"
    );
}

/// **A `Tame` IN FLIGHT IS HELD BY THE BAND'S POOL, EXACTLY AS A TAMED HERD IS**
/// (`docs/plan_standing_upkeep.md` §4.6a, `fauna::herd_upkeep_supply`). The meter's fullness used to
/// decide who supplied the rate — the build crew below its cost, the pool at it — and that test is
/// deleted: the keeping owes the rate from the first work banked, and a build crew supplies nothing.
///
/// And **the verb names the meter**: a `Corral` starting on a herd with no pen progress answers for
/// `animal:pen` from its very first turn, because the supply is stamped in Population and read by the
/// *next* Logistics pass — it has to describe the meter that pass will judge.
#[test]
fn the_keeping_pool_holds_a_half_tamed_herd_and_a_tamed_one_alike() {
    const A_KEEPERS_TURN: f32 = 1.0;
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);

    // --- Mid-Tame: owned, partly gentled, billed to the pool. ----------------------------------
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.owner = Some(FactionId(0));
        herd.set_ladder_position(25.0, &core_sim::LadderConfig::builtin());
    }
    let herd = herd_of(&app, &id);
    assert_eq!(
        core_sim::herd_upkeep_supply(&herd, Some(Improvement::Tame), A_KEEPERS_TURN),
        A_KEEPERS_TURN,
        "a meter being raised is held by the pool like any other"
    );
    assert_eq!(
        core_sim::herd_upkeep_supply(&herd, None, A_KEEPERS_TURN),
        A_KEEPERS_TURN,
        "AND A HALF-TAMED HERD NOBODY IS TAMING CAN BE HELD — the builders leave, the keepers stay"
    );
    // **A `Corral` answers for the PEN from its first turn**, before that meter has any progress.
    assert_eq!(
        core_sim::herd_upkeep_supply(&herd, Some(Improvement::Corral), A_KEEPERS_TURN),
        A_KEEPERS_TURN,
        "the verb names the meter, so a Corral's first turn is credited to the pen it starts"
    );

    // --- Tamed: the same supplier, whatever verb is still hanging off the assignment. ----------
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    }
    let herd = herd_of(&app, &id);
    assert_eq!(
        core_sim::herd_upkeep_supply(&herd, Some(Improvement::Tame), A_KEEPERS_TURN),
        A_KEEPERS_TURN,
        "a herd you have tamed is held by the same pool"
    );
    // And the maintain activity's published count follows the same rate.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    assert!(core_sim::herd_upkeep_demand(&herd, &fauna, &ladder) > 0.0);
    // **AND NOTHING EATS AN ANIMAL BUILD** — neither animal rung declares a `meter_decay`, so a
    // wholly unkept herd's meter rot is `0` and the shed is what its shortfall costs.
    let unkept = herd_of(&app, &id);
    assert_eq!(
        core_sim::herd_meter_rot(&unkept, &fauna, &ladder),
        0.0,
        "the animal web pays a shortfall in animals, never in meter"
    );
}

/// **ON THE TURN A `Tame` BANKS ITS FIRST WORK, THE HUSBANDRY POOL IS ALREADY PAYING FOR IT** — the
/// animal twin of `forage_cultivation::a_builds_first_turn_draws_the_keeping_pool_and_bare_ground_draws_nothing`,
/// and the same defect.
///
/// # The animal web had the identical hole, for its own reason
///
/// `Herd::accrue_domestication` records `owner` on the **first accrual**, which happens inside the
/// assignment loop — *after* `systems::labor::maintenance_shares` has already split the band's
/// keeping pool. The claim gate read `herd_keeping_rung`, which answers `None` for a herd nobody
/// owns yet, so on that one turn a wild herd being tamed claimed nothing, drew a share of `0`, and
/// the stamp paid that zero through `herd_upkeep_supply` — whose own resolver knows a `Tame` names
/// `animal:pastoral` from its first turn. Capture then read the herd *owned*, and published the
/// whole demand as a shortfall on a staffed `husbandry` role.
///
/// Both seams read `fauna::herd_keeping_meter` now.
///
/// **The pair is the test**: a wild herd nobody is taming must still claim nothing, or the gate has
/// become *"always"* and would bill a band for every animal it hunts.
#[test]
fn a_tames_first_turn_draws_the_husbandry_pool_and_a_wild_hunt_draws_nothing() {
    /// What one turn left on the herd.
    struct Kept {
        supplied: f32,
        demand: f32,
        progress: f32,
    }

    let read = |app: &App, id: &str| -> Kept {
        let herd = herd_of(app, id);
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        Kept {
            supplied: herd.upkeep_supplied,
            // **Read as the capture reads it** — `herd_keeping_basis`, the bill the keepers were
            // HANDED when the pool was split, not the live demand the same turn's accrual has since
            // raised. The two came apart when the animal demand began interpolating on the position.
            demand: core_sim::herd_keeping_basis(&herd, &fauna, &ladder),
            progress: herd.rung_work_done(
                core_sim::RungKey::AnimalPastoral,
                &core_sim::LadderConfig::builtin(),
            ),
        }
    };

    // (a) **A `Tame` on its very first turn.** The herd is wild and unowned when the shares split.
    let taming = {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        assert!(
            herd_of(&app, &id).owner.is_none(),
            "fixture: the herd must be unowned before the turn, or the ownership term carries the \
             claim and the verb term is never exercised"
        );
        grant_herding(&mut app);
        spawn_builder(&mut app, &id, Improvement::Tame);
        // **A whole turn in stage order**, because the fixture seats the herd exactly *at* the food
        // peak: the escapement room a build's `eligible` reads is `0` there until Logistics regrows
        // it, so a bare labor pass would measure a Tame that never started.
        run_turns_with_hunt(&mut app, 1);
        read(&app, &id)
    };

    // (b) **The same band hunting the same wild herd with nothing declared** — the other half.
    let hunting = {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        let keepers = keeper_crew(&app, &id);
        spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, keepers);
        run_turns_with_hunt(&mut app, 1);
        read(&app, &id)
    };

    assert!(
        taming.progress > 0.0,
        "fixture: the Tame must bank work on this very turn, or nothing about the ordering is \
         under test"
    );
    // **THE CLAIM MUST NOT BE SHORT OF THE BILL IT WAS HANDED**, which is the invariant this arm has
    // always been about — and it is now stated against the bill rather than against a bare `> 0`.
    //
    // The demand **interpolates on the herd's position** since the animal web got its one-position
    // ladder, so on the turn a Tame banks its *first* work the pool is split against a position of
    // zero and a supply of zero is the correct answer, not a missed claim. What would still be the
    // defect is a **staffed** role publishing less than the bill the same turn stamped
    // (`Herd::upkeep_demanded`), which is what the ordering guarantees.
    assert!(
        taming.supplied + 1e-4 >= taming.demand,
        "a staffed husbandry role must cover the bill its keepers were handed — supplied {} \
         against a stamped demand of {}",
        taming.supplied,
        taming.demand
    );
    // **AND THE TAME REALLY DID BANK**, so the arm above is about a herd the pool was split for
    // rather than one nothing happened to. What it does **not** assert is a positive bill on turn
    // one: the demand interpolates on the position, and the position is `0` when the shares are
    // struck, so *"owes nothing yet"* is the honest reading of a Tame's first turn — and it is the
    // whole of the front-loading fix.
    assert!(
        taming.progress > 0.0,
        "fixture: the Tame must have banked work, or the claim it is about never happened"
    );

    // **The pair.** A wild herd is nobody's to keep and nothing is being started on it, so the pool
    // must put nothing on it — a gate that answered *"always"* fails exactly here.
    assert_eq!(
        hunting.demand, 0.0,
        "a wild herd owes no keeping ({})",
        hunting.demand
    );
    assert_eq!(
        hunting.supplied, 0.0,
        "…so a keeping pool must put nothing on it ({})",
        hunting.supplied
    );
}

/// **THE SUPPLY STAMP ACCUMULATES ACROSS THE BANDS WORKING ONE HERD.** The demand is per-**source**,
/// so two bands each put a fraction of it on the ground; assigning would let whichever band the loop
/// visited last speak for all of them, and the herd would shed as if the other crew were not there.
///
/// It has been a `+=` since slice 3, when no animal rung declared an upkeep and it was therefore
/// inert. It stops being inert here, so it is measured rather than assumed.
#[test]
fn two_bands_keeping_one_herd_sum_their_hands() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    let demand = keeping_demand(&app, &id);
    assert!(
        demand > 1.0,
        "fixture: the herd must want more than one hand"
    );

    // Two bands, each with a single keeper on the same herd, and nobody hunting.
    for _ in 0..2 {
        let band = spawn_crew_of(&mut app, &id, MSY_BIOMASS_FRACTION, None, HUNT_WORKERS);
        set_maintain_workers(&mut app, band, 1);
    }
    // Clear whatever the fixture seeded, so the only supply is what the two bands stamp.
    seat_keeping(&mut app, &id, NOT_HERDED_FIXTURE);
    app.world.run_system_once(advance_labor_allocation);

    // **ONE KEEPER'S SUPPLY IS THE SAME EXPRESSION A BUILDER'S IS** (`docs/plan_standing_upkeep.md`
    // §4.8) — its bare `PER_WORKER_OUTPUT` plus whatever the band's derived `husbandry` kit
    // delivers. Read off the roster rather than stated as a literal, so retuning the hurdles moves
    // the fixture with the game.
    let equipment = core_sim::EquipmentConfig::builtin();
    let one_keeper = core_sim::pool_work_supply(
        1,
        equipment
            .keeping_kit_for_branch(core_sim::RungBranch::Animal, None)
            .map(|kit| {
                equipment.build_work_per_worker(
                    &kit,
                    &core_sim::BandEquipment::start_stocked(&equipment),
                    core_sim::RungBranch::Animal,
                    None,
                )
            })
            .expect("the shipped roster serves the animal web's keeping"),
    );
    // **THE NO-OP GUARD.** A derivation that answered `none` would leave every keeper bare and this
    // whole seam would change nothing while every other assertion still passed.
    assert!(
        one_keeper > core_sim::PER_WORKER_OUTPUT,
        "fixture: a start-stocked band's derived keeping kit must actually deliver something — a \
         bare {one_keeper} means the derivation resolved `none`"
    );
    assert_eq!(
        herd_of(&app, &id).upkeep_supplied,
        2.0 * one_keeper,
        "both bands' keepers are on the herd, so both are counted"
    );
}

/// Put `workers` on a band's **husbandry role** — the fixture's stand-in for
/// `assign_labor <faction> <band> husbandry <workers>`.
fn set_maintain_workers(app: &mut App, band: bevy::prelude::Entity, workers: u32) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists");
    // **SET, not add** — the command it stands in for states a number, and the fixture band already
    // carries a keeping role from `spawn_crew_of`. `set_assignment` is handed exactly the headroom
    // this row needs (what every other row holds, plus these keepers), because a fixture stating a
    // role outright is not testing the refusal a real command can make.
    let headroom = allocation.assigned_total() + workers;
    allocation.set_assignment(LaborTarget::Husbandry, workers, headroom, None);
}

/// **Append the band-wide `builders` pool to a fixture's rows** — the hands a declared build is
/// raised by since the build crew left the tile (`docs/plan_standing_upkeep.md` §2.5). A pool of
/// zero adds no row, which is the honest reading of *"this band is building nothing"*.
///
/// ⛔ **THE POOL GOES OUT BARE, and that is an isolation rather than a default.** An absent kit means
/// *derive per entry*, and the roster's answer for a herd — `hurdling` — adds `+0.5` work per
/// covered worker per turn. A start-stocked band holds a unit per worker and a half, so at the crews
/// these fixtures staff every builder is geared and the pool delivers half again what these arms
/// assert. Naming `none` holds the gear axis at its identity so these arms
/// measure the **crew**, exactly as `FaunaConfig::without_retreat` holds the retreat at its identity
/// across the hunt suites; the geared default is pinned in
/// `core_sim/tests/build_turns_closed_form.rs` and in `equipment_config`'s own unit tests.
fn with_builders_pool(mut rows: Vec<LaborAssignment>, builders: u32) -> Vec<LaborAssignment> {
    if builders > 0 {
        rows.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: builders,
            kit: Some(
                core_sim::EquipmentConfig::builtin()
                    .kit("none")
                    .expect("the shipped roster carries the empty kit"),
            ),
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    rows
}

/// **Append the band-wide `husbandry` role to a fixture's rows** — see [`with_builders_pool`] for
/// the building half.
fn with_keeping_role(mut rows: Vec<LaborAssignment>, keepers: u32) -> Vec<LaborAssignment> {
    if keepers > 0 {
        rows.push(LaborAssignment {
            target: LaborTarget::Husbandry,
            workers: keepers,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    rows
}

/// **The shed self-limits: an over-stocked managed herd converges to its labor capacity from above and
/// STOPS there** (`docs/plan_fauna_neglect_escape.md` §2.2). It sheds a fraction of the *overage*, not
/// the total, so as the herd shrinks toward what its keepers can hold, fewer leave each turn and it
/// halts at capacity — it never overshoots to zero (only total abandonment does that).
#[test]
fn an_over_stocked_managed_herd_converges_to_its_labor_capacity() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);

    // A fixed keeper crew of `herders` hands ⇒ a labor capacity of `herders × aph` animals. Over-stock
    // the herd well above it, owned + domesticated (so it is managed and sheds).
    let herders = 3.0_f32;
    let capacity_animals = herders * aph;
    let start_animals = capacity_animals * 4.0;
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.carrying_capacity = start_animals * body_mass * 2.0; // K well above the stock
        herd.biomass = start_animals * body_mass;
    }

    // Only the shed runs (no regrowth), with staffing re-seated each turn to track the shrinking herd.
    for _ in 0..200 {
        seat_staffing(&mut app, &id, herders, aph);
        app.world.run_system_once(advance_husbandry);
    }

    let herd = herd_of(&app, &id);
    let final_animals = herd.biomass / body_mass;
    assert!(
        final_animals > capacity_animals * 0.5,
        "it converged to its labor capacity, it did NOT overshoot to zero: {final_animals} animals \
         vs capacity {capacity_animals}"
    );
    assert!(
        final_animals <= capacity_animals + 1.0,
        "and it did not stall well above capacity: {final_animals} animals vs capacity \
         {capacity_animals}"
    );
    // PARTIAL neglect keeps the herd TAME and OWNED — it settled smaller, it did not go feral.
    assert!(
        herd.owner.is_some(),
        "a partially-herded herd stays the owner's — only shed-to-zero clears ownership"
    );
    assert!(
        herd.is_domesticated(),
        "and its tameness is untouched by the shedding: {}",
        herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
}

/// **The shed is bounded by the TRUE overage near a `ceil` boundary** (PR #329 review fix). A managed
/// herd sitting just over a `herders_needed = ceil(animals/aph)` boundary must shed only the animals it
/// is genuinely over its labor capacity (~1), NOT the `(1 − herded_fraction) × current_animals`
/// over-estimate the original spec used — which reads dozens over at a hard `ceil` round-up (101 @ aph
/// 50 staffed at 2: true overage **1**, shorthand **33.7**) and overshoots the herd below its real
/// capacity.
#[test]
fn the_shed_is_bounded_by_the_true_overage_near_a_ceil_boundary() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);

    // `current = 2·aph + 2` ⇒ needed = ceil = 3; staffed at 2 herders ⇒ herded_fraction = 2/3, real
    // capacity = 2·aph, true overage = **2 animals**. The old shorthand would read
    // `(1 − 2/3) × (2·aph + 2)` ≈ `0.667·aph` animals over — dozens for the fixture's aph.
    let assigned = 2.0_f32;
    let current_animals = 2.0 * aph + 2.0;
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.carrying_capacity = current_animals * body_mass * 4.0;
        herd.biomass = current_animals * body_mass;
    }
    // Seat the work `assigned` keepers supply — the shed's capacity is `assigned × aph` directly.
    seat_staffing(&mut app, &id, assigned, aph);
    // Spend the rung's neglect grace first — nothing sheds inside it — then measure the one pass
    // that actually bites, so this still reads the *first* shed's size.
    let staffed_fraction = assigned / keeping_demand(&app, &id);
    let grace = neglect_grace(&app, &id);
    run_understaffed_turns(&mut app, &id, staffed_fraction, grace);
    let start_biomass = herd_of(&app, &id).biomass;
    assert_eq!(
        start_biomass,
        current_animals * body_mass,
        "the grace turns cost the herd nothing"
    );

    run_understaffed_turns(&mut app, &id, staffed_fraction, 1);

    // **Rounded to whole animals, because that is what is being asserted.** The count is recovered by
    // dividing a ~1-animal delta by a ~400-animal total, and f32 carries ~7 digits — so the exact
    // quotient lands within ~1e-5 of the integer either side of it, and a hair-tight bound passes or
    // fails on the fixture's magnitude rather than on the shed's behaviour.
    let shed_animals = ((start_biomass - herd_of(&app, &id).biomass) / body_mass).round();
    assert!(
        (1.0..=3.0).contains(&shed_animals),
        "the shed is the true overage (~1–2 animals), not the ceil-boundary over-estimate (~dozens): \
         shed {shed_animals} animals"
    );
}

/// **Partial neglect with REGROWTH active keeps a stable smaller TAME herd** (`docs/plan_fauna_neglect_escape.md`
/// §2.4 option B) — the counterpart to `a_fully_abandoned_pastoral_herd_goes_feral...`. An understaffed
/// (but not abandoned) herd runs the full turn loop — `advance_herds` regrows it, `advance_husbandry`
/// sheds its overage — and settles into a stable herd **below** its ecological `K`, with `owner` intact
/// and `domestication_progress` unchanged. Regrowth is suppressed ONLY at zero herders; with some
/// herders it refills, so the herd never sheds to the floor and never goes feral.
#[test]
fn a_partially_herded_pastoral_herd_stays_tame_with_regrowth() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);
    let herders = 3.0_f32;
    let capacity_animals = herders * aph;
    let cap = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        // Ecological K modestly above the labor capacity, over-stocked to full K.
        let cap = capacity_animals * body_mass * 1.5;
        herd.carrying_capacity = cap;
        herd.biomass = cap;
        cap
    };
    let floor = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        herd_ecology(&herd_of(&app, &id), &fauna).extinction_floor * cap
    };

    // Full turn loop with a fixed, understaffed keeper crew re-seated each turn.
    for _ in 0..80 {
        seat_staffing(&mut app, &id, herders, aph);
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
    let mid = herd_of(&app, &id).biomass;
    for _ in 0..40 {
        seat_staffing(&mut app, &id, herders, aph);
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
    let herd = herd_of(&app, &id);

    assert!(
        herd.owner.is_some(),
        "an understaffed herd stays TAME and owned — it never sheds to zero"
    );
    assert!(
        herd.is_domesticated(),
        "and its tameness is untouched: {}",
        herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
    assert!(
        herd.biomass > floor * 4.0,
        "it is a healthy smaller herd, not a feral remnant at the floor: {} vs floor {floor}",
        herd.biomass
    );
    assert!(
        herd.biomass < cap,
        "the shed bound it BELOW its ecological K (labor-limited): {} vs K {cap}",
        herd.biomass
    );
    assert!(
        (herd.biomass - mid).abs() < cap * 0.10,
        "and it settled to a STABLE size (turn 80 {mid} ≈ turn 120 {})",
        herd.biomass
    );
}

/// **Tameness does not change under neglect — it leaves with the animals** (`docs/plan_fauna_neglect_escape.md`
/// §2.1/§2.4) — the replacement for the retired `an_under_herded_tamed_herd_decays_proportionally_and_recovers`.
/// `domestication_progress` is monotone-up (earned via `Tame`), **never** bled by an unherded turn: a
/// fully-abandoned herd sheds its ANIMALS to the wild web and, when it can shed no more, **despawns** —
/// the tameness meter reads exactly what was earned right up to the moment the entity is gone. There is
/// no husk with a decayed (or reset) meter.
#[test]
fn neglect_never_un_tames_a_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    // A PARTIALLY-tamed, owned, fully-abandoned herd — a partial meter is the sensitive case (a full
    // 1.0 could hide a small decay under the clamp).
    let partial = 0.5_f32;
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.owner = Some(FactionId(0));
        herd.set_ladder_position(partial, &core_sim::LadderConfig::builtin());
    }
    seat_keeping(&mut app, &id, NOT_HERDED_FIXTURE); // nobody herding — maximum shed pressure

    // Pure shed (no regrowth): the meter must not move a bit on any turn the herd still exists, and the
    // herd must eventually bleed out and despawn.
    let mut despawned = false;
    for _ in 0..200 {
        app.world.run_system_once(advance_husbandry);
        match app.world.resource::<HerdRegistry>().find(&id) {
            Some(herd) => assert_eq!(
                herd.rung_work_done(
                    core_sim::RungKey::AnimalPastoral,
                    &core_sim::LadderConfig::builtin()
                ),
                partial,
                "neglect sheds animals, it never touches tameness: {}",
                herd.rung_work_done(
                    core_sim::RungKey::AnimalPastoral,
                    &core_sim::LadderConfig::builtin()
                )
            ),
            None => {
                despawned = true;
                break;
            }
        }
    }
    assert!(
        despawned,
        "a fully-abandoned herd bleeds out entirely and despawns — no ownerless husk survives"
    );
}

/// **A pen sheds SLOWER than a pastoral herd at the same shortfall** (`docs/plan_fauna_neglect_escape.md`
/// §2.2) — the fence buys time (`pen_escape_fraction` < `pastoral_escape_fraction`, validated).
#[test]
fn a_pen_sheds_slower_than_a_pastoral_herd() {
    // Two independent worlds, same fixture, same full-shortfall neglect — one pastoral, one penned.
    let shed_biomass = |corral: bool| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            herd.biomass = herd.carrying_capacity;
        }
        if corral {
            let tile = herd_of(&app, &id).position();
            {
                let mut registry = app.world.resource_mut::<HerdRegistry>();
                let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
                assert!(herd.corral_at(tile, &core_sim::LadderConfig::builtin()));
                // Skip the one-turn penning grace so both start shedding on turn 1.
                herd.corralled_tended_this_turn = false;
            }
        }
        seat_keeping(&mut app, &id, NOT_HERDED_FIXTURE);
        // Pure shed, no regrowth, several turns so the per-rung rate dominates the ±jitter.
        for _ in 0..6 {
            app.world.run_system_once(advance_husbandry);
        }
        herd_of(&app, &id).biomass
    };

    let pastoral_left = shed_biomass(false);
    let pen_left = shed_biomass(true);
    assert!(
        pen_left > pastoral_left,
        "the pen (slower shed) retains MORE than the open-range herd: pen {pen_left} vs pastoral \
         {pastoral_left}"
    );
}

/// **Total abandonment bleeds the whole flock into the wild web over turns, loses the pen, and despawns
/// the empty herd** (`docs/plan_fauna_neglect_escape.md` §2.4): no separate "escape" branch, just the
/// `herded_fraction == 0` limit of the same shed carried all the way to zero animals. The flock is
/// preserved in the wild web; only the emptied managed entity is removed.
#[test]
fn total_abandonment_sheds_the_flock_and_loses_the_pen() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let species = herd_of(&app, &id).species.clone();
    let wild_before: f32 = wild_herds_of(&app, &species)
        .iter()
        .map(|h| h.biomass)
        .sum();
    corral_herd(&mut app, &id);

    assert!(
        run_untended_until_pen_lost(&mut app, 200).is_some(),
        "an untended pen eventually bleeds out and loses the pen"
    );

    // The empty managed entity is gone — no ownerless husk, no pen.
    assert!(
        app.world.resource::<HerdRegistry>().find(&id).is_none(),
        "the bled-out herd is despawned, pen and all"
    );
    // The flock did not vanish — it went to the wild web (at domestication 0).
    let wild_after: f32 = wild_herds_of(&app, &species)
        .iter()
        .map(|h| h.biomass)
        .sum();
    assert!(
        wild_after > wild_before,
        "the shed flock re-entered the wild web: wild biomass {wild_before} -> {wild_after}"
    );
}

/// **Shed animals appear in the wild web** (`docs/plan_fauna_neglect_escape.md` §2.3) — a same-species
/// wild herd on/adjacent gains the biomass (merge), or a fresh wild herd spawns adjacent
/// (`owner = None`, `domestication_progress = 0`). Either way the escapees are re-huntable, not vaporized.
#[test]
fn shed_animals_appear_in_the_wild_web() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let species = herd_of(&app, &id).species.clone();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.biomass = herd.carrying_capacity;
    }
    // Fully under-contained ⇒ a real shed, once the grace is spent.
    seat_keeping(&mut app, &id, NOT_HERDED_FIXTURE);
    let wild_before: f32 = wild_herds_of(&app, &species)
        .iter()
        .map(|h| h.biomass)
        .sum();

    // The grace first (nothing leaves inside it), then the one pass that sheds — so the ONLY change
    // to wild biomass is the placed escapees.
    let grace = neglect_grace(&app, &id);
    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, grace + 1);

    let wild = wild_herds_of(&app, &species);
    let wild_after: f32 = wild.iter().map(|h| h.biomass).sum();
    assert!(
        wild_after > wild_before,
        "the escapees landed in the wild web (merge or spawn): {wild_before} -> {wild_after}"
    );
    for h in &wild {
        assert!(h.owner.is_none(), "a wild carrier is unowned");
        assert_eq!(
            h.rung_work_done(
                core_sim::RungKey::AnimalPastoral,
                &core_sim::LadderConfig::builtin()
            ),
            0.0,
            "and undomesticated"
        );
    }
}

/// **The shed is deterministic under rollback** (`docs/plan_fauna_neglect_escape.md` §3.1): the jitter
/// draws from the world seed stream, so two runs of the identical scenario are bit-identical.
#[test]
fn the_shed_is_deterministic() {
    let run = || -> Vec<(String, i64, u32, u32)> {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        corral_herd(&mut app, &id);
        run_turns_untended(&mut app, 30);
        herd_fingerprint(&app)
    };
    assert_eq!(
        run(),
        run(),
        "two runs of the same shed scenario must be bit-identical (seeded RNG, no wall-clock rand)"
    );
}

// ==========================================================================================
// Neglect-escape SLICE 2 (`docs/plan_fauna_neglect_escape.md` §4 item 1): the edge-gated
// under-herded command-feed notice. Fires the turn a managed herd BECOMES under-contained
// (too few herders to hold all its animals, so it sheds), once per transition, re-arming after
// recovery. Persisted `Herd::under_herded` gates it so a rollback does not spuriously re-fire.
// ==========================================================================================

/// The `herd_under_herded` command-feed entries.
fn under_herded_lines(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| matches!(e.kind, CommandEventKind::HerdUnderHerded))
        .cloned()
        .collect()
}

/// Seat an owned, over-stocked pastoral herd well above a `herders`-hand labor capacity, so it sheds
/// (is under-contained) under partial staffing — and its ecological K is high enough that it does not
/// bleed out over the test's turns.
fn seat_over_stocked_managed(app: &mut App, id: &str, herders: f32, aph: f32, body_mass: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    let capacity_animals = herders * aph;
    herd.carrying_capacity = capacity_animals * body_mass * 8.0;
    herd.biomass = capacity_animals * body_mass * 4.0; // 4× the labor capacity
}

fn set_herded_fraction(app: &mut App, id: &str, fraction: f32) {
    seat_keeping(app, id, fraction);
}

fn set_under_herded(app: &mut App, id: &str, value: bool) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .unwrap()
        .under_herded = value;
}

/// **The under-herded notice fires ONCE on the transition, not every turn it stays under-contained**
/// (`docs/plan_fauna_neglect_escape.md` §4 item 1). The line names the species and carries the
/// machine-readable shortfall in its detail.
#[test]
fn the_under_herded_notice_fires_once_on_becoming_under_contained() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);
    let herders = 3.0;
    seat_over_stocked_managed(&mut app, &id, herders, aph, body_mass);
    let species = herd_of(&app, &id).species.clone();

    // Turn 1: understaffed ⇒ it sheds ⇒ the notice fires exactly once.
    seat_staffing(&mut app, &id, herders, aph);
    app.world.run_system_once(advance_husbandry);
    let lines = under_herded_lines(&app);
    assert_eq!(
        lines.len(),
        1,
        "exactly one notice on becoming under-contained"
    );
    let entry = &lines[0];
    assert_eq!(entry.faction, FactionId(0), "the owner is told");
    assert!(
        entry.label.contains(&species) && entry.label.contains("too few herders"),
        "the line names the species and the shortfall: {}",
        entry.label
    );
    let detail = entry.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("status=under_herded") && detail.contains(&format!("herd={id}")),
        "the detail carries the machine-readable fields: {detail}"
    );

    // It stays under-contained for several more turns — NO new notices.
    for _ in 0..5 {
        seat_staffing(&mut app, &id, herders, aph);
        app.world.run_system_once(advance_husbandry);
    }
    assert_eq!(
        under_herded_lines(&app).len(),
        1,
        "the edge does not re-fire while the herd stays under-contained"
    );
}

/// **The notice RE-FIRES after a recovery and a relapse** — staff it back to full (the flag clears,
/// nothing fires), then understaff again (a fresh notice).
#[test]
fn the_under_herded_notice_re_fires_after_recovery_then_relapse() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);
    let herders = 3.0;
    seat_over_stocked_managed(&mut app, &id, herders, aph, body_mass);

    // Understaffed ⇒ fire once.
    seat_staffing(&mut app, &id, herders, aph);
    app.world.run_system_once(advance_husbandry);
    assert_eq!(under_herded_lines(&app).len(), 1);

    // Fully staff it ⇒ the flag clears, no new notice.
    for _ in 0..2 {
        set_herded_fraction(&mut app, &id, FULLY_HERDED);
        app.world.run_system_once(advance_husbandry);
    }
    assert_eq!(
        under_herded_lines(&app).len(),
        1,
        "recovery announces nothing"
    );
    assert!(
        !herd_of(&app, &id).under_herded,
        "the edge-gate flag cleared on recovery"
    );

    // Relapse: understaff again ⇒ a NEW notice.
    set_herded_fraction(&mut app, &id, 0.0);
    app.world.run_system_once(advance_husbandry);
    assert_eq!(
        under_herded_lines(&app).len(),
        2,
        "a relapse re-fires the notice"
    );
}

/// **The persisted `under_herded` flag suppresses a spurious re-fire** — the rollback guarantee at the
/// unit level (`docs/plan_fauna_neglect_escape.md` §4 item 1). A rollback restores the flag (proven to
/// round-trip by `snapshot::mod`'s herd-state identity test and `integration_tests/fauna_rollback`); a
/// herd restored with the edge already latched does not re-announce, where a transient (reset) flag —
/// the `pen_starving` treatment — would. That contrast is what makes persisting it load-bearing.
#[test]
fn the_persisted_under_herded_flag_suppresses_a_re_fire() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (aph, body_mass) = species_shed_params(&app, &id);
    let herders = 3.0;
    seat_over_stocked_managed(&mut app, &id, herders, aph, body_mass);

    // Simulate the restored state: the flag is preserved true while the herd is still under-contained.
    set_under_herded(&mut app, &id, true);
    seat_staffing(&mut app, &id, herders, aph);
    app.world.run_system_once(advance_husbandry);
    assert!(
        under_herded_lines(&app).is_empty(),
        "a herd restored with the edge already latched does not re-announce"
    );

    // Contrast — a transient (reset-to-false) flag re-fires on the same under-contained turn.
    set_under_herded(&mut app, &id, false);
    seat_staffing(&mut app, &id, herders, aph);
    app.world.run_system_once(advance_husbandry);
    assert_eq!(
        under_herded_lines(&app).len(),
        1,
        "a reset (transient) edge would re-fire — so persisting it is load-bearing"
    );
}

// ---------------------------------------------------------------------------------------------
// The neglect grace on the animal web
// ---------------------------------------------------------------------------------------------

/// **The shed does not bite on the first under-herded turn any more, and it bites on exactly the
/// turn after the grace** — the boundary, pinned from BOTH sides. The plant twin is
/// `forage_cultivation::the_feral_bleed_starts_exactly_one_turn_past_the_grace`; one trigger, two
/// penalties.
#[test]
fn the_shed_starts_exactly_one_turn_past_the_grace() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.biomass = herd.carrying_capacity;
    }
    let start = herd_of(&app, &id).biomass;
    let grace = neglect_grace(&app, &id);
    assert!(grace > 0, "this test needs a rung that forgives something");

    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, grace);
    assert_eq!(
        herd_of(&app, &id).biomass,
        start,
        "not one animal leaves while the grace holds"
    );
    assert_eq!(
        u32::from(herd_of(&app, &id).neglect_turns),
        grace,
        "the counter has climbed to exactly the grace"
    );

    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, 1);
    assert!(
        herd_of(&app, &id).biomass < start,
        "the first turn past the grace sheds animals into the wild web"
    );
}

/// **The grace is CONSECUTIVE neglect** — a turn in which the keepers can hold the flock forgives it
/// outright, so a crew that lapses and recovers never accumulates its way into a shed.
#[test]
fn holding_a_herd_resets_its_neglect_counter() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.biomass = herd.carrying_capacity;
    }
    let start = herd_of(&app, &id).biomass;
    let grace = neglect_grace(&app, &id);

    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, grace);
    // One fully-staffed turn.
    run_understaffed_turns(&mut app, &id, FULLY_HERDED_FIXTURE, 1);
    assert_eq!(
        herd_of(&app, &id).neglect_turns,
        0,
        "a turn the keepers held the flock forgives the neglect outright"
    );

    // The whole grace is available again from scratch — nothing has left after 2 × grace turns of
    // neglect, because they were never consecutive.
    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, grace);
    assert_eq!(
        herd_of(&app, &id).biomass,
        start,
        "the grace is spent from zero again, not from where it left off"
    );
}

/// **The under-herded NOTICE is not gated on the grace, and that is the point of the grace.** The
/// warning fires on the turn the herd genuinely becomes under-contained — the window in which the
/// player can still send hands and lose nothing. Warning only once the animals were already leaving
/// would spend the grace on silence.
#[test]
fn the_under_herded_notice_fires_inside_the_grace() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        herd.biomass = herd.carrying_capacity;
    }
    let start = herd_of(&app, &id).biomass;

    run_understaffed_turns(&mut app, &id, NOT_HERDED_FIXTURE, 1);

    assert_eq!(
        herd_of(&app, &id).biomass,
        start,
        "the grace is still holding — nothing has been lost yet"
    );
    assert!(
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .any(|entry| matches!(entry.kind, CommandEventKind::HerdUnderHerded)),
        "...and the player has already been told, while it is still free to fix"
    );
}

/// A fully-staffed `herded_fraction` — the sim's `FULLY_HERDED`, restated because it is
/// crate-internal.
const FULLY_HERDED_FIXTURE: f32 = 1.0;

/// **A HALF-TAMED HERD IS OWED THE SAME RATE, AND THE BAND'S KEEPING POOL IS WHAT SUPPLIES IT** —
/// the animal half of §4.6a (`docs/plan_standing_upkeep.md`), with **no exception for the animal
/// web**.
///
/// The meter's fullness used to decide who paid — the build crew below its cost, the pool at it —
/// and that test is deleted: the pool holds every meter carrying work from the first work banked.
/// So a half-tamed herd whose keeping is met sheds nothing, and one whose keeping is short sheds in
/// proportion, exactly as an abandoned rung does. Its **builders** are irrelevant to the shed either
/// way, which is what makes a half-tamed herd holdable at all when the taming crew moves on.
#[test]
fn a_half_tamed_herd_sheds_only_when_its_keeping_is_short() {
    let sheds_with = |supplied_fraction: f32| -> bool {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        // Half-tamed: owned, with a real meter under way and nothing finished.
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            herd.owner = Some(FactionId(0));
            herd.set_ladder_position(A_REAL_TAMING_JOB / 2.0, &core_sim::LadderConfig::builtin());
        }
        assert!(
            !herd_of(&app, &id).is_domesticated(),
            "fixture: the meter is half-filled, so the rung is NOT finished"
        );
        let before = herd_of(&app, &id).biomass;
        // The pool's share, stamped where the labor arm would have written it.
        seat_keeping(&mut app, &id, supplied_fraction);
        // Past the rung's grace, so a shortfall genuinely sheds.
        let grace = neglect_grace(&app, &id);
        for _ in 0..=grace + 1 {
            app.world.run_system_once(advance_husbandry);
            // Re-stamp this turn's share, as the labor arm does.
            seat_keeping(&mut app, &id, supplied_fraction);
        }
        herd_of(&app, &id).biomass < before - 1e-3
    };

    assert!(
        !sheds_with(FULLY_HERDED),
        "a keeping that meets the rate holds the flock — nothing leaves"
    );
    assert!(
        sheds_with(NOT_HERDED_FIXTURE),
        "and one that falls short sheds, exactly as an abandoned rung does — the rate is owed at \
         any meter fullness"
    );
}

/// The taming job a half-tamed fixture stands on, in work units — a real cost rather than the
/// one-worker-turn `FABRICATED_BUILD_COST`, so "half built" is a meter with room in it.
const A_REAL_TAMING_JOB: f32 = 50.0;

/// **A MONOTONE ANIMAL METER READS AS "STILL BUILDING", NOT AS A RE-DECLARATION LOOP.**
///
/// The build verb is derived from the meter (`docs/plan_standing_upkeep.md` §2.4), and both animal
/// meters are **monotone-up**: `domestication_progress` lost its bleed to the neglect-escape arc, and
/// a pen's meter never bleeds either. So a part-built animal rung derives *building* and stays that
/// way **until it completes** — which is the honest reading of a `Tame` that is genuinely still in
/// flight, and is nothing like the plant web's complete → erode → repair cycle.
///
/// **Nothing is written**, so there is no loop to be in: the derivation is a pure read of the meter,
/// and a herd nobody is staffing simply sits there accruing nothing. The consequence the plant web
/// has — a *completed* rung falling back into the building state — is unreachable here, which is why
/// no animal rung declares a `meter_decay`.
#[test]
fn a_monotone_animal_meter_stays_building_until_it_completes() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cost = A_REAL_TAMING_JOB;

    // (1) A wild herd: nothing banked, so only the player can name a rung.
    let wild = herd_of(&app, &id);
    assert_eq!(core_sim::herd_build_verb(&wild, None), None);
    assert_eq!(
        core_sim::herd_build_verb(&wild, Some(Improvement::Tame)),
        Some(Improvement::Tame),
        "a zero meter has no answer of its own — the player declares"
    );

    // (2) Part-tamed: the meter answers, with no declaration and no write.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.owner = Some(FactionId(0));
        herd.set_ladder_position(cost / 2.0, &core_sim::LadderConfig::builtin());
    }
    let part = herd_of(&app, &id);
    assert_eq!(
        core_sim::herd_build_verb(&part, None),
        Some(Improvement::Tame),
        "a meter with progress on it declares its own rung"
    );

    // **And it stays that way across turns nobody staffs** — monotone means it cannot slip back, so
    // this is a stable read rather than a flapping one.
    let before = herd_of(&app, &id).ladder_position();
    run_turns_untended(&mut app, 8);
    let after = herd_of(&app, &id);
    assert!(
        after.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ) >= before,
        "monotone: an animal build meter never bleeds ({before} -> {})",
        after.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        )
    );
    assert_eq!(
        core_sim::herd_build_verb(&after, None),
        Some(Improvement::Tame),
        "…so the derived verb is the same answer every turn — no loop, just an unfinished build"
    );

    // (3) Completed: the meter is full, so it declares nothing and the herd is maintaining.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.set_ladder_position(cost, &core_sim::LadderConfig::builtin());
    }
    let tamed = herd_of(&app, &id);
    assert_eq!(
        core_sim::herd_build_verb(&tamed, None),
        None,
        "a full meter is maintaining"
    );
    assert_eq!(
        core_sim::herd_build_verb(&tamed, Some(Improvement::Tame)),
        None,
        "…and a stale `Tame` declaration on it is inert — nothing has to hunt it down and clear it"
    );
    assert_eq!(
        core_sim::herd_build_verb(&tamed, Some(Improvement::Corral)),
        Some(Improvement::Corral),
        "but the pen's meter is at zero, so climbing to it is still the player's to say"
    );

    // (4) **PART-FENCED — the case the `on_completion` rung made unreachable.**
    //
    // `animal:pen` is `partial_credit: on_completion`, so `RungStanding::credit` is pinned to zero
    // at every position short of a finished fence. The pen arm tested exactly that credit, so a herd
    // with real work banked on its fence and **no live declaration** — what `banking.declared_on`
    // answers once the queue entry is gone — derived no verb at all, where the retired
    // `corral_progress > 0` walk answered `Corral`. The three cases above are all pastoral or
    // complete, so none of them could see it.
    {
        let ladder = core_sim::LadderConfig::builtin();
        let pen_base = {
            let herd = herd_of(&app, &id);
            herd.rung_cost(core_sim::RungKey::AnimalPastoral, &ladder)
        };
        let pen_cost = herd_of(&app, &id).rung_cost(core_sim::RungKey::AnimalPen, &ladder);
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.set_ladder_position(pen_base + pen_cost * HALF_A_FENCE, &ladder);
    }
    let part_fenced = herd_of(&app, &id);

    // **THE PRECONDITION** — the state really is part-fenced, and the credit really is zero, or the
    // arm under test is being asked an easier question than the defect was.
    assert_eq!(
        part_fenced.standing().raising,
        Some(core_sim::RungKey::AnimalPen),
        "fixture: the herd must be raising the pen"
    );
    assert_eq!(
        part_fenced.standing().credit,
        core_sim::NO_RUNG_CREDIT,
        "fixture: an `on_completion` rung in flight is worth nothing — which is why the verb cannot \
         be derived from the credit"
    );
    assert!(
        !part_fenced.corral_meter_full(),
        "fixture: the fence must be unfinished, or the herd is maintaining"
    );

    assert_eq!(
        core_sim::herd_build_verb(&part_fenced, None),
        Some(Improvement::Corral),
        "a pen with work banked on it declares its own rung, with no declaration to lean on"
    );
    assert_eq!(
        core_sim::herd_build_verb(&part_fenced, Some(Improvement::Tame)),
        Some(Improvement::Corral),
        "…and it governs a stale `Tame` beside it — a pen with work on it is the rung in flight"
    );
}

/// Half-way up the pen rung — a fence with real work in it and real work left.
const HALF_A_FENCE: f32 = 0.5;

/// **WHAT ONE OF A BAND'S ANIMAL KEEPERS SUPPLIES PER TURN** — its bare `PER_WORKER_OUTPUT` plus
/// whatever the derived `husbandry` kit delivers (`docs/plan_standing_upkeep.md` §4.8: one supply
/// expression, two consumers). Read off the roster rather than stated as a literal, so retuning the
/// hurdles moves the fixture with the game.
fn animal_keeper_supply(keepers: u32) -> f32 {
    let equipment = core_sim::EquipmentConfig::builtin();
    let per_worker = equipment
        .keeping_kit_for_branch(core_sim::RungBranch::Animal, None)
        .map(|kit| {
            equipment.build_work_per_worker(
                &kit,
                &core_sim::BandEquipment::start_stocked(&equipment),
                core_sim::RungBranch::Animal,
                None,
            )
        })
        .expect("the shipped roster serves the animal web's keeping");
    core_sim::pool_work_supply(keepers, per_worker)
}

/// **A SECOND HERD THE SAME BAND CAN WORK** — inside `band_work_range + hunt_leash_tiles` of
/// `near`, and tameable, so it can stand as the band's real pastoral holding beside a blocked head.
fn second_tameable_herd_in_reach(app: &App, near: &str) -> String {
    let reach = app.world.resource::<LaborConfigHandle>().get().hunt_reach();
    let sim = app.world.resource::<SimulationConfig>();
    let (width, wrap) = (sim.grid_size.x, sim.map_topology.wrap_horizontal);
    let registry = app.world.resource::<HerdRegistry>();
    let anchor = registry
        .find(near)
        .expect("the anchor herd exists")
        .position();
    registry
        .herds
        .iter()
        .filter(|herd| herd.id != near && herd.can_domesticate())
        .find(|herd| {
            core_sim::grid_utils::hex_distance_wrapped(anchor, herd.position(), width, wrap)
                <= reach
        })
        .map(|herd| herd.id.clone())
        .expect("a second tameable herd inside the band's hunt reach")
}

/// **A BLOCKED `Tame` AT THE HEAD CLAIMS NO KEEPING, AND THE BAND'S PASTORAL FLOCK IS PAID IN FULL**
/// — the animal twin of `forage_cultivation`'s
/// `a_blocked_head_claims_no_keeping_and_the_holding_beside_it_is_paid_in_full`, and the case
/// reported from play (`docs/plan_standing_upkeep.md` §4.6a).
///
/// The claim side's verb term is narrowed to the **funded head**, which still admitted a head whose
/// own rung gate refuses. A `Tame` on a herd standing at its crew's escapement floor banks nothing
/// on any turn — `crew_is_working_the_source` reads no room — while claiming the pastoral rung's
/// whole demand, and the default `Spread` then divided the band's `husbandry` pool pro rata across a
/// build that was never going to move.
///
/// **Escapement is the staged cause deliberately**: it is the one the playtest sat on, and on the
/// animal web it is self-sustaining (the hunters draw the flock to the floor and an unmet keeping
/// suppresses its regrowth), so nothing on the build line reopens it.
#[test]
fn a_blocked_tame_claims_no_keeping_and_the_pastoral_flock_beside_it_is_paid_in_full() {
    /// The pool standing on the head. A blocked head is only reportable — and only dilutive — when
    /// the player has actually committed builders to it.
    const BUILDERS: u32 = 2;
    /// The blocked herd sits exactly AT this floor, so `max(0, B − floor·K)` is `0` and the Tame's
    /// escapement term refuses. The unblocked arm lifts the flock above it and changes nothing else.
    const AT_THE_FLOOR: f32 = MSY_BIOMASS_FRACTION;
    /// Well above it, so the same crew is genuinely working the same herd.
    const ABOVE_THE_FLOOR: f32 = 0.8;

    struct Turn {
        holding_supplied: f32,
        holding_demand: f32,
        build_supplied: f32,
        build_demand: f32,
        build_progress: f32,
        blocked_reason: String,
    }

    /// **One turn, both arms.** This fixture is about what a Tame's *first* turn does: the blocked
    /// head banks nothing and the open one banks its first work, and neither owes a keeping yet.
    const ONE_TURN: u32 = 1;

    let run = |standing_crop: f32, turns: u32| -> Turn {
        let mut app = spawn_world();
        grant_herding(&mut app);
        let build = prime_thriving_herd(&mut app);
        let holding = second_tameable_herd_in_reach(&app, &build);
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id == build)
                .expect("the blocked herd exists");
            herd.biomass = herd.carrying_capacity * standing_crop;
        }
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id == holding)
                .expect("the holding herd exists");
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            // **This turn's supply, cleared.** `Herd::upkeep_supplied` accumulates across the bands
            // working a source and is zeroed by the Logistics decay pass; this fixture runs the
            // labor arm alone, so a seeded value would be added to rather than replaced.
            herd.upkeep_supplied = 0.0;
        }
        // **The pool covers the holding outright and NOT both**, so "paid in full" and "diluted" are
        // distinguishable outcomes on one fixture. Sized off the shipped seams rather than a
        // literal, because an animal rung's demand rides the flock's own keeper load.
        let holding_demand = keeping_demand(&app, &holding);
        // **The pool covers the holding**, which is what makes "paid in full" a real outcome rather
        // than a pool that could not have short-changed anyone.
        //
        // It used to also require the pool to be **too small for both**, so that a dilution would be
        // visible. That precondition is unsatisfiable now, and the reason is the fix itself: the
        // keeping demand **interpolates on the herd's position**, and a blocked head has banked
        // nothing — so what it would claim is `0`, and there is no second bill for the pool to fall
        // short of. The blocked head is guarded twice over now (the claim gate refuses it, and its
        // bill is zero besides); the arm that can still tell a live claim from a dead one is the
        // **unblocked** one below, where the same herd banks work and does draw.
        let keepers = (holding_demand / animal_keeper_supply(1)).ceil().max(1.0) as u32;
        let pool = animal_keeper_supply(keepers);
        assert!(
            pool >= holding_demand,
            "fixture: the pool must cover the holding — {pool} against {holding_demand}"
        );

        let pos = app
            .world
            .resource::<HerdRegistry>()
            .find(&build)
            .expect("the blocked herd exists")
            .position();
        let tile = app
            .world
            .resource::<TileRegistry>()
            .index(pos.x, pos.y)
            .expect("the herd's tile resolves");
        let hunt_row = |id: &str| LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: id.to_string(),
                floor: AT_THE_FLOOR,
            },
            workers: DIP_VISIBLE_HUNTERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        };
        let rows = with_keeping_role(
            with_builders_pool(vec![hunt_row(&holding), hunt_row(&build)], BUILDERS),
            keepers,
        );
        let staffed: u32 = rows.iter().map(|row| row.staffed_total()).sum();
        app.world.spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(staffed as f32),
                elders: scalar_zero(),
                stores: pen_materials_support::stocked_with_pen_materials(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
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
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: rows,
                build_queue: vec![core_sim::BuildQueueEntry {
                    source: core_sim::BuildSource::Herd(build.clone()),
                    declared: core_sim::BuildJob::Rung(Improvement::Tame),
                    kit: None,
                }],
                ..Default::default()
            },
            ResidentBand,
        ));
        // **TWO passes, because a Tame's first turn owes nothing.** The keeping demand interpolates
        // on the herd's position, and that position is `0` when the first turn's pool is split — so
        // an unblocked build draws nothing on turn one and dilutes nothing, and the liveness half of
        // this test would be measuring the very state the blocked arm is about. The second pass
        // reads a build genuinely part-way up its rung. `advance_husbandry` between them is what
        // clears the per-turn keeping scratch, so turn two is judged on turn two's bill.
        for turn in 0..turns {
            if turn > 0 {
                // **The per-turn keeping scratch, cleared by hand between passes.** In a real turn
                // `advance_husbandry` does this in Logistics; running that whole system here would
                // also regrow the flock, which lifts the blocked arm off its floor and opens the
                // very gate it exists to hold shut. What the second pass needs is only that the
                // bill and the supply describe *its own* turn.
                for herd in app.world.resource_mut::<HerdRegistry>().herds.iter_mut() {
                    herd.upkeep_supplied = 0.0;
                    herd.upkeep_demanded = None;
                }
            }
            app.world.run_system_once(advance_labor_allocation);
        }

        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let read = |id: &str| -> (f32, f32) {
            let herd = herd_of(&app, id);
            (
                herd.upkeep_supplied,
                // **Read the way the CAPTURE reads it** — after the accrual, with no verb in hand.
                core_sim::herd_upkeep_demand(&herd, &fauna, &ladder),
            )
        };
        let (holding_supplied, holding_demand) = read(&holding);
        let (build_supplied, build_demand) = read(&build);
        let built = herd_of(&app, &build);
        Turn {
            holding_supplied,
            holding_demand,
            build_supplied,
            build_demand,
            build_progress: built.rung_work_done(
                core_sim::RungKey::AnimalPastoral,
                &core_sim::LadderConfig::builtin(),
            ),
            blocked_reason: built.build_blocked_reason.key().to_string(),
        }
    };

    // --- (a) THE BLOCKED HEAD ------------------------------------------------------------------
    let blocked = run(AT_THE_FLOOR, ONE_TURN);
    assert_eq!(
        blocked.blocked_reason, "escapement",
        "fixture: the head must be blocked, and by the escapement gate — got '{}'",
        blocked.blocked_reason
    );
    assert_eq!(
        blocked.build_progress, 0.0,
        "fixture: a blocked Tame banks nothing, so its meter must still be at zero (got {})",
        blocked.build_progress
    );
    assert!(
        blocked.holding_demand > 0.0,
        "fixture: the pastoral flock must owe something, or 'paid in full' is vacuous"
    );
    assert!(
        (blocked.holding_supplied - blocked.holding_demand).abs() < 1e-4,
        "a blocked Tame must not dilute the band's pastoral flock: supplied {} of {}",
        blocked.holding_supplied,
        blocked.holding_demand
    );
    // **THE WIRE HALF** — a wild herd with nothing banked owes nothing once the verb is out of the
    // reading, so a stamped share would be a row disagreeing with itself.
    assert_eq!(
        blocked.build_demand, 0.0,
        "a wild herd with nothing banked owes nothing ({})",
        blocked.build_demand
    );
    assert_eq!(
        blocked.build_supplied, 0.0,
        "…so the pool must have put nothing on it either ({})",
        blocked.build_supplied
    );

    // --- (b) THE SAME HEAD, UNBLOCKED ----------------------------------------------------------
    let open = run(ABOVE_THE_FLOOR, ONE_TURN);
    assert_eq!(
        open.blocked_reason, "",
        "fixture: standing the flock above the floor must open the gate — still blocked on '{}'",
        open.blocked_reason
    );
    assert!(
        open.build_progress > 0.0,
        "fixture: the unblocked Tame must bank its first work this turn, or the verb term is never \
         exercised"
    );
    // **AND ON ITS FIRST TURN IT STILL DILUTES NOTHING**, which is not the old answer and is the
    // whole of the front-loading fix. A Tame's claim is its *position*, and the position is `0` when
    // the pool is split — so an unblocked head that banks its first work this turn owes nothing this
    // turn, exactly as the blocked one does. What separates the two arms is `build_progress` above.
    //
    // **The claim then GROWS with the rung**, which is where the dilution actually lives now; that
    // ordering is pinned by `a_half_tamed_herd_owes_about_half_the_pastoral_rate` and
    // `the_cost_and_the_benefit_move_together`, on fixtures that can seat a real position rather
    // than having to reach one through the build.
    assert_eq!(
        open.build_supplied, 0.0,
        "a Tame owes nothing on the turn it banks its first work ({})",
        open.build_supplied
    );
    assert!(
        (open.holding_supplied - open.holding_demand).abs() < 1e-4,
        "…so the flock beside it is still paid in full — {} of {}",
        open.holding_supplied,
        open.holding_demand
    );
}

// ---------------------------------------------------------------------------------------------
// THE GRACE AND THE PRESSURE ARE ONE PREDICATE (`fauna::herd_is_neglected`)
// ---------------------------------------------------------------------------------------------

/// **Staffing that leaves a tenth of the flock unheld** — under the whole-animal gate on a small
/// herd, over it on a large one. The whole fixture is that one fraction read at two herd sizes.
const NINETY_PERCENT_KEPT: f32 = 0.9;

/// **A flock small enough that a tenth of it is less than one animal.** Three head at 10% short is
/// `0.3` of an animal, which `uncontained_overage` correctly answers `None` for: you cannot lose
/// three tenths of a rabbit.
const A_THREE_HEAD_FLOCK: f32 = 3.0;

/// **…and one big enough that the same tenth is ten whole animals** — the state the herd crosses
/// into, where the shed genuinely arms.
const A_HUNDRED_HEAD_FLOCK: f32 = 100.0;

/// Turns of faithful 90% keeping before the flock grows. Long enough that a pressure meter rising
/// `+0.1` a turn would reach the region where `(1 + escape_acceleration)^pressure` clamps the shed
/// rate to `1.0` — which is what the defect did.
const A_LONG_SERVICE: u32 = 300;

/// **⛔ A HERD NEVER ONCE UNDER-CONTAINED CANNOT ACCUMULATE NEGLECT — the grace and the pressure are
/// one predicate.**
///
/// `Herd::neglect_turns` rises only when [`uncontained_overage`] leaves **a whole animal** unheld;
/// `Herd::neglect_pressure` rose on any positive shortfall fraction at all, decayed only on a turn
/// the bill was fully met, and had no ceiling. So a **3-head flock kept at 90%** — overage `0.3`,
/// never under-contained, never shedding, its grace reset every single turn — nevertheless frayed at
/// `+0.1` a turn for as long as it was kept that way. The pressure is the exponent in
/// `rate × (1 + escape_acceleration)^pressure`, so the turn that flock finally grew past the
/// one-animal gate, its first shed fired at a rate three hundred turns of *good* keeping had
/// multiplied.
///
/// Both halves are asserted, and neither is sufficient. The pressure staying at zero is the fix
/// stated; the shed being **identical** to a herd with no history at all is the consequence a player
/// feels. The two runs draw the same jitter (the seed is `map_seed ^ tick ^ id`, and these harness
/// systems advance no tick), so the comparison is exact rather than banded.
#[test]
fn ninety_percent_keeping_never_frays_a_herd_below_the_whole_animal_gate() {
    /// The biomass a herd of `head` animals of this fixture's species stands at.
    fn flock(app: &App, id: &str, head: f32) -> f32 {
        herd_of(app, id).body_mass * head
    }

    // Hold a small flock at 90% for `service` turns, then grow it past the whole-animal gate and
    // return `(pressure before it grew, the biomass its first shed takes)`.
    let run = |service: u32| -> (f32, f32) {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        domesticate(&mut app, &id);

        let small = flock(&app, &id, A_THREE_HEAD_FLOCK);
        reseat(&mut app, &id, small, small);
        for _ in 0..service {
            seat_keeping(&mut app, &id, NINETY_PERCENT_KEPT);
            run_turns_untended(&mut app, 1);
        }
        let pressure = herd_of(&app, &id).neglect_pressure;

        // The flock grows past the gate: the same 10% short is now ten whole animals unheld.
        let big = flock(&app, &id, A_HUNDRED_HEAD_FLOCK);
        reseat(&mut app, &id, big, big);
        let grace = neglect_grace(&app, &id);
        let mut shed = 0.0;
        for _ in 0..=(grace + 2) {
            seat_keeping(&mut app, &id, NINETY_PERCENT_KEPT);
            let before = herd_of(&app, &id).biomass;
            run_turns_untended(&mut app, 1);
            let after = herd_of(&app, &id).biomass;
            if after < before {
                shed = before - after;
                break;
            }
        }
        (pressure, shed)
    };

    let (fresh_pressure, fresh_shed) = run(0);
    let (served_pressure, served_shed) = run(A_LONG_SERVICE);

    // **THE PRECONDITION** — the fixture really does arm the shed once the flock is large, or
    // "sheds the same" would be a comparison of two zeroes.
    assert!(
        fresh_shed > 0.0,
        "fixture: a hundred-head flock kept at 90% must genuinely shed once its grace elapses, or \
         this test compares nothing"
    );
    assert_eq!(
        fresh_pressure, 0.0,
        "fixture: a herd that has just been tamed carries no pressure"
    );

    assert_eq!(
        served_pressure, 0.0,
        "{A_LONG_SERVICE} turns of keeping that never left a whole animal unheld must leave the \
         herd unfrayed — the grace was reset every one of those turns, and the pressure has to be \
         reset by the same predicate or it is measuring a neglect that never happened"
    );
    assert!(
        (served_shed - fresh_shed).abs() < 1e-4,
        "…so its first shed is exactly what a herd with no history sheds: {served_shed} against \
         {fresh_shed}"
    );
}
