//! **THE BUILD COUNTDOWN'S FIVE STATES, ON THE WIRE** (`docs/plan_standing_upkeep.md` §2.4/§4.6b).
//!
//! `buildTurnsRemaining` shipped as one sentinel covering several different situations, so the tile
//! card and the herd drawer — the two surfaces a player reads every turn — rendered **no line at
//! all** for all of them. Most of those situations are not an absence of information:
//!
//! | published | meaning |
//! |---|---|
//! | `>= 0` | a real finish date, **chained** behind everything above it in the band's queue |
//! | `BUILD_METER_HOLDS` (`-2`) | a real, priced build banking **exactly** what its meter is bleeding — the ground stands still |
//! | `BUILD_METER_ROTS` (`-3`) | the same build banking **less** than the bleed — work already bought is being lost |
//! | `BUILD_QUEUE_BLOCKED` (`-4`) | the band's builders are **staffed and standing on this entry** and its own gate refuses it — nothing banks, and nothing behind it moves |
//! | `NO_BUILD_TURNS_ESTIMATE` (`-1`) | the sim looked and there is genuinely no answer — nothing queued here, or a gate refusing a *waiting* entry |
//! | `BUILD_NOT_YET_ESTIMATED` (`-5`) | the sim has not looked — the entry was queued since the last turn resolved. Driven in `build_queue.rs` on both webs |
//!
//! **THE ROT IS THE DENOMINATOR** (`docs/plan_standing_upkeep.md` §4.6a). A build crew supplies
//! nothing toward the maintenance rate — the band's keeping pool owes that for every meter carrying
//! work, at any fullness — so what a build can fail to out-run is the **rot**: what the keeping
//! failed to cover, bleeding off the very meter the builders are raising. **That is actionable and
//! permanent** — the player staffs the keeping — where the no-answer state is transient. Folding it
//! into `-1` made it visible only to a compose sheet that redid the comparison itself.
//!
//! **AND "NEVER FINISHES" IS ITSELF TWO PIECES OF NEWS.** Holding wastes a crew's turn; rotting
//! destroys progress the player has already bought, and the client renders them yellow and red
//! against a real count's green. One `-2` for both made the worse of them unspeakable.
//!
//! **BOTH NON-FINISHING ARMS ARE STAGED BY THE KEEPING POOL AND NO BUILDERS, ON THE SHIPPED
//! LADDER.** The boundary is *work banked*, not hands on the job: a meter carrying work has promised
//! something — the player paid for it — so a half-built meter nobody is building is exactly *the
//! meter holds* (the keeping covers it) or *the meter is losing ground* (it does not). Both plant rot
//! rates are below one worker-turn, so a **staffed** plant build always out-runs its own rot; staging
//! these arms with a small build crew, or with an invented `meter_decay`, would be bending a fixture
//! until the assertion passed rather than describing a state the game reaches.
//!
//! **`-2` IS NOT ONLY A FAILURE.** With no builders and the keeping met it is the player **parking**
//! a half-built improvement — held indefinitely, at no risk, which `docs/plan_standing_upkeep.md`
//! §2.4 exists to make possible. `-3` is the unambiguously bad one.
//!
//! **Asserted on the ENCODED envelope, never on the in-process value**, the discipline
//! `source_crews_on_the_wire.rs` follows: a field can be right in the capture and wrong in the
//! buffer, and the schema/codec/reader path is what a client actually sees.
//!
//! **All four states run on ONE fixture and are asserted pairwise distinct**, because a test that
//! exercised only three of them would pass with the new sentinel wired to the wrong branch. **The
//! EXACT-EQUALITY arm is the one that matters**: a fixture staffed only *below* the rate passes with
//! holding and rotting wired to the same branch, since both are then reached the same way.
//!
//! **And the SECOND TERM of the same quote lives here too** — `cultivationUpkeepDemand`, the rate
//! the rung being quoted costs to hold. `upkeepDemand` beside it resolves through the **at-risk**
//! rung and is therefore `0` on the unstarted source a compose sheet is by definition looking at, so
//! a sheet netting its quote against that promised `workCost / crew` turns for a build that never
//! moves. The fixture that matters is the **no-progress** one: mid-build the two fields name the
//! same rung and agree, so a mid-build fixture would pass with the gap still open.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::TakeSelection;
use core_sim::{
    build_test_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    FactionId, ForageRegistry, GenerationId, LaborAllocation, LaborAssignment, LaborTarget,
    LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort, ResidentBand, RungKey,
    SnapshotHistory, SourcePriority, StartingUnit, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};
use sim_schema::{
    BUILD_METER_HOLDS, BUILD_METER_ROTS, BUILD_NOT_YET_ESTIMATED, BUILD_QUEUE_BLOCKED,
    NO_BUILD_TURNS_ESTIMATE,
};

/// **A GATHERING SITE THE CULTIVATE GATE ADMITS.** Every plant rung requires one
/// (`RungSiteRequirement::requires_gathering_site`), and a refused gate publishes *no estimate* for a
/// reason that has nothing to do with staffing — which would make three of this test's four arms the
/// same state for the wrong reason.
fn a_cultivable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let candidates: Vec<UVec2> = app
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
    candidates
        .into_iter()
        .find(|position| {
            if registry.patch(*position).is_none() {
                return false;
            }
            // **And the ground must grow something the tended rung can commit to.** The Cultivate
            // gate needs a committed species, so a basket whose whole `cultivation_ceiling` is wild
            // would make `eligible` false for a reason that is not the staffing under test.
            let Some(tile) = tiles.get(position) else {
                return false;
            };
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
            core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended).is_some()
        })
        .expect("worldgen curated a gathering site whose basket the tended rung can commit to")
}

/// **The hands a real countdown is quoted at** — more than one, so the job takes several turns and
/// the count is a count rather than `1`.
const A_MULTI_TURN_CREW: u32 = 2;

/// **NOBODY ON THE BUILD** — the staffing both non-finishing arms run at, because the boundary is
/// *work banked* rather than hands: a half-built meter has promised something and answers for itself.
const NOBODY_BUILDING: u32 = core_sim::NO_CREW_ON_THIS_ACTIVITY;

/// A headless world whose `SOURCE` tile carries a patch, with `builders` on a running `Cultivate`.
/// `builders == 0` leaves the source unstaffed for the build.
fn world_with_a_cultivate_staffed_at(builders: u32) -> (App, UVec2) {
    world_with_a_patch(builders, HALF_BUILT)
}

/// **Put `keepers` on the band's `agriculture` role** — the fixture's stand-in for
/// `assign_labor <faction> <band> agriculture <workers>`, which is the only thing that differs
/// between the holding arm and the rotting one. The band is sized to afford the row, or
/// `LaborAllocation::normalize` trims the very role under measurement.
fn staff_the_keeping(app: &mut App, source: UVec2, keepers: u32) {
    // **The band that holds THIS patch**, not the first one the query hands back: worldgen's own
    // starting units are bands too, and staffing one of those would leave the fixture's own
    // `agriculture` pool empty while the test believed it was full.
    let band = {
        let mut query = app
            .world
            .query::<(bevy::ecs::entity::Entity, &LaborAllocation)>();
        query
            .iter(&app.world)
            .find(|(_, allocation)| {
                allocation.assignments.iter().any(|assignment| {
                    matches!(assignment.target, LaborTarget::Forage { tile, .. } if tile == source)
                })
            })
            .map(|(entity, _)| entity)
            .expect("the fixture's band holds the source patch")
    };
    let headroom = {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        let headroom = allocation.assigned_total() + keepers;
        allocation.set_assignment(LaborTarget::Agriculture, keepers, headroom, None);
        headroom
    };
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort");
    if cohort.working.to_f32() < headroom as f32 {
        cohort.working = scalar_from_f32(headroom as f32);
    }
}

