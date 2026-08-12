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
//!                 / (w × buildWorkPerWorkerTurn × floor / foodPeak))
//! ```
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

use core_sim::{
    advance_herds, advance_labor_allocation, build_headless_app, recapture_snapshot_in_place,
    scalar_from_f32, scalar_one, scalar_zero, BandEquipment, DiscoveryProgressLedger,
    EquipmentConfig, FactionId, FaunaConfigHandle, GenerationId, HerdRegistry, Improvement,
    LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, SimulationConfig, SnapshotHistory, TileRegistry, HERDING_DISCOVERY_ID,
    MSY_BIOMASS_FRACTION,
};

/// **The species the fixture reshapes its herd into** — a `pen`-ceiling row, so `can_domesticate()`
/// holds and a `Tame` actually accrues. Named rather than "whatever worldgen spawned" because a
/// `wild`-ceiling herd would make the build silently refuse and the equality below trivially true of
/// two zeroes.
const TAMEABLE_SPECIES: &str = "Wild Boar";

/// **The kit the keepers are sent out with** — the one shipped roster entry declaring
/// `EquipmentStat::BuildWork`, so the gear term under test is non-zero rather than a term the
/// arithmetic never exercises.
const HANDLING_KIT: &str = "husbandry";

/// **The escapement floor the crew holds, and it is deliberately NOT the food peak.**
/// `learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION` is `×1.0` at the peak, where a wrong
/// `floor / foodPeak` term in the client's form would cancel and the equality would pass on
/// arithmetic that is wrong everywhere else. Below the peak the herd also stands well above the
/// floor, so the crew is genuinely working the source.
const BUILDER_FLOOR: f32 = 0.30;

/// Keepers on the herd — enough that the build is not one turn (a one-turn build cannot tell
/// `ceil` from `round`) and enough that the flock is not under-contained.
const KEEPERS: u32 = 6;

/// A herd big enough that its stock stands far above [`BUILDER_FLOOR`], so the crew is working the
/// source every turn of the fixture.
const TEST_CAPACITY: f32 = 4_000.0;

/// **The client's own food-peak constant**, restated here because a client holds no config. It must
/// equal the sim's [`MSY_BIOMASS_FRACTION`] — asserted below, since the two are separate literals in
/// separate languages and nothing else pins them together.
const CLIENT_FLOOR_FOOD_PEAK: f32 = 0.5;

/// A headless world whose one game herd has been reshaped into a tameable species standing at
/// [`TEST_CAPACITY`], with the viewer able to see it.
fn world_with_a_tameable_herd() -> (App, String, UVec2) {
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
        .species_by_display(TAMEABLE_SPECIES)
        .expect("the shipped roster carries the fixture species")
        .clone();
    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd the id came from");
        herd.species = TAMEABLE_SPECIES.to_string();
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

/// A resident band of [`KEEPERS`] taming `fauna_id` on the [`HANDLING_KIT`], holding `gear`.
fn spawn_taming_keepers(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    gear: GearHeld,
) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
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
                working: scalar_from_f32(KEEPERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
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
                    workers: KEEPERS,
                    improvement: Some(Improvement::Tame),
                    kit: Some(kit),
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
    workers: u32,
    floor: f32,
) -> Option<u32> {
    let gear = client_gear_term(build_work_per_worker, build_work_saturating_crew, workers);
    let work_per_turn =
        workers as f32 * build_work_per_worker_turn * (floor / CLIENT_FLOOR_FOOD_PEAK);
    if work_per_turn <= 0.0 {
        return None;
    }
    let remaining = work_cost - work_done - gear;
    if remaining <= 0.0 {
        return None;
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
        let keepers = spawn_taming_keepers(&mut app, pos, &id, gear);
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
        if saturating_crew < KEEPERS {
            saw_saturated = true;
        } else {
            saw_linear = true;
        }

        // (1) **THE GEAR TERM** — the two kit-row terms, evaluated at the committed crew, must equal
        // what the sim stamped on the source from that same crew's coverage.
        let gear = client_gear_term(build_work_per_worker, saturating_crew, KEEPERS);
        assert_eq!(
            gear, herd.build_work_from_gear,
            "the kit row's `min({KEEPERS}, {saturating_crew}) × {build_work_per_worker}` must equal \
             the contribution the sim resolved for that same crew"
        );

        // (2) **THE TURN COUNT** — the whole form against the sim's own answer.
        let quoted = client_turns_estimate(
            herd.tame_work_cost,
            herd.tame_work_done,
            build_work_per_worker,
            saturating_crew,
            herd.build_work_per_worker_turn,
            KEEPERS,
            BUILDER_FLOOR,
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
             per-worker-turn {} staffed {KEEPERS} floor {BUILDER_FLOOR}",
            herd.tame_work_cost, herd.tame_work_done, herd.build_work_per_worker_turn,
        );
    }

    assert!(
        saw_saturated,
        "fixture: one arm must hold LESS gear than it has hands, or the saturation never binds and \
         a form without the `min` would pass"
    );
    assert!(
        saw_linear,
        "fixture: one arm must hold a unit per hand, or saturation is the only regime under test"
    );
}

/// **The client's food-peak constant is the sim's, and nothing else says so.** The form divides the
/// assignment's floor by it (`learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION`), and the
/// client holds its own literal because no config crosses the wire — so a retune of the sim's peak
/// would silently re-scale every turn estimate the sheet draws.
#[test]
fn the_clients_food_peak_constant_is_the_sims_own() {
    assert_eq!(
        CLIENT_FLOOR_FOOD_PEAK, MSY_BIOMASS_FRACTION,
        "the client's SourceForecast.FLOOR_FOOD_PEAK and the sim's MSY_BIOMASS_FRACTION are \
         separate literals in separate languages; a retune of one must move the other"
    );
}
