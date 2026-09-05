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

mod pen_materials_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::TakeSelection;
use core_sim::{
    advance_cultivation, advance_herds, advance_husbandry, advance_labor_allocation,
    build_fraction, build_test_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one,
    scalar_zero, BandEquipment, DiscoveryProgressLedger, EquipmentConfig, FactionId,
    FaunaConfigHandle, ForageRegistry, GenerationId, HerdRegistry, Improvement, LaborAllocation,
    LaborAssignment, LaborTarget, LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, RungKey, SimulationConfig, SnapshotHistory, SourcePriority, TileRegistry,
    DEFAULT_ESCAPEMENT_FLOOR, HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION, PENNING_DISCOVERY_ID,
};

/// **The species the fixture reshapes its herd into** — a `pen`-ceiling row, so `can_domesticate()`
/// holds and a `Tame` actually accrues. Named rather than "whatever worldgen spawned" because a
/// `wild`-ceiling herd would make the build silently refuse and the equality below trivially true of
/// two zeroes.
const TAMEABLE_SPECIES: &str = "Wild Boar";

/// **A tameable species whose `Tame` costs exactly the rung's own price** — `taming_cost_multiplier`
/// **1.0**, where the shipped Wild Boar is 1.25. It is what puts the heavily-geared case within
/// reach of a crew a band can actually staff: at [`KEEPERS`] hands each delivering
/// `PER_WORKER_OUTPUT + 0.5`, a 50-unit job is a handful of turns rather than dozens, and the shipped
/// roster has five such rows (rabbit, fowl, crag goat, wild sheep, snow hare).
const UNSCALED_TAMEABLE_SPECIES: &str = "Crag Goats";

/// **The kit the keepers are sent out with on the HUNT ROW** — the animal web's own take kit, which
/// carries the sled a keeper hauls with. It was `husbandry` until §4.9 item 12b deleted that kit: a
/// pen resolves the ordinary fight, so the hunters who took the herd wild take it penned.
const HANDLING_KIT: &str = "big_game";

/// **The kit the BUILDERS are sent out with** — the animal web's builders kit, whose crook is the
/// gear term under test. It is deliberately not [`HANDLING_KIT`]: a take kit does no building, and
/// what raises a `Tame` is `hurdling`.
const BUILDERS_KIT: &str = "hurdling";

/// **The plant web's builders kit** — hoes, which take work off a Cultivate and nothing off a `Tame`.
const TILLAGE_KIT: &str = "tillage";

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
    let mut app = build_test_app();
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
    /// the regime the cap exists for: six keepers deliver *one worker's worth* of gear work.
    OneSet,
    /// **A party's worth**, what a spawn stocks — every hand equipped, so coverage is uniform and
    /// the gear term is the plain `workers × worth`.
    APartysWorth,
}

/// **The crew the closed-form fixture staffs its build at**, and every term of the estimate is
/// quoted at it — the work banked per turn *and* the gear delivered beside it
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
    let equipment = EquipmentConfig::for_a_stocked_fixture();
    let kit = equipment
        .kit(HANDLING_KIT)
        .expect("the shipped roster carries the big-game kit");
    let builders_kit = equipment
        .kit(BUILDERS_KIT)
        .expect("the shipped roster carries the hurdling kit");
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
                stores: pen_materials_support::stocked_with_pen_materials(),
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
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Hunt {
                            fauna_id: fauna_id.to_string(),
                            floor: BUILDER_FLOOR,
                        },
                        // **The crews are stated GROSS here**, and the herd is sized so the
                        // maintenance rate is a single hand ([`TEST_CAPACITY`]) — so both rows still
                        // clear it comfortably and the nets stay multi-turn, which is what a `ceil`
                        // check needs.
                        workers: KEEPERS,
                        kit: Some(kit.clone()),
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                    // **The build is staffed by the band's own POOL**, at the crew the caller
                    // named (`docs/plan_standing_upkeep.md` §2.5). **The row carries no kit** — a
                    // build's gear is read off the queue ENTRY below (§4.7a ②). The published turns
                    // estimate and the gear stamp are both quoted at the pool, so the two have to
                    // agree for a closed-form check to mean anything.
                    LaborAssignment {
                        target: LaborTarget::Builders,
                        workers: builders,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                ],
                build_queue: improvement
                    .map(|declared| core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Herd(fauna_id.to_string()),
                        declared: core_sim::BuildJob::Rung(declared),
                        // **The kit rides the ENTRY**, which is where a build's gear offset is read
                        // from since §4.7a ②.
                        kit: Some(builders_kit),
                    })
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        ))
        .id()
}