/// **`plant:tended`'s keeping demand in whole hands** — what the holding arm staffs, read straight
/// off the shipped ladder so a retune moves the fixture with the game, and **at the fixture tile's
/// own tender-load**, since the rung quotes its rate per load (`forage::patch_tender_loads`).
fn keeping_demand_in_hands(app: &App, coord: UVec2) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(tender_loads_at(app, coord))
}

/// **THE MEASURE BOTH PLANT RUNGS QUOTE THEIR UPKEEP RATE PER** — this tile's own `K` over
/// `forage.cultivation.capacity_per_tender` (`forage::patch_tender_loads`). Resolved off the ground
/// worldgen actually handed the fixture, never assumed to be the reference tile's one load: these
/// fixtures pick whichever cultivable site the map offers, and its `K` is whatever its biome says.
fn tender_loads_at(app: &App, coord: UVec2) -> f32 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("the fixture tile is on the map");
    let ground = app
        .world
        .get::<core_sim::Tile>(tile_entity)
        .expect("the fixture tile carries a Tile");
    core_sim::patch_tender_loads(
        core_sim::tile_forage_capacity(&labor.forage, ground),
        &labor.forage,
    )
}

/// Turns enough to outlast `plant:tended`'s own grace, so the rotting arm is genuinely bleeding
/// rather than being forgiven. Both arms run the same span, so they differ in the keeping alone.
fn turns_past_the_grace(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_grace_turns()
        + 2
}

/// **The meter of a patch NOBODY HAS STARTED** — the state a compose sheet is by definition looking
/// at, and the only state in which the two upkeep readouts can disagree.
const NOTHING_BANKED: f32 = core_sim::RUNG_UNSTARTED;

/// **A meter genuinely mid-build**, as a fraction of the rung's cost — enough that the patch is
/// *building* by derivation, so the at-risk rung and the quoted rung are the same rung.
const HALF_BUILT: f32 = 0.5;

/// A headless world whose `SOURCE` tile carries a patch with `banked × cost` on its tended meter and
/// `builders` on the build allocation, with the Cultivation gate **open** and a gatherer beside the
/// build.
fn world_with_a_patch(builders: u32, banked: f32) -> (App, UVec2) {
    world_with_a_patch_knowing(builders, banked, THE_GATE_IS_OPEN, A_GATHERER)
}

/// **The gathering crew beside the build** — a real take, so the row is a worked source. It is
/// deliberately a *parameter*, because the abandoned arm below has to take it to zero.
const A_GATHERER: u32 = 1;

/// **Nobody gathering it either** — a patch the band merely **holds**
/// (`docs/plan_standing_upkeep.md` §2.2: take, build and keeping are separate allocations).
const NOBODY_GATHERING: u32 = 0;

/// **The faction knows Cultivation** — every arm but the no-answer one, so `eligible` turns on the
/// staffing and the meter rather than on a gate.
const THE_GATE_IS_OPEN: bool = true;

/// [`world_with_a_patch`] with the rung's own knowledge gate stated, because a **refused gate** is
/// the one state that genuinely has no answer however much work is banked and however many hands are
/// on it.
fn world_with_a_patch_knowing(
    builders: u32,
    banked: f32,
    knows_cultivation: bool,
    gatherers: u32,
) -> (App, UVec2) {
    let mut app = build_test_app();
    // One `update()` runs the Startup worldgen chain, which seeds the tile registry and the patches.
    app.update();
    let source = a_cultivable_site(&mut app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    if knows_cultivation {
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(
                FactionId(0),
                core_sim::CULTIVATION_DISCOVERY_ID,
                scalar_one(),
            );
    }

    // **The meter the arms are quoted from.** At [`HALF_BUILT`] the source is genuinely *building* by
    // derivation, so the countdown arms are about the staffing rather than about whether a build
    // exists; at [`NOTHING_BANKED`] it is unstarted, which is the pre-commit state.
    let cost = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the tended rung builds");
    let crop = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<&core_sim::Tile>();
        let ground = query
            .iter(&app.world)
            .find(|tile| tile.position == source)
            .expect("the source tile exists")
            .clone();
        let composition =
            core_sim::tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
        core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended)
            .expect("the site was chosen for having one")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        // **There is no stamped cost left to set** — a rung's boundaries come from live config
        // now, and "unstarted" is simply a position of zero.
        patch.set_ladder_position(cost * banked, &core_sim::LadderConfig::builtin());
        // ...and an unstarted patch is unowned, for the same reason: the owner lands with the work.
        patch.owner = (banked > NOTHING_BANKED).then_some(FactionId(0));
        // **Stock standing above the floor**, or `crew_is_working_the_source` is false and the arm
        // is measuring an empty patch rather than a staffing.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        // **And the ground is already committed to its crop**, which a patch with progress on it
        // always is in play: the species is stamped on the first worked turn. A fixture that writes
        // the meter directly has to write it too, or the Cultivate gate refuses for a reason that is
        // not the staffing under test. An **unstarted** patch is uncommitted, and the quote resolves
        // its crop off the tile's own basket.
        patch.species = (banked > NOTHING_BANKED).then_some(crop);
    }

    spawn_the_holding_band(
        &mut app,
        tile,
        source,
        gatherers,
        builders,
        // **THE DECLARATION, and only where there is something to declare it on.** A patch with
        // **nothing banked** is exactly the state a compose sheet is looking at — the player has
        // not queued it yet — and that is what makes the wire publish a *projection* rather than
        // a running countdown. A half-built meter is a build in flight and carries its entry.
        (banked > core_sim::RUNG_UNSTARTED).then_some(core_sim::Improvement::Cultivate),
    );
    (app, source)
}

/// ⛔ **THE PATCH ROW'S FOUR KEEPING TERMS ARE ONE BILL — INCLUDING THE HEAD COUNT.**
///
/// `snapshot.fbs` states the identity `upkeepWorkersNeeded == ceil(upkeepDemand /
/// PER_WORKER_OUTPUT)` and tells the client to do no arithmetic of its own, so the two must come
/// off the same seam. `upkeepDemand` and `upkeepShortfall` ship the **stamped** bill — the demand
/// the keepers were handed, taken before the turn's build accrual — while the head count was `ceil`
/// of the **live** interpolated demand, which by capture has already risen. A patch mid-build then
/// published *"wants 3, you have 2"* beside a shortfall of zero.
///
/// **Swept across the rung's whole span**, because a `ceil` hides a disagreement of less than one
/// worker: only the fullnesses where the two readings straddle a whole hand can show it, and which
/// those are is a property of the shipped rates. The sweep needs no fixture to be tuned onto the
/// boundary — it walks over it.
#[test]
fn the_patch_rows_keeping_head_count_is_the_ceil_of_the_bill_it_publishes() {
    /// Steps across the rung, fine enough that some step lands inside the one-worker window the
    /// turn's own accrual opens.
    const STEPS: u32 = 20;

    /// The pool raising the Cultivate — enough that the meter climbs a visible amount in the turn
    /// between the stamp and the capture, which is the whole gap under test.
    const BUILDERS: u32 = A_MULTI_TURN_CREW;

    let mut billed_at_all = false;
    for step in 1..STEPS {
        let banked = step as f32 / STEPS as f32;
        let (mut app, source) = world_with_a_patch(BUILDERS, banked);
        resolve_a_turn(&mut app, source);
        let (demand, needed) = published_patch_field(&app, source, |patch| {
            (patch.upkeepDemand(), patch.upkeepWorkersNeeded())
        });
        billed_at_all |= demand > 0.0;
        assert_eq!(
            needed,
            (demand / core_sim::PER_WORKER_OUTPUT).ceil() as u32,
            "at {banked} of the rung the row must state one bill: {needed} hands published against \
             a demand of {demand}"
        );
    }
    assert!(
        billed_at_all,
        "fixture: the rung must actually bill something somewhere on its span, or the identity is \
         a comparison of zeroes"
    );
}