/// ⛔ **HURDLES SPEED A TAME AND A HOE DOES NOTHING FOR ONE** — the animal mirror of
/// `build_turns_on_the_wire.rs::a_plant_build_is_geared_by_the_hoe_and_by_nothing_else`, read off
/// the encoded herd row's `buildWorkFromGear`.
///
/// The two files together are the whole of the `branch` qualifier's claim: each tool is worth
/// something on **its** web and exactly nothing on the other, and the pool picks the tool off the
/// **entry** rather than off a stored id. The liveness arm carries them both — a filter that zeroed
/// everything would pass the negatives on its own.
#[test]
fn an_animal_build_is_geared_by_the_hurdles_and_by_nothing_else() {
    let published = |kit_id: Option<&str>| -> f32 {
        let (mut app, id, pos) = world_with_a_tameable_herd();
        let keepers = spawn_taming_keepers(&mut app, pos, &id, GearHeld::APartysWorth, KEEPERS);
        set_builders_kit(&mut app, keepers, kit_id);
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
    };

    let derived = published(None);
    assert!(
        derived > core_sim::NO_BUILD_GEAR,
        "**LIVENESS**: an unnamed builders row derives the animal web's own kit, so a Tame is \
         geared — got {derived}"
    );
    assert_eq!(
        published(Some(BUILDERS_KIT)),
        derived,
        "naming the kit the derivation would have picked changes nothing"
    );
    assert_eq!(
        published(Some(TILLAGE_KIT)),
        core_sim::NO_BUILD_GEAR,
        "a hoe is a plant tool and takes NOTHING off a Tame — the branch qualifier's whole job"
    );
    assert_eq!(
        published(Some("none")),
        core_sim::NO_BUILD_GEAR,
        "going out bare is a real selection and must not fall back to the derived kit"
    );
}

/// Re-kit a band's queued build after the fact — `None` clears the override, which is *derive from
/// this entry's own web* (`docs/plan_standing_upkeep.md` §4.7a ②).
///
/// **It rides the queue ENTRY, not the `builders` row**, which carries no kit at all: one stored id
/// per band cannot be right for both food webs, so `assign_labor` refuses a token there.
fn set_builders_kit(app: &mut App, band: bevy::prelude::Entity, kit_id: Option<&str>) {
    let kit = kit_id.map(|id| {
        EquipmentConfig::builtin()
            .kit(id)
            .unwrap_or_else(|| panic!("the shipped roster carries '{id}'"))
    });
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band keeps its allocation");
    assert!(
        !allocation.build_queue.is_empty(),
        "the fixture band carries a queue entry to re-kit"
    );
    for entry in allocation.build_queue.iter_mut() {
        entry.kit = kit.clone();
    }
}