/// **THE BAND THAT HOLDS THE SOURCE** — one gathering row, one builders pool, and at most one queue
/// entry. Shared by every fixture in this file, because the *only* things that differ between them
/// are the three the caller states here.
fn spawn_the_holding_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    source: UVec2,
    gatherers: u32,
    builders: u32,
    declared: Option<core_sim::Improvement>,
) {
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 30,
            children: scalar_zero(),
            working: scalar_from_f32((gatherers + builders + 8) as f32),
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
        ResidentBand,
        LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: source,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: gatherers,
                    kit: None,
                    priority: SourcePriority::default(),
                },
                // **The builders are a band-level POOL** (`docs/plan_standing_upkeep.md` §2.5), and
                // the whole of it goes on the head of the queue below. `builders == 0` is the
                // unstaffed arm — a role row still stands at zero, which is how a player says
                // *stop building* without withdrawing what they declared.
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: builders,
                    kit: None,
                    priority: SourcePriority::default(),
                },
            ],
            build_queue: declared
                .map(|declared| core_sim::BuildQueueEntry {
                    source: core_sim::BuildSource::Patch(source),
                    declared: core_sim::BuildJob::Rung(declared),
                    kit: None,
                })
                .into_iter()
                .collect(),
            ..Default::default()
        },
    ));
}

/// **A WATERED GATHERING SITE WHOSE BASKET CAN CLIMB TO `plant:field`** — [`a_cultivable_site`]'s
/// rung-3 twin, and it has to be its own scan: the Field rung adds fresh water to the site rule and
/// narrows the crop test to plants that can be sown, so a merely cultivable site would refuse the
/// `Sow` gate and publish *no estimate* for a reason that is not the one under test.
///
/// Swept in a totally-ordered `(y, x)` pass, never map iteration order.
fn a_sowable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let config = app.world.resource::<core_sim::SimulationConfig>();
    let map_seed = config.map_seed;
    let wrap = config.map_topology.wrap_horizontal;
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
        let mut query = app.world.query::<&core_sim::Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    for y in 0..height {
        for x in 0..width {
            let coord = UVec2::new(x, y);
            let Some(ground) = tiles.get(&coord) else {
                continue;
            };
            if app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .is_none()
            {
                continue;
            }
            let fresh_water =
                core_sim::tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
                    tiles.get(&neighbor).map(|tile| tile.terrain_tags)
                });
            let refusal = core_sim::rung_site_refusal(
                ladder.rung(RungKey::PlantField),
                ground,
                &labor.forage,
                app.world
                    .resource::<core_sim::FoodSiteRegistry>()
                    .is_site(coord),
                fresh_water,
            );
            if refusal.is_some() {
                continue;
            }
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
            if core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
                .is_some()
            {
                return coord;
            }
        }
    }
    panic!("the shipped map must carry sowable ground — rung 3 is unreachable without it");
}