/// **A POOL CARRYING NOTHING THAT HELPS** — `intensification::NO_BUILD_GEAR` in the client's own
/// units, so the bare arm of a pair is a stated fact rather than an unexplained `0.0`.
const NO_GEAR_PER_WORKER: f32 = 0.0;

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
    // and the gear delivered beside it — `build_work_per_worker` is a **rate per worker**, so the
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
    // **⛔ THE GEAR TERM IS IN THE DENOMINATOR NOW** (`docs/plan_standing_upkeep.md` §4.8). It used
    // to be subtracted from the job (`workCost − workDone − gear(w)`), which granted the kit's help
    // as a one-time lump against the target; a kit raises what a worker *delivers per turn*, so it
    // is an addend on the supply and the numerator is the job, whole.
    //
    // **The saturation survives the move, and that is why the pair stayed on `kitTiers` rather than
    // being folded into a published pool rate.** Coverage arms a *prefix*, so an eleventh keeper
    // with ten sets of hurdles between them adds only their own hands — a pre-averaged rate would
    // lose that silently on the one crew a compose sheet is for: a proposed one, of a size the sim
    // never resolved.
    let gear = client_gear_term(build_work_per_worker, build_work_saturating_crew, builders);
    // **NO FLOOR TERM.** The build reads the assignment's escapement floor no longer
    // (`docs/plan_standing_upkeep.md` §2.2): a build is staffed in its own right, so the builders
    // are not pulling on the source and there is nothing of theirs for a floor to describe. The
    // client's form loses the factor with the sim's.
    let work_per_turn =
        (builders as f32 * build_work_per_worker_turn + gear - meter_rot_per_turn).max(0.0);
    if work_per_turn <= 0.0 {
        return None;
    }
    let remaining = work_cost - work_done;
    if remaining <= 0.0 {
        // **A job already worked off is ONE turn, not "no estimate"** — the sim's own
        // `build_turns_remaining` answers `1` there, so a client form that withheld the line would
        // blank the readout on a build that is finished.
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
    /// **Two turns, because a herd is only MID-build from its second.** The keeping demand
    /// interpolates on the herd's position, and that position is `0` when the first turn's pool is
    /// split — so a herd read after one pass is billed the honest `0` of ground nobody has climbed,
    /// and (1b) below would be ordering a quote against nothing.
    const TURNS_TO_REACH_MID_BUILD: u32 = 2;
    let mut saw_saturated = false;
    let mut saw_linear = false;

    for gear in [GearHeld::OneSet, GearHeld::APartysWorth] {
        let (mut app, id, pos) = world_with_a_tameable_herd();
        // The build's own crew, smaller than the party beside it — the gear stamp reads the
        // BUILD's crew now, so this is the count both halves of the form are judged at.
        let keepers = spawn_taming_keepers(&mut app, pos, &id, gear, THE_BUILD_CREW);
        for _ in 0..TURNS_TO_REACH_MID_BUILD {
            // **Stage order, and `advance_husbandry` is load-bearing here.** It is what clears the
            // per-turn keeping scratch (`Herd::upkeep_supplied` / `upkeep_demanded`), so a chain
            // that skipped it would keep turn one's bill — struck at a position of zero — for ever.
            app.world.run_system_once(advance_herds);
            app.world.run_system_once(advance_husbandry);
            app.world.run_system_once(advance_labor_allocation);
        }
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
            .find(|row| row.kit_id == BUILDERS_KIT)
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

        // (1b) **AND THE BILL IS A SHARE OF THE QUOTE ON A SOURCE MID-BUILD.** They answer different
        // questions — what this herd is *billed* now, and what the rung being *quoted* would cost to
        // hold once it is held — and since the animal web got its one-position ladder the bill
        // **interpolates**: a herd part-way up the pastoral rung owes part of that rung's rate.
        //
        // They used to be asserted **equal**, which was the shipped defect stated as an invariant:
        // `accrue_domestication` recorded an owner on the first work banked and the herd was billed
        // the whole rate from that turn on — 100% of the cost for 0% of the benefit, §2.8's asymmetry
        // inverted. The ordering below is what replaced it, and it is the same shape the plant web's
        // `an_unstarted_patch_publishes_the_quoted_rungs_upkeep_where_the_billed_one_is_zero` pins.
        assert!(
            herd.tame_upkeep_demand > 0.0,
            "fixture: the pastoral rung must cost something to hold, or the ordering is vacuous"
        );
        assert!(
            herd.upkeep_demand > 0.0 && herd.upkeep_demand <= herd.tame_upkeep_demand,
            "a herd raising the pastoral rung is billed a SHARE of what that rung is quoted at — \
             billed {} against a quote of {}",
            herd.upkeep_demand,
            herd.tame_upkeep_demand
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
            // **The pool the wire publishes** — the band's `builders` row, which is where a
            // client reads the count for this form since `docs/plan_standing_upkeep.md` §2.5.
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

/// **THE GEAR OFFSET IS QUOTED AT THE BUILDERS POOL, NOT AT THE BAND'S CREW ON THE SOURCE**
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// `buildWorkPerWorker` is a **rate per worker**, so the count it multiplies has to be the workers
/// actually doing the job. Reading the take crew instead was reachable the moment the two came
/// apart: it let a **single** builder standing beside a large gathering party build at that whole
/// party's rate. (Under the retired `LadderConfig::effective_build_cost` — which took the same
/// product off the *job* and was unfloored — the same mistake paid a rung off outright on its first
/// turn; the seam it is read at is unchanged either way, which is what this arm pins.)
///
/// ⛔ **THE POOL CARRIES ITS OWN KIT NOW, and this is the test that catches the wiring if it does
/// not.** The offset used to ride the *source row's* kit, because the builders stood on the tile; it
/// is read off the `builders` role's own row and coverage since §2.5. A kit that declares
/// `build_work` but does not list `builders` among its `jobs` would resolve the neutral `0.0` — so
/// every arm below would be a comparison of zeroes, which the **liveness** assertion is here to
/// refuse.
///
/// Asserted on four things at once, because each fails independently:
/// 1. the offset is **non-zero at all** — the liveness half, and the kit-wiring guard;
/// 2. it **scales with the pool** — one builder delivers one worker's worth of gear work;
/// 3. it **saturates** at the units the band actually holds, so a builder with no gear left to pick
///    up adds nothing further; and
/// 4. it **does not move when only the take crew moves** — the negative control, and the one a form
///    reading the wrong crew fails.
#[test]
fn the_gear_offset_scales_with_the_builders_pool_and_ignores_the_take_crew() {
    // `GearHeld::OneSet` is the band's reference ledger: it arms a **prefix** of the party, so the
    // saturation point sits well below the crews swept here and both regimes are exercised.
    let saturating = gear_stamped_for(GearHeld::OneSet, KEEPERS, SOLE_BUILDER);
    assert!(
        saturating > NO_GEAR_AT_ALL,
        "fixture: one set of handling gear must deliver something, or every arm below is \
         a comparison of zeroes"
    );

    // (1) Two builders deliver more gear work than one — up to the point the gear runs out.
    let two_builders = gear_stamped_for(GearHeld::APartysWorth, KEEPERS, TWO_BUILDERS);
    let one_builder = gear_stamped_for(GearHeld::APartysWorth, KEEPERS, SOLE_BUILDER);
    assert!(
        one_builder > NO_GEAR_AT_ALL,
        "**LIVENESS**: the pool's own kit must deliver something. A `build_work` item in a \
         kit that does not list the `builders` job resolves the neutral 0.0, and every comparison \
         below then holds over a dead term (offset {one_builder})"
    );
    assert!(
        two_builders > one_builder,
        "the offset must scale with the BUILD crew (one {one_builder}, two {two_builders})"
    );

    // (2) …and saturates: with one set of gear between them, a second builder finds nothing to
    // pick up, so the offset is the same as one builder's.
    assert_eq!(
        gear_stamped_for(GearHeld::OneSet, KEEPERS, TWO_BUILDERS),
        gear_stamped_for(GearHeld::OneSet, KEEPERS, SOLE_BUILDER),
        "the saturating prefix binds against the BUILD crew — an unarmed builder delivers nothing \
         further"
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
        // Row 0 is the worked source; the `builders` row beside it carries the pool and its kit.
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
    let LaborTarget::Hunt { fauna_id, .. } = allocation
        .assignments
        .first()
        .expect("the keeper band keeps its one worked source")
        .target
        .clone()
    else {
        panic!("the keeper band hunts");
    };
    // **A verb DECLARES; it does not staff** (`docs/plan_standing_upkeep.md` §2.5) — completion
    // retires the previous entry, so declaring the next rung is the whole of what `corral` does.
    // The `builders` row the fixture spawned is untouched and simply moves to the new head.
    assert!(allocation.enqueue_build(
        core_sim::BuildSource::Herd(fauna_id),
        core_sim::BuildJob::Rung(improvement),
    ));
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

/// **THE WIRE PUBLISHES THE WHOLE JOB, AND A GEARED POOL THAT FINISHES IT SOONER.**
///
/// **⛔ IT ASSERTED THE DEFECT §4.8 CORRECTED, AND IS RE-AIMED RATHER THAN DELETED.** It read: *"six
/// keepers each holding a set of handling gear take `6 × 8.5 = 51` units off a 50-unit `Tame`"* —
/// the retired subtraction's own units — so
/// the fixture existed to pin a build finishing on its first worked turn **by arithmetic** — which
/// made the crew axis meaningless past that pool size. A kit raises what a worker delivers per turn
/// now, and the job is the same pile with the gear and without.
///
/// So the same fixture, on the same wire, pins **the pair** instead: the published `tameWorkCost` is
/// the rung's whole price with the pool fully equipped, **and** that pool still finishes strictly
/// sooner than a bare one of the same size. Either alone passes a broken model — the first for a kit
/// that does nothing, the second for the subtraction this replaced.
///
/// Asserted **off the exported snapshot** in both states the field has: the **projection** a crew
/// that is deciding reads (the compose sheet's case, and the one the player meets), and the **live**
/// stamp of the crew that ran the build. The client's own form is held to the same answer, because a
/// sheet withholding the line while the card states it is the disagreement this file exists to
/// forbid.
#[test]
fn the_wire_publishes_the_whole_job_and_a_geared_pool_that_finishes_it_sooner() {
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
            .find(|row| row.kit_id == BUILDERS_KIT)
            .expect("the band publishes a tier row per roster kit");

        // **THE JOB IS THE WHOLE JOB, WHATEVER THE POOL CARRIES** — the invariant this arm was
        // re-aimed onto (§4.8). The published `tameWorkCost` is the rung's own price times the
        // species' multiplier, and no term of the crew's kit appears in it.
        let gear = client_gear_term(
            tiers.build_work_per_worker,
            tiers.build_work_saturating_crew,
            KEEPERS,
        );
        assert!(
            gear > 0.0,
            "fixture: {KEEPERS} keepers must actually be carrying handling gear, or both halves of \
             the claim are vacuous"
        );
        assert!(
            herd.tame_work_cost > gear,
            "the published job must be the rung's whole price, not a bar the pool's kit shrank: {} \
             against a kit delivering {gear} a turn",
            herd.tame_work_cost
        );

        // **AND THE GEAR STILL DOES SOMETHING** — the same pool carrying nothing takes strictly
        // longer over the identical job. Struck through the client's own form, so the pair is
        // asserted on the expression the compose sheet evaluates.
        let geared_turns = client_turns_estimate(
            herd.tame_work_cost,
            herd.tame_work_done,
            tiers.build_work_per_worker,
            tiers.build_work_saturating_crew,
            herd.build_work_per_worker_turn,
            KEEPERS,
            herd.meter_rot_per_turn,
        )
        .expect("a staffed crew has an estimate");
        let bare_turns = client_turns_estimate(
            herd.tame_work_cost,
            herd.tame_work_done,
            NO_GEAR_PER_WORKER,
            tiers.build_work_saturating_crew,
            herd.build_work_per_worker_turn,
            KEEPERS,
            herd.meter_rot_per_turn,
        )
        .expect("a staffed bare crew has an estimate");
        assert!(
            geared_turns < bare_turns,
            "the equipped pool must finish the same job sooner: {geared_turns} against \
             {bare_turns} — improvement {improvement:?}, cost {} done {}",
            herd.tame_work_cost,
            herd.tame_work_done
        );

        // And the client's transcribed form must reproduce the sim's own answer, or the compose
        // sheet and the tile card disagree.
        assert_eq!(
            Some(geared_turns),
            u32::try_from(herd.build_turns_remaining).ok(),
            "the client's form must reproduce the sim's answer with the gear in the divisor too"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// THE ROT TERM, WHERE IT IS ACTUALLY NON-ZERO (`docs/plan_standing_upkeep.md` §4.6a)
// ---------------------------------------------------------------------------------------------

/// **THE EQUALITY WITH A LIVE `meterRotPerTurn`, ON THE PLANT WEB, PAST THE GRACE.**
///
/// Every arm above runs on a **herd**, and no animal rung declares a `meter_decay` — so
/// `meterRotPerTurn` is structurally `0` there and the divisor's second term is never exercised. It
/// also runs **one** turn, so the neglect counter never leaves its grace. Between them, those two
/// facts mean the arms above **would not have caught a wrong rot**: the client's form and the sim's
/// answer agree on any rot when the rot is zero.
///
/// This arm closes that: a `plant:tended` build with **no keeping**, walked past the rung's own
/// grace, so the published rot is the rung's real `meter_decay.per_turn` and the client's transcribed
/// form has to net exactly it to land on `buildTurnsRemaining`.
///
/// **The gear term is LIVE here now**, and that is what the hoes bought: the plant web has a build
/// tool, so this arm exercises the rot and the gear together — which is the only place on the wire
/// where both terms of the divisor and both terms of the numerator are non-zero at once.
#[test]
fn the_client_form_reproduces_the_sim_with_a_live_rot_past_the_grace() {
    /// Builders on the Cultivate. More than one, so the quote is a multi-turn count and `ceil` is
    /// exercised rather than saturating at one turn.
    const BUILDERS: u32 = 2;
    /// A gathering crew beside them, so the rung's own work predicate holds.
    const GATHERERS: u32 = 1;
    /// How far the meter is into its job when the walk starts — room to move in either direction,
    /// and low enough that the job is still several turns off when the walk ends (the hoes raise
    /// what each builder banks by half again).
    const HALF_BUILT: f32 = 0.1;
    /// Well above the escapement floor, so `crew_is_working_the_source` stays true every turn.
    const STOCKED: f32 = 0.8;

    let mut app = build_test_app();
    app.update();

    // A curated gathering site whose basket the tended rung can commit to — the Cultivate gate wants
    // both, and a refusal would make the quote `-1` for a reason that is not the rot.
    let (source, crop) = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<SimulationConfig>().map_seed;
        let sites: Vec<UVec2> = app
            .world
            .resource::<core_sim::FoodSiteRegistry>()
            .sites()
            .iter()
            .map(|site| site.position)
            .collect();
        let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
            let mut query = app.world.query::<&core_sim::Tile>();
            query
                .iter(&app.world)
                .map(|tile| (tile.position, tile.clone()))
                .collect()
        };
        let registry = app.world.resource::<ForageRegistry>();
        sites
            .into_iter()
            .find_map(|position| {
                registry.patch(position)?;
                let tile = tiles.get(&position)?;
                let composition =
                    core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
                let crop =
                    core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended)?;
                Some((position, crop))
            })
            .expect("worldgen curated a site whose basket the tended rung can commit to")
    };
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );

    let (cost, grace) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let rung = ladder.rung(RungKey::PlantTended);
        (
            rung.build_cost(core_sim::RUNG_COST_UNSCALED)
                .expect("the tended rung builds"),
            rung.upkeep_grace_turns(),
        )
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        patch.set_ladder_position(cost * HALF_BUILT, &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
        patch.species = Some(crop);
        patch.biomass = patch.carrying_capacity * STOCKED;
    }

    // The band: gatherers, builders, and **no `agriculture` role** — which is what makes the rot real.
    let band_entity = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32((GATHERERS + BUILDERS + 8) as f32),
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
            ResidentBand,
            // **THE LEDGER IS STATED, NOT LEFT ABSENT.** An absent component resolves to one reference
            // ledger in the labor system (sized to the band's workers) and another at capture (one unit
            // of each), so the gear the sim charges and the gear the kit row publishes would be two
            // different numbers — and this arm's whole claim is that they are one.
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
            LaborAllocation {
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Forage {
                            tile: source,
                            floor: DEFAULT_ESCAPEMENT_FLOOR,
                            species: None,
                            take_species: TakeSelection::EVERYTHING,
                        },
                        workers: GATHERERS,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                    // **The builders are a band-level pool** since `docs/plan_standing_upkeep.md` §2.5,
                    // and the whole of it goes on the head of the queue below — which is this patch.
                    // **NO KIT NAMED, so the pool derives one per queue entry** — the head is a patch,
                    // so the roster answers `tillage` and the hoes are what the gear term below reads.
                    LaborAssignment {
                        target: LaborTarget::Builders,
                        workers: BUILDERS,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                ],
                build_queue: vec![core_sim::BuildQueueEntry {
                    source: core_sim::BuildSource::Patch(source),
                    declared: core_sim::BuildJob::Rung(Improvement::Cultivate),
                    kit: None,
                }],
                ..Default::default()
            },
        ))
        .id();

    // Walk past the rung's own grace in the real stage order, so the neglect counter is spent and the
    // published rot is the rung's live `meter_decay.per_turn`.
    for _ in 0..=(grace + 1) {
        app.world.run_system_once(advance_cultivation);
        app.world.run_system_once(advance_labor_allocation);
    }
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let row = snapshot
        .forage_patches
        .iter()
        .find(|patch| patch.x == source.x && patch.y == source.y)
        .expect("the fixture patch is on the wire");

    assert!(
        row.meter_rot_per_turn > 0.0,
        "fixture: the grace must be spent and the keeping empty, or the rot term is untested \
         (published {})",
        row.meter_rot_per_turn
    );
    assert!(
        row.build_turns_remaining > 1,
        "fixture: the sim must quote a multi-turn count, or `ceil` and the divisor are untested \
         (published {})",
        row.build_turns_remaining
    );

    // **THE GEAR PAIR, off the band's own `tillage` row** — the kit the pool derived for this entry,
    // which the client reads the same way it reads a hunt kit's. Publishing the branch beside the
    // worth is what lets it: a `hurdling` row states the same `+0.5` and is worth nothing here.
    let band = snapshot
        .populations
        .iter()
        .find(|population| population.entity == band_entity.to_bits())
        .expect("the building band is on the wire");
    let tiers = band
        .kit_tiers
        .iter()
        .find(|kit| kit.kit_id == TILLAGE_KIT)
        .expect("the band publishes a tier row per roster kit");
    assert_eq!(
        tiers.build_work_branch, "plant",
        "the tillage kit's build gear must publish the web it serves, or the client cannot tell it \
         apart from hurdles"
    );
    assert!(
        tiers.build_work_per_worker > 0.0 && tiers.build_work_saturating_crew > 0,
        "fixture: the hoes must publish a live gear pair, or the term under test is inert \
         (per-worker {} crew {})",
        tiers.build_work_per_worker,
        tiers.build_work_saturating_crew
    );
    let gear = client_gear_term(
        tiers.build_work_per_worker,
        tiers.build_work_saturating_crew,
        BUILDERS,
    );
    assert!(
        (gear - row.build_work_from_gear).abs() < 1e-3,
        "the tillage row's `min({BUILDERS}, {}) × {}` must equal what the sim stamped on the patch: \
         {gear} vs {}",
        tiers.build_work_saturating_crew,
        tiers.build_work_per_worker,
        row.build_work_from_gear
    );

    let quoted = client_turns_estimate(
        row.cultivation_work_cost,
        row.cultivation_work_done,
        tiers.build_work_per_worker,
        tiers.build_work_saturating_crew,
        row.build_work_per_worker_turn,
        BUILDERS,
        row.meter_rot_per_turn,
    );
    assert_eq!(
        quoted,
        u32::try_from(row.build_turns_remaining).ok(),
        "the client's closed form must net the published rot and land on the sim's own answer: \
         cost {} done {} gear {gear} per-worker-turn {} builders {BUILDERS} rot {}",
        row.cultivation_work_cost,
        row.cultivation_work_done,
        row.build_work_per_worker_turn,
        row.meter_rot_per_turn,
    );

    // **AND THE ROT IS LOAD-BEARING IN THAT EQUALITY** — a form that ignored it would land somewhere
    // else, which is what makes the assertion above a guard rather than a coincidence.
    assert_ne!(
        client_turns_estimate(
            row.cultivation_work_cost,
            row.cultivation_work_done,
            tiers.build_work_per_worker,
            tiers.build_work_saturating_crew,
            row.build_work_per_worker_turn,
            BUILDERS,
            core_sim::NO_UPKEEP_DECAY,
        ),
        u32::try_from(row.build_turns_remaining).ok(),
        "a form that dropped the rot term must NOT reproduce the sim's answer, or this arm proves \
         nothing about the term it exists for"
    );
}