/// ⛔ **AN ABANDONED TWO-LEG SOW PUBLISHES A METER STATE, NOT A SILENCE.**
///
/// A `sow` ordered on untended ground climbs `plant:tended` first, so a player who walks away
/// mid-first-leg has a meter carrying work sitting **below** the Field rung's own base. The boundary
/// is *work banked, not hands on the job* (`.claude/rules/core_sim/intensification.md`), so that
/// meter has promised something and owes the player `-2` or `-3` — but the quote read the Field
/// rung's clamped share of the position, which is `0` for the whole of that leg, and `-1` renders as
/// **no line at all** on the tile card.
///
/// Read off the encoded buffer, and staged exactly like the rotting arm above: unkept, past the
/// rung's own grace, with nobody on the pool.
#[test]
fn an_abandoned_two_leg_sow_publishes_the_meter_state_rather_than_no_answer() {
    let mut app = build_test_app();
    app.update();
    let source = a_sowable_site(&mut app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    // **Both gates the climb passes through** — the entry names the Field, and its first leg is the
    // tended rung, so a fixture granting only one of them would be measuring a refusal.
    for knowledge in [
        core_sim::CULTIVATION_DISCOVERY_ID,
        core_sim::SEED_SELECTION_DISCOVERY_ID,
    ] {
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(FactionId(0), knowledge, scalar_one());
    }
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let tended_cost = ladder
        .rung(RungKey::PlantTended)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the tended rung builds");
    let (field_base, _) = core_sim::plant_rung_span(RungKey::PlantField, &ladder);
    let crop = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<&core_sim::Tile>();
        let ground = query
            .iter(&app.world)
            .find(|tile| tile.position == source)
            .expect("the source tile exists")
            .clone();
        let composition =
            core_sim::tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
        core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
            .expect("the site was chosen for having one")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        // **Mid the FIRST leg** — work banked, and every unit of it below the Field's own base,
        // which is the span the per-rung reading cannot see.
        patch.set_ladder_position(tended_cost * HALF_BUILT, &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        patch.species = Some(crop);
        assert!(
            patch.ladder_position() > core_sim::RUNG_UNSTARTED
                && patch.ladder_position() < field_base,
            "fixture: the abandoned sow stands on its first leg, got {}",
            patch.ladder_position()
        );
    }
    spawn_the_holding_band(
        &mut app,
        tile,
        source,
        A_GATHERER,
        NOBODY_BUILDING,
        Some(core_sim::Improvement::Sow),
    );

    let span = turns_past_the_grace(&app);
    let mut published = NO_BUILD_TURNS_ESTIMATE;
    for _ in 0..span {
        published = resolve_a_turn(&mut app, source);
    }
    assert_eq!(
        published, BUILD_METER_ROTS,
        "a two-leg sow abandoned on its first leg is losing bought work and must say so — {published} \
         is the silence the sentinel exists to replace"
    );
}

/// Well above the escapement floor, so the crew is genuinely working the source every turn.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// **One field of the fixture patch's row, read out of the ENCODED buffer** — the artifact a client
/// parses, rather than the state struct the capture built. The row borrows the buffer, so the read
/// happens inside.
fn published_patch_field<T>(
    app: &App,
    source: UVec2,
    read: impl Fn(&shadow_scale_flatbuffers::generated::shadow_scale::sim::ForagePatchState<'_>) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == source.x && patch.y() == source.y)
        .expect("the fixture patch is on the wire");
    read(&row)
}

/// **The patch row's `buildTurnsRemaining`, off the encoded buffer.**
/// ⛔ **A HOE SPEEDS A CULTIVATE AND HURDLES DO NOTHING FOR ONE — and the pool picks the tool off
/// the ENTRY, not off a stored id.**
///
/// Four arms on one fixture, read off the encoded patch row's `buildWorkFromGear`, which is the
/// number the sim actually struck the bar with:
///
/// | the `builders` row says | the pool works with | why |
/// |---|---|---|
/// | nothing | `tillage` | an absent kit means *derive per entry*, and the head is a patch |
/// | `tillage` | `tillage` | an explicit choice wins, and here it agrees |
/// | `hurdling` | `hurdling`, worth **nothing** | hurdles are animal-handling gear; `build_work` names its web |
/// | `none` | nothing | going bare is a real selection, and it must not fall back to the derivation |
///
/// **The first arm is the liveness half and it is not optional**: a branch filter that zeroed
/// *everything* would pass arms three and four on its own, and a derivation that ignored the entry
/// would pass arm two. Every arm is compared against the *same* fixture, so the only thing that
/// differs between them is the kit.
#[test]
fn a_plant_build_is_geared_by_the_hoe_and_by_nothing_else() {
    /// The pool raising the Cultivate. More than one, so a per-worker sum is visible as a sum.
    const BUILDERS: u32 = 2;

    let published = |kit_id: Option<&str>| -> f32 {
        let (mut app, source) = world_with_a_patch(BUILDERS, HALF_BUILT);
        if let Some(kit_id) = kit_id {
            let kit = core_sim::EquipmentConfig::builtin()
                .kit(kit_id)
                .unwrap_or_else(|| panic!("the shipped roster carries '{kit_id}'"));
            // **The FIXTURE band, found by its QUEUE ENTRY** — the start profile's own band is in
            // this world too, and re-kitting that one would leave the measurement untouched. The
            // kit rides the entry rather than the `builders` row, which carries none at all since
            // `docs/plan_standing_upkeep.md` §4.7a ②.
            let mut query = app.world.query::<&mut LaborAllocation>();
            let mut found = false;
            for mut allocation in query.iter_mut(&mut app.world) {
                for entry in allocation.build_queue.iter_mut() {
                    entry.kit = Some(kit.clone());
                    found = true;
                }
            }
            assert!(found, "the fixture band carries a queue entry");
        }
        core_sim::run_turn(&mut app);
        recapture_snapshot_in_place(&mut app.world);
        published_patch_field(&app, source, |patch| patch.buildWorkFromGear())
    };

    let derived = published(None);
    assert!(
        derived > core_sim::NO_BUILD_GEAR,
        "**LIVENESS**: an entry naming no kit derives the plant web's own, so a Cultivate is \
         geared — got {derived}"
    );
    assert_eq!(
        published(Some("tillage")),
        derived,
        "naming the kit the derivation would have picked changes nothing"
    );
    assert_eq!(
        published(Some("hurdling")),
        core_sim::NO_BUILD_GEAR,
        "hurdles are animal-handling gear and take NOTHING off a Cultivate — the branch qualifier's \
         whole job"
    );
    assert_eq!(
        published(Some("none")),
        core_sim::NO_BUILD_GEAR,
        "going out bare is a real selection and must not fall back to the derived kit"
    );
}

/// **WHAT THE POOL'S DERIVED KIT TAKES OFF A PLANT BUILD AT `builders` HANDS** — the client's own
/// gear term, `min(crew, buildWorkSaturatingCrew) × buildWorkPerWorker`, off the band's `tillage`
/// row. `build_turns_closed_form.rs` is where that form is pinned against the sim; here it is only
/// the number this arm's quote has to net.
fn published_tillage_gear(app: &App, builders: u32) -> f32 {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let populations = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the band list");
    let row = populations
        .iter()
        .find_map(|population| {
            population.kitTiers()?.iter().find(|kit| {
                kit.kitId().is_some_and(|id| id == TILLAGE_KIT)
                    && kit.buildWorkPerWorker() > core_sim::NO_BUILD_GEAR
            })
        })
        .expect("some band publishes a live tillage tier row");
    assert_eq!(
        row.buildWorkBranch(),
        Some("plant"),
        "the tillage kit's build gear must publish the web it serves"
    );
    builders.min(row.buildWorkSaturatingCrew()) as f32 * row.buildWorkPerWorker()
}

/// The plant web's builders kit — hoes.
const TILLAGE_KIT: &str = "tillage";

fn published_build_turns(app: &App, source: UVec2) -> i32 {
    published_patch_field(app, source, |patch| patch.buildTurnsRemaining())
}

/// Run one turn and republish, so the countdown on the wire is the one the turn just stamped.
fn resolve_a_turn(app: &mut App, source: UVec2) -> i32 {
    core_sim::run_turn(app);
    recapture_snapshot_in_place(&mut app.world);
    published_build_turns(app, source)
}

/// **THE FIVE STATES, PAIRWISE DISTINCT, ON ONE FIXTURE.**
#[test]
fn the_build_countdown_publishes_five_distinct_states_on_the_wire() {
    // (1) **A staffed build on the SHIPPED ladder publishes a real count** — and it does so with
    // nobody on the keeping, which is the §4.6a headline: the rate is not the builders' bill.
    let (mut app, source) = world_with_a_cultivate_staffed_at(A_MULTI_TURN_CREW);
    let counted = resolve_a_turn(&mut app, source);
    assert!(
        counted >= 0,
        "a staffed build is quoted a finish date whatever its keeping, got {counted}"
    );

    // (2) **A HALF-BUILT METER NOBODY IS BUILDING, WITH THE KEEPING MET, PUBLISHES HOLDING** — the
    // ground stands still: nothing is gained and nothing is lost, and it stays that way for as long
    // as the player leaves it. That is a **parked improvement**, not a failure.
    //
    // **THIS IS THE ARM THE SPLIT LIVES OR DIES ON.** Rotting is reached by a `< 0` comparison and
    // holding by falling through it, so a suite staged only *below* the line would pass with both
    // wired to the same branch. Pinning the exact equality — a rot of exactly zero against a build of
    // exactly zero — is what makes them two answers.
    let (mut app, source) = world_with_a_cultivate_staffed_at(NOBODY_BUILDING);
    let keepers = keeping_demand_in_hands(&app, source);
    assert!(
        keepers > NOBODY_BUILDING,
        "fixture: the rung must cost something to hold, or both arms are the same arm"
    );
    staff_the_keeping(&mut app, source, keepers);
    let span = turns_past_the_grace(&app);
    let mut holding = NO_BUILD_TURNS_ESTIMATE;
    for _ in 0..span {
        holding = resolve_a_turn(&mut app, source);
    }
    assert_eq!(
        holding, BUILD_METER_HOLDS,
        "a half-built meter the keeping covers holds where it is, indefinitely, and says so"
    );

    // (3) **The same meter with the `agriculture` role EMPTY publishes ROTTING** — past the rung's
    // own grace the ground is going backwards, and the player is losing work already bought rather
    // than merely waiting.
    let (mut app, source) = world_with_a_cultivate_staffed_at(NOBODY_BUILDING);
    let span = turns_past_the_grace(&app);
    let mut rotting = NO_BUILD_TURNS_ESTIMATE;
    for _ in 0..span {
        rotting = resolve_a_turn(&mut app, source);
    }
    assert_eq!(
        rotting, BUILD_METER_ROTS,
        "an unkept half-built meter loses bought work — the meter goes backwards, and says so"
    );

    // (4) **A REFUSED GATE AT THE HEAD OF A STAFFED QUEUE IS BLOCKED** (`docs/plan_standing_upkeep.md`
    // §4.6b). The player has committed a pool, the pool is standing on this entry, its own gate
    // refuses it, and so nothing banks and nothing behind it moves. That is a state to say **loudly**
    // — which is exactly what `-1` cannot do, because it renders as no line at all.
    let (mut app, source) =
        world_with_a_patch_knowing(A_MULTI_TURN_CREW, HALF_BUILT, !THE_GATE_IS_OPEN, A_GATHERER);
    let blocked = resolve_a_turn(&mut app, source);
    assert_eq!(
        blocked, BUILD_QUEUE_BLOCKED,
        "the head of a staffed queue whose gate refuses it says so, rather than falling silent"
    );

    // (5) **AND THE SAME REFUSED GATE WITH NOBODY ON THE POOL IS STILL NO ESTIMATE.** The blocked
    // reading is about a **committed pool** getting nowhere; with the pool empty there is no
    // commitment to report on, and the honest answer is the absence of one.
    //
    // **The boundary MOVED off "unstaffed" for the other two arms**, which is why this one is a gate
    // rather than an empty crew alone: work already banked promises as much as a crew does, so an
    // unstaffed half-built meter whose gate is OPEN is arm (2) or arm (3). The other no-answer
    // states — a meter at zero with nobody on it, and the top of the ladder — take this branch.
    let (mut app, source) =
        world_with_a_patch_knowing(NOBODY_BUILDING, HALF_BUILT, !THE_GATE_IS_OPEN, A_GATHERER);
    let refused = resolve_a_turn(&mut app, source);
    assert_eq!(
        refused, NO_BUILD_TURNS_ESTIMATE,
        "a refused gate with nobody standing on it has promised nothing at all"
    );

    // **Pairwise distinct**, which is what stops a new sentinel being wired to the wrong branch.
    let published = [counted, holding, rotting, blocked, refused];
    for (index, left) in published.iter().enumerate() {
        for right in published.iter().skip(index + 1) {
            assert_ne!(
                left, right,
                "the five states must be five numbers on the wire: {published:?}"
            );
        }
    }
}

/// **A PATCH NOBODY IS ON AT ALL STILL ANSWERS — `-3` UNKEPT, `-2` KEPT.** The commonest rotting
/// state in the game, and the one the boundary move exists for
/// (`docs/plan_standing_upkeep.md` §4.6a).
///
/// **No gatherers, no builders, work on the meter.** Under the old boundary this was `-1` on the
/// *unstaffed* rule; it is now the sign of `build_work − rot`, which with no builders is the rot's
/// own sign — so the wire says *the ground is going backwards* to a player who walked away, and says
/// *it is being held* to one who staffed the keeping and parked the build deliberately.
///
/// **AND `crew_is_working_the_source` DOES NOT REFUSE IT**, which is the load-bearing half: that
/// predicate takes the **escapement room** (`max(0, B − floor·K)`), a fact about the *stock against
/// the assignment's floor* and not about who is standing there. A patch nobody gathers regrows toward
/// `K`, so its room is large and the gate is **open** — the plant web's abandoned meters answer
/// honestly. (The animal web's do not, and the difference is not this predicate: there the hunters
/// draw the flock to its floor and the unmet keeping suppresses regrowth, so the room really does go
/// to zero and stays there. That is an **eligibility** stall no balance term can see —
/// `.claude/rules/core_sim/husbandry.md` carries it.)
///
/// The other term of the same conjunction is `patch.species.is_some()`, so the fixture commits a
/// crop exactly as a worked patch does — an uncommitted patch would refuse for a reason that is not
/// the one under test.
#[test]
fn an_abandoned_half_built_patch_publishes_rotting_and_a_kept_one_holding() {
    // (1) **UNKEPT — nobody anywhere near it.** Past the rung's own grace, the meter is losing
    // ground and the wire says so.
    let (mut app, source) = world_with_a_patch_knowing(
        NOBODY_BUILDING,
        HALF_BUILT,
        THE_GATE_IS_OPEN,
        NOBODY_GATHERING,
    );
    let span = turns_past_the_grace(&app);
    let mut abandoned = NO_BUILD_TURNS_ESTIMATE;
    for _ in 0..span {
        abandoned = resolve_a_turn(&mut app, source);
    }
    assert_eq!(
        abandoned, BUILD_METER_ROTS,
        "a half-built patch nobody gathers, builds or keeps is losing bought work, and says so"
    );
    // …and it is the ROT that says it, not an absent crew: the term is published beside the
    // sentinel and is exactly the rung's own bleed.
    assert!(
        published_patch_field(&app, source, |patch| patch.meterRotPerTurn())
            > core_sim::NO_UPKEEP_DECAY,
        "the meter is bleeding, which is what `-3` is struck from"
    );

    // (2) **KEPT — the same patch, the same empty build, the `agriculture` role staffed.** The
    // liveness half: it holds exactly, which is a player PARKING a half-built improvement rather
    // than a failure.
    let (mut app, source) = world_with_a_patch_knowing(
        NOBODY_BUILDING,
        HALF_BUILT,
        THE_GATE_IS_OPEN,
        NOBODY_GATHERING,
    );
    let keepers = keeping_demand_in_hands(&app, source);
    staff_the_keeping(&mut app, source, keepers);
    let span = turns_past_the_grace(&app);
    let mut parked = NO_BUILD_TURNS_ESTIMATE;
    for _ in 0..span {
        parked = resolve_a_turn(&mut app, source);
    }
    assert_eq!(
        parked, BUILD_METER_HOLDS,
        "…and the same patch with its keeping staffed is held, indefinitely, at no risk"
    );
    assert_eq!(
        published_patch_field(&app, source, |patch| patch.meterRotPerTurn()),
        core_sim::NO_UPKEEP_DECAY,
        "nothing is bleeding there, which is what `-2` is struck from"
    );
}

/// **THE PUBLISHED ROT IS EXACTLY WHAT THE NEXT DECAY PASS BLEEDS.**
///
/// Logistics runs before Population, so the pass that bleeds judges the supply the *previous* turn
/// stamped. `RungDef::meter_rot` therefore advances the neglect count by one and publishes what that
/// pass will take — an exact forecast rather than an estimate, because the supply it is struck from
/// is already stamped and nothing the player does next turn can change it.
///
/// **The invariant, asserted turn after turn: `rot published at T == −(the meter's movement at
/// T+1)`.** Three arms, and the third is what makes the other two mean anything:
///
/// - **the boundary** — the last grace turn publishes the rot the *next* turn actually bleeds, and
///   `buildTurnsRemaining` reads `-3` there rather than `-2`;
/// - **the steady state** — the relation holds every turn once the bleed has started;
/// - **the rescue** — keeping staffed mid-grace publishes `0` and the following turn bleeds `0`, so
///   the forward form is shown **not** to over-warn. Without it, a form that always predicted a bleed
///   would pass both the others.
#[test]
fn the_published_rot_is_exactly_what_the_next_decay_pass_bleeds() {
    let (mut app, source) = world_with_a_patch_knowing(
        NOBODY_BUILDING,
        HALF_BUILT,
        THE_GATE_IS_OPEN,
        NOBODY_GATHERING,
    );
    let grace = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_grace_turns();
    assert!(
        grace > 0,
        "fixture: the rung must forgive something, or there is no boundary to pin"
    );

    // **`source` is a PARAMETER, not a capture** — the rescue arm below opens a second world, and a
    // closure holding the first world's tile would read a patch that is not in it.
    let work_done =
        |app: &App, source| published_patch_field(app, source, |p| p.cultivationWorkDone());
    let rot_now = |app: &App, source| published_patch_field(app, source, |p| p.meterRotPerTurn());

    // --- (1) THE BOUNDARY, and (2) the steady state, on one walk. ---------------------------------
    //
    // Every turn: take the rot this turn publishes, resolve the next turn, and assert the meter moved
    // by exactly that. `grace + 3` turns covers the forgiven span, the last grace turn (where the
    // published rot first goes positive while the meter has still not moved) and two steady ones.
    recapture_snapshot_in_place(&mut app.world);
    let mut forecast = rot_now(&app, source);
    let mut before = work_done(&app, source);
    let mut saw_a_forecast_of_a_bleed_before_the_meter_moved = false;
    for turn in 1..=(grace + 3) {
        let published = resolve_a_turn(&mut app, source);
        let now = work_done(&app, source);
        assert!(
            (before - now - forecast).abs() < 1e-4,
            "turn {turn}: the rot published last turn ({forecast}) must be exactly what this turn \
             bled ({})",
            before - now
        );
        let next = rot_now(&app, source);
        // **THE LAST GRACE TURN IS THE ARM THE FORM LIVES ON**: the meter has not moved yet, and the
        // wire already says it is about to — `-3`, not `-2`.
        if next > core_sim::NO_UPKEEP_DECAY && (before - now).abs() < 1e-4 {
            saw_a_forecast_of_a_bleed_before_the_meter_moved = true;
            assert_eq!(
                published, BUILD_METER_ROTS,
                "turn {turn}: a meter with a bleed already determined is LOSING, not holding"
            );
        }
        forecast = next;
        before = now;
    }
    assert!(
        saw_a_forecast_of_a_bleed_before_the_meter_moved,
        "fixture: the walk must cross the grace boundary, or the forward claim is untested"
    );

    // --- (3) THE RESCUE — the arm that proves it does not over-warn. -------------------------------
    //
    // Staff the keeping while the grace is still counting. The forward form must publish `0` on that
    // turn and the next turn must bleed `0` — where a form that merely always predicted a bleed would
    // have promised one that never arrives.
    let (mut app, source) = world_with_a_patch_knowing(
        NOBODY_BUILDING,
        HALF_BUILT,
        THE_GATE_IS_OPEN,
        NOBODY_GATHERING,
    );
    for _ in 1..grace.max(2) {
        resolve_a_turn(&mut app, source);
    }
    let keepers = keeping_demand_in_hands(&app, source);
    staff_the_keeping(&mut app, source, keepers);
    resolve_a_turn(&mut app, source);
    assert_eq!(
        rot_now(&app, source),
        core_sim::NO_UPKEEP_DECAY,
        "the turn the keeping is restored, nothing is forecast — the next pass will take nothing"
    );
    let before = work_done(&app, source);
    resolve_a_turn(&mut app, source);
    assert_eq!(
        work_done(&app, source),
        before,
        "…and the next turn really does bleed nothing: the forecast cannot over-warn"
    );
    assert_eq!(
        rot_now(&app, source),
        core_sim::NO_UPKEEP_DECAY,
        "…and it stays zero while the keeping holds"
    );
}

/// **THE FOUR SENTINELS ARE OUTSIDE THE RANGE A REAL COUNT LIVES IN, and are not each other** — the
/// property every reader leans on when it branches on the sign.
#[test]
fn the_five_sentinels_are_distinct_and_below_every_real_count() {
    const {
        // **Pairwise, over the whole set**, because a new sentinel wired to an existing value is the
        // exact defect the fifth one was added to undo: `-1` carrying both *"the sim looked and had
        // no number"* and *"the sim has not looked"* is what put `⚠ Stalled 0%` on a fresh build.
        let sentinels = [
            NO_BUILD_TURNS_ESTIMATE,
            BUILD_METER_HOLDS,
            BUILD_METER_ROTS,
            BUILD_QUEUE_BLOCKED,
            BUILD_NOT_YET_ESTIMATED,
        ];
        let mut left = 0;
        while left < sentinels.len() {
            assert!(sentinels[left] < 0);
            let mut right = left + 1;
            while right < sentinels.len() {
                assert!(sentinels[left] != sentinels[right]);
                right += 1;
            }
            left += 1;
        }
    }
}

/// **THE QUOTE'S SECOND TERM, ON AN UNSTARTED SOURCE** — the trap this pair of fields exists to
/// close (`docs/plan_standing_upkeep.md` §2).
///
/// `upkeepDemand` resolves through the **at-risk** rung: the newest meter carrying progress. A wild
/// patch has progress on neither, so it publishes an honest `0` — and a compose sheet that netted
/// its quote against *that* subtracted nothing, promised `workCost / crew` turns, and sent the
/// player into a build whose meter never moves because the crew is under the rung's rate.
///
/// `cultivationUpkeepDemand` is the **ladder's** rate for the rung being quoted, resolved at capture
/// whether or not a build is in flight — `workCost`'s own rule, applied to the term that eats it.
///
/// **A MID-BUILD FIXTURE WOULD PASS WITH THE BUG STILL PRESENT**, because there the at-risk rung and
/// the quoted rung are the same rung and the two fields agree. The no-progress case is the whole
/// test; the mid-build arm below is here only to pin that they do not drift apart where they must
/// agree.
#[test]
fn an_unstarted_patch_publishes_the_quoted_rungs_upkeep_where_the_billed_one_is_zero() {
    // **A crew of ONE, below the rung's rate** — the repro's staffing.
    const A_CREW_UNDER_THE_RATE: u32 = 1;
    let (mut app, source) = world_with_a_patch(A_CREW_UNDER_THE_RATE, NOTHING_BANKED);
    // **The quote is per tender-load of the ground the fixture is actually standing on**, so the
    // expectation is struck off *this* world's own tile — the rung's bare rate would be the
    // reference tile's, and `a_cultivable_site` picks whatever the map offers.
    let ladder_rate = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_demand(tender_loads_at(&app, source));
    assert!(
        ladder_rate > core_sim::NO_UPKEEP_DEMAND,
        "fixture: the tended rung must charge a rate, or the two fields cannot differ"
    );
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let billed = published_patch_field(&app, source, |patch| patch.upkeepDemand());
    let quoted = published_patch_field(&app, source, |patch| patch.cultivationUpkeepDemand());
    assert_eq!(
        billed,
        core_sim::NO_UPKEEP_DEMAND,
        "nothing is built here yet, so nothing is billed — `upkeepDemand`'s own meaning, unchanged"
    );
    assert_eq!(
        quoted, ladder_rate,
        "the rung the patch would CLIMB costs the ladder's rate to hold, quoted before the commit"
    );

    // **AND THE PROJECTION IS `work_cost / crew`, WHICH IS ISSUE #545 CLOSED** (§4.6a). One builder
    // against `plant:tended`'s demand of `2.0` really does bank 1 a turn: the `2.0` is the keeping
    // pool's bill, not a tax on the build, and on ground nobody has started there is nothing banked
    // and therefore nothing to rot. The quote used to read `-3` here — *"the meter never reaches its
    // cost"* — about a build that finishes perfectly well.
    let cost = published_patch_field(&app, source, |patch| patch.cultivationWorkCost());
    // **PLUS WHAT THE POOL'S TOOLS DELIVER, IN THE DIVISOR** (`docs/plan_standing_upkeep.md` §4.8).
    // The builders row names no kit, so the pool derives one per queue entry and the roster answers
    // `tillage` for a patch — the hoes are the plant web's build tool. Read off the band's own kit
    // row rather than stated as a literal, so retuning the tool moves the fixture with the game.
    //
    // **The gear used to sit in the NUMERATOR** (`(cost − gear) / crew`), which let a tool shrink
    // the job; it raises what a worker delivers now, so the published `cultivationWorkCost` above is
    // the whole job and the kit shortens the span by supplying more of it per turn.
    let gear = published_tillage_gear(&app, A_CREW_UNDER_THE_RATE);
    assert!(
        gear > core_sim::NO_BUILD_GEAR,
        "fixture: the derived builders kit must deliver real work on a plant build, or this arm is \
         the un-geared quote wearing a gear term's clothes (gear {gear})"
    );
    assert_eq!(
        published_build_turns(&app, source),
        (cost / (A_CREW_UNDER_THE_RATE as f32 * core_sim::PER_WORKER_OUTPUT + gear)).ceil() as i32,
        "one builder is quoted `work_cost / (crew + its gear)` turns — the rate is the keeping's \
         bill, and the tool is in the divisor"
    );
    assert_eq!(
        published_patch_field(&app, source, |patch| patch.meterRotPerTurn()),
        core_sim::NO_UPKEEP_DECAY,
        "…and nothing is banked here, so there is nothing to rot"
    );

    // **MID-BUILD THE TWO ARE STILL TWO NUMBERS, AND THAT IS THE ARC** (`docs/plan_standing_upkeep.md`
    // §2.8). The quote is what a *finished* rung costs to hold; the bill INTERPOLATES on how far up
    // the rung this source has actually been worked, so a half-built meter owes about half. They
    // used to coincide, which is precisely the defect: a patch 1% into a Cultivate was billed the
    // whole rung's rate to hold a hundredth of a thing.
    let (mut app, source) = world_with_a_patch(A_CREW_UNDER_THE_RATE, HALF_BUILT);
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);
    let billed = published_patch_field(&app, source, |patch| patch.upkeepDemand());
    let quoted = published_patch_field(&app, source, |patch| patch.cultivationUpkeepDemand());
    assert!(
        quoted > 0.0,
        "fixture: the tended rung costs something to hold, or the comparison is vacuous"
    );
    assert!(
        billed > core_sim::NO_UPKEEP_DEMAND && billed < quoted,
        "a source part-way up a rung is billed part of that rung's rate: billed {billed} against a \
         quote of {quoted}"
    );
}

/// **⛔ THE PUBLISHED `tileCapacity` IS THE GROUND, AND `carryingCapacity` IS THE GROUND PLUS THE
/// RUNG** — the split that lets a remembered hex state a capacity without stating a ladder position.
///
/// `carryingCapacity` is `tile K × the interpolated field_capacity_gain`, so on a standing Field it is
/// ~2.53× the same ground wild. The client withholds `isField` and `fieldProgress` under fog and does
/// **not** withhold the capacity, on the (now dead) grounds that a capacity is ground nobody can move
/// — so an unredacted `carryingCapacity` hands back the very rung those two redactions hide, and
/// continuously rather than as a boolean. `tileCapacity` is the fog-safe half.
///
/// **Run on a FIELD**, because that is the only rung at which the two numbers differ at all — the
/// wild arm below is the same field's honesty check and is deliberately *not* this test's fixture.
///
/// **Asserted on the ENCODED envelope**, this file's discipline: a field can be right in the capture
/// and wrong in the buffer.
#[test]
fn the_published_tile_capacity_is_the_ground_and_not_the_rung() {
    /// Ground with a stated `K`, so "the tile's own capacity" is a number this test knows rather than
    /// whatever site `a_cultivable_site` happened to pick (which is not stable run to run).
    const GROUND: sim_runtime::TerrainType = sim_runtime::TerrainType::RiverDelta;

    let (mut app, source) = world_with_a_patch(NOBODY_BUILDING, NOTHING_BANKED);
    pin_the_ground(&mut app, source, GROUND);

    // Seat a WHOLE Field, which is where the gain is fully applied.
    {
        let ladder = app.world.resource::<LadderConfigHandle>().get().clone();
        let (field_base, field_width) = core_sim::plant_rung_span(RungKey::PlantField, &ladder);
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(source).expect("the fixture patch");
        patch.set_ladder_position(field_base + field_width, &ladder);
        patch.owner = Some(FactionId(0));
        assert!(
            patch.is_field(),
            "fixture: the patch must stand on plant:field"
        );
    }
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let ground_k = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .forage
        .capacity_for(GROUND);
    let gain = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .forage
        .cultivation
        .field_capacity_gain;

    let tile_capacity = published_patch_field(&app, source, |patch| patch.tileCapacity());
    let carrying = published_patch_field(&app, source, |patch| patch.carryingCapacity());

    // **THE PRECONDITION.** Without it this test passes on any turn the Field's capacity gain
    // silently stops applying — the two numbers would agree and every assertion below would hold
    // about a mechanism that had stopped working.
    assert!(
        (tile_capacity - carrying).abs() > 1e-3,
        "PRECONDITION: on a standing Field the ground and the patch must differ, or the gain is not \
         applying and this test asserts nothing ({tile_capacity} against {carrying})"
    );
    assert!(
        (tile_capacity - ground_k).abs() < 1e-3,
        "`tileCapacity` is the tile's own biome capacity with NO rung in it: {tile_capacity} against \
         {ground_k}"
    );
    assert!(
        (carrying - tile_capacity * gain).abs() < 1e-2,
        "…and `carryingCapacity` is that same ground times the Field's own gain: {carrying} against \
         {tile_capacity} × {gain}"
    );
}

/// **THE WILD ARM: the same field is honest where the two numbers coincide.** A wild patch has bought
/// no gain, so `tileCapacity == carryingCapacity` — which is exactly why a wild patch is **useless**
/// as the Field test's fixture above: publishing `patch.carrying_capacity` into `tileCapacity` would
/// pass here. This arm pins that the field is a real reading and not a constant.
#[test]
fn a_wild_patch_publishes_the_ground_twice() {
    const GROUND: sim_runtime::TerrainType = sim_runtime::TerrainType::PrairieSteppe;

    let (mut app, source) = world_with_a_patch(NOBODY_BUILDING, NOTHING_BANKED);
    pin_the_ground(&mut app, source, GROUND);
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let ground_k = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .forage
        .capacity_for(GROUND);
    let tile_capacity = published_patch_field(&app, source, |patch| patch.tileCapacity());
    let carrying = published_patch_field(&app, source, |patch| patch.carryingCapacity());

    assert!(
        !published_patch_field(&app, source, |patch| patch.isField()),
        "fixture: this arm is about ground nobody has built on"
    );
    assert!(
        (tile_capacity - ground_k).abs() < 1e-3,
        "a wild patch's `tileCapacity` is its biome's own K: {tile_capacity} against {ground_k}"
    );
    assert!(
        (tile_capacity - carrying).abs() < 1e-3,
        "…and with no rung bought the two agree: {tile_capacity} against {carrying}"
    );
}

/// **State the ground under `coord`**, so a test that is about a capacity knows which capacity.
/// Which site [`a_cultivable_site`] lands on is not stable run to run, so a fixture that inherited
/// its terrain would be asserting against a number that moves.
fn pin_the_ground(app: &mut App, coord: UVec2, terrain: sim_runtime::TerrainType) {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("the fixture tile is on the map");
    app.world
        .get_mut::<core_sim::Tile>(tile_entity)
        .expect("the fixture tile carries a Tile")
        .terrain = terrain;
}

/// **⛔ THE QUOTED PRICE IS THE PRICE THE PATCH WILL BE BILLED** — one verdict, one producer.
///
/// `cultivationUpkeepDemand` is what a compose sheet shows **before** the player commits;
/// `upkeepDemand` is what the patch is actually charged. Both plant rungs quote their rate **per
/// tender-load** (`forage::patch_tender_loads`), so a quote struck off the ladder's bare rate would
/// be the *reference tile's* price shown on every patch in the game — promising `4.0` for a Field
/// that will be billed `4.31`.
///
/// **Run on a patch standing on the tended rung, on ground that is NOT the reference tile**, so the
/// two numbers cannot agree by coincidence: at one tender-load every scaling bug is invisible.
#[test]
fn the_quoted_price_is_the_price_the_patch_will_be_billed() {
    /// A whole tended rung on the meter, so the **at-risk** rung the bill is struck through and the
    /// rung the quote names are the same rung — which is what makes the two comparable at all.
    const A_WHOLE_TENDED_RUNG: f32 = 1.0;
    /// Nobody building, so the position cannot climb into the Field's span between the stamp and
    /// the capture and leave the two describing different rungs.
    const NOBODY_BUILDING: u32 = 0;

    /// **Ground that is emphatically not the reference tile** — `RiverDelta`'s `K` of 210 against the
    /// 195 `capacity_per_tender` ships at. **Stated rather than inherited**: which site
    /// [`a_cultivable_site`] lands on is not stable run to run, and it sometimes lands on
    /// `AlluvialPlain`, where a quote struck off the bare ladder rate agrees with the bill by
    /// coincidence and this test asserts nothing.
    const NOT_THE_REFERENCE_TILE: sim_runtime::TerrainType = sim_runtime::TerrainType::RiverDelta;

    let (mut app, source) = world_with_a_patch(NOBODY_BUILDING, A_WHOLE_TENDED_RUNG);
    pin_the_ground(&mut app, source, NOT_THE_REFERENCE_TILE);
    let loads = tender_loads_at(&app, source);
    assert!(
        (loads - core_sim::ONE_TENDER_LOAD).abs() > 1e-3,
        "PRECONDITION: this fixture's ground must NOT be the reference tile ({loads} tender-loads), \
         or a quote struck off the bare ladder rate would agree with the bill by coincidence"
    );

    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let quoted = published_patch_field(&app, source, |patch| patch.cultivationUpkeepDemand());
    let billed = published_patch_field(&app, source, |patch| patch.upkeepDemand());
    assert!(
        billed > core_sim::NO_UPKEEP_DEMAND,
        "PRECONDITION: a patch holding the tended rung is billed something, or both sides are zero"
    );
    assert!(
        (quoted - billed).abs() < 1e-4,
        "the sheet's price and the patch's bill are one number: quoted {quoted}, billed {billed}"
    );

    // **AND IT IS THE SCALED NUMBER, not the ladder's bare rate.** Without this the pair above is
    // satisfiable by scaling neither — two producers agreeing on the wrong answer.
    let bare_rate = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_demand(core_sim::ONE_TENDER_LOAD);
    assert!(
        (quoted - bare_rate).abs() > 1e-3,
        "…and it is this GROUND's price, not the reference tile's: {quoted} against the ladder's \
         bare {bare_rate} at {loads} tender-loads"
    );
    assert!(
        (quoted - bare_rate * loads).abs() < 1e-4,
        "…which is the rung's rate per tender-load of this tile: {quoted} against \
         {bare_rate} × {loads}"
    );
}

/// **A BLOCKED HEAD AT ZERO PROGRESS PUBLISHES `upkeepSupplied 0` AGAINST ITS `upkeepDemand 0`** —
/// a row that no longer disagrees with itself (`docs/plan_standing_upkeep.md` §4.6a).
///
/// The keeping pool's claim side carries a **verb** term because `maintenance_shares` runs before
/// the accrual that banks a build's first work. The capture does not: it reads a source's demand
/// with **no verb in flight**, which is `0` for a meter at zero. So a head whose gate refuses —
/// which banks nothing on any turn — used to be stamped a positive share against a published demand
/// of `0`, on the wire, for as long as the block lasted. (The other half of that defect is the
/// dilution it inflicted on the band's real holdings; that is pinned in `forage_cultivation.rs` and
/// `fauna_husbandry.rs`, where the pool's split is visible.)
///
/// **The pair is what carries it.** A claim gate that answered *never* would pass the blocked arm
/// alone, so the same unstarted head with its knowledge granted must publish a **positive** supply
/// against a positive demand — the first-turn case the verb term exists for.
#[test]
fn a_blocked_head_publishes_no_supply_against_the_zero_demand_it_publishes() {
    /// The pool standing on the head — a blocked head is only reportable when a pool is committed.
    const BUILDERS: u32 = 2;

    // One turn on an **unstarted** patch carrying a queue entry, with the keeping staffed.
    // `NOTHING_BANKED` is the state the whole defect lives in: with progress on the meter the claim
    // comes from the ground rather than from the verb, and must keep coming.
    let run = |knows_cultivation: bool| -> (f32, f32, f32, String, i32) {
        let (mut app, source) =
            world_with_a_patch_knowing(BUILDERS, NOTHING_BANKED, knows_cultivation, A_GATHERER);
        // **The declaration.** `world_with_a_patch_knowing` queues nothing on an unstarted meter —
        // that is its pre-commit shape — so the entry is stated here, which is what puts the pool on
        // this head.
        {
            let mut query = app.world.query::<&mut LaborAllocation>();
            let mut found = false;
            for mut allocation in query.iter_mut(&mut app.world) {
                if allocation.assignments.iter().any(|assignment| {
                    matches!(assignment.target, LaborTarget::Forage { tile, .. } if tile == source)
                }) {
                    allocation.build_queue.push(core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Patch(source),
                        declared: core_sim::BuildJob::Rung(core_sim::Improvement::Cultivate), kit: None,});
                    found = true;
                }
            }
            assert!(found, "fixture: the band holds the source patch");
        }
        let keepers = keeping_demand_in_hands(&app, source);
        staff_the_keeping(&mut app, source, keepers);
        core_sim::run_turn(&mut app);
        recapture_snapshot_in_place(&mut app.world);
        (
            published_patch_field(&app, source, |patch| patch.upkeepSupplied()),
            published_patch_field(&app, source, |patch| patch.upkeepDemand()),
            published_patch_field(&app, source, |patch| patch.cultivationWorkDone()),
            published_patch_field(&app, source, |patch| {
                patch.buildBlockedReason().unwrap_or_default().to_string()
            }),
            published_patch_field(&app, source, |patch| patch.buildTurnsRemaining()),
        )
    };

    let (supplied, demand, banked, reason, turns) = run(!THE_GATE_IS_OPEN);
    assert_eq!(
        turns, BUILD_QUEUE_BLOCKED,
        "fixture: the head must be blocked with a pool standing on it, got {turns}"
    );
    assert_eq!(
        reason, "knowledge",
        "fixture: and blocked by the gate this arm staged, got '{reason}'"
    );
    assert_eq!(
        demand, 0.0,
        "the capture reads a meter at zero as owing nothing — that is what a client sees ({demand})"
    );
    assert_eq!(
        supplied, 0.0,
        "…so the pool must publish nothing against it, or the row disagrees with itself ({supplied})"
    );
    assert_eq!(
        banked, 0.0,
        "fixture: a blocked head banks nothing, which is what makes its demand honestly zero"
    );

    let (supplied, demand, banked, reason, turns) = run(THE_GATE_IS_OPEN);
    assert_eq!(
        reason, "",
        "fixture: granting the knowledge must open the gate, still blocked on '{reason}'"
    );
    assert_ne!(
        turns, BUILD_QUEUE_BLOCKED,
        "fixture: …and the pool must actually be raising it now"
    );
    assert!(
        banked > 0.0,
        "the build banked its first work, or this arm is the blocked one again ({banked})"
    );
    // **THE SUPPLY MATCHES THE BILL, and on this turn the bill is still nothing.** The claim side's
    // verb term is what makes the source *claim* on the turn it banks its first work — without it
    // the row would publish a shortfall on a staffed role — but what it claims is the demand at the
    // position it stood on when the share was split, which is zero. So the row reads
    // `demand == supplied` rather than `supplied > 0`, and the invariant that matters is that it
    // does not read SHORT.
    assert_eq!(
        supplied, demand,
        "…and the keeping pool answered for exactly the bill it was handed — a staffed role must \
         never publish a shortfall it could not have covered ({supplied} against {demand})"
    );
}
